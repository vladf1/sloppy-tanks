//! A lobby-only [`RoomHost`]: seats, tokens, host transfer, room lifetime and the
//! lobby/ping protocol of `MatchHost`, without a simulation.
//!
//! It stands in for the `MatchHost` port so the server can be built, tested and smoke
//! checked on its own. A started "battle" only changes the phase; no tanks, snapshots or
//! control messages exist. Replace it with the real host once `sloppy_core::net` has one.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::host::{ConnectionId, HostOptions, HostOutput, RoomHost};
use crate::protocol::{
    DEFAULT_ROUND_MINUTES, EMPTY_GRACE_MS, MAX_BATTLE_OVERRUN_MS, MAX_ROOM_MS, MAX_ROUND_MINUTES,
    PROTOCOL_VERSION, ROOM_IDLE_MS,
};
use crate::room_list::{RoomListing, RoomPhase};

const MAX_PLAYERS: usize = 8;
const TEAM_SLOTS: u32 = 6;
const MAX_MESSAGES_PER_SECOND: u32 = 60;
const MAP_IDS: [&str; 5] = ["village", "harbor", "quarry", "stress-test", "superstress"];
const DIFFICULTIES: [&str; 3] = ["easy", "normal", "hard"];
const PLAYER_KINDS: [&str; 3] = ["scout", "balanced", "heavy"];

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    map_mode: String,
    difficulty: String,
    humans_only: bool,
    round_minutes: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            map_mode: "village".into(),
            difficulty: "normal".into(),
            humans_only: false,
            round_minutes: DEFAULT_ROUND_MINUTES,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Player {
    player_id: String,
    name: String,
    team: u8,
    slot: u32,
    kind: String,
    connected: bool,
    kills: u32,
    deaths: u32,
}

struct Seat {
    player: Player,
    token: String,
    connection: Option<ConnectionId>,
    disconnected_ms: Option<u64>,
}

struct Client {
    player_id: String,
    window_ms: u64,
    count: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Welcome<'a> {
    r#type: &'static str,
    version: u32,
    content_version: &'a str,
    room_epoch: &'a str,
    player_id: &'a str,
    token: &'a str,
    host_id: &'a str,
    reset: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Lobby<'a> {
    room_epoch: &'a str,
    round_id: u64,
    r#type: &'static str,
    phase: RoomPhase,
    host_id: &'a str,
    players: Vec<&'a Player>,
    scoreboard: [Player; 0],
    settings: &'a Settings,
}

#[derive(Serialize)]
struct ErrorMessage<'a> {
    r#type: &'static str,
    code: &'a str,
    message: &'a str,
    fatal: bool,
}

#[derive(Serialize)]
struct Pong<'a> {
    r#type: &'static str,
    t: &'a Value,
    tick: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RoomReset<'a> {
    r#type: &'static str,
    room_epoch: &'a str,
    reason: &'a str,
}

type Invalid = String;

pub struct LobbyHost {
    options: HostOptions,
    settings: Settings,
    phase: RoomPhase,
    round_id: u64,
    host_id: String,
    disposed: bool,
    dispose_reason: Option<String>,
    seats: Vec<Seat>,
    clients: BTreeMap<ConnectionId, Client>,
    empty_since_ms: Option<u64>,
    active_ms: u64,
}

impl LobbyHost {
    pub fn new(options: HostOptions) -> Self {
        let now_ms = options.now_ms;
        Self {
            options,
            settings: Settings::default(),
            phase: RoomPhase::Lobby,
            round_id: 0,
            host_id: String::new(),
            disposed: false,
            dispose_reason: None,
            seats: Vec::new(),
            clients: BTreeMap::new(),
            empty_since_ms: Some(now_ms),
            active_ms: now_ms,
        }
    }

    fn error(
        &mut self,
        out: &mut HostOutput,
        connection: ConnectionId,
        code: &str,
        message: &str,
        fatal: bool,
    ) {
        let error = ErrorMessage {
            r#type: "error",
            code,
            message,
            fatal,
        };
        out.send(
            connection,
            serde_json::to_string(&error).expect("error serializes"),
        );
        if fatal {
            out.close(connection, 1008, code);
        }
    }

