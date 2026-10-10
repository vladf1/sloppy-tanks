//! Baselines and field deltas (`src/net/replication.ts`), in binary.
//!
//! The host sends a `full` baseline on join, resume and resync, then one snapshot frame per
//! captured tick: only changed fields of each record, removed ids, and the events and
//! projectile path entries (`shot_paths`) since the previous frame. A fixed-point field the client already
//! holds travels as the difference from its value, so a client must apply every frame in
//! order from a baseline; the client [`StateMirror`] applies frames transactionally: a
//! malformed or skipped frame never partially alters the mirror; it asks for a new
//! baseline instead.
//!
//! Messages (all integers LEB128 varints, `signed` zigzag varints):
//!
//! - `full`: `[FULL_MESSAGE] roundId tick seq eventCursor roomEpoch(text)` then the scene
//!   ([`Scene::write`]) and every shell's path ([`LivePaths::write_baseline`]).
//! - `snapshot`: `[SNAPSHOT_MESSAGE] roundId tick ack firstSeq count ackTick ackArrival`,
//!   a hull byte (1 when the seat's [`HullState`] follows, else 0), then `count` frames
//!   with consecutive sequence numbers. `tick` is the last frame's, so a reader that needs
//!   only the newest tick (the traffic bots) stops after `count`.
//!
//! A frame is `tickBack` (the header tick minus the frame's), `signed` elapsed difference
//! in milliseconds, a section mask ([`MATCH_SECTION`] ...) and the sections:
//! match changes; per changed kind (a kind mask first) the record count and each record
//! as an id difference (ascending) and its changes; per kind with removals the count and
//! ascending id differences; events as id differences, `signed` tick offsets back from
//! the frame in thousandths and whole event records; projectile path entries
//! ([`PathEntry::write_binary`]).

use std::collections::HashMap;

use super::json::POSITION_SCALE;
use super::player_controls::Ack;
use super::prediction::HullState;
use super::protocol::{FULL_MESSAGE, SNAPSHOT_MESSAGE};
use super::scene_codec::{
    COVERS, ENTITY_FIELDS, ENTITY_LIMITS, ENTITY_TYPES, EVENT_FIELDS, EntityStore, FRAGMENTS,
    MATCH_FIELDS, MINES, MirrorScene, Scene, Stored, TANKS, read_cover, read_event, read_fragment,
    read_match, read_mine, read_pickup, read_tank, write_event, write_record,
};
use super::schema::{NUMBER_BOUND, ReadResult};
use super::shot_paths::{LivePaths, PathEntry, ShotPath};
use super::wire::{
    ChangedFields, WireReader, WireRecord, put_signed, put_text, put_varint, read_changes,
    thousandths, write_changes,
};
use crate::sim::render_state::{RenderCover, RenderFragment, RenderState, RenderTank};
use crate::sim::types::{Match, Mine, Pickup, SimEvent};

/// Most field changes (updated records plus removals) one frame may carry.
const MAX_CHANGES: usize = 4096;
/// Most events or projectile path entries one frame may carry.
const MAX_FRAME_ITEMS: usize = 2048;
pub const MATCH_SECTION: u64 = 1;
pub const UPDATES_SECTION: u64 = 2;
pub const REMOVED_SECTION: u64 = 4;
pub const EVENTS_SECTION: u64 = 8;
pub const PATHS_SECTION: u64 = 16;
/// Room epochs are UUIDs; anything longer is not a baseline this protocol sent.
const MAX_EPOCH_BYTES: usize = 128;

/// A simulation event stamped with its stream position and tick.
#[derive(Clone, Debug, PartialEq)]
pub struct TimedEvent {
    pub event_id: u64,
    pub tick: f64,
    pub event: SimEvent,
}

/// The host's per-round stream: numbers frames and diffs each captured scene against the
/// previous one. Every client of the round shares its frames.
pub struct StateStream {
    pub seq: u64,
    room_epoch: String,
    round_id: u64,
    previous: Option<Scene>,
    /// Ids of the previous scene's records by kind, and of the scene being written.
    index: [HashMap<u32, usize>; 5],
    current: [HashMap<u32, usize>; 5],
    scratch: SnapshotScratch,
}

#[derive(Default)]
struct SnapshotScratch {
    updates: Vec<u8>,
    removed: Vec<u8>,
    entries: Vec<u32>,
    record: WireRecord,
}

