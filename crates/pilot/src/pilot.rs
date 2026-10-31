//! One pilot: a room seat played by the bot brain through the browser's own client.
//!
//! [`Pilot`] wraps [`NetworkClient`], the transport-free session the page and the desktop
//! app drive, and stands in for both the page and the player. As the page it answers the
//! renderer's requests (it "prepares" an arena by building its [`WorldModel`]) and turns
//! the client's socket actions into its own, numbered so that a pilot that rejoins on
//! another team never mixes up its old socket with the new one. As the player it reads
//! each displayed frame, lets the brain decide, and hands the result back as next frame's
//! controls, held and released like keys and a mouse.
//!
//! Like the client it reads no clock and opens no socket: the command line and the bots
//! Worker forward socket events and call [`Pilot::tick`] every frame with their own
//! milliseconds, then perform [`Pilot::take_actions`].

use sloppy_core::net::client::{
    ClientAction, ClientConfig, ClientNotice, EndCause, FrameEvent, LocalInput, NetworkClient,
    SavedSeat,
};
use sloppy_core::net::protocol::{JoinChoice, Lobby, RoomPhase, RoomSettings};
use sloppy_core::sim::ai::bot_command;
use sloppy_core::sim::bot_strategy::notice_hit;
use sloppy_core::sim::difficulty::Difficulty;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::math::{Random, distance};
use sloppy_core::sim::render_state::{RenderState, RenderTank};
use sloppy_core::sim::types::{AmmoSelection, Match, SimEventType, Team, Weapon};

use crate::orders::{Mission, Orders, same_player};
use crate::report::{Feel, PilotReport, Tally};
use crate::world_model::{Perspective, WorldModel, scene_map};

/// Socket ids carry the client generation above these bits: a pilot that rejoins makes a
/// new client, whose socket ids start again at 1.
const SOCKET_BITS: u32 = 20;
const SOCKET_MASK: u32 = (1 << SOCKET_BITS) - 1;
/// A fire decision holds the trigger this long, as a click does, so it is not lost
/// between two input messages; the host fires once per reload whatever the hold.
const TRIGGER_HOLD_MS: f64 = 120.0;
/// A mine or ammunition request waits this long for the host to answer before it is
/// asked again (the scene shows the answer a round trip later).
const MINE_RETRY_MS: f64 = 1500.0;
const AMMO_RETRY_MS: f64 = 1000.0;
/// A hunted player is "heard" this long into the future every frame, so the brain keeps
/// closing on them out of sight and favors them when in sight.
const HUNT_ALARM_SECONDS: f64 = 1.0;
/// An escort turns on its leader's attacker, and fires on its leader's target, for this
/// long (the brain's own `HIT_ALARM_SECONDS`).
const ESCORT_ALARM_SECONDS: f64 = 4.0;
/// Changing sides takes a leave and a new seat; never more often than this.
const REALIGN_MS: f64 = 10_000.0;
/// Notices are drained until quiet, but never forever.
const MAX_NOTICE_PASSES: usize = 8;

/// The aim a hand on a mouse cannot hold still: two slow sines, radians at full weight.
const fn aim_wobble(skill: Difficulty) -> f64 {
    match skill {
        Difficulty::Easy => 0.06,
        Difficulty::Normal => 0.035,
        Difficulty::Hard => 0.02,
    }
}
const WOBBLE_RATES: [f64; 2] = [1.3, 2.9];
const WOBBLE_WEIGHTS: [f64; 2] = [0.6, 0.4];

pub struct PilotConfig {
    /// The server's WebSocket origin, such as `ws://127.0.0.1:8787`.
    pub server_url: String,
    pub room: String,
    pub name: String,
    pub orders: Orders,
    /// Create the room with these settings instead of joining it.
    pub create: Option<RoomSettings>,
    /// A seat this pilot held before (its runner restarted), to take back.
    pub saved_seat: Option<SavedSeat>,
    /// Seeds the brain's decisions and the aim's unsteadiness.
    pub seed: u32,
}

