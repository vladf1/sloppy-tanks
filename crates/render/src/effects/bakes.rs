//! Texture bakes: textures a full-screen pass draws once on the GPU from WGSL, the
//! first time a material asks for them, so static surface layers that vary slowly
//! across a map are worked out per texel once rather than per pixel every frame. A
//! bake answers a `TextureSource::Generated` key; the texture store holds it as
//! loading until its pass has run (`gpu::Renderer::run_bakes`), so an arena waits for
//! it like any other texture, and both backends draw it with the same WGSL into
//! linear RGBA8 with every mip level (a colour keeps its darks better stored as its
//! square root).
//!
//! A bake works out each texel from its uv alone, or, with a [`TextureBake::source`],
//! also reads the material of that effect (its params and its other textures, bound
//! as for drawing) and the main view's frame uniform (the sun): it then waits until
//! that material exists with those textures loaded, and its WGSL sees a surface
//! shader's frame and material declarations.

use sloppy_core::models::{VILLAGE_GROUND_BAKE as VILLAGE_GROUND_KEY, effects_scenery};

use super::registry::{HARBOR_APRON_WGSL, MEADOW_WGSL, VILLAGE_GROUND};
use crate::shader::InstanceSource;

/// One bake: the generated-texture key it answers, its size in texels and the WGSL
/// pieces that, concatenated, define `fn fs_bake(input: BakeOut) -> @location(0)
/// vec4f`, the texel at `input.uv` (0..1, v increasing down the rows, as a material
/// samples it).
#[derive(Debug, PartialEq, Eq)]
pub struct TextureBake {
    pub key: &'static str,
    pub width: u32,
    pub height: u32,
    /// The effect definition (`EffectDefinition::name`) whose material the bake
    /// reads, if any.
    pub source: Option<&'static str>,
    pub wgsl: &'static [&'static str],
}

impl TextureBake {
    /// Its complete WGSL for this build, entry points `vs_bake` and `fs_bake`.
    pub fn source(&self) -> String {
        self.source_for(InstanceSource::BUILD)
    }

    pub(crate) fn source_for(&self, instances: InstanceSource) -> String {
        let fragment = self.wgsl.concat();
        if self.source.is_some() {
            crate::shader::bake_source_with_material(&fragment, instances)
        } else {
            crate::shader::bake_source(&fragment)
        }
    }

    /// Mip levels down to one texel.
    pub fn levels(&self) -> u32 {
        32 - self.width.max(self.height).leading_zeros()
    }

    /// GPU bytes of the texture and its mip chain.
    pub fn bytes(&self) -> u64 {
        (u64::from(self.width) * u64::from(self.height) * 4 * 4).div_ceil(3)
    }
}

/// Harbor Havoc's ground wear (`harbor_apron_wear.wgsl`): 12.5 cm texels over the
/// 128 m round the arena, enough for the stains, wheel paths and puddles it holds,
/// whose edges are soft; the crisp detail stays per pixel.
pub const HARBOR_APRON_WEAR: TextureBake = TextureBake {
    key: effects_scenery::HARBOR_APRON_WEAR,
    width: 1024,
    height: 1024,
    source: None,
    wgsl: &[
        HARBOR_APRON_WGSL,
        wgsl!("shaders/effects/harbor_apron_wear.wgsl"),
    ],
};

/// The Pine Village ground's still layers (`meadow_bake.wgsl`, from the ground
/// material's plan and params; the page layout is `meadow_sway.wgsl`'s `vg_bake_uv`):
/// six pages of 512² texels, the arena's and the valley's grass, earth and fields.
pub const VILLAGE_GROUND_BAKE: TextureBake = TextureBake {
    key: VILLAGE_GROUND_KEY,
    width: 1536,
    height: 1024,
    source: Some(VILLAGE_GROUND.name),
    wgsl: &[
        MEADOW_WGSL,
        wgsl!("shaders/effects/meadow_bake.wgsl"),
        "@fragment fn fs_bake(input: BakeOut) -> @location(0) vec4f { return village_ground_bake(input.uv); }\n",
    ],
};

pub const BAKES: [TextureBake; 2] = [HARBOR_APRON_WEAR, VILLAGE_GROUND_BAKE];

/// The bake that answers a generated texture key, if any.
pub fn bake_for(key: &str) -> Option<&'static TextureBake> {
    BAKES.iter().find(|bake| bake.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::EffectRegistry;
    use crate::shader::StageSources;
    use crate::shader::webgl_check::{translate_for_webgl, validate, validate_stages};

    /// Every bake is valid WGSL in both its stages, keeps textures out of its vertex
    /// stage, translates for WebGL and reads a registered effect's material.
    #[test]
    fn every_bake_is_valid_wgsl_and_translates_for_webgl() {
        let effects = EffectRegistry::default();
        for bake in &BAKES {
            let source = bake.source_for(InstanceSource::Storage);
            validate(bake.key, &source);
            let stages = StageSources::of(&source, "vs_bake", Some("fs_bake"));
            validate_stages(bake.key, &stages);
            assert!(
                !stages.vertex.contains("texture"),
                "{}: the vertex stage samples",
                bake.key
            );
            translate_for_webgl(bake.key, &bake.source_for(InstanceSource::Texture));
            assert!(bake.source.is_none_or(|name| effects.id(name).is_some()));
            assert_eq!(bake_for(bake.key), Some(bake));
        }
        assert_eq!(bake_for("sign"), None);
    }
}
