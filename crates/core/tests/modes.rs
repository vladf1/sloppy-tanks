//! Game modes and map selection: spawn rows, the solo survival roster and reinforcements,
//! complete authored maps across rounds and modes, and bots engaging on every standard map
//! (the former `tests/modes.test.ts`).

mod support;

use sloppy_core::sim::ai::bot_command;
use sloppy_core::sim::bot_personalities::BotPersonality;
use sloppy_core::sim::data::ARENA;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::maps::MAPS;
use sloppy_core::sim::physics::vector;
use sloppy_core::sim::pickups::collect_pickup;
use sloppy_core::sim::{
    GameMode, MatchPhase, PickupKind, SimEventType, Simulation, SimulationSetup, Team,
    VehicleCommand, VehicleKind,
};
use support::{damage_from, idle};

fn active_enemies(s: &Simulation) -> Vec<usize> {
    (0..s.tanks.len())
        .filter(|&i| !s.tanks[i].human && s.tanks[i].alive)
        .collect()
}

fn solo(seed: f64) -> Simulation {
    let mut s = Simulation::with_seed(seed);
    s.game_mode = GameMode::Solo;
    s.map_mode = MapId::Harbor;
    s.reset(None);
    s.start();
    for tank in &mut s.tanks {
        tank.protection = 0.0;
    }
    s
}

fn kill_by_human(s: &mut Simulation, victim: usize, amount: f64) {
    let (id, team) = (s.human().id, s.human_team);
    s.damage_tank(victim, amount, id, team, None, None);
}

#[test]
fn every_spawn_row_starts_interpolation_and_bot_history_at_its_actual_body_position() {
    let mut sim = Simulation::with_seed(123.0);
    for team in [Team::Blue, Team::Red] {
        for slot in [0, 4, 5, 9, 10, 14] {
            let tank = sim.add_tank(team, false, VehicleKind::Balanced, slot);
            let position = sim.body_translation(sim.tanks[tank].body).planar();
            let tank = &sim.tanks[tank];
            assert_eq!(
                tank.previous, position,
                "team {team:?}, slot {slot}: initial render pose"
            );
            assert_eq!(
                tank.brain.last, position,
                "movement history starts at spawn"
            );
            assert_eq!(
                tank.brain.last_seen, position,
                "target history starts at spawn"
            );
            assert_eq!(
                tank.brain.recovery_goal, position,
                "recovery starts at spawn"
            );
        }
    }
}

#[test]
fn solo_roster_weak_armor_reduced_damage_repairs_and_enemy_replacements() {
    let mut s = solo(123.0);
    assert_eq!(s.tanks.len(), 7);
    assert_eq!(s.tanks.iter().filter(|t| t.team == s.human_team).count(), 1);
    for tank in &s.tanks {
        let p = s.body_translation(tank.body).planar();
        assert!(!s.nav.is_blocked(p));
    }
    let enemy = 1;
    assert!(s.tanks[enemy].hp <= 56.0);
    let h = s.human_index().unwrap();
    let hp = s.tanks[h].hp;
    damage_from(&mut s, h, 40.0, enemy, None, None);
    assert_eq!(s.tanks[h].hp, hp - 16.0);
    s.tanks[enemy].hp = 1.0;
    let repair = s
        .pickups
        .iter()
        .position(|p| p.kind == PickupKind::Repair)
        .unwrap();
    let mut supply = s.pickups[repair].clone();
    collect_pickup(&mut s, enemy, &mut supply);
    s.pickups[repair] = supply;
    assert_eq!(s.tanks[enemy].hp, s.max_health(&s.tanks[enemy]));
    kill_by_human(&mut s, enemy, 999.0);
    for _ in 0..200 {
        idle(&mut s);
    }
    assert!(s.tanks[enemy].alive);
    assert_eq!(s.match_state.phase, MatchPhase::Playing);
}

