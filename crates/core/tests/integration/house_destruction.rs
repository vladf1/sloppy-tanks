//! Destructible village cottages: their hit points, the collapse into the parts
//! they are built from, the heap of rubble they leave as low cover, navigation
//! through the opened footprint and rebuilding in place.

use crate::{net_support, support};

use glam::DVec3;
use rapier3d::prelude::RigidBodyHandle;
use sloppy_core::models::{WallDamage, house_roof_breaks, house_wall_damage};
use sloppy_core::net::replication::{StateMirror, StateStream};
use sloppy_core::net::scene_codec::Scene;
use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::damage::MAX_HOUSE_MARKS;
use sloppy_core::sim::data::{STEP, group, weapon};
use sloppy_core::sim::house_layout::{
    BREACH_REACH, HOUSE_HEALTH, HOUSE_HEAP, HouseDesign, HousePart, HousePiece, RoofBreaks,
    WALL_DEPTH, heap_house, heap_seed, is_house_heap, wall_frames,
};
use sloppy_core::sim::math::Point3;
use sloppy_core::sim::scenery_pieces::MOUND_SINK;
use sloppy_core::sim::stress_test_level::STRESS_TEST_MAP;
use sloppy_core::sim::superstress_level::SUPERSTRESS_MAP;
use sloppy_core::sim::timber_layout::TimberHit;

use sloppy_core::sim::math::{Random, Vec2};
use sloppy_core::sim::{
    CoverKind, DamageCause, FragmentShape, SimEventType, Simulation, Team, Weapon,
};
use support::{clear_arena, collider_groups, event_count, idle};

/// A standard shell's damage.
const SHELL: f64 = weapon(Weapon::Standard).damage;
/// Standard shells it takes to level a cottage: a deliberate effort, not the work of
/// stray fire, and the reason bots drive round cottages rather than breach them.
const COTTAGE_SHELLS: usize = 15;

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
    for i in 1..COTTAGE_SHELLS {
        shell(&mut s, house);
        assert!(s.covers[house].alive, "standing after {i} shells");
    }
    let marks = &s.covers[house].timber_hits;
    assert_eq!(marks.len(), 1, "shells on one spot grow one breach");
    assert!(marks[0].size > 4.0, "{marks:?}");
    shell(&mut s, house);
    assert!(!s.covers[house].alive, "down after {COTTAGE_SHELLS} shells");
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
    let shapes = design.pieces(&RoofBreaks::default());
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
            // The mound lies sunk into the floor, below the stubs round it.
            let sink = if mound { MOUND_SINK } else { 0.0 };
            assert!((p.y - (shape.center.y - sink)).abs() < 1e-4);
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
    tick(&mut s, 13.0);
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
fn a_drum_blast_beside_a_cottage_hurts_it_once() {
    let mut s = village();
    let house = houses(&s)[0];
    let hp = s.covers[house].hp;
    let c = &s.covers[house];
    let beside = Vec2::new(c.x + c.w / 2.0 + 1.0, c.z);
    let (id, team) = (s.human().id, s.human_team);
    s.explode(beside, 3.0, 30.0, id, team, None, DamageCause::Explosion);
    assert_eq!(s.covers[house].hp, hp - 30.0);
    assert!(s.covers[house].kick.is_some(), "a blast pushes it over");
}

/// Every village cottage collapses, alone in an empty arena, into a ruin:
///
/// - Its walls and floors crush into a rubble mound in its footprint that never
///   moves and that tanks, shells and other debris pass through.
/// - The roof comes down onto the mound: its slabs fall as they stood, and then
///   lie shingles up on the pile or lean on the stubs of wall, most of them well
///   up the heap and none far out across the yard. Of the village's some hundred
///   and forty slabs a very few (five at most: debug and release builds round the
///   tumble differently) turn over onto their rafters, and few are left steep.
/// - Most of its lower walls stand on as stubs round its floor, leaning in against
///   the pile, and its upper walls lie on the pile or the ground, at most a couple
///   in a cottage (wedged in the wreck) left on edge, and few in the village.
#[test]
fn every_cottage_collapses_into_stubs_round_a_mound_under_its_roof() {
    let village = village();
    let mut ruins = Ruins::default();
    for house in houses(&village) {
        let c = &village.covers[house];
        ruins.collapse_alone(c.x, c.z, c.w, c.d, c.h);
    }
    let Ruins {
        flipped,
        steep,
        slabs,
        standing,
    } = ruins;
    assert!(flipped.len() <= 5, "{flipped:?}");
    assert!(steep.len() * 10 <= slabs, "{steep:?} of {slabs}");
    // Five on arm64; the x86-64 CI's physics leaves one more on edge.
    assert!(standing.len() <= 6, "{standing:?}");
}

/// What the collapses of a set of cottages left: roof slabs turned over onto their
/// rafters and left steep, of how many, and upper wall shards left on edge.
#[derive(Default)]
struct Ruins {
    flipped: Vec<String>,
    steep: Vec<String>,
    slabs: usize,
    standing: Vec<String>,
}

