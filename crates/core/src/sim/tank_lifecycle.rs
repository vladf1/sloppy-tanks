//! Tank spawning and respawning. The body and contact hull are recreated for each life;
//! identity and score survive respawn.

use std::f64::consts::PI;

use rapier3d::prelude::{ColliderHandle, RigidBodyBuilder, RigidBodyHandle};

use super::ammunition::{clear_ammo, empty_ammo};
use super::arena::spawn_positions;
use super::bot_personalities::{bot_assignment, bot_profile_for, BotPersonality};
use super::data::{STEP, group, vehicle};
use super::hitboxes::tank_contact_collider;
use super::math::{Vec2, best_by};
use super::physics::{interaction_groups, vector};
use super::simulation::{GameMode, Simulation};
use super::simulation_rules::{SIMULATION_RULES, SOLO};
use super::types::{
    Brain, BotMode, Driver, SimEvent, SimEventType, Tank, Team, VehicleCommand, VehicleKind, Weapon,
};

/// Soft-CCD reach for a chassis: two ticks at 1.5x its (tuned) top speed.
pub fn soft_ccd_prediction(kind: VehicleKind, speed_scale: f64) -> f64 {
    vehicle(kind).speed * speed_scale * 1.5 * STEP * 2.0
}

fn create_tank_body(
    simulation: &mut Simulation,
    kind: VehicleKind,
    position: Vec2,
    speed_scale: f64,
) -> (RigidBodyHandle, ColliderHandle) {
    let stats = vehicle(kind);
    let body = simulation.world.insert_body(
        RigidBodyBuilder::dynamic()
            .translation(vector(position.x, SIMULATION_RULES.tank_body_height, position.z))
            .enabled_rotations(false, true, false)
            .linear_damping(SIMULATION_RULES.tank_linear_damping as f32)
            .angular_damping(SIMULATION_RULES.tank_angular_damping as f32)
            .ccd_enabled(true)
            .soft_ccd_prediction(soft_ccd_prediction(kind, speed_scale) as f32),
    );
    let collider = simulation.world.insert_collider(
        tank_contact_collider(kind)
            .mass(stats.mass as f32)
            .collision_groups(interaction_groups(group::TANK))
            .friction(0.05)
            .restitution(0.1),
        Some(body),
    );
    simulation.world.insert_collider(tank_contact_collider(kind), Some(body));
    (body, collider)
}

pub fn spawn_tank(simulation: &mut Simulation, team: Team, human: bool, kind: VehicleKind, slot: usize) -> usize {
    let spawn = if simulation.game_mode == GameMode::Solo && !human {
        let limit = simulation.active_enemy_limit;
        Vec2 {
            x: if team == Team::Blue { -SOLO.spawn_x } else { SOLO.spawn_x },
            z: -SOLO.spawn_half_span_z + ((slot % limit) as f64 * (SOLO.spawn_half_span_z * 2.0)) / (limit as f64 - 1.0),
        }
    } else {
        spawn_positions(team, simulation.map_scale())[slot % 5]
    };
    let offset = if simulation.game_mode == GameMode::Solo { 0.0 } else { (slot / 5) as f64 * 3.0 };
    // Later rows share a spawn lane, but interpolation and AI history must start
    // at their offset body positions, not at the first tank in that lane.
    let position = Vec2 {
        x: spawn.x + if team == Team::Blue { offset } else { -offset },
        z: spawn.z,
    };
    let ordinal = simulation.tanks.iter().filter(|tank| !tank.human).count();
    let assignment = bot_assignment(slot, team, ordinal);
    let kind = if human {
        kind
    } else if simulation.game_mode == GameMode::Team && assignment.personality == BotPersonality::Support {
        // Team support slots carry the fragile, fast HMMWV hunter. Solo mode keeps
        // its original six-enemy roster and does not introduce the team-only unit.
        VehicleKind::Humvee
    } else {
        bot_profile_for(assignment.personality).chassis
    };
    let speed_scale = simulation.speed_tuning.tank_speed;
    let (body, collider) = create_tank_body(simulation, kind, position, speed_scale);
    let player = simulation
        .players
        .as_ref()
        .and_then(|players| players.iter().find(|p| p.team == team && p.slot == slot).cloned());
    let names = &simulation.bot_names;
    let name = if human {
        player.as_ref().map_or_else(|| "YOU".to_string(), |player| player.name.clone())
    } else {
        let mut name = names[ordinal % names.len()].to_string();
        if ordinal >= names.len() {
            name += &format!(" {}", ordinal / names.len() + 1);
        }
        name
    };
    let id = simulation.next_id;
    simulation.next_id += 1;
    let mut tank = Tank {
        id,
        name,
        team,
        human,
        player_id: player.map(|player| player.player_id),
        driver: if human { Driver::Human } else { Driver::Bot },
        life: 0,
        kind,
        body,
        collider,
        hp: vehicle(kind).health,
        alive: true,
        respawn: 0.0,
        protection: SIMULATION_RULES.spawn_protection_seconds,
        selected_ammo: Weapon::Standard,
        ammo: empty_ammo(),
        shield: 0.0,
        shield_points: 0.0,
        rapid: 0.0,
        speed: 0.0,
        laser: 0.0,
        cooldown: 0.0,
        mine_cooldown: 0.0,
        aim: if team == Team::Blue { PI / 2.0 } else { -PI / 2.0 },
        heading: 0.0,
        previous: position,
        recoil: 0.0,
        kills: 0,
        damage_dealt: 0.0,
        life_kills: 0,
        best_life_kills: 0,
        highest_rank: 0,
        deaths: 0,
        xp: 0.0,
        last_combat: 0.0,
        command: VehicleCommand::idle(),
        brain: Brain {
            humvee: None,
            personality: assignment.personality,
            ultra_aggressive: assignment.ultra_aggressive,
            last_seen: position,
            decision: slot as f64 * 0.05,
            target: 0,
            memory: 0.0,
            reaction: 0.3,
            fire_delay: 0.0,
            aim_error: 0.0,
            path: Vec::new(),
            goal: Vec2::ZERO,
            last: position,
            stuck: 0.0,
            recovery: 0.0,
            recovery_goal: position,
            recoveries: 0,
            avoidance: Vec2::ZERO,
            avoidance_time: 0.0,
            pickup_target: 0,
            nav_version: 0,
            mode: BotMode::Advance,
        },
    };
    tank.hp = simulation.max_health(&tank);
    if simulation.is_easy_enemy(&tank) {
        tank.brain.ultra_aggressive = false;
    }
    simulation.tanks.push(tank);
    simulation.tanks.len() - 1
}

