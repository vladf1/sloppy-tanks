//! Seat input on the host (`tests/player-controls.test.ts`) and room rosters on the
//! simulation (`tests/multiplayer-simulation.test.ts`).

use crate::net_support;
use crate::support;

use std::collections::BTreeMap;

use net_support::set_translation;
use serde_json::{Value, json};
use sloppy_core::net::multiplayer_simulation::{
    MultiplayerOptions, create_multiplayer_simulation, release_player_tank, set_driver,
};
use sloppy_core::net::player_controls::{
    Ack, Action, Aim, ControlInput, PlayerControls, encode_input, read_input,
};
use sloppy_core::net::quick_message::QuickMessage;
use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::combat_record::CombatRecord;
use sloppy_core::sim::debris_cleanup::cleanup_candidate;
use sloppy_core::sim::difficulty::{Difficulty, enemy_difficulty};
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::mines::place_mine;
use sloppy_core::sim::shooter_rewind::ShooterRewind;
use sloppy_core::sim::types::{
    AmmoSelection, CoverKind, Driver, FragmentShape, PlayerAssignment, SimEventType, Team,
    VehicleCommand, VehicleKind, Weapon,
};
use sloppy_core::sim::weapons::fire_weapon;
use sloppy_core::sim::{GameMode, Simulation, SimulationSetup};
use support::{clear_arena, tank_index};

fn alice_room() -> (Simulation, u32) {
    let sim = create_multiplayer_simulation(
        4242.0,
        &[PlayerAssignment {
            player_id: "alice".into(),
            name: "Alice".into(),
            team: Team::Blue,
            slot: 0,
            kind: VehicleKind::Balanced,
        }],
        MultiplayerOptions::default(),
    )
    .unwrap();
    let tank = sim.tanks[sim.human_index().unwrap()].id;
    (sim, tank)
}

fn input(epoch: u64, seq: i64, extra: Value) -> Value {
    net_support::merged(
        json!({
            "controlEpoch": epoch,
            "seq": seq,
            "observedTick": 0,
            "moveX": 1,
            "moveZ": 0,
            "aim": { "x": 30, "z": 40 },
            "fire": true,
            "actions": [],
        }),
        extra,
    )
}

fn ack(input_seq: i64, applied_tick: u64, arrival_tick: u64) -> Ack {
    Ack {
        input_seq,
        applied_tick,
        arrival_tick,
    }
}

#[test]
fn input_holds_until_its_lease_expires_then_idles_and_eventually_hands_control_to_a_bot() {
    let (mut sim, tank) = alice_room();
    let mut controls = PlayerControls::new(&sim, tank, 0.0, true).unwrap();
    let epoch = controls.control_epoch;
    assert!(controls.accept(&sim, &input(epoch, 1, json!({})), 0, 0.0));
    assert_eq!(controls.command(&mut sim, 1, 0.0).unwrap().move_x, 1.0);
    assert!(controls.command(&mut sim, 14, 249.0).unwrap().fire);
    assert!(!controls.command(&mut sim, 15, 250.0).unwrap().fire);
    assert_eq!(controls.command(&mut sim, 15, 250.0).unwrap().move_x, 0.0);
    assert!(!controls.command(&mut sim, 299, 4999.0).unwrap().fire);
    let epoch = controls.control_epoch;
    assert!(controls.command(&mut sim, 300, 5000.0).is_none());
    let i = tank_index(&sim, tank);
    assert_eq!(sim.tanks[i].driver, Driver::Bot);
    assert!(sim.tanks[i].human);
    assert!(controls.control_epoch > epoch);
    assert!(
        !controls.accept(
            &sim,
            &input(controls.control_epoch, 2, json!({})),
            300,
            5010.0
        ),
        "late input cannot silently reclaim a bot-driven seat"
    );
    controls.resume(&mut sim, 5010.0);
    assert_eq!(sim.tanks[i].driver, Driver::Human);
    assert!(!controls.command(&mut sim, 301, 5010.0).unwrap().fire);
    let epoch = controls.control_epoch;
    assert!(controls.accept(
        &sim,
        &input(epoch, 1, json!({ "observedTick": 301 })),
        301,
        5011.0
    ));
    assert!(controls.command(&mut sim, 302, 5012.0).unwrap().fire);
}

#[test]
fn held_input_that_outlives_its_lease_counts_one_lapse_until_fresh_input_arrives() {
    let (mut sim, tank) = alice_room();
    let mut controls = PlayerControls::new(&sim, tank, 0.0, true).unwrap();
    let epoch = controls.control_epoch;
    assert!(controls.accept(&sim, &input(epoch, 1, json!({})), 0, 0.0));
    controls.command(&mut sim, 1, 249.0);
    assert_eq!(controls.take_lapses(), 0, "inside the lease");
    controls.command(&mut sim, 15, 250.0);
    controls.command(&mut sim, 16, 300.0);
    assert_eq!(
        controls.take_lapses(),
        1,
        "one lapse per stalled hold, not per tick"
    );
    controls.command(&mut sim, 17, 400.0);
    assert_eq!(controls.take_lapses(), 0);
    assert!(controls.accept(&sim, &input(epoch, 2, json!({})), 0, 500.0));
    controls.command(&mut sim, 30, 800.0);
    assert_eq!(controls.take_lapses(), 1, "fresh input re-arms the count");

    let neutral = json!({ "moveX": 0, "fire": false });
    assert!(controls.accept(&sim, &input(epoch, 3, neutral), 0, 1000.0));
    controls.command(&mut sim, 60, 1500.0);
    assert_eq!(
        controls.take_lapses(),
        0,
        "an idle refresh only holds aim, so its lease may run out"
    );

    assert!(controls.accept(&sim, &input(epoch, 4, json!({})), 0, 2000.0));
    controls.suspend(&mut sim);
    controls.command(&mut sim, 90, 2500.0);
    assert_eq!(controls.take_lapses(), 0, "a suspended seat does not lapse");
}

