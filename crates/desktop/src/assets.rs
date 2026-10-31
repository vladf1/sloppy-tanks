//! The game's runtime files of `public/`, by their path there
//! (`textures/ground/village-ground.webp`): embedded in a native binary (`build.rs`); in
//! a page, the fonts, sounds and previews `index.html` fetches beside the Wasm and leaves
//! in `globalThis.sloppyFiles` (the renderer fetches its textures itself).

use std::borrow::Cow;

#[cfg(not(target_arch = "wasm32"))]
include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// The embedded file at `path`.
#[cfg(not(target_arch = "wasm32"))]
pub fn file(path: &str) -> Option<&'static [u8]> {
    FILES
        .binary_search_by(|(key, _)| (*key).cmp(path))
        .ok()
        .map(|index| FILES[index].1)
}

/// A runtime file, or why there is none yet.
pub enum Asset {
    Ready(Cow<'static, [u8]>),
    /// A page is still downloading it: ask again later.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Loading,
    /// Not embedded, or its download failed.
    Missing,
}

impl Asset {
    /// The bytes, if they are here.
    pub fn ready(self) -> Option<Cow<'static, [u8]>> {
        match self {
            Asset::Ready(bytes) => Some(bytes),
            Asset::Loading | Asset::Missing => None,
        }
    }
}

/// The runtime file at `path`.
#[cfg(not(target_arch = "wasm32"))]
pub fn load(path: &str) -> Asset {
    file(path).map_or(Asset::Missing, |bytes| Asset::Ready(Cow::Borrowed(bytes)))
}

/// The runtime file at `path` as the page left it: its bytes once arrived (copied in,
/// the page keeps its own for a later ask), its pending download until then, `null`
/// once that failed, nothing for a file it never fetched.
#[cfg(target_arch = "wasm32")]
pub fn load(path: &str) -> Asset {
    use wasm_bindgen::JsCast;
    let global = js_sys::global();
    let Ok(files) = js_sys::Reflect::get(&global, &"sloppyFiles".into()) else {
        return Asset::Missing;
    };
    if !files.is_object() {
        return Asset::Missing;
    }
    let file = js_sys::Reflect::get(&files, &path.into()).unwrap_or_default();
    if let Some(bytes) = file.dyn_ref::<js_sys::Uint8Array>() {
        Asset::Ready(Cow::Owned(bytes.to_vec()))
    } else if file.is_null() || file.is_undefined() {
        Asset::Missing
    } else {
        Asset::Loading
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// The models name their textures by page path, so every texture the page serves
    /// must be embedded under that path for the app to draw it.
    #[test]
    fn every_texture_in_public_is_embedded_by_its_page_path() {
        let public = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../public");
        let mut directories = vec![public.join("textures")];
        let mut textures = 0;
        while let Some(directory) = directories.pop() {
            for entry in std::fs::read_dir(&directory).expect("a texture folder") {
                let path = entry.expect("a texture folder entry").path();
                if path.is_dir() {
                    directories.push(path);
                } else if path
                    .extension()
                    .is_some_and(|extension| extension == "webp")
                {
                    let relative = path.strip_prefix(&public).expect("under public/");
                    let key = relative.to_string_lossy().replace('\\', "/");
                    assert!(super::file(&key).is_some(), "{key} is not embedded");
                    textures += 1;
                }
            }
        }
        assert!(textures > 0, "public/textures holds the game's textures");
        assert!(super::file("textures/missing.webp").is_none());
    }
}
