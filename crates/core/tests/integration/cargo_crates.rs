//! Cargo crates as structures: a shell blows out the boards of the wall it strikes, down to
//! its frame, and its burst goes off in the hold, the lid stays on while enough wall is left under it and comes down
//! breaking along its boards once not, clients and late joiners see it all, the Scrap
//! Yard rebuilds a crate in place and the fallen boards clear away.

use crate::support;

use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::crate_layout::crate_wall_rows;
use sloppy_core::sim::crate_structure::{
    CRATE_BASE_PART, CRATE_LID_PART, CRATE_WALLS, crate_board_part,
};
use sloppy_core::sim::math::Vec2;
use sloppy_core::sim::structure::PartSet;
use sloppy_core::sim::structure_physics::breach_point;
use sloppy_core::sim::{CoverKind, SimEventType, Simulation, Weapon};
use support::{clear_arena, fire_at, place_tank, run};

/// A 3 m crate alone at (0, 10), the human parked far off.
fn lone_crate() -> (Simulation, usize) {
    let mut s = Simulation::with_seed(731.0);
    s.start();
    let human = s.human_index().expect("a human tank");
    clear_arena(&mut s, &[human]);
    place_tank(&mut s, 0, -40.0, -40.0, None);
    s.tanks[0].protection = 999.0;
    let crate_ = s.add_cover(&CoverDef::new(
        CoverKind::Cargo,
        0.0,
        10.0,
        3.0,
        3.0,
        2.6,
        80.0,
        0xb47a49,
    ));
    s.nav.rebuild(&s.covers, None);
    s.world.step();
    (s, crate_)
}

fn gone(s: &Simulation, crate_: usize) -> PartSet {
    s.covers[crate_]
        .structure
        .as_ref()
        .expect("a structure")
        .gone
}

#[test]
fn a_shell_tears_out_every_board_of_the_face_it_strikes_and_the_frame_stands() {
    let (rows, _) = crate_wall_rows(2.6);
    let face = |wall: usize| -> Vec<usize> {
        (0..rows)
            .map(|row| crate_board_part(2.6, wall, row))
            .collect()
    };
    // From every shell height and across the face, the middle and near its corners.
    for y in [0.96, 1.09, 1.4] {
        for x in [-1.1, 0.4, 1.1] {
            let (mut s, crate_) = lone_crate();
            let rng = s.rng.state;
            fire_at(
                &mut s,
                Vec2::new(x, 0.0),
                Vec2::new(x, 20.0),
                y,
                Weapon::Standard,
            );
            run(&mut s, 1.0);
            let down = gone(&s, crate_);
            // The back wall faces -z, toward the shell: all its boards go, its frame of
            // battens stands, and so does every other wall's sheathing and the lid.
            assert!(
                face(1).iter().all(|&board| down.contains(board)),
                "({x}, {y}): every board of the struck face went: {down:?}"
            );
            assert!(
                !down.contains(CRATE_WALLS[1]),
                "({x}, {y}): its frame stands"
            );
            assert!(
                !down.contains(CRATE_LID_PART),
                "({x}, {y}): the lid stays on"
            );
            assert!(!down.contains(CRATE_BASE_PART));
            // A hit near a corner bursts by the side wall, which may lose a board to it.
            for wall in if x == 0.4 { vec![0, 2, 3] } else { vec![0] } {
                assert!(
                    face(wall).iter().all(|&board| !down.contains(board)),
                    "({x}, {y}): wall {wall}'s boards stay: {down:?}"
                );
            }
            assert!(s.covers[crate_].alive);
            assert!(
                s.events.iter().any(|e| e.kind == SimEventType::Explosion),
                "the shell bursts in the hold"
            );
            assert_eq!(s.rng.state, rng, "a crate's break draws nothing");
        }
    }
}

