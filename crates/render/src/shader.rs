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

use bytemuck::{Pod, Zeroable};
use sloppy_core::scene::{Blending, Effect, Material, Shading, Side, TextureRef};

use crate::color::hex_to_linear;
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

/// Where the vertex stage reads instance records (`InstanceStore`): each
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
    /// Shadow pass only: dither the shadow of a fading instance.
    pub shadow_fade: bool,
}

impl ShaderKey {
    pub fn main(material: &Material, effect: u16, receive_shadow: bool) -> Self {
        let lit = material.shading == Shading::Standard;
        Self {
            pass: Pass::Main,
            lit,
            alpha_test: material.alpha_test > 0.0,
            alpha_to_coverage: material.alpha_to_coverage && material.alpha_test > 0.0,
            receive_shadow: lit && receive_shadow,
            effect,
            shadow_fade: false,
        }
    }

    /// Depth-only variants ignore everything but what can discard a fragment or
    /// move a vertex, so most casters share one or two shaders. An effect that only
    /// shades the surface matters to an alpha-tested caster alone.
    pub fn shadow(material: &Material, effect: u16, fade: bool, effects: &EffectRegistry) -> Self {
        let alpha_test = material.alpha_test > 0.0;
        let definition = effects.get(effect);
        let moves = definition.is_some_and(|e| e.has_vertex() || e.has_world() || e.has_clip());
        let shades = definition.is_some_and(|e| e.has_surface()) && alpha_test;
        Self {
            pass: Pass::Shadow,
            alpha_test,
            effect: if moves || shades { effect } else { 0 },
            shadow_fade: fade,
            ..Self::default()
        }
    }

    /// The variant's vertex and fragment entry points. A shadow caster that neither
    /// alpha-tests nor dithers writes depth only, without a fragment stage.
    pub fn entry_points(&self) -> (&'static str, Option<&'static str>) {
        match self.pass {
            Pass::Main => ("vs_main", Some("fs_main")),
            Pass::Shadow => (
                "vs_shadow",
                (self.alpha_test || self.shadow_fade).then_some("fs_shadow"),
            ),
        }
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
}

/// Byte offset of `MaterialUniform::params`, for pools that animate them. Taken
/// from the layout, so a field added before `params` cannot send the pools' writes
/// into another field.
pub const MATERIAL_PARAMS_OFFSET: u64 = std::mem::offset_of!(MaterialUniform, params) as u64;

/// `material.wgsl`'s `MaterialUniform`, field for field (checked against naga's
/// layout of the WGSL struct in the tests).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MaterialUniform {
    pub color: [f32; 4],
    pub emissive: [f32; 4],
    pub surface: [f32; 4],
    pub map_transform: [f32; 4],
    pub bump_transform: [f32; 4],
    pub emissive_transform: [f32; 4],
    pub params: [[f32; 4]; 4],
    /// x: `MaterialFeatures` bits.
    pub features: [u32; 4],
}

impl MaterialUniform {
    pub fn of(material: &Material) -> Self {
        let [r, g, b] = hex_to_linear(material.color.0);
        let [er, eg, eb] = hex_to_linear(material.emissive.0);
        let i = material.emissive_intensity;
        let transform = |t: Option<&TextureRef>| {
            t.map_or([1.0, 1.0, 0.0, 0.0], |t| {
                [t.repeat[0], t.repeat[1], t.offset[0], t.offset[1]]
            })
        };
        let mut params = [[0.0; 4]; 4];
        if let Effect::Custom { params: values, .. } = &material.effect {
            for (i, value) in values.iter().take(16).enumerate() {
                params[i / 4][i % 4] = *value;
            }
        }
        Self {
            color: [r, g, b, material.opacity],
            emissive: [er * i, eg * i, eb * i, 0.0],
            surface: [
                material.roughness,
                material.metalness,
                material.alpha_test,
                material.bump_scale,
            ],
            map_transform: transform(material.map.as_ref()),
            bump_transform: transform(material.bump_map.as_ref()),
            emissive_transform: transform(material.emissive_map.as_ref()),
            params,
            features: [MaterialFeatures::of(material).0, 0, 0, 0],
        }
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
            side: shadow_side(material),
        }
    }
}

