//! The UI notices a room page reads from `NetGame.take_notices`.

use serde_json::{Value, json};
use sloppy_core::net::client::ClientNotice;

/// A client notice as the page reads it, or `None` for the arena and baseline requests
/// `NetGame` handles itself.
pub fn page_notice(notice: &ClientNotice, player_id: &str) -> Option<Value> {
    Some(match notice {
        ClientNotice::Status { text, connected } => {
            json!({ "type": "status", "text": text, "connected": connected })
        }
        ClientNotice::Notice(text) => json!({ "type": "notice", "text": text }),
        ClientNotice::Ended { cause, text } => {
            json!({ "type": "ended", "cause": cause.as_str(), "text": text })
        }
        ClientNotice::Lobby(lobby) => {
            let lobby: Value = serde_json::from_str(&lobby.to_json()).unwrap_or_default();
            json!({ "type": "lobby", "lobby": lobby, "playerId": player_id })
        }
        ClientNotice::Result { outcome, team } => {
            json!({ "type": "result", "match": outcome, "team": team })
        }
        ClientNotice::ResetFeedback => json!({ "type": "resetFeedback" }),
        ClientNotice::ClearInput => json!({ "type": "clearInput" }),
        ClientNotice::Reveal => json!({ "type": "reveal" }),
        ClientNotice::PrepareArena { .. } | ClientNotice::ShowBaseline { .. } => return None,
    })
}

/// `prepare`: compile the arena with `prepare_step` until it reports done.
pub fn prepare_notice() -> Value {
    json!({ "type": "prepare" })
}

/// `arenaFailed`: the arena could not be built or compiled.
pub fn arena_failed_notice(error: &str) -> Value {
    json!({ "type": "arenaFailed", "error": error })
}

/// `baselineShown`: the arena drew a new baseline's first frames and takes input again.
pub fn baseline_shown_notice() -> Value {
    json!({ "type": "baselineShown" })
}
