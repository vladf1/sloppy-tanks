//! Baselines, field deltas and the client mirror (`tests/replication.test.ts`), and the
//! host's scene capture against an independent schema reference
//! (`tests/scene-capture.test.ts`).

mod net_support;

use std::collections::{BTreeMap, BTreeSet};

use net_support::{apply_batch, assert_same, batch, same};
use serde_json::{Map, Value, json};
use sloppy_core::net::multiplayer_simulation::{MultiplayerOptions, create_multiplayer_simulation};
use sloppy_core::net::replication::{
    Baseline, BinaryMessage, PATHS_SECTION, REMOVED_SECTION, StateMirror, StateStream, TimedEvent,
    UPDATES_SECTION, read_binary_message,
};
use sloppy_core::net::scene_codec::{
    ENTITY_FIELDS, ENTITY_TYPES, MINE_FIELDS, MINES, MirrorScene, Scene, TANKS, mine, tank,
};
use sloppy_core::net::shot_paths::{PathEntry, ShotLaunch, ShotPath};
use sloppy_core::net::wire::{WireRecord, put_signed, put_varint, write_changes};
use sloppy_core::net::wire_view::WireView;
use sloppy_core::sim::debris_cleanup::DEBRIS_CLEANUP_SECONDS;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::types::{
    CoverKind, FragmentShape, PlayerAssignment, Team, VehicleCommand, VehicleKind, Weapon,
};
use sloppy_core::sim::types::{SimEvent, SimEventType};
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

fn read_scene(scene: &Scene) -> MirrorScene {
    MirrorScene::from_scene(scene.clone()).unwrap()
}

/// A baseline message of the stream's scene, starting the stream with `scene`.
fn full(stream: &mut StateStream, scene: &Scene, tick: u64) -> Vec<u8> {
    stream.full(tick, 0, [], || scene.clone())
}

fn baseline(message: &[u8]) -> Baseline<'_> {
    match read_binary_message(message).unwrap() {
        BinaryMessage::Full(baseline) => baseline,
        BinaryMessage::Snapshot(_) => panic!("not a baseline"),
    }
}

/// The next frame as a one-frame batch message.
fn frame(stream: &mut StateStream, scene: &mut Scene, tick: u64) -> Vec<u8> {
    let body = stream.snapshot(scene, tick, &[], &[]);
    batch(1, 0, stream.seq, &[(tick, body)])
}

/// A client's JSON view, started from a baseline message.
fn view_from(full: &[u8]) -> WireView {
    let mut view = WireView::default();
    view.binary(full).unwrap();
    view
}

