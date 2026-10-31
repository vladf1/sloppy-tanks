//! The pilot's picture of the battle as a [`Simulation`] the bot brain can read: the room's
//! arena built as the host builds it, then refilled every frame from the scene the client
//! displays. The brain (`sim::ai::bot_command`) then decides from what a player in this
//! seat sees (other tanks drawn behind the predicted present, the own hull where
//! prediction has it) rather than from the host's world.
//!
//! The model is never stepped. Tank and movable cover bodies are placed where the scene has
//! them and Rapier only refreshes its queries (`PhysicsWorld::detect_collisions`), so
//! sight lines, steering casts and firing lanes see the displayed positions. Cover that
//! falls loses its colliders and navigation footprint the way the host's does (a tree keeps
//! its tank-only stump), without the host's debris, events or chain reactions.

use std::collections::HashMap;

use rapier3d::prelude::{Collider, PhysicsWorld, Vector};
use sloppy_core::net::multiplayer_simulation::{MultiplayerOptions, create_multiplayer_simulation};
use sloppy_core::sim::Simulation;
use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::bot_personalities::BotPersonality;
use sloppy_core::sim::damage::tree_stump;
use sloppy_core::sim::data::group;
use sloppy_core::sim::difficulty::Difficulty;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::math::{Quat4, Vec2, distance};
use sloppy_core::sim::navigation::Footprint;
use sloppy_core::sim::physics::{interaction_groups, to_rotation, to_vector};
use sloppy_core::sim::render_state::{RenderCover, RenderState, RenderTank};
use sloppy_core::sim::tank_lifecycle::tank_body_parts;
use sloppy_core::sim::types::{Brain, CoverKind, Driver, Tank};

/// A movable cover's navigation footprint is patched once it has moved this far (metres),
/// as the host patches its own (`MOVING_REBUILD_DISTANCE` in `sim/movable_cover.rs`).
const NAV_MOVE_DISTANCE: f64 = 0.5;

/// Which replicated tank is the pilot's, and the player it escorts (whom the brain's
/// support role treats as the one ally worth following).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Perspective {
    pub own: u32,
    pub leader: Option<u32>,
}

pub struct WorldModel {
    pub simulation: Simulation,
    map: MapId,
    /// Cover index by replicated id; the layout gives the arena's cover the host's ids.
    covers: HashMap<u32, usize>,
    /// Where each movable cover's footprint was when navigation last counted it.
    counted: HashMap<u32, Footprint>,
    /// The own tank's role, kept across lives.
    role: BotPersonality,
}

impl WorldModel {
    /// The arena of `map` with no tanks, built by the host's own room setup so the cover,
    /// its ids, the pickups and the level rules match. `skill` tunes the brain as the room
    /// difficulty tunes fill bots.
    pub fn new(map: MapId, skill: Difficulty, role: BotPersonality, seed: f64) -> Self {
        let simulation = create_multiplayer_simulation(
            seed,
            &[],
            MultiplayerOptions {
                map_mode: Some(map),
                difficulty: Some(skill),
                round: None,
                // Humans-only rooms seat no fill bots: the scene's tanks are added instead.
                humans_only: Some(true),
            },
        )
        .expect("an empty roster is always valid");
        let mut simulation = simulation;
        unstepped(&mut simulation);
        let covers = simulation
            .covers
            .iter()
            .enumerate()
            .map(|(index, cover)| (cover.id, index))
            .collect();
        Self {
            simulation,
            map,
            covers,
            counted: HashMap::new(),
            role,
        }
    }

    pub fn map(&self) -> MapId {
        self.map
    }

    /// The model's index of a replicated tank.
    pub fn tank_index(&self, id: u32) -> Option<usize> {
        self.simulation.tank_index(id)
    }

