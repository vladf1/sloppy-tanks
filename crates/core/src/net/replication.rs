//! Baselines and field deltas (`src/net/replication.ts`).
//!
//! The host sends a `full` baseline on join, resume and resync, then one snapshot frame per
//! captured tick: only changed fields of each record (a deleted optional field as `null`),
//! removed ids, and the events and projectile path entries (`shot_paths`) since the previous
//! frame. The client
//! [`StateMirror`] applies frames transactionally: a malformed or skipped frame never
//! partially alters the mirror; it asks for a new baseline instead.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::json::{self, ObjectWriter, write_int, write_str};
use super::scene_codec::{
    COVERS, ENTITY_LIMITS, ENTITY_TYPES, EntityStore, FRAGMENTS, MATCH_FIELDS, MINES, MirrorScene,
    Scene, Stored, TANKS, WireRecord, read_cover, read_entity, read_event, read_fragment,
    read_match, read_mine, read_pickup, read_tank, write_event,
};
use super::schema::{ReadResult, Record, array, field, id, nested, number, number_in, record};
use super::shot_paths::{LivePaths, PathEntry, ShotPath};
use crate::sim::render_state::RenderState;
use crate::sim::types::SimEvent;

/// Most field changes (updates plus removals) one frame may carry.
const MAX_CHANGES: usize = 4096;
/// Most events or projectile path entries one frame may carry.
const MAX_FRAME_ITEMS: usize = 2048;
/// Fields whose `null` is a real value rather than a deletion.
const COVER_NULLABLE: [&str; 2] = ["hp", "maxHp"];
const MATCH_NULLABLE: [&str; 1] = ["winner"];

/// A simulation event stamped with its stream position and tick.
#[derive(Clone, Debug, PartialEq)]
pub struct TimedEvent {
    pub event_id: u64,
    pub tick: f64,
    pub event: SimEvent,
}

impl TimedEvent {
    /// `{"eventId":...,"tick":...,"event":{...}}`, the event rounded by field name.
    pub fn write(event_id: u64, tick: f64, event: &SimEvent) -> String {
        let mut out = String::new();
        let mut writer = ObjectWriter::new(&mut out);
        writer
            .int("eventId", event_id)
            .number("tick", json::position(tick));
        write_event(writer.key("event"), event);
        writer.finish();
        out
    }

    /// `timedEventReader`.
    pub fn read(source: &Record) -> ReadResult<Self> {
        Ok(Self {
            event_id: field(source, "eventId", id)?,
            tick: field(source, "tick", |v| number_in(v, 0.0, 1e9, false))?,
            event: field(source, "event", |v| nested(v, read_event))?,
        })
    }
}

/// Writes `{"field":value,...,"deleted":null}` for fields of `next` whose JSON differs from
/// `previous`, current fields in order followed by deleted ones. `None` when nothing changed.
fn write_changes(previous: Option<&WireRecord>, next: &WireRecord, out: &mut String) -> bool {
    let Some(previous) = previous else {
        out.push_str(next.text());
        return true;
    };
    if previous.text() == next.text() {
        return false;
    }
    let mut any = false;
    let mut add = |out: &mut String, key: &str, value: &str| {
        out.push(if any { ',' } else { '{' });
        any = true;
        write_str(out, key);
        out.push(':');
        out.push_str(value);
    };
    let mut cursor = 0;
    for (key, value) in next.fields() {
        // Records of a kind share one field order, so the match is usually at the cursor.
        let found = previous
            .find_field(key, cursor)
            .or_else(|| previous.find_field(key, 0));
        let same = match found {
            Some((index, previous_value)) => {
                cursor = index + 1;
                previous_value == value
            }
            None => false,
        };
        if !same {
            add(out, key, value);
        }
    }
    for (key, _) in previous.fields() {
        if next.get(key).is_none() {
            add(out, key, "null");
        }
    }
    if any {
        out.push('}');
    }
    any
}

/// The host's per-round stream: numbers frames and diffs each captured scene against the
/// previous one. Every client of the round shares its frames.
pub struct StateStream {
    pub seq: u64,
    room_epoch: String,
    round_id: u64,
    previous: Option<Scene>,
    index: [HashMap<u32, usize>; 5],
    current_ids: HashSet<u32>,
    scratch: SnapshotScratch,
}

