//! The shared fixtures leave a consistent world: only kept tanks, no cover, open navigation.

mod support;

use sloppy_core::sim::VehicleCommand;
use support::{clear_arena, place_tank, simulation, tank_xz};

#[test]
fn clear_arena_keeps_only_the_requested_tanks() {
    let mut s = simulation();
    let human = s.human_index().unwrap();
    clear_arena(&mut s, &[human, 0]);
    assert_eq!(s.tanks.len(), 2);
    assert!(s.tanks[0].human);
    assert!(s.covers.is_empty() && s.pickups.is_empty());
    assert_eq!(s.nav.blocked.iter().filter(|&&cell| cell != 0).count(), 0);
    // Ground plus two tanks remain.
    assert_eq!(s.world.bodies.len(), 3);
    place_tank(&mut s, 0, 1.0, 2.0, Some(0.0));
    s.world.step();
    let p = tank_xz(&s, 0);
    assert!((p.x - 1.0).abs() < 0.01 && (p.z - 2.0).abs() < 0.01);
    s.start();
    for _ in 0..30 {
        s.step(VehicleCommand::idle(), false);
    }
    assert_eq!(s.tanks.len(), 2);
}