/// The vertex inputs of every surface and shadow variant. The effect varyings
/// `extra0`/`extra1` start at zero (`material.wgsl` `effect_input`).
const VERTEX_INPUT_WGSL: &str = "struct VertexIn {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
    @location(3) color: vec4f,
}
fn vertex_extra0(input: VertexIn) -> vec4f { return vec4f(0.0); }
fn vertex_extra1(input: VertexIn) -> vec4f { return vec4f(0.0); }
";

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
    code += VERTEX_INPUT_WGSL;
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

    /// Texture units, fixed per `(group, binding)` of the WGSL that samples them, so a
    /// material switch touches only the material's units.
    pub mod unit {
        /// Group 0 binding 1: the sun shadow map (comparison sampler).
        pub const SHADOW_MAP: u32 = 0;
        /// Group 0 binding 3: the DFG lookup table.
        pub const DFG_LUT: u32 = 1;
        /// Group 0 binding 5: the instance records (`texelFetch`, no sampler).
        pub const INSTANCES: u32 = 2;
        /// Group 1 bindings 1, 3, 5, 7 and 9: a material's map, bump, emissive and two
        /// effect textures; the water's normals and reflection; a cutout caster's map.
        pub const MATERIAL: u32 = 3;
        /// Group 0 binding 0 of the output and mipmap programs: the texture they read.
        pub const SOURCE: u32 = 8;
        /// Uploads and texture setup; no program samples it.
        pub const UPLOAD: u32 = 9;
        pub const COUNT: usize = 10;
    }

    /// Uniform buffer binding points, fixed per `(group, binding)`.
    pub mod block {
        /// Group 0 binding 0: the view's `Frame`.
        pub const FRAME: u32 = 0;
        /// Group 1 binding 0: the material's (or the water's) uniform.
        pub const MATERIAL: u32 = 1;
        /// Group 0 binding 1 of the output program.
        pub const OUTPUT: u32 = 2;
        pub const COUNT: usize = 3;
    }

    /// The texture unit of a WGSL texture's binding.
    pub fn texture_unit(binding: Binding) -> Option<u32> {
        match binding {
            (0, 0) => Some(unit::SOURCE),
            (0, 1) => Some(unit::SHADOW_MAP),
            (0, 3) => Some(unit::DFG_LUT),
            (0, 5) => Some(unit::INSTANCES),
            (1, binding @ (1 | 3 | 5 | 7 | 9)) => Some(unit::MATERIAL + (binding - 1) / 2),
            _ => None,
        }
    }

    /// The uniform block point of a WGSL uniform's binding.
    pub fn block_point(binding: Binding) -> Option<u32> {
        match binding {
            (0, 0) => Some(block::FRAME),
            (1, 0) => Some(block::MATERIAL),
            (0, 1) => Some(block::OUTPUT),
            _ => None,
        }
    }

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

/// Checks this build's WGSL and the WebGL build's shaders natively; the effect tests
/// use them too.
#[cfg(test)]
pub(crate) mod webgl_check {
    use super::*;