// One byte arena for all changed records; entries hold ranges rather than owning strings.
#[derive(Default)]
struct SnapshotScratch {
    changes: String,
    updates: String,
    removed: String,
    entries: Vec<(u32, std::ops::Range<usize>)>,
}

impl StateStream {
    pub fn new(room_epoch: &str, round_id: u64) -> Self {
        Self {
            seq: 0,
            room_epoch: room_epoch.to_string(),
            round_id,
            previous: None,
            index: Default::default(),
            current_ids: HashSet::new(),
            scratch: SnapshotScratch::default(),
        }
    }

    fn remember(&mut self, scene: Scene) -> Option<Scene> {
        for (kind, index) in self.index.iter_mut().enumerate() {
            index.clear();
            for (position, record) in scene.entities[kind].iter().enumerate() {
                assert!(
                    index.insert(record.id, position).is_none(),
                    "Duplicate entity id"
                );
            }
        }
        self.previous.replace(scene)
    }

    /// A `full` baseline message: the whole scene at `tick`, the current `seq`, the id
    /// of the last event it already reflects, and the path of every shell in flight.
    pub fn full<'a>(
        &mut self,
        scene: &Scene,
        tick: u64,
        event_cursor: u64,
        paths: impl IntoIterator<Item = &'a ShotPath>,
    ) -> String {
        if self.previous.is_none() {
            self.remember(scene.clone());
        }
        let mut out = String::new();
        let mut writer = ObjectWriter::new(&mut out);
        writer
            .string("roomEpoch", &self.room_epoch)
            .int("roundId", self.round_id)
            .string("type", "full")
            .int("seq", self.seq)
            .int("tick", tick)
            .int("eventCursor", event_cursor);
        scene.write(writer.key("state"));
        let list = writer.key("paths");
        list.push('[');
        for (index, path) in paths.into_iter().enumerate() {
            if index > 0 {
                list.push(',');
            }
            path.write(list, true);
        }
        list.push(']');
        writer.finish();
        out
    }

    /// The next frame's JSON. `scene` becomes the stream's previous scene and is replaced
    /// by the older one, whose buffers the caller may reuse for its next capture.
    pub fn snapshot(
        &mut self,
        scene: &mut Scene,
        tick: u64,
        events: &[String],
        paths: &[PathEntry],
    ) -> String {
        self.seq += 1;
        let mut out = String::new();
        let mut frame = ObjectWriter::new(&mut out);
        frame
            .int("seq", self.seq)
            .int("tick", tick)
            .number("elapsed", scene.elapsed);
        let previous = self.previous.as_ref();
        let SnapshotScratch {
            changes,
            updates,
            removed,
            entries,
        } = &mut self.scratch;
        changes.clear();
        updates.clear();
        removed.clear();
        if write_changes(
            previous.map(|scene| &scene.match_record),
            &scene.match_record,
            changes,
        ) {
            frame.raw("match", changes);
        }
        for (kind, records) in scene.entities.iter().enumerate() {
            entries.clear();
            changes.clear();
            let before = previous.map(|scene| &scene.entities[kind]);
            for record in records {
                let old = before.and_then(|before| {
                    self.index[kind]
                        .get(&record.id)
                        .map(|&position| &before[position])
                });
                let start = changes.len();
                if write_changes(old, record, changes) {
                    entries.push((record.id, start..changes.len()));
                }
            }
            if !entries.is_empty() {
                // JavaScript orders integer-like object keys numerically.
                entries.sort_unstable_by_key(|(id, _)| *id);
                updates.push(if updates.is_empty() { '{' } else { ',' });
                write_str(updates, ENTITY_TYPES[kind]);
                updates.push_str(":{");
                for (index, (id, text)) in entries.iter().enumerate() {
                    if index > 0 {
                        updates.push(',');
                    }
                    updates.push('"');
                    write_int(updates, u64::from(*id));
                    updates.push_str("\":");
                    updates.push_str(&changes[text.clone()]);
                }
                updates.push('}');
            }
            if let Some(before) = before {
                self.current_ids.clear();
                self.current_ids
                    .extend(records.iter().map(|record| record.id));
                let mut first = true;
                for record in before
                    .iter()
                    .filter(|record| !self.current_ids.contains(&record.id))
                {
                    if first {
                        removed.push(if removed.is_empty() { '{' } else { ',' });
                        write_str(removed, ENTITY_TYPES[kind]);
                        removed.push_str(":[");
                        first = false;
                    } else {
                        removed.push(',');
                    }
                    write_int(removed, u64::from(record.id));
                }
                if !first {
                    removed.push(']');
                }
            }
        }
        if !updates.is_empty() {
            updates.push('}');
            frame.raw("updates", updates);
        }
        if !removed.is_empty() {
            removed.push('}');
            frame.raw("removed", removed);
        }
        if !events.is_empty() {
            let list = frame.key("events");
            list.push('[');
            for (index, event) in events.iter().enumerate() {
                if index > 0 {
                    list.push(',');
                }
                list.push_str(event);
            }
            list.push(']');
        }
        if !paths.is_empty() {
            let list = frame.key("paths");
            list.push('[');
            for (index, entry) in paths.iter().enumerate() {
                if index > 0 {
                    list.push(',');
                }
                entry.write(list);
            }
            list.push(']');
        }
        frame.finish();
        let current = std::mem::take(scene);
        if let Some(old) = self.remember(current) {
            *scene = old;
        }
        out
    }
}

