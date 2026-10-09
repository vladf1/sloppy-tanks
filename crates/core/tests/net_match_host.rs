//! The room host's policy (`tests/match-host.test.ts`): seats, humans-only rooms, reconnects,
//! host transfer, round boundaries, lifetime rules, baselines and snapshot streams.

mod net_support;
mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use net_support::{
    Harness, apply_batch, batch, first_seq, harness, harness_at, last_seq, set_translation,
};
use serde_json::{Value, json};
use sloppy_core::net::protocol::{
    CONTENT_VERSION, FULL_MESSAGE, MAX_BATCH_FRAMES, MAX_BATTLE_OVERRUN_MS, MAX_ROOM_MS,
    MAX_ROUND_MINUTES, Message, PROTOCOL_VERSION, RoomPhase, SNAPSHOT_MESSAGE,
};
use sloppy_core::net::replication::{
    BinaryMessage, StateMirror, StateStream, TimedEvent, read_binary_message,
};
use sloppy_core::net::scene_codec::Scene;
use sloppy_core::net::shot_paths::{MAX_LIVE_PATHS, PATH_TOLERANCE, ShotPath};
use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::types::{CoverKind, Driver, MatchPhase, Shot, SimEvent, SimEventType, Team};
use support::clear_arena;

fn mirror_from(h: &Harness, name: &str) -> (StateMirror, Value) {
    (h.mirror_from_latest_full(name), h.latest(name, "full"))
}

fn apply_latest(h: &Harness, name: &str, mirror: &mut StateMirror) {
    apply_batch(mirror, &h.latest_binary(name, SNAPSHOT_MESSAGE));
}

fn capture(h: &Harness) -> Value {
    Scene::capture(h.host.simulation.as_ref().unwrap()).to_json()
}

fn tank_id(h: &mut Harness, index: usize) -> u32 {
    h.sim().tanks[index].id
}

fn index_of(h: &mut Harness, id: u32) -> usize {
    h.sim().tank_index(id).expect("tank exists")
}

#[test]
fn humans_only_handles_pause_reconnect_late_join_death_and_departures_without_fill_bots() {
    let mut h = harness();
    h.join("alice", json!({ "team": 0 }));
    h.join("bob", json!({ "team": 1 }));
    h.action(
        "alice",
        "settings",
        json!({ "mapMode": "harbor", "difficulty": "normal", "humansOnly": true }),
    );
    assert_eq!(h.latest("bob", "lobby")["settings"]["humansOnly"], true);
    h.action("alice", "start", json!({}));
    assert_eq!(h.sim().tanks.len(), 2);
    let alice = h.tank_of("alice");
    let alice = tank_id(&mut h, alice);
    let bob = h.tank_of("bob");
    let bob = tank_id(&mut h, bob);
    h.action("alice", "suspend", json!({}));
    assert_eq!(h.latest("alice", "control")["driver"], "idle");
    for _ in 0..110 {
        h.advance();
    }
    assert!(h.sim().tanks.iter().all(|tank| tank.driver == Driver::Idle));
    assert_eq!(h.sim().shots_fired, 0);
    h.action("alice", "resume", json!({}));
    let index = index_of(&mut h, alice);
    assert_eq!(h.sim().tanks[index].driver, Driver::Human);
    let token = h.latest("bob", "welcome")["token"].clone();
    h.disconnect("bob");
    let index = index_of(&mut h, bob);
    assert_eq!(h.sim().tanks[index].driver, Driver::Idle);
    h.join(
        "bob-again",
        json!({ "token": token, "roomEpoch": "test-room" }),
    );
    assert_eq!(h.latest("bob-again", "control")["tankId"], bob);
    assert_eq!(h.sim().tanks[index].driver, Driver::Human);
    let bob_body = h.sim().tanks[index].body;
    h.action("bob-again", "leave", json!({}));
    assert_eq!(h.sim().tanks.len(), 1);
    assert!(
        !h.sim().world.bodies.contains(bob_body),
        "leaving removes the hull collider too"
    );
    h.advance();
    let (mut mirror, full) = mirror_from(&h, "alice");
    let full_seq = full["seq"].as_u64().unwrap();
    for message in h.binary("alice", SNAPSHOT_MESSAGE) {
        if first_seq(&message) > full_seq {
            apply_batch(&mut mirror, &message);
        }
    }
    assert_eq!(
        mirror.render(alice).unwrap().tanks.len(),
        1,
        "mirror removes departed tanks"
    );
    h.join("carol", json!({ "team": 1, "kind": "heavy" }));
    let carol = h.tank_of("carol");
    assert_eq!(h.sim().tanks.len(), 2);
    assert_eq!(h.sim().tanks[carol].name, "carol");
    assert_eq!(h.sim().tanks[carol].kind.as_str(), "heavy");
    assert_ne!(
        h.sim().tanks[carol].id,
        bob,
        "new occupant cannot inherit old ordnance ownership"
    );
    h.sim().tanks[carol].protection = 0.0;
    let alice_index = index_of(&mut h, alice);
    let (team, life) = (
        h.sim().tanks[alice_index].team,
        h.sim().tanks[alice_index].life,
    );
    h.sim()
        .damage_tank(carol, 10000.0, alice, team, Some(life), None);
    h.advance();
    let carol = h.tank_of("carol");
    assert!(!h.sim().tanks[carol].alive);
    h.action("carol", "leave", json!({}));
    h.advance();
    assert_eq!(
        h.sim().tanks.len(),
        1,
        "dead departed players cannot respawn"
    );
    h.action("alice", "end", json!({}));
    h.action(
        "alice",
        "settings",
        json!({ "mapMode": "village", "difficulty": "normal", "humansOnly": false }),
    );
    h.action("alice", "start", json!({}));
    assert_eq!(
        h.sim().tanks.len(),
        12,
        "bots can be restored for the next round"
    );
}

