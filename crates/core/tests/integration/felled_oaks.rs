//! A felled oak (`oak_structure`): two standard shells snap its butt, its trunk and limbs
//! tip over as one body, land and break apart, and lie on the ground, not in it, while
//! its rooted stump stays for tanks alone.

use crate::support::{clear_arena, collider_groups, fire_at, place_tank, run};

use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::data::group;
use sloppy_core::sim::math::Vec2;
use sloppy_core::sim::oak_structure::OAK_BUTT;
use sloppy_core::sim::structure::PartSet;
use sloppy_core::sim::{CoverKind, Simulation, Weapon};

/// An oak's place on the standard arena, sized as the arena's trees are.
const OAK: (f64, f64) = (-36.0, -45.0);

/// A cleared arena with one oak standing, the human far off.
fn lone_oak() -> (Simulation, usize) {
    let mut s = Simulation::with_seed(123.0);
    s.start();
    let human = s.human_index().expect("a human tank");
    clear_arena(&mut s, &[human]);
    place_tank(&mut s, 0, 40.0, 40.0, None);
    let (x, z) = OAK;
    let def = CoverDef::new(CoverKind::Tree, x, z, 2.6, 2.6, 5.8, 80.0, 0x169f65);
    let oak = s.add_cover(&def);
    s.nav.rebuild(&s.covers, None);
    assert!(
        s.covers[oak].structure.is_some(),
        "an oak stands as a structure"
    );
    s.world.step();
    (s, oak)
}

#[test]
fn a_felled_oak_breaks_apart_on_the_ground_and_leaves_its_stump() {
    let (mut s, oak) = lone_oak();
    let (x, z) = OAK;
    for _ in 0..2 {
        let (from, to) = (Vec2::new(x, z - 10.0), Vec2::new(x, z));
        fire_at(&mut s, from, to, 1.2, Weapon::Standard);
        run(&mut s, 0.5);
    }
    assert!(!s.covers[oak].alive, "two standard shells fell an oak");

    run(&mut s, 6.0);
    let fallen: Vec<_> = s
        .fragments
        .iter()
        .filter(|piece| {
            piece
                .structure
                .as_ref()
                .is_some_and(|fallen| fallen.cover == oak)
        })
        .collect();
    assert!(
        fallen.len() >= 4,
        "the trunk and its limbs break apart where they land: {} bodies",
        fallen.len()
    );
    for piece in &fallen {
        let body = &s.world.bodies[piece.body];
        let lowest = body
            .colliders()
            .iter()
            .map(|&collider| s.world.colliders[collider].compute_aabb().mins.y)
            .fold(f32::INFINITY, f32::min);
        assert!(
            lowest > -0.15,
            "a fallen part lies on the ground, not in it: {lowest}"
        );
    }
    assert_eq!(
        collider_groups(&s, s.covers[oak].body),
        group::STUMP_CONTACT,
        "the stump stays for tanks alone"
    );
}

#[test]
fn a_felled_oak_reaches_room_clients_and_late_joiners() {
    let (mut s, oak) = lone_oak();
    crate::support::assert_structure_replicates(&mut s, oak, &[PartSet::from_indices([OAK_BUTT])]);
}