/// What a snapshot frame brought besides the scene change.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameExtras {
    pub events: Vec<TimedEvent>,
    pub paths: Vec<PathEntry>,
}

/// A client's mirror of the host's stream.
#[derive(Clone, Debug)]
pub struct StateMirror {
    pub state: Option<MirrorScene>,
    /// Shells in flight at `tick`.
    pub shots: LivePaths,
    pub room_epoch: String,
    pub round_id: u64,
    pub seq: u64,
    pub tick: u64,
    pub event_cursor: u64,
    pub needs_full: bool,
}

impl Default for StateMirror {
    fn default() -> Self {
        Self {
            state: None,
            shots: LivePaths::default(),
            room_epoch: String::new(),
            round_id: 0,
            seq: 0,
            tick: 0,
            event_cursor: 0,
            needs_full: true,
        }
    }
}

/// `{...before, ...changes}` with `null` deleting every field not in `nullable`.
fn apply_changes(before: Option<&Record>, changes: &Record, nullable: &[&str]) -> Record {
    let mut merged = before.cloned().unwrap_or_default();
    for (key, value) in changes {
        if value.is_null() && !nullable.contains(&key.as_str()) {
            merged.remove(key);
        } else {
            merged.insert(key.clone(), value.clone());
        }
    }
    merged
}

/// Validated changes for one kind, applied only once the whole frame is valid.
struct KindChanges<T> {
    updates: Vec<Stored<T>>,
    removals: Vec<u32>,
}

impl<T> Default for KindChanges<T> {
    fn default() -> Self {
        Self {
            updates: Vec::new(),
            removals: Vec::new(),
        }
    }
}

impl<T> KindChanges<T> {
    fn resulting_len(&self, store: &EntityStore<T>) -> usize {
        let added = self
            .updates
            .iter()
            .filter(|stored| !store.contains(stored.id))
            .count();
        store.len() + added - self.removals.len()
    }

    fn apply(self, store: &mut EntityStore<T>) {
        for stored in self.updates {
            store.upsert(stored);
        }
        store.remove_all(&self.removals);
    }
}

#[derive(Default)]
struct FrameChanges {
    tanks: KindChanges<crate::sim::render_state::RenderTank>,
    covers: KindChanges<crate::sim::render_state::RenderCover>,
    fragments: KindChanges<crate::sim::render_state::RenderFragment>,
    mines: KindChanges<crate::sim::types::Mine>,
    pickups: KindChanges<crate::sim::types::Pickup>,
}

fn update_entity<T>(
    store: &EntityStore<T>,
    kind: usize,
    entity_id: u32,
    changes: &Record,
    nullable: &[&str],
    read: fn(&Record) -> ReadResult<T>,
    id_of: fn(&T) -> u32,
) -> ReadResult<Stored<T>> {
    let merged = apply_changes(
        store.get(entity_id).map(|stored| &stored.wire),
        changes,
        nullable,
    );
    let stored = read_entity(kind, merged, read, id_of)?;
    if stored.id != entity_id {
        return Err("Entity identity changed".into());
    }
    Ok(stored)
}

fn entity_kind(name: &str) -> ReadResult<usize> {
    ENTITY_TYPES
        .iter()
        .position(|kind| *kind == name)
        .ok_or_else(|| "Invalid entity type".into())
}

