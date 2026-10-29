//! Public room directory entries (`src/net/room-list.ts`, `room-listing.ts`,
//! `server-address.ts`): what `/rooms` lists and Battle Setup and the traffic bots read.
//! Never names, player ids or seat tokens.
//!
//! The server writes [`RoomListing`]s; the directory forwards them through its TypeScript
//! reader, and a desktop client reads the directory's `/servers` answer back with
//! [`ServerList::read`], which accepts and refuses what the page's `readServerList` does,
//! with the same error texts.

use serde::Serialize;
use serde_json::Value;

use super::multiplayer_simulation::MAX_PLAYERS;
use super::protocol::{
    CONTENT_VERSION, DIFFICULTIES, MAP_MODES, MAX_ROUND_MINUTES, PHASES, is_room_code,
};
use super::scene_codec::name;
use super::schema::{ReadResult, boolean, choice, id, number_in, string};
use crate::sim::difficulty::Difficulty;
use crate::sim::map_options::MapId;

/// Most rooms `/rooms` lists; the least recently refreshed listing is evicted beyond it.
pub const MAX_LISTED_ROOMS: usize = 256;
/// Most servers the directory lists.
pub const MAX_LISTED_SERVERS: usize = 16;
/// A listing not refreshed for this long disappears.
pub const ROOM_LIST_TTL_MS: u64 = 45_000;
/// Human seats in a room, as a listing counts its players.
pub const ROOM_SEATS: u32 = MAX_PLAYERS as u32;
/// Longest content version a listing may name, in UTF-16 units.
const MAX_CONTENT_VERSION_LENGTH: usize = 128;
/// Most team scores a listing carries.
const MAX_SCORES: usize = 2;

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

/// A listing as a client reads it from the directory, with the server that hosts it
/// (`HostedRoom`). It keeps what the TypeScript reader accepts, which is wider than what
/// the server writes: `time` may be fractional and `scores` hold up to two safe integers.
/// `map_mode`, `difficulty` and `phase` are read by the names of [`MAP_MODES`],
/// [`DIFFICULTIES`] and [`PHASES`].
#[derive(Clone, Debug, PartialEq)]
pub struct HostedRoom {
    pub room: String,
    pub content_version: String,
    pub map_mode: MapId,
    pub difficulty: Difficulty,
    pub humans_only: bool,
    pub round_minutes: u32,
    pub players: u32,
    pub reserved: u32,
    pub phase: RoomPhase,
    pub round_id: u64,
    /// Seconds left in the round.
    pub time: f64,
    pub scores: Vec<u64>,
    /// The WebSocket address of the server that hosts it.
    pub server: String,
}

impl HostedRoom {
    /// One listing (`readRoomListing`), copying only the declared fields. The fields are
    /// checked in the TypeScript order, so the first bad one names the same error.
    pub fn read(value: &Value, server: &str) -> ReadResult<Self> {
        let fail = |field: &str| format!("Invalid room listing: {field}");
        let entry = value.as_object().ok_or_else(|| fail("entry"))?;
        let room = entry
            .get("room")
            .and_then(Value::as_str)
            .filter(|room| is_room_code(room))
            .ok_or_else(|| fail("room"))?;
        let content_version = string(entry.get("contentVersion"), MAX_CONTENT_VERSION_LENGTH, 1)
            .map_err(|_| fail("contentVersion"))?;
        let longest_round = f64::from(MAX_ROUND_MINUTES * 60);
        let time =
            number_in(entry.get("time"), 0.0, longest_round, false).map_err(|_| fail("time"))?;
        let scores = match entry.get("scores") {
            Some(Value::Array(scores)) if scores.len() <= MAX_SCORES => scores,
            _ => return Err(fail("scores")),
        };
        let humans_only = boolean(entry.get("humansOnly")).map_err(|_| fail("humansOnly"))?;
        let whole_number = |key: &str, min: f64, max: f64| {
            number_in(entry.get(key), min, max, true).map_err(|_| fail(key))
        };
        let seats = f64::from(ROOM_SEATS);
        Ok(Self {
            room: room.to_owned(),
            content_version,
            map_mode: choice(entry.get("mapMode"), &MAP_MODES).map_err(|_| fail("mapMode"))?,
            difficulty: choice(entry.get("difficulty"), &DIFFICULTIES)
                .map_err(|_| fail("difficulty"))?,
            humans_only,
            round_minutes: whole_number("roundMinutes", 1.0, f64::from(MAX_ROUND_MINUTES))? as u32,
            players: whole_number("players", 0.0, seats)? as u32,
            reserved: whole_number("reserved", 0.0, seats)? as u32,
            phase: choice(entry.get("phase"), &PHASES).map_err(|_| fail("phase"))?,
            round_id: id(entry.get("roundId")).map_err(|_| fail("roundId"))?,
            time,
            scores: scores
                .iter()
                .map(|score| id(Some(score)).map_err(|_| fail("scores")))
                .collect::<ReadResult<_>>()?,
            server: server.to_owned(),
        })
    }