/// Why the connection gave up, as the client reported it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ended {
    pub cause: EndCause,
    pub text: String,
}

pub struct Pilot {
    config: PilotConfig,
    client: NetworkClient,
    /// Counts the clients made; part of every socket id this pilot reports.
    generation: u32,
    actions: Vec<ClientAction>,
    cockpit: Cockpit,
    /// The next frame's controls, decided from the last frame.
    input: LocalInput,
    phase: Option<RoomPhase>,
    phase_since_ms: f64,
    last_realign_ms: f64,
    status: String,
    notice: Option<String>,
    ended: Option<Ended>,
    /// The pilot gave its seat up.
    left: bool,
}

impl Pilot {
    pub fn new(config: PilotConfig) -> Self {
        let client = client(&config);
        let cockpit = Cockpit::new(config.orders.clone(), config.seed);
        Self {
            config,
            client,
            generation: 1,
            actions: Vec::new(),
            cockpit,
            input: LocalInput::default(),
            phase: None,
            phase_since_ms: 0.0,
            last_realign_ms: f64::NEG_INFINITY,
            status: String::new(),
            notice: None,
            ended: None,
            left: false,
        }
    }

    pub fn name(&self) -> &str {
        &self.config.name
    }

    pub fn orders(&self) -> &Orders {
        &self.config.orders
    }

    /// Opens the first socket; the join goes out once it is open.
    pub fn start(&mut self, now_ms: f64) {
        let choice = JoinChoice {
            name: self.config.name.clone(),
            kind: self.config.orders.kind,
            team: None,
            create: self.config.create,
            existing_room: None,
        };
        self.client.connect(choice, now_ms);
        self.collect(now_ms);
    }

    pub fn socket_opened(&mut self, socket: u32, now_ms: f64) {
        if let Some(socket) = self.inner(socket) {
            self.client.socket_opened(socket, now_ms);
        }
    }

    pub fn socket_text(&mut self, socket: u32, text: &str, now_ms: f64) {
        if let Some(socket) = self.inner(socket) {
            self.client.socket_message(socket, text, now_ms);
        }
    }

    pub fn socket_binary(&mut self, socket: u32, bytes: &[u8], now_ms: f64) {
        if let Some(socket) = self.inner(socket) {
            self.client.socket_binary(socket, bytes, now_ms);
        }
    }

    pub fn socket_closed(&mut self, socket: u32, code: u16, now_ms: f64) {
        if let Some(socket) = self.inner(socket) {
            self.client.socket_closed(socket, code, now_ms);
        }
    }

    /// One frame: the client's timers, the renderer's answers, the displayed scene and the
    /// brain's controls for the next frame. Call at a steady rate (30-60 Hz).
    pub fn tick(&mut self, now_ms: f64) {
        self.client.poll(now_ms);
        self.collect(now_ms);
        let input = self.input;
        self.input = match self.client.frame(now_ms, &input) {
            Some(frame) => self
                .cockpit
                .fly(frame.state, &frame.events, frame.dt, now_ms),
            None => LocalInput {
                aim_angle: input.aim_angle,
                ..LocalInput::default()
            },
        };
        self.collect(now_ms);
        self.host_rounds(now_ms);
        self.collect(now_ms);
    }

    /// Gives the seat up (the server frees it at once) and closes the socket.
    pub fn leave(&mut self, now_ms: f64) {
        self.client.leave(now_ms);
        self.left = true;
        self.collect(now_ms);
    }

    /// The socket and storage actions to perform, in order. Socket ids are the pilot's
    /// own; report events with them.
    pub fn take_actions(&mut self) -> Vec<ClientAction> {
        std::mem::take(&mut self.actions)
    }

    /// Whether the pilot has a seat in the room: welcomed, with the lobby seen.
    pub fn seated(&self) -> bool {
        self.client.connected && self.phase.is_some()
    }