impl StateMirror {
    /// Adopts a `full` baseline for the given room instance and round. Errors leave the
    /// mirror unchanged.
    pub fn apply_full(&mut self, value: &Value, room_epoch: &str, round_id: u64) -> ReadResult<()> {
        let data = record(value)?;
        if data.get("type").and_then(Value::as_str) != Some("full")
            || data.get("roomEpoch").and_then(Value::as_str) != Some(room_epoch)
            || data.get("roundId").and_then(Value::as_f64) != Some(round_id as f64)
        {
            return Err("Wrong baseline identity".into());
        }
        let state = MirrorScene::read(data.get("state"))?;
        let seq = id(data.get("seq"))?;
        let tick = id(data.get("tick"))?;
        let cursor = id(data.get("eventCursor"))?;
        let shots =
            LivePaths::read(data.get("paths"), tick).map_err(|error| format!("paths: {error}"))?;
        self.state = Some(state);
        self.shots = shots;
        self.room_epoch = room_epoch.to_string();
        self.round_id = round_id;
        self.seq = seq;
        self.tick = tick;
        self.event_cursor = cursor;
        self.needs_full = false;
        Ok(())
    }

    /// Applies the next frame, returning its new events and path entries, or `None` (and
    /// `needs_full`) when the frame is invalid, out of order, or no baseline is held.
    pub fn apply_snapshot(&mut self, value: &Value) -> Option<FrameExtras> {
        if self.state.is_none() || self.needs_full {
            return None;
        }
        match self.try_apply(value) {
            Ok(extras) => Some(extras),
            Err(_) => {
                self.needs_full = true;
                None
            }
        }
    }

