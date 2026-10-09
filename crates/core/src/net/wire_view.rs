//! Binary server messages as the JSON protocol (version 1) sent them, for tests and the
//! Node checks and tools (through the web crate's `WireView`).
//!
//! Frames carry differences from what the client holds, so the view decodes through its
//! own [`StateMirror`], seeded by the first `full` baseline, and shows each changed field
//! with its absolute value. A changed field of a nested object (`position.x`) shows the
//! whole object, as the JSON diffs did, so a shallow merge keeps working; a deleted
//! optional field shows `null`.

use serde_json::{Map, Value, json};

use super::prediction::HullState;
use super::replication::{
    BinaryMessage, DecodedFrame, FrameExtras, StateMirror, TimedEvent, read_binary_message,
};
use super::scene_codec::{
    ENTITY_FIELDS, ENTITY_TYPES, EVENT_FIELDS, MATCH_FIELDS, json_number, write_event,
};
use super::schema::ReadResult;
use super::shot_paths::PathEntry;
use super::wire::{ChangedFields, Field, FieldKind, Slot, WireRecord};

fn value_json(field: &Field, slot: &Slot) -> ReadResult<Value> {
    Ok(match (field.kind, slot) {
        (_, Slot::Absent) => Value::Null,
        (FieldKind::Fixed(scale), Slot::Number(units)) => json_number(*units as f64 / scale),
        (FieldKind::Flag, Slot::Number(value)) => Value::Bool(*value != 0),
        (FieldKind::Choice(names), Slot::Number(index)) => Value::from(names[*index as usize]),
        (_, Slot::Number(value)) => Value::from(*value),
        (_, Slot::Text(text)) => Value::from(text.as_str()),
        (FieldKind::Blob(_, view), Slot::Blob(bytes)) => {
            view(bytes).map_err(|error| format!("{}: {error}", field.name))?
        }
        (_, Slot::Blob(_)) => return Err(format!("{}: Unexpected blob", field.name)),
    })
}

