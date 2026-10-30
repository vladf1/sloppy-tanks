//! `Game`: single-player Sloppy Tanks for the page. It owns the simulation, the
//! presentation and the renderer, and runs the fixed-step loop of `src/game.ts`.
//! The page makes one coarse call per frame; no per-entity calls cross the
//! Wasm boundary.
//!
//! # Lifecycle
//!
//! ```text
//! const game = await Game.create(canvas, JSON.stringify(config));
//! // Loading screen: compile pipelines, wait for textures and for the GPU to run the
//! // warm-up; a task between steps, a short timer while `gpuPending` (slot 4).
//! while (!(await nextPrepareStep(pending), game.prepare_step(4))[3]) {}
//! game.start();                          // GO: the round begins
//! requestAnimationFrame(function loop(now) {
//!   const result = game.frame(now, input); // input: Float32Array(INPUT_LENGTH)
//!   if (result[4] > 0) audioAndUi(JSON.parse(game.drain_events()));
//!   if (result[6]) ui.update(JSON.parse(game.hud_json()));
//!   requestAnimationFrame(loop);
//! });
//! ```
//!
//! - `Game.create(canvas, config_json)`: config `{ seed?, assetBase, map?,
//!   difficulty?, extraLevels?, humanKind?, humanTeam?, gameMode?, autoplay?,
//!   cssWidth?, cssHeight?, pixelRatio?, firstPerson?, zoom?, hideReticle?, graphics?,
//!   device?, framesInFlight? }` (the view's part is `page::create_view`'s). Choices
//!   follow `initialGameOptions`; the arena is reset and preparation begins.
//!   `graphics` is the saved Graphics setting and `device` the page's device class,
//!   which chooses Auto's starting tier.
//! - `set_options(options_json) -> bool`: Battle Setup choices `{ humanKind,
//!   humanTeam, gameMode, mapMode, difficulty }` (`GameOptions`, camelCase). When
//!   they differ, the arena rebuilds with the map's level rules (extra levels
//!   play as an endless team battle) and needs `prepare_step` again. Returns
//!   whether it rebuilt.
//! - `prepare_step(budget) -> Float64Array [compiled, remaining, texturesPending,
//!   done, gpuPending]`: creates up to `budget` pipelines per call as their
//!   background compiles finish; once nothing remains and textures have loaded it
//!   warms every variant, then draws the first frames and reports `done = 1`.
//!   `gpuPending = 1` while only the GPU is working (background compiles or the
//!   warm-up): poll on a short timer then rather than a task.
//! - `loading_status() -> string | undefined`: Battle Setup's line for the latest
//!   `prepare_step` ("Shaders loaded: 12 of 40"), or none once ready or while a
//!   rebuild only warms up ([`Loading`]).
//! - `start()`: begin the round (GO, PLAY AGAIN). `resume()`: continue a pause.
//!   `pause()`: pause a playing round. `restart()`: a fresh world for Battle
//!   Setup (phase `ready`; prepare again). `end_battle()`: END BATTLE.
//! - `frame(now_ms, input) -> Float32Array` (see [`frame_slot`]): one call per
//!   animation frame. Input is the packed raw control state of
//!   `sloppy_render::presentation::input::slot` (keys, touch sticks, pointer NDC,
//!   fire, one-shot mine/ammo presses since the last frame, look pixels, zoom,
//!   view toggle, wheel ammo steps). With `framesInFlight`, a frame that finds that
//!   many frames still on the GPU is skipped (its presses wait for the next) and hands
//!   back the last result; the frames that run take an even clock
//!   ([`crate::frame_clock::FramePacing`]).
//! - `drain_events() -> JSON`: `{ events: [{ type, id?, label?, own, damageAngle,
//!   deathReport?, feed?, toast? }], sounds: [{ sound, gain, pan }] }` for audio and HUD;
//!   `own` marks the viewer's tank's events, `damageAngle` is the clockwise screen angle
//!   (radians) of damage the viewer took, or null, and the rest is the event's HUD
//!   feedback ([`crate::feedback`]). `sounds` are the file names to play, in order, with
//!   their linear gain and stereo pan ([`crate::sounds`], rate-limited on `frame`'s
//!   clock).
//! - `hud_json() -> JSON`: match, scores, score limit, team names, the score pill, the
//!   human's stats, ammo, rank, hull tone, cooldowns, effects, scoreboard, and the round
//!   recap once results show. The first recap of a result saves personal bests
//!   in `localStorage` and reports them (`best`, `improved`, `persisted`), with the
//!   battle report in words (`report`, `crate::battle_report`).
//! - `pause_json() -> JSON`: the pause menu's facts and standing line.
//! - `stats_json() -> JSON`: frame/sim/render ms, the renderer's draw calls, triangles
//!   and GPU resources, the graphics quality and the physics bodies, colliders, shots,
//!   fragments and standing covers, for diagnostics and the frame recorder.
//! - `nerd_stats_json() -> JSON`: the Stats for nerds panel's sections, `[{ title,
//!   rows: [{ label, value, hint }] }]` ([`crate::nerd_rows`]).
//! - `resize(css_width, css_height, pixel_ratio, exact)`: the drawing buffer is
//!   the CSS size times the pixel ratio, capped at 1.5; `exact` draws at exactly the
//!   CSS size (ratio 1). The scene renders at the graphics quality's scale of it.
//! - `set_graphics(setting)`: the Settings dialog's Graphics choice ("auto",
//!   "high", "medium", "low").
//! - `toggle_first_person() -> bool`: V / the view button while playing.
//! - `camera_preferences() -> Float64Array [firstPerson, zoom]`: the chosen view
//!   and clamped overhead zoom, for the page to save after a camera input.
//! - `set_human_kind(kind)`: the respawn menu's tank, keeping the world.
//! - `set_speed(key, value) -> f64`: "tank-speed" or "bullet-speed" scale.
//! - `debug_*`: the dev `window.sloppy` hooks below and the fixture hooks of
//!   `game/debug.rs`.
//! - `error() -> string | undefined`: the first GPU error, if any.
//!
//! A Rust shell (the desktop app) passes and reads the same values typed rather than as
//! JSON or names: `create_with`, `set_game_options`, `prepare` ([`PrepareStatus`]),
//! `loading` ([`Loading`]), `hud` ([`Hud`]), `take_events` ([`EventBatch`]), `camera`
//! ([`CameraPreferences`]), `nerd_stats` ([`StatsSection`]), `gpu_ms`, and
//! `set_graphics_setting`, `respawn_in` and `set_speed_scale` with the setting, tank and
//! speed themselves.

