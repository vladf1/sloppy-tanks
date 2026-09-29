//! `Game`: single-player Sloppy Tanks for the page. It owns the simulation, the
//! presentation and the renderer, and runs the fixed-step loop of `src/game.ts`.
//! The page makes one coarse call per frame; no per-entity calls cross the
//! Wasm boundary.
//!
//! # Lifecycle
//!
//! ```text
//! const game = await Game.create(canvas, JSON.stringify(config));
//! // Loading screen: compile pipelines and wait for textures, yielding between calls.
//! while (!(await nextTask(), game.prepare_step(4))[3]) {}
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
//!   lastMap?, difficulty?, extraLevels?, humanKind?, humanTeam?, gameMode?,
//!   autoplay?, cssWidth?, cssHeight?, pixelRatio? }`. Choices follow
//!   `initialGameOptions`; the arena is reset and preparation begins.
//! - `set_options(options_json) -> bool`: Battle Setup choices `{ humanKind,
//!   humanTeam, gameMode, mapMode, difficulty }` (`GameOptions`, camelCase). When
//!   they differ, the arena rebuilds with the map's level rules (extra levels
//!   play as an endless team battle) and needs `prepare_step` again. Returns
//!   whether it rebuilt.
//! - `prepare_step(budget) -> Float64Array [compiled, remaining, texturesPending,
//!   done]`: compiles up to `budget` pipelines per call; once nothing remains and
//!   textures have loaded it warms every variant, draws the first frames and
//!   reports `done = 1`.
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
//! - `hud_json() -> JSON`: match, scores, clock, the human's stats, ammo, rank,
//!   cooldowns, effects, scoreboard, and the round recap once results show.
//! - `stats_json() -> JSON`: Stats for nerds (frame/sim/render ms, draw calls,
//!   triangles, GPU resources, bodies, shots, fragments).
//! - `resize(css_width, css_height, pixel_ratio, exact)`: the drawing buffer is
//!   the CSS size times the pixel ratio, capped at 1.5 unless `exact`.
//! - `toggle_first_person() -> bool`: V / the view button while playing.
//! - `set_speed(key, value) -> f64`: "tank-speed" or "bullet-speed" scale.
//! - `debug_*`: the dev `window.sloppy` hooks (`debug_json`, `debug_snapshot`,
//!   `debug_set_autoplay`, `debug_set_overview`, `debug_set_auto_rounds`,
//!   `debug_set_zoom`, `debug_collapse`, `debug_stress`, `debug_soak`).
//! - `error() -> string | undefined`: the first GPU error, if any.

use glam::Vec2;
use serde::Deserialize;
use serde_json::{Value, json};
use sloppy_core::sim::ammunition::{AMMO_ORDER, equipped_weapon, has_ammo};
use sloppy_core::sim::data::{STEP, vehicle};
use sloppy_core::sim::extra_levels::extra_level;
use sloppy_core::sim::game_options::{GameOptions, game_choices, initial_game_options};
use sloppy_core::sim::level_rules::{single_player_rules, standard_rules};
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::match_state::end_battle;
use sloppy_core::sim::round_recap::{Metric, combat_feats, recap_stats};
use sloppy_core::sim::speed_tuning::{SpeedSetting, tune_speed};
use sloppy_core::sim::veterancy::{RANKS, REPAIR_DELAY, rank_index};
use sloppy_core::sim::{
    CoverKind, FragmentShape, GameMode, MatchPhase, RenderState, Shot, SimEvent, SimEventType,
    Simulation, SimulationSetup, Team, VehicleCommand, VehicleKind, Weapon,
};
use sloppy_render::gpu::{Renderer, RendererOptions};
use sloppy_render::presentation::hud::health_bar_state;
use sloppy_render::presentation::input::{CommandBuilder, InputFrame};
use sloppy_render::presentation::view_settings::CAMERA;
use sloppy_render::presentation::{PrepareStatus, Presentation};
use wasm_bindgen::prelude::*;

/// Frame deltas are capped so a stalled tab never fast-forwards the match.
const MAX_FRAME_DELTA_SECONDS: f64 = 0.1;
/// Catch-up after a stall is bounded so one slow frame cannot spiral.
const MAX_CATCH_UP_STEPS: u32 = 5;
const HUD_UPDATE_EVERY_FRAMES: u64 = 4;
const MILLISECONDS_PER_SECOND: f64 = 1000.0;
/// Events waiting for `drain_events`; the simulation caps each tick's queue too.
const MAX_PENDING_EVENTS: usize = 2048;
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
    pub const LENGTH: usize = 10;
}

