//! Timber walls as structures: authored walls never overlap, one shell knocks the beam it
//! strikes out of its posts and leaves a gap the next shell and a line of sight pass
//! through, the posts carry the beams (one shot out drops what it held), clients and late
//! joiners see it all, the Scrap Yard rebuilds a wall in place and the fallen members
//! clear away.

use crate::support;

use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::maps::{ArenaMap, MAPS};
use sloppy_core::sim::math::Vec2;
use sloppy_core::sim::stress_test_level::STRESS_TEST_MAP;
use sloppy_core::sim::structure::PartSet;
use sloppy_core::sim::structure::Role;
use sloppy_core::sim::structure_physics::breach_point;
use sloppy_core::sim::superstress_level::SUPERSTRESS_MAP;
use sloppy_core::sim::timber_layout::{BEAM_COUNT, TIMBER_HEALTH, TimberPartKind};
use sloppy_core::sim::timber_structure::TimberRole;
use sloppy_core::sim::{CoverKind, DamageCause, SimEventType, Simulation, Team, Weapon};
use support::{clear_arena, fire_at, place_tank, run};

/// A 3.7 m timber wall alone along x at z = 10, the human parked far off.
fn lone_wall() -> (Simulation, usize) {
    let mut s = Simulation::with_seed(731.0);
    s.start();
    let human = s.human_index().expect("a human tank");
    clear_arena(&mut s, &[human]);
    place_tank(&mut s, 0, -40.0, -40.0, None);
    s.tanks[0].protection = 999.0;
    let wall = s.add_cover(&CoverDef::new(
        CoverKind::Timber,
        0.0,
        10.0,
        3.7,
        0.9,
        2.8,
        TIMBER_HEALTH,
        0xb47a49,
    ));
    s.nav.rebuild(&s.covers, None);
    s.world.step();
    (s, wall)
}

fn gone(s: &Simulation, wall: usize) -> PartSet {
    s.covers[wall].structure.as_ref().expect("a structure").gone
}

/// The wall's member of `kind` numbered `index` (beams bottom up, posts -x then +x).
fn member(s: &Simulation, wall: usize, kind: TimberPartKind, index: usize) -> usize {
    let layout = &s.covers[wall].structure.as_ref().unwrap().layout;
    (0..layout.parts.len())
        .find(|&part| {
            matches!(layout.parts[part].role, Role::Timber(TimberRole { kind: k, index: i }) if k == kind && i == index)
        })
        .expect("the member")
}

#[test]
fn timber_barriers_meet_without_overlapping_colliders_across_every_map() {
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
fn a_beam_shot_out_leaves_a_gap_the_next_shell_and_sight_pass_through() {
    let (mut s, wall) = lone_wall();
    let rng = s.rng.state;
    let (front, back) = (Vec2::new(0.3, 6.0), Vec2::new(0.3, 14.0));
    assert!(!s.visible(front, back), "a whole wall blocks the line");
    fire_at(
        &mut s,
        Vec2::new(0.3, 0.0),
        Vec2::new(0.3, 20.0),
        1.0,
        Weapon::Standard,
    );
    run(&mut s, 1.0);
    let beam = member(&s, wall, TimberPartKind::Beam, 1);
    assert!(gone(&s, wall).contains(beam), "the struck beam is out");
    for post in [BEAM_COUNT, BEAM_COUNT + 1] {
        assert!(!gone(&s, wall).contains(post), "post {post} stands");
    }
    assert!(s.covers[wall].alive, "one beam does not level the wall");
    assert!(
        s.events
            .iter()
            .any(|e| e.kind == SimEventType::Shatter && e.cover_kind == Some(CoverKind::Timber)),
        "the beam splinters"
    );
    assert!(s.visible(front, back), "sight passes through the gap");
    // The next shell along the same line flies through and on.
    let before = gone(&s, wall);
    let id = fire_at(
        &mut s,
        Vec2::new(0.3, 0.0),
        Vec2::new(0.3, 20.0),
        1.0,
        Weapon::Standard,
    );
    while s.shots.iter().any(|shot| shot.id == id && shot.z < 11.0) {
        run(&mut s, 1.0 / 60.0);
    }
    let shell = s
        .shots
        .iter()
        .find(|shot| shot.id == id)
        .expect("the shell flies on");
    assert!(shell.z > 11.0, "past the wall: {}", shell.z);
    assert_eq!(gone(&s, wall), before, "it breaks nothing on its way");
    assert_eq!(s.rng.state, rng, "breaking a beam draws nothing");
}

#[test]
fn the_posts_carry_the_beams_and_one_shot_out_drops_what_it_held() {
    let (mut s, wall) = lone_wall();
    let post = member(&s, wall, TimberPartKind::Post, BEAM_COUNT);
    let x = s.covers[wall].structure.as_ref().unwrap().layout.parts[post]
        .middle()
        .x;
    // A level shell finds something to break: a post or the beam at its height.
    assert!(breach_point(&s.covers[wall], Vec2::new(0.0, 0.0)).is_some());
    fire_at(
        &mut s,
        Vec2::new(x, 0.0),
        Vec2::new(x, 20.0),
        1.0,
        Weapon::Piercing,
    );
    run(&mut s, 1.0);
    let down = gone(&s, wall);
    assert!(down.contains(post), "the post is shot out");
    for index in 1..BEAM_COUNT {
        let beam = member(&s, wall, TimberPartKind::Beam, index);
        assert!(down.contains(beam), "beam {index} it held comes down");
    }
    let bottom = member(&s, wall, TimberPartKind::Beam, 0);
    assert!(!down.contains(bottom), "the beam lying on the ground stays");
    assert!(
        s.fragments.iter().any(|f| f.of_structure(wall)),
        "the beams fall as bodies"
    );
    // The other post planted in the ground stands alone.
    assert!(!down.contains(member(&s, wall, TimberPartKind::Post, BEAM_COUNT + 1)));
    let bodies = s.world.bodies.len();
    let falling = s.fragments.iter().filter(|f| f.of_structure(wall)).count();
    run(&mut s, 25.0);
    assert!(
        !s.fragments.iter().any(|f| f.of_structure(wall)),
        "the fallen beams clear"
    );
    assert_eq!(
        s.world.bodies.len(),
        bodies - falling,
        "no body is left behind"
    );
}

#[test]
fn a_walls_breaks_and_falling_members_reach_room_clients_and_late_joiners() {
    let (mut s, wall) = lone_wall();
    let beam = member(&s, wall, TimberPartKind::Beam, 1);
    let post = member(&s, wall, TimberPartKind::Post, BEAM_COUNT);
    support::assert_structure_replicates(
        &mut s,
        wall,
        &[PartSet::from_indices([beam]), PartSet::from_indices([post])],
    );
}

#[test]
fn all_eight_garden_corners_share_one_upright_and_adjoining_runs_break_independently() {
    let mut s = Simulation::with_seed(123.0);
    let posts: Vec<usize> = (0..s.covers.len())
        .filter(|&i| s.covers[i].timber_join.is_some_and(|join| join.post))
        .collect();
    assert_eq!(posts.len(), 8);
    let parts = |s: &Simulation, cover: usize| {
        s.covers[cover]
            .structure
            .as_ref()
            .unwrap()
            .layout
            .parts
            .len()
    };
    for post in posts {
        assert_eq!(parts(&s, post), 1, "a corner is one post");
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
                parts(&s, neighbor),
                BEAM_COUNT + 1,
                "no duplicate corner post on the run"
            );
        }
        s.damage_cover(neighbors[0], 1000.0, 999, Team::Blue, None, None);
        assert!(!s.covers[neighbors[0]].alive);
        assert!(s.covers[post].alive);
        assert!(s.covers[neighbors[1]].alive);
        assert!(gone(&s, neighbors[1]).is_empty());
        s.damage_cover(post, 1000.0, 999, Team::Blue, None, None);
        assert!(!s.covers[post].alive);
        assert!(s.covers[neighbors[1]].alive);
    }
}

