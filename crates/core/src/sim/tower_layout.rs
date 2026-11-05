//! Watchtower dimensions shared by its structure (`tower_structure`), its model and
//! the collision footprint, in tower coordinates: the tower's ground centre at the
//! origin, west -x, front +z.

pub struct TowerBase {
    pub offset: f64,
    pub width: f64,
    pub depth: f64,
    pub height: f64,
    pub post_z: f64,
}

/// The two concrete footings, at ±`offset` along x, and the posts standing on them
/// `post_z` either side of the middle. The footings are low pads, so the legs stand
/// bare at the height shells fly (about a metre) and a shell there strikes a leg.
pub const TOWER_BASE: TowerBase = TowerBase {
    offset: 2.55,
    width: 1.3,
    depth: 3.0,
    height: 0.4,
    post_z: 1.05,
};

/// Post section and the top of the posts under the main beams.
pub const POST: f64 = 0.35;
pub const POST_TOP: f64 = 4.645;
/// Top of the steel shoes the posts stand in on the footings.
pub const SHOE_TOP: f64 = 0.75;
/// Height of the girts, where a leg's lower lift meets its upper one.
pub const GIRT_Y: f64 = 2.8;
/// Lowest point of the X-braces within each bent.
pub const BENT_BRACE_LOW: f64 = 1.45;
/// Where a bent's knee braces leave the posts, and the outrigger beams they reach
/// under the joists' overhang.
pub const KNEE_LOW: f64 = 3.55;
pub const OUTRIGGER_Y: f64 = POST_TOP + BEAM - 0.09;
/// Main beams, joists and planks under the deck top.
pub const BEAM: f64 = 0.3;
pub const JOIST_DEPTH: f64 = 0.18;
pub const PLANK: f64 = 0.05;
pub const DECK_TOP: f64 = POST_TOP + BEAM + JOIST_DEPTH + PLANK;
/// Half extents of the deck.
pub const DECK_X: f64 = 3.0;
pub const DECK_Z: f64 = 2.5;
/// Lookout cabin: half extents, wall top (the flat soffit) and wall thickness
/// from the clapboard face to the interior lining.
pub const CABIN_X: f64 = 2.15;
pub const CABIN_Z: f64 = 1.75;
pub const CABIN_TOP: f64 = 7.3;
pub const CABIN_WALL: f64 = 0.12;
/// Window band of the cabin walls and how far the awning shutters reach out.
pub const WINDOW_SILL: f64 = 6.05;
pub const WINDOW_HEIGHT: f64 = 0.85;
pub const AWNING_REACH: f64 = 0.86;
pub const AWNING_BOTTOM: f64 = 6.65;
pub const AWNING_TOP: f64 = 7.12;
/// Hip roof: overhang past the cabin walls, fascia depth and pitch.
pub const ROOF_OVERHANG: f64 = 0.5;
pub const ROOF_EDGE: f64 = 0.15;
pub const ROOF_PITCH: f64 = 0.55;
/// Walkway railing: height above the deck and the inset from the deck edge.
pub const RAIL_HEIGHT: f64 = 1.05;
pub const RAIL_INSET: f64 = 0.06;
/// The ladder up the front, just outside the deck edge.
pub const LADDER_X: f64 = 1.75;
pub const LADDER_HALF: f64 = 0.275;
pub const LADDER_Z: f64 = DECK_Z + 0.135;
pub const LADDER_TOP: f64 = DECK_TOP + RAIL_HEIGHT + 0.05;
/// Lowest point of the X-braces on the faces between the footings.
pub const FACE_BRACE_LOW: f64 = 1.4;

/// Height of the hip roof's ridge.
pub fn roof_apex() -> f64 {
    CABIN_TOP + ROOF_EDGE + ROOF_PITCH * (CABIN_Z + ROOF_OVERHANG)
}
