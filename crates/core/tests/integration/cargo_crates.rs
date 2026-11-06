//! Cargo crates as structures: a shell blows out the wall it strikes and its burst goes
//! off in the hold, the lid stays on while enough wall is left under it and comes down
//! breaking along its boards once not, clients and late joiners see it all, the Scrap
//! Yard rebuilds a crate in place and the fallen boards clear away.

use crate::support;

use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::crate_structure::{CRATE_BASE_PART, CRATE_LID_PART, CRATE_WALLS};
use sloppy_core::sim::math::Vec2;
use sloppy_core::sim::structure::PartSet;
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
fn a_shell_blows_out_the_wall_it_strikes_and_the_lid_stays_on() {
    let (mut s, crate_) = lone_crate();
    let rng = s.rng.state;
    fire_at(
        &mut s,
        Vec2::new(0.4, 0.0),
        Vec2::new(0.4, 20.0),
        1.0,
        Weapon::Standard,
    );
    run(&mut s, 1.0);
    let down = gone(&s, crate_);
    // The back wall faces -z, toward the shell.
    assert!(
        down.contains(CRATE_WALLS[1]),
        "the struck wall is blown out: {down:?}"
    );
    assert!(
        !down.contains(CRATE_LID_PART),
        "the lid stays on three walls"
    );
    assert!(!down.contains(CRATE_BASE_PART));
    assert!(s.covers[crate_].alive);
    assert!(
        s.events.iter().any(|e| e.kind == SimEventType::Explosion),
        "the shell bursts in the hold"
    );
    assert_eq!(s.rng.state, rng, "a crate's break draws nothing");
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
    let parts = sim.covers[crate_]
        .structure
        .as_ref()
        .unwrap()
        .layout
        .parts
        .len();
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
            parts
        );
        assert!(sim.nav.is_blocked(Vec2::new(x, z)));
    }
}
