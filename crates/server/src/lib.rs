//! The Sloppy Tanks multiplayer server: one process hosts every room in memory.
//!
//! - [`server`]: HTTP routes, the `/room/CODE` WebSocket upgrade, limits and shutdown.
//! - [`websocket`]: RFC 6455 framing with RFC 7692 permessage-deflate.
//! - [`room_task`] / [`session`]: one Tokio task per room driving a [`session::RoomSession`]
//!   on the 50 ms cadence around a [`host::RoomHost`].
//! - [`monitor`] / [`dashboard`]: readings, `/stats`, journal lines and `/dashboard`.
//! - [`match_room`]: the real host, `sloppy_core`'s `MatchHost` behind [`host::RoomHost`];
//!   [`lobby_host`] is a lobby-only host the session tests use.

pub mod config;
pub mod dashboard;
pub mod host;
pub mod lobby_host;
pub mod match_room;
pub mod monitor;
pub mod process_stats;
pub mod protocol;
pub mod random;
pub mod rate_limit;
pub mod room_catalog;
pub mod room_list;
pub mod room_task;
pub mod server;
pub mod session;
pub mod socket;
pub mod websocket;
pub mod wire;

#[cfg(test)]
mod session_tests;
