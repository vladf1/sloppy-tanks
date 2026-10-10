//! The authoritative fixed-step simulation: one Rapier world, the entity records, and the
//! tick order that seeded matches depend on.

use std::collections::{BTreeMap, HashMap};

use rapier3d::prelude::*;
use serde::{Deserialize, Serialize, Serializer};

use super::ai::bot_command;
use super::ammunition::{has_ammo, select_ammo};
use super::arena::{CoverDef, PICKUP_LAYOUT};
use super::barrel_physics::barrel_collider;
use super::bot_personalities::{BotPersonality, shuffled_bot_names};
use super::combat_record::CombatRecord;
use super::data::{ARENA, LASER_DEFENSE, SOLO_TIME, STEP, group, vehicle};
use super::debris_cleanup::{
    DEBRIS_CLEANUP_SECONDS, cleanup_candidate, debris_moving, prepare_debris_cleanup,
};
use super::debris_physics::{
    DebrisContact, DebrisMaterial, debris_material, drain_debris_contacts, track_debris_contacts,
};
use super::difficulty::Difficulty;
use super::map_options::{MapId, map_option_for};
use super::maps::{ArenaMap, GroundKind, selected_map};
use super::match_state::{new_match, tick_match};
use super::math::{Point3, Quat4, Random, Vec2, best_by, distance, js_round, to_uint32};
use super::movable_cover::update_movable_cover;
use super::navigation::{Footprint, Navigation};
use super::physics::{
    ContactForces, convex_hull, from_rotation, from_vector, interaction_groups, query_filter,
    vector,
};
use super::pickups::collect_pickup;
use super::projectiles::step_projectiles;
use super::quarry_barrier_shapes::{DRAGON_TOOTH_MASS, dragon_tooth_variant, quarry_barrier_hulls};
use super::quarry_rock_shape::{quarry_rock_shape, quarry_rock_variant};
use super::simulation_rules::{GRAVITY, MAX_FRAGMENTS, SIMULATION_RULES, SOLO, SPAWN_SCORING};
use super::tank_driving::drive_tank;
use super::tank_lifecycle::solo_spawn;
use super::tower_layout::TOWER_BASE;
use super::types::{
    Cover, CoverKind, CoverMotion, Driver, Fragment, Match, MatchPhase, Mine, Pickup,
    PlayerAssignment, Shot, SimEvent, SimEventType, Tank, Team, VehicleCommand, VehicleKind,
    Weapon,
};
use super::veterancy::{rank_index, rank_stats, repair_veteran};
use super::weapons::{fire_weapon, place_mine, step_mines};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GameMode {
    #[default]
    Team,
    Solo,
}

/// Playtest speed scales; multiplayer always uses 1.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpeedTuning {
    #[serde(rename = "tank-speed")]
    pub tank_speed: f64,
    #[serde(rename = "bullet-speed")]
    pub bullet_speed: f64,
}

impl Default for SpeedTuning {
    fn default() -> Self {
        Self {
            tank_speed: 1.0,
            bullet_speed: 1.0,
        }
    }
}

/// Level rules, run at the end of every playing tick with the same fixed step.
pub type AfterStep = fn(&mut Simulation);

/// Planar view bounds that on-screen tank wrecks land inside.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WreckView {
    pub min_x: f64,
    pub max_x: f64,
    pub min_z: f64,
    pub max_z: f64,
}

/// Options applied over the defaults, like the TS `Partial<Simulation>` setup. A `None`
/// leaves the current value; the doubly optional fields can also clear a value.
#[derive(Clone, Default)]
pub struct SimulationSetup {
    pub players: Option<Vec<PlayerAssignment>>,
    pub humans_only: Option<bool>,
    pub human_kind: Option<VehicleKind>,
    pub human_team: Option<Team>,
    pub difficulty: Option<Difficulty>,
    pub game_mode: Option<GameMode>,
    pub map_mode: Option<MapId>,
    pub custom_map: Option<Option<&'static ArenaMap>>,
    pub endless_match: Option<bool>,
    pub round_count: Option<usize>,
    pub human_health_multiplier: Option<f64>,
    pub power_up_duration_multiplier: Option<f64>,
    pub ammo_crate_multiplier: Option<f64>,
    pub max_fragments: Option<usize>,
    pub after_step: Option<Option<AfterStep>>,
    /// The round the constructor prepares (default 2, matching the TS setup).
    pub round: Option<u32>,
}

impl SimulationSetup {
    /// `{ ...self, ...other }`: every option `other` sets wins.
    pub fn merged(self, other: SimulationSetup) -> SimulationSetup {
        SimulationSetup {
            players: other.players.or(self.players),
            humans_only: other.humans_only.or(self.humans_only),
            human_kind: other.human_kind.or(self.human_kind),
            human_team: other.human_team.or(self.human_team),
            difficulty: other.difficulty.or(self.difficulty),
            game_mode: other.game_mode.or(self.game_mode),
            map_mode: other.map_mode.or(self.map_mode),
            custom_map: other.custom_map.or(self.custom_map),
            endless_match: other.endless_match.or(self.endless_match),
            round_count: other.round_count.or(self.round_count),
            human_health_multiplier: other
                .human_health_multiplier
                .or(self.human_health_multiplier),
            power_up_duration_multiplier: other
                .power_up_duration_multiplier
                .or(self.power_up_duration_multiplier),
            ammo_crate_multiplier: other.ammo_crate_multiplier.or(self.ammo_crate_multiplier),
            max_fragments: other.max_fragments.or(self.max_fragments),
            after_step: other.after_step.or(self.after_step),
            round: other.round.or(self.round),
        }
    }

