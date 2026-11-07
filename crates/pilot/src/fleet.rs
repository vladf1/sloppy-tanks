//! Pilots behind plain calls that take and give JSON text and bytes: the bots Worker's
//! engine (`abi` exports these to JavaScript from Wasm). One fleet holds every pilot of a
//! Worker region, each under a handle.
//!
//! A setup is the JSON the Worker writes:
//!
//! ```json
//! {"serverUrl": "wss://…", "room": "ABCD2345", "name": "pilot-wnam-1",
//!  "orders": {"mission": "hunt:vlad", "skill": "normal", "kind": "balanced"},
//!  "create": {"mapMode": "village", "difficulty": "normal", "humansOnly": false,
//!             "roundMinutes": 5},
//!  "savedSeat": {"token": "…", "roomEpoch": "…"}, "seed": 7}
//! ```
//!
//! `orders`, `create`, `savedSeat` and `seed` may be left out. [`Fleet::tick`] answers
//! `{"actions": [...], "ended": null | {"cause", "text"}}`, the actions serialized as
//! [`ClientAction`] (`open`, `send`, `close`, `saveSeat`, `forgetSeat`) with the pilot's
//! socket ids.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sloppy_core::net::client::{ClientAction, SavedSeat};
use sloppy_core::net::protocol::{
    CONTENT_VERSION, MAX_PLAYER_NAME_LENGTH, PROTOCOL_VERSION, RoomSettings, is_room_code,
};

use crate::orders::Orders;
use crate::pilot::{Ended, Pilot, PilotConfig};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PilotSetup {
    server_url: String,
    room: String,
    name: String,
    #[serde(default)]
    orders: Orders,
    #[serde(default)]
    create: Option<Value>,
    #[serde(default)]
    saved_seat: Option<SavedSeat>,
    #[serde(default)]
    seed: u32,
}

/// A pilot's configuration from its JSON setup, checked as the server would check a join.
pub fn read_setup(text: &str) -> Result<PilotConfig, String> {
    let setup: PilotSetup =
        serde_json::from_str(text).map_err(|error| format!("Invalid pilot setup: {error}"))?;
    if !(setup.server_url.starts_with("ws://") || setup.server_url.starts_with("wss://")) {
        return Err(format!(
            "serverUrl must be a ws:// or wss:// address, not {:?}",
            setup.server_url
        ));
    }
    if !is_room_code(&setup.room) {
        return Err(format!("{:?} is not a room code", setup.room));
    }
    let name = setup.name.trim();
    if name.is_empty() || name.chars().count() > MAX_PLAYER_NAME_LENGTH {
        return Err(format!(
            "A pilot's name has 1 to {MAX_PLAYER_NAME_LENGTH} characters"
        ));
    }
    let create = match setup.create {
        Some(Value::Object(record)) => Some(RoomSettings::read(&record)?),
        Some(Value::Null) | None => None,
        Some(_) => return Err("create must be room settings".into()),
    };
    Ok(PilotConfig {
        server_url: setup.server_url,
        room: setup.room,
        name: name.to_string(),
        orders: setup.orders,
        create,
        saved_seat: setup.saved_seat,
        seed: setup.seed,
    })
}

/// The protocol and content versions this engine joins with; a server must report the
/// same in `/health` for its rooms to accept the pilots.
pub fn versions() -> String {
    json!({ "protocol": PROTOCOL_VERSION, "contentVersion": CONTENT_VERSION }).to_string()
}

#[derive(Default)]
pub struct Fleet {
    pilots: BTreeMap<u32, Pilot>,
    next: u32,
}

impl Fleet {
    /// Starts a pilot from its JSON setup; its first socket opens on the next tick.
    pub fn create(&mut self, setup: &str, now_ms: f64) -> Result<u32, String> {
        let mut pilot = Pilot::new(read_setup(setup)?);
        pilot.start(now_ms);
        self.next += 1;
        self.pilots.insert(self.next, pilot);
        Ok(self.next)
    }

    pub fn len(&self) -> usize {
        self.pilots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pilots.is_empty()
    }

    pub fn opened(&mut self, handle: u32, socket: u32, now_ms: f64) {
        if let Some(pilot) = self.pilots.get_mut(&handle) {
            pilot.socket_opened(socket, now_ms);
        }
    }

    pub fn text(&mut self, handle: u32, socket: u32, text: &str, now_ms: f64) {
        if let Some(pilot) = self.pilots.get_mut(&handle) {
            pilot.socket_text(socket, text, now_ms);
        }
    }

    pub fn binary(&mut self, handle: u32, socket: u32, bytes: &[u8], now_ms: f64) {
        if let Some(pilot) = self.pilots.get_mut(&handle) {
            pilot.socket_binary(socket, bytes, now_ms);
        }
    }

    pub fn closed(&mut self, handle: u32, socket: u32, code: u16, now_ms: f64) {
        if let Some(pilot) = self.pilots.get_mut(&handle) {
            pilot.socket_closed(socket, code, now_ms);
        }
    }

    /// One frame of a pilot, and what its runner must do now, written over `output`.
    pub fn tick(&mut self, handle: u32, now_ms: f64, output: &mut Vec<u8>) {
        output.clear();
        let Some(pilot) = self.pilots.get_mut(&handle) else {
            output.extend_from_slice(NO_PILOT.as_bytes());
            return;
        };
        pilot.tick(now_ms);
        write_step(output, pilot.actions(), pilot.ended());
        pilot.clear_actions();
    }

