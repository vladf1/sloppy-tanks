//! "Favor the shooter" in rooms: a player's shell meets the other tanks where its shooter
//! saw them, a view delay ago, and never further back than the cap.

use crate::support;

use std::collections::BTreeMap;

use sloppy_core::net::multiplayer_simulation::{MultiplayerOptions, create_multiplayer_simulation};
use sloppy_core::sim::Simulation;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::shooter_rewind::{MAX_REWIND_TICKS, ShooterRewind};
use sloppy_core::sim::types::{PlayerAssignment, SimEventType, Team, VehicleCommand, VehicleKind};
use support::{clear_arena, place_tank, tank_index};

/// Bob stands broadside 15 m ahead of Alice at x = 0, then moves to `MOVED` m to the side.
const RANGE: f64 = 15.0;
const MOVED: f64 = 6.0;
const FIRE: u64 = 40;

/// Alice and Bob alone on cleared village ground, the host recording hulls for rewinds.
fn duel() -> (Simulation, u32, u32) {
    let player = |name: &str, team, slot| PlayerAssignment {
        player_id: name.into(),
        name: name.into(),
        team,
        slot,
        kind: VehicleKind::Balanced,
    };
    let mut sim = create_multiplayer_simulation(
        77.0,
        &[player("alice", Team::Blue, 0), player("bob", Team::Red, 0)],
        MultiplayerOptions {
            map_mode: Some(MapId::Village),
            humans_only: Some(true),
            ..MultiplayerOptions::default()
        },
    )
    .unwrap();
    sim.start();
    sim.shooter_rewind = Some(ShooterRewind::default());
    let (alice, bob) = (sim.tanks[0].id, sim.tanks[1].id);
    clear_arena(&mut sim, &[0, 1]);
    for tank in &mut sim.tanks {
        tank.protection = 0.0;
    }
    place_tank(&mut sim, 0, 0.0, 0.0, Some(0.0));
    sim.world.step();
    (sim, alice, bob)
}

/// Alice, her view `view` ticks behind, fires once at `aim` on tick [`FIRE`]; Bob moves
/// on tick `moves`. The tick her shell hit him, if it did.
fn shell_hits_bob(view: u32, aim: f64, moves: u64) -> Option<u64> {
    shell_hits_bob_after(view, aim, moves, false)
}

/// As [`shell_hits_bob`], with Alice destroyed the tick after she fires when `killed`.
fn shell_hits_bob_after(view: u32, aim: f64, moves: u64, killed: bool) -> Option<u64> {
    let (mut sim, alice, bob) = duel();
    sim.shooter_rewind
        .as_mut()
        .unwrap()
        .view_ticks
        .insert(alice, view);
    let bob_index = tank_index(&sim, bob);
    for tick in 0..FIRE + 120 {
        let x = if tick < moves { 0.0 } else { MOVED };
        place_tank(&mut sim, bob_index, x, RANGE, Some(90f64.to_radians()));
        if killed && tick == FIRE + 1 {
            let life = sim.tanks[bob_index].life;
            sim.damage_tank(0, f64::INFINITY, bob, Team::Red, Some(life), None);
            assert!(!sim.tanks[0].alive, "alice is destroyed");
        }
        let mut command = VehicleCommand::idle_aiming(aim);
        command.fire = tick == FIRE;
        sim.events.clear();
        sim.step_with(&BTreeMap::from([(alice, command)]));
        if sim
            .events
            .iter()
            .any(|event| event.kind == SimEventType::Hurt && event.id == Some(bob))
        {
            return Some(tick);
        }
    }
    None
}

#[test]
fn a_players_shell_meets_a_tank_where_its_shooter_saw_it() {
    let view = MAX_REWIND_TICKS;
    let at_moved = MOVED.atan2(RANGE);
    // Standing still, Bob is hit on this tick.
    let hit = shell_hits_bob(0, 0.0, u64::MAX).expect("a standing target is hit");
    // He moves just before: on Alice's screen, a view delay behind, he has not moved yet.
    let moves = hit - 1;
    assert_eq!(
        shell_hits_bob(view, 0.0, moves),
        Some(hit),
        "hit where she saw him"
    );
    assert_eq!(
        shell_hits_bob(0, 0.0, moves),
        None,
        "the host's present misses"
    );
    // Where he really is, she has not seen him yet.
    assert_eq!(shell_hits_bob(view, at_moved, moves), None);
    assert!(shell_hits_bob(0, at_moved, moves).is_some());
}

#[test]
fn a_rewind_stops_at_the_cap() {
    let hit = shell_hits_bob(0, 0.0, u64::MAX).expect("a standing target is hit");
    // Bob moved more than the cap before the hit: a view further behind still misses him
    // where it saw him.
    let moves = hit - u64::from(MAX_REWIND_TICKS) - 2;
    assert_eq!(shell_hits_bob(2 * MAX_REWIND_TICKS, 0.0, moves), None);
    assert_eq!(shell_hits_bob(MAX_REWIND_TICKS, 0.0, hit - 1), Some(hit));
}

#[test]
fn a_destroyed_shooters_shell_still_meets_a_tank_where_she_saw_it() {
    let hit = shell_hits_bob(0, 0.0, u64::MAX).expect("a standing target is hit");
    // Her screen shows every tank on the delayed display once her hull is gone.
    assert_eq!(
        shell_hits_bob_after(MAX_REWIND_TICKS, 0.0, hit - 1, true),
        Some(hit)
    );
    assert_eq!(shell_hits_bob_after(0, 0.0, hit - 1, true), None);
}