impl Ruins {
    /// Collapse the cottage box `x, z, w, d, h` alone in an empty arena, check what
    /// holds for each cottage and add what the set is judged on.
    fn collapse_alone(&mut self, x: f64, z: f64, w: f64, d: f64, h: f64) {
        let mut s = Simulation::with_seed(731.0);
        clear_arena(&mut s, &[]);
        let house = s.add_cover(&CoverDef::house(x, z, w, d, h, 0xb87b4c));
        s.nav.rebuild(&s.covers, None);
        s.start();
        s.world.step();
        let design = HouseDesign::of(x, z, w, d, h);
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

        tick(&mut s, 0.5);
        let start_turn = glam::Quat::from_rotation_y(design.yaw() as f32);
        for fragment in &s.fragments {
            if fragment.house_piece.is_some_and(|p| p.part.is_roof()) {
                let q = s.world.bodies[fragment.body].rotation();
                assert!(
                    q.dot(start_turn).abs() > 0.999,
                    "{x} {z}: the roof falls as it stood"
                );
            }
        }

        tick(&mut s, 3.5);
        let (_, end) = mound(&s).expect("the mound lies with the pile");
        assert!((end.x - start.x).abs() + (end.y - start.y).abs() + (end.z - start.z).abs() < 1e-6);
        let pitch = design.roof().pitch;
        let walls = sloppy_core::sim::house_layout::wall_frames(design.w, design.d);
        let (mut raised, mut roof_slabs, mut stubs, mut lower) = (0, 0, 0, 0);
        let mut on_edge = Vec::new();
        for fragment in &s.fragments {
            let Some(piece) = fragment.house_piece else {
                continue;
            };
            let rotation = *s.world.bodies[fragment.body].rotation();
            if piece.part.is_roof() {
                let p = s.body_translation(fragment.body);
                roof_slabs += 1;
                // Up on the mound, which lies sunk below the stubs round it.
                if p.y > 0.5 {
                    raised += 1;
                }
                assert!(
                    (p.x - x).abs() < w / 2.0 + 2.0 && (p.z - z).abs() < d / 2.0 + 2.0,
                    "{x} {z}: {piece:?} came down on the wreck ({p:?})"
                );
                let top = match piece.part {
                    HousePart::RightRoof => glam::Vec3::new(pitch as f32, 1.0, 0.0),
                    HousePart::LeftRoof => glam::Vec3::new(-pitch as f32, 1.0, 0.0),
                    HousePart::FrontRoof => glam::Vec3::new(0.0, 1.0, pitch as f32),
                    _ => glam::Vec3::new(0.0, 1.0, -pitch as f32),
                }
                .normalize();
                let up = (rotation * top).y;
                if up < -0.3 {
                    self.flipped
                        .push(format!("{x} {z}: {} ({up:.2})", String::from(piece)));
                } else if up < 0.3 {
                    self.steep
                        .push(format!("{x} {z}: {} ({up:.2})", String::from(piece)));
                }
            } else if let Some(wall) = piece.part.wall_index() {
                let out = walls[wall].1;
                let face = rotation * glam::Vec3::new(out.x as f32, out.y as f32, out.z as f32);
                if wall != 1 && piece.shard.is_multiple_of(2) {
                    lower += 1;
                    // Standing, its outside face tipped up a little: leaning in.
                    if (0.03..0.6).contains(&face.y) {
                        stubs += 1;
                    }
                } else if face.y.abs() < 0.5 {
                    on_edge.push(format!("{x} {z}: {}", String::from(piece)));
                }
            }
        }
        assert!(
            raised * 2 >= roof_slabs,
            "{x} {z}: {raised} of {roof_slabs} slabs up the heap"
        );
        assert!(
            stubs * 2 >= lower,
            "{x} {z}: {stubs} of {lower} stubs stand"
        );
        assert!(on_edge.len() <= 2, "{on_edge:?}");
        self.slabs += roof_slabs;
        self.standing.extend(on_edge);
    }
}

/// Every cottage box of the extra levels (the Stress Grid's eight and the Scrap Yard's
/// two), placed as their layouts place them.
fn extra_level_cottages() -> Vec<(f64, f64, f64, f64, f64)> {
    [&STRESS_TEST_MAP, &SUPERSTRESS_MAP]
        .into_iter()
        .flat_map(|map| (map.layout)())
        .filter(|cover| cover.kind == CoverKind::House)
        .map(|cover| (cover.x, cover.z, cover.w, cover.d, cover.h))
        .collect()
}