    /// Gives a pilot's seat up; the actions, written over `output`, close its socket.
    pub fn leave(&mut self, handle: u32, now_ms: f64, output: &mut Vec<u8>) {
        output.clear();
        match self.pilots.get_mut(&handle) {
            Some(pilot) => {
                pilot.leave(now_ms);
                write_step(output, pilot.actions(), None);
                pilot.clear_actions();
            }
            None => write_step(output, &[], None),
        }
    }

    pub fn report(&self, handle: u32, now_ms: f64) -> String {
        self.pilots
            .get(&handle)
            .map_or(Value::Null, |pilot| {
                serde_json::to_value(pilot.report(now_ms)).unwrap_or(Value::Null)
            })
            .to_string()
    }

    pub fn remove(&mut self, handle: u32) {
        self.pilots.remove(&handle);
    }
}

/// A frame with nothing to do, the usual answer.
const NO_ACTIONS: &str = r#"{"actions":[],"ended":null}"#;
const NO_PILOT: &str = r#"{"actions":[],"ended":{"cause":"gone","text":"No such pilot"}}"#;

#[derive(Serialize)]
struct Step<'a> {
    actions: &'a [ClientAction],
    ended: Option<StepEnd<'a>>,
}

#[derive(Serialize)]
struct StepEnd<'a> {
    cause: &'a str,
    text: &'a str,
}

/// Appends a step's answer to `output`: a constant when nothing happened, otherwise the
/// actions serialized where they are.
fn write_step(output: &mut Vec<u8>, actions: &[ClientAction], ended: Option<&Ended>) {
    if actions.is_empty() && ended.is_none() {
        output.extend_from_slice(NO_ACTIONS.as_bytes());
        return;
    }
    let step = Step {
        actions,
        ended: ended.map(|ended| StepEnd {
            cause: ended.cause.as_str(),
            text: &ended.text,
        }),
    };
    serde_json::to_writer(output, &step).expect("a step serializes");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orders::Mission;

    #[test]
    fn setups_are_checked_like_joins() {
        let config = read_setup(
            r#"{"serverUrl":"wss://example.com","room":"ABCD2345","name":" pilot-1 ",
                "orders":{"mission":"hunt:vlad"},
                "create":{"mapMode":"harbor","difficulty":"hard","humansOnly":false,"roundMinutes":5}}"#,
        )
        .unwrap();
        assert_eq!(config.name, "pilot-1");
        assert_eq!(config.orders.mission, Mission::Hunt(Some("vlad".into())));
        let create = config.create.unwrap();
        assert_eq!(create.map_mode.as_str(), "harbor");
        assert_eq!(create.round_minutes, 5);
        for bad in [
            r#"{"serverUrl":"https://example.com","room":"ABCD2345","name":"p"}"#,
            r#"{"serverUrl":"ws://example.com","room":"abcd","name":"p"}"#,
            r#"{"serverUrl":"ws://example.com","room":"ABCD2345","name":"  "}"#,
            r#"{"serverUrl":"ws://example.com","room":"ABCD2345","name":"p","create":{"mapMode":"moon"}}"#,
        ] {
            assert!(read_setup(bad).is_err(), "{bad}");
        }
    }

    fn step(fleet: &mut Fleet, handle: u32, now_ms: f64) -> Value {
        let mut output = b"stale".to_vec();
        fleet.tick(handle, now_ms, &mut output);
        serde_json::from_slice(&output).unwrap()
    }

    #[test]
    fn a_new_pilot_opens_its_socket_and_joins_once_it_is_open() {
        let mut fleet = Fleet::default();
        let handle = fleet
            .create(
                r#"{"serverUrl":"ws://127.0.0.1:8787","room":"ABCD2345","name":"pilot"}"#,
                0.0,
            )
            .unwrap();
        let first = step(&mut fleet, handle, 16.0);
        let open = &first["actions"][0];
        assert_eq!(open["type"], "open");
        assert_eq!(open["url"], "ws://127.0.0.1:8787/room/ABCD2345");
        let socket = open["socket"].as_u64().unwrap() as u32;
        fleet.opened(handle, socket, 20.0);
        let next = step(&mut fleet, handle, 33.0);
        let join: Value =
            serde_json::from_str(next["actions"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(join["type"], "join");
        assert_eq!(join["name"], "pilot");
        assert_eq!(next["ended"], Value::Null);
        assert_eq!(
            step(&mut fleet, handle, 50.0),
            json!({ "actions": [], "ended": null }),
            "a quiet frame"
        );
        let report: Value = serde_json::from_str(&fleet.report(handle, 40.0)).unwrap();
        assert_eq!(report["phase"], "connecting");
        assert_eq!(report["mission"], "skirmish");
        let mut output = Vec::new();
        fleet.leave(handle, 60.0, &mut output);
        let left: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(left["ended"], Value::Null);
        assert!(
            left["actions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|action| action["type"] == "close" && action["socket"] == socket),
            "{left}"
        );
        fleet.remove(handle);
        assert!(fleet.is_empty());
        assert_eq!(
            step(&mut fleet, handle, 70.0),
            json!({ "actions": [], "ended": { "cause": "gone", "text": "No such pilot" } })
        );
    }

    #[test]
    fn steps_serialize_like_their_values() {
        let actions = [
            ClientAction::Send {
                socket: 3,
                text: "{\"type\":\"leave\"}".into(),
            },
            ClientAction::CloseSocket { socket: 3 },
            ClientAction::ForgetSeat,
        ];
        let ended = Ended {
            cause: sloppy_core::net::client::EndCause::Rejected,
            text: "Room is full".into(),
        };
        let mut output = Vec::new();
        write_step(&mut output, &actions, Some(&ended));
        let written: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(
            written,
            json!({
                "actions": actions,
                "ended": { "cause": ended.cause.as_str(), "text": ended.text },
            })
        );
    }
}
