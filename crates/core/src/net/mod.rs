//! Multiplayer: the wire protocol, replication, the authoritative room host and the
//! transport-free client state machine. Nothing here opens a socket or reads a clock; the
//! native server and the browser adapt these to WebSockets and their own timers.
//!
//! Host side (native server):
//! - [`match_host`]: `MatchHost`, the room authority (seats, lobby, rounds, lifetime).
//! - [`multiplayer_simulation`]: room rosters on the shared simulation.
//! - [`player_controls`]: one seat's input leases, action queue and control epochs.
//! - [`fixed_step_clock`]: the host's 60 Hz catch-up clock.
//!
//! Shared wire format:
//! - [`protocol`]: constants, room settings, `join`/`lobby`/`control`/`welcome` records.
//! - [`scene_codec`]: scene capture (host) and validation/projection (client).
//! - [`replication`]: baselines, field deltas and the client mirror.
//! - [`room_list`]: public directory listings.
//! - [`schema`] / [`json`]: TypeScript-compatible readers and number formatting.
//!
//! Client side (browser, through the web crate):
//! - [`client`]: `NetworkClient`, the connection and room session state machine.
//! - [`network_timeline`], [`render_timeline`], [`playout_clock`]: delayed, interpolated
//!   display of remote state with a smoothed local hull.
//! - [`input_cadence`]: when input goes out; [`transport_delay`]: development latency.
//! - [`client_setup`]: pending-join validation and serialization.

pub mod client;
pub mod client_setup;
pub mod fixed_step_clock;
pub mod input_cadence;
pub mod json;
pub mod match_host;
pub mod multiplayer_simulation;
pub mod network_timeline;
pub mod player_controls;
pub mod playout_clock;
pub mod protocol;
pub mod render_timeline;
pub mod replication;
pub mod room_list;
pub mod scene_codec;
pub mod schema;
pub mod transport_delay;
