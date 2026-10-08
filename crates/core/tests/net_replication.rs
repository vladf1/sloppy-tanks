//! Baselines, field deltas and the client mirror (`tests/replication.test.ts`), and the
//! host's scene capture against an independent schema reference
//! (`tests/scene-capture.test.ts`).

mod net_support;

use std::collections::{BTreeMap, BTreeSet};

use net_support::{assert_same, same};
use serde_json::{Map, Value, json};
use sloppy_core::net::multiplayer_simulation::{MultiplayerOptions, create_multiplayer_simulation};
use sloppy_core::net::replication::{StateMirror, StateStream};
use sloppy_core::net::scene_codec::{ENTITY_FIELDS, ENTITY_TYPES, MirrorScene, Scene};
use sloppy_core::net::shot_paths::PathEntry;
use sloppy_core::sim::debris_cleanup::DEBRIS_CLEANUP_SECONDS;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::types::{
    CoverKind, FragmentShape, PlayerAssignment, Team, VehicleCommand, VehicleKind, Weapon,
};
use sloppy_core::sim::{Simulation, render_state::RenderState};

fn one_player() -> Vec<PlayerAssignment> {
    vec![PlayerAssignment {
        player_id: "one".into(),
        name: "One".into(),
        team: Team::Blue,
        slot: 0,
        kind: VehicleKind::Balanced,
    }]
}

fn room(map: MapId, players: &[PlayerAssignment]) -> Simulation {
    create_multiplayer_simulation(
        4242.0,
        players,
        MultiplayerOptions {
            map_mode: Some(map),
            ..MultiplayerOptions::default()
        },
    )
    .unwrap()
}

fn human(sim: &Simulation) -> u32 {
    sim.tanks[sim.human_index().unwrap()].id
}

fn step(sim: &mut Simulation, command: VehicleCommand) {
    let mut commands = BTreeMap::new();
    commands.insert(human(sim), command);
    sim.step_with(&commands);
    sim.events.clear();
}

