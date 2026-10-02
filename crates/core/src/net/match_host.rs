//! The authoritative room host (`src/net/match-host.ts`): seats, the lobby protocol, the
//! simulation, per-client baselines and snapshot batches, and the room's own lifetime.
//!
//! It is transport-free: the runtime passes each received text, disconnect and 50 ms timer
//! tick with its millisecond clock, and drains [`HostEvent`]s (sends, closes, directory
//! changes) with [`MatchHost::take_events`] after every call. Nothing is sent re-entrantly.

use std::collections::{BTreeMap, HashMap};

use serde_json::Value;

use super::fixed_step_clock::FixedStepClock;
use super::multiplayer_simulation::{
    MAX_PLAYERS, MultiplayerOptions, TEAM_SLOTS, claim_player_tank, create_multiplayer_simulation,
    release_player_tank,
};
use super::player_controls::PlayerControls;
use super::protocol::{
    CONTENT_VERSION, Control, EMPTY_GRACE_MS, JoinRequest, Lobby, MAX_BATTLE_OVERRUN_MS,
    MAX_CLIENT_MESSAGE_BYTES, MAX_ROOM_MS, PROTOCOL_VERSION, Player, ROOM_IDLE_MS, RoomPhase,
    RoomSettings, Welcome, error_message, read_player_kind, read_team, room_reset_message,
};
use super::replication::{ShotTrace, StateStream, TimedEvent};
use super::room_list::RoomListing;
use super::scene_codec::{Scene, WireShot};
use super::schema::{MAX_SAFE_INTEGER, Record, id, number_in, parse_record};
use crate::sim::data::STEP;
use crate::sim::math::Vec2;
use crate::sim::render_state::RenderShot;
use crate::sim::simulation::Simulation;
use crate::sim::types::{MatchPhase, PlayerAssignment, SimEventType, Team, VehicleCommand};

const MAX_MESSAGES_PER_SECOND: u32 = 60;
/// A client that sends nothing (not even pings) for this long is dropped.
const CLIENT_TIMEOUT_MS: u64 = 65_000;
/// A client whose acknowledged tick falls this far behind stops receiving state.
const MAX_UNACKNOWLEDGED_TICKS: u64 = 180;
const MAX_RESYNCS_PER_SECOND: u32 = 2;
/// Most distinct players one round may count on its scoreboard.
const MAX_PARTICIPANTS: usize = 128;
const DEFAULT_SEED: u32 = 4242;

/// A socket's identity inside the room, chosen by the runtime and never reused.
pub type ConnectionId = u64;

/// What the host asks the transport to do, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostEvent {
    Send {
        connection: ConnectionId,
        text: String,
    },
    Close {
        connection: ConnectionId,
        code: u16,
        reason: String,
    },
    /// Membership, phase or settings changed: publish the directory listing now.
    Changed,
}

/// Everything a new match needs from the runtime.
pub struct MatchHostOptions {
    /// Random per match; clients use it to tell a restarted room from the one they left.
    pub room_epoch: String,
    /// The runtime's monotonic millisecond clock at creation.
    pub now_ms: u64,
    /// Makes a fresh unguessable seat token or player id.
    pub token: Box<dyn FnMut() -> String + Send>,
    /// Seed of the first round's gameplay stream (4242 by default in the TypeScript).
    pub seed: Option<u32>,
    /// Content version joins must present; [`CONTENT_VERSION`] when `None`.
    pub content_version: Option<String>,
}

struct Seat {
    player: Player,
    token: String,
    connection: Option<ConnectionId>,
    disconnected_ms: Option<u64>,
    controls: Option<PlayerControls>,
    control_key: Option<Control>,
    suspended: bool,
    /// A suspended seat whose player still watches the battle behind the in-battle
    /// menu: it keeps receiving snapshots and acknowledging them with its pings.
    watching: bool,
}

struct Client {
    player_id: String,
    last_seen_ms: u64,
    observed_tick: u64,
    window_ms: u64,
    count: u32,
    full_window_ms: u64,
    full_count: u32,
}

/// One identity or lifecycle value `lifecycle_changed` compares between ticks.
#[derive(Clone, Debug, PartialEq)]
enum Note {
    Number(u64),
    Text(Option<String>),
}

