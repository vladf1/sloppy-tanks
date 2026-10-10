//! Tank spawning and respawning. The body and contact hull are recreated for each life;
//! identity and score survive respawn.

use std::f64::consts::PI;

use rapier3d::prelude::{ColliderBuilder, ColliderHandle, RigidBodyBuilder, RigidBodyHandle};

use super::ammunition::clear_ammo;
use super::arena::spawn_positions;
use super::bot_personalities::{BotPersonality, bot_assignment, bot_profile_for};
use super::data::{STEP, group, vehicle};
use super::hitboxes::tank_contact_collider;
use super::math::{Vec2, best_by};
use super::physics::{interaction_groups, vector};
use super::simulation::{GameMode, Simulation};
use super::simulation_rules::{SIMULATION_RULES, SOLO};
use super::types::{
    Brain, Driver, SimEvent, SimEventType, Tank, Team, VehicleCommand, VehicleKind,
};

/// Soft-CCD reach for a chassis: two ticks at 1.5x its (tuned) top speed.
pub fn soft_ccd_prediction(kind: VehicleKind, speed_scale: f64) -> f64 {
    vehicle(kind).speed * speed_scale * 1.5 * STEP * 2.0
}

/// A tank's body and its two colliders (the hull, which carries the mass, and the
/// model-sized contact box), shared with client prediction.
pub fn tank_body_parts(
    kind: VehicleKind,
    position: Vec2,
    speed_scale: f64,
) -> (RigidBodyBuilder, [ColliderBuilder; 2]) {
    let stats = vehicle(kind);
    let body = RigidBodyBuilder::dynamic()
        .translation(vector(
            position.x,
            SIMULATION_RULES.tank_body_height,
            position.z,
        ))
        .enabled_rotations(false, true, false)
        .linear_damping(SIMULATION_RULES.tank_linear_damping as f32)
        .angular_damping(SIMULATION_RULES.tank_angular_damping as f32)
        .ccd_enabled(true)
        .soft_ccd_prediction(soft_ccd_prediction(kind, speed_scale) as f32);
    let hull = tank_contact_collider(kind)
        .mass(stats.mass as f32)
        .collision_groups(interaction_groups(group::TANK))
        .friction(0.05)
        .restitution(0.1);
    (body, [hull, tank_contact_collider(kind)])
}

fn create_tank_body(
    simulation: &mut Simulation,
    kind: VehicleKind,
    position: Vec2,
    speed_scale: f64,
) -> (RigidBodyHandle, ColliderHandle) {
    let (body, [hull, contact]) = tank_body_parts(kind, position, speed_scale);
    let body = simulation.world.insert_body(body);
    let collider = simulation.world.insert_collider(hull, Some(body));
    simulation.world.insert_collider(contact, Some(body));
    (body, collider)
}

/// Solo enemy lane `slot`, spread along the opponent's side of the arena.
pub(crate) fn solo_spawn(team: Team, slot: usize) -> Vec2 {
    let lanes = SOLO.active_enemies;
    Vec2 {
        x: if team == Team::Blue {
            -SOLO.spawn_x
        } else {
            SOLO.spawn_x
        },
        z: -SOLO.spawn_half_span_z
            + ((slot % lanes) as f64 * (SOLO.spawn_half_span_z * 2.0)) / (lanes as f64 - 1.0),
    }
}