/// The extra levels' cottages are other sizes, heights and places than the
/// village's, so each is checked against what makes a village cottage fall: a
/// design that fills its box, roof slabs that fall inside its walls, a heap along
/// its back wall that leaves the front open and can be traced back to its cottage,
/// and a collapse into the same ruin.
#[test]
fn extra_level_cottages_are_built_and_fall_like_the_villages() {
    let cottages = extra_level_cottages();
    assert_eq!(cottages.len(), 10);
    let mut ruins = Ruins::default();
    for &(x, z, w, d, h) in &cottages {
        let design = HouseDesign::of(x, z, w, d, h);
        let corner = design.to_world(DVec3::new(design.w / 2.0, 0.0, design.d / 2.0));
        assert!((corner.x.abs() - w / 2.0).abs() < 1e-9, "{x} {z}");
        assert!((corner.z.abs() - d / 2.0).abs() < 1e-9, "{x} {z}");
        let (inside_w, inside_d) = (design.w / 2.0 - WALL_DEPTH, design.d / 2.0 - WALL_DEPTH);
        for slab in design
            .pieces(&RoofBreaks::default())
            .iter()
            .filter(|p| p.piece.part.is_roof())
        {
            for p in &slab.hull {
                assert!(p.x.abs() < inside_w && p.z.abs() < inside_d, "{x} {z} {p}");
            }
        }
        let (center, size) = design.heap();
        assert!(
            center.z + size.z / 2.0 < 0.0,
            "{x} {z}: the heap lies at the back"
        );
        assert!(design.open_floor() > 3.0, "{x} {z}");
        let seed = heap_seed(0.37, &design);
        let (offset, size) = design.heap_box();
        let (found_at, found) = heap_house(x + offset.x, z + offset.y, size.x, size.y, seed, h);
        assert!((found_at.x - x).abs() < 1e-6 && (found_at.y - z).abs() < 1e-6);
        assert_eq!(
            found.style, design.style,
            "{x} {z}: the heap knows its cottage"
        );
        ruins.collapse_alone(x, z, w, d, h);
    }
    let Ruins {
        flipped,
        steep,
        slabs,
        standing,
    } = ruins;
    // Their slabs at about the village's rate: none turns over on arm64.
    assert!(flipped.len() <= 4, "{flipped:?} of {slabs}");
    assert!(steep.len() * 10 <= slabs, "{steep:?} of {slabs}");
    // None on arm64 since their pieces settle into the heap rather than on its top;
    // the village's limit, as the x86-64 CI's physics leaves more on edge.
    assert!(standing.len() <= 6, "{standing:?}");
}

/// A standard shell striking the cottage's build-frame point `local` (at hull height
/// unless `local.y` says otherwise), as `projectiles` reports a hit.
fn shell_at(s: &mut Simulation, house: usize, local: DVec3) {
    // Fired by nobody in particular: the lone cottage's arena has no tanks.
    let (id, team) = (0, Team::Blue);
    let c = &s.covers[house];
    let p = design(s, house).to_world(local);
    let impact = Point3::new(c.x + p.x, local.y, c.z + p.z);
    s.damage_cover(house, SHELL, id, team, None, Some(impact));
}

/// The cottage part each of its marks struck.
fn struck(s: &Simulation, house: usize) -> Vec<HousePart> {
    let design = design(s, house);
    s.covers[house]
        .timber_hits
        .iter()
        .map(|mark| design.struck_part(mark))
        .collect()
}

#[test]
fn shells_on_one_wall_mark_only_that_wall_and_grow_its_breach() {
    let (mut s, house) = lone_cottage();
    let (w, d) = {
        let design = design(&s, house);
        (design.w, design.d)
    };
    let front = DVec3::new(0.6, 1.0, d / 2.0);
    let mut sizes = Vec::new();
    for _ in 1..COTTAGE_SHELLS {
        shell_at(&mut s, house, front);
        assert_eq!(struck(&s, house), vec![HousePart::FrontWall]);
        sizes.push(s.covers[house].timber_hits[0].size);
    }
    assert!(
        sizes.windows(2).all(|pair| pair[1] >= pair[0]) && sizes[sizes.len() - 1] > sizes[0] * 3.0,
        "every shell grows the breach it lands on: {sizes:?}"
    );
    // A shell on another wall opens a breach there, leaving the first as it was.
    let first = s.covers[house].timber_hits[0];
    let (id, team) = (0, Team::Blue);
    let c = &s.covers[house];
    let p = design(&s, house).to_world(DVec3::new(w / 2.0, 1.0, -0.8));
    let impact = Point3::new(c.x + p.x, 1.0, c.z + p.z);
    s.damage_cover(house, 1.0, id, team, None, Some(impact));
    assert_eq!(
        struck(&s, house),
        vec![HousePart::FrontWall, HousePart::RightWall]
    );
    assert_eq!(s.covers[house].timber_hits[0], first);
    // Nothing on the roof unless a blast struck it there.
    let (h, ridge) = (s.covers[house].h, design(&s, house).roof().ridge);
    assert!(!struck(&s, house).iter().any(|part| part.is_roof()));
    s.covers[house].hp = HOUSE_HEALTH;
    shell_at(&mut s, house, DVec3::new(-w * 0.2, ridge - 0.3, 0.4));
    let parts = struck(&s, house);
    assert_eq!(parts.len(), 3);
    assert!(parts[2].is_roof(), "{parts:?} at {h}");
}

