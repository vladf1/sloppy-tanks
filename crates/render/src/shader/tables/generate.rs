//! Generates the shader tables (`tables.rs`): run by the `shader-tables` binary before
//! every page build, and by the tests that pin the tables to the specializer.

use std::collections::{BTreeMap, HashMap};

use serde_json::{Value, json};
use sloppy_core::scene::Material;

use super::{FORMAT, SOURCES, fixed_key, variant_key};
use crate::effects::EffectRegistry;
use crate::shader::assembly::{
    fixed_shader_source, fixed_stages_for, shader_source_for, stages_for,
};
use crate::shader::{FixedShader, InstanceSource, Pass, ShaderKey, glsl};

/// A page build's graphics backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    WebGpu,
    WebGl,
}

impl Backend {
    pub const ALL: [Self; 2] = [Self::WebGpu, Self::WebGl];

    /// Its tables' file name without the extension, and their `backend`.
    pub fn name(self) -> &'static str {
        match self {
            Self::WebGpu => "webgpu",
            Self::WebGl => "webgl",
        }
    }

    fn instances(self) -> InstanceSource {
        match self {
            Self::WebGpu => InstanceSource::Storage,
            Self::WebGl => InstanceSource::Texture,
        }
    }
}

/// The registry the page draws with: the renderer's own effects, then those
/// presentation registers (`Presentation::new`). The page checks its own against the
/// tables' fingerprint.
pub fn page_registry() -> EffectRegistry {
    let mut effects = EffectRegistry::default();
    for effect in crate::presentation::PRESENTATION_EFFECTS {
        effects.register(effect);
    }
    effects
}

/// Every key `Renderer::class` can build on `backend` with `effects`: each effect (or
/// none) lit or not, plain, alpha-tested or alpha to coverage, receiving the sun's
/// shadow on WebGL's lit variants, and every caster `ShaderKey::shadow` gives.
pub fn variant_keys(effects: &EffectRegistry, backend: Backend) -> Vec<ShaderKey> {
    let count = (1..).take_while(|&id| effects.get(id).is_some()).count() as u16;
    let mut keys = Vec::new();
    for effect in 0..=count {
        for lit in [false, true] {
            for (alpha_test, alpha_to_coverage) in [(false, false), (true, false), (true, true)] {
                for receive_shadow in [false, true] {
                    if receive_shadow && !(lit && backend == Backend::WebGl) {
                        continue;
                    }
                    keys.push(ShaderKey {
                        pass: Pass::Main,
                        lit,
                        alpha_test,
                        alpha_to_coverage,
                        effect,
                        shadow_fade: false,
                        receive_shadow,
                    });
                }
            }
        }
        for alpha_test in [false, true] {
            for fade in [false, true] {
                let material = Material {
                    alpha_test: if alpha_test { 0.5 } else { 0.0 },
                    ..Material::default()
                };
                let key = ShaderKey::shadow(&material, effect, fade, effects);
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
        }
    }
    keys
}

/// Every fixed shader stage pair `backend` compiles: `webgpu/pipelines.rs` and
/// `webgpu/textures.rs`, or `webgl/programs.rs` and `webgl/textures.rs`.
pub fn fixed_stage_list(
    backend: Backend,
) -> Vec<(FixedShader, &'static str, Option<&'static str>)> {
    let mut list = vec![
        (FixedShader::Output, "vs_fullscreen", Some("fs_output")),
        (FixedShader::Output, "vs_post", Some("fs_bloom")),
        (FixedShader::Sky, "vs_sky", Some("fs_sky")),
        (FixedShader::Occlusion, "vs_occlusion", Some("fs_occlusion")),
        (FixedShader::ShadowMerged, "vs_shadow_merged", None),
        (FixedShader::Water, "vs_water", Some("fs_water")),
        (FixedShader::Mipmap, "vs_blit", Some("fs_blit")),
    ];
    match backend {
        // The merged cutouts draw with the per-part casters' fragment stage, and the
        // occlusion's depth pass draws the water too.
        Backend::WebGpu => list.extend([
            (
                FixedShader::ShadowCutout,
                "vs_shadow_cutout",
                Some("fs_shadow"),
            ),
            (FixedShader::Water, "vs_water", None),
        ]),
        Backend::WebGl => list.push((
            FixedShader::ShadowCutout,
            "vs_shadow_cutout",
            Some("fs_shadow_cutout"),
        )),
    }
    for bake in &crate::effects::bakes::BAKES {
        list.push((FixedShader::Bake(bake.key), "vs_bake", Some("fs_bake")));
    }
    list
}

/// One program as the tables hold it: its stages' text (WGSL for WebGPU, GLSL ES for
/// WebGL) and, for WebGL, the uniform blocks and samplers to bind by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub program: glsl::Program,
}

