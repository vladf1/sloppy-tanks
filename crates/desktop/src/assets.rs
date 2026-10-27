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
    #[test]
    fn every_texture_the_arenas_name_is_embedded() {
        assert!(super::file("textures/houses/brick.webp").is_some());
        assert!(super::file("audio/shot.mp3").is_some());
        assert!(super::file("textures/missing.webp").is_none());
    }
}