fn index_scene(index: &mut [HashMap<u32, usize>; 5], scene: &Scene) {
    for (kind, ids) in index.iter_mut().enumerate() {
        ids.clear();
        for (position, record) in scene.entities[kind].iter().enumerate() {
            assert!(
                ids.insert(record.id, position).is_none(),
                "Duplicate entity id"
            );
        }
    }
}

impl StateStream {
    pub fn new(room_epoch: &str, round_id: u64) -> Self {
        Self {
            seq: 0,
            room_epoch: room_epoch.to_string(),
            round_id,
            previous: None,
            index: Default::default(),
            current: Default::default(),
            scratch: SnapshotScratch::default(),
        }
    }

    /// The scene the last frame left clients with, if any frame or baseline was written.
    pub fn previous(&self) -> Option<&Scene> {
        self.previous.as_ref()
    }

    /// A `full` baseline message: the stream's previous scene (the first baseline starts
    /// the stream with `capture`), at `tick` and the current `seq`, with the id of the last
    /// event it already reflects and the path of every shell in flight. Only that scene
    /// works as a baseline: the next frame's differences apply to it.
    pub fn full<'a>(
        &mut self,
        tick: u64,
        event_cursor: u64,
        paths: impl IntoIterator<Item = &'a ShotPath>,
        capture: impl FnOnce() -> Scene,
    ) -> Vec<u8> {
        if self.previous.is_none() {
            let scene = capture();
            index_scene(&mut self.index, &scene);
            self.previous = Some(scene);
        }
        let scene = self.previous.as_ref().expect("set above");
        let mut out = Vec::with_capacity(16 * 1024);
        out.push(FULL_MESSAGE);
        put_varint(&mut out, self.round_id);
        put_varint(&mut out, tick);
        put_varint(&mut out, self.seq);
        put_varint(&mut out, event_cursor);
        put_text(&mut out, &self.room_epoch);
        scene.write(&mut out);
        LivePaths::write_baseline(paths, tick, &mut out);
        out
    }

    /// The next frame's body (without its tick, which the batch header carries). `scene`
    /// becomes the stream's previous scene and is replaced by the older one, whose
    /// buffers the caller may reuse for its next capture.
    pub fn snapshot(
        &mut self,
        scene: &mut Scene,
        tick: u64,
        events: &[TimedEvent],
        paths: &[PathEntry],
    ) -> Vec<u8> {
        self.seq += 1;
        index_scene(&mut self.current, scene);
        let previous = self.previous.as_ref();
        let SnapshotScratch {
            updates,
            removed,
            entries,
            record,
        } = &mut self.scratch;
        let mut out = Vec::with_capacity(256);
        let mut sections = 0;
        let elapsed = thousandths(scene.elapsed);
        put_signed(
            &mut out,
            elapsed - previous.map_or(0, |scene| thousandths(scene.elapsed)),
        );
        let mut body = Vec::new();
        if write_changes(
            MATCH_FIELDS,
            previous.map(|scene| &scene.match_record),
            &scene.match_record,
            &mut body,
        ) {
            sections |= MATCH_SECTION;
        }
        updates.clear();
        removed.clear();
        let (mut updated_kinds, mut removed_kinds) = (0u64, 0u64);
        for (kind, records) in scene.entities.iter().enumerate() {
            let fields = ENTITY_FIELDS[kind];
            let before = previous.map(|scene| &scene.entities[kind]);
            let old = |id: u32| {
                before.and_then(|before| self.index[kind].get(&id).map(|&at| &before[at]))
            };
            // Ascending ids: clients append new records in id order, as JavaScript visited
            // integer object keys.
            entries.clear();
            entries.extend(records.iter().map(|record| record.id));
            entries.sort_unstable();
            let mut count = 0u64;
            let mut last = 0u32;
            let mut changes = Vec::new();
            for &id in entries.iter() {
                let next = &records[self.current[kind][&id]];
                let start = changes.len();
                put_varint(&mut changes, u64::from(id - last));
                if write_changes(fields, old(id), next, &mut changes) {
                    last = id;
                    count += 1;
                } else {
                    changes.truncate(start);
                }
            }
            if count > 0 {
                updated_kinds |= 1 << kind;
                put_varint(updates, count);
                updates.extend_from_slice(&changes);
            }
            if let Some(before) = before {
                entries.clear();
                entries.extend(
                    before
                        .iter()
                        .map(|record| record.id)
                        .filter(|id| !self.current[kind].contains_key(id)),
                );
                if !entries.is_empty() {
                    removed_kinds |= 1 << kind;
                    entries.sort_unstable();
                    put_varint(removed, entries.len() as u64);
                    let mut last = 0;
                    for &id in entries.iter() {
                        put_varint(removed, u64::from(id - last));
                        last = id;
                    }
                }
            }
        }
        if updated_kinds != 0 {
            sections |= UPDATES_SECTION;
            put_varint(&mut body, updated_kinds);
            body.extend_from_slice(updates);
        }
        if removed_kinds != 0 {
            sections |= REMOVED_SECTION;
            put_varint(&mut body, removed_kinds);
            body.extend_from_slice(removed);
        }
        let frame_tick = thousandths(tick as f64);
        if !events.is_empty() {
            sections |= EVENTS_SECTION;
            put_varint(&mut body, events.len() as u64);
            let mut last = 0;
            for event in events {
                put_varint(&mut body, event.event_id - last);
                last = event.event_id;
                put_signed(&mut body, frame_tick - thousandths(event.tick));
                write_event(record, &event.event);
                write_record(EVENT_FIELDS, record, &mut body);
            }
        }
        if !paths.is_empty() {
            sections |= PATHS_SECTION;
            put_varint(&mut body, paths.len() as u64);
            for entry in paths {
                entry.write_binary(&mut body, tick);
            }
        }
        put_varint(&mut out, sections);
        out.extend_from_slice(&body);
        let current = std::mem::take(scene);
        std::mem::swap(&mut self.index, &mut self.current);
        if let Some(old) = self.previous.replace(current) {
            *scene = old;
        }
        out
    }
}

