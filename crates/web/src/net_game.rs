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
//!   firstPerson?, zoom?, hideReticle?, framesInFlight? }` (the view's part is
//!   `page::create_view`'s). `server` is
//!   the validated WebSocket origin; `latency`/`jitter`/`stall` are the development
//!   transport-delay URL parameters (omit them in production); `dodge: "0"` draws
//!   incoming shells on the display clock alone and `shooter: "0"` asks the host to
//!   favor no shooter (development comparisons).
//! - Actions (`take_actions`, in order): `{type: "open", socket, url}`, `{type: "send",
//!   socket, text}` (close the socket instead when its `bufferedAmount` exceeds 16 KiB),
//!   `{type: "close", socket}`, `{type: "saveSeat", token, roomEpoch}`, `{type:
//!   "forgetSeat"}`. Report socket events with `socket_opened`, `socket_message` (text
//!   frames), `socket_binary` (binary frames, as a `Uint8Array`; set the socket's
//!   `binaryType` to `"arraybuffer"`) and `socket_closed` (close code).
//! - Notices (`take_notices`): `status {text, connected, title?, message?}` (the
//!   connection dialog's words while not connected), `notice {text}`, `ended {text,
//!   title, retry, setup}` (the dialog's title and buttons, `retry` null where joining
//!   again cannot work; also after `renderer_failed`), `lobby {lobby, playerId, menu:
//!   {title, hint, summary}, arrivals: [{text}]}` (the wire lobby record, the room
//!   menu's words for it and the feed lines of players who joined or came back),
//!   `result {match}` (the finished round's final score),
//!   `resetFeedback`, `clearInput`, `reveal`, `prepare` (run `prepare_step` between tasks
//!   until it reports done; while it reports `gpuPending`, wait on a short timer),
//!   `arenaFailed {error}` and `baselineShown` (the arena drew the first frames of a new
//!   baseline and takes input again).
//! - `frame(now, input) -> Float32Array` ([`net_frame_slot`]): input is the packed raw
//!   control state of `Game.frame` (`sloppy_render::presentation::input::slot`); command
//!   building, aiming and the input cadence run here.
//! - `drain_events()` has the shape of `Game.drain_events()`; `hud_json()` carries
//!   `Game.hud_json()`'s `match`, `human`, `scoreboard` and `scorePill` (the room lobby
//!   has the results).
//! - `stats_json(now)`: `Game.stats_json()`'s renderer and frame readings, the
//!   displayed scene's counts, a `network` block (RTT, updates, playout buffer, late
//!   batches, ticks, input seq/ack) and a `feel` block, for diagnostics and the
//!   latency matrix.
//! - `nerd_stats_json(now)`: the Stats for nerds panel's sections, as
//!   `Game.nerd_stats_json()`; the update rate is measured between calls.
//! - Intents: `choose`, `settings`, `start`, `pause`, `resume`, `end`, `rejoin`,
//!   `leave`, `select_ammo`, `set_hidden`, `stop` (`pagehide`), `renderer_failed`,
//!   `toggle_first_person`, `resize`.
//! - `camera_preferences() -> Float64Array [firstPerson, zoom]`: the chosen view
//!   and clamped overhead zoom, for the page to save after a camera input.
//!
//! A Rust shell (the desktop app) passes and reads the same values typed:
//! `create_with`, `join` ([`JoinChoice`]), `choose_seat`, `set_rules` ([`RoomSettings`]),
//! `take_room_actions` ([`ClientAction`]), `take_room_notices` ([`RoomNotice`]),
//! `prepare` ([`PrepareStatus`]), `hud` ([`RoomHud`]), `take_events` ([`EventBatch`]),
//! `camera` ([`CameraPreferences`]), `nerd_stats` ([`StatsSection`]), `gpu_ms` and
//! `set_graphics_setting` (the setting itself).

use crate::Shell;
use crate::events::{EventBatch, PendingEvent, batch_json, take_events};
use crate::feedback::event_feedback;
use crate::net_frame_slot;
use crate::sounds::SoundPlanner;

