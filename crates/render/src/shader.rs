//! Shader variants assembled from WGSL template pieces. A `ShaderKey` captures
//! everything that changes the generated code; `PipelineKey` adds fixed-function
//! state. Both are cached by value, so every distinct material setup compiles once
//! per page and survives round resets.
//!
//! Each distinct shader costs a full compile on a cold shader cache (about 0.3 s
//! for a lit one on an Apple GPU, one at a time in WebKit), so only what changes the
//! cost of every pixel is a variant: lighting, shadow reception, alpha test and
//! effects. The cheap per-material features ([`MaterialFeatures`]) are uniform
//! branches, and the side a material draws follows from the pipeline's culling.

use sloppy_core::scene::{Blending, Material, Shading, Side};

use crate::effects::EffectRegistry;
use crate::material::shadow_side;

mod specialize;

pub const COMMON_WGSL: &str = include_str!("shaders/common.wgsl");
pub const MATERIAL_WGSL: &str = include_str!("shaders/material.wgsl");
pub const STANDARD_WGSL: &str = include_str!("shaders/standard.wgsl");
pub const SHADOW_WGSL: &str = include_str!("shaders/shadow.wgsl");
pub const WATER_WGSL: &str = include_str!("shaders/water.wgsl");
pub const OUTPUT_WGSL: &str = include_str!("shaders/output.wgsl");
pub const MIPMAP_WGSL: &str = include_str!("shaders/mipmap.wgsl");
pub const SHADOW_MERGED_WGSL: &str = include_str!("shaders/shadow_merged.wgsl");
pub const SHADOW_CUTOUT_WGSL: &str = include_str!("shaders/shadow_cutout.wgsl");

/// Where the vertex stage reads instance records (`gpu/instance_store.rs`): each
/// source defines the frame group's binding 5 and `instance_at`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstanceSource {
    /// WebGPU: a storage buffer.
    Storage,
    /// WebGL2, which has no storage buffers: an RGBA32F texture.
    Texture,
}

impl InstanceSource {
    /// The source this build's backend uses.
    pub const BUILD: Self = if cfg!(feature = "webgl") {
        Self::Texture
    } else {
        Self::Storage
    };

    fn wgsl(self) -> &'static str {
        match self {
            Self::Storage => include_str!("shaders/instances_storage.wgsl"),
            Self::Texture => include_str!("shaders/instances_texture.wgsl"),
        }
    }
}