    /// Refills the model from `state`, the scene this frame displays.
    pub fn sync(&mut self, state: &RenderState, perspective: Perspective) {
        self.sync_covers(state);
        self.sync_tanks(state, perspective);
        let simulation = &mut self.simulation;
        simulation.pickups.clone_from(&state.pickups);
        simulation.mines.clone_from(&state.mines);
        simulation.match_state = state.match_state.clone();
        simulation.elapsed = state.elapsed;
        // Moved bodies reach the colliders and the query structure without a step.
        simulation.world.detect_collisions(&(), &());
    }

    fn sync_covers(&mut self, state: &RenderState) {
        for cover in &state.covers {
            let index = match self.covers.get(&cover.id) {
                Some(&index) => index,
                None => self.add_cover(cover),
            };
            let standing = self.simulation.covers[index].alive;
            if standing && !cover.alive {
                self.fell(index);
            } else if !standing && cover.alive {
                self.simulation.restore_cover(index);
            }
            if cover.alive && cover.kind.movable() {
                self.place_movable(index, cover);
            }
        }
    }

    /// Cover the layout did not hold, such as the rubble a collapse left.
    fn add_cover(&mut self, cover: &RenderCover) -> usize {
        let simulation = &mut self.simulation;
        let index = simulation.add_cover(&CoverDef {
            kind: cover.kind,
            x: cover.x,
            z: cover.z,
            w: cover.w,
            d: cover.d,
            h: cover.h,
            hp: if cover.destructible {
                cover.max_hp
            } else {
                f64::INFINITY
            },
            color: cover.color,
            timber_join: cover.timber_join,
            timber_bays: None,
            debris_seed: cover.debris_seed,
        });
        simulation.covers[index].id = cover.id;
        let region = Footprint::from(&simulation.covers[index]);
        simulation.nav.rebuild(&simulation.covers, Some(region));
        self.covers.insert(cover.id, index);
        index
    }

    /// Takes a cover out as the host's destruction does: no colliders and no footprint, or
    /// for a tree its stump, which only tanks touch.
    fn fell(&mut self, index: usize) {
        let simulation = &mut self.simulation;
        let cover = &simulation.covers[index];
        let body = cover.body;
        if simulation.world.bodies.contains(body) {
            for collider in simulation.world.bodies[body].colliders().to_vec() {
                simulation.cover_by_collider.remove(&collider);
            }
            if cover.kind == CoverKind::Tree {
                let stump = tree_stump(cover.x, cover.z, cover.w, cover.d, cover.h);
                let collider: &mut Collider = &mut simulation.world.colliders[cover.collider];
                collider.set_shape(stump.shape);
                collider.set_translation_wrt_parent(stump.offset);
                collider.set_collision_groups(interaction_groups(group::STUMP_CONTACT));
            } else {
                simulation.remove_body(body);
            }
        }
        simulation.covers[index].alive = false;
        let region = Footprint::from(&simulation.covers[index]);
        simulation.nav.rebuild(&simulation.covers, Some(region));
    }

    /// Drums, teeth and hedgehogs stand where the scene has them; their footprint follows
    /// once it has moved enough to change a route.
    fn place_movable(&mut self, index: usize, cover: &RenderCover) {
        let simulation = &mut self.simulation;
        let body = simulation.covers[index].body;
        if let Some(body) = simulation.world.bodies.get_mut(body) {
            body.set_translation(to_vector(cover.position), true);
            body.set_rotation(to_rotation(cover.rotation), true);
            body.set_linvel(Vector::ZERO, true);
            body.set_angvel(Vector::ZERO, true);
        }
        let footprint = Footprint {
            x: cover.x,
            z: cover.z,
            w: cover.w,
            d: cover.d,
        };
        let counted = *self
            .counted
            .entry(cover.id)
            .or_insert_with(|| Footprint::from(&simulation.covers[index]));
        let moved = distance(
            Vec2::new(counted.x, counted.z),
            Vec2::new(footprint.x, footprint.z),
        )
        .max((counted.w - footprint.w).abs() / 2.0)
        .max((counted.d - footprint.d).abs() / 2.0);
        if moved < NAV_MOVE_DISTANCE {
            return;
        }
        let model = &mut simulation.covers[index];
        (model.x, model.z, model.w, model.d) = (footprint.x, footprint.z, footprint.w, footprint.d);
        simulation
            .nav
            .rebuild(&simulation.covers, Some(union(counted, footprint)));
        self.counted.insert(cover.id, footprint);
    }

