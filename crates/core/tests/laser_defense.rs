//! The laser defense pickup: zaps stop every munition without splash or credit, the 50%
//! chance is rolled once per shell and defender, allies/outgoing/passing shells are ignored,
//! swept range entry and cover occlusion, moving defenders, pickup timing and bot interest.
//! Ported from `tests/laser-defense.test.ts`.
//!
//! The TypeScript test replaced `s.rng.next` with constant stubs and counted calls. Here the
//! seeded stream starts from a state whose next draws fall on the same side of the 50%
//! chance, and draws are counted from how far the Mulberry32 state advanced.

mod support;

use sloppy_core::sim::ai::bot_command;
use sloppy_core::sim::ammunition::AMMO_ORDER;
use sloppy_core::sim::data::{LASER_DEFENSE, STEP};
use sloppy_core::sim::math::{Random, Vec2};
use sloppy_core::sim::weapons::{collect_pickup, step_projectiles};
use sloppy_core::sim::{
    MatchPhase, PickupKind, Shot, SimEventType, Simulation, Team, VehicleCommand, VehicleKind,
    Weapon,
};
use support::{clear_arena, concrete, decide, event_count, set_translation, supply};

/// Mulberry32 advances its state by 0x6d2b79f5 per draw.
const MULBERRY_INCREMENT: f64 = 1_831_565_813.0;
/// A state whose next 25 draws are all at least 0.5: every laser check misses.
const TWENTY_FIVE_MISSES: f64 = 34_978_059.0;

/// The first non-negative integer state whose next `count` draws all satisfy `accept`.
fn rng_whose_next(count: usize, accept: impl Fn(f64) -> bool) -> Random {
    (0u32..)
        .map(|state| Random::new(state as f64))
        .find(|rng| {
            let mut probe = rng.clone();
            (0..count).all(|_| accept(probe.next()))
        })
        .unwrap()
}

/// Stands in for a stub returning 0: every laser check succeeds.
fn zapping_rng() -> Random {
    rng_whose_next(8, |draw| draw < LASER_DEFENSE.chance)
}

/// Stands in for a stub returning 0.5: every laser check misses.
fn missing_rng() -> Random {
    rng_whose_next(8, |draw| draw >= LASER_DEFENSE.chance)
}

fn draws_since(before: &Random, after: &Random) -> u64 {
    ((after.state - before.state) / MULBERRY_INCREMENT).round() as u64
}

/// The human alone at the origin on the blue team, laser active (index 0).
fn fixture() -> Simulation {
    let mut s = Simulation::with_seed(123.0);
    let human = s.human_index().unwrap();
    clear_arena(&mut s, &[human]);
    s.tanks[0].team = Team::Blue;
    s.human_team = Team::Blue;
    s.tanks[0].protection = 0.0;
    s.tanks[0].laser = LASER_DEFENSE.duration;
    set_translation(&mut s, 0, 0.0, 0.0);
    let body = s.tanks[0].body;
    s.world.bodies[body].set_rotation(Default::default(), true);
    s.tanks[0].previous = Vec2::new(0.0, 0.0);
    s.world.step();
    s.start();
    s
}

/// A red shell heading north at the defender; `adjust` applies the test's overrides.
fn incoming(s: &mut Simulation, weapon: Weapon, adjust: impl FnOnce(&mut Shot)) -> u32 {
    let id = s.next_id;
    s.next_id += 1;
    let mut shot = Shot {
        id,
        x: 0.0,
        z: -6.0,
        vx: 0.0,
        vz: 20.0,
        team: Team::Red,
        owner: 999,
        weapon,
        damage: 40.0,
        bounces: 0,
        life: 3.5,
        piercing: if weapon == Weapon::Piercing { 1 } else { 0 },
        ..Shot::default()
    };
    adjust(&mut shot);
    s.shots.push(shot);
    id
}

#[test]
fn laser_stops_every_munition_without_splash_kill_credit_or_changing_cannon_cooldown() {
    for weapon in AMMO_ORDER {
        let mut s = fixture();
        s.rng = zapping_rng();
        s.tanks[0].cooldown = 0.6;
        let hp = s.tanks[0].hp;
        incoming(&mut s, weapon, |_| {});
        step_projectiles(&mut s, STEP, false);
        assert_eq!(s.shots.len(), 0, "{weapon:?}");
        assert_eq!(s.tanks[0].hp, hp);
        assert_eq!(s.tanks[0].cooldown, 0.6);
        assert_eq!(s.match_state.scores, [0, 0]);
        assert_eq!(s.tanks[0].kills, 0);
        let kinds: Vec<_> = s.events.iter().map(|e| e.kind).collect();
        assert_eq!(kinds, vec![SimEventType::Laser]);
        assert_eq!(s.events[0].id, Some(s.tanks[0].id));
        let from = s.events[0].from.unwrap();
        assert_eq!(from.x, 0.0);
        assert!(from.y > 1.0);
    }
}

