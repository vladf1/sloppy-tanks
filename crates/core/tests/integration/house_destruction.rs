//! Destructible village cottages: their hit points, the collapse into the parts
//! they are built from, the heap of rubble they leave as low cover, navigation
//! through the opened footprint, rebuilding in place and a reset restoring them.

use crate::support;

use glam::DVec3;
use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::data::{STEP, group};
use sloppy_core::sim::house_layout::{
    HOUSE_HEALTH, HOUSE_HEAP, HouseDesign, HousePart, HousePiece, is_house_heap,
};

use sloppy_core::sim::math::{Random, Vec2};
use sloppy_core::sim::{CoverKind, DamageCause, FragmentShape, SimEventType, Simulation, Team};
use support::{clear_arena, collider_groups, event_count, idle};

/// A standard shell's damage.
const SHELL: f64 = 40.0;

fn village() -> Simulation {
    let mut s = Simulation::with_seed(2024.0);
    s.start();
    s
}

fn houses(s: &Simulation) -> Vec<usize> {
    (0..s.covers.len())
        .filter(|&i| s.covers[i].kind == CoverKind::House)
        .collect()
}

fn heaps(s: &Simulation) -> Vec<usize> {
    (0..s.covers.len())
        .filter(|&i| s.covers[i].kind == CoverKind::Rubble && is_house_heap(s.covers[i].h))
        .collect()
}

fn design(s: &Simulation, house: usize) -> HouseDesign {
    let c = &s.covers[house];
    HouseDesign::of(c.x, c.z, c.w, c.d, c.h)
}

/// A point of the cottage's build frame in the world.
fn at(s: &Simulation, house: usize, local: DVec3) -> Vec2 {
    let c = &s.covers[house];
    let p = design(s, house).to_world(local);
    Vec2::new(c.x + p.x, c.z + p.z)
}

fn tick(s: &mut Simulation, seconds: f64) {
    for _ in 0..(seconds / STEP).round() as usize {
        idle(s);
    }
}

fn shell(s: &mut Simulation, house: usize) {
    let (id, team) = (s.human().id, s.human_team);
    s.damage_cover(house, SHELL, id, team, None, None);
}

#[test]
fn every_village_cottage_takes_fifteen_shells_and_keeps_their_marks() {
    let mut s = village();
    let all = houses(&s);
    assert_eq!(all.len(), 14);
    for &house in &all {
        assert!(s.covers[house].destructible);
        assert_eq!(s.covers[house].max_hp, HOUSE_HEALTH);
    }
    let house = all[0];
    let shells = (HOUSE_HEALTH / SHELL).ceil() as usize;
    for i in 1..shells {
        shell(&mut s, house);
        assert!(s.covers[house].alive, "standing after {i} shells");
    }
    assert_eq!(
        s.covers[house].timber_hits.len(),
        6,
        "it keeps the marks of the first shells to hit it"
    );
    shell(&mut s, house);
    assert!(!s.covers[house].alive, "down after {shells} shells");
    assert!(shells >= 10, "levelling a cottage is a deliberate effort");
}

/// An empty arena with one cottage where the village's first one stands.
fn lone_cottage() -> (Simulation, usize) {
    let mut s = Simulation::with_seed(731.0);
    clear_arena(&mut s, &[]);
    let house = s.add_cover(&CoverDef::house(-45.0, -39.0, 5.0, 6.0, 4.6, 0xb87b4c));
    s.nav.rebuild(&s.covers, None);
    s.start();
    s.world.step();
    (s, house)
}

