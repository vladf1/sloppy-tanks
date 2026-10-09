//! Protocol constants, room settings and the lobby/control/join records
//! (`src/net/protocol.ts`).
//!
//! Server messages. Low-rate ones are JSON text, in the TypeScript key order:
//!
//! | type         | fields                                                                          |
//! | ------------ | ------------------------------------------------------------------------------- |
//! | `welcome`    | `type version contentVersion roomEpoch playerId token hostId reset`             |
//! | `lobby`      | `roomEpoch roundId type phase hostId players scoreboard settings`               |
//! | `control`    | `roomEpoch roundId type tankId life controlEpoch driver`                        |
//! | `pong`       | `type t tick`                                                                   |
//! | `error`      | `type code message fatal`                                                       |
//! | `room-reset` | `type roomEpoch reason`                                                         |
//!
//! State travels in binary frames whose first byte is the type ([`FULL_MESSAGE`] or
//! [`SNAPSHOT_MESSAGE`]), then varints `roundId tick`; see `replication` for the rest. A
//! snapshot's `tick` is its newest frame's, and its next varint is `ack`, the latest
//! input seq applied for this seat.
//!
//! Client messages carry `type` and `roundId` (the socket already names the room):
//! `join` (see [`JoinRequest`]), `input` (see `player_controls`), `ping {t, observedTick}`,
//! `choose {kind, team?}`, `settings {mapMode, difficulty, humansOnly, roundMinutes?}`,
//! `start`, `end`, `suspend {watch?}`, `resume`, `resync`, `leave`. `suspend` hands the
//! seat's tank to a bot or idles it; with `watch` (the in-battle menu) the seat keeps
//! receiving snapshots and acknowledges them with its pings.

use serde_json::Value;

use super::json::ObjectWriter;
use super::schema::{
    ReadResult, Record, array, boolean, choice, field, id, id32, nested, number_in, optional,
    string,
};
use crate::sim::difficulty::Difficulty;
use crate::sim::map_options::MapId;
use crate::sim::types::{Driver, Team, VehicleKind};

pub use super::room_list::RoomPhase;

/// 2: projectiles as paths sent once; 3: binary `full` and `snapshot` messages with fields
/// as differences.
pub const PROTOCOL_VERSION: u32 = 3;
/// First byte of a binary baseline message.
pub const FULL_MESSAGE: u8 = 1;
/// First byte of a binary snapshot batch.
pub const SNAPSHOT_MESSAGE: u8 = 2;
/// Hash of the sources clients and server must agree on. The build stamps it through
/// `SLOPPY_CONTENT_VERSION`; unstamped builds (tests, `cargo run`) use `test-content`.
pub const CONTENT_VERSION: &str = match option_env!("SLOPPY_CONTENT_VERSION") {
    Some(version) => version,
    None => "test-content",
};
/// Largest client message, in UTF-8 bytes, that a room accepts.
pub const MAX_CLIENT_MESSAGE_BYTES: usize = 4096;
/// Largest server message, in UTF-16 units or bytes, that a client reads.
pub const MAX_SERVER_MESSAGE_BYTES: usize = 1_000_000;
/// A dropped connection keeps its seat, and an emptied room stays, this long.
pub const EMPTY_GRACE_MS: u64 = 30_000;
/// Lobbies and results screens without activity expire after this long.
pub const ROOM_IDLE_MS: u64 = 5 * 60_000;
pub const DEFAULT_ROUND_MINUTES: u32 = 20;
pub const MAX_ROUND_MINUTES: u32 = 99;
/// A room hosts no new battle after this long; one already under way may finish.
pub const MAX_ROOM_MS: u64 = 4 * 60 * 60_000;
/// How far a battle may run past [`MAX_ROOM_MS`]: its longest length plus overtime, so an
/// endless next-kill overtime still cannot hold a room open forever.
pub const MAX_BATTLE_OVERRUN_MS: u64 = (MAX_ROUND_MINUTES as u64 + 30) * 60_000;

/// Room codes are eight characters from an alphabet without look-alikes (`/^[A-Z2-9]{8}$/`).
pub fn is_room_code(code: &str) -> bool {
    code.len() == 8
        && code
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || (b'2'..=b'9').contains(&byte))
}

