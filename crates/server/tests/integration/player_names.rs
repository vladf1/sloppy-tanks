//! Offensive player names: a join's name is masked with a callsign before the room's host
//! seats it, while ordinary names, including ones that contain a listed word, pass.

use serde_json::{Value, json};
use sloppy_core::net::match_host::{HostEvent, MatchHostOptions};
use sloppy_server::host::{HostOutput, RoomHost};
use sloppy_server::match_room::MatchRoom;
use sloppy_server::player_names::is_offensive;
use sloppy_server::protocol::{CONTENT_VERSION, Message, PROTOCOL_VERSION};

const ORDINARY_NAMES: &[&str] = &[
    "Scunthorpe",
    "assassin",
    "Dickens",
    "cocktail",
    "Hancock",
    "John Hancock",
    "Hitchcock",
    "Cockburn",
    "Analyst",
    "Therapist",
    "Titanic",
    "Pen Island",
    "Sussex",
    "Nigeria",
    "Gay",
    "Jew",
    "Tank Killer",
    "Headshot",
    "Sniper69",
    "Damn Hell",
];

const OFFENSIVE_NAMES: &[&str] = &[
    "fuck",
    "FUCKER",
    "f u c k",
    "f.u.c.k",
    "fuuuuuck",
    "phuck",
    "fvck",
    "xXfuckXx",
    "sh1t",
    "b1tch",
    "n1gger",
    "n i g g e r",
    "faggot",
    "c0ck",
    "p3nis",
    "pussy",
    "cunt",
    "whore",
    "porn",
    "kys",
];

#[test]
fn ordinary_names_pass_and_offensive_ones_do_not() {
    for name in ORDINARY_NAMES {
        assert!(!is_offensive(name), "{name} is an ordinary name");
    }
    for name in OFFENSIVE_NAMES {
        assert!(is_offensive(name), "{name} should be masked");
    }
}

fn room() -> MatchRoom {
    let mut tokens = 0u32;
    MatchRoom::new(MatchHostOptions {
        room_epoch: "epoch".into(),
        now_ms: 0,
        token: Box::new(move || {
            tokens += 1;
            format!("token-{tokens:016}")
        }),
        seed: 1,
        content_version: None,
    })
}

fn join(name: &str) -> String {
    json!({
        "type": "join",
        "version": PROTOCOL_VERSION,
        "contentVersion": CONTENT_VERSION,
        "name": name,
        "kind": "balanced",
    })
    .to_string()
}

/// The names in the last lobby the room sent, in seat order.
fn lobby_names(out: &mut HostOutput) -> Vec<String> {
    let lobby = out
        .take()
        .into_iter()
        .filter_map(|event| match event {
            HostEvent::Send {
                message: Message::Text(text),
                ..
            } => serde_json::from_str::<Value>(&text).ok(),
            _ => None,
        })
        .rfind(|message| message["type"] == "lobby")
        .expect("a lobby");
    lobby["players"]
        .as_array()
        .unwrap()
        .iter()
        .map(|player| player["name"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_join_seats_an_offensive_name_under_a_callsign_and_others_as_typed() {
    let mut room = room();
    let mut out = HostOutput::default();
    // A player who typed a callsign keeps it, and masked players get the next free ones.
    let joins = [
        "Tanker 1",
        "  fuuuuck ",
        "Scunthorpe",
        "\\u0073h1t",
        "Hancock",
    ];
    for (connection, name) in (1..).zip(joins) {
        // The escaped name arrives as `sh1t`, which JSON decodes to "sh1t".
        let text = if name.starts_with('\\') {
            join("NAME").replace("NAME", name)
        } else {
            join(name)
        };
        room.receive(connection, &text, 0, &mut out);
    }
    assert_eq!(
        lobby_names(&mut out),
        ["Tanker 1", "Tanker 2", "Scunthorpe", "Tanker 3", "Hancock"]
    );
}