    /// Assign every set option to `simulation` (the TS `Object.assign`). Takes effect at the
    /// next `reset`.
    pub fn apply(&self, simulation: &mut Simulation) {
        fn set<T: Copy>(target: &mut T, value: Option<T>) {
            if let Some(value) = value {
                *target = value;
            }
        }
        if let Some(players) = &self.players {
            simulation.players = Some(players.clone());
        }
        set(&mut simulation.humans_only, self.humans_only);
        set(&mut simulation.human_kind, self.human_kind);
        set(&mut simulation.human_team, self.human_team);
        set(&mut simulation.difficulty, self.difficulty);
        set(&mut simulation.game_mode, self.game_mode);
        set(&mut simulation.map_mode, self.map_mode);
        set(&mut simulation.custom_map, self.custom_map);
        set(&mut simulation.endless_match, self.endless_match);
        set(&mut simulation.round_count, self.round_count);
        set(
            &mut simulation.human_health_multiplier,
            self.human_health_multiplier,
        );
        set(
            &mut simulation.power_up_duration_multiplier,
            self.power_up_duration_multiplier,
        );
        set(
            &mut simulation.ammo_crate_multiplier,
            self.ammo_crate_multiplier,
        );
        set(&mut simulation.max_fragments, self.max_fragments);
        set(&mut simulation.after_step, self.after_step);
    }
}

/// One projectile sweep, recorded for the server's presentation trace when enabled.
/// Observing never changes combat.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectileMove {
    /// The shell after this sweep.
    pub shot: Shot,
    /// Seconds swept.
    pub seconds: f64,
    /// Seconds into the tick at which the sweep started.
    pub offset: f64,
}

pub struct Simulation {
    pub world: PhysicsWorld,
    pub(crate) contact_forces: ContactForces,
    /// Indices into `covers` of drums, teeth and hedgehogs.
    pub movable_covers: Vec<usize>,
    pub rng: Random,
    pub tanks: Vec<Tank>,
    /// Covers are only ever appended during a round, so indices stay stable.
    pub covers: Vec<Cover>,
    pub cover_by_collider: HashMap<ColliderHandle, usize>,
    pub shots: Vec<Shot>,
    pub(crate) projectile_tank_positions: Vec<Option<Point3>>,
    pub(crate) bot_targets: super::bot_strategy::TargetScratch,
    pub mines: Vec<Mine>,
    /// Stable mine order while detonations can recursively remove entries.
    pub(crate) mine_update_ids: Vec<u32>,
    pub pickups: Vec<Pickup>,
    /// Every crate sees the same post-physics tank positions, with storage reused per tick.
    pickup_tank_positions: Vec<Option<Vec2>>,
    pub fragments: Vec<Fragment>,
    /// Drained by consumers; capped at `SIMULATION_RULES.max_pending_events` each tick.
    pub events: Vec<SimEvent>,
    pub nav: Navigation,
    pub match_state: Match,
    pub next_id: u32,
    pub elapsed: f64,
    pub combat_record: CombatRecord,
    pub seed: f64,
    pub human_team: Team,
    pub human_kind: VehicleKind,
    /// The multiplayer roster; `None` for local play.
    pub players: Option<Vec<PlayerAssignment>>,
    pub humans_only: bool,
    pub speed_tuning: SpeedTuning,
    /// Optional level rules, run at the end of every playing tick.
    pub after_step: Option<AfterStep>,
    /// When `Some`, every projectile sweep is recorded here for the caller to drain.
    pub projectile_moves: Option<Vec<ProjectileMove>>,
    pub difficulty: Difficulty,
    pub game_mode: GameMode,
    pub endless_match: bool,
    pub map_mode: MapId,
    pub custom_map: Option<&'static ArenaMap>,
    pub human_health_multiplier: f64,
    pub power_up_duration_multiplier: f64,
    pub ammo_crate_multiplier: f64,
    current_map: &'static ArenaMap,
    pub reinforcement_delay: f64,
    pub max_fragments: usize,
    pub wreck_view: Option<WreckView>,
    pub destroyed: u32,
    pub shots_fired: u32,
    pub bot_breach_shots: u32,
    pub bot_reroutes: u32,
    pub round_count: usize,
    pub(crate) bot_names: Vec<&'static str>,
    /// Metadata for bodies whose contact forces feed debris-impact events.
    pub(crate) debris_contacts: HashMap<RigidBodyHandle, DebrisContact>,
}