/// Records each entity's identity and lifecycle state in `seen`, in place, and reports
/// whether anything differed from the previous call: a tank's life, death or occupant,
/// cover destruction or rebuild, pickup availability, or any entity joining or leaving.
fn lifecycle_changed(simulation: &Simulation, seen: &mut Vec<Note>) -> bool {
    struct Recorder<'a> {
        seen: &'a mut Vec<Note>,
        index: usize,
        changed: bool,
    }
    impl Recorder<'_> {
        fn set(&mut self, note: Note) {
            if self.index < self.seen.len() {
                self.seen[self.index] = note;
            } else {
                self.seen.push(note);
            }
            self.changed = true;
        }

        fn number(&mut self, value: u64) {
            if self.seen.get(self.index) != Some(&Note::Number(value)) {
                self.set(Note::Number(value));
            }
            self.index += 1;
        }

        fn text(&mut self, value: Option<&str>) {
            let same = matches!(
                self.seen.get(self.index),
                Some(Note::Text(previous)) if previous.as_deref() == value
            );
            if !same {
                self.set(Note::Text(value.map(str::to_string)));
            }
            self.index += 1;
        }
    }
    let mut note = Recorder {
        seen,
        index: 0,
        changed: false,
    };
    note.number(simulation.tanks.len() as u64);
    for tank in &simulation.tanks {
        note.number(u64::from(tank.id));
        note.number(u64::from(tank.life));
        note.number(u64::from(tank.alive));
        note.text(tank.player_id.as_deref());
    }
    note.number(simulation.covers.len() as u64);
    for cover in &simulation.covers {
        note.number(u64::from(cover.id));
        note.number(u64::from(cover.alive));
    }
    note.number(simulation.fragments.len() as u64);
    for fragment in &simulation.fragments {
        note.number(u64::from(fragment.id));
    }
    note.number(simulation.mines.len() as u64);
    for mine in &simulation.mines {
        note.number(u64::from(mine.id));
    }
    note.number(simulation.pickups.len() as u64);
    for pickup in &simulation.pickups {
        note.number(u64::from(pickup.id));
        note.number(u64::from(pickup.available));
    }
    let total = note.index;
    if note.seen.len() != total {
        note.seen.truncate(total);
        note.changed = true;
    }
    note.changed
}

/// A test or tooling observer run after every simulation step, before its events drain.
pub type TickHook = Box<dyn FnMut(&mut Simulation, u64) + Send>;

/// Owns the simulation and room policy. Call [`advance`](Self::advance) from a 50 ms timer.
pub struct MatchHost {
    pub simulation: Option<Simulation>,
    pub settings: RoomSettings,
    pub phase: RoomPhase,
    pub round_id: u64,
    pub host_id: String,
    pub disposed: bool,
    /// Why the room ended (empty, expired, overload...), for server logs.
    pub dispose_reason: Option<String>,
    /// Runs after each simulation step (see [`TickHook`]).
    pub tick_hook: Option<TickHook>,
    room_epoch: String,
    created_ms: u64,
    token: Box<dyn FnMut() -> String + Send>,
    seed: u32,
    content_version: String,
    seats: Vec<Seat>,
    /// In connection order, like the TypeScript `Map`.
    clients: Vec<(ConnectionId, Client)>,
    /// Everyone who played this round, departed players included, in first-seen order.
    participants: Vec<Player>,
    owners: HashMap<(u32, u32), String>,
    clock: Option<FixedStepClock>,
    stream: Option<StateStream>,
    events: Vec<String>,
    traces: Vec<ShotTrace>,
    cursor: u64,
    lifecycle: Vec<Note>,
    frames: Vec<(u64, String)>,
    empty_since_ms: Option<u64>,
    active_ms: u64,
    scene: Scene,
    out: Vec<HostEvent>,
}

type Handled = Result<(), String>;

impl MatchHost {
    pub fn new(options: MatchHostOptions) -> Self {
        Self {
            simulation: None,
            settings: RoomSettings::default(),
            phase: RoomPhase::Lobby,
            round_id: 0,
            host_id: String::new(),
            disposed: false,
            dispose_reason: None,
            tick_hook: None,
            room_epoch: options.room_epoch,
            created_ms: options.now_ms,
            token: options.token,
            seed: options.seed.unwrap_or(DEFAULT_SEED),
            content_version: options
                .content_version
                .unwrap_or_else(|| CONTENT_VERSION.to_string()),
            seats: Vec::new(),
            clients: Vec::new(),
            participants: Vec::new(),
            owners: HashMap::new(),
            clock: None,
            stream: None,
            events: Vec::new(),
            traces: Vec::new(),
            cursor: 0,
            lifecycle: Vec::new(),
            frames: Vec::new(),
            empty_since_ms: Some(options.now_ms),
            active_ms: options.now_ms,
            scene: Scene::default(),
            out: Vec::new(),
        }
    }

    pub fn room_epoch(&self) -> &str {
        &self.room_epoch
    }

    /// The simulation tick of the current round.
    pub fn tick(&self) -> u64 {
        self.clock.as_ref().map_or(0, |clock| clock.tick)
    }

    /// Elapsed time the fixed-step clock still owes the simulation.
    pub fn debt_ms(&self) -> f64 {
        self.clock.as_ref().map_or(0.0, |clock| clock.debt_ms)
    }

    /// Connections that hold a seat.
    pub fn connections(&self) -> usize {
        self.clients.len()
    }

    /// Reserved seats, including players inside their reconnect grace.
    pub fn reserved(&self) -> usize {
        self.seats.len()
    }

    /// The events produced since the last call, in order.
    pub fn take_events(&mut self) -> Vec<HostEvent> {
        std::mem::take(&mut self.out)
    }

    fn send(&mut self, connection: ConnectionId, text: String) {
        self.out.push(HostEvent::Send { connection, text });
    }

    fn close(&mut self, connection: ConnectionId, code: u16, reason: &str) {
        self.out.push(HostEvent::Close {
            connection,
            code,
            reason: reason.to_string(),
        });
    }

    fn error(&mut self, connection: ConnectionId, code: &str, message: &str, fatal: bool) {
        self.send(connection, error_message(code, message, fatal));
        if fatal {
            self.close(connection, 1008, code);
        }
    }

    fn client_index(&self, connection: ConnectionId) -> Option<usize> {
        self.clients.iter().position(|(id, _)| *id == connection)
    }

