//! Replaces `loading-assets.ts`, which waited for whatever textures scenery
//! construction had started loading: here the textures a scene needs are read
//! from its built node tree, so the page can fetch (or generate) them before the
//! first frame.

use crate::scene::{Effect, Node, TextureSource};

use super::effects_scenery::{
    GRIT_TEXTURE, QUARRY_SOIL, QUARRY_SOIL_TEXTURE, SAND_DRIFT, SANDSTONE,
};

/// Textures a custom effect samples beyond its material's `map` and `bump_map`.
fn effect_textures(effect: &Effect) -> &'static [TextureSource] {
    const GRIT: &[TextureSource] = &[TextureSource::File(GRIT_TEXTURE)];
    const SOIL: &[TextureSource] = &[TextureSource::Generated(QUARRY_SOIL_TEXTURE)];
    match effect {
        Effect::Custom { name, .. } if *name == QUARRY_SOIL || *name == SAND_DRIFT => GRIT,
        Effect::Custom { name, .. } if *name == SANDSTONE => SOIL,
        _ => &[],
    }
}

/// Every texture drawn by `root` (visible or not), in first-use order without
/// duplicates: material maps, bump maps and effect inputs. File sources are paths
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
        for texture in [&material.map, &material.bump_map].into_iter().flatten() {
            add(&texture.source);
        }
        for source in effect_textures(&material.effect) {
            add(source);
        }
    });
    textures
}
