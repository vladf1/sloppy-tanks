//! Copies every handwritten WGSL file under `src/` into `OUT_DIR/wgsl/` without its
//! comments, trailing spaces and blank lines, for the [`wgsl!`] macro to embed.
//!
//! The shaders explain themselves at length, and the browser never sees those
//! comments: `shader::specialize` strips them before compiling. Embedding the sources
//! as written put about 190 KB of comments into each engine's binary and its linear
//! memory (60 KB of the brotli download). This is the same stripping
//! `specialize::strip_comments` does, so the code a browser compiles is unchanged.
//!
//! A build without the specializer (the page's shell) draws the variants the build
//! generated (`shader/tables.rs`) and embeds only each file's hook declarations, from
//! `OUT_DIR/wgsl-hooks/`: the lines the effect registry searches to learn which hooks
//! an effect defines (`EffectDefinition::find_hooks`).
//!
//! It also hashes every file under `src/` into `SLOPPY_RENDER_SOURCES`, which the
//! generated tables carry and the page compares with its own, so a binary never draws
//! with tables generated from other sources.

use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    // Cargo scans a directory given here for changes to anything inside it.
    println!("cargo::rerun-if-changed=src");
    copy_stripped(Path::new("src"), &out.join("wgsl"), &out.join("wgsl-hooks"));
    let mut hash = FNV_OFFSET;
    hash_sources(Path::new("src"), &mut hash);
    println!("cargo::rustc-env=SLOPPY_RENDER_SOURCES={hash:016x}");
}

/// Mirror the `.wgsl` files under `dir` into `out`, stripped, and their hook
/// declarations into `hooks`.
fn copy_stripped(dir: &Path, out: &Path, hooks: &Path) {
    for path in sorted_entries(dir) {
        let name = path.file_name().expect("entry name");
        if path.is_dir() {
            copy_stripped(&path, &out.join(name), &hooks.join(name));
        } else if path
            .extension()
            .is_some_and(|extension| extension == "wgsl")
        {
            let code = fs::read_to_string(&path).expect("UTF-8 WGSL source");
            fs::create_dir_all(out).expect("writable OUT_DIR");
            fs::create_dir_all(hooks).expect("writable OUT_DIR");
            let stripped = strip_comments(&code);
            write_if_changed(&hooks.join(name), &hook_lines(&stripped));
            write_if_changed(&out.join(name), &stripped);
        }
    }
}

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .expect("readable source directory")
        .map(|entry| entry.expect("directory entry").path())
        .collect();
    entries.sort();
    entries
}

/// Leave an unchanged file alone so its timestamp does not rebuild the crate.
fn write_if_changed(target: &Path, contents: &str) {
    if fs::read_to_string(target).ok().as_deref() != Some(contents) {
        fs::write(target, contents).expect("writable OUT_DIR");
    }
}

/// The hooks an effect can define (`effects/registry.rs` `Hooks::of`).
const HOOKS: [&str; 4] = [
    "fn effect_vertex(",
    "fn effect_world(",
    "fn effect_clip(",
    "fn effect_surface(",
];

/// The lines of `code` that declare a hook: searching them finds every hook the whole
/// file declares.
fn hook_lines(code: &str) -> String {
    code.lines()
        .filter(|line| HOOKS.iter().any(|hook| line.contains(hook)))
        .flat_map(|line| [line, "\n"])
        .collect()
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

fn fnv(hash: &mut u64, bytes: &[u8]) {
    for &byte in bytes {
        *hash ^= u64::from(byte);
        *hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
}

/// Every file under `dir` into `hash`, with its path, in a fixed order.
fn hash_sources(dir: &Path, hash: &mut u64) {
    for path in sorted_entries(dir) {
        if path.is_dir() {
            hash_sources(&path, hash);
        } else {
            fnv(hash, path.to_string_lossy().as_bytes());
            fnv(hash, &[0]);
            fnv(hash, &fs::read(&path).expect("readable source file"));
            fnv(hash, &[0]);
        }
    }
}

/// `code` without `//` and (nesting) `/* */` comments, trailing whitespace and blank
/// lines. WGSL has no string literals, so a `/` pair always starts a comment.
fn strip_comments(code: &str) -> String {
    let bytes = code.as_bytes();
    let mut kept = String::with_capacity(code.len());
    let mut i = 0;
    let mut start = 0;
    while i < bytes.len() {
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'/') {
            kept.push_str(&code[start..i]);
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            start = i;
        } else if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
            kept.push_str(&code[start..i]);
            let mut depth = 0;
            while i < bytes.len() {
                if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
                    depth += 1;
                    i += 2;
                } else if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            start = i;
        } else {
            i += 1;
        }
    }
    kept.push_str(&code[start..]);
    let mut lines = String::with_capacity(kept.len());
    for line in kept
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
    {
        lines.push_str(line);
        lines.push('\n');
    }
    lines
}