/// Every program `backend` can ask for, as the specializer and naga give them.
pub fn entries(backend: Backend) -> Vec<Entry> {
    let effects = page_registry();
    let source = backend.instances();
    let mut entries = Vec::new();
    for key in variant_keys(&effects, backend) {
        let (vertex, fragment) = key.entry_points();
        let program = match backend {
            Backend::WebGpu => wgsl_program(stages_for(&key, &effects, source)),
            Backend::WebGl => {
                let code = shader_source_for(&key, &effects, source);
                translate(&variant_key(&key), &code, vertex, fragment)
            }
        };
        entries.push(Entry {
            key: variant_key(&key),
            program,
        });
    }
    for (shader, vertex, fragment) in fixed_stage_list(backend) {
        let key = fixed_key(shader, vertex, fragment);
        let program = match backend {
            Backend::WebGpu => wgsl_program(fixed_stages_for(shader, vertex, fragment, source)),
            Backend::WebGl => {
                translate(&key, &fixed_shader_source(shader, source), vertex, fragment)
            }
        };
        entries.push(Entry { key, program });
    }
    entries
}

fn wgsl_program(stages: crate::shader::StageSources) -> glsl::Program {
    glsl::Program {
        vertex: stages.vertex,
        fragment: stages.fragment,
        ..glsl::Program::default()
    }
}

/// The WebGL program of `code`'s `vertex` and `fragment` entry points.
fn translate(label: &str, code: &str, vertex: &str, fragment: Option<&str>) -> glsl::Program {
    let entries: Vec<&str> = std::iter::once(vertex).chain(fragment).collect();
    let stages = glsl::translate(label, code, &entries).unwrap_or_else(|error| panic!("{error}"));
    let mut program = glsl::Program::default();
    for (index, stage) in stages.into_iter().enumerate() {
        program.blocks.extend(stage.blocks);
        program.samplers.extend(stage.samplers);
        if index == 0 {
            program.vertex = stage.source;
        } else {
            program.fragment = Some(stage.source);
        }
    }
    program
}

/// The tables of `backend`, as `index.html` fetches them:
///
/// ```json
/// {
///   "format": 1, "backend": "webgpu", "sources": "…", "registry": "…",
///   "lines": ["line", …],
///   "texts": [[3, [10, 42], 7, …], …],
///   "stages": { "main:la:4": [vertex, fragment], … }
/// }
/// ```
///
/// A text is its lines joined by `\n`, each run of them a line index or `[first,
/// count]` of consecutive lines. A stage entry names its vertex and fragment texts (-1
/// for none) and, on WebGL, its uniform blocks and samplers as `[name, group,
/// binding]`.
pub fn tables_json(backend: Backend) -> String {
    let registry = format!("{:016x}", page_registry().fingerprint());
    encode(backend, &entries(backend), &registry)
}