#[test]
fn solo_survives_beyond_50_kills_ends_on_death_or_ten_minutes_and_resets_cleanly() {
    let mut s = solo(123.0);
    assert_eq!(s.match_state.time, 600.0);
    let initial_bodies = s.world.bodies.len();
    for _ in 0..120 {
        let enemy = s.tanks.iter().position(|t| !t.human && t.alive).unwrap();
        s.tanks[enemy].protection = 0.0;
        kill_by_human(&mut s, enemy, 9999.0);
        s.reinforcement_delay = 0.0;
        s.reinforce_solo();
        assert_eq!(
            s.match_state.phase,
            MatchPhase::Playing,
            "Neither 20 nor 50 kills ends survival"
        );
        assert_eq!(s.tanks.len(), 7, "Enemy slots stay bounded");
        assert_eq!(s.tanks[enemy].xp, 0.0, "Replacement starts Rookie");
        assert!(s.world.bodies.len() <= initial_bodies + s.max_fragments);
    }
    assert_eq!(s.human().kills, 120);
    assert_eq!(active_enemies(&s).len(), 6);
    s.match_state.phase = MatchPhase::Paused;
    let time = s.match_state.time;
    idle(&mut s);
    assert_eq!(s.match_state.time, time);

    s.reset(None);
    s.start();
    let h = s.human_index().unwrap();
    s.tanks[h].protection = 0.0;
    damage_from(&mut s, h, 9999.0, 1, None, None);
    assert_eq!(s.match_state.phase, MatchPhase::Results);
    assert_ne!(s.match_state.winner, Some(s.human_team));

    s.reset(None);
    s.start();
    s.match_state.time = 0.001;
    idle(&mut s);
    assert_eq!(s.match_state.phase, MatchPhase::Results);
    assert_eq!(s.match_state.winner, Some(s.human_team));
    assert_eq!(s.match_state.time, 0.0);
    assert!(!s.match_state.overtime);
    assert_eq!(s.human().kills, 0);

    s.game_mode = GameMode::Team;
    s.reset(None);
    assert_eq!(s.tanks.len(), 12);
    assert_eq!(s.match_state.time, 300.0);
    assert_eq!(s.tanks.iter().filter(|t| t.team == Team::Blue).count(), 6);
}

#[test]
fn solo_enemies_fire_slowly_and_never_lay_mines() {
    let mut s = solo(123.0);
    let t = 1;
    let h = s.human_index().unwrap();
    s.tanks[t].brain.personality = BotPersonality::Minelayer;
    let (bot_body, human_body) = (s.tanks[t].body, s.tanks[h].body);
    s.world.bodies[bot_body].set_translation(vector(0.0, 0.65, 0.0), true);
    s.world.bodies[human_body].set_translation(vector(0.0, 0.65, 8.0), true);
    s.world.step();
    let human_id = s.tanks[h].id;
    let tank = &mut s.tanks[t];
    tank.brain.target = human_id;
    tank.brain.memory = 3.0;
    tank.brain.decision = 1.0;
    tank.brain.reaction = 0.0;
    tank.brain.aim_error = 0.0;
    tank.aim = 0.0;
    let command = bot_command(&mut s, t, 1.0 / 60.0);
    assert!(command.fire);
    assert!(!command.mine);
    assert!(s.tanks[t].brain.fire_delay >= 2.0);
}

#[test]
fn solo_reinforcements_replenish_six_active_enemies_and_reset_the_kill_counter() {
    let mut s = solo(123.0);
    assert_eq!(active_enemies(&s).len(), 6);
    let victim = active_enemies(&s)[0];
    kill_by_human(&mut s, victim, 999.0);
    assert_eq!(active_enemies(&s).len(), 5);
    idle(&mut s);
    assert_eq!(active_enemies(&s).len(), 6);
    assert!(s.tanks[victim].alive);
    assert_eq!(s.human().kills, 1);
    for _ in 0..90 {
        idle(&mut s);
        assert!(active_enemies(&s).len() <= 6);
    }
    s.reset(None);
    assert_eq!(active_enemies(&s).len(), 6);
    assert_eq!(s.tanks.len(), 7);
    assert_eq!(s.human().kills, 0);
}

