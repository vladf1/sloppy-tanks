//! The simulation benchmark's seeded autoplay in Wasm, where the page runs it: build it
//! for wasm32 and time builds against each other with `scripts/benchmarks/sim-wasm.mjs`.
//! `prepare(map, seed)` builds a match (0 village, 1 stress-test, 2 harbor, 3
//! superstress) and plays the 600 warm-up ticks; `steps(n)` plays n more and returns how
//! many it played; `fingerprint()` hashes every body's pose, so two builds can show they
//! played the same. Natively, `cargo run --release -p sloppy-core --example sim_wasm --
//! <map> <seed> <ticks>` prints the fingerprint and the mean tick time (native and Wasm
//! fingerprints can differ on macOS, whose system math library rounds differently).
//! Manual evidence, not a CI gate.
//!
//! ```sh
//! RUSTFLAGS="-C target-feature=+simd128 -C link-arg=--export=prepare -C link-arg=--export=steps -C link-arg=--export=fingerprint" \
//!   cargo build --release --target wasm32-unknown-unknown -p sloppy-core --example sim_wasm --target-dir target/sim-wasm
//! ```

use std::cell::RefCell;

use sloppy_core::sim::extra_levels::extra_level;
use sloppy_core::sim::level_rules::single_player_rules;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::{MatchPhase, Simulation, SimulationSetup, VehicleCommand};

const MAPS: [&str; 4] = ["village", "stress-test", "harbor", "superstress"];
const WARMUP_TICKS: u32 = 600;

thread_local! {
    static SIMULATION: RefCell<Option<Simulation>> = const { RefCell::new(None) };
}

fn new_match(map: &str, seed: f64) -> Simulation {
    let id = MapId::parse(map).expect("known map id");
    let setup = SimulationSetup {
        map_mode: Some(id),
        ..SimulationSetup::default()
    }
    .merged(extra_level(id).map(single_player_rules).unwrap_or_default());
    let mut simulation = Simulation::new(seed, setup);
    simulation.start();
    simulation
}

fn play(simulation: &mut Simulation, ticks: u32) -> u32 {
    for tick in 0..ticks {
        if simulation.match_state.phase != MatchPhase::Playing {
            return tick;
        }
        simulation.step(VehicleCommand::idle(), true);
        simulation.events.clear();
    }
    ticks
}

fn hash(simulation: &Simulation) -> u64 {
    // FNV-1a over every body's translation and rotation bits.
    let mut h: u64 = 0xcbf29ce484222325;
    let mut mix = |bits: u32| {
        h ^= bits as u64;
        h = h.wrapping_mul(0x100000001b3);
    };
    for (_, body) in simulation.world.bodies.iter() {
        let t = body.translation();
        let r = body.rotation();
        for v in [t.x, t.y, t.z, r.x, r.y, r.z, r.w] {
            mix(v.to_bits());
        }
    }
    mix(simulation.world.bodies.len() as u32);
    h
}

#[unsafe(no_mangle)]
pub extern "C" fn prepare(map: u32, seed: f64) {
    let mut simulation = new_match(MAPS[map as usize], seed);
    play(&mut simulation, WARMUP_TICKS);
    SIMULATION.with(|cell| *cell.borrow_mut() = Some(simulation));
}

#[unsafe(no_mangle)]
pub extern "C" fn steps(n: u32) -> u32 {
    SIMULATION.with(|cell| play(cell.borrow_mut().as_mut().unwrap(), n))
}

#[unsafe(no_mangle)]
pub extern "C" fn fingerprint() -> u64 {
    SIMULATION.with(|cell| hash(cell.borrow().as_ref().unwrap()))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let map = args.get(1).map_or("village", String::as_str);
    let seed: f64 = args.get(2).and_then(|t| t.parse().ok()).unwrap_or(7.0);
    let ticks: u32 = args.get(3).and_then(|t| t.parse().ok()).unwrap_or(3600);
    let index = MAPS.iter().position(|m| *m == map).expect("map") as u32;
    prepare(index, seed);
    let start = std::time::Instant::now();
    let played = steps(ticks);
    let ms = start.elapsed().as_secs_f64() * 1e3;
    println!(
        "{map} seed {seed}: {played} ticks, {:.4} ms/tick, fingerprint {:016x}",
        ms / played as f64,
        fingerprint()
    );
}
