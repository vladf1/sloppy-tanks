//! Cold build times of the models a page builds before its first round: each theme's
//! scenery, the vehicles with their wrecks, and each map's covers at full health.
//! Model caches live for the process, so every sample is a fresh process that builds
//! one workload once; the report gives the minimum and median of the samples.
//!
//! `cargo run -p sloppy-core --release --example model_build_benchmark -- [runs] [workload...]`
//! (runs defaults to 15; workloads: village harbor quarry tanks vehicles village-covers
//! harbor-covers quarry-covers).

use std::process::Command;
use std::time::Instant;

use sloppy_core::models::{
    CoverShape, MapTheme, TreeDetail, WreckPart, batch, build_scenery, cover_model,
    release_tank_models, tank_model, wreck_model,
};
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::types::{Team, VehicleKind};
use sloppy_core::sim::{RenderState, Simulation};

const WORKLOADS: [&str; 8] = [
    "village",
    "harbor",
    "quarry",
    "tanks",
    "vehicles",
    "village-covers",
    "harbor-covers",
    "quarry-covers",
];

fn covers(map: MapId) -> impl FnOnce() {
    let mut simulation = Simulation::with_seed(417.0);
    simulation.map_mode = map;
    simulation.reset(None);
    let mut view = RenderState::default();
    simulation.fill_render_state(&mut view, None);
    move || {
        for cover in &view.covers {
            let mut root = cover_model(&CoverShape::from(cover), TreeDetail::Full, 0);
            batch(&mut root);
            std::hint::black_box(&root);
        }
    }
}

/// Build one workload once and return its milliseconds.
fn sample(workload: &str) -> f64 {
    let build: Box<dyn FnOnce()> = match workload {
        "village" => Box::new(|| drop(std::hint::black_box(build_scenery(MapTheme::Village)))),
        "harbor" => Box::new(|| drop(std::hint::black_box(build_scenery(MapTheme::Harbor)))),
        "quarry" => Box::new(|| drop(std::hint::black_box(build_scenery(MapTheme::Quarry)))),
        // The live vehicle models alone (released first, for --repeat).
        "tanks" => Box::new(|| {
            release_tank_models();
            for kind in VehicleKind::ALL {
                for team in [Team::Blue, Team::Red] {
                    std::hint::black_box(tank_model(kind, team));
                }
            }
        }),
        "vehicles" => Box::new(|| {
            for kind in VehicleKind::ALL {
                for team in [Team::Blue, Team::Red] {
                    std::hint::black_box(tank_model(kind, team));
                    for part in [
                        WreckPart::Intact,
                        WreckPart::Hull,
                        WreckPart::Turret,
                        WreckPart::TurretBarrel,
                        WreckPart::Barrel,
                    ] {
                        std::hint::black_box(wreck_model(kind, team, part));
                    }
                }
            }
        }),
        "village-covers" => Box::new(covers(MapId::Village)),
        "harbor-covers" => Box::new(covers(MapId::Harbor)),
        "quarry-covers" => Box::new(covers(MapId::Quarry)),
        other => panic!("unknown workload {other}"),
    };
    let start = Instant::now();
    build();
    start.elapsed().as_secs_f64() * 1e3
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [flag, workload] = args.as_slice()
        && flag == "--sample"
    {
        println!("{}", sample(workload));
        return;
    }
    // For a sampling profiler: one workload again and again in this process (warm
    // caches after the first).
    if let [flag, workload, count] = args.as_slice()
        && flag == "--repeat"
    {
        let count: usize = count.parse().expect("a count");
        let total: f64 = (0..count).map(|_| sample(workload)).sum();
        println!("{:.2} ms per build", total / count as f64);
        return;
    }
    let runs: usize = args.first().and_then(|a| a.parse().ok()).unwrap_or(15);
    let named: Vec<&str> = args
        .iter()
        .skip_while(|a| a.parse::<usize>().is_ok())
        .map(String::as_str)
        .collect();
    let workloads = if named.is_empty() {
        WORKLOADS.to_vec()
    } else {
        named
    };
    let exe = std::env::current_exe().expect("own path");
    for workload in workloads {
        let mut times: Vec<f64> = (0..runs)
            .map(|_| {
                let output = Command::new(&exe)
                    .args(["--sample", workload])
                    .output()
                    .expect("run a sample");
                assert!(output.status.success(), "{workload} sample failed");
                String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .parse()
                    .expect("milliseconds")
            })
            .collect();
        times.sort_by(f64::total_cmp);
        println!(
            "{workload:>15}: min {:7.2} ms  median {:7.2} ms  max {:7.2} ms  ({runs} runs)",
            times[0],
            times[runs / 2],
            times[runs - 1]
        );
    }
}