fn encode(backend: Backend, entries: &[Entry], registry: &str) -> String {
    let mut texts = Texts::default();
    let mut stages = BTreeMap::new();
    for entry in entries {
        let vertex = texts.index(&entry.program.vertex);
        let fragment = entry
            .program
            .fragment
            .as_deref()
            .map_or(-1, |fragment| texts.index(fragment));
        let mut value = vec![json!(vertex), json!(fragment)];
        if backend == Backend::WebGl {
            let bindings = |list: &[(String, glsl::Binding)]| -> Value {
                list.iter()
                    .map(|(name, (group, binding))| json!([name, group, binding]))
                    .collect()
            };
            value.push(bindings(&entry.program.blocks));
            value.push(bindings(&entry.program.samplers));
        }
        stages.insert(entry.key.clone(), Value::Array(value));
    }
    json!({
        "format": FORMAT,
        "backend": backend.name(),
        "sources": SOURCES,
        "registry": registry,
        "lines": texts.lines,
        "texts": texts.runs,
        "stages": stages,
    })
    .to_string()
}

/// Each distinct text once, as runs of each distinct line once.
#[derive(Default)]
struct Texts {
    /// Text index by text.
    texts: HashMap<String, i64>,
    /// Each text's runs of line indices: a lone line's index, or `[first, count]`.
    runs: Vec<Value>,
    lines: Vec<String>,
    line_index: HashMap<String, usize>,
}

impl Texts {
    fn index(&mut self, text: &str) -> i64 {
        if let Some(&index) = self.texts.get(text) {
            return index;
        }
        let ids: Vec<usize> = text.split('\n').map(|line| self.line(line)).collect();
        let mut runs = Vec::new();
        let mut start = 0;
        while start < ids.len() {
            let mut end = start + 1;
            while end < ids.len() && ids[end] == ids[end - 1] + 1 {
                end += 1;
            }
            runs.push(if end - start == 1 {
                json!(ids[start])
            } else {
                json!([ids[start], end - start])
            });
            start = end;
        }
        let index = self.runs.len() as i64;
        self.runs.push(Value::Array(runs));
        self.texts.insert(text.to_owned(), index);
        index
    }

