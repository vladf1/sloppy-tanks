//! The directory's room list as a client reads it (`tests/room-list.test.ts`,
//! `tests/directory.test.ts`, `tests/server-address.test.ts`): the same limits, order and
//! error texts as the page's reader, what the server writes reading back unchanged, room
//! codes and the round-length stepper.

use crate::net_support;

use serde_json::{Value, json};
use sloppy_core::net::protocol::{
    CONTENT_VERSION, DIFFICULTIES, MAP_MODES, PHASES, is_room_code, read_room_code, room_code,
    stepped_minutes,
};
use sloppy_core::net::room_list::{
    HostedRoom, MAX_LISTED_ROOMS, MAX_LISTED_SERVERS, ROOM_SEATS, RoomListing, ServerList,
    ServerListing, game_server_address, read_rooms,
};
use sloppy_core::net::schema::MAX_SAFE_INTEGER;

const SERVER: &str = "wss://1-1-1-1.nip.io";

fn room(code: &str, players: u32, version: &str) -> Value {
    json!({
        "room": code, "contentVersion": version, "mapMode": "harbor",
        "difficulty": "hard", "humansOnly": false, "roundMinutes": 20,
        "players": players, "reserved": players, "phase": "playing", "roundId": 3,
        "time": 512.5, "scores": [4, 7],
    })
}

/// `room` with `changes` merged over it.
fn room_with(changes: Value) -> Value {
    net_support::merged(room("ABCDEFGH", 1, "x"), changes)
}

fn one_server(rooms: Value) -> Value {
    json!({ "servers": [{ "url": SERVER, "contentVersion": "x", "rooms": rooms }] })
}

fn read_one(listing: Value) -> Result<HostedRoom, String> {
    HostedRoom::read(&listing, SERVER)
}

/// The option `name` names among `options`.
fn named<T: Copy>(options: &[(&str, T)], name: &str) -> T {
    options
        .iter()
        .find(|(option, _)| *option == name)
        .map(|&(_, value)| value)
        .expect("a listed name")
}

/// What a client reads back from a listing the server wrote.
fn hosted(listing: &RoomListing, server: &str) -> HostedRoom {
    HostedRoom {
        room: listing.room.clone(),
        content_version: listing.content_version.clone(),
        map_mode: named(&MAP_MODES, &listing.map_mode),
        difficulty: named(&DIFFICULTIES, &listing.difficulty),
        humans_only: listing.humans_only,
        round_minutes: listing.round_minutes,
        players: listing.players,
        reserved: listing.reserved,
        phase: listing.phase,
        round_id: listing.round_id,
        time: f64::from(listing.time),
        scores: listing.scores.map(u64::from).to_vec(),
        server: server.to_owned(),
    }
}

#[test]
fn servers_keep_their_order_and_their_rooms_list_together_fullest_first() {
    let value = json!({ "servers": [
        { "url": "wss://1-1-1-1.nip.io/", "contentVersion": "old",
          "rooms": [room("OLDROOM2", 3, "old")] },
        { "url": "wss://2-2-2-2.nip.io", "contentVersion": CONTENT_VERSION, "serverBuild": "ignored",
          "rooms": [room("NEWROOM2", 1, CONTENT_VERSION), room("NEWROOM3", 5, CONTENT_VERSION)] },
    ]});
    let list = ServerList::read(&value, false).unwrap();
    assert_eq!(
        list.servers,
        [
            ServerListing {
                url: "wss://1-1-1-1.nip.io".into(),
                content_version: "old".into()
            },
            ServerListing {
                url: "wss://2-2-2-2.nip.io".into(),
                content_version: CONTENT_VERSION.into()
            },
        ]
    );
    let rooms: Vec<_> = list
        .rooms
        .iter()
        .map(|room| (room.room.as_str(), room.server.as_str()))
        .collect();
    assert_eq!(
        rooms,
        [
            ("NEWROOM3", "wss://2-2-2-2.nip.io"),
            ("OLDROOM2", "wss://1-1-1-1.nip.io"),
            ("NEWROOM2", "wss://2-2-2-2.nip.io"),
        ]
    );
    assert!(list.rooms[0].joinable());
    assert!(!list.rooms[1].joinable(), "another game version");
    assert_eq!(list.new_room_server(), Some("wss://2-2-2-2.nip.io"));
}

#[test]
fn equally_full_rooms_list_by_code() {
    let list = ServerList::read(
        &one_server(json!([
            room("ZZZZZZZZ", 2, "x"),
            room("AAAAAAA2", 2, "x"),
            room("AAAAAAAB", 2, "x")
        ])),
        false,
    )
    .unwrap();
    let codes: Vec<_> = list.rooms.iter().map(|room| room.room.as_str()).collect();
    assert_eq!(codes, ["AAAAAAA2", "AAAAAAAB", "ZZZZZZZZ"]);
}

