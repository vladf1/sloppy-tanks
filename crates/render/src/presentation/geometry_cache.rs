//! The page's cache of generated geometry: a returning visitor reads the vehicle
//! models and a theme's scenery back from IndexedDB instead of building them again
//! (about 60 ms of a fast desktop's load, four times that on a slow CPU).
//!
//! Files (`sloppy_core::scene_codec`) hold what the core builders return, before
//! presentation splits or batches it: `build_scenery`'s root and every vehicle model.
//! Their key is the geometry version (`geometryVersion` in
//! `scripts/content-version.mjs`: the sources the cached builders compile, proven by a
//! build of sloppy-core with nothing else, with their crates, compiler, profiles and
//! flags) and the graphics backend, which build separate binaries; reading checks it,
//! so a release that changes no model keeps its files, and one that does cannot read
//! them. The vehicles give the simulation its hitboxes (`tank_hull`), so no file may
//! outlive the code that made it. Reading gives back the same values and the same `Arc` sharing, within
//! the files and with the builders' stores (`models::shared_values`), so batching,
//! uploads and draw counts come out as for a built model.
//!
//! The page (`src/geometry-cache.ts`) reads the vehicles and the last theme's file as
//! it boots, any other theme when a build asks for it ([`reading`]), and stores what
//! the shell hands it: a built scenery is encoded into the page's memory at the
//! build's next step ([`encode_step`], about 5 ms on a fast desktop), and stored only
//! once the arena is ready ([`write_step`], from `Presentation::prebuild_step`).
//! Natively, and in a page opened with `?geometry-cache=off`, there is no cache.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

use sloppy_core::models::{MapTheme, Scenery, build_scenery, built_tank_models, offer_tank_models};
/// The build's geometry version (`SLOPPY_GEOMETRY_VERSION`, stamped by
/// `scripts/build-wasm.mjs`).
const GEOMETRY_VERSION: Option<&str> = option_env!("SLOPPY_GEOMETRY_VERSION");
use sloppy_core::scene::Node;

/// The file of a theme's scenery.
fn scenery_file(theme: MapTheme) -> &'static str {
    match theme {
        MapTheme::Village => "scenery-village",
        MapTheme::Harbor => "scenery-harbor",
        MapTheme::Quarry => "scenery-quarry",
    }
}

const VEHICLES: &str = "vehicles";

/// Files this build reads and writes: the geometry version and the graphics backend.
fn version() -> String {
    let backend = if cfg!(feature = "webgl") {
        "webgl"
    } else {
        "webgpu"
    };
    format!("{}/{backend}", GEOMETRY_VERSION.unwrap_or_default())
}

/// What a file's header must say: its version and name.
fn key(file: &str) -> String {
    format!("{}/{file}", version())
}

/// Builds without a geometry version (tests, `cargo run`) cannot tell their files
/// apart, so they keep none.
fn enabled() -> bool {
    GEOMETRY_VERSION.is_some()
}

/// How long a build stepped over frames waits for a file the page is still reading
/// before it builds the model instead.
const READ_WAIT_MS: f64 = 250.0;
/// How long the shell's start waits for the vehicles' file.
const VEHICLE_WAIT_MS: f64 = 30.0;