#[test]
fn a_collapse_breaks_the_cottage_into_its_parts_and_leaves_a_heap_of_low_cover() {
    let (mut s, house) = lone_cottage();
    let bodies = s.world.bodies.len();
    let covers = s.covers.len();
    let design = design(&s, house);
    let state = s.rng.state;
    s.damage_cover(house, 10_000.0, 999, Team::Blue, None, None);
    let mut expected = Random::new(state);
    expected.next();
    assert_eq!(
        s.rng.state, expected.state,
        "one combat draw: the heap's seed"
    );
    assert!(!s.covers[house].alive);
    assert!(event_count(&s, SimEventType::Destroy) >= 1);
    let pieces: Vec<HousePiece> = s.fragments.iter().filter_map(|f| f.house_piece).collect();
    let shapes = design.pieces();
    assert_eq!(pieces.len(), shapes.len(), "every part falls once");
    for (piece, shape) in pieces.iter().zip(&shapes) {
        assert_eq!(*piece, shape.piece);
    }
    for f in &s.fragments {
        assert_eq!(f.source_kind, Some(CoverKind::House));
        let mound = f
            .house_piece
            .is_some_and(|piece| piece.part == HousePart::Mound);
        // A stub of wall is led down onto the ground and the rubble mound lies
        // where it heaps; everything else falls.
        if f.house_stub.is_some() {
            assert!(s.world.bodies[f.body].is_kinematic());
            let piece = f.house_piece.expect("a stub is a cottage's shard");
            assert!(piece.part.wall_index().is_some() && piece.shard.is_multiple_of(2));
        } else if mound {
            assert!(s.world.bodies[f.body].is_fixed());
        } else {
            assert!(s.world.bodies[f.body].is_dynamic());
        }
        // The shards rest on the mound, which nothing else touches.
        let groups = if mound {
            group::HOUSE_MOUND
        } else if f.house_piece.is_some() {
            group::HOUSE_SHARD
        } else {
            group::PUSHABLE_DEBRIS
        };
        assert_eq!(collider_groups(&s, f.body), groups);
        if let Some(piece) = f.house_piece {
            assert_eq!(f.house_id, Some(s.covers[house].id));
            let shape = shapes.iter().find(|shape| shape.piece == piece).unwrap();
            let start = at(&s, house, shape.center);
            let p = s.body_translation(f.body);
            assert!(
                (p.x - start.x).abs() < 1e-4 && (p.z - start.z).abs() < 1e-4,
                "{piece:?} starts where it stood"
            );
            assert!((p.y - shape.center.y).abs() < 1e-4);
        } else {
            assert_eq!(
                f.shape,
                Some(FragmentShape::Beam),
                "loose rafters and boards"
            );
        }
    }
    let heap = heaps(&s);
    assert_eq!(heap.len(), 1, "one heap");
    let heap = heap[0];
    assert_eq!(s.covers.len(), covers + 1);
    assert!(s.covers[heap].alive && !s.covers[heap].destructible);
    let height = s.covers[heap].h;
    assert_eq!(height, HOUSE_HEAP.height);
    assert!(height > 1.0 && height < 1.4, "low cover");
    assert!(
        s.nav
            .is_blocked(Vec2::new(s.covers[heap].x, s.covers[heap].z))
    );
    // Everything comes down: no piece is left standing to the old ridge.
    tick(&mut s, 3.0);
    for f in s.fragments.iter().filter(|f| f.house_piece.is_some()) {
        let y = s.body_translation(f.body).y;
        assert!(y < design.h * 0.6, "{:?} came down ({y})", f.house_piece);
    }
    // The pieces lie on the heap a while, then sink into it, leaving the heap's body
    // for the cottage's.
    tick(&mut s, 5.0);
    assert!(
        s.fragments.iter().any(|f| f.house_piece.is_some()),
        "the collapse stays to be seen"
    );
    tick(&mut s, 6.0);
    assert!(s.fragments.is_empty());
    assert_eq!(s.world.bodies.len(), bodies);
}

#[test]
fn tanks_can_drive_through_the_open_front_of_a_collapsed_cottage() {
    let mut s = village();
    for house in houses(&s) {
        let design = design(&s, house);
        let front = at(&s, house, DVec3::new(0.0, 0.0, design.d / 2.0 - 1.2));
        let (left, right) = (
            at(
                &s,
                house,
                DVec3::new(-design.w / 2.0 - 4.0, 0.0, design.d / 2.0 - 1.2),
            ),
            at(
                &s,
                house,
                DVec3::new(design.w / 2.0 + 4.0, 0.0, design.d / 2.0 - 1.2),
            ),
        );
        assert!(s.nav.is_blocked(front), "the standing cottage blocks");
        let (id, team) = (s.human().id, s.human_team);
        s.damage_cover(house, 10_000.0, id, team, None, None);
        assert!(!s.nav.is_blocked(front), "its front opens");
        let path = s.nav.find(left, right);
        assert!(
            path.iter()
                .any(|p| (p.x - front.x).abs() < 1.6 && (p.z - front.z).abs() < 1.6),
            "the shortest way runs through the old footprint"
        );
    }
    for p in [Vec2::new(-53.0, 0.0), Vec2::new(53.0, 0.0)] {
        assert!(!s.nav.find(p, Vec2::ZERO).is_empty());
    }
}

#[test]
fn a_rebuilt_cottage_clears_its_heap_and_falls_again_onto_the_same_one() {
    let mut s = village();
    let house = houses(&s)[5];
    let place = Vec2::new(s.covers[house].x, s.covers[house].z);
    let (id, team) = (s.human().id, s.human_team);
    s.damage_cover(house, 10_000.0, id, team, None, None);
    let covers = s.covers.len();
    s.restore_cover(house);
    assert!(s.covers[house].alive);
    assert_eq!(s.covers[house].hp, HOUSE_HEALTH);
    assert!(s.covers[house].timber_hits.is_empty());
    assert!(heaps(&s).iter().all(|&heap| !s.covers[heap].alive));
    assert!(s.nav.is_blocked(place));
    s.damage_cover(house, 10_000.0, id, team, None, None);
    assert_eq!(s.covers.len(), covers, "the heap is reused");
    assert_eq!(
        heaps(&s)
            .iter()
            .filter(|&&heap| s.covers[heap].alive)
            .count(),
        1
    );
}