#[test]
fn humans_only_removes_expired_reservations_and_accepts_new_occupants_without_growing_the_roster() {
    let mut h = harness();
    h.join("alice", json!({ "team": 0 }));
    h.join("bob", json!({ "team": 1 }));
    h.action(
        "alice",
        "settings",
        json!({ "mapMode": "village", "difficulty": "easy", "humansOnly": true }),
    );
    h.action("alice", "start", json!({}));
    let bob_body = h.sim().tanks[1].body;
    h.disconnect("bob");
    for _ in 0..601 {
        h.advance();
        for messages in h.messages.values_mut() {
            let excess = messages.len().saturating_sub(12);
            messages.drain(..excess);
        }
        for texts in h.texts.values_mut() {
            texts.clear();
        }
        for wire in h.wire.values_mut() {
            wire.clear();
        }
    }
    assert_eq!(h.sim().tanks.len(), 1);
    assert!(!h.sim().world.bodies.contains(bob_body));
    let bodies = h.sim().world.bodies.len();
    for i in 0..10 {
        let name = format!("late-{i}");
        h.join(&name, json!({ "team": 1 }));
        assert_eq!(h.sim().tanks.len(), 2);
        assert_eq!(h.sim().world.bodies.len(), bodies + 1);
        h.action(&name, "leave", json!({}));
        assert_eq!(h.sim().tanks.len(), 1);
        assert_eq!(h.sim().world.bodies.len(), bodies);
    }
}

fn input(control_epoch: &Value, seq: u64, observed: u64, extra: Value) -> Value {
    net_support::merged(
        json!({
            "controlEpoch": control_epoch,
            "seq": seq,
            "observedTick": observed,
            "moveX": 0,
            "moveZ": 0,
            "aim": { "angle": 0 },
        }),
        extra,
    )
}

#[test]
fn the_host_counts_held_input_that_lapses_before_the_next_input_arrives() {
    let mut h = harness();
    h.join("alice", json!({}));
    h.action("alice", "start", json!({}));
    let epoch = h.latest("alice", "control")["controlEpoch"].clone();
    let tick = h.host.tick();
    h.action(
        "alice",
        "input",
        input(&epoch, 1, tick, json!({ "moveX": 1 })),
    );
    h.advance();
    assert_eq!(h.host.input_lapses(), 0);
    // Pings keep arriving, but only input renews the lease.
    for _ in 0..5 {
        h.advance();
    }
    assert_eq!(h.host.input_lapses(), 1);
    let tick = h.host.tick();
    h.action("alice", "input", input(&epoch, 2, tick, json!({})));
    for _ in 0..10 {
        h.advance();
    }
    assert_eq!(h.host.input_lapses(), 1, "released controls never lapse");
}

/// Every projectile path entry `name` received after its latest baseline.
fn path_entries(h: &Harness, name: &str) -> Vec<Value> {
    h.all(name)
        .iter()
        .filter(|message| message["type"] == "snapshot")
        .flat_map(|message| message["snapshots"].as_array().cloned().unwrap_or_default())
        .flat_map(|snap| snap["paths"].as_array().cloned().unwrap_or_default())
        .collect()
}

#[test]
fn a_shell_in_straight_flight_is_sent_once_and_its_path_finds_it_frames_later() {
    let mut h = harness();
    h.join("alice", json!({}));
    h.action("alice", "start", json!({}));
    let keep = h.all_tanks();
    clear_arena(h.sim(), &keep);
    let human = h.sim().human_index().unwrap();
    set_translation(h.sim(), human, 0.0, 0.65, 0.0);
    let epoch = h.latest("alice", "control")["controlEpoch"].clone();
    let tick = h.host.tick();
    h.action(
        "alice",
        "input",
        input(&epoch, 1, tick, json!({ "fire": true })),
    );
    for _ in 0..4 {
        h.advance();
    }
    let entries = path_entries(&h, "alice");
    let tick = h.host.tick() as f64;
    let sim = h.host.simulation.as_ref().unwrap();
    let owner = sim.tanks[human].id;
    let flying: Vec<_> = sim
        .shots
        .iter()
        .filter(|shot| shot.owner == owner)
        .collect();
    assert!(!flying.is_empty());
    for shot in flying {
        let sent: Vec<&Value> = entries
            .iter()
            .filter(|entry| entry["id"] == shot.id)
            .collect();
        assert_eq!(sent.len(), 1, "one launch, nothing per frame");
        assert!(sent[0]["weapon"].is_string(), "the launch names its shell");
        let path = ShotPath::read(sent[0].as_object().unwrap(), None).unwrap();
        let drawn = path.at(tick);
        assert!((drawn.x - shot.x).hypot(drawn.z - shot.z) < 0.01);
    }
}

#[test]
fn ending_the_round_ends_every_shell_in_flight_where_it_stopped() {
    let mut h = harness();
    h.join("alice", json!({}));
    h.action("alice", "start", json!({}));
    let keep = h.all_tanks();
    clear_arena(h.sim(), &keep);
    let human = h.sim().human_index().unwrap();
    set_translation(h.sim(), human, 0.0, 0.65, 0.0);
    let epoch = h.latest("alice", "control")["controlEpoch"].clone();
    let tick = h.host.tick();
    h.action(
        "alice",
        "input",
        input(&epoch, 1, tick, json!({ "fire": true })),
    );
    h.advance();
    h.advance();
    assert!(!h.sim().shots.is_empty(), "a shell is in flight");
    h.action("alice", "end", json!({}));
    let entries = path_entries(&h, "alice");
    let sim = h.host.simulation.as_ref().unwrap();
    for shot in &sim.shots {
        let launch = entries
            .iter()
            .find(|entry| entry["id"] == shot.id && entry["weapon"].is_string())
            .expect("the shell was launched");
        let end = entries
            .iter()
            .find(|entry| entry["id"] == shot.id && entry["end"].is_number())
            .expect("the frozen round ends the shell's path");
        let path = ShotPath::read(launch.as_object().unwrap(), None).unwrap();
        let drawn = path.at(end["end"].as_f64().unwrap());
        assert!(
            (drawn.x - shot.x).hypot(drawn.z - shot.z) < 0.01,
            "and it ends where the shell stopped"
        );
    }
}

/// Applies the snapshot batches `name` received after the first `seen`, asserting each
/// frame applies, and counts them in `seen`.
fn apply_new(h: &Harness, name: &str, mirror: &mut StateMirror, seen: &mut usize) {
    let batches = h.binary(name, SNAPSHOT_MESSAGE);
    for bytes in &batches[*seen..] {
        apply_batch(mirror, bytes);
    }
    *seen = batches.len();
}

