//! Heap allocations and time of the simulation's destruction paths, natively:
//! `cargo run -p sloppy-core --release --example sim_allocations`.
//! Counts every allocation through a counting global allocator over: what the
//! structures' standing rules cost (`Structure::unsupported` on every structure of the
//! village and the Stress Grid for a fixed series of parts gone), a cottage collapse
//! (each village cottage broken, blasted and levelled, then a few seconds of the collapse
//! played out) and a minute each of the village, the Stress Grid and the Scrap Yard. Each prints a fingerprint of its results,
//! so two builds can show they played the same. Manual evidence, not a CI gate.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use sloppy_core::sim::extra_levels::extra_level;
use sloppy_core::sim::house_structure::PartKind;
use sloppy_core::sim::level_rules::single_player_rules;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::math::Point3;
use sloppy_core::sim::structure::PartSet;
use sloppy_core::sim::structure_physics::HE_BLAST_RADIUS;
use sloppy_core::sim::{CoverKind, MatchPhase, Simulation, SimulationSetup, VehicleCommand};

struct Counting;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// What `work` returns, with the allocations, bytes and milliseconds it took.
fn measure<T>(work: impl FnOnce() -> T) -> (T, u64, u64, f64) {
    let (count, bytes) = (
        ALLOCATIONS.load(Ordering::Relaxed),
        BYTES.load(Ordering::Relaxed),
    );
    let start = Instant::now();
    let result = work();
    let ms = start.elapsed().as_secs_f64() * 1e3;
    (
        result,
        ALLOCATIONS.load(Ordering::Relaxed) - count,
        BYTES.load(Ordering::Relaxed) - bytes,
        ms,
    )
}

fn report(what: &str, allocations: u64, bytes: u64, ms: f64, fingerprint: u64) {
    println!(
        "{what}: {allocations} allocations, {:.1} KB, {ms:.1} ms, fingerprint {fingerprint:016x}",
        bytes as f64 / 1024.0
    );
}

/// Where a match is: the rng, every body's place and every structure's parts gone.
fn fingerprint(simulation: &Simulation) -> u64 {
    let mut hasher = DefaultHasher::new();
    simulation.rng.state.to_bits().hash(&mut hasher);
    simulation.elapsed.to_bits().hash(&mut hasher);
    simulation.fragments.len().hash(&mut hasher);
    simulation.destroyed.hash(&mut hasher);
    for (_, body) in simulation.world.bodies.iter() {
        let p = body.translation();
        let r = body.rotation();
        for value in [p.x, p.y, p.z, r.x, r.y, r.z, r.w] {
            value.to_bits().hash(&mut hasher);
        }
    }
    for cover in &simulation.covers {
        cover.alive.hash(&mut hasher);
        if let Some(state) = &cover.structure {
            state.gone.hash(&mut hasher);
        }
    }
    hasher.finish()
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

/// `Structure::unsupported` on every structure of `map` for each part gone alone, then
/// growing runs of every third, fifth and seventh part.
fn standing_rules(name: &str, map: MapId) {
    let simulation = Simulation::new(731.0, setup(map));
    let layouts: Vec<_> = simulation
        .covers
        .iter()
        .filter_map(|cover| cover.structure.as_ref().map(|state| state.layout.clone()))
        .collect();
    let (hash, allocations, bytes, ms) = measure(|| {
        let mut hasher = DefaultHasher::new();
        for layout in &layouts {
            let count = layout.parts.len();
            for gone in 0..count {
                layout
                    .unsupported(&PartSet::from_indices([gone]))
                    .hash(&mut hasher);
            }
            for stride in [3, 5, 7] {
                for end in (stride..count).step_by(count / 6 + 1) {
                    let gone = PartSet::from_indices((0..end).step_by(stride));
                    layout.unsupported(&gone).hash(&mut hasher);
                }
            }
        }
        hasher.finish()
    });
    report(
        &format!(
            "standing rules on the {name}'s {} structures",
            layouts.len()
        ),
        allocations,
        bytes,
        ms,
        hash,
    );
}

fn cottage_collapse() {
    let mut s = Simulation::with_seed(731.0);
    s.start();
    let houses: Vec<usize> = (0..s.covers.len())
        .filter(|&i| s.covers[i].kind == CoverKind::House)
        .collect();
    let (_, allocations, bytes, ms) = measure(|| {
        for &house in &houses {
            let layout = s.covers[house].structure.as_ref().unwrap().layout.clone();
            let panel = (0..layout.parts.len())
                .find(|&i| {
                    layout.parts[i].house().kind == PartKind::Panel && layout.parts[i].grounded
                })
                .unwrap();
            s.break_structure(
                house,
                PartSet::from_indices([panel]),
                PartSet::EMPTY,
                None,
                None,
            );
            let c = &s.covers[house];
            let inside = layout.to_world(layout.parts[panel].middle() * 0.6);
            let point = Point3::new(c.x + inside.x, 1.0, c.z + inside.z);
            s.structure_blast(point, HE_BLAST_RADIUS, 40.0, None);
            let footing = PartSet::from_indices((0..layout.parts.len()).filter(|&i| {
                layout.parts[i].grounded && layout.parts[i].house().kind != PartKind::Chimney
            }));
            s.break_structure(house, footing, PartSet::EMPTY, None, None);
        }
    });
    report(
        &format!("break, blast and level {} cottages", houses.len()),
        allocations,
        bytes,
        ms,
        fingerprint(&s),
    );
    let (_, allocations, bytes, ms) = measure(|| {
        for _ in 0..300 {
            s.step(VehicleCommand::idle(), true);
            s.events.clear();
        }
    });
    report(
        "the next 300 ticks of their collapse",
        allocations,
        bytes,
        ms,
        fingerprint(&s),
    );
}

/// A minute of `map`'s autoplay after ten seconds of warm-up.
fn match_minute(name: &str, map: MapId) {
    let mut s = Simulation::new(79.0, setup(map));
    s.start();
    for _ in 0..600 {
        s.step(VehicleCommand::idle(), true);
        s.events.clear();
    }
    let (ticks, allocations, bytes, ms) = measure(|| {
        let mut ticks = 0;
        for _ in 0..3600 {
            if s.match_state.phase != MatchPhase::Playing {
                break;
            }
            s.step(VehicleCommand::idle(), true);
            s.events.clear();
            ticks += 1;
        }
        ticks
    });
    report(
        &format!("{name}, {ticks} ticks after 600 warm-up"),
        allocations,
        bytes,
        ms,
        fingerprint(&s),
    );
}

fn main() {
    standing_rules("village", MapId::Village);
    standing_rules("Stress Grid", MapId::StressTest);
    cottage_collapse();
    match_minute("village", MapId::Village);
    match_minute("Stress Grid", MapId::StressTest);
    match_minute("Scrap Yard", MapId::Superstress);
}