/// Bring a dead tank back with a fresh body and life, at `position` or the best-scored
/// team spawn lane.
pub fn respawn_tank(simulation: &mut Simulation, tank_index: usize, position: Option<Vec2>) {
    let tank = &simulation.tanks[tank_index];
    let kind = if tank.human && !simulation.multiplayer() {
        simulation.human_kind
    } else {
        tank.kind
    };
    let team = tank.team;
    let enemies: Vec<usize> = (0..simulation.tanks.len())
        .filter(|&i| simulation.tanks[i].alive && simulation.tanks[i].team != team)
        .collect();
    let friends: Vec<usize> = (0..simulation.tanks.len())
        .filter(|&i| i != tank_index && simulation.tanks[i].alive && simulation.tanks[i].team == team)
        .collect();
    let p = position.unwrap_or_else(|| {
        best_by(spawn_positions(team, simulation.map_scale()), |&candidate| {
            simulation.spawn_score(candidate, &enemies, &friends)
        })
        .expect("a team always has spawn lanes")
    });
    simulation.tanks[tank_index].kind = kind;
    let speed_scale = simulation.speed_tuning.tank_speed;
    let (body, collider) = create_tank_body(simulation, kind, p, speed_scale);
    let records = simulation.records(&simulation.tanks[tank_index]);
    if records {
        simulation.combat_record.life_started = simulation.elapsed;
    }
    let elapsed = simulation.elapsed;
    let tank = &mut simulation.tanks[tank_index];
    tank.body = body;
    tank.collider = collider;
    tank.xp = 0.0;
    tank.life_kills = 0;
    tank.last_combat = elapsed;
    let max = simulation.max_health(&simulation.tanks[tank_index]);
    let tank = &mut simulation.tanks[tank_index];
    tank.hp = max;
    tank.alive = true;
    tank.protection = SIMULATION_RULES.spawn_protection_seconds;
    clear_ammo(tank);
    tank.shield = 0.0;
    tank.shield_points = 0.0;
    tank.rapid = 0.0;
    tank.speed = 0.0;
    tank.laser = 0.0;
    tank.cooldown = 0.0;
    tank.mine_cooldown = 0.0;
    tank.previous = p;
    tank.command = VehicleCommand::idle();
    let brain = &mut tank.brain;
    brain.humvee = None;
    brain.path.clear();
    brain.decision = 0.0;
    brain.fire_delay = 0.0;
    brain.target = 0;
    brain.memory = 0.0;
    brain.reaction = 0.3;
    brain.last_seen = p;
    brain.last = p;
    brain.stuck = 0.0;
    brain.recovery = 0.0;
    brain.avoidance_time = 0.0;
    brain.recoveries = 0;
    brain.pickup_target = 0;
    brain.avoidance = Vec2::ZERO;
    brain.recovery_goal = p;
    let id = tank.id;
    let mut event = SimEvent::at(SimEventType::Respawn, p.x, p.z);
    event.id = Some(id);
    simulation.events.push(event);
}