#[test]
fn shells_past_the_path_limit_fly_undrawn_and_are_drawn_once_paths_free_up() {
    let mut h = harness();
    h.join("alice", json!({}));
    h.action(
        "alice",
        "settings",
        json!({ "mapMode": "harbor", "difficulty": "normal", "humansOnly": true }),
    );
    h.action("alice", "start", json!({}));
    let keep = h.all_tanks();
    clear_arena(h.sim(), &keep);
    let human = h.sim().human_index().unwrap();
    set_translation(h.sim(), human, 0.0, 0.65, 0.0);
    let mut mirror = h.mirror_from_latest_full("alice");
    let mut seen = h.binary("alice", SNAPSHOT_MESSAGE).len();
    // A grid of slow friendly shells away from the only tank: the first 300 expire soon,
    // the rest outlive them, and together they pass the limit.
    let short_lived = 300;
    let total = MAX_LIVE_PATHS + 88;
    let sim = h.sim();
    let (owner, team) = (sim.tanks[human].id, sim.tanks[human].team);
    for i in 0..total {
        let id = sim.next_id;
        sim.next_id += 1;
        sim.shots.push(Shot {
            id,
            x: (i % 30) as f64 - 15.0,
            z: 5.0 + (i / 30) as f64,
            y: Some(1.0),
            vz: 2.0,
            owner,
            team,
            damage: 1.0,
            life: if i < short_lived { 0.3 } else { 3.0 },
            ..Shot::default()
        });
    }
    h.advance();
    apply_new(&h, "alice", &mut mirror, &mut seen);
    assert_eq!(h.sim().shots.len(), total, "every shell flies");
    assert_eq!(
        mirror.shots.paths.len(),
        MAX_LIVE_PATHS,
        "only the limit is drawn"
    );
    h.action("alice", "resync", json!({}));
    let resynced = h.mirror_from_latest_full("alice");
    assert_eq!(resynced.shots.paths.len(), MAX_LIVE_PATHS);
    assert!(!mirror.needs_full && !resynced.needs_full);

    for _ in 0..8 {
        h.advance();
    }
    apply_new(&h, "alice", &mut mirror, &mut seen);
    assert!(!mirror.needs_full);
    let tick = h.host.tick() as f64;
    let sim = h.host.simulation.as_ref().unwrap();
    assert_eq!(
        sim.shots.len(),
        total - short_lived,
        "the short-lived expired"
    );
    assert_eq!(
        mirror.shots.paths.len(),
        sim.shots.len(),
        "every shell is drawn"
    );
    for shot in &sim.shots {
        let path = mirror
            .shots
            .paths
            .iter()
            .find(|path| path.id == shot.id)
            .expect("a path for the shell");
        let drawn = path.at(tick);
        assert!((drawn.x - shot.x).hypot(drawn.z - shot.z) <= PATH_TOLERANCE + 0.01);
    }
}

#[test]
fn two_seats_drive_independently_reconnect_revokes_the_old_socket_and_host_transfer_persists() {
    let mut h = harness();
    h.join("alice", json!({ "kind": "scout", "team": 0 }));
    h.join("bob", json!({ "kind": "heavy", "team": 1 }));
    h.action("alice", "start", json!({}));
    let token = h.latest("alice", "welcome")["token"].clone();
    let first = h.latest("alice", "control");
    let second = h.latest("bob", "control");
    let a = first["tankId"].as_u64().unwrap() as u32;
    let b = second["tankId"].as_u64().unwrap() as u32;
    let keep = h.all_tanks();
    clear_arena(h.sim(), &keep);
    let ai = index_of(&mut h, a);
    let bi = index_of(&mut h, b);
    set_translation(h.sim(), ai, -10.0, 0.65, 0.0);
    set_translation(h.sim(), bi, 10.0, 0.65, 0.0);
    for seq in 1..=20 {
        for (client, control, move_z) in [("alice", &first, 1), ("bob", &second, -1)] {
            let tick = h.host.tick();
            h.action(
                client,
                "input",
                input(
                    &control["controlEpoch"],
                    seq,
                    tick,
                    json!({ "moveZ": move_z, "fire": false, "actions": [] }),
                ),
            );
        }
        h.advance();
    }
    let sim = h.sim();
    let az = sim.body_translation(sim.tanks[ai].body).z;
    let bz = sim.body_translation(sim.tanks[bi].body).z;
    assert!(az > 3.0, "alice moved to {az}");
    assert!(bz < -2.0, "bob moved to {bz}");
    h.disconnect("alice");
    assert_eq!(h.sim().tanks[ai].driver, Driver::Bot);
    assert_eq!(
        h.latest("bob", "lobby")["hostId"],
        h.latest("bob", "welcome")["playerId"]
    );
    h.join(
        "alice-new",
        json!({ "token": token, "roomEpoch": "test-room", "team": 1, "kind": "heavy" }),
    );
    assert_eq!(h.sim().tanks[ai].kind.as_str(), "scout");
    assert_eq!(h.sim().tanks[ai].team, Team::Blue);
    assert_eq!(h.sim().tanks[ai].driver, Driver::Human);
    assert_eq!(
        h.latest("alice-new", "lobby")["hostId"],
        h.latest("bob", "welcome")["playerId"]
    );
    h.join(
        "alice-newer",
        json!({ "token": token, "roomEpoch": "test-room" }),
    );
    assert!(h.closed.iter().any(|name| name == "alice-new"));
    assert!(
        h.latest("alice-newer", "control")["controlEpoch"].as_u64()
            > first["controlEpoch"].as_u64()
    );
    let tick = h.host.tick();
    h.action(
        "alice-new",
        "input",
        input(
            &first["controlEpoch"],
            99,
            tick,
            json!({ "moveX": 1, "fire": true, "actions": [] }),
        ),
    );
    assert_eq!(
        h.sim().tanks[ai].driver,
        Driver::Human,
        "revoked socket cannot suspend the replacement socket"
    );
}

#[test]
fn round_and_epoch_boundaries_suspension_seat_expiry_and_empty_room_cleanup_are_bounded() {
    let mut h = harness();
    h.join("alice", json!({}));
    h.action("alice", "start", json!({}));
    let control = h.latest("alice", "control");
    let tank = h.sim().human_index().unwrap();
    h.action("alice", "suspend", json!({}));
    assert_eq!(h.sim().tanks[tank].driver, Driver::Bot);
    h.action("alice", "resume", json!({}));
    assert_eq!(h.sim().tanks[tank].driver, Driver::Human);
    h.action(
        "alice",
        "input",
        net_support::merged(
            input(&control["controlEpoch"], 1, 0, json!({})),
            json!({ "roundId": 0, "moveX": 1, "fire": true, "actions": [{ "type": "mine" }] }),
        ),
    );
    h.advance();
    assert!(!h.sim().tanks[tank].command.fire);
    assert_eq!(h.sim().mines.len(), 0);
    h.disconnect("alice");
    h.now += 30_000;
    h.tick_only(0);
    assert!(h.host.disposed);
    assert!(h.host.simulation.is_none());
}

