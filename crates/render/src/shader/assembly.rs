//! Assembles each shader variant from the WGSL templates: the variant constants of its
//! key and effect, its effect's hooks and the shared hooks of the others, reduced to
//! the code its flags enable and split into one module per stage (`specialize.rs`).
//!
//! The desktop apps, the engine build and the native tests run this whenever a
//! pipeline first asks for a variant. The page's shell does not carry it: the build
//! runs it for every variant either backend can ask for and the page reads the result
//! (`tables.rs`), the same text byte for byte.

use super::specialize;
use super::{FixedShader, InstanceSource, Pass, ShaderKey, StageSources, occlusion_scale};
use crate::effects::{EffectDefinition, EffectRegistry};

pub const COMMON_WGSL: &str = wgsl!("shaders/common.wgsl");
pub const MATERIAL_WGSL: &str = wgsl!("shaders/material.wgsl");
pub const STANDARD_WGSL: &str = wgsl!(
    "shaders/surface_io.wgsl",
    "shaders/surface_vertex.wgsl",
    "shaders/standard.wgsl"
);
pub const SHADOW_WGSL: &str = wgsl!(
    "shaders/surface_io.wgsl",
    "shaders/surface_vertex.wgsl",
    "shaders/shadow.wgsl"
);
pub const WATER_WGSL: &str = wgsl!("shaders/water.wgsl");
pub const OUTPUT_WGSL: &str = wgsl!("shaders/output.wgsl");
pub const SKY_WGSL: &str = wgsl!("shaders/sky.wgsl");
/// Screen-space ambient occlusion, without its depth binding (`occlusion_source`).
pub const OCCLUSION_WGSL: &str = wgsl!("shaders/occlusion.wgsl");
pub const MIPMAP_WGSL: &str = wgsl!("shaders/mipmap.wgsl");
pub const SHADOW_MERGED_WGSL: &str = wgsl!("shaders/shadow_merged.wgsl");
pub const SHADOW_CUTOUT_WGSL: &str = wgsl!("shaders/surface_io.wgsl", "shaders/shadow_cutout.wgsl");

impl InstanceSource {
    pub(crate) fn wgsl(self) -> &'static str {
        match self {
            Self::Storage => wgsl!("shaders/instances_storage.wgsl"),
            Self::Texture => wgsl!("shaders/instances_texture.wgsl"),
        }
    }
}

/// The frame group's bindings of every shader but the output pass.
const FRAME_BINDINGS_WGSL: &str = wgsl!("shaders/frame_bindings.wgsl");

/// `LITE`: true on the WebGL2 build (the texture source), whose weaker devices get
/// cheaper shadow filters, sky noise and occlusion. Folded like a variant flag
/// (`lite_flag`), so each build's shaders carry only their own filter.
fn lite_wgsl(source: InstanceSource) -> String {
    let (name, lite) = lite_flag(source);
    format!("const {name}: bool = {lite};\n")
}

fn lite_flag(source: InstanceSource) -> (&'static str, bool) {
    ("LITE", source == InstanceSource::Texture)
}

