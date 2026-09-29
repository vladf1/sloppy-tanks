//! Timber walls (the former `tests/timber-walls.test.ts`): authored colliders never overlap,
//! bays breach independently and restore on reset, corner posts break into one physical post,
//! blasts destroy each bay once, and loose beams keep their pose and stack.

mod support;

use std::f64::consts::FRAC_1_SQRT_2;

use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::data::{STEP, group};
use sloppy_core::sim::maps::{ArenaMap, MAPS};
use sloppy_core::sim::math::{Point3, Quat4, Vec2};
use sloppy_core::sim::physics::{to_rotation, vector};
use sloppy_core::sim::projectiles::step_projectiles;
use sloppy_core::sim::simulation::packed_groups;
use sloppy_core::sim::stress_test_level::STRESS_TEST_MAP;
use sloppy_core::sim::superstress_level::SUPERSTRESS_MAP;
use sloppy_core::sim::timber_layout::{
    TimberPartKind, TimberWall, timber_damage_stage, timber_parts,
};
use sloppy_core::sim::{CoverKind, DamageCause, Shot, SimEventType, Simulation, Team, Weapon};
use support::clear_arena;

/// Whether two packed interaction groups (membership << 16 | filter) collide.
fn allows(a: u32, b: u32) -> bool {
    ((a >> 16) & b & 0xffff) != 0 && ((b >> 16) & a & 0xffff) != 0
}

fn collider_groups(s: &Simulation, body: rapier3d::prelude::RigidBodyHandle) -> u32 {
    let collider = s.world.bodies[body].colliders()[0];
    packed_groups(s.world.colliders[collider].collision_groups())
}

fn timber(x: f64, z: f64, w: f64, d: f64) -> CoverDef {
    CoverDef::new(CoverKind::Timber, x, z, w, d, 2.8, 120.0, 0xb47a49)
}

#[test]
fn timber_barriers_meet_without_overlapping_colliders_across_every_map() {
    // The TS test also compared Three.js part-model bounds for every damage pose; that
    // rendering check stays with the presentation code.
    let maps: Vec<&ArenaMap> = MAPS
        .iter()
        .chain([&STRESS_TEST_MAP, &SUPERSTRESS_MAP])
        .collect();
    for map in maps {
        let id = map.id.as_str();
        let walls: Vec<CoverDef> = (map.layout)()
            .into_iter()
            .filter(|c| c.kind == CoverKind::Timber)
            .collect();
        assert!(
            walls.iter().all(|wall| wall.hp == 80.0),
            "{id}: timber takes two standard hits"
        );
        for i in 0..walls.len() {
            for j in i + 1..walls.len() {
                let (a, b) = (&walls[i], &walls[j]);
                assert!(
                    (a.x - b.x).abs() >= (a.w + b.w) / 2.0 - 1e-6
                        || (a.z - b.z).abs() >= (a.d + b.d) / 2.0 - 1e-6,
                    "{id}: overlapping timber colliders at {},{} and {},{}",
                    a.x,
                    a.z,
                    b.x,
                    b.z
                );
            }
        }
    }
}

#[test]
fn two_shells_breach_one_timber_bay_clearing_physics_and_bot_navigation_and_reset_restores_it() {
    let mut s = Simulation::with_seed(123.0);
    let bodies: Vec<_> = s.tanks.iter().map(|t| t.body).collect();
    for body in bodies {
        s.remove_body(body);
    }
    s.tanks.clear();
    let wall = s
        .covers
        .iter()
        .position(|c| c.kind == CoverKind::Timber && c.x == -2.0 && c.z == 13.0)
        .unwrap();
    let neighbor = s
        .covers
        .iter()
        .position(|c| c.kind == CoverKind::Timber && c.x == 2.0 && c.z == 13.0)
        .unwrap();
    let (wx, wz) = (s.covers[wall].x, s.covers[wall].z);
    let handle = s.covers[wall].collider;
    let version = s.nav.version;
    assert_eq!(s.nav.blocked[s.nav.index(Vec2::new(wx, wz))], 1);
    for hit in 1..=2 {
        let id = s.next_id;
        s.next_id += 1;
        s.shots = vec![Shot {
            id,
            x: wx,
            z: wz - 5.0,
            vx: 0.0,
            vz: 600.0,
            owner: 999,
            team: Team::Blue,
            damage: 40.0,
            bounces: 0,
            life: 2.0,
            piercing: 0,
            weapon: Weapon::Standard,
            ..Shot::default()
        }];
        step_projectiles(&mut s, STEP, false);
        assert_eq!(s.covers[wall].hp, 80.0 - hit as f64 * 40.0);
        assert_eq!(s.covers[wall].alive, hit < 2);
        assert_eq!(s.shots.len(), 0);
    }
    assert_eq!(s.covers[neighbor].hp, 80.0);
    assert!(s.covers[neighbor].alive);
    assert!(!s.cover_by_collider.contains_key(&handle));
    assert!(s.nav.version > version);
    assert_eq!(s.nav.blocked[s.nav.index(Vec2::new(wx, wz))], 0);
    s.world.step();
    assert!(s.visible(Vec2::new(wx, wz - 3.0), Vec2::new(wx, wz + 3.0)));
    assert!(!s.fragments.is_empty());
    s.reset(None);
    assert!(
        s.covers
            .iter()
            .filter(|c| c.kind == CoverKind::Timber)
            .all(|c| c.alive && c.hp == 80.0)
    );
}

