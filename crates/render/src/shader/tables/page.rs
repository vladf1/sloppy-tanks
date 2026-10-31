//! The page's shader stages, read from the tables `index.html` fetched beside the
//! binary (`tables.rs`). A page whose tables are missing, come from other sources or
//! another registry, or lack a key stops with an error that says so: it has no
//! specializer to fall back on, and each of these means a broken build.

use std::cell::OnceCell;

use js_sys::{Array, Reflect};
use wasm_bindgen::prelude::*;

use super::{FORMAT, SOURCES, fixed_key, variant_key};
use crate::effects::EffectRegistry;
use crate::shader::{FixedShader, ShaderKey};

#[wasm_bindgen(module = "/src/shader/tables.js")]
extern "C" {
    #[wasm_bindgen(js_name = shaderTables)]
    fn shader_tables() -> JsValue;
    #[wasm_bindgen(js_name = shaderText)]
    fn shader_text(tables: &JsValue, index: u32) -> String;
}

/// The backend these tables are for (`generate::Backend::name`).
const BACKEND: &str = if cfg!(feature = "webgl") {
    "webgl"
} else {
    "webgpu"
};

thread_local! {
    static TABLES: OnceCell<JsValue> = const { OnceCell::new() };
}

fn field(object: &JsValue, name: &str) -> JsValue {
    Reflect::get(object, &name.into()).unwrap_or(JsValue::UNDEFINED)
}

/// The page's tables, checked against this binary the first time.
fn with_tables<T>(read: impl FnOnce(&JsValue) -> T) -> T {
    TABLES.with(|cell| {
        read(cell.get_or_init(|| {
            let tables = shader_tables();
            if tables.is_null() {
                panic!("the page loaded no shader tables (`sloppyShaders`, index.html)");
            }
            let text = |name| field(&tables, name).as_string().unwrap_or_default();
            let format = field(&tables, "format").as_f64().unwrap_or(0.0);
            let (backend, sources) = (text("backend"), text("sources"));
            if format != f64::from(FORMAT) || backend != BACKEND || sources != SOURCES {
                panic!(
                    "the shader tables are {backend} format {format} from sources {sources}, \
                     not {BACKEND} format {FORMAT} from {SOURCES}: rebuild the page with \
                     `pnpm run wasm`"
                );
            }
            tables
        }))
    })
}

/// The stage entry `key` names: its vertex and fragment text indices, then on WebGL
/// its uniform blocks and samplers.
fn entry(tables: &JsValue, key: &str) -> Array {
    let entry = field(&field(tables, "stages"), key);
    if entry.is_undefined() {
        panic!("the shader tables have no stages for {key} (shader/tables/generate.rs)");
    }
    entry.into()
}

/// Text `index` of an entry, none for -1.
fn text(tables: &JsValue, index: JsValue) -> Option<String> {
    let index = index.as_f64().expect("a text index");
    (index >= 0.0).then(|| shader_text(tables, index as u32))
}

/// Panics unless `effects` is the registry the tables were generated for.
fn check_registry(tables: &JsValue, effects: &EffectRegistry) {
    let ours = format!("{:016x}", effects.fingerprint());
    let theirs = field(tables, "registry").as_string().unwrap_or_default();
    if theirs != ours {
        panic!(
            "the shader tables were generated for effect registry {theirs}, not this page's \
             {ours}: `generate::page_registry` must register what `Presentation::new` does"
        );
    }
}

#[cfg(not(feature = "webgl"))]
mod webgpu {
    use super::*;
    use crate::shader::StageSources;

    fn stages(tables: &JsValue, key: &str) -> StageSources {
        let entry = entry(tables, key);
        StageSources {
            vertex: text(tables, entry.get(0)).expect("a vertex stage"),
            fragment: text(tables, entry.get(1)),
        }
    }

    /// A surface or shadow variant's stages.
    pub fn variant_stages(key: &ShaderKey, effects: &EffectRegistry) -> StageSources {
        with_tables(|tables| {
            check_registry(tables, effects);
            stages(tables, &variant_key(key))
        })
    }

    /// A fixed shader's `vertex` and `fragment` stages.
    pub fn fixed_stages(shader: FixedShader, vertex: &str, fragment: Option<&str>) -> StageSources {
        with_tables(|tables| stages(tables, &fixed_key(shader, vertex, fragment)))
    }
}

#[cfg(not(feature = "webgl"))]
pub use webgpu::{fixed_stages, variant_stages};

#[cfg(feature = "webgl")]
mod webgl {
    use super::*;
    use crate::shader::glsl::{Binding, Program};

    fn bindings(list: JsValue) -> Vec<(String, Binding)> {
        Array::from(&list)
            .iter()
            .map(|binding| {
                let binding = Array::from(&binding);
                let number = |i| binding.get(i).as_f64().expect("a binding number") as u32;
                let name = binding.get(0).as_string().expect("a binding name");
                (name, (number(1), number(2)))
            })
            .collect()
    }

    fn program(tables: &JsValue, key: &str) -> Program {
        let entry = entry(tables, key);
        Program {
            vertex: text(tables, entry.get(0)).expect("a vertex stage"),
            fragment: text(tables, entry.get(1)),
            blocks: bindings(entry.get(2)),
            samplers: bindings(entry.get(3)),
        }
    }

    /// A surface or shadow variant's GLSL program.
    pub fn variant_program(key: &ShaderKey, effects: &EffectRegistry) -> Program {
        with_tables(|tables| {
            check_registry(tables, effects);
            program(tables, &variant_key(key))
        })
    }

    /// A fixed shader's GLSL program of its `vertex` and `fragment` entry points.
    pub fn fixed_program(shader: FixedShader, vertex: &str, fragment: Option<&str>) -> Program {
        with_tables(|tables| program(tables, &fixed_key(shader, vertex, fragment)))
    }
}

#[cfg(feature = "webgl")]
pub use webgl::{fixed_program, variant_program};