    fn seat_of(&self, player_id: &str) -> Option<usize> {
        self.seats
            .iter()
            .position(|seat| seat.player.player_id == player_id)
    }

    fn free_slot(&self, team: u8, except: Option<usize>) -> Option<u32> {
        (0..TEAM_SLOTS).find(|slot| {
            !self.seats.iter().enumerate().any(|(index, seat)| {
                Some(index) != except && seat.player.team == team && seat.player.slot == *slot
            })
        })
    }

    fn auto_team(&self, except: Option<usize>) -> u8 {
        let count = |team: u8| {
            self.seats
                .iter()
                .enumerate()
                .filter(|(index, seat)| Some(*index) != except && seat.player.team == team)
                .count()
        };
        if count(0) <= count(1) { 0 } else { 1 }
    }

    fn broadcast_lobby(&self, out: &mut HostOutput) {
        let lobby = Lobby {
            room_epoch: &self.options.room_epoch,
            round_id: self.round_id,
            r#type: "lobby",
            phase: self.phase,
            host_id: &self.host_id,
            players: self.seats.iter().map(|seat| &seat.player).collect(),
            scoreboard: [],
            settings: &self.settings,
        };
        let body = serde_json::to_string(&lobby).expect("lobby serializes");
        for connection in self.clients.keys() {
            out.send(*connection, body.clone());
        }
        out.changed();
    }

    fn handle(
        &mut self,
        connection: ConnectionId,
        text: &str,
        now_ms: u64,
        out: &mut HostOutput,
    ) -> Result<(), Invalid> {
        let value: Value = serde_json::from_str(text).map_err(|_| "Invalid JSON".to_string())?;
        let message = value.as_object().ok_or("Expected object")?;
        let kind = message.get("type").and_then(Value::as_str).unwrap_or("");
        if kind == "join" {
            if self.clients.contains_key(&connection) {
                return Err("Already joined".into());
            }
            return self.join(connection, message, now_ms, out);
        }
        let client = self.clients.get_mut(&connection).ok_or("Join required")?;
        if now_ms.saturating_sub(client.window_ms) >= 1000 {
            client.window_ms = now_ms;
            client.count = 0;
        }
        client.count += 1;
        if client.count > MAX_MESSAGES_PER_SECOND {
            return Err("Message rate exceeded".into());
        }
        let player_id = client.player_id.clone();
        if message.get("roundId").and_then(Value::as_u64) != Some(self.round_id) {
            return Ok(());
        }
        let seat = self.seat_of(&player_id).ok_or("Join required")?;
        let is_host = self.host_id == player_id;
        match kind {
            "ping" => {
                // Echo the client's own number, so `5` stays `5` rather than `5.0`.
                let t = message
                    .get("t")
                    .filter(|t| t.as_f64().is_some_and(|t| t >= 0.0))
                    .ok_or("Invalid number")?;
                if message
                    .get("observedTick")
                    .and_then(Value::as_u64)
                    .ok_or("Invalid number")?
                    > 0
                {
                    return Err("Invalid observed tick".into());
                }
                let pong = Pong {
                    r#type: "pong",
                    t,
                    tick: 0,
                };
                out.send(
                    connection,
                    serde_json::to_string(&pong).expect("pong serializes"),
                );
            }
            "choose" => {
                if self.phase == RoomPhase::Playing {
                    return Err("Choices are locked during a round".into());
                }
                let kind = choice(message.get("kind"), &PLAYER_KINDS)?;
                let team = match message.get("team") {
                    None => self.auto_team(Some(seat)),
                    Some(team) => read_team(team)?,
                };
                let Some(slot) = self.free_slot(team, Some(seat)) else {
                    self.error(
                        out,
                        connection,
                        "team-full",
                        "That team has six reserved seats.",
                        false,
                    );
                    return Ok(());
                };
                let player = &mut self.seats[seat].player;
                (player.team, player.slot, player.kind) = (team, slot, kind);
                self.active_ms = now_ms;
                self.broadcast_lobby(out);
            }
            "settings" => {
                if !is_host || self.phase == RoomPhase::Playing {
                    return Err("Only the lobby host can change settings".into());
                }
                self.settings = read_settings(message)?;
                self.active_ms = now_ms;
                self.broadcast_lobby(out);
            }
            "start" => {
                if !is_host || self.phase == RoomPhase::Playing {
                    return Err("Only the lobby host can start".into());
                }
                self.start(now_ms, out);
            }
            "end" => {
                if !is_host || self.phase != RoomPhase::Playing {
                    return Err("Only the host can end the round".into());
                }
                self.phase = RoomPhase::Results;
                self.active_ms = now_ms;
                self.broadcast_lobby(out);
            }
            "suspend" | "resume" | "resync" | "input" => {}
            "leave" => {
                self.disconnect(connection, now_ms, out);
                self.seats.remove(seat);
                self.broadcast_lobby(out);
                out.close(connection, 1000, "Left room");
                if self.seats.is_empty() {
                    self.dispose("empty", out);
                }
            }
            _ => return Err("Unknown message".into()),
        }
        Ok(())
    }