pub const PLAYER_KINDS: [(&str, VehicleKind); 3] = [
    ("scout", VehicleKind::Scout),
    ("balanced", VehicleKind::Balanced),
    ("heavy", VehicleKind::Heavy),
];
pub const DIFFICULTIES: [(&str, Difficulty); 3] = [
    ("easy", Difficulty::Easy),
    ("normal", Difficulty::Normal),
    ("hard", Difficulty::Hard),
];
pub const MAP_MODES: [(&str, MapId); 5] = [
    ("village", MapId::Village),
    ("harbor", MapId::Harbor),
    ("quarry", MapId::Quarry),
    ("stress-test", MapId::StressTest),
    ("superstress", MapId::Superstress),
];

/// A message to one client: JSON text, or a binary state message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    Text(String),
    Binary(Vec<u8>),
}

impl Message {
    pub fn len(&self) -> usize {
        match self {
            Message::Text(text) => text.len(),
            Message::Binary(bytes) => bytes.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The message's type: the text's `"type"`, or the binary state message's name.
    pub fn binary_type(bytes: &[u8]) -> &'static str {
        match bytes.first() {
            Some(&FULL_MESSAGE) => "full",
            Some(&SNAPSHOT_MESSAGE) => "snapshot",
            _ => "other",
        }
    }
}

/// `team`: 0 or 1.
pub fn read_team(value: Option<&Value>) -> ReadResult<Team> {
    match value.and_then(Value::as_f64) {
        Some(0.0) => Ok(Team::Blue),
        Some(1.0) => Ok(Team::Red),
        _ => Err("Invalid choice".into()),
    }
}

pub fn read_player_kind(value: Option<&Value>) -> ReadResult<VehicleKind> {
    choice(value, &PLAYER_KINDS)
}

pub fn read_map_mode(value: Option<&Value>) -> ReadResult<MapId> {
    choice(value, &MAP_MODES)
}

pub fn read_difficulty(value: Option<&Value>) -> ReadResult<Difficulty> {
    choice(value, &DIFFICULTIES)
}

/// `roundMinutesReader`: whole minutes from 1 to [`MAX_ROUND_MINUTES`].
pub fn read_round_minutes(value: Option<&Value>) -> ReadResult<u32> {
    number_in(value, 1.0, f64::from(MAX_ROUND_MINUTES), true).map(|minutes| minutes as u32)
}

/// The host's choices for the next battle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoomSettings {
    pub map_mode: MapId,
    pub difficulty: Difficulty,
    pub humans_only: bool,
    pub round_minutes: u32,
}

impl Default for RoomSettings {
    fn default() -> Self {
        Self {
            map_mode: MapId::Village,
            difficulty: Difficulty::Normal,
            humans_only: false,
            round_minutes: DEFAULT_ROUND_MINUTES,
        }
    }
}

impl RoomSettings {
    /// `settingsReader`: an omitted `roundMinutes` means the default length.
    pub fn read(source: &Record) -> ReadResult<Self> {
        Ok(Self {
            map_mode: field(source, "mapMode", read_map_mode)?,
            difficulty: field(source, "difficulty", read_difficulty)?,
            humans_only: field(source, "humansOnly", boolean)?,
            round_minutes: field(source, "roundMinutes", |value| match value {
                None => Ok(DEFAULT_ROUND_MINUTES),
                some => read_round_minutes(some),
            })?,
        })
    }

    pub fn write_fields(&self, writer: &mut ObjectWriter<'_>) {
        writer
            .string("mapMode", self.map_mode.as_str())
            .string("difficulty", self.difficulty.as_str())
            .boolean("humansOnly", self.humans_only)
            .int("roundMinutes", u64::from(self.round_minutes));
    }

    pub fn to_json(&self) -> String {
        super::json::object(|writer| self.write_fields(writer))
    }
}

/// A `join` request (`joinReader`).
#[derive(Clone, Debug, PartialEq)]
pub struct JoinRequest {
    pub version: u64,
    pub content_version: String,
    pub name: String,
    pub kind: VehicleKind,
    pub team: Option<Team>,
    pub token: Option<String>,
    pub room_epoch: Option<String>,
    pub create: Option<RoomSettings>,
    pub existing_room: Option<bool>,
}