impl Simulation {
    pub fn new(seed: f64, setup: SimulationSetup) -> Self {
        let mut rng = Random::new(seed);
        let human_team = if rng.next() < 0.5 {
            Team::Blue
        } else {
            Team::Red
        };
        let mut simulation = Simulation {
            world: PhysicsWorld::new(),
            contact_forces: ContactForces::default(),
            movable_covers: Vec::new(),
            rng,
            tanks: Vec::new(),
            covers: Vec::new(),
            cover_by_collider: HashMap::new(),
            shots: Vec::new(),
            projectile_tank_positions: Vec::new(),
            bot_targets: Default::default(),
            mines: Vec::new(),
            mine_update_ids: Vec::new(),
            pickups: Vec::new(),
            pickup_tank_positions: Vec::new(),
            fragments: Vec::new(),
            events: Vec::new(),
            nav: Navigation::new(),
            match_state: new_match(1),
            next_id: 1,
            elapsed: 0.0,
            combat_record: CombatRecord::default(),
            seed,
            human_team,
            human_kind: VehicleKind::Balanced,
            players: None,
            humans_only: false,
            speed_tuning: SpeedTuning::default(),
            after_step: None,
            projectile_moves: None,
            difficulty: Difficulty::Normal,
            game_mode: GameMode::Team,
            endless_match: false,
            map_mode: MapId::Village,
            custom_map: None,
            human_health_multiplier: 1.0,
            power_up_duration_multiplier: 1.0,
            ammo_crate_multiplier: 1.0,
            current_map: &super::maps::MAPS[0],
            reinforcement_delay: 0.0,
            max_fragments: MAX_FRAGMENTS,
            wreck_view: None,
            destroyed: 0,
            shots_fired: 0,
            bot_breach_shots: 0,
            bot_reroutes: 0,
            round_count: SIMULATION_RULES.default_tank_count,
            bot_names: Vec::new(),
            debris_contacts: HashMap::new(),
        };
        setup.apply(&mut simulation);
        // reset advances the round (its number and the seed of the bot names), so start one
        // round earlier rather than building and discarding a physics world first.
        simulation.match_state = new_match(setup.round.unwrap_or(2) - 1);
        simulation.reset(None);
        simulation
    }

    /// A seeded simulation with the default setup.
    pub fn with_seed(seed: f64) -> Self {
        Self::new(seed, SimulationSetup::default())
    }

    pub fn multiplayer(&self) -> bool {
        self.players.is_some()
    }

    /// Whether this tank's round statistics feed the local recap.
    pub fn records(&self, tank: &Tank) -> bool {
        !self.multiplayer() && tank.human
    }

    pub fn is_easy_enemy(&self, tank: &Tank) -> bool {
        self.game_mode == GameMode::Solo && !tank.human
    }

    pub fn max_health(&self, tank: &Tank) -> f64 {
        js_round(
            vehicle(tank.kind).health
                * if tank.human {
                    self.human_health_multiplier
                } else {
                    1.0
                }
                * if self.is_easy_enemy(tank) {
                    SOLO.enemy_health_multiplier
                } else {
                    1.0
                }
                * rank_stats(tank.xp).health
                * 100.0,
        ) / 100.0
    }

