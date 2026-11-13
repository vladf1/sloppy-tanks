//! Per-tick bot steering: waypoint following, obstacle avoidance and stuck recovery.

use std::f64::consts::PI;

use rapier3d::parry::bounding_volume::{Aabb, BoundingVolume};
use rapier3d::parry::query::{Ray, RayCast, ShapeCastOptions};
use rapier3d::prelude::{ColliderHandle, Pose, QueryFilter, Vector};

use super::data::group;
use super::math::{Quat4, Vec2, angle_delta, distance};
use super::physics::{interaction_groups, to_rotation, vector};
use super::simulation::Simulation;
use super::types::VehicleKind;

// World-space clearance and timing, separate from personality-level combat preferences.
const WAYPOINT_RADIUS: f64 = 0.55;
const LOOKAHEAD_WAYPOINTS: usize = 8;
const ARRIVAL_RADIUS: f64 = 0.25;
const BRAKING_DISTANCE: f64 = 1.5;

const HULL_MARGIN: f64 = 0.08;
/// Widens the boxes that rule out steering obstacles well past any rounding in a cast's
/// time of impact, so an obstacle is skipped only when it cannot stop the cast.
const SWEEP_SLACK: f64 = 0.02;
const STEERING_DEADZONE: f64 = 0.01;
const LOOKAHEAD_DISTANCE: f64 = 1.5;
const CLEAR_FRACTION: f64 = 0.9;
const CLEARANCE_WEIGHT: f64 = 4.0;
const CONTINUITY_WEIGHT: f64 = 0.35;
const MINIMUM_CLEARANCE: f64 = 0.2;
const COMMITMENT_SECONDS: f64 = 0.55;
const HUMVEE_LOOKAHEAD: f64 = 2.4;

const PROGRESS_DISTANCE: f64 = 0.8;
const MOVEMENT_DEADZONE: f64 = 0.1;
const STUCK_SECONDS: f64 = 1.2;
const DETOUR_DISTANCE: f64 = 6.0;
const RECOVERY_COMMITMENT_SECONDS: f64 = 1.8;

/// Follow a few visible waypoints ahead, then brake as the destination approaches.
pub fn route_direction(simulation: &mut Simulation, tank_index: usize) -> Vec2 {
    let position = simulation.tank_planar(tank_index);
    let nav = &simulation.nav;
    let brain = &mut simulation.tanks[tank_index].brain;
    let goal = if brain.recovery > 0.0 {
        brain.recovery_goal
    } else {
        brain.goal
    };
    let arrived = brain
        .path
        .iter()
        .take_while(|&&waypoint| distance(position, waypoint) < WAYPOINT_RADIUS)
        .count();
    brain.path.drain(..arrived);
    let mut skip = 0;
    for (i, &waypoint) in brain.path.iter().enumerate().take(LOOKAHEAD_WAYPOINTS).skip(1) {
        if !nav.clear_line(position, waypoint) {
            break;
        }
        skip = i;
    }
    if skip > 0 {
        brain.path.drain(..skip);
    }
    // An unreachable goal is not permission to drive straight through its obstruction.
    let waypoint = match brain.path.first() {
        Some(&waypoint) => waypoint,
        None if nav.clear_line(position, goal) => goal,
        None => position,
    };
    let dx = waypoint.x - position.x;
    let dz = waypoint.z - position.z;
    let d = dx.hypot(dz);
    if d < ARRIVAL_RADIUS {
        return Vec2::ZERO;
    }
    let speed = if brain.path.len() > 1 {
        1.0
    } else {
        1f64.min(d / BRAKING_DISTANCE)
    };
    Vec2::new((dx / d) * speed, (dz / d) * speed)
}

/// Storage for steering casts, reused from cast to cast.
#[derive(Default)]
pub(crate) struct SteeringScratch {
    /// The colliders a cast's sweep might meet, with their boxes.
    obstacles: Vec<(ColliderHandle, Aabb)>,
    /// The obstacles one hull orientation's sweep reaches, with when it reaches their box.
    reached: Vec<(f32, usize)>,
}

