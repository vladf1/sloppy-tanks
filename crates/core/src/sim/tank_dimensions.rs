//! Tank hull bounds and muzzles in the tank's local frame.
//!
//! The previous engine measured these once from the rendered tank models (the hull
//! includes tracks; the independently rotating gun is not a hull target). Until the model
//! builders measure the Rust models, these are the exact values the TypeScript measurement
//! produced; the model-derived measurement replaces them.

use super::math::Point3;
use super::types::VehicleKind;

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

const SCOUT: TankDimensions = TankDimensions {
    center: Point3::new(0.0, 0.18420179971489656, 0.0),
    size: Point3::new(1.9127049249722883, 0.6960582679971488, 3.7155932591971026),
    muzzle: Point3::new(0.0, 0.7068692088382038, 3.0977503563791875),
    visual_muzzle: Point3::new(0.0, 0.7068692088382038, 3.0977503563791875),
};
const BALANCED: TankDimensions = TankDimensions {
    center: Point3::new(0.0, 0.2066838842975206, 0.0),
    size: Point3::new(1.9500000067239949, 0.7308471074380165, 4.27091664998905),
    muzzle: Point3::new(0.0, 0.7735537190082644, 3.098243801652892),
    visual_muzzle: Point3::new(0.0, 0.7735537190082644, 3.098243801652892),
};
const HEAVY: TankDimensions = TankDimensions {
    center: Point3::new(0.0, 0.22860020491803268, 0.0),
    size: Point3::new(2.144467220272685, 0.7951684426229506, 4.702433035247105),
    muzzle: Point3::new(0.0, 0.8406311475409833, 3.812862704918032),
    visual_muzzle: Point3::new(0.0, 0.8406311475409833, 3.812862704918032),
};
// Keep the visual roof launcher high, but put the combat lane inside the planar tank hit
// volume used by the rest of the simulation.
const HUMVEE: TankDimensions = TankDimensions {
    center: Point3::new(0.0, 0.9584999933682383, 0.01044000171124937),
    size: Point3::new(2.0988000003419818, 2.348999987514317, 4.510080004044771),
    muzzle: Point3::new(0.0, 1.15, 1.53),
    visual_muzzle: Point3::new(0.0, 1.917, 1.53),
};

/// Hull bounds in the tank's local frame.
pub const fn tank_hull(kind: VehicleKind) -> &'static TankDimensions {
    match kind {
        VehicleKind::Scout => &SCOUT,
        VehicleKind::Balanced => &BALANCED,
        VehicleKind::Heavy => &HEAVY,
        VehicleKind::Humvee => &HUMVEE,
    }
}

pub const fn tank_muzzle(kind: VehicleKind) -> Point3 {
    tank_hull(kind).muzzle
}

/// Render-only launch point; unlike `tank_muzzle`, this is not used for collision queries.
pub const fn tank_visual_muzzle(kind: VehicleKind) -> Point3 {
    tank_hull(kind).visual_muzzle
}

/// The Humvee model's body length scale (humvee-model.ts); wrecks size the tumbling
/// chassis with it. The model builders own the model; keep the two values equal.
pub const HUMVEE_BODY_LENGTH_SCALE: f64 = 1.16;
