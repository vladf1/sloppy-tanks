//! Multiplayer: the wire protocol, replication, the authoritative room host and the
//! transport-free client state machine. Nothing here opens a socket or reads a clock; the
//! native server and the browser adapt these to WebSockets and their own timers.

pub mod fixed_step_clock;
pub mod json;
pub mod match_host;
pub mod multiplayer_simulation;
pub mod player_controls;
pub mod protocol;
pub mod replication;
pub mod room_list;
pub mod scene_codec;
pub mod schema;
