//! The capture hooks the page's `?automation` shell offers single player: `place` sets
//! the player's tank down through [`Simulation::place_tank`], and `calm` holds the bots
//! still and harmless ([`Simulation::calm`]).

use crate::support;

use std::f64::consts::FRAC_PI_2;

use sloppy_core::sim::Simulation;
use sloppy_core::sim::math::Vec2;
use support::{PLAYER, idle, place_tank, run, squad, tank_xz};

/// Planar distance a tank at rest may settle by.
const SETTLE_METRES: f64 = 0.02;
/// A shooter id no tank has.
const NOBODY: u32 = 999_999;

fn bot_positions(s: &Simulation) -> Vec<Vec2> {
    (0..s.tanks.len())
        .filter(|&i| !s.tanks[i].human)
        .map(|i| tank_xz(s, i))
        .collect()
}

#[test]
fn a_placed_tank_stays_where_it_was_set_down() {
    let mut s = squad();
    s.calm = true;
    place_tank(&mut s, PLAYER, 10.0, -6.0, Some(FRAC_PI_2));
    run(&mut s, 1.0);
    let at = tank_xz(&s, PLAYER);
    assert!(
        (at.x - 10.0).hypot(at.z + 6.0) < SETTLE_METRES,
        "the tank rests at ({}, {})",
        at.x,
        at.z
    );
    assert!((s.tanks[PLAYER].heading - FRAC_PI_2).abs() < 1e-3);
}

#[test]
fn a_calm_field_holds_the_bots_still_and_harmless() {
    let mut s = Simulation::with_seed(7.0);
    s.calm = true;
    s.start();
    let before = bot_positions(&s);
    run(&mut s, 3.0);
    for (from, to) in before.iter().zip(&bot_positions(&s)) {
        assert!(
            (to.x - from.x).hypot(to.z - from.z) < SETTLE_METRES,
            "a bot moved from ({}, {}) to ({}, {})",
            from.x,
            from.z,
            to.x,
            to.z
        );
    }
    assert_eq!(s.shots_fired, 0);
    let bot = s.tanks.iter().position(|tank| !tank.human).unwrap();
    s.tanks[bot].protection = 0.0;
    let hp = s.tanks[bot].hp;
    let enemy = s.tanks[bot].team.opponent();
    s.damage_tank(bot, 50.0, NOBODY, enemy, None, None);
    assert_eq!(s.tanks[bot].hp, hp, "nobody takes damage");
}

#[test]
fn turning_calm_off_lets_the_bots_fight_again() {
    let mut s = squad();
    s.calm = true;
    run(&mut s, 2.0);
    assert_eq!(s.shots_fired, 0);
    s.calm = false;
    for _ in 0..600 {
        idle(&mut s);
        if s.shots_fired > 0 {
            break;
        }
    }
    assert!(s.shots_fired > 0, "the enemy 14 m away fires");
}