#[test]
fn ordered_actions_survive_coalesced_inputs_run_once_and_stale_clicks_expire_independently() {
    let (mut sim, tank) = alice_room();
    let mut controls = PlayerControls::new(&sim, tank, 0.0, true).unwrap();
    let e = controls.control_epoch;
    let actions =
        json!({ "actions": [{ "type": "mine" }, { "type": "ammo", "weapon": "rocket" }] });
    assert!(controls.accept(&sim, &input(e, 1, actions), 0, 0.0));
    assert!(controls.accept(
        &sim,
        &input(
            e,
            2,
            json!({ "moveX": -1, "actions": [{ "type": "mine" }] })
        ),
        0,
        10.0
    ));
    assert!(controls.command(&mut sim, 1, 20.0).unwrap().mine);
    assert_eq!(controls.ack, ack(2, 1, 1));
    let second = controls.command(&mut sim, 2, 30.0).unwrap();
    assert_eq!(second.move_x, -1.0);
    assert_eq!(
        second.ammo_selection,
        Some(AmmoSelection::Weapon(Weapon::Rocket))
    );
    assert!(!second.mine);
    assert!(controls.command(&mut sim, 3, 40.0).unwrap().mine);
    assert!(!controls.command(&mut sim, 4, 50.0).unwrap().mine);
    assert_eq!(controls.ack, ack(2, 1, 1));
    controls.accept(
        &sim,
        &input(e, 3, json!({ "actions": [{ "type": "mine" }] })),
        0,
        100.0,
    );
    controls.accept(&sim, &input(e, 4, json!({})), 0, 300.0);
    assert!(
        !controls.command(&mut sim, 20, 350.0).unwrap().mine,
        "new input cannot renew an old action"
    );
}

#[test]
fn validation_rejects_malformed_out_of_range_stale_duplicate_and_over_capacity_messages_atomically()
{
    let (mut sim, tank) = alice_room();
    let mut controls = PlayerControls::new(&sim, tank, 0.0, true).unwrap();
    let e = controls.control_epoch;
    let malformed = [
        Value::Null,
        json!([]),
        json!({}),
        input(e, 1, json!({ "moveX": null })),
        input(e, 1, json!({ "moveZ": 2 })),
        input(e, 1, json!({ "aim": { "angle": null } })),
        input(e, 1, json!({ "aim": { "x": 1025, "z": 0 } })),
        input(e, 1, json!({ "seq": 1.5 })),
        input(e, 1, json!({ "controlEpoch": 0 })),
        input(e, 1, json!({ "observedTick": 31 })),
        input(e, 1, json!({ "observedTick": -1 })),
        input(
            e,
            1,
            json!({ "actions": [{ "type": "ammo", "weapon": "tow" }] }),
        ),
        input(
            e,
            1,
            json!({ "actions": [{ "type": "ammo", "weapon": "rocket", "extra": true }] }),
        ),
        input(e, 1, json!({ "aim": { "x": 1, "z": 2, "angle": 0 } })),
    ];
    for data in &malformed {
        assert!(!controls.accept(&sim, data, 30, 0.0), "accepted {data}");
    }
    assert!(
        !controls.accept(&sim, &input(e, 1, json!({})), 31, 0.0),
        "observed tick older than 500ms"
    );
    let eight: Vec<Value> = (0..8).map(|_| json!({ "type": "mine" })).collect();
    assert!(controls.accept(&sim, &input(e, 1, json!({ "actions": eight })), 0, 0.0));
    assert!(!controls.accept(&sim, &input(e, 1, json!({})), 0, 1.0));
    assert!(!controls.accept(
        &sim,
        &input(e, 2, json!({ "actions": [{ "type": "mine" }] })),
        0,
        1.0
    ));
    assert!(controls.command(&mut sim, 1, 1.0).unwrap().mine);
    assert!(
        controls.accept(
            &sim,
            &input(e, 2, json!({ "actions": [{ "type": "mine" }] })),
            0,
            2.0
        ),
        "rejected input did not consume its sequence"
    );
    assert_eq!(controls.ack, ack(1, 1, 1));
}

#[test]
fn point_aim_is_recomputed_from_authority() {
    let (mut sim, tank) = alice_room();
    let mut controls = PlayerControls::new(&sim, tank, 0.0, true).unwrap();
    let e = controls.control_epoch;
    let i = tank_index(&sim, tank);
    controls.accept(
        &sim,
        &input(e, 1, json!({ "aim": { "x": 10, "z": 10 } })),
        0,
        0.0,
    );
    set_translation(&mut sim, i, 0.0, 0.65, 0.0);
    assert_eq!(
        controls.command(&mut sim, 1, 10.0).unwrap().aim,
        std::f64::consts::FRAC_PI_4
    );
    set_translation(&mut sim, i, 10.0, 0.65, 0.0);
    let command = controls.command(&mut sim, 2, 20.0).unwrap();
    assert_eq!(command.aim, 0.0);
    assert_eq!(command.move_x, 1.0);
    assert!(!command.mine);
    controls.accept(
        &sim,
        &input(e, 2, json!({ "aim": { "angle": -1 } })),
        0,
        30.0,
    );
    assert_eq!(controls.command(&mut sim, 3, 40.0).unwrap().aim, -1.0);
}

