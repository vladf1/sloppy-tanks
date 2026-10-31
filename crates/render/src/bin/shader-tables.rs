//! Writes the shader tables of both page builds (`shader/tables.rs`) into the directory
//! named on the command line, as `webgpu.json` and `webgl.json`. `scripts/build-wasm.mjs`
//! runs it before building the page.

use std::path::PathBuf;

use sloppy_render::shader::tables::generate::{Backend, tables_json};

fn main() {
    let directory = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("usage: shader-tables <output directory>"),
    );
    std::fs::create_dir_all(&directory).expect("a writable output directory");
    for backend in Backend::ALL {
        let path = directory.join(format!("{}.json", backend.name()));
        let tables = tables_json(backend);
        // Unchanged tables keep their timestamp, so Vite has nothing to reload.
        if std::fs::read_to_string(&path).ok().as_deref() != Some(tables.as_str()) {
            std::fs::write(&path, tables).expect("a writable output directory");
        }
    }
}
