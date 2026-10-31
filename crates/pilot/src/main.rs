//! `sloppy-pilot`: pilots from this machine (see `crates/pilot/README.md`).

#[cfg(not(target_arch = "wasm32"))]
mod native;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
    native::main()
}

/// The command line has no Wasm build; the Worker uses the library's exports.
#[cfg(target_arch = "wasm32")]
fn main() {}