#[test]
fn all_eight_garden_corners_share_one_upright_and_adjoining_runs_break_independently() {
    let mut s = Simulation::with_seed(123.0);
    let posts: Vec<usize> = (0..s.covers.len())
        .filter(|&i| s.covers[i].timber_join.is_some_and(|join| join.post))
        .collect();
    assert_eq!(posts.len(), 8);
    for post in posts {
        let parts = timber_parts(&TimberWall::of(&s.covers[post]), 0);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].kind, TimberPartKind::Post);
        assert_eq!(parts[0].w, s.covers[post].w);
        assert_eq!(parts[0].d, s.covers[post].d);
        let p = s.covers[post].clone();
        let neighbors: Vec<usize> = (0..s.covers.len())
            .filter(|&i| {
                let c = &s.covers[i];
                if c.kind != CoverKind::Timber || c.timber_join.is_some_and(|join| join.post) {
                    return false;
                }
                let gap_x = (c.x - p.x).abs() - (c.w + p.w) / 2.0;
                let gap_z = (c.z - p.z).abs() - (c.d + p.d) / 2.0;
                ((gap_x - 0.04).abs() < 1e-6 && (c.z - p.z).abs() < 1e-6)
                    || ((gap_z - 0.04).abs() < 1e-6 && (c.x - p.x).abs() < 1e-6)
            })
            .collect();
        assert_eq!(
            neighbors.len(),
            2,
            "two perpendicular runs end at each connector"
        );
        for &neighbor in &neighbors {
            assert_eq!(
                timber_parts(&TimberWall::of(&s.covers[neighbor]), 0).len(),
                5,
                "no duplicate corner post on the run"
            );
        }
        s.damage_cover(neighbors[0], 80.0, 999, Team::Blue, None, None);
        assert!(s.covers[post].alive);
        assert_eq!(s.covers[neighbors[1]].hp, 80.0);
        let count = s.fragments.len();
        s.damage_cover(
            post,
            40.0,
            999,
            Team::Blue,
            None,
            Some(Point3::new(p.x, 1.0, p.z - 0.45)),
        );
        assert!(s.covers[post].alive);
        assert_eq!(
            timber_parts(&TimberWall::of(&s.covers[post]), 2)[0]
                .marks
                .len(),
            1
        );
        s.damage_cover(post, 40.0, 999, Team::Blue, None, None);
        assert!(!s.covers[post].alive);
        assert!(s.covers[neighbors[1]].alive);
        assert_eq!(
            s.fragments.len(),
            count + 1,
            "connector becomes exactly one physical post"
        );
        let fragment = s.fragments.last().unwrap();
        assert_eq!(
            fragment.timber_part.as_ref().map(|part| part.kind),
            Some(TimberPartKind::Post)
        );
        assert_eq!(fragment.dimensions, Some(Point3::new(p.w, p.h, p.d)));
        assert_eq!(collider_groups(&s, fragment.body), group::TIMBER_DEBRIS);
    }
}

#[test]
fn blast_can_destroy_adjacent_timber_bays_without_duplicate_destruction() {
    let mut s = Simulation::with_seed(123.0);
    let bays: Vec<usize> = (0..s.covers.len())
        .filter(|&i| {
            let c = &s.covers[i];
            c.kind == CoverKind::Timber && c.z == 13.0 && c.x.abs() == 2.0
        })
        .collect();
    let (id, team) = (s.human().id, s.human_team);
    s.explode(
        Vec2::new(0.0, 13.0),
        5.0,
        120.0,
        id,
        team,
        None,
        DamageCause::Explosion,
    );
    for bay in bays {
        assert!(!s.covers[bay].alive);
        s.damage_cover(bay, 999.0, id, team, None, None);
        let bay_id = s.covers[bay].id;
        assert_eq!(
            s.events
                .iter()
                .filter(|e| e.kind == SimEventType::Destroy && e.id == Some(bay_id))
                .count(),
            1
        );
    }
}

