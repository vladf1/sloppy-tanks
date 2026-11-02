//! The engine JSON the page parses, pinned byte for byte for a seeded game
//! (`tests/fixtures/page-json.json`): `hud_json` while playing and at Results with the
//! round recap, `drain_events`, and a room's HUD, actions and notices with the room
//! menu's words. The desktop shell reads the same values typed, so changing how it reads
//! them must leave this text alone.
//! Rerun with `SLOPPY_UPDATE_FIXTURES=1` after a deliberate change to the page's JSON or
//! to the seeded arena.
//!
//! The match is set by hand on a freshly built arena rather than played: physics steps
//! may round differently between platforms, while a new arena is the same everywhere
//! (`crates/core/tests/integration/initial_state.rs`).

use std::collections::BTreeMap;

use serde_json::{Value, json};
use sloppy_core::net::client::{ClientAction, ClientNotice, EndCause};
use sloppy_core::net::protocol::{Lobby, RoomPhase};
use sloppy_core::net::schema::parse_record;
use sloppy_core::sim::match_state::end_battle;
use sloppy_core::sim::render_state::RenderTank;
use sloppy_core::sim::round_recap::{RecordStorage, StorageUnavailable};
use sloppy_core::sim::{Match, MatchPhase, SimEvent, Simulation, SimulationSetup, Team, Weapon};

use crate::Shell;
use crate::events::{PendingEvent, batch_json, take_events};
use crate::feedback::event_feedback;
use crate::hud::{EffectTimers, Scoreboard, room_hud, simulation_hud};
use crate::recap::round_recap;
use crate::room_notices::RoomNotice;
use crate::room_text::RoomText;
use crate::sounds::SoundPlanner;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/page-json.json");
const SEED: f64 = 20261028.0;

