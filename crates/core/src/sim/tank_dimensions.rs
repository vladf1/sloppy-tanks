//! Tank hull bounds and muzzles in the tank's local frame, measured from the vehicle
//! models (`models::tank_dimensions`) exactly as the previous engine measured its
//! rendered models. The hull includes tracks; the independently rotating gun is not a
//! hull target.

use std::sync::OnceLock;

use super::math::Point3;
use super::types::VehicleKind;
use crate::models;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TankDimensions {
    /// Hull bounding-box center, local frame.
    pub center: Point3,
    /// Hull bounding-box size (full extents).
    pub size: Point3,
    /// Combat launch point: inside the planar hit volume for every chassis.
    pub muzzle: Point3,
    /// Render-only launch point; not used for collision queries.
    pub visual_muzzle: Point3,
}

fn point(v: glam::DVec3) -> Point3 {
    Point3::new(v.x, v.y, v.z)
}

/// Hull bounds in the tank's local frame.
pub fn tank_hull(kind: VehicleKind) -> &'static TankDimensions {
    static DIMENSIONS: OnceLock<[TankDimensions; 4]> = OnceLock::new();
    &DIMENSIONS.get_or_init(|| {
        VehicleKind::ALL.map(|kind| {
            let measured = models::tank_dimensions(kind);
            TankDimensions {
                center: point(measured.center),
                size: point(measured.size),
                muzzle: point(measured.muzzle),
                visual_muzzle: point(measured.visual_muzzle),
            }
        })
    })[kind.index()]
}

pub fn tank_muzzle(kind: VehicleKind) -> Point3 {
    tank_hull(kind).muzzle
}

/// Render-only launch point; unlike `tank_muzzle`, this is not used for collision queries.
pub fn tank_visual_muzzle(kind: VehicleKind) -> Point3 {
    tank_hull(kind).visual_muzzle
}

/// The Humvee model's body length scale; wrecks size the tumbling chassis with it.
pub use crate::models::HUMVEE_BODY_LENGTH_SCALE;
