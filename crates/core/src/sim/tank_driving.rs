//! Track steering applied as bounded impulses; collisions and knockback retain momentum.

use std::f64::consts::PI;

use rapier3d::prelude::RigidBody;

use super::data::{HULL_TURN_SPEED, MOVE_ACCELERATION, REVERSE_SPEED, vehicle};
use super::math::{Quat4, angle_delta};
use super::physics::{to_rotation, vector};
use super::types::{Tank, VehicleCommand, VehicleKind};

const DRIVE_DEADZONE: f64 = 0.05;
const REVERSE_ANGLE_EPSILON: f64 = 1e-6;
const SPEED_BOOST_MULTIPLIER: f64 = 1.5;

pub fn drive_tank(
    tank: &mut Tank,
    body: &mut RigidBody,
    command: &VehicleCommand,
    dt: f64,
    speed_scale: f64,
) {
    let input_magnitude = command.move_x.hypot(command.move_z);
    let speed = vehicle(tank.kind).speed
        * speed_scale
        * if tank.speed > 0.0 {
            SPEED_BOOST_MULTIPLIER
        } else {
            1.0
        };
    let mut drive = 0.0;
    if input_magnitude > DRIVE_DEADZONE {
        let desired = command.move_x.atan2(command.move_z);
        // Choose the nearer end of the hull; perpendicular input favors forward.
        let reverse_requested =
            angle_delta(tank.heading, desired).abs() > PI / 2.0 + REVERSE_ANGLE_EPSILON;
        // HMMWVs are hunters: rotate and drive forward to retreat instead of backing into
        // danger. Only a committed stuck-position recovery may use reverse gear.
        let reverse = reverse_requested
            && (tank.kind != VehicleKind::Humvee || tank.human || tank.brain.recovery > 0.0);
        let target = desired + if reverse { PI } else { 0.0 };
        let turn = angle_delta(tank.heading, target);
        tank.heading += (-HULL_TURN_SPEED * dt).max((HULL_TURN_SPEED * dt).min(turn));
        // Unequal track speeds make an arc. Sharp turns shed speed toward a pivot.
        let alignment = 0f64.max(angle_delta(tank.heading, target).cos());
        drive = input_magnitude.min(1.0)
            * speed
            * alignment
            * alignment
            * if reverse { -REVERSE_SPEED } else { 1.0 };
    }
    let desired_velocity_x = tank.heading.sin() * drive;
    let desired_velocity_z = tank.heading.cos() * drive;
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
    body.set_rotation(to_rotation(Quat4::yaw(tank.heading)), true);
}