/// A batch's single frame as the former JSON.
fn frame_json(view: &mut WireView, message: &[u8]) -> Value {
    view.binary(message).unwrap()["snapshots"][0].clone()
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
        let full_message = full(&mut stream, &Scene::capture(&sim), 0);
        assert!(full_message.len() < 160_000, "Full-state wire budget");
        mirror
            .apply_full(&baseline(&full_message), "room", 1)
            .unwrap();
        let mut view = view_from(&full_message);
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
            let frame = frame(&mut stream, &mut next, tick);
            assert!(frame.len() < 128_000, "Burst snapshot wire budget");
            removed |= frame_json(&mut view, &frame).get("removed").is_some();
            apply_batch(&mut mirror, &frame);
            assert_eq!(mirror.state.as_ref().unwrap().to_scene(), scene);
            assert_same(
                &mirror.state.as_ref().unwrap().to_value(),
                &scene.to_json(),
                "mirror equals the capture",
            );
            let viewer = human(&sim);
            let expected = read_scene(&scene).render(viewer).unwrap();
            assert_eq!(mirror.render(viewer).unwrap(), expected);
            if tick == 60 || tick == 93 {
                let mut late = StateMirror::default();
                late.apply_full(&baseline(&full(&mut stream, &scene, tick)), "room", 1)
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
    let mut view = view_from(&full(&mut stream, &scene, 0));
    let mut same_scene = scene.clone();
    let message = frame(&mut stream, &mut same_scene, 1);
    // Type, round, tick, ack, seq, count; then tick back, elapsed and an empty section mask.
    assert_eq!(message.len(), 6 + 3);
    let frame = frame_json(&mut view, &message);
    let keys: Vec<&String> = frame.as_object().unwrap().keys().collect();
    assert_eq!(keys.len(), 3);
    for key in ["seq", "tick", "elapsed"] {
        assert!(frame.get(key).is_some());
    }
    let value = scene.to_json();
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
    sim.fragments[0].life = 5.0;
    let mut stream = StateStream::new("room", 1);
    let mut view = view_from(&full(&mut stream, &Scene::capture(&sim), 0));
    let mut life_update = |stream: &mut StateStream, sim: &Simulation, tick| {
        let message = frame(stream, &mut Scene::capture(sim), tick);
        frame_json(&mut view, &message)["updates"]["fragments"][piece.to_string()]["life"].clone()
    };
    sim.fragments[0].life = 4.0;
    assert_eq!(life_update(&mut stream, &sim, 1), Value::Null);
    sim.fragments[0].life = DEBRIS_CLEANUP_SECONDS / 2.0;
    assert_eq!(
        life_update(&mut stream, &sim, 2).as_f64(),
        Some(DEBRIS_CLEANUP_SECONDS / 2.0)
    );
}

/// A hand-written frame body: elapsed unchanged, then `sections` and their bytes.
fn frame_body(sections: u64, write: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    let mut body = Vec::new();
    put_signed(&mut body, 0);
    put_varint(&mut body, sections);
    write(&mut body);
    body
}

/// One kind's records: the kind mask, the count, then each id difference and record.
fn records(out: &mut Vec<u8>, kind: usize, records: &[(u32, Vec<u8>)]) {
    put_varint(out, 1 << kind);
    put_varint(out, records.len() as u64);
    let mut last = 0;
    for (id, changes) in records {
        put_varint(out, u64::from(id - last));
        last = *id;
        out.extend_from_slice(changes);
    }
}

fn hp_change(hp: i64) -> Vec<u8> {
    let mut out = Vec::new();
    put_varint(&mut out, 1 << (tank::HP + 1));
    put_signed(&mut out, hp);
    out
}

#[test]
fn mirror_rejects_corrupt_or_skipped_deltas_atomically_and_a_full_baseline_repairs_it() {
    let sim = room(MapId::Village, &[]);
    let state = Scene::capture(&sim);
    let mut stream = StateStream::new("r", 1);
    let mut mirror = StateMirror::default();
    let full_message = full(&mut stream, &state, 0);
    mirror.apply_full(&baseline(&full_message), "r", 1).unwrap();
    let before = mirror.state.as_ref().unwrap().to_value();
    let tank = sim.tanks[0].id;
    // A team outside 0..=1 fails the tank reader after the hp change already decoded.
    let bad = frame_body(UPDATES_SECTION, |out| {
        let mut changes = Vec::new();
        put_varint(
            &mut changes,
            (1 << (tank::HP + 1)) | (1 << (tank::TEAM + 1)),
        );
        put_signed(&mut changes, -100);
        put_varint(&mut changes, 5);
        records(out, TANKS, &[(tank, changes)]);
    });
    let apply = |mirror: &mut StateMirror, message: &[u8]| {
        let BinaryMessage::Snapshot(mut batch) = read_binary_message(message).unwrap() else {
            unreachable!()
        };
        mirror.apply_snapshot(&mut batch)
    };
    assert!(apply(&mut mirror, &batch(1, 0, 1, &[(3, bad.clone())])).is_none());
    assert_eq!(mirror.state.as_ref().unwrap().to_value(), before);
    assert!(mirror.needs_full);
    mirror.apply_full(&baseline(&full_message), "r", 1).unwrap();
    let good = frame_body(UPDATES_SECTION, |out| {
        records(out, TANKS, &[(tank, hp_change(-100))])
    });
    assert!(apply(&mut mirror, &batch(1, 0, 9, &[(3, good.clone())])).is_none());
    assert_eq!(mirror.state.as_ref().unwrap().to_value(), before);
    mirror.apply_full(&baseline(&full_message), "r", 1).unwrap();
    let mut truncated = batch(1, 0, 1, &[(3, good)]);
    truncated.pop();
    assert!(apply(&mut mirror, &truncated).is_none());
    assert_eq!(mirror.state.as_ref().unwrap().to_value(), before);
    mirror.apply_full(&baseline(&full_message), "r", 1).unwrap();
    assert!(!mirror.needs_full);
}

/// A frame of path entries alone, as a one-frame batch.
fn paths_frame(seq: u64, tick: u64, entries: &[PathEntry]) -> Vec<u8> {
    let body = frame_body(PATHS_SECTION, |out| {
        put_varint(out, entries.len() as u64);
        for entry in entries {
            entry.write_binary(out, tick);
        }
    });
    batch(1, 0, seq, &[(tick, body)])
}

fn path(id: u32, tick: f64, x: f64, z: f64, vx: f64, vz: f64, launch: ShotLaunch) -> ShotPath {
    ShotPath {
        id,
        tick,
        x,
        z,
        vx,
        vz,
        launch,
    }
}

#[test]
fn projectile_paths_apply_in_order_inherit_their_launch_and_a_bad_entry_rejects_the_frame() {
    let sim = room(MapId::Village, &[]);
    let state = Scene::capture(&sim);
    let viewer = sim.tanks[0].id;
    let mut mirror = StateMirror::default();
    let full_message = full(&mut StateStream::new("r", 1), &state, 0);
    mirror.apply_full(&baseline(&full_message), "r", 1).unwrap();
    let apply = |mirror: &mut StateMirror, message: &[u8]| {
        let BinaryMessage::Snapshot(mut batch) = read_binary_message(message).unwrap() else {
            unreachable!()
        };
        mirror.apply_snapshot(&mut batch)
    };
    let ricochet = ShotLaunch {
        team: Team::Red,
        weapon: Weapon::Ricochet,
        y: Some(1.2),
        visual_y: Some(1.6),
        thrust: None,
    };
    let launch = PathEntry::Launch(path(900, 0.5, 0.0, 0.0, 30.0, 0.0, ricochet));
    assert!(apply(&mut mirror, &paths_frame(1, 3, &[launch])).is_some());
    let shot = mirror.render(viewer).unwrap().shots[0];
    assert_eq!(
        (shot.x, shot.z),
        (30.0 * 2.5 / 60.0, 0.0),
        "drawn at the frame tick"
    );
    // A change sends no launch fields; the reader's placeholder launch must not survive.
    let blank = ShotLaunch {
        team: Team::Blue,
        weapon: Weapon::Standard,
        y: None,
        visual_y: None,
        thrust: None,
    };
    let bounce = PathEntry::Change(path(900, 4.0, 1.75, 0.0, -30.0, 0.0, blank));
    let extras = apply(&mut mirror, &paths_frame(2, 6, &[bounce])).unwrap();
    let PathEntry::Change(changed) = extras.paths[0] else {
        panic!("a new path for a flying shell");
    };
    assert_eq!(
        (
            changed.launch.weapon,
            changed.launch.team,
            changed.launch.visual_y
        ),
        (Weapon::Ricochet, Team::Red, Some(1.6)),
        "later paths keep the launch fields"
    );
    let shells = |mirror: &StateMirror| mirror.shots.paths.clone();
    let before = shells(&mirror);
    let change = |id, tick| PathEntry::Change(path(id, tick, 0.0, 0.0, 1.0, 0.0, blank));
    for (bad, why) in [
        (launch, "a second launch"),
        (change(901, 5.0), "an unknown shell"),
        (
            PathEntry::End { id: 901, tick: 5.0 },
            "the end of an unknown shell",
        ),
        (change(900, 3.5), "an earlier path"),
        (
            PathEntry::End { id: 900, tick: 3.9 },
            "an end before the path",
        ),
        (
            PathEntry::End { id: 900, tick: 9.5 },
            "an end after the frame",
        ),
        (change(900, 9.5), "a future path"),
    ] {
        // A valid launch first: the frame is rejected whole, not up to the bad entry.
        let standard = ShotLaunch {
            team: Team::Blue,
            ..blank
        };
        let other = PathEntry::Launch(path(950, 7.0, 5.0, 5.0, 0.0, 9.0, standard));
        let mut probe = mirror.clone();
        assert!(
            apply(&mut probe, &paths_frame(3, 9, &[other, bad])).is_none(),
            "{why}"
        );
        assert_eq!(shells(&probe), before, "{why} leaves the shells");
        assert_eq!(probe.seq, 2, "{why} leaves the stream");
    }
    let end = PathEntry::End { id: 900, tick: 7.0 };
    assert!(apply(&mut mirror, &paths_frame(3, 9, &[end])).is_some());
    assert!(mirror.render(viewer).unwrap().shots.is_empty());
}

#[test]
fn mirror_allows_one_change_per_entity_of_each_kind_in_a_frame() {
    let sim = room(MapId::Village, &[]);
    let state = Scene::capture(&sim);
    let mut mirror = StateMirror::default();
    let full_message = full(&mut StateStream::new("r", 1), &state, 0);
    mirror.apply_full(&baseline(&full_message), "r", 1).unwrap();
    let before = mirror.state.as_ref().unwrap().to_value();
    let tank = sim.tanks[0].id;
    let hp = mirror.state.as_ref().unwrap().tanks.records[0].value.hp;
    let conflicting = frame_body(UPDATES_SECTION | REMOVED_SECTION, |out| {
        records(out, TANKS, &[(tank, hp_change(100 - (hp * 100.0) as i64))]);
        put_varint(out, 1 << TANKS);
        put_varint(out, 1);
        put_varint(out, u64::from(tank));
    });
    let message = batch(1, 0, 1, &[(1, conflicting)]);
    let BinaryMessage::Snapshot(mut conflict) = read_binary_message(&message).unwrap() else {
        unreachable!()
    };
    assert!(mirror.apply_snapshot(&mut conflict).is_none());
    assert_eq!(mirror.state.as_ref().unwrap().to_value(), before);
    assert!(mirror.needs_full);
    mirror.apply_full(&baseline(&full_message), "r", 1).unwrap();
    // Ids are claimed per kind, so another kind may reuse a tank's id in the same frame.
    let mut mine_record = WireRecord::default();
    mine_record.reset(tank, MINE_FIELDS.len());
    mine_record.set_fixed(mine::ARM, 0.0, 100.0);
    mine_record.set_fixed(mine::LIFE, 1.0, 100.0);
    mine_record.set_fixed(mine::X, 0.0, 1000.0);
    mine_record.set_fixed(mine::Z, 0.0, 1000.0);
    mine_record.set_count(mine::OWNER, u64::from(tank));
    mine_record.set_count(mine::TEAM, 0);
    let mut mine_changes = Vec::new();
    write_changes(MINE_FIELDS, None, &mine_record, &mut mine_changes);
    let shared = frame_body(UPDATES_SECTION, |out| {
        put_varint(out, (1 << TANKS) | (1 << MINES));
        for (id, changes) in [
            (tank, hp_change(100 - (hp * 100.0) as i64)),
            (tank, mine_changes),
        ] {
            put_varint(out, 1);
            put_varint(out, u64::from(id));
            out.extend_from_slice(&changes);
        }
    });
    apply_batch(&mut mirror, &batch(1, 0, 1, &[(1, shared)]));
    let scene = mirror.state.as_ref().unwrap();
    assert_eq!(scene.tanks.records[0].value.hp, 1.0);
    assert_eq!(scene.mines.records.last().unwrap().wire, mine_record);
    // The removal that conflicted with the update above is valid in a frame of its own.
    let removal = frame_body(REMOVED_SECTION, |out| {
        put_varint(out, 1 << TANKS);
        put_varint(out, 1);
        put_varint(out, u64::from(tank));
    });
    apply_batch(&mut mirror, &batch(1, 0, 2, &[(1, removal)]));
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

/// A kind's top-level wire fields, `id` first, as the JSON records named them.
fn field_names(kind: usize) -> Vec<&'static str> {
    let mut names = vec!["id"];
    for field in ENTITY_FIELDS[kind] {
        let name = field.name.split('.').next().unwrap();
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
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
                        select(&Value::Object(record), &field_names(kind), nullable)
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
        let mut previous = Scene::capture(&sim).to_json();
        assert_same(&previous, &reference_scene(&sim), &format!("{map:?} start"));
        let mut view = view_from(&full(&mut stream, &Scene::capture(&sim), 0));
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
            if tick == 240 && !sim.mines.is_empty() {
                // Shells no longer travel as records, so make sure a removal is encoded.
                sim.mines.remove(0);
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
            let captured = scene.to_json();
            assert_same(
                &captured,
                &reference_scene(&sim),
                &format!("{map:?} scene at tick {tick}"),
            );
            let message = frame(&mut stream, &mut scene, tick);
            let frame = frame_json(&mut view, &message);
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
    let mut view = view_from(&full(&mut stream, &Scene::capture(&sim), 0));
    for (index, seed) in [Some(12.0), None, Some(34.0)].into_iter().enumerate() {
        sim.covers[0].debris_seed = seed;
        let mut scene = Scene::capture(&sim);
        let message = frame(&mut stream, &mut scene, index as u64 + 1);
        let frame = frame_json(&mut view, &message);
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
    let full_message = full(&mut stream, &scene, 0);
    let mut view = view_from(&full_message);
    let mut mirror = StateMirror::default();
    mirror
        .apply_full(&baseline(&full_message), "room", 1)
        .unwrap();
    let removed = sim.tanks.pop().unwrap().id;
    sim.tanks.reverse(); // Frames still list records by ascending id.
    for tank in &mut sim.tanks {
        tank.hp -= 1.0;
    }
    sim.match_state.scores[0] = 2;
    scene.capture_from(&sim);
    let events: Vec<TimedEvent> = ["one", "two"]
        .into_iter()
        .enumerate()
        .map(|(index, label)| TimedEvent {
            event_id: index as u64 + 1,
            tick: 1.0,
            event: SimEvent {
                label: Some(label.into()),
                ..SimEvent::at(SimEventType::Notice, 0.0, 0.0)
            },
        })
        .collect();
    let body = stream.snapshot(&mut scene, 1, &events, &[]);
    let message = batch(1, 0, 1, &[(1, body)]);
    let BinaryMessage::Snapshot(mut decoded) = read_binary_message(&message).unwrap() else {
        unreachable!()
    };
    let decoded = mirror.decode(&mut decoded, true).unwrap();
    let ids: Vec<u32> = decoded.changed.iter().map(|change| change.id).collect();
    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
    let changes = frame_json(&mut view, &message);
    assert_eq!(changes["removed"]["tanks"], json!([removed]));
    let labels: Vec<&Value> = changes["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| &event["event"]["label"])
        .collect();
    assert_eq!(labels, [&json!("one"), &json!("two")]);
    for tick in 2..5 {
        scene.capture_from(&sim);
        let message = frame(&mut stream, &mut scene, tick);
        assert_same(
            &frame_json(&mut view, &message),
            &json!({ "seq": tick, "tick": tick, "elapsed": sim.elapsed }),
            "unchanged frames contain no stale scratch data",
        );
    }
}
