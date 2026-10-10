//! Core combat and match rules: navigation reuse, damage attribution, protection, respawn,
//! drum chains, the match clock and score limit, resets, rosters, bots in opened ruins and
//! friendly-fire lanes (the former `tests/game.test.ts`).

mod support;

use sloppy_core::sim::ai::{bot_command, friendly_blocks_shot};
use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::bot_personalities::BotPersonality;
use sloppy_core::sim::data::{STEP, vehicle};
use sloppy_core::sim::match_state::{award_kill, new_match, tick_match};
use sloppy_core::sim::math::Vec2;
use sloppy_core::sim::navigation::Navigation;
use sloppy_core::sim::physics::vector;
use sloppy_core::sim::projectiles::step_projectiles;
use sloppy_core::sim::weapons::fire_weapon;
use sloppy_core::sim::{
    AmmoSelection, BotMode, CoverKind, DamageCause, MatchPhase, PickupKind, SimEventType,
    Simulation, Team, VehicleCommand, VehicleKind, Weapon,
};
use support::{damage_from, idle, set_translation, supply};

fn game() -> Simulation {
    let mut s = Simulation::with_seed(123.0);
    s.start();
    for tank in &mut s.tanks {
        tank.protection = 0.0;
    }
    s
}

/// Teleports a tank without changing its heading and steps the world once.
fn place(s: &mut Simulation, index: usize, x: f64, z: f64) -> usize {
    let tank = &mut s.tanks[index];
    let body = &mut s.world.bodies[tank.body];
    body.set_translation(vector(x, 0.65, z), true);
    body.set_linvel(vector(0.0, 0.0, 0.0), true);
    tank.previous = Vec2::new(x, z);
    s.world.step();
    index
}

/// Removes every cover but the boundary walls, then lines the tanks up out of the way.
fn clear(s: &mut Simulation) {
    for i in 0..s.covers.len() {
        if s.covers[i].kind != CoverKind::Boundary {
            s.covers[i].alive = false;
            let body = s.covers[i].body;
            s.remove_body(body);
        }
    }
    s.nav.rebuild(&s.covers, None);
    for i in 0..s.tanks.len() {
        place(s, i, -30.0 + i as f64 * 3.0, 30.0);
    }
}

fn drum(x: f64) -> CoverDef {
    CoverDef::new(CoverKind::Drum, x, 0.0, 1.0, 1.0, 2.0, 30.0, 0)
}

fn timber(x: f64, z: f64) -> CoverDef {
    CoverDef::new(CoverKind::Timber, x, z, 2.0, 1.0, 2.0, 80.0, 0)
}

#[test]
fn reused_navigation_searches_recover_from_unreachable_goals_and_changed_topology() {
    let mut nav = Navigation::new();
    let from = Vec2::new(-40.0, -40.0);
    let to = Vec2::new(40.0, 40.0);
    let saved = nav.find(from, to);
    nav.blocked.fill(1);
    assert_eq!(nav.find(from, to), Vec::<Vec2>::new());
    nav.rebuild(&[], None);
    assert_eq!(nav.find(to, from), Navigation::new().find(to, from));
    assert_eq!(nav.find(from, to), saved);
    assert_eq!(nav.find(from, from), Vec::<Vec2>::new());
}

#[test]
fn few_hit_combat_friendly_safety_exactly_once_kills_and_attribution() {
    let mut s = game();
    let (a, b) = (0, 1);
    let ally_id = s.tanks[2].id;
    s.damage_tank(a, 40.0, ally_id, Team::Blue, None, None);
    assert_eq!(s.tanks[a].hp, vehicle(s.tanks[a].kind).health);
    let a_id = s.tanks[a].id;
    s.damage_tank(b, 1000.0, a_id, Team::Blue, None, None);
    assert_eq!(s.match_state.scores[0], 1);
    s.damage_tank(b, 1000.0, a_id, Team::Blue, None, None);
    assert_eq!(s.match_state.scores[0], 1);
    assert_eq!(s.tanks[a].kills, 1);
}

