//! The server's integration tests, one module per area, built as one test binary: each
//! separate binary was linked against the server and the engine by itself on every edit.
//! Add a new area as a module here rather than a file directly under `tests/`, which
//! Cargo would build as another binary.
//! `cargo test --workspace -p sloppy-server --test integration directory::` runs one area.

mod support;

mod directory;
mod match_room;
mod player_names;
mod server;
mod socket;