    /// Why the connection gave up, once it has; it does not retry after that.
    pub fn ended(&self) -> Option<&Ended> {
        self.ended.as_ref()
    }

    /// The scene the pilot last displayed (and decided from).
    pub fn scene(&self) -> Option<&RenderState> {
        self.client.display()
    }

    pub fn report(&self, now_ms: f64) -> PilotReport {
        let lobby = self.client.lobby();
        let me = lobby.and_then(|lobby| {
            lobby
                .players
                .iter()
                .find(|player| player.player_id == self.client.player_id)
        });
        let viewer = self.client.display().and_then(RenderState::viewer);
        PilotReport {
            name: self.config.name.clone(),
            room: self.config.room.clone(),
            mission: self.config.orders.mission.describe(),
            phase: if self.left {
                "left"
            } else if self.ended.is_some() {
                "ended"
            } else if !self.client.connected {
                "connecting"
            } else {
                self.phase.map_or("connecting", RoomPhase::as_str)
            },
            team: me.map(|player| player.team as u8),
            host: lobby.is_some_and(|lobby| lobby.host_id == self.client.player_id),
            status: self.status.clone(),
            notice: self.notice.clone(),
            ended: self.ended.as_ref().map(|ended| ended.cause.as_str().into()),
            ended_text: self.ended.as_ref().map(|ended| ended.text.clone()),
            score_kills: viewer.map_or(0, |viewer| viewer.kills),
            score_deaths: viewer.map_or(0, |viewer| viewer.deaths),
            tally: self.cockpit.tally,
            feel: Feel::read(
                &self.client.stats(now_ms),
                &self.client.feel,
                &self.client.own_shots.stats,
            ),
        }
    }

    /// The client's socket id for one of this pilot's, if it belongs to the current client.
    fn inner(&self, socket: u32) -> Option<u32> {
        (socket >> SOCKET_BITS == self.generation).then_some(socket & SOCKET_MASK)
    }

    fn outer(&self, socket: u32) -> u32 {
        (self.generation << SOCKET_BITS) | (socket & SOCKET_MASK)
    }

    /// Answers the client's notices and moves its actions out, until both are quiet.
    fn collect(&mut self, now_ms: f64) {
        for _ in 0..MAX_NOTICE_PASSES {
            let notices = self.client.take_notices();
            let actions = self.client.take_actions();
            if notices.is_empty() && actions.is_empty() {
                return;
            }
            for action in actions {
                let action = self.translate(action);
                self.actions.push(action);
            }
            for notice in notices {
                self.notice(notice, now_ms);
            }
        }
    }

    fn translate(&mut self, action: ClientAction) -> ClientAction {
        match action {
            ClientAction::OpenSocket { socket, url } => ClientAction::OpenSocket {
                socket: self.outer(socket),
                url,
            },
            ClientAction::Send { socket, text } => ClientAction::Send {
                socket: self.outer(socket),
                text,
            },
            ClientAction::CloseSocket { socket } => ClientAction::CloseSocket {
                socket: self.outer(socket),
            },
            ClientAction::SaveSeat { token, room_epoch } => {
                self.config.saved_seat = Some(SavedSeat {
                    token: token.clone(),
                    room_epoch: room_epoch.clone(),
                });
                ClientAction::SaveSeat { token, room_epoch }
            }
            ClientAction::ForgetSeat => {
                self.config.saved_seat = None;
                ClientAction::ForgetSeat
            }
        }
    }