#[test]
fn self_explosion_damages_owner_and_self_kill_awards_no_point() {
    let mut s = game();
    let a = 0;
    let position = s.body_translation(s.tanks[a].body).planar();
    let (id, team) = (s.tanks[a].id, s.tanks[a].team);
    s.explode(
        position,
        6.0,
        1000.0,
        id,
        team,
        None,
        DamageCause::Explosion,
    );
    assert!(!s.tanks[a].alive);
    assert_eq!(s.match_state.scores, [0, 0]);
}

#[test]
fn protection_prevents_damage_expires_after_two_seconds_and_firing_cancels_it() {
    let mut s = game();
    let a = s.human_index().unwrap();
    s.tanks[a].protection = 2.0;
    let enemy_team = s.tanks[a].team.opponent();
    s.damage_tank(a, 1000.0, 999, enemy_team, None, None);
    assert!(s.tanks[a].alive);
    fire_weapon(&mut s, a);
    assert_eq!(s.tanks[a].protection, 0.0);
    s.tanks[a].protection = 2.0;
    for _ in 0..121 {
        idle(&mut s);
    }
    assert_eq!(s.tanks[a].protection, 0.0);
}

#[test]
fn respawn_occurs_after_three_seconds_with_protection_and_selected_class() {
    let mut s = game();
    let a = s.human_index().unwrap();
    let enemy_team = s.tanks[a].team.opponent();
    s.damage_tank(a, 1000.0, 999, enemy_team, None, None);
    s.human_kind = VehicleKind::Heavy;
    for _ in 0..179 {
        idle(&mut s);
    }
    assert!(!s.tanks[a].alive);
    for _ in 0..3 {
        idle(&mut s);
    }
    assert!(s.tanks[a].alive);
    assert_eq!(s.tanks[a].hp, 140.0);
    assert!(s.tanks[a].protection > 1.9);
}

#[test]
fn drum_chain_kills_keep_the_initiating_team_and_self_kills_do_not_score() {
    let mut s = game();
    clear(&mut s);
    let a = place(&mut s, 0, -20.0, 0.0);
    let b = place(&mut s, 1, 6.0, 0.0);
    s.tanks[b].hp = 20.0;
    let d1 = s.add_cover(&drum(0.0));
    let d2 = s.add_cover(&drum(4.0));
    let (id, team) = (s.tanks[a].id, s.tanks[a].team);
    s.damage_cover(d1, 40.0, id, team, None, None);
    assert!(!s.covers[d2].alive);
    assert!(!s.tanks[b].alive);
    assert_eq!(s.tanks[a].kills, 1);
    assert_eq!(s.destroyed, 2);
}

#[test]
fn match_time_tie_overtime_next_valid_kill_and_score_limit() {
    let mut m = new_match(1);
    m.phase = MatchPhase::Playing;
    m.time = STEP;
    tick_match(&mut m, STEP);
    assert!(m.overtime);
    award_kill(&mut m, Team::Blue, Team::Blue, true, true);
    assert_eq!(m.phase, MatchPhase::Playing);
    award_kill(&mut m, Team::Blue, Team::Red, false, true);
    assert_eq!(m.winner, Some(Team::Red));

    let mut n = new_match(1);
    n.phase = MatchPhase::Playing;
    n.scores = [49, 48];
    award_kill(&mut n, Team::Red, Team::Blue, false, true);
    assert_eq!(n.phase, MatchPhase::Playing);
    assert_eq!(n.winner, None);
    n.scores = [98, 48];
    award_kill(&mut n, Team::Red, Team::Blue, false, true);
    assert_eq!(n.phase, MatchPhase::Playing);
    award_kill(&mut n, Team::Red, Team::Blue, false, true);
    assert_eq!(n.scores[0], 100);
    assert_eq!(n.winner, Some(Team::Blue));

    let mut timed = new_match(1);
    timed.phase = MatchPhase::Playing;
    timed.time = 0.1;
    timed.scores = [2, 3];
    tick_match(&mut timed, 0.2);
    assert_eq!(timed.winner, Some(Team::Red));

    let mut endless = new_match(1);
    endless.phase = MatchPhase::Playing;
    endless.scores = [99, 99];
    award_kill(&mut endless, Team::Red, Team::Blue, false, false);
    assert_eq!(endless.scores, [100, 99]);
    assert_eq!(endless.phase, MatchPhase::Playing);
    assert_eq!(endless.winner, None);
}