/// Check the visible hull, including other tanks, before choosing a steering direction.
/// Callers only compare the result with `needed`, so casting stops once it falls short.
///
/// Matches casting the hull through the whole world's query pipeline, more cheaply: one
/// broad-phase pass collects what the filter accepts in reach of both sweeps (the world's
/// cast tries no collider whose broad-phase box its sweep misses), then each sweep casts
/// against those nearest first, each pair exactly as the world's cast does, and keeps the
/// earliest impact.
fn clearance(
    simulation: &Simulation,
    scratch: &mut SteeringScratch,
    tank_index: usize,
    direction: Vec2,
    length: f64,
    needed: f64,
) -> f64 {
    let tank = &simulation.tanks[tank_index];
    let collider = &simulation.world.colliders[tank.collider];
    let position = collider.translation();
    let angle = direction.x.atan2(direction.z);
    let velocity = vector(direction.x, 0.0, direction.z);
    // Check both the current hull and the orientation it is turning toward.
    let poses = [
        tank.heading,
        tank.heading + angle_delta(tank.heading, angle),
    ]
    .map(|yaw| Pose::from_parts(position, to_rotation(Quat4::yaw(yaw))));
    let hulls = poses.map(|pose| collider.shape().compute_aabb(&pose));
    let travel = velocity * length as f32;
    let reach = hulls[0]
        .merged(&hulls[1])
        .loosened((HULL_MARGIN + SWEEP_SLACK) as f32);
    let swept = Aabb::new(reach.mins + travel.min(Vector::ZERO), reach.maxs + travel.max(Vector::ZERO));
    let query = if tank.kind == VehicleKind::Humvee {
        group::STEERING_QUERY | group::DEBRIS_QUERY
    } else {
        group::STEERING_QUERY
    };
    let filter = QueryFilter::default()
        .groups(interaction_groups(query))
        .exclude_rigid_body(tank.body);
    scratch.obstacles.clear();
    scratch.obstacles.extend(
        simulation
            .world
            .query_pipeline_with_filter(filter)
            .intersect_aabb_conservative(swept)
            .map(|(handle, obstacle)| (handle, obstacle.compute_aabb())),
    );
    let mut clear = length;
    if scratch.obstacles.is_empty() {
        return clear;
    }
    let ray = Ray::new(Vector::ZERO, velocity);
    let dispatcher = simulation.world.narrow_phase.query_dispatcher();
    for (pose, hull) in poses.iter().zip(hulls) {
        let options = ShapeCastOptions {
            max_time_of_impact: clear as f32,
            target_distance: HULL_MARGIN as f32,
            stop_at_penetration: false,
            compute_impact_geometry_on_penetration: true,
        };
        // When the sweep reaches each obstacle's box, widened past any rounding in a cast's
        // time of impact: an obstacle first reached after an impact cannot come earlier.
        let shift = -hull.center();
        let margin = hull.half_extents() + Vector::splat((HULL_MARGIN + SWEEP_SLACK) as f32);
        scratch.reached.clear();
        for (index, (_, bounds)) in scratch.obstacles.iter().enumerate() {
            let sum = Aabb::new(bounds.mins + shift - margin, bounds.maxs + shift + margin);
            if let Some(time) = sum.cast_local_ray(&ray, options.max_time_of_impact, true) {
                scratch.reached.push((time, index));
            }
        }
        scratch.reached.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        let mut earliest = options.max_time_of_impact;
        for &(time, index) in &scratch.reached {
            if time >= earliest {
                break;
            }
            let obstacle = &simulation.world.colliders[scratch.obstacles[index].0];
            let at = obstacle.position();
            if let Ok(Some(hit)) = dispatcher.cast_shapes(
                &at.inv_mul(pose),
                at.rotation.inverse() * velocity,
                obstacle.shape(),
                collider.shape(),
                options,
            ) && hit.time_of_impact < earliest
            {
                earliest = hit.time_of_impact;
            }
        }
        if earliest < options.max_time_of_impact {
            clear = clear.min(earliest as f64);
        }
        if clear < needed {
            break;
        }
    }
    clear
}

/// Commit briefly to an open side instead of alternating retreat and attack every tick.
pub fn steer_bot(simulation: &mut Simulation, tank_index: usize, desired: Vec2, dt: f64) -> Vec2 {
    let mut scratch = std::mem::take(&mut simulation.steering);
    let steering = steer_around(simulation, &mut scratch, tank_index, desired, dt);
    simulation.steering = scratch;
    steering
}