    fn notice(&mut self, notice: ClientNotice, now_ms: f64) {
        match notice {
            ClientNotice::PrepareArena {
                round_id,
                room_epoch,
            } => {
                // A new round is a new host simulation: its ids restart, so the model does.
                if let Some(state) = self.client.arena_state() {
                    let lobby_map = self.client.lobby().map(|lobby| lobby.settings.map_mode);
                    self.cockpit.prepare(&state, lobby_map);
                    self.client.arena_prepared(round_id, &room_epoch, now_ms);
                } else {
                    self.client.arena_failed();
                }
            }
            ClientNotice::ShowBaseline { baseline } => {
                self.client.baseline_shown(baseline, now_ms);
            }
            ClientNotice::Status { text, .. } => self.status = text,
            ClientNotice::Notice(text) => self.notice = Some(text),
            ClientNotice::Ended { cause, text } => self.ended = Some(Ended { cause, text }),
            ClientNotice::Lobby(lobby) => self.lobby(&lobby, now_ms),
            ClientNotice::Result { outcome, team } => self.cockpit.result(&outcome, team),
            ClientNotice::ResetFeedback | ClientNotice::ClearInput | ClientNotice::Reveal => {}
        }
    }

    fn lobby(&mut self, lobby: &Lobby, now_ms: f64) {
        if self.phase != Some(lobby.phase) {
            self.phase = Some(lobby.phase);
            self.phase_since_ms = now_ms;
            if lobby.phase == RoomPhase::Playing {
                self.cockpit.tally.rounds += 1;
            }
        }
        self.align(lobby, now_ms);
    }

    /// Moves to the side the mission needs: the other team from a hunted player, the same
    /// as an escorted one. Between battles that is a team choice; during one the seat is
    /// given up and taken again on that side.
    fn align(&mut self, lobby: &Lobby, now_ms: f64) {
        let Some(wanted) = self.config.orders.mission.player() else {
            return;
        };
        let me = &self.client.player_id;
        let (Some(mine), Some(theirs)) = (
            lobby.players.iter().find(|player| &player.player_id == me),
            lobby
                .players
                .iter()
                .find(|player| &player.player_id != me && same_player(&player.name, wanted)),
        ) else {
            return;
        };
        let side = match self.config.orders.mission {
            Mission::Escort(_) => theirs.team,
            Mission::Hunt(_) | Mission::Skirmish => theirs.team.opponent(),
        };
        if mine.team == side || now_ms - self.last_realign_ms < REALIGN_MS {
            return;
        }
        self.last_realign_ms = now_ms;
        self.cockpit.tally.team_changes += 1;
        if lobby.phase == RoomPhase::Playing {
            self.rejoin(side, now_ms);
        } else {
            self.client
                .choose(Some(side), self.config.orders.kind, now_ms);
        }
    }

    /// Leaves and joins again on `side` with a new client, as a person would reload.
    fn rejoin(&mut self, side: Team, now_ms: f64) {
        self.client.leave(now_ms);
        self.collect(now_ms);
        self.generation += 1;
        self.config.saved_seat = None;
        self.client = client(&self.config);
        self.client.connect(
            JoinChoice {
                name: self.config.name.clone(),
                kind: self.config.orders.kind,
                team: Some(side),
                create: None,
                existing_room: Some(true),
            },
            now_ms,
        );
    }

    /// A pilot that hosts the room starts the next battle once the lobby or results have
    /// shown for a while, so a room it made never strands people.
    fn host_rounds(&mut self, now_ms: f64) {
        let Some(lobby) = self.client.lobby() else {
            return;
        };
        if lobby.host_id == self.client.player_id
            && lobby.phase != RoomPhase::Playing
            && self.ended.is_none()
            && now_ms - self.phase_since_ms >= self.config.orders.host_start_seconds * 1000.0
        {
            self.phase_since_ms = now_ms;
            self.client.start(now_ms);
        }
    }
}

fn client(config: &PilotConfig) -> NetworkClient {
    NetworkClient::new(ClientConfig {
        server_url: config.server_url.clone(),
        room: config.room.clone(),
        saved_seat: config.saved_seat.clone(),
        delay: None,
        seed: config.seed,
        // A pilot plays with the browser's defaults, so it feels what a player does.
        dodger_first: true,
        shooter_first: true,
    })
}