/// Writes a snapshot message's header for one seat; the frames follow, each its tick back
/// from `tick` and its body. The acknowledged input's ticks and the seat's hull come after
/// the batch's own fields, which is all the traffic bots read.
pub fn write_snapshot_header(
    out: &mut Vec<u8>,
    round_id: u64,
    tick: u64,
    ack: &Ack,
    first_seq: u64,
    count: usize,
    hull: Option<&HullState>,
) {
    out.push(SNAPSHOT_MESSAGE);
    put_varint(out, round_id);
    put_varint(out, tick);
    put_varint(out, ack.input_seq.max(0) as u64);
    put_varint(out, first_seq);
    put_varint(out, count as u64);
    put_varint(out, ack.applied_tick);
    put_varint(out, ack.arrival_tick);
    match hull {
        Some(hull) => {
            out.push(1);
            hull.write(out);
        }
        None => out.push(0),
    }
}

/// A `full` baseline's header, with its scene still to read.
#[derive(Clone, Debug)]
pub struct Baseline<'a> {
    pub round_id: u64,
    pub tick: u64,
    pub seq: u64,
    pub event_cursor: u64,
    pub room_epoch: &'a str,
    scene: WireReader<'a>,
}

/// A snapshot batch's header, with its frames still to read.
#[derive(Clone, Debug)]
pub struct SnapshotBatch<'a> {
    pub round_id: u64,
    /// The last frame's tick.
    pub tick: u64,
    /// The latest input sequence the host applied for this seat.
    pub ack: u64,
    pub first_seq: u64,
    pub count: u64,
    /// The tick that input first drove, and the first it could have driven on arrival.
    pub ack_tick: u64,
    pub ack_arrival: u64,
    /// The seat's tank after `tick`, while it lives.
    pub hull: Option<HullState>,
    read: u64,
    frames: WireReader<'a>,
}