#[test]
fn a_heartbeat_in_flight_from_the_previous_round_cannot_disconnect_a_player() {
    let mut h = harness();
    h.join("alice", json!({}));
    h.action("alice", "start", json!({}));
    for _ in 0..10 {
        h.advance();
    }
    let old_tick = h.host.tick();
    h.action("alice", "end", json!({}));
    h.action("alice", "start", json!({}));
    let now = h.now;
    h.action(
        "alice",
        "ping",
        json!({ "roundId": 1, "observedTick": old_tick, "t": now }),
    );
    assert_eq!(h.host.connections(), 1);
    assert!(h.closed.is_empty());
    h.action("alice", "ping", json!({ "observedTick": 0, "t": now }));
    assert_eq!(h.latest("alice", "pong")["tick"], 0);
}

#[test]
fn suspended_clients_receive_no_snapshot_backlog_and_resume_with_a_current_baseline() {
    let mut h = harness();
    h.join("alice", json!({}));
    h.action("alice", "start", json!({}));
    h.action("alice", "suspend", json!({}));
    for _ in 0..80 {
        h.advance();
    }
    assert_eq!(h.count("alice", "snapshot"), 0);
    h.action("alice", "resume", json!({}));
    assert_eq!(h.latest("alice", "full")["tick"], h.host.tick());
    h.advance();
    assert_eq!(h.host.connections(), 1);
    assert!(h.count("alice", "snapshot") > 0);
}

#[test]
fn late_join_starts_a_fresh_chassis_and_score_and_departed_ordnance_cannot_credit_the_newcomer() {
    let mut h = harness();
    h.join("alice", json!({ "team": 0 }));
    h.join("bob", json!({ "team": 1 }));
    h.action("alice", "start", json!({}));
    let a = h.tank_of("alice");
    let (a_id, old_life) = (h.sim().tanks[a].id, h.sim().tanks[a].life);
    h.action("alice", "leave", json!({}));
    h.join("carol", json!({ "team": 0, "kind": "scout" }));
    let c = h.tank_of("carol");
    let carol = h.sim().tanks[c].clone();
    assert_eq!(carol.id, a_id);
    assert!(carol.life > old_life);
    assert_eq!(carol.kind.as_str(), "scout");
    assert_eq!(carol.kills, 0);
    let victim = h
        .sim()
        .tanks
        .iter()
        .position(|tank| tank.team == Team::Red)
        .unwrap();
    h.sim().tanks[victim].protection = 0.0;
    h.sim()
        .damage_tank(victim, 10000.0, carol.id, carol.team, Some(old_life), None);
    h.advance();
    let c = h.tank_of("carol");
    assert_eq!(h.sim().tanks[c].kills, 0);
    assert_eq!(h.sim().tanks[c].xp, 0.0);
    h.action("bob", "end", json!({}));
    let score = h.latest("bob", "lobby")["scoreboard"].clone();
    let kills = |name: &str| {
        score
            .as_array()
            .unwrap()
            .iter()
            .find(|player| player["name"] == name)
            .map(|player| player["kills"].clone())
    };
    assert_eq!(kills("alice"), Some(json!(1)));
    assert_eq!(kills("carol"), Some(json!(0)));
    h.action("bob", "start", json!({}));
    assert_eq!(h.host.round_id, 2);
    assert_eq!(h.sim().tanks.len(), 12);
}

#[test]
fn a_full_room_full_team_invalid_kind_and_incompatible_build_are_rejected_before_authority() {
    let mut h = harness();
    h.join("bad-version", json!({ "version": 999 }));
    assert_eq!(h.latest("bad-version", "error")["code"], "incompatible");
    h.join("bad-kind", json!({ "kind": "humvee" }));
    assert!(h.closed.iter().any(|name| name == "bad-kind"));
    assert_eq!(
        h.latest("bad-kind", "error")["message"],
        "kind: Invalid choice"
    );
    for i in 0..6 {
        h.join(&format!("p{i}"), json!({ "team": 0 }));
    }
    h.join("full-team", json!({ "team": 0 }));
    assert_eq!(h.latest("full-team", "error")["code"], "team-full");
    h.join("p6", json!({ "team": 1 }));
    h.join("p7", json!({ "team": 1 }));
    h.join("ninth", json!({}));
    assert_eq!(h.latest("ninth", "error")["code"], "room-full");
    h.action("p0", "start", json!({}));
    assert_eq!(h.sim().tanks.iter().filter(|tank| tank.human).count(), 8);
}

#[test]
fn real_host_messages_apply_to_mirrors_and_projectile_paths_survive_an_impact_between_snapshots() {
    let mut h = harness();
    h.join("alice", json!({}));
    h.action("alice", "start", json!({}));
    let (mut mirror, _) = mirror_from(&h, "alice");
    let control = h.latest("alice", "control");
    let tank = h.sim().human_index().unwrap();
    h.sim().tanks[tank].protection = 0.0;
    set_translation(h.sim(), tank, 0.0, 0.65, 0.0);
    let keep = h.all_tanks();
    clear_arena(h.sim(), &keep);
    let tank = h.sim().human_index().unwrap();
    h.sim().add_cover(&CoverDef::new(
        CoverKind::Concrete,
        0.0,
        4.0,
        10.0,
        1.0,
        4.0,
        100.0,
        0x999999,
    ));
    h.action(
        "alice",
        "input",
        input(
            &control["controlEpoch"],
            1,
            0,
            json!({ "fire": true, "actions": [] }),
        ),
    );
    h.advance();
    let snaps = h.latest("alice", "snapshot")["snapshots"].clone();
    apply_latest(&h, "alice", &mut mirror);
    assert_eq!(mirror.state.as_ref().unwrap().to_value(), capture(&h));
    let team = h.sim().tanks[tank].team.index();
    let sim = h.host.simulation.as_ref().unwrap();
    let entries: Vec<Value> = snaps
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|snap| snap["paths"].as_array().cloned().unwrap_or_default())
        .collect();
    let launch = entries
        .iter()
        .find(|entry| entry["team"] == team && !sim.shots.iter().any(|s| entry["id"] == s.id))
        .expect("the shell that hit the wall was launched in these frames");
    assert!(
        entries
            .iter()
            .any(|entry| entry["id"] == launch["id"] && entry["end"].is_number()),
        "and ended in them"
    );
    assert!(mirror.shots.paths.is_empty());
    assert!(
        snaps
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|snap| snap["events"].as_array().cloned().unwrap_or_default())
            .any(|event| event["event"]["type"] == "impact")
    );
}

