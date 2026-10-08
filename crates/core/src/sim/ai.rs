//! Bot brains produce the same `VehicleCommand` a human's controls do.

use super::bot_movement::{recover_bot, route_direction, steer_bot};
use super::bot_personalities::{
    BotPersonality, bot_profile, bot_reload, combat_movement, preferred_ammo,
};
use super::bot_strategy::update_bot_goal;
use super::combat_rules::COMBAT;
use super::data::weapon;
use super::difficulty::enemy_difficulty;
use super::hitboxes::{ShotProbe, tank_hit_time};
use super::humvee_tactics::{humvee_can_fire, humvee_holding_position, steady_humvee_shot};
use super::math::{Vec2, angle_delta, distance};
use super::simulation::Simulation;
use super::types::{AmmoSelection, BotMode, CoverKind, VehicleCommand, VehicleKind, Weapon};

const BREACH_RANGE: f64 = 14.0;
const BREACH_ROUTE_ANGLE: f64 = 0.5;
const BREACH_FIRE_ANGLE: f64 = 0.15;
const MINE_RANGE: f64 = 17.0;

/// Check each firing lane against the same hulls used by projectile collision.
pub fn friendly_blocks_shot(
    simulation: &Simulation,
    tank_index: usize,
    aim: f64,
    fired: Weapon,
    range: f64,
) -> bool {
    let tank = &simulation.tanks[tank_index];
    let position = simulation.body_translation(tank.body);
    let offsets: &[f64] = if fired == Weapon::Spread {
        &[-COMBAT.spread_angle, 0.0, COMBAT.spread_angle]
    } else {
        &[0.0]
    };
    offsets.iter().any(|offset| {
        let probe = ShotProbe {
            x: position.x,
            y: None,
            z: position.z,
            vx: (aim + offset).sin(),
            vz: (aim + offset).cos(),
            ignored: Some(tank.id),
        };
        let mut nearest = range;
        let mut blocked = false;
        for candidate in &simulation.tanks {
            if let Some(hit) = tank_hit_time(simulation, &probe, candidate, nearest, 0.0, 0.0, None)
            {
                nearest = hit;
                blocked = candidate.team == tank.team;
            }
        }
        blocked
    })
}

