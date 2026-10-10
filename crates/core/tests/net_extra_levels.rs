//! Extra levels in rooms (`tests/extra-levels-multiplayer.test.ts`): the Scrap Yard and
//! Stress Grid play their own rules online and replicate to clients that know them only
//! from the scene.

mod net_support;

use std::collections::BTreeSet;

use net_support::{Harness, harness};
use serde_json::{Value, json};
use sloppy_core::net::protocol::{MAX_SERVER_MESSAGE_BYTES, Message};
use sloppy_core::net::replication::{BinaryMessage, StateMirror, read_binary_message};
use sloppy_core::sim::data::vehicle;
use sloppy_core::sim::extra_levels::extra_level;
use sloppy_core::sim::map_options::MAP_OPTIONS;
use sloppy_core::sim::maps::MAPS;
use sloppy_core::sim::stress_test_level::{
    STRESS_AMMO_CRATE_MULTIPLIER, STRESS_PLAYER_HEALTH_MULTIPLIER, STRESS_POWER_UP_MULTIPLIER,
    STRESS_TEST_MAP,
};
use sloppy_core::sim::superstress_level::{SUPERSTRESS_MAX_FRAGMENTS, SUPERSTRESS_SCALE};
use sloppy_core::sim::types::{Driver, VehicleKind};

/// Room settings for a new room on `map`, as Battle Setup's CREATE ROOM sends them.
fn create(map: &str) -> Value {
    json!({ "create": { "mapMode": map, "difficulty": "normal", "humansOnly": false, "roundMinutes": 5 } })
}

#[test]
fn every_extra_level_offered_by_the_menu_has_a_matching_level_with_its_roster() {
    for option in MAP_OPTIONS {
        if !option.extra {
            assert!(MAPS.iter().any(|map| map.id == option.id));
            continue;
        }
        let level = extra_level(option.id).expect("an extra level has a setup");
        let map = level.custom_map.flatten().expect("its own arena");
        // Its map id names the replicated theme, which clients validate against map ids.
        assert_eq!(map.id, option.id);
        assert_eq!(map.name, option.name);
        assert_eq!(map.description, option.description);
        assert_eq!(
            option.team_tanks.unwrap() * 2,
            level.round_count.unwrap(),
            "lobby rosters count the bots"
        );
    }
}

#[test]
fn a_scrap_yard_room_plays_the_yard_rules_and_its_host_can_switch_back_to_a_standard_map() {
    let mut yard = harness();
    yard.join("alice", create("superstress"));
    yard.join("bob", json!({}));
    assert_eq!(
        yard.latest("bob", "lobby")["settings"]["mapMode"],
        "superstress"
    );
    assert_eq!(
        yard.host.directory_entry("YARDROOM").map_mode,
        "superstress"
    );
    let boosted = vehicle(VehicleKind::Balanced).health * STRESS_PLAYER_HEALTH_MULTIPLIER;
    {
        let sim = yard.sim();
        assert_eq!(sim.map_name(), "SCRAP YARD");
        assert_eq!(sim.tanks.len(), 30);
        assert_eq!(sim.max_fragments, SUPERSTRESS_MAX_FRAGMENTS);
        assert!(
            sim.after_step.is_some(),
            "the yard's rebuild and debris rules run in the room"
        );
        assert!(!sim.endless_match, "rooms keep their match length");
        let alice = sim.tanks.iter().find(|tank| tank.name == "alice").unwrap();
        assert_eq!(
            sim.max_health(alice),
            boosted,
            "players are nearly invulnerable"
        );
        assert_eq!(alice.hp, boosted);
        let bot = sim.tanks.iter().find(|tank| !tank.human).unwrap();
        assert_eq!(
            sim.max_health(bot),
            vehicle(bot.kind).health,
            "bots keep normal health"
        );
        assert_eq!(sim.power_up_duration_multiplier, STRESS_POWER_UP_MULTIPLIER);
        assert_eq!(sim.ammo_crate_multiplier, STRESS_AMMO_CRATE_MULTIPLIER);
    }
    // A late player takes over one of the 30 bot tanks instead of adding a 31st.
    yard.join("erin", json!({}));
    let sim = yard.sim();
    assert_eq!(sim.tanks.len(), 30);
    assert_eq!(
        sim.tanks
            .iter()
            .filter(|tank| tank.driver == Driver::Human)
            .count(),
        3
    );
    let erin = sim
        .tanks
        .iter()
        .find(|tank| tank.name == "erin" && tank.human)
        .unwrap();
    assert_eq!(
        erin.hp, boosted,
        "a taken-over bot tank respawns with the player's hull"
    );

    // Between battles the host picks any map, standard or extra, like any room setting.
    yard.action("alice", "end", json!({}));
    yard.action(
        "alice",
        "settings",
        json!({ "mapMode": "village", "difficulty": "normal", "humansOnly": false }),
    );
    yard.action("alice", "start", json!({}));
    let village = yard.sim();
    assert_eq!(village.map_name(), "PINE VILLAGE");
    assert_eq!(village.tanks.len(), 12);
    assert!(village.after_step.is_none());
    let host = village
        .tanks
        .iter()
        .find(|tank| tank.name == "alice")
        .unwrap();
    assert_eq!(
        village.max_health(host),
        vehicle(VehicleKind::Balanced).health
    );
    assert_eq!(yard.host.directory_entry("YARDROOM").map_mode, "village");
}

