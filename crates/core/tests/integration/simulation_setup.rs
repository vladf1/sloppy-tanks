//! A simulation built directly from a setup matches one configured after construction and
//! reset, before and after seeded combat and across later rounds (the former
//! `tests/simulation-setup.test.ts`).

use rapier3d::prelude::{ColliderHandle, RigidBodyHandle};
use sloppy_core::sim::data::STEP;
use sloppy_core::sim::difficulty::Difficulty;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::math::js_round;
use sloppy_core::sim::{GameMode, Simulation, SimulationSetup, Team, VehicleCommand, VehicleKind};

/// Everything that identifies a round, with physics handles replaced by body poses.
fn snapshot(sim: &Simulation) -> String {
    let tanks: Vec<String> = sim
        .tanks
        .iter()
        .map(|tank| {
            let mut plain = tank.clone();
            plain.body = RigidBodyHandle::invalid();
            plain.collider = ColliderHandle::invalid();
            format!(
                "{plain:?} position={:?} velocity={:?}",
                sim.body_translation(tank.body),
                sim.body_linvel(tank.body)
            )
        })
        .collect();
    let covers: Vec<String> = sim
        .covers
        .iter()
        .map(|c| format!("{} {:?} {} {} {} {}", c.id, c.kind, c.x, c.z, c.hp, c.alive))
        .collect();
    format!(
        "match={:?}\nmap={}\nrng={}\nnext_id={}\npickups={:?}\nbodies={}\ncolliders={}\ntanks={tanks:#?}\ncovers={covers:?}\nshots={:?}",
        sim.match_state,
        sim.map_name(),
        sim.rng.state,
        sim.next_id,
        sim.pickups,
        sim.world.bodies.len(),
        sim.world.colliders.len(),
        sim.shots,
    )
}

#[test]
fn direct_setup_builds_one_world_and_preserves_legacy_seeded_rounds_and_subsequent_resets() {
    // One team and one solo round cover the options that change roster and RNG order.
    let cases = [
        SimulationSetup {
            map_mode: Some(MapId::Harbor),
            human_team: Some(Team::Blue),
            human_kind: Some(VehicleKind::Scout),
            ..SimulationSetup::default()
        },
        SimulationSetup {
            map_mode: Some(MapId::Quarry),
            game_mode: Some(GameMode::Solo),
            difficulty: Some(Difficulty::Hard),
            ..SimulationSetup::default()
        },
    ];
    for options in cases {
        // The TS test also counted `reset` calls with a mock (exactly one for the direct
        // setup); Rust cannot observe that, but the matching round number below shows the
        // constructor prepared round 3 without advancing through an extra round.
        let mut direct = Simulation::new(
            20402.0,
            options.clone().merged(SimulationSetup {
                round: Some(3),
                ..SimulationSetup::default()
            }),
        );
        let mut legacy = Simulation::with_seed(20402.0);
        options.apply(&mut legacy);
        legacy.reset(None);
        assert_eq!(direct.match_state.round, 3);
        assert_eq!(snapshot(&direct), snapshot(&legacy));
        direct.start();
        legacy.start();
        for _ in 0..js_round(2.0 / STEP) as usize {
            direct.step(VehicleCommand::idle(), true);
            legacy.step(VehicleCommand::idle(), true);
        }
        assert_eq!(
            snapshot(&direct),
            snapshot(&legacy),
            "seeded combat must remain identical"
        );
        direct.reset(None);
        legacy.reset(None);
        assert_eq!(
            snapshot(&direct),
            snapshot(&legacy),
            "later rounds retain map/RNG order"
        );
    }
}
