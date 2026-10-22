//! Bake Pine Village's ground plan (`models::bake_village_ground`) as raw RGBA8 rows
//! to the given file: `cargo run -p sloppy-core --release --example village_ground -- <out>`.
//! `scripts/generate-village-ground.ts` runs it and encodes the WebP.

use sloppy_core::models::{GROUND_PLAN_SIZE, bake_village_ground};

fn main() {
    let out = std::env::args().nth(1).expect("output path");
    let pixels = bake_village_ground();
    assert_eq!(pixels.len(), GROUND_PLAN_SIZE * GROUND_PLAN_SIZE * 2 * 4);
    std::fs::write(&out, pixels).expect("write the plan");
}
