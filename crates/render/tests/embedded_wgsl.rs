//! The binary embeds the WGSL files without their comments (`build.rs`): the browser
//! never sees them, and as written they were about 190 KB of each engine's download.

use sloppy_render::effects::BUILTIN_EFFECTS;
use sloppy_render::presentation::PRESENTATION_EFFECTS;
use sloppy_render::shader::{
    COMMON_WGSL, MATERIAL_WGSL, MIPMAP_WGSL, OCCLUSION_WGSL, OUTPUT_WGSL, SHADOW_CUTOUT_WGSL,
    SHADOW_MERGED_WGSL, SHADOW_WGSL, SKY_WGSL, STANDARD_WGSL, WATER_WGSL,
};

#[test]
fn embedded_shaders_carry_no_comments_or_blank_lines() {
    let fixed = [
        COMMON_WGSL,
        MATERIAL_WGSL,
        STANDARD_WGSL,
        SHADOW_WGSL,
        WATER_WGSL,
        OUTPUT_WGSL,
        SKY_WGSL,
        OCCLUSION_WGSL,
        MIPMAP_WGSL,
        SHADOW_MERGED_WGSL,
        SHADOW_CUTOUT_WGSL,
    ];
    let effects = BUILTIN_EFFECTS
        .iter()
        .chain(PRESENTATION_EFFECTS.iter())
        .map(|effect| effect.wgsl);
    for source in fixed.into_iter().chain(effects) {
        assert!(!source.is_empty());
        assert!(!source.contains("//") && !source.contains("/*"));
        assert!(
            source
                .lines()
                .all(|line| !line.trim().is_empty() && line == line.trim_end())
        );
        assert!(source.ends_with('\n'));
    }
}
