//! The engine tables TypeScript keeps copies of, written from the Rust that owns them to
//! `tests/fixtures/ts-copies.json`: the server directory's room listing reader
//! (`directory/room-listing.ts`) and the game preview's input and frame slots
//! (`tools/engine-tables.ts`). `tests/ts-copies.test.ts` checks the copies against that
//! file, so they cannot drift. Rerun with `SLOPPY_UPDATE_FIXTURES=1` after a deliberate
//! change, then `bun run format` (Prettier owns the file's layout).

use serde_json::{Value, json};
use sloppy_core::net::protocol::{
    DEFAULT_ROUND_MINUTES, DIFFICULTIES, MAP_MODES, MAX_ROUND_MINUTES, PHASES, ROOM_CODE_LENGTH,
    is_room_code,
};
use sloppy_core::net::room_list::{HostedRoom, MAX_LISTED_ROOMS, ROOM_SEATS};
use sloppy_core::sim::MatchPhase;
use sloppy_render::presentation::input::slot;

use crate::frame_slot::{self, phase_code};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ts-copies.json");

fn copies() -> Value {
    fn names<T>(table: &[(&'static str, T)]) -> Vec<&'static str> {
        table.iter().map(|(name, _)| *name).collect()
    }
    json!({
        "rooms": {
            "maxListedRooms": MAX_LISTED_ROOMS,
            "seats": ROOM_SEATS,
            "maxContentVersionLength": longest_listed_content_version(),
            "codeLength": ROOM_CODE_LENGTH,
            "codeCharacters": room_code_characters(),
            "maxRoundMinutes": MAX_ROUND_MINUTES,
            "mapModes": names(&MAP_MODES),
            "difficulties": names(&DIFFICULTIES),
            "phases": names(&PHASES),
        },
        "inputSlots": {
            "up": slot::UP,
            "down": slot::DOWN,
            "left": slot::LEFT,
            "right": slot::RIGHT,
            "touchMoveX": slot::TOUCH_MOVE_X,
            "touchMoveZ": slot::TOUCH_MOVE_Z,
            "fire": slot::FIRE,
            "mine": slot::MINE,
            "ammoSlot": slot::AMMO_SLOT,
            "ammoStep": slot::AMMO_STEP,
            "pointerX": slot::POINTER_X,
            "pointerY": slot::POINTER_Y,
            "stickTurn": slot::STICK_TURN,
            "lookPixels": slot::LOOK_PIXELS,
            "zoom": slot::ZOOM,
            "toggleView": slot::TOGGLE_VIEW,
            "wheelAmmo": slot::WHEEL_AMMO,
            "length": slot::LENGTH,
        },
        "frameSlots": {
            "phase": frame_slot::PHASE,
            "humanAlive": frame_slot::HUMAN_ALIVE,
            "events": frame_slot::EVENTS,
            "clearInput": frame_slot::CLEAR_INPUT,
        },
        "phases": phases(),
    })
}

/// The longest content version a listing may carry, found through the engine's own
/// reader.
fn longest_listed_content_version() -> usize {
    let accepts = |length: usize| {
        let listing = json!({
            "room": "ABCDEFGH",
            "contentVersion": "v".repeat(length),
            "mapMode": "village",
            "difficulty": "normal",
            "humansOnly": false,
            "roundMinutes": DEFAULT_ROUND_MINUTES,
            "players": 1,
            "reserved": 1,
            "phase": "lobby",
            "roundId": 1,
            "time": 0,
            "scores": [0, 0],
        });
        HostedRoom::read(&listing, "wss://example.test").is_ok()
    };
    assert!(!accepts(0), "an empty content version is refused");
    let longest = (1..=4096)
        .take_while(|&length| accepts(length))
        .last()
        .expect("a one-letter content version is accepted");
    assert!(!accepts(longest + 1), "content versions have a limit");
    longest
}

/// The printable characters a room code may be made of.
fn room_code_characters() -> String {
    (b' '..=b'~')
        .map(char::from)
        .filter(|&c| is_room_code(&c.to_string().repeat(ROOM_CODE_LENGTH)))
        .collect()
}

/// Each match phase's name and its code in a frame's phase slot.
fn phases() -> Value {
    let phases = [
        MatchPhase::Ready,
        MatchPhase::Playing,
        MatchPhase::Paused,
        MatchPhase::Results,
    ];
    Value::Object(
        phases
            .into_iter()
            .map(|phase| {
                let name = serde_json::to_value(phase).unwrap();
                (
                    name.as_str().unwrap().to_owned(),
                    json!(phase_code(phase) as u32),
                )
            })
            .collect(),
    )
}

#[test]
fn the_typescript_copies_match_the_engines_tables() {
    let copies = copies();
    if std::env::var_os("SLOPPY_UPDATE_FIXTURES").is_some() {
        let text = serde_json::to_string_pretty(&copies).unwrap() + "\n";
        std::fs::write(FIXTURE, text).unwrap();
    }
    let pinned: Value = serde_json::from_str(
        &std::fs::read_to_string(FIXTURE)
            .expect("run with SLOPPY_UPDATE_FIXTURES=1 to write the fixture"),
    )
    .unwrap();
    assert_eq!(
        copies, pinned,
        "the engine's tables differ from {FIXTURE}; rerun with SLOPPY_UPDATE_FIXTURES=1 after a deliberate change"
    );
}
