//! The shader tables: every surface, shadow and fixed shader stage either page build
//! can ask for, generated with the build so the page's shell carries no specializer.
//!
//! Before a page build, `scripts/build-wasm.mjs` runs the `shader-tables` generator
//! (`src/bin/shader-tables.rs`, [`generate`]), which assembles every variant key the
//! page's effect registry can produce ([`generate::variant_keys`]) with the same code
//! the desktop apps run ([`super::assembly`]), splits each into its stages, translates
//! them to GLSL ES for the WebGL build, and writes one JSON file per backend into the
//! ignored `src/generated/shaders/`. `index.html` fetches its backend's file beside
//! the binary and keeps it in JavaScript as `globalThis.sloppyShaders`; the renderer
//! asks it for the stages a pipeline needs, one string at a time ([`page`]), so the
//! texts never sit in the Wasm memory, which never shrinks.
//!
//! Most of the text repeats across variants, further apart than gzip's window (GitHub
//! Pages serves gzip), so the file stores each distinct line once and each text as
//! runs of those lines (`tables.js`): the WebGPU file is about 80 KB gzipped against
//! 750 KB for the texts one after another.
//!
//! The texts are those the specializer gives, byte for byte (`generate`'s tests), so
//! WebKit's per-stage compile sharing and the unrolled loops stay as they are. A file
//! names the sources it was generated from ([`SOURCES`]) and the effect registry it was
//! generated for (`EffectRegistry::fingerprint`): the page stops with an error when
//! either differs from its own, or when a pipeline asks for a key the file lacks, since
//! it has no specializer to fall back on and the mismatch means a broken build.

use super::{FixedShader, Pass, ShaderKey};

#[cfg(any(test, feature = "shader-tables"))]
pub mod generate;
#[cfg(all(
    target_arch = "wasm32",
    any(feature = "webgl", not(feature = "specializer"))
))]
pub mod page;

/// Raised when the files' layout changes.
pub const FORMAT: u32 = 1;

/// A hash of every file under `crates/render/src/` this binary was built from
/// (`build.rs`), which the tables generated from the same sources carry.
pub const SOURCES: &str = env!("SLOPPY_RENDER_SOURCES");

/// A variant's name in the tables: its pass, its flags (lit, alpha test, alpha to
/// coverage, shadow fade, receives shadow) and its effect id.
pub fn variant_key(key: &ShaderKey) -> String {
    let pass = match key.pass {
        Pass::Main => "main",
        Pass::Shadow => "shadow",
    };
    let flags: String = [
        (key.lit, 'l'),
        (key.alpha_test, 'a'),
        (key.alpha_to_coverage, 'c'),
        (key.shadow_fade, 'f'),
        (key.receive_shadow, 'r'),
    ]
    .into_iter()
    .filter_map(|(set, letter)| set.then_some(letter))
    .collect();
    format!("{pass}:{flags}:{}", key.effect)
}

/// A fixed shader's stages' name in the tables.
pub fn fixed_key(shader: FixedShader, vertex: &str, fragment: Option<&str>) -> String {
    format!("{}:{vertex}:{}", shader.name(), fragment.unwrap_or("-"))
}

/// FNV-1a over `bytes`, continuing from `hash`.
pub(crate) fn fnv(mut hash: u64, bytes: &[u8]) -> u64 {
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

pub(crate) const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
