//! Shell hit volumes around tank hulls, and the hull contact collider.

use std::sync::OnceLock;

use rapier3d::parry::query::{Ray, RayCast};
use rapier3d::parry::shape::Cuboid;
use rapier3d::prelude::{ColliderBuilder, Pose};

use super::data::group;
use super::math::{Point3, Vec2, hypot3};
use super::physics::{interaction_groups, vector};
use super::shooter_rewind::PastHull;
use super::simulation::Simulation;
use super::tank_dimensions::tank_hull;
use super::types::{Shot, Tank, VehicleKind};

pub const SHELL_HIT_RADIUS: f64 = 0.18;
/// Combat is planar; visual launcher height does not enlarge the target.
pub const HIT_HALF_HEIGHT: f64 = 0.9;
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
    /// The tank this lane passes through: the shooter, until its shell ricochets.
    pub ignored: Option<u32>,
}

impl From<&Shot> for ShotProbe {
    fn from(shot: &Shot) -> Self {
        Self {
            x: shot.x,
            y: shot.y,
            z: shot.z,
            vx: shot.vx,
            vz: shot.vz,
            ignored: (!shot.ricocheted).then_some(shot.owner),
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

/// Squared reach of the hit box from its body's origin in any orientation: its
/// half-diagonal plus the hull offset and the query tolerance. Chassis geometry
/// never changes, so derive these from the measured hulls once.
fn reach_squared(kind: VehicleKind) -> f64 {
    static REACHES: OnceLock<[f64; 4]> = OnceLock::new();
    REACHES.get_or_init(|| {
        VehicleKind::ALL.map(|kind| {
            let hull = tank_hull(kind);
            let half_diagonal = hypot3(
                hull.size.x / 2.0 + SHELL_HIT_RADIUS,
                HIT_HALF_HEIGHT,
                hull.size.z / 2.0 + SHELL_HIT_RADIUS,
            );
            let reach = half_diagonal + hull.center.x.hypot(hull.center.z) + REACH_TOLERANCE;
            reach * reach
        })
    })[kind.index()]
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

/// A hull's planar velocity over a tick from `previous` to `end`; zero for a zero-length
/// tick.
#[inline]
pub(crate) fn sweep_velocity(previous: Vec2, end: Point3, frame_delta: f64) -> Vec2 {
    if frame_delta > 0.0 {
        Vec2::new(
            (end.x - previous.x) / frame_delta,
            (end.z - previous.z) / frame_delta,
        )
    } else {
        Vec2::ZERO
    }
}

/// Where a hull is for a shell sweep.
#[derive(Clone, Copy, Debug)]
pub enum HullAt<'a> {
    /// The live body.
    Body,
    /// The live body, its translation read once by a caller testing many shells.
    Translation(Point3),
    /// Where it was some ticks ago, for a shell that favors its shooter.
    Past(&'a PastHull),
}

/// Sweep a shell against the hull, accounting for this tick's tank translation.
pub fn tank_hit_time(
    simulation: &Simulation,
    shot: &ShotProbe,
    tank: &Tank,
    limit: f64,
    elapsed: f64,
    frame_delta: f64,
    hull: HullAt,
) -> Option<f64> {
    if !tank.alive || shot.ignored == Some(tank.id) {
        return None;
    }
    let (previous, end) = match hull {
        HullAt::Body => (tank.previous, simulation.body_translation(tank.body)),
        HullAt::Translation(end) => (tank.previous, end),
        HullAt::Past(past) => (past.previous, past.end),
    };
    let Vec2 { x: vx, z: vz } = sweep_velocity(previous, end, frame_delta);
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
    let closest_x = dx - rx * closest;
    let closest_z = dz - rz * closest;
    // This is only a conservative rejection; Rapier still decides the exact hit.
    // Squared distances avoid a software hypot for every shell/hull pair in Wasm.
    if closest_x * closest_x + closest_z * closest_z > reach_squared(tank.kind) {
        return None;
    }
    let center = tank_hull(tank.kind).center;
    let rotation = match hull {
        HullAt::Past(past) => past.rotation,
        _ => *simulation.world.bodies[tank.body].rotation(),
    };
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
