//! Bot steering, route following, stuck recovery, target and goal persistence, and the
//! playtest speed sliders (the former `tests/bot-movement.test.ts`).

mod support;

use sloppy_core::sim::ai::bot_command;
use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::bot_movement::route_direction;
use sloppy_core::sim::bot_personalities::BotPersonality;
use sloppy_core::sim::data::{STEP, vehicle, weapon};
use sloppy_core::sim::math::{Vec2, angle_delta, distance};
use sloppy_core::sim::physics::vector;
use sloppy_core::sim::speed_tuning::{SpeedSetting, tune_speed};
use sloppy_core::sim::weapons::fire_weapon;
use sloppy_core::sim::{
    BotMode, CoverKind, DamageCause, DamageSource, Pickup, PickupKind, Simulation, Team,
    VehicleCommand, VehicleKind, Weapon,
};
use support::clear_arena;

fn arena() -> Simulation {
    let mut s = Simulation::with_seed(123.0);
    clear_arena(&mut s, &[]);
    s
}

/// Moves a tank without touching its rotation or velocity, and protects it for the test.
fn place(s: &mut Simulation, index: usize, x: f64, z: f64) {
    let tank = &mut s.tanks[index];
    s.world.bodies[tank.body].set_translation(vector(x, 0.65, z), true);
    tank.previous = Vec2::new(x, z);
    tank.brain.last = Vec2::new(x, z);
    tank.protection = 1000.0;
}

/// Plays `seconds` of idle-human ticks and counts reversals of the watched bot's command.
fn run(s: &mut Simulation, index: usize, seconds: usize) -> usize {
    s.world.step();
    s.start();
    let mut reversals = 0;
    let mut last = Vec2::ZERO;
    for _ in 0..seconds * 60 {
        s.step(VehicleCommand::idle(), false);
        let c = s.tanks[index].command;
        let d = c.move_x.hypot(c.move_z);
        if d > 0.15 {
            let next = Vec2::new(c.move_x / d, c.move_z / d);
            if next.x * last.x + next.z * last.z < -0.5 {
                reversals += 1;
            }
            last = next;
        }
        s.events.clear();
    }
    reversals
}

fn position(s: &Simulation, index: usize) -> Vec2 {
    s.body_translation(s.tanks[index].body).planar()
}

fn concrete(x: f64, z: f64, w: f64, d: f64) -> CoverDef {
    CoverDef::new(CoverKind::Concrete, x, z, w, d, 3.0, f64::INFINITY, 0)
}

fn pickup(id: u32, kind: PickupKind, x: f64, z: f64) -> Pickup {
    Pickup {
        id,
        kind,
        x,
        z,
        available: true,
        cooldown: 0.0,
        cooldown_duration: 0.0,
    }
}

#[test]
fn bots_brake_and_settle_near_a_destination_without_repeated_direction_flips_including_double_speed()
 {
    for personality in [
        BotPersonality::Scout,
        BotPersonality::Guard,
        BotPersonality::Heavy,
    ] {
        for speed in [1.0, 2.0] {
            let mut s = arena();
            let slot = if personality == BotPersonality::Heavy {
                3
            } else {
                1
            };
            let bot = s.add_tank(Team::Blue, false, VehicleKind::Balanced, slot);
            s.tanks[bot].brain.personality = personality;
            place(&mut s, bot, 0.0, 0.0);
            s.tanks[bot].brain.decision = 999.0;
            s.tanks[bot].brain.goal = Vec2::new(0.0, 0.7);
            tune_speed(&mut s, SpeedSetting::TankSpeed, speed);
            let reversals = run(&mut s, bot, 4);
            let label = format!("{personality:?} at {speed}x");
            assert!(reversals <= 1, "{label}: {reversals} reversals");
            let settled = distance(position(&s, bot), s.tanks[bot].brain.goal);
            assert!(settled < 0.3, "{label}: {settled} from the goal");
            let command = s.tanks[bot].command;
            assert!(
                command.move_x.hypot(command.move_z) < 0.01,
                "{label}: still driving"
            );
        }
    }
}

#[test]
fn a_retreating_guard_skirts_a_wall_instead_of_alternating_attack_and_retreat() {
    let mut s = arena();
    let bot = s.add_tank(Team::Blue, false, VehicleKind::Balanced, 1);
    let enemy = s.add_tank(Team::Red, true, VehicleKind::Balanced, 0);
    place(&mut s, bot, 0.0, -8.0);
    place(&mut s, enemy, 0.0, 0.0);
    s.add_cover(&CoverDef::new(
        CoverKind::Concrete,
        0.0,
        -12.0,
        16.0,
        2.0,
        3.0,
        f64::INFINITY,
        0,
    ));
    s.nav.rebuild(&s.covers, None);
    assert!(run(&mut s, bot, 4) < 5);
    assert!(
        position(&s, bot).x.abs() > 8.0,
        "escape past the edge of the wall"
    );
}