#[test]
fn endless_team_matches_ignore_both_the_score_limit_and_match_timer() {
    let mut s = game();
    s.endless_match = true;
    s.match_state.scores = [99, 99];
    s.match_state.time = STEP;
    let victim = s.tanks.iter().position(|t| t.team == Team::Red).unwrap();
    let killer = s.tanks.iter().position(|t| t.team == Team::Blue).unwrap();
    damage_from(&mut s, victim, 9999.0, killer, None, None);
    idle(&mut s);
    assert_eq!(s.match_state.scores, [100, 99]);
    assert_eq!(s.match_state.time, STEP);
    assert_eq!(s.match_state.phase, MatchPhase::Playing);
    assert_eq!(s.match_state.winner, None);
    assert!(!s.match_state.overtime);
}

#[test]
fn complete_reset_restores_counts_cover_pickups_scores_nav_and_rng() {
    let mut s = game();
    let counts = s.snapshot().counts;
    let covers = |s: &Simulation| {
        s.covers
            .iter()
            .map(|cover| (cover.kind, cover.alive, cover.hp))
            .collect::<Vec<_>>()
    };
    let initial_covers = covers(&s);
    let blocked = s.nav.blocked.clone();
    let rng = s.rng.state;
    let (id, team) = (s.human().id, s.human_team);
    // Collapsing towers append rubble; like the TS copy, only the original covers are hit.
    for c in 0..s.covers.len() {
        s.damage_cover(c, 999.0, id, team, None, None);
    }
    assert_ne!(covers(&s), initial_covers, "the round destroyed cover");
    assert_ne!(s.nav.blocked, blocked, "destruction reopened navigation");
    s.shots_fired = 100;
    s.match_state.scores = [20, 10];
    s.reset(None);
    assert_eq!(s.snapshot().counts, counts);
    assert_eq!(covers(&s), initial_covers);
    assert_eq!(s.nav.blocked, blocked);
    assert_eq!(
        s.rng.state, rng,
        "reset reseeds and replays construction draws"
    );
    assert_eq!(s.match_state.scores, [0, 0]);
    assert_eq!(s.destroyed, 0);
    assert!(
        s.pickups
            .iter()
            .all(|p| p.available == (p.kind != PickupKind::Laser))
    );
}

#[test]
fn bots_cross_opened_tower_footprint_and_continue_combat_through_ruins() {
    let mut s = game();
    let towers: Vec<usize> = (0..s.covers.len())
        .filter(|&c| s.covers[c].kind == CoverKind::Tower)
        .collect();
    let (id, team) = (s.human().id, s.human_team);
    for &c in &towers {
        s.damage_cover(c, 999.0, id, team, None, None);
    }
    let tower_footprints: Vec<(f64, f64)> = towers
        .iter()
        .map(|&c| (s.covers[c].x, s.covers[c].z))
        .collect();
    // Place a scout at the entrance with a useful pickup beyond the shortcut.
    // This exercises steering through the opening without relying on a random patrol.
    let (tower_x, tower_z) = tower_footprints[0];
    let scout_index = s
        .tanks
        .iter()
        .position(|t| !t.human && t.kind == VehicleKind::Scout)
        .unwrap();
    let scout = place(&mut s, scout_index, tower_x, tower_z - 5.0);
    s.tanks[scout].brain.personality = BotPersonality::Scout;
    let pickup = supply(&mut s, PickupKind::Rapid, tower_x, tower_z + 5.0);
    s.pickups.push(pickup);
    let mut crossed = false;
    for _ in 0..60 * 45 {
        s.step(VehicleCommand::idle(), true);
        for tank in s.tanks.iter().filter(|t| t.alive) {
            let p = s.body_translation(tank.body);
            if tower_footprints
                .iter()
                .any(|&(x, z)| (p.x - x).abs() < 1.2 && (p.z - z).abs() < 2.0)
            {
                crossed = true;
            }
        }
    }
    assert!(crossed, "a bot traverses an opened shortcut");
    assert!(
        s.match_state.scores[0] > 0 && s.match_state.scores[1] > 0,
        "{:?}",
        s.match_state
    );
    assert!(s.bot_reroutes > 0);
}

