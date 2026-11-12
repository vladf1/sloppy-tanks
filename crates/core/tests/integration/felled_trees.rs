//! A felled tree of every family (`tree_structure`): two standard shells snap its butt,
//! its trunk and limbs tip over as one body, the limbs that strike the ground snap off
//! under it, and it lies on the ground, not in it or up on its limbs, while its rooted
//! stump stays for tanks alone. Shells fly through its leaves and a conifer's boughs;
//! a broadleaf's limbs snap off to a shell and snap in two where they lie.

use crate::support::{clear_arena, collider_groups, fire_at, idle, place_tank, run};

use sloppy_core::models::TREE_FAMILIES;
use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::data::group;
use sloppy_core::sim::grown_layout::TreePart;
use sloppy_core::sim::math::{Point3, Vec2};
use sloppy_core::sim::physics::interaction_groups;
use sloppy_core::sim::structure::{PartSet, Role};
use sloppy_core::sim::tree_proportions::tree_proportions;
use sloppy_core::sim::tree_structure::{TREE_BUTT, TREE_TRUNK};
use sloppy_core::sim::{CoverKind, SimEventType, Simulation, Weapon};

/// Near an oak's place on the standard arena, trees sized as the arena's are.
const NEAR: (f64, f64) = (-36.0, -45.0);
const SIZE: [f64; 3] = [2.6, 2.6, 5.8];
/// Families whose limbs are a broadleaf's: oak, birch and aspen.
const BROADLEAVES: [u32; 3] = [3, 4, 5];

/// A place near [`NEAR`] where a tree of `family` grows.
fn place_of(family: u32) -> (f64, f64) {
    let [w, d, h] = SIZE;
    (0..400)
        .map(|step| (NEAR.0 + f64::from(step) * 0.37, NEAR.1))
        .find(|&(x, z)| tree_proportions(x, z, w, d, h).family == family)
        .expect("every family grows somewhere near")
}

/// A cleared arena with one tree of `family` standing, the human far off.
fn lone_tree(family: u32) -> (Simulation, usize, (f64, f64)) {
    let mut s = Simulation::with_seed(123.0);
    s.start();
    let human = s.human_index().expect("a human tank");
    clear_arena(&mut s, &[human]);
    place_tank(&mut s, 0, 40.0, 40.0, None);
    let (x, z) = place_of(family);
    let [w, d, h] = SIZE;
    let def = CoverDef::new(CoverKind::Tree, x, z, w, d, h, 80.0, 0x169f65);
    let tree = s.add_cover(&def);
    s.nav.rebuild(&s.covers, None);
    assert!(
        s.covers[tree].structure.is_some(),
        "a {} stands as a structure",
        TREE_FAMILIES[family as usize]
    );
    s.world.step();
    (s, tree, (x, z))
}

/// Fell the tree with two standard shells on its butt and let it come to rest.
fn fell(s: &mut Simulation, tree: usize, (x, z): (f64, f64), name: &str) {
    for _ in 0..2 {
        fire_at(
            s,
            Vec2::new(x, z - 10.0),
            Vec2::new(x, z),
            1.2,
            Weapon::Standard,
        );
        run(s, 0.5);
    }
    assert!(!s.covers[tree].alive, "two standard shells fell a {name}");
    run(s, 6.0);
}

/// The fallen bodies of `tree`.
fn fallen(s: &Simulation, tree: usize) -> Vec<usize> {
    (0..s.fragments.len())
        .filter(|&index| {
            s.fragments[index]
                .structure
                .as_ref()
                .is_some_and(|fallen| fallen.cover == tree)
        })
        .collect()
}

#[test]
fn a_felled_tree_breaks_apart_on_the_ground_and_leaves_its_stump() {
    for family in 0..6 {
        let name = TREE_FAMILIES[family as usize];
        let (mut s, tree, place) = lone_tree(family);
        fell(&mut s, tree, place, name);
        let fallen = fallen(&s, tree);
        assert!(
            fallen.len() >= 2,
            "the limbs a {name} strikes the ground with snap off: {} bodies",
            fallen.len()
        );
        // The limbs that held it up are gone from under it: its trunk lies on the ground.
        let trunk = fallen
            .iter()
            .map(|&index| &s.fragments[index])
            .find(|piece| piece.structure.as_ref().unwrap().parts.contains(TREE_TRUNK))
            .expect("the trunk lies on");
        let trunk_low = s.world.bodies[trunk.body]
            .colliders()
            .iter()
            .map(|&collider| &s.world.colliders[collider])
            .filter(|collider| collider.user_data as usize == TREE_TRUNK)
            .map(|collider| collider.compute_aabb().mins.y)
            .fold(f32::INFINITY, f32::min);
        assert!(
            trunk_low < 0.3,
            "a {name}'s trunk lies on the ground: {trunk_low}"
        );
        for &index in &fallen {
            let body = &s.world.bodies[s.fragments[index].body];
            let lowest = body
                .colliders()
                .iter()
                .map(|&collider| s.world.colliders[collider].compute_aabb().mins.y)
                .fold(f32::INFINITY, f32::min);
            assert!(
                lowest > -0.15,
                "a {name}'s fallen part lies on the ground, not in it: {lowest}"
            );
        }
        assert_eq!(
            collider_groups(&s, s.covers[tree].body),
            group::STUMP_CONTACT,
            "a {name}'s stump stays for tanks alone"
        );
    }
}

