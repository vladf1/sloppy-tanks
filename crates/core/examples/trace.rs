//! Per-tick trace of a seeded autoplay match, for comparing against the TypeScript engine:
//! `cargo run -p sloppy-core --release --example trace -- <seed> <map|solo> <ticks>`.

use sloppy_core::sim::extra_levels::extra_level;
use sloppy_core::sim::level_rules::single_player_rules;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::{GameMode, Simulation, SimulationSetup, VehicleCommand};

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
        other => {
            let id = MapId::parse(other).expect("known map");
            let base = SimulationSetup {
                map_mode: Some(id),
                ..SimulationSetup::default()
            };
            match extra_level(id) {
                Some(level) => base.merged(single_player_rules(level)),
                None => base,
            }
        }
    };
    let mut simulation = Simulation::new(seed, setup);
    simulation.start();
    for tick in 1..=ticks {
        simulation.step(VehicleCommand::idle(), true);
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
