//! The server directory (`src/net/room-list.ts`, `room-listing.ts`, `server-address.ts`):
//! which game servers run, the content version each plays and their open rooms. Battle
//! Setup's Multiplayer tab reads it off the main thread every few seconds.

use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use serde_json::Value;
use sloppy_core::net::protocol::{CONTENT_VERSION, MAX_ROUND_MINUTES, is_room_code};

/// The production directory, unless the build names another (`SLOPPY_DIRECTORY_URL`).
pub const DEFAULT_DIRECTORY: &str = match option_env!("SLOPPY_DIRECTORY_URL") {
    Some(url) => url,
    None => "https://sloppy-tanks-directory.fridman.me",
};
/// The environment whose servers the app lists.
const ENVIRONMENT: &str = "production";
/// Servers the directory lists at most.
pub const MAX_LISTED_SERVERS: usize = 16;
/// Rooms one server lists at most.
pub const MAX_LISTED_ROOMS: usize = 256;
/// Human seats in a room.
pub const ROOM_SEATS: u32 = 8;
/// A directory request that has not answered by now has failed.
pub const LIST_TIMEOUT: Duration = Duration::from_secs(8);
/// The letters of a room code: no look-alikes (0/O, 1/I).
const ROOM_CODE_ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

const MAP_IDS: [&str; 5] = ["village", "harbor", "quarry", "stress-test", "superstress"];
const DIFFICULTIES: [&str; 3] = ["easy", "normal", "hard"];
const PHASES: [&str; 3] = ["lobby", "playing", "results"];

/// One room's public listing (`RoomListing`). Never names, player ids or tokens.
#[derive(Clone, Debug, PartialEq)]
pub struct RoomListing {
    pub room: String,
    pub content_version: String,
    pub map_mode: String,
    pub difficulty: String,
    pub humans_only: bool,
    pub round_minutes: u32,
    pub players: u32,
    pub reserved: u32,
    /// "lobby", "playing" or "results".
    pub phase: String,
    pub round_id: u64,
    /// Seconds left in the round.
    pub time: f64,
    pub scores: Vec<u64>,
    /// The WebSocket address of the server that hosts it.
    pub server: String,
}

impl RoomListing {
    /// Whether this build can take a seat in it: a free seat and the same game.
    pub fn joinable(&self) -> bool {
        self.reserved < ROOM_SEATS && self.content_version == CONTENT_VERSION
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
    pub rooms: Vec<RoomListing>,
}

impl ServerList {
    /// The server a new room opens on: the first that plays this build's game.
    pub fn new_room_server(&self) -> Option<&str> {
        self.servers
            .iter()
            .find(|server| server.content_version == CONTENT_VERSION)
            .map(|server| server.url.as_str())
    }
}

fn is_local_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]")
}

/// Split `scheme://host[:port][/path]` into its scheme, host (without the port) and the
/// rest; `None` for credentials, a query, a fragment or no host.
fn split_address(address: &str) -> Option<(&str, &str, &str)> {
    let (scheme, rest) = address.split_once("://")?;
    if rest.contains(['?', '#', '@']) {
        return None;
    }
    let authority = rest.split('/').next().unwrap_or_default();
    let host = if authority.starts_with('[') {
        authority.split_inclusive(']').next().unwrap_or_default()
    } else {
        authority.split(':').next().unwrap_or_default()
    };
    if host.is_empty() || host.contains(char::is_whitespace) {
        return None;
    }
    Some((scheme, host, rest))
}

/// The directory's `/servers` address for `base`: `http:` or `https:` without
/// credentials, query or fragment, and anything but a local server needs `https:`.
pub fn servers_address(base: &str) -> Result<String, String> {
    let invalid = || format!("Invalid server directory address: {base}");
    let (scheme, host, _) = split_address(base).ok_or_else(invalid)?;
    let local = is_local_host(host);
    if !(scheme == "https" || (scheme == "http" && local)) {
        return Err(invalid());
    }
    Ok(format!(
        "{}/servers?environment={ENVIRONMENT}",
        base.trim_end_matches('/')
    ))
}

/// Whether the directory at `base` is a local server, which may list `ws:` servers.
pub fn is_local_directory(base: &str) -> bool {
    split_address(base).is_some_and(|(_, host, _)| is_local_host(host))
}

