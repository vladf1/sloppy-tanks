//! Replaces `loading-assets.ts`, which waited for whatever textures scenery
//! construction had started loading: here the textures a scene needs are read
//! from its built node tree, so the page can fetch (or generate) them before the
//! first frame.

use crate::scene::{Node, TextureSource};

/// Every texture drawn by `root` (visible or not), in first-use order without
/// duplicates: material maps, bump maps, emissive maps and effect inputs
/// (`extra_textures`). File sources are paths
/// under `public/`; generated ones are baked in Rust or drawn by the browser (see
/// `effects_scenery::generated_texture` and `canvas_texture`).
pub fn node_textures(root: &Node) -> Vec<TextureSource> {
    let mut textures = Vec::new();
    let mut add = |source: &TextureSource| {
        if !textures.contains(source) {
            textures.push(source.clone());
        }
    };
    root.traverse(glam::DMat4::IDENTITY, &mut |node, _| {
        let Some(drawable) = &node.drawable else {
            return;
        };
        let material = &drawable.material;
        let maps = [&material.map, &material.bump_map, &material.emissive_map];
        for texture in maps.into_iter().flatten() {
            add(&texture.source);
        }
        for (_, texture) in &material.extra_textures {
            add(&texture.source);
        }
    });
    textures
}