/// Personal bests a returning player saved, whatever the key.
struct SavedBests(&'static str);

impl RecordStorage for SavedBests {
    fn get_item(&self, _key: &str) -> Result<Option<String>, StorageUnavailable> {
        Ok(Some(self.0.to_owned()))
    }

    fn set_item(&mut self, _key: &str, _value: &str) -> Result<(), StorageUnavailable> {
        Ok(())
    }
}

/// A new arena of the page's first round, two minutes into a close match: the human a
/// wounded veteran with special ammunition and every timed effect running, and a few
/// kills on the board.
fn seeded_match() -> Simulation {
    let mut sim = Simulation::new(
        SEED,
        SimulationSetup {
            round: Some(3),
            ..SimulationSetup::default()
        },
    );
    sim.start();
    sim.elapsed = 121.25;
    sim.match_state.time = 178.75;
    sim.match_state.scores = [7, 9];
    let human = sim.human_index().unwrap();
    let tank = &mut sim.tanks[human];
    tank.hp = 61.5;
    tank.xp = 320.0;
    tank.kills = 4;
    tank.deaths = 2;
    tank.ammo.spread = 6.0;
    tank.ammo.rocket = 3.0;
    tank.selected_ammo = Weapon::Rocket;
    tank.mine_cooldown = 2.5;
    tank.protection = 1.5;
    tank.shield = 4.0;
    tank.shield_points = 80.0;
    tank.rapid = 3.25;
    tank.speed = 0.5;
    tank.laser = 7.0;
    tank.last_combat = 100.0;
    tank.damage_dealt = 412.6;
    tank.best_life_kills = 3;
    tank.highest_rank = 1;
    for (index, other) in sim.tanks.iter_mut().filter(|tank| !tank.human).enumerate() {
        other.kills = (index % 4) as u32;
        other.deaths = (index % 3) as u32;
        other.alive = index != 2;
    }
    let combat = &mut sim.combat_record;
    combat.shots = 31;
    combat.direct_hits = 17;
    combat.busiest_minute = 3;
    combat.multikill = 3;
    combat.revenge_kills = 1;
    combat.clutch_kills = 1;
    combat.pickups = 5;
    sim
}

/// One of each event kind with every optional field among them, the viewer's flags and
/// the feedback for the tanks of `sim`, ending with the viewer's death.
fn events(sim: &Simulation) -> Vec<PendingEvent> {
    let human = sim.human().id;
    let other = human + 1;
    let events = json!([
        {"type": "shot", "x": 1.5, "z": -2.25, "id": human, "owner": human, "ownerLife": 2,
            "weapon": "rocket", "team": 0, "size": 1.0, "color": 0xff591c},
        {"type": "hurt", "x": 3.0, "z": 4.0, "id": human, "owner": other, "ownerLife": 1,
            "team": 0, "damageSource": {"cause": "spread", "origin": {"x": 6.0, "z": 8.5}}},
        {"type": "hurt", "x": -7.0, "z": 2.0, "id": other, "owner": human, "team": 1,
            "damageSource": {"cause": "standard", "origin": {"x": 1.0, "z": 1.0}}},
        {"type": "death", "x": -7.0, "z": 2.0, "id": other, "owner": human, "ownerLife": 2,
            "team": 1, "deathStyle": "burnout",
            "damageSource": {"cause": "mine", "origin": {"x": -7.5, "z": 2.5}}},
        {"type": "destroy", "x": 10.0, "z": -3.5, "coverKind": "timber", "material": "wood",
            "force": 2.5, "height": 1.25, "from": {"x": 9.0, "y": 0.5, "z": -3.0}},
        {"type": "shatter", "x": 11.0, "z": 0.25, "coverKind": "cargo", "height": 0.75,
            "size": 1.2, "color": 0x8a5a44, "from": {"x": 10.5, "y": 0.8, "z": 0.0}},
        {"type": "debris-impact", "x": 0.5, "z": 0.5, "material": "metal", "force": 0.375},
        {"type": "impact", "x": 2.0, "z": 2.0, "weapon": "piercing", "team": 1},
        {"type": "ricochet", "x": 2.5, "z": 2.0, "weapon": "ricochet"},
        {"type": "explosion", "x": -1.0, "z": 5.0, "owner": other, "size": 2.0,
            "weapon": "tow"},
        {"type": "laser", "x": 0.0, "z": 1.0, "id": human, "from": {"x": 0.0, "y": 1.5,
            "z": 0.0}},
        {"type": "notice", "x": 0.0, "z": 0.0, "id": human, "label": "Out of \"ROCKETS\""},
        {"type": "pickup", "x": 4.0, "z": 4.0, "id": human, "label": "SHIELD",
            "color": 0x5fd8ff},
        {"type": "promotion", "x": 0.0, "z": 0.0, "id": human, "label": "VETERAN"},
        {"type": "respawn", "x": -40.0, "z": 30.0, "id": other, "team": 1},
        {"type": "death", "x": 3.0, "z": 4.0, "id": human, "owner": other, "team": 0,
            "damageSource": {"cause": "rocket", "origin": {"x": 6.0, "z": 8.5}}},
    ]);
    let scoreboard = Scoreboard::Simulated(&sim.tanks);
    serde_json::from_value::<Vec<SimEvent>>(events)
        .unwrap()
        .into_iter()
        .enumerate()
        .map(|(index, event)| {
            let own = event.id == Some(human);
            PendingEvent {
                player_hit: index == 2 || index == 3,
                own,
                damage_angle: (index == 1).then_some(-0.75),
                feedback: event_feedback(&event, own, human, &scoreboard),
                event,
            }
        })
        .collect()
}

/// The room page's drained notices for one of each kind.
fn notices() -> Value {
    let lobby = parse_record(
        r#"{"roomEpoch":"e1","roundId":2,"type":"lobby","phase":"results","hostId":"p1",
        "players":[{"playerId":"p1","name":"NITRO \"N\"","kind":"heavy","team":1,"slot":0,
        "connected":true,"bot":false,"kills":3,"deaths":1,"tankId":4}],"scoreboard":[],
        "settings":{"mapMode":"quarry","difficulty":"easy","humansOnly":false,"roundMinutes":15}}"#,
    )
    .unwrap();
    let outcome = Match {
        phase: MatchPhase::Results,
        time: 12.5,
        scores: [100, 40],
        overtime: true,
        ended_early: Some(false),
        winner: Some(Team::Blue),
        round: 2,
    };
    let notices = [
        ClientNotice::Status {
            text: "Connected".into(),
            connected: true,
        },
        ClientNotice::Notice("Team full".into()),
        ClientNotice::Ended {
            cause: EndCause::OtherTab,
            text: "Playing in another tab".into(),
        },
        ClientNotice::Lobby(Lobby::read(&lobby).unwrap()),
        ClientNotice::Result {
            outcome,
            team: Team::Red,
        },
        ClientNotice::ResetFeedback,
        ClientNotice::ClearInput,
        ClientNotice::Reveal,
        ClientNotice::PrepareArena {
            round_id: 2,
            room_epoch: "e1".into(),
        },
        ClientNotice::ShowBaseline { baseline: 7 },
    ];
    let mut text = RoomText::default();
    let mut notices: Vec<RoomNotice> = notices
        .into_iter()
        .filter_map(|notice| RoomNotice::from_client(notice, "p1", &mut text, Shell::Page))
        .chain([
            RoomNotice::Prepare,
            RoomNotice::ArenaFailed("No adapter".into()),
            RoomNotice::BaselineShown,
        ])
        .collect();
    // The room menu's other words: once back in the room, a newcomer in the feed, a
    // drop's dialog during a battle and the renderer's own ending.
    let mut lobby = Lobby::read(&lobby).unwrap();
    let mut newcomer = lobby.players[0].clone();
    newcomer.player_id = "p2".into();
    newcomer.name = "Crank".into();
    newcomer.team = Team::Blue;
    lobby.players.push(newcomer);
    lobby.phase = RoomPhase::Playing;
    let later = [
        ClientNotice::Status {
            text: "Connected".into(),
            connected: true,
        },
        ClientNotice::Lobby(lobby),
        ClientNotice::Status {
            text: "Reconnecting…".into(),
            connected: false,
        },
    ];
    notices.extend(
        later
            .into_iter()
            .filter_map(|notice| RoomNotice::from_client(notice, "p1", &mut text, Shell::Page)),
    );
    notices.push(RoomNotice::Ended(text.renderer_stopped(Shell::Page)));
    Value::Array(notices.iter().map(RoomNotice::to_value).collect())
}

