pub mod physics;
#[cfg(target_arch = "wasm32")]
mod renderer;

pub mod feature_probe;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn physics_probe(scale: f32) -> Vec<u32> {
    feature_probe::run(scale).to_vec()
}