thread_local! {
    /// Built models to write once the arena is ready: file name and tree.
    static PENDING: RefCell<Vec<(&'static str, Node)>> = const { RefCell::new(Vec::new()) };
}

/// The vehicle models came from the page this session: nothing to write.
static VEHICLES_READ: AtomicBool = AtomicBool::new(false);

/// A theme's scenery, and whether it was read back: read back when the page has its
/// file, otherwise built and kept to write once the arena is ready.
pub fn scenery(theme: MapTheme) -> (Scenery, bool) {
    let file = scenery_file(theme);
    if enabled()
        && let Some(root) = page::take(file, &key(file))
    {
        return (Scenery::from_root(theme, root), true);
    }
    let started = page::now();
    let scenery = build_scenery(theme);
    page::stat(file, "buildMs", page::now() - started);
    if enabled() && page::present() {
        // Encoded at the build's next step (`encode_step`), as presentation's copy
        // shares these meshes until then.
        PENDING.with_borrow_mut(|pending| pending.push((file, scenery.root().clone())));
    }
    (scenery, false)
}

/// Whether a build stepped over frames should wait for the page to read `theme`'s
/// file, having waited `waited_ms`: asks the page for it the first time.
pub fn reading(theme: MapTheme, waited_ms: f64) -> bool {
    enabled() && page::reading(scenery_file(theme)) && waited_ms < READ_WAIT_MS
}

/// Wait briefly for the page to read the vehicles' file, if it is reading one, so
/// [`offer_vehicles`] finds it: the shell asks while its graphics start.
pub async fn await_vehicles() {
    if enabled() {
        page::when(VEHICLES, VEHICLE_WAIT_MS).await;
    }
}

/// Offer the vehicle models the page has read, before anything measures or builds
/// one: once any is built, read models would no longer share what built ones do.
pub fn offer_vehicles() {
    if !enabled()
        || VEHICLES_READ.load(Ordering::Relaxed)
        || sloppy_core::models::built_tank_model_count() > 0
    {
        return;
    }
    if let Some(group) = page::take(VEHICLES, &key(VEHICLES)) {
        VEHICLES_READ.store(offer_tank_models(group), Ordering::Relaxed);
    }
}

/// Encode one built scenery into the page's memory; true if there was one. A build
/// takes this as a step of its own right after the model, while the tree is alive
/// anyway: kept until the arena is ready instead, its meshes raised the Wasm heap's
/// peak (which never comes down) by 12 MB.
pub fn encode_step() -> bool {
    let Some((file, root)) = PENDING.with_borrow_mut(Vec::pop) else {
        return false;
    };
    page::write(file, &version(), &key(file), &root);
    true
}

/// Once the arena is ready, between its frames: encode what is left and let the page
/// store what it was handed, off the critical path. True while there was work.
pub fn write_step() -> bool {
    if encode_step() {
        return true;
    }
    page::flush();
    false
}

/// Hand the page every vehicle model, once all are built and before they are
/// released, unless they came from it.
pub fn write_vehicles() {
    if !enabled() || VEHICLES_READ.load(Ordering::Relaxed) || !page::present() {
        return;
    }
    if let Some(group) = built_tank_models() {
        page::write(VEHICLES, &version(), &key(VEHICLES), &group);
        page::flush();
    }
}

/// The page's clock, for the build's wait.
pub fn now() -> f64 {
    page::now()
}

#[cfg(target_arch = "wasm32")]
mod page {
    use js_sys::{ArrayBuffer, Function, Object, Promise, Reflect, Uint8Array};
    use sloppy_core::scene::Node;
    use sloppy_core::scene_codec::{decode, encode, header_len, sections};
    use wasm_bindgen::{JsCast, JsValue};

    /// `globalThis.sloppyGeometry` (`src/geometry-cache.ts`), when the page keeps a
    /// cache.
    fn cache() -> Option<Object> {
        Reflect::get(&js_sys::global(), &"sloppyGeometry".into())
            .ok()?
            .dyn_into()
            .ok()
    }

    fn get(object: &JsValue, name: &str) -> Option<JsValue> {
        Reflect::get(object, &name.into())
            .ok()
            .filter(|value| !value.is_undefined())
    }

    fn call(name: &str, args: &[JsValue]) -> Option<JsValue> {
        let cache = cache()?;
        let function: Function = get(&cache, name)?.dyn_into().ok()?;
        let args: js_sys::Array = args.iter().collect();
        function.apply(&cache, &args).ok()
    }

    pub fn present() -> bool {
        cache().is_some()
    }

    pub fn now() -> f64 {
        web_sys::window()
            .and_then(|window| window.performance())
            .map_or(0.0, |performance| performance.now())
    }

    /// Record a measurement for the checks (`sloppyGeometry.stats[file][what]`).
    pub fn stat(file: &str, what: &str, value: f64) {
        let Some(stats) = cache().and_then(|cache| get(&cache, "stats")) else {
            return;
        };
        let entry = get(&stats, file).unwrap_or_else(|| {
            let entry: JsValue = Object::new().into();
            let _ = Reflect::set(&stats, &file.into(), &entry);
            entry
        });
        let _ = Reflect::set(&entry, &what.into(), &value.into());
    }

    /// Whether the page is reading `file`, asking it to the first time.
    pub fn reading(file: &str) -> bool {
        call("request", &[file.into()]).is_some_and(|value| value.is_instance_of::<Promise>())
    }

    pub async fn when(file: &str, ms: f64) {
        if let Some(promise) = call("when", &[file.into(), ms.into()])
            .and_then(|value| value.dyn_into::<Promise>().ok())
        {
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
        }
    }

    /// The model in `file` if the page has read it, checked against `key`. The page's
    /// copy goes either way: a file that does not decode is built and written again.
    pub fn take(file: &str, key: &str) -> Option<Node> {
        let buffer: ArrayBuffer = call("take", &[file.into()])?.dyn_into().ok()?;
        let started = now();
        let all = Uint8Array::new(&buffer);
        let total = all.length() as usize;
        let decoded = (|| {
            if total < header_len() {
                return Err("a short file");
            }
            let mut header = vec![0; header_len()];
            all.subarray(0, header_len() as u32).copy_to(&mut header);
            let (structure_len, data_len) = sections(&header).map_err(|error| error.0)?;
            if total != structure_len + data_len {
                return Err("a truncated file");
            }
            let mut structure = vec![0; structure_len];
            all.subarray(0, structure_len as u32)
                .copy_to(&mut structure);
            decode(&structure, key, &mut |offset, out| {
                let start = structure_len + offset;
                if start + out.len() > total {
                    return false;
                }
                all.subarray(start as u32, (start + out.len()) as u32)
                    .copy_to(out);
                true
            })
            .map_err(|error| error.0)
        })();
        stat(file, "readMs", now() - started);
        match decoded {
            Ok(root) => Some(root),
            Err(reason) => {
                stat(file, "rejected", 1.0);
                web_sys::console::info_1(
                    &format!("The cached {file} was not used: {reason}").into(),
                );
                None
            }
        }
    }

    /// Let the page store what it was handed.
    pub fn flush() {
        call("flush", &[]);
    }

    /// Encode `root` into a buffer of the page's and hand it over to store.
    pub fn write(file: &str, version: &str, key: &str, root: &Node) {
        let started = now();
        let encoded = encode(root, key);
        let buffer = ArrayBuffer::new(encoded.len() as u32);
        let view = Uint8Array::new(&buffer);
        view.subarray(0, encoded.structure.len() as u32)
            .copy_from(&encoded.structure);
        let mut at = encoded.structure.len();
        for slice in &encoded.data {
            if !slice.is_empty() {
                view.subarray(at as u32, (at + slice.len()) as u32)
                    .copy_from(slice);
            }
            at += slice.len();
        }
        stat(file, "writeMs", now() - started);
        call("write", &[file.into(), version.into(), buffer.into()]);
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod page {
    use sloppy_core::scene::Node;

    pub fn present() -> bool {
        false
    }
    pub fn now() -> f64 {
        0.0
    }
    pub fn stat(_file: &str, _what: &str, _value: f64) {}
    pub fn reading(_file: &str) -> bool {
        false
    }
    pub async fn when(_file: &str, _ms: f64) {}
    pub fn take(_file: &str, _key: &str) -> Option<Node> {
        None
    }
    pub fn write(_file: &str, _version: &str, _key: &str, _root: &Node) {}
    pub fn flush() {}
}
