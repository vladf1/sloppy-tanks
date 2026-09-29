//! Shell hit volumes around tank hulls, and the hull contact collider.

use rapier3d::parry::query::{Ray, RayCast};
use rapier3d::parry::shape::Cuboid;
use rapier3d::prelude::{ColliderBuilder, Pose};

use super::data::group;
use super::math::{Point3, hypot3};
use super::physics::{interaction_groups, vector};
use super::simulation::Simulation;
use super::tank_dimensions::tank_hull;
use super::types::{Shot, Tank, VehicleKind};

pub const SHELL_HIT_RADIUS: f64 = 0.18;
/// Combat is planar; visual launcher height does not enlarge the target.
const HIT_HALF_HEIGHT: f64 = 0.9;
/// Covers floating-point differences between the bound below and Rapier's own ray test.
const REACH_TOLERANCE: f64 = 0.01;

/// The planar lane of a shell, or of a probe testing a firing lane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotProbe {
    pub x: f64,
    /// Lane height; one metre when unset.
    pub y: Option<f64>,
    pub z: f64,
    pub vx: f64,
    pub vz: f64,
    pub owner: u32,
}

impl From<&Shot> for ShotProbe {
    fn from(shot: &Shot) -> Self {
        Self {
            x: shot.x,
            y: shot.y,
            z: shot.z,
            vx: shot.vx,
            vz: shot.vz,
            owner: shot.owner,
        }
    }
}

fn hit_shape(kind: VehicleKind) -> Cuboid {
    let size = tank_hull(kind).size;
    Cuboid::new(vector(
        size.x / 2.0 + SHELL_HIT_RADIUS,
        HIT_HALF_HEIGHT,
        size.z / 2.0 + SHELL_HIT_RADIUS,
    ))
}

/// Farthest any point of a hit box can lie from its body's origin in plan view, in any
/// orientation: the box's half-diagonal plus the hull's offset from the origin.
fn reach(kind: VehicleKind) -> f64 {
    let hull = tank_hull(kind);
    let half_diagonal = hypot3(
        hull.size.x / 2.0 + SHELL_HIT_RADIUS,
        HIT_HALF_HEIGHT,
        hull.size.z / 2.0 + SHELL_HIT_RADIUS,
    );
    half_diagonal + hull.center.x.hypot(hull.center.z) + REACH_TOLERANCE
}

/// Full visible footprint for tank contact, without the shell-radius allowance.
pub fn tank_contact_collider(kind: VehicleKind) -> ColliderBuilder {
    let hull = tank_hull(kind);
    ColliderBuilder::cuboid((hull.size.x / 2.0) as f32, 0.6, (hull.size.z / 2.0) as f32)
        .translation(vector(hull.center.x, 0.0, hull.center.z))
        .collision_groups(interaction_groups(group::TANK_CONTACT))
        .mass(0.0)
        .friction(0.05)
        .restitution(0.0)
}

/// Sweep a shell against the hull, accounting for this tick's tank translation. Callers
/// testing many shells may pass the live body's current translation read once.
pub fn tank_hit_time(
    simulation: &Simulation,
    shot: &ShotProbe,
    tank: &Tank,
    limit: f64,
    elapsed: f64,
    frame_delta: f64,
    translation: Option<Point3>,
) -> Option<f64> {
    if !tank.alive || tank.id == shot.owner {
        return None;
    }
    let end = translation.unwrap_or_else(|| simulation.body_translation(tank.body));
    let vx = if frame_delta > 0.0 {
        (end.x - tank.previous.x) / frame_delta
    } else {
        0.0
    };
    let vz = if frame_delta > 0.0 {
        (end.z - tank.previous.z) / frame_delta
    } else {
        0.0
    };
    // Most lanes pass far from most hulls. If the ray's closest approach to the body within
    // the limit stays beyond the box's reach, Rapier could not report a hit either.
    let dx = end.x - vx * (frame_delta - elapsed) - shot.x;
    let dz = end.z - vz * (frame_delta - elapsed) - shot.z;
    let rx = shot.vx - vx;
    let rz = shot.vz - vz;
    let speed_squared = rx * rx + rz * rz;
    let closest = if speed_squared > 0.0 {
        0f64.max(limit.min((dx * rx + dz * rz) / speed_squared))
    } else {
        0.0
    };
    if (dx - rx * closest).hypot(dz - rz * closest) > reach(tank.kind) {
        return None;
    }
    let center = tank_hull(tank.kind).center;
    let rotation = *simulation.world.bodies[tank.body].rotation();
    // Live tanks rotate only around the vertical axis.
    let (ry, rw) = (rotation.y as f64, rotation.w as f64);
    let cos = 1.0 - 2.0 * ry * ry;
    let sin = 2.0 * rw * ry;
    let position = vector(
        end.x - vx * (frame_delta - elapsed) + center.x * cos + center.z * sin,
        end.y,
        end.z - vz * (frame_delta - elapsed) - center.x * sin + center.z * cos,
    );
    let ray = Ray::new(
        vector(shot.x, shot.y.unwrap_or(1.0), shot.z),
        vector(rx, 0.0, rz),
    );
    let time = hit_shape(tank.kind).cast_ray(
        &Pose::from_parts(position, rotation),
        &ray,
        limit as f32,
        true,
    )? as f64;
    (time >= 0.0 && time <= limit).then_some(time)
}