#[test]
fn each_authored_map_builds_completely_in_both_modes_keeps_it_for_the_match_and_resets_without_leaking_bodies()
 {
    for game_mode in [GameMode::Team, GameMode::Solo] {
        let mut sim = Simulation::with_seed(912.0);
        sim.game_mode = game_mode;
        for map in &MAPS {
            sim.map_mode = map.id;
            let mut bodies = 0;
            for round in 0..2 {
                sim.reset(None);
                if round == 0 {
                    bodies = sim.world.bodies.len();
                }
                assert_eq!(
                    sim.world.bodies.len(),
                    bodies,
                    "a new round rebuilds the same world"
                );
                assert_eq!(
                    sim.map_theme(),
                    map.id.as_str(),
                    "selection survives a new round"
                );
                assert_eq!(
                    sim.map_name(),
                    map.name.to_uppercase(),
                    "show the actual battlefield name"
                );
                let built: Vec<_> = sim
                    .covers
                    .iter()
                    .map(|c| (c.kind, c.x, c.z, c.w, c.d, c.h, c.hp, c.color))
                    .collect();
                let authored: Vec<_> = (map.layout)()
                    .iter()
                    .map(|c| (c.kind, c.x, c.z, c.w, c.d, c.h, c.hp, c.color))
                    .collect();
                assert_eq!(built, authored, "use the entire authored layout");
                assert_eq!(
                    sim.tanks.len(),
                    if game_mode == GameMode::Solo { 7 } else { 12 }
                );
                sim.start();
                idle(&mut sim);
                sim.match_state.phase = MatchPhase::Paused;
                idle(&mut sim);
                assert_eq!(
                    sim.map_theme(),
                    map.id.as_str(),
                    "playing and pausing never change the map"
                );
            }
        }
    }
}

/// Seed 417 bots land their first tank hit by step 432 on every map; this leaves margin.
const BATTLE_STEPS: usize = 600;

fn bots_fire_and_hit_inside_the_boundary(map: MapId) {
    let mut sim = Simulation::new(
        417.0,
        SimulationSetup {
            map_mode: Some(map),
            ..SimulationSetup::default()
        },
    );
    sim.start();
    let ids: Vec<u32> = sim.tanks.iter().map(|tank| tank.id).collect();
    let mut hits = 0;
    for _ in 0..BATTLE_STEPS {
        sim.step(VehicleCommand::idle(), true);
        for event in &sim.events {
            if matches!(event.kind, SimEventType::Hurt | SimEventType::Death)
                && event.owner.is_some_and(|owner| ids.contains(&owner))
                && event.owner != event.id
            {
                hits += 1;
            }
        }
        sim.events.clear();
        for tank in sim.tanks.iter().filter(|tank| tank.alive) {
            let p = sim.body_translation(tank.body);
            assert!(p.x.is_finite() && p.z.is_finite());
            assert!(
                p.x.abs() < ARENA && p.z.abs() < ARENA,
                "{} left the arena",
                tank.name
            );
        }
    }
    assert!(sim.shots_fired > 0, "bots engage on this layout");
    assert!(hits > 0, "bot shells reach other tanks");
}

#[test]
fn pine_village_bots_fire_and_hit_each_other_while_the_boundary_walls_contain_every_tank() {
    bots_fire_and_hit_inside_the_boundary(MapId::Village);
}

#[test]
fn harbor_bots_fire_and_hit_each_other_while_the_boundary_walls_contain_every_tank() {
    bots_fire_and_hit_inside_the_boundary(MapId::Harbor);
}

#[test]
fn quarry_bots_fire_and_hit_each_other_while_the_boundary_walls_contain_every_tank() {
    bots_fire_and_hit_inside_the_boundary(MapId::Quarry);
}
