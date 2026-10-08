//! Per-room server work for one 50 ms host interval: three fixed steps with the projectile
//! path recording `MatchHost` does after each, then the scene capture, field diff and JSON
//! it performs for each snapshot frame.
//! `cargo run -p sloppy-core --release --example capture_benchmark -- [output.json]`
//! seeds each standard map and both extra levels with one idle player, warms up 1200
//! ticks, then times 400 intervals. Manual evidence, not a CI gate.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use serde_json::{Value, json};
use sloppy_core::net::multiplayer_simulation::{MultiplayerOptions, create_multiplayer_simulation};
use sloppy_core::net::replication::StateStream;
use sloppy_core::net::scene_codec::Scene;
use sloppy_core::net::shot_paths::ShotPathRecorder;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::{PlayerAssignment, Team, VehicleKind};

const WARMUP_TICKS: u64 = 1200;
const INTERVALS: usize = 400;
const STEPS_PER_INTERVAL: u64 = 3;
const SEED: f64 = 4242.0;
const ROOMS: [(&str, MapId); 5] = [
    ("village", MapId::Village),
    ("harbor", MapId::Harbor),
    ("quarry", MapId::Quarry),
    ("stress-grid", MapId::StressTest),
    ("scrap-yard", MapId::Superstress),
];
const STAGES: [&str; 4] = ["physics", "paths", "capture", "diff+json"];

fn summary(samples: &mut [f64]) -> Value {
    samples.sort_by(f64::total_cmp);
    let n = samples.len();
    let at = |fraction: f64| samples[((fraction * n as f64) as usize).min(n - 1)];
    let round = |value: f64| (value * 1000.0).round() / 1000.0;
    json!({
        "n": n,
        "mean": round(samples.iter().sum::<f64>() / n as f64),
        "p50": round(at(0.5)),
        "p95": round(at(0.95)),
        "max": round(samples[n - 1]),
    })
}

fn main() {
    let output = std::env::args().nth(1).map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../artifacts/performance/capture-benchmark.json")
        },
        PathBuf::from,
    );
    let player = PlayerAssignment {
        player_id: "one".into(),
        name: "One".into(),
        team: Team::Blue,
        slot: 0,
        kind: VehicleKind::Balanced,
    };
    let mut results = serde_json::Map::new();
    for (name, map) in ROOMS {
        let options = MultiplayerOptions {
            map_mode: Some(map),
            ..MultiplayerOptions::default()
        };
        let mut sim = create_multiplayer_simulation(SEED, std::slice::from_ref(&player), options)
            .expect("valid room");
        sim.start();
        sim.projectile_moves = Some(Vec::new());
        let mut paths = ShotPathRecorder::default();
        let idle = BTreeMap::new();
        for tick in 1..=WARMUP_TICKS {
            sim.step_with(&idle);
            sim.events.clear();
            paths.follow(&mut sim, tick);
        }
        paths.clear_entries();
        let mut stream = StateStream::new("benchmark", 1);
        stream.full(&Scene::capture(&sim), WARMUP_TICKS, 0, paths.paths());
        let mut samples: [Vec<f64>; 4] = Default::default();
        let mut scene = Scene::default();
        let mut tick = WARMUP_TICKS;
        let mut bytes = 0;
        for _ in 0..INTERVALS {
            let mut physics = 0.0;
            let mut recording = 0.0;
            for _ in 0..STEPS_PER_INTERVAL {
                let start = Instant::now();
                sim.step_with(&idle);
                sim.events.clear();
                tick += 1;
                let stepped = Instant::now();
                paths.follow(&mut sim, tick);
                physics += (stepped - start).as_secs_f64() * 1000.0;
                recording += stepped.elapsed().as_secs_f64() * 1000.0;
            }
            let stepped = Instant::now();
            scene.capture_from(&sim);
            let captured = Instant::now();
            bytes += stream
                .snapshot(&mut scene, tick, &[], paths.entries())
                .len();
            paths.clear_entries();
            let done = Instant::now();
            samples[0].push(physics);
            samples[1].push(recording);
            samples[2].push((captured - stepped).as_secs_f64() * 1000.0);
            samples[3].push((done - captured).as_secs_f64() * 1000.0);
        }
        println!(
            "{name} ({} tanks, {} covers, {} fragments; {INTERVALS} intervals, {} B/frame)",
            sim.tanks.len(),
            sim.covers.len(),
            sim.fragments.len(),
            bytes / INTERVALS
        );
        let mut room = serde_json::Map::new();
        for (stage, values) in STAGES.iter().zip(samples.iter_mut()) {
            let stats = summary(values);
            println!(
                "  {stage:<9} mean {} ms  p50 {}  p95 {}  max {}",
                stats["mean"], stats["p50"], stats["p95"], stats["max"]
            );
            room.insert(stage.to_string(), stats);
        }
        results.insert(name.to_string(), Value::Object(room));
    }
    if let Some(directory) = output.parent() {
        std::fs::create_dir_all(directory).expect("create the output directory");
    }
    let report = json!({
        "warmupTicks": WARMUP_TICKS,
        "intervals": INTERVALS,
        "results": results,
    });
    std::fs::write(
        &output,
        serde_json::to_string_pretty(&report).expect("JSON"),
    )
    .expect("write the report");
    println!("Wrote {}", output.display());
}
