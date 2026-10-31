//! The pilot crate's integration tests, one module per area, built as one test binary.
//! `cargo test -p sloppy-pilot --test integration` runs them (the crate resolves the
//! workspace's features, so this compiles nothing twice); add `--release` for quicker rooms.

mod support;

mod missions;
mod world_model;