#[test]
fn head_on_allies_pass_each_other_and_both_reach_their_destinations() {
    let mut s = arena();
    let a = s.add_tank(Team::Blue, false, VehicleKind::Balanced, 1);
    let b = s.add_tank(Team::Blue, false, VehicleKind::Balanced, 1);
    place(&mut s, a, 0.0, -6.0);
    place(&mut s, b, 0.0, 6.0);
    for (t, z) in [(a, 16.0), (b, -16.0)] {
        s.tanks[t].brain.decision = 999.0;
        s.tanks[t].brain.goal = Vec2::new(0.0, z);
        let (from, goal) = (s.tanks[t].previous, s.tanks[t].brain.goal);
        s.tanks[t].brain.path = s.nav.find(from, goal);
    }
    assert!(run(&mut s, a, 10) < 5);
    assert!(distance(position(&s, a), s.tanks[a].brain.goal) < 0.5);
    assert!(distance(position(&s, b), s.tanks[b].brain.goal) < 0.5);
}

#[test]
fn a_stalled_bot_commits_to_its_recovery_route_across_combat_decisions_and_clears_it_on_respawn() {
    let mut s = arena();
    let bot = s.add_tank(Team::Blue, false, VehicleKind::Balanced, 1);
    let enemy = s.add_tank(Team::Red, true, VehicleKind::Balanced, 0);
    place(&mut s, bot, 0.0, 0.0);
    place(&mut s, enemy, 0.0, 10.0);
    s.world.step();
    s.start();
    s.tanks[bot].brain.stuck = 1.3;
    s.tanks[bot].brain.decision = 0.0;
    bot_command(&mut s, bot, STEP);
    assert_eq!(s.tanks[bot].brain.recoveries, 1);
    assert!(s.tanks[bot].brain.recovery > 1.0);
    let goal = s.tanks[bot].brain.recovery_goal;
    for _ in 0..5 {
        s.tanks[bot].brain.decision = 0.0;
        s.step(VehicleCommand::idle(), false);
        let brain = &s.tanks[bot].brain;
        assert_eq!(brain.recovery_goal, goal);
        assert_eq!(brain.recoveries, 1);
        assert!(distance(*brain.path.last().expect("recovery path"), goal) < 1.2);
    }
    assert!(s.snapshot().tanks[0].recovering);
    s.tanks[bot].protection = 0.0;
    let (enemy_id, enemy_team) = (s.tanks[enemy].id, s.tanks[enemy].team);
    s.damage_tank(bot, 999.0, enemy_id, enemy_team, None, None);
    s.respawn(bot, None);
    let brain = &s.tanks[bot].brain;
    assert_eq!(brain.recovery, 0.0);
    assert_eq!(brain.avoidance_time, 0.0);
    assert_eq!(brain.stuck, 0.0);
    assert_eq!(brain.recoveries, 0);
}

#[test]
fn bots_retain_comparable_visible_targets_but_react_to_a_substantially_closer_enemy() {
    let mut s = arena();
    let bot = s.add_tank(Team::Blue, false, VehicleKind::Balanced, 1);
    let a = s.add_tank(Team::Red, true, VehicleKind::Balanced, 0);
    let b = s.add_tank(Team::Red, true, VehicleKind::Balanced, 0);
    place(&mut s, bot, 0.0, 0.0);
    place(&mut s, a, 2.0, 18.0);
    place(&mut s, b, -2.0, 19.0);
    s.world.step();
    s.tanks[bot].brain.decision = 0.0;
    bot_command(&mut s, bot, STEP);
    assert_eq!(s.tanks[bot].brain.target, s.tanks[a].id);
    place(&mut s, b, -2.0, 17.0);
    s.world.step();
    s.tanks[bot].brain.decision = 0.0;
    bot_command(&mut s, bot, STEP);
    assert_eq!(s.tanks[bot].brain.target, s.tanks[a].id);
    place(&mut s, b, -2.0, 9.0);
    s.world.step();
    s.tanks[bot].brain.decision = 0.0;
    bot_command(&mut s, bot, STEP);
    assert_eq!(s.tanks[bot].brain.target, s.tanks[b].id);
}

