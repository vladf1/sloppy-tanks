//! Tank hull bounds and muzzles in the tank's local frame, measured from the vehicle
//! models (`models::tank_dimensions`) exactly as the previous engine measured its
//! rendered models. The hull includes tracks; the independently rotating gun is not a
//! hull target.

use std::sync::OnceLock;

use glam::DVec3;

use super::types::VehicleKind;
pub use crate::models::TankDimensions;

/// Hull bounds in the tank's local frame, measured once per chassis.
pub fn tank_hull(kind: VehicleKind) -> &'static TankDimensions {
    static DIMENSIONS: OnceLock<[TankDimensions; 4]> = OnceLock::new();
    &DIMENSIONS.get_or_init(|| VehicleKind::ALL.map(crate::models::tank_dimensions))[kind.index()]
}

pub fn tank_muzzle(kind: VehicleKind) -> DVec3 {
    tank_hull(kind).muzzle
}

/// Render-only launch point; unlike `tank_muzzle`, this is not used for collision queries.
pub fn tank_visual_muzzle(kind: VehicleKind) -> DVec3 {
    tank_hull(kind).visual_muzzle
}

/// The Humvee model's body length scale; wrecks size the tumbling chassis with it.
pub use crate::models::HUMVEE_BODY_LENGTH_SCALE;