impl Simulation {
    pub fn add_tank(&mut self, team: Team, human: bool, kind: VehicleKind, slot: usize) -> usize {
        let spawn = if self.game_mode == GameMode::Solo && !human {
            solo_spawn(team, slot)
        } else {
            spawn_positions(team, self.map_scale())[slot % 5]
        };
        let offset = if self.game_mode == GameMode::Solo {
            0.0
        } else {
            (slot / 5) as f64 * 3.0
        };
        // Later rows share a spawn lane, but interpolation and AI history must start
        // at their offset body positions, not at the first tank in that lane.
        let position = Vec2 {
            x: spawn.x + if team == Team::Blue { offset } else { -offset },
            z: spawn.z,
        };
        let ordinal = self.tanks.iter().filter(|tank| !tank.human).count();
        let assignment = bot_assignment(slot, team, ordinal);
        let kind = if human {
            kind
        } else if self.game_mode == GameMode::Team
            && assignment.personality == BotPersonality::Support
        {
            // Team support slots carry the fragile, fast HMMWV hunter. Solo mode keeps
            // its original six-enemy roster and does not introduce the team-only unit.
            VehicleKind::Humvee
        } else {
            bot_profile_for(assignment.personality).chassis
        };
        let speed_scale = self.speed_tuning.tank_speed;
        let (body, collider) = create_tank_body(self, kind, position, speed_scale);
        let player = self.player_at(team, slot).cloned();
        let names = &self.bot_names;
        let name = if human {
            player
                .as_ref()
                .map_or_else(|| "YOU".to_string(), |player| player.name.clone())
        } else {
            let mut name = names[ordinal % names.len()].to_string();
            if ordinal >= names.len() {
                name += &format!(" {}", ordinal / names.len() + 1);
            }
            name
        };
        // Everything left out starts at zero, empty or its type's default.
        let mut tank = Tank {
            id: self.allocate_id(),
            name,
            team,
            human,
            player_id: player.map(|player| player.player_id),
            driver: if human { Driver::Human } else { Driver::Bot },
            kind,
            body,
            collider,
            alive: true,
            protection: SIMULATION_RULES.spawn_protection_seconds,
            aim: if team == Team::Blue {
                PI / 2.0
            } else {
                -PI / 2.0
            },
            previous: position,
            brain: Brain {
                personality: assignment.personality,
                ultra_aggressive: assignment.ultra_aggressive,
                last_seen: position,
                decision: slot as f64 * 0.05,
                reaction: 0.3,
                last: position,
                recovery_goal: position,
                attacked_from: position,
                ..Brain::default()
            },
            ..Tank::default()
        };
        tank.hp = self.max_health(&tank);
        if self.is_easy_enemy(&tank) {
            tank.brain.ultra_aggressive = false;
        }
        self.tanks.push(tank);
        self.tanks.len() - 1
    }

    /// Bring a dead tank back with a fresh body and life, at `position` or the best-scored
    /// team spawn lane.
    pub fn respawn(&mut self, tank_index: usize, position: Option<Vec2>) {
        let tank = &self.tanks[tank_index];
        let kind = if tank.human && !self.multiplayer() {
            self.human_kind
        } else {
            tank.kind
        };
        let team = tank.team;
        let enemies: Vec<usize> = (0..self.tanks.len())
            .filter(|&i| self.tanks[i].alive && self.tanks[i].team != team)
            .collect();
        let friends: Vec<usize> = (0..self.tanks.len())
            .filter(|&i| i != tank_index && self.tanks[i].alive && self.tanks[i].team == team)
            .collect();
        let p = position.unwrap_or_else(|| {
            best_by(spawn_positions(team, self.map_scale()), |&candidate| {
                self.spawn_score(candidate, &enemies, &friends)
            })
            .expect("a team always has spawn lanes")
        });
        self.tanks[tank_index].kind = kind;
        let speed_scale = self.speed_tuning.tank_speed;
        let (body, collider) = create_tank_body(self, kind, p, speed_scale);
        let records = self.records(&self.tanks[tank_index]);
        if records {
            self.combat_record.life_started = self.elapsed;
        }
        let elapsed = self.elapsed;
        let tank = &mut self.tanks[tank_index];
        tank.body = body;
        tank.collider = collider;
        tank.xp = 0.0;
        tank.life_kills = 0;
        tank.last_combat = elapsed;
        let max = self.max_health(&self.tanks[tank_index]);
        let tank = &mut self.tanks[tank_index];
        tank.hp = max;
        tank.alive = true;
        tank.protection = SIMULATION_RULES.spawn_protection_seconds;
        clear_ammo(tank);
        tank.shield = 0.0;
        tank.shield_points = 0.0;
        tank.rapid = 0.0;
        tank.speed = 0.0;
        tank.laser = 0.0;
        tank.laser_recharge = 0.0;
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
        brain.attacker = 0;
        brain.alarm = 0.0;
        let id = tank.id;
        let mut event = SimEvent::at(SimEventType::Respawn, p.x, p.z);
        event.id = Some(id);
        self.events.push(event);
    }
}