#[test]
fn fifty_percent_chance_is_rolled_once_per_shot_and_tank_and_misses_can_still_hit() {
    let mut s = fixture();
    s.rng = missing_rng();
    let before = s.rng.clone();
    // Record every sweep so the shell's final state is readable after it is removed.
    s.projectile_moves = Some(Vec::new());
    let p = incoming(&mut s, Weapon::Standard, |_| {});
    let hp = s.tanks[0].hp;
    let mut i = 0;
    while i < 20 && !s.shots.is_empty() {
        step_projectiles(&mut s, STEP, false);
        i += 1;
    }
    assert_eq!(draws_since(&before, &s.rng), 1);
    let last = s
        .projectile_moves
        .as_ref()
        .unwrap()
        .iter()
        .rev()
        .find(|m| m.shot.id == p)
        .unwrap();
    assert_eq!(last.shot.laser_checked_by, vec![s.tanks[0].id]);
    assert_eq!(s.tanks[0].hp, hp - 40.0);
    assert_eq!(event_count(&s, SimEventType::Laser), 0);
}

#[test]
fn seeded_sampling_stays_near_fifty_percent_instead_of_becoming_guaranteed_over_many_frames() {
    let mut s = fixture();
    s.rng = Random::new(7788.0);
    let mut blocked = 0;
    for _ in 0..1000 {
        incoming(&mut s, Weapon::Standard, |_| {});
        step_projectiles(&mut s, STEP, false);
        if s.shots.is_empty() {
            blocked += 1;
        }
        s.shots.clear();
        s.events.clear();
        // Each shell is a separate encounter, after the laser has recharged.
        s.tanks[0].laser_recharge = 0.0;
    }
    assert!(blocked > 450 && blocked < 550, "{blocked}/1000");
}

#[test]
fn laser_ignores_allies_outgoing_and_passing_shots_and_inactive_or_dead_defenders() {
    for mode in [
        "ally",
        "outgoing",
        "passing",
        "out-of-range",
        "expired",
        "dead",
    ] {
        let mut s = fixture();
        s.rng = zapping_rng();
        if mode == "expired" {
            s.tanks[0].laser = 0.0;
        }
        if mode == "dead" {
            let (id, team) = (s.tanks[0].id, s.tanks[0].team);
            s.damage_tank(0, 999.0, id, team, None, None);
            assert_eq!(s.tanks[0].laser, 0.0);
        }
        // Wreck construction consumes the same seeded RNG during setup.
        let before = s.rng.clone();
        incoming(&mut s, Weapon::Standard, |shot| match mode {
            "ally" => shot.team = Team::Blue,
            "outgoing" => shot.vz = -20.0,
            "passing" => shot.x = 5.0,
            "out-of-range" => shot.z = -12.0,
            _ => {}
        });
        step_projectiles(&mut s, STEP, false);
        assert_eq!(draws_since(&before, &s.rng), 0, "{mode}");
        assert_eq!(s.shots.len(), 1, "{mode}");
    }
}

#[test]
fn swept_range_entry_catches_fast_shells_before_impact_while_earlier_cover_still_wins() {
    for wall in [false, true] {
        let mut s = fixture();
        s.rng = zapping_rng();
        let before = s.rng.clone();
        if wall {
            s.add_cover(&concrete(0.0, -9.0, 5.0, 0.4));
        }
        s.world.step();
        incoming(&mut s, Weapon::Standard, |shot| {
            shot.z = -12.0;
            shot.vz = 1200.0;
        });
        step_projectiles(&mut s, STEP, false);
        assert_eq!(
            draws_since(&before, &s.rng),
            if wall { 0 } else { 1 },
            "wall={wall}"
        );
        assert_eq!(s.shots.len(), 0);
        assert_eq!(s.tanks[0].hp, 100.0);
        let beam = s.events.iter().find(|e| e.kind == SimEventType::Laser);
        assert_eq!(beam.is_some(), !wall);
        if let Some(beam) = beam {
            assert!((beam.z + LASER_DEFENSE.range).abs() < 1e-6, "{}", beam.z);
        }
    }
}

