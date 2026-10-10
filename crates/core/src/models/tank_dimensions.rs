//! Port of `tank-dimensions.ts`: hull bounds and muzzles measured from the models.
//!
//! Each chassis is measured with the same geometry and transforms as rendering;
//! the simulation (`sim::tank_dimensions`) measures each kind once and caches it.
//! The hull includes tracks; the independently rotating gun is not a hull target.

use glam::{DMat4, DVec3};

use super::tank_model::tank_model;
use super::{Team, VehicleKind, part};
use crate::geometry::node_bounds;

/// The Humvee's combat launch height. The visual roof launcher stays high, but its
/// combat lane sits inside the planar tank hit volume the simulation uses.
const HUMVEE_COMBAT_MUZZLE_Y: f64 = 1.15;

/// Measurements in the vehicle's local frame (model scale applied).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TankDimensions {
    /// Center of the hull's bounds (`Box3.setFromObject(hull)`).
    pub center: DVec3,
    /// Size of the hull's bounds.
    pub size: DVec3,
    /// Launch point for combat queries.
    pub muzzle: DVec3,
    /// Render-only launch point; unlike `muzzle`, never used for collision queries.
    pub visual_muzzle: DVec3,
}

/// Measure one chassis from its team-0 model.
pub fn tank_dimensions(kind: VehicleKind) -> TankDimensions {
    let model = tank_model(kind, Team::Blue);
    let (hull, hull_parent) = model
        .find_with_parent_world(part::HULL, DMat4::IDENTITY)
        .expect("vehicle models have a hull");
    let bounds = node_bounds(hull, hull_parent);
    let (muzzle, muzzle_parent) = model
        .find_with_parent_world(part::MUZZLE, DMat4::IDENTITY)
        .expect("vehicle models have a muzzle");
    let visual_muzzle = muzzle.world_matrix(muzzle_parent).w_axis.truncate();
    let mut combat_muzzle = visual_muzzle;
    if kind == VehicleKind::Humvee {
        combat_muzzle.y = HUMVEE_COMBAT_MUZZLE_Y;
    }
    TankDimensions {
        center: bounds.center(),
        size: bounds.size(),
        muzzle: combat_muzzle,
        visual_muzzle,
    }
}