#[test]
fn bots_skip_a_closer_enemy_behind_cover_for_the_nearest_one_in_sight() {
    let mut s = arena();
    let bot = s.add_tank(Team::Blue, false, VehicleKind::Balanced, 1);
    let hidden = s.add_tank(Team::Red, true, VehicleKind::Balanced, 0);
    let farther = s.add_tank(Team::Red, true, VehicleKind::Balanced, 0);
    let farthest = s.add_tank(Team::Red, true, VehicleKind::Balanced, 0);
    place(&mut s, bot, 0.0, 0.0);
    place(&mut s, hidden, 0.0, 10.0);
    place(&mut s, farther, 12.0, 12.0);
    place(&mut s, farthest, -14.0, 14.0);
    s.add_cover(&concrete(0.0, 6.0, 3.0, 1.0));
    s.world.step();
    s.tanks[bot].brain.decision = 0.0;
    bot_command(&mut s, bot, STEP);
    assert_eq!(s.tanks[bot].brain.target, s.tanks[farther].id);
    // Hunters track through cover, so the closest enemy wins without a sight line.
    s.tanks[bot].brain.ultra_aggressive = true;
    s.tanks[bot].brain.decision = 0.0;
    bot_command(&mut s, bot, STEP);
    assert_eq!(s.tanks[bot].brain.target, s.tanks[hidden].id);
}

/// Damage `victim` as if `shooter` (on team Red) hit it with `cause`.
fn hit(s: &mut Simulation, victim: usize, shooter: usize, cause: DamageCause) {
    s.tanks[victim].protection = 0.0;
    let owner = s.tanks[shooter].id;
    let source = DamageSource {
        cause,
        origin: tank_position(s, victim),
    };
    s.damage_tank(victim, 1.0, owner, Team::Red, None, Some(source));
}

fn tank_position(s: &Simulation, index: usize) -> Vec2 {
    s.body_translation(s.tanks[index].body).planar()
}

#[test]
fn bots_turn_on_a_farther_enemy_whose_shell_hits_them_but_not_on_a_mine_layer() {
    let mut s = arena();
    let bot = s.add_tank(Team::Blue, false, VehicleKind::Balanced, 1);
    let near = s.add_tank(Team::Red, true, VehicleKind::Balanced, 0);
    let far = s.add_tank(Team::Red, true, VehicleKind::Balanced, 0);
    place(&mut s, bot, 0.0, 0.0);
    place(&mut s, near, 2.0, 10.0);
    place(&mut s, far, -2.0, 18.0);
    s.world.step();
    s.tanks[bot].brain.decision = 0.0;
    bot_command(&mut s, bot, STEP);
    assert_eq!(s.tanks[bot].brain.target, s.tanks[near].id);
    // A mine names no shooter the bot could have seen.
    hit(&mut s, bot, far, DamageCause::Mine);
    s.tanks[bot].brain.decision = 0.0;
    bot_command(&mut s, bot, STEP);
    assert_eq!(s.tanks[bot].brain.target, s.tanks[near].id);
    // A shell makes the bot reconsider on its next tick, without waiting for a decision.
    hit(&mut s, bot, far, DamageCause::Standard);
    assert_eq!(s.tanks[bot].brain.decision, 0.0);
    bot_command(&mut s, bot, STEP);
    assert_eq!(s.tanks[bot].brain.target, s.tanks[far].id);
}

#[test]
fn a_bot_shot_from_out_of_sight_turns_toward_the_shooter_and_gives_up_when_the_alarm_ends() {
    let mut s = arena();
    let bot = s.add_tank(Team::Blue, false, VehicleKind::Balanced, 1);
    let shooter = s.add_tank(Team::Red, true, VehicleKind::Balanced, 0);
    place(&mut s, bot, 0.0, 0.0);
    place(&mut s, shooter, 0.0, 20.0);
    s.add_cover(&concrete(0.0, 8.0, 8.0, 1.0));
    s.world.step();
    s.tanks[bot].aim = std::f64::consts::PI;
    s.tanks[bot].brain.decision = 0.0;
    bot_command(&mut s, bot, STEP);
    assert_eq!(s.tanks[bot].brain.target, 0);

    hit(&mut s, bot, shooter, DamageCause::Rocket);
    let shooter_at = tank_position(&s, shooter);
    let mut fired = false;
    for _ in 0..60 {
        let command = bot_command(&mut s, bot, STEP);
        s.tanks[bot].aim = command.aim;
        fired |= command.fire;
    }
    let brain = &s.tanks[bot].brain;
    assert_eq!(brain.target, s.tanks[shooter].id);
    assert_eq!(brain.mode, BotMode::Fight);
    assert!(
        distance(brain.goal, shooter_at) < 1.0,
        "the bot heads for the shooter"
    );
    assert!(
        // Within the bot's deliberate aim error.
        angle_delta(s.tanks[bot].aim, 0.0).abs() < 0.3,
        "the turret turned toward the shooter, aim {}",
        s.tanks[bot].aim
    );
    assert!(!fired, "the bot never fires at a shooter it cannot see");

    for _ in 0..5 * 60 {
        bot_command(&mut s, bot, STEP);
    }
    assert_eq!(s.tanks[bot].brain.target, 0);
    assert_eq!(s.tanks[bot].brain.mode, BotMode::Advance);

    hit(&mut s, bot, shooter, DamageCause::Standard);
    s.respawn(bot, None);
    assert_eq!(s.tanks[bot].brain.attacker, 0);
    assert_eq!(s.tanks[bot].brain.alarm, 0.0);
}