use crate::Shell;
use crate::battle_report::pause_report;
use crate::events::{EventBatch, PendingEvent, batch_json, take_events};
use crate::feedback::event_feedback;
use crate::frame_clock::{FramePacing, FrameTimes, StepClock};
use crate::frame_slot::{self, phase_code};
use crate::hud::{EffectTimers, Hud, Loading, Scoreboard, simulation_hud};
use crate::nerd_rows::{GameStats, SceneStats, StatsSection, game_sections};
use crate::page::{
    BakedPixels, CameraPreferences, CanvasSize, Graphics, HUD_UPDATE_EVERY_FRAMES, PREPARED,
    PageError, PageView, ViewConfig, aim, create_view, js_error, now_ms, parse, prepare_result,
    queue_event,
};
use crate::recap::{RoundRecap, round_recap};
use crate::sounds::SoundPlanner;
use crate::stats::{frame_readings, presentation_stats, renderer_readings};
use sloppy_render::gpu::RenderTarget;

use glam::Vec2;
use serde::de::DeserializeOwned;
use serde::de::value::StringDeserializer;
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};
use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::data::STEP;
use sloppy_core::sim::difficulty::Difficulty;
use sloppy_core::sim::extra_levels::extra_level;
use sloppy_core::sim::game_options::{GameOptions, game_choices, initial_game_options};
use sloppy_core::sim::level_rules::{single_player_rules, standard_rules};
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::match_state::end_battle;
use sloppy_core::sim::render_state::RenderTank;
#[cfg(target_arch = "wasm32")]
use sloppy_core::sim::round_recap::{RecordStorage, StorageUnavailable};
use sloppy_core::sim::speed_tuning::{SpeedSetting, tune_speed};
use sloppy_core::sim::types::{DamageCause, DamageSource};
use sloppy_core::sim::{
    CoverKind, FragmentShape, GameMode, MatchPhase, RenderState, Shot, SimEventType, Simulation,
    SimulationSetup, Team, VehicleCommand, VehicleKind, Weapon,
};
use sloppy_render::presentation::input::{CommandBuilder, InputFrame};
use sloppy_render::presentation::view_settings::CAMERA;
use sloppy_render::presentation::{PrepareStatus, Presentation};
use sloppy_render::quality::GraphicsSetting;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

mod debug;

/// The round the browser's first arena prepares (it seeds bot names).
const FIRST_ROUND: u32 = 3;
/// Metres along an autoplaying turret where its reticle sits: the enemy's distance
/// within these bounds, or the middle range when it has no enemy.
const AUTOPLAY_AIM_RANGE: f64 = 14.0;
const AUTOPLAY_MIN_AIM_RANGE: f64 = 7.0;
const AUTOPLAY_MAX_AIM_RANGE: f64 = 26.0;

/// Battle Setup's choices with the map's level rules: extra levels bring their own
/// arena and play endless.
fn options_setup(options: &GameOptions) -> SimulationSetup {
    let level_rules = match extra_level(options.map_mode) {
        Some(level) => single_player_rules(level),
        None => standard_rules(),
    };
    SimulationSetup {
        human_kind: Some(options.human_kind),
        human_team: Some(options.human_team),
        game_mode: Some(options.game_mode),
        map_mode: Some(options.map_mode),
        difficulty: Some(options.difficulty),
        ..SimulationSetup::default()
    }
    .merged(level_rules)
}

/// Fixed, dynamic and sleeping (dynamic) bodies in the physics world.
fn body_counts(sim: &Simulation) -> (usize, usize, usize) {
    let (mut fixed, mut dynamic, mut sleeping) = (0, 0, 0);
    for (_, body) in sim.world.bodies.iter() {
        if body.is_fixed() {
            fixed += 1;
        }
        if body.is_dynamic() {
            dynamic += 1;
            if body.is_sleeping() {
                sleeping += 1;
            }
        }
    }
    (fixed, dynamic, sleeping)
}

/// `Game.create`'s choices besides the view configuration (`page::ViewConfig`). They
/// follow `initialGameOptions`: the map a link requests or the player last chose, which
/// stands only where extra levels are offered.
#[derive(Clone, Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct GameConfig {
    #[serde(deserialize_with = "known_choice")]
    pub map: Option<MapId>,
    #[serde(deserialize_with = "known_choice")]
    pub difficulty: Option<Difficulty>,
    pub extra_levels: bool,
    pub human_kind: Option<VehicleKind>,
    pub human_team: Option<Team>,
    pub game_mode: Option<GameMode>,
    /// The human's tank drives itself (`?autoplay`).
    pub autoplay: bool,
}

/// A choice by its name; a name this build does not know reads as none, as
/// `initialGameOptions` ignores an unknown `?map=`.
fn known_choice<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let name = Option::<String>::deserialize(deserializer)?;
    Ok(name.and_then(|name| {
        T::deserialize(StringDeserializer::<serde::de::value::Error>::new(name)).ok()
    }))
}

