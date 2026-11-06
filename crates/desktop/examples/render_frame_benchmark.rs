//! Manual native evidence of the renderer's per-frame CPU, not a CI gate or a browser
//! timing claim: a seeded autoplay match on each map drawn by the desktop engine (wgpu
//! on Metal or Direct3D 12) into a hidden window, timing and counting the heap
//! allocations of the presentation update (`Presentation::update`, the effects
//! included) and of `Renderer::render` (culls, draw lists, uploads and the native
//! encoding of the passes) frame by frame. Every sample is kept.
//!
//! A hash of what each measured frame handed the GPU (`Renderer::frame_digest`, and the
//! effect pools' records) is printed per map, so before/after runs prove they drew the
//! same frames. With `SLOPPY_ALLOCATION_SITES=n` it also lists where the measured
//! frames allocate, from a backtrace of every `n`th allocation (build with line tables
//! for names).
//!
//! ```sh
//! CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
//! CARGO_PROFILE_RELEASE_STRIP=false CARGO_PROFILE_RELEASE_DEBUG=line-tables-only \
//! cargo run --release -p sloppy-desktop --example render_frame_benchmark -- out.json [seed] [maps]
//! ```
//!
//! `maps` is a comma-separated list of map ids (default village, harbor, quarry,
//! stress-test and superstress).