#[test]
fn a_cottage_keeps_a_bounded_number_of_marks_and_no_random_draw() {
    let (mut s, house) = lone_cottage();
    let rng = s.rng.clone();
    let d = design(&s, house);
    // Shells spread over every wall and the roof, far more than a cottage remembers,
    // none of them felling it.
    for k in 0..60 {
        let t = f64::from(k) * 0.61;
        let local = match k % 5 {
            0 => DVec3::new((t.sin()) * d.w * 0.45, 1.0, d.d / 2.0),
            1 => DVec3::new(-d.w / 2.0, 0.6 + (k % 3) as f64 * 0.6, t.cos() * d.d * 0.45),
            2 => DVec3::new(d.w / 2.0, 1.2, t.sin() * d.d * 0.45),
            3 => DVec3::new(t.cos() * d.w * 0.45, 1.0, -d.d / 2.0),
            _ => DVec3::new(
                t.sin() * d.w * 0.3,
                d.roof().ridge - 0.4,
                t.cos() * d.d * 0.4,
            ),
        };
        shell_at(&mut s, house, local);
        s.covers[house].hp = HOUSE_HEALTH;
        let marks = &s.covers[house].timber_hits;
        assert!(marks.len() <= MAX_HOUSE_MARKS, "{}", marks.len());
        assert!(
            marks
                .iter()
                .all(|mark| mark.size.is_finite() && mark.size <= 8.0)
        );
    }
    assert_eq!(s.covers[house].timber_hits.len(), MAX_HOUSE_MARKS);
    assert_eq!(
        s.rng, rng,
        "marking a cottage draws nothing from the match's stream"
    );
}

#[test]
fn a_cottages_marks_reach_room_clients_and_late_joiners() {
    use net_support::{apply_batch, baseline, batch};
    use sloppy_core::net::multiplayer_simulation::{
        MultiplayerOptions, create_multiplayer_simulation,
    };
    use sloppy_core::net::scene_codec::MirrorScene;
    use sloppy_core::sim::map_options::MapId;
    use sloppy_core::sim::types::{PlayerAssignment, VehicleKind};
    let players = [PlayerAssignment {
        player_id: "one".into(),
        name: "One".into(),
        team: Team::Blue,
        slot: 0,
        kind: VehicleKind::Balanced,
    }];
    let mut s = create_multiplayer_simulation(
        4242.0,
        &players,
        MultiplayerOptions {
            map_mode: Some(MapId::Village),
            ..MultiplayerOptions::default()
        },
    )
    .unwrap();
    s.start();
    let house = houses(&s)[0];
    let id = s.covers[house].id;
    let viewer = s.human().id;
    let mut stream = StateStream::new("room", 1);
    let mut mirror = StateMirror::default();
    let scene = Scene::capture(&s);
    mirror
        .apply_full(
            &baseline(&stream.full(0, 0, [], || scene.clone())),
            "room",
            1,
        )
        .unwrap();
    let d = design(&s, house);
    let same = |shown: &[TimberHit], marks: &[TimberHit]| {
        shown.len() == marks.len()
            && shown.iter().zip(marks).all(|(a, b)| {
                [a.x - b.x, a.y - b.y, a.z - b.z, a.size - b.size]
                    .iter()
                    .all(|delta| delta.abs() < 0.01)
            })
    };
    for tick in 1..=6 {
        shell_at(&mut s, house, DVec3::new(0.4, 1.0, d.d / 2.0));
        if tick == 4 {
            shell_at(&mut s, house, DVec3::new(-d.w / 2.0, 1.0, 0.5));
        }
        let mut next = Scene::capture(&s);
        let body = stream.snapshot(&mut next, tick, &[], &[]);
        apply_batch(&mut mirror, &batch(1, 0, stream.seq, &[(tick, body)]));
        let shown = mirror.render(viewer).unwrap();
        let cover = shown.covers.iter().find(|cover| cover.id == id).unwrap();
        assert!(
            same(&cover.timber_hits, &s.covers[house].timber_hits),
            "tick {tick}: {:?} against {:?}",
            cover.timber_hits,
            s.covers[house].timber_hits
        );
    }
    // A client joining now sees the breaches already there.
    let scene = Scene::capture(&s);
    let late = MirrorScene::from_scene(scene)
        .unwrap()
        .render(viewer)
        .unwrap();
    let cover = late.covers.iter().find(|cover| cover.id == id).unwrap();
    assert_eq!(cover.timber_hits.len(), 2);
    assert!(same(&cover.timber_hits, &s.covers[house].timber_hits));
}

/// A lone cottage shelled down at its build-frame point `local`, its pieces' poses
/// (in its build frame) and starting speeds as it falls, and their poses `seconds`
/// later.
struct Fall {
    start: Vec<(HousePiece, DVec3, DVec3, DVec3)>,
    later: Vec<(HousePiece, DVec3)>,
}

fn fall(local: DVec3, seconds: f64) -> Fall {
    let (mut s, house) = lone_cottage();
    let design = design(&s, house);
    while s.covers[house].alive {
        shell_at(&mut s, house, local);
    }
    let (x, z) = (s.covers[house].x, s.covers[house].z);
    let poses = |s: &Simulation| -> Vec<(HousePiece, DVec3, DVec3, DVec3)> {
        s.fragments
            .iter()
            .filter_map(|f| {
                let piece = f.house_piece?;
                let body = &s.world.bodies[f.body];
                let p = body.translation();
                let (v, w) = (body.linvel(), body.angvel());
                let local = design.to_local(DVec3::new(
                    f64::from(p.x) - x,
                    f64::from(p.y),
                    f64::from(p.z) - z,
                ));
                let speed =
                    design.to_local(DVec3::new(f64::from(v.x), f64::from(v.y), f64::from(v.z)));
                let spin =
                    design.to_local(DVec3::new(f64::from(w.x), f64::from(w.y), f64::from(w.z)));
                Some((piece, local, speed, spin))
            })
            .collect()
    };
    let start = poses(&s);
    tick(&mut s, seconds);
    let later = poses(&s)
        .into_iter()
        .map(|(piece, p, _, _)| (piece, p))
        .collect();
    Fall { start, later }
}