/// The player side of a pilot: the brain, its model of the battle and the hands on the
/// controls.
struct Cockpit {
    orders: Orders,
    world: Option<WorldModel>,
    seed: u32,
    rng: Random,
    /// The phases of the aim's two wobbles.
    wobble: [f64; 2],
    /// The hunted tank the brain last heard of.
    hunted: Option<u32>,
    fire_until_ms: f64,
    last_mine_ms: f64,
    requested_ammo: Option<(Weapon, f64)>,
    tally: Tally,
}

impl Cockpit {
    fn new(orders: Orders, seed: u32) -> Self {
        let mut rng = Random::new(f64::from(seed));
        let wobble = [
            rng.range(0.0, std::f64::consts::TAU),
            rng.range(0.0, std::f64::consts::TAU),
        ];
        Self {
            orders,
            world: None,
            seed,
            rng,
            wobble,
            hunted: None,
            fire_until_ms: f64::NEG_INFINITY,
            last_mine_ms: f64::NEG_INFINITY,
            requested_ammo: None,
            tally: Tally::default(),
        }
    }

    /// Builds the model of a new round's arena.
    fn prepare(&mut self, state: &RenderState, lobby_map: Option<MapId>) {
        let map = scene_map(state, lobby_map);
        // Each round's brain draws its own decisions.
        let seed = f64::from(self.seed) + self.rng.range(0.0, 1_000_000.0).floor();
        self.world = Some(WorldModel::new(
            map,
            self.orders.skill,
            self.orders.role(),
            seed,
        ));
        self.hunted = None;
        self.requested_ammo = None;
    }

    fn result(&mut self, outcome: &Match, team: Team) {
        match outcome.winner {
            Some(winner) if winner == team => self.tally.wins += 1,
            Some(_) => self.tally.losses += 1,
            None => {}
        }
    }

