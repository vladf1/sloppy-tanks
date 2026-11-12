//! What building an arena costs before its first frame, natively: per workload, the
//! time, heap allocations, bytes allocated, the peak of live heap bytes above where it
//! began, and a hash of what it built, so two builds can show they produce the same.
//! Model caches live for the process, so each sample is a fresh process; hashing runs
//! outside the time and the counts.
//!
//! `cargo run -p sloppy-core --release --example arena_load -- [runs] [workload...]`
//! (runs defaults to 9; workloads: sim-<map> builds a seeded `Simulation` on a map,
//! reset-<map> resets one, nav-<map> rebuilds its navigation grid 20 times,
//! scenery-<theme> builds a theme's scenery, covers-<map> a map's cover models batched
//! while its simulation stands, samples-<map> each structure kind's broken sample and
//! debris, vehicles the vehicles with their wrecks, encode-<theme> and decode-<theme>
//! a theme's scenery in the geometry cache's form; maps village harbor quarry
//! stress-test superstress, themes village harbor quarry). `--repeat <workload> <n>`
//! runs one workload n times in one process, for a sampling profiler. Manual evidence,
//! not a CI gate.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use sloppy_core::models::{
    CoverShape, MapTheme, TreeDetail, WreckPart, batch, build_scenery, cover_model,
    structure_debris, tank_model, wreck_model,
};
use sloppy_core::scene_codec::{decode_bytes, encode};
use sloppy_core::sim::extra_levels::extra_level;
use sloppy_core::sim::level_rules::single_player_rules;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::math::Vec2;
use sloppy_core::sim::navigation::GRID_SIZE;
use sloppy_core::sim::render_state::RenderCover;
use sloppy_core::sim::structure::PartSet;
use sloppy_core::sim::structure_physics::structural;
use sloppy_core::sim::tower_structure::tower_structure;
use sloppy_core::sim::types::{Team, VehicleKind};
use sloppy_core::sim::{CoverKind, RenderState, Simulation, SimulationSetup};

#[path = "../tests/integration/model_pin_hash.rs"]
#[allow(dead_code)]
mod model_pin_hash;

struct Counting;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE: AtomicU64 = AtomicU64::new(0);
static PEAK: AtomicU64 = AtomicU64::new(0);
/// While set, allocations are not counted (live bytes and their peak still are).
static PAUSED: AtomicBool = AtomicBool::new(false);
static UNTIMED_NS: AtomicU64 = AtomicU64::new(0);

fn count(size: usize) {
    if !PAUSED.load(Ordering::Relaxed) {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(size as u64, Ordering::Relaxed);
    }
}

