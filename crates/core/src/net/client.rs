//! The browser's multiplayer session as a transport-free state machine: the logic of
//! `src/net/connection.ts` and `src/net/client.ts` without WebSockets, timers, storage or
//! the DOM.
//!
//! # Driving it (for the web bindings)
//!
//! The page keeps one [`NetworkClient`] per room page and forwards everything with its
//! `performance.now()` clock in milliseconds:
//!
//! 1. [`NetworkClient::new`] with the server address, room code and any seat saved in
//!    `sessionStorage`; then [`connect`](NetworkClient::connect) with the player's
//!    [`JoinChoice`].
//! 2. After every call, drain [`take_actions`](NetworkClient::take_actions) and perform
//!    them in order: [`ClientAction::OpenSocket`] creates a WebSocket (remember its
//!    `socket` id), `Send` writes text to that socket, `CloseSocket` closes it, and
//!    `SaveSeat`/`ForgetSeat` update `sessionStorage`. Before a `Send`, a socket whose
//!    `bufferedAmount` exceeds 16 KiB should be closed instead (as the TypeScript did).
//! 3. Forward socket events: [`socket_opened`](NetworkClient::socket_opened),
//!    [`socket_message`](NetworkClient::socket_message) for text frames,
//!    [`socket_closed`](NetworkClient::socket_closed) with the close code. Events for a
//!    socket id that is no longer current are ignored.
//! 4. Call [`poll`](NetworkClient::poll) at least every 250 ms (a `setInterval` or each
//!    animation frame): it runs the 1 s heartbeat, reconnect backoff and the dev delay.
//! 5. Each animation frame, call [`frame`](NetworkClient::frame) with the local controls
//!    ([`LocalInput`]). It returns the [`RenderState`] to draw (the same type local play
//!    draws) and the events whose display time arrived, and sends input at the browser's
//!    cadence. `None` means nothing new to draw (arena not ready, hidden page).
//! 6. Drain [`take_notices`](NetworkClient::take_notices) for the UI: status lines,
//!    notices, the lobby, round results, the end of the connection, and two requests to
//!    the renderer: [`ClientNotice::PrepareArena`] (build and warm the arena for
//!    [`arena_state`](NetworkClient::arena_state), then call
//!    [`arena_prepared`](NetworkClient::arena_prepared) or
//!    [`arena_failed`](NetworkClient::arena_failed)) and [`ClientNotice::ShowBaseline`]
//!    (rebuild models for [`display`](NetworkClient::display), draw the first frames, then
//!    call [`baseline_shown`](NetworkClient::baseline_shown)). `ClearInput` asks the page to
//!    drop held keys and clicks; `Reveal` means the room page may replace Battle Setup.
//! 7. Menu buttons map to [`choose`](NetworkClient::choose),
//!    [`settings`](NetworkClient::settings), [`start`](NetworkClient::start),
//!    [`pause`](NetworkClient::pause), [`resume`](NetworkClient::resume),
//!    [`end`](NetworkClient::end), [`rejoin`](NetworkClient::rejoin),
//!    [`leave`](NetworkClient::leave) and [`select_ammo`](NetworkClient::select_ammo);
//!    page visibility to [`set_hidden`](NetworkClient::set_hidden), `pagehide` to
//!    [`stop`](NetworkClient::stop).

use serde_json::Value;

use super::input_cadence::{InputCadence, InputSample};
use super::network_timeline::NetworkTimeline;
use super::player_controls::{Action, Aim, ControlInput, MAX_QUEUED_ACTIONS, encode_input};
use super::protocol::{
    CONTENT_VERSION, Control, JoinChoice, Lobby, MAX_ROOM_MS, MAX_SERVER_MESSAGE_BYTES,
    PROTOCOL_VERSION, ROOM_IDLE_MS, RoomPhase, RoomSettings, client_message,
};
use super::replication::StateMirror;
use super::schema::{ReadResult, Record, id, parse_record, string, text_length};
use super::transport_delay::{DelaySettings, TransportDelay};
use crate::sim::ammunition::{AMMO_ORDER, has_ammo_for};
use crate::sim::math::Random;
use crate::sim::render_state::RenderState;
use crate::sim::types::{
    AmmoSelection, Driver, Match, MatchPhase, SimEvent, SimEventType, Team, Weapon,
};

const RECONNECT_WINDOW_MS: f64 = 30_000.0;
const HEARTBEAT_MS: f64 = 1000.0;
/// A socket that has not been welcomed, or has heard nothing, for this long is closed.
const SILENCE_MS: f64 = 10_000.0;
const MAX_RETRY_MS: f64 = 5000.0;
const FIRST_RETRY_MS: f64 = 500.0;
/// Most snapshot frames one batch may carry.
const MAX_BATCH_FRAMES: usize = 8;
/// A bot-driven seat asks for its tank back at most this often.
const RESUME_RETRY_MS: f64 = 1000.0;
const MAX_FRAME_SECONDS: f64 = 0.1;

/// Why a connection stopped retrying. Each cause offers its own way back into a game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndCause {
    /// The server stayed unreachable for the whole reconnect window.
    Lost,
    /// The room itself is gone: a server restart, a time limit or a server fault.
    RoomEnded,
    /// The server let the seat go; joining again takes a new one.
    SeatExpired,
    /// The same seat connected from another tab, which now drives the tank.
    OtherTab,
    /// This page and the server run different game versions.
    Outdated,
    Rejected,
}

