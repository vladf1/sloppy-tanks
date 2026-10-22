//! Port of `village-roads.ts`: the Pine Village road strips, shared by the ground
//! shader and surface effects (track dust) so their boundaries stay aligned.

use crate::sim::data::ARENA;

/// How far inside a road edge the solid dirt starts, in metres: the ground shader
/// frays the edge into grass for about this far.
pub const ROAD_SHOULDER: f64 = 0.7;

/// One road strip: centre, footprint and the height its mesh floats at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VillageRoad {
    pub x: f64,
    pub z: f64,
    pub w: f64,
    pub d: f64,
    pub y: f64,
}

/// `VILLAGE_ROADS`: three north-south roads, then three east-west ones (the later
/// ones float slightly higher so crossings do not z-fight).
pub fn village_roads() -> [VillageRoad; 6] {
    let along = |x: f64| VillageRoad {
        x,
        z: 0.0,
        w: if x == 0.0 { 18.0 } else { 10.0 },
        d: ARENA * 2.0 - 2.0,
        y: 0.0425,
    };
    let across = |z: f64| VillageRoad {
        x: 0.0,
        z,
        w: ARENA * 2.0 - 2.0,
        d: if z == 0.0 { 12.0 } else { 8.0 },
        y: 0.0625,
    };
    [
        along(-52.0),
        along(0.0),
        along(52.0),
        across(-38.0),
        across(0.0),
        across(38.0),
    ]
}

/// The village square's two paved streets round the arena's centre, as half
/// extents in metres: the main street, then the cross street. The ground shader
/// (`meadow_sway.wgsl`, `VG_SQUARE_MAIN` and `VG_SQUARE_CROSS`) lays its setts
/// and kerb on the same outline.
pub const VILLAGE_SQUARE: [[f64; 2]; 2] = [[7.4, 26.0], [20.5, 4.4]];

/// Whether (x, z) lies on the village square or within `margin` metres of it.
pub fn in_village_square(x: f64, z: f64, margin: f64) -> bool {
    VILLAGE_SQUARE
        .iter()
        .any(|[hx, hz]| x.abs() < hx + margin && z.abs() < hz + margin)
}

/// `isVillageDirt`: dust starts on the solid dirt, leaving the grass-blended
/// shoulders quiet.
pub fn is_village_dirt(x: f64, z: f64) -> bool {
    village_roads().iter().any(|road| {
        (x - road.x).abs() <= road.w / 2.0 - ROAD_SHOULDER
            && (z - road.z).abs() <= road.d / 2.0 - ROAD_SHOULDER
    })
}