    fn join(
        &mut self,
        connection: ConnectionId,
        message: &Map<String, Value>,
        now_ms: u64,
        out: &mut HostOutput,
    ) -> Result<(), Invalid> {
        let version = message
            .get("version")
            .and_then(Value::as_u64)
            .ok_or("version: Invalid number")?;
        let content = text(message.get("contentVersion"), 1, 128)
            .map_err(|error| format!("contentVersion: {error}"))?;
        let name = text(message.get("name"), 1, 24).map_err(|error| format!("name: {error}"))?;
        let kind =
            choice(message.get("kind"), &PLAYER_KINDS).map_err(|error| format!("kind: {error}"))?;
        let team = message.get("team").map(read_team).transpose()?;
        let token = message
            .get("token")
            .map(|token| text(Some(token), 16, 128))
            .transpose()?;
        let room_epoch = message
            .get("roomEpoch")
            .map(|epoch| text(Some(epoch), 1, 128))
            .transpose()?;
        let create = match message.get("create") {
            Some(Value::Object(settings)) => Some(read_settings(settings)?),
            Some(_) => return Err("create: Expected object".into()),
            None => None,
        };
        let existing_room = match message.get("existingRoom") {
            Some(value) => value.as_bool().ok_or("existingRoom: Invalid boolean")?,
            None => false,
        };
        if version != u64::from(PROTOCOL_VERSION) || content != self.options.content_version {
            self.error(
                out,
                connection,
                "incompatible",
                "Game updated. Reload this page before joining.",
                true,
            );
            return Ok(());
        }
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err("Enter a player name".into());
        }
        let found = token
            .as_ref()
            .and_then(|token| self.seats.iter().position(|seat| &seat.token == token));
        let creating = create.is_some() && found.is_none() && room_epoch.is_none();
        if creating && !self.seats.is_empty() {
            self.error(
                out,
                connection,
                "room-exists",
                "That room code is already in use. Create another room.",
                true,
            );
            return Ok(());
        }
        if existing_room && found.is_none() && self.seats.is_empty() {
            self.error(out, connection, "room-gone", "This room has ended.", true);
            return Ok(());
        }
        if token.is_some()
            && room_epoch.as_deref() == Some(&self.options.room_epoch)
            && found.is_none()
        {
            self.error(
                out,
                connection,
                "seat-expired",
                "Your reserved seat expired. Join again to take a new seat.",
                true,
            );
            return Ok(());
        }
        let reset = room_epoch
            .as_ref()
            .is_some_and(|epoch| *epoch != self.options.room_epoch);
        let seat = match found {
            None => {
                if self.seats.len() >= MAX_PLAYERS {
                    self.error(
                        out,
                        connection,
                        "room-full",
                        "This room has eight reserved player seats.",
                        true,
                    );
                    return Ok(());
                }
                let preferred = team.unwrap_or_else(|| self.auto_team(None));
                let side = if team.is_none() && self.free_slot(preferred, None).is_none() {
                    1 - preferred
                } else {
                    preferred
                };
                let Some(slot) = self.free_slot(side, None) else {
                    self.error(
                        out,
                        connection,
                        "team-full",
                        "That team has six reserved seats.",
                        true,
                    );
                    return Ok(());
                };
                let player = Player {
                    player_id: (self.options.token)(),
                    name,
                    team: side,
                    slot,
                    kind,
                    connected: true,
                    kills: 0,
                    deaths: 0,
                };
                let token = (self.options.token)();
                self.seats.push(Seat {
                    player,
                    token,
                    connection: Some(connection),
                    disconnected_ms: None,
                });
                self.seats.len() - 1
            }
            Some(index) => {
                if let Some(old) = self.seats[index].connection.take() {
                    self.clients.remove(&old);
                    out.close(old, 4001, "Seat reconnected elsewhere");
                }
                let seat = &mut self.seats[index];
                seat.connection = Some(connection);
                seat.player.connected = true;
                seat.disconnected_ms = None;
                index
            }
        };
        let player_id = self.seats[seat].player.player_id.clone();
        self.clients.insert(
            connection,
            Client {
                player_id: player_id.clone(),
                window_ms: now_ms,
                count: 0,
            },
        );
        self.empty_since_ms = None;
        self.active_ms = now_ms;
        if self.host_id.is_empty() {
            self.host_id = player_id;
        }
        let welcome = Welcome {
            r#type: "welcome",
            version: PROTOCOL_VERSION,
            content_version: &self.options.content_version,
            room_epoch: &self.options.room_epoch,
            player_id: &self.seats[seat].player.player_id,
            token: &self.seats[seat].token,
            host_id: &self.host_id,
            reset,
        };
        out.send(
            connection,
            serde_json::to_string(&welcome).expect("welcome serializes"),
        );
        self.broadcast_lobby(out);
        if let Some(settings) = create.filter(|_| creating) {
            self.settings = settings;
            self.start(now_ms, out);
        }
        Ok(())
    }

    fn start(&mut self, now_ms: u64, out: &mut HostOutput) {
        self.round_id += 1;
        self.phase = RoomPhase::Playing;
        self.active_ms = now_ms;
        self.broadcast_lobby(out);
    }
}

