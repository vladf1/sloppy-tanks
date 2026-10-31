//! Pilots: headless players that join a room over the network like a browser tab and
//! fight with the bot brain, so multiplayer can be tried against something that plays
//! like a person from wherever the pilot runs. See `crates/pilot/README.md`.
//!
//! - [`orders`]: missions (skirmish, hunt a player, escort a player), skill and tank.
//! - [`world_model`]: the brain's [`Simulation`](sloppy_core::sim::Simulation), refilled
//!   every frame from the scene the client displays.
//! - [`pilot`]: one seat: `sloppy_core`'s `NetworkClient` driven by the brain.
//! - [`report`]: the battle record and the network feel a pilot reports.
//! - [`command_line`]: the options of the `sloppy-pilot` command (`src/native/`).
//! - [`fleet`]: pilots behind JSON calls, for the bots Worker's Wasm engine (`abi`).

pub mod command_line;
pub mod fleet;
pub mod orders;
pub mod pilot;
pub mod report;
pub mod world_model;

#[cfg(target_arch = "wasm32")]
mod abi;