#[test]
fn death_respawn_suspension_and_reconnect_epochs_discard_old_held_input_and_queued_actions() {
    let (mut sim, tank) = alice_room();
    let mut controls = PlayerControls::new(&sim, tank, 0.0, true).unwrap();
    let old = input(
        controls.control_epoch,
        1,
        json!({ "actions": [{ "type": "mine" }] }),
    );
    controls.accept(&sim, &old, 0, 0.0);
    let i = tank_index(&sim, tank);
    sim.tanks[i].protection = 0.0;
    sim.damage_tank(i, 10000.0, 0, Team::Red, None, None);
    let dead = controls.command(&mut sim, 1, 10.0).unwrap();
    assert!(!dead.mine);
    assert!(!dead.fire);
    assert!(!controls.accept(&sim, &old, 0, 10.0));
    let dead_epoch = controls.control_epoch;
    sim.respawn(i, None);
    assert!(!controls.command(&mut sim, 2, 20.0).unwrap().mine);
    assert!(controls.control_epoch > dead_epoch);
    let fresh = input(
        controls.control_epoch,
        1,
        json!({ "actions": [{ "type": "mine" }] }),
    );
    controls.accept(&sim, &fresh, 0, 30.0);
    controls.suspend(&mut sim);
    let suspended = controls.control_epoch;
    controls.suspend(&mut sim);
    assert_eq!(controls.control_epoch, suspended);
    assert!(controls.command(&mut sim, 3, 40.0).is_none());
    controls.resume(&mut sim, 50.0);
    assert!(!controls.command(&mut sim, 4, 50.0).unwrap().mine);
    assert!(!controls.command(&mut sim, 5, 60.0).unwrap().fire);
    assert!(!controls.accept(&sim, &old, 0, 60.0));
}

#[test]
fn rate_limited_traffic_cannot_extend_an_input_lease() {
    let (mut sim, tank) = alice_room();
    let mut controls = PlayerControls::new(&sim, tank, 0.0, true).unwrap();
    let e = controls.control_epoch;
    for seq in 1..=60 {
        assert!(controls.accept(&sim, &input(e, seq, json!({})), 0, 0.0));
    }
    assert!(!controls.accept(&sim, &input(e, 61, json!({})), 0, 200.0));
    assert!(!controls.command(&mut sim, 15, 250.0).unwrap().fire);
    assert!(controls.accept(&sim, &input(e, 61, json!({})), 0, 1000.0));
    assert!(controls.command(&mut sim, 60, 1000.0).unwrap().fire);
}

#[test]
fn human_only_suspension_and_input_timeout_never_enable_ai_and_resume_clears_old_actions() {
    let (mut sim, tank) = alice_room();
    let mut controls = PlayerControls::new(&sim, tank, 0.0, false).unwrap();
    let packet = input(
        controls.control_epoch,
        1,
        json!({ "aim": { "angle": 0 }, "actions": [{ "type": "mine" }] }),
    );
    assert!(controls.accept(&sim, &packet, 0, 0.0));
    controls.suspend(&mut sim);
    let i = tank_index(&sim, tank);
    assert_eq!(sim.tanks[i].driver, Driver::Idle);
    assert!(!controls.accept(&sim, &packet, 0, 1.0));
    sim.start();
    let mut commands = BTreeMap::new();
    commands.insert(
        tank,
        VehicleCommand {
            move_x: 1.0,
            fire: true,
            mine: true,
            ..VehicleCommand::idle()
        },
    );
    for _ in 0..30 {
        sim.step_with(&commands);
    }
    assert_eq!(
        sim.tanks[i].command.move_x, 0.0,
        "idle driver cannot consume stale commands"
    );
    assert!(!sim.tanks[i].command.fire);
    assert!(!sim.tanks[i].command.mine);
    controls.resume(&mut sim, 100.0);
    assert_eq!(sim.tanks[i].driver, Driver::Human);
    assert!(!controls.command(&mut sim, 1, 100.0).unwrap().fire);
    assert!(!controls.command(&mut sim, 1, 100.0).unwrap().mine);
    controls.command(&mut sim, 300, 5100.0);
    assert_eq!(
        sim.tanks[i].driver,
        Driver::Idle,
        "silent human-only seats never hand control to AI"
    );
}

#[test]
fn wire_input_is_rounded_omits_idle_defaults_and_is_accepted_as_the_same_command() {
    let (mut sim, tank) = alice_room();
    let mut controls = PlayerControls::new(&sim, tank, 0.0, true).unwrap();
    let e = controls.control_epoch;
    let idle = ControlInput {
        control_epoch: e,
        seq: 1,
        observed_tick: 0,
        tick: None,
        move_x: 0.0,
        move_z: 0.0,
        aim: Aim::Point {
            x: 30.000_400_1,
            z: -40.123_456_7,
        },
        fire: false,
        actions: Vec::new(),
        view: 0,
    };
    let wire: Value = serde_json::from_str(&encode_input(&idle, 1)).unwrap();
    assert!(wire.get("fire").is_none());
    assert!(wire.get("actions").is_none());
    assert_eq!(wire["aim"], json!({ "x": 30, "z": -40.123 }));
    assert!(controls.accept(&sim, &wire, 0, 0.0));
    assert!(!controls.command(&mut sim, 1, 0.0).unwrap().fire);
    let edge = ControlInput {
        seq: 2,
        move_x: -0.123_456,
        aim: Aim::Angle(std::f64::consts::PI - 1e-6),
        fire: true,
        actions: vec![Action::Mine],
        ..idle
    };
    let wire: Value = serde_json::from_str(&encode_input(&edge, 1)).unwrap();
    assert_eq!(wire["moveX"], json!(-0.12));
    assert_eq!(
        wire["aim"],
        json!({ "angle": std::f64::consts::PI }),
        "rounding cannot push an angle past pi"
    );
    assert!(controls.accept(&sim, &wire, 1, 1.0));
    let command = controls.command(&mut sim, 2, 1.0).unwrap();
    assert!(command.fire);
    assert!(command.mine);
}

