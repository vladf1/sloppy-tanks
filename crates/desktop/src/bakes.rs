//! Generated textures baked off the main thread (`src/game/texture-bake.ts`): the
//! engine hands out a bake (`claim_texture_bake`), its rows split across threads, and
//! the pixels return through `supply_texture` on a later frame.

use std::sync::mpsc::{Receiver, channel};

use sloppy_core::models::effects_scenery::QUARRY_SOIL_TEXTURE;
use sloppy_core::models::{QUARRY_SOIL_SIZE, bake_quarry_ground, sand_accum};

/// A bake running on its threads.
pub struct Bake {
    pub key: String,
    pixels: Receiver<Vec<u8>>,
}

/// Start baking `key` across the machine's cores; `None` for a key the shell cannot
/// bake (the engine then bakes it itself).
pub fn start(key: String) -> Option<Bake> {
    if key != QUARRY_SOIL_TEXTURE {
        return None;
    }
    let (sender, pixels) = channel();
    std::thread::spawn(move || {
        let bands = std::thread::available_parallelism().map_or(4, |cores| cores.get().max(2) - 1);
        let row = |band: usize| QUARRY_SOIL_SIZE * band / bands;
        let parts: Vec<Vec<u8>> = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..bands)
                .map(|band| {
                    scope.spawn(move || bake_quarry_ground(sand_accum(), row(band), row(band + 1)))
                })
                .collect();
            workers
                .into_iter()
                .map(|worker| worker.join().unwrap_or_default())
                .collect()
        });
        let _ = sender.send(parts.concat());
    });
    Some(Bake { key, pixels })
}

impl Bake {
    /// The finished pixels, once.
    pub fn finished(&self) -> Option<Vec<u8>> {
        self.pixels.try_recv().ok()
    }
}
