//! Headless seeded autoplay tick time on one map:
//! `cargo run -p sloppy-core --release --example simulation_benchmark -- <map> <seed>`.
//! Warms up 600 ticks, times 3600 and prints JSON. Manual evidence, not a CI gate: for
//! an engine change, run a base-commit worktree and the candidate alternately over several
//! maps and seeds.

use std::time::Instant;

use serde_json::json;
use sloppy_core::sim::extra_levels::extra_level;
use sloppy_core::sim::level_rules::single_player_rules;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::{MatchPhase, Simulation, SimulationSetup, VehicleCommand};

const WARMUP_TICKS: usize = 600;
const MEASURE_TICKS: usize = 3600;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let map = args.get(1).map_or("village", String::as_str);
    let seed: f64 = args
        .get(2)
        .and_then(|text| text.parse().ok())
        .unwrap_or(79.0);
    let id = MapId::parse(map).expect("known map id");
    let setup = SimulationSetup {
        map_mode: Some(id),
        ..SimulationSetup::default()
    }
    .merged(extra_level(id).map(single_player_rules).unwrap_or_default());
    let mut simulation = Simulation::new(seed, setup);
    simulation.start();
    let mut tick_ms = Vec::with_capacity(MEASURE_TICKS);
    let (mut max_bodies, mut max_fragments) = (0, 0);
    for tick in 0..WARMUP_TICKS + MEASURE_TICKS {
        if simulation.match_state.phase != MatchPhase::Playing {
            break;
        }
        let start = Instant::now();
        simulation.step(VehicleCommand::idle(), true);
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        simulation.events.clear();
        if tick < WARMUP_TICKS {
            continue;
        }
        tick_ms.push(elapsed);
        max_bodies = max_bodies.max(simulation.world.bodies.len());
        max_fragments = max_fragments.max(simulation.fragments.len());
    }
    let ticks = tick_ms.len();
    assert!(ticks > 0, "The match ended during warm-up");
    let mean = tick_ms.iter().sum::<f64>() / ticks as f64;
    let mut sorted = tick_ms;
    sorted.sort_by(f64::total_cmp);
    let percentile = |p: f64| sorted[((p * ticks as f64) as usize).min(ticks - 1)];
    println!(
        "{}",
        json!({
            "map": map,
            "seed": seed,
            "ticks": ticks,
            "tanks": simulation.tanks.len(),
            "maxBodies": max_bodies,
            "maxFragments": max_fragments,
            "meanMs": mean,
            "p50": percentile(0.5),
            "p95": percentile(0.95),
            "p99": percentile(0.99),
            "max": sorted[ticks - 1],
        })
    );
}