/// The frame declarations and bindings with this build's instance records.
fn frame_wgsl(source: InstanceSource) -> String {
    format!(
        "{}{COMMON_WGSL}{FRAME_BINDINGS_WGSL}{}",
        lite_wgsl(source),
        source.wgsl()
    )
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

/// The variant constants the templates branch on. The shadow pass's fragment stage
/// tests the alpha and dithers a fading caster at run time (`shadow.wgsl`), so every
/// caster with one shares it, except where `SHADOW_SURFACE`: an alpha-tested caster
/// whose effect shades the surface that the test reads.
fn flags(key: &ShaderKey, effect: Option<&EffectDefinition>) -> [(&'static str, bool); 6] {
    [
        ("LIT", key.lit),
        ("ALPHA_TEST", key.alpha_test),
        ("ALPHA_TO_COVERAGE", key.alpha_to_coverage),
        ("SHADOW_FADE", key.shadow_fade),
        ("RECEIVE_SHADOW", key.receive_shadow),
        (
            "SHADOW_SURFACE",
            key.pass == Pass::Shadow && key.alpha_test && effect.is_some_and(|e| e.has_surface()),
        ),
    ]
}

/// The complete WGSL for a surface or shadow variant, reduced to the code its flags
/// enable (`specialize.rs`).
pub fn shader_source(key: &ShaderKey, effects: &EffectRegistry) -> String {
    shader_source_for(key, effects, InstanceSource::BUILD)
}

pub(crate) fn shader_source_for(
    key: &ShaderKey,
    effects: &EffectRegistry,
    source: InstanceSource,
) -> String {
    let effect = effects.get(key.effect);
    let flags = flags(key, effect);
    let mut code = String::with_capacity(24 * 1024);
    for (name, value) in flags {
        code += &format!("const {name}: bool = {value};\n");
    }
    code += VERTEX_INPUT_WGSL;
    code += &frame_wgsl(source);
    code += MATERIAL_WGSL;
    if let Some(effect) = effect {
        code += effect.wgsl;
        code += "\n";
        code += effect.entry;
    }
    code += &vertex_hooks(effect, effects);
    code += &surface_hook(key, effect, effects, source);
    code += match key.pass {
        Pass::Main => STANDARD_WGSL,
        Pass::Shadow => SHADOW_WGSL,
    };
    let mut folded = flags.to_vec();
    folded.push(lite_flag(source));
    specialize::specialize(&code, &folded)
}

/// The vertex-stage hooks: name, parameters and the arguments that pass them on.
const VERTEX_HOOKS: [(&str, &str, &str); 3] = [
    ("effect_vertex", "v: ptr<function, EffectVertex>", "v"),
    (
        "effect_world",
        "w: ptr<function, EffectWorld>, v: EffectVertex",
        "w, v",
    ),
    (
        "effect_clip",
        "clip: ptr<function, vec4f>, w: EffectWorld, v: EffectVertex",
        "clip, w, v",
    ),
];

/// A hook `name(params)` that runs each of `branches` (none: a no-op).
fn dispatch_hook(name: &str, params: &str, branches: &[String]) -> String {
    if branches.is_empty() {
        return format!("fn {name}({params}) {{}}\n");
    }
    format!(
        "fn {name}({params}) {{\n    let effect = material.features.y;\n    {}\n}}\n",
        branches.join(" else ")
    )
}

/// The variant's `effect_vertex`, `effect_world` and `effect_clip` that its effect
/// does not define itself. A variant of an effect with shared vertex hooks
/// (`EffectDefinition::shared_vertex`) takes every one of them, each run where the
/// material names its effect (`MaterialUniform::features`), so all those effects'
/// surfaces have one vertex stage and their casters another. Every other variant
/// takes none: the material read and the branches cost WebKit about 1% of the main
/// view's GPU time when every plain draw ran them.
fn vertex_hooks(effect: Option<&EffectDefinition>, effects: &EffectRegistry) -> String {
    let mut code = String::new();
    let mut branches: [Vec<String>; 3] = Default::default();
    // An effect's own vertex hooks never sit beside shared ones (`registry.rs`).
    let own = effect
        .filter(|e| e.has_own_vertex_hooks())
        .map(|e| [e.has_vertex(), e.has_world(), e.has_clip()]);
    if own.is_none() && effect.is_some_and(|e| e.has_shared_vertex()) {
        for (id, shared) in effects.sharing_hooks() {
            let mut hooks = shared.shared_vertex.to_string();
            for ((hook, _, args), branches) in VERTEX_HOOKS.iter().zip(&mut branches) {
                let name = format!("shared_{}_{id}", &hook["effect_".len()..]);
                let definition = format!("fn {hook}(");
                if hooks.contains(&definition) {
                    hooks = hooks.replace(&definition, &format!("fn {name}("));
                    branches.push(format!(
                        "if effect == {id}u {{\n        {name}({args});\n    }}"
                    ));
                }
            }
            code += &hooks;
            code += "\n";
        }
    }
    let defined = own.unwrap_or_default();
    for (((hook, params, _), branches), defined) in VERTEX_HOOKS.iter().zip(&branches).zip(defined)
    {
        if !defined {
            code += &dispatch_hook(hook, params, branches);
        }
    }
    code
}

/// The variant's `effect_surface` unless its effect defines one of its own. A lit
/// WebGPU surface variant takes every lit shared hook
/// (`EffectDefinition::shared_surface`), each run where the material names its effect
/// (`MaterialUniform::features`), so the lit materials of those effects and of none
/// draw with the same fragment stage; an unlit variant of an effect with an unlit
/// shared hook (`shared_unlit`) takes every such hook, so those effects share one
/// stage; any other variant takes its effect's shared hook as its own. The WebGL
/// build shares none: its translation paid for the branches in every lit pixel (1% of
/// a Tesla-size village frame), and its weaker devices count frames, not compiles.
fn surface_hook(
    key: &ShaderKey,
    effect: Option<&EffectDefinition>,
    effects: &EffectRegistry,
    source: InstanceSource,
) -> String {
    const NONE: &str = "fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {}\n";
    if effect.is_some_and(|e| e.has_own_surface()) {
        return String::new();
    }
    // A variant whose effect shares its hook with the other kind keeps it as its own,
    // and plain unlit variants none: the branches changed the bits of their blended
    // pixels, which the output's haze test turned into visible steps.
    let dispatches = if key.lit {
        effect.is_none_or(|e| !e.has_shared_surface() || e.shares_surface_with(true))
    } else {
        effect.is_some_and(|e| e.shares_surface_with(false))
    };
    if key.pass == Pass::Main && dispatches && source == InstanceSource::Storage {
        let mut hooks = String::new();
        let mut branches = Vec::new();
        for (id, shared) in effects.sharing_hooks() {
            if shared.shares_surface_with(key.lit) {
                let name = format!("shared_surface_{id}");
                hooks += &shared
                    .shared_surface
                    .replace("fn effect_surface(", &format!("fn {name}("));
                hooks += "\n";
                branches.push(format!(
                    "if effect == {id}u {{\n        {name}(s, f);\n    }}"
                ));
            }
        }
        let params = "s: ptr<function, Surface>, f: EffectFragment";
        return hooks + &dispatch_hook("effect_surface", params, &branches);
    }
    match effect.filter(|e| e.has_shared_surface()) {
        Some(shared) => format!("{}\n", shared.shared_surface),
        None => NONE.into(),
    }
}

impl StageSources {
    /// The stages of `code` for `vertex_entry` and, if any, `fragment_entry`.
    pub fn of(code: &str, vertex_entry: &str, fragment_entry: Option<&str>) -> Self {
        let entries: Vec<&str> = std::iter::once(vertex_entry)
            .chain(fragment_entry)
            .collect();
        let mut stages = specialize::entry_point_sources(code, &entries).into_iter();
        Self {
            vertex: stages.next().expect("a vertex stage"),
            fragment: stages.next(),
        }
    }

    /// A surface or shadow variant's stages.
    pub fn variant(key: &ShaderKey, effects: &EffectRegistry) -> Self {
        let (vertex, fragment) = key.entry_points();
        Self::of(&shader_source(key, effects), vertex, fragment)
    }
}

/// The merged shadow-caster shader (`shadow_merge.rs`).
pub fn shadow_merged_source() -> String {
    fixed_source(SHADOW_MERGED_WGSL, InstanceSource::BUILD)
}

/// The merged alpha-tested shadow-caster shader.
pub fn shadow_cutout_source() -> String {
    fixed_source(SHADOW_CUTOUT_WGSL, InstanceSource::BUILD)
}

/// The WebGPU stages of the merged alpha-tested casters: their own vertex stage and
/// the fragment stage every per-part caster with one shares (`shadow.wgsl`), the
/// same text as theirs, so WebKit compiles it once for both.
pub fn shadow_cutout_stages() -> StageSources {
    shadow_cutout_stages_for(InstanceSource::BUILD)
}

fn shadow_cutout_stages_for(source: InstanceSource) -> StageSources {
    let caster = ShaderKey {
        pass: Pass::Shadow,
        alpha_test: true,
        ..ShaderKey::default()
    };
    let cutout = fixed_source(SHADOW_CUTOUT_WGSL, source);
    StageSources {
        vertex: specialize::entry_point_source(&cutout, "vs_shadow_cutout"),
        fragment: stages_for(&caster, &EffectRegistry::default(), source).fragment,
    }
}

/// The water shader: frame declarations plus the water template.
pub fn water_source() -> String {
    fixed_source(WATER_WGSL, InstanceSource::BUILD)
}

/// The sky shader: frame declarations plus the sky template.
pub fn sky_source() -> String {
    fixed_source(SKY_WGSL, InstanceSource::BUILD)
}

/// The ambient occlusion shader, `scale` depth texels per occlusion texel along each
/// axis. WebGPU reads the depth texture of its own depth pass (`depth_pass`); WebGL2
/// reads the full-size single-sample copy its resolve blits, as floats, and probes six
/// points rather than eight on its weaker devices.
pub fn occlusion_source(depth_pass: bool, scale: u32) -> String {
    let depth = if depth_pass {
        "@group(0) @binding(0) var depth: texture_depth_2d;\n\
         fn load_depth(pixel: vec2i) -> f32 { return textureLoad(depth, pixel, 0); }\n\
         const SAMPLES: i32 = 8;\n"
    } else {
        "@group(0) @binding(0) var depth: texture_2d<f32>;\n\
         fn load_depth(pixel: vec2i) -> f32 { return textureLoad(depth, pixel, 0).r; }\n\
         const SAMPLES: i32 = 6;\n"
    };
    let scale = format!("const OCCLUSION_SCALE: i32 = {scale};\n");
    specialize::specialize(&format!("{depth}{scale}{OCCLUSION_WGSL}"), &[])
}

/// The full-screen triangle every texture bake draws (`effects::bakes`), and the uv
/// its fragment stage shades: 0..1 across the target, v increasing down its rows.
const BAKE_WGSL: &str = "struct BakeOut {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
}
@vertex
fn vs_bake(@builtin(vertex_index) index: u32) -> BakeOut {
    let uv = vec2f(f32((index << 1u) & 2u), f32(index & 2u));
    var out: BakeOut;
    out.position = vec4f(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}
";

/// A texture bake's complete WGSL from the code defining its `fs_bake`, reduced to
/// what its entry points reach.
pub fn bake_source(fragment: &str) -> String {
    specialize::specialize(&format!("{BAKE_WGSL}{fragment}"), &[])
}

/// The same for a bake that reads a material (`TextureBake::source`): with a surface
/// shader's frame and material declarations ahead of its code.
pub fn bake_source_with_material(fragment: &str, instances: InstanceSource) -> String {
    let code = format!(
        "{VERTEX_INPUT_WGSL}{}{MATERIAL_WGSL}{BAKE_WGSL}{fragment}",
        frame_wgsl(instances)
    );
    specialize::specialize(&code, &[lite_flag(instances)])
}

/// The output and bloom shader, its loops unrolled like every other shader's. The
/// output pass reads the main view's `Frame` for the haze it adds (`haze_inscatter`).
pub fn output_source() -> String {
    output_source_for(InstanceSource::BUILD)
}

pub(crate) fn output_source_for(source: InstanceSource) -> String {
    specialize::specialize(
        &format!("{}{COMMON_WGSL}{OUTPUT_WGSL}", lite_wgsl(source)),
        &[lite_flag(source)],
    )
}

pub(crate) fn fixed_source(template: &str, source: InstanceSource) -> String {
    specialize::specialize(
        &format!("{}{template}", frame_wgsl(source)),
        &[lite_flag(source)],
    )
}

/// A surface or shadow variant's stages for this build's backend.
pub fn variant_stages(key: &ShaderKey, effects: &EffectRegistry) -> StageSources {
    StageSources::variant(key, effects)
}

/// A surface or shadow variant's stages as the backend that reads instance records
/// from `source` compiles them.
pub fn stages_for(
    key: &ShaderKey,
    effects: &EffectRegistry,
    source: InstanceSource,
) -> StageSources {
    let (vertex, fragment) = key.entry_points();
    StageSources::of(&shader_source_for(key, effects, source), vertex, fragment)
}

/// The complete WGSL of a fixed shader for the backend that reads instance records
/// from `source`.
pub fn fixed_shader_source(shader: FixedShader, source: InstanceSource) -> String {
    match shader {
        FixedShader::Output => output_source_for(source),
        FixedShader::Sky => fixed_source(SKY_WGSL, source),
        FixedShader::Occlusion => {
            occlusion_source(source == InstanceSource::Storage, occlusion_scale(source))
        }
        FixedShader::ShadowMerged => fixed_source(SHADOW_MERGED_WGSL, source),
        FixedShader::ShadowCutout => fixed_source(SHADOW_CUTOUT_WGSL, source),
        FixedShader::Water => fixed_source(WATER_WGSL, source),
        FixedShader::Mipmap => MIPMAP_WGSL.to_owned(),
        FixedShader::Bake(key) => crate::effects::bakes::bake_for(key)
            .unwrap_or_else(|| panic!("no texture bake {key}"))
            .source_for(source),
    }
}

/// A fixed shader's `vertex` and `fragment` stages for this build's backend.
pub fn fixed_stages(shader: FixedShader, vertex: &str, fragment: Option<&str>) -> StageSources {
    fixed_stages_for(shader, vertex, fragment, InstanceSource::BUILD)
}

/// The same for the backend that reads instance records from `source`. The merged
/// alpha-tested casters' `fs_shadow` is the per-part casters' fragment stage
/// (`shadow_cutout_stages`).
pub fn fixed_stages_for(
    shader: FixedShader,
    vertex: &str,
    fragment: Option<&str>,
    source: InstanceSource,
) -> StageSources {
    if shader == FixedShader::ShadowCutout && fragment == Some("fs_shadow") {
        return shadow_cutout_stages_for(source);
    }
    StageSources::of(&fixed_shader_source(shader, source), vertex, fragment)
}
