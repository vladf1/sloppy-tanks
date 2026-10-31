//! `model_build_benchmark`'s workloads for Wasm, timed by its caller: the page runs
//! the models in Wasm at opt-level z with Rust's own libm, where their costs differ
//! from a native build's. `build(workload)` builds one workload once (0 village,
//! 1 harbor, 2 quarry, 3 vehicles and their wrecks, 5 the village's covers after
//! setting up its simulation, which 4 does alone, to subtract); every
//! instance starts with empty caches, so one fresh instance per sample times a cold
//! build. Build it with
//!
//! ```sh
//! RUSTFLAGS="-C link-arg=--export=build" cargo build --release \
//!   --target wasm32-unknown-unknown -p sloppy-core --example model_build_wasm \
//!   --target-dir target/wasm-bench
//! ```
//!
//! and time it from Node, which needs no imports for it:
//!
//! ```js
//! const module = new WebAssembly.Module(fs.readFileSync(path));
//! const instance = new WebAssembly.Instance(module, {});
//! const start = performance.now();
//! instance.exports.build(0);
//! console.log(performance.now() - start);
//! ```
//!
//! `pin(workload)` returns the `model_pins` hash of the same builds in Wasm (0-2
//! the themes' scenery, 3 the vehicles and wrecks, 5 the village's covers at every
//! damage stage), to compare a change's Wasm output with its parent's (export it
//! too: `--export=build --export=pin`).

use sloppy_core::models::{
    CoverShape, MapTheme, TreeDetail, WreckPart, batch, build_scenery, cover_model, tank_model,
    wreck_model,
};
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::types::{Team, VehicleKind};
use sloppy_core::sim::{RenderState, Simulation};

#[unsafe(no_mangle)]
pub extern "C" fn build(workload: u32) {
    match workload {
        0 => drop(std::hint::black_box(build_scenery(MapTheme::Village))),
        1 => drop(std::hint::black_box(build_scenery(MapTheme::Harbor))),
        2 => drop(std::hint::black_box(build_scenery(MapTheme::Quarry))),
        3 => {
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
        }
        _ => {
            let mut simulation = Simulation::with_seed(417.0);
            simulation.map_mode = MapId::Village;
            simulation.reset(None);
            let mut view = RenderState::default();
            simulation.fill_render_state(&mut view, None);
            if workload == 4 {
                std::hint::black_box(&view);
                return;
            }
            for cover in &view.covers {
                let mut root = cover_model(&CoverShape::from(cover), TreeDetail::Full, 0);
                batch(&mut root);
                std::hint::black_box(&root);
            }
        }
    }
}

#[path = "../tests/integration/model_pin_hash.rs"]
mod model_pin_hash;

#[unsafe(no_mangle)]
pub extern "C" fn pin(workload: u32) -> u64 {
    match workload {
        0 => model_pin_hash::scenery_pin(MapTheme::Village),
        1 => model_pin_hash::scenery_pin(MapTheme::Harbor),
        2 => model_pin_hash::scenery_pin(MapTheme::Quarry),
        3 => model_pin_hash::vehicles_pin(),
        _ => model_pin_hash::covers_pin(MapId::Village),
    }
}

fn main() {}