fn projectile_stops_at_teammate(weapon: Weapon) {
    let mut s = game();
    clear(&mut s);
    let shooter = place(&mut s, 0, 0.0, -12.0);
    let ally = place(&mut s, 2, 0.0, 0.0);
    let enemy = place(&mut s, 1, 0.0, 12.0);
    s.tanks[ally].shield = 20.0;
    s.tanks[ally].shield_points = 120.0;
    let hp = s.tanks[ally].hp;
    s.tanks[shooter].aim = 0.0;
    s.tanks[shooter].selected_ammo = weapon;
    if let Some(special) = weapon.special() {
        *s.tanks[shooter].ammo.get_mut(special) = 1.0;
    }
    fire_weapon(&mut s, shooter);
    for shot in &mut s.shots {
        shot.x = 0.0;
        shot.z = -5.0;
        shot.vx = 0.0;
        shot.vz = 600.0;
    }
    step_projectiles(&mut s, STEP, false);
    assert_eq!(s.shots.len(), 0);
    assert_eq!(s.tanks[ally].hp, hp);
    assert_eq!(s.tanks[ally].shield_points, 120.0);
    assert_eq!(s.tanks[enemy].hp, vehicle(s.tanks[enemy].kind).health);
    let ally_id = s.tanks[ally].id;
    assert!(
        !s.events
            .iter()
            .any(|e| e.kind == SimEventType::Hurt && e.id == Some(ally_id))
    );
    assert!(
        s.events
            .iter()
            .any(|e| e.kind == SimEventType::Impact && e.color == Some(0xb9d7e5))
    );
    assert_eq!(
        s.events.iter().any(|e| e.kind == SimEventType::Explosion),
        weapon == Weapon::Rocket
    );
}

#[test]
fn standard_stops_at_teammates_without_draining_hull_or_shields() {
    projectile_stops_at_teammate(Weapon::Standard);
}

#[test]
fn spread_stops_at_teammates_without_draining_hull_or_shields() {
    projectile_stops_at_teammate(Weapon::Spread);
}

#[test]
fn ricochet_stops_at_teammates_without_draining_hull_or_shields() {
    projectile_stops_at_teammate(Weapon::Ricochet);
}

#[test]
fn piercing_stops_at_teammates_without_draining_hull_or_shields() {
    projectile_stops_at_teammate(Weapon::Piercing);
}

#[test]
fn rocket_stops_at_teammates_without_draining_hull_or_shields() {
    projectile_stops_at_teammate(Weapon::Rocket);
}

#[test]
fn bots_hold_fire_for_allies_and_resume_when_their_firing_lane_clears() {
    let mut s = game();
    clear(&mut s);
    let bot = place(&mut s, 2, 0.0, -12.0);
    let ally = place(&mut s, 0, 0.0, -5.0);
    let enemy = place(&mut s, 1, 0.0, 8.0);
    s.tanks[bot].aim = 0.0;
    let enemy_id = s.tanks[enemy].id;
    let brain = &mut s.tanks[bot].brain;
    brain.target = enemy_id;
    brain.memory = 10.0;
    brain.decision = 10.0;
    brain.reaction = 0.0;
    brain.fire_delay = 0.0;
    brain.aim_error = 0.0;
    brain.mode = BotMode::Fight;
    assert!(!bot_command(&mut s, bot, STEP).fire);
    assert_eq!(s.tanks[bot].brain.fire_delay, 0.0);
    set_translation(&mut s, ally, 12.0, -5.0);
    assert!(bot_command(&mut s, bot, STEP).fire);
    set_translation(&mut s, ally, 0.0, 16.0);
    assert!(!friendly_blocks_shot(&s, bot, 0.0, Weapon::Standard, 35.0));
}

