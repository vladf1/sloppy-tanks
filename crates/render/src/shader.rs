//! Shader variants assembled from WGSL template pieces. A `ShaderKey` captures
//! everything that changes the generated code; `PipelineKey` adds fixed-function
//! state. Both are cached by value, so every distinct material setup compiles once
//! per page and survives round resets.

use sloppy_core::scene::{Blending, Material, Shading, Side};

use crate::effects::EffectRegistry;
use crate::material::{forces_opaque_alpha, shadow_side};

pub const COMMON_WGSL: &str = include_str!("shaders/common.wgsl");
pub const MATERIAL_WGSL: &str = include_str!("shaders/material.wgsl");
pub const STANDARD_WGSL: &str = include_str!("shaders/standard.wgsl");
pub const SHADOW_WGSL: &str = include_str!("shaders/shadow.wgsl");
pub const WATER_WGSL: &str = include_str!("shaders/water.wgsl");
pub const OUTPUT_WGSL: &str = include_str!("shaders/output.wgsl");
pub const MIPMAP_WGSL: &str = include_str!("shaders/mipmap.wgsl");

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Pass {
    /// Lit color into the HDR target (main view and water reflection).
    #[default]
    Main,
    /// Depth into the sun's shadow map.
    Shadow,
}

/// Everything that changes generated WGSL.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ShaderKey {
    pub pass: Pass,
    /// MeshStandardMaterial lighting; false is MeshBasicMaterial.
    pub lit: bool,
    pub map: bool,
    pub bump: bool,
    pub vertex_colors: bool,
    pub flat: bool,
    pub alpha_test: bool,
    pub alpha_to_coverage: bool,
    pub force_opaque: bool,
    pub fog: bool,
    pub receive_shadow: bool,
    pub side: Side,
    /// Effect id from the registry; 0 is none.
    pub effect: u16,
    /// Extra vertex attributes the effect reads (0..=2).
    pub extra_attributes: u8,
    /// Shadow pass only: dither the shadow of a fading instance.
    pub shadow_fade: bool,
}

impl ShaderKey {
    pub fn main(
        material: &Material,
        effect: u16,
        extra_attributes: u8,
        receive_shadow: bool,
    ) -> Self {
        let lit = material.shading == Shading::Standard;
        Self {
            pass: Pass::Main,
            lit,
            map: material.map.is_some(),
            bump: lit && material.bump_map.is_some(),
            vertex_colors: material.vertex_colors,
            flat: material.flat_shading,
            alpha_test: material.alpha_test > 0.0,
            alpha_to_coverage: material.alpha_to_coverage && material.alpha_test > 0.0,
            force_opaque: forces_opaque_alpha(material),
            fog: material.fog,
            receive_shadow: lit && receive_shadow,
            side: material.side,
            effect,
            extra_attributes,
            shadow_fade: false,
        }
    }

    /// Depth-only variants ignore everything but what can discard a fragment or
    /// move a vertex, so most casters share one or two shaders.
    pub fn shadow(material: &Material, effect: u16, extra_attributes: u8, fade: bool) -> Self {
        let alpha_test = material.alpha_test > 0.0;
        Self {
            pass: Pass::Shadow,
            map: alpha_test && material.map.is_some(),
            alpha_test,
            side: shadow_side(material.side),
            effect,
            extra_attributes,
            shadow_fade: fade,
            ..Self::default()
        }
    }

    /// Whether this shadow variant needs a fragment stage at all.
    pub fn shadow_needs_fragment(&self, effects: &EffectRegistry) -> bool {
        self.alpha_test
            || self.shadow_fade
            || effects
                .get(self.effect)
                .is_some_and(|e| e.has_surface() && self.alpha_test)
    }
}

/// Blend and depth state beyond the shader.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlendMode {
    /// Blending disabled (Three enables no blend for opaque normal blending).
    Replace,
    /// srcAlpha, oneMinusSrcAlpha; alpha one, oneMinusSrcAlpha.
    Normal,
    /// srcAlpha, one; alpha one, one.
    Additive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PipelineKey {
    pub shader: ShaderKey,
    pub blend: BlendMode,
    pub depth_test: bool,
    pub depth_write: bool,
    /// The faces that are drawn (culling removes the others).
    pub side: Side,
    pub alpha_to_coverage: bool,
}

impl PipelineKey {
    /// `faded` draws an opaque material with blending, for per-instance opacity.
    pub fn main(shader: ShaderKey, material: &Material, faded: bool) -> Self {
        let mut shader = shader;
        let blend = match material.blending {
            Blending::Additive => BlendMode::Additive,
            Blending::Normal if material.transparent || faded => BlendMode::Normal,
            Blending::Normal => BlendMode::Replace,
        };
        if faded {
            shader.force_opaque = false;
        }
        Self {
            shader,
            blend,
            depth_test: material.depth_test,
            depth_write: material.depth_write,
            side: material.side,
            alpha_to_coverage: shader.alpha_to_coverage,
        }
    }

    pub fn shadow(shader: ShaderKey) -> Self {
        Self {
            shader,
            blend: BlendMode::Replace,
            depth_test: true,
            depth_write: true,
            side: shader.side,
            alpha_to_coverage: false,
        }
    }
}

fn flag(name: &str, value: bool) -> String {
    format!("const {name}: bool = {value};\n")
}

