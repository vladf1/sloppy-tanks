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
//!   cssWidth?, cssHeight?, pixelRatio?, firstPerson?, zoom?, hideReticle? }`. Choices follow `initialGameOptions`; the arena is reset and
//!   preparation begins.
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
//! - `start()`: begin the round (GO, PLAY AGAIN). `resume()`: continue a pause.
//!   `pause()`: pause a playing round. `restart()`: a fresh world for Battle
//!   Setup (phase `ready`; prepare again). `end_battle()`: END BATTLE.
//! - `frame(now_ms, input) -> Float32Array` (see [`frame_slot`]): one call per
//!   animation frame. Input is the packed raw control state of
//!   `sloppy_render::presentation::input::slot` (keys, touch sticks, pointer NDC,
//!   fire, one-shot mine/ammo presses since the last frame, look pixels, zoom,
//!   view toggle, wheel ammo steps).
//! - `drain_events() -> JSON`: `{ listener: {x, z}, listenerRight: {x, z},
//!   events: [SimEvent & { playerHit, own, damageAngle }] }` for audio and HUD;
//!   `damageAngle` is the clockwise screen angle (radians) of damage the viewer
//!   took, or null.
//! - `hud_json() -> JSON`: match, scores, score limit, team names, clock, the
//!   human's stats, ammo, rank, cooldowns, effects, scoreboard, and the round
//!   recap once results show. The first recap of a result saves personal bests
//!   in `localStorage` and reports them (`best`, `improved`, `persisted`).
//! - `stats_json() -> JSON`: Stats for nerds (frame/sim/render ms, draw calls,
//!   triangles, GPU resources, bodies and their sleep state, shots, pickups,
//!   fragments, particles, pixel ratio).
//! - `resize(css_width, css_height, pixel_ratio, exact)`: the drawing buffer is
//!   the CSS size times the pixel ratio, capped at 1.5; `exact` draws at exactly the
//!   CSS size (ratio 1).
//! - `toggle_first_person() -> bool`: V / the view button while playing.
//! - `camera_preferences() -> Float64Array [firstPerson, zoom]`: the chosen view
//!   and clamped overhead zoom, for the page to save after a camera input.
//! - `set_human_kind(kind)`: the respawn menu's tank, keeping the world.
//! - `set_speed(key, value) -> f64`: "tank-speed" or "bullet-speed" scale.
//! - `debug_*`: the dev `window.sloppy` hooks below and the fixture hooks of
//!   `game/debug.rs`.
//! - `error() -> string | undefined`: the first GPU error, if any.

use crate::events::{PendingEvent, drain_events};
use crate::hud::{HudHuman, Scoreboard, human_json};
use crate::page::{
    CanvasSize, FrameTimes, HUD_UPDATE_EVERY_FRAMES, PREPARED, aim, create_view, js_error, now_ms,
    parse, prepare_result, queue_event,
};
use crate::stats::presentation_stats;

use glam::Vec2;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::data::{SCORE_LIMIT, STEP, TEAM_NAMES};
use sloppy_core::sim::difficulty::Difficulty;
use sloppy_core::sim::extra_levels::extra_level;
use sloppy_core::sim::game_options::{GameOptions, game_choices, initial_game_options};
use sloppy_core::sim::level_rules::{single_player_rules, standard_rules};
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::match_state::end_battle;
use sloppy_core::sim::render_state::RenderTank;
use sloppy_core::sim::round_recap::{
    Metric, RecordStorage, StorageUnavailable, combat_feats, recap_stats, save_personal_bests,
};
use sloppy_core::sim::simulation::SpeedTuning;
use sloppy_core::sim::speed_tuning::{SpeedSetting, tune_speed};
use sloppy_core::sim::veterancy::RANKS;
use sloppy_core::sim::{
    CoverKind, FragmentShape, GameMode, Match, MatchPhase, RenderState, Shot, SimEventType,
    Simulation, SimulationSetup, Team, VehicleCommand, VehicleKind, Weapon,
};
use sloppy_render::presentation::Presentation;
use sloppy_render::presentation::input::{CommandBuilder, InputFrame};
use sloppy_render::presentation::view_settings::CAMERA;
use wasm_bindgen::prelude::*;

