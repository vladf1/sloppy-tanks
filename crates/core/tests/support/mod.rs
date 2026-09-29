//! Shared test fixtures (the former `tests/fixtures.ts`). Include with `mod support;`.
#![allow(dead_code)]

use sloppy_core::sim::math::{Quat4, Vec2};
use sloppy_core::sim::navigation::Footprint;
use sloppy_core::sim::physics::{to_rotation, vector};
use sloppy_core::sim::{Simulation, SimulationSetup, Tank};

/// The TypeScript default seed.
pub const DEFAULT_SEED: f64 = 12345.0;

/// `new Simulation()`.
pub fn simulation() -> Simulation {
    Simulation::with_seed(DEFAULT_SEED)
}

/// `new Simulation(seed, setup)`.
pub fn simulation_with(seed: f64, setup: SimulationSetup) -> Simulation {
    Simulation::new(seed, setup)
}

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
    s.nav.rebuild(&[], None::<Footprint>);
}

/// Teleports a tank at rest, keeping interpolation and bot history on the new pose.
pub fn place_tank(s: &mut Simulation, index: usize, x: f64, z: f64, heading: Option<f64>) {
    let tank = &mut s.tanks[index];
    let heading = heading.unwrap_or(tank.heading);
    tank.heading = heading;
    tank.previous = Vec2::new(x, z);
    tank.brain.last = Vec2::new(x, z);
    let body = &mut s.world.bodies[tank.body];
    body.set_translation(vector(x, 0.65, z), true);
    body.set_rotation(to_rotation(Quat4::yaw(heading)), true);
    body.set_linvel(vector(0.0, 0.0, 0.0), true);
    body.set_angvel(vector(0.0, 0.0, 0.0), true);
}

/// The index of the tank with `id`.
pub fn tank_index(s: &Simulation, id: u32) -> usize {
    s.tank_index(id).expect("tank exists")
}

/// Planar body position of a live tank.
pub fn tank_xz(s: &Simulation, index: usize) -> Vec2 {
    s.body_translation(s.tanks[index].body).planar()
}