#[test]
fn a_click_whose_press_and_release_start_on_one_tick_still_fires_once() {
    let (mut sim, tank) = alice_room();
    let mut controls = PlayerControls::new(&sim, tank, 0.0, true).unwrap();
    let still = json!({ "moveX": 0 });
    // The press asked for tick 3 but arrived for tick 5; its release asked for tick 5.
    let press = input(
        1,
        1,
        net_support::merged(still.clone(), json!({ "tick": 3 })),
    );
    let release = input(
        1,
        2,
        net_support::merged(still, json!({ "tick": 5, "fire": false })),
    );
    assert!(controls.accept_at(&sim, &press, 4, 5, 70.0));
    assert!(controls.accept_at(&sim, &release, 4, 5, 72.0));
    let at_five = controls.command(&mut sim, 5, 83.0).unwrap();
    assert!(
        at_five.fire,
        "the overtaken press fires on the tick it was due"
    );
    assert!(!controls.command(&mut sim, 6, 100.0).unwrap().fire);
    assert_eq!(controls.ack.input_seq, 2, "the release drives from then on");
}

#[test]
fn an_action_asked_to_wait_keeps_its_lease_until_its_tick() {
    let (mut sim, tank) = alice_room();
    let mut controls = PlayerControls::new(&sim, tank, 0.0, true).unwrap();
    // A mine asked for 20 ticks (333 ms) after its arrival, longer than a lease.
    let later = input(1, 1, json!({ "tick": 21, "actions": [{ "type": "mine" }] }));
    assert!(controls.accept_at(&sim, &later, 0, 1, 0.0));
    assert!(!controls.command(&mut sim, 20, 320.0).unwrap().mine);
    assert!(
        controls.command(&mut sim, 21, 340.0).unwrap().mine,
        "the mine is laid on the tick it asked for"
    );
}

#[test]
fn a_stand_in_bot_or_a_new_occupant_fires_without_the_players_view_delay() {
    let (mut sim, tank) = alice_room();
    sim.shooter_rewind = Some(ShooterRewind::default());
    let index = tank_index(&sim, tank);
    let mut controls = PlayerControls::new(&sim, tank, 0.0, true).unwrap();
    assert!(controls.accept(&sim, &input(1, 1, json!({ "view": 12 })), 0, 0.0));
    controls.command(&mut sim, 1, 0.0).unwrap();
    let rewind = |sim: &Simulation| {
        sim.shooter_rewind
            .as_ref()
            .unwrap()
            .view_ticks
            .get(&tank)
            .copied()
    };
    assert_eq!(rewind(&sim), Some(12));
    // Her shell is fixed at her view delay and keeps it once she leaves.
    fire_weapon(&mut sim, index);
    let hers = sim.shots.last().unwrap().id;
    ShooterRewind::record(&mut sim);
    controls.suspend(&mut sim);
    assert_eq!(rewind(&sim), None, "the stand-in bot sees the present");
    sim.tanks[index].cooldown = 0.0;
    fire_weapon(&mut sim, index);
    let bots = sim.shots.last().unwrap().id;
    assert_ne!(bots, hers);
    ShooterRewind::record(&mut sim);
    let shells = sim.shooter_rewind.as_ref().unwrap();
    assert_eq!(shells.shell_ticks(hers), 12);
    assert_eq!(
        shells.shell_ticks(bots),
        0,
        "the bot's shell rewinds nothing"
    );

    // A departing player's seat forgets the view too.
    sim.shooter_rewind
        .as_mut()
        .unwrap()
        .view_ticks
        .insert(tank, 12);
    release_player_tank(&mut sim, tank);
    assert_eq!(rewind(&sim), None);
}

// ---- Room rosters -------------------------------------------------------------------

fn players() -> [PlayerAssignment; 2] {
    [
        PlayerAssignment {
            player_id: "alice".into(),
            name: "Alice".into(),
            team: Team::Blue,
            slot: 0,
            kind: VehicleKind::Scout,
        },
        PlayerAssignment {
            player_id: "bob".into(),
            name: "Bob".into(),
            team: Team::Red,
            slot: 0,
            kind: VehicleKind::Heavy,
        },
    ]
}

/// A room seating Alice and Bob.
fn alice_and_bob_room(options: MultiplayerOptions) -> Simulation {
    create_multiplayer_simulation(4242.0, &players(), options).unwrap()
}

#[test]
fn humans_only_creates_just_assigned_seats_including_sparse_slots() {
    for map in [MapId::Village, MapId::Harbor, MapId::Quarry] {
        let roster: Vec<PlayerAssignment> = players()
            .into_iter()
            .enumerate()
            .map(|(i, player)| PlayerAssignment {
                slot: i + 3,
                ..player
            })
            .collect();
        let mut sim = create_multiplayer_simulation(
            4242.0,
            &roster,
            MultiplayerOptions {
                map_mode: Some(map),
                humans_only: Some(true),
                ..MultiplayerOptions::default()
            },
        )
        .unwrap();
        assert_eq!(sim.tanks.len(), 2);
        assert!(
            sim.tanks
                .iter()
                .all(|tank| tank.human && tank.driver == Driver::Human)
        );
        let ids: Vec<&str> = sim
            .tanks
            .iter()
            .map(|tank| tank.player_id.as_deref().unwrap())
            .collect();
        assert_eq!(ids, ["alice", "bob"]);
        sim.start();
        for _ in 0..180 {
            sim.step_with(&BTreeMap::new());
        }
        assert_eq!(sim.shots_fired, 0);
        sim.reset(None);
        assert_eq!(sim.tanks.len(), 2);
    }
}

fn arena(sim: &mut Simulation) {
    let keep: Vec<usize> = (0..sim.tanks.len())
        .filter(|&i| sim.tanks[i].human)
        .collect();
    clear_arena(sim, &keep);
    for i in 0..sim.tanks.len() {
        set_translation(sim, i, i as f64 * 20.0 - 10.0, 0.65, 0.0);
        sim.tanks[i].heading = 0.0;
        sim.tanks[i].protection = 0.0;
    }
    sim.world.step();
    sim.start();
}