    fn try_apply(&mut self, value: &Value) -> ReadResult<FrameExtras> {
        let data = record(value)?;
        if data.get("seq").and_then(Value::as_f64) != Some((self.seq + 1) as f64) {
            return Err("Snapshot gap".into());
        }
        let tick = id(data.get("tick"))?;
        if tick < self.tick {
            return Err("Tick went backwards".into());
        }
        let state = self.state.as_ref().expect("checked by apply_snapshot");
        let mut changes = FrameChanges::default();
        let mut claimed: [HashSet<u32>; 5] = Default::default();
        let mut count = 0;
        let mut claim = |kind: usize, entity: u32| -> ReadResult<()> {
            if !claimed[kind].insert(entity) {
                return Err("Duplicate change".into());
            }
            if count >= MAX_CHANGES {
                return Err("Invalid changes".into());
            }
            count += 1;
            Ok(())
        };
        let empty = Value::Object(Record::new());
        let section = |key: &str| match data.get(key) {
            None | Some(Value::Null) => &empty,
            Some(value) => value,
        };
        for (kind_name, by_id) in record(section("updates"))? {
            let kind = entity_kind(kind_name)?;
            // JavaScript visits integer keys in numeric order, so new records append in
            // id order; serde's map would visit them as text.
            let mut entries = record(by_id)?
                .iter()
                .map(|(id_text, fields)| {
                    id_text
                        .parse::<f64>()
                        .ok()
                        .and_then(|number| id(Some(&Value::from(number))).ok())
                        .filter(|number| number.to_string() == *id_text)
                        .and_then(|number| u32::try_from(number).ok())
                        .map(|entity| (entity, fields))
                        .ok_or_else(|| "Invalid entity id".to_string())
                })
                .collect::<ReadResult<Vec<_>>>()?;
            entries.sort_by_key(|(entity, _)| *entity);
            for (entity, fields) in entries {
                claim(kind, entity)?;
                let fields = record(fields)?;
                match kind {
                    TANKS => changes.tanks.updates.push(update_entity(
                        &state.tanks,
                        kind,
                        entity,
                        fields,
                        &[],
                        read_tank,
                        |t| t.id,
                    )?),
                    COVERS => changes.covers.updates.push(update_entity(
                        &state.covers,
                        kind,
                        entity,
                        fields,
                        &COVER_NULLABLE,
                        read_cover,
                        |c| c.id,
                    )?),
                    FRAGMENTS => changes.fragments.updates.push(update_entity(
                        &state.fragments,
                        kind,
                        entity,
                        fields,
                        &[],
                        read_fragment,
                        |f| f.id,
                    )?),
                    MINES => changes.mines.updates.push(update_entity(
                        &state.mines,
                        kind,
                        entity,
                        fields,
                        &[],
                        read_mine,
                        |m| m.id,
                    )?),
                    _ => changes.pickups.updates.push(update_entity(
                        &state.pickups,
                        kind,
                        entity,
                        fields,
                        &[],
                        read_pickup,
                        |p| p.id,
                    )?),
                }
            }
        }
        for (kind_name, ids) in record(section("removed"))? {
            let kind = entity_kind(kind_name)?;
            for entity in array(Some(ids), MAX_CHANGES, |item| id(Some(item)))? {
                let entity = u32::try_from(entity).map_err(|_| "Unknown removal")?;
                claim(kind, entity)?;
                let known = match kind {
                    TANKS => state.tanks.contains(entity),
                    COVERS => state.covers.contains(entity),
                    FRAGMENTS => state.fragments.contains(entity),
                    MINES => state.mines.contains(entity),
                    _ => state.pickups.contains(entity),
                };
                if !known {
                    return Err("Unknown removal".into());
                }
                match kind {
                    TANKS => changes.tanks.removals.push(entity),
                    COVERS => changes.covers.removals.push(entity),
                    FRAGMENTS => changes.fragments.removals.push(entity),
                    MINES => changes.mines.removals.push(entity),
                    _ => changes.pickups.removals.push(entity),
                }
            }
        }
        let lengths = [
            changes.tanks.resulting_len(&state.tanks),
            changes.covers.resulting_len(&state.covers),
            changes.fragments.resulting_len(&state.fragments),
            changes.mines.resulting_len(&state.mines),
            changes.pickups.resulting_len(&state.pickups),
        ];
        for (kind, length) in lengths.into_iter().enumerate() {
            if length > ENTITY_LIMITS[kind] {
                return Err(format!("entities: {}: Invalid list", ENTITY_TYPES[kind]));
            }
        }
        let elapsed = field(data, "elapsed", number)?;
        let match_changes = record(section("match"))?;
        let mut match_wire = apply_changes(Some(&state.match_wire), match_changes, &MATCH_NULLABLE);
        let match_state = read_match(&match_wire).map_err(|error| format!("match: {error}"))?;
        match_wire.retain(|key, _| MATCH_FIELDS.contains(&key.as_str()));
        if lengths[TANKS] == 0 {
            return Err("Missing viewer".into());
        }
        let events = match data.get("events") {
            None => Vec::new(),
            some => array(some, MAX_FRAME_ITEMS, |item| {
                nested(Some(item), TimedEvent::read)
            })?,
        };
        let events: Vec<TimedEvent> = events
            .into_iter()
            .filter(|event| event.event_id > self.event_cursor)
            .collect();
        let mut shots = self.shots.clone();
        let paths = match data.get("paths") {
            None => Vec::new(),
            Some(Value::Array(items)) if items.len() <= MAX_FRAME_ITEMS => shots
                .apply(items, tick)
                .map_err(|error| format!("paths: {error}"))?,
            Some(_) => return Err("paths: Invalid list".into()),
        };
        let mut cursor = self.event_cursor;
        for event in &events {
            if event.event_id != cursor + 1 || event.tick > tick as f64 {
                return Err("Event gap".into());
            }
            cursor = event.event_id;
        }
        let state = self.state.as_mut().expect("checked by apply_snapshot");
        changes.tanks.apply(&mut state.tanks);
        changes.covers.apply(&mut state.covers);
        changes.fragments.apply(&mut state.fragments);
        changes.mines.apply(&mut state.mines);
        changes.pickups.apply(&mut state.pickups);
        state.elapsed = elapsed;
        state.match_wire = match_wire;
        state.match_state = match_state;
        self.shots = shots;
        self.tick = tick;
        self.seq += 1;
        self.event_cursor = cursor;
        Ok(FrameExtras { events, paths })
    }

    /// The render state for `viewer`, its shells where their paths put them at `tick`.
    pub fn render(&self, viewer: u32) -> ReadResult<RenderState> {
        let mut state = self
            .state
            .as_ref()
            .ok_or_else(|| "No baseline".to_string())?
            .render(viewer)?;
        self.shots.fill(&mut state.shots, self.tick as f64);
        Ok(state)
    }
}
