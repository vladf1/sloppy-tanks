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
use sloppy_render::presentation::input::slot;

use crate::catalog::{AMMO_OPTIONS, BATTLE_FORMATS, DIFFICULTY_OPTIONS, MAP_CARDS, TANK_CARDS};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/page-catalog.json"
);

/// `Game` and `NetGame` compile only for the page and the desktop (`wasm32` or the
/// `native` feature), which native test builds leave out, so their frame tables are read
/// from the source.
const GAME_SOURCE: &str = include_str!("game.rs");
const NET_GAME_SOURCE: &str = include_str!("net_game.rs");
/// The Rust copy of the page's zoom step is the desktop shell's (`crates/desktop` reads
/// the wheel itself), which this crate cannot depend on.
const DESKTOP_INPUT_SOURCE: &str = include_str!("../../desktop/src/input.rs");

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
        "zoomStep": desktop_zoom_step(),
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

/// `frame_slot` in `game.rs`, by the page's camel-case names.
fn frame_slots() -> Value {
    camel_case_keys(slot_constants(module_body(GAME_SOURCE, "frame_slot"), &[]))
}

/// `net_frame_slot` in `net_game.rs`, with the slots it shares from `frame_slot`.
fn net_frame_slots() -> Value {
    let shared = slot_constants(module_body(GAME_SOURCE, "frame_slot"), &[]);
    camel_case_keys(slot_constants(
        module_body(NET_GAME_SOURCE, "net_frame_slot"),
        &shared,
    ))
}

/// The match phase numbers `frame_slot::PHASE` carries (`phase_code`), by serialized name.
fn phases() -> Value {
    let body = module_body(GAME_SOURCE, "frame_slot");
    let start = body
        .find("fn phase_code")
        .expect("frame_slot has phase_code");
    let mut phases = serde_json::Map::new();
    for line in body[start..].lines().skip(1) {
        let line = line.trim();
        if line == "}" {
            break;
        }
        let Some((variant, code)) = line.strip_prefix("MatchPhase::").and_then(|arm| {
            let (variant, code) = arm.split_once(" => ")?;
            Some((variant, code.strip_suffix(',')?))
        }) else {
            continue;
        };
        let name = variant.to_lowercase();
        let phase: MatchPhase = serde_json::from_value(json!(name)).expect("a match phase");
        assert_eq!(serde_json::to_value(phase).unwrap(), json!(name));
        let code: f32 = code.parse().expect("a phase number");
        phases.insert(name, json!(code as u32));
    }
    assert_eq!(phases.len(), 4, "every match phase has a number");
    Value::Object(phases)
}

fn desktop_zoom_step() -> f64 {
    let prefix = "const ZOOM_STEP: f64 = ";
    DESKTOP_INPUT_SOURCE
        .lines()
        .find_map(|line| line.strip_prefix(prefix)?.strip_suffix(';'))
        .expect("the desktop names its zoom step")
        .parse()
        .expect("a number")
}

/// The source of `pub mod name { ... }`, a module at the file's top level.
fn module_body<'a>(source: &'a str, name: &str) -> &'a str {
    let start = source
        .find(&format!("\npub mod {name} {{"))
        .unwrap_or_else(|| panic!("no top-level `pub mod {name}`"));
    let body = &source[start + 1..];
    &body[..body.find("\n}").expect("the module closes")]
}

/// A slot module's `pub const NAME: usize = N;` items, and the names its `pub use`
/// re-exports from `shared`.
fn slot_constants(body: &str, shared: &[(String, usize)]) -> Vec<(String, usize)> {
    let mut slots = Vec::new();
    for line in body.lines().map(str::trim) {
        if let Some(item) = line.strip_prefix("pub const ") {
            let (name, value) = item
                .strip_suffix(';')
                .and_then(|item| item.split_once(": usize = "))
                .unwrap_or_else(|| panic!("a slot is `NAME: usize = N`: {line}"));
            let value = value.parse().unwrap_or_else(|_| panic!("a number: {line}"));
            slots.push((name.to_owned(), value));
        }
    }
    if let Some(start) = body.find("pub use ") {
        let names = &body[start..];
        let names = &names[names.find('{').unwrap() + 1..names.find('}').unwrap()];
        for name in names
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            let (_, value) = shared
                .iter()
                .find(|(shared, _)| shared == name)
                .unwrap_or_else(|| panic!("{name} is a shared slot"));
            slots.push((name.to_owned(), *value));
        }
    }
    slots
}

/// `HULL_ANGLE` as the page's `hullAngle`.
fn camel_case_keys(slots: Vec<(String, usize)>) -> Value {
    let camel_case = |name: &str| {
        let mut words = name.split('_').map(str::to_lowercase);
        let first = words.next().unwrap_or_default();
        words.fold(first, |mut camel, word| {
            let mut letters = word.chars();
            camel.extend(letters.next().map(|c| c.to_ascii_uppercase()));
            camel.extend(letters);
            camel
        })
    };
    Value::Object(
        slots
            .into_iter()
            .map(|(name, value)| (camel_case(&name), json!(value)))
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
