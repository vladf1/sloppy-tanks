//! Names in a room: players play under drawn names whatever they send, pilots under a
//! bot's, and a bot standing in for an away player under a bot's name with its own score.

use crate::net_support;

use net_support::{Harness, harness, shown};
use serde_json::{Value, json};
use sloppy_core::net::player_names::is_player_name;
use sloppy_core::net::protocol::EMPTY_GRACE_MS;
use sloppy_core::sim::bot_personalities::BOT_NAMES;

const ROOM: &str = r#"{ "mapMode": "village", "difficulty": "normal", "humansOnly": false }"#;

fn room() -> Harness {
    let mut h = harness();
    let create: Value = serde_json::from_str(ROOM).unwrap();
    h.join("alice", json!({ "team": 0, "create": create }));
    h.join("bob", json!({ "team": 1 }));
    h.advance();
    h
}

fn lobby_player(h: &Harness, viewer: &str, player: &str) -> Value {
    let id = h.latest(player, "welcome")["playerId"].clone();
    h.latest(viewer, "lobby")["players"]
        .as_array()
        .unwrap()
        .iter()
        .find(|seat| seat["playerId"] == id)
        .unwrap()
        .clone()
}

fn scoreboard_row(h: &Harness, viewer: &str, name: &str) -> Value {
    h.latest(viewer, "lobby")["scoreboard"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == name)
        .unwrap_or_else(|| panic!("{name} is on the scoreboard"))
        .clone()
}

/// `killer`'s tank destroys `victim`'s, crediting the killer's current life.
fn destroy(h: &mut Harness, killer: usize, victim: usize) {
    let sim = h.sim();
    let (id, team, life) = (
        sim.tanks[killer].id,
        sim.tanks[killer].team,
        sim.tanks[killer].life,
    );
    sim.tanks[victim].protection = 0.0;
    sim.damage_tank(victim, 10000.0, id, team, Some(life), None);
    h.advance();
}

#[test]
fn a_typed_name_never_reaches_the_room_and_a_shared_name_is_numbered() {
    let mut h = room();
    h.join("typed", json!({ "name": "Vlad <b>was</b> here" }));
    let typed = lobby_player(&h, "alice", "typed");
    let name = typed["name"].as_str().unwrap();
    assert!(is_player_name(name), "{name} is drawn for the seat");
    assert_eq!(typed["bot"], false);
    let tank = h.tank_of("typed");
    assert_eq!(h.sim().tanks[tank].name, name);
    assert!(!h.sim().tanks[tank].bot);

    h.join("twin", json!({ "name": shown("alice") }));
    assert_eq!(
        lobby_player(&h, "alice", "twin")["name"],
        format!("{} 2", shown("alice"))
    );
    assert_eq!(lobby_player(&h, "alice", "alice")["name"], shown("alice"));
}

#[test]
fn a_pilot_plays_under_an_unused_bots_name_and_mark() {
    let mut h = room();
    h.join("pilot", json!({ "name": shown("carol"), "bot": true }));
    let seat = lobby_player(&h, "alice", "pilot");
    assert_eq!(seat["bot"], true);
    let name = seat["name"].as_str().unwrap().to_string();
    assert!(BOT_NAMES.contains(&name.as_str()), "{name} is a bot's name");
    let tank = h.tank_of("pilot");
    let sim = h.sim();
    assert!(sim.tanks[tank].human && sim.tanks[tank].bot);
    assert_eq!(sim.tanks[tank].name, name);
    assert_eq!(
        sim.tanks.iter().filter(|tank| tank.name == name).count(),
        1,
        "no fill bot shares {name}"
    );
    // A new round's fill bots leave the pilot's name to it, and it stays marked.
    h.action("alice", "end", json!({}));
    h.action("alice", "start", json!({}));
    let tank = h.tank_of("pilot");
    let sim = h.sim();
    assert!(sim.tanks[tank].bot);
    assert_eq!(sim.tanks.iter().filter(|tank| tank.name == name).count(), 1);
}

#[test]
fn a_stand_in_plays_under_a_bots_name_and_score_and_the_player_returns_with_theirs() {
    let mut h = room();
    let welcome = h.latest("alice", "welcome");
    let alice = h.tank_of("alice");
    let bob = h.tank_of("bob");
    destroy(&mut h, alice, bob);
    assert_eq!(h.sim().tanks[alice].kills, 1);

    h.disconnect("alice");
    h.advance();
    let stand_in = h.sim().tanks[alice].name.clone();
    {
        let tank = &h.sim().tanks[alice];
        assert!(tank.bot && tank.human, "the seat stays the player's");
        assert!(!is_player_name(&stand_in), "{stand_in} reads as a bot");
        assert_eq!(tank.kills, 0, "the stand-in starts its own score");
    }
    // The replicated scene shows the stand-in, marked as a bot.
    let mirrored = h.mirrored("bob");
    let tanks = mirrored["entities"]["tanks"].as_array().unwrap();
    let id = h.sim().tanks[alice].id;
    let drawn = tanks.iter().find(|tank| tank["id"] == id).unwrap();
    assert_eq!(drawn["name"], stand_in.as_str());
    assert_eq!(drawn["bot"], true);

    // The stand-in's kill and death are its own, not the player's.
    let bob = h.tank_of("bob");
    h.sim().tanks[bob].respawn = 0.0;
    h.sim().respawn(bob, None);
    destroy(&mut h, alice, bob);
    assert_eq!(h.sim().tanks[alice].kills, 1);
    h.sim().respawn(bob, None);
    destroy(&mut h, bob, alice);
    assert_eq!(h.sim().tanks[alice].deaths, 1);

    h.join(
        "alice-return",
        json!({ "token": welcome["token"], "roomEpoch": welcome["roomEpoch"] }),
    );
    h.advance();
    let tank = &h.sim().tanks[alice];
    assert_eq!(
        tank.name,
        shown("alice"),
        "the player's name returns with them"
    );
    assert!(!tank.bot);
    assert_eq!((tank.kills, tank.deaths), (1, 0), "and their own score");
    h.action("bob", "end", json!({}));
    let row = scoreboard_row(&h, "bob", &shown("alice"));
    assert_eq!((&row["kills"], &row["deaths"]), (&json!(1), &json!(0)));
}

#[test]
fn a_released_seat_hands_its_tank_to_a_fill_bot_under_a_bots_name() {
    let mut h = room();
    let alice = h.tank_of("alice");
    h.disconnect("alice");
    h.advance();
    let stand_in = h.sim().tanks[alice].name.clone();
    for _ in 0..=EMPTY_GRACE_MS / 50 {
        h.advance();
    }
    let tank = &h.sim().tanks[alice];
    assert!(!tank.human && tank.bot);
    assert_eq!(tank.name, stand_in, "the stand-in's name carries on");

    // A seat given up at once hands its tank over the same way.
    h.join("carol", json!({ "team": 1 }));
    let carol = h.tank_of("carol");
    h.action("carol", "leave", json!({}));
    let tank = &h.sim().tanks[carol];
    assert!(!tank.human && tank.bot);
    assert!(
        BOT_NAMES.contains(&tank.name.as_str()),
        "{} is a bot's name",
        tank.name
    );
}
