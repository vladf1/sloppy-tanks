//! Seeded headless matches and reset checks (the former `scripts/validate.ts`):
//! `cargo run -p sloppy-core --release --example validate`. Writes its summary to the
//! ignored `artifacts/performance/simulation-results.json`.

use std::path::PathBuf;
use std::time::Instant;

use serde_json::json;
use sloppy_core::sim::{MatchPhase, SimEventType, Simulation, VehicleCommand};

/// Rounds per run, each seeded `round * SEED_STRIDE`.
const ROUNDS: u32 = 10;
const SEED_STRIDE: f64 = 79.0;
/// Rounds up to this one leave the human idle; later rounds hand it to the bot brain.
const IDLE_HUMAN_ROUNDS: u32 = 3;
/// A round stops after six simulated minutes even without a result.
const MAX_STEPS: u32 = 60 * 360;

fn main() {
    let started = Instant::now();
    let mut rounds = Vec::new();
    let mut max_bodies = 0;
    let mut max_fragments = 0;
    for round in 1..=ROUNDS {
        let mut simulation = Simulation::with_seed(round as f64 * SEED_STRIDE);
        let initial_bodies = simulation.world.bodies.len();
        simulation.start();
        let mut steps = 0;
        let (mut promotions, mut bot_promotions, mut elite, mut heroic) = (0, 0, 0, 0);
        while simulation.match_state.phase == MatchPhase::Playing && steps < MAX_STEPS {
            simulation.step(VehicleCommand::idle(), round > IDLE_HUMAN_ROUNDS);
            let human = simulation.human().id;
            for event in simulation.events.drain(..) {
                if event.kind != SimEventType::Promotion {
                    continue;
                }
                promotions += 1;
                if event.id != Some(human) {
                    bot_promotions += 1;
                }
                match event.label.as_deref() {
                    Some("PROMOTED TO ELITE") => elite += 1,
                    Some("PROMOTED TO HEROIC") => heroic += 1,
                    _ => {}
                }
            }
            max_bodies = max_bodies.max(simulation.world.bodies.len());
            max_fragments = max_fragments.max(simulation.fragments.len());
            steps += 1;
        }
        let summary = json!({
            "seed": simulation.seed,
            "humanIdle": round <= IDLE_HUMAN_ROUNDS,
            "seconds": steps as f64 / 60.0,
            "scores": simulation.match_state.scores,
            "winner": simulation.match_state.winner,
            "destroyed": simulation.destroyed,
            "reroutes": simulation.bot_reroutes,
            "breachShots": simulation.bot_breach_shots,
            "veterancy": {
                "promotions": promotions,
                "botPromotions": bot_promotions,
                "elite": elite,
                "heroic": heroic,
            },
            "towersRemaining": simulation
                .covers
                .iter()
                .filter(|cover| cover.kind == sloppy_core::sim::CoverKind::Tower && cover.alive)
                .count(),
        });
        simulation.reset(None);
        assert_eq!(
            simulation.world.bodies.len(),
            initial_bodies,
            "Reset count {}",
            simulation.world.bodies.len()
        );
        println!("{summary}");
        rounds.push(summary);
    }
    let result = json!({
        "type": "accelerated simulation, no rendering",
        "wallSeconds": started.elapsed().as_secs_f64(),
        "rounds": rounds,
        "maxBodies": max_bodies,
        "maxFragments": max_fragments,
    });
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../artifacts/performance");
    std::fs::create_dir_all(&directory).expect("create artifacts/performance");
    std::fs::write(
        directory.join("simulation-results.json"),
        serde_json::to_string_pretty(&result).expect("serializable results"),
    )
    .expect("write simulation-results.json");
}