use std::alloc::{GlobalAlloc, Layout, System};
use std::backtrace::Backtrace;
use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};
use std::hash::{DefaultHasher, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use serde::Serialize;
use sloppy_core::sim::data::STEP;
use sloppy_core::sim::extra_levels::extra_level;
use sloppy_core::sim::level_rules::{single_player_rules, standard_rules};
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::{RenderState, Simulation, SimulationSetup, VehicleCommand};
use sloppy_render::gpu::{RenderTarget, Renderer, set_asset_files};
use sloppy_render::presentation::Presentation;
use winit::dpi::PhysicalSize;
use winit::event_loop::EventLoop;
use winit::window::Window;

const WIDTH: u32 = 1600;
const HEIGHT: u32 = 1000;
/// Frames drawn before measuring (the round's first seconds), then measured.
const WARM_UP_FRAMES: usize = 600;
const MEASURED_FRAMES: usize = 1200;

struct CountingAllocator;
static COUNTING: AtomicBool = AtomicBool::new(false);
static TRACING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
/// Every this many allocations is traced (`SLOPPY_ALLOCATION_SITES`).
static TRACE_EVERY: AtomicU64 = AtomicU64::new(1);
static TRACE_COUNT: AtomicU64 = AtomicU64::new(0);
static TRACES: Mutex<Vec<(&'static str, Backtrace)>> = Mutex::new(Vec::new());
static STAGE: Mutex<&'static str> = Mutex::new("");

thread_local! {
    /// Set while a trace is taken, whose own allocations are not traced.
    static IN_TRACE: Cell<bool> = const { Cell::new(false) };
}

fn record(bytes: usize) {
    if !COUNTING.load(Relaxed) {
        return;
    }
    ALLOCATIONS.fetch_add(1, Relaxed);
    BYTES.fetch_add(bytes as u64, Relaxed);
    if TRACING.load(Relaxed)
        && !IN_TRACE.with(Cell::get)
        && TRACE_COUNT
            .fetch_add(1, Relaxed)
            .is_multiple_of(TRACE_EVERY.load(Relaxed))
    {
        IN_TRACE.with(|flag| flag.set(true));
        let stage = *STAGE.lock().unwrap();
        let trace = Backtrace::force_capture();
        TRACES.lock().unwrap().push((stage, trace));
        IN_TRACE.with(|flag| flag.set(false));
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

fn measure<T>(stage: &'static str, run: impl FnOnce() -> T) -> (T, Sample) {
    *STAGE.lock().unwrap() = stage;
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

/// The game's runtime files, read from `public/` once and kept.
fn asset_file(path: &str) -> Option<&'static [u8]> {
    static FILES: OnceLock<Mutex<HashMap<String, &'static [u8]>>> = OnceLock::new();
    let files = FILES.get_or_init(Default::default);
    let mut files = files.lock().unwrap();
    if let Some(bytes) = files.get(path) {
        return Some(bytes);
    }
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../public")
        .join(path);
    let bytes: &'static [u8] = Box::leak(std::fs::read(file).ok()?.into_boxed_slice());
    files.insert(path.to_owned(), bytes);
    Some(bytes)
}

/// The arena of `map` built, prepared and drawn twice, as the game does before GO.
fn build_arena(view: &mut Presentation, state: &RenderState) {
    view.begin_build(state);
    while !view.build_step(state) {}
    loop {
        let status = view.prepare_step(64).expect("prepare");
        if status.ready {
            break;
        }
        if status.gpu_pending {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    view.finish_prepare(state).expect("first frames");
}

fn wait_for_gpu(renderer: &mut Renderer) {
    renderer.await_gpu();
    while !renderer.gpu_idle() {
        std::thread::yield_now();
    }
}

/// The few frames of the innermost `sloppy` calls that allocated, per stage.
fn report_sites() {
    let traces = std::mem::take(&mut *TRACES.lock().unwrap());
    let mut sites: BTreeMap<(&str, String), usize> = BTreeMap::new();
    for (stage, trace) in &traces {
        let text = trace.to_string();
        let frames: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|line| {
                !line.starts_with("at ")
                    && line.contains("sloppy_")
                    && !line.contains("render_frame_benchmark")
            })
            .take(6)
            .collect();
        *sites.entry((stage, frames.join(" <- "))).or_default() += 1;
    }
    let mut sites: Vec<_> = sites.into_iter().collect();
    sites.sort_by(|a, b| b.1.cmp(&a.1));
    println!("allocation sites ({} allocations traced):", traces.len());
    for ((stage, site), count) in sites {
        println!("{count:6} {stage}: {site}");
    }
}

fn main() {
    let output = std::env::args().nth(1).expect("output.json argument");
    let seed: f64 = std::env::args()
        .nth(2)
        .map_or(4242.0, |value| value.parse().expect("numeric seed"));
    let maps: Vec<MapId> = std::env::args().nth(3).map_or_else(
        || {
            vec![
                MapId::Village,
                MapId::Harbor,
                MapId::Quarry,
                MapId::StressTest,
                MapId::Superstress,
            ]
        },
        |list| {
            list.split(',')
                .map(|id| MapId::parse(id).unwrap_or_else(|| panic!("unknown map {id}")))
                .collect()
        },
    );
    let trace_every = std::env::var("SLOPPY_ALLOCATION_SITES")
        .ok()
        .map(|every| every.parse::<u64>().expect("trace every n allocations"));
    let trace_sites = trace_every.is_some();
    TRACE_EVERY.store(trace_every.unwrap_or(1).max(1), Relaxed);
    set_asset_files(asset_file);
    let event_loop = EventLoop::new().expect("event loop");
    #[allow(deprecated)]
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(WIDTH, HEIGHT)),
            )
            .expect("hidden window"),
    );
    let target = RenderTarget {
        window: sloppy_render::wgpu::SurfaceTarget::from(window.clone()),
        width: WIDTH,
        height: HEIGHT,
    };
    let renderer =
        pollster::block_on(Renderer::new(target, String::new(), true)).expect("renderer");
    let mut view = Presentation::new(renderer, seed.to_bits());
    view.resize(WIDTH, HEIGHT);
    let mut reports = Vec::new();
    for map in maps {
        let level = extra_level(map).map_or_else(standard_rules, single_player_rules);
        let setup = SimulationSetup {
            map_mode: Some(map),
            round: Some(3),
            ..SimulationSetup::default()
        }
        .merged(level);
        let mut sim = Simulation::new(seed, setup);
        let mut state = sim.render_state(None);
        build_arena(&mut view, &state);
        sim.start();
        let mut samples: BTreeMap<&str, Vec<Sample>> = BTreeMap::new();
        let mut frame_hash = DefaultHasher::new();
        let mut pool_hash = DefaultHasher::new();
        for frame in 0..WARM_UP_FRAMES + MEASURED_FRAMES {
            let measured = frame >= WARM_UP_FRAMES;
            sim.set_wreck_view(Some(view.wreck_view()));
            sim.step(VehicleCommand::default(), true);
            sim.fill_render_state(&mut state, None);
            for event in std::mem::take(&mut sim.events) {
                view.event(&event, false);
            }
            TRACING.store(trace_sites && measured, Relaxed);
            let ((), update) = measure("update", || view.update(&state, 1.0, STEP, false));
            let time = view.time as f32;
            let (result, render) = measure("render", || view.renderer.render(time));
            TRACING.store(false, Relaxed);
            result.expect("frame");
            wait_for_gpu(&mut view.renderer);
            if measured {
                samples.entry("update").or_default().push(update);
                samples.entry("render").or_default().push(render);
                frame_hash.write_u64(view.renderer.frame_digest());
                view.effects.systems.for_each_pool(|pool| {
                    pool_hash.write_usize(pool.len());
                    for record in pool.records() {
                        let rows = record.world_rows.iter().chain([&record.tint, &record.data]);
                        for value in rows.flatten() {
                            pool_hash.write_u32(value.to_bits());
                        }
                    }
                });
            }
        }
        let (frame_hash, pool_hash) = (frame_hash.finish(), pool_hash.finish());
        println!(
            "{}: frames {frame_hash:016x} pools {pool_hash:016x}",
            map.as_str()
        );
        for (stage, list) in &samples {
            let count = list.len() as f64;
            let allocations = list.iter().map(|s| s.allocations).sum::<u64>() as f64 / count;
            let bytes = list.iter().map(|s| s.bytes).sum::<u64>() as f64 / count;
            let mut micros: Vec<f64> = list.iter().map(|s| s.micros).collect();
            micros.sort_by(f64::total_cmp);
            let mean = micros.iter().sum::<f64>() / count;
            println!(
                "  {stage:7} {allocations:8.1} allocations/frame {bytes:10.0} B/frame  µs mean {mean:7.1} median {:7.1} p90 {:7.1} max {:7.1}",
                micros[micros.len() / 2],
                micros[micros.len() * 9 / 10],
                micros[micros.len() - 1],
            );
        }
        reports.push(serde_json::json!({
            "map": map.as_str(),
            "frameHash": format!("{frame_hash:016x}"),
            "poolHash": format!("{pool_hash:016x}"),
            "samples": samples,
        }));
    }
    if trace_sites {
        report_sites();
    }
    std::fs::write(
        output,
        serde_json::to_string(&serde_json::json!({
            "seed": seed, "width": WIDTH, "height": HEIGHT,
            "warmUpFrames": WARM_UP_FRAMES, "measuredFrames": MEASURED_FRAMES,
            "results": reports,
        }))
        .unwrap(),
    )
    .expect("write report");
}