fn insert(object: &mut Map<String, Value>, path: &str, value: Value) {
    match path.split_once('.') {
        None => {
            object.insert(path.to_string(), value);
        }
        Some((head, rest)) => {
            let nested = object
                .entry(head.to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            if let Value::Object(nested) = nested {
                insert(nested, rest, value);
            }
        }
    }
}

fn top(name: &str) -> &str {
    name.split_once('.').map_or(name, |(head, _)| head)
}

/// A whole record: its present fields (absent nullable ones as `null`), with `id` first
/// for entities.
pub fn record_json(fields: &[Field], record: &WireRecord, with_id: bool) -> ReadResult<Value> {
    let mut object = Map::new();
    if with_id {
        object.insert("id".into(), Value::from(record.id));
    }
    for (field, slot) in fields.iter().zip(&record.slots) {
        if !slot.is_absent() || field.nullable {
            insert(&mut object, field.name, value_json(field, slot)?);
        }
    }
    Ok(Value::Object(object))
}

/// The fields a frame changed: each touched top-level field whole, `null` when deleted.
pub fn changes_json(
    fields: &[Field],
    record: &WireRecord,
    changed: ChangedFields,
) -> ReadResult<Value> {
    let touched = changed.set | changed.deleted;
    let mut object = Map::new();
    for (index, field) in fields.iter().enumerate() {
        if touched & (1 << index) == 0 || object.contains_key(top(field.name)) {
            continue;
        }
        let key = top(field.name);
        let group: Vec<(usize, &Field)> = fields
            .iter()
            .enumerate()
            .filter(|(_, other)| top(other.name) == key)
            .collect();
        if group.iter().all(|(at, _)| record.slots[*at].is_absent()) {
            object.insert(key.to_string(), Value::Null);
            continue;
        }
        for (at, member) in group {
            let slot = &record.slots[at];
            if !slot.is_absent() || member.nullable {
                insert(&mut object, member.name, value_json(member, slot)?);
            }
        }
    }
    Ok(Value::Object(object))
}

pub fn event_json(event: &TimedEvent) -> ReadResult<Value> {
    let mut record = WireRecord::default();
    write_event(&mut record, &event.event);
    Ok(json!({
        "eventId": event.event_id,
        "tick": json_number(event.tick),
        "event": record_json(EVENT_FIELDS, &record, false)?,
    }))
}

/// A projectile path entry as the JSON protocol wrote it.
pub fn path_json(entry: &PathEntry) -> Value {
    let mut text = String::new();
    entry.write(&mut text);
    serde_json::from_str(&text).expect("path entries write JSON")
}

/// A decoded frame in the JSON protocol's shape: `seq`, `tick`, `elapsed`, and `match`,
/// `updates`, `removed`, `events` and `paths` when present.
pub fn frame_json(frame: &DecodedFrame) -> ReadResult<Value> {
    let mut object = Map::new();
    object.insert("seq".into(), Value::from(frame.seq));
    object.insert("tick".into(), Value::from(frame.tick));
    object.insert("elapsed".into(), json_number(frame.elapsed));
    if let Some((record, changed)) = frame.match_change() {
        object.insert("match".into(), changes_json(MATCH_FIELDS, record, changed)?);
    }
    let mut updates = Map::new();
    for change in &frame.changed {
        let fields = ENTITY_FIELDS[change.kind];
        let value = if change.added {
            record_json(fields, &change.record, true)?
        } else {
            changes_json(fields, &change.record, change.changed)?
        };
        if let Value::Object(by_id) = updates
            .entry(ENTITY_TYPES[change.kind])
            .or_insert_with(|| Value::Object(Map::new()))
        {
            by_id.insert(change.id.to_string(), value);
        }
    }
    if !updates.is_empty() {
        object.insert("updates".into(), Value::Object(updates));
    }
    let removed: Map<String, Value> = frame
        .removed
        .iter()
        .enumerate()
        .filter(|(_, ids)| !ids.is_empty())
        .map(|(kind, ids)| (ENTITY_TYPES[kind].to_string(), json!(ids)))
        .collect();
    if !removed.is_empty() {
        object.insert("removed".into(), Value::Object(removed));
    }
    if !frame.events.is_empty() {
        let events = frame
            .events
            .iter()
            .map(event_json)
            .collect::<ReadResult<_>>()?;
        object.insert("events".into(), Value::Array(events));
    }
    if !frame.paths.is_empty() {
        let paths = frame.paths.iter().map(path_json).collect();
        object.insert("paths".into(), Value::Array(paths));
    }
    Ok(Value::Object(object))
}

/// One connection's server messages as JSON.
#[derive(Clone, Debug, Default)]
pub struct WireView {
    pub mirror: StateMirror,
}

impl WireView {
    /// A binary message as JSON; frames advance this view's mirror. Text messages are
    /// parsed as they are.
    pub fn binary(&mut self, bytes: &[u8]) -> ReadResult<Value> {
        self.binary_with_extras(bytes).map(|(value, _)| value)
    }

    /// Like [`binary`](Self::binary), also returning each frame's events and path entries.
    pub fn binary_with_extras(&mut self, bytes: &[u8]) -> ReadResult<(Value, Vec<FrameExtras>)> {
        match read_binary_message(bytes)? {
            BinaryMessage::Full(baseline) => {
                self.mirror
                    .apply_full(&baseline, baseline.room_epoch, baseline.round_id)?;
                let state = self.mirror.state.as_ref().expect("just applied");
                Ok((
                    json!({
                        "roomEpoch": baseline.room_epoch,
                        "roundId": baseline.round_id,
                        "type": "full",
                        "seq": baseline.seq,
                        "tick": baseline.tick,
                        "eventCursor": baseline.event_cursor,
                        "state": state.to_value(),
                        "paths": self
                            .mirror
                            .shots
                            .paths
                            .iter()
                            .map(|path| path_json(&PathEntry::Launch(*path)))
                            .collect::<Vec<_>>(),
                    }),
                    Vec::new(),
                ))
            }
            BinaryMessage::Snapshot(mut batch) => {
                let mut frames = Vec::new();
                let mut extras = Vec::new();
                // A batch for another round, or before any baseline, is skipped like the
                // client skips it.
                if batch.round_id == self.mirror.round_id && self.mirror.state.is_some() {
                    for _ in 0..batch.count {
                        let frame = self.mirror.decode(&mut batch, true)?;
                        frames.push(frame_json(&frame)?);
                        extras.push(self.mirror.commit(frame));
                    }
                }
                Ok((
                    json!({
                        "type": "snapshot",
                        "roundId": batch.round_id,
                        "ack": batch.ack,
                        "ackTick": batch.ack_tick,
                        "ackArrival": batch.ack_arrival,
                        "hull": batch.hull.as_ref().map(HullState::to_json),
                        "snapshots": frames,
                    }),
                    extras,
                ))
            }
        }
    }
}
