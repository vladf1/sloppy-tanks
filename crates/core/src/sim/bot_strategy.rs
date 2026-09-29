//! Bot decisions: target, retreat/pickup/patrol goal and route. Runs only on decision ticks.

use rapier3d::parry::shape::Ball;
use rapier3d::prelude::Pose;

use super::ammunition::can_collect_ammo;
use super::bot_personalities::{BotPersonality, bot_ammo, bot_profile};
use super::data::{group, weapon};
use super::difficulty::enemy_difficulty;
use super::humvee_tactics::update_humvee_goal;
use super::math::{Vec2, best_by, distance};
use super::physics::{query_filter, vector};
use super::simulation::Simulation;
use super::types::{BotMode, PickupKind, Team, VehicleKind, Weapon};

// Decision cadence is intentionally slower than steering, which runs every simulation tick.
const DECISION_MIN_SECONDS: f64 = 0.22;
const DECISION_MAX_SECONDS: f64 = 0.42;
const TARGET_STICKINESS: f64 = 0.75;
const REPAIR_SEEK_HEALTH_FRACTION: f64 = 0.4;
const REPAIR_COLLECT_HEALTH_FRACTION: f64 = 0.8;
const EFFECT_REFRESH_SECONDS: f64 = 2.0;
const PREFERRED_AMMO_DISTANCE_BONUS: f64 = 3.0;
/// Humvee threat scores count other visible enemies within this distance of a candidate.
const HUMVEE_CROWD_RADIUS: f64 = 14.0;
const HUMVEE_CROWD_PENALTY: f64 = 12.0;

