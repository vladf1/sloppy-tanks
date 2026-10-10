//! Fixture hooks for the browser checks and `tests/*.browser.html` (`debug_*`).
//!
//! These calls arrange the engine for the checks: an emptied arena, placed and patched
//! tanks, damage through the shared damage paths, pickups, shells and mines, fixed
//! simulation steps without a frame, still frames with an optional fixed camera, and a
//! read-only inspection of what every entity's view shows. Arranging state here
//! bypasses the gameplay rules on purpose; none of it runs in play.
//!
//! - `debug_clear_arena(keep)`: remove every cover, pickup, shell, mine and tank
//!   except the human and the tanks with ids in `keep`; navigation sees open floor.
//! - `debug_place_tank(id, x, z, heading)`: teleport a tank at rest (`heading` NaN
//!   keeps it).
//! - `debug_set_tank(id, patch)`: `{ hp, xp, protection, shield, shieldPoints,
//!   rapid, speed, laser, cooldown, mineCooldown, aim, respawn, selectedAmmo, ammo,
//!   kills, deaths, damageDealt, bestLifeKills, highestRank, name, frozen }`; `frozen`
//!   stops a bot deciding and locks it in place.
//! - `debug_add_tank(team, kind) -> id`, `debug_add_cover(spec) -> id`.
//! - `debug_damage_tank(id, amount, owner, ownerTeam)` and
//!   `debug_damage_cover(id, amount)` go through the shared damage helpers; an owner
//!   that is not a tank id credits nobody.
//! - `debug_set_sim(patch)`: `{ elapsed, gameMode, reinforcementDelay, match: {
//!   phase, time, scores, winner }, combatRecord: {...} }`.
//! - `debug_set_pickups(pickups)`, `debug_add_shot(shot)`, `debug_add_mine(x, z,
//!   team, arm)`, `debug_set_fragment_life(life)`, `debug_explode(x, z, radius,
//!   damage)`, `debug_reinforce()`, `debug_rig_rng(below)` (the next gameplay draw is
//!   below `below`).
//! - `debug_step(ticks, moveX, moveZ)`: fixed simulation steps with the human's
//!   command, without drawing; `debug_render(alpha, dt, overview, camera)` draws one
//!   frame, from `camera = [px, py, pz, tx, ty, tz]` when given.
//! - `debug_screen_point(x, y, z)`: where a world point shows, in CSS pixels.
//! - `debug_view_json()`: `Presentation::inspect` as JSON; `debug_covers_json()`: the
//!   simulation's covers.
//! - `debug_water_json()`, `debug_set_water_reflection(on)`, `debug_probe(x, y, z, size, color)` (a plain
//!   box for pixel probes; `size <= 0` removes it).
//! - `debug_seats(seed)`: two player seats on a room simulation, each drawn from its
//!   own viewer; see the method.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};
use sloppy_core::geometry::box_geometry;
use sloppy_core::net::fixed_step_clock::SIMULATION_STEP_MS;
use sloppy_core::net::multiplayer_simulation::{MultiplayerOptions, create_multiplayer_simulation};
use sloppy_core::net::player_controls::PlayerControls;
use sloppy_core::scene::{Material, Node};
use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::math::{Quat4, Vec2};
use sloppy_core::sim::physics::{to_rotation, vector};
use sloppy_core::sim::{
    AmmoInventory, CoverKind, DamageCause, GameMode, MatchPhase, Mine, Pickup, PlayerAssignment,
    Shot, Team, VehicleCommand, VehicleKind, Weapon,
};
use sloppy_render::camera::PerspectiveCamera;
use sloppy_render::gpu::{InstanceId, Lifetime, ModelId};
use wasm_bindgen::prelude::*;

use super::{Game, js_error, parse};

/// A bot that should hold still waits this long before deciding anything.
const FROZEN_BRAIN_SECONDS: f64 = 999.0;
/// A shooter id no tank has: damage it deals credits nobody.
const NOBODY: u32 = 999_999;
/// Room fixture: fixed steps simulated, and how often each seat sends input.
const SEAT_TICKS: u64 = 90;
const SEAT_INPUT_EVERY_TICKS: u64 = 3;

