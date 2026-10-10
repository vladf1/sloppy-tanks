//! Port of `first-person.ts`: the player's view from inside the turret. It is
//! presentation state: each tick it becomes the same `VehicleCommand` aim and
//! movement the overhead pointer produces, so the simulation never knows which
//! view is in use.

use sloppy_core::sim::VehicleCommand;
use sloppy_core::sim::math::angle_delta;

use super::view_settings::FIRST_PERSON;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FirstPersonLook {
    pub enabled: bool,
    /// View and turret heading in the `Tank.aim` convention: facing (sin, cos) on X/Z.
    pub yaw: f64,
}

impl FirstPersonLook {
    /// Entering starts from the turret's current aim so the view never snaps.
    pub fn toggle(&mut self, aim: f64) {
        self.enabled = !self.enabled;
        if self.enabled {
            self.yaw = angle_delta(0.0, aim);
        }
    }

    /// Positive mouse pixels or stick X turn to the right, which lowers yaw.
    pub fn turn(&mut self, pixels: f64, stick_x: f64, dt: f64) {
        let turn = pixels * FIRST_PERSON.mouse_radians_per_pixel
            + stick_x * FIRST_PERSON.touch_turn_radians_per_second * dt;
        self.yaw = angle_delta(0.0, self.yaw - turn);
    }

    /// Forward input drives where the turret looks; strafing input heads to its sides.
    pub fn steer(&self, command: VehicleCommand) -> VehicleCommand {
        if !self.enabled {
            return command;
        }
        let (move_x, move_z) = view_relative_move(command.move_x, command.move_z, self.yaw);
        VehicleCommand {
            move_x,
            move_z,
            ..command
        }
    }

    /// Clockwise screen angle of a world bearing, with the view's heading straight up.
    pub fn screen_angle(&self, bearing: f64) -> f64 {
        angle_delta(bearing, self.yaw)
    }
}

/// Rotate screen-style movement (negative Z is forward, positive X is right) into
/// the world for a view facing `yaw`. The overhead camera faces yaw = π, where
/// this is the identity. The result is capped at unit length: a rotated keyboard
/// diagonal would otherwise put up to √2 on one axis, which the multiplayer server
/// rejects as out-of-range input.
pub fn view_relative_move(move_x: f64, move_z: f64, yaw: f64) -> (f64, f64) {
    let (sin, cos) = yaw.sin_cos();
    let scale = 1.0 / move_x.hypot(move_z).max(1.0);
    (
        (-move_x * cos - move_z * sin) * scale,
        (move_x * sin - move_z * cos) * scale,
    )
}

fn ease_in_out(t: f64) -> f64 {
    if t < 0.5 {
        4.0 * t.powi(3)
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// How far along the line from the overhead pose to the eye the camera is at a
/// view blend (0 overhead, 1 seated in the turret).
pub fn seat_flight(blend: f64) -> f64 {
    ease_in_out(blend)
}

/// How far the camera has turned from the overhead gaze to the turret's heading.
pub fn seat_turn(blend: f64) -> f64 {
    let start = FIRST_PERSON.transition_turn_start;
    ease_in_out(((blend - start) / (1.0 - start)).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn overhead_heading_is_the_identity() {
        let (x, z) = view_relative_move(1.0, -1.0, PI);
        assert!((x - 1.0 / 2f64.sqrt()).abs() < 1e-12);
        assert!((z + 1.0 / 2f64.sqrt()).abs() < 1e-12);
        let (x, z) = view_relative_move(0.0, -1.0, 0.0);
        // Facing +Z (yaw 0), forward drives +Z.
        assert!(x.abs() < 1e-12 && (z - 1.0).abs() < 1e-12);
    }

    #[test]
    fn toggling_starts_at_the_turret_aim_and_turning_lowers_yaw() {
        let mut look = FirstPersonLook::default();
        look.toggle(7.0);
        assert!(look.enabled);
        assert!((look.yaw - angle_delta(0.0, 7.0)).abs() < 1e-12);
        let before = look.yaw;
        look.turn(10.0, 0.0, 0.016);
        assert!((before - look.yaw - 0.032).abs() < 1e-9);
        let command = look.steer(VehicleCommand {
            move_z: -1.0,
            ..VehicleCommand::idle()
        });
        assert!((command.move_x - look.yaw.sin()).abs() < 1e-9);
        assert!((command.move_z - look.yaw.cos()).abs() < 1e-9);
        look.toggle(0.0);
        assert!(!look.enabled);
    }

    #[test]
    fn seat_curves_run_from_zero_to_one() {
        assert_eq!(seat_flight(0.0), 0.0);
        assert_eq!(seat_flight(1.0), 1.0);
        assert_eq!(seat_turn(0.3), 0.0);
        assert_eq!(seat_turn(1.0), 1.0);
        assert!((seat_flight(0.5) - 0.5).abs() < 1e-12);
    }
}
