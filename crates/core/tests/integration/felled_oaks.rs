//! A felled oak (`oak_structure`): two standard shells snap its butt, its trunk and limbs
//! tip over as one body, the limbs that strike the ground snap off under it, and it lies
//! on the ground, not in it or up on its limbs, while its rooted stump stays for tanks
//! alone.

use crate::support::{clear_arena, collider_groups, fire_at, place_tank, run};

use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::data::group;
use sloppy_core::sim::math::Vec2;
use sloppy_core::sim::oak_layout::TreePart;
use sloppy_core::sim::oak_structure::{OAK_BUTT, OAK_TRUNK};
use sloppy_core::sim::structure::{PartSet, Role};
use sloppy_core::sim::{CoverKind, SimEventType, Simulation, Weapon};

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
        fallen.len() >= 2,
        "the limbs that strike the ground snap off: {} bodies",
        fallen.len()
    );
    // The limbs that held it up are gone from under it: its trunk lies on the ground.
    let trunk = fallen
        .iter()
        .find(|piece| piece.structure.as_ref().unwrap().parts.contains(OAK_TRUNK))
        .expect("the trunk lies on");
    let trunk_low = s.world.bodies[trunk.body]
        .colliders()
        .iter()
        .map(|&collider| &s.world.colliders[collider])
        .filter(|collider| collider.user_data as usize == OAK_TRUNK)
        .map(|collider| collider.compute_aabb().mins.y)
        .fold(f32::INFINITY, f32::min);
    assert!(trunk_low < 0.3, "the trunk lies on the ground: {trunk_low}");
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

/// Shells through the crown rustle its leaves and fly on, the tree unhurt.
#[test]
fn shells_fly_through_an_oaks_leaves_rustling_them() {
    let (mut s, oak) = lone_oak();
    let (x, z) = OAK;
    let mut rustled = 0;
    for height in [3.0, 3.6, 4.2, 4.8, 5.4] {
        for offset in [-1.6, -0.8, 0.0, 0.8, 1.6] {
            let before = s.covers[oak].structure.as_ref().unwrap().gone;
            let (from, to) = (
                Vec2::new(x + offset, z - 12.0),
                Vec2::new(x + offset, z + 12.0),
            );
            let id = fire_at(&mut s, from, to, height, Weapon::Standard);
            let mut rustle = false;
            for _ in 0..40 {
                crate::support::idle(&mut s);
                rustle |= s.events.iter().any(|event| {
                    event.kind == SimEventType::Rustle && event.id == Some(s.covers[oak].id)
                });
            }
            let gone = s.covers[oak].structure.as_ref().unwrap().gone;
            // Still flying, past the tree.
            if rustle && gone == before && s.shots.iter().any(|shot| shot.id == id) {
                rustled += 1;
            }
        }
    }
    assert!(rustled > 0, "a shell flies through the leaves");
    assert!(
        s.covers[oak].alive,
        "shots through the crown never fell the tree"
    );
    assert_eq!(s.covers[oak].hp, s.covers[oak].max_hp, "nor hurt its trunk");
}

/// A shell on a limb's main branch snaps that limb off whole, and the tree stands.
#[test]
fn a_shell_snaps_a_limb_off_a_standing_oak() {
    let (mut s, oak) = lone_oak();
    let (x, z) = OAK;
    let layout = s.covers[oak].structure.as_ref().unwrap().layout.clone();
    let limb = (0..layout.parts.len())
        .find(|&index| matches!(layout.parts[index].role, Role::Tree(TreePart::Limb(_))))
        .unwrap();
    let middle = layout.parts[limb].middle();
    let (from, to) = (
        Vec2::new(x + middle.x, z + middle.z - 12.0),
        Vec2::new(x + middle.x, z + middle.z),
    );
    fire_at(&mut s, from, to, middle.y, Weapon::Standard);
    run(&mut s, 1.0);
    let gone = s.covers[oak].structure.as_ref().unwrap().gone;
    assert!(gone.contains(limb), "the limb snaps off");
    assert!(
        gone.iter()
            .all(|part| matches!(layout.parts[part].role, Role::Tree(TreePart::Limb(_)))),
        "only limbs come off a shot crown"
    );
    assert!(s.covers[oak].alive, "the tree stands");
    assert!(
        s.fragments.iter().any(|piece| piece
            .structure
            .as_ref()
            .is_some_and(|fallen| fallen.cover == oak && fallen.parts.contains(limb))),
        "it falls whole"
    );
}

/// A fallen limb a shell meets snaps in two where it lies; it never bursts and vanishes.
#[test]
fn a_shot_fallen_limb_snaps_in_two() {
    let (mut s, oak) = lone_oak();
    let (x, z) = OAK;
    for _ in 0..2 {
        fire_at(
            &mut s,
            Vec2::new(x, z - 10.0),
            Vec2::new(x, z),
            1.2,
            Weapon::Standard,
        );
        run(&mut s, 0.5);
    }
    run(&mut s, 6.0);
    let layout = s.covers[oak].structure.as_ref().unwrap().layout.clone();
    let limb = s
        .fragments
        .iter()
        .position(|piece| {
            piece.structure.as_ref().is_some_and(|fallen| {
                fallen.cover == oak
                    && fallen.parts.len() == 1
                    && matches!(
                        layout.part(fallen.parts.iter().next().unwrap()).role,
                        Role::Tree(TreePart::Limb(_))
                    )
            })
        })
        .expect("a fallen limb on its own");
    let part = s.fragments[limb]
        .structure
        .as_ref()
        .unwrap()
        .parts
        .iter()
        .next()
        .unwrap();
    let pieces_before = s.fragments.len();
    let middle = s.world.bodies[s.fragments[limb].body].center_of_mass();
    let point =
        sloppy_core::sim::math::Point3::new(middle.x.into(), middle.y.into(), middle.z.into());
    s.break_loose_body(limb, part, point, 0.4);
    assert_eq!(
        s.fragments.len(),
        pieces_before + 1,
        "the limb snaps in two"
    );
    let snapped: Vec<_> = s
        .fragments
        .iter()
        .filter_map(|piece| piece.structure.as_ref())
        .filter(|fallen| fallen.cover == oak)
        .flat_map(|fallen| fallen.parts.iter())
        .filter(|&index| {
            matches!(
                layout.part(index).role,
                Role::Tree(TreePart::LimbPiece { .. })
            )
        })
        .collect();
    assert_eq!(snapped.len(), 2, "both pieces lie on");
}
