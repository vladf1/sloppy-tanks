//! Generated textures baked off the main thread (`src/game/texture-bake.ts`): the
//! engine hands out a bake (`claim_texture_bake`), its rows split into bands baked side
//! by side, and the pixels return through `supply_pixels` on a later frame. Natively
//! the bands run on threads; in a page on workers (`src/bake-worker.ts`), each running
//! its own instance of the page's module that only bakes. A bake that cannot start, or
//! fails, goes back to the engine, which bakes it in bands between preparation steps.

#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::{Receiver, channel};

use sloppy_core::models::effects_scenery::QUARRY_SOIL_TEXTURE;
#[cfg(not(target_arch = "wasm32"))]
use sloppy_core::models::{QUARRY_SOIL_SIZE, bake_quarry_ground, sand_accum};

/// A bake's RGBA rows: in a page the JS array its workers filled, so the pixels never
/// enter this instance's heap (which never shrinks).
#[cfg(not(target_arch = "wasm32"))]
pub type Pixels = Vec<u8>;
#[cfg(target_arch = "wasm32")]
pub type Pixels = js_sys::Uint8Array;

/// A bake running on its threads or workers.
pub struct Bake {
    pub key: String,
    #[cfg(not(target_arch = "wasm32"))]
    pixels: Receiver<Vec<u8>>,
    #[cfg(target_arch = "wasm32")]
    workers: page::Workers,
}

/// Start baking `key` across the machine's cores; `None` for a key the shell cannot
/// bake, or in a page that cannot start workers (the engine then bakes it itself).
pub fn start(key: String) -> Option<Bake> {
    if key != QUARRY_SOIL_TEXTURE {
        return None;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (sender, pixels) = channel();
        std::thread::spawn(move || {
            let bands =
                std::thread::available_parallelism().map_or(4, |cores| cores.get().max(2) - 1);
            let row = |band: usize| QUARRY_SOIL_SIZE * band / bands;
            let parts: Vec<Vec<u8>> = std::thread::scope(|scope| {
                let workers: Vec<_> = (0..bands)
                    .map(|band| {
                        scope.spawn(move || {
                            bake_quarry_ground(sand_accum(), row(band), row(band + 1))
                        })
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
    #[cfg(target_arch = "wasm32")]
    {
        let workers = page::Workers::start(&key)?;
        Some(Bake { key, workers })
    }
}

impl Bake {
    /// The finished pixels, once; an error when a worker failed.
    pub fn finished(&self) -> Option<Result<Pixels, String>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.pixels.try_recv().ok().map(Ok)
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.workers.finished()
        }
    }
}

/// The page's bake workers (`src/bake-worker.ts`), which `index.html` starts for the
/// shell (`sloppyBakeWorker`): one band each.
#[cfg(target_arch = "wasm32")]
mod page {
    use std::cell::RefCell;
    use std::rc::Rc;

    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::Closure;

    /// Most workers one bake splits into, as the page shell's did.
    const MAX_WORKERS: u32 = 4;

    /// The bands that have come back, in order, or why one could not.
    struct Bands {
        parts: Vec<Option<js_sys::Uint8Array>>,
        error: Option<String>,
    }

    /// A bake's workers, one band each; dropping it stops them.
    pub struct Workers {
        workers: Vec<web_sys::Worker>,
        bands: Rc<RefCell<Bands>>,
        _listeners: Vec<Closure<dyn FnMut(web_sys::MessageEvent)>>,
        _error_listeners: Vec<Closure<dyn FnMut(web_sys::Event)>>,
    }

    /// Workers for one bake: leave a core to the page and one to the GPU process.
    fn worker_count(window: &web_sys::Window) -> u32 {
        let cores = window.navigator().hardware_concurrency() as u32;
        cores.saturating_sub(2).clamp(1, MAX_WORKERS)
    }

    impl Workers {
        /// One worker per band of `key`, each sent this instance's module; `None` when
        /// the page starts no workers.
        pub fn start(key: &str) -> Option<Self> {
            let window = web_sys::window()?;
            let start: js_sys::Function = js_sys::Reflect::get(&window, &"sloppyBakeWorker".into())
                .ok()?
                .dyn_into()
                .ok()?;
            let count = worker_count(&window);
            let bands = Rc::new(RefCell::new(Bands {
                parts: vec![None; count as usize],
                error: None,
            }));
            let mut workers = Self {
                workers: Vec::new(),
                bands: bands.clone(),
                _listeners: Vec::new(),
                _error_listeners: Vec::new(),
            };
            for band in 0..count {
                let worker: web_sys::Worker = start.call0(&window).ok()?.dyn_into().ok()?;
                let answers = bands.clone();
                // The listeners call into JS before they borrow the bands: a JS exception
                // would leave a borrow behind.
                let listener = Closure::<dyn FnMut(_)>::new(move |event: web_sys::MessageEvent| {
                    match event.data().dyn_into::<js_sys::Uint8Array>() {
                        Ok(pixels) => answers.borrow_mut().parts[band as usize] = Some(pixels),
                        Err(error) => {
                            let error = error.as_string().unwrap_or_else(|| "no pixels".into());
                            answers.borrow_mut().error.get_or_insert(error);
                        }
                    }
                });
                let failures = bands.clone();
                let error_listener = Closure::<dyn FnMut(_)>::new(move |event: web_sys::Event| {
                    // A script error is an `ErrorEvent`; a script that did not load, an `Event`.
                    let error = event.dyn_ref::<web_sys::ErrorEvent>().map_or_else(
                        || "a bake worker did not load".into(),
                        |error| error.message(),
                    );
                    failures.borrow_mut().error.get_or_insert(error);
                });
                worker.set_onmessage(Some(listener.as_ref().unchecked_ref()));
                worker.set_onerror(Some(error_listener.as_ref().unchecked_ref()));
                let request = js_sys::Object::new();
                for (name, value) in [
                    ("module", wasm_bindgen::module()),
                    ("key", key.into()),
                    ("band", band.into()),
                    ("bands", count.into()),
                ] {
                    js_sys::Reflect::set(&request, &name.into(), &value).ok()?;
                }
                worker.post_message(&request).ok()?;
                workers.workers.push(worker);
                workers._listeners.push(listener);
                workers._error_listeners.push(error_listener);
            }
            Some(workers)
        }

        /// Every band's rows top to bottom in one array, once all have come back.
        pub fn finished(&self) -> Option<Result<js_sys::Uint8Array, String>> {
            let bands = self.bands.borrow();
            if let Some(error) = &bands.error {
                web_sys::console::warn_1(
                    &format!("Baking in workers failed; the engine bakes it instead: {error}")
                        .into(),
                );
                return Some(Err(error.clone()));
            }
            let parts: Vec<&js_sys::Uint8Array> = bands
                .parts
                .iter()
                .map(Option::as_ref)
                .collect::<Option<_>>()?;
            let length = parts.iter().map(|part| part.length()).sum();
            let pixels = js_sys::Uint8Array::new_with_length(length);
            let mut offset = 0;
            for part in parts {
                pixels.set(part, offset);
                offset += part.length();
            }
            Some(Ok(pixels))
        }
    }

    impl Drop for Workers {
        fn drop(&mut self) {
            for worker in &self.workers {
                worker.terminate();
            }
        }
    }
}