    fn sync_tanks(&mut self, state: &RenderState, perspective: Perspective) {
        let simulation = &mut self.simulation;
        // Tanks that left the room (a humans-only seat given up) go with their bodies.
        let mut index = 0;
        while index < simulation.tanks.len() {
            let id = simulation.tanks[index].id;
            if state.tanks.iter().any(|tank| tank.id == id) {
                index += 1;
                continue;
            }
            let tank = simulation.tanks.remove(index);
            if tank.alive {
                simulation.remove_body(tank.body);
            }
        }
        for seen in &state.tanks {
            let own = seen.id == perspective.own;
            let index = match simulation.tank_index(seen.id) {
                Some(index) => index,
                None => {
                    simulation.tanks.push(new_tank(seen, own, self.role));
                    simulation.tanks.len() - 1
                }
            };
            sync_tank(simulation, index, seen, own);
            // Support bots escort their nearest ally that is not support itself, so every
            // ally but the leader counts as support.
            if !own {
                simulation.tanks[index].brain.personality = if Some(seen.id) == perspective.leader {
                    BotPersonality::Guard
                } else {
                    BotPersonality::Support
                };
            }
        }
    }
}

/// Rebuilds the arena's cover in a physics world that has never been stepped. The host's
/// reset ends with a step, which files every body under Rapier's simulation islands; bodies
/// added after it and only ever refreshed by the collision pipeline belong to none, which
/// Rapier does not allow. A world that is never stepped tracks no islands at all. The
/// ground slab stays out: no query the brain makes reads it.
fn unstepped(simulation: &mut Simulation) {
    let defs: Vec<(u32, CoverDef)> = simulation
        .covers
        .iter()
        .map(|cover| {
            let def = CoverDef {
                kind: cover.kind,
                x: cover.x,
                z: cover.z,
                w: cover.w,
                d: cover.d,
                h: cover.h,
                hp: cover.max_hp,
                color: cover.color,
                timber_join: cover.timber_join,
                timber_bays: None,
                debris_seed: cover.debris_seed,
            };
            (cover.id, def)
        })
        .collect();
    simulation.world = PhysicsWorld::new();
    simulation.covers.clear();
    simulation.cover_by_collider.clear();
    simulation.movable_covers.clear();
    for (id, def) in defs {
        let index = simulation.add_cover(&def);
        simulation.covers[index].id = id;
    }
}

/// A tank record for a replicated tank; its body comes with its first live sync.
fn new_tank(seen: &RenderTank, own: bool, role: BotPersonality) -> Tank {
    let position = seen.position.planar();
    Tank {
        id: seen.id,
        name: seen.name.clone(),
        team: seen.team,
        // The own tank is the brain's: as a fill bot, it takes the room's difficulty.
        human: !own && seen.human,
        driver: if own { Driver::Bot } else { Driver::Human },
        kind: seen.kind,
        life: seen.life,
        alive: false,
        aim: seen.aim,
        heading: seen.heading,
        previous: position,
        brain: Brain {
            personality: role,
            ..fresh_brain(position)
        },
        ..Tank::default()
    }
}

/// A brain at the start of a life at `position`, as the host's respawn leaves it.
fn fresh_brain(position: Vec2) -> Brain {
    Brain {
        last_seen: position,
        reaction: 0.3,
        last: position,
        recovery_goal: position,
        attacked_from: position,
        goal: position,
        ..Brain::default()
    }
}