    /// Parse and validate WGSL with naga.
    pub fn validate(label: &str, code: &str) {
        let module = naga::front::wgsl::parse_str(code)
            .unwrap_or_else(|error| panic!("{label}: {}", error.emit_to_string(code)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap_or_else(|error| panic!("{label}: {error:?}"));
    }

    /// Translate every entry point as the WebGL backend does (GLSL ES 3.00).
    pub fn translate_for_webgl(label: &str, code: &str) {
        let module = naga::front::wgsl::parse_str(code)
            .unwrap_or_else(|error| panic!("{label}: {}", error.emit_to_string(code)));
        let entries: Vec<&str> = module
            .entry_points
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        let stages =
            glsl::translate(label, code, &entries).unwrap_or_else(|error| panic!("{error}"));
        // The WebGL backend binds every block and sampler by its WGSL binding; one it
        // has no point or unit for would read nothing, and two bindings on one unit
        // would read each other's texture.
        let mut units = std::collections::HashMap::new();
        for stage in &stages {
            for (name, binding) in &stage.blocks {
                assert!(
                    glsl::block_point(*binding).is_some(),
                    "{label}: uniform block {name} at {binding:?} has no WebGL point"
                );
            }
            for (name, binding) in &stage.samplers {
                let unit = glsl::texture_unit(*binding).unwrap_or_else(|| {
                    panic!("{label}: sampler {name} at {binding:?} has no WebGL unit")
                });
                let other = *units.entry(unit).or_insert(*binding);
                assert_eq!(other, *binding, "{label}: two textures on unit {unit}");
            }
        }
    }

    /// A surface or shadow variant as the WebGL build assembles and translates it.
    pub fn translate_variant(label: &str, key: &ShaderKey, effects: &EffectRegistry) {
        let code = shader_source_for(key, effects, InstanceSource::Texture);
        translate_for_webgl(&format!("{label} {key:?}"), &code);
    }

    /// Validate a variant as this build assembles it, then translate its WebGL build.
    pub fn check_variant(label: &str, key: &ShaderKey, effects: &EffectRegistry) {
        validate(&format!("{label} {key:?}"), &shader_source(key, effects));
        translate_variant(label, key, effects);
    }

    /// Check effect `id` for both builds in the variants its materials use: basic
    /// and lit, with and without an alpha test, and the shadow pass.
    pub fn validate_effect(effects: &EffectRegistry, id: u16) {
        let effect = effects.get(id).expect("a registered effect");
        for (lit, alpha_test) in [(false, false), (true, false), (true, true), (false, true)] {
            let key = ShaderKey {
                pass: Pass::Main,
                lit,
                alpha_test,
                receive_shadow: lit,
                effect: id,
                ..ShaderKey::default()
            };
            check_variant(effect.name, &key, effects);
        }
        for alpha_test in [false, true] {
            let shadow = ShaderKey {
                pass: Pass::Shadow,
                alpha_test,
                shadow_fade: effect.shadow_fade,
                effect: id,
                ..ShaderKey::default()
            };
            check_variant(effect.name, &shadow, effects);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::webgl_check::{check_variant, translate_for_webgl, validate};
    use super::*;

    #[test]
    fn every_variant_is_valid_wgsl_and_translates_for_webgl() {
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
                    shadow_fade: false,
                };
                check_variant("surface", &key, &effects);
                // Lighting and shadow reception (bits 0 and 3) leave a caster unchanged.
                if !bit(0) && !bit(3) {
                    let shadow = ShaderKey {
                        pass: Pass::Shadow,
                        alpha_test: bit(1),
                        shadow_fade: bit(2),
                        effect,
                        ..ShaderKey::default()
                    };
                    check_variant("shadow", &shadow, &effects);
                }
            }
        }
        translate_for_webgl("water", &fixed_source(WATER_WGSL, texture));
        translate_for_webgl("shadow merged", &fixed_source(SHADOW_MERGED_WGSL, texture));
        translate_for_webgl("shadow cutout", &fixed_source(SHADOW_CUTOUT_WGSL, texture));
        translate_for_webgl("output", OUTPUT_WGSL);
        translate_for_webgl("mipmap", MIPMAP_WGSL);
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
    fn instance_texture_rows_match_the_store() {
        let declarations = [
            format!(
                "const RECORDS_PER_ROW: u32 = {}u;",
                crate::draw_list::RECORDS_PER_ROW
            ),
            format!(
                "const RECORD_TEXELS: u32 = {}u;",
                crate::draw_list::RECORD_TEXELS
            ),
        ];
        for declaration in declarations {
            assert!(InstanceSource::Texture.wgsl().contains(&declaration));
        }
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
            &ShaderKey::shadow(&Material::default(), 0, false, &effects),
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
            ShaderKey::main(&plain, 0, true),
            ShaderKey::main(&featured, 0, true)
        );
        let features = MaterialFeatures::of(&featured);
        for feature in [
            MaterialFeatures::MAP,
            MaterialFeatures::EMISSIVE_MAP,
            MaterialFeatures::BUMP,
            MaterialFeatures::VERTEX_COLORS,
            MaterialFeatures::FLAT_SHADING,
        ] {
            assert!(features.0 & feature != 0);
        }
        assert!(features.0 & MaterialFeatures::FOG == 0);
        assert!(MaterialFeatures::of(&plain).0 & MaterialFeatures::FOG != 0);
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
        assert!(features.0 & MaterialFeatures::EMISSIVE_MAP == 0);
        assert!(features.0 & MaterialFeatures::BUMP == 0);
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
    fn material_uniform_matches_the_wgsl_layout() {
        use std::mem::offset_of;

        let code = shader_source(&ShaderKey::default(), &EffectRegistry::default());
        let module = naga::front::wgsl::parse_str(&code).expect("valid WGSL");
        let (members, span) = module
            .types
            .iter()
            .find_map(|(_, ty)| match &ty.inner {
                naga::TypeInner::Struct { members, span }
                    if ty.name.as_deref() == Some("MaterialUniform") =>
                {
                    Some((members, *span))
                }
                _ => None,
            })
            .expect("the surface shader declares MaterialUniform");
        let wgsl: Vec<(&str, usize)> = members
            .iter()
            .map(|member| (member.name.as_deref().unwrap_or(""), member.offset as usize))
            .collect();
        let rust = [
            ("color", offset_of!(MaterialUniform, color)),
            ("emissive", offset_of!(MaterialUniform, emissive)),
            ("surface", offset_of!(MaterialUniform, surface)),
            ("map_transform", offset_of!(MaterialUniform, map_transform)),
            (
                "bump_transform",
                offset_of!(MaterialUniform, bump_transform),
            ),
            (
                "emissive_transform",
                offset_of!(MaterialUniform, emissive_transform),
            ),
            ("params", offset_of!(MaterialUniform, params)),
            ("features", offset_of!(MaterialUniform, features)),
        ];
        assert_eq!(wgsl, rust);
        assert_eq!(span as usize, size_of::<MaterialUniform>());
        // The pools' per-frame effect clocks land where the effects read `params`.
        let (_, params) = wgsl.iter().find(|(name, _)| *name == "params").unwrap();
        assert_eq!(MATERIAL_PARAMS_OFFSET, *params as u64);
    }

    #[test]
    fn shadow_keys_collapse_irrelevant_state() {
        let effects = EffectRegistry::default();
        let red = Material::standard(0xff0000, 0.1, 0.5);
        let a = ShaderKey::shadow(&red, 0, false, &effects);
        let green = Material {
            flat_shading: true,
            fog: false,
            side: Side::Double,
            ..Material::basic(0x00ff00)
        };
        let b = ShaderKey::shadow(&green, 0, false, &effects);
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
        assert_eq!(ShaderKey::shadow(&solid, pulse, false, &effects).effect, 0);
        assert_eq!(ShaderKey::shadow(&solid, pulse, true, &effects).effect, 0);
        assert_eq!(
            ShaderKey::shadow(&cutout, pulse, false, &effects).effect,
            pulse
        );
        assert_eq!(
            ShaderKey::shadow(&solid, wave, false, &effects).effect,
            wave
        );
    }

    #[test]
    fn polygon_offset_biases_only_the_main_pass() {
        let drift = Material {
            polygon_offset: Some((-1.0, -1.0)),
            ..Material::default()
        };
        let main = PipelineKey::main(ShaderKey::main(&drift, 0, true), &drift, false);
        assert_eq!(main.depth_bias.constant, -1);
        assert_eq!(main.depth_bias.slope_scale(), -1.0);
        let effects = EffectRegistry::default();
        let shadow = PipelineKey::shadow(ShaderKey::shadow(&drift, 0, false, &effects), &drift);
        assert_eq!(shadow.depth_bias, DepthBias::default());
        let plain = PipelineKey::main(
            ShaderKey::main(&Material::default(), 0, true),
            &Material::default(),
            false,
        );
        assert_ne!(plain, main);
    }
}