#[test]
fn a_felled_tree_reaches_room_clients_and_late_joiners() {
    for family in 0..6 {
        let (mut s, tree, _) = lone_tree(family);
        crate::support::assert_structure_replicates(
            &mut s,
            tree,
            &[PartSet::from_indices([TREE_BUTT])],
        );
    }
}

/// Shells through the crown, clear of the trunk, rustle its leaves and fly on, the tree
/// unhurt.
#[test]
fn shells_fly_through_a_trees_leaves_rustling_them() {
    for family in 0..6 {
        let name = TREE_FAMILIES[family as usize];
        let (mut s, tree, (x, z)) = lone_tree(family);
        let mut rustled = 0;
        for height in [2.4, 3.0, 3.6, 4.2, 4.8] {
            for offset in [-1.6, -1.1, 1.1, 1.6] {
                let before = s.covers[tree].structure.as_ref().unwrap().gone;
                let (from, to) = (
                    Vec2::new(x + offset, z - 12.0),
                    Vec2::new(x + offset, z + 12.0),
                );
                let id = fire_at(&mut s, from, to, height, Weapon::Standard);
                let mut rustle = false;
                for _ in 0..40 {
                    idle(&mut s);
                    rustle |= s.events.iter().any(|event| {
                        event.kind == SimEventType::Rustle && event.id == Some(s.covers[tree].id)
                    });
                    s.events.clear();
                }
                let gone = s.covers[tree].structure.as_ref().unwrap().gone;
                // Still flying, past the tree.
                if rustle && gone == before && s.shots.iter().any(|shot| shot.id == id) {
                    rustled += 1;
                }
            }
        }
        assert!(rustled > 0, "a shell flies through a {name}'s leaves");
        assert!(
            s.covers[tree].alive,
            "shots through a {name}'s crown never fell it"
        );
        assert_eq!(
            s.covers[tree].hp, s.covers[tree].max_hp,
            "nor hurt its trunk"
        );
    }
}

/// A conifer's boughs are too thin for a shell to meet: every shell through its crown
/// clear of the trunk flies on and leaves the tree whole.
#[test]
fn shells_fly_through_a_conifers_boughs() {
    for family in 0..3 {
        let name = TREE_FAMILIES[family as usize];
        let (mut s, tree, (x, z)) = lone_tree(family);
        for height in [1.5, 2.4, 3.3, 4.2] {
            for offset in [-1.2, -0.7, 0.7, 1.2] {
                let (from, to) = (
                    Vec2::new(x + offset, z - 12.0),
                    Vec2::new(x + offset, z + 12.0),
                );
                let id = fire_at(&mut s, from, to, height, Weapon::Standard);
                run(&mut s, 0.6);
                assert!(
                    s.shots.iter().any(|shot| shot.id == id),
                    "a shell at {height} m, {offset} m aside flies through a {name}"
                );
            }
        }
        assert!(
            s.covers[tree].structure.as_ref().unwrap().gone.is_empty(),
            "a {name} keeps every bough"
        );
    }
}

/// A shell on a broadleaf limb snaps that limb off whole, and the tree stands.
#[test]
fn a_shell_snaps_a_limb_off_a_standing_broadleaf() {
    for family in BROADLEAVES {
        let name = TREE_FAMILIES[family as usize];
        let (mut s, tree, (x, z)) = lone_tree(family);
        let layout = s.covers[tree].structure.as_ref().unwrap().layout.clone();
        // The limb that reaches out furthest toward the shell, so it meets that first.
        let limb = (0..layout.parts.len())
            .filter(|&index| matches!(layout.parts[index].role, Role::Tree(TreePart::Limb(_))))
            .min_by(|&a, &b| layout.parts[a].min.z.total_cmp(&layout.parts[b].min.z))
            .unwrap();
        let part = &layout.parts[limb];
        let middle = part.middle();
        let (from, to) = (
            Vec2::new(x + middle.x, z + part.min.z - 12.0),
            Vec2::new(x + middle.x, z + middle.z),
        );
        fire_at(&mut s, from, to, middle.y, Weapon::Standard);
        run(&mut s, 1.0);
        let gone = s.covers[tree].structure.as_ref().unwrap().gone;
        assert!(gone.contains(limb), "a {name}'s limb snaps off");
        assert!(
            gone.iter()
                .all(|part| matches!(layout.parts[part].role, Role::Tree(TreePart::Limb(_)))),
            "only limbs come off a shot {name}'s crown"
        );
        assert!(s.covers[tree].alive, "the {name} stands");
        assert!(
            s.fragments.iter().any(|piece| piece
                .structure
                .as_ref()
                .is_some_and(|fallen| fallen.cover == tree && fallen.parts.contains(limb))),
            "its limb falls whole"
        );
    }
}