impl EndCause {
    pub fn as_str(self) -> &'static str {
        match self {
            EndCause::Lost => "lost",
            EndCause::RoomEnded => "room-ended",
            EndCause::SeatExpired => "seat-expired",
            EndCause::OtherTab => "other-tab",
            EndCause::Outdated => "outdated",
            EndCause::Rejected => "rejected",
        }
    }
}

/// Server `room-reset` reasons, in words a player can act on.
pub fn room_end_text(reason: &str) -> String {
    match reason {
        "server-restart" => "The game server restarted, which closed every room.".into(),
        "expired" => format!(
            "Rooms close after {} hours, or after {} idle minutes between battles.",
            MAX_ROOM_MS / 3_600_000,
            ROOM_IDLE_MS / 60_000
        ),
        "overload" => "The game server fell behind and had to close this room.".into(),
        "simulation-error" => "The battle hit a server error and the room closed.".into(),
        _ => "The room was closed.".into(),
    }
}

/// Fatal server error codes that are not a plain rejection.
fn fatal_cause(code: Option<&str>) -> EndCause {
    match code {
        Some("incompatible") => EndCause::Outdated,
        Some("seat-expired") => EndCause::SeatExpired,
        Some("expired") | Some("room-gone") => EndCause::RoomEnded,
        _ => EndCause::Rejected,
    }
}

/// What the page must do with sockets and storage, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientAction {
    /// Open a WebSocket to `url` and report its events with this `socket` id.
    OpenSocket {
        socket: u32,
        url: String,
    },
    Send {
        socket: u32,
        text: String,
    },
    CloseSocket {
        socket: u32,
    },
    /// Keep the seat for a reload (`sessionStorage`, keyed by server and room).
    SaveSeat {
        token: String,
        room_epoch: String,
    },
    ForgetSeat,
}

/// UI and renderer notifications, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum ClientNotice {
    /// Progress while (re)connecting; `connected` once the seat is back.
    Status {
        text: String,
        connected: bool,
    },
    /// A server answer to the player's last request, such as a full team.
    Notice(String),
    /// The connection gave up and will not retry on its own.
    Ended {
        cause: EndCause,
        text: String,
    },
    Lobby(Lobby),
    /// The finished round as the viewer's tank played it; precedes its results lobby.
    Result {
        outcome: Match,
        team: Team,
    },
    /// A new room instance or round: clear the kill feed and damage indicators.
    ResetFeedback,
    /// Drop held keys, clicks and pending ammo choices.
    ClearInput,
    /// The room page may replace Battle Setup.
    Reveal,
    /// Build and warm the arena for [`NetworkClient::arena_state`], then call
    /// [`NetworkClient::arena_prepared`].
    PrepareArena {
        round_id: u64,
        room_epoch: String,
    },
    /// Rebuild models for [`NetworkClient::display`] and draw the first frames, then call
    /// [`NetworkClient::baseline_shown`].
    ShowBaseline {
        baseline: u32,
    },
}

/// A seat kept across reloads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedSeat {
    pub token: String,
    pub room_epoch: String,
}

pub struct ClientConfig {
    /// The server's WebSocket origin, such as `ws://127.0.0.1:8787`.
    pub server_url: String,
    pub room: String,
    pub saved_seat: Option<SavedSeat>,
    /// Development transport delay (`?latency=&jitter=&stall=`).
    pub delay: Option<DelaySettings>,
    /// Seeds the delay's jitter.
    pub seed: u32,
}

/// The local controls for one frame, after the page resolved the camera and pointer.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LocalInput {
    pub move_x: f64,
    pub move_z: f64,
    pub fire: bool,
    /// A mine click this frame (one-shot).
    pub mine: bool,
    /// An ammo key, wheel step or button this frame (one-shot).
    pub ammo_selection: Option<AmmoSelection>,
    /// Turret angle for immediate display (radians around Y).
    pub aim_angle: f64,
    /// The world point under the pointer, sent instead of the angle when set (not in first
    /// person or touch aiming).
    pub aim_point: Option<(f64, f64)>,
}

/// One displayed event with the viewer-relative flags presentation and audio need.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameEvent {
    pub event: SimEvent,
    /// The viewer hurt or killed an enemy.
    pub player_hit: bool,
    /// The event is about the viewer's own tank.
    pub own: bool,
}

/// What to draw this frame.
pub struct ClientFrame<'a> {
    pub state: &'a RenderState,
    pub events: Vec<FrameEvent>,
    /// Seconds since the previous frame, capped at 0.1.
    pub dt: f64,
}

/// Rows for the network statistics overlay.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NetworkStats {
    pub rtt_ms: f64,
    /// Full states and snapshot batches received this page session.
    pub received_updates: u64,
    pub snapshot_age_ms: f64,
    pub buffer_ms: f64,
    pub margin_ms: f64,
    pub underrun: f64,
    pub server_tick: u64,
    pub input_seq: i64,
    pub input_ack: i64,
    pub connected: bool,
}

struct Socket {
    id: u32,
    open: bool,
}