/// A binary server message.
#[derive(Clone, Debug)]
pub enum BinaryMessage<'a> {
    Full(Baseline<'a>),
    Snapshot(SnapshotBatch<'a>),
}

/// Reads a binary message's header.
pub fn read_binary_message(bytes: &[u8]) -> ReadResult<BinaryMessage<'_>> {
    let mut reader = WireReader::new(bytes);
    match reader.byte()? {
        FULL_MESSAGE => Ok(BinaryMessage::Full(Baseline {
            round_id: reader.varint()?,
            tick: reader.varint()?,
            seq: reader.varint()?,
            event_cursor: reader.varint()?,
            room_epoch: reader.text(MAX_EPOCH_BYTES)?,
            scene: reader,
        })),
        SNAPSHOT_MESSAGE => Ok(BinaryMessage::Snapshot(SnapshotBatch {
            round_id: reader.varint()?,
            tick: reader.varint()?,
            ack: reader.varint()?,
            first_seq: reader.varint()?,
            count: reader.varint()?,
            ack_tick: reader.varint()?,
            ack_arrival: reader.varint()?,
            hull: match reader.byte()? {
                0 => None,
                1 => Some(HullState::read(&mut reader)?),
                _ => return Err("Invalid hull".into()),
            },
            read: 0,
            frames: reader,
        })),
        _ => Err("Unknown message".into()),
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
            .filter(|stored| !store.contains(stored.wire.id))
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
    tanks: KindChanges<RenderTank>,
    covers: KindChanges<RenderCover>,
    fragments: KindChanges<RenderFragment>,
    mines: KindChanges<Mine>,
    pickups: KindChanges<Pickup>,
}

/// Reads record `id`'s changes into `changes`, returning the fields it changed, whether
/// it is new to the mirror, and its resulting wire record.
fn update<'a, T>(
    store: &EntityStore<T>,
    changes: &'a mut KindChanges<T>,
    kind: usize,
    id: u32,
    reader: &mut WireReader<'_>,
    read: fn(&WireRecord) -> ReadResult<T>,
) -> ReadResult<(ChangedFields, bool, &'a WireRecord)> {
    let held = store.get(id);
    let mut wire = held.map_or_else(
        || WireRecord {
            id,
            slots: Vec::new(),
        },
        |stored| stored.wire.clone(),
    );
    let changed = read_changes(ENTITY_FIELDS[kind], &mut wire, reader)?;
    let value = read(&wire)?;
    changes.updates.push(Stored { wire, value });
    let pushed = &changes.updates.last().expect("just pushed").wire;
    Ok((changed, held.is_none(), pushed))
}

/// Queues the removal of record `id`, which the mirror must hold and the frame must not
/// also change.
fn remove<T>(store: &EntityStore<T>, changes: &mut KindChanges<T>, id: u32) -> ReadResult<()> {
    if changes.updates.iter().any(|stored| stored.wire.id == id) {
        return Err("Duplicate change".into());
    }
    if !store.contains(id) {
        return Err("Unknown removal".into());
    }
    changes.removals.push(id);
    Ok(())
}

/// One record a frame changed, for the JSON view.
#[derive(Clone, Debug)]
pub struct ChangedRecord {
    pub kind: usize,
    pub id: u32,
    pub changed: ChangedFields,
    /// The client did not hold it before: the frame sent it whole.
    pub added: bool,
    pub record: WireRecord,
}

/// A decoded, validated frame, applied by [`StateMirror::commit`].
pub struct DecodedFrame {
    pub seq: u64,
    pub tick: u64,
    pub elapsed: f64,
    changes: FrameChanges,
    match_wire: Option<(WireRecord, Match, ChangedFields)>,
    pub removed: [Vec<u32>; 5],
    pub events: Vec<TimedEvent>,
    pub paths: Vec<PathEntry>,
    /// The shells in flight after this frame.
    shots: LivePaths,
    event_cursor: u64,
    /// Filled only when decoding for the JSON view.
    pub changed: Vec<ChangedRecord>,
}

impl DecodedFrame {
    /// The match record after this frame and the fields it changed, if any.
    pub fn match_change(&self) -> Option<(&WireRecord, ChangedFields)> {
        self.match_wire
            .as_ref()
            .map(|(record, _, changed)| (record, *changed))
    }
}

impl StateMirror {
    /// Adopts a `full` baseline for the given room instance and round. Errors leave the
    /// mirror unchanged.
    pub fn apply_full(
        &mut self,
        baseline: &Baseline<'_>,
        room_epoch: &str,
        round_id: u64,
    ) -> ReadResult<()> {
        if baseline.room_epoch != room_epoch || baseline.round_id != round_id {
            return Err("Wrong baseline identity".into());
        }
        let mut scene = baseline.scene.clone();
        let state = MirrorScene::read(&mut scene)?;
        let shots = LivePaths::read_binary(&mut scene, baseline.tick)
            .map_err(|error| format!("paths: {error}"))?;
        if !scene.is_empty() {
            return Err("Trailing data".into());
        }
        self.state = Some(state);
        self.shots = shots;
        self.room_epoch = room_epoch.to_string();
        self.round_id = round_id;
        self.seq = baseline.seq;
        self.tick = baseline.tick;
        self.event_cursor = baseline.event_cursor;
        self.needs_full = false;
        Ok(())
    }