    /// The renderer's theme: the map's own theme, or an extra level's id.
    pub fn map_theme(&self) -> &'static str {
        match self.current_map.theme {
            Some(theme) => theme.as_str(),
            None => self.current_map.id.as_str(),
        }
    }

    pub fn map_floor(&self) -> Option<GroundKind> {
        self.current_map.floor
    }

    pub fn map_outer_floor(&self) -> Option<GroundKind> {
        self.current_map.outer_floor
    }

    pub fn map_outer_floor_extent(&self) -> Option<f64> {
        self.current_map.outer_floor_extent
    }

    pub fn map_name(&self) -> String {
        map_option_for(self.current_map.id).name.to_uppercase()
    }

    /// Compact maps shrink the shared spawn lanes, pickups and patrol routes about the centre.
    pub fn map_scale(&self) -> f64 {
        self.current_map.scale.unwrap_or(1.0)
    }

    pub fn reset(&mut self, count: Option<usize>) {
        let count = count.unwrap_or(self.round_count);
        self.world = arena_world();
        self.contact_forces.clear();
        self.debris_contacts.clear();
        self.movable_covers.clear();
        self.rng = Random::new(self.seed);
        self.next_id = 1;
        self.tanks.clear();
        self.covers.clear();
        self.cover_by_collider.clear();
        self.shots.clear();
        self.mines.clear();
        self.mine_update_ids.clear();
        self.pickups.clear();
        self.pickup_tank_positions.clear();
        self.fragments.clear();
        self.events.clear();
        self.elapsed = 0.0;
        self.combat_record = CombatRecord::default();
        self.reinforcement_delay = 0.0;
        self.wreck_view = None;
        self.destroyed = 0;
        self.shots_fired = 0;
        self.bot_breach_shots = 0;
        self.bot_reroutes = 0;
        self.round_count = count;
        self.match_state = new_match(self.match_state.round + 1);
        if self.game_mode == GameMode::Solo {
            self.match_state.time = SOLO_TIME;
        }
        let round_seed = to_uint32(
            self.seed + self.match_state.round as f64 * SIMULATION_RULES.round_seed_stride,
        );
        self.bot_names = shuffled_bot_names(round_seed as f64);
        self.current_map = selected_map(self.map_mode, self.custom_map);
        for cover in (self.current_map.layout)() {
            self.add_cover(&cover);
        }
        let scale = self.map_scale();
        self.pickups = PICKUP_LAYOUT
            .iter()
            .map(|placement| {
                let laser = placement.kind == super::types::PickupKind::Laser;
                let cooldown = if laser {
                    LASER_DEFENSE.initial_delay
                } else {
                    0.0
                };
                Pickup {
                    id: self.allocate_id(),
                    kind: placement.kind,
                    x: placement.x * scale,
                    z: placement.z * scale,
                    available: !laser,
                    cooldown,
                    cooldown_duration: cooldown,
                }
            })
            .collect();
        self.nav = Navigation::new();
        self.nav.rebuild(&self.covers, None);
        if self.game_mode == GameMode::Solo {
            self.add_tank(self.human_team, true, self.human_kind, 2);
            for slot in 0..SOLO.active_enemies {
                self.add_tank(self.human_team.opponent(), false, VehicleKind::Scout, slot);
            }
        } else {
            for i in 0..count {
                let team = Team::from_index(i % 2);
                let slot = i / 2;
                let player_kind = self.player_at(team, slot).map(|player| player.kind);
                if self.multiplayer() && self.humans_only && player_kind.is_none() {
                    continue;
                }
                let human = if self.multiplayer() {
                    player_kind.is_some()
                } else {
                    i == self.human_team.index()
                };
                let kind = match player_kind {
                    Some(kind) => kind,
                    None if i == self.human_team.index() => self.human_kind,
                    None => VehicleKind::PLAYABLE[slot % 3],
                };
                self.add_tank(team, human, kind, slot);
            }
        }
        self.world.step();
    }

    /// Create a cover record and its physics body from an authored placement.
    pub fn add_cover(&mut self, def: &CoverDef) -> usize {
        let (body, colliders) = self.cover_body(def.kind, def.x, def.z, def.w, def.d, def.h);
        let cover = Cover {
            id: self.allocate_id(),
            kind: def.kind,
            x: def.x,
            z: def.z,
            w: def.w,
            d: def.d,
            h: def.h,
            hp: def.hp,
            max_hp: def.hp,
            alive: true,
            destructible: def.hp.is_finite(),
            body,
            collider: colliders[0],
            color: def.color,
            debris_seed: def.debris_seed,
            timber_hits: Vec::new(),
            timber_join: def.timber_join,
            kick: None,
            motion: None,
            fallen_at: None,
        };
        let index = self.covers.len();
        if cover.kind.movable() {
            self.movable_covers.push(index);
        }
        self.covers.push(cover);
        self.register_cover(index, &colliders);
        index
    }

    /// Rebuild a destroyed cover at its authored place. Like a tank respawn, the body is
    /// new but the identity is kept, so render views and replication update one record.
    pub fn restore_cover(&mut self, index: usize) {
        if self.covers[index].alive {
            return;
        }
        if self.covers[index].kind == CoverKind::Tower {
            // The tower stands back on its footings in place of their rubble.
            let (x, z) = (self.covers[index].x, self.covers[index].z);
            for side in [-1.0, 1.0] {
                if let Some(rubble) = self.tower_rubble(x + side * TOWER_BASE.offset, z, true) {
                    self.retire_cover(rubble);
                }
            }
        }
        // A felled tree keeps its body as the stump footprint.
        let old_body = self.covers[index].body;
        if self.world.bodies.contains(old_body) {
            self.remove_body(old_body);
        }
        let cover = &mut self.covers[index];
        if let Some(motion) = cover.motion {
            cover.x = motion.origin_x;
            cover.z = motion.origin_z;
            cover.w = motion.w;
            cover.d = motion.d;
        }
        cover.hp = cover.max_hp;
        cover.alive = true;
        cover.timber_hits.clear();
        cover.kick = None;
        let (kind, x, z, w, d, h) = (cover.kind, cover.x, cover.z, cover.w, cover.d, cover.h);
        let (body, colliders) = self.cover_body(kind, x, z, w, d, h);
        self.covers[index].body = body;
        self.covers[index].collider = colliders[0];
        self.register_cover(index, &colliders);
        let region = Footprint::from(&self.covers[index]);
        self.nav.rebuild(&self.covers, Some(region));
    }

    /// The rubble cover a collapsed tower left on the footing at `x, z`, standing or
    /// cleared away.
    pub fn tower_rubble(&self, x: f64, z: f64, alive: bool) -> Option<usize> {
        self.covers
            .iter()
            .position(|c| c.kind == CoverKind::Rubble && c.alive == alive && c.x == x && c.z == z)
    }

    /// Take a standing cover out of the world without destroying it: no debris, events
    /// or score. A rebuilt watchtower clears the rubble its collapse left this way.
    fn retire_cover(&mut self, index: usize) {
        let body = self.covers[index].body;
        let colliders = self.world.bodies[body].colliders().to_vec();
        for collider in colliders {
            self.cover_by_collider.remove(&collider);
        }
        self.remove_body(body);
        self.covers[index].alive = false;
        let region = Footprint::from(&self.covers[index]);
        self.nav.rebuild(&self.covers, Some(region));
    }

    fn cover_body(
        &mut self,
        kind: CoverKind,
        x: f64,
        z: f64,
        w: f64,
        d: f64,
        h: f64,
    ) -> (RigidBodyHandle, Vec<ColliderHandle>) {
        let parts = cover_parts(kind, x, z, w, d, h);
        let body = self.world.insert_body(parts.body);
        let colliders = parts
            .colliders
            .into_iter()
            .map(|collider| self.world.insert_collider(collider, Some(body)))
            .collect();
        if let Some(footprint) = parts.tank_footprint {
            self.world.insert_collider(footprint, Some(body));
        }
        (body, colliders)
    }

    fn register_cover(&mut self, index: usize, colliders: &[ColliderHandle]) {
        let cover = &mut self.covers[index];
        if cover.kind.movable() {
            cover.motion = Some(CoverMotion {
                origin_x: cover.x,
                origin_z: cover.z,
                w: cover.w,
                d: cover.d,
                x: cover.x,
                z: cover.z,
                nav_w: cover.w,
                nav_d: cover.d,
                check_at: 0.0,
            });
            let (id, body, surface) = (cover.id, cover.body, cover_surface(cover.kind));
            for &part in colliders {
                track_debris_contacts(self, body, part, id, surface);
            }
        }
        for &part in colliders {
            self.cover_by_collider.insert(part, index);
        }
    }

    /// The first human tank, if any.
    pub fn human_index(&self) -> Option<usize> {
        self.tanks.iter().position(|tank| tank.human)
    }

    /// The first human tank. Local play always has one.
    pub fn human(&self) -> &Tank {
        &self.tanks[self.human_index().expect("the simulation has a human tank")]
    }

    pub fn tank_index(&self, id: u32) -> Option<usize> {
        self.tanks.iter().position(|tank| tank.id == id)
    }

    /// The multiplayer seat assigned to a team's spawn slot.
    pub(crate) fn player_at(&self, team: Team, slot: usize) -> Option<&PlayerAssignment> {
        self.players
            .as_ref()?
            .iter()
            .find(|player| player.team == team && player.slot == slot)
    }

    /// The next entity id; tanks, cover, pickups, shells, mines and debris share one sequence.
    pub fn allocate_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn start(&mut self) {
        self.match_state.phase = MatchPhase::Playing;
    }

    /// Advance one fixed tick of local play. The human tank follows `command` unless
    /// `autoplay` hands it to the bot brain.
    pub fn step(&mut self, command: VehicleCommand, autoplay: bool) {
        assert!(
            !self.multiplayer(),
            "Multiplayer requires per-tank commands through step_with"
        );
        self.advance(Some(command), autoplay, None);
    }

    /// Commands last one tick. Missing human input is idle; only explicit bot drivers use AI.
    pub fn step_with(&mut self, commands: &BTreeMap<u32, VehicleCommand>) {
        self.advance(None, false, Some(commands));
    }

    fn advance(
        &mut self,
        command: Option<VehicleCommand>,
        autoplay: bool,
        commands: Option<&BTreeMap<u32, VehicleCommand>>,
    ) {
        if self.match_state.phase != MatchPhase::Playing {
            return;
        }
        self.elapsed += STEP;
        if self.game_mode == GameMode::Solo {
            self.match_state.time = 0f64.max(self.match_state.time - STEP);
            self.check_solo_result();
            if self.match_state.phase == MatchPhase::Playing {
                self.reinforce_solo();
            }
        } else if !self.endless_match {
            tick_match(&mut self.match_state, STEP);
        }
        if self.match_state.phase != MatchPhase::Playing {
            return;
        }
        for i in 0..self.tanks.len() {
            if !self.tanks[i].alive {
                if self.game_mode == GameMode::Solo {
                    continue;
                }
                self.tanks[i].respawn -= STEP;
                if self.tanks[i].respawn <= 0.0 {
                    self.respawn(i, None);
                }
                continue;
            }
            let position = self.body_translation(self.tanks[i].body);
            let tank = &mut self.tanks[i];
            tank.previous = position.planar();
            tank.protection = 0f64.max(tank.protection - STEP);
            tank.cooldown = 0f64.max(tank.cooldown - STEP);
            tank.mine_cooldown = 0f64.max(tank.mine_cooldown - STEP);
            tank.shield = 0f64.max(tank.shield - STEP);
            if tank.shield == 0.0 {
                tank.shield_points = 0.0;
            }
            tank.rapid = 0f64.max(tank.rapid - STEP);
            tank.speed = 0f64.max(tank.speed - STEP);
            tank.laser = 0f64.max(tank.laser - STEP);
            tank.laser_recharge = 0f64.max(tank.laser_recharge - STEP);
            tank.recoil =
                0f64.max(tank.recoil - STEP * SIMULATION_RULES.recoil_recovery_per_second);
            let c = match commands {
                Some(commands) => match tank.driver {
                    Driver::Bot => bot_command(self, i, STEP),
                    Driver::Human => commands
                        .get(&tank.id)
                        .copied()
                        .unwrap_or_else(|| VehicleCommand::idle_aiming(tank.aim)),
                    Driver::Idle => VehicleCommand::idle_aiming(tank.aim),
                },
                None if tank.human && !autoplay => command.unwrap_or_default(),
                None => bot_command(self, i, STEP),
            };
            let tank = &mut self.tanks[i];
            tank.command = c;
            if let Some(super::types::AmmoSelection::Weapon(chosen)) = c.ammo_selection
                && tank.human
                && !has_ammo(tank, chosen)
            {
                let mut event = SimEvent::at(SimEventType::Notice, position.x, position.z);
                event.id = Some(tank.id);
                event.label = Some(format!(
                    "{} EMPTY — collect an ammo crate",
                    chosen.as_str().to_uppercase()
                ));
                self.events.push(event);
            }
            let tank = &mut self.tanks[i];
            select_ammo(tank, c.ammo_selection);
            tank.aim = c.aim;
            let body = &mut self.world.bodies[tank.body];
            drive_tank(tank, body, &c, STEP, self.speed_tuning.tank_speed);
            if c.fire {
                fire_weapon(self, i);
            }
            if c.mine {
                place_mine(self, i);
            }
        }
        self.world.step_with_events(&(), &self.contact_forces);
        drain_debris_contacts(self);
        update_movable_cover(self);
        step_projectiles(self, STEP, true);
        step_mines(self, STEP);
        for i in 0..self.tanks.len() {
            repair_veteran(self, i, STEP);
        }
        // Collecting changes stats, never positions, so each hull is read once for every crate.
        self.pickup_tank_positions.clear();
        self.pickup_tank_positions
            .extend(self.tanks.iter().map(|tank| {
                tank.alive
                    .then(|| from_vector(self.world.bodies[tank.body].translation()).planar())
            }));
        for p in 0..self.pickups.len() {
            let pickup = &mut self.pickups[p];
            if !pickup.available {
                pickup.cooldown -= STEP;
                if pickup.cooldown <= 0.0 {
                    pickup.available = true;
                }
                continue;
            }
            let at = Vec2::new(pickup.x, pickup.z);
            for i in 0..self.pickup_tank_positions.len() {
                if let Some(position) = self.pickup_tank_positions[i]
                    && distance(position, at) < SIMULATION_RULES.pickup_radius
                {
                    let mut supply = self.pickups[p].clone();
                    let taken = collect_pickup(self, i, &mut supply);
                    self.pickups[p] = supply;
                    if taken {
                        break;
                    }
                }
            }
        }
        prepare_debris_cleanup(self);
        let mut i = self.fragments.len();
        while i > 0 {
            i -= 1;
            let fragment = &self.fragments[i];
            // Keep substantial debris solid while it is still moving, with a hard age limit.
            let hold = fragment.life > DEBRIS_CLEANUP_SECONDS
                && fragment.life - STEP <= DEBRIS_CLEANUP_SECONDS
                && fragment
                    .expires_at
                    .is_some_and(|expires| self.elapsed < expires - DEBRIS_CLEANUP_SECONDS)
                && debris_moving(self, fragment);
            let fragment = &mut self.fragments[i];
            if !hold {
                fragment.life -= STEP;
            }
            if fragment.life <= DEBRIS_CLEANUP_SECONDS {
                let body = fragment.body;
                let collider = self.world.bodies[body].colliders()[0];
                let collider = &mut self.world.colliders[collider];
                if packed_groups(collider.collision_groups()) != group::FRAGMENT {
                    collider.set_collision_groups(interaction_groups(group::FRAGMENT));
                }
            }
            if self.fragments[i].life <= 0.0 {
                let body = self.fragments[i].body;
                self.remove_body(body);
                self.fragments.remove(i);
            }
        }
        if let Some(after_step) = self.after_step {
            after_step(self);
        }
        // Consumers drain every rendered frame; headless sessions remain bounded too.
        if self.events.len() > SIMULATION_RULES.max_pending_events {
            let excess = self.events.len() - SIMULATION_RULES.max_pending_events;
            self.events.drain(..excess);
        }
    }

    pub fn reinforce_solo(&mut self) {
        self.reinforcement_delay = 0f64.max(self.reinforcement_delay - STEP);
        if self.reinforcement_delay > 0.0 {
            return;
        }
        let living: Vec<usize> = (0..self.tanks.len())
            .filter(|&i| !self.tanks[i].human && self.tanks[i].alive)
            .collect();
        if living.len() >= SOLO.active_enemies {
            return;
        }
        let team = self.human_team.opponent();
        let slots: Vec<Vec2> = (0..SOLO.active_enemies)
            .map(|slot| solo_spawn(team, slot))
            .filter(|&p| {
                self.tanks.iter().all(|tank| {
                    !tank.alive
                        || distance(
                            p,
                            from_vector(self.world.bodies[tank.body].translation()).planar(),
                        ) > 4.0
                })
            })
            .collect();
        let human = self.human_index().into_iter().collect::<Vec<_>>();
        let spawn = best_by(slots, |&p| self.spawn_score(p, &human, &living));
        let Some(spawn) = spawn else {
            return;
        };
        // Reuse the six enemy slots so long runs do not accumulate tanks or HUD meshes.
        let Some(replacement) = self
            .tanks
            .iter()
            .position(|tank| !tank.human && !tank.alive)
        else {
            return;
        };
        self.respawn(replacement, Some(spawn));
        self.reinforcement_delay = SOLO.reinforcement_seconds;
    }

    pub fn check_solo_result(&mut self) {
        if self.game_mode != GameMode::Solo || self.match_state.phase != MatchPhase::Playing {
            return;
        }
        if !self.human().alive {
            self.match_state.winner = Some(self.human_team.opponent());
            self.match_state.phase = MatchPhase::Results;
        } else if self.match_state.time == 0.0 {
            self.match_state.winner = Some(self.human_team);
            self.match_state.phase = MatchPhase::Results;
        }
    }

    pub fn spawn_score(&self, position: Vec2, enemies: &[usize], friends: &[usize]) -> f64 {
        let mut score = SPAWN_SCORING.maximum_enemy_distance;
        for &enemy in enemies {
            let q = self.tank_planar(enemy);
            score = score.min(
                distance(position, q)
                    - if self.visible(position, q) {
                        SPAWN_SCORING.visible_enemy_penalty
                    } else {
                        0.0
                    },
            );
        }
        for &friend in friends {
            let q = self.tank_planar(friend);
            score -= 0f64.max(SPAWN_SCORING.ally_clearance - distance(position, q))
                * SPAWN_SCORING.ally_proximity_penalty;
        }
        score
    }

    /// Whether a one-metre-high sight line between two planar points clears all cover.
    pub fn visible(&self, a: Vec2, b: Vec2) -> bool {
        let len = distance(a, b);
        if len < 0.01 {
            return true;
        }
        let ray = Ray::new(
            vector(a.x, 1.0, a.z),
            vector((b.x - a.x) / len, 0.0, (b.z - a.z) / len),
        );
        self.world
            .cast_ray(&ray, len as f32, true, query_filter(group::COVER_QUERY))
            .is_none()
    }

    /// Evict the least relevant debris until `count` more pieces fit the budget.
    pub fn reserve_fragments(&mut self, count: usize) {
        while self.fragments.len() + count > self.max_fragments {
            let Some(old) = cleanup_candidate(self) else {
                break;
            };
            let body = self.fragments[old].body;
            self.fragments.remove(old);
            self.remove_body(body);
        }
    }

    /// Remove a body with its colliders, and forget its debris-contact metadata.
    pub fn remove_body(&mut self, body: RigidBodyHandle) {
        self.world.remove_body(body);
        self.debris_contacts.remove(&body);
    }

    pub fn body_translation(&self, body: RigidBodyHandle) -> Point3 {
        from_vector(self.world.bodies[body].translation())
    }

    /// A live tank's body position on the X/Z plane.
    pub fn tank_planar(&self, tank_index: usize) -> Vec2 {
        self.body_translation(self.tanks[tank_index].body).planar()
    }

    pub fn body_rotation(&self, body: RigidBodyHandle) -> Quat4 {
        from_rotation(*self.world.bodies[body].rotation())
    }

    pub fn body_linvel(&self, body: RigidBodyHandle) -> Point3 {
        from_vector(self.world.bodies[body].linvel())
    }

    /// Set the planar wreck-landing bounds from the local camera view (ignored online).
    pub fn set_wreck_view(&mut self, view: Option<WreckView>) {
        if !self.multiplayer() {
            self.wreck_view = view;
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            seed: self.seed,
            difficulty: self.difficulty,
            elapsed: self.elapsed,
            match_state: self.match_state.clone(),
            tanks: self
                .tanks
                .iter()
                .map(|tank| {
                    let position = self.tank_position(tank).planar();
                    TankSnapshot {
                        id: tank.id,
                        name: tank.name.clone(),
                        team: tank.team,
                        kind: tank.kind,
                        alive: tank.alive,
                        hp: tank.hp,
                        max_hp: self.max_health(tank),
                        xp: tank.xp,
                        rank: rank_index(tank.xp),
                        selected_ammo: tank.selected_ammo,
                        ammo: tank.ammo,
                        laser: tank.laser,
                        x: position.x,
                        z: position.z,
                        aim: tank.aim,
                        kills: tank.kills,
                        deaths: tank.deaths,
                        mode: tank.brain.mode,
                        recovering: tank.brain.recovery > 0.0,
                        recoveries: tank.brain.recoveries,
                        personality: if tank.human {
                            None
                        } else {
                            Some(tank.brain.personality)
                        },
                        ultra_aggressive: !tank.human && tank.brain.ultra_aggressive,
                    }
                })
                .collect(),
            counts: SnapshotCounts {
                bodies: self.world.bodies.len(),
                colliders: self.world.colliders.len(),
                shots: self.shots.len(),
                mines: self.mines.len(),
                fragments: self.fragments.len(),
                covers: self.covers.iter().filter(|cover| cover.alive).count(),
            },
            destroyed: self.destroyed,
            nav_version: self.nav.version,
            bot_reroutes: self.bot_reroutes,
            bot_breach_shots: self.bot_breach_shots,
        }
    }
}