thread_local! {
    /// The pixel probe box, if one is placed.
    static PROBE: Cell<Option<(ModelId, InstanceId)>> = const { Cell::new(None) };
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct TankPatch {
    hp: Option<f64>,
    xp: Option<f64>,
    protection: Option<f64>,
    shield: Option<f64>,
    shield_points: Option<f64>,
    rapid: Option<f64>,
    speed: Option<f64>,
    laser: Option<f64>,
    cooldown: Option<f64>,
    mine_cooldown: Option<f64>,
    aim: Option<f64>,
    respawn: Option<f64>,
    selected_ammo: Option<Weapon>,
    ammo: Option<AmmoInventory>,
    kills: Option<u32>,
    deaths: Option<u32>,
    damage_dealt: Option<f64>,
    best_life_kills: Option<u32>,
    highest_rank: Option<usize>,
    name: Option<String>,
    frozen: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct MatchPatch {
    phase: Option<MatchPhase>,
    time: Option<f64>,
    scores: Option<[u32; 2]>,
    winner: Option<Option<Team>>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct CombatRecordPatch {
    life_started: Option<f64>,
    longest_life: Option<f64>,
    busiest_minute: Option<usize>,
    multikill: Option<usize>,
    clutch_kills: Option<u32>,
    revenge_kills: Option<u32>,
    posthumous_kills: Option<u32>,
    mine_kills: Option<u32>,
    cover_destroyed: Option<u32>,
    pickups: Option<u32>,
    shots: Option<u32>,
    direct_hits: Option<u32>,
    damage_taken: Option<f64>,
    shield_absorbed: Option<f64>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct SimPatch {
    elapsed: Option<f64>,
    game_mode: Option<GameMode>,
    reinforcement_delay: Option<f64>,
    #[serde(rename = "match")]
    match_state: Option<MatchPatch>,
    combat_record: Option<CombatRecordPatch>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoverSpec {
    kind: CoverKind,
    x: f64,
    z: f64,
    w: f64,
    d: f64,
    h: f64,
    /// Null (or missing) for indestructible cover.
    hp: Option<f64>,
    color: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShotSpec {
    x: f64,
    z: f64,
    vx: f64,
    vz: f64,
    team: Team,
    /// Not a tank id when the shooter should not matter.
    owner: u32,
    weapon: Weapon,
    damage: f64,
    #[serde(default)]
    bounces: u32,
    life: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PickupSpec {
    kind: sloppy_core::sim::PickupKind,
    x: f64,
    z: f64,
    available: bool,
    #[serde(default)]
    cooldown: f64,
    #[serde(default)]
    cooldown_duration: f64,
}

macro_rules! apply {
    ($target:expr, $patch:expr, $($field:ident),+) => {
        $(if let Some(value) = $patch.$field { $target.$field = value; })+
    };
}

#[wasm_bindgen]
impl Game {
    /// Empty the arena except the human and the tanks whose ids are in `keep`.
    pub fn debug_clear_arena(&mut self, keep: Vec<u32>) {
        let sim = &mut self.sim;
        let bodies: Vec<_> = sim
            .covers
            .iter()
            .map(|cover| cover.body)
            .chain(
                sim.tanks
                    .iter()
                    .filter(|tank| !tank.human && !keep.contains(&tank.id))
                    .map(|tank| tank.body),
            )
            .filter(|&body| sim.world.bodies.contains(body))
            .collect();
        for body in bodies {
            sim.remove_body(body);
        }
        sim.tanks
            .retain(|tank| tank.human || keep.contains(&tank.id));
        sim.covers.clear();
        sim.movable_covers.clear();
        sim.cover_by_collider.clear();
        sim.pickups.clear();
        sim.shots.clear();
        sim.mines.clear();
        sim.events.clear();
        sim.nav.rebuild(&[], None);
        self.events.clear();
        self.fill_state();
        self.view.reset(&self.state);
    }

    /// Teleport a tank at rest; `heading` NaN keeps its heading.
    pub fn debug_place_tank(&mut self, id: u32, x: f64, z: f64, heading: f64) {
        let Some(index) = self.sim.tank_index(id) else {
            return;
        };
        let tank = &mut self.sim.tanks[index];
        if !heading.is_nan() {
            tank.heading = heading;
        }
        let heading = tank.heading;
        tank.previous = Vec2::new(x, z);
        tank.brain.last = Vec2::new(x, z);
        let body = tank.body;
        if let Some(body) = self.sim.world.bodies.get_mut(body) {
            body.set_translation(vector(x, 0.65, z), true);
            body.set_rotation(to_rotation(Quat4::yaw(heading)), true);
            body.set_linvel(vector(0.0, 0.0, 0.0), true);
            body.set_angvel(vector(0.0, 0.0, 0.0), true);
        }
    }

    pub fn debug_set_tank(&mut self, id: u32, patch_json: &str) -> Result<(), JsValue> {
        let patch: TankPatch = parse(patch_json)?;
        let index = self
            .sim
            .tank_index(id)
            .ok_or_else(|| js_error(format!("No tank {id}")))?;
        let tank = &mut self.sim.tanks[index];
        apply!(
            tank,
            patch,
            hp,
            xp,
            protection,
            shield,
            shield_points,
            rapid,
            speed,
            laser,
            cooldown,
            mine_cooldown,
            aim,
            respawn,
            selected_ammo,
            ammo,
            kills,
            deaths,
            damage_dealt,
            best_life_kills,
            highest_rank
        );
        if let Some(name) = patch.name {
            tank.name = name;
        }
        if patch.frozen == Some(true) {
            tank.brain.reaction = FROZEN_BRAIN_SECONDS;
            tank.brain.decision = FROZEN_BRAIN_SECONDS;
            let body = tank.body;
            if let Some(body) = self.sim.world.bodies.get_mut(body) {
                body.set_enabled_translations(false, true, false, true);
            }
        }
        Ok(())
    }

    /// Add a bot tank; returns its id.
    pub fn debug_add_tank(&mut self, team: u8, kind: &str) -> Result<u32, JsValue> {
        let kind: VehicleKind = serde_json::from_value(Value::from(kind))
            .map_err(|error| js_error(error.to_string()))?;
        let slot = self.sim.tanks.len();
        let index = self
            .sim
            .add_tank(Team::from_index(usize::from(team)), false, kind, slot);
        Ok(self.sim.tanks[index].id)
    }

    /// Damage a tank through the shared damage path.
    pub fn debug_damage_tank(&mut self, id: u32, amount: f64, owner: u32, owner_team: u8) {
        if let Some(index) = self.sim.tank_index(id) {
            let life = self
                .sim
                .tank_index(owner)
                .map(|owner| self.sim.tanks[owner].life);
            self.sim.damage_tank(
                index,
                amount,
                owner,
                Team::from_index(usize::from(owner_team)),
                life,
                None,
            );
        }
    }

    /// Damage a cover as the human would.
    pub fn debug_damage_cover(&mut self, id: u32, amount: f64) {
        let Some(index) = self.sim.covers.iter().position(|cover| cover.id == id) else {
            return;
        };
        let human = self.sim.human();
        let (owner, team, life) = (human.id, human.team, human.life);
        self.sim
            .damage_cover(index, amount, owner, team, Some(life), None);
    }

    /// Add a cover and its navigation footprint; returns its id.
    pub fn debug_add_cover(&mut self, spec_json: &str) -> Result<u32, JsValue> {
        let spec: CoverSpec = parse(spec_json)?;
        let index = self.sim.add_cover(&CoverDef::new(
            spec.kind,
            spec.x,
            spec.z,
            spec.w,
            spec.d,
            spec.h,
            spec.hp.unwrap_or(f64::INFINITY),
            spec.color,
        ));
        self.sim.nav.rebuild(&self.sim.covers, None);
        Ok(self.sim.covers[index].id)
    }

    pub fn debug_set_sim(&mut self, patch_json: &str) -> Result<(), JsValue> {
        let patch: SimPatch = parse(patch_json)?;
        let sim = &mut self.sim;
        apply!(sim, patch, elapsed, game_mode, reinforcement_delay);
        if let Some(state) = patch.match_state {
            apply!(sim.match_state, state, phase, time, scores, winner);
        }
        if let Some(record) = patch.combat_record {
            apply!(
                sim.combat_record,
                record,
                life_started,
                longest_life,
                busiest_minute,
                multikill,
                clutch_kills,
                revenge_kills,
                posthumous_kills,
                mine_kills,
                cover_destroyed,
                pickups,
                shots,
                direct_hits,
                damage_taken,
                shield_absorbed
            );
        }
        Ok(())
    }

    /// Replace every pickup; each gets a fresh id.
    pub fn debug_set_pickups(&mut self, pickups_json: &str) -> Result<Vec<u32>, JsValue> {
        let specs: Vec<PickupSpec> = parse(pickups_json)?;
        let sim = &mut self.sim;
        sim.pickups = specs
            .into_iter()
            .map(|spec| {
                let id = sim.allocate_id();
                Pickup {
                    id,
                    kind: spec.kind,
                    x: spec.x,
                    z: spec.z,
                    available: spec.available,
                    cooldown: spec.cooldown,
                    cooldown_duration: spec.cooldown_duration,
                }
            })
            .collect();
        Ok(sim.pickups.iter().map(|pickup| pickup.id).collect())
    }

    /// Put a shell in flight; returns its id.
    pub fn debug_add_shot(&mut self, shot_json: &str) -> Result<u32, JsValue> {
        let spec: ShotSpec = parse(shot_json)?;
        let id = self.sim.allocate_id();
        self.sim.shots.push(Shot {
            id,
            x: spec.x,
            z: spec.z,
            vx: spec.vx,
            vz: spec.vz,
            team: spec.team,
            owner: spec.owner,
            weapon: spec.weapon,
            damage: spec.damage,
            bounces: spec.bounces,
            life: spec.life,
            ..Shot::default()
        });
        Ok(id)
    }

    pub fn debug_clear_shots(&mut self) {
        self.sim.shots.clear();
    }

    /// Lay a mine owned by the human; returns its id.
    pub fn debug_add_mine(&mut self, x: f64, z: f64, team: u8, arm: f64) -> u32 {
        let id = self.sim.allocate_id();
        let owner = self.sim.human().id;
        self.sim.mines.push(Mine {
            id,
            owner,
            owner_life: None,
            damage: None,
            team: Team::from_index(usize::from(team)),
            x,
            z,
            arm,
            life: 20.0,
        });
        id
    }

    /// Set every fragment's remaining life (seconds), to show debris cleanup stages.
    pub fn debug_set_fragment_life(&mut self, life: f64) {
        for fragment in &mut self.sim.fragments {
            fragment.life = life;
        }
    }

    pub fn debug_clear_mines(&mut self) {
        self.sim.mines.clear();
    }

    /// A blast credited to nobody on blue, through the shared explosion path.
    pub fn debug_explode(&mut self, x: f64, z: f64, radius: f64, damage: f64) {
        self.sim.explode(
            Vec2::new(x, z),
            radius,
            damage,
            NOBODY,
            Team::Blue,
            None,
            DamageCause::Explosion,
        );
    }

    /// Solo Assault's reinforcement check, as the next tick would run it.
    pub fn debug_reinforce(&mut self) {
        self.sim.reinforce_solo();
    }

    /// Move the gameplay stream to a state whose next draw is below `below`
    /// (forces one chance roll, like a mocked `rng.next`).
    pub fn debug_rig_rng(&mut self, below: f64) {
        let mut state = self.sim.rng.state.floor();
        loop {
            state += 1.0;
            let mut probe = sloppy_core::sim::math::Random::new(state);
            if probe.next() < below {
                self.sim.rng.state = state;
                return;
            }
        }
    }

    /// Fixed simulation steps with the human driving `(move_x, move_z)`; nothing is
    /// drawn. Events wait for the next frame or `debug_render`.
    pub fn debug_step(&mut self, ticks: u32, move_x: f64, move_z: f64) {
        let aim = self.sim.human().aim;
        let command = VehicleCommand {
            move_x,
            move_z,
            ..VehicleCommand::idle_aiming(aim)
        };
        for _ in 0..ticks {
            self.sim.set_wreck_view(Some(self.view.wreck_view()));
            self.sim.step(command, self.autoplay);
        }
    }

    /// Draw one frame of the current state, routing pending events first. `camera`
    /// `[px, py, pz, tx, ty, tz]` redraws it from that fixed pose.
    pub fn debug_render(
        &mut self,
        alpha: f64,
        dt: f64,
        overview: bool,
        camera: Vec<f32>,
    ) -> Result<(), JsValue> {
        self.fill_state();
        self.route_events();
        self.view
            .render(&self.state, alpha, dt, overview)
            .map_err(js_error)?;
        self.sim.set_wreck_view(Some(self.view.wreck_view()));
        if camera.len() == 6 {
            let mut pose: PerspectiveCamera = self.view.renderer.camera();
            pose.look_at(
                glam::Vec3::from_slice(&camera[..3]),
                glam::Vec3::from_slice(&camera[3..]),
            );
            self.view.renderer.set_camera(pose);
            self.view
                .renderer
                .render(self.view.time as f32)
                .map_err(js_error)?;
        }
        Ok(())
    }

    /// A world point in the last frame's camera as canvas CSS pixels `[x, y]`, for
    /// aiming real pointer input at it.
    pub fn debug_screen_point(&self, x: f32, y: f32, z: f32) -> Vec<f32> {
        let ndc = self
            .view
            .renderer
            .camera()
            .project(glam::Vec3::new(x, y, z));
        vec![
            (ndc.x + 1.0) * 0.5 * self.canvas.css.x,
            (1.0 - ndc.y) * 0.5 * self.canvas.css.y,
        ]
    }

    /// What every entity's view shows after the last frame (`Presentation::inspect`).
    pub fn debug_view_json(&mut self) -> String {
        let inspection = self.view.inspect();
        let effects = self.view.effects.systems.stats();
        let v3 = |v: glam::Vec3| [v.x, v.y, v.z];
        let reticle = &inspection.reticle;
        json!({
            "reticle": {
                "visible": reticle.visible, "confirmed": reticle.confirmed,
                "ready": reticle.ready, "reloading": reticle.reloading,
                "scale": reticle.scale, "position": v3(reticle.position),
            },
            "theme": inspection.theme,
            "tanks": inspection.tanks.iter().map(|tank| json!({
                "id": tank.id, "shown": tank.shown, "barShown": tank.bar_shown,
                "chevrons": tank.chevrons, "position": v3(tank.position),
                "barPosition": v3(tank.bar_position), "pitch": tank.pitch, "roll": tank.roll,
                "turretTiltError": tank.turret_tilt_error,
                "turretForwardInHull": v3(tank.turret_forward_in_hull),
            })).collect::<Vec<_>>(),
            "covers": inspection.covers.iter().map(|cover| json!({
                "id": cover.id, "shown": cover.shown, "stage": cover.stage,
                "modelKey": cover.model_key, "crown": cover.crown, "cut": cover.cut,
            })).collect::<Vec<_>>(),
            "pickups": inspection.pickups.iter().map(|pickup| json!({
                "id": pickup.id, "baseShown": pickup.base_shown, "gem": pickup.gem,
                "ring": pickup.ring, "ringDim": pickup.ring_dim, "refill": pickup.refill,
                "segments": pickup.segments,
            })).collect::<Vec<_>>(),
            "fragments": inspection.fragments.iter().map(|fragment| json!({
                "id": fragment.id, "look": fragment.look, "shown": fragment.shown,
                "opacity": fragment.opacity, "position": v3(fragment.position),
                "scale": v3(fragment.scale), "timberMarks": fragment.timber_marks,
            })).collect::<Vec<_>>(),
            "mines": inspection.mines,
            "laser": {
                "lenses": inspection.laser_lenses,
                "cores": inspection.laser_cores,
            },
            "effects": {
                "particles": effects.particles,
                "blasts": effects.blasts,
                "puffs": effects.puffs,
                "trackMarks": effects.track_marks,
                "trackDust": effects.track_dust,
                "projectiles": effects.projectiles,
                "laserBeams": effects.laser_beams,
                "pickupEffects": effects.pickup_effects,
                "instances": effects.instances,
            },
        })
        .to_string()
    }

    /// Every cover: `{ id, kind, x, z, w, d, h, alive, hp, maxHp, color }`.
    pub fn debug_covers_json(&self) -> String {
        let covers: Vec<Value> = self
            .sim
            .covers
            .iter()
            .map(|cover| {
                json!({
                    "id": cover.id, "kind": cover.kind, "x": cover.x, "z": cover.z,
                    "w": cover.w, "d": cover.d, "h": cover.h, "alive": cover.alive,
                    "hp": cover.hp, "maxHp": cover.max_hp, "color": cover.color,
                })
            })
            .collect();
        Value::from(covers).to_string()
    }

    /// The water's height, reflection flag and calm extent, or null without water.
    pub fn debug_water_json(&self) -> String {
        match self.view.renderer.water_settings() {
            Some(water) => json!({
                "height": water.height,
                "reflection": water.reflection,
                "calmExtent": water.calm_extent,
            })
            .to_string(),
            None => "null".into(),
        }
    }

    /// Enable or skip the water's reflection pass (the reflection check's baseline).
    pub fn debug_set_water_reflection(&mut self, enabled: bool) {
        self.view.renderer.set_water_reflection(enabled);
    }

    /// Place a plain box of `color` for pixel probes; `size <= 0` removes it.
    pub fn debug_probe(&mut self, x: f32, y: f32, z: f32, size: f32, color: u32) {
        let renderer = &mut self.view.renderer;
        if let Some((model, instance)) = PROBE.with(Cell::take) {
            renderer.remove_instance(instance);
            renderer.remove_model(model);
        }
        if size <= 0.0 {
            return;
        }
        let size = f64::from(size);
        let node = Node::mesh(
            Arc::new(box_geometry(size, size, size)),
            Arc::new(Material::basic(color)),
        );
        let model = renderer.add_model(&node, Lifetime::Round);
        if let Some(instance) = renderer.add_instance(
            model,
            glam::Mat4::from_translation(glam::Vec3::new(x, y, z)),
            Lifetime::Round,
        ) {
            PROBE.with(|probe| probe.set(Some((model, instance))));
        }
    }

    /// Two player seats (a scout on blue driving right, a heavy on red driving left)
    /// on a room simulation built like the server's, stepped through the seats'
    /// `PlayerControls` for 90 ticks; then each seat's viewer is drawn on this
    /// presentation. Returns per viewer the follow point, every seat's body and
    /// model position and bar height, and whether drawing left the room's snapshot
    /// and random stream untouched. The single-player world is restored afterwards
    /// (prepare it again before playing).
    pub fn debug_seats(&mut self, seed: f64) -> Result<String, JsValue> {
        let players = [
            ("alice", "Alice", Team::Blue, VehicleKind::Scout),
            ("bob", "Bob", Team::Red, VehicleKind::Heavy),
        ]
        .map(|(id, name, team, kind)| PlayerAssignment {
            player_id: id.into(),
            name: name.into(),
            team,
            slot: 0,
            kind,
        });
        let mut room = create_multiplayer_simulation(seed, &players, MultiplayerOptions::default())
            .map_err(js_error)?;
        let seats: Vec<u32> = room
            .tanks
            .iter()
            .filter(|tank| tank.human)
            .map(|tank| tank.id)
            .collect();
        let mut controls = seats
            .iter()
            .map(|&id| PlayerControls::new(&room, id, 0.0, false))
            .collect::<Result<Vec<_>, _>>()
            .map_err(js_error)?;
        let position = |room: &sloppy_core::sim::Simulation, id: u32| {
            let tank = &room.tanks[room.tank_index(id).expect("seat")];
            let p = room.body_translation(tank.body);
            [p.x, p.z]
        };
        let starts: Vec<[f64; 2]> = seats.iter().map(|&id| position(&room, id)).collect();
        room.start();
        for tick in 1..=SEAT_TICKS {
            let now = tick as f64 * SIMULATION_STEP_MS;
            if tick % SEAT_INPUT_EVERY_TICKS == 1 {
                for (index, control) in controls.iter_mut().enumerate() {
                    let right = index == 0;
                    let input = json!({
                        "type": "input",
                        "controlEpoch": control.control_epoch,
                        "seq": tick,
                        "observedTick": tick - 1,
                        "moveX": if right { 1 } else { -1 },
                        "moveZ": 0,
                        "aim": { "angle": if right { 1.0 } else { -1.0 } * std::f64::consts::FRAC_PI_2 },
                        "fire": false,
                        "actions": [],
                    });
                    control.accept(&room, &input, tick - 1, now);
                }
            }
            let mut commands = BTreeMap::new();
            for control in &mut controls {
                if let Some(command) = control.command(&mut room, tick, now) {
                    commands.insert(control.tank_id, command);
                }
            }
            room.step_with(&commands);
        }
        let ends: Vec<[f64; 2]> = seats.iter().map(|&id| position(&room, id)).collect();
        let before = serde_json::to_string(&room.snapshot()).unwrap_or_default();
        let rng = room.rng.state;
        let mut viewers = Vec::new();
        for &viewer in &seats {
            let state = room.render_state(Some(viewer));
            self.view.reset(&state);
            self.view
                .render(&state, 1.0, 1.0 / 60.0, false)
                .map_err(js_error)?;
            let inspection = self.view.inspect();
            let follow = self.view.rig.follow;
            let poses: Vec<Value> = seats
                .iter()
                .map(|&id| {
                    let tank = inspection.tanks.iter().find(|tank| tank.id == id);
                    json!({
                        "id": id,
                        "body": position(&room, id),
                        "model": tank.map(|tank| [tank.position.x, tank.position.z]),
                        "barHeight": tank.map(|tank| tank.bar_position.y),
                    })
                })
                .collect();
            viewers.push(json!({
                "viewerId": viewer,
                "follow": [follow.x, follow.z],
                "poses": poses,
                "unchanged": serde_json::to_string(&room.snapshot()).unwrap_or_default() == before
                    && room.rng.state == rng,
                "speedTuning": room.speed_tuning,
            }));
        }
        self.reset_view();
        Ok(
            json!({ "seats": seats, "starts": starts, "ends": ends, "viewers": viewers })
                .to_string(),
        )
    }
}
