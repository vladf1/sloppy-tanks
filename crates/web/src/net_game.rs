//! `NetGame`: one multiplayer room page. It owns the transport-free
//! [`NetworkClient`] (connection, replication, interpolation, input cadence) and the
//! same presentation and renderer local play uses. The page supplies WebSockets,
//! storage, timers and the DOM; everything between a server frame and a drawn frame,
//! and between a key press and an `input` message, stays on this side.
//!
//! # Driving it
//!
//! ```text
//! const game = await NetGame.create(canvas, JSON.stringify(config));
//! game.connect(JSON.stringify(choice), performance.now());
//! pump();                                   // after every call below
//! // pump(): for (const a of JSON.parse(game.take_actions())) socket/storage work;
//! //         for (const n of JSON.parse(game.take_notices())) UI work;
//! setInterval(() => { game.poll(performance.now()); pump(); }, 250);
//! requestAnimationFrame(function loop(now) {
//!   const result = game.frame(now, packedInput); pump();
//!   if (result[EVENTS] > 0) audioAndUi(JSON.parse(game.drain_events()));
//!   if (result[HUD_DUE]) ui.update(JSON.parse(game.hud_json()));
//!   requestAnimationFrame(loop);
//! });
//! ```
//!
//! - `create(canvas, config)`: `{ server, room, savedSeat?: {token, roomEpoch}, latency?,
//!   jitter?, stall?, seed?, assetBase, cssWidth?, cssHeight?, pixelRatio?,
//!   firstPerson?, zoom?, hideReticle? }`. `server` is
//!   the validated WebSocket origin; `latency`/`jitter`/`stall` are the development
//!   transport-delay URL parameters (omit them in production).
//! - Actions (`take_actions`, in order): `{type: "open", socket, url}`, `{type: "send",
//!   socket, text}` (close the socket instead when its `bufferedAmount` exceeds 16 KiB),
//!   `{type: "close", socket}`, `{type: "saveSeat", token, roomEpoch}`, `{type:
//!   "forgetSeat"}`. Report socket events with `socket_opened`, `socket_message` (text
//!   frames), `socket_binary` (binary frames, as a `Uint8Array`; set the socket's
//!   `binaryType` to `"arraybuffer"`) and `socket_closed` (close code).
//! - Notices (`take_notices`): `status {text, connected}`, `notice {text}`, `ended {cause,
//!   text}`, `lobby {lobby, playerId}` (the wire lobby record), `result {match, team}`,
//!   `resetFeedback`, `clearInput`, `reveal`, `prepare` (run `prepare_step` between tasks
//!   until it reports done; while it reports `gpuPending`, wait on a short timer),
//!   `arenaFailed {error}` and `baselineShown` (the arena drew the first frames of a new
//!   baseline and takes input again).
//! - `frame(now, input) -> Float32Array` ([`net_frame_slot`]): input is the packed raw
//!   control state of `Game.frame` (`sloppy_render::presentation::input::slot`); command
//!   building, aiming and the input cadence run here.
//! - `drain_events()` and `hud_json()` have the shapes of `Game.drain_events()` and
//!   `Game.hud_json()` (`recap` is always null; the room lobby has the results).
//! - `stats_json(now)`: `Game.stats_json()`'s renderer rows, the displayed scene's counts
//!   and a `network` block (RTT, updates, playout buffer, late batches, ticks, input
//!   seq/ack).
//! - Intents: `choose`, `settings`, `start`, `pause`, `resume`, `end`, `rejoin`,
//!   `leave`, `select_ammo`, `set_menu`, `set_hidden`, `stop` (`pagehide`),
//!   `toggle_first_person`, `resize`.
//! - `camera_preferences() -> Float64Array [firstPerson, zoom]`: the chosen view
//!   and clamped overhead zoom, for the page to save after a camera input.

use crate::events::{PendingEvent, drain_events};

use glam::Vec2;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sloppy_core::net::client::{
    ClientAction, ClientConfig, ClientNotice, LocalInput, NetworkClient, SavedSeat,
};
use sloppy_core::net::client_setup::{pending_join, pending_join_text};
use sloppy_core::net::protocol::{
    CONTENT_VERSION, JoinChoice, RoomPhase, RoomSettings, is_room_code, read_player_kind, read_team,
};
use sloppy_core::net::transport_delay::DelaySettings;
use sloppy_core::sim::map_options::map_option_for;
use sloppy_core::sim::simulation::SpeedTuning;
use sloppy_core::sim::{GameMode, MatchPhase, RenderState, SimEventType, Weapon};
use sloppy_render::gpu::{Renderer, RendererOptions};
use sloppy_render::presentation::Presentation;
use sloppy_render::presentation::input::{CommandBuilder, InputFrame};
use sloppy_render::presentation::view_settings::CAMERA;
use wasm_bindgen::prelude::*;