/// A game server's WebSocket address, without a trailing slash: `wss:` (or `ws:` from a
/// local directory) without credentials, query or fragment.
pub fn game_server_address(url: &Value, local: bool) -> Result<String, String> {
    let invalid = || "Invalid game server address".to_owned();
    let url = url.as_str().ok_or_else(invalid)?;
    let (scheme, _, _) = split_address(url).ok_or_else(invalid)?;
    if !(scheme == "wss" || (scheme == "ws" && local)) {
        return Err(invalid());
    }
    Ok(url.trim_end_matches('/').to_owned())
}

fn whole_number(value: Option<&Value>, max: u64, field: &str) -> Result<u64, String> {
    value
        .and_then(Value::as_u64)
        .filter(|&number| number <= max)
        .ok_or_else(|| format!("Invalid room listing: {field}"))
}

fn one_of(values: &[&str], value: Option<&Value>, field: &str) -> Result<String, String> {
    value
        .and_then(Value::as_str)
        .filter(|text| values.contains(text))
        .map(str::to_owned)
        .ok_or_else(|| format!("Invalid room listing: {field}"))
}

/// One room listing, copying only the declared fields (`readRoomListing`).
fn read_room(value: &Value, server: &str) -> Result<RoomListing, String> {
    let fail = |field: &str| format!("Invalid room listing: {field}");
    let entry = value.as_object().ok_or_else(|| fail("entry"))?;
    let room = entry
        .get("room")
        .and_then(Value::as_str)
        .filter(|room| is_room_code(room))
        .ok_or_else(|| fail("room"))?;
    let content_version = entry
        .get("contentVersion")
        .and_then(Value::as_str)
        .filter(|version| (1..=128).contains(&version.chars().count()))
        .ok_or_else(|| fail("contentVersion"))?;
    let time = entry
        .get("time")
        .and_then(Value::as_f64)
        .filter(|time| (0.0..=f64::from(MAX_ROUND_MINUTES * 60)).contains(time))
        .ok_or_else(|| fail("time"))?;
    let scores = entry
        .get("scores")
        .and_then(Value::as_array)
        .filter(|scores| scores.len() <= 2)
        .ok_or_else(|| fail("scores"))?
        .iter()
        .map(|score| whole_number(Some(score), u64::MAX >> 11, "scores"))
        .collect::<Result<_, _>>()?;
    let round_minutes = whole_number(
        entry.get("roundMinutes"),
        u64::from(MAX_ROUND_MINUTES),
        "roundMinutes",
    )?;
    if round_minutes < 1 {
        return Err(fail("roundMinutes"));
    }
    Ok(RoomListing {
        room: room.to_owned(),
        content_version: content_version.to_owned(),
        map_mode: one_of(&MAP_IDS, entry.get("mapMode"), "mapMode")?,
        difficulty: one_of(&DIFFICULTIES, entry.get("difficulty"), "difficulty")?,
        humans_only: entry
            .get("humansOnly")
            .and_then(Value::as_bool)
            .ok_or_else(|| fail("humansOnly"))?,
        round_minutes: round_minutes as u32,
        players: whole_number(entry.get("players"), u64::from(ROOM_SEATS), "players")? as u32,
        reserved: whole_number(entry.get("reserved"), u64::from(ROOM_SEATS), "reserved")? as u32,
        phase: one_of(&PHASES, entry.get("phase"), "phase")?,
        round_id: whole_number(entry.get("roundId"), u64::MAX >> 11, "roundId")?,
        time,
        scores,
        server: server.to_owned(),
    })
}

/// The directory's `{ servers: [{ url, contentVersion, rooms, ... }] }` (`readServerList`),
/// at most [`MAX_LISTED_SERVERS`] servers of at most [`MAX_LISTED_ROOMS`] rooms each.
/// `local`: the directory is a local server, whose servers may be plain `ws:`. Fails on
/// anything malformed.
pub fn read_server_list(value: &Value, local: bool) -> Result<ServerList, String> {
    let entries = value
        .get("servers")
        .and_then(Value::as_array)
        .filter(|entries| entries.len() <= MAX_LISTED_SERVERS)
        .ok_or("Invalid server listing: servers")?;
    let mut list = ServerList::default();
    for entry in entries {
        let entry = entry.as_object().ok_or("Invalid server listing: server")?;
        let url = game_server_address(entry.get("url").unwrap_or(&Value::Null), local)?;
        let content_version = entry
            .get("contentVersion")
            .and_then(Value::as_str)
            .ok_or("Invalid server listing: server contentVersion")?;
        let rooms = entry
            .get("rooms")
            .and_then(Value::as_array)
            .filter(|rooms| rooms.len() <= MAX_LISTED_ROOMS)
            .ok_or("Invalid room listing: rooms")?;
        for room in rooms {
            list.rooms.push(read_room(room, &url)?);
        }
        list.servers.push(ServerListing {
            url,
            content_version: content_version.to_owned(),
        });
    }
    list.rooms
        .sort_by(|a, b| b.players.cmp(&a.players).then_with(|| a.room.cmp(&b.room)));
    Ok(list)
}

