//! Public room directory entries (`src/net/room-list.ts`): what `/rooms` lists and Battle
//! Setup and the traffic bots read. Never names, player ids or seat tokens.

use serde::Serialize;
use serde_json::Value;

use super::protocol::{
    DEFAULT_ROUND_MINUTES, MAX_ROUND_MINUTES, PHASES, is_room_code, read_difficulty, read_map_mode,
    read_round_minutes,
};
use super::schema::{
    ReadResult, Record, array, boolean, choice, field, id, id32, nested, number_in, string,
};

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
        match self {
            RoomPhase::Lobby => "lobby",
            RoomPhase::Playing => "playing",
            RoomPhase::Results => "results",
        }
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

impl RoomListing {
    /// `roomListingReader`.
    pub fn read(source: &Record) -> ReadResult<Self> {
        let scores = field(source, "scores", |v| array(v, 2, |item| id32(Some(item))))?;
        let listing = Self {
            room: field(source, "room", |v| {
                let code = string(v, 8, 8)?;
                if is_room_code(&code) {
                    Ok(code)
                } else {
                    Err("Invalid room code".into())
                }
            })?,
            content_version: field(source, "contentVersion", |v| string(v, 128, 1))?,
            map_mode: field(source, "mapMode", read_map_mode)?
                .as_str()
                .to_string(),
            difficulty: field(source, "difficulty", read_difficulty)?
                .as_str()
                .to_string(),
            humans_only: field(source, "humansOnly", boolean)?,
            round_minutes: field(source, "roundMinutes", |value| match value {
                None => Ok(DEFAULT_ROUND_MINUTES),
                some => read_round_minutes(some),
            })?,
            players: field(source, "players", |v| number_in(v, 0.0, 8.0, true))? as u32,
            reserved: field(source, "reserved", |v| number_in(v, 0.0, 8.0, true))? as u32,
            phase: field(source, "phase", |v| choice(v, &PHASES))?,
            round_id: field(source, "roundId", id)?,
            time: field(source, "time", |v| {
                number_in(v, 0.0, f64::from(MAX_ROUND_MINUTES * 60), false)
            })? as u32,
            scores: [
                scores.first().copied().unwrap_or(0),
                scores.get(1).copied().unwrap_or(0),
            ],
        };
        Ok(listing)
    }
}

/// `roomListReader`: the `/rooms` response body.
pub fn read_room_list(value: &Value) -> ReadResult<Vec<RoomListing>> {
    nested(Some(value), |source| {
        field(source, "rooms", |v| {
            array(v, MAX_LISTED_ROOMS, |item| {
                nested(Some(item), RoomListing::read)
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listings_round_trip_through_their_reader() {
        let listing = RoomListing {
            room: "ABCDEFGH".into(),
            content_version: "v".into(),
            map_mode: "harbor".into(),
            difficulty: "easy".into(),
            humans_only: true,
            round_minutes: 5,
            players: 2,
            reserved: 3,
            phase: RoomPhase::Playing,
            round_id: 4,
            time: 280,
            scores: [1, 2],
        };
        let value = serde_json::json!({ "rooms": [listing] });
        assert_eq!(read_room_list(&value), Ok(vec![listing]));
        let bad = serde_json::json!({ "rooms": [{ "room": "abcdefgh" }] });
        assert_eq!(
            read_room_list(&bad),
            Err("rooms: room: Invalid room code".into())
        );
    }
}