/// The starting speeds of a cottage shelled down through one wall, summed to a
/// fingerprint (to a millimetre a second): a roof-driven fall must leave them as
/// they were.
fn speeds_print(fall: &Fall) -> i64 {
    fall.start
        .iter()
        .enumerate()
        .map(|(i, (_, _, v, w))| {
            let round = |x: f64| (x * 1000.0).round() as i64;
            (i as i64 + 1)
                * (round(v.x)
                    + 3 * round(v.y)
                    + 5 * round(v.z)
                    + 7 * round(w.x)
                    + 11 * round(w.y)
                    + 13 * round(w.z))
        })
        .sum()
}

#[test]
fn a_cottage_shelled_through_its_roof_caves_in_and_a_wall_breach_falls_as_before() {
    let (s, house) = lone_cottage();
    let d = design(&s, house);
    let wall = fall(DVec3::new(0.4, 1.0, d.d / 2.0), 0.35);
    // The fingerprint of the fall as it was before roof-driven collapses caved in.
    assert_eq!(speeds_print(&wall), 25_510);
    let roof = fall(DVec3::new(0.5, d.h - 0.35, 0.3), 0.35);
    let mean = |fall: &Fall, pick: &dyn Fn(HousePiece) -> bool, of: &dyn Fn(DVec3) -> f64| {
        let values: Vec<f64> = fall
            .later
            .iter()
            .filter(|(piece, _)| pick(*piece))
            .map(|(_, p)| of(*p))
            .collect();
        values.iter().sum::<f64>() / values.len().max(1) as f64
    };
    let roofs = |piece: HousePiece| piece.part.is_roof();
    let upper_walls = |piece: HousePiece| {
        piece.part.wall_index().is_some()
            && (piece.part == HousePart::BackWall || piece.shard % 2 == 1)
    };
    let height = |p: DVec3| p.y;
    let out = |p: DVec3| p.x.abs().max(p.z.abs());
    // The roof caves down into the house where the walls' fall leaves it standing
    // a moment longer, and the walls under it buckle in beneath it rather than
    // being thrown out.
    assert!(
        mean(&roof, &roofs, &height) < mean(&wall, &roofs, &height) - 0.2,
        "roof slabs {} against {}",
        mean(&roof, &roofs, &height),
        mean(&wall, &roofs, &height)
    );
    assert!(
        mean(&roof, &upper_walls, &out) <= mean(&wall, &upper_walls, &out) + 0.05,
        "upper walls {} against {}",
        mean(&roof, &upper_walls, &out),
        mean(&wall, &upper_walls, &out)
    );
    // Its slabs started down at once.
    assert!(
        roof.start
            .iter()
            .filter(|(piece, ..)| piece.part.is_roof())
            .all(|(_, _, v, _)| v.y < -1.0)
    );
}

/// A lone cottage shelled down through one wall, left `seconds` to settle.
fn fallen_cottage(seconds: f64) -> (Simulation, usize) {
    let (mut s, house) = lone_cottage();
    let d = design(&s, house);
    while s.covers[house].alive {
        shell_at(&mut s, house, DVec3::new(0.4, 1.0, d.d / 2.0));
    }
    tick(&mut s, seconds);
    (s, house)
}

/// Each cottage piece's id, position and speed.
fn pile(s: &Simulation) -> Vec<(u32, DVec3, f32)> {
    s.fragments
        .iter()
        .filter(|f| {
            f.house_piece
                .is_some_and(|piece| piece.part != HousePart::Mound)
        })
        .map(|f| {
            let body = &s.world.bodies[f.body];
            let p = body.translation();
            let p = DVec3::new(f64::from(p.x), f64::from(p.y), f64::from(p.z));
            (f.id, p, body.linvel().length())
        })
        .collect()
}

