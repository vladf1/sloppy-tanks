//! Shootable mines: fast shells detonate mines exactly once, cover and clean misses protect
//! them, chains credit the shooter; mines arm after a delay, ignore allies, keep their
//! owner, and chain safely during iteration. Ported from `tests/shoot-mines.test.ts`.

mod support;

use sloppy_core::sim::data::STEP;
use sloppy_core::sim::physics::vector;
use sloppy_core::sim::weapons::{place_mine, step_mines, step_projectiles};
use sloppy_core::sim::{Mine, Shot, SimEventType, Simulation, Team, VehicleKind};
use support::{clear_arena, concrete, event_count, place_tank};

fn arena() -> Simulation {
    let mut s = Simulation::with_seed(123.0);
    clear_arena(&mut s, &[]);
    s
}

/// A blue layer (index 0) and a red victim (index 1) `gap` metres east of it.
fn duel(layer_x: f64, gap: f64) -> Simulation {
    let mut s = Simulation::with_seed(123.0);
    let layer = s.tanks.iter().position(|t| t.team == Team::Blue).unwrap();
    let victim = s.tanks.iter().position(|t| t.team == Team::Red).unwrap();
    clear_arena(&mut s, &[layer, victim]);
    s.start();
    for tank in &mut s.tanks {
        tank.protection = 0.0;
    }
    place_tank(&mut s, 0, layer_x, 0.0, None);
    place_tank(&mut s, 1, layer_x + gap, 0.0, None);
    s.world.step();
    s
}

fn mine(s: &mut Simulation, x: f64, owner: u32, team: Team, arm: f64) -> Mine {
    let id = s.next_id;
    s.next_id += 1;
    Mine {
        id,
        owner,
        owner_life: None,
        damage: None,
        team,
        x,
        z: 0.0,
        arm,
        life: 25.0,
    }
}

/// A fast standard shell fired north by an absent blue shooter.
fn shell(s: &mut Simulation, x: f64) {
    let id = s.next_id;
    s.next_id += 1;
    s.shots.push(Shot {
        id,
        x,
        z: -5.0,
        vx: 0.0,
        vz: 600.0,
        damage: 40.0,
        life: 1.0,
        owner: 999,
        team: Team::Blue,
        ..Shot::default()
    });
}

#[test]
fn fast_shells_detonate_enemy_and_friendly_mines_armed_or_arming_exactly_once() {
    for team in [Team::Blue, Team::Red] {
        for arm in [0.0, 0.8] {
            let mut s = arena();
            let m = mine(&mut s, 0.0, 5, team, arm);
            s.mines.push(m);
            shell(&mut s, 0.0);
            step_projectiles(&mut s, STEP, false);
            assert_eq!(s.mines.len(), 0);
            assert_eq!(s.shots.len(), 0);
            assert_eq!(event_count(&s, SimEventType::Explosion), 1);
        }
    }
}

#[test]
fn cover_and_clean_misses_protect_mines_while_grazing_hits_register() {
    for scenario in ["cover", "miss", "graze"] {
        let mut s = arena();
        let m = mine(&mut s, 0.0, 5, Team::Red, 0.0);
        s.mines.push(m);
        if scenario == "cover" {
            s.add_cover(&concrete(0.0, -2.0, 3.0, 0.3));
        }
        s.world.step();
        let x = match scenario {
            "miss" => 0.8,
            "graze" => 0.6,
            _ => 0.0,
        };
        shell(&mut s, x);
        step_projectiles(&mut s, STEP, false);
        assert_eq!(
            s.mines.len(),
            if scenario == "graze" { 0 } else { 1 },
            "{scenario}"
        );
    }
}

#[test]
fn shooting_a_mine_chains_nearby_mines_once_and_credits_the_shooter() {
    let mut s = arena();
    s.start();
    let target = s.add_tank(Team::Red, true, VehicleKind::Balanced, 0);
    let body = s.tanks[target].body;
    s.world.bodies[body].set_translation(vector(3.0, 0.65, 0.0), true);
    s.tanks[target].protection = 0.0;
    s.tanks[target].hp = 10.0;
    for x in [0.0, 1.0] {
        let m = mine(&mut s, x, 5, Team::Red, 0.8);
        s.mines.push(m);
    }
    shell(&mut s, 0.0);
    step_projectiles(&mut s, STEP, false);
    assert_eq!(s.mines.len(), 0);
    assert_eq!(event_count(&s, SimEventType::Explosion), 2);
    assert!(!s.tanks[target].alive);
    assert_eq!(s.match_state.scores[0], 1);
}

#[test]
fn mines_arm_after_delay_ignore_allies_and_preserve_original_owner() {
    let mut s = duel(0.0, 1.0);
    s.tanks[1].hp = 40.0;
    place_mine(&mut s, 0);
    assert_eq!(s.mines.len(), 1);
    step_mines(&mut s, 0.5);
    assert!(s.tanks[1].alive);
    assert_eq!(s.mines.len(), 1);
    step_mines(&mut s, 0.4);
    assert_eq!(s.mines.len(), 0);
    assert!(!s.tanks[1].alive);
    assert_eq!(s.match_state.scores[0], 1);
}

#[test]
fn chain_triggered_mines_are_removed_safely_during_mine_iteration() {
    let mut s = duel(-20.0, 21.0);
    s.tanks[1].hp = 20.0;
    let (owner, team) = (s.tanks[0].id, s.tanks[0].team);
    for x in [0.0, 1.0, 2.0, 3.0] {
        let m = mine(&mut s, x, owner, team, 0.0);
        s.mines.push(m);
    }
    step_mines(&mut s, STEP);
    assert_eq!(s.mines.len(), 0);
    assert_eq!(s.match_state.scores[0], 1);
}