    /// Applies the batch's next frame, returning its new events and path entries, or `None`
    /// (and `needs_full`) when the frame is invalid, out of order, or no baseline is held.
    /// A failed frame leaves the rest of the batch unreadable.
    pub fn apply_snapshot(&mut self, batch: &mut SnapshotBatch<'_>) -> Option<FrameExtras> {
        if self.state.is_none() || self.needs_full {
            return None;
        }
        match self.decode(batch, false) {
            Ok(frame) => Some(self.commit(frame)),
            Err(_) => {
                self.needs_full = true;
                None
            }
        }
    }

    /// Reads and validates the batch's next frame without changing the mirror. With
    /// `view`, the frame also lists every record it changed.
    pub fn decode(&self, batch: &mut SnapshotBatch<'_>, view: bool) -> ReadResult<DecodedFrame> {
        let state = self
            .state
            .as_ref()
            .ok_or_else(|| "No baseline".to_string())?;
        if batch.read >= batch.count {
            return Err("No more frames".into());
        }
        let seq = batch.first_seq + batch.read;
        batch.read += 1;
        if seq != self.seq + 1 {
            return Err("Snapshot gap".into());
        }
        let reader = &mut batch.frames;
        let tick = batch
            .tick
            .checked_sub(reader.varint()?)
            .ok_or_else(|| "Invalid tick".to_string())?;
        if tick < self.tick {
            return Err("Tick went backwards".into());
        }
        let elapsed_units = thousandths(state.elapsed)
            .checked_add(reader.signed()?)
            .ok_or_else(|| "Invalid number".to_string())?;
        let elapsed = elapsed_units as f64 / POSITION_SCALE;
        if elapsed.abs() > NUMBER_BOUND {
            return Err("elapsed: Invalid number".into());
        }
        let sections = reader.varint()?;
        if sections >> 5 != 0 {
            return Err("Invalid sections".into());
        }
        let mut frame = DecodedFrame {
            seq,
            tick,
            elapsed,
            changes: FrameChanges::default(),
            match_wire: None,
            removed: Default::default(),
            events: Vec::new(),
            paths: Vec::new(),
            shots: LivePaths::default(),
            event_cursor: self.event_cursor,
            changed: Vec::new(),
        };
        if sections & MATCH_SECTION != 0 {
            let mut wire = state.match_wire.clone();
            let changed = read_changes(MATCH_FIELDS, &mut wire, reader)
                .map_err(|error| format!("match: {error}"))?;
            let value = read_match(&wire).map_err(|error| format!("match: {error}"))?;
            frame.match_wire = Some((wire, value, changed));
        }
        let mut count = 0;
        let mut claim = |items: u64| -> ReadResult<()> {
            count += items as usize;
            if count > MAX_CHANGES {
                return Err("Invalid changes".into());
            }
            Ok(())
        };
        let kinds = |reader: &mut WireReader<'_>, present: bool| -> ReadResult<u64> {
            if !present {
                return Ok(0);
            }
            match reader.varint()? {
                0 => Err("Invalid sections".into()),
                kinds if kinds >> ENTITY_TYPES.len() == 0 => Ok(kinds),
                _ => Err("Invalid entity type".into()),
            }
        };
        let updated = kinds(reader, sections & UPDATES_SECTION != 0)?;
        for kind in (0..ENTITY_TYPES.len()).filter(|kind| updated & (1 << kind) != 0) {
            let records = reader.varint()?;
            claim(records)?;
            let mut id = 0u32;
            for index in 0..records {
                let step = reader.varint32()?;
                if index > 0 && step == 0 {
                    return Err("Duplicate change".into());
                }
                id = id
                    .checked_add(step)
                    .ok_or_else(|| "Invalid entity id".to_string())?;
                let changes = &mut frame.changes;
                let label = |error: String| format!("{}: {error}", ENTITY_TYPES[kind]);
                let (changed, added, record) = match kind {
                    TANKS => update(
                        &state.tanks,
                        &mut changes.tanks,
                        kind,
                        id,
                        reader,
                        read_tank,
                    ),
                    COVERS => update(
                        &state.covers,
                        &mut changes.covers,
                        kind,
                        id,
                        reader,
                        read_cover,
                    ),
                    FRAGMENTS => update(
                        &state.fragments,
                        &mut changes.fragments,
                        kind,
                        id,
                        reader,
                        read_fragment,
                    ),
                    MINES => update(
                        &state.mines,
                        &mut changes.mines,
                        kind,
                        id,
                        reader,
                        read_mine,
                    ),
                    _ => update(
                        &state.pickups,
                        &mut changes.pickups,
                        kind,
                        id,
                        reader,
                        read_pickup,
                    ),
                }
                .map_err(label)?;
                if view {
                    frame.changed.push(ChangedRecord {
                        kind,
                        id,
                        changed,
                        added,
                        record: record.clone(),
                    });
                }
            }
        }
        let removed = kinds(reader, sections & REMOVED_SECTION != 0)?;
        for kind in (0..ENTITY_TYPES.len()).filter(|kind| removed & (1 << kind) != 0) {
            let records = reader.varint()?;
            claim(records)?;
            let mut id = 0u32;
            for index in 0..records {
                let step = reader.varint32()?;
                if index > 0 && step == 0 {
                    return Err("Duplicate change".into());
                }
                id = id
                    .checked_add(step)
                    .ok_or_else(|| "Unknown removal".to_string())?;
                let changes = &mut frame.changes;
                match kind {
                    TANKS => remove(&state.tanks, &mut changes.tanks, id),
                    COVERS => remove(&state.covers, &mut changes.covers, id),
                    FRAGMENTS => remove(&state.fragments, &mut changes.fragments, id),
                    MINES => remove(&state.mines, &mut changes.mines, id),
                    _ => remove(&state.pickups, &mut changes.pickups, id),
                }?;
                frame.removed[kind].push(id);
            }
        }
        let changes = &frame.changes;
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
        if lengths[TANKS] == 0 {
            return Err("Missing viewer".into());
        }
        let frame_tick = thousandths(tick as f64);
        let items = |reader: &mut WireReader<'_>, present: bool| -> ReadResult<u64> {
            if !present {
                return Ok(0);
            }
            match reader.varint()? {
                count if count as usize <= MAX_FRAME_ITEMS => Ok(count),
                _ => Err("Invalid list".into()),
            }
        };
        let mut record = WireRecord::default();
        let mut event_id = 0u64;
        for _ in 0..items(reader, sections & EVENTS_SECTION != 0)? {
            event_id = event_id
                .checked_add(reader.varint()?)
                .ok_or_else(|| "Invalid number".to_string())?;
            let back = reader.signed()?;
            record.slots.clear();
            read_changes(EVENT_FIELDS, &mut record, reader)
                .map_err(|error| format!("event: {error}"))?;
            let event = read_event(&record).map_err(|error| format!("event: {error}"))?;
            let event_tick = frame_tick
                .checked_sub(back)
                .ok_or_else(|| "Invalid number".to_string())?;
            if event_id <= frame.event_cursor {
                // Already reflected by the baseline this mirror started from.
                continue;
            }
            if event_id != frame.event_cursor + 1 || back < 0 || event_tick < 0 {
                return Err("Event gap".into());
            }
            frame.event_cursor = event_id;
            frame.events.push(TimedEvent {
                event_id,
                tick: event_tick as f64 / POSITION_SCALE,
                event,
            });
        }
        frame.shots = self.shots.clone();
        let paths = items(reader, sections & PATHS_SECTION != 0)?;
        frame.paths = frame
            .shots
            .apply_binary(reader, paths, tick)
            .map_err(|error| format!("paths: {error}"))?;
        if batch.read == batch.count && !batch.frames.is_empty() {
            return Err("Trailing data".into());
        }
        Ok(frame)
    }

    /// Applies a frame [`decode`](Self::decode) validated against this mirror.
    pub fn commit(&mut self, frame: DecodedFrame) -> FrameExtras {
        let state = self.state.as_mut().expect("decoded against a baseline");
        let FrameChanges {
            tanks,
            covers,
            fragments,
            mines,
            pickups,
        } = frame.changes;
        tanks.apply(&mut state.tanks);
        covers.apply(&mut state.covers);
        fragments.apply(&mut state.fragments);
        mines.apply(&mut state.mines);
        pickups.apply(&mut state.pickups);
        state.elapsed = frame.elapsed;
        if let Some((wire, value, _)) = frame.match_wire {
            state.match_wire = wire;
            state.match_state = value;
        }
        self.shots = frame.shots;
        self.tick = frame.tick;
        self.seq = frame.seq;
        self.event_cursor = frame.event_cursor;
        FrameExtras {
            events: frame.events,
            paths: frame.paths,
        }
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

    /// Fills `state` for `viewer`, reusing its allocations.
    pub fn fill_render_state(&self, state: &mut RenderState, viewer: u32) -> ReadResult<()> {
        self.state
            .as_ref()
            .ok_or_else(|| "No baseline".to_string())?
            .fill_render_state(state, viewer)
    }
}