    fn seat_index(&self, player_id: &str) -> Option<usize> {
        self.seats
            .iter()
            .position(|seat| seat.player.player_id == player_id)
    }

    /// One text message from a connection.
    pub fn receive(&mut self, connection: ConnectionId, text: &str, now_ms: u64) {
        if self.disposed {
            self.error(
                connection,
                "expired",
                "This room has expired. Create a new room.",
                true,
            );
            return;
        }
        if let Err(message) = self.handle(connection, text, now_ms) {
            self.disconnect(connection, now_ms);
            self.error(connection, "invalid-message", &message, true);
        }
    }

    fn handle(&mut self, connection: ConnectionId, text: &str, now_ms: u64) -> Handled {
        if text.len() > MAX_CLIENT_MESSAGE_BYTES {
            return Err("Message too large".into());
        }
        let message = parse_record(text)?;
        let kind = message
            .get("type")
            .and_then(Value::as_str)
            .map(str::to_string);
        let kind = kind.as_deref();
        if kind == Some("join") {
            if self.client_index(connection).is_some() {
                return Err("Already joined".into());
            }
            return self.join(connection, &message, now_ms);
        }
        let client_index = self.client_index(connection).ok_or("Join required")?;
        let tick = self.tick();
        let round = self.round_id as f64;
        let client = &mut self.clients[client_index].1;
        if now_ms.saturating_sub(client.window_ms) >= 1000 {
            client.window_ms = now_ms;
            client.count = 0;
        }
        client.count += 1;
        if client.count > MAX_MESSAGES_PER_SECOND {
            return Err("Message rate exceeded".into());
        }
        // A connection only ever belongs to this room instance, so messages name just the round.
        let current_round = message.get("roundId").and_then(Value::as_f64) == Some(round);
        if kind == Some("ping") {
            if !current_round {
                return Ok(());
            }
            let observed = id(message.get("observedTick"))?;
            let t = number_in(message.get("t"), 0.0, MAX_SAFE_INTEGER, false)?;
            if observed > tick {
                return Err("Invalid observed tick".into());
            }
            client.observed_tick = client.observed_tick.max(observed);
            client.last_seen_ms = now_ms;
            let pong = super::json::object(|writer| {
                writer
                    .string("type", "pong")
                    .number("t", t)
                    .int("tick", tick);
            });
            self.send(connection, pong);
            return Ok(());
        }
        if !current_round {
            return Ok(());
        }
        client.last_seen_ms = now_ms;
        let player_id = client.player_id.clone();
        let seat = self.seat_index(&player_id).ok_or("Join required")?;
        let is_host = self.host_id == player_id;
        let playing = self.phase == RoomPhase::Playing;
        match kind.unwrap_or("") {
            "input" => {
                if !playing {
                    return Ok(());
                }
                let (Some(simulation), Some(controls)) =
                    (self.simulation.as_ref(), self.seats[seat].controls.as_mut())
                else {
                    return Ok(());
                };
                let value = Value::Object(message);
                if controls.accept(simulation, &value, tick, now_ms as f64) {
                    let observed = id(value.get("observedTick"))?;
                    let client = &mut self.clients[client_index].1;
                    client.observed_tick = client.observed_tick.max(observed);
                }
            }
            "choose" => {
                if playing {
                    return Err("Choices are locked during a round".into());
                }
                let choice = read_player_kind(message.get("kind"))?;
                let side = match message.get("team") {
                    None => self.auto_team(Some(seat)),
                    some => read_team(some)?,
                };
                let Some(slot) = self.free_slot(side, Some(seat)) else {
                    self.error(
                        connection,
                        "team-full",
                        "That team has six reserved seats.",
                        false,
                    );
                    return Ok(());
                };
                let player = &mut self.seats[seat].player;
                player.team = side;
                player.slot = slot as u32;
                player.kind = choice;
                self.active_ms = now_ms;
                self.broadcast_lobby();
            }
            "settings" => {
                if !is_host || playing {
                    return Err("Only the lobby host can change settings".into());
                }
                self.settings = RoomSettings::read(&message)?;
                self.active_ms = now_ms;
                self.broadcast_lobby();
            }
            "start" => {
                if !is_host || playing {
                    return Err("Only the lobby host can start".into());
                }
                self.start(now_ms)?;
            }
            "end" => {
                if !is_host || !playing {
                    return Err("Only the host can end the round".into());
                }
                if let Some(simulation) = self.simulation.as_mut() {
                    simulation.match_state.phase = MatchPhase::Results;
                    simulation.match_state.ended_early = Some(true);
                }
                self.finish(now_ms);
            }
            "suspend" => {
                self.seats[seat].suspended = true;
                self.seats[seat].watching = message.get("watch") == Some(&Value::Bool(true));
                if let (Some(simulation), Some(controls)) =
                    (self.simulation.as_mut(), self.seats[seat].controls.as_mut())
                {
                    controls.suspend(simulation);
                }
                self.send_control(seat);
            }
            "resume" => {
                self.seats[seat].suspended = false;
                self.seats[seat].watching = false;
                if let (Some(simulation), Some(controls)) =
                    (self.simulation.as_mut(), self.seats[seat].controls.as_mut())
                {
                    controls.resume(simulation, now_ms as f64);
                }
                self.clients[client_index].1.observed_tick = tick;
                self.send_control(seat);
                self.send_full_limited(connection, client_index, now_ms)?;
            }
            "resync" => self.send_full_limited(connection, client_index, now_ms)?,
            "leave" => {
                self.disconnect(connection, now_ms);
                if let Some(seat) = self.seat_index(&player_id) {
                    self.release(seat);
                }
                self.broadcast_lobby();
                self.close(connection, 1000, "Left room");
                if self.seats.is_empty() {
                    self.dispose("empty");
                }
            }
            _ => return Err("Unknown message".into()),
        }
        Ok(())
    }