impl JoinRequest {
    pub fn read(source: &Record) -> ReadResult<Self> {
        Ok(Self {
            version: field(source, "version", id)?,
            content_version: field(source, "contentVersion", |v| string(v, 128, 1))?,
            name: field(source, "name", |v| string(v, 24, 1))?,
            kind: field(source, "kind", read_player_kind)?,
            team: field(source, "team", |v| optional(v, read_team))?,
            token: field(source, "token", |v| optional(v, |v| string(v, 128, 16)))?,
            room_epoch: field(source, "roomEpoch", |v| optional(v, |v| string(v, 128, 1)))?,
            create: field(source, "create", |v| {
                optional(v, |v| nested(v, RoomSettings::read))
            })?,
            existing_room: field(source, "existingRoom", |v| optional(v, boolean))?,
        })
    }
}

/// A seat as the lobby lists it (`Player`).
#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    pub player_id: String,
    pub name: String,
    pub kind: VehicleKind,
    pub team: Team,
    pub slot: u32,
    pub connected: bool,
    pub kills: u32,
    pub deaths: u32,
    pub tank_id: Option<u32>,
}

impl Player {
    /// `playerReader`.
    pub fn read(source: &Record) -> ReadResult<Self> {
        Ok(Self {
            player_id: field(source, "playerId", |v| string(v, 128, 1))?,
            name: field(source, "name", |v| string(v, 24, 1))?,
            team: field(source, "team", read_team)?,
            slot: field(source, "slot", id32)?,
            kind: field(source, "kind", read_player_kind)?,
            connected: field(source, "connected", boolean)?,
            tank_id: field(source, "tankId", |v| optional(v, id32))?,
            kills: field(source, "kills", id32)?,
            deaths: field(source, "deaths", id32)?,
        })
    }

    /// The host's key order: `tankId` joins the record once the seat has a tank.
    pub fn write(&self, out: &mut String) {
        let mut writer = ObjectWriter::new(out);
        writer
            .string("playerId", &self.player_id)
            .string("name", &self.name)
            .string("kind", self.kind.as_str())
            .int("team", self.team.index() as u64)
            .int("slot", u64::from(self.slot))
            .boolean("connected", self.connected)
            .int("kills", u64::from(self.kills))
            .int("deaths", u64::from(self.deaths));
        if let Some(tank) = self.tank_id {
            writer.int("tankId", u64::from(tank));
        }
        writer.finish();
    }
}

/// The room's membership and settings (`Lobby`).
#[derive(Clone, Debug, PartialEq)]
pub struct Lobby {
    pub room_epoch: String,
    pub round_id: u64,
    pub phase: RoomPhase,
    pub host_id: String,
    pub players: Vec<Player>,
    pub scoreboard: Vec<Player>,
    pub settings: RoomSettings,
}

pub const PHASES: [(&str, RoomPhase); 3] = [
    ("lobby", RoomPhase::Lobby),
    ("playing", RoomPhase::Playing),
    ("results", RoomPhase::Results),
];

impl Lobby {
    /// `lobbyReader`.
    pub fn read(source: &Record) -> ReadResult<Self> {
        field(source, "type", |v| choice(v, &[("lobby", ())]))?;
        let player = |value: &Value| nested(Some(value), Player::read);
        Ok(Self {
            room_epoch: field(source, "roomEpoch", |v| string(v, 128, 1))?,
            round_id: field(source, "roundId", id)?,
            phase: field(source, "phase", |v| choice(v, &PHASES))?,
            host_id: field(source, "hostId", |v| string(v, 128, 0))?,
            players: field(source, "players", |v| array(v, 8, player))?,
            scoreboard: field(source, "scoreboard", |v| array(v, 128, player))?,
            settings: field(source, "settings", |v| nested(v, RoomSettings::read))?,
        })
    }