    fn line(&mut self, line: &str) -> usize {
        if let Some(&id) = self.line_index.get(line) {
            return id;
        }
        self.lines.push(line.to_owned());
        self.line_index
            .insert(line.to_owned(), self.lines.len() - 1);
        self.lines.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shader::webgl_check::assert_no_loop;

    /// The page's reading of the tables (`tables.js` `shaderText` and `page.rs`).
    fn decode(json: &str) -> (Value, HashMap<String, Entry>) {
        let tables: Value = serde_json::from_str(json).expect("valid JSON");
        let lines: Vec<&str> = tables["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|line| line.as_str().unwrap())
            .collect();
        let text = |index: i64| -> Option<String> {
            let runs = tables["texts"].get(usize::try_from(index).ok()?)?;
            let mut out = Vec::new();
            for run in runs.as_array().unwrap() {
                match run {
                    Value::Number(line) => out.push(lines[line.as_u64().unwrap() as usize]),
                    Value::Array(pair) => {
                        let first = pair[0].as_u64().unwrap() as usize;
                        let count = pair[1].as_u64().unwrap() as usize;
                        out.extend(&lines[first..first + count]);
                    }
                    _ => panic!("a run is a line or a pair"),
                }
            }
            Some(out.join("\n"))
        };
        let bindings = |value: Option<&Value>| -> Vec<(String, glsl::Binding)> {
            value.map_or_else(Vec::new, |list| {
                list.as_array()
                    .unwrap()
                    .iter()
                    .map(|b| {
                        let number = |i: usize| b[i].as_u64().unwrap() as u32;
                        (b[0].as_str().unwrap().to_owned(), (number(1), number(2)))
                    })
                    .collect()
            })
        };
        let entries = tables["stages"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, stage)| {
                let program = glsl::Program {
                    vertex: text(stage[0].as_i64().unwrap()).expect("a vertex stage"),
                    fragment: text(stage[1].as_i64().unwrap()),
                    blocks: bindings(stage.get(2)),
                    samplers: bindings(stage.get(3)),
                };
                let entry = Entry {
                    key: key.clone(),
                    program,
                };
                (key.clone(), entry)
            })
            .collect();
        (tables, entries)
    }

    /// The tables hold exactly what the specializer gives each key, byte for byte: so
    /// the stages WebKit compiles once for many variants stay shared, and no loop
    /// returns. The WebGPU stages are checked against the specializer's own call
    /// (`StageSources::variant` through `stages_for`).
    #[test]
    fn generated_tables_match_the_specializer() {
        let effects = page_registry();
        for backend in Backend::ALL {
            let entries = entries(backend);
            let registry = format!("{:016x}", effects.fingerprint());
            let json = encode(backend, &entries, &registry);
            let (tables, decoded) = decode(&json);
            assert_eq!(tables["format"], FORMAT);
            assert_eq!(tables["backend"], backend.name());
            assert_eq!(tables["sources"], SOURCES);
            assert_eq!(tables["registry"], registry.as_str());
            assert_eq!(decoded.len(), entries.len(), "every key once");
            for entry in &entries {
                assert_eq!(decoded.get(&entry.key), Some(entry), "{}", entry.key);
                if backend == Backend::WebGpu {
                    assert_no_loop(&entry.key, &entry.program.vertex);
                    assert_no_loop(&entry.key, entry.program.fragment.as_deref().unwrap_or(""));
                }
            }
            for key in variant_keys(&effects, backend) {
                let entry = &decoded[&variant_key(&key)];
                if backend == Backend::WebGpu {
                    let stages = stages_for(&key, &effects, InstanceSource::Storage);
                    assert_eq!(entry.program.vertex, stages.vertex, "{key:?}");
                    assert_eq!(entry.program.fragment, stages.fragment, "{key:?}");
                } else {
                    // GLSL keeps no loop where the WGSL had none.
                    for stage in [Some(&entry.program.vertex), entry.program.fragment.as_ref()]
                        .into_iter()
                        .flatten()
                    {
                        assert!(
                            !stage.contains("for (") && !stage.contains("while"),
                            "{key:?}"
                        );
                    }
                }
            }
        }
    }

    /// Every key a part can have on either backend is in the tables: the materials'
    /// keys, the WebGL build's shadow reception and the casters' collapsed effects.
    #[test]
    fn the_tables_cover_every_key_a_material_gives() {
        let effects = page_registry();
        let cutout = Material {
            alpha_test: 0.5,
            alpha_to_coverage: true,
            ..Material::standard(0x808080, 0.5, 0.0)
        };
        let basic = Material::basic(0xffffff);
        for backend in Backend::ALL {
            let keys = variant_keys(&effects, backend);
            for effect in 0..=keys.iter().map(|key| key.effect).max().unwrap() {
                for material in [&cutout, &basic] {
                    let main = ShaderKey::main(material, effect);
                    assert!(keys.contains(&main), "{main:?}");
                    for fade in [false, true] {
                        let shadow = ShaderKey::shadow(material, effect, fade, &effects);
                        assert!(keys.contains(&shadow), "{shadow:?}");
                    }
                }
            }
            let received = ShaderKey {
                receive_shadow: true,
                ..ShaderKey::main(&cutout, 0)
            };
            assert_eq!(keys.contains(&received), backend == Backend::WebGl);
        }
    }

    /// The tables keep each line once: the closed WebGPU set holds well under the
    /// million bytes of its texts one after another.
    #[test]
    fn tables_store_lines_once() {
        let json = tables_json(Backend::WebGpu);
        let (tables, _) = decode(&json);
        let texts = tables["texts"].as_array().unwrap().len();
        assert!(texts > 100, "{texts} texts");
        assert!(json.len() < 1_000_000, "{} bytes", json.len());
    }
}