    fn free_slot(&self, side: Team, except: Option<usize>) -> Option<usize> {
        (0..TEAM_SLOTS).find(|&slot| {
            !self.seats.iter().enumerate().any(|(index, seat)| {
                Some(index) != except
                    && seat.player.team == side
                    && seat.player.slot as usize == slot
            })
        })
    }

    fn auto_team(&self, except: Option<usize>) -> Team {
        let count = |side: Team| {
            self.seats
                .iter()
                .enumerate()
                .filter(|(index, seat)| Some(*index) != except && seat.player.team == side)
                .count()
        };
        if count(Team::Blue) <= count(Team::Red) {
            Team::Blue
        } else {
            Team::Red
        }
    }

    fn join(&mut self, connection: ConnectionId, message: &Record, now_ms: u64) -> Handled {
        let request = JoinRequest::read(message)?;
        if request.version != u64::from(PROTOCOL_VERSION)
            || request.content_version != self.content_version
        {
            self.error(
                connection,
                "incompatible",
                "Game updated. Reload this page before joining.",
                true,
            );
            return Ok(());
        }
        let name = request.name.trim().to_string();
        if name.is_empty() {
            return Err("Enter a player name".into());
        }
        let found = request
            .token
            .as_ref()
            .and_then(|token| self.seats.iter().position(|seat| &seat.token == token));
        let create = request.create.is_some() && found.is_none() && request.room_epoch.is_none();
        if create && !self.seats.is_empty() {
            self.error(
                connection,
                "room-exists",
                "That room code is already in use. Create another room.",
                true,
            );
            return Ok(());
        }
        if request.existing_room == Some(true) && found.is_none() && self.seats.is_empty() {
            self.error(connection, "room-gone", "This room has ended.", true);
            return Ok(());
        }
        if request.token.is_some()
            && request.room_epoch.as_deref() == Some(self.room_epoch.as_str())
            && found.is_none()
        {
            self.error(
                connection,
                "seat-expired",
                "Your reserved seat expired. Join again to take a new seat.",
                true,
            );
            return Ok(());
        }
        let reset = request
            .room_epoch
            .as_ref()
            .is_some_and(|epoch| *epoch != self.room_epoch);
        let tick = self.tick();
        let seat = match found {
            None => {
                if self.seats.len() >= MAX_PLAYERS {
                    self.error(
                        connection,
                        "room-full",
                        "This room has eight reserved player seats.",
                        true,
                    );
                    return Ok(());
                }
                if self.participants.len() >= MAX_PARTICIPANTS {
                    self.error(
                        connection,
                        "round-full",
                        "This round has reached its participant limit. Try the next round.",
                        true,
                    );
                    return Ok(());
                }
                let preferred = request.team.unwrap_or_else(|| self.auto_team(None));
                let side = if request.team.is_none() && self.free_slot(preferred, None).is_none() {
                    preferred.opponent()
                } else {
                    preferred
                };
                let Some(slot) = self.free_slot(side, None) else {
                    self.error(
                        connection,
                        "team-full",
                        "That team has six reserved seats.",
                        true,
                    );
                    return Ok(());
                };
                let player = Player {
                    player_id: (self.token)(),
                    name,
                    kind: request.kind,
                    team: side,
                    slot: slot as u32,
                    connected: true,
                    kills: 0,
                    deaths: 0,
                    tank_id: None,
                };
                let token = (self.token)();
                self.seats.push(Seat {
                    player,
                    token,
                    connection: Some(connection),
                    disconnected_ms: None,
                    controls: None,
                    control_key: None,
                    suspended: false,
                    watching: false,
                });
                let seat = self.seats.len() - 1;
                if self.simulation.is_some() {
                    self.seat_tank(seat, now_ms)?;
                    let player = self.seats[seat].player.clone();
                    self.set_participant(player);
                    self.drain_events(tick);
                }
                seat
            }
            Some(seat) => {
                if let Some(old) = self.seats[seat].connection.take() {
                    self.clients.retain(|(id, _)| *id != old);
                    self.close(old, 4001, "Seat reconnected elsewhere");
                }
                let entry = &mut self.seats[seat];
                entry.connection = Some(connection);
                entry.player.connected = true;
                entry.disconnected_ms = None;
                entry.suspended = false;
                entry.watching = false;
                entry.control_key = None;
                if let (Some(simulation), Some(controls)) =
                    (self.simulation.as_mut(), entry.controls.as_mut())
                {
                    controls.resume(simulation, now_ms as f64);
                }
                seat
            }
        };
        let player_id = self.seats[seat].player.player_id.clone();
        self.clients.push((
            connection,
            Client {
                player_id: player_id.clone(),
                last_seen_ms: now_ms,
                observed_tick: tick,
                window_ms: now_ms,
                count: 0,
                full_window_ms: now_ms,
                full_count: 0,
            },
        ));
        self.empty_since_ms = None;
        self.active_ms = now_ms;
        if self.host_id.is_empty() {
            self.host_id.clone_from(&player_id);
        }
        let welcome = Welcome {
            version: PROTOCOL_VERSION,
            content_version: self.content_version.clone(),
            room_epoch: self.room_epoch.clone(),
            player_id,
            token: self.seats[seat].token.clone(),
            host_id: self.host_id.clone(),
            reset,
        };
        self.send(connection, welcome.to_json());
        self.broadcast_lobby();
        self.send_control(seat);
        self.send_full(connection);
        if create {
            self.settings = request.create.expect("create implies settings");
            self.start(now_ms)?;
        }
        Ok(())
    }

