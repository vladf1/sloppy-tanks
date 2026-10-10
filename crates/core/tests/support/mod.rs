//! Shared test fixtures (the former `tests/fixtures.ts`). Include with `mod support;`.
#![allow(dead_code)]

use rapier3d::prelude::RigidBodyHandle;
use sloppy_core::sim::ai::bot_command;
use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::data::STEP;
use sloppy_core::sim::math::{Quat4, Vec2};
use sloppy_core::sim::physics::{to_rotation, vector};
use sloppy_core::sim::simulation::packed_groups;
use sloppy_core::sim::simulation_rules::SIMULATION_RULES;
use sloppy_core::sim::{
    CoverKind, DamageSource, Pickup, PickupKind, Shot, SimEventType, Simulation, Tank,
    VehicleCommand,
};

/// Indices of the human, its enemy and its ally after `player_enemy_ally`.
pub const PLAYER: usize = 0;
pub const ENEMY: usize = 1;
pub const ALLY: usize = 2;

/// Hull clearance an extra level's spawns and pickups keep from every cover.
const SPAWN_CLEARANCE: f64 = 1.5;

/// Empties the authored map so a test controls every obstacle: removes all cover, pickups
/// and every tank not in `keep`, and rebuilds navigation for the open floor. The kept tanks
/// become indices `0..keep.len()` in `keep` order. Callers still place the kept tanks and
/// step the world.
pub fn clear_arena(s: &mut Simulation, keep: &[usize]) {
    let bodies: Vec<_> = s
        .covers
        .iter()
        .map(|cover| cover.body)
        .filter(|&body| s.world.bodies.contains(body))
        .collect();
    for body in bodies {
        s.remove_body(body);
    }
    let kept: Vec<Tank> = keep.iter().map(|&i| s.tanks[i].clone()).collect();
    let removed: Vec<_> = s
        .tanks
        .iter()
        .enumerate()
        .filter(|(i, tank)| !keep.contains(i) && s.world.bodies.contains(tank.body))
        .map(|(_, tank)| tank.body)
        .collect();
    for body in removed {
        s.remove_body(body);
    }
    s.covers.clear();
    s.movable_covers.clear();
    s.cover_by_collider.clear();
    s.tanks = kept;
    s.pickups.clear();
    s.nav.rebuild(&[], None);
}

/// Seed 123 cleared down to the human, its first enemy and its first bot ally (indices
/// `PLAYER`, `ENEMY`, `ALLY`); callers place them.
pub fn player_enemy_ally() -> Simulation {
    let mut s = Simulation::with_seed(123.0);
    let player = s.human_index().unwrap();
    let team = s.tanks[player].team;
    let enemy = s.tanks.iter().position(|t| t.team != team).unwrap();
    let ally = s
        .tanks
        .iter()
        .position(|t| !t.human && t.team == team)
        .unwrap();
    clear_arena(&mut s, &[player, enemy, ally]);
    s
}

/// The human, one enemy and one ally, unprotected and 14 m apart along +z.
pub fn squad() -> Simulation {
    let mut s = player_enemy_ally();
    for i in 0..s.tanks.len() {
        let z = i as f64 * 14.0;
        s.tanks[i].protection = 0.0;
        set_translation(&mut s, i, 0.0, z);
        s.tanks[i].previous = Vec2::new(0.0, z);
    }
    s.world.step();
    s.start();
    s
}

/// Teleports a tank at rest, keeping interpolation and bot history on the new pose.
pub fn place_tank(s: &mut Simulation, index: usize, x: f64, z: f64, heading: Option<f64>) {
    let tank = &mut s.tanks[index];
    let heading = heading.unwrap_or(tank.heading);
    tank.heading = heading;
    tank.previous = Vec2::new(x, z);
    tank.brain.last = Vec2::new(x, z);
    let body = &mut s.world.bodies[tank.body];
    body.set_translation(vector(x, SIMULATION_RULES.tank_body_height, z), true);
    body.set_rotation(to_rotation(Quat4::yaw(heading)), true);
    body.set_linvel(vector(0.0, 0.0, 0.0), true);
    body.set_angvel(vector(0.0, 0.0, 0.0), true);
}