#[test]
fn pickup_and_patrol_destinations_persist_across_decisions_and_unavailable_crates_are_abandoned() {
    let mut s = arena();
    let bot = s.add_tank(Team::Blue, false, VehicleKind::Balanced, 1);
    place(&mut s, bot, 20.0, -38.0);
    s.world.step();
    s.tanks[bot].brain.goal = Vec2::new(20.0, -38.0);
    s.tanks[bot].brain.decision = 0.0;
    bot_command(&mut s, bot, STEP);
    let patrol = s.tanks[bot].brain.goal;
    assert_eq!(patrol.x, 46.0);
    for _ in 0..5 {
        s.tanks[bot].brain.decision = 0.0;
        bot_command(&mut s, bot, STEP);
        assert_eq!(s.tanks[bot].brain.goal, patrol);
    }
    s.pickups = vec![
        pickup(999, PickupKind::Rocket, 24.0, -38.0),
        pickup(1000, PickupKind::Piercing, 15.0, -38.0),
    ];
    s.tanks[bot].brain.decision = 0.0;
    bot_command(&mut s, bot, STEP);
    assert_eq!(s.tanks[bot].brain.pickup_target, 999);
    s.pickups[1].x = 17.0;
    s.tanks[bot].brain.decision = 0.0;
    bot_command(&mut s, bot, STEP);
    assert_eq!(s.tanks[bot].brain.pickup_target, 999);
    s.pickups[0].available = false;
    s.tanks[bot].brain.decision = 0.0;
    bot_command(&mut s, bot, STEP);
    assert_eq!(s.tanks[bot].brain.pickup_target, 1000);
}

#[test]
fn path_lookahead_cannot_shortcut_a_wall_and_unreachable_goals_never_become_direct_movement() {
    let mut s = arena();
    let bot = s.add_tank(Team::Blue, false, VehicleKind::Balanced, 1);
    place(&mut s, bot, 0.0, -5.0);
    s.add_cover(&concrete(0.0, 0.0, 10.0, 2.0));
    s.nav.rebuild(&s.covers, None);
    s.world.step();
    let goal = Vec2::new(0.0, 5.0);
    assert!(!s.nav.clear_line(s.tanks[bot].previous, goal));
    s.tanks[bot].brain.goal = goal;
    s.tanks[bot].brain.path = Vec::new();
    assert_eq!(route_direction(&mut s, bot), Vec2::ZERO);
    let from = s.tanks[bot].previous;
    s.tanks[bot].brain.path = s.nav.find(from, goal);
    let direction = route_direction(&mut s, bot);
    assert!(
        direction.x.abs() > 0.8,
        "route around the wall rather than through it"
    );
}

#[test]
fn speed_sliders_scale_from_defaults_without_compounding_and_update_active_shells_and_collision_prediction()
 {
    let mut s = arena();
    let t = s.add_tank(Team::Blue, true, VehicleKind::Balanced, 0);
    place(&mut s, t, 0.0, 0.0);
    s.tanks[t].aim = 0.0;
    let tank_base = vehicle(VehicleKind::Balanced).speed;
    let shell_base = weapon(Weapon::Standard).speed;
    fire_weapon(&mut s, t);
    tune_speed(&mut s, SpeedSetting::TankSpeed, 1.5);
    tune_speed(&mut s, SpeedSetting::TankSpeed, 1.5);
    assert_eq!(vehicle(VehicleKind::Balanced).speed, tank_base);
    assert_eq!(s.speed_tuning.tank_speed, 1.5);
    let prediction = s.world.bodies[s.tanks[t].body].soft_ccd_prediction() as f64;
    assert!((prediction - tank_base * 1.5 * 1.5 * STEP * 2.0).abs() < 1e-6);
    tune_speed(&mut s, SpeedSetting::BulletSpeed, 0.5);
    tune_speed(&mut s, SpeedSetting::BulletSpeed, 0.5);
    assert_eq!(weapon(Weapon::Standard).speed, shell_base);
    assert_eq!(s.speed_tuning.bullet_speed, 0.5);
    assert_eq!(s.shots[0].vz, shell_base * 0.5);
    assert_eq!(tune_speed(&mut s, SpeedSetting::TankSpeed, f64::NAN), 1.0);
}
