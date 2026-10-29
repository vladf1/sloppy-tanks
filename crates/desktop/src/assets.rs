//! The runtime files of `public/` embedded in the binary (`build.rs`), by their path
//! under `public/` (`textures/ground/village-ground.webp`).

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// The embedded file at `path`.
pub fn file(path: &str) -> Option<&'static [u8]> {
    FILES
        .binary_search_by(|(key, _)| (*key).cmp(path))
        .ok()
        .map(|index| FILES[index].1)
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