#[test]
fn a_reset_restores_every_cottage_and_the_body_count() {
    let mut s = village();
    let bodies = s.world.bodies.len();
    let covers = s.covers.len();
    let (id, team) = (s.human().id, s.human_team);
    for house in houses(&s) {
        s.damage_cover(house, 10_000.0, id, team, None, None);
    }
    assert_eq!(heaps(&s).len(), 14);
    tick(&mut s, 1.0);
    s.reset(None);
    s.start();
    assert_eq!(s.covers.len(), covers);
    assert!(heaps(&s).is_empty());
    for house in houses(&s) {
        assert!(s.covers[house].alive);
        assert_eq!(s.covers[house].hp, HOUSE_HEALTH);
    }
    assert_eq!(s.world.bodies.len(), bodies);
}

#[test]
fn a_drum_blast_beside_a_cottage_hurts_it_once_and_bots_never_breach_cottages() {
    let mut s = village();
    let house = houses(&s)[0];
    let hp = s.covers[house].hp;
    let c = &s.covers[house];
    let beside = Vec2::new(c.x + c.w / 2.0 + 1.0, c.z);
    let (id, team) = (s.human().id, s.human_team);
    s.explode(beside, 3.0, 30.0, id, team, None, DamageCause::Explosion);
    assert_eq!(s.covers[house].hp, hp - 30.0);
    assert!(s.covers[house].kick.is_some(), "a blast pushes it over");
    // A whole autoplay minute of bots leaves most of the village standing.
    let mut s = Simulation::with_seed(77.0);
    s.start();
    tick(&mut s, 60.0);
    let standing = houses(&s).iter().filter(|&&h| s.covers[h].alive).count();
    assert!(
        standing >= 12,
        "{standing} of 14 cottages stand after a minute"
    );
}

/// A collapse settles into a ruin: most of each cottage's lower walls stand on as
/// stubs round its floor, leaning in against the pile, and its upper walls lie on
/// the pile or the ground, at most a couple in a cottage (wedged in the wreck)
/// left on edge, and few in the village.
#[test]
fn every_cottage_collapse_leaves_stubs_of_wall_round_a_pile() {
    let village = village();
    let mut standing = Vec::new();
    for house in houses(&village) {
        let c = &village.covers[house];
        let (x, z, w, d, h) = (c.x, c.z, c.w, c.d, c.h);
        let mut s = Simulation::with_seed(731.0);
        clear_arena(&mut s, &[]);
        let house = s.add_cover(&CoverDef::house(x, z, w, d, h, 0xb87b4c));
        s.nav.rebuild(&s.covers, None);
        s.start();
        s.world.step();
        let design = HouseDesign::of(x, z, w, d, h);
        s.damage_cover(house, 100_000.0, 999, Team::Blue, None, None);
        tick(&mut s, 4.0);
        let walls = sloppy_core::sim::house_layout::wall_frames(design.w, design.d);
        let (mut stubs, mut lower) = (0, 0);
        let mut here = Vec::new();
        for fragment in &s.fragments {
            let Some(piece) = fragment.house_piece else {
                continue;
            };
            let Some(wall) = piece.part.wall_index() else {
                continue;
            };
            let out = walls[wall].1;
            let body = &s.world.bodies[fragment.body];
            let face = *body.rotation() * glam::Vec3::new(out.x as f32, out.y as f32, out.z as f32);
            if wall != 1 && piece.shard.is_multiple_of(2) {
                lower += 1;
                // Standing, its outside face tipped up a little: leaning in.
                if (0.03..0.6).contains(&face.y) {
                    stubs += 1;
                }
            } else if face.y.abs() < 0.5 {
                here.push(format!("{x} {z}: {}", String::from(piece)));
            }
        }
        assert!(
            stubs * 2 >= lower,
            "{x} {z}: {stubs} of {lower} stubs stand"
        );
        assert!(here.len() <= 2, "{here:?}");
        standing.extend(here);
    }
    // Five on arm64; the x86-64 CI's physics leaves one more on edge.
    assert!(standing.len() <= 6, "{standing:?}");
}