fn parse(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

fn read_scene(scene: &Scene) -> MirrorScene {
    MirrorScene::read(Some(&parse(&scene.to_json()))).unwrap()
}

fn render_json(state: &RenderState) -> Value {
    serde_json::to_value(state).unwrap()
}

#[test]
fn full_and_field_deltas_round_trip_through_json_including_destruction_and_late_joins() {
    for map in [MapId::Village, MapId::Harbor, MapId::Quarry] {
        let mut sim = room(map, &one_player());
        sim.start();
        let mut stream = StateStream::new("room", 1);
        let mut mirror = StateMirror::default();
        let full = stream.full(&Scene::capture(&sim), 0, 0, &[]);
        assert!(full.len() < 160_000, "Full-state wire budget");
        mirror.apply_full(&parse(&full), "room", 1).unwrap();
        let mut removed = false;
        for tick in 1..=180 {
            if tick == 30 {
                let targets: Vec<usize> = (0..sim.covers.len())
                    .filter(|&i| sim.covers[i].alive && sim.covers[i].destructible)
                    .take(8)
                    .collect();
                for cover in targets {
                    sim.damage_cover(cover, 10000.0, 0, Team::Blue, None, None);
                }
            }
            if tick == 90 {
                for fragment in &mut sim.fragments {
                    fragment.life = 0.0;
                }
            }
            step(
                &mut sim,
                VehicleCommand {
                    move_x: 1.0,
                    fire: true,
                    ..VehicleCommand::idle()
                },
            );
            if tick % 3 != 0 {
                continue;
            }
            let scene = Scene::capture(&sim);
            let mut next = scene.clone();
            let frame = stream.snapshot(&mut next, tick, &[], &[]);
            assert!(frame.len() < 128_000, "Burst snapshot wire budget");
            removed |= parse(&frame).get("removed").is_some();
            assert!(
                mirror.apply_snapshot(&parse(&frame)).is_some(),
                "{map:?} tick {tick}"
            );
            assert_same(
                &mirror.state.as_ref().unwrap().to_value(),
                &parse(&scene.to_json()),
                "mirror equals the capture",
            );
            let viewer = human(&sim);
            let expected = read_scene(&scene).render(viewer).unwrap();
            assert_eq!(mirror.render(viewer).unwrap(), expected);
            if tick == 60 || tick == 93 {
                let mut late = StateMirror::default();
                late.apply_full(&parse(&stream.full(&scene, tick, 0, &[])), "room", 1)
                    .unwrap();
                assert_eq!(late.render(viewer).unwrap(), mirror.render(viewer).unwrap());
            }
        }
        assert!(removed, "{map:?} removes entities");
        let view = mirror.render(human(&sim)).unwrap();
        assert!(
            view.covers
                .iter()
                .any(|cover| cover.max_hp == f64::INFINITY)
        );
        let text = mirror.state.as_ref().unwrap().to_value().to_string();
        for forbidden in ["\"body\"", "\"collider\"", "Infinity", "NaN"] {
            assert!(
                !text.contains(forbidden),
                "{forbidden} leaked into the scene"
            );
        }
    }
}

#[test]
fn frames_omit_identity_and_unchanged_data_and_scenes_carry_only_presentation_fields() {
    let mut sim = room(MapId::Village, &one_player());
    sim.start();
    step(
        &mut sim,
        VehicleCommand {
            fire: true,
            ..VehicleCommand::idle()
        },
    );
    let scene = Scene::capture(&sim);
    let mut stream = StateStream::new("room", 1);
    stream.full(&scene, 0, 0, &[]);
    let mut same_scene = scene.clone();
    let frame = parse(&stream.snapshot(&mut same_scene, 1, &[], &[]));
    let keys: Vec<&String> = frame.as_object().unwrap().keys().collect();
    assert_eq!(keys.len(), 3);
    for key in ["seq", "tick", "elapsed"] {
        assert!(frame.get(key).is_some());
    }
    let value = parse(&scene.to_json());
    let entities = &value["entities"];
    assert!(
        entities.get("shots").is_none(),
        "shells travel as paths, not records"
    );
    assert!(
        entities["covers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|cover| cover.get("motion").is_some())
    );
    for cover in entities["covers"].as_array().unwrap() {
        if let Some(motion) = cover.get("motion") {
            let keys: BTreeSet<&str> = motion
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            assert_eq!(keys, BTreeSet::from(["originX", "originZ", "w", "d"]));
        }
    }
    assert!(
        entities["tanks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|tank| tank.get("previous").is_none())
    );
    let viewer = read_scene(&scene).render(human(&sim)).unwrap();
    let viewer = viewer.viewer().unwrap();
    assert_eq!(viewer.previous.x, viewer.position.x);
    assert_eq!(viewer.previous.z, viewer.position.z);
}

#[test]
fn debris_life_reaches_clients_only_once_its_final_fade_begins() {
    let mut sim = room(MapId::Village, &one_player());
    sim.fragment(0.0, 0.0, 0xffffff, 0.5, FragmentShape::Shard, 1.0);
    let piece = sim.fragments[0].id;
    let life_update =
        |frame: &str| parse(frame)["updates"]["fragments"][piece.to_string()]["life"].clone();
    sim.fragments[0].life = 5.0;
    let mut stream = StateStream::new("room", 1);
    stream.full(&Scene::capture(&sim), 0, 0, &[]);
    sim.fragments[0].life = 4.0;
    let frame = stream.snapshot(&mut Scene::capture(&sim), 1, &[], &[]);
    assert_eq!(life_update(&frame), Value::Null);
    sim.fragments[0].life = DEBRIS_CLEANUP_SECONDS / 2.0;
    let frame = stream.snapshot(&mut Scene::capture(&sim), 2, &[], &[]);
    assert_eq!(
        life_update(&frame).as_f64(),
        Some(DEBRIS_CLEANUP_SECONDS / 2.0)
    );
}

#[test]
fn mirror_rejects_corrupt_or_skipped_deltas_atomically_and_a_full_baseline_repairs_it() {
    let sim = room(MapId::Village, &[]);
    let state = Scene::capture(&sim);
    let mut stream = StateStream::new("r", 1);
    let mut mirror = StateMirror::default();
    let full = parse(&stream.full(&state, 0, 0, &[]));
    mirror.apply_full(&full, "r", 1).unwrap();
    let before = mirror.state.as_ref().unwrap().to_value();
    let mut bad = parse(&stream.snapshot(&mut state.clone(), 3, &[], &[]));
    bad["updates"] = json!({ "tanks": { sim.tanks[0].id.to_string(): { "hp": "bad" } } });
    assert!(mirror.apply_snapshot(&bad).is_none());
    assert_eq!(mirror.state.as_ref().unwrap().to_value(), before);
    assert!(mirror.needs_full);
    mirror.apply_full(&full, "r", 1).unwrap();
    bad["seq"] = json!(9);
    assert!(mirror.apply_snapshot(&bad).is_none());
    assert_eq!(mirror.state.as_ref().unwrap().to_value(), before);
    mirror
        .apply_full(&parse(&stream.full(&state, 3, 0, &[])), "r", 1)
        .unwrap();
    assert!(!mirror.needs_full);
}

#[test]
fn projectile_paths_apply_in_order_inherit_their_launch_and_a_bad_entry_rejects_the_frame() {
    let sim = room(MapId::Village, &[]);
    let state = Scene::capture(&sim);
    let viewer = sim.tanks[0].id;
    let elapsed = state.elapsed;
    let mut mirror = StateMirror::default();
    let full = parse(&StateStream::new("r", 1).full(&state, 0, 0, &[]));
    mirror.apply_full(&full, "r", 1).unwrap();
    let frame = |seq: u64, tick: u64, paths: Value| json!({ "seq": seq, "tick": tick, "elapsed": elapsed, "paths": paths });
    let launch = json!({
        "id": 900, "tick": 0.5, "x": 0, "z": 0, "vx": 30, "vz": 0,
        "team": 1, "weapon": "ricochet", "y": 1.2, "visualY": 1.6,
    });
    assert!(
        mirror
            .apply_snapshot(&frame(1, 3, json!([launch])))
            .is_some()
    );
    let shot = mirror.render(viewer).unwrap().shots[0];
    assert_eq!(
        (shot.x, shot.z),
        (30.0 * 2.5 / 60.0, 0.0),
        "drawn at the frame tick"
    );
    let bounce = json!({ "id": 900, "tick": 4, "x": 1.75, "z": 0, "vx": -30, "vz": 0 });
    let extras = mirror
        .apply_snapshot(&frame(2, 6, json!([bounce])))
        .unwrap();
    let PathEntry::Change(path) = extras.paths[0] else {
        panic!("a new path for a flying shell");
    };
    assert_eq!(
        (path.launch.weapon, path.launch.team, path.launch.visual_y),
        (Weapon::Ricochet, Team::Red, Some(1.6)),
        "later paths keep the launch fields"
    );
    let shells = |mirror: &StateMirror| mirror.shots.paths.clone();
    let before = shells(&mirror);
    for (bad, why) in [
        (launch.clone(), "a second launch"),
        (
            json!({ "id": 901, "tick": 5, "x": 0, "z": 0, "vx": 1, "vz": 0 }),
            "an unknown shell",
        ),
        (
            json!({ "id": 901, "end": 5 }),
            "the end of an unknown shell",
        ),
        (
            json!({ "id": 900, "tick": 3.5, "x": 0, "z": 0, "vx": 1, "vz": 0 }),
            "an earlier path",
        ),
        (json!({ "id": 900, "end": 3.9 }), "an end before the path"),
        (json!({ "id": 900, "end": 9.5 }), "an end after the frame"),
        (
            json!({ "id": 900, "tick": 9.5, "x": 0, "z": 0, "vx": 1, "vz": 0 }),
            "a future path",
        ),
    ] {
        // A valid launch first: the frame is rejected whole, not up to the bad entry.
        let other = json!({ "id": 950, "tick": 7, "x": 5, "z": 5, "vx": 0, "vz": 9, "team": 0, "weapon": "standard" });
        let mut probe = mirror.clone();
        assert!(
            probe
                .apply_snapshot(&frame(3, 9, json!([other, bad])))
                .is_none(),
            "{why}"
        );
        assert_eq!(shells(&probe), before, "{why} leaves the shells");
        assert_eq!(probe.seq, 2, "{why} leaves the stream");
    }
    let end = json!([{ "id": 900, "end": 7 }]);
    assert!(mirror.apply_snapshot(&frame(3, 9, end)).is_some());
    assert!(mirror.render(viewer).unwrap().shots.is_empty());
}

#[test]
fn mirror_allows_one_change_per_entity_of_each_kind_in_a_frame() {
    let sim = room(MapId::Village, &[]);
    let state = Scene::capture(&sim);
    let mut mirror = StateMirror::default();
    let full = parse(&StateStream::new("r", 1).full(&state, 0, 0, &[]));
    mirror.apply_full(&full, "r", 1).unwrap();
    let before = mirror.state.as_ref().unwrap().to_value();
    let tank = sim.tanks[0].id;
    let key = tank.to_string();
    let elapsed = state.elapsed;
    let mine = json!({ "id": tank, "x": 0, "z": 0, "owner": tank, "team": 0, "arm": 0, "life": 1 });
    let conflicting = json!({
        "seq": 1, "tick": 1, "elapsed": elapsed,
        "updates": { "tanks": { key.clone(): { "hp": 1 } } },
        "removed": { "tanks": [tank] },
    });
    assert!(mirror.apply_snapshot(&conflicting).is_none());
    assert_eq!(mirror.state.as_ref().unwrap().to_value(), before);
    assert!(mirror.needs_full);
    mirror.apply_full(&full, "r", 1).unwrap();
    // Ids are claimed per kind, so another kind may reuse a tank's id in the same frame.
    let shared = json!({
        "seq": 1, "tick": 1, "elapsed": elapsed,
        "updates": { "tanks": { key.clone(): { "hp": 1 } }, "mines": { key.clone(): mine } },
    });
    assert!(mirror.apply_snapshot(&shared).is_some());
    let scene = mirror.state.as_ref().unwrap();
    assert_eq!(scene.tanks.records[0].value.hp, 1.0);
    assert!(same(
        &Value::Object(scene.mines.records.last().unwrap().wire.clone()),
        &mine
    ));
    // The removal that conflicted with the update above is valid in a frame of its own.
    let removal =
        json!({ "seq": 2, "tick": 1, "elapsed": elapsed, "removed": { "tanks": [tank] } });
    assert!(mirror.apply_snapshot(&removal).is_some());
    assert!(!mirror.state.as_ref().unwrap().tanks.contains(tank));
}

// ---- The capture against an independent reference ----------------------------------

const ANGLES: [&str; 4] = ["aim", "heading", "yaw", "lean"];
const VALUES: [&str; 20] = [
    "hp",
    "maxHp",
    "xp",
    "shield",
    "shieldPoints",
    "protection",
    "laser",
    "recoil",
    "cooldown",
    "mineCooldown",
    "respawn",
    "rapid",
    "speed",
    "lastCombat",
    "life",
    "arm",
    "createdAt",
    "expiresAt",
    "cooldownDuration",
    "time",
];

/// `rounded` from `scene-codec.ts`: numbers by field name, recursively.
fn rounded(value: &Value, key: &str, parent: &str) -> Value {
    match value {
        Value::Number(number) => {
            let scale = if parent == "rotation" || ANGLES.contains(&key) {
                10000.0
            } else if VALUES.contains(&key) {
                100.0
            } else {
                1000.0
            };
            let x = number.as_f64().unwrap();
            let r = sloppy_core::sim::math::js_round(x * scale) / scale;
            json!(if r == 0.0 { 0.0 } else { r })
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| rounded(item, key, parent))
                .collect(),
        ),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(field, item)| (field.clone(), rounded(item, field, key)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The readers' field selection over plain render views: undefined (null) optionals and
/// fields outside the schema are dropped.
fn select(record: &Value, fields: &[&str], nullable: &[&str]) -> Value {
    let mut out = Map::new();
    for field in fields {
        match record.get(*field) {
            Some(Value::Null) if nullable.contains(field) => {
                out.insert((*field).to_string(), Value::Null);
            }
            Some(Value::Null) | None => {}
            Some(value) => {
                out.insert((*field).to_string(), strip_nulls(value));
            }
        }
    }
    Value::Object(out)
}

fn strip_nulls(value: &Value) -> Value {
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .filter(|(_, item)| !item.is_null())
                .map(|(key, item)| (key.clone(), strip_nulls(item)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(strip_nulls).collect()),
        other => other.clone(),
    }
}

fn reference_scene(sim: &Simulation) -> Value {
    let view = render_json(&sim.render_state(Some(sim.tanks[0].id)));
    let kind_records =
        |kind: usize, nullable: &[&str], adjust: &dyn Fn(&mut Map<String, Value>)| {
            Value::Array(
                view[ENTITY_TYPES[kind]]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|record| {
                        let mut record = record.as_object().unwrap().clone();
                        adjust(&mut record);
                        select(&Value::Object(record), ENTITY_FIELDS[kind], nullable)
                    })
                    .collect(),
            )
        };
    let entities = json!({
        "tanks": kind_records(0, &[], &|_| {}),
        "covers": kind_records(1, &["hp", "maxHp"], &|cover| {
            // Timber joins keep only their set ends, and unmarked walls carry no hit list.
            if let Some(Value::Object(join)) = cover.get_mut("timberJoin") {
                join.retain(|_, flag| flag == &Value::Bool(true));
            }
            if cover.get("timberHits").and_then(Value::as_array).is_some_and(Vec::is_empty) {
                cover.remove("timberHits");
            }
        }),
        "fragments": kind_records(2, &[], &|fragment| {
            let life = fragment["life"].as_f64().unwrap().min(DEBRIS_CLEANUP_SECONDS);
            fragment.insert("life".into(), json!(life));
        }),
        "mines": kind_records(3, &[], &|_| {}),
        "pickups": kind_records(4, &[], &|_| {}),
    });
    let mut map = Map::new();
    map.insert("theme".into(), view["mapTheme"].clone());
    for (from, to) in [
        ("mapFloor", "floor"),
        ("mapOuterFloor", "outerFloor"),
        ("mapOuterFloorExtent", "outerFloorExtent"),
    ] {
        if !view[from].is_null() {
            map.insert(to.into(), view[from].clone());
        }
    }
    if view["mapScale"].as_f64() != Some(1.0) {
        map.insert("scale".into(), view["mapScale"].clone());
    }
    let scene = json!({
        "entities": entities,
        "elapsed": view["elapsed"],
        "match": strip_nulls_except(&view["match"], "winner"),
        "map": Value::Object(map),
    });
    rounded(&scene, "", "")
}

fn strip_nulls_except(value: &Value, keep: &str) -> Value {
    Value::Object(
        value
            .as_object()
            .unwrap()
            .iter()
            .filter(|(key, item)| !item.is_null() || key.as_str() == keep)
            .map(|(key, item)| (key.clone(), item.clone()))
            .collect(),
    )
}

/// Fields whose JSON differs, deletions as null.
fn reference_changes(previous: Option<&Value>, next: &Value) -> Option<Value> {
    let next = next.as_object().unwrap();
    let mut changes = Map::new();
    let empty = Map::new();
    let before = previous.map_or(&empty, |value| value.as_object().unwrap());
    let keys: BTreeSet<&String> = next.keys().chain(before.keys()).collect();
    for key in keys {
        let old = before.get(key);
        let new = next.get(key);
        let equal = match (old, new) {
            (Some(a), Some(b)) => same(a, b),
            (None, None) => true,
            _ => false,
        };
        if previous.is_none() || !equal {
            changes.insert(key.clone(), new.cloned().unwrap_or(Value::Null));
        }
    }
    (!changes.is_empty()).then_some(Value::Object(changes))
}

fn reference_delta(previous: &Value, next: &Value) -> Value {
    let mut delta = Map::new();
    if let Some(changes) = reference_changes(Some(&previous["match"]), &next["match"]) {
        delta.insert("match".into(), changes);
    }
    let mut updates = Map::new();
    let mut removed = Map::new();
    for kind in ENTITY_TYPES {
        let earlier: BTreeMap<u64, &Value> = previous["entities"][kind]
            .as_array()
            .unwrap()
            .iter()
            .map(|entity| (entity["id"].as_u64().unwrap(), entity))
            .collect();
        let mut kind_updates = Map::new();
        for entity in next["entities"][kind].as_array().unwrap() {
            let id = entity["id"].as_u64().unwrap();
            if let Some(changes) = reference_changes(earlier.get(&id).copied(), entity) {
                kind_updates.insert(id.to_string(), changes);
            }
        }
        if !kind_updates.is_empty() {
            updates.insert(kind.into(), Value::Object(kind_updates));
        }
        let ids: BTreeSet<u64> = next["entities"][kind]
            .as_array()
            .unwrap()
            .iter()
            .map(|entity| entity["id"].as_u64().unwrap())
            .collect();
        let gone: Vec<Value> = previous["entities"][kind]
            .as_array()
            .unwrap()
            .iter()
            .map(|entity| entity["id"].as_u64().unwrap())
            .filter(|id| !ids.contains(id))
            .map(Value::from)
            .collect();
        if !gone.is_empty() {
            removed.insert(kind.into(), Value::Array(gone));
        }
    }
    if !updates.is_empty() {
        delta.insert("updates".into(), Value::Object(updates));
    }
    if !removed.is_empty() {
        delta.insert("removed".into(), Value::Object(removed));
    }
    Value::Object(delta)
}

fn seen_fields(scene: &Value, seen: &mut BTreeSet<String>) {
    for kind in ENTITY_TYPES {
        for entity in scene["entities"][kind].as_array().unwrap() {
            for field in entity.as_object().unwrap().keys() {
                seen.insert(format!("{kind}.{field}"));
            }
        }
    }
    for cover in scene["entities"]["covers"].as_array().unwrap() {
        if cover["hp"].is_null() {
            seen.insert("covers.hp=null".into());
        }
    }
}

const EXPECTED_FIELDS: [&str; 16] = [
    "covers.debrisSeed",
    "covers.timberHits",
    "covers.timberJoin",
    "covers.motion",
    "covers.hp=null",
    "fragments.shape",
    "fragments.dimensions",
    "fragments.material",
    "fragments.sourceKind",
    "fragments.timberPart",
    "fragments.createdAt",
    "fragments.wreck",
    "fragments.part",
    "fragments.team",
    "mines.ownerLife",
    "pickups.cooldownDuration",
];

#[test]
fn captured_scenes_and_field_deltas_match_the_schema_reference() {
    let mut seen = BTreeSet::new();
    let mut removals = 0;
    for (map, ticks) in [
        (MapId::Village, 450),
        (MapId::Harbor, 450),
        (MapId::Quarry, 450),
        (MapId::Superstress, 300),
    ] {
        let mut sim = room(map, &one_player());
        sim.start();
        let mut stream = StateStream::new("room", 1);
        let mut previous = parse(&Scene::capture(&sim).to_json());
        assert_same(&previous, &reference_scene(&sim), &format!("{map:?} start"));
        stream.full(&Scene::capture(&sim), 0, 0, &[]);
        for tick in 1..=ticks {
            if tick == 30 {
                // Collapsed towers leave seeded rubble; the rest leave debris and removals.
                let destructible: Vec<usize> = (0..sim.covers.len())
                    .filter(|&i| sim.covers[i].alive && sim.covers[i].destructible)
                    .collect();
                let towers = destructible
                    .iter()
                    .copied()
                    .filter(|&i| sim.covers[i].kind == CoverKind::Tower);
                let targets: Vec<usize> = towers
                    .chain(destructible.iter().copied().take(12))
                    .collect();
                for cover in targets {
                    if sim.covers[cover].alive {
                        sim.damage_cover(cover, 10000.0, 0, Team::Blue, None, None);
                    }
                }
            }
            let t = tick as f64;
            step(
                &mut sim,
                VehicleCommand {
                    move_x: (t / 40.0).sin(),
                    move_z: (t / 55.0).cos(),
                    aim: t / 25.0,
                    fire: true,
                    mine: tick % 60 == 0,
                    ammo_selection: None,
                },
            );
            if tick % 3 != 0 {
                continue;
            }
            let mut scene = Scene::capture(&sim);
            let text = scene.to_json();
            let captured = parse(&text);
            assert_same(
                &captured,
                &reference_scene(&sim),
                &format!("{map:?} scene at tick {tick}"),
            );
            let frame = parse(&stream.snapshot(&mut scene, tick, &[], &[]));
            let mut delta = Map::new();
            for key in ["match", "updates", "removed"] {
                if let Some(value) = frame.get(key) {
                    delta.insert(key.into(), value.clone());
                }
            }
            assert_same(
                &Value::Object(delta),
                &reference_delta(&previous, &captured),
                &format!("{map:?} delta at tick {tick}"),
            );
            removals += frame["removed"].as_object().map_or(0, |removed| {
                removed
                    .values()
                    .map(|ids| ids.as_array().unwrap().len())
                    .sum()
            });
            seen_fields(&captured, &mut seen);
            previous = captured;
        }
    }
    assert!(removals > 0, "Seeded matches remove entities");
    let missing: Vec<&str> = EXPECTED_FIELDS
        .iter()
        .copied()
        .filter(|field| !seen.contains(*field))
        .collect();
    assert!(
        missing.is_empty(),
        "Seeded matches exercise optional wire fields; missing {missing:?}"
    );
}

#[test]
fn optional_fields_can_appear_and_disappear_between_unchanged_fields() {
    let mut sim = room(MapId::Village, &one_player());
    sim.covers[0].debris_seed = None;
    let cover_id = sim.covers[0].id.to_string();
    let mut stream = StateStream::new("room", 1);
    stream.full(&Scene::capture(&sim), 0, 0, &[]);
    for (index, seed) in [Some(12.0), None, Some(34.0)].into_iter().enumerate() {
        sim.covers[0].debris_seed = seed;
        let mut scene = Scene::capture(&sim);
        let frame = parse(&stream.snapshot(&mut scene, index as u64 + 1, &[], &[]));
        assert_same(
            &frame["updates"],
            &json!({"covers": {&cover_id: {"debrisSeed": seed}}}),
            "only the optional field changes",
        );
    }
}

#[test]
fn snapshot_scratch_does_not_leak_updates_removals_or_events_into_the_next_frame() {
    let mut sim = room(MapId::Village, &one_player());
    let mut scene = Scene::capture(&sim);
    let mut stream = StateStream::new("room", 1);
    stream.full(&scene, 0, 0, &[]);
    let removed = sim.tanks.pop().unwrap().id;
    sim.tanks.reverse(); // Wire keys must still be in numeric order.
    for tank in &mut sim.tanks {
        tank.hp -= 1.0;
    }
    sim.match_state.scores[0] = 2;
    scene.capture_from(&sim);
    let events = [
        r#"{"label":"one"}"#.to_string(),
        r#"{"label":"two"}"#.to_string(),
    ];
    let wire = stream.snapshot(&mut scene, 1, &events, &[]);
    let frame = parse(&wire);
    assert_eq!(frame["removed"]["tanks"], json!([removed]));
    assert_eq!(frame["events"], json!([{"label":"one"}, {"label":"two"}]));
    let mut ids: Vec<_> = sim.tanks.iter().map(|tank| tank.id).collect();
    ids.sort_unstable();
    let positions: Vec<_> = ids
        .iter()
        .map(|id| wire.find(&format!("\"{id}\":{{")).unwrap())
        .collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    for tick in 2..5 {
        scene.capture_from(&sim);
        assert_same(
            &parse(&stream.snapshot(&mut scene, tick, &[], &[])),
            &json!({ "seq": tick, "tick": tick, "elapsed": sim.elapsed }),
            "unchanged frames contain no stale scratch data",
        );
    }
}