use glam::Vec2;
use serde::Deserialize;
use serde_json::{Value, json};
use sloppy_core::net::client::{
    ClientAction, ClientConfig, ClientNotice, LocalInput, NetworkClient, SavedSeat,
};
use sloppy_core::net::client_setup::pending_join;
use sloppy_core::net::feel_readings::Samples;
use sloppy_core::net::hull_prediction::CorrectionCause;
use sloppy_core::net::protocol::{
    JoinChoice, RoomPhase, RoomSettings, is_room_code, read_player_kind, read_team,
};
use sloppy_core::net::schema::{parse_record, text_length};
use sloppy_core::net::transport_delay::DelaySettings;
use sloppy_core::sim::{MatchPhase, RenderState, Team, VehicleKind, Weapon};
use sloppy_render::presentation::input::{CommandBuilder, InputFrame};
use sloppy_render::presentation::{PrepareStatus, Presentation};
use sloppy_render::quality::GraphicsSetting;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use crate::frame_clock::{FramePacing, FrameTimes};
use crate::hud::{EffectTimers, RoomHud, Scoreboard, room_hud};
use crate::nerd_rows::{
    CauseStats, ConnectionStats, FeelStats, NetworkSampler, OwnShotStats, RoomStats, SceneStats,
    StatsSection, Summary, room_sections,
};
use crate::page::{
    CameraPreferences, CanvasSize, Graphics, HUD_UPDATE_EVERY_FRAMES, PREPARED, PageError,
    PageView, ViewConfig, aim, create_view, js_error, now_ms, parse, prepare_result, queue_event,
};
use crate::room_notices::RoomNotice;
use crate::room_text::RoomText;
use crate::stats::{frame_readings, presentation_stats, renderer_readings};
use sloppy_render::gpu::RenderTarget;

/// Notices waiting for `take_notices`; a hidden HUD must not grow this without bound.
const MAX_PENDING_NOTICES: usize = 256;

/// `NetGame.create`'s room settings besides the view configuration
/// (`page::ViewConfig`).
#[derive(Clone, Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct NetConfig {
    /// The server's validated WebSocket origin.
    pub server: String,
    pub room: String,
    /// A seat this player held in the room before a reload.
    pub saved_seat: Option<SavedSeat>,
    /// The development transport delay parameters; omitted in production.
    pub latency: Option<String>,
    pub jitter: Option<String>,
    pub stall: Option<String>,
    /// Development switch: `"0"` draws incoming shells on the display clock alone.
    pub dodge: Option<String>,
    /// Development switch: `"0"` sends no view delay, so the host favors no shooter.
    pub shooter: Option<String>,
}