#[test]
fn a_shell_into_a_fallen_cottage_throws_the_pieces_near_it_and_leaves_the_rest() {
    let (mut s, house) = fallen_cottage(8.0);
    let before = pile(&s);
    assert!(
        before.iter().all(|(_, _, speed)| *speed < 0.3),
        "the pile has settled"
    );
    // A standard shell flown at hull height through the pile's middle from the
    // open front, as a tank would fire into it.
    let (c, d) = (s.covers[house].clone(), design(&s, house));
    let from = at(&s, house, DVec3::new(0.0, 0.0, d.d / 2.0 + 14.0));
    let to = at(&s, house, DVec3::ZERO);
    let (dx, dz) = (to.x - from.x, to.z - from.z);
    let length = dx.hypot(dz);
    let speed = weapon(Weapon::Standard).speed;
    let id = s.allocate_id();
    s.shots.push(sloppy_core::sim::Shot {
        id,
        x: from.x,
        z: from.z,
        vx: dx / length * speed,
        vz: dz / length * speed,
        team: Team::Blue,
        owner: 0,
        weapon: Weapon::Standard,
        damage: SHELL,
        life: 3.0,
        ..Default::default()
    });
    let rng = s.rng.state;
    let mut burst = None;
    for _ in 0..90 {
        s.events.clear();
        idle(&mut s);
        if let Some(event) = s.events.iter().find(|event| {
            event.kind == SimEventType::Impact && event.cover_kind == Some(CoverKind::House)
        }) {
            burst = Some((event.x, event.z));
            break;
        }
    }
    let (x, z) = burst.expect("the shell bursts in the pile as rubble");
    assert_eq!(s.rng.state, rng, "throwing the pile draws nothing");
    assert!((x - c.x).abs() < c.w && (z - c.z).abs() < c.d);
    // Pieces near the burst are thrown up; those far from it lie as they were.
    let thrown = pile(&s);
    let near = |p: DVec3| ((p.x - x).powi(2) + (p.z - z).powi(2)).sqrt();
    assert!(
        thrown
            .iter()
            .any(|(_, p, speed)| near(*p) < 2.5 && *speed > 0.8),
        "{thrown:?}"
    );
    for ((_, p, _), (_, q, speed)) in before.iter().zip(&thrown) {
        if near(*p) > 4.5 {
            assert!(*speed < 0.3 && p.distance(*q) < 0.05, "{p} moved to {q}");
        }
    }
    // They land again nearby and settle.
    tick(&mut s, 3.0);
    for (_, p, speed) in pile(&s) {
        assert!(speed < 0.5, "settled again ({p}, {speed} m/s)");
        assert!(
            (p.x - c.x).abs() < c.w && (p.z - c.z).abs() < c.d,
            "{p} stays by the heap"
        );
    }
}

#[test]
fn a_fallen_cottages_pieces_sink_one_at_a_time_from_the_top_of_the_pile() {
    let (mut s, _) = fallen_cottage(12.0);
    let all = pile(&s).len();
    assert!(all > 20, "{all}");
    let mut counts = Vec::new();
    for _ in 0..8 {
        tick(&mut s, 1.0);
        counts.push(pile(&s).len());
    }
    // From 13 s to 21 s the pile goes down over several seconds, never more than
    // a third of it in any one second.
    assert_eq!(counts[counts.len() - 1], 0, "{counts:?}");
    let mut last = all;
    let mut steps = 0;
    for &count in &counts {
        assert!(last - count <= all / 3 + 1, "{all} {counts:?}");
        steps += usize::from(count < last);
        last = count;
    }
    assert!(steps >= 4, "{counts:?}");
}

#[test]
fn every_piece_a_cottage_breaks_into_reaches_room_clients_by_name() {
    use sloppy_core::net::scene_codec::HOUSE_PIECES;
    assert_eq!(HOUSE_PIECES.len(), HousePiece::ALL.len());
    for piece in HousePiece::ALL {
        assert!(
            HOUSE_PIECES
                .iter()
                .any(|(name, named)| *named == piece && *name == String::from(piece)),
            "{piece:?}"
        );
    }
}

/// A breach's box along its wall (centre ± [`BREACH_REACH`] per unit of size), as
/// the model tears it, as (from, to) along `along` in the cottage's frame.
fn breach_span(s: &Simulation, house: usize, mark: &TimberHit, along: DVec3) -> (f64, f64) {
    let local = design(s, house).to_local(DVec3::new(mark.x, mark.y, mark.z));
    let u = local.dot(along);
    let reach = BREACH_REACH.0 * mark.size;
    (u - reach, u + reach)
}

#[test]
fn a_shell_beside_a_breach_tears_it_on_over_where_it_landed() {
    let (mut s, house) = lone_cottage();
    let d = design(&s, house);
    let along = DVec3::X;
    let first = DVec3::new(-0.6, 1.0, d.d / 2.0);
    for _ in 0..4 {
        shell_at(&mut s, house, first);
    }
    let marks = s.covers[house].timber_hits.clone();
    assert_eq!(marks.len(), 1);
    let before = breach_span(&s, house, &marks[0], along);
    // Beside the breach, close enough to join it rather than open its own.
    let beside = DVec3::new(-0.6 + 1.1, 1.0, d.d / 2.0);
    let rng = s.rng.state;
    shell_at(&mut s, house, beside);
    assert_eq!(s.rng.state, rng);
    let marks = s.covers[house].timber_hits.clone();
    assert_eq!(marks.len(), 1, "it joined the breach");
    let after = breach_span(&s, house, &marks[0], along);
    // The breach now reaches over the strike and still covers all it did.
    assert!(
        after.0 <= before.0 + 1e-9 && after.1 >= before.1 - 1e-9,
        "{before:?} {after:?}"
    );
    let reach = BREACH_REACH.0 * 0.5;
    assert!(after.1 >= beside.x + reach, "{after:?} over {}", beside.x);
    // It grew toward the strike, hardly out the other way.
    assert!(
        before.0 - after.0 < 0.25 * (after.1 - before.1),
        "{before:?} {after:?}"
    );
    // A shell on its middle still grows it in place.
    let middle = (after.0 + after.1) / 2.0;
    shell_at(&mut s, house, DVec3::new(middle, 1.0, d.d / 2.0));
    let marks = &s.covers[house].timber_hits;
    let again = breach_span(&s, house, &marks[0], along);
    assert!(
        ((again.0 + again.1) / 2.0 - middle).abs() < 1e-6,
        "{again:?}"
    );
}

