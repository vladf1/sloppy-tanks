//! Replaces `loading-assets.ts`, which waited for whatever textures scenery
//! construction had started loading: here the textures a scene needs are read
//! from its built node tree, so presentation can generate them before the first
//! frame.

use crate::scene::{Node, TextureSource};
use crate::sim::maps::MapTheme;

/// The texture files of `theme`'s scenery (`build_scenery`), which a page fetches as an
/// arena's build begins rather than once that model, the build's costliest piece, has
/// been built (a test keeps each list equal to what its scenery draws).
pub fn scenery_texture_files(theme: MapTheme) -> &'static [&'static str] {
    match theme {
        MapTheme::Village => &[
            "textures/ground/dry-grass.webp",
            "textures/ground/packed-dirt.webp",
            "textures/ground/village-ground.webp",
            "textures/water/normals.webp",
            "textures/trees/atlas.webp",
            "textures/walls/weathered-concrete.webp",
            "textures/houses/siding.webp",
            "textures/houses/shingles.webp",
            "textures/houses/clapboard.webp",
            "textures/houses/stone.webp",
            "textures/houses/brick.webp",
        ],
        MapTheme::Harbor => &[
            "textures/water/normals.webp",
            "textures/harbor/steel.webp",
            "textures/harbor/dock.webp",
            "textures/harbor/container-ribbed.webp",
            "textures/harbor/container.webp",
        ],
        MapTheme::Quarry => &[
            "textures/ground/packed-dirt.webp",
            "textures/tanks/armor-wear.webp",
            "textures/tanks/armor-camo.webp",
            "textures/quarry/sandstone.webp",
            "textures/walls/weathered-concrete.webp",
            "textures/harbor/steel.webp",
        ],
    }
}

/// The texture files every round's build loads beside its scenery's: the vehicles,
/// supplies, covers, wrecks and the debris they break into.
const ROUND_TEXTURE_FILES: &[&str] = &[
    "textures/tanks/armor-camo.webp",
    "textures/tanks/armor-wear.webp",
    "textures/tanks/armor-scorch.webp",
    "textures/pickups/atlas.webp",
    "textures/wood/timber.webp",
    "textures/barrels/painted-drum.webp",
    "textures/trees/bark.webp",
    "textures/trees/rings.webp",
    "textures/houses/siding.webp",
    "textures/walls/weathered-concrete.webp",
];

/// The village's birches, the one cover a theme alone has.
const VILLAGE_COVER_TEXTURE_FILES: &[&str] = &["textures/trees/birch.webp"];

/// Every texture file a round on `theme` loads while its arena builds, each once. The
/// renderer reports a build that asks for a file this leaves out, or none for one it
/// names.
pub fn round_texture_files(theme: MapTheme) -> Vec<&'static str> {
    let covers = match theme {
        MapTheme::Village => VILLAGE_COVER_TEXTURE_FILES,
        MapTheme::Harbor | MapTheme::Quarry => &[],
    };
    let mut files: Vec<&'static str> = Vec::new();
    for &file in [scenery_texture_files(theme), ROUND_TEXTURE_FILES, covers]
        .into_iter()
        .flatten()
    {
        if !files.contains(&file) {
            files.push(file);
        }
    }
    files
}

/// Every texture drawn by `root` (visible or not), in first-use order without
/// duplicates: material maps, bump maps, emissive maps and effect inputs
/// (`extra_textures`). File sources are paths
/// under `public/`; generated ones are baked in Rust or drawn by the browser (see
/// `bake_quarry_ground` and `effects_scenery::canvas_texture`).
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