#[test]
fn a_room_joins_with_a_free_seat_and_a_new_room_waits_for_this_version() {
    let mut full = read_one(room("ABCDEFGH", ROOM_SEATS, CONTENT_VERSION)).unwrap();
    assert!(!full.joinable());
    full.reserved = ROOM_SEATS - 1;
    assert!(full.joinable());
    let other = ServerList::read(
        &json!({ "servers": [{ "url": SERVER, "contentVersion": "other", "rooms": [] }] }),
        false,
    )
    .unwrap();
    assert_eq!(other.new_room_server(), None);
}

#[test]
fn a_malformed_directory_list_is_refused_whole() {
    let refused = |value: Value| ServerList::read(&value, false).unwrap_err();
    for value in [
        json!({ "rooms": [] }),
        json!(null),
        json!([]),
        json!({ "servers": {} }),
    ] {
        assert_eq!(refused(value), "Invalid server listing: servers");
    }
    assert_eq!(
        refused(json!({ "servers": [1] })),
        "Invalid server listing: server"
    );
    assert_eq!(
        refused(
            json!({ "servers": [{ "url": "ws://1-1-1-1.nip.io", "contentVersion": "x", "rooms": [] }] })
        ),
        "Invalid game server address"
    );
    assert_eq!(
        refused(json!({ "servers": [{ "url": SERVER, "contentVersion": 1, "rooms": [] }] })),
        "Invalid server listing: server contentVersion"
    );
    assert_eq!(
        refused(json!({ "servers": [{ "url": SERVER, "contentVersion": "x" }] })),
        "Invalid room listing: rooms"
    );
    assert_eq!(
        refused(one_server(json!([
            room("ABCDEFGH", 1, "x"),
            room_with(json!({ "players": 9 }))
        ]))),
        "Invalid room listing: players",
        "one bad room spoils the list"
    );
    // A server's own content version is any text; only a room's is bounded.
    assert!(
        ServerList::read(
            &json!({ "servers": [{ "url": SERVER, "contentVersion": "", "rooms": [] }] }),
            false
        )
        .is_ok()
    );
}

#[test]
fn the_directory_lists_at_most_sixteen_servers_of_256_rooms() {
    let servers = |count: usize| {
        let entries: Vec<Value> = (0..count)
            .map(|_| json!({ "url": SERVER, "contentVersion": "x", "rooms": [] }))
            .collect();
        ServerList::read(&json!({ "servers": entries }), false)
    };
    assert_eq!(servers(MAX_LISTED_SERVERS).unwrap().servers.len(), 16);
    assert_eq!(
        servers(MAX_LISTED_SERVERS + 1).unwrap_err(),
        "Invalid server listing: servers"
    );
    let rooms = |count: usize| {
        let rooms: Vec<Value> = (0..count)
            .map(|index| room(&room_code([index as u8, 0, 0, 0, 0, 0, 0, 0]), 1, "x"))
            .collect();
        read_rooms(Some(&Value::Array(rooms)), SERVER)
    };
    assert_eq!(rooms(MAX_LISTED_ROOMS).unwrap().len(), 256);
    assert_eq!(
        rooms(MAX_LISTED_ROOMS + 1).unwrap_err(),
        "Invalid room listing: rooms"
    );
    assert_eq!(
        read_rooms(Some(&json!({})), SERVER).unwrap_err(),
        "Invalid room listing: rooms"
    );
    assert_eq!(
        read_rooms(None, SERVER).unwrap_err(),
        "Invalid room listing: rooms"
    );
}