#[test]
fn multiplayer_fills_twelve_stable_slots_validates_ownership_and_enforces_capacities() {
    let base = players();
    let roster: Vec<PlayerAssignment> = (0..8)
        .map(|i| PlayerAssignment {
            player_id: format!("p{i}"),
            slot: i / 2,
            ..base[i % 2].clone()
        })
        .collect();
    let mut sim =
        create_multiplayer_simulation(4242.0, &roster, MultiplayerOptions::default()).unwrap();
    assert_eq!(sim.tanks.len(), 12);
    assert_eq!(sim.tanks.iter().filter(|tank| tank.human).count(), 8);
    assert_eq!(
        sim.tanks
            .iter()
            .filter(|tank| tank.team == Team::Blue)
            .count(),
        6
    );
    let p1 = sim
        .tanks
        .iter()
        .find(|tank| tank.player_id.as_deref() == Some("p1"))
        .unwrap();
    assert_eq!(p1.kind, VehicleKind::Heavy);
    let ids: Vec<u32> = sim.tanks.iter().map(|tank| tank.id).collect();
    sim.reset(None);
    assert_eq!(
        sim.tanks.iter().map(|tank| tank.id).collect::<Vec<_>>(),
        ids
    );
    assert_eq!(sim.tanks.iter().filter(|tank| tank.human).count(), 8);
    let bot = sim.tanks.iter().position(|tank| !tank.human).unwrap();
    assert!(set_driver(&mut sim, bot, Driver::Human).is_err());
    let invalid: [Vec<PlayerAssignment>; 5] = [
        {
            let mut nine = roster.clone();
            nine.push(PlayerAssignment {
                player_id: "ninth".into(),
                slot: 5,
                ..base[0].clone()
            });
            nine
        },
        vec![base[0].clone(), base[0].clone()],
        vec![PlayerAssignment {
            slot: 6,
            ..base[0].clone()
        }],
        vec![PlayerAssignment {
            kind: VehicleKind::Humvee,
            ..base[0].clone()
        }],
        vec![PlayerAssignment {
            name: " ".into(),
            ..base[0].clone()
        }],
    ];
    for roster in invalid {
        assert!(
            create_multiplayer_simulation(4242.0, &roster, MultiplayerOptions::default()).is_err()
        );
    }
}

#[test]
fn two_player_commands_move_independently_omitted_commands_idle_and_actions_are_one_tick() {
    let mut sim = alice_and_bob_room(MultiplayerOptions::default());
    arena(&mut sim);
    let (a, b) = (sim.tanks[0].id, sim.tanks[1].id);
    for _ in 0..90 {
        let mut commands = BTreeMap::new();
        commands.insert(
            a,
            VehicleCommand {
                move_z: 1.0,
                aim: 0.3,
                ..VehicleCommand::idle()
            },
        );
        commands.insert(
            b,
            VehicleCommand {
                move_z: -1.0,
                aim: -0.4,
                ..VehicleCommand::idle()
            },
        );
        sim.step_with(&commands);
    }
    assert!(sim.body_translation(sim.tanks[0].body).z > 5.0);
    assert!(sim.body_translation(sim.tanks[1].body).z < -3.0);
    assert_eq!(sim.tanks[0].aim, 0.3);
    assert_eq!(sim.tanks[1].aim, -0.4);
    sim.tanks[0].ammo.rocket = 3.0;
    let mut commands = BTreeMap::new();
    commands.insert(
        a,
        VehicleCommand {
            mine: true,
            ammo_selection: Some(AmmoSelection::Weapon(Weapon::Rocket)),
            ..VehicleCommand::idle()
        },
    );
    sim.step_with(&commands);
    assert_eq!(sim.mines.len(), 1);
    assert_eq!(sim.tanks[0].selected_ammo, Weapon::Rocket);
    for _ in 0..400 {
        sim.step_with(&BTreeMap::new());
    }
    assert_eq!(
        sim.mines.len(),
        1,
        "a missing command never repeats the mine action"
    );
    assert!(!sim.tanks[0].command.fire);
    assert!(!sim.tanks[0].command.mine);
    assert_eq!(
        sim.tanks[0].command.aim, 0.0,
        "last explicit aim survives missing input"
    );
    let velocity = sim.body_linvel(sim.tanks[0].body);
    assert!(velocity.x.hypot(velocity.z) < 0.01);
}