use crate::hud::{HudHuman, Scoreboard, human_json, scoreboard_json};
use crate::stats::presentation_stats;

const MILLISECONDS_PER_SECOND: f64 = 1000.0;
const HUD_UPDATE_EVERY_FRAMES: u64 = 4;
/// Events waiting for `drain_events`; a hidden HUD must not grow this without bound.
const MAX_PENDING_EVENTS: usize = 2048;
/// Notices waiting for `take_notices`, for the same reason.
const MAX_PENDING_NOTICES: usize = 256;

/// Slots of `frame`'s result. The first eleven match `Game.frame`.
pub mod net_frame_slot {
    /// 0 lobby, 1 playing, 2 playing with the room menu open, 3 results.
    pub const PHASE: usize = 0;
    pub const HUMAN_ALIVE: usize = 1;
    /// Show the first-person cockpit HUD.
    pub const COCKPIT: usize = 2;
    /// The hull's clockwise screen angle with the turret straight up (compass).
    pub const HULL_ANGLE: usize = 3;
    /// Events waiting for `drain_events`.
    pub const EVENTS: usize = 4;
    /// 1 when local controls do not drive the tank (menu, dead, bot-driven, resyncing).
    pub const CLEAR_INPUT: usize = 5;
    /// 1 on frames the HUD should refresh (`hud_json`).
    pub const HUD_DUE: usize = 6;
    /// Interpolation, input and event routing, in milliseconds.
    pub const SIM_MS: usize = 7;
    pub const RENDER_MS: usize = 8;
    /// 1 while first person is enabled.
    pub const FIRST_PERSON: usize = 9;
    /// This frame's capped delta in seconds.
    pub const DT: usize = 10;
    /// 1 when the arena drew this frame.
    pub const DRAWN: usize = 11;
    /// 1 when the page should free a captured pointer: a menu, a drop or no battle.
    /// A death keeps it captured for the respawn.
    pub const POINTER_FREE: usize = 12;
    pub const LENGTH: usize = 13;
}

fn js_error(message: impl Into<String>) -> JsValue {
    js_sys::Error::new(&message.into()).into()
}

thread_local! {
    // Looked up once: each frame reads the clock several times.
    static PERFORMANCE: Option<web_sys::Performance> =
        web_sys::window().and_then(|window| window.performance());
}

fn now_ms() -> f64 {
    PERFORMANCE.with(|performance| performance.as_ref().map_or(0.0, |p| p.now()))
}