fn text(value: Option<&Value>, min: usize, max: usize) -> Result<String, Invalid> {
    let text = value.and_then(Value::as_str).ok_or("Invalid text")?;
    // The TypeScript readers measure UTF-16 length.
    let length = text.encode_utf16().count();
    if length < min || length > max {
        return Err("Invalid text".into());
    }
    Ok(text.to_string())
}

fn choice(value: Option<&Value>, options: &[&str]) -> Result<String, Invalid> {
    value
        .and_then(Value::as_str)
        .filter(|value| options.contains(value))
        .map(str::to_string)
        .ok_or_else(|| "Invalid choice".into())
}

fn read_team(value: &Value) -> Result<u8, Invalid> {
    match value.as_u64() {
        Some(team @ 0..=1) => Ok(team as u8),
        _ => Err("Invalid choice".into()),
    }
}

fn read_settings(message: &Map<String, Value>) -> Result<Settings, Invalid> {
    let round_minutes = match message.get("roundMinutes") {
        None => DEFAULT_ROUND_MINUTES,
        Some(value) => value
            .as_u64()
            .filter(|minutes| (1..=u64::from(MAX_ROUND_MINUTES)).contains(minutes))
            .ok_or("roundMinutes: Invalid number")? as u32,
    };
    Ok(Settings {
        map_mode: choice(message.get("mapMode"), &MAP_IDS)
            .map_err(|error| format!("mapMode: {error}"))?,
        difficulty: choice(message.get("difficulty"), &DIFFICULTIES)
            .map_err(|error| format!("difficulty: {error}"))?,
        humans_only: message
            .get("humansOnly")
            .and_then(Value::as_bool)
            .ok_or("humansOnly: Invalid boolean")?,
        round_minutes,
    })
}