/// The frame declarations with this build's instance records.
fn frame_wgsl(source: InstanceSource) -> String {
    format!("{COMMON_WGSL}{}", source.wgsl())
}

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
    pub alpha_test: bool,
    pub alpha_to_coverage: bool,
    pub receive_shadow: bool,
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
            alpha_test: material.alpha_test > 0.0,
            alpha_to_coverage: material.alpha_to_coverage && material.alpha_test > 0.0,
            receive_shadow: lit && receive_shadow,
            effect,
            extra_attributes,
            shadow_fade: false,
        }
    }

    /// Depth-only variants ignore everything but what can discard a fragment or
    /// move a vertex, so most casters share one or two shaders. An effect that only
    /// shades the surface matters to an alpha-tested caster alone.
    pub fn shadow(
        material: &Material,
        effect: u16,
        extra_attributes: u8,
        fade: bool,
        effects: &EffectRegistry,
    ) -> Self {
        let alpha_test = material.alpha_test > 0.0;
        let definition = effects.get(effect);
        let moves = definition.is_some_and(|e| e.has_vertex() || e.has_world() || e.has_clip());
        let shades = definition.is_some_and(|e| e.has_surface()) && alpha_test;
        Self {
            pass: Pass::Shadow,
            alpha_test,
            effect: if moves || shades { effect } else { 0 },
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

/// The `MATERIAL_*` feature bits of `material.wgsl`, written to the material
/// uniform: features that cost a uniform branch rather than a shader variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialFeatures(pub u32);

impl MaterialFeatures {
    pub const MAP: u32 = 1;
    pub const EMISSIVE_MAP: u32 = 2;
    pub const BUMP: u32 = 4;
    pub const VERTEX_COLORS: u32 = 8;
    pub const FLAT_SHADING: u32 = 16;
    pub const FOG: u32 = 32;

    pub fn of(material: &Material) -> Self {
        let lit = material.shading == Shading::Standard;
        let features = [
            (Self::MAP, material.map.is_some()),
            // Emissive and bump maps apply to standard materials only.
            (Self::EMISSIVE_MAP, lit && material.emissive_map.is_some()),
            (Self::BUMP, lit && material.bump_map.is_some()),
            (Self::VERTEX_COLORS, material.vertex_colors),
            (Self::FLAT_SHADING, material.flat_shading),
            (Self::FOG, material.fog),
        ];
        Self(
            features
                .into_iter()
                .filter(|&(_, used)| used)
                .fold(0, |bits, (bit, _)| bits | bit),
        )
    }

    pub fn has(self, feature: u32) -> bool {
        self.0 & feature != 0
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

/// Three's `polygonOffset` as a WebGPU depth bias: `constant` is the units,
/// `slope_scale_bits` the factor's f32 bits (keys must hash). Zero is no bias.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DepthBias {
    pub constant: i32,
    pub slope_scale_bits: u32,
}

impl DepthBias {
    /// The main-pass bias of a material (`polygon_offset = (factor, units)`).
    pub fn of(material: &Material) -> Self {
        material
            .polygon_offset
            .map_or_else(Self::default, |(factor, units)| Self {
                constant: units.round() as i32,
                slope_scale_bits: factor.to_bits(),
            })
    }

    pub fn slope_scale(self) -> f32 {
        f32::from_bits(self.slope_scale_bits)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PipelineKey {
    pub shader: ShaderKey,
    pub blend: BlendMode,
    pub depth_test: bool,
    pub depth_write: bool,
    /// Main pass only; shadow casters keep the shadow map's own bias.
    pub depth_bias: DepthBias,
    /// The faces that are drawn (culling removes the others).
    pub side: Side,
    pub alpha_to_coverage: bool,
}

impl PipelineKey {
    /// `faded` draws an opaque material with blending, for per-instance opacity.
    pub fn main(shader: ShaderKey, material: &Material, faded: bool) -> Self {
        let blend = match material.blending {
            Blending::Additive => BlendMode::Additive,
            Blending::Normal if material.transparent || faded => BlendMode::Normal,
            Blending::Normal => BlendMode::Replace,
        };
        Self {
            shader,
            blend,
            depth_test: material.depth_test,
            depth_write: material.depth_write,
            depth_bias: DepthBias::of(material),
            side: material.side,
            alpha_to_coverage: shader.alpha_to_coverage,
        }
    }

    /// The shadow pass draws the faces `Material.shadow_side` names (Three's default
    /// is the side opposite the drawn one).
    pub fn shadow(shader: ShaderKey, material: &Material) -> Self {
        Self {
            shader,
            blend: BlendMode::Replace,
            depth_test: true,
            depth_write: true,
            depth_bias: DepthBias::default(),
            side: material
                .shadow_side
                .unwrap_or_else(|| shadow_side(material.side)),
            alpha_to_coverage: false,
        }
    }
}

fn vertex_input(extra_attributes: u8) -> String {
    let mut code = String::from(
        "struct VertexIn {\n    @location(0) position: vec3f,\n    @location(1) normal: vec3f,\n    \
         @location(2) uv: vec2f,\n    @location(3) color: vec4f,\n",
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

/// The variant constants the templates branch on.
fn flags(key: &ShaderKey) -> [(&'static str, bool); 5] {
    [
        ("LIT", key.lit),
        ("ALPHA_TEST", key.alpha_test),
        ("ALPHA_TO_COVERAGE", key.alpha_to_coverage),
        ("RECEIVE_SHADOW", key.receive_shadow),
        ("SHADOW_FADE", key.shadow_fade),
    ]
}

/// The complete WGSL for a surface or shadow variant, reduced to the code its flags
/// enable (`specialize.rs`).
pub fn shader_source(key: &ShaderKey, effects: &EffectRegistry) -> String {
    shader_source_for(key, effects, InstanceSource::BUILD)
}

fn shader_source_for(key: &ShaderKey, effects: &EffectRegistry, source: InstanceSource) -> String {
    let flags = flags(key);
    let mut code = String::with_capacity(24 * 1024);
    for (name, value) in flags {
        code += &format!("const {name}: bool = {value};\n");
    }
    let effect = effects.get(key.effect);
    // Only effects read the extra attributes; without one the pipeline still lists
    // them in its vertex layout, which WebGPU allows.
    code += &vertex_input(if effect.is_some() {
        key.extra_attributes
    } else {
        0
    });
    code += &frame_wgsl(source);
    code += MATERIAL_WGSL;
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
    if !effect.is_some_and(|e| e.has_clip()) {
        code += "fn effect_clip(clip: ptr<function, vec4f>, w: EffectWorld, v: EffectVertex) {}\n";
    }
    if !effect.is_some_and(|e| e.has_surface()) {
        code += "fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {}\n";
    }
    code += match key.pass {
        Pass::Main => STANDARD_WGSL,
        Pass::Shadow => SHADOW_WGSL,
    };
    specialize::specialize(&code, &flags)
}

/// The merged shadow-caster shader (`shadow_merge.rs`).
pub fn shadow_merged_source() -> String {
    fixed_source(SHADOW_MERGED_WGSL, InstanceSource::BUILD)
}

/// The merged alpha-tested shadow-caster shader.
pub fn shadow_cutout_source() -> String {
    fixed_source(SHADOW_CUTOUT_WGSL, InstanceSource::BUILD)
}

/// The water shader: frame declarations plus the water template.
pub fn water_source() -> String {
    fixed_source(WATER_WGSL, InstanceSource::BUILD)
}

fn fixed_source(template: &str, source: InstanceSource) -> String {
    specialize::specialize(&format!("{}{template}", frame_wgsl(source)), &[])
}

/// The GLSL ES 3.00 the WebGL backend compiles, translated from the WGSL with naga
/// the way wgpu's GL backend did: clip-space Y flipped and depth mapped to GL's
/// -1..1 (`ADJUST_COORDINATE_SPACE`), so offscreen targets hold rows in WebGPU's
/// order and store WebGPU's depth, and a point size written for every vertex.
#[cfg(any(test, feature = "webgl"))]
pub mod glsl {
    use naga::back::glsl;

    /// A WGSL `(group, binding)`.
    pub type Binding = (u32, u32);

    /// One translated stage and what it declares, by WGSL binding: ES 3.00 has no
    /// `layout(binding)`, so the backend binds uniform blocks and samplers by name.
    pub struct Stage {
        pub source: String,
        /// Uniform block names (they differ per stage).
        pub blocks: Vec<(String, Binding)>,
        /// Sampler uniform names, by the binding of the texture they read.
        pub samplers: Vec<(String, Binding)>,
    }

    /// Translate the entry points `entries` of one WGSL module, in order.
    pub fn translate(label: &str, wgsl: &str, entries: &[&str]) -> Result<Vec<Stage>, String> {
        let module = naga::front::wgsl::parse_str(wgsl)
            .map_err(|error| format!("{label}: {}", error.emit_to_string(wgsl)))?;
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .map_err(|error| format!("{label}: {error:?}"))?;
        let options = glsl::Options {
            version: glsl::Version::Embedded {
                version: 300,
                is_webgl: true,
            },
            writer_flags: glsl::WriterFlags::ADJUST_COORDINATE_SPACE
                | glsl::WriterFlags::FORCE_POINT_SIZE,
            ..glsl::Options::default()
        };
        let binding = |handle: naga::Handle<naga::GlobalVariable>| {
            module.global_variables[handle]
                .binding
                .as_ref()
                .map(|binding| (binding.group, binding.binding))
        };
        entries
            .iter()
            .map(|&name| {
                let entry = module
                    .entry_points
                    .iter()
                    .find(|entry| entry.name == name)
                    .ok_or_else(|| format!("{label}: no entry point {name}"))?;
                let pipeline = glsl::PipelineOptions {
                    shader_stage: entry.stage,
                    entry_point: entry.name.clone(),
                    multiview: None,
                };
                let mut source = String::new();
                let reflection = glsl::Writer::new(
                    &mut source,
                    &module,
                    &info,
                    &options,
                    &pipeline,
                    naga::proc::BoundsCheckPolicies::default(),
                )
                .and_then(|mut writer| writer.write())
                .map_err(|error| format!("{label} {name}: {error}"))?;
                Ok(Stage {
                    source,
                    blocks: reflection
                        .uniforms
                        .iter()
                        .filter_map(|(&handle, name)| Some((name.clone(), binding(handle)?)))
                        .collect(),
                    samplers: reflection
                        .texture_mapping
                        .iter()
                        .filter_map(|(name, mapping)| {
                            Some((name.clone(), binding(mapping.texture)?))
                        })
                        .collect(),
                })
            })
            .collect()
    }
}

/// Checks the WebGL build's shaders natively; the effect tests use them too.
#[cfg(test)]
pub(crate) mod webgl_check {
    use super::*;

    /// Translate every entry point as the WebGL backend does (GLSL ES 3.00).
    pub fn translate_for_webgl(label: &str, code: &str) {
        let module = naga::front::wgsl::parse_str(code)
            .unwrap_or_else(|error| panic!("{label}: {}", error.emit_to_string(code)));
        let entries: Vec<&str> = module
            .entry_points
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        if let Err(error) = glsl::translate(label, code, &entries) {
            panic!("{error}");
        }
    }

    /// A surface or shadow variant as the WebGL build assembles and translates it.
    pub fn translate_variant(label: &str, key: &ShaderKey, effects: &EffectRegistry) {
        let code = shader_source_for(key, effects, InstanceSource::Texture);
        translate_for_webgl(&format!("{label} {key:?}"), &code);
    }
}

#[cfg(test)]
mod tests {
    use sloppy_core::scene::TextureRef;

    use super::webgl_check::{translate_for_webgl, translate_variant};
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
        for bits in 0..(1u32 << 4) {
            let bit = |i: u32| bits & (1 << i) != 0;
            for effect in 0..=2u16 {
                for extra_attributes in 0..=2 {
                    let key = ShaderKey {
                        pass: Pass::Main,
                        lit: bit(0),
                        alpha_test: bit(1),
                        alpha_to_coverage: bit(1) && bit(2),
                        receive_shadow: bit(0) && bit(3),
                        effect,
                        extra_attributes,
                        shadow_fade: false,
                    };
                    validate(&format!("{key:?}"), &shader_source(&key, &effects));
                }
            }
        }
    }

    #[test]
    fn shadow_variants_are_valid_wgsl() {
        let effects = EffectRegistry::default();
        for alpha_test in [false, true] {
            for fade in [false, true] {
                for effect in 0..=2u16 {
                    let key = ShaderKey {
                        pass: Pass::Shadow,
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
        validate("shadow merged", &shadow_merged_source());
        validate("shadow cutout", &shadow_cutout_source());
        validate("output", OUTPUT_WGSL);
        validate("mipmap", MIPMAP_WGSL);
    }

    #[test]
    fn every_variant_translates_for_webgl() {
        let effects = EffectRegistry::default();
        let texture = InstanceSource::Texture;
        for bits in 0..(1u32 << 4) {
            let bit = |i: u32| bits & (1 << i) != 0;
            for effect in 0..=2u16 {
                let key = ShaderKey {
                    pass: Pass::Main,
                    lit: bit(0),
                    alpha_test: bit(1),
                    alpha_to_coverage: bit(1) && bit(2),
                    receive_shadow: bit(0) && bit(3),
                    effect,
                    extra_attributes: 2,
                    shadow_fade: false,
                };
                translate_variant("surface", &key, &effects);
                let shadow = ShaderKey {
                    pass: Pass::Shadow,
                    alpha_test: bit(1),
                    shadow_fade: bit(2),
                    effect,
                    ..ShaderKey::default()
                };
                translate_variant("shadow", &shadow, &effects);
            }
        }
        translate_for_webgl("water", &fixed_source(WATER_WGSL, texture));
        translate_for_webgl("shadow merged", &fixed_source(SHADOW_MERGED_WGSL, texture));
        translate_for_webgl("shadow cutout", &fixed_source(SHADOW_CUTOUT_WGSL, texture));
        translate_for_webgl("output", OUTPUT_WGSL);
        translate_for_webgl("mipmap", MIPMAP_WGSL);
    }

    #[test]
    fn instance_texture_rows_match_the_store() {
        let declaration = format!(
            "const RECORDS_PER_ROW: u32 = {}u;",
            crate::draw_list::RECORDS_PER_ROW
        );
        assert!(InstanceSource::Texture.wgsl().contains(&declaration));
    }

    #[test]
    fn variants_carry_only_the_code_they_run() {
        let effects = EffectRegistry::default();
        let unlit = shader_source(&ShaderKey::default(), &effects);
        assert!(!unlit.contains("shade_standard") && !unlit.contains("bump_texture,"));
        assert!(!unlit.contains("discard") && !unlit.contains("//"));
        let lit = shader_source(
            &ShaderKey {
                lit: true,
                ..ShaderKey::default()
            },
            &effects,
        );
        assert!(lit.contains("fn shade_standard(") && !lit.contains("fn sun_shadow("));
        let shadowed = shader_source(
            &ShaderKey {
                lit: true,
                receive_shadow: true,
                ..ShaderKey::default()
            },
            &effects,
        );
        assert!(shadowed.contains("fn sun_shadow("));
        let caster = shader_source(
            &ShaderKey::shadow(&Material::default(), 0, 0, false, &effects),
            &effects,
        );
        assert!(!caster.contains("map_texture,") && !caster.contains("fn tsl_hash("));
    }

    #[test]
    fn cheap_material_features_share_one_shader() {
        let atlas = TextureRef::file("atlas.webp");
        let plain = Material::standard(0x808080, 0.5, 0.1);
        let featured = Material {
            map: Some(atlas.clone()),
            emissive_map: Some(atlas.clone()),
            bump_map: Some(atlas),
            vertex_colors: true,
            flat_shading: true,
            fog: false,
            side: Side::Double,
            ..plain.clone()
        };
        assert_eq!(
            ShaderKey::main(&plain, 0, 0, true),
            ShaderKey::main(&featured, 0, 0, true)
        );
        let features = MaterialFeatures::of(&featured);
        for feature in [
            MaterialFeatures::MAP,
            MaterialFeatures::EMISSIVE_MAP,
            MaterialFeatures::BUMP,
            MaterialFeatures::VERTEX_COLORS,
            MaterialFeatures::FLAT_SHADING,
        ] {
            assert!(features.has(feature));
        }
        assert!(!features.has(MaterialFeatures::FOG));
        assert!(MaterialFeatures::of(&plain).has(MaterialFeatures::FOG));
    }

    #[test]
    fn emissive_and_bump_maps_only_apply_to_standard_materials() {
        let atlas = TextureRef::file("atlas.webp");
        let unlit = Material {
            emissive_map: Some(atlas.clone()),
            bump_map: Some(atlas),
            ..Material::basic(0xffffff)
        };
        let features = MaterialFeatures::of(&unlit);
        assert!(!features.has(MaterialFeatures::EMISSIVE_MAP));
        assert!(!features.has(MaterialFeatures::BUMP));
    }

    #[test]
    fn material_feature_bits_match_the_wgsl() {
        for (name, bit) in [
            ("MAP", MaterialFeatures::MAP),
            ("EMISSIVE_MAP", MaterialFeatures::EMISSIVE_MAP),
            ("BUMP", MaterialFeatures::BUMP),
            ("VERTEX_COLORS", MaterialFeatures::VERTEX_COLORS),
            ("FLAT_SHADING", MaterialFeatures::FLAT_SHADING),
            ("FOG", MaterialFeatures::FOG),
        ] {
            let declaration = format!("const MATERIAL_{name}: u32 = {bit}u;");
            assert!(MATERIAL_WGSL.contains(&declaration), "{declaration}");
        }
    }

    #[test]
    fn shadow_keys_collapse_irrelevant_state() {
        let effects = EffectRegistry::default();
        let red = Material::standard(0xff0000, 0.1, 0.5);
        let a = ShaderKey::shadow(&red, 0, 0, false, &effects);
        let green = Material {
            flat_shading: true,
            fog: false,
            side: Side::Double,
            ..Material::basic(0x00ff00)
        };
        let b = ShaderKey::shadow(&green, 0, 0, false, &effects);
        assert_eq!(a, b);
        assert_eq!(PipelineKey::shadow(a, &red).side, Side::Back);
        assert_eq!(PipelineKey::shadow(b, &green).side, Side::Double);
    }

    #[test]
    fn casters_ignore_effects_that_only_shade_the_surface() {
        let effects = EffectRegistry::default();
        let id = |name| effects.id(name).unwrap();
        let (pulse, wave) = (id("pulse"), id("wave"));
        let solid = Material::default();
        let cutout = Material {
            alpha_test: 0.5,
            ..Material::default()
        };
        assert_eq!(
            ShaderKey::shadow(&solid, pulse, 0, false, &effects).effect,
            0
        );
        assert_eq!(
            ShaderKey::shadow(&solid, pulse, 0, true, &effects).effect,
            0
        );
        assert_eq!(
            ShaderKey::shadow(&cutout, pulse, 0, false, &effects).effect,
            pulse
        );
        assert_eq!(
            ShaderKey::shadow(&solid, wave, 0, false, &effects).effect,
            wave
        );
        let plain = ShaderKey::shadow(&solid, 0, 0, false, &effects);
        let with_attributes = ShaderKey::shadow(&solid, pulse, 2, false, &effects);
        assert_eq!(
            shader_source(&plain, &effects),
            shader_source(&with_attributes, &effects)
        );
    }

    #[test]
    fn polygon_offset_biases_only_the_main_pass() {
        let drift = Material {
            polygon_offset: Some((-1.0, -1.0)),
            ..Material::default()
        };
        let main = PipelineKey::main(ShaderKey::main(&drift, 0, 0, true), &drift, false);
        assert_eq!(main.depth_bias.constant, -1);
        assert_eq!(main.depth_bias.slope_scale(), -1.0);
        let effects = EffectRegistry::default();
        let shadow = PipelineKey::shadow(ShaderKey::shadow(&drift, 0, 0, false, &effects), &drift);
        assert_eq!(shadow.depth_bias, DepthBias::default());
        let plain = PipelineKey::main(
            ShaderKey::main(&Material::default(), 0, 0, true),
            &Material::default(),
            false,
        );
        assert_ne!(plain, main);
    }
}