#[test]
fn driver_handoff_preserves_player_balance_and_respawn_uses_each_seats_chassis() {
    let mut sim = alice_and_bob_room(MultiplayerOptions {
        difficulty: Some(Difficulty::Hard),
        ..MultiplayerOptions::default()
    });
    let humans: Vec<usize> = (0..sim.tanks.len())
        .filter(|&i| sim.tanks[i].human)
        .collect();
    let (a, b) = (humans[0], humans[1]);
    sim.tanks[a].kills = 4;
    sim.tanks[a].xp = 100.0;
    let identity = |sim: &Simulation| {
        let tank = &sim.tanks[a];
        (tank.id, tank.kind, tank.team, tank.xp, sim.max_health(tank))
    };
    let before = identity(&sim);
    let player = sim.tanks[a].name.clone();
    let life = sim.tanks[a].life;
    set_driver(&mut sim, a, Driver::Bot).unwrap();
    assert_eq!(identity(&sim), before);
    // A new life without a death: the stand-in's ordnance never credits the player.
    assert_eq!((sim.tanks[a].life, sim.tanks[a].deaths), (life + 1, 0));
    // The stand-in shows a bot's name, marked as a bot, with a score of its own.
    let stand_in = sim.tanks[a].name.clone();
    assert!(sim.tanks[a].bot);
    assert_eq!(sim.tanks[a].away_player.as_deref(), Some(player.as_str()));
    assert_eq!(sim.tanks[a].kills, 0);
    assert!(
        sim.tanks
            .iter()
            .enumerate()
            .all(|(i, tank)| i == a || tank.name != stand_in),
        "{stand_in} is no other tank's name"
    );
    assert_eq!(enemy_difficulty(&sim, &sim.tanks[a]).damage, 1.0);
    for bot in sim.tanks.iter().filter(|tank| !tank.human) {
        assert_eq!(enemy_difficulty(&sim, bot).damage, 1.15);
    }
    sim.start();
    sim.step_with(&BTreeMap::new());
    assert_eq!(sim.tanks[a].driver, Driver::Bot);
    set_driver(&mut sim, a, Driver::Human).unwrap();
    assert_eq!(
        sim.tanks[a].name, player,
        "the player's name returns with them"
    );
    assert!(!sim.tanks[a].bot);
    assert_eq!(sim.tanks[a].away_player, None);
    assert_eq!(sim.tanks[a].life, life + 2);
    sim.tanks[a].protection = 0.0;
    sim.tanks[b].protection = 0.0;
    let (b_id, b_team, b_life) = (sim.tanks[b].id, sim.tanks[b].team, sim.tanks[b].life);
    let (a_id, a_team, a_life) = (sim.tanks[a].id, sim.tanks[a].team, sim.tanks[a].life);
    sim.damage_tank(a, 10000.0, b_id, b_team, Some(b_life), None);
    sim.damage_tank(b, 10000.0, a_id, a_team, Some(a_life), None);
    for _ in 0..240 {
        sim.step_with(&BTreeMap::new());
    }
    assert_eq!(sim.tanks[a].kind, VehicleKind::Scout);
    assert_eq!(sim.tanks[b].kind, VehicleKind::Heavy);
    assert!(sim.tanks[a].alive && sim.tanks[b].alive);
    assert_eq!((sim.tanks[a].life, sim.tanks[a].deaths), (life + 3, 1));
    assert_eq!((sim.tanks[b].life, sim.tanks[b].deaths), (1, 1));
}

#[test]
fn old_life_ordnance_cannot_award_replacement_xp_or_life_kills_and_the_recap_stays_empty() {
    let mut sim = alice_and_bob_room(MultiplayerOptions::default());
    arena(&mut sim);
    let old_life = sim.tanks[0].life;
    fire_weapon(&mut sim, 0);
    place_mine(&mut sim, 0);
    assert_eq!(sim.shots[0].owner_life, Some(old_life));
    assert_eq!(sim.mines[0].owner_life, Some(old_life));
    // A future seat reassignment starts a new generation without a fake death.
    sim.tanks[0].life += 1;
    let (a_id, a_team) = (sim.tanks[0].id, sim.tanks[0].team);
    sim.damage_tank(1, 10000.0, a_id, a_team, Some(old_life), None);
    assert_eq!(sim.tanks[0].deaths, 0);
    assert_eq!(sim.tanks[0].kills, 1);
    assert_eq!(sim.tanks[0].life_kills, 0);
    assert_eq!(sim.tanks[0].xp, 0.0);
    assert_eq!(sim.combat_record, CombatRecord::default());
    assert!(
        sim.events
            .iter()
            .filter(|event| event.kind == SimEventType::Death)
            .all(|event| event.label.is_none())
    );
    let drum = sim.add_cover(&CoverDef::new(
        CoverKind::Drum,
        40.0,
        40.0,
        1.0,
        1.0,
        2.0,
        20.0,
        0x888888,
    ));
    let life = sim.tanks[0].life;
    sim.damage_cover(drum, 100.0, a_id, a_team, Some(life), None);
    for event in &sim.events {
        assert!(event.x.is_finite());
    }
}

#[test]
fn fill_bot_damage_applies_equally_on_both_teams_and_never_changes_player_seat_damage() {
    let mut sim = alice_and_bob_room(MultiplayerOptions {
        difficulty: Some(Difficulty::Hard),
        ..MultiplayerOptions::default()
    });
    let humans: Vec<usize> = (0..sim.tanks.len())
        .filter(|&i| sim.tanks[i].human)
        .collect();
    for &victim in &humans {
        let bot = (0..sim.tanks.len())
            .find(|&i| !sim.tanks[i].human && sim.tanks[i].team != sim.tanks[victim].team)
            .unwrap();
        sim.tanks[victim].protection = 0.0;
        let hp = sim.tanks[victim].hp;
        let (id, team, life) = (sim.tanks[bot].id, sim.tanks[bot].team, sim.tanks[bot].life);
        sim.damage_tank(victim, 10.0, id, team, Some(life), None);
        assert_eq!(sim.tanks[victim].hp, hp - 11.5);
    }
    let (a, b) = (humans[0], humans[1]);
    set_driver(&mut sim, a, Driver::Bot).unwrap();
    let hp = sim.tanks[b].hp;
    let (id, team, life) = (sim.tanks[a].id, sim.tanks[a].team, sim.tanks[a].life);
    sim.damage_tank(b, 10.0, id, team, Some(life), None);
    assert_eq!(sim.tanks[b].hp, hp - 10.0);
}

#[test]
fn debris_cleanup_preserves_proximity_to_either_player_rather_than_just_the_first_viewer() {
    let mut sim = alice_and_bob_room(MultiplayerOptions::default());
    arena(&mut sim);
    set_translation(&mut sim, 0, -50.0, 0.65, 0.0);
    set_translation(&mut sim, 1, 50.0, 0.65, 0.0);
    sim.fragment(50.0, 0.0, 0x888888, 1.0, FragmentShape::Shard, 1.0);
    sim.fragment(0.0, 0.0, 0x888888, 1.0, FragmentShape::Shard, 1.0);
    for i in 0..sim.fragments.len() {
        let body = sim.fragments[i].body;
        sim.world.bodies[body].sleep();
        sim.fragments[i].life = 5.0;
    }
    assert_eq!(cleanup_candidate(&mut sim), Some(1));
}