#[test]
fn membership_and_lifecycle_changes_between_broadcasts_keep_a_frame_at_their_own_tick() {
    let mut h = harness();
    h.join("alice", json!({}));
    h.action("alice", "start", json!({}));
    let (mut mirror, _) = mirror_from(&h, "alice");
    let first = h.host.tick() + 1;
    let pickup = h.sim().pickups[0].id;
    let cover = h
        .sim()
        .covers
        .iter()
        .position(|cover| cover.alive && cover.destructible)
        .unwrap();
    let cover_id = h.sim().covers[cover].id;
    let piece = Arc::new(AtomicU32::new(0));
    let recorded = piece.clone();
    // A pickup vanishes and returns inside one 50 ms batch; debris lands and is cleared.
    h.host.tick_hook = Some(Box::new(move |sim, tick| {
        let supply = sim.pickups.iter().position(|p| p.id == pickup).unwrap();
        if tick == first {
            sim.pickups[supply].available = false;
            sim.damage_cover(cover, 10000.0, 0, Team::Blue, None, None);
            sim.fragment(
                0.0,
                0.0,
                0xffffff,
                0.5,
                sloppy_core::sim::types::FragmentShape::Shard,
                1.0,
            );
            recorded.store(sim.fragments.last().unwrap().id, Ordering::SeqCst);
        } else if tick == first + 1 {
            sim.pickups[supply].available = true;
            sim.fragments.last_mut().unwrap().life = 0.0;
        }
    }));
    h.advance();
    let frames = h.latest("alice", "snapshot")["snapshots"].clone();
    let ticks: Vec<u64> = frames
        .as_array()
        .unwrap()
        .iter()
        .map(|frame| frame["tick"].as_u64().unwrap())
        .collect();
    assert_eq!(ticks, vec![first, first + 1, first + 2]);
    let piece = piece.load(Ordering::SeqCst);
    let mut states = Vec::new();
    let latest = h.latest_binary("alice", SNAPSHOT_MESSAGE);
    let BinaryMessage::Snapshot(mut batch) = read_binary_message(&latest).unwrap() else {
        unreachable!()
    };
    for _ in 0..batch.count {
        assert!(mirror.apply_snapshot(&mut batch).is_some());
        states.push(mirror.state.clone().unwrap());
    }
    let available: Vec<bool> = states
        .iter()
        .map(|scene| scene.pickups.get(pickup).unwrap().value.available)
        .collect();
    assert_eq!(available, vec![false, true, true]);
    assert!(!states[0].covers.get(cover_id).unwrap().value.alive);
    assert!(states[0].fragments.contains(piece));
    assert!(!states[2].fragments.contains(piece));
    assert_eq!(mirror.state.as_ref().unwrap().to_value(), capture(&h));
}

#[test]
fn slow_readers_disconnect_and_overload_terminates_the_room_instead_of_skipping_physics() {
    let mut slow = harness();
    slow.join("alice", json!({}));
    slow.action("alice", "start", json!({}));
    for _ in 0..70 {
        slow.tick_only(50);
    }
    assert_eq!(slow.host.connections(), 0);
    assert!(slow.closed.iter().any(|name| name == "alice"));

    let mut overloaded = harness();
    overloaded.join("alice", json!({}));
    overloaded.action("alice", "start", json!({}));
    overloaded.tick_only(1000);
    assert!(overloaded.host.disposed);
    assert_eq!(
        overloaded.latest("alice", "room-reset")["reason"],
        "overload"
    );
}

#[test]
fn resync_skips_events_already_included_in_its_baseline_and_repeated_rounds_retain_12_slots() {
    let mut h = harness();
    h.join("alice", json!({}));
    for round in 1..=12 {
        h.action("alice", "start", json!({}));
        assert_eq!(h.host.round_id, round);
        assert_eq!(h.sim().tanks.len(), 12);
        assert_eq!(h.latest("alice", "control")["controlEpoch"], 1);
        h.advance();
        h.action("alice", "end", json!({}));
    }
    let mut state = Scene::capture(h.host.simulation.as_ref().unwrap());
    let mut stream = StateStream::new("r", 1);
    let mut mirror = StateMirror::default();
    let full = stream.full(0, 2, [], || state.clone());
    let BinaryMessage::Full(baseline) = read_binary_message(&full).unwrap() else {
        unreachable!()
    };
    mirror.apply_full(&baseline, "r", 1).unwrap();
    let events: Vec<TimedEvent> = (1..=3)
        .map(|id| TimedEvent {
            event_id: id,
            tick: id as f64,
            event: SimEvent::at(SimEventType::Impact, 0.0, 0.0),
        })
        .collect();
    let frame = stream.snapshot(&mut state, 3, &events, &[]);
    let result = apply_batch(&mut mirror, &batch(1, 0, 1, &[(3, frame)]));
    assert_eq!(
        result[0]
            .events
            .iter()
            .map(|e| e.event_id)
            .collect::<Vec<_>>(),
        vec![3]
    );
}

