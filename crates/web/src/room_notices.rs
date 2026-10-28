//! The UI notices a room page reads from `NetGame.take_notices`: the client's notices
//! for the page with the room menu's words ([`RoomText`]), and `NetGame`'s own about
//! preparing the arena.

use serde_json::{Value, json};
use sloppy_core::net::client::ClientNotice;
use sloppy_core::net::protocol::Lobby;
use sloppy_core::sim::{Match, Team};

use crate::Shell;
use crate::room_text::{Arrival, ConnectionDialog, Ending, MenuText, RoomText};

/// UI work for the room page, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum RoomNotice {
    /// Progress while (re)connecting. `dialog` holds the connection dialog's words while
    /// the socket is down; `None` once the seat is back.
    Status {
        text: String,
        dialog: Option<ConnectionDialog>,
    },
    /// A server answer to the player's last request, such as a full team.
    Notice(String),
    /// The connection gave up and will not retry on its own, or the renderer stopped.
    Ended(Ending),
    /// The room's membership and settings, this player's id in it, the menu's words for
    /// it and who joined or came back since the last lobby.
    Lobby {
        lobby: Box<Lobby>,
        player_id: String,
        menu: MenuText,
        arrivals: Vec<Arrival>,
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
    /// A client notice for the page, with the menu's words from `room_text`, or `None`
    /// for the arena and baseline requests `NetGame` handles itself.
    pub fn from_client(
        notice: ClientNotice,
        player_id: &str,
        room_text: &mut RoomText,
        shell: Shell,
    ) -> Option<Self> {
        Some(match notice {
            ClientNotice::Status { text, connected } => {
                let dialog = room_text.status(&text, connected);
                RoomNotice::Status { text, dialog }
            }
            ClientNotice::Notice(text) => RoomNotice::Notice(text),
            ClientNotice::Ended { cause, text } => {
                RoomNotice::Ended(room_text.ended(Some(cause), text, shell))
            }
            ClientNotice::Lobby(lobby) => {
                let (menu, arrivals) = room_text.lobby(&lobby, player_id);
                RoomNotice::Lobby {
                    lobby: Box::new(lobby),
                    player_id: player_id.to_owned(),
                    menu,
                    arrivals,
                }
            }
            ClientNotice::Result { outcome, team } => {
                room_text.result(&outcome, team);
                RoomNotice::Result { outcome, team }
            }
            ClientNotice::ResetFeedback => RoomNotice::ResetFeedback,
            ClientNotice::ClearInput => RoomNotice::ClearInput,
            ClientNotice::Reveal => RoomNotice::Reveal,
            ClientNotice::PrepareArena { .. } | ClientNotice::ShowBaseline { .. } => return None,
        })
    }

    /// The notice as the page reads it: `{type, ...}`.
    pub fn to_value(&self) -> Value {
        match self {
            RoomNotice::Status { text, dialog } => {
                let mut value =
                    json!({ "type": "status", "text": text, "connected": dialog.is_none() });
                if let Some(dialog) = dialog {
                    value["title"] = dialog.title.into();
                    value["message"] = dialog.message.as_str().into();
                }
                value
            }
            RoomNotice::Notice(text) => json!({ "type": "notice", "text": text }),
            RoomNotice::Ended(ending) => json!({
                "type": "ended",
                "text": ending.text,
                "title": ending.title,
                "retry": ending.retry,
                "setup": ending.setup,
            }),
            RoomNotice::Lobby {
                lobby,
                player_id,
                menu,
                arrivals,
            } => {
                let lobby: Value = serde_json::from_str(&lobby.to_json()).unwrap_or_default();
                json!({
                    "type": "lobby",
                    "lobby": lobby,
                    "playerId": player_id,
                    "menu": menu,
                    "arrivals": arrivals,
                })
            }
            RoomNotice::Result { outcome, .. } => json!({ "type": "result", "match": outcome }),
            RoomNotice::ResetFeedback => json!({ "type": "resetFeedback" }),
            RoomNotice::ClearInput => json!({ "type": "clearInput" }),
            RoomNotice::Reveal => json!({ "type": "reveal" }),
            RoomNotice::Prepare => json!({ "type": "prepare" }),
            RoomNotice::ArenaFailed(error) => json!({ "type": "arenaFailed", "error": error }),
            RoomNotice::BaselineShown => json!({ "type": "baselineShown" }),
        }
    }
}