#[test]
fn each_malformed_field_is_named() {
    let long_version = "v".repeat(129);
    // A code point outside the BMP counts twice, as JavaScript's `length` does.
    let long_emoji = "😀".repeat(65);
    let cases = [
        (json!(1), "entry"),
        (json!([]), "entry"),
        (json!(null), "entry"),
        (json!({ "room": "A" }), "room"),
        (room_with(json!({ "room": "abcdefgh" })), "room"),
        (room_with(json!({ "room": "ABCDEF01" })), "room"),
        (room_with(json!({ "room": "ABCDEFGHJ" })), "room"),
        (room_with(json!({ "contentVersion": "" })), "contentVersion"),
        (
            room_with(json!({ "contentVersion": long_version })),
            "contentVersion",
        ),
        (
            room_with(json!({ "contentVersion": long_emoji })),
            "contentVersion",
        ),
        (room_with(json!({ "time": -1 })), "time"),
        (room_with(json!({ "time": 5940.5 })), "time"),
        (room_with(json!({ "time": "60" })), "time"),
        (room_with(json!({ "scores": [1, 2, 3] })), "scores"),
        (room_with(json!({ "scores": {} })), "scores"),
        (room_with(json!({ "scores": [1.5] })), "scores"),
        (room_with(json!({ "scores": [-1] })), "scores"),
        (
            room_with(json!({ "scores": [9_007_199_254_740_992u64] })),
            "scores",
        ),
        (room_with(json!({ "humansOnly": 0 })), "humansOnly"),
        (room_with(json!({ "mapMode": "<script>" })), "mapMode"),
        (room_with(json!({ "mapMode": "Village" })), "mapMode"),
        (room_with(json!({ "difficulty": "brutal" })), "difficulty"),
        (room_with(json!({ "roundMinutes": 0 })), "roundMinutes"),
        (room_with(json!({ "roundMinutes": 100 })), "roundMinutes"),
        (room_with(json!({ "roundMinutes": 1.5 })), "roundMinutes"),
        (room_with(json!({ "players": 9 })), "players"),
        (room_with(json!({ "players": -1 })), "players"),
        (room_with(json!({ "reserved": 9 })), "reserved"),
        (room_with(json!({ "phase": "ended" })), "phase"),
        (
            room_with(json!({ "roundId": 9_007_199_254_740_992u64 })),
            "roundId",
        ),
        (room_with(json!({ "roundId": "3" })), "roundId"),
    ];
    for (listing, field) in cases {
        assert_eq!(
            read_one(listing.clone()),
            Err(format!("Invalid room listing: {field}")),
            "{listing}"
        );
    }
    // The page checks the room, its version, time and scores before the rest.
    assert_eq!(
        read_one(room_with(json!({ "mapMode": "moon", "scores": [1, 2, 3] }))),
        Err("Invalid room listing: scores".into())
    );
    assert_eq!(
        read_one(room_with(json!({ "room": "A", "time": -1 }))),
        Err("Invalid room listing: room".into())
    );
    let mut missing = room("ABCDEFGH", 1, "x");
    missing.as_object_mut().unwrap().remove("humansOnly");
    assert_eq!(
        read_one(missing),
        Err("Invalid room listing: humansOnly".into())
    );
}

#[test]
fn listings_at_their_limits_are_accepted() {
    let edge = read_one(room_with(json!({
        "contentVersion": "v".repeat(128), "time": 5940, "scores": [9_007_199_254_740_991u64],
        "roundMinutes": 99, "players": 8, "reserved": 8, "roundId": 9_007_199_254_740_991u64,
        "phase": "results", "mapMode": "superstress", "difficulty": "easy", "humansOnly": true,
        "name": "dropped",
    })))
    .unwrap();
    assert_eq!(edge.scores, [MAX_SAFE_INTEGER as u64]);
    assert_eq!(edge.round_id, MAX_SAFE_INTEGER as u64);
    assert_eq!(
        (edge.players, edge.reserved, edge.round_minutes),
        (8, 8, 99)
    );
    assert_eq!(
        (
            edge.map_mode.as_str(),
            edge.difficulty.as_str(),
            edge.phase.as_str()
        ),
        ("superstress", "easy", "results")
    );
    let low = read_one(room_with(json!({ "time": 0, "scores": [], "roundMinutes": 1, "players": 0, "reserved": 0, "roundId": 0 }))).unwrap();
    assert_eq!((low.time, low.scores.len(), low.round_minutes), (0.0, 0, 1));
    // JavaScript cannot tell 3.0 from 3, so neither does the reader.
    let whole = read_one(room_with(json!({ "players": 3.0, "time": 12.25 }))).unwrap();
    assert_eq!((whole.players, whole.time), (3, 12.25));
}