#[test]
fn standard_rooms_send_the_same_lobby_and_scene_fields_as_before() {
    let mut standard = harness();
    standard.join("carol", json!({}));
    standard.action("carol", "start", json!({}));
    assert_eq!(standard.sim().map_name(), "PINE VILLAGE");
    assert_eq!(standard.sim().tanks.len(), 12);
    let listing = serde_json::to_value(standard.host.directory_entry("ROOMCODE")).unwrap();
    assert!(listing.get("scenario").is_none());
    assert!(
        standard
            .all("carol")
            .iter()
            .map(Value::to_string)
            .all(|text| !text.contains("\"scenario\"") && !text.contains("\"scale\""))
    );
}

struct Mirrored {
    mirror: StateMirror,
    most_fragments: usize,
    rebuilt: BTreeSet<u32>,
}

/// Plays `steps` host intervals and mirrors the scene as a client would, checking each
/// message.
fn mirror_room(level: &mut Harness, steps: usize) -> Mirrored {
    let mut mirror = StateMirror::default();
    let round = level.host.round_id;
    let mut read = 0;
    let mut most_fragments = 0;
    let mut largest_batch = 0;
    let mut fallen = BTreeSet::new();
    let mut rebuilt = BTreeSet::new();
    for _ in 0..steps {
        level.advance();
        let messages = &level.wire["alice"];
        for message in &messages[read..] {
            assert!(message.len() < MAX_SERVER_MESSAGE_BYTES);
            let Message::Binary(bytes) = message else {
                continue;
            };
            match read_binary_message(bytes).unwrap() {
                BinaryMessage::Full(baseline) => {
                    mirror.apply_full(&baseline, "test-room", round).unwrap();
                }
                BinaryMessage::Snapshot(mut batch) => {
                    largest_batch = largest_batch.max(batch.count);
                    for _ in 0..batch.count {
                        assert!(
                            mirror.apply_snapshot(&mut batch).is_some(),
                            "every snapshot passes client validation"
                        );
                    }
                }
            }
        }
        read = messages.len();
        let scene = mirror.state.as_ref().unwrap();
        most_fragments = most_fragments.max(scene.fragments.len());
        for cover in scene.covers.records.iter().map(|stored| &stored.value) {
            if !cover.alive {
                fallen.insert(cover.id);
            } else if fallen.contains(&cover.id) {
                rebuilt.insert(cover.id);
            }
        }
    }
    assert!(
        !level.host.disposed,
        "the room keeps up with its fixed step"
    );
    assert!(largest_batch <= 8, "clients accept every snapshot batch");
    Mirrored {
        mirror,
        most_fragments,
        rebuilt,
    }
}

#[test]
fn a_scrap_yard_room_replicates_the_compact_yard_its_debris_and_rebuilt_cover() {
    let mut yard = harness();
    yard.join("alice", create("superstress"));
    let result = mirror_room(&mut yard, 15 * 20);
    let scene = result.mirror.state.as_ref().unwrap();
    assert_eq!(scene.map.theme.as_str(), "superstress");
    assert_eq!(scene.tanks.len(), 30);
    assert!(
        result.most_fragments > 128,
        "debris reached {} pieces",
        result.most_fragments
    );
    assert!(
        !result.rebuilt.is_empty(),
        "rebuilt cover reaches clients with its identity"
    );
    let view = scene.render(scene.tanks.records[0].wire.id).unwrap();
    assert_eq!(view.map_scale, SUPERSTRESS_SCALE);
}

#[test]
fn a_stress_grid_room_fields_its_30_tanks_on_the_full_size_grid() {
    let mut grid = harness();
    grid.join("alice", create("stress-test"));
    let alice = {
        let sim = grid.sim();
        assert_eq!(sim.map_name(), "STRESS GRID");
        let alice = sim.tanks.iter().find(|tank| tank.name == "alice").unwrap();
        assert_eq!(
            sim.max_health(alice),
            vehicle(VehicleKind::Balanced).health * STRESS_PLAYER_HEALTH_MULTIPLIER
        );
        alice.id
    };
    let result = mirror_room(&mut grid, 5 * 20);
    let scene = result.mirror.state.as_ref().unwrap();
    assert_eq!(scene.map.theme.as_str(), "stress-test");
    assert_eq!(scene.map.floor, STRESS_TEST_MAP.floor);
    assert_eq!(
        scene.tanks.len(),
        extra_level(STRESS_TEST_MAP.id)
            .unwrap()
            .round_count
            .unwrap()
    );
    assert_eq!(scene.render(alice).unwrap().map_scale, 1.0);
}