fn vertex_input(extra_attributes: u8) -> String {
    let mut code = String::from(
        "struct VertexIn {\n    @location(0) position: vec3f,\n    @location(1) normal: vec3f,\n    \
         @location(2) uv: vec2f,\n    @location(3) color: vec3f,\n",
    );
    for slot in 0..extra_attributes {
        code += &format!("    @location({}) extra{slot}: vec4f,\n", 4 + slot);
    }
    code += "}\n";
    for slot in 0..2u8 {
        let value = if slot < extra_attributes {
            format!("input.extra{slot}")
        } else {
            "vec4f(0.0)".into()
        };
        code += &format!("fn vertex_extra{slot}(input: VertexIn) -> vec4f {{ return {value}; }}\n");
    }
    code
}

/// The complete WGSL for a surface or shadow variant.
pub fn shader_source(key: &ShaderKey, effects: &EffectRegistry) -> String {
    let mut code = String::with_capacity(24 * 1024);
    code += &flag("LIT", key.lit);
    code += &flag("HAS_MAP", key.map);
    code += &flag("HAS_BUMP", key.bump);
    code += &flag("VERTEX_COLORS", key.vertex_colors);
    code += &flag("FLAT_SHADING", key.flat);
    code += &flag("ALPHA_TEST", key.alpha_test);
    code += &flag("ALPHA_TO_COVERAGE", key.alpha_to_coverage);
    code += &flag("FORCE_OPAQUE", key.force_opaque);
    code += &flag("FOG", key.fog);
    code += &flag("RECEIVE_SHADOW", key.receive_shadow);
    code += &flag("DOUBLE_SIDED", key.side == Side::Double);
    code += &flag("BACK_SIDE", key.side == Side::Back);
    code += &flag("SHADOW_FADE", key.shadow_fade);
    code += &vertex_input(key.extra_attributes);
    code += COMMON_WGSL;
    code += MATERIAL_WGSL;
    let effect = effects.get(key.effect);
    if let Some(effect) = effect {
        code += effect.wgsl;
        code += "\n";
    }
    if !effect.is_some_and(|e| e.has_vertex()) {
        code += "fn effect_vertex(v: ptr<function, EffectVertex>) {}\n";
    }
    if !effect.is_some_and(|e| e.has_world()) {
        code += "fn effect_world(w: ptr<function, EffectWorld>, v: EffectVertex) {}\n";
    }
    if !effect.is_some_and(|e| e.has_surface()) {
        code += "fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {}\n";
    }
    code += match key.pass {
        Pass::Main => STANDARD_WGSL,
        Pass::Shadow => SHADOW_WGSL,
    };
    code
}

/// The water shader: frame declarations plus the water template.
pub fn water_source() -> String {
    format!("{}{}", COMMON_WGSL, WATER_WGSL)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validate(label: &str, code: &str) {
        let module = naga::front::wgsl::parse_str(code)
            .unwrap_or_else(|error| panic!("{label}: {}", error.emit_to_string(code)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap_or_else(|error| panic!("{label}: {error:?}"));
    }

    #[test]
    fn every_surface_variant_is_valid_wgsl() {
        let effects = EffectRegistry::default();
        let mut count = 0;
        for bits in 0..(1u32 << 9) {
            let bit = |i: u32| bits & (1 << i) != 0;
            for effect in 0..=2u16 {
                let key = ShaderKey {
                    pass: Pass::Main,
                    lit: bit(0),
                    map: bit(1),
                    bump: bit(2),
                    vertex_colors: bit(3),
                    flat: bit(4),
                    alpha_test: bit(5),
                    alpha_to_coverage: bit(6),
                    force_opaque: bit(7),
                    fog: true,
                    receive_shadow: bit(8),
                    side: [Side::Front, Side::Back, Side::Double][(bits % 3) as usize],
                    effect,
                    extra_attributes: (bits % 3) as u8,
                    shadow_fade: false,
                };
                // Validating every combination is slow in debug; sample them.
                if bits % 7 == 0 || effect > 0 && bits % 31 == 0 {
                    validate(&format!("{key:?}"), &shader_source(&key, &effects));
                    count += 1;
                }
            }
        }
        assert!(count > 60);
    }

    #[test]
    fn shadow_variants_are_valid_wgsl() {
        let effects = EffectRegistry::default();
        for alpha_test in [false, true] {
            for fade in [false, true] {
                for effect in 0..=2u16 {
                    let key = ShaderKey {
                        pass: Pass::Shadow,
                        map: alpha_test,
                        alpha_test,
                        shadow_fade: fade,
                        effect,
                        ..ShaderKey::default()
                    };
                    validate(&format!("{key:?}"), &shader_source(&key, &effects));
                }
            }
        }
    }

    #[test]
    fn fixed_shaders_are_valid_wgsl() {
        validate("water", &water_source());
        validate("output", OUTPUT_WGSL);
        validate("mipmap", MIPMAP_WGSL);
    }

    #[test]
    fn shadow_keys_collapse_irrelevant_state() {
        let a = ShaderKey::shadow(&Material::standard(0xff0000, 0.1, 0.5), 0, 0, false);
        let b = ShaderKey::shadow(
            &Material {
                flat_shading: true,
                fog: false,
                ..Material::basic(0x00ff00)
            },
            0,
            0,
            false,
        );
        assert_eq!(a, b);
        assert_eq!(a.side, Side::Back);
    }
}
