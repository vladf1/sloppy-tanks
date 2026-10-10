//! Per-tick trace of a seeded autoplay match, for comparing against the TypeScript engine:
//! `cargo run -p sloppy-core --release --example trace -- <seed> <map|solo|mp> <ticks>`.
//! Its TypeScript twin, `crates/core/tests/fixtures/trace.ts`, left with that engine; run it
//! from Git history in a baseline checkout (`35afd91`). The two print identical lines until
//! the physics engines' contact responses first differ.

use std::collections::BTreeMap;

use sloppy_core::sim::extra_levels::extra_level;
use sloppy_core::sim::level_rules::single_player_rules;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::{
    GameMode, PlayerAssignment, Simulation, SimulationSetup, Team, VehicleCommand, VehicleKind,
};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed: f64 = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(12345.0);
    let map = args.get(2).map_or("village", String::as_str);
    let ticks: usize = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(600);
    let setup = match map {
        "solo" => SimulationSetup {
            game_mode: Some(GameMode::Solo),
            ..SimulationSetup::default()
        },
        // Two seats: a heavy driving north and firing every 50 ticks, and an idle scout.
        "mp" => SimulationSetup {
            game_mode: Some(GameMode::Team),
            round_count: Some(12),
            players: Some(vec![
                PlayerAssignment {
                    player_id: "a".into(),
                    name: "ALPHA".into(),
                    team: Team::Blue,
                    slot: 1,
                    kind: VehicleKind::Heavy,
                },
                PlayerAssignment {
                    player_id: "b".into(),
                    name: "BRAVO".into(),
                    team: Team::Red,
                    slot: 0,
                    kind: VehicleKind::Scout,
                },
            ]),
            ..SimulationSetup::default()
        },
        other => {
            let id = MapId::parse(other).expect("known map");
            SimulationSetup {
                map_mode: Some(id),
                ..SimulationSetup::default()
            }
            .merged(extra_level(id).map(single_player_rules).unwrap_or_default())
        }
    };
    let mut simulation = Simulation::new(seed, setup);
    simulation.start();
    for tick in 1..=ticks {
        if map == "mp" {
            let seat = simulation
                .tanks
                .iter()
                .find(|tank| tank.player_id.as_deref() == Some("a"))
                .expect("seat a has a tank")
                .id;
            let command = VehicleCommand {
                move_z: 1.0,
                fire: tick % 50 == 0,
                ..VehicleCommand::idle()
            };
            simulation.step_with(&BTreeMap::from([(seat, command)]));
        } else {
            simulation.step(VehicleCommand::idle(), true);
        }
        let tanks: Vec<String> = simulation
            .tanks
            .iter()
            .map(|tank| {
                if tank.alive {
                    let p = simulation.body_translation(tank.body);
                    format!("{:.3},{:.3}", p.x, p.z)
                } else {
                    "dead".to_string()
                }
            })
            .collect();
        println!(
            "{tick} rng={} shots={} frags={} destroyed={} {}",
            simulation.rng.state,
            simulation.shots.len(),
            simulation.fragments.len(),
            simulation.destroyed,
            tanks.join(" ")
        );
    }
}