pub struct NetworkClient {
    config: ClientConfig,
    // Connection (connection.ts).
    pub room_epoch: String,
    pub round_id: u64,
    pub player_id: String,
    pub observed_tick: u64,
    pub rtt_ms: f64,
    pub connected: bool,
    /// Set once the connection gives up or leaves; until then it retries on its own.
    pub stopped: bool,
    token: Option<String>,
    socket: Option<Socket>,
    next_socket: u32,
    retry_at: Option<f64>,
    heartbeat_at: Option<f64>,
    opened_at: f64,
    retry_started: Option<f64>,
    attempt: u32,
    last_message_at: f64,
    choice: Option<JoinChoice>,
    selected_choice: Option<JoinChoice>,
    delay: Option<TransportDelay>,
    // Game state (client.ts).
    mirror: StateMirror,
    timeline: NetworkTimeline,
    control: Option<Control>,
    display: Option<RenderState>,
    ready_round: u64,
    preparing: bool,
    has_arena: bool,
    active: bool,
    seq: i64,
    pending: Vec<Action>,
    pending_weapon: Option<Weapon>,
    requested_ammo: Option<Weapon>,
    last_frame_ms: Option<f64>,
    cadence: InputCadence,
    requested_full: bool,
    last_resume_ms: f64,
    phase: RoomPhase,
    lobby: Option<Lobby>,
    menu: bool,
    hidden: bool,
    joining: bool,
    baselines: u32,
    received_updates: u64,
    last_snapshot_ms: f64,
    applied_input: i64,
    actions: Vec<ClientAction>,
    notices: Vec<ClientNotice>,
}

impl NetworkClient {
    pub fn new(config: ClientConfig) -> Self {
        let (token, room_epoch) = match &config.saved_seat {
            Some(seat) => (Some(seat.token.clone()), seat.room_epoch.clone()),
            None => (None, String::new()),
        };
        Self {
            config,
            room_epoch,
            round_id: 0,
            player_id: String::new(),
            observed_tick: 0,
            rtt_ms: 0.0,
            connected: false,
            stopped: false,
            token,
            socket: None,
            next_socket: 0,
            retry_at: None,
            heartbeat_at: None,
            opened_at: 0.0,
            retry_started: None,
            attempt: 0,
            last_message_at: 0.0,
            choice: None,
            selected_choice: None,
            delay: None,
            mirror: StateMirror::default(),
            timeline: NetworkTimeline::default(),
            control: None,
            display: None,
            ready_round: 0,
            preparing: false,
            has_arena: false,
            active: false,
            seq: 0,
            pending: Vec::new(),
            pending_weapon: None,
            requested_ammo: None,
            last_frame_ms: None,
            cadence: InputCadence::default(),
            requested_full: false,
            last_resume_ms: f64::NEG_INFINITY,
            phase: RoomPhase::Lobby,
            lobby: None,
            menu: false,
            hidden: false,
            joining: true,
            baselines: 0,
            received_updates: 0,
            last_snapshot_ms: 0.0,
            applied_input: 0,
            actions: Vec::new(),
            notices: Vec::new(),
        }
    }

    pub fn take_actions(&mut self) -> Vec<ClientAction> {
        std::mem::take(&mut self.actions)
    }

    pub fn take_notices(&mut self) -> Vec<ClientNotice> {
        std::mem::take(&mut self.notices)
    }

    // ---- Accessors ------------------------------------------------------------------

    pub fn phase(&self) -> RoomPhase {
        self.phase
    }

    pub fn lobby(&self) -> Option<&Lobby> {
        self.lobby.as_ref()
    }

    pub fn control(&self) -> Option<&Control> {
        self.control.as_ref()
    }

    /// The last displayed (or baseline) scene.
    pub fn display(&self) -> Option<&RenderState> {
        self.display.as_ref()
    }

    /// The replicated scene for the controlled tank, for arena preparation.
    pub fn arena_state(&self) -> Option<RenderState> {
        let control = self.control.as_ref()?;
        self.mirror.render(control.tank_id).ok()
    }

    pub fn mirror(&self) -> &StateMirror {
        &self.mirror
    }

    pub fn menu_open(&self) -> bool {
        self.menu
    }

    /// Whether local controls drive the tank now (`activeInput`).
    pub fn active_input(&self) -> bool {
        let viewer_ok = match (&self.display, &self.control) {
            (Some(display), Some(control)) => display
                .viewer()
                .is_some_and(|viewer| viewer.alive && viewer.life == control.life),
            _ => false,
        };
        self.active
            && self.connected
            && !self.menu
            && !self.hidden
            && self.phase == RoomPhase::Playing
            && self
                .control
                .as_ref()
                .is_some_and(|control| control.driver == Driver::Human)
            && viewer_ok
            && !self.mirror.needs_full
    }

    pub fn stats(&self, now_ms: f64) -> NetworkStats {
        NetworkStats {
            rtt_ms: self.rtt_ms,
            received_updates: self.received_updates,
            snapshot_age_ms: now_ms - self.last_snapshot_ms,
            buffer_ms: self.timeline.clock.buffer_ms,
            margin_ms: self.timeline.margin_ms(),
            underrun: self.timeline.clock.underrun,
            server_tick: self.mirror.tick,
            input_seq: self.seq,
            input_ack: self.applied_input,
            connected: self.connected,
        }
    }

