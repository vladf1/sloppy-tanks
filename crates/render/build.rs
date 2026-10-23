//! Copies every handwritten WGSL file under `src/` into `OUT_DIR/wgsl/` without its
//! comments, trailing spaces and blank lines, for the [`wgsl!`] macro to embed.
//!
//! The shaders explain themselves at length, and the browser never sees those
//! comments: `shader::specialize` strips them before compiling. Embedding the sources
//! as written put about 190 KB of comments into each engine's binary and its linear
//! memory (60 KB of the brotli download). This is the same stripping
//! `specialize::strip_comments` does, so the code a browser compiles is unchanged.

use std::fs;
use std::path::Path;

fn main() {
    let out = Path::new(&std::env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("wgsl");
    // Cargo scans a directory given here for changes to anything inside it.
    println!("cargo::rerun-if-changed=src");
    copy_stripped(Path::new("src"), &out);
}

/// Mirror the `.wgsl` files under `dir` into `out`, stripped.
fn copy_stripped(dir: &Path, out: &Path) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .expect("readable source directory")
        .map(|entry| entry.expect("directory entry").path())
        .collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().expect("entry name");
        if path.is_dir() {
            copy_stripped(&path, &out.join(name));
        } else if path
            .extension()
            .is_some_and(|extension| extension == "wgsl")
        {
            let code = fs::read_to_string(&path).expect("UTF-8 WGSL source");
            fs::create_dir_all(out).expect("writable OUT_DIR");
            let target = out.join(name);
            let stripped = strip_comments(&code);
            // Leave an unchanged file alone so its timestamp does not rebuild the crate.
            if fs::read_to_string(&target).ok().as_deref() != Some(stripped.as_str()) {
                fs::write(&target, stripped).expect("writable OUT_DIR");
            }
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