/// The roof pancakes onto the wreck: its slabs come down as they stood and lie
/// shingles up on the pile or lean on the stubs of wall: of the village's some
/// hundred and forty slabs a very few (five at most: debug and release
/// builds round the tumble differently) turn over onto their rafters, and few are
/// left steep.
#[test]
fn every_cottage_roof_comes_down_shingles_up() {
    let village = village();

    let (mut flipped, mut steep, mut slabs) = (Vec::new(), Vec::new(), 0);
    for house in houses(&village) {
        let c = &village.covers[house];
        let (x, z, w, d, h) = (c.x, c.z, c.w, c.d, c.h);
        let mut s = Simulation::with_seed(731.0);
        clear_arena(&mut s, &[]);
        let house = s.add_cover(&CoverDef::house(x, z, w, d, h, 0xb87b4c));
        s.nav.rebuild(&s.covers, None);
        s.start();
        s.world.step();
        let pitch = HouseDesign::of(x, z, w, d, h).roof().pitch;
        s.damage_cover(house, 100_000.0, 999, Team::Blue, None, None);
        // Coming down, the slabs keep the roof's pitch.
        tick(&mut s, 0.5);
        for fragment in &s.fragments {
            if fragment.house_piece.is_some_and(|p| p.part.is_roof()) {
                let q = s.world.bodies[fragment.body].rotation();
                let turn = HouseDesign::of(x, z, w, d, h).yaw() as f32;
                let start = glam::Quat::from_rotation_y(turn);
                assert!(
                    q.dot(start).abs() > 0.999,
                    "{x} {z}: the roof falls as it stood"
                );
            }
        }
        tick(&mut s, 3.5);
        for fragment in &s.fragments {
            let Some(piece) = fragment.house_piece.filter(|p| p.part.is_roof()) else {
                continue;
            };
            let top = match piece.part {
                HousePart::RightRoof => glam::Vec3::new(pitch as f32, 1.0, 0.0),
                HousePart::LeftRoof => glam::Vec3::new(-pitch as f32, 1.0, 0.0),
                HousePart::FrontRoof => glam::Vec3::new(0.0, 1.0, pitch as f32),
                _ => glam::Vec3::new(0.0, 1.0, -pitch as f32),
            }
            .normalize();
            let up = (*s.world.bodies[fragment.body].rotation() * top).y;
            slabs += 1;
            if up < -0.3 {
                flipped.push(format!("{x} {z}: {} ({up:.2})", String::from(piece)));
            } else if up < 0.3 {
                steep.push(format!("{x} {z}: {} ({up:.2})", String::from(piece)));
            }
        }
    }
    assert!(flipped.len() <= 5, "{flipped:?}");
    assert!(steep.len() * 10 <= slabs, "{steep:?} of {slabs}");
}

/// A collapse heaps up rather than spreading flat: the cottage's walls and floors
/// crush into a rubble mound in its footprint that never moves and that tanks,
/// shells and other debris pass through, and the roof comes down onto it, most of
/// its slabs left lying well up the heap and none far out across the yard.
#[test]
fn every_cottage_collapse_heaps_its_roof_on_a_rubble_mound() {
    let village = village();
    for house in houses(&village) {
        let c = &village.covers[house];
        let (x, z, w, d, h) = (c.x, c.z, c.w, c.d, c.h);
        let mut s = Simulation::with_seed(731.0);
        clear_arena(&mut s, &[]);
        let house = s.add_cover(&CoverDef::house(x, z, w, d, h, 0xb87b4c));
        s.nav.rebuild(&s.covers, None);
        s.start();
        s.world.step();
        s.damage_cover(house, 100_000.0, 999, Team::Blue, None, None);
        let mound = |s: &Simulation| {
            s.fragments
                .iter()
                .find(|f| f.house_piece.is_some_and(|p| p.part == HousePart::Mound))
                .map(|f| (f.body, s.body_translation(f.body)))
        };
        let (body, start) = mound(&s).expect("the collapse heaps a mound");
        assert_eq!(collider_groups(&s, body) >> 16 & (group::TANK >> 16), 0);
        assert_eq!(group::TANK & 0xffff & (group::HOUSE_MOUND >> 16), 0);
        assert_eq!(group::DEBRIS_QUERY & 0xffff & (group::HOUSE_MOUND >> 16), 0);
        tick(&mut s, 4.0);
        let (_, end) = mound(&s).expect("the mound lies with the pile");
        assert!((end.x - start.x).abs() + (end.y - start.y).abs() + (end.z - start.z).abs() < 1e-6);
        let (mut raised, mut slabs) = (0, 0);
        for f in &s.fragments {
            if !f.house_piece.is_some_and(|p| p.part.is_roof()) {
                continue;
            }
            let p = s.body_translation(f.body);
            slabs += 1;
            if p.y > 0.75 {
                raised += 1;
            }
            assert!(
                (p.x - x).abs() < w / 2.0 + 2.0 && (p.z - z).abs() < d / 2.0 + 2.0,
                "{x} {z}: {:?} came down on the wreck ({p:?})",
                f.house_piece
            );
        }
        assert!(
            raised * 2 >= slabs,
            "{x} {z}: {raised} of {slabs} slabs up the heap"
        );
    }
}