fn record(json: &str) -> Result<Map<String, Value>, JsValue> {
    match serde_json::from_str::<Value>(json) {
        Ok(Value::Object(fields)) => Ok(fields),
        Ok(_) => Err(js_error("Expected an object")),
        Err(error) => Err(js_error(error.to_string())),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeatConfig {
    token: String,
    room_epoch: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct NetConfig {
    server: String,
    room: String,
    saved_seat: Option<SeatConfig>,
    latency: Option<String>,
    jitter: Option<String>,
    stall: Option<String>,
    seed: Option<f64>,
    asset_base: String,
    css_width: Option<f64>,
    css_height: Option<f64>,
    pixel_ratio: Option<f64>,
    first_person: bool,
    zoom: Option<f64>,
    hide_reticle: bool,
}

impl NetConfig {
    /// A stored seat only counts when it looks like one the server issued.
    fn saved_seat(&self) -> Option<SavedSeat> {
        let seat = self.saved_seat.as_ref()?;
        let token_length = seat.token.encode_utf16().count();
        let epoch_length = seat.room_epoch.encode_utf16().count();
        ((16..=128).contains(&token_length) && (1..=128).contains(&epoch_length)).then(|| {
            SavedSeat {
                token: seat.token.clone(),
                room_epoch: seat.room_epoch.clone(),
            }
        })
    }

    fn delay(&self) -> Option<DelaySettings> {
        (self.latency.is_some() || self.jitter.is_some() || self.stall.is_some()).then(|| {
            DelaySettings::from_params(
                self.latency.as_deref(),
                self.jitter.as_deref(),
                self.stall.as_deref(),
            )
        })
    }
}

/// The arena being compiled for a round (`PrepareArena`).
struct PendingArena {
    round_id: u64,
    room_epoch: String,
    state: RenderState,
}

#[derive(Clone, Copy, Debug, Default)]
struct FrameTimes {
    frame_ms: f64,
    sim_ms: f64,
    render_ms: f64,
    average_ms: f64,
}

#[wasm_bindgen]
pub struct NetGame {
    client: NetworkClient,
    view: Presentation,
    commands: CommandBuilder,
    arena: Option<PendingArena>,
    /// A full state rebuilt the scene; its first frames draw on the next `frame`.
    baseline: Option<u32>,
    prepared: bool,
    events: Vec<PendingEvent>,
    notices: Vec<Value>,
    last_ms: Option<f64>,
    frame_index: u64,
    times: FrameTimes,
    client_size: Vec2,
    pixel_ratio: f64,
    exact: bool,
}

#[wasm_bindgen]
impl NetGame {
    /// Create the renderer on `canvas` and the room session (not yet connected).
    /// Rejects when WebGPU is unavailable or the configuration is invalid.
    pub async fn create(
        canvas: web_sys::HtmlCanvasElement,
        config_json: &str,
    ) -> Result<NetGame, JsValue> {
        console_error_panic_hook::set_once();
        let config: NetConfig =
            serde_json::from_str(config_json).map_err(|error| js_error(error.to_string()))?;
        if !is_room_code(&config.room) {
            return Err(js_error("Invalid room code"));
        }
        let seed = config
            .seed
            .unwrap_or_else(|| (js_sys::Math::random() * 1e9).floor());
        let client_size = Vec2::new(
            config.css_width.unwrap_or(f64::from(canvas.client_width())) as f32,
            config
                .css_height
                .unwrap_or(f64::from(canvas.client_height())) as f32,
        );
        let renderer = Renderer::new(
            canvas,
            RendererOptions {
                asset_base: config.asset_base.clone(),
            },
        )
        .await
        .map_err(js_error)?;
        let mut view = Presentation::new(renderer, seed.to_bits());
        view.rig
            .restore_preferences(config.first_person, config.zoom);
        view.hide_reticle = config.hide_reticle;
        let client = NetworkClient::new(ClientConfig {
            server_url: config.server.clone(),
            room: config.room.clone(),
            saved_seat: config.saved_seat(),
            delay: config.delay(),
            seed: seed as u32,
        });
        let mut game = NetGame {
            client,
            view,
            commands: CommandBuilder::default(),
            arena: None,
            baseline: None,
            prepared: false,
            events: Vec::new(),
            notices: Vec::new(),
            last_ms: None,
            frame_index: 0,
            times: FrameTimes::default(),
            client_size,
            pixel_ratio: config.pixel_ratio.unwrap_or(1.0),
            exact: false,
        };
        game.apply_size();
        Ok(game)
    }

    /// The content version this build joins with (`SLOPPY_CONTENT_VERSION`).
    pub fn content_version() -> String {
        CONTENT_VERSION.to_string()
    }

    /// The choices Battle Setup stored for `room` before reloading into it, as `JoinChoice`
    /// JSON, or undefined when the saved text is missing, for another room, or invalid.
    pub fn pending_join(saved: Option<String>, room: &str) -> Option<String> {
        let choice = pending_join(saved.as_deref(), room)?;
        let text = pending_join_text(room, &choice).ok()?;
        let value: Value = serde_json::from_str(&text).ok()?;
        Some(value.get("choice")?.to_string())
    }

    // ---------------------------------------------------------- connection

    /// Join the room with `choice_json` (`{ name, kind, team?, create?, existingRoom? }`).
    pub fn connect(&mut self, choice_json: &str, now: f64) -> Result<(), JsValue> {
        let choice = JoinChoice::read(&record(choice_json)?).map_err(js_error)?;
        self.client.connect(choice, now);
        Ok(())
    }

    /// Socket and storage work for the page, as a JSON array (see the module docs).
    pub fn take_actions(&mut self) -> String {
        let actions: Vec<Value> = self
            .client
            .take_actions()
            .into_iter()
            .map(|action| match action {
                ClientAction::OpenSocket { socket, url } => {
                    json!({ "type": "open", "socket": socket, "url": url })
                }
                ClientAction::Send { socket, text } => {
                    json!({ "type": "send", "socket": socket, "text": text })
                }
                ClientAction::CloseSocket { socket } => {
                    json!({ "type": "close", "socket": socket })
                }
                ClientAction::SaveSeat { token, room_epoch } => {
                    json!({ "type": "saveSeat", "token": token, "roomEpoch": room_epoch })
                }
                ClientAction::ForgetSeat => json!({ "type": "forgetSeat" }),
            })
            .collect();
        Value::Array(actions).to_string()
    }

    /// UI notices as a JSON array (see the module docs). Arena preparation and baseline
    /// requests are handled here and surface as `prepare` and `baselineShown`.
    pub fn take_notices(&mut self) -> String {
        self.drain_client_notices();
        Value::Array(std::mem::take(&mut self.notices)).to_string()
    }

    pub fn socket_opened(&mut self, socket: u32, now: f64) {
        self.client.socket_opened(socket, now);
    }

    pub fn socket_message(&mut self, socket: u32, text: &str, now: f64) {
        self.client.socket_message(socket, text, now);
    }

    /// A binary frame (a baseline or snapshot batch), copied out of the page's
    /// `ArrayBuffer` in one call.
    pub fn socket_binary(&mut self, socket: u32, bytes: &[u8], now: f64) {
        self.client.socket_binary(socket, bytes, now);
    }

    pub fn socket_closed(&mut self, socket: u32, code: u16, now: f64) {
        self.client.socket_closed(socket, code, now);
    }

    /// Heartbeat, reconnect backoff and development delay; at least every 250 ms.
    pub fn poll(&mut self, now: f64) {
        self.client.poll(now);
    }

    /// Stop retrying and close the socket (`pagehide`).
    pub fn stop(&mut self) {
        self.client.stop();
    }

    // ---------------------------------------------------------- room intents

    /// Change team (`team` < 0: auto) or tank between battles.
    pub fn choose(&mut self, team: i32, kind: &str, now: f64) -> Result<(), JsValue> {
        let kind = read_player_kind(Some(&Value::from(kind))).map_err(js_error)?;
        let team = if team < 0 {
            None
        } else {
            Some(read_team(Some(&Value::from(team))).map_err(js_error)?)
        };
        self.client.choose(team, kind, now);
        Ok(())
    }

    /// The host's rules: `{ mapMode, difficulty, humansOnly, roundMinutes }`.
    pub fn settings(&mut self, settings_json: &str, now: f64) -> Result<(), JsValue> {
        let settings = RoomSettings::read(&record(settings_json)?).map_err(js_error)?;
        self.client.settings(settings, now);
        Ok(())
    }

    pub fn start(&mut self, now: f64) {
        self.commands.clear();
        self.client.start(now);
    }

    /// Open the in-battle menu; returns whether it is open.
    pub fn pause(&mut self, now: f64) -> bool {
        self.client.pause(now);
        if self.client.menu_open() {
            self.commands.clear();
        }
        self.client.menu_open()
    }

    /// Close the menu and ask for the tank back; returns whether the menu is still open
    /// (it stays open while disconnected).
    pub fn resume(&mut self, now: f64) -> bool {
        self.commands.clear();
        self.client.resume(now);
        self.client.menu_open()
    }

    pub fn set_menu(&mut self, open: bool) {
        self.client.set_menu(open);
    }

    /// Development transport delay from the page's latency slider: round trip, jitter
    /// and stall in milliseconds, clamped like the `latency`/`jitter`/`stall` parameters.
    pub fn set_transport_delay(&mut self, latency: f64, jitter: f64, stall: f64) {
        let [latency, jitter, stall] = [latency, jitter, stall].map(|value| value.to_string());
        self.client.set_delay(DelaySettings::from_params(
            Some(&latency),
            Some(&jitter),
            Some(&stall),
        ));
    }

    pub fn end(&mut self, now: f64) {
        self.commands.clear();
        self.client.end(now);
    }

    pub fn rejoin(&mut self, now: f64) {
        self.client.rejoin(now);
    }

    pub fn leave(&mut self, now: f64) {
        self.client.leave(now);
    }

    /// An ammo button in the HUD (`"standard"`, `"spread"`, …).
    pub fn select_ammo(&mut self, weapon: &str) {
        if let Ok(weapon) = serde_json::from_value::<Weapon>(Value::from(weapon)) {
            self.client.select_ammo(weapon);
        }
    }

    pub fn set_hidden(&mut self, hidden: bool, now: f64) {
        self.commands.clear();
        self.client.set_hidden(hidden, now);
    }

    /// Enter or leave first person during a battle; returns whether it is on.
    pub fn toggle_first_person(&mut self) -> bool {
        if self.client.phase() == RoomPhase::Playing
            && !self.client.menu_open()
            && let Some(viewer) = self.client.display().and_then(RenderState::viewer)
        {
            self.view.rig.first_person.toggle(viewer.aim);
        }
        self.view.rig.first_person.enabled
    }

    pub fn camera_preferences(&self) -> Vec<f64> {
        self.view.rig.preferences().to_vec()
    }

    pub fn resize(&mut self, css_width: f64, css_height: f64, pixel_ratio: f64, exact: bool) {
        self.client_size = Vec2::new(css_width as f32, css_height as f32);
        self.pixel_ratio = pixel_ratio;
        self.exact = exact;
        self.apply_size();
    }

    // ---------------------------------------------------------- queries

    /// Whether local controls drive the tank now.
    pub fn active_input(&self) -> bool {
        self.client.active_input()
    }

    pub fn connected(&self) -> bool {
        self.client.connected
    }

    pub fn menu_open(&self) -> bool {
        self.client.menu_open()
    }

    /// `"lobby"`, `"playing"` or `"results"`.
    pub fn phase(&self) -> String {
        self.client.phase().as_str().to_string()
    }

    pub fn player_id(&self) -> String {
        self.client.player_id.clone()
    }

    pub fn error(&self) -> Option<String> {
        self.view.renderer.error()
    }

    // ---------------------------------------------------------- arena

    /// Create up to `budget` background-compiled pipelines for the arena `prepare` asked
    /// for; `[compiled, remaining, texturesPending, done, gpuPending]`. Once compiled it
    /// warms every variant and waits for the GPU to run that work (`gpuPending`, also set
    /// while only background compiles remain: poll again after a short timer rather than
    /// a task); when done it draws the first frames and
    /// tells the room the arena is ready. The seat stays suspended until then, so a
    /// shader compile that takes seconds never makes the room drop this connection.
    pub fn prepare_step(&mut self, budget: u32, now: f64) -> Vec<f64> {
        if self.arena.is_none() {
            return vec![0.0, 0.0, 0.0, 1.0, 0.0];
        }
        let status = match self.view.prepare_step(budget.max(1)) {
            Ok(status) => status,
            Err(error) => {
                self.arena_failed(error);
                return vec![0.0, 0.0, 0.0, 1.0, 0.0];
            }
        };
        if status.ready
            && let Some(arena) = self.arena.take()
        {
            match self.view.finish_prepare(&arena.state) {
                Ok(()) => {
                    self.prepared = true;
                    self.client
                        .arena_prepared(arena.round_id, &arena.room_epoch, now);
                }
                Err(error) => self.arena_failed(error),
            }
        }
        let flag = |value: bool| if value { 1.0 } else { 0.0 };
        vec![
            f64::from(status.compiled),
            f64::from(status.remaining),
            f64::from(status.textures_pending),
            flag(status.ready),
            flag(status.gpu_pending),
        ]
    }

    /// A generated texture to bake off the main thread (`bake_texture(key)` in a
    /// worker), handed out once; `supply_texture` returns the pixels.
    pub fn claim_texture_bake(&mut self) -> Option<String> {
        self.view.claim_texture_bake().map(str::to_owned)
    }

    /// The pixels of a claimed bake; `false` when the arena no longer waits for them.
    /// They reach the GPU from JS memory, never copied into this instance's heap.
    pub fn supply_texture(
        &mut self,
        key: &str,
        rgba: &js_sys::Uint8Array,
    ) -> Result<bool, JsValue> {
        self.view.supply_texture(key, rgba).map_err(js_error)
    }

    /// Bake a claimed texture on the main thread after all (the worker failed).
    pub fn release_texture_bake(&mut self, key: &str) {
        self.view.release_texture_bake(key);
    }

    // ---------------------------------------------------------- frame

    /// One animation frame: advance the interpolated display, send input due now, route
    /// events to presentation and draw. See [`net_frame_slot`] for the result.
    pub fn frame(&mut self, now: f64, input: &[f32]) -> Result<Vec<f32>, JsValue> {
        let input = InputFrame::from_slice(input);
        let last = self.last_ms.unwrap_or(now);
        let raw = ((now - last) / MILLISECONDS_PER_SECOND).max(0.0);
        let dt = raw.min(0.1);
        self.last_ms = Some(last.max(now));
        self.times.frame_ms = raw * MILLISECONDS_PER_SECOND;
        if raw > 0.0 {
            self.times.average_ms += (self.times.frame_ms - self.times.average_ms) * 0.05;
        }
        // The development delay delivers messages on this clock too.
        self.client.poll(now);
        self.drain_client_notices();
        self.view.rig.zoom_by(input.zoom);
        if input.toggle_view {
            self.toggle_first_person();
        }
        if let Some(baseline) = self.baseline.take() {
            self.show_baseline(baseline, now)?;
        }
        let update_start = now_ms();
        let active = self.client.active_input();
        self.commands.queue(&input, active, now);
        let local = if active {
            self.local_input(&input, dt)
        } else {
            self.commands.clear();
            LocalInput::default()
        };
        let mut render_ms = 0.0;
        let Self {
            client,
            view,
            events,
            ..
        } = self;
        let drawn = match client.frame(now, &local) {
            Some(frame) => {
                for displayed in frame.events {
                    view.event(&displayed.event, displayed.player_hit);
                    let hurt = matches!(
                        displayed.event.kind,
                        SimEventType::Hurt | SimEventType::Death
                    );
                    let damage_angle = (displayed.own && hurt)
                        .then(|| view.damage_angle(&displayed.event))
                        .flatten();
                    if events.len() >= MAX_PENDING_EVENTS {
                        events.remove(0);
                    }
                    events.push(PendingEvent {
                        event: displayed.event,
                        player_hit: displayed.player_hit,
                        own: displayed.own,
                        damage_angle,
                    });
                }
                let render_start = now_ms();
                view.render(frame.state, 1.0, frame.dt, false)
                    .map_err(js_error)?;
                render_ms = now_ms() - render_start;
                true
            }
            None => false,
        };
        self.times.sim_ms = now_ms() - update_start - render_ms;
        self.times.render_ms = render_ms;
        Ok(self.frame_result(drawn, dt))
    }

    /// Displayed events for audio and HUD, as JSON (`Game.drain_events()`'s shape).
    pub fn drain_events(&mut self) -> String {
        let (x, z) =
            self.client
                .display()
                .and_then(RenderState::viewer)
                .map_or((0.0, 0.0), |viewer| {
                    if viewer.alive {
                        (viewer.position.x, viewer.position.z)
                    } else {
                        (viewer.previous.x, viewer.previous.z)
                    }
                });
        drain_events(&mut self.events, (x, z), self.view.rig.listener_right)
    }

    /// The HUD for the displayed scene, in `Game.hud_json()`'s shape, or `null` before
    /// the first scene. Map and difficulty come from the room's rules.
    pub fn hud_json(&self) -> String {
        let Some(state) = self.client.display() else {
            return "null".into();
        };
        let Some(viewer) = state.viewer() else {
            return "null".into();
        };
        let settings = self.client.lobby().map(|lobby| lobby.settings);
        let map = settings.map(|settings| settings.map_mode);
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Hud<'a> {
            #[serde(rename = "match")]
            match_state: &'a sloppy_core::sim::Match,
            elapsed: f64,
            game_mode: GameMode,
            endless_match: bool,
            map_mode: Option<&'a str>,
            map_name: Option<&'a str>,
            difficulty: Option<&'a str>,
            human_team: sloppy_core::sim::Team,
            active_enemies: usize,
            speed_tuning: SpeedTuning,
            human: HudHuman<'a>,
            scoreboard: Scoreboard<'a>,
            recap: (),
        }
        serde_json::to_string(&Hud {
            match_state: &state.match_state,
            elapsed: state.elapsed,
            game_mode: GameMode::Team,
            endless_match: false,
            map_mode: map.map(|map| map.as_str()),
            map_name: map.map(|map| map_option_for(map).name),
            difficulty: settings.map(|settings| settings.difficulty.as_str()),
            human_team: viewer.team,
            active_enemies: state
                .tanks
                .iter()
                .filter(|tank| tank.team != viewer.team && tank.alive)
                .count(),
            speed_tuning: SpeedTuning::default(),
            human: human_json(viewer, state.elapsed),
            scoreboard: scoreboard_json(&state.tanks),
            recap: (),
        })
        .expect("room HUD serializes")
    }

    /// Stats for nerds: renderer rows, the displayed scene and the network.
    pub fn stats_json(&self, now: f64) -> String {
        let mut stats = presentation_stats(&self.view);
        stats.insert("frameMs".into(), self.times.frame_ms.into());
        stats.insert("averageFrameMs".into(), self.times.average_ms.into());
        let fps = if self.times.average_ms > 0.0 {
            1000.0 / self.times.average_ms
        } else {
            0.0
        };
        stats.insert("fps".into(), fps.into());
        stats.insert("simMs".into(), self.times.sim_ms.into());
        stats.insert("renderMs".into(), self.times.render_ms.into());
        if let Some(state) = self.client.display() {
            stats.insert(
                "scene".into(),
                json!({
                    "tanks": state.tanks.len(),
                    "alive": state.tanks.iter().filter(|tank| tank.alive).count(),
                    "mines": state.mines.len(),
                    "pickups": state.pickups.len(),
                    "pickupsReady": state.pickups.iter().filter(|pickup| pickup.available).count(),
                    "shots": state.shots.len(),
                    "fragments": state.fragments.len(),
                    "elapsed": state.elapsed,
                }),
            );
        }
        let network = self.client.stats(now);
        stats.insert(
            "network".into(),
            json!({
                "rttMs": network.rtt_ms,
                "receivedUpdates": network.received_updates,
                "snapshotAgeMs": network.snapshot_age_ms,
                "bufferMs": network.buffer_ms,
                "marginMs": network.margin_ms,
                "underrun": network.underrun,
                "serverTick": network.server_tick,
                "inputSeq": network.input_seq,
                "inputAck": network.input_ack,
                "connected": network.connected,
                "lateBatches": network.late_batches,
                "longestBatchGapMs": network.longest_batch_gap_ms,
                "predictionLeadMs": network.prediction_lead_ms,
                "corrections": network.corrections,
                "correctionTotalM": network.correction_total_m,
                "correctionMaxM": network.correction_max_m,
                "correctionMPerS": network.correction_m_per_s,
                "correctionP95M": network.correction_p95_m,
                "inputStarts": network.input_starts,
                "lateInputStarts": network.late_input_starts,
            }),
        );
        Value::Object(stats).to_string()
    }

    /// What the browser checks read from `window.sloppyMultiplayer`.
    pub fn debug_json(&self) -> String {
        let client = &self.client;
        let control = client.control().map(|control| {
            json!({
                "roomEpoch": control.room_epoch,
                "roundId": control.round_id,
                "tankId": control.tank_id,
                "life": control.life,
                "controlEpoch": control.control_epoch,
                "driver": sloppy_core::net::protocol::driver_name(control.driver),
            })
        });
        let display = client.display().map(|state| {
            let tanks: Vec<Value> = state
                .tanks
                .iter()
                .map(|tank| {
                    json!({
                        "id": tank.id, "name": tank.name, "team": tank.team, "kind": tank.kind,
                        "human": tank.human, "alive": tank.alive, "life": tank.life,
                        "hp": tank.hp, "aim": tank.aim, "heading": tank.heading,
                        "position": tank.position,
                    })
                })
                .collect();
            json!({
                "viewerId": state.viewer_id,
                "mapTheme": state.map_theme,
                "match": state.match_state,
                "elapsed": state.elapsed,
                "viewer": state.viewer(),
                "tanks": tanks,
                "shots": state.shots.len(),
                "mines": state.mines.len(),
                "fragments": state.fragments.len(),
            })
        });
        let rig = &self.view.rig;
        json!({
            "connection": {
                "connected": client.connected,
                "stopped": client.stopped,
                "roomEpoch": client.room_epoch,
                "roundId": client.round_id,
                "playerId": client.player_id,
                "rtt": client.rtt_ms,
            },
            "tick": client.mirror().tick,
            "phase": client.phase().as_str(),
            "menu": client.menu_open(),
            "activeInput": client.active_input(),
            "prepared": self.prepared,
            "control": control,
            "display": display,
            "view": {
                "zoom": rig.zoom,
                "firstPerson": rig.first_person.enabled,
                "yaw": rig.first_person.yaw,
                "canvas": self.view.renderer.size(),
            },
        })
        .to_string()
    }
}

impl NetGame {
    fn apply_size(&mut self) {
        let ratio = if self.exact {
            1.0
        } else {
            self.pixel_ratio.min(CAMERA.max_pixel_ratio)
        };
        let width = (f64::from(self.client_size.x) * ratio).round().max(1.0) as u32;
        let height = (f64::from(self.client_size.y) * ratio).round().max(1.0) as u32;
        self.view.resize(width, height);
    }

    fn notice(&mut self, notice: Value) {
        if self.notices.len() >= MAX_PENDING_NOTICES {
            self.notices.remove(0);
        }
        self.notices.push(notice);
    }

    /// Turn client notices into page notices; arena and baseline requests go to the
    /// presentation here.
    fn drain_client_notices(&mut self) {
        for notice in self.client.take_notices() {
            let value = match notice {
                ClientNotice::Status { text, connected } => {
                    json!({ "type": "status", "text": text, "connected": connected })
                }
                ClientNotice::Notice(text) => json!({ "type": "notice", "text": text }),
                ClientNotice::Ended { cause, text } => {
                    json!({ "type": "ended", "cause": cause.as_str(), "text": text })
                }
                ClientNotice::Lobby(lobby) => {
                    let lobby: Value = serde_json::from_str(&lobby.to_json()).unwrap_or_default();
                    json!({ "type": "lobby", "lobby": lobby, "playerId": self.client.player_id })
                }
                ClientNotice::Result { outcome, team } => {
                    json!({ "type": "result", "match": outcome, "team": team })
                }
                ClientNotice::ResetFeedback => json!({ "type": "resetFeedback" }),
                ClientNotice::ClearInput => {
                    self.commands.clear();
                    json!({ "type": "clearInput" })
                }
                ClientNotice::Reveal => json!({ "type": "reveal" }),
                ClientNotice::PrepareArena {
                    round_id,
                    room_epoch,
                } => {
                    self.begin_arena(round_id, room_epoch);
                    continue;
                }
                ClientNotice::ShowBaseline { baseline } => {
                    self.baseline = Some(baseline);
                    continue;
                }
            };
            self.notice(value);
        }
    }

    /// Build the round's arena from the replicated scene and start compiling it.
    fn begin_arena(&mut self, round_id: u64, room_epoch: String) {
        let Some(state) = self.client.arena_state() else {
            self.arena_failed("The room sent no arena to prepare.".into());
            return;
        };
        self.prepared = false;
        self.baseline = None;
        self.view.build_scenery(&state.map_theme);
        self.view.reset(&state);
        self.view.begin_prepare(&state);
        self.arena = Some(PendingArena {
            round_id,
            room_epoch,
            state,
        });
        self.notice(json!({ "type": "prepare" }));
    }

    fn arena_failed(&mut self, error: String) {
        self.arena = None;
        self.client.arena_failed();
        self.notice(json!({ "type": "arenaFailed", "error": error }));
    }

    /// A full state makes presentation rebuild every model. Draw the first frames before
    /// the arena takes input again, as the TypeScript `showBaseline` did.
    fn show_baseline(&mut self, baseline: u32, now: f64) -> Result<(), JsValue> {
        let Self { client, view, .. } = self;
        let Some(display) = client.display() else {
            return Ok(());
        };
        view.build_scenery(&display.map_theme);
        view.reset(display);
        for _ in 0..2 {
            view.render(display, 1.0, 0.0, false).map_err(js_error)?;
        }
        self.events.clear();
        self.client.baseline_shown(baseline, now);
        self.drain_client_notices();
        self.notice(json!({ "type": "baselineShown" }));
        Ok(())
    }

    /// The controls for this frame: movement relative to first person, the aim from the
    /// pointer, touch stick or first-person yaw, and queued one-shot actions.
    fn local_input(&mut self, input: &InputFrame, dt: f64) -> LocalInput {
        let Some(viewer) = self.client.display().and_then(RenderState::viewer) else {
            return LocalInput::default();
        };
        let (x, z) = (viewer.position.x, viewer.position.z);
        let rig = &mut self.view.rig;
        let (angle, aim_point) = if rig.first_person.enabled {
            let stick = if input.aim_stick_held {
                f64::from(input.touch_aim)
            } else {
                0.0
            };
            rig.first_person.turn(input.look_pixels, stick, dt);
            (rig.first_person.yaw, None)
        } else {
            let target = rig.aim(Vec2::new(input.pointer.0, input.pointer.1));
            let (tx, tz) = (f64::from(target.x), f64::from(target.z));
            ((tx - x).atan2(tz - z), Some((tx, tz)))
        };
        let command = self.commands.command(input, angle, true);
        let command = self.view.rig.first_person.steer(command);
        LocalInput {
            move_x: command.move_x,
            move_z: command.move_z,
            fire: command.fire,
            mine: command.mine,
            ammo_selection: command.ammo_selection,
            aim_angle: angle,
            aim_point,
        }
    }

    fn frame_result(&mut self, drawn: bool, dt: f64) -> Vec<f32> {
        use net_frame_slot as slot;
        let mut result = vec![0.0; slot::LENGTH];
        let phase = self.client.phase();
        let menu = self.client.menu_open();
        let flag = |value: bool| f32::from(u8::from(value));
        result[slot::PHASE] = match phase {
            RoomPhase::Lobby => 0.0,
            RoomPhase::Playing if menu => 2.0,
            RoomPhase::Playing => 1.0,
            RoomPhase::Results => 3.0,
        };
        let viewer = self.client.display().and_then(RenderState::viewer);
        let playing = phase == RoomPhase::Playing
            && self
                .client
                .display()
                .is_some_and(|state| state.match_state.phase != MatchPhase::Ready);
        result[slot::HUMAN_ALIVE] = flag(viewer.is_some_and(|viewer| viewer.alive));
        result[slot::COCKPIT] = flag(playing && self.view.rig.seat_wanted);
        result[slot::HULL_ANGLE] = viewer.map_or(0.0, |viewer| {
            self.view.rig.first_person.screen_angle(viewer.heading) as f32
        });
        result[slot::EVENTS] = self.events.len() as f32;
        result[slot::CLEAR_INPUT] = flag(!self.client.active_input());
        result[slot::HUD_DUE] = flag(
            drawn
                && (self.frame_index.is_multiple_of(HUD_UPDATE_EVERY_FRAMES)
                    || !self.events.is_empty()),
        );
        result[slot::SIM_MS] = self.times.sim_ms as f32;
        result[slot::RENDER_MS] = self.times.render_ms as f32;
        result[slot::FIRST_PERSON] = flag(self.view.rig.first_person.enabled);
        result[slot::DT] = dt as f32;
        result[slot::DRAWN] = flag(drawn);
        result[slot::POINTER_FREE] =
            flag(!drawn || !self.client.connected || menu || phase != RoomPhase::Playing);
        self.frame_index += 1;
        result
    }
}