    pub fn to_json(&self) -> String {
        let mut out = String::new();
        let mut writer = ObjectWriter::new(&mut out);
        writer
            .string("roomEpoch", &self.room_epoch)
            .int("roundId", self.round_id)
            .string("type", "lobby")
            .string("phase", self.phase.as_str())
            .string("hostId", &self.host_id);
        write_players(writer.key("players"), &self.players);
        write_players(writer.key("scoreboard"), &self.scoreboard);
        let settings = self.settings.to_json();
        writer.raw("settings", &settings);
        writer.finish();
        out
    }
}

fn write_players(out: &mut String, players: &[Player]) {
    out.push('[');
    for (index, player) in players.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        player.write(out);
    }
    out.push(']');
}

pub const DRIVERS: [(&str, Driver); 3] = [
    ("human", Driver::Human),
    ("bot", Driver::Bot),
    ("idle", Driver::Idle),
];

pub fn driver_name(driver: Driver) -> &'static str {
    match driver {
        Driver::Human => "human",
        Driver::Bot => "bot",
        Driver::Idle => "idle",
    }
}

/// Which tank a seat drives and who drives it now (`Control`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Control {
    pub room_epoch: String,
    pub round_id: u64,
    pub tank_id: u32,
    pub life: u32,
    pub control_epoch: u64,
    pub driver: Driver,
}

impl Control {
    /// `controlReader`.
    pub fn read(source: &Record) -> ReadResult<Self> {
        field(source, "type", |v| choice(v, &[("control", ())]))?;
        Ok(Self {
            room_epoch: field(source, "roomEpoch", |v| string(v, 128, 1))?,
            round_id: field(source, "roundId", id)?,
            tank_id: field(source, "tankId", id32)?,
            life: field(source, "life", id32)?,
            control_epoch: field(source, "controlEpoch", id)?,
            driver: field(source, "driver", |v| choice(v, &DRIVERS))?,
        })
    }

    pub fn to_json(&self) -> String {
        super::json::object(|writer| {
            writer
                .string("roomEpoch", &self.room_epoch)
                .int("roundId", self.round_id)
                .string("type", "control")
                .int("tankId", u64::from(self.tank_id))
                .int("life", u64::from(self.life))
                .int("controlEpoch", self.control_epoch)
                .string("driver", driver_name(self.driver));
        })
    }
}

/// `{"type":"error",...}`; a fatal error also closes the socket with code 1008.
pub fn error_message(code: &str, message: &str, fatal: bool) -> String {
    super::json::object(|writer| {
        writer
            .string("type", "error")
            .string("code", code)
            .string("message", message)
            .boolean("fatal", fatal);
    })
}

pub fn room_reset_message(room_epoch: &str, reason: &str) -> String {
    super::json::object(|writer| {
        writer
            .string("type", "room-reset")
            .string("roomEpoch", room_epoch)
            .string("reason", reason);
    })
}

/// The seat granted by a `join`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Welcome {
    pub version: u32,
    pub content_version: String,
    pub room_epoch: String,
    pub player_id: String,
    pub token: String,
    pub host_id: String,
    /// The client presented a seat from a room instance that no longer exists.
    pub reset: bool,
}

impl Welcome {
    /// Starts with `{"type":"welcome"`, which the server session recognizes.
    pub fn to_json(&self) -> String {
        super::json::object(|writer| {
            writer
                .string("type", "welcome")
                .int("version", u64::from(self.version))
                .string("contentVersion", &self.content_version)
                .string("roomEpoch", &self.room_epoch)
                .string("playerId", &self.player_id)
                .string("token", &self.token)
                .string("hostId", &self.host_id)
                .boolean("reset", self.reset);
        })
    }
}