fn js_error(message: impl Into<String>) -> JsValue {
    js_sys::Error::new(&message.into()).into()
}

fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|window| window.performance())
        .map_or(0.0, |performance| performance.now())
}

fn phase_code(phase: MatchPhase) -> f32 {
    match phase {
        MatchPhase::Ready => 0.0,
        MatchPhase::Playing => 1.0,
        MatchPhase::Paused => 2.0,
        MatchPhase::Results => 3.0,
    }
}

/// A map's level rules: extra levels bring their own arena and play endless.
fn level_rules(map: MapId) -> SimulationSetup {
    match extra_level(map) {
        Some(level) => single_player_rules(level),
        None => standard_rules(),
    }
}

fn options_setup(options: &GameOptions) -> SimulationSetup {
    SimulationSetup {
        human_kind: Some(options.human_kind),
        human_team: Some(options.human_team),
        game_mode: Some(options.game_mode),
        map_mode: Some(options.map_mode),
        difficulty: Some(options.difficulty),
        ..SimulationSetup::default()
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct GameConfig {
    seed: Option<f64>,
    asset_base: String,
    map: Option<String>,
    last_map: Option<String>,
    difficulty: Option<String>,
    extra_levels: bool,
    human_kind: Option<VehicleKind>,
    human_team: Option<Team>,
    game_mode: Option<GameMode>,
    autoplay: bool,
    css_width: Option<f64>,
    css_height: Option<f64>,
    pixel_ratio: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Preparation {
    /// Samples registered; pipelines compiling.
    Compiling,
    /// Warmed and first frames drawn.
    #[default]
    Done,
}

struct PendingEvent {
    event: SimEvent,
    player_hit: bool,
    own: bool,
    damage_angle: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default)]
struct FrameTimes {
    frame_ms: f64,
    sim_ms: f64,
    render_ms: f64,
    /// Exponential average of frame time for the fps readout.
    average_ms: f64,
}

#[wasm_bindgen]
pub struct Game {
    sim: Simulation,
    view: Presentation,
    state: RenderState,
    options: GameOptions,
    commands: CommandBuilder,
    accumulator: f64,
    last_ms: Option<f64>,
    active: bool,
    autoplay: bool,
    overview: bool,
    auto_rounds: bool,
    completed_rounds: u32,
    frame_index: u64,
    preparation: Preparation,
    events: Vec<PendingEvent>,
    client: Vec2,
    pixel_ratio: f64,
    exact: bool,
    times: FrameTimes,
}

#[wasm_bindgen]
impl Game {
    /// Create the renderer on `canvas`, build the chosen arena and begin
    /// preparing it. Rejects when WebGPU is unavailable.
    pub async fn create(
        canvas: web_sys::HtmlCanvasElement,
        config_json: &str,
    ) -> Result<Game, JsValue> {
        console_error_panic_hook::set_once();
        let config: GameConfig =
            serde_json::from_str(config_json).map_err(|error| js_error(error.to_string()))?;
        let seed = config
            .seed
            .unwrap_or_else(|| (js_sys::Math::random() * 1e9).floor());
        let mut options = initial_game_options(
            seed,
            config.map.as_deref(),
            config.extra_levels,
            config.difficulty.as_deref(),
            config.last_map.as_deref(),
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
        let client = Vec2::new(
            config.css_width.unwrap_or(f64::from(canvas.client_width())) as f32,
            config
                .css_height
                .unwrap_or(f64::from(canvas.client_height())) as f32,
        );
        let renderer = Renderer::new(
            canvas,
            RendererOptions {
                asset_base: config.asset_base,
            },
        )
        .await
        .map_err(js_error)?;
        let mut view = Presentation::new(renderer, seed.to_bits());
        let setup = options_setup(&options)
            .merged(level_rules(options.map_mode))
            .merged(SimulationSetup {
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
            last_ms: None,
            active: false,
            autoplay: config.autoplay,
            overview: false,
            auto_rounds: false,
            completed_rounds: 0,
            frame_index: 0,
            preparation: Preparation::Done,
            events: Vec::new(),
            client,
            pixel_ratio: config.pixel_ratio.unwrap_or(1.0),
            exact: false,
            times: FrameTimes::default(),
        };
        game.apply_size();
        game.reset_view();
        Ok(game)
    }

    // ---------------------------------------------------------- lifecycle

    /// Apply Battle Setup choices; rebuilds the arena when they changed.
    pub fn set_options(&mut self, options_json: &str) -> Result<bool, JsValue> {
        let options: GameOptions =
            serde_json::from_str(options_json).map_err(|error| js_error(error.to_string()))?;
        if options == self.options && game_choices(&self.sim) == options {
            return Ok(false);
        }
        self.options = options;
        options_setup(&options)
            .merged(level_rules(options.map_mode))
            .apply(&mut self.sim);
        self.sim.reset(None);
        self.commands.clear();
        self.reset_view();
        Ok(true)
    }

    /// Compile up to `budget` pipelines; `[compiled, remaining, texturesPending, done]`.
    pub fn prepare_step(&mut self, budget: u32) -> Result<Vec<f64>, JsValue> {
        if self.preparation == Preparation::Done {
            return Ok(vec![0.0, 0.0, 0.0, 1.0]);
        }
        let PrepareStatus {
            compiled,
            remaining,
            textures_pending,
        } = self.view.prepare_step(budget.max(1));
        let ready = remaining == 0 && textures_pending == 0;
        if ready {
            self.fill_state();
            self.view.finish_prepare(&self.state).map_err(js_error)?;
            self.preparation = Preparation::Done;
        }
        Ok(vec![
            f64::from(compiled),
            f64::from(remaining),
            f64::from(textures_pending),
            if ready { 1.0 } else { 0.0 },
        ])
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
        self.last_ms = None;
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
        self.accumulator = 0.0;
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
        // A queued timestamp can precede a slow rebuild; never run time backwards.
        let last = self.last_ms.unwrap_or(now);
        let raw = ((now - last) / MILLISECONDS_PER_SECOND).max(0.0);
        let dt = raw.min(MAX_FRAME_DELTA_SECONDS);
        self.last_ms = Some(last.max(now));
        self.times.frame_ms = raw * MILLISECONDS_PER_SECOND;
        if raw > 0.0 {
            self.times.average_ms += (self.times.frame_ms - self.times.average_ms) * 0.05;
        }
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
            let look = &mut self.view.rig.first_person;
            let angle = if look.enabled {
                if alive {
                    let stick = if input.aim_stick_held {
                        f64::from(input.touch_aim.0)
                    } else {
                        0.0
                    };
                    look.turn(input.look_pixels, stick, dt);
                }
                look.yaw
            } else {
                let aim = if input.touch_aiming {
                    self.view.rig.touch_aim(
                        glam::DVec3::new(position.0, 0.0, position.1),
                        Vec2::new(input.touch_aim.0, input.touch_aim.1),
                        self.client,
                    )
                } else {
                    self.view
                        .rig
                        .aim(Vec2::new(input.pointer.0, input.pointer.1))
                };
                (f64::from(aim.x) - position.0).atan2(f64::from(aim.z) - position.1)
            };
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
        self.frame_index += 1;
        Ok(result)
    }

    /// Pending events for audio and HUD, as JSON.
    pub fn drain_events(&mut self) -> String {
        let (x, z) = self.human_position();
        let (right_x, right_z) = self.view.rig.listener_right;
        let events: Vec<Value> = self
            .events
            .drain(..)
            .map(|pending| {
                let mut value = serde_json::to_value(&pending.event).unwrap_or(Value::Null);
                if let Value::Object(fields) = &mut value {
                    fields.insert("playerHit".into(), pending.player_hit.into());
                    fields.insert("own".into(), pending.own.into());
                    fields.insert(
                        "damageAngle".into(),
                        pending.damage_angle.map_or(Value::Null, Value::from),
                    );
                }
                value
            })
            .collect();
        json!({
            "listener": { "x": x, "z": z },
            "listenerRight": { "x": right_x, "z": right_z },
            "events": events,
        })
        .to_string()
    }

    /// Everything the page HUD and menus show, as JSON.
    pub fn hud_json(&self) -> String {
        let sim = &self.sim;
        let tank = sim.human();
        let max_hp = sim.max_health(tank);
        let rank = rank_index(tank.xp);
        let stats = &RANKS[rank];
        let health = health_bar_state(tank.hp, max_hp, tank.team);
        let selected = equipped_weapon(tank);
        let ammo: Vec<Value> = AMMO_ORDER
            .iter()
            .map(|&weapon| {
                json!({
                    "weapon": weapon,
                    "count": weapon.special().map(|kind| tank.ammo.get(kind)),
                    "selected": weapon == selected,
                    "available": has_ammo(tank, weapon),
                })
            })
            .collect();
        let self_repair = tank.alive
            && stats.repair > 0.0
            && tank.hp < max_hp
            && sim.elapsed - tank.last_combat >= REPAIR_DELAY;
        let scoreboard: Vec<Value> = sim
            .tanks
            .iter()
            .map(|t| {
                json!({
                    "id": t.id, "name": t.name, "team": t.team, "kind": t.kind,
                    "human": t.human, "alive": t.alive, "kills": t.kills,
                    "deaths": t.deaths, "rank": rank_index(t.xp),
                })
            })
            .collect();
        let recap = (sim.match_state.phase == MatchPhase::Results).then(|| {
            let stats = recap_stats(sim);
            let combat = &sim.combat_record;
            let metrics: serde_json::Map<String, Value> = Metric::ALL
                .iter()
                .map(|&metric| (metric.key().to_string(), Value::from(stats.get(metric))))
                .collect();
            json!({
                "stats": metrics,
                "feats": combat_feats(&stats, combat.shots, combat.direct_hits),
                "shots": combat.shots,
                "directHits": combat.direct_hits,
                "damageTaken": combat.damage_taken,
                "shieldAbsorbed": combat.shield_absorbed,
                "recordsKey": format!(
                    "sloppy-records-v1:{}:{}:{}",
                    serde_json::to_value(sim.game_mode).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default(),
                    sim.map_name(),
                    serde_json::to_value(sim.difficulty).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default(),
                ),
            })
        });
        json!({
            "match": sim.match_state,
            "elapsed": sim.elapsed,
            "gameMode": sim.game_mode,
            "endlessMatch": sim.endless_match,
            "mapMode": sim.map_mode,
            "mapName": sim.map_name(),
            "difficulty": sim.difficulty,
            "humanTeam": sim.human_team,
            "activeEnemies": sim.tanks.iter().filter(|t| !t.human && t.alive).count(),
            "speedTuning": sim.speed_tuning,
            "human": {
                "id": tank.id,
                "name": tank.name,
                "kind": tank.kind,
                "vehicleName": vehicle(tank.kind).name,
                "team": tank.team,
                "alive": tank.alive,
                "hp": tank.hp,
                "maxHp": max_hp,
                "healthRatio": health.ratio,
                "healthColor": health.color,
                "xp": tank.xp,
                "rank": rank,
                "rankName": stats.name,
                "rankDamage": stats.damage,
                "rankFireRate": stats.fire_rate,
                "rankHealth": stats.health,
                "rankRepair": stats.repair,
                "selectedAmmo": tank.selected_ammo,
                "equipped": selected,
                "ammo": ammo,
                "cooldown": tank.cooldown,
                "mineCooldown": tank.mine_cooldown,
                "protection": tank.protection,
                "shield": tank.shield,
                "shieldPoints": tank.shield_points,
                "rapid": tank.rapid,
                "speed": tank.speed,
                "laser": tank.laser,
                "respawn": tank.respawn,
                "kills": tank.kills,
                "deaths": tank.deaths,
                "selfRepair": self_repair,
            },
            "scoreboard": scoreboard,
            "recap": recap,
        })
        .to_string()
    }

    /// Stats for nerds, as JSON.
    pub fn stats_json(&self) -> String {
        let render = self.view.renderer.stats();
        let view = self.view.stats();
        let counts = self.sim.snapshot().counts;
        json!({
            "frameMs": self.times.frame_ms,
            "averageFrameMs": self.times.average_ms,
            "fps": if self.times.average_ms > 0.0 { 1000.0 / self.times.average_ms } else { 0.0 },
            "simMs": self.times.sim_ms,
            "renderMs": self.times.render_ms,
            "drawCalls": render.draw_calls,
            "triangles": render.triangles,
            "shadowDrawCalls": render.shadow_draw_calls,
            "reflectionDrawCalls": render.reflection_draw_calls,
            "instanceRecords": render.instance_records,
            "pipelines": render.pipelines,
            "shaderModules": render.shader_modules,
            "latePipelines": render.late_pipelines,
            "meshes": render.meshes,
            "materials": render.materials,
            "textures": render.textures,
            "texturesPending": render.textures_pending,
            "buffers": render.buffers,
            "models": render.models,
            "instances": render.instances,
            "drawClasses": render.draw_classes,
            "gpuBytes": render.gpu_bytes,
            "bodies": counts.bodies,
            "colliders": counts.colliders,
            "shots": counts.shots,
            "mines": counts.mines,
            "fragments": counts.fragments,
            "covers": counts.covers,
            "tanks": self.sim.tanks.len(),
            "view": {
                "tanks": view.tanks,
                "covers": view.covers,
                "coverModels": view.cover_models,
                "fragments": view.fragments,
                "pickups": view.pickups,
                "mines": view.mines,
                "pickupEffects": view.pickup_effects,
                "branches": view.branches,
            },
        })
        .to_string()
    }

    /// The canvas's CSS size and the device pixel ratio.
    pub fn resize(&mut self, css_width: f64, css_height: f64, pixel_ratio: f64, exact: bool) {
        self.client = Vec2::new(css_width as f32, css_height as f32);
        self.pixel_ratio = pixel_ratio;
        self.exact = exact;
        self.apply_size();
    }

    /// Enter or leave first person (only while playing); returns whether it is on.
    pub fn toggle_first_person(&mut self) -> bool {
        if self.sim.match_state.phase == MatchPhase::Playing {
            let aim = self.sim.human().aim;
            self.view.rig.first_person.toggle(aim);
        }
        self.view.rig.first_person.enabled
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
            "shots": sim.shots.len(),
            "fragments": sim.fragments.len(),
            "bodies": sim.world.bodies.len(),
            "autoplay": self.autoplay,
            "overview": self.overview,
            "completedRounds": self.completed_rounds,
            "prepared": self.preparation == Preparation::Done,
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
        self.view.rig.zoom = zoom.clamp(CAMERA.min_zoom, CAMERA.max_zoom);
        self.view.rig.zoom
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

    /// The legacy stress scene: 24 tanks, full debris and 200 shells.
    pub fn debug_stress(&mut self) {
        self.sim.reset(Some(24));
        self.reset_view();
        self.sim.start();
        self.active = true;
        self.autoplay = true;
        for _ in 0..self.sim.max_fragments {
            let x = self.sim.rng.range(-15.0, 15.0);
            let z = self.sim.rng.range(-15.0, 15.0);
            self.sim
                .fragment(x, z, 0xc5a978, 0.5, FragmentShape::Shard, 1.0);
        }
        let owners: Vec<u32> = self.sim.tanks.iter().map(|tank| tank.id).collect();
        for i in 0..200u32 {
            let angle = f64::from(i) * std::f64::consts::TAU / 200.0;
            let id = self.sim.next_id;
            self.sim.next_id += 1;
            self.sim.shots.push(Shot {
                id,
                x: angle.sin() * 3.0,
                z: angle.cos() * 3.0,
                vx: angle.cos() * 45.0,
                vz: angle.sin() * 45.0,
                owner: owners[i as usize % owners.len()],
                team: Team::from_index(i as usize),
                damage: 40.0,
                bounces: 4,
                life: 4.0,
                weapon: Weapon::Standard,
                ..Shot::default()
            });
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
        let tank = self.sim.human();
        if tank.alive {
            let p = self.sim.tank_position(tank);
            (p.x, p.z)
        } else {
            (tank.previous.x, tank.previous.z)
        }
    }

    fn fill_state(&mut self) {
        self.sim.fill_render_state(&mut self.state, None);
    }

    fn apply_size(&mut self) {
        let ratio = if self.exact {
            1.0
        } else {
            self.pixel_ratio.min(CAMERA.max_pixel_ratio)
        };
        let width = (f64::from(self.client.x) * ratio).round().max(1.0) as u32;
        let height = (f64::from(self.client.y) * ratio).round().max(1.0) as u32;
        self.view.resize(width, height);
    }

    /// Rebuild the presentation for the simulation's current world and begin
    /// preparing it.
    fn reset_view(&mut self) {
        self.fill_state();
        self.view.reset(&self.state);
        self.sim.set_wreck_view(Some(self.view.wreck_view()));
        self.view.begin_prepare(&self.state);
        self.preparation = Preparation::Compiling;
        self.events.clear();
        self.accumulator = 0.0;
    }

    fn route_events(&mut self) {
        if self.sim.events.is_empty() {
            return;
        }
        let human = self.sim.human();
        let (human_id, human_team) = (human.id, human.team);
        let events = std::mem::take(&mut self.sim.events);
        for event in &events {
            let player_hit = matches!(event.kind, SimEventType::Hurt | SimEventType::Death)
                && event.owner == Some(human_id)
                && event.team != Some(human_team);
            self.view.event(&self.state, event, player_hit);
            let own = event.id == Some(human_id);
            let damage_angle = (own
                && matches!(event.kind, SimEventType::Hurt | SimEventType::Death))
            .then(|| self.view.damage_angle(event))
            .flatten();
            if self.events.len() >= MAX_PENDING_EVENTS {
                self.events.remove(0);
            }
            self.events.push(PendingEvent {
                event: event.clone(),
                player_hit,
                own,
                damage_angle,
            });
        }
        // Keep the simulation's allocation for the next tick.
        let mut events = events;
        events.clear();
        if self.sim.events.is_empty() {
            self.sim.events = events;
        }
    }
}