fn sync_tank(simulation: &mut Simulation, index: usize, seen: &RenderTank, own: bool) {
    let speed_scale = simulation.speed_tuning.tank_speed;
    let tank = &simulation.tanks[index];
    let new_life = tank.life != seen.life || tank.kind != seen.kind;
    if tank.alive && (!seen.alive || new_life) {
        let body = tank.body;
        simulation.remove_body(body);
        simulation.tanks[index].alive = false;
    }
    let tank = &simulation.tanks[index];
    if seen.alive && !tank.alive {
        let (builder, [hull, contact]) =
            tank_body_parts(seen.kind, seen.position.planar(), speed_scale);
        let body = simulation.world.insert_body(builder);
        let collider = simulation.world.insert_collider(hull, Some(body));
        simulation.world.insert_collider(contact, Some(body));
        let tank = &mut simulation.tanks[index];
        tank.body = body;
        tank.collider = collider;
        tank.alive = true;
        if own {
            let personality = tank.brain.personality;
            tank.brain = Brain {
                personality,
                ..fresh_brain(seen.position.planar())
            };
            tank.aim = seen.aim;
        }
    }
    let tank = &mut simulation.tanks[index];
    tank.life = seen.life;
    tank.kind = seen.kind;
    tank.team = seen.team;
    tank.name.clone_from(&seen.name);
    tank.heading = seen.heading;
    tank.previous = seen.position.planar();
    tank.xp = seen.xp;
    tank.shield = seen.shield;
    tank.shield_points = seen.shield_points;
    tank.protection = seen.protection;
    tank.laser = seen.laser;
    tank.rapid = seen.rapid;
    tank.speed = seen.speed;
    tank.cooldown = seen.cooldown;
    tank.mine_cooldown = seen.mine_cooldown;
    tank.selected_ammo = seen.selected_ammo;
    tank.ammo = seen.ammo;
    tank.kills = seen.kills;
    tank.deaths = seen.deaths;
    tank.respawn = seen.respawn;
    if !own {
        // The own turret is the brain's, which slews it at its role's rate.
        tank.aim = seen.aim;
    }
    // Health as a share of what the model counts as full, since the room's rules (an extra
    // level's tougher player hulls) may scale the replicated maximum.
    let share = if seen.max_hp > 0.0 {
        (seen.hp / seen.max_hp).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let full = simulation.max_health(&simulation.tanks[index]);
    let tank = &mut simulation.tanks[index];
    tank.hp = share * full;
    if !tank.alive {
        return;
    }
    let body = &mut simulation.world.bodies[tank.body];
    body.set_translation(to_vector(seen.position), true);
    body.set_rotation(to_rotation(Quat4::yaw(seen.heading)), true);
    body.set_linvel(to_vector(seen.velocity), true);
    body.set_angvel(Vector::ZERO, true);
}

/// The footprint covering both `a` and `b`.
fn union(a: Footprint, b: Footprint) -> Footprint {
    let left = (a.x - a.w / 2.0).min(b.x - b.w / 2.0);
    let right = (a.x + a.w / 2.0).max(b.x + b.w / 2.0);
    let near = (a.z - a.d / 2.0).min(b.z - b.d / 2.0);
    let far = (a.z + a.d / 2.0).max(b.z + b.d / 2.0);
    Footprint {
        x: (left + right) / 2.0,
        z: (near + far) / 2.0,
        w: right - left,
        d: far - near,
    }
}

/// The map a scene shows: an extra level names itself, a standard map by its theme;
/// otherwise the lobby's setting.
pub fn scene_map(state: &RenderState, lobby_map: Option<MapId>) -> MapId {
    state
        .custom_map
        .or_else(|| {
            MapId::ALL
                .into_iter()
                .find(|map| map.as_str() == state.map_theme)
        })
        .or(lobby_map)
        .unwrap_or_default()
}