impl NetConfig {
    /// A stored seat only counts when it looks like one the server issued.
    fn saved_seat(&self) -> Option<SavedSeat> {
        self.saved_seat.clone().filter(|seat| {
            (16..=128).contains(&text_length(&seat.token))
                && (1..=128).contains(&text_length(&seat.room_epoch))
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct NetGame {
    /// The shell drawing this game, for the wording the page and the desktop app say
    /// differently.
    shell: Shell,
    client: NetworkClient,
    view: Presentation,
    commands: CommandBuilder,
    arena: Option<PendingArena>,
    /// A full state rebuilt the scene; its first frames draw on the next `frame`.
    baseline: Option<u32>,
    prepared: bool,
    events: Vec<PendingEvent>,
    /// The sounds drained events play, on the frame clock.
    sounds: SoundPlanner,
    notices: Vec<RoomNotice>,
    /// What the room menu said so far, for the words of the next notices.
    room_text: RoomText,
    /// How long the viewer's effects have run, which their timer rings drain from.
    effect_timers: EffectTimers,
    frame_index: u64,
    times: FrameTimes,
    pacing: FramePacing,
    canvas: CanvasSize,
    graphics: Graphics,
    /// The last frame's result, which a frame skipped for the GPU hands back.
    last_result: Vec<f32>,
    /// Main-thread milliseconds spent applying received messages (including deliveries of
    /// the development delay) this page session, for the statistics.
    receive_ms: f64,
    /// The previous Stats for nerds reading, which the update rate is measured from.
    network_sampler: NetworkSampler,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl NetGame {
    /// Create the renderer on `canvas` and the room session (not yet connected).
    /// Rejects when the renderer cannot start or the configuration is invalid.
    pub async fn create(canvas: RenderTarget, config_json: &str) -> Result<NetGame, PageError> {
        #[cfg(target_arch = "wasm32")]
        console_error_panic_hook::set_once();
        Self::create_with(canvas, parse(config_json)?, parse(config_json)?).await
    }

    /// The choices Battle Setup stored for `room` before reloading into it, as `JoinChoice`
    /// JSON, or undefined when the saved text is missing, for another room, or invalid.
    pub fn pending_join(saved: Option<String>, room: &str) -> Option<String> {
        pending_join(saved.as_deref(), room).map(|choice| choice.to_json())
    }

    // ---------------------------------------------------------- connection

    /// Join the room with `choice_json` (`{ name, kind, team?, create?, existingRoom? }`).
    pub fn connect(&mut self, choice_json: &str, now: f64) -> Result<(), PageError> {
        let choice =
            JoinChoice::read(&parse_record(choice_json).map_err(js_error)?).map_err(js_error)?;
        self.join(choice, now);
        Ok(())
    }

    /// Socket and storage work for the page, as a JSON array (see the module docs).
    pub fn take_actions(&mut self) -> String {
        serde_json::to_string(&self.take_room_actions()).expect("client actions serialize")
    }

    /// UI notices as a JSON array (see the module docs). Arena preparation and baseline
    /// requests are handled here and surface as `prepare` and `baselineShown`.
    pub fn take_notices(&mut self) -> String {
        let notices = self.take_room_notices();
        Value::Array(notices.iter().map(RoomNotice::to_value).collect()).to_string()
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
        let start = now_ms();
        self.client.socket_binary(socket, bytes, now);
        self.receive_ms += now_ms() - start;
    }

    pub fn socket_closed(&mut self, socket: u32, code: u16, now: f64) {
        self.client.socket_closed(socket, code, now);
    }

    /// Heartbeat, reconnect backoff and development delay; at least every 250 ms.
    pub fn poll(&mut self, now: f64) {
        let start = now_ms();
        self.client.poll(now);
        self.receive_ms += now_ms() - start;
    }

    /// Stop retrying and close the socket (`pagehide`).
    pub fn stop(&mut self) {
        self.client.stop();
    }

    /// The shell's renderer failed (a GPU error or a lost device): give up the
    /// connection, and end the room with an `ended` notice that says so.
    pub fn renderer_failed(&mut self) {
        self.client.stop();
        self.drain_client_notices();
        let ending = self.room_text.renderer_stopped(self.shell);
        self.notice(RoomNotice::Ended(ending));
    }

    // ---------------------------------------------------------- room intents

    /// Change team (`team` < 0: auto) or tank between battles.
    pub fn choose(&mut self, team: i32, kind: &str, now: f64) -> Result<(), PageError> {
        let kind = read_player_kind(Some(&Value::from(kind))).map_err(js_error)?;
        let team = if team < 0 {
            None
        } else {
            Some(read_team(Some(&Value::from(team))).map_err(js_error)?)
        };
        self.choose_seat(team, kind, now)
    }

    /// The host's rules: `{ mapMode, difficulty, humansOnly, roundMinutes }`.
    pub fn settings(&mut self, settings_json: &str, now: f64) -> Result<(), PageError> {
        let settings = RoomSettings::read(&parse_record(settings_json).map_err(js_error)?)
            .map_err(js_error)?;
        self.set_rules(settings, now);
        Ok(())
    }

    pub fn start(&mut self, now: f64) {
        self.commands.clear();
        self.client.start(now);
        self.graphics.round_started();
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

    /// End the battle as host. Its results then say it ended, since this seat has the
    /// menu open and gets no final state.
    pub fn end(&mut self, now: f64) {
        // Notices already received keep the words of before.
        self.drain_client_notices();
        self.room_text.ending_round();
        self.commands.clear();
        self.client.end(now);
    }

    /// Join the stopped room again; the connection dialog reads RECONNECTING.
    pub fn rejoin(&mut self, now: f64) {
        self.drain_client_notices();
        self.room_text.rejoining();
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

    pub fn resize(&mut self, css_width: f64, css_height: f64, pixel_ratio: f64) {
        self.canvas = CanvasSize {
            css: Vec2::new(css_width as f32, css_height as f32),
            device_pixel_ratio: pixel_ratio,
            exact: false,
        };
        self.canvas.apply(&mut self.view);
        self.graphics
            .set_pixel_ratio(&mut self.view.renderer, self.canvas.pixel_ratio());
    }

    /// The Settings dialog's Graphics choice: "auto", "high", "medium" or "low".
    #[cfg(target_arch = "wasm32")]
    pub fn set_graphics(&mut self, setting: &str) {
        self.set_graphics_setting(GraphicsSetting::parse(setting));
    }

    // ---------------------------------------------------------- queries

    /// Whether local controls drive the tank now.
    pub fn active_input(&self) -> bool {
        self.client.active_input()
    }

    pub fn connected(&self) -> bool {
        self.client.connected
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
        prepare_result(&self.prepare(budget, now))
    }

    /// Build one of the models preparation left for after the arena is ready (the
    /// library's wrecks); false once none remain. The page calls it between tasks.
    pub fn prebuild_step(&mut self) -> bool {
        self.view.prebuild_step()
    }

    /// A generated texture to bake off the main thread (`bake_texture(key)` in a
    /// worker), handed out once; `supply_texture` returns the pixels.
    pub fn claim_texture_bake(&mut self) -> Option<String> {
        self.view.claim_texture_bake().map(str::to_owned)
    }

    /// The pixels of a claimed bake; `false` when the arena no longer waits for them.
    /// They reach the GPU from JS memory, never copied into this instance's heap.
    #[cfg(target_arch = "wasm32")]
    pub fn supply_texture(
        &mut self,
        key: &str,
        rgba: &js_sys::Uint8Array,
    ) -> Result<bool, PageError> {
        self.view.supply_texture(key, rgba).map_err(js_error)
    }

    /// The pixels of a claimed bake, baked on the shell's threads.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn supply_texture(&mut self, key: &str, rgba: &[u8]) -> Result<bool, PageError> {
        self.view.supply_texture(key, rgba).map_err(js_error)
    }

    /// Bake a claimed texture on the main thread after all (the worker failed).
    pub fn release_texture_bake(&mut self, key: &str) {
        self.view.release_texture_bake(key);
    }

    /// The renderer, for the desktop shell's overlay and presentation.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn renderer(&mut self) -> &mut sloppy_render::gpu::Renderer {
        &mut self.view.renderer
    }

    // ---------------------------------------------------------- frame

    /// One animation frame: advance the interpolated display, send input due now, route
    /// events to presentation and draw. See [`net_frame_slot`] for the result.
    pub fn frame(&mut self, now: f64, input: &[f32]) -> Result<Vec<f32>, PageError> {
        let input = InputFrame::from_slice(input);
        self.sounds.set_clock(now);
        // The development delay delivers messages on this clock too.
        self.poll(now);
        self.drain_client_notices();
        self.view.rig.zoom_by(input.zoom);
        if input.toggle_view {
            self.toggle_first_person();
        }
        // Skipped while the GPU still runs earlier frames, a frame only takes its input.
        // Rooms run at the timestamp itself, which the network client's clocks share.
        let in_flight = self.view.renderer.frames_in_flight();
        if self.pacing.frame_time(now, in_flight).is_none() {
            let active = self.client.active_input();
            self.commands.queue(&input, active, now);
            if active && self.view.rig.first_person.enabled {
                self.view.rig.first_person.turn(input.look_pixels, 0.0, 0.0);
            }
            let mut result = self.last_result.clone();
            result[net_frame_slot::RAN] = 0.0;
            result[net_frame_slot::EVENTS] = self.events.len() as f32;
            result[net_frame_slot::HUD_DUE] = 0.0;
            result[net_frame_slot::SIM_MS] = 0.0;
            result[net_frame_slot::RENDER_MS] = 0.0;
            result[net_frame_slot::DT] = 0.0;
            return Ok(result);
        }
        let dt = self.times.advance(now);
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
                let scoreboard = Scoreboard::Rendered(&frame.state.tanks);
                for displayed in frame.events {
                    let feedback = event_feedback(
                        &displayed.event,
                        displayed.own,
                        frame.state.viewer_id,
                        &scoreboard,
                    );
                    queue_event(
                        events,
                        view,
                        displayed.event,
                        displayed.player_hit,
                        displayed.own,
                        feedback,
                    );
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
        let playing = drawn
            && self
                .client
                .display()
                .is_some_and(|state| state.match_state.phase == MatchPhase::Playing);
        self.graphics
            .frame(&mut self.view.renderer, &self.times, playing);
        let result = self.frame_result(drawn, dt);
        self.last_result.clone_from(&result);
        Ok(result)
    }

    /// Displayed events for audio and HUD, as JSON (`Game.drain_events()`'s shape).
    pub fn drain_events(&mut self) -> String {
        batch_json(self.take_events(), &mut self.events)
    }

    /// The HUD for the displayed scene (`Game.hud_json()`'s `match`, `human`,
    /// `scoreboard` and `scorePill`), or `null` before the first scene.
    pub fn hud_json(&mut self) -> String {
        let Some(hud) = self.hud() else {
            return "null".into();
        };
        serde_json::to_string(&hud).expect("room HUD serializes")
    }

    /// Stats for nerds' readings: renderer and frame rows, the displayed scene and the
    /// network, for diagnostics and the network checks.
    pub fn stats_json(&self, now: f64) -> String {
        let mut stats = presentation_stats(&self.view, &self.times);
        stats.insert(
            "quality".into(),
            self.graphics
                .stats(&self.view.renderer, self.canvas.pixel_ratio()),
        );
        if let Some(scene) = self.scene_stats() {
            stats.insert("scene".into(), json!(scene));
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
                "correctionMPerS": network.correction_m_per_s,
                "correctionP95M": network.correction_p95_m,
                "inputStarts": network.input_starts,
                "lateInputStarts": network.late_input_starts,
                "correctionCauses": CorrectionCause::ALL
                    .iter()
                    .zip(network.correction_causes)
                    .map(|(cause, (count, metres_per_second))| {
                        (
                            cause.as_str().to_string(),
                            json!({ "count": count, "mPerS": metres_per_second }),
                        )
                    })
                    .collect::<serde_json::Map<_, _>>(),
                "receiveMs": self.receive_ms,
                "corrections": network.corrections,
                "confirmedSnapshots": network.confirmed_snapshots,
                "phantomStrikes": network.phantom_strikes,
            }),
        );
        let feel = &self.client.feel;
        let own = &self.client.own_shots.stats;
        stats.insert(
            "feel".into(),
            json!({
                "viewDelayMs": summary(&feel.view_delay),
                "fireToFlashMs": summary(&feel.fire_to_flash),
                "fireToReportMs": summary(&feel.fire_to_report),
                "unansweredPresses": feel.unanswered_presses,
                "hitOffsetM": summary(&feel.hit_offset),
                "incomingOffsetM": summary(&feel.incoming_offset),
                "incomingBehindM": summary(&feel.incoming_behind),
                "unseenIncoming": feel.unseen_incoming,
                "ownShots": {
                    "predicted": own.predicted,
                    "confirmed": own.confirmed,
                    "unconfirmedShells": own.unconfirmed_shells,
                    "unpredicted": own.unpredicted,
                    "phantomStrikes": own.phantom_strikes,
                    "errorM": summary(&own.error),
                },
            }),
        );
        Value::Object(stats).to_string()
    }

    /// Stats for nerds' sections and rows as JSON ([`StatsSection`]) at `now`, for the
    /// panel's refreshes, which the update rate is measured between.
    pub fn nerd_stats_json(&mut self, now: f64) -> String {
        serde_json::to_string(&self.nerd_stats(now)).expect("stats rows serialize")
    }

    /// Where the ground point `x`, `z` shows on the canvas, as `[x, y]` in normalized
    /// device coordinates, or empty when it is off screen: for browser checks that aim.
    pub fn debug_screen_point(&self, x: f64, z: f64) -> Vec<f32> {
        let point = self
            .view
            .rig
            .camera
            .project(glam::Vec3::new(x as f32, 0.0, z as f32));
        if point.z > 0.0 && point.z < 1.0 && point.x.abs() < 1.0 && point.y.abs() < 1.0 {
            vec![point.x, point.y]
        } else {
            Vec::new()
        }
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
                "shotList": state
                    .shots
                    .iter()
                    .map(|shot| json!({ "id": shot.id, "team": shot.team, "x": shot.x, "z": shot.z }))
                    .collect::<Vec<Value>>(),
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

/// The calls above that cross the boundary as JSON or packed numbers, typed for a Rust
/// shell (the desktop app); the page's getters serialize these same values.
impl NetGame {
    /// `create` with the view and the room's configuration as values.
    pub async fn create_with(
        canvas: RenderTarget,
        view_config: ViewConfig,
        config: NetConfig,
    ) -> Result<NetGame, PageError> {
        if !is_room_code(&config.room) {
            return Err(js_error("Invalid room code"));
        }
        let PageView {
            view,
            canvas,
            graphics,
            seed,
            pacing,
            shell,
        } = create_view(canvas, view_config).await?;
        let client = NetworkClient::new(ClientConfig {
            server_url: config.server.clone(),
            room: config.room.clone(),
            saved_seat: config.saved_seat(),
            delay: config.delay(),
            seed: seed as u32,
            dodger_first: config.dodge.as_deref() != Some("0"),
            shooter_first: config.shooter.as_deref() != Some("0"),
        });
        let mut game = NetGame {
            client,
            view,
            commands: CommandBuilder::default(),
            arena: None,
            baseline: None,
            prepared: false,
            events: Vec::new(),
            sounds: SoundPlanner::default(),
            notices: Vec::new(),
            room_text: RoomText::default(),
            effect_timers: EffectTimers::default(),
            frame_index: 0,
            times: FrameTimes::default(),
            pacing,
            canvas,
            shell,
            graphics,
            last_result: vec![0.0; net_frame_slot::LENGTH],
            receive_ms: 0.0,
            network_sampler: NetworkSampler::default(),
        };
        game.canvas.apply(&mut game.view);
        Ok(game)
    }

    /// Join the room with `choice`.
    pub fn join(&mut self, choice: JoinChoice, now: f64) {
        self.client.connect(choice, now);
    }

    /// Change team (`None`: the side with fewer humans) or tank between battles; a
    /// player cannot drive a Humvee.
    pub fn choose_seat(
        &mut self,
        team: Option<Team>,
        kind: VehicleKind,
        now: f64,
    ) -> Result<(), PageError> {
        if !VehicleKind::PLAYABLE.contains(&kind) {
            return Err(js_error("Invalid choice"));
        }
        self.client.choose(team, kind, now);
        Ok(())
    }

    /// The host's rules for the next battle.
    pub fn set_rules(&mut self, settings: RoomSettings, now: f64) {
        self.client.settings(settings, now);
    }

    /// Socket and storage work, in order (see the module docs).
    pub fn take_room_actions(&mut self) -> Vec<ClientAction> {
        self.client.take_actions()
    }

    /// UI notices, in order. Arena preparation and baseline requests are handled here
    /// and surface as `Prepare` and `BaselineShown`.
    pub fn take_room_notices(&mut self) -> Vec<RoomNotice> {
        self.drain_client_notices();
        std::mem::take(&mut self.notices)
    }

    /// Create up to `budget` background-compiled pipelines for the arena `Prepare` asked
    /// for (see `prepare_step`).
    pub fn prepare(&mut self, budget: u32, now: f64) -> PrepareStatus {
        if self.arena.is_none() {
            return PREPARED;
        }
        let status = match self.view.prepare_step(budget.max(1)) {
            Ok(status) => status,
            Err(error) => {
                self.arena_failed(error);
                return PREPARED;
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
        status
    }

    /// The HUD for the displayed scene, or `None` before the first scene.
    pub fn hud(&mut self) -> Option<RoomHud<'_>> {
        let timers = &mut self.effect_timers;
        self.client
            .display()
            .and_then(|state| room_hud(state, timers))
    }

    /// Displayed events for audio and HUD, heard from the viewer's tank.
    pub fn take_events(&mut self) -> EventBatch {
        let listener = self.listener();
        let right = self.view.rig.listener_right;
        take_events(&mut self.events, listener, right, &mut self.sounds)
    }

    /// The chosen view and clamped overhead zoom.
    pub fn camera(&self) -> CameraPreferences {
        CameraPreferences::of(&self.view.rig)
    }

    /// The Settings dialog's Graphics choice.
    pub fn set_graphics_setting(&mut self, setting: GraphicsSetting) {
        self.graphics.set_setting(&mut self.view.renderer, setting);
    }

    /// Stats for nerds' sections and rows at `now`; the update rate is measured since
    /// the previous call.
    pub fn nerd_stats(&mut self, now: f64) -> Vec<StatsSection> {
        let network = self.client.stats(now);
        let feel = &self.client.feel;
        let own = &self.client.own_shots.stats;
        let stats = RoomStats {
            frame: frame_readings(&self.times),
            renderer: renderer_readings(
                &self.view.renderer,
                &self.graphics,
                self.canvas.pixel_ratio(),
            ),
            scene: self.scene_stats(),
            network: ConnectionStats {
                rtt_ms: network.rtt_ms,
                received_updates: network.received_updates,
                snapshot_age_ms: network.snapshot_age_ms,
                buffer_ms: network.buffer_ms,
                margin_ms: network.margin_ms,
                underrun: network.underrun,
                server_tick: network.server_tick,
                input_seq: network.input_seq,
                input_ack: network.input_ack,
                connected: network.connected,
                late_batches: network.late_batches,
                longest_batch_gap_ms: network.longest_batch_gap_ms,
                prediction_lead_ms: network.prediction_lead_ms,
                correction_m_per_s: network.correction_m_per_s,
                correction_p95_m: network.correction_p95_m,
                input_starts: network.input_starts,
                late_input_starts: network.late_input_starts,
                correction_causes: network
                    .correction_causes
                    .map(|(count, m_per_s)| CauseStats { count, m_per_s }),
                receive_ms: self.receive_ms,
                phantom_strikes: network.phantom_strikes,
            },
            feel: FeelStats {
                view_delay_ms: Summary::from(&feel.view_delay),
                fire_to_flash_ms: Summary::from(&feel.fire_to_flash),
                fire_to_report_ms: Summary::from(&feel.fire_to_report),
                hit_offset_m: Summary::from(&feel.hit_offset),
                incoming_offset_m: Summary::from(&feel.incoming_offset),
                incoming_behind_m: Summary::from(&feel.incoming_behind),
                unseen_incoming: feel.unseen_incoming,
                own_shots: OwnShotStats {
                    predicted: own.predicted,
                    confirmed: own.confirmed,
                    unconfirmed_shells: own.unconfirmed_shells,
                    unpredicted: own.unpredicted,
                    phantom_strikes: own.phantom_strikes,
                    error_m: Summary::from(&own.error),
                },
            },
        };
        let rates = self.network_sampler.sample(now, &stats.network);
        room_sections(&stats, rates, self.shell)
    }

    /// The GPU's smoothed frame time in milliseconds, once sampled.
    pub fn gpu_ms(&self) -> Option<f64> {
        self.graphics.gpu_ms()
    }
}

impl NetGame {
    /// The displayed scene's counts, once a scene shows.
    fn scene_stats(&self) -> Option<SceneStats> {
        let count = |count: usize| count as u64;
        self.client.display().map(|state| SceneStats {
            tanks: count(state.tanks.len()),
            alive: count(state.tanks.iter().filter(|tank| tank.alive).count()),
            mines: count(state.mines.len()),
            pickups: count(state.pickups.len()),
            pickups_ready: count(
                state
                    .pickups
                    .iter()
                    .filter(|pickup| pickup.available)
                    .count(),
            ),
            shots: count(state.shots.len()),
            fragments: count(state.fragments.len()),
            elapsed: state.elapsed,
        })
    }

    fn notice(&mut self, notice: RoomNotice) {
        if self.notices.len() >= MAX_PENDING_NOTICES {
            self.notices.remove(0);
        }
        self.notices.push(notice);
    }

    /// Where the viewer hears the displayed events: its tank, or where it died.
    fn listener(&self) -> (f64, f64) {
        self.client
            .display()
            .and_then(RenderState::viewer)
            .map_or((0.0, 0.0), |viewer| {
                if viewer.alive {
                    (viewer.position.x, viewer.position.z)
                } else {
                    (viewer.previous.x, viewer.previous.z)
                }
            })
    }

    /// Turn client notices into page notices; arena and baseline requests go to the
    /// presentation here.
    fn drain_client_notices(&mut self) {
        for notice in self.client.take_notices() {
            match notice {
                ClientNotice::ClearInput => self.commands.clear(),
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
                _ => {}
            }
            if let Some(notice) = RoomNotice::from_client(
                notice,
                &self.client.player_id,
                &mut self.room_text,
                self.shell,
            ) {
                self.notice(notice);
            }
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
        self.notice(RoomNotice::Prepare);
    }

    fn arena_failed(&mut self, error: String) {
        self.arena = None;
        self.client.arena_failed();
        self.notice(RoomNotice::ArenaFailed(error));
    }

    /// A full state makes presentation rebuild every model. Draw the first frames before
    /// the arena takes input again, as the TypeScript `showBaseline` did.
    fn show_baseline(&mut self, baseline: u32, now: f64) -> Result<(), PageError> {
        let Self { client, view, .. } = self;
        let Some(display) = client.display() else {
            return Ok(());
        };
        view.build_scenery(&display.map_theme);
        view.reset(display);
        for _ in 0..2 {
            view.render(display, 1.0, 0.0, false).map_err(js_error)?;
        }
        self.graphics.round_started();
        self.events.clear();
        self.client.baseline_shown(baseline, now);
        self.drain_client_notices();
        self.notice(RoomNotice::BaselineShown);
        Ok(())
    }

    /// The controls for this frame: movement relative to first person, the aim from the
    /// pointer, touch stick or first-person yaw, and queued one-shot actions.
    fn local_input(&mut self, input: &InputFrame, dt: f64) -> LocalInput {
        let Some(viewer) = self.client.display().and_then(RenderState::viewer) else {
            return LocalInput::default();
        };
        let position = (viewer.position.x, viewer.position.z);
        let (angle, aim_point) = aim(&mut self.view.rig, input, position, dt, true);
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
        let viewer = self.client.display().and_then(RenderState::viewer);
        let playing = phase == RoomPhase::Playing
            && self
                .client
                .display()
                .is_some_and(|state| state.match_state.phase != MatchPhase::Ready);
        result[slot::COCKPIT] = flag(playing && self.view.rig.seat_wanted);
        result[slot::HULL_ANGLE] = viewer.map_or(0.0, |viewer| {
            self.view.rig.first_person.screen_angle(viewer.heading) as f32
        });
        result[slot::EVENTS] = self.events.len() as f32;
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
        result[slot::RAN] = 1.0;
        result[slot::POINTER_FREE] =
            flag(!drawn || !self.client.connected || menu || phase != RoomPhase::Playing);
        let (viewer_x, viewer_y) = self.view.viewer_ndc().unwrap_or((f32::NAN, f32::NAN));
        result[slot::VIEWER_X] = viewer_x;
        result[slot::VIEWER_Y] = viewer_y;
        self.frame_index += 1;
        result
    }
}

/// A feel reading as the statistics show it: sample count, mean, median, 95th percentile
/// and largest.
fn summary(samples: &Samples) -> Value {
    json!({
        "count": samples.count,
        "mean": samples.mean(),
        "p50": samples.percentile(0.5),
        "p95": samples.percentile(0.95),
        "max": samples.largest,
    })
}