    // ---- Connection -----------------------------------------------------------------

    fn socket_url(&self) -> String {
        format!(
            "{}/room/{}",
            self.config.server_url.trim_end_matches('/'),
            self.config.room
        )
    }

    fn is_current(&self, socket: u32) -> bool {
        !self.stopped
            && self
                .socket
                .as_ref()
                .is_some_and(|current| current.id == socket)
    }

    /// Joins (or rejoins) the room with `choice`, replacing any current socket.
    pub fn connect(&mut self, choice: JoinChoice, now_ms: f64) {
        self.stop();
        if self.selected_choice.is_none() {
            self.selected_choice = Some(choice.clone());
        }
        self.choice = Some(choice);
        self.stopped = false;
        self.retry_started = None;
        self.attempt = 0;
        if let Some(settings) = self.config.delay {
            let mut random = Random::new(f64::from(self.config.seed));
            self.delay = Some(TransportDelay::new(
                settings,
                Box::new(move || random.next()),
            ));
        }
        self.open(now_ms);
    }

    fn open(&mut self, now_ms: f64) {
        if self.stopped || self.choice.is_none() {
            return;
        }
        let text = if self.attempt > 0 {
            "Still trying to reach the game server. Your seat is held for 30 seconds."
        } else {
            "Connecting to the room…"
        };
        self.status(text, false);
        self.next_socket += 1;
        let id = self.next_socket;
        self.socket = Some(Socket { id, open: false });
        self.actions.push(ClientAction::OpenSocket {
            socket: id,
            url: self.socket_url(),
        });
        self.opened_at = now_ms;
        self.last_message_at = now_ms;
        self.heartbeat_at = Some(now_ms + HEARTBEAT_MS);
    }

    pub fn socket_opened(&mut self, socket: u32, _now_ms: f64) {
        if !self.is_current(socket) {
            return;
        }
        if let Some(current) = self.socket.as_mut() {
            current.open = true;
        }
        let choice = self.choice.as_ref().expect("an open socket has a choice");
        let text = choice.join_message(self.token.as_deref(), Some(&self.room_epoch));
        self.raw(text, _now_ms);
    }

    pub fn socket_message(&mut self, socket: u32, text: &str, now_ms: f64) {
        if !self.is_current(socket) {
            return;
        }
        if text_length(text) > MAX_SERVER_MESSAGE_BYTES {
            self.fail(
                EndCause::Rejected,
                "The server sent a message this page can't read.",
            );
            return;
        }
        match self.delay.as_mut() {
            Some(delay) => {
                if delay.receive(text.to_string(), now_ms).is_err() {
                    self.fail(
                        EndCause::Rejected,
                        "The server sent a message this page can't read.",
                    );
                }
            }
            None => self.receive(text, now_ms),
        }
    }

    pub fn socket_closed(&mut self, socket: u32, code: u16, now_ms: f64) {
        if !self.is_current(socket) {
            return;
        }
        self.socket = None;
        self.connected = false;
        if let Some(delay) = self.delay.as_mut() {
            delay.clear();
        }
        self.clear_input();
        self.heartbeat_at = None;
        if code == 4001 {
            self.fail(
                EndCause::OtherTab,
                "Your seat is now playing in another tab or window.",
            );
            return;
        }
        if code == 1008 {
            self.fail(EndCause::Rejected, "The server closed the connection.");
            return;
        }
        let started = *self.retry_started.get_or_insert(now_ms);
        if now_ms - started >= RECONNECT_WINDOW_MS {
            self.fail(
                EndCause::Lost,
                "The game server hasn't answered for 30 seconds, so your seat may be gone.",
            );
            return;
        }
        self.status("Reconnecting…", false);
        let backoff = (FIRST_RETRY_MS * 2f64.powi(self.attempt as i32)).min(MAX_RETRY_MS);
        self.retry_at = Some(now_ms + backoff);
    }

    /// Timers: reconnect backoff, the 1 s heartbeat (ping, or close a silent socket) and
    /// the development delay's deliveries.
    pub fn poll(&mut self, now_ms: f64) {
        if self.retry_at.is_some_and(|at| at <= now_ms) {
            self.retry_at = None;
            self.attempt += 1;
            self.open(now_ms);
        }
        if let (Some(at), Some(socket)) = (self.heartbeat_at, self.socket.as_ref())
            && at <= now_ms
            && !self.stopped
        {
            let socket = socket.id;
            let next = at + HEARTBEAT_MS;
            self.heartbeat_at = Some(if next <= now_ms {
                now_ms + HEARTBEAT_MS
            } else {
                next
            });
            if (!self.connected && now_ms - self.opened_at > SILENCE_MS)
                || now_ms - self.last_message_at > SILENCE_MS
            {
                self.actions.push(ClientAction::CloseSocket { socket });
            } else if self.connected {
                let t = crate::sim::math::js_round(now_ms);
                let observed = self.observed_tick;
                self.send("ping", now_ms, |writer| {
                    writer.number("t", t).int("observedTick", observed);
                });
            }
        }
        if let Some(delay) = self.delay.as_mut() {
            let outbound = delay.outbound.receive(now_ms);
            let inbound = delay.inbound.receive(now_ms);
            for text in outbound {
                if let Some(socket) = self.socket.as_ref().filter(|socket| socket.open)
                    && !self.stopped
                {
                    self.actions.push(ClientAction::Send {
                        socket: socket.id,
                        text,
                    });
                }
            }
            for text in inbound {
                if self.socket.is_some() && !self.stopped {
                    self.receive(&text, now_ms);
                }
            }
        }
    }

