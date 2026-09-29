//! Port of `harbor-water.ts`: one 340 m water plane under every harbor berth, so a
//! single reflection target covers them all. Reused across rounds.

use crate::geometry::plane_geometry;
use crate::scene::Node;

use super::water_surface::{WaterKind, water_surface};

/// Height of the harbor water surface below the apron.
pub const HARBOR_WATER_HEIGHT: f64 = -2.2;
const HARBOR_WATER_SIZE: f64 = 340.0;

/// `new HarborWater().mesh`.
pub fn harbor_water() -> Node {
    water_surface(
        plane_geometry(HARBOR_WATER_SIZE, HARBOR_WATER_SIZE),
        WaterKind::Harbor,
        HARBOR_WATER_HEIGHT,
    )
}
