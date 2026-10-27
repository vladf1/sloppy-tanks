//! `NetGame`'s JSON for the room shell (`src/net/client.ts`): the socket and storage
//! work `take_actions` asks for, the UI notices `take_notices` reports, and the choice a
//! join sends. Mirrors `ClientAction` and the notices of `crates/web/src/net_game.rs`.

use serde::Deserialize;
use serde_json::{Value, json};
use sloppy_core::net::protocol::{Lobby, MAX_PLAYER_NAME_LENGTH};
use sloppy_core::net::schema::parse_record;
use sloppy_core::sim::bot_personalities::BOT_NAMES;

use crate::engine_api::MatchState;

/// Socket and storage work, in order.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RoomAction {
    Open { socket: u32, url: String },
    Send { socket: u32, text: String },
    Close { socket: u32 },
    SaveSeat { token: String, room_epoch: String },
    ForgetSeat,
}

impl RoomAction {
    /// `take_actions`' array; anything unreadable is dropped.
    pub fn parse_all(json: &str) -> Vec<Self> {
        serde_json::from_str::<Vec<Value>>(json)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| serde_json::from_value(value).ok())
            .collect()
    }
}

/// UI work, in order.
#[derive(Clone, Debug)]
pub enum RoomNotice {
    Status {
        text: String,
        connected: bool,
    },
    Notice(String),
    /// The connection gave up; `cause` is `EndCause::as_str`.
    Ended {
        cause: String,
        text: String,
    },
    Lobby {
        lobby: Box<Lobby>,
        player_id: String,
    },
    /// The finished round as the viewer's tank played it.
    Result {
        outcome: MatchState,
        team: u8,
    },
    ResetFeedback,
    ClearInput,
    /// The room may replace Battle Setup.
    Reveal,
    /// Compile the arena with `prepare_step` until it reports done.
    Prepare,
    ArenaFailed(String),
    BaselineShown,
}

impl RoomNotice {
    fn read(value: &Value) -> Option<Self> {
        let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
        Some(match value.get("type")?.as_str()? {
            "status" => RoomNotice::Status {
                text: text("text")?,
                connected: value.get("connected")?.as_bool()?,
            },
            "notice" => RoomNotice::Notice(text("text")?),
            "ended" => RoomNotice::Ended {
                cause: text("cause")?,
                text: text("text")?,
            },
            "lobby" => {
                let record = parse_record(&value.get("lobby")?.to_string()).ok()?;
                RoomNotice::Lobby {
                    lobby: Box::new(Lobby::read(&record).ok()?),
                    player_id: text("playerId").unwrap_or_default(),
                }
            }
            "result" => RoomNotice::Result {
                outcome: serde_json::from_value(value.get("match")?.clone()).ok()?,
                team: value.get("team")?.as_u64()? as u8,
            },
            "resetFeedback" => RoomNotice::ResetFeedback,
            "clearInput" => RoomNotice::ClearInput,
            "reveal" => RoomNotice::Reveal,
            "prepare" => RoomNotice::Prepare,
            "arenaFailed" => RoomNotice::ArenaFailed(text("error").unwrap_or_default()),
            "baselineShown" => RoomNotice::BaselineShown,
            _ => return None,
        })
    }

    /// `take_notices`' array; anything unreadable is dropped.
    pub fn parse_all(json: &str) -> Vec<Self> {
        serde_json::from_str::<Vec<Value>>(json)
            .unwrap_or_default()
            .iter()
            .filter_map(Self::read)
            .collect()
    }
}

/// The rules a new room opens with, or the host changes (`RoomSettings`).
#[derive(Clone, Debug, PartialEq)]
pub struct RoomRules {
    pub map_mode: String,
    pub difficulty: String,
    pub humans_only: bool,
    pub round_minutes: u32,
}

impl RoomRules {
    pub fn to_json(&self) -> Value {
        json!({
            "mapMode": self.map_mode,
            "difficulty": self.difficulty,
            "humansOnly": self.humans_only,
            "roundMinutes": self.round_minutes,
        })
    }
}

/// The room Battle Setup chose and how to take a seat in it (`RoomSelection`).
#[derive(Clone, Debug, PartialEq)]
pub struct RoomSelection {
    pub room: String,
    /// The WebSocket address of the server that hosts it.
    pub server: String,
    pub name: String,
    /// "scout", "balanced" or "heavy".
    pub kind: String,
    /// `None`: the team with fewer humans.
    pub team: Option<u8>,
    /// The rules of a room this join creates; `None` joins an open room.
    pub create: Option<RoomRules>,
}

