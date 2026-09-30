//! Choices a page stores before reloading into a room (`src/net/pending-join.ts`).
//! Storage stays in the page; these functions validate and serialize the selection.

use serde_json::Value;

use super::protocol::{JoinChoice, is_room_code};
use super::schema::{ReadResult, record, text_length};

/// The choices stored for `room` before a reload (`takePendingJoin`), or `None` when the
/// saved text is missing, for another room, or invalid.
pub fn pending_join(saved: Option<&str>, room: &str) -> Option<JoinChoice> {
    let value: Value = serde_json::from_str(saved?).ok()?;
    let pending = record(&value).ok()?;
    if pending.get("room").and_then(Value::as_str) != Some(room) {
        return None;
    }
    let choice = pending.get("choice")?.as_object()?;
    JoinChoice::read(choice).ok()
}

/// The text a page stores before reloading into `room` (`joinAfterReload`).
pub fn pending_join_text(room: &str, choice: &JoinChoice) -> ReadResult<String> {
    if !is_room_code(room) || text_length(&choice.name) > 24 {
        return Err("Invalid room selection".into());
    }
    let mut out = String::new();
    let mut writer = super::json::ObjectWriter::new(&mut out);
    writer.string("room", room);
    let body = writer.key("choice");
    let mut inner = super::json::ObjectWriter::new(body);
    inner
        .string("name", &choice.name)
        .string("kind", choice.kind.as_str());
    if let Some(team) = choice.team {
        inner.int("team", team.index() as u64);
    }
    if let Some(create) = &choice.create {
        let settings = create.to_json();
        inner.raw("create", &settings);
    }
    if let Some(existing) = choice.existing_room {
        inner.boolean("existingRoom", existing);
    }
    inner.finish();
    writer.finish();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::types::VehicleKind;

    #[test]
    fn pending_joins_round_trip() {
        let choice = JoinChoice {
            name: "Ace".into(),
            kind: VehicleKind::Heavy,
            team: None,
            create: None,
            existing_room: Some(true),
        };
        let text = pending_join_text("ABCDEFGH", &choice).unwrap();
        assert_eq!(pending_join(Some(&text), "ABCDEFGH"), Some(choice));
        assert_eq!(pending_join(Some(&text), "HGFEDCBA"), None);
        assert_eq!(pending_join(Some("{"), "ABCDEFGH"), None);
    }
}