/// Choose a target, retreat/pickup/patrol goal and route; called only on decision ticks.
pub fn update_bot_goal(
    simulation: &mut Simulation,
    tank_index: usize,
    role: usize,
    easy: bool,
    aggressive: bool,
    preferred: Weapon,
) {
    let tank = &simulation.tanks[tank_index];
    let position3 = simulation.body_translation(tank.body);
    let position = position3.planar();
    let profile = bot_profile(tank);
    let previous_mode = tank.brain.mode;
    let (team, kind, previous_target) = (tank.team, tank.kind, tank.brain.target);
    simulation.tanks[tank_index].brain.decision = simulation
        .rng
        .range(DECISION_MIN_SECONDS, DECISION_MAX_SECONDS);
    // Rapier broad phase gathers local actors; team and perception rules are controller-level
    // filters. The group filter skips cover and debris.
    let sight = Ball::new(profile.sight as f32);
    let mut enemies: Vec<usize> = Vec::new();
    for (handle, _) in simulation.world.intersect_shape(
        Pose::from_translation(vector(position3.x, position3.y, position3.z)),
        &sight,
        query_filter(group::TANK_QUERY),
    ) {
        if let Some(enemy) = simulation.tanks.iter().position(|candidate| {
            candidate.alive && candidate.team != team && candidate.collider == handle
        }) {
            enemies.push(enemy);
        }
    }
    let enemy_position = |simulation: &Simulation, enemy: usize| {
        simulation
            .body_translation(simulation.tanks[enemy].body)
            .planar()
    };
    let seen = |simulation: &Simulation, enemy: usize| {
        aggressive || simulation.visible(position, enemy_position(simulation, enemy))
    };
    let closeness = |simulation: &Simulation, enemy: usize| {
        -distance(position, enemy_position(simulation, enemy))
            * if simulation.tanks[enemy].id == previous_target {
                TARGET_STICKINESS
            } else {
                1.0
            }
    };
    let target = if kind == VehicleKind::Humvee {
        // A HMMWV avoids crowds, so each score counts the other visible threats nearby.
        let threats: Vec<usize> = enemies
            .iter()
            .copied()
            .filter(|&enemy| seen(simulation, enemy))
            .collect();
        best_by(threats.iter().copied(), |&candidate| {
            let crowd = threats
                .iter()
                .filter(|&&other| {
                    other != candidate
                        && distance(
                            enemy_position(simulation, other),
                            enemy_position(simulation, candidate),
                        ) < HUMVEE_CROWD_RADIUS
                })
                .count();
            closeness(simulation, candidate) - crowd as f64 * HUMVEE_CROWD_PENALTY
        })
    } else {
        // Other scores ignore the rest of the threats, so sight lines are tested from the best
        // score down (ties in reported order, as best_by keeps them). The first visible enemy is
        // best_by's choice, without a ray to every enemy in sight range.
        let scores: Vec<f64> = enemies
            .iter()
            .map(|&enemy| closeness(simulation, enemy))
            .collect();
        let mut ranked: Vec<usize> = (0..enemies.len()).collect();
        ranked.sort_by(|&a, &b| {
            let difference = scores[b] - scores[a];
            if difference != 0.0 && !difference.is_nan() {
                difference.partial_cmp(&0.0).expect("finite difference")
            } else {
                a.cmp(&b)
            }
        });
        ranked
            .into_iter()
            .find(|&index| seen(simulation, enemies[index]))
            .map(|index| enemies[index])
    };
    let tuning = enemy_difficulty(simulation, &simulation.tanks[tank_index]);
    if let Some(target) = target {
        let target_id = simulation.tanks[target].id;
        if target_id != previous_target {
            let reaction = if easy {
                simulation.rng.range(1.0, 1.6)
            } else if aggressive {
                simulation.rng.range(0.3, 0.5)
            } else {
                simulation.rng.range(0.4, 0.8)
            };
            simulation.tanks[tank_index].brain.reaction = reaction * tuning.reaction;
        }
        let last_seen = enemy_position(simulation, target);
        let brain = &mut simulation.tanks[tank_index].brain;
        brain.target = target_id;
        brain.memory = if aggressive { 3.0 } else { 1.5 };
        brain.last_seen = last_seen;
        if kind != VehicleKind::Humvee {
            brain.goal = last_seen;
        }
        brain.mode = BotMode::Fight;
    } else if simulation.tanks[tank_index].brain.memory <= 0.0 {
        let brain = &mut simulation.tanks[tank_index].brain;
        brain.target = 0;
        brain.mode = BotMode::Advance;
    }
    let aim_error = simulation.rng.range(-profile.aim_error, profile.aim_error)
        + if easy {
            simulation.rng.range(-0.2, 0.2)
        } else {
            0.0
        };
    simulation.tanks[tank_index].brain.aim_error = aim_error * tuning.aim_error;
    if kind == VehicleKind::Humvee && update_humvee_goal(simulation, tank_index) {
        return;
    }
    let tank = &simulation.tanks[tank_index];
    let max_health = simulation.max_health(tank);
    let multiplier = simulation.ammo_crate_multiplier;
    let useful: Vec<usize> = (0..simulation.pickups.len())
        .filter(|&p| {
            let pickup = &simulation.pickups[p];
            pickup.available
                && (pickup.kind != PickupKind::Repair
                    || tank.hp < max_health * REPAIR_COLLECT_HEALTH_FRACTION)
                && (pickup.kind != PickupKind::Rapid || tank.rapid < EFFECT_REFRESH_SECONDS)
                && pickup
                    .kind
                    .special_ammo()
                    .is_none_or(|ammo| can_collect_ammo(tank, ammo, multiplier))
                && (pickup.kind != PickupKind::Speed || tank.speed < EFFECT_REFRESH_SECONDS)
                && (pickup.kind != PickupKind::Laser || tank.laser < EFFECT_REFRESH_SECONDS)
                && (pickup.kind != PickupKind::Shield
                    || tank.shield < EFFECT_REFRESH_SECONDS
                    || tank.shield_points < weapon(Weapon::Standard).damage)
        })
        .collect();
    let pickup_at = |p: usize| Vec2::new(simulation.pickups[p].x, simulation.pickups[p].z);
    let favourite = bot_ammo(tank.brain.personality);
    let nearest = useful
        .iter()
        .copied()
        .find(|&p| simulation.pickups[p].id == tank.brain.pickup_target)
        .or_else(|| {
            best_by(useful.iter().copied(), |&p| {
                -distance(position, pickup_at(p))
                    + if simulation.pickups[p].kind.weapon() == Some(favourite) {
                        PREFERRED_AMMO_DISTANCE_BONUS
                    } else {
                        0.0
                    }
            })
        });
    let hurt = tank.hp < max_health * REPAIR_SEEK_HEALTH_FRACTION;
    let repair = best_by(useful.iter().copied(), |&p| {
        if simulation.pickups[p].kind == PickupKind::Repair {
            -distance(position, pickup_at(p))
        } else {
            f64::NEG_INFINITY
        }
    });
    let nav_version = tank.brain.nav_version;
    let brain_goal = tank.brain.goal;
    let personality = tank.brain.personality;
    simulation.tanks[tank_index].brain.pickup_target = 0;
    let pickup_reach = if profile.stationary && target.is_some() {
        5.0
    } else if aggressive {
        7.0
    } else {
        12.0
    };
    if let (true, Some(repair)) = (hurt, repair) {
        let (id, goal) = (simulation.pickups[repair].id, pickup_at(repair));
        let brain = &mut simulation.tanks[tank_index].brain;
        brain.pickup_target = id;
        brain.goal = goal;
        brain.mode = BotMode::Retreat;
    } else if let Some(nearest) = nearest
        && distance(position, pickup_at(nearest)) < pickup_reach
        && (target.is_none() || preferred == Weapon::Standard)
    {
        let (id, goal) = (simulation.pickups[nearest].id, pickup_at(nearest));
        let brain = &mut simulation.tanks[tank_index].brain;
        brain.pickup_target = id;
        brain.goal = goal;
        brain.mode = BotMode::Pickup;
    } else if simulation.tanks[tank_index].brain.target == 0
        && simulation.tanks[tank_index].brain.memory <= 0.0
    {
        // Keep the chosen patrol destination until arrival instead of flipping
        // between a flank waypoint and a new random destination every decision.
        if previous_mode != BotMode::Advance
            || nav_version == 0
            || distance(position, brain_goal) < 2.0
        {
            let scale = simulation.map_scale();
            let blue = team == Team::Blue;
            let flank = Vec2::new(
                (if blue { 20.0 } else { -20.0 }) * scale,
                [-38.0, 0.0, 38.0][role % 3] * (if blue { 1.0 } else { -1.0 }) * scale,
            );
            let goal = if distance(position, flank) < 4.0 {
                let x = (if blue { 46.0 } else { -46.0 }) * scale;
                Vec2::new(x, simulation.rng.range(-44.0, 44.0) * scale)
            } else {
                flank
            };
            simulation.tanks[tank_index].brain.goal = goal;
        }
    }
    if !easy
        && target.is_none()
        && simulation.tanks[tank_index].brain.mode == BotMode::Advance
        && personality == BotPersonality::Support
        && kind != VehicleKind::Humvee
    {
        let allies = (0..simulation.tanks.len()).filter(|&i| {
            let candidate = &simulation.tanks[i];
            candidate.alive
                && candidate.team == team
                && i != tank_index
                && candidate.brain.personality != BotPersonality::Support
        });
        let closest = best_by(allies, |&i| {
            -distance(position, enemy_position(simulation, i))
        });
        if let Some(closest) = closest {
            let ally = enemy_position(simulation, closest);
            let brain = &mut simulation.tanks[tank_index].brain;
            brain.goal = Vec2::new(ally.x + if team == Team::Blue { -4.0 } else { 4.0 }, ally.z);
            brain.mode = BotMode::Escort;
        }
    }
    if easy && target.is_none() && simulation.tanks[tank_index].brain.mode == BotMode::Advance {
        let human = simulation.human();
        if human.alive {
            let goal = simulation.body_translation(human.body).planar();
            simulation.tanks[tank_index].brain.goal = goal;
        }
    }
    // A patrol/escort point can land inside cover. Finish at its
    // navigable neighbor rather than stopping short of an impossible destination.
    let goal = simulation.tanks[tank_index].brain.goal;
    if simulation.nav.is_blocked(goal) {
        let nearest = simulation
            .nav
            .point(simulation.nav.nearest(simulation.nav.index(goal)));
        simulation.tanks[tank_index].brain.goal = nearest;
    }
    let brain = &simulation.tanks[tank_index].brain;
    let route_goal = if brain.recovery > 0.0 {
        brain.recovery_goal
    } else {
        brain.goal
    };
    if brain.nav_version != simulation.nav.version
        || (brain.path.is_empty() && distance(position, route_goal) > 0.7)
        || (brain.recovery <= 0.0
            && brain
                .path
                .last()
                .is_some_and(|&last| distance(last, route_goal) > 4.0))
    {
        let path = simulation.nav.find(position, route_goal);
        let nav_version = simulation.nav.version;
        let brain = &mut simulation.tanks[tank_index].brain;
        brain.path = path;
        brain.nav_version = nav_version;
        simulation.bot_reroutes += 1;
    }
}