/// The index of a fallen body of `tree` that is one of its limbs alone, if any.
fn lone_limb(s: &Simulation, tree: usize) -> Option<usize> {
    let layout = s.covers[tree].structure.as_ref().unwrap().layout.clone();
    fallen(s, tree).into_iter().find(|&index| {
        let parts = s.fragments[index].structure.as_ref().unwrap().parts;
        parts.len() == 1
            && matches!(
                layout.part(parts.iter().next().unwrap()).role,
                Role::Tree(TreePart::Limb(_))
            )
    })
}

/// A broadleaf's fallen limb a shell meets snaps in two where it lies; it never bursts
/// and vanishes.
#[test]
fn a_shot_fallen_limb_snaps_in_two() {
    for family in BROADLEAVES {
        let name = TREE_FAMILIES[family as usize];
        let (mut s, tree, place) = lone_tree(family);
        fell(&mut s, tree, place, name);
        let layout = s.covers[tree].structure.as_ref().unwrap().layout.clone();
        let limb = lone_limb(&s, tree).expect("a fallen limb on its own");
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
        let point = Point3::new(middle.x.into(), middle.y.into(), middle.z.into());
        let gave_way = s.break_loose_body(limb, part, point, 0.4);
        assert!(!gave_way, "a {name}'s limb gives a little, never way");
        assert_eq!(
            s.fragments.len(),
            pieces_before + 1,
            "a {name}'s limb snaps in two"
        );
        let snapped = fallen(&s, tree)
            .into_iter()
            .flat_map(|index| s.fragments[index].structure.as_ref().unwrap().parts.iter())
            .filter(|&index| {
                matches!(
                    layout.part(index).role,
                    Role::Tree(TreePart::LimbPiece { .. })
                )
            })
            .count();
        assert_eq!(snapped, 2, "both pieces lie on");
    }
}

/// An armour-piercing shell that meets a broadleaf's fallen limb stops in it: the wood
/// is knocked aside, not shattered out of its path, so a shell flying on would meet it
/// again and again in one tick.
#[test]
fn a_piercing_shell_stops_in_a_fallen_limb() {
    for family in BROADLEAVES {
        let name = TREE_FAMILIES[family as usize];
        let (mut s, tree, place) = lone_tree(family);
        fell(&mut s, tree, place, name);
        let Some(limb) = fallen(&s, tree).into_iter().find(|&index| {
            !s.fragments[index]
                .structure
                .as_ref()
                .unwrap()
                .parts
                .contains(TREE_TRUNK)
        }) else {
            panic!("a {name}'s fallen limb on its own");
        };
        let middle = s.world.bodies[s.fragments[limb].body].center_of_mass();
        let (mx, my, mz) = (
            f64::from(middle.x),
            f64::from(middle.y),
            f64::from(middle.z),
        );
        s.events.clear();
        let id = fire_at(
            &mut s,
            Vec2::new(mx - 8.0, mz),
            Vec2::new(mx, mz),
            my,
            Weapon::Piercing,
        );
        let mut impacts = 0;
        for _ in 0..60 {
            idle(&mut s);
            impacts += s
                .events
                .iter()
                .filter(|event| {
                    event.kind == SimEventType::Impact && event.cover_kind == Some(CoverKind::Tree)
                })
                .count();
            s.events.clear();
        }
        assert_eq!(impacts, 1, "the shell meets a {name}'s limb once");
        assert!(s.shots.iter().all(|shot| shot.id != id), "and stops in it");
    }
}

/// A conifer's fallen boughs bear nothing: they rest on the ground alone, and its trunk,
/// shells and tanks pass through them.
#[test]
fn a_conifers_fallen_boughs_meet_only_the_ground() {
    for family in 0..3 {
        let name = TREE_FAMILIES[family as usize];
        let (mut s, tree, place) = lone_tree(family);
        fell(&mut s, tree, place, name);
        let mut boughs = 0;
        for index in fallen(&s, tree) {
            let body = &s.world.bodies[s.fragments[index].body];
            for &collider in body.colliders() {
                let collider = &s.world.colliders[collider];
                if collider.user_data as usize > TREE_TRUNK {
                    boughs += 1;
                    assert_eq!(
                        collider.collision_groups(),
                        interaction_groups(group::BOUGH_DEBRIS),
                        "a {name}'s fallen bough meets only the ground"
                    );
                }
            }
        }
        assert!(boughs > 0, "a {name} has boughs");
    }
}