/// Personal bests live in the page's `localStorage`; private browsing or a full
/// quota only makes them session-only.
#[cfg(target_arch = "wasm32")]
struct BrowserStorage(Option<web_sys::Storage>);

/// Natively, personal bests live in a JSON file of the same keys and values
/// ([`crate::records`]).
#[cfg(not(target_arch = "wasm32"))]
use crate::records::FileStorage as BrowserStorage;

#[cfg(target_arch = "wasm32")]
impl BrowserStorage {
    fn local() -> Self {
        Self(web_sys::window().and_then(|window| window.local_storage().ok().flatten()))
    }
}

#[cfg(target_arch = "wasm32")]
impl RecordStorage for BrowserStorage {
    fn get_item(&self, key: &str) -> Result<Option<String>, StorageUnavailable> {
        let storage = self.0.as_ref().ok_or(StorageUnavailable)?;
        storage.get_item(key).map_err(|_| StorageUnavailable)
    }

    fn set_item(&mut self, key: &str, value: &str) -> Result<(), StorageUnavailable> {
        let storage = self.0.as_ref().ok_or(StorageUnavailable)?;
        storage.set_item(key, value).map_err(|_| StorageUnavailable)
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct Game {
    /// The shell drawing this game, for the wording the page and the desktop app say
    /// differently.
    shell: Shell,
    sim: Simulation,
    view: Presentation,
    state: RenderState,
    options: GameOptions,
    commands: CommandBuilder,
    steps: StepClock,
    pacing: FramePacing,
    active: bool,
    autoplay: bool,
    overview: bool,
    auto_rounds: bool,
    completed_rounds: u32,
    frame_index: u64,
    /// Warmed and first frames drawn; false while pipelines compile.
    prepared: bool,
    /// Pipelines created since the arena began preparing, and Battle Setup's line for
    /// the latest step.
    prepare_compiled: u32,
    loading: Option<Loading>,
    events: Vec<PendingEvent>,
    /// The sounds drained events play, on the frame clock.
    sounds: SoundPlanner,
    canvas: CanvasSize,
    times: FrameTimes,
    graphics: Graphics,
    /// The human's tank as the HUD reads it, refilled from the simulation each read.
    hud_tank: RenderTank,
    /// How long the human's effects have run, which their timer rings drain from.
    effect_timers: EffectTimers,
    /// The finished round's recap; personal bests are saved once per result.
    recap: Option<RoundRecap>,
    /// The last frame's result, which a frame skipped for the GPU hands back.
    last_result: Vec<f32>,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Game {
    /// Create the renderer on `canvas`, build the chosen arena and begin
    /// preparing it. Rejects when the renderer cannot start.
    pub async fn create(canvas: RenderTarget, config_json: &str) -> Result<Game, PageError> {
        #[cfg(target_arch = "wasm32")]
        console_error_panic_hook::set_once();
        Self::create_with(canvas, parse(config_json)?, parse(config_json)?).await
    }

    // ---------------------------------------------------------- lifecycle

    /// Apply Battle Setup choices; rebuilds the arena when they changed.
    pub fn set_options(&mut self, options_json: &str) -> Result<bool, PageError> {
        Ok(self.set_game_options(&parse(options_json)?))
    }

    /// Compile up to `budget` pipelines;
    /// `[compiled, remaining, texturesPending, done, gpuPending]`.
    pub fn prepare_step(&mut self, budget: u32) -> Result<Vec<f64>, PageError> {
        self.prepare(budget).map(|status| prepare_result(&status))
    }

    /// Battle Setup's line for the latest `prepare_step`, if it has one.
    pub fn loading_status(&self) -> Option<String> {
        self.loading.map(|line| line.to_string())
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

    /// Bake a claimed texture on the main thread after all (the worker failed).
    pub fn release_texture_bake(&mut self, key: &str) {
        self.view.release_texture_bake(key);
    }

    /// Begin the round: GO, or PLAY AGAIN after results.
    pub fn start(&mut self) {
        if self.sim.match_state.phase == MatchPhase::Results {
            self.sim.reset(None);
            self.reset_view();
        }
        self.commands.clear();
        self.sim.start();
        self.active = true;
        self.steps.reset();
        self.graphics.round_started();
        // Animation frames queued before GO (behind a slow arena rebuild) carry older
        // timestamps; measuring from now keeps them from advancing the new round.
        // Autoplay keeps its seeded timeline frame-exact whatever the page's load
        // time instead: its first frame starts the round with a full catch-up.
        self.times.last_ms = Some(if self.autoplay {
            f64::NEG_INFINITY
        } else {
            now_ms()
        });
    }

    pub fn resume(&mut self) {
        if self.sim.match_state.phase == MatchPhase::Paused {
            self.commands.clear();
            self.sim.start();
            self.steps.reset();
        }
    }

    pub fn pause(&mut self) {
        if self.sim.match_state.phase == MatchPhase::Playing {
            self.sim.match_state.phase = MatchPhase::Paused;
            self.commands.clear();
            self.steps.reset();
        }
    }

    /// A fresh world behind Battle Setup (phase `ready`); prepare again.
    pub fn restart(&mut self) {
        self.commands.clear();
        self.sim.reset(None);
        self.options = game_choices(&self.sim);
        self.reset_view();
    }

    /// END BATTLE from the pause menu.
    pub fn end_battle(&mut self) {
        end_battle(&mut self.sim.match_state);
    }

    // ---------------------------------------------------------- frame

    /// One animation frame: steps the simulation at the fixed rate, routes
    /// events to presentation, and draws. See [`frame_slot`] for the result.
    pub fn frame(&mut self, now: f64, input: &[f32]) -> Result<Vec<f32>, PageError> {
        let input = InputFrame::from_slice(input);
        self.sounds.set_clock(now);
        // A paced frame runs at its smoothed time; skipped, it only takes the input.
        let in_flight = self.view.renderer.frames_in_flight();
        let Some(now) = self.pacing.frame_time(now, in_flight) else {
            return Ok(self.wait_for_gpu(&input, now));
        };
        let dt = self.times.advance(now);
        let mut result = vec![0.0; frame_slot::LENGTH];
        result[frame_slot::RAN] = 1.0;
        if !self.active {
            result[frame_slot::PHASE] = phase_code(self.sim.match_state.phase);
            self.last_result.clone_from(&result);
            return Ok(result);
        }
        if self.sim.match_state.phase == MatchPhase::Results && self.auto_rounds {
            self.commands.clear();
            self.completed_rounds += 1;
            self.sim.reset(None);
            self.reset_view();
            self.sim.start();
            self.graphics.round_started();
        }
        self.view.rig.zoom_by(input.zoom);
        if input.toggle_view {
            self.toggle_first_person();
        }
        let sim_start = now_ms();
        let human = self.human_index();
        let playing = self.sim.match_state.phase == MatchPhase::Playing;
        let alive = self.sim.tanks[human].alive;
        self.commands.queue(&input, playing && alive, now);
        if playing {
            let steps = self.steps.advance(dt);
            let position = self.human_position();
            // Autoplay's turret answers to its brain, not the pointer: the reticle
            // follows the gun and the camera leans with it.
            let angle = match self.autoplay_aim_point() {
                Some(point) if !self.view.rig.first_person.enabled => {
                    self.view.rig.track_aim(point, dt);
                    self.sim.human().aim
                }
                _ => aim(&mut self.view.rig, &input, position, dt, alive).0,
            };
            for _ in 0..steps {
                let active = self.sim.match_state.phase == MatchPhase::Playing
                    && self.sim.tanks[human].alive;
                let command = self.commands.command(&input, angle, active);
                let command = self.view.rig.first_person.steer(command);
                self.sim.set_wreck_view(Some(self.view.wreck_view()));
                self.sim.step(command, self.autoplay);
            }
        } else {
            self.steps.reset();
        }
        let human = self.human_index();
        let clear_input =
            self.sim.match_state.phase != MatchPhase::Playing || !self.sim.tanks[human].alive;
        if clear_input {
            self.commands.clear();
        }
        self.times.sim_ms = now_ms() - sim_start;
        self.fill_state();
        self.route_events();
        let render_start = now_ms();
        if self.sim.match_state.phase != MatchPhase::Ready {
            let alpha = if self.sim.match_state.phase == MatchPhase::Playing {
                self.steps.alpha()
            } else {
                1.0
            };
            self.view
                .render(&self.state, alpha, dt, self.overview)
                .map_err(js_error)?;
            self.sim.set_wreck_view(Some(self.view.wreck_view()));
        }
        self.times.render_ms = now_ms() - render_start;
        let playing = self.sim.match_state.phase == MatchPhase::Playing;
        self.graphics
            .frame(&mut self.view.renderer, &self.times, playing);
        let tank = &self.sim.tanks[human];
        result[frame_slot::PHASE] = phase_code(self.sim.match_state.phase);
        result[frame_slot::HUMAN_ALIVE] = f32::from(u8::from(tank.alive));
        result[frame_slot::COCKPIT] = f32::from(u8::from(
            self.sim.match_state.phase != MatchPhase::Ready && self.view.rig.seat_wanted,
        ));
        result[frame_slot::HULL_ANGLE] =
            self.view.rig.first_person.screen_angle(tank.heading) as f32;
        result[frame_slot::EVENTS] = self.events.len() as f32;
        result[frame_slot::CLEAR_INPUT] = f32::from(u8::from(clear_input));
        result[frame_slot::HUD_DUE] = f32::from(u8::from(
            self.frame_index.is_multiple_of(HUD_UPDATE_EVERY_FRAMES),
        ));
        result[frame_slot::SIM_MS] = self.times.sim_ms as f32;
        result[frame_slot::RENDER_MS] = self.times.render_ms as f32;
        result[frame_slot::FIRST_PERSON] = f32::from(u8::from(self.view.rig.first_person.enabled));
        result[frame_slot::DT] = dt as f32;
        let (viewer_x, viewer_y) = self.view.viewer_ndc().unwrap_or((f32::NAN, f32::NAN));
        result[frame_slot::VIEWER_X] = viewer_x;
        result[frame_slot::VIEWER_Y] = viewer_y;
        self.frame_index += 1;
        self.last_result.clone_from(&result);
        Ok(result)
    }

    /// An animation frame skipped while the GPU still runs earlier ones: the view and
    /// first-person look take its input at once, its presses wait for the next tick, and
    /// the page keeps the last frame's result.
    fn wait_for_gpu(&mut self, input: &InputFrame, now: f64) -> Vec<f32> {
        self.view.rig.zoom_by(input.zoom);
        if input.toggle_view {
            self.toggle_first_person();
        }
        let playing = self.sim.match_state.phase == MatchPhase::Playing;
        let alive = self.sim.human().alive;
        self.commands.queue(input, playing && alive, now);
        if playing && alive && self.view.rig.first_person.enabled {
            self.view.rig.first_person.turn(input.look_pixels, 0.0, 0.0);
        }
        let mut result = self.last_result.clone();
        result[frame_slot::RAN] = 0.0;
        result[frame_slot::EVENTS] = self.events.len() as f32;
        result[frame_slot::HUD_DUE] = 0.0;
        result[frame_slot::SIM_MS] = 0.0;
        result[frame_slot::RENDER_MS] = 0.0;
        result[frame_slot::FIRST_PERSON] = f32::from(u8::from(self.view.rig.first_person.enabled));
        result[frame_slot::DT] = 0.0;
        result
    }

    /// Pending events for audio and HUD, as JSON.
    pub fn drain_events(&mut self) -> String {
        batch_json(self.take_events(), &mut self.events)
    }

    /// Everything the page HUD and menus show, as JSON.
    pub fn hud_json(&mut self) -> String {
        // Written straight to text: the page reads this every few frames.
        serde_json::to_string(&self.hud()).unwrap_or_default()
    }

    /// Pause's facts and the line under its title, as JSON, read as the pause menu opens.
    pub fn pause_json(&mut self) -> String {
        serde_json::to_string(&pause_report(&self.hud())).unwrap_or_default()
    }

    /// Stats for nerds' readings, as JSON, for diagnostics and the frame recorder.
    pub fn stats_json(&mut self) -> String {
        let mut stats = presentation_stats(&self.view, &self.times);
        let sim = &self.sim;
        let simulation = json!({
            "quality": self.graphics.stats(&self.view.renderer, self.canvas.pixel_ratio()),
            "bodies": sim.world.bodies.len(),
            "colliders": sim.world.colliders.len(),
            "shots": sim.shots.len(),
            "fragments": sim.fragments.len(),
            "covers": sim.covers.iter().filter(|cover| cover.alive).count(),
        });
        if let Value::Object(rows) = simulation {
            stats.extend(rows);
        }
        Value::Object(stats).to_string()
    }

    /// Stats for nerds' sections and rows as JSON ([`StatsSection`]), for the panel's
    /// refreshes.
    pub fn nerd_stats_json(&mut self) -> String {
        serde_json::to_string(&self.nerd_stats()).expect("stats rows serialize")
    }

    /// The canvas's CSS size and the device pixel ratio.
    pub fn resize(&mut self, css_width: f64, css_height: f64, pixel_ratio: f64, exact: bool) {
        self.canvas = CanvasSize {
            css: Vec2::new(css_width as f32, css_height as f32),
            device_pixel_ratio: pixel_ratio,
            exact,
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

    /// Enter or leave first person (only while playing); returns whether it is on.
    pub fn toggle_first_person(&mut self) -> bool {
        if self.sim.match_state.phase == MatchPhase::Playing {
            let aim = self.sim.human().aim;
            self.view.rig.first_person.toggle(aim);
        }
        self.view.rig.first_person.enabled
    }

    pub fn camera_preferences(&self) -> Vec<f64> {
        self.view.rig.preferences().to_vec()
    }

    /// The tank the human respawns in, chosen from the respawn menu mid-round, by name.
    #[cfg(target_arch = "wasm32")]
    pub fn set_human_kind(&mut self, kind: &str) -> Result<(), PageError> {
        let kind: VehicleKind = serde_json::from_value(Value::from(kind))
            .map_err(|error| js_error(error.to_string()))?;
        self.respawn_in(kind)
    }

    /// "tank-speed" or "bullet-speed"; returns the scale in effect.
    #[cfg(target_arch = "wasm32")]
    pub fn set_speed(&mut self, key: &str, value: f64) -> f64 {
        let key = match key {
            "tank-speed" => SpeedSetting::TankSpeed,
            "bullet-speed" => SpeedSetting::BulletSpeed,
            _ => return 1.0,
        };
        self.set_speed_scale(key, value)
    }

    pub fn error(&self) -> Option<String> {
        self.view.renderer.error()
    }

    // ---------------------------------------------------------- diagnostics

    /// What the browser checks read from `window.sloppy` (sim and view state).
    pub fn debug_json(&self) -> String {
        let sim = &self.sim;
        let human = sim.human();
        let position = sim.tank_position(human);
        let rig = &self.view.rig;
        let camera = rig.camera;
        let crosshair = self.view.crosshair.w_axis;
        let tanks: Vec<Value> = sim
            .tanks
            .iter()
            .map(|tank| {
                let p = sim.tank_position(tank);
                json!({
                    "id": tank.id, "kind": tank.kind, "team": tank.team, "human": tank.human,
                    "alive": tank.alive, "x": p.x, "z": p.z, "hp": tank.hp,
                    "shown": self.view.tank_shown(tank.id),
                })
            })
            .collect();
        let fragments: Vec<Value> = self
            .state
            .fragments
            .iter()
            .take(64)
            .map(|f| {
                json!({
                    "id": f.id, "shape": f.shape, "wreck": f.wreck, "part": f.part,
                    "life": f.life, "createdAt": f.created_at, "y": f.position.y,
                })
            })
            .collect();
        json!({
            "fragmentViews": fragments,
            "seed": sim.seed,
            "phase": sim.match_state.phase,
            "match": sim.match_state,
            "elapsed": sim.elapsed,
            "mapMode": sim.map_mode,
            "mapName": sim.map_name(),
            "gameMode": sim.game_mode,
            "difficulty": sim.difficulty,
            "endlessMatch": sim.endless_match,
            "humanTeam": sim.human_team,
            "humanKind": sim.human_kind,
            "shotsFired": sim.shots_fired,
            "combatRecord": sim.combat_record,
            "shots": sim.shots.len(),
            "fragments": sim.fragments.len(),
            "bodies": sim.world.bodies.len(),
            "autoplay": self.autoplay,
            "overview": self.overview,
            "completedRounds": self.completed_rounds,
            "prepared": self.prepared,
            "human": {
                "id": human.id, "kind": human.kind, "team": human.team, "alive": human.alive,
                "x": position.x, "y": position.y, "z": position.z,
                "aim": human.aim, "heading": human.heading, "hp": human.hp,
                "kills": human.kills, "deaths": human.deaths, "cooldown": human.cooldown,
                "mineCooldown": human.mine_cooldown, "selectedAmmo": human.selected_ammo,
                "ammo": human.ammo, "xp": human.xp,
            },
            "tanks": tanks,
            "view": {
                "zoom": rig.zoom,
                "minZoom": CAMERA.min_zoom,
                "maxZoom": CAMERA.max_zoom,
                "time": self.view.time,
                "inFirstPerson": rig.in_first_person,
                "seatWanted": rig.seat_wanted,
                "seatBlend": rig.seat_blend,
                "firstPerson": { "enabled": rig.first_person.enabled, "yaw": rig.first_person.yaw },
                "camera": {
                    "fov": camera.fov_y_degrees,
                    "position": [camera.position.x, camera.position.y, camera.position.z],
                    "target": [camera.target.x, camera.target.y, camera.target.z],
                },
                "crosshair": {
                    "visible": self.view.crosshair_visible,
                    "position": [crosshair.x, crosshair.y, crosshair.z],
                },
                "aim": [rig.aim_point.x, rig.aim_point.y, rig.aim_point.z],
                "wreckView": rig.wreck_view,
                "canvas": self.view.renderer.size(),
            },
        })
        .to_string()
    }

    pub fn debug_snapshot(&self) -> String {
        serde_json::to_string(&self.sim.snapshot()).unwrap_or_default()
    }

    pub fn debug_set_autoplay(&mut self, value: bool) -> bool {
        self.autoplay = value;
        value
    }

    pub fn debug_set_overview(&mut self, value: bool) {
        self.overview = value;
    }

    pub fn debug_set_auto_rounds(&mut self, value: bool) -> bool {
        self.auto_rounds = value;
        value
    }

    pub fn debug_set_zoom(&mut self, zoom: f64) -> f64 {
        self.view.rig.set_zoom(zoom);
        self.view.rig.zoom
    }

    /// Sky reflection strength (0 off) until the next map's theme resets it,
    /// for comparing the lighting within one page.
    pub fn debug_set_reflections(&mut self, strength: f32) {
        let mut environment = *self.view.renderer.environment();
        environment.reflections = strength.max(0.0);
        self.view.renderer.set_environment(environment);
    }

    /// Topple every tower and drum.
    pub fn debug_collapse(&mut self) {
        let human = self.sim.human();
        let (id, team) = (human.id, self.sim.human_team);
        for index in 0..self.sim.covers.len() {
            let kind = self.sim.covers[index].kind;
            if matches!(kind, CoverKind::Tower | CoverKind::Drum) && self.sim.covers[index].alive {
                self.sim.damage_cover(index, 999.0, id, team, None, None);
            }
        }
    }

    /// The stress scene: 24 tanks, full debris and 200 shells.
    pub fn debug_stress(&mut self) {
        self.sim.reset(Some(24));
        self.reset_view();
        self.sim.start();
        self.active = true;
        self.autoplay = true;
        self.stress_debris_and_shells(3.0);
    }

    /// Stock every special ammunition with `count` rounds (HUD and input checks).
    pub fn debug_give_ammo(&mut self, count: f64) {
        let index = self.human_index();
        let ammo = &mut self.sim.tanks[index].ammo;
        ammo.spread = count;
        ammo.rocket = count;
        ammo.ricochet = count;
        ammo.piercing = count;
    }

    /// Destroy the human's tank as an enemy kill (death, respawn and pointer checks).
    pub fn debug_kill_human(&mut self) {
        let index = self.human_index();
        let tank = &mut self.sim.tanks[index];
        tank.protection = 0.0;
        tank.shield = 0.0;
        let (team, body) = (tank.team, tank.body);
        // The nearest living enemy fires a standard shell, so the kill reports its
        // weapon and direction like a real one (the respawn card and feed show both).
        let at = self.sim.body_translation(body).planar();
        let enemy = self
            .sim
            .tanks
            .iter()
            .filter(|other| other.team != team && other.alive)
            .map(|other| (other, self.sim.body_translation(other.body).planar()))
            .min_by(|(_, a), (_, b)| {
                (a.x - at.x)
                    .hypot(a.z - at.z)
                    .total_cmp(&(b.x - at.x).hypot(b.z - at.z))
            });
        let (owner, owner_team, origin) = enemy.map_or(
            (0, Team::from_index(team.index() + 1), at),
            |(other, position)| (other.id, other.team, position),
        );
        let source = DamageSource {
            cause: DamageCause::Standard,
            origin,
        };
        self.sim
            .damage_tank(index, 1e12, owner, owner_team, None, Some(source));
    }

    /// Profiling setup: the seed, tank count and human team the next reset uses.
    pub fn debug_configure(&mut self, seed: f64, tank_count: usize, human_team: u8) {
        self.sim.seed = seed;
        self.sim.round_count = tank_count;
        self.sim.human_team = Team::from_index(usize::from(human_team));
    }

    /// The profiling stress burst: refill debris and a ring of 200 shells, and drop
    /// three drums on the centre line, blowing up the last.
    pub fn debug_stress_burst(&mut self) {
        self.stress_debris_and_shells(15.0);
        let (id, team) = (self.sim.human().id, self.sim.human_team);
        for x in [-5.0, 0.0, 5.0] {
            let def = CoverDef::new(CoverKind::Drum, x, 0.0, 1.2, 1.2, 1.7, 30.0, 0xe3854d);
            let index = self.sim.add_cover(&def);
            if x == 5.0 {
                self.sim.damage_cover(index, 999.0, id, team, None, None);
            }
        }
    }

    /// Simulate `seconds` of autoplay as fast as possible, resetting finished
    /// rounds; returns `{ simulatedSeconds, resets, snapshot }`.
    pub fn debug_soak(&mut self, seconds: f64) -> String {
        let steps = (seconds / STEP) as u64;
        let mut resets = 0;
        for _ in 0..steps {
            if self.sim.match_state.phase != MatchPhase::Playing {
                self.sim.reset(None);
                self.reset_view();
                self.sim.start();
                resets += 1;
            }
            self.sim.step(VehicleCommand::idle(), true);
        }
        self.sim.events.clear();
        json!({
            "simulatedSeconds": steps as f64 * STEP,
            "resets": resets,
            "snapshot": self.sim.snapshot(),
        })
        .to_string()
    }
}

/// The calls above that cross the boundary as JSON or packed numbers, typed for a Rust
/// shell (the desktop app); the page's getters serialize these same values.
impl Game {
    /// The pixels of a claimed bake, baked on the shell's threads or the page's workers.
    pub fn supply_pixels(&mut self, key: &str, rgba: &BakedPixels) -> Result<bool, PageError> {
        self.view.supply_texture(key, rgba).map_err(js_error)
    }

    /// The renderer, for the desktop shell's overlay and presentation.
    pub fn renderer(&mut self) -> &mut sloppy_render::gpu::Renderer {
        &mut self.view.renderer
    }

    /// `create` with the view and the game's configuration as values.
    pub async fn create_with(
        canvas: RenderTarget,
        view_config: ViewConfig,
        config: GameConfig,
    ) -> Result<Game, PageError> {
        let PageView {
            mut view,
            canvas,
            graphics,
            seed,
            pacing,
            shell,
        } = create_view(canvas, view_config).await?;
        let mut options =
            initial_game_options(seed, config.map, config.extra_levels, config.difficulty);
        if let Some(kind) = config.human_kind {
            options.human_kind = kind;
        }
        if let Some(team) = config.human_team {
            options.human_team = team;
        }
        if let Some(mode) = config.game_mode {
            options.game_mode = mode;
        }
        let options = options.offered(config.extra_levels);
        let setup = options_setup(&options).merged(SimulationSetup {
            round: Some(FIRST_ROUND),
            ..SimulationSetup::default()
        });
        let sim = Simulation::new(seed, setup);
        let state = sim.render_state(None);
        view.build_scenery(&state.map_theme);
        let mut game = Game {
            sim,
            view,
            state,
            options,
            commands: CommandBuilder::default(),
            steps: StepClock::default(),
            pacing,
            active: false,
            autoplay: config.autoplay,
            overview: false,
            auto_rounds: false,
            completed_rounds: 0,
            frame_index: 0,
            prepared: true,
            prepare_compiled: 0,
            loading: None,
            events: Vec::new(),
            sounds: SoundPlanner::default(),
            canvas,
            shell,
            times: FrameTimes::default(),
            graphics,
            hud_tank: RenderTank::default(),
            effect_timers: EffectTimers::default(),
            recap: None,
            last_result: vec![0.0; frame_slot::LENGTH],
        };
        game.canvas.apply(&mut game.view);
        game.reset_view();
        Ok(game)
    }

    /// Apply Battle Setup choices; rebuilds the arena when they changed and returns
    /// whether it did.
    pub fn set_game_options(&mut self, options: &GameOptions) -> bool {
        let options = *options;
        if options == self.options && game_choices(&self.sim) == options {
            return false;
        }
        self.options = options;
        options_setup(&options).apply(&mut self.sim);
        self.sim.reset(None);
        self.commands.clear();
        self.reset_view();
        true
    }

    /// Compile up to `budget` pipelines; once nothing remains, warm every variant and
    /// draw the first frames.
    pub fn prepare(&mut self, budget: u32) -> Result<PrepareStatus, PageError> {
        if self.prepared {
            return Ok(PREPARED);
        }
        let status = self.view.prepare_step(budget.max(1)).map_err(js_error)?;
        self.prepare_compiled += status.compiled;
        self.loading = if status.ready {
            None
        } else {
            Loading::of(
                self.prepare_compiled,
                status.remaining,
                status.textures_pending,
                status.gpu_pending,
            )
        };
        if status.ready {
            self.fill_state();
            self.view.finish_prepare(&self.state).map_err(js_error)?;
            self.prepared = true;
        }
        Ok(status)
    }

    /// Everything the HUD and menus show. The first read of a result saves personal
    /// bests and reports them in the recap.
    pub fn hud(&mut self) -> Hud<'_> {
        if self.sim.match_state.phase == MatchPhase::Results && self.recap.is_none() {
            self.recap = Some(round_recap(
                &self.sim,
                &mut BrowserStorage::local(),
                self.shell,
            ));
        }
        let recap = if self.sim.match_state.phase == MatchPhase::Results {
            self.recap.as_ref()
        } else {
            None
        };
        simulation_hud(
            &self.sim,
            &mut self.hud_tank,
            recap,
            &mut self.effect_timers,
        )
    }

    /// Battle Setup's line for the latest `prepare` step: `None` once the arena is
    /// ready, and while a rebuild that reused everything only warms up.
    pub fn loading(&self) -> Option<Loading> {
        self.loading
    }

    /// Pending events for audio and HUD, heard from the human's tank.
    pub fn take_events(&mut self) -> EventBatch {
        let (x, z) = self.human_position();
        let right = self.view.rig.listener_right;
        take_events(&mut self.events, (x, z), right, &mut self.sounds)
    }

    /// The chosen view and clamped overhead zoom.
    pub fn camera(&self) -> CameraPreferences {
        CameraPreferences::of(&self.view.rig)
    }

    /// The Settings dialog's Graphics choice.
    pub fn set_graphics_setting(&mut self, setting: GraphicsSetting) {
        self.graphics.set_setting(&mut self.view.renderer, setting);
    }

    /// The tank the human respawns in, chosen from the respawn menu mid-round. The world
    /// is kept; Battle Setup's next `set_options` sees the same choice.
    pub fn respawn_in(&mut self, kind: VehicleKind) -> Result<(), PageError> {
        if kind == VehicleKind::Humvee {
            return Err(js_error("The player cannot drive a Humvee"));
        }
        self.sim.human_kind = kind;
        self.options.human_kind = kind;
        Ok(())
    }

    /// A speed scale; returns the scale in effect.
    pub fn set_speed_scale(&mut self, key: SpeedSetting, value: f64) -> f64 {
        tune_speed(&mut self.sim, key, value)
    }

    /// Stats for nerds' sections and rows, read from the game now.
    pub fn nerd_stats(&mut self) -> Vec<StatsSection> {
        let particles = self.view.effects.systems.stats().particles;
        let sim = &self.sim;
        let (fixed, dynamic, sleeping) = body_counts(sim);
        let count = |count: usize| count as u64;
        let stats = GameStats {
            frame: frame_readings(&self.times),
            renderer: renderer_readings(
                &self.view.renderer,
                &self.graphics,
                self.canvas.pixel_ratio(),
            ),
            scene: SceneStats {
                tanks: count(sim.tanks.len()),
                alive: count(sim.tanks.iter().filter(|tank| tank.alive).count()),
                mines: count(sim.mines.len()),
                pickups: count(sim.pickups.len()),
                pickups_ready: count(sim.pickups.iter().filter(|pickup| pickup.available).count()),
                shots: count(sim.shots.len()),
                fragments: count(sim.fragments.len()),
                elapsed: sim.elapsed,
            },
            bodies: count(sim.world.bodies.len()),
            fixed_bodies: count(fixed),
            dynamic_bodies: count(dynamic),
            sleeping_bodies: count(sleeping),
            colliders: count(sim.world.colliders.len()),
            particles: particles.into(),
            max_fragments: count(sim.max_fragments),
        };
        game_sections(&stats, self.shell)
    }

    /// The GPU's smoothed frame time in milliseconds, once sampled.
    pub fn gpu_ms(&self) -> Option<f64> {
        self.graphics.gpu_ms()
    }
}

impl Game {
    fn human_index(&self) -> usize {
        self.sim.human_index().unwrap_or(0)
    }

    /// Where an autoplaying human's gun points: along the turret at the distance of
    /// the enemy its brain is fighting, or a typical range without one. `None`
    /// outside autoplay or while destroyed.
    fn autoplay_aim_point(&self) -> Option<glam::Vec3> {
        let tank = self.sim.human();
        if !self.autoplay || !tank.alive {
            return None;
        }
        let (x, z) = self.human_position();
        let range = self
            .sim
            .tanks
            .iter()
            .find(|enemy| enemy.alive && enemy.id == tank.brain.target)
            .map_or(AUTOPLAY_AIM_RANGE, |enemy| {
                let p = self.sim.tank_position(enemy);
                (p.x - x)
                    .hypot(p.z - z)
                    .clamp(AUTOPLAY_MIN_AIM_RANGE, AUTOPLAY_MAX_AIM_RANGE)
            });
        Some(glam::Vec3::new(
            (x + tank.aim.sin() * range) as f32,
            0.0,
            (z + tank.aim.cos() * range) as f32,
        ))
    }

    /// The listener and aim origin: the body, or where the tank died.
    fn human_position(&self) -> (f64, f64) {
        let p = self.sim.tank_position(self.sim.human());
        (p.x, p.z)
    }

    fn fill_state(&mut self) {
        self.sim.fill_render_state(&mut self.state, None);
    }

    /// Refill debris to the fragment cap and the shells to a ring of 200 of `radius`
    /// metres around the centre (the stress scenes).
    fn stress_debris_and_shells(&mut self, radius: f64) {
        while self.sim.fragments.len() < self.sim.max_fragments {
            let x = self.sim.rng.range(-15.0, 15.0);
            let z = self.sim.rng.range(-15.0, 15.0);
            self.sim
                .fragment(x, z, 0xc5a978, 0.5, FragmentShape::Shard, 1.0);
        }
        let owners: Vec<u32> = self.sim.tanks.iter().map(|tank| tank.id).collect();
        for i in self.sim.shots.len()..200 {
            let angle = i as f64 * std::f64::consts::TAU / 200.0;
            let id = self.sim.allocate_id();
            self.sim.shots.push(Shot {
                id,
                x: angle.sin() * radius,
                z: angle.cos() * radius,
                vx: angle.cos() * 45.0,
                vz: angle.sin() * 45.0,
                owner: owners[i % owners.len()],
                team: Team::from_index(i),
                damage: 40.0,
                bounces: 4,
                life: 4.0,
                weapon: Weapon::Standard,
                ..Shot::default()
            });
        }
    }

    /// Rebuild the presentation for the simulation's current world and begin
    /// preparing it.
    fn reset_view(&mut self) {
        self.fill_state();
        self.view.reset(&self.state);
        self.sim.set_wreck_view(Some(self.view.wreck_view()));
        self.view.begin_prepare(&self.state);
        self.prepared = false;
        self.prepare_compiled = 0;
        self.loading = None;
        self.recap = None;
        self.events.clear();
        self.steps.reset();
    }

    fn route_events(&mut self) {
        if self.sim.events.is_empty() {
            return;
        }
        let human = self.sim.human();
        let (human_id, human_team) = (human.id, human.team);
        let mut events = std::mem::take(&mut self.sim.events);
        for event in events.drain(..) {
            let player_hit = matches!(event.kind, SimEventType::Hurt | SimEventType::Death)
                && event.owner == Some(human_id)
                && event.team != Some(human_team);
            let own = event.id == Some(human_id);
            let scoreboard = Scoreboard::Simulated(&self.sim.tanks);
            let feedback = event_feedback(&event, own, human_id, &scoreboard);
            queue_event(
                &mut self.events,
                &mut self.view,
                event,
                player_hit,
                own,
                feedback,
            );
        }
        // Keep the simulation's allocation for the next tick.
        if self.sim.events.is_empty() {
            self.sim.events = events;
        }
    }
}
