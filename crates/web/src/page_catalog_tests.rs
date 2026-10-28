//! The data the page holds in TypeScript because it shows Battle Setup and the room
//! browser before it loads the engine (`src/game/map-options.ts`, `ammo-options.ts`, the
//! cards of `battle-setup.html`, the room listing reader, the input and frame slot
//! tables), written from the Rust that owns it to `tests/fixtures/page-catalog.json`.
//! `tests/page-catalog.test.ts` checks the page's copies against that file, so the two
//! cannot drift. Rerun with `SLOPPY_UPDATE_FIXTURES=1` after a deliberate change, then
//! `pnpm run format` (Prettier owns the file's layout).

use serde_json::{Value, json};
use sloppy_core::net::client::{MAX_BUFFERED_BYTES, POLL_MS};
use sloppy_core::net::protocol::{
    DEFAULT_ROUND_MINUTES, DIFFICULTIES, MAP_MODES, MAX_PLAYER_NAME_LENGTH, MAX_ROUND_MINUTES,
    PHASES, PLAYER_KINDS, ROOM_CODE_ALPHABET, ROOM_CODE_LENGTH, is_room_code,
};
use sloppy_core::net::room_list::{HostedRoom, MAX_LISTED_ROOMS, MAX_LISTED_SERVERS, ROOM_SEATS};
use sloppy_core::sim::MatchPhase;
use sloppy_core::sim::bot_personalities::BOT_NAMES;
use sloppy_core::sim::data::TEAM_NAMES;
use sloppy_render::presentation::input::{ZOOM_STEP, slot};

use crate::catalog::{AMMO_OPTIONS, BATTLE_FORMATS, DIFFICULTY_OPTIONS, MAP_CARDS, TANK_CARDS};
use crate::frame_slot::{self, phase_code};
use crate::net_frame_slot;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/page-catalog.json"
);

fn catalog() -> Value {
    json!({
        "maps": maps(),
        "tanks": tanks(),
        "ammo": ammo(),
        "battleFormats": BATTLE_FORMATS.map(|format| json!({
            "mode": format.mode.as_str(),
            "name": format.name,
            "detail": format.detail,
        })),
        "difficulties": DIFFICULTY_OPTIONS.map(|option| json!({
            "difficulty": option.difficulty.as_str(),
            "name": option.name,
            "detail": option.detail,
        })),
        "playerNames": {
            "defaults": BOT_NAMES.as_slice(),
            "maxLength": MAX_PLAYER_NAME_LENGTH,
        },
        "teamNames": TEAM_NAMES,
        "rooms": rooms(),
        "inputSlots": input_slots(),
        "frameSlots": frame_slots(),
        "netFrameSlots": net_frame_slots(),
        "phases": phases(),
        "zoomStep": ZOOM_STEP,
        "roomConnection": {
            "pollMs": POLL_MS,
            "maxBufferedBytes": MAX_BUFFERED_BYTES,
        },
    })
}

/// The map cards in the shape of the page's `MAP_OPTIONS`, which names `extra` and
/// `teamTanks` only for an extra level.
fn maps() -> Vec<Value> {
    MAP_CARDS
        .iter()
        .map(|card| {
            let mut map = json!({
                "id": card.id.as_str(),
                "name": card.name(),
                "description": card.description,
                "icon": card.icon,
                "tint": color(card.tint),
            });
            if card.extra() {
                map["extra"] = json!(true);
            }
            if let Some(tanks) = card.team_tanks {
                map["teamTanks"] = json!(tanks);
            }
            map
        })
        .collect()
}

/// The tank cards as `battle-setup.html` writes them, bars included.
fn tanks() -> Vec<Value> {
    TANK_CARDS
        .iter()
        .map(|card| {
            json!({
                "kind": card.kind.as_str(),
                "name": card.name(),
                "class": card.class,
                "shortClass": card.short_class,
                "hitPoints": card.hit_points(),
                "kmPerHour": card.km_per_hour(),
                "hitPointsBar": page_number(card.hit_points_share()),
                "speedBar": page_number(card.speed_share()),
            })
        })
        .collect()
}

fn ammo() -> Vec<Value> {
    AMMO_OPTIONS
        .iter()
        .map(|option| {
            json!({
                "weapon": option.weapon.as_str(),
                "label": option.label(),
                "name": option.name,
                "color": color(option.color()),
                "help": option.help,
            })
        })
        .collect()
}