    /// Claims a tank for a seat that joined a round already built.
    fn seat_tank(&mut self, seat: usize, now_ms: u64) -> Handled {
        let simulation = self.simulation.as_mut().expect("a round is built");
        let player = &self.seats[seat].player;
        let assignment = PlayerAssignment {
            player_id: player.player_id.clone(),
            name: player.name.clone(),
            team: player.team,
            slot: player.slot as usize,
            kind: player.kind,
        };
        let index = claim_player_tank(simulation, &assignment);
        let tank_id = simulation.tanks[index].id;
        let controls =
            PlayerControls::new(simulation, tank_id, now_ms as f64, !simulation.humans_only)?;
        self.remember_owner(index);
        let entry = &mut self.seats[seat];
        entry.player.tank_id = Some(tank_id);
        entry.controls = Some(controls);
        Ok(())
    }

    fn set_participant(&mut self, player: Player) {
        match self
            .participants
            .iter_mut()
            .find(|entry| entry.player_id == player.player_id)
        {
            Some(entry) => *entry = player,
            None => self.participants.push(player),
        }
    }

    /// The room's public listing. Valid after disposal, with `players: 0`.
    pub fn directory_entry(&self, room: &str) -> RoomListing {
        let time = self
            .simulation
            .as_ref()
            .map_or(f64::from(self.settings.round_minutes * 60), |simulation| {
                simulation.match_state.time
            });
        RoomListing {
            room: room.to_string(),
            content_version: self.content_version.clone(),
            map_mode: self.settings.map_mode.as_str().to_string(),
            difficulty: self.settings.difficulty.as_str().to_string(),
            humans_only: self.settings.humans_only,
            round_minutes: self.settings.round_minutes,
            players: self.clients.len() as u32,
            reserved: self.seats.len() as u32,
            phase: self.phase,
            round_id: self.round_id,
            time: time.ceil().max(0.0) as u32,
            scores: self
                .simulation
                .as_ref()
                .map_or([0, 0], |simulation| simulation.match_state.scores),
        }
    }

    fn remember_owner(&mut self, tank_index: usize) {
        let Some(simulation) = self.simulation.as_ref() else {
            return;
        };
        let tank = &simulation.tanks[tank_index];
        if let Some(player) = &tank.player_id {
            self.owners
                .entry((tank.id, tank.life))
                .or_insert_with(|| player.clone());
        }
    }

    fn start(&mut self, now_ms: u64) -> Handled {
        self.simulation = None;
        self.round_id += 1;
        self.cursor = 0;
        self.events.clear();
        self.traces.clear();
        self.owners.clear();
        self.participants.clear();
        let players: Vec<PlayerAssignment> = self
            .seats
            .iter()
            .map(|seat| PlayerAssignment {
                player_id: seat.player.player_id.clone(),
                name: seat.player.name.clone(),
                team: seat.player.team,
                slot: seat.player.slot as usize,
                kind: seat.player.kind,
            })
            .collect();
        let seed = self.seed.wrapping_add(self.round_id as u32).wrapping_sub(1);
        let mut simulation = create_multiplayer_simulation(
            f64::from(seed),
            &players,
            MultiplayerOptions {
                map_mode: Some(self.settings.map_mode),
                difficulty: Some(self.settings.difficulty),
                round: Some(self.round_id as u32),
                humans_only: Some(self.settings.humans_only),
            },
        )?;
        simulation.match_state.time = f64::from(self.settings.round_minutes * 60);
        simulation.projectile_moves = Some(Vec::new());
        self.clock = Some(FixedStepClock::new(now_ms as f64));
        self.stream = Some(StateStream::new(&self.room_epoch, self.round_id));
        for (_, client) in &mut self.clients {
            client.observed_tick = 0;
        }
        let bot_takeover = !simulation.humans_only;
        self.simulation = Some(simulation);
        for seat in 0..self.seats.len() {
            let simulation = self.simulation.as_ref().expect("just built");
            let player_id = &self.seats[seat].player.player_id;
            let index = simulation
                .tanks
                .iter()
                .position(|tank| tank.player_id.as_deref() == Some(player_id))
                .expect("every seat has a tank");
            let tank_id = simulation.tanks[index].id;
            let mut controls =
                PlayerControls::new(simulation, tank_id, now_ms as f64, bot_takeover)?;
            let entry = &mut self.seats[seat];
            entry.player.tank_id = Some(tank_id);
            entry.player.kills = 0;
            entry.player.deaths = 0;
            let player = entry.player.clone();
            let idle = entry.connection.is_none() || entry.suspended;
            entry.control_key = None;
            if idle {
                controls.suspend(self.simulation.as_mut().expect("just built"));
            }
            self.seats[seat].controls = Some(controls);
            self.set_participant(player);
            self.remember_owner(index);
        }
        let simulation = self.simulation.as_mut().expect("just built");
        simulation.start();
        self.phase = RoomPhase::Playing;
        self.active_ms = now_ms;
        self.lifecycle.clear();
        lifecycle_changed(simulation, &mut self.lifecycle);
        self.frames.clear();
        self.broadcast_lobby();
        for seat in 0..self.seats.len() {
            self.send_control(seat);
            if let Some(connection) = self.seats[seat].connection {
                self.send_full(connection);
            }
        }
        Ok(())
    }