#[test]
fn viewer_specific_state_cannot_affect_multiplayer_wreck_placement_or_simulation() {
    let mut a = alice_and_bob_room(MultiplayerOptions::default());
    let mut b = alice_and_bob_room(MultiplayerOptions::default());
    a.set_wreck_view(Some(sloppy_core::sim::simulation::WreckView {
        min_x: -1.0,
        max_x: 1.0,
        min_z: -1.0,
        max_z: 1.0,
    }));
    assert!(a.wreck_view.is_none());
    for sim in [&mut a, &mut b] {
        sim.start();
        let victim = sim.human_index().unwrap();
        sim.tanks[victim].protection = 0.0;
        sim.damage_tank(victim, 10000.0, 0, Team::Red, None, None);
        for _ in 0..60 {
            sim.step_with(&BTreeMap::new());
        }
    }
    assert_eq!(a.snapshot(), b.snapshot());
    let positions = |sim: &Simulation| -> Vec<(f64, f64, f64)> {
        sim.fragments
            .iter()
            .map(|fragment| {
                let p = sim.body_translation(fragment.body);
                (p.x, p.y, p.z)
            })
            .collect()
    };
    assert_eq!(positions(&a), positions(&b));
    assert_eq!(a.rng.state, b.rng.state);
}

#[test]
fn per_tank_dispatch_preserves_legacy_command_and_rng_order() {
    // One case per map covers every difficulty and both modes.
    for (map, difficulty, mode) in [
        (MapId::Village, Difficulty::Easy, GameMode::Team),
        (MapId::Harbor, Difficulty::Normal, GameMode::Solo),
        (MapId::Quarry, Difficulty::Hard, GameMode::Team),
    ] {
        let setup = || SimulationSetup {
            map_mode: Some(map),
            difficulty: Some(difficulty),
            game_mode: Some(mode),
            ..SimulationSetup::default()
        };
        let mut before = Simulation::new(4242.0, setup());
        let mut after = Simulation::new(4242.0, setup());
        before.start();
        after.start();
        for i in 0..90 {
            let command = VehicleCommand {
                move_z: if i < 30 { 1.0 } else { 0.0 },
                move_x: if i >= 30 { 1.0 } else { 0.0 },
                aim: f64::from(i) / 90.0,
                fire: true,
                mine: i == 20,
                ammo_selection: None,
            };
            before.step(command, false);
            let mut commands = BTreeMap::new();
            commands.insert(after.human().id, command);
            after.step_with(&commands);
        }
        assert_eq!(after.snapshot(), before.snapshot());
        assert_eq!(after.rng.state, before.rng.state);
        assert_eq!(after.combat_record, before.combat_record);
    }
}

