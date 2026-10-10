//! Manual native allocation evidence, not a CI gate or a browser/GPU timing claim.
//! `cargo run --release -p sloppy-web --example allocation_benchmark -- output.json [seed]`
//! Counts allocation/reallocation requests and requested bytes (not live/peak memory).
//! Keeps every sample, including warm-up-free timing outliers, and hashes wire/render/HUD
//! output outside the measured stages so before/after runs also check seeded parity.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::time::Instant;

use serde::Serialize;
use sloppy_core::net::multiplayer_simulation::{MultiplayerOptions, create_multiplayer_simulation};
use sloppy_core::net::network_timeline::NetworkTimeline;
use sloppy_core::net::replication::StateStream;
use sloppy_core::net::scene_codec::Scene;
use sloppy_core::net::shot_paths::{LivePaths, ShotPathRecorder};
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::{PlayerAssignment, RenderState, Team, VehicleKind};
use sloppy_render::effects::EffectSystems;
use sloppy_web::hud::{Scoreboard, human_json};

struct CountingAllocator;
static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

fn record(bytes: usize) {
    if COUNTING.load(Relaxed) {
        ALLOCATIONS.fetch_add(1, Relaxed);
        BYTES.fetch_add(bytes as u64, Relaxed);
    }
}

// Delegates each allocation to System with the original pointer/layout unchanged.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[derive(Serialize)]
struct Sample {
    allocations: u64,
    bytes: u64,
    micros: f64,
}

fn measure<T>(run: impl FnOnce() -> T) -> (T, Sample) {
    ALLOCATIONS.store(0, Relaxed);
    BYTES.store(0, Relaxed);
    COUNTING.store(true, Relaxed);
    let start = Instant::now();
    let result = run();
    let micros = start.elapsed().as_secs_f64() * 1e6;
    COUNTING.store(false, Relaxed);
    (
        result,
        Sample {
            allocations: ALLOCATIONS.load(Relaxed),
            bytes: BYTES.load(Relaxed),
            micros,
        },
    )
}

fn hash(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash = (*hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
    }
}

fn main() {
    let output = std::env::args().nth(1).expect("output.json argument");
    let seed = std::env::args()
        .nth(2)
        .map_or(4242.0, |value| value.parse().expect("numeric seed"));
    let mut reports = Vec::new();
    for map in [
        MapId::Village,
        MapId::Harbor,
        MapId::Quarry,
        MapId::StressTest,
        MapId::Superstress,
    ] {
        let player = PlayerAssignment {
            player_id: "one".into(),
            name: "One".into(),
            team: Team::Blue,
            slot: 0,
            kind: VehicleKind::Balanced,
        };
        let mut simulation = create_multiplayer_simulation(
            seed,
            &[player],
            MultiplayerOptions {
                map_mode: Some(map),
                ..MultiplayerOptions::default()
            },
        )
        .expect("valid room");
        simulation.start();
        simulation.projectile_moves = Some(Vec::new());
        let mut paths = ShotPathRecorder::default();
        let idle = BTreeMap::new();
        let mut scene = Scene::default();
        let mut stream = StateStream::new("benchmark", 1);
        let mut state = RenderState::default();
        let mut display = RenderState::default();
        let mut timeline = NetworkTimeline::default();
        let mut effects = EffectSystems::default();
        let mut samples: BTreeMap<&str, Vec<Sample>> = BTreeMap::new();
        let mut hashes = [0xcbf29ce484222325; 3];
        // 20 seconds warm-up, then 20 seconds measured, 3 ticks per snapshot.
        for interval in 0..800 {
            let tick = (interval + 1) * 3;
            let (_, step) = measure(|| {
                for step in 0..3 {
                    simulation.step_with(&idle);
                    simulation.events.clear();
                    paths.follow(&mut simulation, tick - 2 + step);
                }
            });
            let (_, capture) = measure(|| scene.capture_from(&simulation));
            let (wire, diff) = measure(|| stream.snapshot(&mut scene, tick, &[], paths.entries()));
            let entries = paths.entries().to_vec();
            paths.clear_entries();
            let (_, render) = measure(|| simulation.fill_render_state(&mut state, None));
            let now = tick as f64 * 1000.0 / 60.0;
            // The client constructs an owned frame from its mirror before enqueueing it.
            // Keep that construction outside this stage in both versions of a comparison.
            let received = state.clone();
            let (_, push) = measure(|| {
                if interval == 0 {
                    let shots = LivePaths {
                        paths: paths.paths().copied().collect(),
                    };
                    timeline.reset(&received, tick, now, &shots);
                } else {
                    timeline.push(received, tick, Vec::new(), entries).unwrap();
                    timeline.arrive(now);
                }
            });
            let (_, read) = measure(|| {
                for frame in 0..3 {
                    timeline.read(
                        now + frame as f64 * 1000.0 / 60.0,
                        40.0,
                        1.0 / 60.0,
                        &mut display,
                    );
                }
            });
            let (_, effect) = measure(|| effects.update(&state, 1.0, 0.05, now / 1000.0));
            let (hud, hud_sample) = measure(|| {
                #[derive(Serialize)]
                struct HudParts<H, S> {
                    human: H,
                    scoreboard: S,
                }
                serde_json::to_string(&HudParts {
                    human: human_json(state.viewer().expect("viewer"), state.elapsed),
                    scoreboard: Scoreboard::Rendered(&state.tanks),
                })
                .unwrap()
            });
            if interval >= 400 {
                for (stage, sample) in [
                    ("simulation", step),
                    ("capture", capture),
                    ("diff", diff),
                    ("renderState", render),
                    ("timelinePush", push),
                    ("timelineRead3", read),
                    ("effects", effect),
                    ("hud", hud_sample),
                ] {
                    samples.entry(stage).or_default().push(sample);
                }
                hash(&mut hashes[0], &wire);
                hash(&mut hashes[1], &serde_json::to_vec(&display).unwrap());
                // Object key order is not a HUD API; normalize before comparing.
                let hud: serde_json::Value = serde_json::from_str(&hud).unwrap();
                hash(&mut hashes[2], &serde_json::to_vec(&hud).unwrap());
            }
        }
        println!("{}: wire/render/HUD {hashes:x?}", map.as_str());
        reports
            .push(serde_json::json!({"map": map.as_str(), "hashes": hashes, "samples": samples}));
    }
    std::fs::write(
        output,
        serde_json::to_string(&serde_json::json!({
            "seed": seed, "warmupTicks": 1200, "measuredTicks": 1200,
            "intervals": 400, "results": reports,
        }))
        .unwrap(),
    )
    .expect("write report");
}