    fn drain_events(&mut self, tick: u64) {
        let Some(simulation) = self.simulation.as_mut() else {
            return;
        };
        for event in simulation.events.drain(..) {
            if event.kind == SimEventType::Death {
                if let Some(victim) = self
                    .seats
                    .iter_mut()
                    .find(|seat| seat.player.tank_id.is_some() && seat.player.tank_id == event.id)
                {
                    victim.player.deaths += 1;
                }
                let owner_id = match (event.owner, event.owner_life) {
                    (Some(owner), Some(life)) => self.owners.get(&(owner, life)).cloned(),
                    _ => None,
                };
                if let Some(owner_id) = owner_id {
                    let credit = event.owner != event.id;
                    let owner = match self
                        .seats
                        .iter_mut()
                        .find(|seat| seat.player.player_id == owner_id)
                    {
                        Some(seat) => Some(&mut seat.player),
                        None => self
                            .participants
                            .iter_mut()
                            .find(|player| player.player_id == owner_id),
                    };
                    if let Some(owner) = owner
                        && credit
                        && Some(owner.team) != event.team
                    {
                        owner.kills += 1;
                    }
                }
            }
            self.cursor += 1;
            self.events
                .push(TimedEvent::write(self.cursor, tick as f64, &event));
        }
        for seat in 0..self.seats.len() {
            let player = &self.seats[seat].player;
            // Most ticks leave the scoreboard unchanged; keep its owned strings until
            // a score or seat change actually needs a new participant record.
            if !self.participants.iter().any(|entry| entry == player) {
                self.set_participant(player.clone());
            }
            let Some(tank_id) = self.seats[seat].controls.as_ref().map(|c| c.tank_id) else {
                continue;
            };
            let simulation = self.simulation.as_mut().expect("checked above");
            if let Some(index) = simulation.tank_index(tank_id) {
                let tank = &mut simulation.tanks[index];
                tank.kills = self.seats[seat].player.kills;
                tank.deaths = self.seats[seat].player.deaths;
                self.remember_owner(index);
            }
        }
    }

    /// The 50 ms timer: expire the room or seats, drop connections that stopped consuming
    /// state, step the simulation by the elapsed fixed steps and send snapshot batches.
    pub fn advance(&mut self, now_ms: u64) {
        if self.disposed {
            return;
        }
        let playing = self.phase == RoomPhase::Playing;
        let age = now_ms.saturating_sub(self.created_ms);
        if self
            .empty_since_ms
            .is_some_and(|since| now_ms.saturating_sub(since) >= EMPTY_GRACE_MS)
            // Past its lifetime a room lets the battle under way finish, then closes
            // instead of hosting another.
            || age >= MAX_ROOM_MS + if playing { MAX_BATTLE_OVERRUN_MS } else { 0 }
            || (!playing && now_ms.saturating_sub(self.active_ms) >= ROOM_IDLE_MS)
        {
            self.dispose("expired");
            return;
        }
        let mut changed = false;
        let expired: Vec<String> = self
            .seats
            .iter()
            .filter(|seat| {
                seat.disconnected_ms
                    .is_some_and(|since| now_ms.saturating_sub(since) >= EMPTY_GRACE_MS)
            })
            .map(|seat| seat.player.player_id.clone())
            .collect();
        for player_id in expired {
            if let Some(seat) = self.seat_index(&player_id) {
                self.release(seat);
                changed = true;
            }
        }
        let tick = self.tick();
        let stale: Vec<ConnectionId> = self
            .clients
            .iter()
            .filter(|(_, client)| {
                // A hidden or preparing seat gets no stream to acknowledge; a watching one does.
                let suspended = self
                    .seat_index(&client.player_id)
                    .is_some_and(|seat| self.seats[seat].suspended && !self.seats[seat].watching);
                now_ms.saturating_sub(client.last_seen_ms) > CLIENT_TIMEOUT_MS
                    || (!suspended
                        && tick.saturating_sub(client.observed_tick) > MAX_UNACKNOWLEDGED_TICKS)
            })
            .map(|(connection, _)| *connection)
            .collect();
        for connection in stale {
            self.disconnect(connection, now_ms);
            self.close(connection, 4002, "Connection is not consuming state");
        }
        if self.phase == RoomPhase::Playing {
            let mut clock = self.clock.take().expect("a playing room has a clock");
            let result = clock.advance(now_ms as f64, |tick| self.step(tick, now_ms));
            self.clock = Some(clock);
            if result != Ok(true) {
                self.dispose("overload");
                return;
            }
            for seat in 0..self.seats.len() {
                // Synchronize post-step deaths/respawns before publishing the next control epoch.
                if let (Some(simulation), Some(controls)) =
                    (self.simulation.as_ref(), self.seats[seat].controls.as_mut())
                {
                    controls.refresh_life(simulation);
                }
                self.send_control(seat);
            }
            self.broadcast_snapshot();
            if self
                .simulation
                .as_ref()
                .is_some_and(|simulation| simulation.match_state.phase == MatchPhase::Results)
            {
                self.finish(now_ms);
            }
        }
        if changed {
            self.broadcast_lobby();
        }
    }

