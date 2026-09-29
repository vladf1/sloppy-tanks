//! The laser defense pickup: a chance to zap incoming enemy shells.

use super::data::LASER_DEFENSE;
use super::math::Vec2;
use super::simulation::Simulation;
use super::types::{Shot, Tank};

/// Relative speeds below this cannot bring a shell into range.
const MINIMUM_RELATIVE_SPEED_SQUARED: f64 = 1e-8;

/// First entry into defense range, in the same time coordinates as other contacts.
/// Relative motion also catches a tank driving into the path of an incoming shell.
pub fn laser_contact_time(
    simulation: &Simulation,
    shot: &Shot,
    tank: &Tank,
    limit: f64,
    elapsed: f64,
    frame_delta: f64,
) -> Option<f64> {
    if !tank.alive || tank.laser <= 0.0 || tank.team == shot.team || shot.laser_checked_by.contains(&tank.id) {
        return None;
    }
    let end = simulation.body_translation(tank.body);
    let tx = if frame_delta > 0.0 { (end.x - tank.previous.x) / frame_delta } else { 0.0 };
    let tz = if frame_delta > 0.0 { (end.z - tank.previous.z) / frame_delta } else { 0.0 };
    let x = end.x - tx * (frame_delta - elapsed);
    let z = end.z - tz * (frame_delta - elapsed);
    let dx = shot.x - x;
    let dz = shot.z - z;
    let vx = shot.vx - tx;
    let vz = shot.vz - tz;
    let speed2 = vx * vx + vz * vz;
    let approach = dx * vx + dz * vz;
    if speed2 < MINIMUM_RELATIVE_SPEED_SQUARED || approach >= 0.0 {
        return None;
    }
    let distance2 = dx * dx + dz * dz;
    // Ignore shots traveling away or passing safely to the side.
    if distance2 - (approach * approach) / speed2 > LASER_DEFENSE.threat_radius.powi(2) {
        return None;
    }
    let c = distance2 - LASER_DEFENSE.range.powi(2);
    let discriminant = approach * approach - speed2 * c;
    if discriminant < 0.0 {
        return None;
    }
    let time = if c <= 0.0 { 0.0 } else { (-approach - discriminant.sqrt()) / speed2 };
    if time < 0.0 || time > limit {
        return None;
    }
    simulation
        .visible(
            Vec2::new(x + tx * time, z + tz * time),
            Vec2::new(shot.x + shot.vx * time, shot.z + shot.vz * time),
        )
        .then_some(time)
}