#[test]
fn shells_either_side_of_a_corner_keep_a_breach_on_each_wall() {
    let (mut s, house) = lone_cottage();
    let d = design(&s, house);
    // One shell on the side wall just short of the corner, then two on the front wall
    // just round it: close enough that a search by distance alone would merge them.
    shell_at(&mut s, house, DVec3::new(d.w / 2.0, 1.0, d.d / 2.0 - 0.2));
    for _ in 0..2 {
        shell_at(&mut s, house, DVec3::new(d.w / 2.0 - 0.2, 1.0, d.d / 2.0));
    }
    let parts = struck(&s, house);
    assert!(
        parts.contains(&HousePart::wall(2)),
        "side wall kept its breach: {parts:?}"
    );
    assert!(
        parts.contains(&HousePart::wall(0)),
        "front wall has its own: {parts:?}"
    );
    assert_eq!(parts.len(), 2, "{parts:?}");
}

/// Wall `index`'s first window and the build-frame point a shell flying at hull height
/// (1 m, under its sill) strikes when aimed at its middle.
fn window_target(d: &HouseDesign, index: usize) -> ([f64; 4], DVec3, DVec3) {
    let (center, _, along, _) = wall_frames(d.w, d.d)[index];
    let window = house_wall_damage(d, &[], index).windows[0];
    let u = (window[0] + window[2]) / 2.0;
    (window, center + along * u + DVec3::Y, along)
}

fn covers_window(damage: &WallDamage, window: [f64; 4]) -> bool {
    damage
        .breaches
        .iter()
        .any(|b| b[0] <= window[0] && b[1] <= window[1] && b[2] >= window[2] && b[3] >= window[3])
}

#[test]
fn a_shell_into_a_window_tears_it_out_with_the_wall_round_it() {
    let (mut s, house) = lone_cottage();
    let d = design(&s, house);
    for index in 0..4 {
        let (window, target, _) = window_target(&d, index);
        let rng = s.rng.state;
        s.covers[house].hp = HOUSE_HEALTH;
        shell_at(&mut s, house, target);
        assert_eq!(s.rng.state, rng, "no random draw");
        let marks = s.covers[house].timber_hits.clone();
        let damage = house_wall_damage(&d, &marks, index);
        // Its glass, sash and frame go with the wall round them: the breach takes
        // in the whole opening and runs out past it into the siding.
        assert!(damage.torn_out.contains(&0), "wall {index}: {damage:?}");
        assert!(covers_window(&damage, window), "wall {index}: {damage:?}");
        let breach = damage
            .breaches
            .iter()
            .find(|b| b[0] <= window[0] && b[2] >= window[2])
            .unwrap();
        // (Beside the door its canopy's stud holds on that side.)
        assert!(
            (breach[0] < window[0] - 0.1 || breach[2] > window[2] + 0.1)
                && breach[1] < window[1] - 0.2
                && breach[3] > window[3],
            "wall {index}: torn into the siding round it: {breach:?} {window:?}"
        );
        // Only that window: any other on the wall keeps its glass.
        assert!(
            damage.blown_in.is_empty() && damage.torn_out == vec![0],
            "wall {index}: {damage:?}"
        );
    }
    // The headers over the windows still carry the roof: one shell drops no eave.
    let breaks = house_roof_breaks(&d, &s.covers[house].timber_hits);
    assert!(breaks.eave_spans.is_empty(), "{:?}", breaks.eave_spans);
}

#[test]
fn a_breach_that_grows_into_a_window_swallows_it() {
    let (mut s, house) = lone_cottage();
    let d = design(&s, house);
    let index = (0..4)
        .find(|&index| house_wall_damage(&d, &[], index).windows.len() == 1)
        .expect("a wall with one window");
    let (window, target, along) = window_target(&d, index);
    // Beside the window, clear of its casing; each shell tears the breach on.
    let beside = target + along * ((window[2] - window[0]) / 2.0 + 0.85);
    let mut swallowed = false;
    for _ in 0..4 {
        shell_at(&mut s, house, beside);
        let damage = house_wall_damage(&d, &s.covers[house].timber_hits, index);
        // A breach never stops part way into the window's casing: it holds short of
        // it or takes the whole window.
        if damage.torn_out.contains(&0) {
            assert!(covers_window(&damage, window), "{damage:?}");
            swallowed = true;
        } else {
            assert!(
                damage
                    .breaches
                    .iter()
                    .all(|b| b[2] <= window[0] || b[0] >= window[2]),
                "{damage:?}"
            );
        }
    }
    assert!(swallowed, "four shells beside it reached the window");
}