#[test]
fn bot_spread_checks_side_pellets_and_ignores_dead_allies() {
    let mut s = game();
    clear(&mut s);
    let bot = place(&mut s, 2, 0.0, -20.0);
    let ally = place(
        &mut s,
        0,
        0.19f64.sin() * 30.0,
        -20.0 + 0.19f64.cos() * 30.0,
    );
    assert!(!friendly_blocks_shot(&s, bot, 0.0, Weapon::Standard, 35.0));
    assert!(friendly_blocks_shot(&s, bot, 0.0, Weapon::Spread, 35.0));
    s.tanks[ally].alive = false;
    assert!(!friendly_blocks_shot(&s, bot, 0.0, Weapon::Spread, 35.0));
}

#[test]
fn a_bot_keeps_its_turret_on_a_visible_target_instead_of_breaching_cover_beside_it() {
    let mut s = game();
    clear(&mut s);
    let bot = place(&mut s, 2, 0.0, 0.0);
    let enemy = place(&mut s, 1, 0.0, 15.0);
    s.tanks[bot].aim = 0.0;
    // Timber about 0.34 rad off the enemy's bearing: inside the breach cone, outside the
    // fire cone, so a bot that is still reacting used to swing to it and never come back.
    s.add_cover(&timber(3.0, 8.5));
    let enemy_id = s.tanks[enemy].id;
    let brain = &mut s.tanks[bot].brain;
    brain.personality = BotPersonality::Guard;
    brain.target = enemy_id;
    brain.memory = 10.0;
    brain.decision = 10.0;
    brain.reaction = 0.6;
    brain.fire_delay = 0.0;
    brain.aim_error = 0.0;
    brain.mode = BotMode::Fight;
    brain.goal = Vec2::new(0.0, 15.0);
    let mut fired_at_enemy = false;
    for _ in 0..60 {
        let command = bot_command(&mut s, bot, STEP);
        s.tanks[bot].aim = command.aim;
        assert!(
            command.aim.abs() < 0.1,
            "the turret left the visible enemy for cover at aim {}",
            command.aim
        );
        fired_at_enemy |= command.fire;
    }
    assert!(fired_at_enemy, "the bot fires once its reaction passes");
}

#[test]
fn breaching_checks_the_standard_shell_lane_even_when_spread_ammo_is_preferred() {
    let mut s = game();
    clear(&mut s);
    let bot = place(&mut s, 2, 0.0, 0.0);
    place(&mut s, 0, 2.6, 10.0);
    place(&mut s, 1, 40.0, 40.0);
    s.tanks[bot].aim = 0.0;
    s.tanks[bot].ammo.spread = 5.0;
    let brain = &mut s.tanks[bot].brain;
    brain.personality = BotPersonality::Scout;
    brain.target = 0;
    brain.memory = 0.0;
    brain.decision = 10.0;
    brain.fire_delay = 0.0;
    brain.goal = Vec2::new(0.0, 13.0);
    s.add_cover(&timber(0.0, 13.0));
    assert!(!friendly_blocks_shot(&s, bot, 0.0, Weapon::Standard, 13.0));
    assert!(friendly_blocks_shot(&s, bot, 0.0, Weapon::Spread, 13.0));
    let command = bot_command(&mut s, bot, STEP);
    assert_eq!(
        command.ammo_selection,
        Some(AmmoSelection::Weapon(Weapon::Standard))
    );
    assert!(command.fire);
}