/// Moves a tank's body only: its heading, velocity and recorded poses stay as they were.
pub fn set_translation(s: &mut Simulation, index: usize, x: f64, z: f64) {
    let body = s.tanks[index].body;
    s.world.bodies[body].set_translation(vector(x, SIMULATION_RULES.tank_body_height, z), true);
}

/// The index of the tank with `id`.
pub fn tank_index(s: &Simulation, id: u32) -> usize {
    s.tank_index(id).expect("tank exists")
}

/// Planar body position of a live tank.
pub fn tank_xz(s: &Simulation, index: usize) -> Vec2 {
    s.body_translation(s.tanks[index].body).planar()
}

/// Authored planar position of a cover.
pub fn cover_at(s: &Simulation, cover: usize) -> Vec2 {
    Vec2::new(s.covers[cover].x, s.covers[cover].z)
}

/// Packed interaction groups of a body's first collider.
pub fn collider_groups(s: &Simulation, body: RigidBodyHandle) -> u32 {
    let collider = s.world.bodies[body].colliders()[0];
    packed_groups(s.world.colliders[collider].collision_groups())
}

/// An indestructible 3 m concrete wall.
pub fn concrete(x: f64, z: f64, w: f64, d: f64) -> CoverDef {
    CoverDef::new(CoverKind::Concrete, x, z, w, d, 3.0, f64::INFINITY, 0)
}

/// An available pickup with no cooldown.
pub fn pickup(id: u32, kind: PickupKind, x: f64, z: f64) -> Pickup {
    Pickup {
        id,
        kind,
        x,
        z,
        available: true,
        cooldown: 0.0,
        cooldown_duration: 0.0,
    }
}

/// An available pickup that takes the next simulation id.
pub fn supply(s: &mut Simulation, kind: PickupKind, x: f64, z: f64) -> Pickup {
    let id = s.next_id;
    s.next_id += 1;
    pickup(id, kind, x, z)
}

/// `s.damageTank(victim, amount, attacker.id, attacker.team, ownerLife, source)`.
pub fn damage_from(
    s: &mut Simulation,
    victim: usize,
    amount: f64,
    attacker: usize,
    owner_life: Option<u32>,
    source: Option<DamageSource>,
) {
    let (id, team) = (s.tanks[attacker].id, s.tanks[attacker].team);
    s.damage_tank(victim, amount, id, team, owner_life, source);
}

/// One simulation tick with idle human input.
pub fn idle(s: &mut Simulation) {
    s.step(VehicleCommand::idle(), false);
}

/// Forces a fresh decision and runs one bot tick.
pub fn decide(s: &mut Simulation, bot: usize) -> VehicleCommand {
    s.tanks[bot].brain.decision = 0.0;
    bot_command(s, bot, STEP)
}

/// How many pending events are of `kind`.
pub fn event_count(s: &Simulation, kind: SimEventType) -> usize {
    s.events.iter().filter(|e| e.kind == kind).count()
}

/// The shot with `id`, which must still be in flight.
pub fn shot_by_id(s: &Simulation, id: u32) -> &Shot {
    s.shots
        .iter()
        .find(|shot| shot.id == id)
        .expect("shot still in flight")
}

/// Asserts that every pickup and tank spawn stands on open floor, has a route from the
/// centre and keeps `SPAWN_CLEARANCE` from every cover. Returns the checked points.
pub fn assert_spawns_and_pickups_clear(sim: &mut Simulation) -> Vec<Vec2> {
    let mut points: Vec<Vec2> = sim.pickups.iter().map(|p| Vec2::new(p.x, p.z)).collect();
    points.extend(
        sim.tanks
            .iter()
            .map(|tank| sim.body_translation(tank.body).planar()),
    );
    for &point in &points {
        assert!(!sim.nav.is_blocked(point), "blocked {point:?}");
        assert!(
            (point.x == 0.0 && point.z == 0.0) || !sim.nav.find(Vec2::ZERO, point).is_empty(),
            "unreachable {point:?}"
        );
        for cover in &sim.covers {
            assert!(
                (point.x - cover.x).abs() >= cover.w / 2.0 + SPAWN_CLEARANCE
                    || (point.z - cover.z).abs() >= cover.d / 2.0 + SPAWN_CLEARANCE,
                "no hull clearance at {point:?} beside {:?}",
                cover.kind
            );
        }
    }
    points
}
