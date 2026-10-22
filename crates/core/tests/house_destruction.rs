//! Destructible village cottages: their hit points, the collapse into the parts
//! they are built from, the heap of rubble they leave as low cover, navigation
//! through the opened footprint, rebuilding in place and a reset restoring them.

mod support;

use glam::DVec3;
use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::data::{STEP, group};
use sloppy_core::sim::house_layout::{
    HOUSE_HEALTH, HOUSE_HEAP, HouseDesign, HousePiece, is_house_heap,
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
        assert!(s.world.bodies[f.body].is_dynamic());
        assert_eq!(collider_groups(&s, f.body), group::PUSHABLE_DEBRIS);
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