    fn step(&mut self, tick: u64, now_ms: u64) {
        let Some(simulation) = self.simulation.as_mut() else {
            return;
        };
        if simulation.match_state.phase != MatchPhase::Playing {
            return;
        }
        let mut commands: BTreeMap<u32, VehicleCommand> = BTreeMap::new();
        for seat in &mut self.seats {
            if let Some(controls) = seat.controls.as_mut()
                && let Some(command) = controls.command(simulation, tick, now_ms as f64)
            {
                commands.insert(controls.tank_id, command);
            }
        }
        simulation.step_with(&commands);
        if let Some(hook) = self.tick_hook.as_mut() {
            hook(simulation, tick);
        }
        let mut moves = simulation
            .projectile_moves
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default();
        for projectile in moves.drain(..) {
            self.trace(
                tick,
                &projectile.shot,
                projectile.seconds,
                projectile.offset,
            );
        }
        // Keep the trace capture capacity for the next tick's projectile sweeps.
        if let Some(stored) = self
            .simulation
            .as_mut()
            .and_then(|simulation| simulation.projectile_moves.as_mut())
        {
            *stored = moves;
        }
        self.drain_events(tick);
        // Intermediate deaths/respawns and membership changes survive the 20 Hz batching.
        let simulation = self.simulation.as_ref().expect("stepped above");
        if lifecycle_changed(simulation, &mut self.lifecycle) {
            self.capture_frame(tick);
        }
    }

    /// Records one projectile sweep as a straight segment between fractional ticks.
    fn trace(&mut self, tick: u64, shot: &crate::sim::types::Shot, seconds: f64, offset: f64) {
        if seconds <= 0.0 {
            return;
        }
        let start = RenderShot {
            x: shot.x - shot.vx * seconds,
            z: shot.z - shot.vz * seconds,
            ..RenderShot::from(shot)
        };
        let wire = WireShot::rounded(start);
        let trace = ShotTrace {
            tick: super::json::position(tick as f64 - 1.0 + offset / STEP),
            end_tick: super::json::position(tick as f64 - 1.0 + (offset + seconds) / STEP),
            shot: RenderShot {
                id: wire.id,
                x: wire.x,
                z: wire.z,
                y: wire.y,
                visual_y: wire.visual_y,
                vx: wire.vx,
                vz: wire.vz,
                weapon: wire.weapon,
                team: wire.team,
            },
            end: Vec2::new(super::json::position(shot.x), super::json::position(shot.z)),
        };
        // Straight flight is linear, so consecutive ticks share one segment until a bounce,
        // steering or height change starts a new one.
        if let Some(previous) = self
            .traces
            .iter_mut()
            .rev()
            .find(|previous| previous.shot.id == shot.id)
            && previous.end_tick == trace.tick
            && previous.shot.vx == trace.shot.vx
            && previous.shot.vz == trace.shot.vz
            && previous.shot.y == trace.shot.y
            && previous.shot.visual_y == trace.shot.visual_y
        {
            previous.end_tick = trace.end_tick;
            previous.end = trace.end;
            return;
        }
        self.traces.push(trace);
    }

    fn broadcast_snapshot(&mut self) {
        if self.simulation.is_none() || self.stream.is_none() {
            return;
        }
        let tick = self.tick();
        if self.frames.last().map(|(frame_tick, _)| *frame_tick) != Some(tick)
            || !self.events.is_empty()
        {
            self.capture_frame(tick);
        }
        let mut body = String::from("[");
        for (index, (_, frame)) in self.frames.drain(..).enumerate() {
            if index > 0 {
                body.push(',');
            }
            body.push_str(&frame);
        }
        body.push(']');
        let head = format!(
            "{{\"type\":\"snapshot\",\"roundId\":{},\"ack\":",
            self.round_id
        );
        let mut sends = Vec::new();
        for (connection, client) in &self.clients {
            let Some(seat) = self.seat_index(&client.player_id) else {
                continue;
            };
            // Hidden or preparing clients receive a fresh baseline on resume, not an
            // accumulating stream; a client watching behind its menu keeps the stream.
            if self.seats[seat].suspended && !self.seats[seat].watching {
                continue;
            }
            let ack = self.seats[seat]
                .controls
                .as_ref()
                .map_or(0, |controls| controls.ack.input_seq);
            sends.push((*connection, format!("{head}{ack},\"snapshots\":{body}}}")));
        }
        for (connection, text) in sends {
            self.send(connection, text);
        }
    }