#[test]
fn auto_team_balances_human_seats_honors_explicit_teams_and_excludes_the_player_changing_teams() {
    let mut h = harness();
    h.join("alice", json!({ "team": 1 }));
    h.join("bob", json!({}));
    h.join("carol", json!({}));
    let teams = |h: &Harness| -> Vec<u64> {
        h.latest("alice", "lobby")["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|player| player["team"].as_u64().unwrap())
            .collect()
    };
    assert_eq!(teams(&h), vec![1, 0, 0]);
    h.action("carol", "choose", json!({ "kind": "balanced", "team": 1 }));
    h.join("dave", json!({}));
    assert_eq!(teams(&h), vec![1, 0, 1, 0]);
    h.action("alice", "choose", json!({ "kind": "balanced" }));
    assert_eq!(teams(&h)[0], 1);
}

#[test]
fn create_starts_a_selected_humans_only_map_immediately_and_subsequent_players_join_the_running_battle()
 {
    let mut h = harness();
    let create = json!({ "mapMode": "quarry", "difficulty": "normal", "humansOnly": true });
    h.join("alice", json!({ "create": create }));
    assert_eq!(h.host.phase, RoomPhase::Playing);
    assert_eq!(h.host.settings.map_mode.as_str(), "quarry");
    assert_eq!(h.sim().tanks.len(), 1);
    assert_eq!(h.latest("alice", "full")["roundId"], 1);
    h.join("collision", json!({ "create": create }));
    assert_eq!(h.latest("collision", "error")["code"], "room-exists");
    h.join("bob", json!({ "existingRoom": true }));
    assert_eq!(h.sim().tanks.len(), 2);
    assert_eq!(h.latest("bob", "full")["roundId"], 1);
    let teams: Vec<Team> = h.sim().tanks.iter().map(|tank| tank.team).collect();
    assert_eq!(teams, vec![Team::Blue, Team::Red]);
    assert_eq!(
        serde_json::to_value(h.host.directory_entry("ABCDEFGH")).unwrap(),
        json!({
            "room": "ABCDEFGH",
            "contentVersion": CONTENT_VERSION,
            "mapMode": "quarry",
            "difficulty": "normal",
            "humansOnly": true,
            "roundMinutes": 20,
            "players": 2,
            "reserved": 2,
            "phase": "playing",
            "roundId": 1,
            "time": 1200,
            "scores": [0, 0],
        })
    );
    h.action("alice", "leave", json!({}));
    assert!(!h.host.disposed);
    h.action("bob", "leave", json!({}));
    assert!(h.host.disposed);
    assert!(h.host.simulation.is_none());
    assert_eq!(h.host.directory_entry("ABCDEFGH").players, 0);
}

#[test]
fn a_token_from_an_expired_room_epoch_joins_as_a_new_player_with_a_reset_notice() {
    let mut h = harness();
    h.join("alice", json!({}));
    assert_eq!(h.latest("alice", "welcome")["reset"], false);
    h.join(
        "returning",
        json!({ "token": "credential-from-an-old-room", "roomEpoch": "expired-room" }),
    );
    let welcome = h.latest("returning", "welcome");
    assert_eq!(
        welcome["reset"], true,
        "the client must drop its old session state"
    );
    assert_eq!(welcome["roomEpoch"], "test-room");
    assert_ne!(welcome["token"], "credential-from-an-old-room");
    assert_eq!(
        h.latest("returning", "lobby")["players"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    // A stale token for this same epoch names a seat that expired, which is refused instead.
    h.join(
        "expired",
        json!({ "token": "credential-never-issued", "roomEpoch": "test-room" }),
    );
    assert_eq!(h.latest("expired", "error")["code"], "seat-expired");
}

#[test]
fn a_stale_directory_selection_cannot_recreate_an_empty_room_and_a_dropped_connection_retains_its_grace()
 {
    let mut h = harness();
    h.join("stale", json!({ "existingRoom": true }));
    assert_eq!(h.latest("stale", "error")["code"], "room-gone");
    assert_eq!(h.host.connections(), 0);
    h.join(
        "alice",
        json!({ "create": { "mapMode": "harbor", "difficulty": "easy", "humansOnly": true } }),
    );
    let token = h.latest("alice", "welcome")["token"].clone();
    h.disconnect("alice");
    assert!(!h.host.disposed);
    assert_eq!(h.host.directory_entry("ABCDEFGH").players, 0);
    assert_eq!(h.host.directory_entry("ABCDEFGH").reserved, 1);
    h.join(
        "back",
        json!({ "token": token, "roomEpoch": "test-room", "existingRoom": true }),
    );
    assert_eq!(h.host.connections(), 1);
    assert_eq!(h.host.round_id, 1);
}

#[test]
fn round_length_defaults_to_twenty_minutes_and_is_controlled_by_the_host_between_rounds() {
    let settings = json!({ "mapMode": "village", "difficulty": "normal", "humansOnly": true });
    let with_minutes =
        |minutes: u32| net_support::merged(settings.clone(), json!({ "roundMinutes": minutes }));
    let mut h = harness();
    h.join("alice", json!({ "create": settings }));
    h.join("bob", json!({}));
    assert_eq!(h.sim().match_state.time, 1200.0);
    assert_eq!(h.latest("bob", "lobby")["settings"]["roundMinutes"], 20);
    h.action("alice", "end", json!({}));
    h.action("bob", "settings", with_minutes(1));
    assert_eq!(
        h.host.settings.round_minutes, 20,
        "Guest cannot change the next round"
    );
    let token = h.latest("bob", "welcome")["token"].clone();
    h.join("bob", json!({ "token": token, "roomEpoch": "test-room" }));
    h.action("alice", "settings", with_minutes(1));
    h.action("alice", "start", json!({}));
    assert_eq!(h.sim().match_state.time, 60.0);
    assert_eq!(h.latest("bob", "lobby")["settings"]["roundMinutes"], 1);
    h.sim().match_state.scores = [1, 0];
    // Skip to the last frame of the minute instead of simulating all of it.
    h.sim().match_state.time = 0.01;
    h.advance();
    assert_eq!(h.host.phase, RoomPhase::Results);
    assert_eq!(h.sim().match_state.winner, Some(Team::Blue));
    assert_eq!(h.sim().match_state.time, 0.0);
    h.action("alice", "start", json!({}));
    h.action("alice", "settings", with_minutes(20));
    assert_eq!(
        h.host.settings.round_minutes, 1,
        "Cannot change a running match"
    );
}

#[test]
fn a_room_past_its_lifetime_lets_the_battle_under_way_finish_then_closes() {
    let create = json!({
        "mapMode": "village",
        "difficulty": "normal",
        "humansOnly": true,
        "roundMinutes": MAX_ROUND_MINUTES,
    });
    // Created exactly one lifetime ago: the battle it starts now keeps running.
    let mut h = harness_at(-(MAX_ROOM_MS as i64), "test-room", 4242);
    h.join("alice", json!({ "create": create }));
    assert_eq!(h.host.phase, RoomPhase::Playing);
    assert_eq!(
        h.host.directory_entry("ABCDEFGH").time,
        MAX_ROUND_MINUTES * 60
    );
    h.advance();
    assert!(!h.host.disposed, "A battle under way is not cut short");
    h.sim().match_state.scores = [1, 0];
    h.sim().match_state.time = 0.01;
    h.advance();
    assert_eq!(h.host.phase, RoomPhase::Results);
    h.advance();
    assert!(
        h.host.disposed,
        "No new battle once the lifetime has passed"
    );
    assert_eq!(h.latest("alice", "room-reset")["reason"], "expired");
    // Even a battle stuck in overtime ends with the room at the hard limit.
    let mut stuck = harness_at(
        -((MAX_ROOM_MS + MAX_BATTLE_OVERRUN_MS) as i64),
        "test-room",
        4242,
    );
    stuck.join("alice", json!({ "create": create }));
    stuck.advance();
    assert!(stuck.host.disposed);
}

#[test]
fn live_snapshots_carry_each_players_authoritative_kills_and_preserve_them_through_respawn() {
    let mut h = harness();
    h.join(
        "alice",
        json!({
            "team": 0,
            "create": { "mapMode": "village", "difficulty": "normal", "humansOnly": true },
        }),
    );
    h.join("bob", json!({ "team": 1 }));
    let alice = h.sim().tanks[0].clone();
    let bob = h.sim().tanks[1].id;
    let mut mirror = h.mirror_from_latest_full("alice");
    h.sim().tanks[1].protection = 0.0;
    h.sim()
        .damage_tank(1, 10000.0, alice.id, alice.team, Some(alice.life), None);
    h.advance();
    apply_latest(&h, "alice", &mut mirror);
    assert_eq!(mirror.render(alice.id).unwrap().viewer().unwrap().kills, 1);
    assert_eq!(mirror.render(bob).unwrap().viewer().unwrap().deaths, 1);
    h.sim().respawn(0, None);
    h.advance();
    apply_latest(&h, "alice", &mut mirror);
    assert_eq!(mirror.render(alice.id).unwrap().viewer().unwrap().kills, 1);
    h.join("carol", json!({}));
    let players = h.latest("carol", "lobby")["players"].clone();
    let alice_row = players
        .as_array()
        .unwrap()
        .iter()
        .find(|player| player["name"] == "alice")
        .unwrap()
        .clone();
    assert_eq!(alice_row["kills"], 1);
}

#[test]
fn wire_messages_keep_the_typescript_key_order() {
    let mut h = harness();
    h.join("alice", json!({}));
    h.action("alice", "start", json!({}));
    h.advance();
    let texts: Vec<String> = h.wire["alice"]
        .iter()
        .filter_map(|message| match message {
            Message::Text(text) => Some(text.clone()),
            Message::Binary(_) => None,
        })
        .collect();
    let starts = |prefix: &str| texts.iter().any(|text| text.starts_with(prefix));
    assert!(starts(&format!(
        r#"{{"type":"welcome","version":{PROTOCOL_VERSION},"contentVersion":"#
    )));
    assert!(starts(
        r#"{"roomEpoch":"test-room","roundId":0,"type":"lobby","phase":"lobby""#
    ));
    assert!(starts(
        r#"{"roomEpoch":"test-room","roundId":1,"type":"control","tankId":"#
    ));
    // State is binary: a type byte, then the round and tick.
    let binary: Vec<&Vec<u8>> = h.wire["alice"]
        .iter()
        .filter_map(|message| match message {
            Message::Binary(bytes) => Some(bytes),
            Message::Text(_) => None,
        })
        .collect();
    assert!(
        binary
            .iter()
            .any(|bytes| bytes[..3] == [FULL_MESSAGE, 1, 0])
    );
    assert!(
        binary
            .iter()
            .any(|bytes| bytes[..2] == [SNAPSHOT_MESSAGE, 1])
    );
    assert!(starts(r#"{"type":"pong","t":50,"tick":0}"#));
    assert!(
        !texts
            .iter()
            .any(|text| text.contains(".0,") || text.contains(".0}"))
    );
    let _ = MatchPhase::Playing;
}

#[test]
fn scoreboard_updates_after_idle_ticks_disconnect_and_reconnect() {
    let mut h = harness();
    h.join(
        "alice",
        json!({ "team": 0, "create": { "mapMode": "village", "difficulty": "normal", "humansOnly": true } }),
    );
    h.join("bob", json!({ "team": 1 }));
    let welcome = h.latest("alice", "welcome");
    for _ in 0..10 {
        h.advance();
    }
    let alice_index = h.tank_of("alice");
    let alice = h.sim().tanks[alice_index].clone();
    let bob = h.tank_of("bob");
    h.sim().tanks[bob].protection = 0.0;
    h.sim()
        .damage_tank(bob, 10000.0, alice.id, alice.team, Some(alice.life), None);
    h.advance();
    h.disconnect("alice");
    h.advance();
    h.join(
        "alice-return",
        json!({ "token": welcome["token"], "roomEpoch": welcome["roomEpoch"] }),
    );
    h.advance();
    h.action("bob", "end", json!({}));
    let lobby = h.latest("bob", "lobby");
    let scoreboard = lobby["scoreboard"].as_array().unwrap();
    let alice = scoreboard
        .iter()
        .find(|player| player["name"] == "alice")
        .unwrap();
    let bob = scoreboard
        .iter()
        .find(|player| player["name"] == "bob")
        .unwrap();
    assert_eq!(alice["kills"], 1);
    assert_eq!(alice["connected"], true);
    assert_eq!(bob["deaths"], 1);
}

#[test]
fn control_messages_follow_life_driver_and_round_changes_without_idle_repeats() {
    let mut h = harness();
    h.join(
        "alice",
        json!({ "create": { "humansOnly": true, "mapMode": "village", "difficulty": "normal" } }),
    );
    let controls = h.count("alice", "control");
    for _ in 0..10 {
        h.advance();
    }
    assert_eq!(h.count("alice", "control"), controls);
    let life = h.latest("alice", "control")["life"].as_u64().unwrap();
    let tank = h.tank_of("alice");
    h.sim().tanks[tank].protection = 0.0;
    let tank_id = h.sim().tanks[tank].id;
    h.sim()
        .damage_tank(tank, 10000.0, tank_id, Team::Red, None, None);
    h.sim().respawn(tank, None);
    h.advance();
    assert_eq!(h.count("alice", "control"), controls + 1);
    assert_eq!(h.latest("alice", "control")["life"], life + 1);
    h.action("alice", "suspend", json!({}));
    assert_eq!(h.latest("alice", "control")["driver"], "idle");
    h.action("alice", "resume", json!({}));
    assert_eq!(h.latest("alice", "control")["driver"], "human");
    h.action("alice", "end", json!({}));
    h.action("alice", "start", json!({}));
    assert_eq!(h.latest("alice", "control")["roundId"], 2);
    let controls = h.count("alice", "control");
    h.advance();
    assert_eq!(h.count("alice", "control"), controls);
}

#[test]
fn a_mid_round_join_gets_the_streamed_scene_and_every_client_keeps_matching_the_host() {
    let mut h = harness();
    let create = json!({ "mapMode": "village", "difficulty": "normal", "humansOnly": false });
    h.join("alice", json!({ "create": create }));
    h.advance();
    h.advance();
    let seq_before = last_seq(&h.latest_binary("alice", SNAPSHOT_MESSAGE));
    // Taking over a bot's tank changes the simulation between frames, so the host streams
    // that change as a frame before the baseline, which must show exactly the streamed scene.
    h.join("bob", json!({ "existingRoom": true }));
    let full = h.latest("bob", "full");
    let baseline_seq = full["seq"].as_u64().unwrap();
    assert_eq!(baseline_seq, seq_before + 1, "the join became a frame");
    let bob = h.latest("bob", "control")["tankId"].as_u64().unwrap();
    let tank = full["state"]["entities"]["tanks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tank| tank["id"] == bob)
        .unwrap()
        .clone();
    assert_eq!(tank["name"], "bob");
    assert_eq!(tank["human"], true);
    h.advance();
    let alice_batch = h.latest_binary("alice", SNAPSHOT_MESSAGE);
    let bob_batch = h.latest_binary("bob", SNAPSHOT_MESSAGE);
    assert_eq!(
        first_seq(&alice_batch),
        baseline_seq,
        "alice receives the join frame"
    );
    assert_eq!(
        first_seq(&bob_batch),
        baseline_seq + 1,
        "bob's baseline already shows it"
    );
    let truth = capture(&h);
    assert_eq!(h.mirrored("alice"), truth);
    assert_eq!(h.mirrored("bob"), truth);
    for _ in 0..20 {
        h.advance();
        let truth = capture(&h);
        assert_eq!(h.mirrored("alice"), truth);
        assert_eq!(h.mirrored("bob"), truth);
    }
}

#[test]
fn seats_joining_between_intervals_reach_clients_in_batches_they_accept() {
    let mut h = harness();
    let create = json!({ "mapMode": "village", "difficulty": "normal", "humansOnly": false });
    h.join("alice", json!({ "create": create }));
    h.advance();
    let before = h.binary("alice", SNAPSHOT_MESSAGE).len();
    // Each join takes over a bot's tank between intervals, which the host streams as a
    // frame of its own before that seat's baseline.
    for index in 0..7 {
        h.join(&format!("late-{index}"), json!({ "existingRoom": true }));
    }
    h.advance();
    let counts: Vec<u64> = h.binary("alice", SNAPSHOT_MESSAGE)[before..]
        .iter()
        .map(|bytes| match read_binary_message(bytes).unwrap() {
            BinaryMessage::Snapshot(batch) => batch.count,
            BinaryMessage::Full(_) => unreachable!(),
        })
        .collect();
    assert!(
        counts.iter().sum::<u64>() > MAX_BATCH_FRAMES as u64,
        "the joins produced more frames than one batch holds: {counts:?}"
    );
    assert!(
        counts.iter().all(|count| *count <= MAX_BATCH_FRAMES as u64),
        "every batch fits the client's limit: {counts:?}"
    );
    assert_eq!(h.mirrored("alice"), capture(&h));
    assert_eq!(h.mirrored("late-6"), capture(&h));
}

/// Input between two timer callbacks drives from the tick after its arrival, not from the
/// start of the next batch; a requested later tick is honoured, an earlier one is clamped
/// to arrival; each snapshot reports the acknowledged input's ticks and the viewer's hull.
#[test]
fn input_drives_from_its_arrival_or_requested_tick_never_retroactively() {
    let mut h = harness();
    h.join("alice", json!({}));
    h.action("alice", "start", json!({}));
    h.tick_only(50);
    assert_eq!(h.host.tick(), 3, "one batch is three ticks");
    let alice = h.tank_of("alice");
    let driven = Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = driven.clone();
    h.host.tick_hook = Some(Box::new(move |sim, tick| {
        log.lock()
            .unwrap()
            .push((tick, sim.tanks[alice].command.move_z));
    }));
    let epoch = h.latest("alice", "control")["controlEpoch"].clone();
    let send = |h: &mut Harness, seq: u64, extra: Value| {
        let tick = h.host.tick();
        h.action("alice", "input", input(&epoch, seq, tick, extra));
    };

    // 20 ms into the next batch: ticks 4 (due at 66.7 ms) ran before it in time.
    h.now += 20;
    send(&mut h, 1, json!({ "moveZ": 1 }));
    h.tick_only(30);
    let snapshot = h.latest("alice", "snapshot");
    assert_eq!(
        (
            &snapshot["ack"],
            &snapshot["ackTick"],
            &snapshot["ackArrival"]
        ),
        (&json!(1), &json!(5), &json!(5))
    );
    assert_eq!(
        *driven.lock().unwrap(),
        [(4, 0.0), (5, 1.0), (6, 1.0)],
        "the batch's earlier tick keeps the old input"
    );
    let hull = &snapshot["hull"];
    assert_eq!(hull["tick"], 6);
    assert!(
        hull["v"][2].as_f64().unwrap() > 0.0,
        "the hull drives forward"
    );

    // Asked for a later tick, the input waits for it; asked for the past, it starts on arrival.
    driven.lock().unwrap().clear();
    h.now += 10;
    send(&mut h, 2, json!({ "moveZ": -1, "tick": 9 }));
    h.tick_only(40);
    let snapshot = h.latest("alice", "snapshot");
    assert_eq!(
        (
            &snapshot["ack"],
            &snapshot["ackTick"],
            &snapshot["ackArrival"]
        ),
        (&json!(2), &json!(9), &json!(7))
    );
    assert_eq!(*driven.lock().unwrap(), [(7, 1.0), (8, 1.0), (9, -1.0)]);
    h.now += 5;
    send(&mut h, 3, json!({ "moveZ": 0.5, "tick": 2 }));
    h.tick_only(45);
    let snapshot = h.latest("alice", "snapshot");
    assert_eq!(
        (
            &snapshot["ack"],
            &snapshot["ackTick"],
            &snapshot["ackArrival"]
        ),
        (&json!(3), &json!(10), &json!(10))
    );
}