fn actions() -> Vec<ClientAction> {
    vec![
        ClientAction::OpenSocket {
            socket: 1,
            url: "wss://example.test/room/ABCDEFGH".into(),
        },
        ClientAction::Send {
            socket: 1,
            text: r#"{"type":"ping"}"#.into(),
        },
        ClientAction::SaveSeat {
            token: "t".repeat(16),
            room_epoch: "e1".into(),
        },
        ClientAction::ForgetSeat,
        ClientAction::CloseSocket { socket: 1 },
    ]
}

/// Each pinned text by name.
fn page_json() -> BTreeMap<&'static str, String> {
    let mut texts = BTreeMap::new();
    let mut sim = seeded_match();
    let mut events = events(&sim);
    let mut sounds = SoundPlanner::default();
    sounds.set_clock(1000.0);
    texts.insert(
        "events",
        batch_json(
            take_events(&mut events, (12.5, -3.25), (0.6, -0.8), &mut sounds),
            &mut events,
        ),
    );
    let mut tank = RenderTank::default();
    let hud = simulation_hud(&sim, &mut tank, None, &mut EffectTimers::default());
    let text = serde_json::to_string(&hud).unwrap();
    // A copy the desktop keeps between reads reads the same.
    assert_eq!(serde_json::to_string(&hud.into_owned()).unwrap(), text);
    texts.insert("hud", text);
    let state = sim.render_state(None);
    let room = room_hud(&state, &mut EffectTimers::default()).unwrap();
    let room = serde_json::to_string(&room).unwrap();
    texts.insert("roomHud", room);

    end_battle(&mut sim.match_state);
    let mut storage = SavedBests(r#"{"kills":9,"damage":250,"bestLife":2,"rank":"high"}"#);
    let recap = round_recap(&sim, &mut storage, Shell::Page);
    let hud = simulation_hud(&sim, &mut tank, Some(&recap), &mut EffectTimers::default());
    let text = serde_json::to_string(&hud).unwrap();
    assert_eq!(serde_json::to_string(&hud.into_owned()).unwrap(), text);
    texts.insert("resultsHud", text);

    texts.insert("actions", serde_json::to_string(&actions()).unwrap());
    texts.insert("notices", notices().to_string());
    texts
}

#[test]
fn the_page_reads_the_same_json() {
    let texts = page_json();
    if std::env::var_os("SLOPPY_UPDATE_FIXTURES").is_some() {
        let text = serde_json::to_string_pretty(&texts).unwrap() + "\n";
        std::fs::write(FIXTURE, text).unwrap();
    }
    let pinned: BTreeMap<String, String> = serde_json::from_str(
        &std::fs::read_to_string(FIXTURE)
            .expect("run with SLOPPY_UPDATE_FIXTURES=1 to write the fixture"),
    )
    .unwrap();
    assert_eq!(
        texts.keys().collect::<Vec<_>>(),
        pinned.keys().collect::<Vec<_>>()
    );
    for (name, text) in &texts {
        assert_eq!(
            text, &pinned[*name],
            "{name} differs from {FIXTURE} byte for byte"
        );
    }
}
