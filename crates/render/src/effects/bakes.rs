//! Textures the GPU renders once from a material: an effect's still layers, worked
//! out per texel of a world-space atlas instead of per pixel of every frame.
//!
//! A material names a bake by its key, `TextureRef::generated(key)`, among its
//! textures. The material of the bake's [`BakeDefinition::source`] effect supplies
//! everything the bake reads: its uniform (the effect params) and its other
//! textures, bound as for drawing, with the frame's main-view uniform (the sun).
//! The renderer (`gpu::Renderer::run_bakes`) renders it once those textures have
//! loaded and its pipeline has compiled, then renders the mip chain; until then the
//! materials naming it sample the white placeholder and it counts as a pending
//! texture, so an arena's preparation waits for it. It stays for the page's life,
//! like every other texture.
//!
//! The bake's WGSL sees the frame and material declarations of a surface shader and
//! defines `fn bake(uv: vec2f) -> vec4f`: the value of the texel at `uv` over the
//! whole target, rows from the first (`shaders/bake.wgsl`), stored as linear RGBA8
//! (so the texture reference must not ask for sRGB; a colour keeps its darks
//! better stored as its square root). It runs one full-screen pass, so it may be as
//! heavy as its texel count allows, and costs one pipeline compile.

use sloppy_core::models::VILLAGE_GROUND_BAKE;

use super::registry::{MEADOW_WGSL, VILLAGE_GROUND};

/// One registered bake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BakeDefinition {
    /// The `TextureSource::Generated` key materials name it by.
    pub key: &'static str,
    pub width: u32,
    pub height: u32,
    /// The effect definition (`EffectDefinition::name`) of the material it renders from.
    pub source: &'static str,
    /// WGSL shared with the source effect (held once in the binary).
    pub wgsl: &'static str,
    /// WGSL appended after `wgsl`, defining `bake`.
    pub entry: &'static str,
}

/// The Pine Village ground's still layers (`meadow_bake.wgsl`; the page layout is
/// `meadow_sway.wgsl`'s `vg_bake_uv`): six pages of 512² texels, the arena's and the
/// valley's grass, earth and fields.
pub const VILLAGE_GROUND_BAKE_DEFINITION: BakeDefinition = BakeDefinition {
    key: VILLAGE_GROUND_BAKE,
    width: 1536,
    height: 1024,
    source: VILLAGE_GROUND.name,
    wgsl: MEADOW_WGSL,
    entry: concat!(
        include_str!("../shaders/effects/meadow_bake.wgsl"),
        "fn bake(uv: vec2f) -> vec4f { return village_ground_bake(uv); }\n",
    ),
};

pub const BUILTIN_BAKES: [BakeDefinition; 1] = [VILLAGE_GROUND_BAKE_DEFINITION];

/// The bake that renders the generated texture `key`, if one does.
pub fn find(key: &str) -> Option<&'static BakeDefinition> {
    BUILTIN_BAKES.iter().find(|bake| bake.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::EffectRegistry;
    use crate::shader::webgl_check::{translate_for_webgl, validate};
    use crate::shader::{InstanceSource, StageSources, bake_source_for};

    /// Every bake's source effect is registered, and its shader is valid WGSL for
    /// both builds, each WebGPU stage alone too.
    #[test]
    fn every_bake_compiles_for_both_builds() {
        let effects = EffectRegistry::default();
        for bake in BUILTIN_BAKES {
            assert!(
                effects.id(bake.source).is_some(),
                "{}: no source effect",
                bake.key
            );
            assert_eq!(find(bake.key), Some(&bake));
            let code = bake_source_for(&bake, InstanceSource::Storage);
            validate(bake.key, &code);
            let stages = StageSources::of(&code, "vs_bake", Some("fs_bake"));
            validate(&format!("{} vertex", bake.key), &stages.vertex);
            validate(
                &format!("{} fragment", bake.key),
                stages.fragment.as_deref().expect("a fragment stage"),
            );
            assert!(
                !stages.vertex.contains("texture"),
                "{}: the vertex stage samples",
                bake.key
            );
            translate_for_webgl(bake.key, &bake_source_for(&bake, InstanceSource::Texture));
        }
    }
}