#[test]
fn cover_occludes_lasers_within_range_and_shells_already_hitting_the_hull_take_priority() {
    for mode in ["cover", "hull"] {
        let mut s = fixture();
        s.rng = zapping_rng();
        let before = s.rng.clone();
        if mode == "cover" {
            s.add_cover(&concrete(0.0, -3.0, 5.0, 0.5));
        }
        s.world.step();
        incoming(&mut s, Weapon::Standard, |shot| {
            if mode == "hull" {
                shot.z = -0.5;
            }
        });
        step_projectiles(&mut s, STEP, false);
        assert_eq!(draws_since(&before, &s.rng), 0, "{mode}");
        assert_eq!(
            s.tanks[0].hp,
            if mode == "hull" { 60.0 } else { 100.0 },
            "{mode}"
        );
    }
}

#[test]
fn moving_defenders_intercept_at_their_swept_position() {
    let mut s = fixture();
    s.rng = zapping_rng();
    set_translation(&mut s, 0, 0.0, -1.0);
    s.tanks[0].previous = Vec2::new(0.0, 0.0);
    incoming(&mut s, Weapon::Standard, |shot| shot.z = -7.5);
    step_projectiles(&mut s, STEP, true);
    assert_eq!(s.shots.len(), 0);
    let e = s
        .events
        .iter()
        .find(|e| e.kind == SimEventType::Laser)
        .unwrap();
    let from = e.from.unwrap();
    assert!(from.z < 0.0 && from.z > -1.0, "{}", from.z);
    assert!((e.z - from.z + LASER_DEFENSE.range).abs() < 1e-6);
}

#[test]
fn multiple_missed_defenses_do_not_exhaust_the_contact_budget_or_freeze_projectile_time() {
    let mut s = fixture();
    for i in 0..24 {
        let friend = s.add_tank(Team::Blue, false, VehicleKind::Balanced, i);
        s.tanks[friend].laser = 6.0;
        set_translation(&mut s, friend, 0.0, 0.0);
    }
    s.rng = Random::new(TWENTY_FIVE_MISSES);
    let mut probe = s.rng.clone();
    assert!((0..25).all(|_| probe.next() >= LASER_DEFENSE.chance));
    let p = incoming(&mut s, Weapon::Standard, |_| {});
    step_projectiles(&mut s, STEP, false);
    let shot = s.shots.iter().find(|shot| shot.id == p).unwrap();
    assert_eq!(shot.laser_checked_by.len(), 25);
    assert!((shot.z - (-6.0 + 20.0 * STEP)).abs() < 1e-7, "{}", shot.z);
    assert_eq!(s.tanks[0].hp, 100.0);
}

#[test]
fn laser_refreshes_to_twenty_seconds_pauses_expires_and_clears_on_death_respawn_and_reset() {
    let mut s = fixture();
    let mut p = supply(&mut s, PickupKind::Laser, 0.0, 0.0);
    s.tanks[0].laser = 2.0;
    s.tanks[0].cooldown = 0.4;
    assert!(collect_pickup(&mut s, 0, &mut p));
    assert_eq!(s.tanks[0].laser, 20.0);
    assert_eq!(p.cooldown, 45.0);
    assert_eq!(p.cooldown_duration, 45.0);
    assert_eq!(s.tanks[0].cooldown, 0.4);
    assert!(!collect_pickup(&mut s, 0, &mut p));
    s.match_state.phase = MatchPhase::Paused;
    for _ in 0..60 {
        s.step(VehicleCommand::idle(), false);
    }
    assert_eq!(s.tanks[0].laser, 20.0);
    s.start();
    // Fast-forward to the final tick of the 20-second timer.
    s.tanks[0].laser = STEP;
    s.step(VehicleCommand::idle(), false);
    assert_eq!(s.tanks[0].laser, 0.0);
    let mut fresh = supply(&mut s, PickupKind::Laser, 0.0, 0.0);
    collect_pickup(&mut s, 0, &mut fresh);
    let (id, team) = (s.tanks[0].id, s.tanks[0].team);
    s.damage_tank(0, 999.0, id, team, None, None);
    assert_eq!(s.tanks[0].laser, 0.0);
    s.respawn(0, None);
    assert_eq!(s.tanks[0].laser, 0.0);
    let mut fresh = supply(&mut s, PickupKind::Laser, 0.0, 0.0);
    collect_pickup(&mut s, 0, &mut fresh);
    s.reset(None);
    assert!(s.tanks.iter().all(|t| t.laser == 0.0));
    assert!(s.snapshot().tanks.iter().all(|t| t.laser == 0.0));
}