#[test]
fn listings_the_server_writes_read_back_unchanged() {
    let mut written = Vec::new();
    for (phase_index, (_, phase)) in PHASES.iter().enumerate() {
        for (map_index, (map_mode, _)) in MAP_MODES.iter().enumerate() {
            for (difficulty_index, (difficulty, _)) in DIFFICULTIES.iter().enumerate() {
                let index = written.len() as u32;
                let players = index % (ROOM_SEATS + 1);
                let round_minutes = 1 + index * 7 % 99;
                written.push(RoomListing {
                    room: room_code([
                        phase_index as u8,
                        map_index as u8,
                        difficulty_index as u8,
                        3,
                        4,
                        5,
                        6,
                        7,
                    ]),
                    content_version: CONTENT_VERSION.into(),
                    map_mode: (*map_mode).into(),
                    difficulty: (*difficulty).into(),
                    humans_only: index.is_multiple_of(2),
                    round_minutes,
                    players,
                    reserved: (players + index % 3).min(ROOM_SEATS),
                    phase: *phase,
                    round_id: if index == 0 {
                        MAX_SAFE_INTEGER as u64
                    } else {
                        u64::from(index)
                    },
                    time: if index.is_multiple_of(2) {
                        round_minutes * 60
                    } else {
                        0
                    },
                    scores: [u32::MAX, index],
                });
            }
        }
    }
    // Rooms from a real host: a lobby before the first round and a battle under way.
    let lobby = net_support::test_host("lobby-room", 7, None);
    written.push(lobby.directory_entry("LOBBYRM2"));
    let mut battle = net_support::harness();
    battle.join("alice", json!({}));
    battle.action("alice", "start", json!({}));
    for _ in 0..10 {
        battle.advance();
    }
    let playing = battle.host.directory_entry("BATTLE22");
    assert_eq!(playing.phase.as_str(), "playing");
    written.push(playing);

    // As the server's `/servers` writes them.
    let body = serde_json::to_string(&json!({ "servers": [{
        "url": SERVER, "contentVersion": CONTENT_VERSION, "serverBuild": "b", "rooms": written,
    }] }))
    .unwrap();
    let list = ServerList::read(&serde_json::from_str(&body).unwrap(), false).unwrap();
    assert_eq!(list.rooms.len(), written.len());
    for listing in &written {
        let read = list.rooms.iter().find(|room| room.room == listing.room);
        assert_eq!(read, Some(&hosted(listing, SERVER)), "{}", listing.room);
    }
}

#[test]
fn plain_sockets_come_only_from_a_local_directory() {
    let address = |url: Value, local: bool| game_server_address(Some(&url), local);
    assert_eq!(
        address(json!("wss://1-2-3-4.nip.io"), false).unwrap(),
        "wss://1-2-3-4.nip.io"
    );
    assert_eq!(
        address(json!("ws://127.0.0.1:8787/"), true).unwrap(),
        "ws://127.0.0.1:8787"
    );
    assert_eq!(
        address(json!("wss://[::1]:9000/rooms/"), false).unwrap(),
        "wss://[::1]:9000/rooms"
    );
    for (url, local) in [
        (json!("ws://1-2-3-4.nip.io"), false),
        (json!("https://1-2-3-4.nip.io"), true),
        (json!("wss://a@b.example"), true),
        (json!("wss://a:b@b.example"), true),
        (json!("wss://b.example/?x"), true),
        (json!("wss://b.example/#x"), true),
        (json!("wss://b.example:port"), true),
        (json!("wss://b.example:65536"), true),
        (json!("wss://"), true),
        (json!("wss://b example"), true),
        (json!("not a url"), true),
        (json!(42), true),
    ] {
        assert_eq!(
            address(url.clone(), local),
            Err("Invalid game server address".into()),
            "{url}"
        );
    }
    assert_eq!(
        game_server_address(None, true),
        Err("Invalid game server address".into())
    );
}

#[test]
fn room_codes_use_the_unambiguous_alphabet() {
    let code = room_code([0, 31, 32, 255, 7, 8, 9, 10]);
    assert_eq!(code, "A9A9HJKL");
    assert!(is_room_code(&code));
    for byte in 0..=u8::MAX {
        let code = room_code([byte; 8]);
        assert!(is_room_code(&code), "{code}");
        assert!(!code.contains(['0', '1', 'I', 'O']), "{code}");
    }
    assert_eq!(read_room_code(" ab cd2345 "), Some("ABCD2345".into()));
    assert_eq!(read_room_code("kq7m2xhp"), Some("KQ7M2XHP".into()));
    assert_eq!(read_room_code("ABCD234"), None);
    assert_eq!(read_room_code("ABCD2341"), None, "1 is not in the alphabet");
    assert_eq!(read_room_code("ABCDÉFGH"), None);
}

#[test]
fn the_length_stepper_moves_by_five_minutes_within_1_to_99() {
    assert_eq!(stepped_minutes(20, 5), 25);
    assert_eq!(stepped_minutes(20, -5), 15);
    assert_eq!(stepped_minutes(22, 5), 25);
    assert_eq!(stepped_minutes(22, -5), 20);
    assert_eq!(stepped_minutes(5, -5), 1);
    assert_eq!(stepped_minutes(1, 5), 5);
    assert_eq!(stepped_minutes(97, 5), 99);
    assert_eq!(stepped_minutes(98, 5), 99);
    assert_eq!(stepped_minutes(99, 5), 99);
    assert_eq!(
        stepped_minutes(f64::NAN, 5),
        25,
        "not a number: the default"
    );
    assert_eq!(stepped_minutes(f64::INFINITY, -5), 15);
    assert_eq!(stepped_minutes(22.5, -5), 20);
}
