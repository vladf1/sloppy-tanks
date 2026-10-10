//! Choices a page stores before reloading into a room (`src/net/pending-join.ts`).
//! Storage stays in the page; this validates the stored selection.

use serde_json::Value;

use super::protocol::JoinChoice;
use super::schema::record;

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
        let text = format!(r#"{{"room":"ABCDEFGH","choice":{}}}"#, choice.to_json());
        assert_eq!(pending_join(Some(&text), "ABCDEFGH"), Some(choice));
        assert_eq!(pending_join(Some(&text), "HGFEDCBA"), None);
        assert_eq!(pending_join(Some("{"), "ABCDEFGH"), None);
    }
}