/// A cottage felled through the wall under one of its slopes: that slope goes down
/// first, and the roof settles into the heap the cottage leaves, onto the rubble
/// mound sunk under it, rather than lying level across the heap's top and the stubs
/// like a table.
#[test]
fn a_breached_cottages_roof_leads_down_over_the_breach_and_settles_into_its_heap() {
    let (mut s, house) = lone_cottage();
    let d = design(&s, house);
    let inward = d.to_world(DVec3::new(-1.0, 0.0, 0.0));
    while s.covers[house].alive {
        // As a shell flying into the wall under the right slope drives it.
        s.covers[house].kick = Some(Vec2::new(inward.x, inward.z));
        shell_at(&mut s, house, DVec3::new(d.w / 2.0, 1.0, 0.4));
    }
    tick(&mut s, 0.3);
    let mean = |s: &Simulation,
                pick: &dyn Fn(HousePart) -> bool,
                of: &dyn Fn(&Simulation, &RigidBodyHandle) -> f64| {
        let values: Vec<f64> = s
            .fragments
            .iter()
            .filter(|f| f.house_piece.is_some_and(|p| pick(p.part)))
            .map(|f| of(s, &f.body))
            .collect();
        values.iter().sum::<f64>() / values.len().max(1) as f64
    };
    let falling =
        |s: &Simulation, body: &RigidBodyHandle| f64::from(s.world.bodies[*body].linvel().y);
    let over = mean(&s, &|part| part == HousePart::RightRoof, &falling);
    let behind = mean(&s, &|part| part == HousePart::LeftRoof, &falling);
    assert!(
        over < behind - 0.3,
        "over the breach {over}, behind {behind}"
    );
    tick(&mut s, 5.7);
    let height = |s: &Simulation, body: &RigidBodyHandle| s.body_translation(*body).y;
    let roof = mean(&s, &|part| part.is_roof(), &height);
    assert!(roof < HOUSE_HEAP.height, "the roof lies at {roof}");
    // The heap is cover to tanks and shells but not a shelf for its pieces.
    let heap_index = heaps(&s)[0];
    assert_eq!(
        collider_groups(&s, s.covers[heap_index].body),
        group::HOUSE_HEAP
    );
    let heap = s.covers[heap_index].collider;
    for f in s.fragments.iter().filter(|f| f.house_piece.is_some()) {
        for &collider in s.world.bodies[f.body].colliders() {
            assert!(
                s.world
                    .narrow_phase
                    .contact_pairs_with(collider)
                    .all(|pair| {
                        pair.collider1 != heap && pair.collider2 != heap
                            || !pair.has_any_active_contact()
                    })
            );
        }
    }
}

#[test]
fn the_slope_across_from_a_dropped_eave_breaks_where_it_lands_on_its_stubs() {
    let s = village();
    let mut runs = Vec::new();
    for house in houses(&s) {
        let d = design(&s, house);
        let span = (1.0, -0.9, 0.7);
        let dropped = RoofBreaks {
            eave_spans: vec![span],
            holes: Vec::new(),
        };
        let slabs = |breaks: &RoofBreaks, part: HousePart| -> Vec<u8> {
            d.pieces(breaks)
                .iter()
                .filter(|p| p.piece.part == part)
                .map(|p| p.piece.shard)
                .collect()
        };
        // The far slope comes down across its wall's stubs: broken across into a
        // ridge slab and an eave slab (or more), never one whole slab.
        let far = slabs(&dropped, HousePart::LeftRoof);
        assert!(
            far.iter().any(|s| s % 2 == 0) && far.iter().any(|s| s % 2 == 1),
            "{far:?}"
        );
        runs.push(far.len());
        // Undamaged, it may still fall whole, as before.
        assert!(
            d.roof_split(-1.0, (-1.0e3, 1.0e3), &RoofBreaks::default())
                .is_none()
        );
        assert_eq!(
            d.pieces(&dropped).len(),
            d.pieces(&dropped).len(),
            "deterministic"
        );
    }
    // The cottages differ: some far slopes also split down a rafter line.
    assert!(runs.iter().min() < runs.iter().max(), "{runs:?}");
}

#[test]
fn a_fallen_cottages_garden_is_crushed_a_stretch_at_a_time() {
    let s = village();
    let mut gardens = 0;
    for house in houses(&s) {
        let c = &s.covers[house];
        let props = sloppy_core::models::heap_prop_footprints(c.x, c.z, c.w, c.d, 0.0);
        // The plinth's front, the yard, then a garden's bed and its stretches.
        if props.len() <= 3 {
            continue;
        }
        gardens += 1;
        let bed = props[2];
        assert!(props.len() >= 5, "{props:?}");
        for stretch in &props[3..] {
            assert!(
                stretch[0] >= bed[0] - 1e-6
                    && stretch[1] >= bed[1] - 1e-6
                    && stretch[2] <= bed[2] + 1e-6
                    && stretch[3] <= bed[3] + 1e-6,
                "{stretch:?} in {bed:?}"
            );
            let area = (stretch[2] - stretch[0]) * (stretch[3] - stretch[1]);
            let whole = (bed[2] - bed[0]) * (bed[3] - bed[1]);
            assert!(area < whole * 0.6, "{stretch:?} {bed:?}");
        }
    }
    assert!(gardens > 0);
}
