//! Texture bakes: textures a full-screen pass draws once on the GPU from WGSL, the
//! first time a material asks for them, so static surface layers that vary slowly
//! across a map are worked out per texel once rather than per pixel every frame. A
//! bake answers a `TextureSource::Generated` key; the texture store holds it as
//! loading until its pass has run (`gpu::textures`), so an arena waits for it like
//! any other texture, and both backends draw it with the same WGSL.

use sloppy_core::models::effects_scenery;

use super::registry::HARBOR_APRON_WGSL;

/// One bake: the generated-texture key it answers, its edge in texels (square,
/// linear RGBA8, with every mip level) and the WGSL pieces that, concatenated, define
/// `fn fs_bake(input: BakeOut) -> @location(0) vec4f`, the texel at `input.uv`
/// (0..1, v increasing down the rows, as a material samples it).
#[derive(Debug, PartialEq, Eq)]
pub struct TextureBake {
    pub key: &'static str,
    pub size: u32,
    pub wgsl: &'static [&'static str],
}

impl TextureBake {
    /// Its complete WGSL, entry points `vs_bake` and `fs_bake`.
    pub fn source(&self) -> String {
        crate::shader::bake_source(&self.wgsl.concat())
    }

    /// GPU bytes of the texture and its mip chain.
    pub fn bytes(&self) -> u64 {
        (u64::from(self.size) * u64::from(self.size) * 4 * 4).div_ceil(3)
    }
}

/// Harbor Havoc's ground wear (`harbor_apron_wear.wgsl`): 12.5 cm texels over the
/// 128 m round the arena, enough for the stains, wheel paths and puddles it holds,
/// whose edges are soft; the crisp detail stays per pixel.
pub const HARBOR_APRON_WEAR: TextureBake = TextureBake {
    key: effects_scenery::HARBOR_APRON_WEAR,
    size: 1024,
    wgsl: &[
        HARBOR_APRON_WGSL,
        include_str!("../shaders/effects/harbor_apron_wear.wgsl"),
    ],
};

pub const BAKES: [TextureBake; 1] = [HARBOR_APRON_WEAR];

/// The bake that answers a generated texture key, if any.
pub fn bake_for(key: &str) -> Option<&'static TextureBake> {
    BAKES.iter().find(|bake| bake.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shader::StageSources;
    use crate::shader::webgl_check::{translate_for_webgl, validate, validate_stages};

    /// Every bake is valid WGSL in both its stages and translates for WebGL.
    #[test]
    fn every_bake_is_valid_wgsl_and_translates_for_webgl() {
        for bake in &BAKES {
            let source = bake.source();
            validate(bake.key, &source);
            validate_stages(
                bake.key,
                &StageSources::of(&source, "vs_bake", Some("fs_bake")),
            );
            translate_for_webgl(bake.key, &source);
            assert!(bake.size.is_power_of_two());
        }
        assert_eq!(
            bake_for(effects_scenery::HARBOR_APRON_WEAR),
            Some(&HARBOR_APRON_WEAR)
        );
        assert_eq!(bake_for("sign"), None);
    }
}