fn steer_around(
    simulation: &mut Simulation,
    scratch: &mut SteeringScratch,
    tank_index: usize,
    desired: Vec2,
    dt: f64,
) -> Vec2 {
    let magnitude = desired.x.hypot(desired.z);
    let brain = &mut simulation.tanks[tank_index].brain;
    brain.avoidance_time = 0f64.max(brain.avoidance_time - dt);
    if magnitude < STEERING_DEADZONE {
        brain.avoidance_time = 0.0;
        return Vec2::ZERO;
    }
    let (avoidance, avoidance_time) = (brain.avoidance, brain.avoidance_time);
    let direction = Vec2::new(desired.x / magnitude, desired.z / magnitude);
    let lookahead = if simulation.tanks[tank_index].kind == VehicleKind::Humvee {
        HUMVEE_LOOKAHEAD
    } else {
        LOOKAHEAD_DISTANCE
    };
    let enough = lookahead * CLEAR_FRACTION;
    if avoidance_time > 0.0
        && clearance(simulation, scratch, tank_index, avoidance, lookahead, enough) >= enough
    {
        return Vec2::new(avoidance.x * magnitude, avoidance.z * magnitude);
    }
    if clearance(simulation, scratch, tank_index, direction, lookahead, enough) >= enough {
        return desired;
    }
    let mut best = Vec2::ZERO;
    let mut best_score = f64::NEG_INFINITY;
    let mut best_clearance = 0.0;
    // Keep right when meeting another tank; the same local rule separates both vehicles.
    for angle in [
        PI / 4.0,
        PI / 2.0,
        -PI / 4.0,
        -PI / 2.0,
        PI * 0.75,
        -PI * 0.75,
        PI,
    ] {
        let cos = angle.cos();
        let sin = angle.sin();
        let candidate = Vec2::new(
            direction.x * cos + direction.z * sin,
            direction.z * cos - direction.x * sin,
        );
        let continuity = candidate.x * avoidance.x + candidate.z * avoidance.z;
        let score = |open: f64| {
            1f64.min(open / lookahead) * CLEARANCE_WEIGHT + cos + continuity * CONTINUITY_WEIGHT
        };
        // An obstruction only lowers the score, so a side that cannot win fully open needs no cast.
        if score(lookahead) <= best_score {
            continue;
        }
        let open = clearance(
            simulation,
            scratch,
            tank_index,
            candidate,
            lookahead,
            MINIMUM_CLEARANCE,
        );
        if open > MINIMUM_CLEARANCE && score(open) > best_score {
            best = candidate;
            best_score = score(open);
            best_clearance = open;
        }
    }
    let brain = &mut simulation.tanks[tank_index].brain;
    brain.avoidance = best;
    brain.avoidance_time = COMMITMENT_SECONDS;
    let speed = magnitude * 1f64.min(best_clearance / lookahead);
    Vec2::new(best.x * speed, best.z * speed)
}

/// Sustained lack of progress triggers a committed detour, independent of decision timing.
pub fn recover_bot(simulation: &mut Simulation, tank_index: usize, desired: Vec2, dt: f64) {
    let position = simulation.tank_planar(tank_index);
    let brain = &mut simulation.tanks[tank_index].brain;
    brain.recovery = 0f64.max(brain.recovery - dt);
    if distance(position, brain.last) > PROGRESS_DISTANCE
        || desired.x.hypot(desired.z) < MOVEMENT_DEADZONE
    {
        brain.last = position;
        brain.stuck = 0.0;
    } else {
        brain.stuck += dt;
    }
    if brain.recovery > 0.0 && distance(position, brain.recovery_goal) < PROGRESS_DISTANCE {
        brain.recovery = 0.0;
    }
    if brain.stuck < STUCK_SECONDS || brain.recovery > 0.0 {
        return;
    }
    let angle = desired.x.atan2(desired.z);
    let side = if brain.recoveries.is_multiple_of(2) {
        1.0
    } else {
        -1.0
    };
    for offset in [(side * PI) / 2.0, (-side * PI) / 2.0, PI, (side * PI) / 4.0] {
        let goal = Vec2::new(
            position.x + (angle + offset).sin() * DETOUR_DISTANCE,
            position.z + (angle + offset).cos() * DETOUR_DISTANCE,
        );
        if simulation.nav.is_blocked(goal) {
            continue;
        }
        simulation
            .nav
            .find_into(position, goal, &mut simulation.scratch.path);
        if simulation.scratch.path.is_empty() {
            continue;
        }
        let nav_version = simulation.nav.version;
        let brain = &mut simulation.tanks[tank_index].brain;
        std::mem::swap(&mut brain.path, &mut simulation.scratch.path);
        brain.recovery_goal = goal;
        brain.recovery = RECOVERY_COMMITMENT_SECONDS;
        brain.nav_version = nav_version;
        brain.recoveries += 1;
        simulation.bot_reroutes += 1;
        brain.avoidance_time = 0.0;
        brain.stuck = 0.0;
        brain.last = position;
        return;
    }
    let brain = &mut simulation.tanks[tank_index].brain;
    brain.stuck = 0.0;
    brain.decision = 0.0; // Retry the strategic route if there is no local exit.
}