/// A fresh room code from eight random bytes.
pub fn room_code(random: [u8; 8]) -> String {
    random
        .iter()
        .map(|byte| char::from(ROOM_CODE_ALPHABET[usize::from(byte & 31)]))
        .collect()
}

/// Eight bytes from the standard library's per-process random hash keys.
pub fn random_bytes() -> [u8; 8] {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |time| time.as_nanos()),
    );
    hasher.finish().to_le_bytes()
}

/// A typed room code as the server spells it, or `None` when it is not one.
pub fn read_room_code(text: &str) -> Option<String> {
    let code: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_uppercase();
    is_room_code(&code).then_some(code)
}

/// The new room's length after one press of a stepper (`steppedMinutes`): the next
/// multiple of `step` minutes in that direction, whole minutes from 1 to the maximum.
pub fn stepped_minutes(minutes: u32, step: i32) -> u32 {
    let current = i64::from(minutes);
    let step = i64::from(step);
    let size = step.abs().max(1);
    // Up: the multiple at or below, then one step; down: the multiple at or above, then
    // one step back.
    let multiple = if step > 0 {
        current / size * size
    } else {
        (current + size - 1) / size * size
    };
    (multiple + step).clamp(1, i64::from(MAX_ROUND_MINUTES)) as u32
}

/// A directory request running on its own thread.
pub struct DirectoryRequest {
    answer: Receiver<Result<ServerList, String>>,
}

impl DirectoryRequest {
    /// GET the servers listed at `base` (see [`servers_address`]).
    pub fn start(base: &str) -> Self {
        let (sender, answer) = channel();
        let address = servers_address(base);
        let local = is_local_directory(base);
        std::thread::spawn(move || {
            let result = address.and_then(|address| fetch(&address, local));
            let _ = sender.send(result);
        });
        Self { answer }
    }

    /// The answer once it has come: the list, or why there is none in words a player
    /// reads.
    pub fn answer(&self) -> Option<Result<ServerList, String>> {
        match self.answer.try_recv() {
            Ok(answer) => Some(answer),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("Rooms unavailable".into())),
        }
    }
}