impl RoomSelection {
    /// `connect`'s `JoinChoice` JSON.
    pub fn choice_json(&self) -> String {
        let mut choice = json!({ "name": self.name, "kind": self.kind });
        if let Some(team) = self.team {
            choice["team"] = json!(team);
        }
        match &self.create {
            Some(rules) => choice["create"] = rules.to_json(),
            None => choice["existingRoom"] = json!(true),
        }
        choice.to_string()
    }
}

/// A name to play under: the saved one, cut to the longest a room takes, or a random
/// bot name (`preferredPlayerName`).
pub fn preferred_player_name(saved: &str, random: u64) -> String {
    let saved: String = saved.trim().chars().take(MAX_PLAYER_NAME_LENGTH).collect();
    if saved.is_empty() {
        BOT_NAMES[(random % BOT_NAMES.len() as u64) as usize].to_owned()
    } else {
        saved
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::net::protocol::JoinChoice;

    #[test]
    fn actions_read_as_the_client_writes_them() {
        let actions = RoomAction::parse_all(
            r#"[{"type":"open","socket":1,"url":"ws://127.0.0.1:1/room/ABCDEFGH"},
                {"type":"send","socket":1,"text":"{}"},
                {"type":"saveSeat","token":"t","roomEpoch":"e"},
                {"type":"forgetSeat"},{"type":"close","socket":1},{"type":"mystery"}]"#,
        );
        assert_eq!(
            actions,
            vec![
                RoomAction::Open {
                    socket: 1,
                    url: "ws://127.0.0.1:1/room/ABCDEFGH".into()
                },
                RoomAction::Send {
                    socket: 1,
                    text: "{}".into()
                },
                RoomAction::SaveSeat {
                    token: "t".into(),
                    room_epoch: "e".into()
                },
                RoomAction::ForgetSeat,
                RoomAction::Close { socket: 1 },
            ]
        );
        assert!(RoomAction::parse_all("not json").is_empty());
    }

    #[test]
    fn notices_carry_the_lobby_and_the_result() {
        let lobby = r#"{"roomEpoch":"e1","roundId":2,"type":"lobby","phase":"results","hostId":"p1",
            "players":[{"playerId":"p1","name":"NITRO","kind":"heavy","team":1,"slot":0,
            "connected":true,"kills":3,"deaths":1,"tankId":4}],"scoreboard":[],
            "settings":{"mapMode":"quarry","difficulty":"easy","humansOnly":false,"roundMinutes":15}}"#;
        let json = format!(
            r#"[{{"type":"status","text":"Connected","connected":true}},
                {{"type":"lobby","lobby":{lobby},"playerId":"p1"}},
                {{"type":"result","match":{{"phase":"results","time":0,"scores":[100,40],
                  "overtime":false,"winner":1,"round":2}},"team":1}},
                {{"type":"ended","cause":"lost","text":"Gone"}},{{"type":"prepare"}}]"#
        );
        let notices = RoomNotice::parse_all(&json);
        assert_eq!(notices.len(), 5);
        let RoomNotice::Lobby { lobby, player_id } = &notices[1] else {
            panic!("a lobby");
        };
        assert_eq!(player_id, "p1");
        assert_eq!(lobby.players[0].name, "NITRO");
        assert_eq!(lobby.settings.round_minutes, 15);
        let RoomNotice::Result { outcome, team } = &notices[2] else {
            panic!("a result");
        };
        assert_eq!((outcome.winner, *team), (Some(1), 1));
        assert!(matches!(notices[4], RoomNotice::Prepare));
    }

    #[test]
    fn a_selection_makes_a_choice_the_engine_accepts() {
        let mut selection = RoomSelection {
            room: "ABCDEFGH".into(),
            server: "ws://127.0.0.1:1".into(),
            name: "RIVET".into(),
            kind: "scout".into(),
            team: Some(1),
            create: Some(RoomRules {
                map_mode: "harbor".into(),
                difficulty: "normal".into(),
                humans_only: true,
                round_minutes: 25,
            }),
        };
        let read = |selection: &RoomSelection| {
            JoinChoice::read(&parse_record(&selection.choice_json()).unwrap()).unwrap()
        };
        let created = read(&selection);
        assert_eq!(created.create.unwrap().round_minutes, 25);
        assert_eq!(created.existing_room, None);
        selection.create = None;
        selection.team = None;
        let joined = read(&selection);
        assert_eq!((joined.team, joined.existing_room), (None, Some(true)));
    }

    #[test]
    fn a_saved_name_wins_over_a_random_one() {
        assert_eq!(preferred_player_name("  Vlad ", 3), "Vlad");
        assert_eq!(preferred_player_name(&"x".repeat(30), 3).len(), 24);
        assert!(BOT_NAMES.contains(&preferred_player_name(" ", 12345).as_str()));
    }
}