/// The host reads plain `input` text without a JSON tree. Whatever it reads that way must
/// equal what `read_input` reads from the parsed `Value`, and every other form must be
/// left to that path. Each case says whether the direct reader takes it and whether the
/// input is valid.
#[test]
fn the_direct_input_reader_agrees_with_the_value_reader_or_leaves_the_message_to_it() {
    let canonical = |aim: Aim, tick: Option<u64>, fire: bool, actions: Vec<Action>, view: u32| {
        let input = ControlInput {
            control_epoch: 3,
            seq: 7,
            observed_tick: 40,
            tick,
            move_x: -0.5,
            move_z: 1.0,
            aim,
            fire,
            actions,
            view,
        };
        encode_input(&input, 2)
    };
    let base = r#""type":"input","roundId":2,"controlEpoch":3,"seq":7,"observedTick":40,"moveX":1,"moveZ":0"#;
    let with = |rest: &str| format!("{{{base},{rest}}}");
    let point = r#""aim":{"x":3,"z":4}"#;
    let mine_and_rocket = vec![Action::Mine, Action::Ammo(Weapon::Rocket)];
    let cases: Vec<(String, bool, bool)> = vec![
        // What clients send.
        (
            canonical(Aim::Point { x: 1.5, z: -2.0 }, None, false, vec![], 0),
            true,
            true,
        ),
        (
            canonical(Aim::Angle(-1.25), Some(41), true, vec![], 12),
            true,
            true,
        ),
        (
            canonical(Aim::Angle(0.5), Some(44), true, mine_and_rocket, 600),
            true,
            true,
        ),
        // Aim: the key count decides, nulls count, and an array is not an object.
        (with(r#""aim":{"angle":3.14}"#), true, true),
        (with(r#""aim":{"angle":3.2}"#), true, false),
        (with(r#""aim":{"x":1}"#), true, false),
        (with(r#""aim":{"angle":0.5,"x":1}"#), true, false),
        (with(r#""aim":{"x":1,"z":2,"angle":0}"#), true, false),
        (with(r#""aim":{}"#), true, false),
        (with(r#""aim":{"x":1025,"z":0}"#), true, false),
        (with(r#""aim":{"angle":null}"#), false, false),
        (with(r#""aim":{"x":1,"z":null}"#), false, false),
        (with(r#""aim":{"x":1,"z":2,"w":3}"#), false, false),
        (with(r#""aim":[0.5]"#), false, false),
        (with(r#""aim":null"#), true, false),
        (with(r#""aim":"north""#), false, false),
        (format!("{{{base}}}"), true, false),
        // Fire, tick and view: null reads as missing, other types are refused.
        (with(&format!(r#"{point},"fire":null"#)), true, true),
        (with(&format!(r#"{point},"fire":1"#)), false, false),
        (
            with(&format!(r#"{point},"tick":null,"view":null"#)),
            true,
            true,
        ),
        (with(&format!(r#"{point},"tick":-1"#)), true, false),
        (with(&format!(r#"{point},"tick":1.5"#)), true, false),
        (with(&format!(r#"{point},"tick":"3""#)), false, false),
        (with(&format!(r#"{point},"view":601"#)), true, false),
        (with(&format!(r#"{point},"view":-1"#)), true, false),
        // Actions: object shapes, key counts and the bot-only TOW.
        (with(&format!(r#"{point},"actions":null"#)), true, true),
        (with(&format!(r#"{point},"actions":[]"#)), true, true),
        (with(&format!(r#"{point},"actions":{{}}"#)), false, false),
        (
            with(&format!(
                r#"{point},"actions":[{{"type":"mine","weapon":"rocket"}}]"#
            )),
            true,
            false,
        ),
        (
            with(&format!(
                r#"{point},"actions":[{{"type":"mine","weapon":null}}]"#
            )),
            false,
            false,
        ),
        (
            with(&format!(r#"{point},"actions":[{{"type":"ammo"}}]"#)),
            true,
            false,
        ),
        (
            with(&format!(
                r#"{point},"actions":[{{"type":"ammo","weapon":"tow"}}]"#
            )),
            true,
            false,
        ),
        (
            with(&format!(
                r#"{point},"actions":[{{"type":"ammo","weapon":"rocket","x":1}}]"#
            )),
            false,
            false,
        ),
        (
            with(&format!(r#"{point},"actions":[{{"weapon":"rocket"}}]"#)),
            false,
            false,
        ),
        (
            with(&format!(r#"{point},"actions":[["mine"]]"#)),
            false,
            false,
        ),
        (
            with(&format!(r#"{point},"actions":[{{"type":"bomb"}}]"#)),
            true,
            false,
        ),
        // Numbers: bounds, signs, fractions and integers past 2^53.
        (
            format!(
                r#"{{"type":"input","controlEpoch":-5,"seq":1,"observedTick":0,"moveX":1,"moveZ":0,{point}}}"#
            ),
            true,
            true,
        ),
        (
            format!(
                r#"{{"type":"input","controlEpoch":1,"seq":1,"observedTick":0,"moveX":1.0000001,"moveZ":0,{point}}}"#
            ),
            true,
            false,
        ),
        (
            format!(
                r#"{{"type":"input","controlEpoch":1,"seq":1e20,"observedTick":0,"moveX":1,"moveZ":0,{point}}}"#
            ),
            true,
            false,
        ),
        (
            format!(
                r#"{{"type":"input","controlEpoch":1,"seq":1,"observedTick":-0,"moveX":1,"moveZ":-0.0,{point}}}"#
            ),
            true,
            true,
        ),
        (
            format!(
                r#"{{"type":"input","controlEpoch":18446744073709551616,"seq":1,"observedTick":0,"moveX":1,"moveZ":0,{point}}}"#
            ),
            true,
            false,
        ),
        (
            format!(
                r#"{{"type":"input","controlEpoch":1,"seq":true,"observedTick":0,"moveX":1,"moveZ":0,{point}}}"#
            ),
            false,
            false,
        ),
        // Keys: the last duplicate wins in a `Value`, unknown keys are ignored there, and
        // escapes decode alike.
        (
            with(&format!(r#"{point},"moveX":0.5,"moveX":2"#)),
            false,
            false,
        ),
        (
            with(&format!(r#"{point},"moveX":2,"moveX":0.5"#)),
            false,
            true,
        ),
        (
            with(&format!(r#"{point},"extra":{{"deep":[1,2,3]}}"#)),
            false,
            true,
        ),
        (with(r#""aim":{"x":3,"\u007a":4}"#), true, true),
        (with(&format!(r#"{point},"view\u0000":1"#)), false, true),
        (
            format!(
                r#"{{"type":"inp\u0075t","controlEpoch":3,"seq":7,"observedTick":40,"moveX":1,"moveZ":0,{point}}}"#
            ),
            false,
            true,
        ),
        // Not an input: another type, no type, another shape, invalid JSON.
        (r#"{"type":"resync","roundId":2}"#.to_string(), false, false),
        (format!(r#"{{"roundId":2,{point}}}"#), false, false),
        (r#"["input",2]"#.to_string(), false, false),
        (format!(r#"{{"type":"input",{point}"#), false, false),
        (format!(r#"{{"type":"input",{point}}} x"#), false, false),
        (
            format!(r#"{{"type":"input","moveX":1e400,{point}}}"#),
            false,
            false,
        ),
    ];
    for (text, direct, valid) in &cases {
        let value = serde_json::from_str::<Value>(text).ok();
        let read = value.as_ref().and_then(read_input);
        assert_eq!(read.is_some(), *valid, "valid: {text}");
        match QuickMessage::parse(text) {
            Some(message) => {
                assert!(direct, "read directly: {text}");
                assert_eq!(message.input(), read, "{text}");
                let value = value.expect("valid JSON");
                assert_eq!(message.round_id(), value["roundId"].as_f64(), "{text}");
                assert_eq!(message.observed_tick(), value["observedTick"].as_f64());
            }
            None => assert!(!direct, "left to the Value path: {text}"),
        }
    }
}

#[test]
fn the_direct_reader_reads_a_ping_as_the_value_reader_does() {
    for text in [
        r#"{"type":"ping","roundId":2,"observedTick":40,"t":1234.5}"#,
        r#"{"type":"ping","roundId":null,"observedTick":-1,"t":null}"#,
        r#"{"type":"ping"}"#,
    ] {
        let value = serde_json::from_str::<Value>(text).unwrap();
        let message = QuickMessage::parse(text).expect("a plain ping");
        assert!(message.is_ping());
        assert_eq!(message.round_id(), value["roundId"].as_f64());
        assert_eq!(message.observed_tick(), value["observedTick"].as_f64());
        assert_eq!(message.t(), value["t"].as_f64());
    }
    for text in [
        r#"{"type":"ping","t":"soon"}"#,
        r#"{"type":"ping","t":1,"t":2}"#,
        r#"{"type":"ping","watch":true}"#,
    ] {
        assert!(QuickMessage::parse(text).is_none(), "{text}");
    }
}
