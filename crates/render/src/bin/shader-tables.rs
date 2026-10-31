//! Writes the shader tables of the page builds named on the command line (`webgpu`,
//! `webgl`; both when none is) into the directory named first, as `webgpu.json` and
//! `webgl.json` (`shader/tables.rs`). `scripts/build-wasm.mjs` runs it for the builds it
//! is about to make, so a build's tables never change under a binary it leaves alone.

use std::path::PathBuf;

use sloppy_render::shader::tables::generate::{Backend, tables_json};

fn main() {
    let mut arguments = std::env::args().skip(1);
    let directory = PathBuf::from(
        arguments
            .next()
            .expect("usage: shader-tables <output directory> [webgpu] [webgl]"),
    );
    let named: Vec<String> = arguments.collect();
    std::fs::create_dir_all(&directory).expect("a writable output directory");
    for backend in Backend::ALL
        .into_iter()
        .filter(|backend| named.is_empty() || named.iter().any(|name| name == backend.name()))
    {
        let path = directory.join(format!("{}.json", backend.name()));
        let tables = tables_json(backend);
        // Unchanged tables keep their timestamp, so Vite has nothing to reload.
        if std::fs::read_to_string(&path).ok().as_deref() != Some(tables.as_str()) {
            std::fs::write(&path, tables).expect("a writable output directory");
        }
    }
}
