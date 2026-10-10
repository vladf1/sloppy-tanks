//! Track steering applied as bounded impulses; collisions and knockback retain momentum.
//! The easter egg's Cybertrucks steer like cars instead.

use std::f64::consts::PI;

use rapier3d::prelude::RigidBody;

use super::data::{HULL_TURN_SPEED, MOVE_ACCELERATION, REVERSE_SPEED, vehicle};
use super::math::{Quat4, angle_delta};
use super::physics::{to_rotation, vector};
use super::types::{Tank, VehicleCommand, VehicleKind};

const DRIVE_DEADZONE: f64 = 0.05;
const REVERSE_ANGLE_EPSILON: f64 = 1e-6;
const SPEED_BOOST_MULTIPLIER: f64 = 1.5;
/// A Cybertruck turns only while it rolls, no tighter than this radius (metres), and
/// at least this fast (radians per second) so one at rest can pull away into a turn.
const TRUCK_TURN_RADIUS: f64 = 5.0;
const TRUCK_CREEP_TURN: f64 = 0.35;
/// The share of its speed a Cybertruck keeps through the sharpest turn: it arcs round
/// rather than pivoting.
const TRUCK_TURN_SPEED_SHARE: f64 = 0.5;

pub fn drive_tank(
    tank: &mut Tank,
    body: &mut RigidBody,
    command: &VehicleCommand,
    dt: f64,
    speed_scale: f64,
) {
    let mut hull = Hull {
        kind: tank.kind,
        heading: tank.heading,
        boosted: tank.speed > 0.0,
        // HMMWVs are hunters: rotate and drive forward to retreat instead of backing into
        // danger, and Cybertrucks are cars. Only a committed stuck-position recovery may
        // use reverse gear.
        reverse_gear: !(tank.kind == VehicleKind::Humvee || tank.kind.neutral())
            || tank.human
            || tank.brain.recovery > 0.0,
    };
    drive_hull(
        &mut hull,
        body,
        command.move_x,
        command.move_z,
        dt,
        speed_scale,
    );
    tank.heading = hull.heading;
}

/// What the drive model reads from a tank: enough for client prediction to drive the
/// viewer's hull exactly as the simulation does, without the rest of the tank.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hull {
    pub kind: VehicleKind,
    pub heading: f64,
    /// A speed pickup is active.
    pub boosted: bool,
    pub reverse_gear: bool,
}

pub fn drive_hull(
    hull: &mut Hull,
    body: &mut RigidBody,
    move_x: f64,
    move_z: f64,
    dt: f64,
    speed_scale: f64,
) {
    let input_magnitude = move_x.hypot(move_z);
    let speed = vehicle(hull.kind).speed
        * speed_scale
        * if hull.boosted {
            SPEED_BOOST_MULTIPLIER
        } else {
            1.0
        };
    let mut drive = 0.0;
    if input_magnitude > DRIVE_DEADZONE {
        let desired = move_x.atan2(move_z);
        // Choose the nearer end of the hull; perpendicular input favors forward.
        let reverse_requested =
            angle_delta(hull.heading, desired).abs() > PI / 2.0 + REVERSE_ANGLE_EPSILON;
        let reverse = reverse_requested && hull.reverse_gear;
        let target = desired + if reverse { PI } else { 0.0 };
        let turn = angle_delta(hull.heading, target);
        let car = hull.kind.neutral();
        let turn_speed = if car {
            car_turn_speed(body, hull.heading)
        } else {
            HULL_TURN_SPEED
        };
        hull.heading += (-turn_speed * dt).max((turn_speed * dt).min(turn));
        // Unequal track speeds make an arc. Sharp turns shed speed toward a pivot; a car
        // keeps rolling round.
        let alignment = 0f64.max(angle_delta(hull.heading, target).cos());
        let share = if car {
            TRUCK_TURN_SPEED_SHARE + (1.0 - TRUCK_TURN_SPEED_SHARE) * alignment
        } else {
            alignment * alignment
        };
        drive =
            input_magnitude.min(1.0) * speed * share * if reverse { -REVERSE_SPEED } else { 1.0 };
    }
    let desired_velocity_x = hull.heading.sin() * drive;
    let desired_velocity_z = hull.heading.cos() * drive;
    let velocity = body.linvel();
    let velocity_delta_x = desired_velocity_x - velocity.x as f64;
    let velocity_delta_z = desired_velocity_z - velocity.z as f64;
    let delta = velocity_delta_x.hypot(velocity_delta_z);
    let acceleration_fraction =
        1f64.min((MOVE_ACCELERATION * dt) / if delta == 0.0 { 1.0 } else { delta });
    // Bounded impulses preserve knockback; no per-frame velocity overwrite.
    let mass = body.mass() as f64;
    body.apply_impulse(
        vector(
            velocity_delta_x * acceleration_fraction * mass,
            0.0,
            velocity_delta_z * acceleration_fraction * mass,
        ),
        true,
    );
    body.set_rotation(to_rotation(Quat4::yaw(hull.heading)), true);
}

/// How fast a car may turn: its rolling speed over the tightest turning radius, within
/// the creep and track turn speeds.
fn car_turn_speed(body: &RigidBody, heading: f64) -> f64 {
    let velocity = body.linvel();
    let rolling = (velocity.x as f64 * heading.sin() + velocity.z as f64 * heading.cos()).abs();
    (rolling / TRUCK_TURN_RADIUS).clamp(TRUCK_CREEP_TURN, HULL_TURN_SPEED)
}