/// Choose goals on decision ticks, then produce the same input command used by human controls.
pub fn bot_command(simulation: &mut Simulation, tank_index: usize, dt: f64) -> VehicleCommand {
    let role = tank_index / 2;
    let tank = &simulation.tanks[tank_index];
    let profile = bot_profile(tank);
    let easy = simulation.is_easy_enemy(tank);
    let aggressive = !easy && tank.brain.ultra_aggressive;
    let turn_speed = if easy {
        1.5
    } else if aggressive {
        5.2f64.max(profile.turn * 1.4)
    } else {
        profile.turn
    };
    let aim = tank.aim;
    let turn = |desired: f64| {
        aim + (-turn_speed * dt).max((turn_speed * dt).min(angle_delta(aim, desired)))
    };
    let preferred = preferred_ammo(tank);
    let kind = tank.kind;
    let position = simulation.body_translation(tank.body).planar();
    let brain = &mut simulation.tanks[tank_index].brain;
    brain.decision -= dt;
    brain.reaction -= dt;
    brain.fire_delay = 0f64.max(brain.fire_delay - dt);
    brain.memory -= dt;
    if brain.decision <= 0.0 {
        update_bot_goal(simulation, tank_index, role, easy, aggressive, preferred);
    }
    let mut firing_range = BREACH_RANGE;
    let mut breaching = false;
    let mut target_in_sight = false;
    let mut humvee_lane = false;
    let mut command = VehicleCommand::idle();
    command.ammo_selection = Some(AmmoSelection::Weapon(Weapon::Standard));
    let Vec2 {
        x: mut mx,
        z: mut mz,
    } = route_direction(simulation, tank_index);
    let brain = &simulation.tanks[tank_index].brain;
    let target = simulation
        .tanks
        .iter()
        .position(|enemy| enemy.id == brain.target && enemy.alive);
    match target {
        Some(target) if brain.memory > 0.0 => {
            let actual = simulation
                .body_translation(simulation.tanks[target].body)
                .planar();
            let seen = simulation.visible(position, actual);
            target_in_sight = seen;
            humvee_lane =
                kind == VehicleKind::Humvee && seen && humvee_can_fire(simulation, tank_index);
            if seen {
                command.ammo_selection =
                    (preferred != Weapon::Tow).then_some(AmmoSelection::Weapon(preferred));
            }
            if seen || aggressive {
                simulation.tanks[tank_index].brain.last_seen = actual;
            }
            let q = if seen || aggressive {
                actual
            } else {
                simulation.tanks[tank_index].brain.last_seen
            };
            let velocity = if seen {
                simulation
                    .body_linvel(simulation.tanks[target].body)
                    .planar()
            } else {
                Vec2::ZERO
            };
            let d = distance(position, q);
            firing_range = d;
            let lead = if easy { 0.1 } else { 0.65 };
            let shell_speed = weapon(preferred).speed * simulation.speed_tuning.bullet_speed;
            let aim_error = simulation.tanks[tank_index].brain.aim_error;
            let desired = (q.x + (velocity.x * d * lead) / shell_speed - position.x)
                .atan2(q.z + (velocity.z * d * lead) / shell_speed - position.z)
                + aim_error;
            command.aim = turn(desired);
            let brain = &simulation.tanks[tank_index].brain;
            command.fire = seen
                && (kind != VehicleKind::Humvee || humvee_can_fire(simulation, tank_index))
                && d <= profile.sight
                && brain.reaction <= 0.0
                && angle_delta(command.aim, desired).abs()
                    < if profile.stationary { 0.13 } else { 0.2 };
            if kind != VehicleKind::Humvee
                && brain.mode == BotMode::Fight
                && seen
                && brain.recovery <= 0.0
            {
                let strafe = if role.is_multiple_of(2) { -1.0 } else { 1.0 };
                let movement = combat_movement(
                    &simulation.tanks[tank_index],
                    q.x - position.x,
                    q.z - position.z,
                    strafe,
                );
                mx = movement.x;
                mz = movement.z;
            }
            command.mine = !easy
                && brain.personality == BotPersonality::Minelayer
                && d < MINE_RANGE
                && simulation.tanks[tank_index].mine_cooldown <= 0.0;
        }
        _ => command.aim = turn(mx.atan2(mz)),
    }
    // Deliberately clear nearby weak timber and towers that obstruct a useful route.
    // HMMWVs carry only a TOW: never spend an anti-tank missile breaching scenery.
    // A target in sight keeps the turret while it slews or the bot reacts; otherwise
    // cover beside the target's bearing would win every tick the shot is not yet lined up.
    if !command.fire && !target_in_sight && kind != VehicleKind::Humvee {
        let goal = simulation.tanks[tank_index].brain.goal;
        let route = (goal.x - position.x).atan2(goal.z - position.z);
        // A distance is never shorter than either axis offset; the box only skips far covers.
        let weak = simulation.covers.iter().find(|o| {
            o.alive
                && o.destructible
                && o.kind != CoverKind::Drum
                && (o.x - position.x).abs() < BREACH_RANGE
                && (o.z - position.z).abs() < BREACH_RANGE
                && distance(position, Vec2::new(o.x, o.z)) < BREACH_RANGE
                && angle_delta(route, (o.x - position.x).atan2(o.z - position.z)).abs()
                    < BREACH_ROUTE_ANGLE
        });
        if let Some(weak) = weak {
            command.ammo_selection = Some(AmmoSelection::Weapon(Weapon::Standard));
            let desired = (weak.x - position.x).atan2(weak.z - position.z);
            command.aim = turn(desired);
            command.fire = angle_delta(command.aim, desired).abs() < BREACH_FIRE_ANGLE;
            firing_range = distance(position, Vec2::new(weak.x, weak.z));
            breaching = true;
        }
    }
    let firing_weapon = if breaching {
        Weapon::Standard
    } else {
        preferred
    };
    // Personality cadence below cancels any shot while fire_delay runs, so the lanes only need
    // checking for a shot that can leave now or for a HMMWV steadying on a clear lane.
    if command.fire
        && (simulation.tanks[tank_index].brain.fire_delay <= 0.0 || humvee_lane)
        && friendly_blocks_shot(
            simulation,
            tank_index,
            command.aim,
            firing_weapon,
            firing_range,
        )
    {
        command.fire = false;
        humvee_lane = false;
    }
    if kind == VehicleKind::Humvee {
        let steady = steady_humvee_shot(simulation, tank_index, humvee_lane, dt);
        command.fire = command.fire && steady;
    }
    // Personality cadence also applies when breaching; human weapon cadence is separate.
    if simulation.tanks[tank_index].brain.fire_delay > 0.0 {
        command.fire = false;
    } else if command.fire && simulation.tanks[tank_index].cooldown == 0.0 {
        if breaching {
            simulation.bot_breach_shots += 1;
        }
        let fire_delay = if easy {
            simulation.rng.range(2.0, 3.0)
        } else {
            let jitter = simulation.rng.range(0.1, 0.25);
            bot_reload(&simulation.tanks[tank_index], jitter, Some(firing_weapon))
        };
        let reload = enemy_difficulty(simulation, &simulation.tanks[tank_index]).reload;
        simulation.tanks[tank_index].brain.fire_delay = fire_delay * reload;
    }
    if kind == VehicleKind::Humvee && humvee_holding_position(simulation, tank_index) {
        command.move_x = 0.0;
        command.move_z = 0.0;
        simulation.tanks[tank_index].brain.stuck = 0.0;
        return command;
    }
    recover_bot(simulation, tank_index, Vec2::new(mx, mz), dt);
    if simulation.tanks[tank_index].brain.recovery > 0.0 {
        Vec2 { x: mx, z: mz } = route_direction(simulation, tank_index);
    }
    Vec2 { x: mx, z: mz } = steer_bot(simulation, tank_index, Vec2::new(mx, mz), dt);
    let movement_scale = if easy {
        0.65
    } else if aggressive {
        1f64.min(profile.speed * 1.35 + 0.15)
    } else {
        profile.speed
    };
    let length = 1f64.max(mx.hypot(mz));
    command.move_x = (mx / length) * movement_scale;
    command.move_z = (mz / length) * movement_scale;
    command
}
