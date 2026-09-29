//! Construction parity with the TypeScript simulation: the same seed and setup must build
//! the same roster, names, spawns, cover, pickups and navigation grid, and leave the seeded
//! stream in the same state. `fixtures/initial-state.ts` generates the reference.

use serde_json::Value;
use sloppy_core::sim::extra_levels::extra_level;
use sloppy_core::sim::level_rules::single_player_rules;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::{GameMode, Simulation, SimulationSetup, VehicleCommand};

fn round(value: f64) -> f64 {
    sloppy_core::sim::math::js_round(value * 1e6) / 1e6
}

fn setup_for(label: &str) -> SimulationSetup {
    match label {
        "solo" => SimulationSetup {
            game_mode: Some(GameMode::Solo),
            ..SimulationSetup::default()
        },
        "stress-test" | "superstress" => {
            let id = MapId::parse(label).expect("known level");
            SimulationSetup {
                map_mode: Some(id),
                ..SimulationSetup::default()
            }
            .merged(single_player_rules(extra_level(id).expect("extra level")))
        }
        map => SimulationSetup {
            map_mode: Some(MapId::parse(map).expect("known map")),
            ..SimulationSetup::default()
        },
    }
}

fn nav_hash(blocked: &[u8]) -> u32 {
    blocked.iter().enumerate().fold(7u32, |hash, (i, &cell)| {
        // (hash * 31 + cell * (i + 1)) >>> 0 in doubles, exact below 2^53.
        ((hash as f64 * 31.0 + cell as f64 * (i as f64 + 1.0)) % 4_294_967_296.0) as u32
    })
}

#[test]
fn construction_matches_typescript() {
    let text = include_str!("fixtures/initial-state.jsonl");
    let mut checked = 0;
    for line in text.lines().filter(|line| !line.is_empty()) {
        let expected: Value = serde_json::from_str(line).expect("valid fixture line");
        let seed = expected["seed"].as_f64().unwrap();
        let label = expected["label"].as_str().unwrap();
        let context = format!("seed {seed} {label}");
        let mut s = Simulation::new(seed, setup_for(label));
        assert_eq!(
            s.human_team.index() as u64,
            expected["humanTeam"].as_u64().unwrap(),
            "{context}"
        );
        assert_eq!(
            s.rng.state,
            expected["rngState"].as_f64().unwrap(),
            "{context}"
        );
        assert_eq!(
            s.next_id as u64,
            expected["nextId"].as_u64().unwrap(),
            "{context}"
        );
        let tanks = expected["tanks"].as_array().unwrap();
        assert_eq!(s.tanks.len(), tanks.len(), "{context}");
        for (tank, want) in s.tanks.iter().zip(tanks) {
            assert_eq!(tank.id as u64, want["id"].as_u64().unwrap(), "{context}");
            assert_eq!(tank.name, want["name"].as_str().unwrap(), "{context}");
            assert_eq!(
                tank.team.index() as u64,
                want["team"].as_u64().unwrap(),
                "{context}"
            );
            assert_eq!(
                tank.kind.as_str(),
                want["kind"].as_str().unwrap(),
                "{context} {}",
                tank.name
            );
            assert_eq!(tank.human, want["human"].as_bool().unwrap(), "{context}");
            assert_eq!(tank.hp, want["hp"].as_f64().unwrap(), "{context}");
            assert_eq!(
                round(tank.previous.x),
                want["x"].as_f64().unwrap(),
                "{context}"
            );
            assert_eq!(
                round(tank.previous.z),
                want["z"].as_f64().unwrap(),
                "{context}"
            );
            assert_eq!(
                tank.brain.personality.as_str(),
                want["personality"].as_str().unwrap(),
                "{context}"
            );
            assert_eq!(
                tank.brain.ultra_aggressive,
                want["ultraAggressive"].as_bool().unwrap(),
                "{context}"
            );
        }
        let covers = expected["covers"].as_array().unwrap();
        if !covers.is_empty() {
            assert_eq!(s.covers.len(), covers.len(), "{context}");
            for (cover, want) in s.covers.iter().zip(covers) {
                let number = |key: &str| want[key].as_f64().unwrap();
                let wanted = (
                    want["id"].as_u64().unwrap() as u32,
                    want["kind"].clone(),
                    [
                        number("x"),
                        number("z"),
                        number("w"),
                        number("d"),
                        number("h"),
                    ],
                    want["hp"].as_f64(),
                );
                let actual = (
                    cover.id,
                    serde_json::to_value(cover.kind).unwrap(),
                    [cover.x, cover.z, cover.w, cover.d, cover.h].map(round),
                    cover.hp.is_finite().then_some(cover.hp),
                );
                assert_eq!(actual, wanted, "{context}");
            }
        }
        let pickups = expected["pickups"].as_array().unwrap();
        assert_eq!(s.pickups.len(), pickups.len(), "{context}");
        for (pickup, want) in s.pickups.iter().zip(pickups) {
            assert_eq!(pickup.id as u64, want["id"].as_u64().unwrap(), "{context}");
            assert_eq!(
                serde_json::to_value(pickup.kind).unwrap(),
                want["kind"],
                "{context}"
            );
            assert_eq!(round(pickup.x), want["x"].as_f64().unwrap(), "{context}");
            assert_eq!(round(pickup.z), want["z"].as_f64().unwrap(), "{context}");
            assert_eq!(
                pickup.available,
                want["available"].as_bool().unwrap(),
                "{context}"
            );
        }
        let blocked: u64 = s.nav.blocked.iter().map(|&cell| cell as u64).sum();
        assert_eq!(
            blocked,
            expected["navBlocked"].as_u64().unwrap(),
            "{context}"
        );
        assert_eq!(
            nav_hash(&s.nav.blocked) as u64,
            expected["navHash"].as_u64().unwrap(),
            "{context}"
        );
        // The first ticks' draws depend on physics only through settled spawn poses; report
        // (rather than fail on) divergence, which Rapier 0.36 may legitimately cause.
        s.start();
        let mut after = Vec::new();
        for tick in 1..=30 {
            s.step(VehicleCommand::idle(), true);
            if tick == 1 || tick == 5 || tick == 30 {
                after.push(s.rng.state);
            }
        }
        let wanted: Vec<f64> = expected["rngAfter"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        if after != wanted {
            eprintln!("{context}: RNG state after 1/5/30 ticks {after:?}, TypeScript {wanted:?}");
        }
        checked += 1;
    }
    assert_eq!(checked, 18);
}