/// Shell after shell through one wall's middle: the first opens its face, the second
/// flies through its frame and opens the far wall, and later ones fly through both; a
/// shell at a wall's battens breaks its frame (the strike and its burst against them),
/// and the wall comes away.
#[test]
fn shells_open_a_crate_face_by_face_and_its_battens_bring_a_wall_down() {
    let (mut s, crate_) = lone_crate();
    let (rows, _) = crate_wall_rows(2.6);
    let face = |wall: usize| -> Vec<usize> {
        (0..rows)
            .map(|row| crate_board_part(2.6, wall, row))
            .collect()
    };
    for _ in 0..3 {
        fire_at(
            &mut s,
            Vec2::new(0.4, 0.0),
            Vec2::new(0.4, 20.0),
            1.0,
            Weapon::Standard,
        );
        run(&mut s, 0.6);
    }
    let down = gone(&s, crate_);
    for wall in [0, 1] {
        assert!(
            face(wall).iter().all(|&board| down.contains(board)),
            "wall {wall} is open: {down:?}"
        );
        assert!(
            !down.contains(CRATE_WALLS[wall]),
            "wall {wall}'s frame stands"
        );
    }
    assert!(
        !down.contains(CRATE_LID_PART),
        "the lid stays on its frames"
    );
    assert!(s.covers[crate_].alive);
    fire_at(
        &mut s,
        Vec2::new(-1.42, 0.0),
        Vec2::new(-1.42, 20.0),
        1.0,
        Weapon::Standard,
    );
    run(&mut s, 0.6);
    assert!(
        gone(&s, crate_).contains(CRATE_WALLS[1]),
        "the struck frame broke"
    );
}

#[test]
fn the_lid_comes_down_once_too_little_wall_is_left_and_breaks_along_its_boards() {
    let (mut s, crate_) = lone_crate();
    let walls = PartSet::from_indices(CRATE_WALLS[..3].iter().copied());
    s.break_structure(crate_, walls, PartSet::EMPTY, None, None);
    let down = gone(&s, crate_);
    assert!(down.contains(CRATE_LID_PART), "the lid falls: {down:?}");
    assert!(down.contains(CRATE_WALLS[3]), "the lone wall goes with it");
    assert!(
        !s.covers[crate_].alive,
        "a crate without its walls is destroyed"
    );
    run(&mut s, 3.0);
    let pieces: Vec<PartSet> = s
        .fragments
        .iter()
        .filter(|f| f.of_structure(crate_))
        .filter_map(|f| f.structure_parts())
        .collect();
    let layout = s.covers[crate_].structure.as_ref().unwrap().layout.clone();
    assert!(
        pieces
            .iter()
            .any(|parts| parts.iter().any(|index| index >= layout.parts.len())),
        "the lid or the wall broke along its boards on landing: {pieces:?}"
    );
    run(&mut s, 20.0);
    assert!(
        !s.fragments.iter().any(|f| f.of_structure(crate_)),
        "the boards clear"
    );
}

#[test]
fn a_crates_breaks_and_falling_boards_reach_room_clients_and_late_joiners() {
    let (mut s, crate_) = lone_crate();
    support::assert_structure_replicates(
        &mut s,
        crate_,
        &[
            PartSet::from_indices([CRATE_WALLS[0]]),
            PartSet::from_indices([CRATE_WALLS[1], CRATE_WALLS[2]]),
        ],
    );
}

#[test]
fn a_scrap_yard_crate_falls_and_rises_again_in_place() {
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
    let crate_ = sim
        .covers
        .iter()
        .position(|c| c.kind == CoverKind::Cargo)
        .unwrap();
    for tank in 0..sim.tanks.len() {
        support::set_translation(&mut sim, tank, -30.0, 0.0);
    }
    let (x, z) = (sim.covers[crate_].x, sim.covers[crate_].z);
    sim.start();
    let (id, team) = (sim.human().id, sim.human().team);
    for cycle in 0..2 {
        sim.damage_cover(crate_, 1000.0, id, team, None, None);
        assert!(!sim.covers[crate_].alive, "cycle {cycle}: it broke");
        for _ in 0..(1.0 / sloppy_core::sim::data::STEP) as usize {
            sim.step(sloppy_core::sim::VehicleCommand::idle(), false);
        }
        superstress_rules(&mut sim);
        sim.elapsed += REBUILD_SECONDS;
        superstress_rules(&mut sim);
        assert!(sim.covers[crate_].alive, "cycle {cycle}: the crate rebuilt");
        assert!(gone(&sim, crate_).is_empty());
        assert!(
            !sim.fragments.iter().any(|f| f.of_structure(crate_)),
            "cycle {cycle}: its boards are cleared"
        );
        assert_eq!(
            sim.cover_by_collider
                .values()
                .filter(|&&index| index == crate_)
                .count(),
            1
        );
        assert!(sim.nav.is_blocked(Vec2::new(x, z)));
    }
}

/// Bots drive round a crate rather than shoot their way through it: its frame of
/// battens blocks a tank until several shells break it.
#[test]
fn bots_do_not_breach_a_crate() {
    let (s, crate_) = lone_crate();
    assert!(breach_point(&s.covers[crate_], Vec2::new(0.0, 0.0)).is_none());
}