    fn receive(&mut self, text: &str, now_ms: f64) {
        if let Err(error) = self.handle(text, now_ms) {
            // Anything the page cannot read means the page and server disagree.
            let _ = error;
            self.fail(
                EndCause::Outdated,
                "This page couldn't read the game state. Reload to get the latest version.",
            );
        }
    }

    fn handle(&mut self, text: &str, now_ms: f64) -> ReadResult<()> {
        let message = parse_record(text)?;
        self.last_message_at = now_ms;
        let kind = message
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        match kind.as_str() {
            "welcome" => {
                if message.get("version").and_then(Value::as_f64)
                    != Some(f64::from(PROTOCOL_VERSION))
                    || message.get("contentVersion").and_then(Value::as_str)
                        != Some(CONTENT_VERSION)
                {
                    self.fail(
                        EndCause::Outdated,
                        "Sloppy Tanks was updated. Reload to get the new version.",
                    );
                    return Ok(());
                }
                self.room_epoch = string(message.get("roomEpoch"), 128, 1)?;
                self.player_id = string(message.get("playerId"), 128, 1)?;
                let token = string(message.get("token"), 128, 16)?;
                self.token = Some(token.clone());
                self.connected = true;
                self.retry_started = None;
                self.attempt = 0;
                self.observed_tick = 0;
                self.clear_input();
                self.actions.push(ClientAction::SaveSeat {
                    token,
                    room_epoch: self.room_epoch.clone(),
                });
                self.status("Connected", true);
                if message.get("reset").and_then(Value::as_bool) == Some(true) {
                    self.notices.push(ClientNotice::Notice(
                        "The server restarted the room. This is a fresh lobby.".into(),
                    ));
                }
            }
            "pong" => {
                let sent = message
                    .get("t")
                    .and_then(Value::as_f64)
                    .filter(|t| t.is_finite())
                    .ok_or("Invalid pong")?;
                self.rtt_ms = (now_ms - sent).max(0.0);
                id(message.get("tick"))?;
            }
            "error" => {
                let text = string(message.get("message"), 200, 0)?;
                if message.get("fatal").is_some_and(truthy) {
                    let cause = fatal_cause(message.get("code").and_then(Value::as_str));
                    if cause == EndCause::SeatExpired {
                        self.forget_seat();
                    }
                    self.fail(cause, &text);
                    return Ok(());
                }
                self.notices.push(ClientNotice::Notice(text));
            }
            "room-reset" => {
                self.forget_seat();
                let reason = string(message.get("reason"), 80, 0)?;
                self.fail(EndCause::RoomEnded, &room_end_text(&reason));
                return Ok(());
            }
            _ => {}
        }
        self.on_message(&kind, message, now_ms)
    }