#[test]
fn timber_uses_four_beams_and_two_posts_retaining_their_damage_and_pose_on_breakup() {
    for along in [true, false] {
        let mut s = Simulation::with_seed(123.0);
        let (w, d) = if along { (4.2, 0.9) } else { (0.9, 4.2) };
        let wall = s.add_cover(&timber(0.0, 0.0, w, d));
        s.damage_cover(
            wall,
            105.0,
            999,
            Team::Blue,
            None,
            Some(Point3::new(0.7, 1.0, -0.45)),
        );
        let body = s.covers[wall].body;
        assert_eq!(s.world.bodies[body].colliders().len(), 1);
        assert!(s.world.bodies[body].is_fixed());
        let stage = timber_damage_stage(s.covers[wall].hp, s.covers[wall].max_hp);
        assert_eq!(stage, 3);
        let parts = timber_parts(&TimberWall::of(&s.covers[wall]), stage);
        assert_eq!(
            parts
                .iter()
                .filter(|p| p.kind == TimberPartKind::Beam)
                .count(),
            4
        );
        assert_eq!(
            parts
                .iter()
                .filter(|p| p.kind == TimberPartKind::Post)
                .count(),
            2
        );
        let (wx, wz) = (s.covers[wall].x, s.covers[wall].z);
        s.damage_cover(wall, 15.0, 999, Team::Blue, None, None);
        assert_eq!(s.fragments.len(), 6);
        for (fragment, part) in s.fragments.iter().zip(&parts) {
            assert_eq!(fragment.timber_part.as_ref(), Some(part));
            let p = s.body_translation(fragment.body);
            assert!((p.x - (wx + part.x)).abs() < 1e-5);
            assert!((p.y - part.y).abs() < 1e-5);
            assert!((p.z - (wz + part.z)).abs() < 1e-5);
            assert_eq!(
                fragment.dimensions,
                Some(Point3::new(part.w, part.h, part.d))
            );
            let expected = Quat4::from_euler_xyz(0.0, part.yaw, part.lean);
            let q = s.body_rotation(fragment.body);
            let dot = expected.x * q.x + expected.y * q.y + expected.z * q.z + expected.w * q.w;
            assert!(dot.abs() > 0.99999, "pose dot {dot}");
            assert_eq!(s.world.bodies[fragment.body].colliders().len(), 1);
            assert_eq!(collider_groups(&s, fragment.body), group::TIMBER_DEBRIS);
        }
    }
}

#[test]
fn detached_timber_beams_land_across_one_another_and_remain_stacked() {
    let mut s = Simulation::with_seed(123.0);
    clear_arena(&mut s, &[]);
    let wall = s.add_cover(&timber(0.0, 0.0, 4.2, 0.9));
    s.damage_cover(wall, 120.0, 999, Team::Blue, None, None);
    let extra: Vec<_> = s.fragments[2..].iter().map(|piece| piece.body).collect();
    for body in extra {
        s.remove_body(body);
    }
    s.fragments.truncate(2);
    for (i, piece) in s.fragments.iter().enumerate() {
        let body = &mut s.world.bodies[piece.body];
        body.set_translation(vector(0.0, 1.0 + i as f64 * 2.0, 0.0), true);
        let turn = if i > 0 { FRAC_1_SQRT_2 } else { 0.0 };
        let w = if i > 0 { FRAC_1_SQRT_2 } else { 1.0 };
        body.set_rotation(
            to_rotation(Quat4 {
                x: 0.0,
                y: turn,
                z: 0.0,
                w,
            }),
            true,
        );
        body.set_linvel(vector(0.0, 0.0, 0.0), true);
        body.set_angvel(vector(0.0, 0.0, 0.0), true);
    }
    for _ in 0..480 {
        s.world.step();
    }
    let (lower, upper) = (s.fragments[0].body, s.fragments[1].body);
    let a = s.body_translation(lower);
    let b = s.body_translation(upper);
    let thickness = s.fragments[0].dimensions.unwrap().y;
    assert!(
        (a.y - thickness / 2.0).abs() < 0.03,
        "lower beam rests on the ground: {}",
        a.y
    );
    assert!(
        b.y - a.y > thickness - 0.03,
        "upper beam rests on the lower beam, not through it: {} {}",
        a.y,
        b.y
    );
    assert!(
        b.x.abs() < 0.1 && b.z.abs() < 0.1,
        "crossed beams keep their support"
    );
    assert!(
        s.world.bodies[lower].is_sleeping() && s.world.bodies[upper].is_sleeping(),
        "the pile settles"
    );
    assert!(allows(group::TIMBER_DEBRIS, group::TIMBER_DEBRIS));
    assert!(allows(group::TIMBER_DEBRIS, group::TANK));
    for excluded in [group::FRAGMENT, group::COVER_QUERY, group::STEERING_QUERY] {
        assert!(!allows(group::TIMBER_DEBRIS, excluded));
    }
}
