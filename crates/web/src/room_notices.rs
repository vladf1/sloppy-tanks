//! The UI notices a room page reads from `NetGame.take_notices`: the client's notices
//! for the page, and `NetGame`'s own about preparing the arena.

use serde_json::{Value, json};
use sloppy_core::net::client::{ClientNotice, EndCause};
use sloppy_core::net::protocol::Lobby;
use sloppy_core::sim::{Match, Team};

/// UI work for the room page, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum RoomNotice {
    /// Progress while (re)connecting; `connected` once the seat is back.
    Status { text: String, connected: bool },
    /// A server answer to the player's last request, such as a full team.
    Notice(String),
    /// The connection gave up and will not retry on its own.
    Ended { cause: EndCause, text: String },
    /// The room's membership and settings, and this player's id in it.
    Lobby {
        lobby: Box<Lobby>,
        player_id: String,
    },
    /// The finished round as the viewer's tank played it; precedes its results lobby.
    Result { outcome: Match, team: Team },
    /// A new room instance or round: clear the kill feed and damage indicators.
    ResetFeedback,
    /// Drop held keys, clicks and pending ammo choices.
    ClearInput,
    /// The room page may replace Battle Setup.
    Reveal,
    /// Run `prepare_step` between tasks until it reports done.
    Prepare,
    /// The arena could not be built or compiled.
    ArenaFailed(String),
    /// The arena drew a new baseline's first frames and takes input again.
    BaselineShown,
}

impl RoomNotice {
    /// A client notice for the page, or `None` for the arena and baseline requests
    /// `NetGame` handles itself.
    pub fn from_client(notice: ClientNotice, player_id: &str) -> Option<Self> {
        Some(match notice {
            ClientNotice::Status { text, connected } => RoomNotice::Status { text, connected },
            ClientNotice::Notice(text) => RoomNotice::Notice(text),
            ClientNotice::Ended { cause, text } => RoomNotice::Ended { cause, text },
            ClientNotice::Lobby(lobby) => RoomNotice::Lobby {
                lobby: Box::new(lobby),
                player_id: player_id.to_owned(),
            },
            ClientNotice::Result { outcome, team } => RoomNotice::Result { outcome, team },
            ClientNotice::ResetFeedback => RoomNotice::ResetFeedback,
            ClientNotice::ClearInput => RoomNotice::ClearInput,
            ClientNotice::Reveal => RoomNotice::Reveal,
            ClientNotice::PrepareArena { .. } | ClientNotice::ShowBaseline { .. } => return None,
        })
    }

    /// The notice as the page reads it: `{type, ...}`.
    pub fn to_value(&self) -> Value {
        match self {
            RoomNotice::Status { text, connected } => {
                json!({ "type": "status", "text": text, "connected": connected })
            }
            RoomNotice::Notice(text) => json!({ "type": "notice", "text": text }),
            RoomNotice::Ended { cause, text } => {
                json!({ "type": "ended", "cause": cause.as_str(), "text": text })
            }
            RoomNotice::Lobby { lobby, player_id } => {
                let lobby: Value = serde_json::from_str(&lobby.to_json()).unwrap_or_default();
                json!({ "type": "lobby", "lobby": lobby, "playerId": player_id })
            }
            RoomNotice::Result { outcome, team } => {
                json!({ "type": "result", "match": outcome, "team": team })
            }
            RoomNotice::ResetFeedback => json!({ "type": "resetFeedback" }),
            RoomNotice::ClearInput => json!({ "type": "clearInput" }),
            RoomNotice::Reveal => json!({ "type": "reveal" }),
            RoomNotice::Prepare => json!({ "type": "prepare" }),
            RoomNotice::ArenaFailed(error) => json!({ "type": "arenaFailed", "error": error }),
            RoomNotice::BaselineShown => json!({ "type": "baselineShown" }),
        }
    }
}