#[test]
fn one_central_rare_pickup_starts_delayed_and_refills_much_slower_than_ordinary_pickups() {
    let s = Simulation::with_seed(12.0);
    let rare: Vec<_> = s
        .pickups
        .iter()
        .filter(|p| p.kind == PickupKind::Laser)
        .collect();
    assert_eq!(rare.len(), 1);
    assert_eq!((rare[0].x, rare[0].z), (0.0, 0.0));
    assert!(!rare[0].available);
    assert_eq!(rare[0].cooldown, 25.0);
    assert_eq!(rare[0].cooldown_duration, 25.0);
    assert!(
        s.pickups
            .iter()
            .filter(|p| p.kind != PickupKind::Laser)
            .all(|p| p.available)
    );

    let mut arena = fixture();
    let mut p = supply(&mut arena, PickupKind::Laser, 0.0, 0.0);
    collect_pickup(&mut arena, 0, &mut p);
    arena.pickups = vec![p];
    set_translation(&mut arena, 0, 15.0, 0.0);
    arena.tanks[0].previous = Vec2::new(15.0, 0.0);
    assert_eq!(arena.pickups[0].cooldown, LASER_DEFENSE.respawn);
    // Fast-forward 44 of the 45 seconds, then let the pickup system count down the rest.
    arena.pickups[0].cooldown -= 44.0;
    for _ in 0..59 {
        arena.step(VehicleCommand::idle(), false);
    }
    assert!(!arena.pickups[0].available);
    for _ in 0..2 {
        arena.step(VehicleCommand::idle(), false);
    }
    assert!(arena.pickups[0].available);
}

#[test]
fn bots_seek_an_available_laser_when_useful_and_leave_it_while_theirs_is_fresh() {
    let mut s = fixture();
    s.tanks[0].human = false;
    s.tanks[0].laser = 0.0;
    let mut p = supply(&mut s, PickupKind::Laser, 0.0, 0.0);
    p.z = 4.0;
    s.pickups = vec![p];
    bot_command(&mut s, 0, STEP);
    assert_eq!(s.tanks[0].brain.pickup_target, s.pickups[0].id);
    s.tanks[0].laser = 6.0;
    decide(&mut s, 0);
    assert_eq!(s.tanks[0].brain.pickup_target, 0);
}

/// Sweep projectiles for `ticks` ticks, counting the recharge down as `Simulation::step` does.
fn sweep(s: &mut Simulation, ticks: usize) {
    for _ in 0..ticks {
        s.tanks[0].laser_recharge = 0f64.max(s.tanks[0].laser_recharge - STEP);
        step_projectiles(s, STEP, false);
    }
}

#[test]
fn a_zap_recharges_before_the_next_so_two_shells_together_cannot_both_be_stopped() {
    let mut s = fixture();
    s.rng = zapping_rng();
    let before = s.rng.clone();
    for x in [0.0, 0.6] {
        incoming(&mut s, Weapon::Standard, |shot| {
            shot.x = x;
            shot.vz = 40.0;
        });
    }
    sweep(&mut s, 30);
    assert_eq!(draws_since(&before, &s.rng), 1);
    assert_eq!(event_count(&s, SimEventType::Laser), 1);
    assert_eq!(s.tanks[0].hp, 60.0);
}

#[test]
fn a_shell_arriving_during_the_recharge_gets_its_chance_once_the_laser_is_ready() {
    let mut s = fixture();
    s.rng = zapping_rng();
    incoming(&mut s, Weapon::Standard, |_| {});
    incoming(&mut s, Weapon::Standard, |shot| shot.z = -8.0);
    sweep(&mut s, 30);
    assert!(s.shots.is_empty());
    assert_eq!(s.tanks[0].hp, 100.0);
    let beams: Vec<_> = s
        .events
        .iter()
        .filter(|e| e.kind == SimEventType::Laser)
        .collect();
    assert_eq!(beams.len(), 2);
    // The trailing shell entered range 0.05 s in but was zapped at the 0.2 s recharge.
    let expected = -8.0 + 20.0 * LASER_DEFENSE.recharge;
    assert!((beams[1].z - expected).abs() < 1e-6, "{}", beams[1].z);
}