impl RoomHost for LobbyHost {
    fn receive(&mut self, connection: ConnectionId, text: &str, now_ms: u64, out: &mut HostOutput) {
        if self.disposed {
            self.error(
                out,
                connection,
                "expired",
                "This room has expired. Create a new room.",
                true,
            );
            return;
        }
        if let Err(message) = self.handle(connection, text, now_ms, out) {
            self.disconnect(connection, now_ms, out);
            self.error(out, connection, "invalid-message", &message, true);
        }
    }

    fn disconnect(&mut self, connection: ConnectionId, now_ms: u64, out: &mut HostOutput) {
        let Some(client) = self.clients.remove(&connection) else {
            return;
        };
        if let Some(index) = self.seat_of(&client.player_id) {
            let seat = &mut self.seats[index];
            seat.connection = None;
            seat.disconnected_ms = Some(now_ms);
            seat.player.connected = false;
        }
        if self.host_id == client.player_id {
            self.host_id = self
                .seats
                .iter()
                .find(|seat| seat.connection.is_some())
                .map(|seat| seat.player.player_id.clone())
                .unwrap_or_default();
        }
        if self.clients.is_empty() {
            self.empty_since_ms = Some(now_ms);
        }
        self.broadcast_lobby(out);
    }

    fn advance(&mut self, now_ms: u64, out: &mut HostOutput) {
        if self.disposed {
            return;
        }
        let age = now_ms.saturating_sub(self.options.now_ms);
        let overrun = if self.phase == RoomPhase::Playing {
            MAX_BATTLE_OVERRUN_MS
        } else {
            0
        };
        if self
            .empty_since_ms
            .is_some_and(|since| now_ms.saturating_sub(since) >= EMPTY_GRACE_MS)
            || age >= MAX_ROOM_MS + overrun
            || (self.phase != RoomPhase::Playing
                && now_ms.saturating_sub(self.active_ms) >= ROOM_IDLE_MS)
        {
            self.dispose("expired", out);
            return;
        }
        let before = self.seats.len();
        self.seats.retain(|seat| {
            seat.disconnected_ms
                .is_none_or(|since| now_ms.saturating_sub(since) < EMPTY_GRACE_MS)
        });
        if self.seats.len() != before {
            self.broadcast_lobby(out);
        }
    }

    fn dispose(&mut self, reason: &str, out: &mut HostOutput) {
        if self.disposed {
            return;
        }
        self.disposed = true;
        self.dispose_reason = Some(reason.to_string());
        let reset = RoomReset {
            r#type: "room-reset",
            room_epoch: &self.options.room_epoch,
            reason,
        };
        let reset = serde_json::to_string(&reset).expect("room-reset serializes");
        for connection in self.clients.keys() {
            out.send(*connection, reset.clone());
            out.close(*connection, 1012, reason);
        }
        self.clients.clear();
        self.seats.clear();
        out.changed();
    }

    fn is_disposed(&self) -> bool {
        self.disposed
    }

    fn dispose_reason(&self) -> Option<&str> {
        self.dispose_reason.as_deref()
    }

    fn directory_entry(&self, room: &str) -> RoomListing {
        RoomListing {
            room: room.to_string(),
            content_version: self.options.content_version.clone(),
            map_mode: self.settings.map_mode.clone(),
            difficulty: self.settings.difficulty.clone(),
            humans_only: self.settings.humans_only,
            round_minutes: self.settings.round_minutes,
            players: self.clients.len() as u32,
            reserved: self.seats.len() as u32,
            phase: self.phase,
            round_id: self.round_id,
            time: self.settings.round_minutes * 60,
            scores: [0, 0],
        }
    }

    fn connections(&self) -> u32 {
        self.clients.len() as u32
    }

    fn tick(&self) -> u64 {
        0
    }

    fn debt_ms(&self) -> f64 {
        0.0
    }
}