#[test]
fn a_blast_can_level_adjacent_bays_each_exactly_once() {
    let mut s = Simulation::with_seed(123.0);
    let bays: Vec<usize> = (0..s.covers.len())
        .filter(|&i| {
            let c = &s.covers[i];
            c.kind == CoverKind::Timber && c.z == 13.0 && c.x.abs() == 2.0
        })
        .collect();
    let (id, team) = (s.human().id, s.human_team);
    for _ in 0..3 {
        s.explode(
            Vec2::new(0.0, 13.0),
            5.0,
            120.0,
            id,
            team,
            None,
            DamageCause::Explosion,
        );
    }
    for bay in bays {
        assert!(!gone(&s, bay).is_empty(), "the blast reached bay {bay}");
        s.damage_cover(bay, 9999.0, id, team, None, None);
        assert!(!s.covers[bay].alive);
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
fn a_scrap_yard_wall_falls_and_rises_again_in_place() {
    use sloppy_core::sim::SimulationSetup;
    use sloppy_core::sim::level_rules::single_player_rules;
    use sloppy_core::sim::superstress_level::{
        REBUILD_SECONDS, superstress_level, superstress_rules,
    };
    let mut sim = Simulation::new(
        731.0,
        single_player_rules(superstress_level()).merged(SimulationSetup {
            round: Some(3),
            ..SimulationSetup::default()
        }),
    );
    let wall = sim
        .covers
        .iter()
        .position(|c| c.kind == CoverKind::Timber && c.timber_join.is_none())
        .unwrap();
    for tank in 0..sim.tanks.len() {
        support::set_translation(&mut sim, tank, -30.0, 0.0);
    }
    let (x, z) = (sim.covers[wall].x, sim.covers[wall].z);
    let parts = sim.covers[wall]
        .structure
        .as_ref()
        .unwrap()
        .layout
        .parts
        .len();
    sim.start();
    let (id, team) = (sim.human().id, sim.human().team);
    for cycle in 0..2 {
        sim.damage_cover(wall, 1000.0, id, team, None, None);
        assert!(!sim.covers[wall].alive, "cycle {cycle}: it fell");
        for _ in 0..(1.0 / sloppy_core::sim::data::STEP) as usize {
            sim.step(sloppy_core::sim::VehicleCommand::idle(), false);
        }
        superstress_rules(&mut sim);
        sim.elapsed += REBUILD_SECONDS;
        superstress_rules(&mut sim);
        assert!(sim.covers[wall].alive, "cycle {cycle}: the wall rebuilt");
        assert!(gone(&sim, wall).is_empty());
        assert!(
            !sim.fragments.iter().any(|f| f.of_structure(wall)),
            "cycle {cycle}: its members are cleared"
        );
        assert_eq!(
            sim.cover_by_collider
                .values()
                .filter(|&&index| index == wall)
                .count(),
            parts,
            "cycle {cycle}: every member stands again"
        );
        assert!(
            sim.nav.is_blocked(Vec2::new(x, z)),
            "it blocks routes again"
        );
    }
}