/// What Battle Setup's room browser and the listing reader accept.
fn rooms() -> Value {
    fn names<T>(table: &[(&'static str, T)]) -> Vec<&'static str> {
        table.iter().map(|(name, _)| *name).collect()
    }
    json!({
        "maxListedServers": MAX_LISTED_SERVERS,
        "maxListedRooms": MAX_LISTED_ROOMS,
        "seats": ROOM_SEATS,
        "maxContentVersionLength": longest_listed_content_version(),
        "codeLength": ROOM_CODE_LENGTH,
        "codeAlphabet": std::str::from_utf8(ROOM_CODE_ALPHABET).unwrap(),
        "codeCharacters": room_code_characters(),
        "defaultRoundMinutes": DEFAULT_ROUND_MINUTES,
        "maxRoundMinutes": MAX_ROUND_MINUTES,
        "mapModes": names(&MAP_MODES),
        "difficulties": names(&DIFFICULTIES),
        "phases": names(&PHASES),
        "playerKinds": names(&PLAYER_KINDS),
    })
}

/// The longest `contentVersion` the listing reader accepts, found by asking it.
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

/// The printable ASCII characters a room code may hold, typed codes included.
fn room_code_characters() -> String {
    (b' '..=b'~')
        .map(char::from)
        .filter(|&c| is_room_code(&c.to_string().repeat(ROOM_CODE_LENGTH)))
        .collect()
}

fn input_slots() -> Value {
    json!({
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
    })
}

/// `frame_slot` by the page's names.
fn frame_slots() -> Value {
    json!({
        "phase": frame_slot::PHASE,
        "humanAlive": frame_slot::HUMAN_ALIVE,
        "cockpit": frame_slot::COCKPIT,
        "hullAngle": frame_slot::HULL_ANGLE,
        "events": frame_slot::EVENTS,
        "clearInput": frame_slot::CLEAR_INPUT,
        "hudDue": frame_slot::HUD_DUE,
        "simMs": frame_slot::SIM_MS,
        "renderMs": frame_slot::RENDER_MS,
        "firstPerson": frame_slot::FIRST_PERSON,
        "dt": frame_slot::DT,
        "viewerX": frame_slot::VIEWER_X,
        "viewerY": frame_slot::VIEWER_Y,
        "ran": frame_slot::RAN,
        "length": frame_slot::LENGTH,
    })
}

/// `net_frame_slot` by the room page's names, the slots it shares with `frame_slot`
/// included.
fn net_frame_slots() -> Value {
    json!({
        "cockpit": net_frame_slot::COCKPIT,
        "hullAngle": net_frame_slot::HULL_ANGLE,
        "events": net_frame_slot::EVENTS,
        "hudDue": net_frame_slot::HUD_DUE,
        "simMs": net_frame_slot::SIM_MS,
        "renderMs": net_frame_slot::RENDER_MS,
        "firstPerson": net_frame_slot::FIRST_PERSON,
        "dt": net_frame_slot::DT,
        "drawn": net_frame_slot::DRAWN,
        "pointerFree": net_frame_slot::POINTER_FREE,
        "viewerX": net_frame_slot::VIEWER_X,
        "viewerY": net_frame_slot::VIEWER_Y,
        "ran": net_frame_slot::RAN,
        "length": net_frame_slot::LENGTH,
    })
}

/// The number `frame_slot::PHASE` carries for each match phase, by its serialized name.
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

fn color(rgb: u32) -> String {
    format!("#{rgb:06x}")
}

/// A share as the page writes it: the `f32`'s shortest decimal, not its `f64` widening.
fn page_number(share: f32) -> f64 {
    share.to_string().parse().unwrap()
}

#[test]
fn the_page_catalog_matches_the_engines_tables() {
    let catalog = catalog();
    if std::env::var_os("SLOPPY_UPDATE_FIXTURES").is_some() {
        let text = serde_json::to_string_pretty(&catalog).unwrap() + "\n";
        std::fs::write(FIXTURE, text).unwrap();
    }
    let pinned: Value = serde_json::from_str(
        &std::fs::read_to_string(FIXTURE)
            .expect("run with SLOPPY_UPDATE_FIXTURES=1 to write the fixture"),
    )
    .unwrap();
    let (Value::Object(catalog), Value::Object(pinned)) = (&catalog, &pinned) else {
        panic!("the catalog is an object");
    };
    assert_eq!(
        catalog.keys().collect::<Vec<_>>(),
        pinned.keys().collect::<Vec<_>>()
    );
    for (name, value) in catalog {
        assert_eq!(
            value, &pinned[name],
            "{name} differs from {FIXTURE}; rerun with SLOPPY_UPDATE_FIXTURES=1 after a deliberate change"
        );
    }
}