    fn capture_frame(&mut self, tick: u64) {
        let (Some(simulation), Some(stream)) = (self.simulation.as_ref(), self.stream.as_mut())
        else {
            return;
        };
        self.scene.capture_from(simulation);
        let frame = stream.snapshot(&mut self.scene, tick, &self.events, &self.traces);
        self.frames.push((tick, frame));
        self.events.clear();
        self.traces.clear();
    }

    fn send_control(&mut self, seat: usize) {
        let entry = &self.seats[seat];
        let (Some(connection), Some(controls), Some(simulation)) = (
            entry.connection,
            entry.controls.as_ref(),
            self.simulation.as_ref(),
        ) else {
            return;
        };
        let Some(index) = simulation.tank_index(controls.tank_id) else {
            return;
        };
        let tank = &simulation.tanks[index];
        if entry.control_key.as_ref().is_some_and(|previous| {
            previous.room_epoch == self.room_epoch
                && previous.round_id == self.round_id
                && previous.tank_id == tank.id
                && previous.life == tank.life
                && previous.control_epoch == controls.control_epoch
                && previous.driver == tank.driver
        }) {
            return;
        }
        let control = Control {
            room_epoch: self.room_epoch.clone(),
            round_id: self.round_id,
            tank_id: tank.id,
            life: tank.life,
            control_epoch: controls.control_epoch,
            driver: tank.driver,
        };
        let text = control.to_json();
        self.seats[seat].control_key = Some(control);
        self.send(connection, text);
    }

    fn send_full_limited(
        &mut self,
        connection: ConnectionId,
        client_index: usize,
        now_ms: u64,
    ) -> Handled {
        let client = &mut self.clients[client_index].1;
        if now_ms.saturating_sub(client.full_window_ms) >= 1000 {
            client.full_window_ms = now_ms;
            client.full_count = 0;
        }
        client.full_count += 1;
        if client.full_count > MAX_RESYNCS_PER_SECOND {
            return Err("Too many full-state requests".into());
        }
        self.send_full(connection);
        Ok(())
    }

    fn send_full(&mut self, connection: ConnectionId) {
        let tick = self.tick();
        let (Some(simulation), Some(stream)) = (self.simulation.as_ref(), self.stream.as_mut())
        else {
            return;
        };
        let scene = Scene::capture(simulation);
        let text = stream.full(&scene, tick, self.cursor);
        self.send(connection, text);
    }

    fn finish(&mut self, now_ms: u64) {
        self.phase = RoomPhase::Results;
        self.active_ms = now_ms;
        // END BATTLE can arrive outside a timer callback, so publish its final state too.
        self.broadcast_snapshot();
        self.broadcast_lobby();
    }

    /// The current lobby message.
    pub fn lobby(&self) -> Lobby {
        Lobby {
            room_epoch: self.room_epoch.clone(),
            round_id: self.round_id,
            phase: self.phase,
            host_id: self.host_id.clone(),
            players: self.seats.iter().map(|seat| seat.player.clone()).collect(),
            scoreboard: self.participants.clone(),
            settings: self.settings,
        }
    }

    fn broadcast_lobby(&mut self) {
        let body = self.lobby().to_json();
        let connections: Vec<ConnectionId> = self.clients.iter().map(|(id, _)| *id).collect();
        for connection in connections {
            self.send(connection, body.clone());
        }
        self.out.push(HostEvent::Changed);
    }

    /// The connection is gone. Its seat, if any, stays reserved for the reconnect grace.
    /// Unknown connections are ignored.
    pub fn disconnect(&mut self, connection: ConnectionId, now_ms: u64) {
        let Some(index) = self.client_index(connection) else {
            return;
        };
        let (_, client) = self.clients.remove(index);
        if let Some(seat) = self.seat_index(&client.player_id) {
            let entry = &mut self.seats[seat];
            entry.connection = None;
            entry.disconnected_ms = Some(now_ms);
            entry.player.connected = false;
            if let (Some(simulation), Some(controls)) =
                (self.simulation.as_mut(), entry.controls.as_mut())
            {
                controls.suspend(simulation);
            }
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
        self.broadcast_lobby();
    }

    fn release(&mut self, seat: usize) {
        let entry = self.seats.remove(seat);
        if let (Some(controls), Some(simulation)) = (entry.controls, self.simulation.as_mut()) {
            release_player_tank(simulation, controls.tank_id);
            self.set_participant(Player {
                connected: false,
                ..entry.player
            });
        }
    }

    /// Ends the match now: every connection gets `room-reset` and a 1012 close. Calling it
    /// again does nothing.
    pub fn dispose(&mut self, reason: &str) {
        if self.disposed {
            return;
        }
        self.disposed = true;
        self.dispose_reason = Some(reason.to_string());
        let reset = room_reset_message(&self.room_epoch, reason);
        let connections: Vec<ConnectionId> = self.clients.iter().map(|(id, _)| *id).collect();
        for connection in connections {
            self.send(connection, reset.clone());
            self.close(connection, 1012, reason);
        }
        self.clients.clear();
        self.seats.clear();
        self.events.clear();
        self.traces.clear();
        self.frames.clear();
        self.participants.clear();
        self.owners.clear();
        self.simulation = None;
        self.clock = None;
        self.stream = None;
        self.out.push(HostEvent::Changed);
    }
}