    /// Whether this build can take a seat in it: a free seat and the same game.
    pub fn joinable(&self) -> bool {
        self.reserved < ROOM_SEATS && self.content_version == CONTENT_VERSION
    }
}

/// A server's `rooms` (`readRooms`): at most [`MAX_LISTED_ROOMS`] listings, each hosted
/// by `server`.
pub fn read_rooms(value: Option<&Value>, server: &str) -> ReadResult<Vec<HostedRoom>> {
    match value {
        Some(Value::Array(rooms)) if rooms.len() <= MAX_LISTED_ROOMS => rooms
            .iter()
            .map(|room| HostedRoom::read(room, server))
            .collect(),
        _ => Err("Invalid room listing: rooms".into()),
    }
}

/// A game server as the directory lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct ServerListing {
    pub url: String,
    pub content_version: String,
}

/// The directory's answer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServerList {
    /// Fewest rooms first, the order a new room prefers.
    pub servers: Vec<ServerListing>,
    /// Every server's rooms, the fullest first.
    pub rooms: Vec<HostedRoom>,
}

impl ServerList {
    /// The directory's `{ servers: [{ url, contentVersion, rooms, ... }] }`
    /// (`readServerList`), at most [`MAX_LISTED_SERVERS`]. `local`: the directory is on
    /// this machine, so its servers may be plain `ws:` (see [`game_server_address`]).
    /// Refuses the whole list when anything in it is malformed.
    pub fn read(value: &Value, local: bool) -> ReadResult<Self> {
        let entries = match value.get("servers") {
            Some(Value::Array(entries)) if entries.len() <= MAX_LISTED_SERVERS => entries,
            _ => return Err("Invalid server listing: servers".into()),
        };
        let mut list = Self::default();
        for entry in entries {
            let entry = entry.as_object().ok_or("Invalid server listing: server")?;
            let url = game_server_address(entry.get("url"), local)?;
            let content_version = entry
                .get("contentVersion")
                .and_then(Value::as_str)
                .ok_or("Invalid server listing: server contentVersion")?;
            list.rooms.extend(read_rooms(entry.get("rooms"), &url)?);
            list.servers.push(ServerListing {
                url,
                content_version: content_version.to_owned(),
            });
        }
        // Stable, so equal rooms keep the directory's order. Codes are ASCII, which
        // `localeCompare` orders as bytes do.
        list.rooms
            .sort_by(|a, b| b.players.cmp(&a.players).then_with(|| a.room.cmp(&b.room)));
        Ok(list)
    }

    /// The server a new room opens on: the first that plays this build's game.
    pub fn new_room_server(&self) -> Option<&str> {
        self.servers
            .iter()
            .find(|server| server.content_version == CONTENT_VERSION)
            .map(|server| server.url.as_str())
    }
}

/// A game server's WebSocket address from the directory (`gameServerAddress`), without a
/// trailing slash: `wss:`, or also `ws:` when `local`, without credentials, query or
/// fragment.
pub fn game_server_address(url: Option<&Value>, local: bool) -> ReadResult<String> {
    url.and_then(Value::as_str)
        .filter(|url| {
            split_address(url)
                .is_some_and(|(scheme, _)| scheme == "wss" || (scheme == "ws" && local))
        })
        .map(|url| url.strip_suffix('/').unwrap_or(url).to_owned())
        .ok_or_else(|| "Invalid game server address".into())
}

/// The scheme and host (without the port; an IPv6 host keeps its brackets) of
/// `scheme://host[:port][/path]`, or `None` for credentials, a query, a fragment, a port
/// that is not a number or no host. Unlike a browser's `URL` it normalizes nothing, so it
/// refuses addresses a browser would rewrite into this form (an empty `?`, a host outside
/// ASCII).
pub fn split_address(address: &str) -> Option<(&str, &str)> {
    let (scheme, rest) = address.split_once("://")?;
    if rest.contains(['?', '#']) {
        return None;
    }
    let authority = rest.split('/').next().unwrap_or_default();
    let (host, port) = match authority.rfind(':') {
        // A colon inside an IPv6 host's brackets starts no port.
        Some(colon) if !authority[colon..].contains(']') => {
            (&authority[..colon], &authority[colon + 1..])
        }
        _ => (authority, ""),
    };
    let port_valid = port.is_empty()
        || (port.bytes().all(|byte| byte.is_ascii_digit()) && port.parse::<u16>().is_ok());
    (is_host(host) && port_valid).then_some((scheme, host))
}

/// A DNS name or IPv4 address, or an IPv6 address in brackets. Refusing every other
/// character also refuses credentials (`user@host`).
fn is_host(host: &str) -> bool {
    let ipv6 = host
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'));
    let (name, allowed): (&str, fn(u8) -> bool) = match ipv6 {
        Some(ipv6) => (ipv6, |byte| {
            byte.is_ascii_hexdigit() || matches!(byte, b':' | b'.')
        }),
        None => (host, |byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_')
        }),
    };
    !name.is_empty() && name.bytes().all(allowed)
}