fn grow(size: usize) {
    let live = LIVE.fetch_add(size as u64, Ordering::Relaxed) + size as u64;
    PEAK.fetch_max(live, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        grow(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(new_size);
        LIVE.fetch_sub(layout.size() as u64, Ordering::Relaxed);
        grow(new_size);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Run `work` outside a sample's time and allocation counts: its hashing, and dropping
/// what only the hash reads.
fn untimed<T>(work: impl FnOnce() -> T) -> T {
    PAUSED.store(true, Ordering::Relaxed);
    let start = Instant::now();
    let result = work();
    UNTIMED_NS.fetch_add(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
    PAUSED.store(false, Ordering::Relaxed);
    result
}

fn setup(map: MapId) -> SimulationSetup {
    SimulationSetup {
        map_mode: Some(map),
        ..SimulationSetup::default()
    }
    .merged(
        extra_level(map)
            .map(single_player_rules)
            .unwrap_or_default(),
    )
}

/// Where a fresh world is: the rng, every body and collider, the covers with their
/// structures' parts, and the grid.
fn fingerprint(simulation: &Simulation) -> u64 {
    let mut hasher = DefaultHasher::new();
    simulation.rng.state.to_bits().hash(&mut hasher);
    simulation.world.bodies.len().hash(&mut hasher);
    simulation.world.colliders.len().hash(&mut hasher);
    for (_, body) in simulation.world.bodies.iter() {
        let p = body.translation();
        let r = body.rotation();
        for value in [p.x, p.y, p.z, r.x, r.y, r.z, r.w, body.mass()] {
            value.to_bits().hash(&mut hasher);
        }
        body.colliders().len().hash(&mut hasher);
    }
    for (_, collider) in simulation.world.colliders.iter() {
        let p = collider.position();
        for value in [
            p.translation.x,
            p.translation.y,
            p.translation.z,
            p.rotation.x,
            p.rotation.y,
            p.rotation.z,
            p.rotation.w,
            collider.mass(),
        ] {
            value.to_bits().hash(&mut hasher);
        }
        let aabb = collider.compute_aabb();
        for value in [aabb.mins, aabb.maxs] {
            for c in [value.x, value.y, value.z] {
                c.to_bits().hash(&mut hasher);
            }
        }
        collider.user_data.hash(&mut hasher);
    }
    for cover in &simulation.covers {
        cover.id.hash(&mut hasher);
        cover.x.to_bits().hash(&mut hasher);
        cover.z.to_bits().hash(&mut hasher);
        cover.alive.hash(&mut hasher);
        if let Some(state) = &cover.structure {
            for part in &state.layout.parts {
                for value in [part.mass, part.min.x, part.min.y, part.max.x, part.max.z] {
                    value.to_bits().hash(&mut hasher);
                }
                part.shapes.len().hash(&mut hasher);
            }
            state.layout.pieces.len().hash(&mut hasher);
        }
    }
    for tank in &simulation.tanks {
        tank.id.hash(&mut hasher);
    }
    let cell = 120.0 / GRID_SIZE as f64;
    for z in 0..GRID_SIZE {
        for x in 0..GRID_SIZE {
            let p = Vec2::new(
                (x as f64 + 0.5) * cell - 60.0,
                (z as f64 + 0.5) * cell - 60.0,
            );
            simulation.nav.is_blocked(p).hash(&mut hasher);
        }
    }
    simulation.nav.version.hash(&mut hasher);
    hasher.finish()
}

fn map(name: &str) -> MapId {
    MapId::parse(name).unwrap_or_else(|| panic!("unknown map {name}"))
}

fn theme(name: &str) -> MapTheme {
    match name {
        "village" => MapTheme::Village,
        "harbor" => MapTheme::Harbor,
        "quarry" => MapTheme::Quarry,
        other => panic!("unknown theme {other}"),
    }
}

/// A map's simulation and its scene: a page builds a round's models while its
/// simulation stands.
fn render_state(map: MapId) -> (Simulation, RenderState) {
    let simulation = Simulation::new(417.0, setup(map));
    let mut view = RenderState::default();
    simulation.fill_render_state(&mut view, None);
    (simulation, view)
}

/// The work of one workload, set up but not run, returning its hash.
fn workload(name: &str) -> Box<dyn FnOnce() -> u64> {
    let (kind, arg) = name.split_once('-').unwrap_or((name, ""));
    match kind {
        "sim" => {
            let map = map(arg);
            Box::new(move || {
                let simulation = Simulation::new(731.0, setup(map));
                untimed(move || fingerprint(&simulation))
            })
        }
        "reset" => {
            let mut simulation = Simulation::new(731.0, setup(map(arg)));
            Box::new(move || {
                simulation.reset(None);
                untimed(move || fingerprint(&simulation))
            })
        }
        "nav" => {
            let mut simulation = Simulation::new(731.0, setup(map(arg)));
            Box::new(move || {
                for _ in 0..20 {
                    simulation.nav.rebuild(&simulation.covers, None);
                }
                untimed(move || fingerprint(&simulation))
            })
        }
        "scenery" => {
            let theme = theme(arg);
            Box::new(move || {
                let scenery = build_scenery(theme);
                untimed(move || model_pin_hash::node_pin(scenery.root()))
            })
        }
        "encode" => {
            let scenery = build_scenery(theme(arg));
            Box::new(move || {
                let bytes = encode(scenery.root(), "arena_load").to_bytes();
                untimed(move || {
                    drop(scenery);
                    let mut hasher = DefaultHasher::new();
                    bytes.hash(&mut hasher);
                    hasher.finish()
                })
            })
        }
        // A cached visit's read-back, in a process that built nothing: the file is
        // written by another process.
        "decode" => {
            let path = std::env::temp_dir().join(format!("arena_load-{arg}.scenery"));
            let status = Command::new(std::env::current_exe().expect("own path"))
                .args(["--encode", arg])
                .arg(&path)
                .status()
                .expect("encode the scenery");
            assert!(status.success(), "encoding {arg} failed");
            let bytes = std::fs::read(&path).expect("the encoded scenery");
            Box::new(move || {
                let root = decode_bytes(&bytes, "arena_load").expect("reads back");
                untimed(move || model_pin_hash::node_pin(&root))
            })
        }
        "covers" => {
            let standing = render_state(map(arg));
            Box::new(move || {
                let mut hasher = DefaultHasher::new();
                for cover in &standing.1.covers {
                    let mut root = cover_model(&CoverShape::from(cover), TreeDetail::Full);
                    batch(&mut root);
                    untimed(|| model_pin_hash::node_pin(&root).hash(&mut hasher));
                }
                untimed(move || {
                    drop(standing);
                    hasher.finish()
                })
            })
        }
        "samples" => {
            let standing = render_state(map(arg));
            Box::new(move || {
                let covers = &standing.1.covers;
                let mut hasher = DefaultHasher::new();
                let mut seen = std::collections::HashSet::new();
                for cover in covers {
                    if !structural(cover.kind) || !seen.insert(cover.kind) {
                        continue;
                    }
                    let cover = covers
                        .iter()
                        .filter(|other| other.kind == cover.kind)
                        .min_by(|a, b| (a.w * a.d * a.h).total_cmp(&(b.w * b.d * b.h)))
                        .unwrap_or(cover);
                    let broken = match cover.kind {
                        CoverKind::House => PartSet::from_indices(5..12),
                        CoverKind::Tower => tower_structure().all(),
                        _ => PartSet::from_indices([0, 1]),
                    };
                    let damaged = RenderCover { broken, ..*cover };
                    if cover.kind != CoverKind::Tower {
                        let mut root = cover_model(&CoverShape::from(&damaged), TreeDetail::Full);
                        batch(&mut root);
                        untimed(|| model_pin_hash::node_pin(&root).hash(&mut hasher));
                    }
                    let debris = structure_debris(&CoverShape::from(cover), &broken);
                    untimed(|| model_pin_hash::node_pin(&debris).hash(&mut hasher));
                }
                untimed(move || {
                    drop(standing);
                    hasher.finish()
                })
            })
        }
        "vehicles" => Box::new(|| {
            let mut hasher = DefaultHasher::new();
            for kind in VehicleKind::ALL {
                for team in [Team::Blue, Team::Red] {
                    let tank = tank_model(kind, team);
                    untimed(|| model_pin_hash::node_pin(&tank).hash(&mut hasher));
                    for part in [
                        WreckPart::Intact,
                        WreckPart::Hull,
                        WreckPart::Turret,
                        WreckPart::TurretBarrel,
                        WreckPart::Barrel,
                    ] {
                        let wreck = wreck_model(kind, team, part);
                        untimed(|| model_pin_hash::node_pin(&wreck).hash(&mut hasher));
                    }
                }
            }
            hasher.finish()
        }),
        _ => panic!("unknown workload {name}"),
    }
}

/// Run one workload once: milliseconds, allocations, bytes, peak bytes, hash.
fn sample(name: &str) -> (f64, u64, u64, u64, u64) {
    let work = workload(name);
    let (count, bytes) = (
        ALLOCATIONS.load(Ordering::Relaxed),
        BYTES.load(Ordering::Relaxed),
    );
    let base = LIVE.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    UNTIMED_NS.store(0, Ordering::Relaxed);
    let start = Instant::now();
    let hash = work();
    let elapsed = start.elapsed().as_nanos() as u64 - UNTIMED_NS.load(Ordering::Relaxed);
    (
        elapsed as f64 / 1e6,
        ALLOCATIONS.load(Ordering::Relaxed) - count,
        BYTES.load(Ordering::Relaxed) - bytes,
        PEAK.load(Ordering::Relaxed) - base,
        hash,
    )
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [flag, name] = args.as_slice()
        && flag == "--sample"
    {
        let (ms, allocations, bytes, peak, hash) = sample(name);
        println!("{ms} {allocations} {bytes} {peak} {hash}");
        return;
    }
    if let [flag, name, path] = args.as_slice()
        && flag == "--encode"
    {
        let scenery = build_scenery(theme(name));
        std::fs::write(path, encode(scenery.root(), "arena_load").to_bytes()).expect("written");
        return;
    }
    if let [flag, name, count] = args.as_slice()
        && flag == "--repeat"
    {
        let count: usize = count.parse().expect("a count");
        let total: f64 = (0..count).map(|_| sample(name).0).sum();
        println!("{:.2} ms per run", total / count as f64);
        return;
    }
    let runs: usize = args.first().and_then(|a| a.parse().ok()).unwrap_or(9);
    let named: Vec<&str> = args
        .iter()
        .skip_while(|a| a.parse::<usize>().is_ok())
        .map(String::as_str)
        .collect();
    let exe = std::env::current_exe().expect("own path");
    for name in named {
        let mut times = Vec::new();
        let mut rest = None;
        for _ in 0..runs {
            let output = Command::new(&exe)
                .args(["--sample", name])
                .output()
                .expect("run a sample");
            assert!(output.status.success(), "{name} sample failed");
            let text = String::from_utf8_lossy(&output.stdout);
            let fields: Vec<&str> = text.split_whitespace().collect();
            times.push(fields[0].parse::<f64>().expect("milliseconds"));
            let others: Vec<u64> = fields[1..].iter().map(|f| f.parse().unwrap()).collect();
            if let Some(previous) = &rest {
                assert_eq!(previous, &others, "{name} differs between runs");
            }
            rest = Some(others);
        }
        times.sort_by(f64::total_cmp);
        let rest = rest.expect("a run");
        println!(
            "{name:>20}: min {:8.2} ms  median {:8.2} ms  {:>9} allocs  {:>8.1} MB alloc  {:>7.1} MB peak  hash {:016x}",
            times[0],
            times[runs / 2],
            rest[0],
            rest[1] as f64 / 1048576.0,
            rest[2] as f64 / 1048576.0,
            rest[3]
        );
    }
}
