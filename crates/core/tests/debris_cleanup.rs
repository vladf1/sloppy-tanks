//! Debris budget and fade (the former `tests/debris-cleanup.test.ts`): which piece is evicted
//! first, gradual cleanup under pressure, and the hard expiry of moving debris.

mod support;

use sloppy_core::sim::data::STEP;
use sloppy_core::sim::debris_cleanup::{cleanup_candidate, prepare_debris_cleanup};
use sloppy_core::sim::physics::vector;
use sloppy_core::sim::{FragmentShape, Simulation};
use support::idle;

/// The default-seed arena with every tank removed.
fn arena() -> Simulation {
    let mut sim = Simulation::with_seed(12345.0);
    let bodies: Vec<_> = sim.tanks.iter().map(|tank| tank.body).collect();
    for body in bodies {
        sim.remove_body(body);
    }
    sim.tanks.clear();
    sim.start();
    sim
}

/// Adds a two-second shard at x and returns its fragment id; settled pieces are put to sleep.
fn fragment(sim: &mut Simulation, x: f64, moving: bool) -> u32 {
    sim.fragment(x, 0.0, 0x805336, 0.5, FragmentShape::Shard, 1.0);
    let f = sim.fragments.last_mut().unwrap();
    f.life = 2.0;
    let (id, body) = (f.id, f.body);
    if !moving {
        sim.world.bodies[body].sleep();
    }
    id
}

fn index_of(sim: &Simulation, id: u32) -> Option<usize> {
    sim.fragments.iter().position(|f| f.id == id)
}

#[test]
fn budget_eviction_preserves_nearby_moving_debris_ahead_of_distant_settled_pieces() {
    let mut sim = arena();
    let nearby = fragment(&mut sim, 0.0, true);
    let distant = fragment(&mut sim, 40.0, false);
    let distant_body = sim.fragments[index_of(&sim, distant).unwrap()].body;
    assert_eq!(cleanup_candidate(&sim), index_of(&sim, distant));
    sim.max_fragments = 2;
    fragment(&mut sim, 2.0, true);
    assert!(index_of(&sim, nearby).is_some());
    assert!(index_of(&sim, distant).is_none());
    assert!(!sim.world.bodies.contains(distant_body));
    assert_eq!(sim.fragments.len(), 2);
}

#[test]
fn pressure_starts_a_gradual_cleanup_of_old_settled_pieces_without_deleting_them() {
    let mut sim = arena();
    sim.max_fragments = 5;
    let settled = fragment(&mut sim, 40.0, false);
    for i in 0..4 {
        fragment(&mut sim, i as f64, true);
    }
    prepare_debris_cleanup(&mut sim);
    assert_eq!(sim.fragments[index_of(&sim, settled).unwrap()].life, 1.0);
    assert_eq!(sim.fragments.len(), 5);
    assert!(
        sim.fragments
            .iter()
            .filter(|f| f.id != settled)
            .all(|f| f.life == 2.0)
    );
    prepare_debris_cleanup(&mut sim);
    assert_eq!(sim.fragments.iter().filter(|f| f.life == 1.0).count(), 1);
}

#[test]
fn moving_substantial_debris_delays_cleanup_but_still_obeys_its_hard_expiration() {
    let mut sim = arena();
    let id = fragment(&mut sim, 0.0, true);
    let i = index_of(&sim, id).unwrap();
    let body = sim.fragments[i].body;
    sim.world.bodies[body].set_translation(vector(0.0, 10.0, 0.0), true);
    sim.world.bodies[body].set_linvel(vector(3.0, 0.0, 0.0), true);
    sim.fragments[i].life = 1.0 + STEP / 2.0;
    sim.fragments[i].expires_at = Some(18.0);
    idle(&mut sim);
    let i = index_of(&sim, id).unwrap();
    assert!(
        sim.fragments[i].life > 1.0,
        "moving piece has not started sinking"
    );
    sim.fragments[i].expires_at = Some(sim.elapsed + 1.0);
    idle(&mut sim);
    let i = index_of(&sim, id).unwrap();
    assert!(
        sim.fragments[i].life < 1.0,
        "hard expiration starts cleanup even while moving"
    );
    for _ in 0..61 {
        idle(&mut sim);
    }
    assert!(index_of(&sim, id).is_none());
}
