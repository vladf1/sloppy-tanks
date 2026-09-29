//! Page-level choices before a room connects: the server address
//! (`src/net/server-address.ts`), the remembered player name (`player-name.ts`) and the
//! choices a page stores before reloading into a room (`pending-join.ts`). Storage, the
//! location and random numbers stay in the page; these functions hold the rules.

use serde_json::Value;

use super::protocol::{JoinChoice, is_room_code};
use super::schema::{ReadResult, record, text_length};
use crate::sim::bot_personalities::BOT_NAMES;

/// The local development server, used on `localhost` when nothing else is configured.
pub const LOCAL_SERVER: &str = "ws://127.0.0.1:8787";
/// `sessionStorage` key of the choices a page stores before reloading into a room.
pub const PENDING_JOIN_KEY: &str = "sloppy-pending-join";
/// `localStorage` key of the remembered player name.
pub const PLAYER_NAME_KEY: &str = "sloppy-player-name";

pub fn is_local_host(hostname: &str) -> bool {
    matches!(hostname, "localhost" | "127.0.0.1")
}

/// The game server's WebSocket address, or `None` when this site has no multiplayer.
///
/// `server_param` is the `?server=` override (development builds only), `configured` the
/// build's `VITE_MULTIPLAYER_URL`. The address must be `ws:` or `wss:` without
/// credentials, query or fragment, and anything but a local page needs `wss:`. The
/// result has no trailing slash.
pub fn server_address(
    server_param: Option<&str>,
    configured: Option<&str>,
    hostname: &str,
) -> Result<Option<String>, String> {
    let local = is_local_host(hostname);
    let endpoint = server_param
        .filter(|value| !value.is_empty())
        .or(configured.filter(|value| !value.is_empty()))
        .or(local.then_some(LOCAL_SERVER));
    let Some(endpoint) = endpoint else {
        return Ok(None);
    };
    let invalid = || "Invalid multiplayer server configuration".to_string();
    let lower = endpoint.to_ascii_lowercase();
    let (secure, rest) = if let Some(rest) = lower.strip_prefix("wss://") {
        (true, &endpoint[endpoint.len() - rest.len()..])
    } else if let Some(rest) = lower.strip_prefix("ws://") {
        (false, &endpoint[endpoint.len() - rest.len()..])
    } else {
        return Err(invalid());
    };
    let authority = rest.split('/').next().unwrap_or("");
    if authority.is_empty()
        || authority.contains('@')
        || rest.contains('?')
        || rest.contains('#')
        || (!local && !secure)
    {
        return Err(invalid());
    }
    Ok(Some(endpoint.trim_end_matches('/').to_string()))
}

/// The name the page suggests: the remembered one (trimmed, at most 24 characters), or a
/// bot name picked by `random`.
pub fn preferred_player_name(saved: Option<&str>, random: u32) -> String {
    if let Some(saved) = saved {
        let trimmed = saved.trim();
        let units: Vec<u16> = trimmed.encode_utf16().take(24).collect();
        let name = String::from_utf16_lossy(&units);
        if !name.is_empty() {
            return name;
        }
    }
    BOT_NAMES[random as usize % BOT_NAMES.len()].to_string()
}

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
    fn server_addresses_follow_the_page_rules() {
        assert_eq!(
            server_address(None, None, "localhost"),
            Ok(Some(LOCAL_SERVER.to_string()))
        );
        assert_eq!(server_address(None, None, "sloppy-tanks.example"), Ok(None));
        assert_eq!(
            server_address(None, Some("wss://play.example/"), "sloppy-tanks.example"),
            Ok(Some("wss://play.example".to_string()))
        );
        assert!(server_address(None, Some("ws://play.example"), "site.example").is_err());
        assert!(server_address(Some("wss://a@b.example"), None, "localhost").is_err());
        assert!(server_address(Some("wss://b.example/?x"), None, "localhost").is_err());
        assert!(server_address(Some("https://b.example"), None, "localhost").is_err());
    }

    #[test]
    fn names_and_pending_joins_round_trip() {
        assert_eq!(preferred_player_name(Some("  Ace  "), 0), "Ace");
        assert_eq!(preferred_player_name(Some("   "), 1), BOT_NAMES[1]);
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
