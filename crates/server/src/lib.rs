//! The Sloppy Tanks multiplayer server: one process hosts every room in memory.

pub mod config;
pub mod dashboard;
pub mod host;
pub mod lobby_host;
pub mod monitor;
pub mod process_stats;
pub mod protocol;
pub mod random;
pub mod rate_limit;
pub mod room_catalog;
pub mod room_list;
pub mod session;
pub mod websocket;

#[cfg(test)]
mod session_tests;