fn fetch(address: &str, local: bool) -> Result<ServerList, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(LIST_TIMEOUT))
        .build()
        .into();
    let mut response = agent.get(address).call().map_err(|error| match error {
        ureq::Error::Timeout(_) => "The room list timed out".to_owned(),
        ureq::Error::StatusCode(_) => "Rooms unavailable".to_owned(),
        _ => "Can't reach the game server".to_owned(),
    })?;
    let value: Value = response
        .body_mut()
        .read_json()
        .map_err(|_| "Rooms unavailable".to_owned())?;
    let list = read_server_list(&value, local)?;
    if list.servers.is_empty() {
        return Err("No game server is running".into());
    }
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn room(code: &str, players: u32, version: &str) -> Value {
        json!({
            "room": code, "contentVersion": version, "mapMode": "harbor",
            "difficulty": "hard", "humansOnly": false, "roundMinutes": 20,
            "players": players, "reserved": players, "phase": "playing", "roundId": 3,
            "time": 512.5, "scores": [4, 7],
        })
    }

    #[test]
    fn a_server_list_reads_rooms_fullest_first_with_their_server() {
        let value = json!({ "servers": [
            { "url": "wss://a.example/", "contentVersion": "other", "rooms": [room("AAAAAAAA", 2, "other")] },
            { "url": "wss://b.example", "contentVersion": CONTENT_VERSION,
              "rooms": [room("BBBBBBBB", 5, CONTENT_VERSION)], "cpus": 2 },
        ]});
        let list = read_server_list(&value, false).unwrap();
        assert_eq!(list.servers.len(), 2);
        assert_eq!(list.rooms[0].room, "BBBBBBBB");
        assert_eq!(list.rooms[0].server, "wss://b.example");
        assert_eq!(list.rooms[1].server, "wss://a.example");
        assert!(list.rooms[0].joinable());
        assert!(!list.rooms[1].joinable(), "another game version");
        assert_eq!(list.new_room_server(), Some("wss://b.example"));
    }

    #[test]
    fn a_new_room_waits_for_a_server_of_this_version() {
        let value = json!({ "servers": [
            { "url": "wss://a.example", "contentVersion": "other", "rooms": [] },
        ]});
        assert_eq!(
            read_server_list(&value, false).unwrap().new_room_server(),
            None
        );
    }

    #[test]
    fn malformed_listings_are_refused() {
        let server = |rooms: Value| json!({ "servers": [{ "url": "wss://a.example", "contentVersion": "x", "rooms": rooms }] });
        assert!(read_server_list(&json!({}), false).is_err());
        assert!(read_server_list(&json!({ "servers": [1] }), false).is_err());
        let mut bad = room("AAAAAAAA", 1, "x");
        bad["room"] = json!("lower123");
        assert!(read_server_list(&server(json!([bad])), false).is_err());
        let mut bad = room("AAAAAAAA", 1, "x");
        bad["players"] = json!(9);
        assert!(read_server_list(&server(json!([bad])), false).is_err());
        let mut bad = room("AAAAAAAA", 1, "x");
        bad["mapMode"] = json!("moon");
        assert!(read_server_list(&server(json!([bad])), false).is_err());
        let mut bad = room("AAAAAAAA", 1, "x");
        bad["roundMinutes"] = json!(0);
        assert!(read_server_list(&server(json!([bad])), false).is_err());
        let mut bad = room("AAAAAAAA", 1, "x");
        bad["scores"] = json!([1, 2, 3]);
        assert!(read_server_list(&server(json!([bad])), false).is_err());
        let too_many: Vec<Value> = (0..=MAX_LISTED_SERVERS)
            .map(|_| json!({ "url": "wss://a.example", "contentVersion": "x", "rooms": [] }))
            .collect();
        assert!(read_server_list(&json!({ "servers": too_many }), false).is_err());
    }

    #[test]
    fn plain_sockets_come_only_from_a_local_directory() {
        let value = json!({ "servers": [{ "url": "ws://127.0.0.1:9000", "contentVersion": "x", "rooms": [] }] });
        assert!(read_server_list(&value, false).is_err());
        assert!(read_server_list(&value, true).is_ok());
        for bad in [
            "https://a.example",
            "wss://user@a.example",
            "wss://a.example/?x",
            "wss://",
        ] {
            assert!(game_server_address(&json!(bad), true).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_directory_needs_https_except_on_this_machine() {
        assert_eq!(
            servers_address("https://dir.example/").unwrap(),
            "https://dir.example/servers?environment=production"
        );
        assert_eq!(
            servers_address("http://127.0.0.1:8790").unwrap(),
            "http://127.0.0.1:8790/servers?environment=production"
        );
        assert!(servers_address("http://dir.example").is_err());
        assert!(servers_address("https://dir.example/?a=b").is_err());
        assert!(servers_address("ftp://localhost").is_err());
        assert!(is_local_directory("http://localhost:8790"));
        assert!(!is_local_directory(DEFAULT_DIRECTORY));
        assert!(servers_address(DEFAULT_DIRECTORY).is_ok());
    }

    #[test]
    fn room_codes_use_the_unambiguous_alphabet() {
        let code = room_code([0, 31, 32, 255, 7, 8, 9, 10]);
        assert_eq!(code, "A9A9HJKL");
        assert!(is_room_code(&code));
        assert!(is_room_code(&room_code(random_bytes())));
        assert_eq!(read_room_code(" ab cd2345 "), Some("ABCD2345".into()));
        assert_eq!(read_room_code("ABCD234"), None);
        assert_eq!(read_room_code("ABCD2341"), None, "1 is not in the alphabet");
    }

    #[test]
    fn the_length_stepper_moves_to_the_next_multiple() {
        assert_eq!(stepped_minutes(20, 5), 25);
        assert_eq!(stepped_minutes(22, 5), 25);
        assert_eq!(stepped_minutes(22, -5), 20);
        assert_eq!(stepped_minutes(20, -5), 15);
        assert_eq!(stepped_minutes(5, -5), 1);
        assert_eq!(stepped_minutes(1, 5), 5);
        assert_eq!(stepped_minutes(98, 5), 99);
        assert_eq!(stepped_minutes(99, 5), 99);
    }
}
