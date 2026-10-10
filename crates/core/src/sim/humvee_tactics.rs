//! TOW Humvee hunter tactics: fire from a safe range, then withdraw to concealment.
//! Runs on the existing decision cadence, without consuming additional combat RNG.

use std::f64::consts::PI;

use super::math::{Vec2, distance};
use super::simulation::Simulation;
use super::types::{BotMode, Team};

const RANGE: f64 = 28.0;
const MIN_RANGE: f64 = 16.0;
const MAX_HOLD_RANGE: f64 = 32.0;
const ARRIVAL: f64 = 2.0;
const REPLAN_SECONDS: f64 = 1.0;
const WITHDRAW_SECONDS: f64 = 4.5;
const RELOAD_PAUSE: f64 = 0.6;
const SEARCH_SECONDS: f64 = 5.0;
const REPOSITION_DISTANCE: f64 = 4.0;
const THREAT_MOVED_DISTANCE: f64 = 4.0;
const FIRING_POINT_ARRIVAL: f64 = 3.0;
const HIDDEN_ESCAPE_BONUS: f64 = 40.0;
const MINIMUM_OPEN_GAIN: f64 = 5.0;
pub const HUMVEE_AIM_SECONDS: f64 = 0.75;
pub const HUMVEE_DEPARTURE_SECONDS: f64 = 0.65;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumveePhase {
    Attack,
    Withdraw,
    Hide,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumveeTactics {
    pub phase: HumveePhase,
    pub escape: Vec2,
    pub firing_point: Vec2,
    pub ready_at: f64,
    pub replan_at: f64,
    pub deadline: f64,
    pub last_shot: Option<Vec2>,
    pub planned_threat: Option<Vec2>,
    pub flank: f64,
    pub aim_seconds: f64,
    pub aim_target: Option<u32>,
    pub departure_at: f64,
}

fn set_goal(simulation: &mut Simulation, tank_index: usize, goal: Vec2) {
    let brain = &simulation.tanks[tank_index].brain;
    if distance(brain.goal, goal) > 1.0 || brain.nav_version != simulation.nav.version {
        let from = simulation.tank_planar(tank_index);
        let brain = &mut simulation.tanks[tank_index].brain;
        brain.goal = goal;
        simulation.nav.find_into(from, goal, &mut brain.path);
        brain.nav_version = simulation.nav.version;
        simulation.bot_reroutes += 1;
    }
}

/// Prefer nearby concealment; in open terrain, withdraw away from the target.
fn escape_point(simulation: &Simulation, from: Vec2, threat: Vec2) -> Option<Vec2> {
    let away = (from.x - threat.x).atan2(from.z - threat.z);
    const RADII: [f64; 2] = [8.0, 14.0];
    const DIRECTIONS: usize = 12;
    let mut candidates = [(Vec2::ZERO, 0.0); RADII.len() * DIRECTIONS];
    let mut count = 0;
    for radius in RADII {
        for i in 0..DIRECTIONS {
            let angle = away + (i as f64 * PI * 2.0) / DIRECTIONS as f64;
            let point = simulation.nav.point(simulation.nav.index(Vec2::new(
                from.x + angle.sin() * radius,
                from.z + angle.cos() * radius,
            )));
            if simulation.nav.is_blocked(point) || distance(point, threat) < MIN_RANGE {
                continue;
            }
            let hidden = !simulation.visible(point, threat);
            let gain = distance(point, threat) - distance(from, threat);
            if !hidden && gain < MINIMUM_OPEN_GAIN {
                continue;
            }
            let score =
                (if hidden { HIDDEN_ESCAPE_BONUS } else { 0.0 }) + gain - distance(from, point);
            candidates[count] = (point, score);
            count += 1;
        }
    }
    let candidates = &mut candidates[..count];
    candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    // A clear escape leg cannot detour toward or through the target.
    candidates
        .iter()
        .map(|&(point, _)| point)
        .find(|&point| simulation.nav.clear_line(from, point))
}

/// Plan the Humvee's next goal. Returns true when the tactics chose the goal and route.
pub fn update_humvee_goal(simulation: &mut Simulation, tank_index: usize) -> bool {
    let position = simulation.tank_planar(tank_index);
    let elapsed = simulation.elapsed;
    let tank = &mut simulation.tanks[tank_index];
    let flank = if tank.team == Team::Blue { 1.0 } else { -1.0 };
    let threat = tank.brain.last_seen;
    let has_target = tank.brain.target != 0;
    let tactics = tank.brain.humvee.get_or_insert(HumveeTactics {
        phase: HumveePhase::Attack,
        escape: position,
        firing_point: position,
        ready_at: 0.0,
        replan_at: 0.0,
        deadline: 0.0,
        last_shot: None,
        planned_threat: None,
        flank,
        aim_seconds: 0.0,
        aim_target: None,
        departure_at: 0.0,
    });
    if tactics.phase == HumveePhase::Attack && has_target && distance(position, threat) < MIN_RANGE
    {
        let escape = escape_point(simulation, position, threat).unwrap_or(position);
        let tactics = humvee(simulation, tank_index);
        tactics.phase = HumveePhase::Withdraw;
        tactics.escape = escape;
        tactics.ready_at = elapsed + REPLAN_SECONDS;
        tactics.deadline = elapsed + WITHDRAW_SECONDS;
    }
    if humvee(simulation, tank_index).phase != HumveePhase::Attack {
        simulation.tanks[tank_index].brain.mode = BotMode::Retreat;
        let tactics = humvee(simulation, tank_index);
        let mut reached = distance(position, tactics.escape) < ARRIVAL;
        if reached {
            tactics.phase = HumveePhase::Hide;
        }
        // A moving enemy or destroyed cover can invalidate the original hiding place.
        let replan_at = tactics.replan_at;
        if elapsed >= replan_at
            && ((reached && simulation.visible(position, threat))
                || distance(position, threat) < MIN_RANGE)
        {
            let escape = escape_point(simulation, position, threat);
            let tactics = humvee(simulation, tank_index);
            if let Some(escape) = escape {
                tactics.escape = escape;
                tactics.phase = HumveePhase::Withdraw;
                reached = false;
            }
            tactics.replan_at = elapsed + REPLAN_SECONDS;
        }
        let escape = humvee(simulation, tank_index).escape;
        set_goal(simulation, tank_index, escape);
        let tank = &simulation.tanks[tank_index];
        let tactics = tank.brain.humvee.as_ref().expect("humvee tactics");
        if elapsed < tactics.ready_at
            || tank.cooldown > 0.0
            || tank.brain.fire_delay > 0.0
            || (!reached && elapsed < tactics.deadline)
        {
            return true;
        }
        let tactics = humvee(simulation, tank_index);
        tactics.phase = HumveePhase::Attack;
        tactics.planned_threat = None;
        tactics.replan_at = 0.0;
    }
    let brain = &simulation.tanks[tank_index].brain;
    let tactics = brain.humvee.as_ref().expect("humvee tactics");
    if (brain.target == 0 || brain.memory <= 0.0)
        && tactics
            .last_shot
            .is_none_or(|_| elapsed > tactics.deadline + SEARCH_SECONDS)
    {
        return false;
    }
    simulation.tanks[tank_index].brain.mode = BotMode::Fight;
    let tactics = humvee(simulation, tank_index);
    if elapsed < tactics.replan_at {
        let firing_point = tactics.firing_point;
        set_goal(simulation, tank_index, firing_point);
        return true;
    }
    tactics.replan_at = elapsed + REPLAN_SECONDS;
    let (planned_threat, firing_point, last_shot, flank) = (
        tactics.planned_threat,
        tactics.firing_point,
        tactics.last_shot,
        tactics.flank,
    );
    if planned_threat.is_some_and(|planned| distance(planned, threat) < THREAT_MOVED_DISTANCE)
        && !simulation.nav.is_blocked(firing_point)
        && simulation.visible(firing_point, threat)
    {
        set_goal(simulation, tank_index, firing_point);
        return true;
    }
    let angle = (position.x - threat.x).atan2(position.z - threat.z);
    let hold = (last_shot.is_none()
        && distance(position, threat) >= MIN_RANGE
        && distance(position, threat) <= MAX_HOLD_RANGE)
        .then_some(position);
    let candidates = hold
        .into_iter()
        .chain([0.3, 0.6, 0.9, -0.3, -0.6, 0.0].map(|offset| {
            let heading = angle + offset * flank;
            Vec2::new(
                threat.x + heading.sin() * RANGE,
                threat.z + heading.cos() * RANGE,
            )
        }));
    for candidate in candidates {
        let point = simulation.nav.point(simulation.nav.index(candidate));
        if simulation.nav.is_blocked(point)
            || last_shot.is_some_and(|shot| distance(point, shot) < REPOSITION_DISTANCE)
            || !simulation.visible(point, threat)
        {
            continue;
        }
        let Some(escape) = escape_point(simulation, point, threat) else {
            continue;
        };
        let path = simulation.nav.find(position, point);
        if (path.is_empty() && distance(position, point) > ARRIVAL)
            || path
                .iter()
                .any(|&waypoint| distance(waypoint, threat) < MIN_RANGE - 2.0)
        {
            continue;
        }
        let nav_version = simulation.nav.version;
        let brain = &mut simulation.tanks[tank_index].brain;
        let tactics = brain.humvee.as_mut().expect("humvee tactics");
        tactics.escape = escape;
        tactics.firing_point = point;
        tactics.planned_threat = Some(threat);
        brain.goal = point;
        brain.path = path;
        brain.nav_version = nav_version;
        simulation.bot_reroutes += 1;
        return true;
    }
    // No safe firing position: create distance rather than charging the enemy.
    let escape = escape_point(simulation, position, threat).unwrap_or(position);
    let tactics = humvee(simulation, tank_index);
    tactics.escape = escape;
    tactics.phase = HumveePhase::Withdraw;
    tactics.ready_at = elapsed + REPLAN_SECONDS;
    tactics.deadline = elapsed + WITHDRAW_SECONDS;
    simulation.tanks[tank_index].brain.mode = BotMode::Retreat;
    set_goal(simulation, tank_index, escape);
    true
}

fn humvee(simulation: &mut Simulation, tank_index: usize) -> &mut HumveeTactics {
    simulation.tanks[tank_index]
        .brain
        .humvee
        .as_mut()
        .expect("humvee tactics")
}

/// Commit only after an actual launch, not an attempted or ally-blocked shot.
pub fn withdraw_humvee(simulation: &mut Simulation, tank_index: usize) {
    let position = simulation.tank_planar(tank_index);
    let elapsed = simulation.elapsed;
    let tank = &mut simulation.tanks[tank_index];
    let reload = tank.cooldown.max(tank.brain.fire_delay);
    let Some(tactics) = tank.brain.humvee.as_mut() else {
        return;
    };
    tactics.last_shot = Some(position);
    tactics.flank *= -1.0;
    tactics.aim_seconds = 0.0;
    tactics.departure_at = elapsed + HUMVEE_DEPARTURE_SECONDS;
    tactics.phase = HumveePhase::Withdraw;
    tactics.ready_at = elapsed + reload + RELOAD_PAUSE;
    tactics.deadline = elapsed + WITHDRAW_SECONDS;
    tactics.replan_at = elapsed + REPLAN_SECONDS;
    let escape = tactics.escape;
    tank.brain.mode = BotMode::Retreat;
    tank.brain.recovery = 0.0;
    tank.brain.avoidance_time = 0.0;
    set_goal(simulation, tank_index, escape);
}

pub fn humvee_can_fire(simulation: &Simulation, tank_index: usize) -> bool {
    let tank = &simulation.tanks[tank_index];
    let position = simulation.tank_planar(tank_index);
    tank.brain
        .humvee
        .as_ref()
        .is_some_and(|tactics| tactics.phase == HumveePhase::Attack)
        && distance(position, tank.brain.last_seen) >= MIN_RANGE
        && distance(position, tank.brain.goal) < FIRING_POINT_ARRIVAL
}

/// A visible firing pause gives opponents time to line up a counter-shot.
pub fn steady_humvee_shot(
    simulation: &mut Simulation,
    tank_index: usize,
    can_fire: bool,
    dt: f64,
) -> bool {
    let tank = &mut simulation.tanks[tank_index];
    let (target, cooldown, fire_delay) = (tank.brain.target, tank.cooldown, tank.brain.fire_delay);
    let Some(tactics) = tank.brain.humvee.as_mut() else {
        return false;
    };
    if tactics.aim_target != Some(target) {
        tactics.aim_seconds = 0.0;
        tactics.aim_target = Some(target);
    }
    if !can_fire || cooldown > 0.0 || fire_delay > 0.0 {
        tactics.aim_seconds = 0.0;
        return false;
    }
    tactics.aim_seconds += dt;
    tactics.aim_seconds >= HUMVEE_AIM_SECONDS
}

pub fn humvee_holding_position(simulation: &Simulation, tank_index: usize) -> bool {
    simulation.tanks[tank_index]
        .brain
        .humvee
        .as_ref()
        .is_some_and(|tactics| {
            tactics.aim_seconds > 0.0 || simulation.elapsed < tactics.departure_at
        })
}
