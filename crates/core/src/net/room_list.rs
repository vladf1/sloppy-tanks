//! Public room directory entries (`src/net/room-list.ts`): what `/rooms` lists and Battle
//! Setup and the traffic bots read. Never names, player ids or seat tokens.

use serde::Serialize;

use super::protocol::PHASES;
use super::scene_codec::name;

/// Most rooms `/rooms` lists; the least recently refreshed listing is evicted beyond it.
pub const MAX_LISTED_ROOMS: usize = 256;
/// A listing not refreshed for this long disappears.
pub const ROOM_LIST_TTL_MS: u64 = 45_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RoomPhase {
    Lobby,
    Playing,
    Results,
}

impl RoomPhase {
    pub fn as_str(self) -> &'static str {
        name(&PHASES, self)
    }
}

/// Public metadata for one room. Field order and names match the TypeScript
/// `RoomListing`, which clients and traffic bots decode.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomListing {
    pub room: String,
    pub content_version: String,
    pub map_mode: String,
    pub difficulty: String,
    pub humans_only: bool,
    pub round_minutes: u32,
    /// Connected players.
    pub players: u32,
    /// Reserved seats, including players inside their reconnect grace.
    pub reserved: u32,
    pub phase: RoomPhase,
    pub round_id: u64,
    /// Whole seconds left in the round (the round length before the first one starts).
    pub time: u32,
    pub scores: [u32; 2],
}