    /// The controls for the next frame, from the scene this frame displays.
    fn fly(
        &mut self,
        state: &RenderState,
        events: &[FrameEvent],
        dt: f64,
        now_ms: f64,
    ) -> LocalInput {
        let Some(me) = state.viewer() else {
            return LocalInput::default();
        };
        self.count(state, me, events);
        let idle = LocalInput {
            aim_angle: me.aim,
            ..LocalInput::default()
        };
        let Some(world) = self.world.as_mut() else {
            return idle;
        };
        if !me.alive {
            self.fire_until_ms = f64::NEG_INFINITY;
            return idle;
        }
        let leader = match &self.orders.mission {
            Mission::Escort(name) => state
                .tanks
                .iter()
                .find(|tank| {
                    tank.id != me.id && tank.team == me.team && same_player(&tank.name, name)
                })
                .map(|tank| tank.id),
            _ => None,
        };
        world.sync(state, Perspective { own: me.id, leader });
        let Some(index) = world.tank_index(me.id) else {
            return idle;
        };
        let simulation = &mut world.simulation;
        // Missions steer the brain through what a hit leaves in its memory: the attacker it
        // favors as a target and closes in on when nobody is in sight.
        let hunted = match &self.orders.mission {
            Mission::Hunt(name) => quarry(state, me, name.as_deref()),
            _ => None,
        };
        if let Some(target) = hunted {
            let brain = &mut simulation.tanks[index].brain;
            brain.attacker = target.id;
            brain.attacked_from = target.position.planar();
            brain.alarm = brain.alarm.max(HUNT_ALARM_SECONDS);
            if self.hunted != Some(target.id) && brain.target != target.id {
                brain.decision = 0.0;
            }
        }
        self.hunted = hunted.map(|target| target.id);
        for shown in events {
            let event = &shown.event;
            if !matches!(event.kind, SimEventType::Hurt | SimEventType::Death) {
                continue;
            }
            let (Some(victim), Some(owner)) = (event.id, event.owner) else {
                continue;
            };
            if victim == me.id && hunted.is_none() {
                notice_hit(simulation, index, owner, event.damage_source);
            }
            let Some(leader) = leader else {
                continue;
            };
            // An escort fires on what its leader fires on and turns on whoever hits it.
            let enemy = if owner == leader && victim != leader {
                Some(victim)
            } else if victim == leader && owner != leader {
                Some(owner)
            } else {
                None
            };
            let Some(enemy) = enemy.and_then(|id| state.tanks.iter().find(|tank| tank.id == id))
            else {
                continue;
            };
            if enemy.alive && enemy.team != me.team {
                let brain = &mut simulation.tanks[index].brain;
                brain.attacker = enemy.id;
                brain.attacked_from = enemy.position.planar();
                brain.alarm = ESCORT_ALARM_SECONDS;
                brain.decision = 0.0;
            }
        }
        let command = bot_command(simulation, index, dt);
        // The host turns a bot's turret to its command each tick; the model's is the brain's.
        simulation.tanks[index].aim = command.aim;
        if command.fire {
            self.tally.trigger_pulls += 1;
            self.fire_until_ms = now_ms + TRIGGER_HOLD_MS;
        }
        let mine = command.mine && now_ms - self.last_mine_ms >= MINE_RETRY_MS;
        if mine {
            self.last_mine_ms = now_ms;
        }
        let ammo_selection = match command.ammo_selection {
            Some(AmmoSelection::Weapon(weapon))
                if weapon != me.selected_ammo
                    && self.requested_ammo.is_none_or(|(asked, at)| {
                        asked != weapon || now_ms - at >= AMMO_RETRY_MS
                    }) =>
            {
                self.requested_ammo = Some((weapon, now_ms));
                Some(AmmoSelection::Weapon(weapon))
            }
            _ => None,
        };
        let seconds = now_ms / 1000.0;
        let wobble: f64 = (0..2)
            .map(|i| WOBBLE_WEIGHTS[i] * (seconds * WOBBLE_RATES[i] + self.wobble[i]).sin())
            .sum::<f64>()
            * aim_wobble(self.orders.skill);
        LocalInput {
            move_x: command.move_x,
            move_z: command.move_z,
            fire: now_ms < self.fire_until_ms,
            mine,
            ammo_selection,
            aim_angle: command.aim + wobble,
            aim_point: None,
        }
    }

    /// Counts the pilot's hits, kills, hits taken and deaths in a frame's events.
    fn count(&mut self, state: &RenderState, me: &RenderTank, events: &[FrameEvent]) {
        for shown in events {
            let event = &shown.event;
            let death = match event.kind {
                SimEventType::Hurt => false,
                SimEventType::Death => true,
                _ => continue,
            };
            let player = |id: Option<u32>| {
                id.is_some_and(|id| state.tanks.iter().any(|tank| tank.id == id && tank.human))
            };
            if event.id == Some(state.viewer_id) {
                self.tally.hits_taken += 1;
                self.tally.deaths += u32::from(death);
                self.tally.player_hits_taken +=
                    u32::from(event.owner != event.id && player(event.owner));
            } else if event.owner == Some(state.viewer_id) && event.team != Some(me.team) {
                self.tally.hits += 1;
                self.tally.kills += u32::from(death);
                self.tally.player_hits += u32::from(player(event.id));
            }
        }
    }
}

/// The live enemy a hunt goes after: the named player, or with no name the nearest enemy
/// player (not fill bot).
fn quarry<'a>(
    state: &'a RenderState,
    me: &RenderTank,
    name: Option<&str>,
) -> Option<&'a RenderTank> {
    let enemies = state
        .tanks
        .iter()
        .filter(|tank| tank.alive && tank.team != me.team);
    match name {
        Some(name) => enemies
            .into_iter()
            .find(|tank| same_player(&tank.name, name)),
        None => enemies.filter(|tank| tank.human).min_by(|a, b| {
            let from = me.position.planar();
            distance(from, a.position.planar()).total_cmp(&distance(from, b.position.planar()))
        }),
    }
}