mod debug;

/// Catch-up after a stall is bounded so one slow frame cannot spiral.
const MAX_CATCH_UP_STEPS: u32 = 5;
/// The round the browser's first arena prepares (it seeds bot names).
const FIRST_ROUND: u32 = 3;

/// Slots of `frame`'s result.
pub mod frame_slot {
    /// 0 ready, 1 playing, 2 paused, 3 results.
    pub const PHASE: usize = 0;
    pub const HUMAN_ALIVE: usize = 1;
    /// Show the first-person cockpit HUD (from the start of the flight in).
    pub const COCKPIT: usize = 2;
    /// The hull's clockwise screen angle with the turret straight up (compass).
    pub const HULL_ANGLE: usize = 3;
    /// Events waiting for `drain_events`.
    pub const EVENTS: usize = 4;
    /// 1 when the page should drop held input (paused, dead, results).
    pub const CLEAR_INPUT: usize = 5;
    /// 1 on frames the HUD should refresh (`hud_json`).
    pub const HUD_DUE: usize = 6;
    pub const SIM_MS: usize = 7;
    pub const RENDER_MS: usize = 8;
    /// 1 while first person is enabled (the page captures the pointer).
    pub const FIRST_PERSON: usize = 9;
    /// This frame's capped delta in seconds (the HUD advances its timers by it).
    pub const DT: usize = 10;
    pub const LENGTH: usize = 11;
}

/// `Game.hud_json`: the page HUD's view of the match and the human's tank.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Hud<'a> {
    #[serde(rename = "match")]
    match_state: &'a Match,
    elapsed: f64,
    game_mode: GameMode,
    endless_match: bool,
    map_mode: MapId,
    map_name: String,
    difficulty: Difficulty,
    human_team: Team,
    score_limit: u32,
    team_names: [&'static str; 2],
    active_enemies: usize,
    speed_tuning: &'a SpeedTuning,
    human: HudHuman,
    scoreboard: Scoreboard<'a>,
    recap: Option<&'a Value>,
}

fn phase_code(phase: MatchPhase) -> f32 {
    match phase {
        MatchPhase::Ready => 0.0,
        MatchPhase::Playing => 1.0,
        MatchPhase::Paused => 2.0,
        MatchPhase::Results => 3.0,
    }
}

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

/// `Game.create`'s choices besides the view configuration (`page::create_view`).
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct GameConfig {
    map: Option<String>,
    difficulty: Option<String>,
    extra_levels: bool,
    human_kind: Option<VehicleKind>,
    human_team: Option<Team>,
    game_mode: Option<GameMode>,
    autoplay: bool,
}

/// Personal bests live in the page's `localStorage`; private browsing or a full
/// quota only makes them session-only.
struct BrowserStorage(Option<web_sys::Storage>);

impl BrowserStorage {
    fn local() -> Self {
        Self(web_sys::window().and_then(|window| window.local_storage().ok().flatten()))
    }
}

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

#[wasm_bindgen]
pub struct Game {
    sim: Simulation,
    view: Presentation,
    state: RenderState,
    options: GameOptions,
    commands: CommandBuilder,
    accumulator: f64,
    active: bool,
    autoplay: bool,
    overview: bool,
    auto_rounds: bool,
    completed_rounds: u32,
    frame_index: u64,
    /// Warmed and first frames drawn; false while pipelines compile.
    prepared: bool,
    events: Vec<PendingEvent>,
    canvas: CanvasSize,
    times: FrameTimes,
    /// The human's tank as the HUD reads it, refilled from the simulation each read.
    hud_tank: RenderTank,
    /// The finished round's recap; personal bests are saved once per result.
    recap: Option<Value>,
}