/// Serializable diagnostics of the whole simulation, matching the TS `snapshot()`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub seed: f64,
    pub difficulty: Difficulty,
    pub elapsed: f64,
    #[serde(rename = "match")]
    pub match_state: Match,
    pub tanks: Vec<TankSnapshot>,
    pub counts: SnapshotCounts,
    pub destroyed: u32,
    pub nav_version: u32,
    pub bot_reroutes: u32,
    pub bot_breach_shots: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TankSnapshot {
    pub id: u32,
    pub name: String,
    pub team: Team,
    pub kind: VehicleKind,
    pub alive: bool,
    pub hp: f64,
    pub max_hp: f64,
    pub xp: f64,
    pub rank: usize,
    pub selected_ammo: Weapon,
    pub ammo: super::types::AmmoInventory,
    pub laser: f64,
    pub x: f64,
    pub z: f64,
    pub aim: f64,
    pub kills: u32,
    pub deaths: u32,
    pub mode: super::types::BotMode,
    pub recovering: bool,
    pub recoveries: u32,
    /// The bot personality; serialized as "player" for humans.
    #[serde(serialize_with = "personality_or_player")]
    pub personality: Option<BotPersonality>,
    pub ultra_aggressive: bool,
}

fn personality_or_player<S: Serializer>(
    value: &Option<BotPersonality>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(personality) => personality.serialize(serializer),
        None => serializer.serialize_str("player"),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SnapshotCounts {
    pub bodies: usize,
    pub colliders: usize,
    pub shots: usize,
    pub mines: usize,
    pub fragments: usize,
    pub covers: usize,
}

/// A fresh physics world at the fixed step whose first body is the arena's ground slab,
/// shared with client prediction so its world matches the server's.
pub(crate) fn arena_world() -> PhysicsWorld {
    let mut world = PhysicsWorld::new();
    world.gravity = vector(0.0, -GRAVITY, 0.0);
    world.integration_parameters.dt = STEP as f32;
    let ground = world.insert_body(RigidBodyBuilder::fixed().translation(vector(0.0, -0.5, 0.0)));
    world.insert_collider(
        ColliderBuilder::cuboid((ARENA + 2.0) as f32, 0.5, (ARENA + 2.0) as f32)
            .collision_groups(interaction_groups(group::GROUND)),
        Some(ground),
    );
    world
}

/// A cover's body and colliders as the simulation builds them, shared with client
/// prediction so its copy of the arena collides exactly like the server's.
pub struct CoverParts {
    pub body: RigidBodyBuilder,
    /// The cover's own colliders; the first is its `collider`.
    pub colliders: Vec<ColliderBuilder>,
    /// A tank-only footprint that is not part of the cover's hit volume.
    pub tank_footprint: Option<ColliderBuilder>,
}

pub fn cover_parts(kind: CoverKind, x: f64, z: f64, w: f64, d: f64, h: f64) -> CoverParts {
    let drum = kind == CoverKind::Drum;
    let movable = kind.movable();
    let builder = if movable {
        RigidBodyBuilder::dynamic()
            .can_sleep(true)
            .sleeping(true)
            .linear_damping(if drum { 0.6 } else { 0.2 })
            .angular_damping(if drum { 0.3 } else { 0.4 })
            .ccd_enabled(drum)
    } else {
        RigidBodyBuilder::fixed()
    };
    let shapes: Vec<ColliderBuilder> = match kind {
        CoverKind::Teeth | CoverKind::Hedgehog => {
            quarry_barrier_hulls(kind, w, h, d, dragon_tooth_variant(x, z))
                .iter()
                .map(|points| convex_hull(points))
                .collect()
        }
        CoverKind::Drum => vec![barrel_collider(w, h, d)],
        CoverKind::Rock => {
            let rock = quarry_rock_shape(w, h, d, quarry_rock_variant(x, z));
            let vertices = rock
                .positions
                .as_chunks::<3>()
                .0
                .iter()
                .map(|p| Vector::new(p[0], (p[1] as f64 - h / 2.0) as f32, p[2]))
                .collect();
            let indices = rock.indices.as_chunks::<3>().0.to_vec();
            vec![ColliderBuilder::trimesh(vertices, indices).expect("authored rock mesh is valid")]
        }
        _ => vec![ColliderBuilder::cuboid(
            (w / 2.0) as f32,
            (h / 2.0) as f32,
            (d / 2.0) as f32,
        )],
    };
    let surface = cover_surface(kind);
    let count = shapes.len() as f64;
    let colliders = shapes
        .into_iter()
        .map(|shape| {
            if movable {
                let mass = if drum {
                    0.45
                } else if kind == CoverKind::Teeth {
                    DRAGON_TOOTH_MASS
                } else {
                    6.0
                };
                let material = debris_material(surface);
                shape
                    .collision_groups(interaction_groups(group::MOVABLE_COVER))
                    .mass((mass / count) as f32)
                    .friction(material.friction as f32)
                    .restitution(material.restitution as f32)
            } else {
                shape
                    .collision_groups(interaction_groups(group::COVER))
                    .friction(0.4)
            }
        })
        .collect();
    // Tanks use the navigation footprint so the slope cannot lift their planar hulls.
    // Shells still hit only the visible pyramid, including its open upper shoulders.
    let tank_footprint = (kind == CoverKind::Teeth).then(|| {
        ColliderBuilder::cuboid((w / 2.0) as f32, (h / 2.0) as f32, (d / 2.0) as f32)
            .collision_groups(interaction_groups(group::TOOTH_CONTACT))
            .mass(0.0)
            .friction(0.8)
    });
    CoverParts {
        body: builder.translation(vector(x, h / 2.0, z)),
        colliders,
        tank_footprint,
    }
}

/// The packed 32-bit form of collision groups.
pub fn packed_groups(groups: InteractionGroups) -> u32 {
    (groups.memberships.bits() << 16) | (groups.filter.bits() & 0xffff)
}

/// Barrels and hedgehogs ring as metal; everything else movable is concrete.
pub fn cover_surface(kind: CoverKind) -> DebrisMaterial {
    if kind == CoverKind::Drum || kind == CoverKind::Hedgehog {
        DebrisMaterial::Metal
    } else {
        DebrisMaterial::Concrete
    }
}