    fn on_message(&mut self, kind: &str, message: Record, now_ms: f64) -> ReadResult<()> {
        match kind {
            "welcome" => {
                if self.mirror.room_epoch != self.room_epoch {
                    self.ready_round = 0;
                    self.notices.push(ClientNotice::ResetFeedback);
                }
                self.mirror.needs_full = true;
                self.active = false;
                self.control = None;
                self.requested_full = false;
            }
            "lobby" => {
                let lobby = Lobby::read(&message)?;
                if lobby.room_epoch != self.room_epoch {
                    return Ok(());
                }
                if lobby.round_id != self.round_id {
                    self.round_id = lobby.round_id;
                    self.observed_tick = 0;
                    self.mirror.needs_full = true;
                    self.active = false;
                    self.control = None;
                    self.clear_input();
                    self.notices.push(ClientNotice::ResetFeedback);
                }
                self.phase = lobby.phase;
                // The final snapshot precedes the results lobby. A suspended seat (menu open)
                // receives no snapshots, so its mirror may still hold the round in play.
                if self.phase == RoomPhase::Results
                    && let Some(control) = &self.control
                    && self
                        .mirror
                        .state
                        .as_ref()
                        .is_some_and(|state| state.match_state.phase == MatchPhase::Results)
                {
                    let last = self.mirror.render(control.tank_id)?;
                    let team = last.viewer().map_or(Team::Blue, |viewer| viewer.team);
                    self.notices.push(ClientNotice::Result {
                        outcome: last.match_state,
                        team,
                    });
                }
                if self.phase == RoomPhase::Results {
                    self.menu = false;
                }
                let phase = self.phase;
                self.lobby = Some(lobby.clone());
                self.notices.push(ClientNotice::Lobby(lobby));
                if phase != RoomPhase::Playing {
                    self.clear_input();
                    // A room between battles needs its menu now. A new room starts its battle
                    // right after this first lobby, so it keeps loading behind Battle Setup.
                    let creating = self
                        .selected_choice
                        .as_ref()
                        .is_some_and(|choice| choice.create.is_some());
                    if phase == RoomPhase::Results || !creating {
                        self.reveal();
                    }
                }
            }
            "control" => {
                let next = Control::read(&message)?;
                if next.room_epoch != self.room_epoch || next.round_id != self.round_id {
                    return Ok(());
                }
                let changed = self.control.as_ref().is_none_or(|control| {
                    next.control_epoch != control.control_epoch || next.life != control.life
                });
                if changed {
                    self.clear_input();
                }
                let driver = next.driver;
                self.control = Some(next);
                if driver != Driver::Human
                    && !self.menu
                    && !self.hidden
                    && self.ready_round == self.round_id
                    && now_ms - self.last_resume_ms > RESUME_RETRY_MS
                {
                    self.last_resume_ms = now_ms;
                    self.send("resume", now_ms, |_| {});
                }
            }
            "full" => {
                self.last_snapshot_ms = now_ms;
                self.applied_input = 0;
                let value = Value::Object(message);
                self.mirror
                    .apply_full(&value, &self.room_epoch.clone(), self.round_id)?;
                self.received_updates += 1;
                self.observed_tick = self.mirror.tick;
                self.requested_full = false;
                self.clear_input();
                self.reset_display(now_ms)?;
                if self.ready_round != self.round_id {
                    self.prepare(now_ms);
                } else if self.has_arena && self.display.is_some() {
                    self.show_baseline();
                }
            }
            "snapshot" => {
                self.applied_input = id(message.get("ack"))? as i64;
                self.last_snapshot_ms = now_ms;
                let frames = match message.get("snapshots") {
                    Some(Value::Array(frames)) if frames.len() <= MAX_BATCH_FRAMES => frames,
                    _ => return Err("Invalid frame batch".into()),
                };
                self.received_updates += 1;
                if message.get("roundId").and_then(Value::as_f64) != Some(self.round_id as f64) {
                    return Ok(());
                }
                let mut pushed = false;
                for frame in frames {
                    let Some(extras) = self.mirror.apply_snapshot(frame) else {
                        self.request_full(now_ms);
                        break;
                    };
                    self.observed_tick = self.mirror.tick;
                    if let Some(control) = &self.control
                        && self.ready_round == self.round_id
                        && !self.hidden
                    {
                        let pushed_frame = self.mirror.render(control.tank_id).and_then(|state| {
                            self.timeline.push(
                                &state,
                                self.mirror.tick,
                                extras.events,
                                extras.traces,
                            )
                        });
                        if pushed_frame.is_err() {
                            self.request_full(now_ms);
                            break;
                        }
                        pushed = true;
                    }
                }
                if pushed {
                    self.timeline.arrive(self.last_snapshot_ms);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn send(
        &mut self,
        kind: &str,
        now_ms: f64,
        fields: impl FnOnce(&mut super::json::ObjectWriter<'_>),
    ) -> bool {
        let text = client_message(kind, self.round_id, fields);
        self.raw(text, now_ms)
    }

    fn raw(&mut self, text: String, now_ms: f64) -> bool {
        let Some(socket) = self.socket.as_ref().filter(|socket| socket.open) else {
            return false;
        };
        if self.stopped {
            return false;
        }
        let socket = socket.id;
        match self.delay.as_mut() {
            Some(delay) => delay.send(text, now_ms).is_ok(),
            None => {
                self.actions.push(ClientAction::Send { socket, text });
                true
            }
        }
    }

    fn forget_seat(&mut self) {
        self.token = None;
        self.room_epoch.clear();
        self.actions.push(ClientAction::ForgetSeat);
    }

    fn fail(&mut self, cause: EndCause, text: &str) {
        self.stop();
        self.active = false;
        self.clear_input();
        self.notices.push(ClientNotice::Ended {
            cause,
            text: text.to_string(),
        });
    }

    /// Stops retrying and closes the socket (`pagehide`, or before a new connect).
    pub fn stop(&mut self) {
        self.stopped = true;
        self.connected = false;
        self.retry_at = None;
        self.heartbeat_at = None;
        if let Some(delay) = self.delay.as_mut() {
            delay.clear();
        }
        self.clear_input();
        if let Some(socket) = self.socket.take() {
            self.actions
                .push(ClientAction::CloseSocket { socket: socket.id });
        }
    }

    fn status(&mut self, text: &str, connected: bool) {
        if !connected {
            self.active = false;
            self.clear_input();
        }
        self.notices.push(ClientNotice::Status {
            text: text.to_string(),
            connected,
        });
    }

    // ---- Arena lifecycle --------------------------------------------------------------

    fn clear_input(&mut self) {
        self.pending.clear();
        self.pending_weapon = None;
        self.requested_ammo = None;
        if self.notices.last() != Some(&ClientNotice::ClearInput) {
            self.notices.push(ClientNotice::ClearInput);
        }
    }

    fn reveal(&mut self) {
        if self.joining {
            self.joining = false;
            self.notices.push(ClientNotice::Reveal);
        }
    }

    fn reset_display(&mut self, now_ms: f64) -> ReadResult<()> {
        let Some(control) = &self.control else {
            return Ok(());
        };
        if self.mirror.state.is_none() {
            return Ok(());
        }
        let display = self.mirror.render(control.tank_id)?;
        self.timeline.reset(&display, self.mirror.tick, now_ms);
        self.display = Some(display);
        Ok(())
    }

    fn request_full(&mut self, now_ms: f64) {
        if !self.requested_full {
            self.requested_full = true;
            self.active = false;
            self.clear_input();
            self.send("resync", now_ms, |_| {});
        }
    }

    fn prepare(&mut self, now_ms: f64) {
        if self.preparing || self.control.is_none() || self.mirror.state.is_none() {
            return;
        }
        self.preparing = true;
        self.active = false;
        self.send("suspend", now_ms, |_| {});
        self.status("Preparing the arena…", true);
        self.notices.push(ClientNotice::PrepareArena {
            round_id: self.round_id,
            room_epoch: self.room_epoch.clone(),
        });
    }

    /// The renderer finished preparing the arena requested by
    /// [`ClientNotice::PrepareArena`] for `round_id` and `room_epoch`.
    pub fn arena_prepared(&mut self, round_id: u64, room_epoch: &str, now_ms: f64) {
        self.preparing = false;
        self.has_arena = true;
        let current = round_id == self.round_id && room_epoch == self.room_epoch;
        if current && self.mirror.state.is_some() && self.control.is_some() {
            self.ready_round = round_id;
            if self.reset_display(now_ms).is_err() {
                self.receive_failure();
                return;
            }
            self.status("Connected", true);
            self.last_resume_ms = now_ms;
            // Resuming sends a fresh full state; its baseline starts the arena.
            self.send("resume", now_ms, |_| {});
        }
        // The host may start a new round while GPU compilation for the old one is pending.
        if !current && self.mirror.state.is_some() && self.control.is_some() && self.connected {
            self.prepare(now_ms);
        }
    }

    /// The renderer could not build the arena; the page returns to Battle Setup.
    pub fn arena_failed(&mut self) {
        self.preparing = false;
        self.active = false;
        self.clear_input();
    }

    fn receive_failure(&mut self) {
        self.fail(
            EndCause::Outdated,
            "This page couldn't read the game state. Reload to get the latest version.",
        );
    }

    /// A full state makes the renderer rebuild every model, and drawing new models the
    /// first time stalls; input waits until the first frames are drawn.
    fn show_baseline(&mut self) {
        self.baselines += 1;
        self.active = false;
        self.notices.push(ClientNotice::ShowBaseline {
            baseline: self.baselines,
        });
    }

    /// The renderer drew the first frames of baseline `baseline`.
    pub fn baseline_shown(&mut self, baseline: u32, now_ms: f64) {
        if baseline != self.baselines || self.ready_round != self.round_id || self.control.is_none()
        {
            return;
        }
        // Snapshots kept arriving while the frames were drawn.
        if self.reset_display(now_ms).is_err() {
            self.receive_failure();
            return;
        }
        self.active = true;
        self.reveal();
    }

    // ---- UI intents -------------------------------------------------------------------

    /// Change team (`None`: auto) or tank between battles.
    pub fn choose(
        &mut self,
        team: Option<Team>,
        kind: crate::sim::types::VehicleKind,
        now_ms: f64,
    ) {
        self.send("choose", now_ms, |writer| {
            if let Some(team) = team {
                writer.int("team", team.index() as u64);
            }
            writer.string("kind", kind.as_str());
        });
    }

    /// The host's rules for the next battle.
    pub fn settings(&mut self, settings: RoomSettings, now_ms: f64) {
        self.send("settings", now_ms, |writer| settings.write_fields(writer));
    }

    pub fn start(&mut self, now_ms: f64) {
        self.clear_input();
        self.send("start", now_ms, |_| {});
    }

    /// Opens the in-battle menu; a bot (or idle, humans-only) drives meanwhile and the
    /// battle keeps playing behind the menu.
    pub fn pause(&mut self, now_ms: f64) {
        if self.phase != RoomPhase::Playing || self.menu || self.joining {
            return;
        }
        self.menu = true;
        self.clear_input();
        self.watch(now_ms);
    }

    /// Hand the tank over but keep the snapshot stream, so the menu shows a live battle.
    fn watch(&mut self, now_ms: f64) {
        self.send("suspend", now_ms, |writer| {
            writer.boolean("watch", true);
        });
    }

    /// Closes the menu and asks for the tank back with a fresh baseline.
    pub fn resume(&mut self, now_ms: f64) {
        if !self.connected {
            return;
        }
        self.clear_input();
        self.menu = false;
        self.active = false;
        self.last_resume_ms = now_ms;
        self.send("resume", now_ms, |_| {});
    }

    /// The UI opened or closed its menu by itself.
    pub fn set_menu(&mut self, open: bool) {
        self.menu = open;
    }

    pub fn end(&mut self, now_ms: f64) {
        self.clear_input();
        self.send("end", now_ms, |_| {});
    }

    /// Joins this room again after the connection ended: a seat the server still holds
    /// resumes; otherwise this takes a new one.
    pub fn rejoin(&mut self, now_ms: f64) {
        let Some(choice) = self.selected_choice.clone() else {
            return;
        };
        self.connect(
            JoinChoice {
                create: None,
                existing_room: Some(true),
                ..choice
            },
            now_ms,
        );
    }

    /// Gives the seat up and closes the socket; the page then leaves.
    pub fn leave(&mut self, now_ms: f64) {
        self.send("leave", now_ms, |_| {});
        self.forget_seat();
        self.stop();
    }

    /// An ammo button in the HUD.
    pub fn select_ammo(&mut self, weapon: Weapon) {
        if self.active_input() {
            self.requested_ammo = Some(weapon);
        }
    }

    /// Page visibility: a hidden page suspends its seat; showing it resumes a battle.
    pub fn set_hidden(&mut self, hidden: bool, now_ms: f64) {
        self.hidden = hidden;
        self.clear_input();
        if hidden {
            self.send("suspend", now_ms, |_| {});
            self.active = false;
        } else if self.phase == RoomPhase::Playing && !self.menu {
            self.resume(now_ms);
        } else if self.phase == RoomPhase::Playing {
            // Back behind an open menu: watch again from a fresh baseline, which also
            // reactivates the display when no snapshot went missing meanwhile.
            self.watch(now_ms);
            self.request_full(now_ms);
        }
    }

    // ---- Frames -----------------------------------------------------------------------

    /// Advances the display and returns what to draw, sending input due at this frame.
    pub fn frame(&mut self, now_ms: f64, input: &LocalInput) -> Option<ClientFrame<'_>> {
        let dt = self.last_frame_ms.map_or(0.0, |last| {
            ((now_ms - last) / 1000.0).clamp(0.0, MAX_FRAME_SECONDS)
        });
        self.last_frame_ms = Some(now_ms);
        if !self.active
            || !self.has_arena
            || self.control.is_none()
            || self.mirror.state.is_none()
            || self.hidden
        {
            return None;
        }
        // The battle keeps playing behind the in-battle menu too.
        let (state, displayed) = self.timeline.read(now_ms, self.rtt_ms, dt);
        match &mut self.display {
            Some(display) => display.clone_from(state),
            None => self.display = Some(state.clone()),
        }
        let display = self.display.as_ref().expect("just set");
        let viewer_id = display.viewer_id;
        let viewer_team = display.viewer().map(|viewer| viewer.team);
        let events = displayed
            .into_iter()
            .map(|event| FrameEvent {
                player_hit: matches!(event.kind, SimEventType::Hurt | SimEventType::Death)
                    && event.owner == Some(viewer_id)
                    && event.team != viewer_team,
                own: event.id == Some(viewer_id),
                event,
            })
            .collect();
        self.collect(now_ms, input);
        Some(ClientFrame {
            state: self.display.as_ref().expect("checked above"),
            events,
            dt,
        })
    }

    fn collect(&mut self, now_ms: f64, input: &LocalInput) {
        if !self.active_input() {
            return;
        }
        let (Some(display), Some(control)) = (self.display.as_mut(), self.control.as_ref()) else {
            return;
        };
        // Aim is immediate presentation feedback; only the server decides what the shot hits.
        let viewer_id = display.viewer_id;
        for tank in &mut display.tanks {
            if tank.id == viewer_id {
                tank.aim = input.aim_angle;
            }
        }
        let viewer = display.viewer().expect("active input has a viewer").clone();
        let control_epoch = control.control_epoch;
        if input.mine {
            self.pending.push(Action::Mine);
        }
        let selection = input
            .ammo_selection
            .or(self.requested_ammo.take().map(AmmoSelection::Weapon));
        if let Some(selection) = selection {
            let weapon = match selection {
                AmmoSelection::Weapon(weapon) => weapon,
                AmmoSelection::Step(step) => {
                    let current = self.pending_weapon.unwrap_or(viewer.selected_ammo);
                    let count = AMMO_ORDER.len() as i64;
                    let index = AMMO_ORDER
                        .iter()
                        .position(|weapon| *weapon == current)
                        .map_or(-1, |index| index as i64);
                    (1..=count)
                        .map(|offset| {
                            AMMO_ORDER[(index + i64::from(step) * offset + count).rem_euclid(count)
                                as usize]
                        })
                        .find(|candidate| has_ammo_for(viewer.kind, &viewer.ammo, *candidate))
                        .unwrap_or(current)
                }
            };
            self.pending_weapon = Some(weapon);
            self.pending.push(Action::Ammo(weapon));
        }
        if self.pending.len() > MAX_QUEUED_ACTIONS {
            let excess = self.pending.len() - MAX_QUEUED_ACTIONS;
            self.pending.drain(..excess);
        }
        let sample = InputSample {
            control_epoch,
            move_x: input.move_x,
            move_z: input.move_z,
            aim: match input.aim_point {
                Some((x, z)) => Aim::Point { x, z },
                None => Aim::Angle(input.aim_angle),
            },
            fire: input.fire,
            actions: self.pending.clone(),
        };
        if !self.cadence.due(&sample, now_ms) {
            return;
        }
        let text = encode_input(
            &ControlInput {
                control_epoch,
                seq: self.seq + 1,
                observed_tick: self.mirror.tick as i64,
                move_x: sample.move_x,
                move_z: sample.move_z,
                aim: sample.aim,
                fire: sample.fire,
                actions: sample.actions.clone(),
            },
            self.round_id,
        );
        if self.raw(text, now_ms) {
            self.seq += 1;
            self.cadence.sent(&sample, now_ms);
            self.pending.clear();
            self.pending_weapon = None;
        }
    }
}

/// JavaScript truthiness for a JSON value.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}