#[wasm_bindgen]
impl Game {
    /// Create the renderer on `canvas`, build the chosen arena and begin
    /// preparing it. Rejects when the renderer cannot start.
    pub async fn create(
        canvas: web_sys::HtmlCanvasElement,
        config_json: &str,
    ) -> Result<Game, JsValue> {
        console_error_panic_hook::set_once();
        let config: GameConfig = parse(config_json)?;
        let (mut view, canvas, seed) = create_view(canvas, config_json).await?;
        let mut options = initial_game_options(
            seed,
            config.map.as_deref(),
            config.extra_levels,
            config.difficulty.as_deref(),
        );
        if let Some(kind) = config.human_kind {
            options.human_kind = kind;
        }
        if let Some(team) = config.human_team {
            options.human_team = team;
        }
        if let Some(mode) = config.game_mode {
            options.game_mode = mode;
        }
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
            accumulator: 0.0,
            active: false,
            autoplay: config.autoplay,
            overview: false,
            auto_rounds: false,
            completed_rounds: 0,
            frame_index: 0,
            prepared: true,
            events: Vec::new(),
            canvas,
            times: FrameTimes::default(),
            hud_tank: RenderTank::default(),
            recap: None,
        };
        game.canvas.apply(&mut game.view);
        game.reset_view();
        Ok(game)
    }

    // ---------------------------------------------------------- lifecycle

    /// Apply Battle Setup choices; rebuilds the arena when they changed.
    pub fn set_options(&mut self, options_json: &str) -> Result<bool, JsValue> {
        let options: GameOptions = parse(options_json)?;
        if options == self.options && game_choices(&self.sim) == options {
            return Ok(false);
        }
        self.options = options;
        options_setup(&options).apply(&mut self.sim);
        self.sim.reset(None);
        self.commands.clear();
        self.reset_view();
        Ok(true)
    }

    /// Compile up to `budget` pipelines;
    /// `[compiled, remaining, texturesPending, done, gpuPending]`.
    pub fn prepare_step(&mut self, budget: u32) -> Result<Vec<f64>, JsValue> {
        if self.prepared {
            return Ok(PREPARED.to_vec());
        }
        let status = self.view.prepare_step(budget.max(1)).map_err(js_error)?;
        if status.ready {
            self.fill_state();
            self.view.finish_prepare(&self.state).map_err(js_error)?;
            self.prepared = true;
        }
        Ok(prepare_result(&status))
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

    /// Begin the round: GO, or PLAY AGAIN after results.
    pub fn start(&mut self) {
        if self.sim.match_state.phase == MatchPhase::Results {
            self.sim.reset(None);
            self.reset_view();
        }
        self.commands.clear();
        self.sim.start();
        self.active = true;
        self.accumulator = 0.0;
        // Animation frames queued before GO (behind a slow arena rebuild) carry older
        // timestamps; measuring from now keeps them from advancing the new round.
        self.times.last_ms = Some(now_ms());
    }

    pub fn resume(&mut self) {
        if self.sim.match_state.phase == MatchPhase::Paused {
            self.commands.clear();
            self.sim.start();
            self.accumulator = 0.0;
        }
    }

    pub fn pause(&mut self) {
        if self.sim.match_state.phase == MatchPhase::Playing {
            self.sim.match_state.phase = MatchPhase::Paused;
            self.commands.clear();
            self.accumulator = 0.0;
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
    pub fn frame(&mut self, now: f64, input: &[f32]) -> Result<Vec<f32>, JsValue> {
        let input = InputFrame::from_slice(input);
        let dt = self.times.advance(now);
        let mut result = vec![0.0; frame_slot::LENGTH];
        if !self.active {
            result[frame_slot::PHASE] = phase_code(self.sim.match_state.phase);
            return Ok(result);
        }
        if self.sim.match_state.phase == MatchPhase::Results && self.auto_rounds {
            self.commands.clear();
            self.completed_rounds += 1;
            self.sim.reset(None);
            self.reset_view();
            self.sim.start();
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
            self.accumulator = (self.accumulator + dt).min(STEP * f64::from(MAX_CATCH_UP_STEPS));
            let position = self.human_position();
            let (angle, _) = aim(&mut self.view.rig, &input, position, dt, alive);
            let mut steps = 0;
            while self.accumulator >= STEP && steps < MAX_CATCH_UP_STEPS {
                let active = self.sim.match_state.phase == MatchPhase::Playing
                    && self.sim.tanks[human].alive;
                let command = self.commands.command(&input, angle, active);
                let command = self.view.rig.first_person.steer(command);
                self.sim.set_wreck_view(Some(self.view.wreck_view()));
                self.sim.step(command, self.autoplay);
                self.accumulator -= STEP;
                steps += 1;
            }
        } else {
            self.accumulator = 0.0;
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
                self.accumulator / STEP
            } else {
                1.0
            };
            self.view
                .render(&self.state, alpha, dt, self.overview)
                .map_err(js_error)?;
            self.sim.set_wreck_view(Some(self.view.wreck_view()));
        }
        self.times.render_ms = now_ms() - render_start;
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
        self.frame_index += 1;
        Ok(result)
    }

    /// Pending events for audio and HUD, as JSON.
    pub fn drain_events(&mut self) -> String {
        let (x, z) = self.human_position();
        drain_events(&mut self.events, (x, z), self.view.rig.listener_right)
    }

    /// Everything the page HUD and menus show, as JSON.
    pub fn hud_json(&mut self) -> String {
        if self.sim.match_state.phase == MatchPhase::Results && self.recap.is_none() {
            self.recap = Some(self.finish_recap());
        }
        let sim = &self.sim;
        // From the simulation, not the last frame's state: debug hooks change tanks
        // between frames.
        sim.fill_tank(&mut self.hud_tank, sim.human());
        let recap = if sim.match_state.phase == MatchPhase::Results {
            self.recap.as_ref()
        } else {
            None
        };
        let hud = Hud {
            match_state: &sim.match_state,
            elapsed: sim.elapsed,
            game_mode: sim.game_mode,
            endless_match: sim.endless_match,
            map_mode: sim.map_mode,
            map_name: sim.map_name(),
            difficulty: sim.difficulty,
            human_team: sim.human_team,
            score_limit: SCORE_LIMIT,
            team_names: TEAM_NAMES,
            active_enemies: sim.tanks.iter().filter(|t| !t.human && t.alive).count(),
            speed_tuning: &sim.speed_tuning,
            human: human_json(&self.hud_tank, sim.elapsed),
            scoreboard: Scoreboard::Simulated(&sim.tanks),
            recap,
        };
        // Written straight to text: the page reads this every few frames.
        serde_json::to_string(&hud).unwrap_or_default()
    }

    /// Stats for nerds, as JSON.
    pub fn stats_json(&mut self) -> String {
        let effects = self.view.effects.systems.stats();
        let mut stats = presentation_stats(&self.view, &self.times);
        let sim = &self.sim;
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
        let simulation = json!({
            "fixedBodies": fixed,
            "dynamicBodies": dynamic,
            "sleepingBodies": sleeping,
            "tanksAlive": sim.tanks.iter().filter(|tank| tank.alive).count(),
            "pickups": sim.pickups.len(),
            "pickupsReady": sim.pickups.iter().filter(|pickup| pickup.available).count(),
            "maxFragments": sim.max_fragments,
            "particles": effects.particles,
            "elapsed": sim.elapsed,
            "pixelRatio": self.canvas.pixel_ratio(),
            "bodies": sim.world.bodies.len(),
            "colliders": sim.world.colliders.len(),
            "shots": sim.shots.len(),
            "mines": sim.mines.len(),
            "fragments": sim.fragments.len(),
            "covers": sim.covers.iter().filter(|cover| cover.alive).count(),
            "tanks": sim.tanks.len(),
        });
        if let Value::Object(rows) = simulation {
            stats.extend(rows);
        }
        Value::Object(stats).to_string()
    }

    /// The canvas's CSS size and the device pixel ratio.
    pub fn resize(&mut self, css_width: f64, css_height: f64, pixel_ratio: f64, exact: bool) {
        self.canvas = CanvasSize {
            css: Vec2::new(css_width as f32, css_height as f32),
            device_pixel_ratio: pixel_ratio,
            exact,
        };
        self.canvas.apply(&mut self.view);
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

    /// The tank the human respawns in, chosen from the respawn menu mid-round. The
    /// world is kept; Battle Setup's next `set_options` sees the same choice.
    pub fn set_human_kind(&mut self, kind: &str) -> Result<(), JsValue> {
        let kind: VehicleKind = serde_json::from_value(Value::from(kind))
            .map_err(|error| js_error(error.to_string()))?;
        if kind == VehicleKind::Humvee {
            return Err(js_error("The player cannot drive a Humvee"));
        }
        self.sim.human_kind = kind;
        self.options.human_kind = kind;
        Ok(())
    }

    /// "tank-speed" or "bullet-speed"; returns the scale in effect.
    pub fn set_speed(&mut self, key: &str, value: f64) -> f64 {
        let key = match key {
            "tank-speed" => SpeedSetting::TankSpeed,
            "bullet-speed" => SpeedSetting::BulletSpeed,
            _ => return 1.0,
        };
        tune_speed(&mut self.sim, key, value)
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
        let team = tank.team;
        let enemy = self.sim.tanks.iter().find(|other| other.team != team);
        let (owner, owner_team) = enemy.map_or((0, Team::from_index(team.index() + 1)), |other| {
            (other.id, other.team)
        });
        self.sim
            .damage_tank(index, 1e12, owner, owner_team, None, None);
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

impl Game {
    fn human_index(&self) -> usize {
        self.sim.human_index().unwrap_or(0)
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

    /// The results screen's recap, saving personal bests for this mode, map and
    /// difficulty once.
    fn finish_recap(&self) -> Value {
        let sim = &self.sim;
        let stats = recap_stats(sim);
        let combat = &sim.combat_record;
        let label = |value: Value| value.as_str().map(str::to_owned).unwrap_or_default();
        let key = format!(
            "sloppy-records-v1:{}:{}:{}",
            label(json!(sim.game_mode)),
            sim.map_name(),
            label(json!(sim.difficulty)),
        );
        let records = save_personal_bests(&mut BrowserStorage::local(), &key, &stats);
        let metrics = |values: &sloppy_core::sim::round_recap::RecapStats| {
            Metric::ALL
                .iter()
                .map(|&metric| (metric.key().to_string(), Value::from(values.get(metric))))
                .collect::<serde_json::Map<String, Value>>()
        };
        json!({
            "stats": metrics(&stats),
            "best": metrics(&records.best),
            "improved": records.improved.iter().map(|metric| metric.key()).collect::<Vec<_>>(),
            "persisted": records.persisted,
            "feats": combat_feats(&stats, combat.shots, combat.direct_hits),
            "shots": combat.shots,
            "directHits": combat.direct_hits,
            "damageTaken": combat.damage_taken,
            "shieldAbsorbed": combat.shield_absorbed,
            "rankNames": RANKS.iter().map(|rank| rank.name).collect::<Vec<_>>(),
        })
    }

    /// Rebuild the presentation for the simulation's current world and begin
    /// preparing it.
    fn reset_view(&mut self) {
        self.fill_state();
        self.view.reset(&self.state);
        self.sim.set_wreck_view(Some(self.view.wreck_view()));
        self.view.begin_prepare(&self.state);
        self.prepared = false;
        self.recap = None;
        self.events.clear();
        self.accumulator = 0.0;
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
            queue_event(&mut self.events, &mut self.view, event, player_hit, own);
        }
        // Keep the simulation's allocation for the next tick.
        if self.sim.events.is_empty() {
            self.sim.events = events;
        }
    }
}