/// A client message: `{"type":..., "roundId":..., ...fields}` (`Connection.send`).
pub fn client_message(
    kind: &str,
    round_id: u64,
    fields: impl FnOnce(&mut ObjectWriter<'_>),
) -> String {
    super::json::object(|writer| {
        writer.string("type", kind).int("roundId", round_id);
        fields(writer);
    })
}

/// A player's choices for joining a room (`JoinChoice` in `connection.ts`).
#[derive(Clone, Debug, PartialEq)]
pub struct JoinChoice {
    pub name: String,
    pub kind: VehicleKind,
    pub team: Option<Team>,
    pub create: Option<RoomSettings>,
    pub existing_room: Option<bool>,
}

impl JoinChoice {
    /// `pendingChoiceReader`: the choices a page stores before reloading into a room.
    pub fn read(source: &Record) -> ReadResult<Self> {
        Ok(Self {
            name: field(source, "name", |v| string(v, 24, 1))?,
            kind: field(source, "kind", read_player_kind)?,
            team: field(source, "team", |v| optional(v, read_team))?,
            create: field(source, "create", |v| {
                optional(v, |v| nested(v, RoomSettings::read))
            })?,
            existing_room: field(source, "existingRoom", |v| optional(v, boolean))?,
        })
    }

    /// The `join` message for this choice, with a held seat's credentials if any.
    pub fn join_message(&self, token: Option<&str>, room_epoch: Option<&str>) -> String {
        super::json::object(|writer| {
            writer
                .string("type", "join")
                .int("version", u64::from(PROTOCOL_VERSION))
                .string("contentVersion", CONTENT_VERSION)
                .string("name", &self.name)
                .string("kind", self.kind.as_str());
            if let Some(team) = self.team {
                writer.int("team", team.index() as u64);
            }
            if let Some(create) = &self.create {
                let settings = create.to_json();
                writer.raw("create", &settings);
            }
            if let Some(existing) = self.existing_room {
                writer.boolean("existingRoom", existing);
            }
            if let Some(token) = token {
                writer.string("token", token);
            }
            if let Some(epoch) = room_epoch.filter(|epoch| !epoch.is_empty()) {
                writer.string("roomEpoch", epoch);
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn map(value: Value) -> Record {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn round_length_defaults_to_twenty_minutes_and_validates_bounds() {
        let settings = json!({ "mapMode": "village", "difficulty": "normal", "humansOnly": true });
        assert_eq!(
            RoomSettings::read(&map(settings.clone()))
                .unwrap()
                .round_minutes,
            20
        );
        let mut with = map(settings);
        with.insert("roundMinutes".into(), json!(99));
        assert_eq!(RoomSettings::read(&with).unwrap().round_minutes, 99);
        for bad in [json!(0), json!(100), json!(1.5), Value::Null, json!("10")] {
            with.insert("roundMinutes".into(), bad);
            assert_eq!(
                RoomSettings::read(&with),
                Err("roundMinutes: Invalid number".into())
            );
        }
    }

    #[test]
    fn join_errors_name_the_field() {
        let join = map(json!({
            "type": "join", "version": 1, "contentVersion": "x", "name": "A", "kind": "humvee"
        }));
        assert_eq!(JoinRequest::read(&join), Err("kind: Invalid choice".into()));
        let join = map(json!({
            "version": 1, "contentVersion": "x", "name": "A", "kind": "scout",
            "create": { "mapMode": "moon", "difficulty": "easy", "humansOnly": false }
        }));
        assert_eq!(
            JoinRequest::read(&join),
            Err("create: mapMode: Invalid choice".into())
        );
    }

    #[test]
    fn lobby_and_control_round_trip() {
        let lobby = Lobby {
            room_epoch: "e".into(),
            round_id: 2,
            phase: RoomPhase::Playing,
            host_id: "p".into(),
            players: vec![Player {
                player_id: "p".into(),
                name: "Alice".into(),
                kind: VehicleKind::Scout,
                team: Team::Red,
                slot: 1,
                connected: true,
                kills: 3,
                deaths: 0,
                tank_id: Some(4),
            }],
            scoreboard: Vec::new(),
            settings: RoomSettings::default(),
        };
        let text = lobby.to_json();
        assert!(text.starts_with(r#"{"roomEpoch":"e","roundId":2,"type":"lobby""#));
        let parsed: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(Lobby::read(parsed.as_object().unwrap()), Ok(lobby));
        let control = Control {
            room_epoch: "e".into(),
            round_id: 1,
            tank_id: 3,
            life: 2,
            control_epoch: 5,
            driver: Driver::Idle,
        };
        let parsed: Value = serde_json::from_str(&control.to_json()).unwrap();
        assert_eq!(Control::read(parsed.as_object().unwrap()), Ok(control));
    }
}
