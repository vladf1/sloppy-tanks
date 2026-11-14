//! Senda: Rumbo, Gaviota and Neblina as the neighbourhood lays them out, traced
//! from an aerial photograph and brought into the arena at under a third of their
//! length and half their breadth.
//!
//! Neblina comes down from Senda Parkway on a diagonal across the arena's north-west and
//! sweeps round a broad curve east, where it becomes Gaviota, which runs to its
//! cul-de-sac; no street goes on south. Rumbo leaves Neblina near the top and curves
//! east to its own. Their houses stand detached in rows along the curving streets, side
//! yards between them and their fronts stepping lot by lot, each turned to front its
//! street with a driveway to its garage: both sides of Rumbo and Gaviota, their last
//! lots turning round their turning circles to face them, Neblina's west side in one even run from
//! its corner lot on Senda Parkway round the outside of the curve to a little park where
//! the name changes, Gaviota's south row on from there, and Neblina's east side only
//! round the bend. The streets are drawn short, and the spawn lanes at
//! the arena's edges bound them, so more lots would mean narrower houses or side yards.
//! A greenbelt of grass, scrub and a gravel swale lies between the back gardens of
//! Rumbo's south row and Gaviota's north row, opening onto Neblina between their
//! corner lots, and a planted bank rises from Rumbo's north row to Senda Parkway, which
//! runs along its top above the whole row. The tract's block wall and view fences close
//! the level along the neighbourhood's edges ([`senda_boundary`]). The shared spawn
//! lanes fall behind Neblina's west row and on the hillside east of the cul-de-sacs;
//! the level lays out its own pickups on its streets, greenbelt and slopes.
//!
//! Shared by the simulation and the scenery that paves the same streets.

use glam::DVec2;

use super::arena::{BOUNDARY_THICKNESS, CoverDef, PickupPlacement, spawn_positions};
use super::house_layout::{
    GARAGE_HOUSE_PITCH, GarageSide, HOUSE_HEALTH, HouseDesign, HousePlan, Plan, SENDA_STYLES,
    senda_lot, style_design, wall_frames,
};
use super::kinds::{CoverKind, PickupKind, Team};
use super::map_options::MapId;
use super::maps::{ArenaMap, MapTheme};
use super::model_data::ARENA;
use super::senda_yards::back_wall;
use super::tree_proportions::tree_proportions;

/// How the traced aerial maps into the arena: the turn that lays Rumbo along +x, the
/// rotated aerial pixel that lands at the arena's centre, and arena metres per aerial
/// pixel along the streets and across them (the photo is about 0.19 m a pixel). The
/// streets are drawn at under a third of their length so the neighbourhood, from the
/// back fences of Neblina's west row to both cul-de-sacs, fits between the spawn lanes;
/// across them a little more, so the greenbelt keeps room between the back gardens.
const AERIAL_TURN_DEGREES: f64 = 13.7;
const AERIAL_CENTRE: (f64, f64) = (838.0, 955.0);
const ALONG_PER_PIXEL: f64 = 0.058;
const ACROSS_PER_PIXEL: f64 = 0.1;

/// The streets' centre lines as traced on the aerial (pixels, x east, y south).
///
/// Senda Parkway comes down from the north past the hillside east of Rumbo's turning
/// circle, bends west and runs along the top of the bank above the whole of Rumbo's
/// north row, nearly parallel to its back walls just outside the level's wall, as the
/// real Way does, then down across the bank, widening it, and round one smooth curve past
/// Neblina's mouth. The real Way carries on south-south-west there, but drawn that way
/// it would run over the spawn lane's pads. So inside the wall it levels out between the
/// two north-western pads, leaving Neblina's corner house its corner lot on the Way's south side.
/// It ends at the level's west wall (the trace's first point lands on it): nothing of it
/// runs on outside the level there.
/// (The trace runs from the west.)
const SENDA_PARKWAY_TRACE: &[(f64, f64)] = &[
    (-48.0, 631.0),
    (85.0, 597.0),
    (221.0, 561.0),
    (268.0, 546.0),
    (312.0, 527.0),
    (350.0, 503.0),
    (382.0, 476.0),
    (420.0, 432.0),
    (540.0, 315.0),
    (660.0, 222.0),
    (780.0, 148.0),
    (901.0, 84.0),
    (1028.0, 22.0),
    (1159.0, -21.0),
    (1309.0, -63.0),
    (1475.0, -108.0),
    (1641.0, -156.0),
    (1772.0, -200.0),
    (1867.0, -249.0),
    (1940.0, -313.0),
    (1990.0, -398.0),
    (2019.0, -497.0),
    (2029.0, -603.0),
];
/// Neblina from Senda Parkway down to its bend, where it sweeps round east and becomes
/// Gaviota, on to Gaviota's cul-de-sac. No street goes on south past the bend.
///
/// Down to the bend the points lie east of the aerial's own: the arena draws the
/// streets at a smaller scale along Rumbo than across it, which would stand Neblina's
/// diagonal nearly upright (about 70 degrees off Rumbo). Laid this way it keeps nearer
/// the street's lean (about 60 degrees) and sweeps round a broad curve into
/// Gaviota rather than turning a corner.
const NEBLINA_GAVIOTA_TRACE: &[(f64, f64)] = &[
    (319.0, 545.0),
    (424.0, 609.0),
    (528.0, 672.0),
    (632.0, 736.0),
    (736.0, 800.0),
    (813.0, 846.0),
    (876.0, 874.0),
    (953.0, 888.0),
    (1039.0, 887.0),
    (1127.0, 873.0),
    (1250.0, 845.0),
    (1350.0, 830.0),
    (1450.0, 820.0),
    (1550.0, 812.0),
    (1650.0, 806.0),
    (1745.0, 798.0),
];
/// Where the name changes from Neblina to Gaviota: the trace's point halfway round the
/// curve.
const NEBLINA_BEND: usize = 7;
/// Rumbo from its corner on Neblina's east side.
const RUMBO_TRACE: &[(f64, f64)] = &[
    (565.0, 696.0),
    (624.0, 647.0),
    (680.0, 612.0),
    (760.0, 582.0),
    (840.0, 556.0),
    (950.0, 522.0),
    (1060.0, 490.0),
    (1160.0, 462.0),
    (1260.0, 433.0),
    (1360.0, 405.0),
    (1460.0, 383.0),
    (1560.0, 364.0),
    (1655.0, 346.0),
];
/// The turning circles at Rumbo's and Gaviota's ends, where the aerial has their middles,
/// and how far (m) back along its street's last heading each true-size circle stands from
/// there, so its sidewalk ring keeps clear of the spawn pad on the hillside past it.
const RUMBO_CUL_DE_SAC_TRACE: (f64, f64) = (1722.0, 322.0);
const GAVIOTA_CUL_DE_SAC_TRACE: (f64, f64) = (1810.0, 782.0);
const RUMBO_CUL_DE_SAC_SHIFT: f64 = 3.0;
const GAVIOTA_CUL_DE_SAC_SHIFT: f64 = 1.7;

/// Half widths (m): a residential street kerb to kerb, and Senda Parkway's four lanes
/// with their planted median.
pub const STREET_HALF: f64 = 2.8;
pub const SENDA_PARKWAY_HALF: f64 = 4.2;
pub const MEDIAN_HALF: f64 = 0.8;
/// The grass parkway (with its street trees) and sidewalk between the kerb and the
/// front yards, each the tract's 1.3 m at the tank's scale.
pub const PARKWAY: f64 = 0.7;
pub const SIDEWALK: f64 = 0.7;
/// A cul-de-sac's kerb radius and the planted island in its middle: the tract's 12.7 m
/// and 3.7 m at the tank's scale.
pub const CUL_DE_SAC_RADIUS: f64 = 6.8;
pub const CUL_DE_SAC_ISLAND: f64 = 2.0;

/// A house's box along its street and its lot's depth from the house's front (the
/// large plan's, [`plan_depth`]), the least its front stands back from the sidewalk,
/// and the lots' least spacing along the street: the tract's at the tank's scale (0.533
/// game m a real metre), an 11.7 m wide two-storey box 14-15 m deep whose garage door
/// stands 6.4 m from the kerb, on lots about 15 m apart. Its walls stay low
/// ([`WALL_PLATES`]), so a front is about twice as wide as its eaves are high.
pub const HOUSE_WIDTH: f64 = 6.2;
pub const HOUSE_DEPTH: f64 = 8.0;
pub const SETBACK: f64 = 2.0;
pub const LOT_PITCH: f64 = 7.8;
/// How much further back (m) than [`SETBACK`] a lot's house stands, lot by lot, so the
/// fronts step a little as the tract's different plans do rather than lining up as a
/// terrace; Neblina's west row steps every other house.
const STAGGER: [f64; 3] = [0.0, 0.4, 0.2];
const WEST_ROW_STAGGER: [f64; 2] = [0.0, 0.4];
/// The least side yard (m) between two houses' walls (the tract's 3-3.5 m, too narrow
/// for a tank), and how far along its street a row looks on for room when a lot is
/// taken.
pub const SIDE_YARD: f64 = 1.6;
const LOT_SEARCH: f64 = 0.1;
/// How finely (steps to a [`LOT_SEARCH`]) a row looks back for the first place a lot
/// fits, so a row loses no room to the search's step from lot to lot.
const LOT_REFINE: usize = 10;
/// The least distance (m) from the bend park's middle to a house of the rows either side
/// of it ([`bend_park`]): a narrow lot of Neblina's west row's, or the row's own pitch
/// where that is more.
const PARK_LOT: f64 = 7.45;
/// How far (m) up Neblina from its bend Gaviota's north row begins: the greenbelt
/// opens onto Neblina above it.
const GREENBELT_MOUTH: f64 = 0.0;
/// Neblina's west row from Senda Parkway round to the bend, in [`senda_houses`]' rows,
/// its lots' spacing and its houses' width: between Senda Parkway and the park at the
/// bend, some round the curve's outside, the first on the corner of Senda Parkway, its
/// houses the tract's narrower plan (10.9 m), each a side yard from the next.
const NEBLINA_WEST_ROW: usize = 3;
/// The lot of Neblina's west row (counted from the bend park) left open for the paseo, a
/// walled walk from the spawn field behind the row to Neblina's sidewalk, as the tract's
/// walks to its parks run between two lots; and how wide (m) the walk runs between its
/// walls.
const NEBLINA_PASEO_LOT: usize = 3;
pub const PASEO_WIDTH: f64 = 6.6;
/// Gaviota's north row in [`senda_houses`]' rows, its lots' spacing and the most (m)
/// its fronts step back. It holds about as many houses as Rumbo's south row across
/// the greenbelt, but it starts round the inside of Neblina's bend, a lot
/// further east, and ends as short of its turning circle's far rim. So its houses are as
/// narrow as Neblina's west row's, each a side yard from the next, and stand square to
/// Gaviota even round the bend, where turned ones would close in on each other at the
/// back and stand a lot apart at the front; and they step back only a little, since
/// their back yards meet the greenbelt where it is narrowest.
const GAVIOTA_NORTH_ROW: usize = 2;
const GAVIOTA_NORTH_PITCH: f64 = WEST_ROW_WIDTH + SIDE_YARD;
const GAVIOTA_NORTH_STEP: f64 = 0.2;
const WEST_ROW_PITCH: f64 = WEST_ROW_WIDTH + SIDE_YARD;
pub const WEST_ROW_WIDTH: f64 = 5.8;
/// Where along its street a row's first lot stands (m from the street's start).
const FIRST_LOT: f64 = 3.0;
/// How much nearer (m) than a lot's pitch two houses of a row may stand, for rounding.
const PITCH_TOLERANCE: f64 = 0.01;
/// Wall plate heights (m), varied lot by lot so the rows do not read as copies: two
/// storeys of 1.6 m (the tract's eaves stand 6 m up, about half the house's width) under
/// a tile roof whose ridge rises over them by half the house's width at
/// [`GARAGE_HOUSE_PITCH`].
const WALL_PLATES: [f64; 4] = [3.2, 3.15, 3.25, 3.2];
/// The highest wall plate (m) of a house on a narrower lot.
const NARROW_PLATE: f64 = 3.2;
/// Room (m) a house keeps from a turning circle's sidewalk it does not face, from a spawn
/// pad's rim, from a pickup's centre, and from the arena's edge.
const STREET_ROOM: f64 = 0.4;
/// The room (m) a corner lot's house keeps from the side street's sidewalk, past its
/// side yard; Rumbo's corner lot on its north side stands back past a broader planted
/// one toward Neblina, its front on Rumbo.
const SIDE_STREET_ROOM: f64 = 2.0;
const CORNER_LOT_ROOM: f64 = 2.5;
const SPAWN_ROOM: f64 = 2.5;
/// A spawn pad's radius (its rim's outer edge).
const SPAWN_PAD: f64 = 2.3;
const PICKUP_ROOM: f64 = 2.5;
const EDGE_ROOM: f64 = 3.0;
/// The rows run on round the turning circle at a street's end, as the tract's do: where a
/// lot of the row would stand nearer the circle's middle than a house facing the circle
/// across its own front yard, its house stands round the circle instead, on a pie-shaped
/// lot, its front turned to the circle's middle. A row looks this far (m) on past the
/// circle's middle for such lots; the spawn pads and the level's wall end it sooner.
const CUL_DE_SAC_ROW_END: f64 = 40.0;

/// The colour of the level's wall where it is drawn as the tract's capped split-face
/// block, which its model knows it by.
pub const SENDA_WALL_COLOR: u32 = 0xc8b28c;
const TREE_COLOR: u32 = 0x3f7a3a;
/// The parkway trees' colour, by which their model knows them as the tract's sycamores in
/// autumn leaf ([`is_street_tree`]); a hit throws chips of it.
pub const STREET_TREE_COLOR: u32 = 0x9c8b5a;
/// The parkway trees, one before most lots: their footprint and height, the least room
/// (m) between two of them and between one and a driveway, and how often a lot keeps
/// none (every this many lots, one). The trunk stands in the parkway strip; the box,
/// which the tree's model takes its crown's width from, reaches a quarter metre over the
/// kerb and the sidewalk, since a box as narrow as the strip grows a crown under a metre
/// wide where the tract's are three to five (1.6 to 2.7 at the tank's scale). They stand
/// about as tall as the houses' ridges, as the tract's twenty-year-old trees do.
pub const STREET_TREE: f64 = PARKWAY + 0.5;
const STREET_TREE_HEIGHT: f64 = 4.5;
const STREET_TREE_ROOM: f64 = 2.5;
const STREET_TREE_DRIVEWAY_ROOM: f64 = 0.2;
const STREET_TREE_SKIP: usize = 4;
/// A driveway's width past its garage door's (m), as the scenery paves it.
pub const DRIVEWAY_PAST_DOOR: f64 = 0.15;
/// How far (m) a Senda Parkway tree's trunk keeps inside its parkway's edges, and a
/// residential street tree's inside its narrower strip's.
const TREE_KERB_ROOM: f64 = 0.25;
const STREET_TREE_KERB_ROOM: f64 = 0.15;

/// An aerial pixel's place in the arena.
pub fn from_aerial((px, py): (f64, f64)) -> DVec2 {
    let (sin, cos) = AERIAL_TURN_DEGREES.to_radians().sin_cos();
    let (x, y) = (px * cos - py * sin, px * sin + py * cos);
    DVec2::new(
        (x - AERIAL_CENTRE.0) * ALONG_PER_PIXEL,
        (y - AERIAL_CENTRE.1) * ACROSS_PER_PIXEL,
    )
}

/// A street's centre line through the arena.
#[derive(Clone, Debug)]
pub struct Line(pub Vec<DVec2>);

impl Line {
    fn traced(trace: &[(f64, f64)]) -> Self {
        Line(trace.iter().copied().map(from_aerial).collect())
    }

    pub fn length(&self) -> f64 {
        self.0
            .windows(2)
            .map(|pair| pair[0].distance(pair[1]))
            .sum()
    }

    /// The point `s` metres along the line and the line's direction there.
    pub fn at(&self, s: f64) -> (DVec2, DVec2) {
        let mut rest = s.max(0.0);
        for pair in self.0.windows(2) {
            let span = pair[0].distance(pair[1]);
            let direction = (pair[1] - pair[0]) / span;
            if rest <= span {
                return (pair[0] + direction * rest, direction);
            }
            rest -= span;
        }
        let n = self.0.len();
        let direction = (self.0[n - 1] - self.0[n - 2]).normalize();
        (self.0[n - 1], direction)
    }

    /// The nearest distance from `p` to the line.
    pub fn distance(&self, p: DVec2) -> f64 {
        self.0
            .windows(2)
            .map(|pair| {
                let (a, b) = (pair[0], pair[1]);
                let u = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
                p.distance(a + (b - a) * u)
            })
            .fold(f64::INFINITY, f64::min)
    }

    /// The distance from the line to `p`, positive to its left and negative to its
    /// right (by its nearest span).
    pub fn signed_distance(&self, p: DVec2) -> f64 {
        let mut best = (f64::INFINITY, 0.0);
        for pair in self.0.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let u = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
            let offset = p - (a + (b - a) * u);
            let gap = offset.length();
            if gap < best.0 {
                best = (gap, gap.copysign(offset.dot(left(b - a))));
            }
        }
        best.1
    }

    /// How far along the line its nearest point to `p` lies.
    pub fn along(&self, p: DVec2) -> f64 {
        let mut best = (f64::INFINITY, 0.0);
        let mut start = 0.0;
        for pair in self.0.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let span = a.distance(b);
            let u = ((p - a).dot(b - a) / (span * span)).clamp(0.0, 1.0);
            let gap = p.distance(a + (b - a) * u);
            if gap < best.0 {
                best = (gap, start + u * span);
            }
            start += span;
        }
        best.1
    }

    /// The line `offset` metres to its left (positive) or right, joint by joint.
    pub fn offset(&self, offset: f64) -> Vec<DVec2> {
        let n = self.0.len();
        (0..n)
            .map(|i| {
                let before = self.0[i.saturating_sub(1)];
                let after = self.0[(i + 1).min(n - 1)];
                let along = (after - before).normalize();
                self.0[i] + left(along) * offset
            })
            .collect()
    }
}

/// The direction a quarter turn to the left of `along` (seen from above, +z south).
pub fn left(along: DVec2) -> DVec2 {
    DVec2::new(along.y, -along.x)
}

/// The neighbourhood's streets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Street {
    SendaParkway,
    /// Neblina down from Senda Parkway, sweeping round into Gaviota.
    NeblinaGaviota,
    Rumbo,
}

impl Street {
    pub const ALL: [Street; 3] = [Street::SendaParkway, Street::NeblinaGaviota, Street::Rumbo];

    pub fn line(self) -> Line {
        Line::traced(match self {
            Street::SendaParkway => SENDA_PARKWAY_TRACE,
            Street::NeblinaGaviota => NEBLINA_GAVIOTA_TRACE,
            Street::Rumbo => RUMBO_TRACE,
        })
    }

    /// Half its width kerb to kerb.
    pub fn half(self) -> f64 {
        match self {
            Street::SendaParkway => SENDA_PARKWAY_HALF,
            _ => STREET_HALF,
        }
    }

    /// Its cul-de-sac's middle, for the streets that end in one.
    pub fn cul_de_sac(self) -> Option<DVec2> {
        let (traced, shift, trace) = match self {
            Street::Rumbo => (RUMBO_CUL_DE_SAC_TRACE, RUMBO_CUL_DE_SAC_SHIFT, RUMBO_TRACE),
            Street::NeblinaGaviota => (
                GAVIOTA_CUL_DE_SAC_TRACE,
                GAVIOTA_CUL_DE_SAC_SHIFT,
                NEBLINA_GAVIOTA_TRACE,
            ),
            Street::SendaParkway => return None,
        };
        let n = trace.len();
        let heading = (from_aerial(trace[n - 1]) - from_aerial(trace[n - 2])).normalize();
        Some(from_aerial(traced) - heading * shift)
    }
}

/// Whether a house or lot centred at `at` and facing `facing` stands round the turning
/// circle at `middle`, its front to the circle's middle across its front yard.
pub fn faces_circle(at: DVec2, facing: DVec2, middle: DVec2) -> bool {
    let reach = CUL_DE_SAC_RADIUS + PARKWAY + SIDEWALK + SETBACK + HOUSE_DEPTH + 1.0;
    let toward = middle - at;
    toward.length() < reach && facing.dot(toward.normalize()) > FACES_CIRCLE
}

/// The least cosine between a house's facing and the way to a turning circle's middle
/// for it to face the circle (about two degrees, its turn rounded to the degree).
const FACES_CIRCLE: f64 = 0.999;

/// How far (m) `p` stands outside the back edge of `street`'s sidewalk (negative on its
/// paving, parkway or sidewalk): on Senda Parkway's south side, its wide path's.
pub fn past_sidewalk(street: Street, p: DVec2) -> f64 {
    let line = street.line();
    let walk = if street == Street::SendaParkway && line.signed_distance(p) < 0.0 {
        WAY_PARKWAY + WAY_PATH
    } else {
        PARKWAY + SIDEWALK
    };
    line.distance(p) - street.half() - walk
}

/// The distance from a street's centre line to the fronts of the houses along it.
pub fn house_front() -> f64 {
    STREET_HALF + PARKWAY + SIDEWALK + SETBACK
}

/// How far (m) a lot runs on behind its house: the back yard with its patio, to the
/// back fence.
pub const BACK_YARD: f64 = 2.4;

/// The distance from a street's centre line to the back fences of the lots along it,
/// behind the furthest-set house.
pub fn back_fence() -> f64 {
    house_front() + STAGGER.into_iter().fold(0.0, f64::max) + HOUSE_DEPTH + BACK_YARD
}

/// How many times its design the level stands in the world. Senda is laid out in design
/// metres, the scale its streets were traced to (the standard arena's, at about half
/// real size like the tanks), and stands this much larger about the arena's middle, so
/// its houses, streets and trees stand larger beside the vehicles while they, the
/// camera and the effects keep their size. Every function of the level answers in
/// design metres; [`senda_layout`] scales its cover, the map declares the scale
/// (`ArenaMap::scale`), the scenery scales its root and a house's structure and model
/// are its design's scaled ([`HouseDesign::scale`]). A world position asks the level in
/// design metres through [`senda_design_point`].
pub const SENDA_SCALE: f64 = 1.25;

/// A world position `(x, z)` in the level's design metres ([`SENDA_SCALE`]).
pub fn senda_design_point(x: f64, z: f64) -> DVec2 {
    DVec2::new(x, z) / SENDA_SCALE
}

/// Whether `(x, z)` (design metres) lies on Senda's paving: a street with its sidewalks
/// or a cul-de-sac. Ground effects throw concrete grit there, not earth.
pub fn is_senda_paved(x: f64, z: f64) -> bool {
    let p = DVec2::new(x, z);
    Street::ALL.into_iter().any(|street| {
        street.line().distance(p) < street.half() + PARKWAY + SIDEWALK
            || street
                .cul_de_sac()
                .is_some_and(|middle| middle.distance(p) < CUL_DE_SAC_RADIUS)
    })
}

/// One house: its cover box centre, ridge and plan, and how far its front stands back
/// from the sidewalk (its driveway's length from the garage door to the sidewalk).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SendaHouse {
    pub x: f64,
    pub z: f64,
    /// Its width along its street, and its depth front to back ([`plan_depth`]); its
    /// lot is a full [`HOUSE_DEPTH`] deep ([`SendaHouse::lot_centre`]).
    pub w: f64,
    pub d: f64,
    pub h: f64,
    pub plan: HousePlan,
    pub setback: f64,
    /// The row it stands in, the side of the street it fronts.
    pub row: Row,
}

/// The last list of houses a layout question was asked about, with its answer. One
/// arena asks for the lots and the greenbelt's lane of the same houses a dozen times, and
/// each answer costs milliseconds; a room's clock runs while its simulation is built.
pub(super) struct LastAnswer<T>(std::sync::Mutex<Option<(Vec<SendaHouse>, T)>>);

impl<T: Clone> LastAnswer<T> {
    pub(super) const fn new() -> Self {
        LastAnswer(std::sync::Mutex::new(None))
    }

    /// The answer for `houses`: the kept one if it was theirs, or `work`'s, kept.
    pub(super) fn of(&self, houses: &[SendaHouse], work: impl FnOnce() -> T) -> T {
        let kept = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((asked, answer)) = kept.as_ref()
            && asked == houses
        {
            return answer.clone();
        }
        // Worked out with the store free: the lots ask for the lane.
        drop(kept);
        let answer = work();
        *self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
            Some((houses.to_vec(), answer.clone()));
        answer
    }
}

/// The neighbourhood's rows of houses, in the order [`senda_houses`] lays them out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Row {
    RumboNorth,
    RumboSouth,
    /// Gaviota's north side, with the houses on the inside of Neblina's curve.
    GaviotaNorth,
    /// Neblina's west side, from Senda Parkway round the outside of the curve.
    NeblinaWest,
    GaviotaSouth,
}

impl Row {
    pub const ALL: [Row; 5] = [
        Row::RumboNorth,
        Row::RumboSouth,
        Row::GaviotaNorth,
        Row::NeblinaWest,
        Row::GaviotaSouth,
    ];

    /// The street its houses front.
    pub fn street(self) -> Street {
        match self {
            Row::RumboNorth | Row::RumboSouth => Street::Rumbo,
            _ => Street::NeblinaGaviota,
        }
    }
}

/// A paseo: a walk between two lots of a row, from the open ground behind the row to the
/// back of its street's sidewalk, [`PASEO_WIDTH`] between its walls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paseo {
    /// Its middle where it leaves the ground behind the row, past the lots' back walls.
    pub field: DVec2,
    /// Its middle at the back of the sidewalk.
    pub sidewalk: DVec2,
}

impl Paseo {
    /// How far `p` lies from the walk's middle line, carried on across the sidewalk and
    /// parkway to the kerb.
    pub fn distance(&self, p: DVec2) -> f64 {
        let to_kerb = (self.sidewalk - self.field).normalize() * (PARKWAY + SIDEWALK);
        Line(vec![self.field, self.sidewalk + to_kerb]).distance(p)
    }
}

/// The paseos through Neblina's west row: wherever two of its houses stand a lot apart.
pub fn paseos(houses: &[SendaHouse]) -> Vec<Paseo> {
    let line = Street::NeblinaGaviota.line();
    let mut row: Vec<&SendaHouse> = houses
        .iter()
        .filter(|house| house.row == Row::NeblinaWest)
        .collect();
    row.sort_by(|a, b| {
        let along = |house: &SendaHouse| line.along(DVec2::new(house.x, house.z));
        along(a).total_cmp(&along(b))
    });
    row.windows(2)
        .filter_map(|pair| {
            let (a, b) = (pair[0], pair[1]);
            let (at_a, at_b) = (DVec2::new(a.x, a.z), DVec2::new(b.x, b.z));
            if at_a.distance(at_b) < 1.5 * WEST_ROW_PITCH {
                return None;
            }
            let middle = (at_a + at_b) / 2.0;
            let facing = (a.facing() + b.facing()).normalize();
            let mut sidewalk = middle;
            while past_sidewalk(Street::NeblinaGaviota, sidewalk) > 0.0 {
                sidewalk += facing * 0.05;
            }
            Some(Paseo {
                field: middle - facing * (HOUSE_DEPTH / 2.0 + BACK_YARD + PASEO_FIELD),
                sidewalk,
            })
        })
        .collect()
}

/// How far (m) past its lots' back walls a paseo's way runs out onto the field.
const PASEO_FIELD: f64 = 2.0;

impl SendaHouse {
    /// The way its front faces.
    pub fn facing(&self) -> DVec2 {
        let yaw = (f64::from(self.plan.street) * 90.0 + f64::from(self.plan.angle)).to_radians();
        DVec2::new(yaw.sin(), yaw.cos())
    }

    /// The house its plan builds.
    pub fn design(&self) -> HouseDesign {
        HouseDesign::planned(self.x, self.z, self.w, self.d, self.h, self.plan)
    }

    /// Its lot's centre, where a [`HOUSE_DEPTH`] house would stand: a shallower plan's
    /// house stands toward its street from it, its front where the lot puts every front,
    /// so a smaller house has a deeper back yard.
    pub fn lot_centre(&self) -> DVec2 {
        let yaw = (f64::from(self.plan.street) * 90.0 + f64::from(self.plan.angle)).to_radians();
        let facing = DVec2::new(yaw.sin(), yaw.cos());
        DVec2::new(self.x, self.z) - facing * (HOUSE_DEPTH - self.d) / 2.0
    }
}

/// How deep (m) each floor plan's house is: the large plan fills the lot's depth
/// ([`HOUSE_DEPTH`]); the medium and small are shallower, their fronts where the large
/// plan's stands, so a smaller house leaves a deeper back yard.
pub fn plan_depth(plan: Plan) -> f64 {
    match plan {
        Plan::Small => 7.4,
        Plan::Medium => 7.7,
        Plan::Large => HOUSE_DEPTH,
    }
}

/// A house on the lot centred at `at` facing `facing`: its plan's quarter turns and the
/// whole degrees past them, its place rounded to the centimetre so a room's copy is built
/// the same; a shallower plan stands toward its street, its front where the lot's is.
fn house(footprint: &Footprint, lot: usize, row: usize, setback: f64) -> SendaHouse {
    let Footprint { at, facing, w } = *footprint;
    let (style, garage) = senda_lot(row, lot);
    let d = plan_depth(style_design(style).0);
    let at = at + facing * (HOUSE_DEPTH - d) / 2.0;
    let yaw = facing.x.atan2(facing.y).to_degrees();
    let quarter = (yaw / 90.0).round();
    let street = (quarter as i32).rem_euclid(4) as i8;
    let angle = (yaw - quarter * 90.0).round() as i8;
    SendaHouse {
        x: (at.x * 100.0).round() / 100.0,
        z: (at.y * 100.0).round() / 100.0,
        w,
        d,
        h: WALL_PLATES[(lot + row) % WALL_PLATES.len()].min(if w < HOUSE_WIDTH {
            NARROW_PLATE
        } else {
            f64::INFINITY
        }) + GARAGE_HOUSE_PITCH * w / 2.0,
        plan: HousePlan {
            style,
            street: if street == 3 { -1 } else { street },
            angle,
            garage,
        },
        setback,
        row: Row::ALL[row],
    }
}

/// Whether a house centred at `at` and facing `facing` stands clear of every street's
/// paving (its own street's beyond its front yard), the cul-de-sacs, the spawn pads,
/// the pickups and the arena's edge, keeping `street_room` (m) off other streets'
/// sidewalks.
fn clear(footprint: &Footprint, own: Option<Street>, street_room: f64) -> bool {
    let Footprint { at, facing, w } = *footprint;
    let reach = HOUSE_DEPTH.hypot(w) / 2.0;
    let along = left(facing);
    let corners = footprint.corners(0.0);
    let off_streets = Street::ALL.into_iter().all(|street| {
        let line = street.line();
        if Some(street) == own {
            let room = street.half() + PARKWAY + SIDEWALK + SETBACK * 0.5;
            corners.iter().all(|&corner| line.distance(corner) > room)
        } else {
            corners
                .iter()
                .all(|&corner| past_sidewalk(street, corner) > street_room)
        }
    });
    let off_bulbs = Street::ALL
        .into_iter()
        .filter_map(Street::cul_de_sac)
        .all(|middle| {
            let room = CUL_DE_SAC_RADIUS + PARKWAY + SIDEWALK + STREET_ROOM;
            corners.iter().all(|&corner| middle.distance(corner) > room)
        });
    // Measured from the house's walls, so a house may stand level with the gap between
    // two pads, its back to the spawn lane, as long as each pad keeps its room.
    let off_spawns = [Team::Blue, Team::Red]
        .into_iter()
        .flat_map(|team| spawn_positions(team, 1.0))
        .all(|spawn| {
            let offset = DVec2::new(spawn.x, spawn.z) - at;
            let outside = DVec2::new(
                (offset.dot(along).abs() - w / 2.0).max(0.0),
                (offset.dot(facing).abs() - HOUSE_DEPTH / 2.0).max(0.0),
            );
            outside.length() > SPAWN_PAD + SPAWN_ROOM
        });
    let off_pickups = SENDA_PICKUPS
        .iter()
        .all(|p| DVec2::new(p.x, p.z).distance(at) > reach + PICKUP_ROOM);
    let inside = at.x.abs().max(at.y.abs()) < ARENA - reach - EDGE_ROOM;
    off_streets && off_bulbs && off_spawns && off_pickups && inside
}

/// A house's footprint: its centre and the way its front faces.
#[derive(Clone, Copy, Debug)]
struct Footprint {
    at: DVec2,
    facing: DVec2,
    /// Its width along its street.
    w: f64,
}

impl Footprint {
    /// Turned to the whole degree and placed to the centimetre, as [`house`] builds it,
    /// so the side yards measured on it are the built ones.
    fn rounded(self) -> Self {
        let yaw = self
            .facing
            .x
            .atan2(self.facing.y)
            .to_degrees()
            .round()
            .to_radians();
        Footprint {
            at: (self.at * 100.0).round() / 100.0,
            facing: DVec2::new(yaw.sin(), yaw.cos()),
            w: self.w,
        }
    }

    /// Its corners, grown by `grow` all round.
    fn corners(&self, grow: f64) -> [DVec2; 4] {
        let along = left(self.facing) * (self.w / 2.0 + grow);
        let back = self.facing * (HOUSE_DEPTH / 2.0 + grow);
        [
            self.at - along - back,
            self.at + along - back,
            self.at + along + back,
            self.at - along + back,
        ]
    }

    /// Whether the two stand closer than a side yard: they overlap (separating axes) or a
    /// corner of one stands nearer than that to a wall of the other.
    fn crowds(&self, other: &Footprint) -> bool {
        // Most pairs stand streets apart: further than their corners reach plus the yard
        // (and a centimetre for the rounding), no corner nears a wall and nothing overlaps.
        let reach = |footprint: &Footprint| (footprint.w / 2.0).hypot(HOUSE_DEPTH / 2.0);
        if self.at.distance(other.at) > reach(self) + reach(other) + SIDE_YARD + 0.01 {
            return false;
        }
        let (a, b) = (self.corners(0.0), other.corners(0.0));
        let to_walls = |p: DVec2, quad: &[DVec2; 4]| {
            (0..4)
                .map(|i| {
                    let (s, e) = (quad[i], quad[(i + 1) % 4]);
                    let u = ((p - s).dot(e - s) / (e - s).length_squared()).clamp(0.0, 1.0);
                    p.distance(s + (e - s) * u)
                })
                .fold(f64::INFINITY, f64::min)
        };
        let near = a.iter().any(|&p| to_walls(p, &b) < SIDE_YARD)
            || b.iter().any(|&p| to_walls(p, &a) < SIDE_YARD);
        let axes = [
            self.facing,
            left(self.facing),
            other.facing,
            left(other.facing),
        ];
        near || axes.into_iter().all(|axis| {
            let span = |corners: &[DVec2; 4]| {
                corners
                    .iter()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), c| {
                        let t = c.dot(axis);
                        (lo.min(t), hi.max(t))
                    })
            };
            let ((a0, a1), (b0, b1)) = (span(&a), span(&b));
            a0 < b1 && b0 < a1
        })
    }
}

/// How far along Neblina (from Senda Parkway) it bends east into Gaviota.
pub fn neblina_bend() -> f64 {
    Line::traced(&NEBLINA_GAVIOTA_TRACE[..=NEBLINA_BEND]).length()
}

/// The little lawn park with its paths on the outside of Neblina's curve, where the
/// name changes to Gaviota: the middle of the lot it takes between the last house of
/// Neblina's west row and the first of Gaviota's south row.
pub fn bend_park() -> DVec2 {
    let line = Street::NeblinaGaviota.line();
    let bend = neblina_bend();
    let along = (line.at(bend + LOT_PITCH).0 - line.at(bend - LOT_PITCH).0).normalize();
    line.at(bend).0 - left(along) * (house_front() + HOUSE_DEPTH / 2.0)
}

/// How far along Neblina (from Senda Parkway) Gaviota's north row begins: above it the
/// greenbelt opens onto Neblina.
pub fn greenbelt_mouth() -> f64 {
    neblina_bend() - GREENBELT_MOUTH
}

/// Every house: the rows along both sides of Rumbo, Neblina's east side from Rumbo
/// down round its bend into Gaviota's north side, Neblina's west side from the park on
/// the outside of the curve up to Senda Parkway, and Gaviota's south side from the park.
/// Rumbo's and Gaviota's rows run on round their turning circles, on pie-shaped lots
/// facing the circles' middles, till the spawn pads and the level's wall end them. Each
/// row runs on from lot to lot; where a lot is taken (a street, a spawn, a neighbour round
/// a bend) it looks a little further on. A house left with no neighbour in its row stands
/// alone on the field and is dropped. Neblina's west row leaves one lot open for the
/// paseo ([`NEBLINA_PASEO_LOT`]), the rest of its lots where they would stand anyway.
///
/// Laid out once a process: the search costs about 20 ms, and one arena asks for the
/// houses a dozen times (a room's clock runs while its simulation is built).
pub fn senda_houses() -> Vec<SendaHouse> {
    static HOUSES: std::sync::OnceLock<Vec<SendaHouse>> = std::sync::OnceLock::new();
    HOUSES.get_or_init(lay_out_houses).clone()
}

fn lay_out_houses() -> Vec<SendaHouse> {
    let mut houses: Vec<(SendaHouse, Footprint)> = Vec::new();
    // The lots laid out only to keep the row's places: the paseo's.
    let mut open_lots: Vec<usize> = Vec::new();
    let bend = neblina_bend();
    // Neblina's east side has houses only round its bend: above it the greenbelt
    // opens onto the street between Rumbo's corner house and Gaviota's, and above
    // Rumbo lies the slope.
    let greenbelt_mouth = greenbelt_mouth();
    // Each row: its street, which side (left of the way the street was traced), and
    // where along it the row starts and which way it runs. Neblina's west side and
    // Gaviota's south side run away either way from the little park on the outside of
    // the curve, where the name changes: west up Neblina to Senda Parkway, round the
    // curve's outside first, and south on along Gaviota to its cul-de-sac.
    let rows = [
        (Street::Rumbo, 1.0, FIRST_LOT, 1.0, CORNER_LOT_ROOM),
        (Street::Rumbo, -1.0, FIRST_LOT, 1.0, SIDE_STREET_ROOM),
        (
            Street::NeblinaGaviota,
            1.0,
            greenbelt_mouth,
            1.0,
            SIDE_STREET_ROOM,
        ),
        (Street::NeblinaGaviota, -1.0, bend, -1.0, SIDE_STREET_ROOM),
        (Street::NeblinaGaviota, -1.0, bend, 1.0, SIDE_STREET_ROOM),
    ];
    let park = bend_park();
    let gaviota = Street::NeblinaGaviota.line();
    let gaviota_heading = (gaviota.at(gaviota.length()).0 - gaviota.at(bend).0).normalize();
    for (row, &(street, side, start, way, street_room)) in rows.iter().enumerate() {
        let line = street.line();
        let length = line.length();
        // A row running toward its street's cul-de-sac looks on past the street's end
        // for lots round the turning circle.
        let (end, reach, bulb) = match street.cul_de_sac() {
            Some(middle) if way > 0.0 => {
                let (end, along) = line.at(length);
                let past = (middle - end).dot(along) + CUL_DE_SAC_ROW_END;
                (end, past.max(0.0), Some(middle))
            }
            _ => (line.at(length).0, 0.0, None),
        };
        let point_at = |s: f64| {
            if s <= length {
                line.at(s).0
            } else {
                end + line.at(length).1 * (s - length)
            }
        };
        let mut lot = 0;
        let mut s = start;
        let mut last: Option<DVec2> = None;
        // Neblina's west row has its own narrower lots that step deeper.
        let west = row == NEBLINA_WEST_ROW;
        let (width, pitch) = if west {
            (WEST_ROW_WIDTH, WEST_ROW_PITCH)
        } else if row == GAVIOTA_NORTH_ROW {
            (WEST_ROW_WIDTH, GAVIOTA_NORTH_PITCH)
        } else {
            (HOUSE_WIDTH, LOT_PITCH)
        };
        // Each house turns with its street over a lot either side, not with each kink
        // of its trace, so Neblina's diagonal west row shows its side yards as slanted
        // slots, as the real one does.
        let chord = |from: f64, to: f64| (point_at(to) - point_at(from)).normalize();
        // The house of lot `lot` that fits where the row has reached `s`, and its setback.
        let fit = |s: f64, lot: usize, last: Option<DVec2>, houses: &[(SendaHouse, Footprint)]| {
            let point = point_at(s);
            let along = chord(s - pitch, s + pitch);
            let out = left(along) * side;
            let staggers = if west {
                // A lot hemmed in behind (the spawn lane past the top of the row) takes
                // the row's shallow step where its deep one has no room.
                let deep = WEST_ROW_STAGGER[lot % WEST_ROW_STAGGER.len()];
                [deep, WEST_ROW_STAGGER.into_iter().fold(deep, f64::min)]
            } else {
                let step = STAGGER[(lot + row) % STAGGER.len()];
                if row == GAVIOTA_NORTH_ROW {
                    [step.min(GAVIOTA_NORTH_STEP); 2]
                } else {
                    [step; 2]
                }
            };
            staggers
                .map(|stagger| SETBACK + stagger)
                .into_iter()
                .find_map(|setback| {
                    let distance = house_front() - SETBACK + setback + HOUSE_DEPTH / 2.0;
                    let mut at = point + out * distance;
                    // Round the turning circle a house faces its middle across the same
                    // front yard, sidewalk and parkway as a house on a street, its lot one of
                    // the circle's pie-shaped ones.
                    let ring = CUL_DE_SAC_RADIUS + PARKWAY + SIDEWALK + setback + HOUSE_DEPTH / 2.0;
                    let round_bulb =
                        bulb.filter(|&middle| s > length || middle.distance(at) < ring);
                    let facing = match round_bulb {
                        Some(middle) => {
                            at = middle + (at - middle).normalize() * ring;
                            (middle - at).normalize()
                        }
                        None if row == GAVIOTA_NORTH_ROW => -left(gaviota_heading),
                        None => -out,
                    };
                    let footprint = Footprint {
                        at,
                        facing,
                        w: width,
                    }
                    .rounded();
                    // A lot's pitch is measured between the houses, so the lots fan out
                    // round the outside of a curve as the real ones do; the park takes a lot
                    // of its row's own.
                    let spaced = last
                        .is_none_or(|at| at.distance(footprint.at) > pitch - PITCH_TOLERANCE)
                        && footprint.at.distance(park) > pitch.max(PARK_LOT);
                    let room = houses.iter().all(|(_, other)| !other.crowds(&footprint));
                    (spaced && room && clear(&footprint, Some(street), street_room))
                        .then_some((footprint, setback))
                })
        };
        while s >= FIRST_LOT && s < length + reach {
            if fit(s, lot, last, &houses).is_some() {
                // The search steps coarsely; the lot stands at the first place it fits.
                let mut first = s;
                for step in 1..LOT_REFINE {
                    let back = s - way * LOT_SEARCH * step as f64 / LOT_REFINE as f64;
                    if fit(back, lot, last, &houses).is_none() {
                        break;
                    }
                    first = back;
                }
                let (footprint, setback) = fit(first, lot, last, &houses).expect("a lot that fits");
                if row == NEBLINA_WEST_ROW && lot == NEBLINA_PASEO_LOT {
                    open_lots.push(houses.len());
                }
                houses.push((house(&footprint, lot, row, setback), footprint));
                last = Some(footprint.at);
                lot += 1;
                s = first;
            }
            s += LOT_SEARCH * way;
        }
    }
    let neighboured = |at: DVec2| {
        houses
            .iter()
            .filter(|(_, other)| other.at.distance(at) < LOT_PITCH * 2.6)
            .count()
            > 1
    };
    houses
        .iter()
        .enumerate()
        .filter(|&(k, (_, footprint))| neighboured(footprint.at) && !open_lots.contains(&k))
        .map(|(_, &(house, _))| house)
        .collect()
}

/// The tree families a footprint's seed picks (`tree_proportions`) for the
/// neighbourhood's broadleaves: an oak, its rounded crown standing in for the coast live
/// oaks of the greenbelt and the slopes, and a birch, its pale slender trunk and light
/// open crown standing in for the parkways' young street trees.
const OAK: u32 = 3;
const BIRCH: u32 = 4;
/// How far (m) east of a wanted spot [`tree_site`] looks for one, in centimetre steps.
const SITE_SEARCH: usize = 400;

/// The spot nearest east of `(x, z)` where a tree comes up of `family`: a tree's family
/// follows from where it stands.
fn tree_site(x: f64, z: f64, family: u32) -> (f64, f64) {
    (0..SITE_SEARCH)
        .map(|step| (x + step as f64 * 0.01, z))
        .find(|&(x, z)| tree_proportions(x, z, 1.0, 1.0, 1.0).family == family)
        .unwrap_or((x, z))
}

/// The spot nearest east of `(x, z)` where a tree comes up an oak: the neighbourhood's
/// big trees are coast live oaks and sycamores, never the village's pines and birches.
pub fn oak_site(x: f64, z: f64) -> (f64, f64) {
    tree_site(x, z, OAK)
}

/// The parkway trees, small round-crowned broadleaves: one in the parkway strip before
/// most lots, between its driveway and its lot line (or, where that has no room, on the
/// front door's side), never before a driveway, wherever that is clear of the pickups,
/// the asphalt and the sidewalks of every street, the turning circles and the trees
/// already placed.
fn street_trees(houses: &[SendaHouse]) -> Vec<CoverDef> {
    let paseos = paseos(houses);
    let mut trees: Vec<CoverDef> = Vec::new();
    let lines: Vec<(Street, Line)> = Street::ALL.map(|street| (street, street.line())).into();
    let lots = super::senda_yards::senda_lots(houses);
    // Each lot's driveway, kerb to garage door, in its own frame: its span across and
    // from its house's front out to the kerb.
    let driveways: Vec<(f64, f64, f64)> = lots
        .iter()
        .map(|lot| {
            let design = lot.house.design();
            let plan = design.wall_plan(design.door_wall);
            let half = (plan.garage_width() + DRIVEWAY_PAST_DOOR) / 2.0;
            let garage = super::senda_yards::front_openings(lot).map_or(f64::NAN, |(_, g)| g);
            (
                garage - half,
                garage + half,
                lot.house.setback + SIDEWALK + PARKWAY,
            )
        })
        .collect();
    let off_driveways = |spot: DVec2| {
        let room = STREET_TREE / 2.0 + STREET_TREE_DRIVEWAY_ROOM;
        lots.iter()
            .zip(&driveways)
            .all(|(lot, &(low, high, length))| {
                let (u, v) = lot.local(spot);
                let v = v - lot.half_depth;
                u < low - room || u > high + room || v < -room || v > length + room
            })
    };
    for (k, lot) in lots.iter().enumerate() {
        let Some((door, garage)) = super::senda_yards::front_openings(lot) else {
            continue;
        };
        if k % STREET_TREE_SKIP == STREET_TREE_SKIP - 1 {
            continue;
        }
        let (low, high, _) = driveways[k];
        let side = (garage - door).signum();
        // The lot lines' places across its front, at the gates.
        let line = |side: f64| lot.local(lot.sides[usize::from(side > 0.0)].gate).0;
        let beyond = if side > 0.0 { high } else { low };
        let candidates = [(beyond + line(side)) / 2.0, (door + line(-side)) / 2.0];
        let street = match lot.house.row {
            Row::RumboNorth | Row::RumboSouth => Street::Rumbo,
            _ => Street::NeblinaGaviota,
        };
        let street_line = &lines[Street::ALL.iter().position(|&s| s == street).unwrap_or(0)].1;
        // A lot round a turning circle has its tree in the circle's parkway.
        let bulb = street
            .cul_de_sac()
            .filter(|&middle| faces_circle(lot.centre, lot.front, middle));
        // On a parkway: off every street's asphalt and sidewalks, and the circles'.
        let in_parkway = |spot: DVec2| {
            let off_kerb = |off: f64| {
                off > STREET_TREE_KERB_ROOM
                    && !(off > PARKWAY - STREET_TREE_KERB_ROOM
                        && off < PARKWAY + SIDEWALK + STREET_TREE_KERB_ROOM)
            };
            lines
                .iter()
                .all(|(street, line)| off_kerb(line.distance(spot) - street.half()))
                && Street::ALL
                    .into_iter()
                    .filter_map(Street::cul_de_sac)
                    .all(|middle| off_kerb(middle.distance(spot) - CUL_DE_SAC_RADIUS))
        };
        let Some(spot) = candidates.into_iter().find_map(|u| {
            // Straight out from the lot's front to its own street's parkway, or the
            // circle's.
            let front = lot.at(u, lot.half_depth);
            let wanted = match bulb {
                Some(middle) => {
                    middle + (front - middle).normalize() * (CUL_DE_SAC_RADIUS + PARKWAY / 2.0)
                }
                None => {
                    let (point, along) = street_line.at(street_line.along(front));
                    let out = left(along) * street_line.signed_distance(front).signum();
                    point + out * (street.half() + PARKWAY / 2.0)
                }
            };
            let (x, z) = tree_site(wanted.x, wanted.y, OAK);
            let spot = DVec2::new(x, z);
            (spot.distance(wanted) < 0.3 && off_driveways(spot) && in_parkway(spot)).then_some(spot)
        }) else {
            continue;
        };
        let (x, z) = (spot.x, spot.y);
        let crowded = SENDA_PICKUPS
            .iter()
            .any(|p| DVec2::new(p.x, p.z).distance(spot) < PICKUP_ROOM + 1.5)
            || trees
                .iter()
                .any(|tree| DVec2::new(tree.x, tree.z).distance(spot) < STREET_TREE_ROOM);
        if !crowded
            && in_arena(spot)
            && !at_junction(spot)
            && off_paseos(&paseos, spot, STREET_TREE)
        {
            trees.push(CoverDef::new(
                CoverKind::Tree,
                x,
                z,
                STREET_TREE,
                STREET_TREE,
                STREET_TREE_HEIGHT,
                80.0,
                STREET_TREE_COLOR,
            ));
        }
    }
    trees
}

/// Senda's wheelie bins out for collection: explosive drums drawn as a dark green
/// trash bin and a blue recycling bin ([`is_trash_bin`] picks the look by colour), their
/// footprint across and front to back and their height (m), and their health. A real
/// bin stands 1.1 m tall; these are that at the houses' scale, a little over so the
/// drum still reads from the play camera.
pub const TRASH_BIN_COLORS: [u32; 2] = [0x3e4b3c, 0x2d5c8a];
pub const TRASH_BIN: (f64, f64, f64) = (0.42, 0.47, 0.68);
const TRASH_BIN_HEALTH: f64 = 20.0;
/// One house in this many puts its bins out, starting with the first.
const TRASH_BIN_HOUSES: usize = 3;
/// Where (m) the bins stand: past the kerb into the street, and from the driveway's
/// edge on its side away from the front door, and apart from each other.
const TRASH_BIN_KERB_ROOM: f64 = 0.35;
const TRASH_BIN_DRIVEWAY_ROOM: f64 = 0.5;
const TRASH_BIN_GAP: f64 = 0.62;
/// The least distance (m) between two houses' bins, across the street too, so a tank
/// never has to weave between them.
const TRASH_BINS_APART: f64 = 7.0;

/// Whether a drum is one of Senda's wheelie bins.
pub fn is_trash_bin(color: u32) -> bool {
    TRASH_BIN_COLORS.contains(&color)
}

/// Whether a tree cover of `color` is one of the parkway trees.
pub fn is_street_tree(color: u32) -> bool {
    color == STREET_TREE_COLOR
}

/// The bins of every [`TRASH_BIN_HOUSES`]th house, a trash and a recycling bin side by
/// side in the street against the kerb beside its driveway, turned to face the street,
/// wherever both stand on a straight run of asphalt clear of the turning circles, the
/// spawn pads and the pickups.
fn trash_bins(houses: &[SendaHouse]) -> Vec<CoverDef> {
    let paseos = paseos(houses);
    let lines: Vec<(Street, Line)> = Street::ALL.map(|street| (street, street.line())).into();
    let (w, d, h) = TRASH_BIN;
    let mut bins = Vec::new();
    for house in houses.iter().step_by(TRASH_BIN_HOUSES) {
        let design = house.design();
        let (centre, out, along, _) = wall_frames(design.w, design.d)[design.door_wall];
        let plan = design.wall_plan(design.door_wall);
        let (Some(garage), Some(door)) = (plan.garage(), plan.door()) else {
            continue;
        };
        // Beside the driveway, on its side away from the front walk.
        let away = -(door - garage).signum();
        let edge = garage + away * (plan.garage_width() / 2.0 + TRASH_BIN_DRIVEWAY_ROOM);
        let out_world = design.to_world(out);
        let reach = house.setback + SIDEWALK + PARKWAY + TRASH_BIN_KERB_ROOM;
        let spots = [0.0, TRASH_BIN_GAP].map(|step| {
            let face = design.to_world(centre + along * (edge + away * step));
            DVec2::new(
                house.x + face.x + out_world.x * reach,
                house.z + face.z + out_world.z * reach,
            )
        });
        let clear = spots.iter().all(|&spot| {
            let at_kerb = lines.iter().any(|(street, line)| {
                let off = street.half() - line.distance(spot);
                (d / 2.0..TRASH_BIN_KERB_ROOM + d).contains(&off)
            });
            let off_circles = Street::ALL
                .into_iter()
                .filter_map(Street::cul_de_sac)
                .all(|middle| middle.distance(spot) > CUL_DE_SAC_RADIUS + w);
            at_kerb
                && off_circles
                && !at_junction(spot)
                && off_paseos(&paseos, spot, w)
                && in_arena(spot)
                && off_pads_and_pickups(spot, w)
                && bins.iter().all(|bin: &CoverDef| {
                    DVec2::new(bin.x, bin.z).distance(spot) > TRASH_BINS_APART
                })
        });
        if !clear {
            continue;
        }
        // Their fronts (the lid's lip) face the street, handles and wheels the house.
        let yaw = out_world.x.atan2(out_world.z).to_degrees();
        let quarter = (yaw / 90.0).round();
        let plan = HousePlan {
            style: 0,
            street: quarter as i8,
            angle: (yaw - quarter * 90.0).round() as i8,
            garage: GarageSide::Left,
        };
        for (spot, color) in spots.into_iter().zip(TRASH_BIN_COLORS) {
            bins.push(CoverDef {
                house_plan: Some(plan),
                ..CoverDef::new(
                    CoverKind::Drum,
                    (spot.x * 100.0).round() / 100.0,
                    (spot.y * 100.0).round() / 100.0,
                    w,
                    d,
                    h,
                    TRASH_BIN_HEALTH,
                    color,
                )
            });
        }
    }
    bins
}

/// The street lamps: a row lamp by the side yard of every [`LAMP_EVERY`]th house, and
/// Senda Parkway's tall ones one every [`WAY_LAMP_SPACING`] (m) along both its parkways;
/// how near (m) two may stand and how far (m) a pole keeps off a kerb; their heights (m).
const LAMP_EVERY: usize = 5;
const WAY_LAMP_SPACING: f64 = 2.0 * MEDIAN_TREE_SPACING;
const LAMP_ROOM: f64 = 8.0;
const LAMP_KERB_ROOM: f64 = 0.3;
pub const LAMP_HEIGHT: f64 = 2.8;
pub const WAY_LAMP_HEIGHT: f64 = 4.6;
/// A lamp inside the level is cover a tank cannot drive through: a pole this many metres
/// across that a shell or two brings down. Its colour marks its look (a row's lamp, a
/// tall one) and is its painted metal, which its scrap falls in.
pub const STREET_LAMP_COLORS: [u32; 2] = [0x2a3a2e, 0x2a3a2f];
const LAMP_POLE_BOX: f64 = 0.3;
const LAMP_HEALTH: f64 = 40.0;

/// A cover drawn as a street lamp: whether it is a tall one.
pub fn street_lamp_look(color: u32) -> Option<bool> {
    STREET_LAMP_COLORS
        .iter()
        .position(|&lamp| lamp == color)
        .map(|k| k == 1)
}

/// Where the street lamps stand, and whether each is one of Senda Parkway's tall ones:
/// those first, then the rows' lamps; each in a parkway strip ([`lamp_clear`]) and no row
/// lamp within [`LAMP_ROOM`] of one already standing (round a corner two rows' lamps, or
/// a row's and the Way's, would stand together).
pub fn lamp_sites() -> Vec<(DVec2, bool)> {
    let lines: Vec<(Street, Line)> = Street::ALL.map(|street| (street, street.line())).into();
    let mut sites: Vec<(DVec2, bool)> = Vec::new();
    for at in way_lamp_sites() {
        if lamp_clear(at, &lines) {
            sites.push((at, true));
        }
    }
    let houses = senda_houses();
    let paseos = paseos(&houses);
    for house in houses.iter().step_by(LAMP_EVERY) {
        let design = house.design();
        let (centre, out, along, half) = wall_frames(design.w, design.d)[design.door_wall];
        let reach = house.setback + SIDEWALK + PARKWAY / 2.0;
        let spot = design.to_world(centre + along * (half + SIDE_YARD / 2.0) + out * reach);
        let at = DVec2::new(house.x + spot.x, house.z + spot.z);
        let crowded = sites.iter().any(|(lamp, _)| lamp.distance(at) < LAMP_ROOM);
        if lamp_clear(at, &lines) && !crowded && off_paseos(&paseos, at, LAMP_POLE_BOX) {
            sites.push((at, false));
        }
    }
    sites
}

/// Whether a street lamp may stand at `at`: off every street's asphalt and kerb by
/// [`LAMP_KERB_ROOM`], off the turning circles, and off the residential streets'
/// sidewalks, so only in a parkway strip.
fn lamp_clear(at: DVec2, lines: &[(Street, Line)]) -> bool {
    let off_circles = Street::ALL
        .into_iter()
        .filter_map(Street::cul_de_sac)
        .all(|middle| middle.distance(at) > CUL_DE_SAC_RADIUS + LAMP_KERB_ROOM);
    off_circles
        && lines.iter().all(|(street, line)| {
            let off = line.distance(at) - street.half();
            let sidewalk = *street != Street::SendaParkway
                && (PARKWAY - 0.1..PARKWAY + SIDEWALK + 0.1).contains(&off);
            off > LAMP_KERB_ROOM && !sidewalk
        })
}

/// Where Senda Parkway's tall lamps stand, in the middle of both its parkways, short of the
/// level's wall where the Way meets it.
fn way_lamp_sites() -> Vec<DVec2> {
    let way = Street::SendaParkway.line();
    let half = Street::SendaParkway.half();
    let boundary = senda_boundary();
    let mut sites = Vec::new();
    let mut s = MEDIAN_TREE_SPACING / 4.0;
    while s < way.length() {
        let (point, along) = way.at(s);
        for (side, strip) in [(1.0, PARKWAY), (-1.0, WAY_PARKWAY)] {
            let site = point + left(along) * side * (half + strip / 2.0);
            if clear_of_wall(&boundary, site, WAY_WALL_ROOM) {
                sites.push(site);
            }
        }
        s += WAY_LAMP_SPACING;
    }
    sites
}

/// Whether the lamp at `at` stands inside the level, where it is cover.
pub fn lamp_is_cover(boundary: &[DVec2], at: DVec2) -> bool {
    in_arena(at) && inside_boundary(boundary, at)
}

/// The lamps inside the level, as cover.
fn street_lamp_covers(boundary: &[DVec2]) -> Vec<CoverDef> {
    lamp_sites()
        .into_iter()
        .filter(|&(at, _)| lamp_is_cover(boundary, at))
        .map(|(at, tall)| {
            let height = if tall { WAY_LAMP_HEIGHT } else { LAMP_HEIGHT };
            CoverDef::new(
                CoverKind::Concrete,
                (at.x * 100.0).round() / 100.0,
                (at.y * 100.0).round() / 100.0,
                LAMP_POLE_BOX,
                LAMP_POLE_BOX,
                height,
                LAMP_HEALTH,
                STREET_LAMP_COLORS[usize::from(tall)],
            )
        })
        .collect()
}

/// Senda Parkway's trees, one every this many metres down its median and along the
/// parkways either side, half a spacing on; their footprint and height, and how far (m)
/// a trunk may stand off the median's middle or its parkway's.
pub const MEDIAN_TREE_SPACING: f64 = 9.0;
const MEDIAN_TREE: f64 = 1.4;
const MEDIAN_TREE_HEIGHT: f64 = 5.6;
const MEDIAN_TRUNK_ROOM: f64 = 0.4;
/// The groundcover strip between Senda Parkway's south kerb and its wide path, which
/// carries that side's trees and lamps, and the path's width (m).
pub const WAY_PARKWAY: f64 = 2.2;
pub const WAY_PATH: f64 = 2.0;

/// Senda Parkway's planted median opens across Neblina's mouth for the turn into it, its
/// noses this far (m) either side of Neblina's kerbs.
const MEDIAN_OPENING_ROOM: f64 = 1.5;
/// The kerbed splitter island in Neblina's mouth, on its centre line: how far (m) down
/// Neblina from Senda Parkway's south kerb it starts, its length, and its width at its
/// round end, from which it tapers to a point down Neblina.
pub const SPLITTER_ISLAND: (f64, f64, f64) = (1.0, 6.0, 1.2);

/// Whether `p`, or anything within `room` (m) of it, lies in the opening of Senda Parkway's
/// median across Neblina's mouth.
pub fn in_median_opening(p: DVec2, room: f64) -> bool {
    let neblina = Street::NeblinaGaviota.line();
    neblina.along(p) < Street::SendaParkway.half() * 2.0
        && neblina.distance(p) < STREET_HALF + MEDIAN_OPENING_ROOM + room
}

/// The splitter island in Neblina's mouth: how far along Neblina (from its start on
/// Senda Parkway's middle) it begins and ends.
pub fn splitter_island() -> (f64, f64) {
    let (neblina, way) = (Street::NeblinaGaviota.line(), Street::SendaParkway.line());
    let mut kerb = 0.0;
    while way.distance(neblina.at(kerb).0) < Street::SendaParkway.half() {
        kerb += 0.1;
    }
    let (inside, length, _) = SPLITTER_ISLAND;
    (kerb + inside, kerb + inside + length)
}

/// Where Senda Parkway's trees stand: every [`MEDIAN_TREE_SPACING`] down its median and,
/// half a spacing on, in the parkway either side, each at the nearest spot where a young
/// birch comes up that stays on its strip.
pub fn way_tree_sites() -> Vec<DVec2> {
    let way = Street::SendaParkway.line();
    let half = Street::SendaParkway.half();
    let mut sites = Vec::new();
    let mut s = MEDIAN_TREE_SPACING / 2.0;
    while s < way.length() {
        let (point, _) = way.at(s);
        // Young slender trees down the narrow median, oaks along the parkways.
        let (x, z) = tree_site(point.x, point.y, BIRCH);
        let site = DVec2::new(x, z);
        if way.distance(site) < MEDIAN_TRUNK_ROOM && !in_median_opening(site, MEDIAN_TREE) {
            sites.push(site);
        }
        let (point, along) = way.at(s + MEDIAN_TREE_SPACING / 2.0);
        // North the narrow parkway before its sidewalk, south the groundcover strip, with
        // young round-crowned street trees, as the Way's are.
        for (side, strip) in [(1.0, PARKWAY), (-1.0, WAY_PARKWAY)] {
            let middle = half + strip / 2.0;
            let wanted = point + left(along) * side * middle;
            let (x, z) = tree_site(wanted.x, wanted.y, BIRCH);
            let site = DVec2::new(x, z);
            let room = (strip / 2.0 - TREE_KERB_ROOM).max(MEDIAN_TRUNK_ROOM);
            if (way.distance(site) - middle).abs() < room {
                sites.push(site);
            }
        }
        s += MEDIAN_TREE_SPACING;
    }
    sites
}

/// Whether a tree at `at` stands inside the arena, where it is cover; outside it the
/// scenery draws it.
pub fn in_arena(at: DVec2) -> bool {
    at.x.abs().max(at.y.abs()) < ARENA - 2.0
}

/// How far (m) past a street's kerb a corner keeps clear of parkway trees and bins: the
/// junction's sight triangle, and room for a tank to turn the corner between them.
const JUNCTION_ROOM: f64 = 6.0;

/// Whether a cover `size` (m) across at `at` keeps out of every paseo and its way out
/// across the sidewalk and parkway to the kerb.
fn off_paseos(paseos: &[Paseo], at: DVec2, size: f64) -> bool {
    paseos
        .iter()
        .all(|paseo| paseo.distance(at) > PASEO_WIDTH / 2.0 + size / 2.0)
}

/// Whether `at` lies at a junction: within [`JUNCTION_ROOM`] of two streets' kerbs.
fn at_junction(at: DVec2) -> bool {
    Street::ALL
        .into_iter()
        .filter(|street| street.line().distance(at) - street.half() < JUNCTION_ROOM)
        .count()
        > 1
}

/// Whether a tree of footprint `size` at `at` keeps clear of the spawn pads and the
/// pickups.
fn off_pads_and_pickups(at: DVec2, size: f64) -> bool {
    let off_pads = [Team::Blue, Team::Red]
        .into_iter()
        .flat_map(|team| spawn_positions(team, 1.0))
        .all(|spawn| DVec2::new(spawn.x, spawn.z).distance(at) > SPAWN_PAD + SPAWN_ROOM + size);
    let off_pickups = SENDA_PICKUPS
        .iter()
        .all(|p| DVec2::new(p.x, p.z).distance(at) > PICKUP_ROOM + size);
    off_pads && off_pickups
}

/// Senda Parkway's trees that stand inside the wall, clear of the spawn pads and pickups
/// and short of the wall.
fn way_trees(boundary: &[DVec2]) -> Vec<CoverDef> {
    way_tree_sites()
        .into_iter()
        .filter(|&at| {
            in_arena(at)
                && inside_boundary(boundary, at)
                && clear_of_wall(boundary, at, WAY_WALL_ROOM)
                && off_pads_and_pickups(at, MEDIAN_TREE)
        })
        .map(|at| {
            CoverDef::new(
                CoverKind::Tree,
                at.x,
                at.y,
                MEDIAN_TREE,
                MEDIAN_TREE,
                MEDIAN_TREE_HEIGHT,
                80.0,
                TREE_COLOR,
            )
        })
        .collect()
}

/// Senda Parkway's trees the scenery draws: outside the wall or past the arena's edge,
/// where the Way runs on outside the level, short of the wall.
pub fn outer_way_tree_sites() -> Vec<DVec2> {
    let boundary = senda_boundary();
    way_tree_sites()
        .into_iter()
        .filter(|&at| {
            (!in_arena(at) || !inside_boundary(&boundary, at))
                && clear_of_wall(&boundary, at, WAY_WALL_ROOM)
        })
        .collect()
}

/// The greenbelt's middle line between the back fences of Rumbo's south row and
/// Gaviota's north row, a point a metre along Gaviota, over the longest stretch where
/// the greenbelt keeps `room` (m) from both streets' lots.
pub fn greenbelt_line(room: f64) -> Vec<DVec2> {
    let (gaviota, rumbo) = (Street::NeblinaGaviota.line(), Street::Rumbo.line());
    let lots = house_front() + HOUSE_DEPTH + room;
    let mut best: Vec<DVec2> = Vec::new();
    let mut run: Vec<DVec2> = Vec::new();
    let mut s = 0.0;
    while s < gaviota.length() {
        let (point, along) = gaviota.at(s);
        let out = left(along);
        // Halfway to Rumbo across the greenbelt, found by halving the difference.
        let mut reach = lots;
        for _ in 0..8 {
            reach = (reach + rumbo.distance(point + out * reach)) / 2.0;
        }
        let at = point + out * reach;
        if gaviota.distance(at).min(rumbo.distance(at)) > lots && at.y.abs() < ARENA * 1.5 {
            run.push(at);
        } else {
            if run.len() > best.len() {
                best = std::mem::take(&mut run);
            }
            run.clear();
        }
        s += 1.0;
    }
    if run.len() > best.len() {
        best = run;
    }
    best
}

/// The greenbelt's trees, willows and oaks along its middle, one every this many metres
/// either side of it by turns, how far off the middle, and their heights.
const GREENBELT_TREE_SPACING: f64 = 24.0;
const GREENBELT_TREE_OFFSETS: [f64; 3] = [4.2, 5.0, 6.0];
/// The neighbourhood's bigger trees (the greenbelt's, the bank's, the slope band's and
/// the cul-de-sac islands'), as the tract's: crowns 5-8 m across and 9-11 m tall at the
/// tank's scale, a little over a house's ridge. Their heights in turn, and their
/// footprints (an oak's crown spreads to about 1.2-1.45 times it).
const BIG_TREE_HEIGHTS: [f64; 3] = [5.2, 4.8, 5.5];
const BIG_TREE: f64 = 2.4;
const BANK_TREES: [f64; 3] = [2.3, 2.6, 2.4];
/// The bank behind Rumbo's north row: a clump of small trees behind every
/// [`BANK_CLUMP_EVERY`]th lot from the corner at Neblina, scattered on its mulch,
/// each tree along the row from the lot's middle and behind its back wall (m), by turns
/// of the clumps' shapes; a tree whose spot is taken moves this far (m) along the row
/// either way.
const BANK_CLUMP_EVERY: usize = 2;
const BANK_CLUMPS: [&[(f64, f64)]; 3] = [
    &[(-1.2, 1.9), (1.6, 3.4), (-0.3, 6.0)],
    &[(0.9, 2.6), (-1.5, 4.9)],
    &[(-0.6, 3.0), (1.8, 5.6)],
];
const BANK_TREE_SHIFT: f64 = 2.2;
/// The band of oaks and peppers on the top of the slope down to Gardenia, behind the back
/// walls of Gaviota's south row: one behind each lot line, this far (m) past the walls,
/// their crowns over the gardens' corners, wherever it leaves a tank's lane (m) along the
/// level's wall.
const BACK_TREE_REACH: f64 = 1.5;
pub const SOUTH_LANE: f64 = 4.5;

/// The trees on the bank behind Rumbo's north row ([`BANK_CLUMPS`]): where each wants to
/// stand, its footprint and height.
fn bank_trees(houses: &[SendaHouse]) -> Vec<(Vec<DVec2>, f64, f64)> {
    let rumbo = Street::Rumbo.line();
    let mut row: Vec<_> = super::senda_yards::senda_lots(houses)
        .into_iter()
        .filter(|lot| lot.house.row == Row::RumboNorth)
        .collect();
    row.sort_by(|a, b| rumbo.along(a.centre).total_cmp(&rumbo.along(b.centre)));
    let mut wanted = Vec::new();
    for (k, lot) in row.iter().step_by(BANK_CLUMP_EVERY).enumerate() {
        let rear = lot.rear_v();
        for (n, &(u, reach)) in BANK_CLUMPS[k % BANK_CLUMPS.len()].iter().enumerate() {
            let turn = k * 3 + n;
            wanted.push((
                [0.0, BANK_TREE_SHIFT, -BANK_TREE_SHIFT]
                    .map(|shift| lot.at(u + shift, rear - reach))
                    .into(),
                BANK_TREES[turn % BANK_TREES.len()],
                BIG_TREE_HEIGHTS[turn % BIG_TREE_HEIGHTS.len()],
            ));
        }
    }
    wanted
}

/// The greenbelt's trees, the bank's and the band behind Gaviota's south row: oaks, at
/// spots clear of the spawn pads, the pickups, the paving, the lots and each other, the
/// band a lane short of the level's wall at `boundary`.
fn big_trees(boundary: &[DVec2], houses: &[SendaHouse]) -> Vec<CoverDef> {
    // Each tree's spots in order of preference, its footprint and its height.
    let mut wanted: Vec<(Vec<DVec2>, f64, f64)> = Vec::new();
    // The greenbelt's trees stand either side of its lane, by turns, clear of it.
    let lane = Line(super::senda_yards::greenbelt_lane_of(houses));
    if lane.0.len() > 1 {
        let mut s = GREENBELT_TREE_SPACING / 2.0;
        let mut k = 0;
        while s < lane.length() {
            let (point, along) = lane.at(s);
            let side = if k % 2 == 0 { 1.0 } else { -1.0 };
            let height = BIG_TREE_HEIGHTS[k % BIG_TREE_HEIGHTS.len()];
            wanted.push((
                [side, -side]
                    .into_iter()
                    .flat_map(|side| {
                        GREENBELT_TREE_OFFSETS.map(|offset| point + left(along) * side * offset)
                    })
                    .collect(),
                BIG_TREE,
                height,
            ));
            s += GREENBELT_TREE_SPACING;
            k += 1;
        }
    }
    wanted.extend(bank_trees(houses));
    for (k, at) in behind_gaviota_south(BACK_TREE_REACH)
        .into_iter()
        .enumerate()
    {
        let height = BIG_TREE_HEIGHTS[k % BIG_TREE_HEIGHTS.len()];
        if clear_of_wall(boundary, at, BIG_TREE / 2.0 + SOUTH_LANE) {
            wanted.push((vec![at], BIG_TREE, height));
        }
    }
    let way = Street::SendaParkway.line();
    let lots = super::senda_yards::senda_lots(houses);
    let mut trees: Vec<CoverDef> = Vec::new();
    for (spots, size, height) in wanted {
        let clear = |at: DVec2| {
            // Off the paving and Senda Parkway's wide path along its south side.
            let off_way =
                way.distance(at) > Street::SendaParkway.half() + WAY_PARKWAY + WAY_PATH + 1.0;
            in_arena(at)
                && off_pads_and_pickups(at, size)
                && off_way
                && !is_senda_paved(at.x, at.y)
                && (lane.0.len() < 2
                    || lane.distance(at) > super::senda_yards::LANE_ROOM + size / 2.0 * 1.5)
                && lots.iter().all(|lot| lot.inside(at) < -size / 2.0)
                && trees.iter().all(|tree| {
                    DVec2::new(tree.x, tree.z).distance(at) > (tree.w + size) / 2.0 * 1.1
                })
        };
        let found = spots.into_iter().find_map(|at| {
            let (x, z) = oak_site(at.x, at.y);
            clear(DVec2::new(x, z)).then_some((x, z))
        });
        if let Some((x, z)) = found {
            trees.push(CoverDef::new(
                CoverKind::Tree,
                x,
                z,
                size,
                size,
                height,
                80.0,
                TREE_COLOR,
            ));
        }
    }
    trees
}

/// The tract's wall: its height (m), and how far (m) from Gaviota's centre line its south
/// run stands behind the lots of Gaviota's south row, across the planted top of the
/// slope down to Gardenia with its one band of trees, a tank's lane between them and the
/// wall (where it has stood since the level was laid out, whatever depth the lots take).
const WALL_HEIGHT: f64 = 1.8;
const SOUTH_WALL_FROM_STREET: f64 = 27.4;
/// The least distance (m) south of the arena's middle the south run keeps behind the bend
/// park and the row's west end, where the row stands furthest north: the bots' southern
/// flank points (38 m south) stay well inside it.
const SOUTH_WALL_LEAST: f64 = 40.0;
/// How far (m) from Senda Parkway's middle the wall runs along its north side: past the
/// north sidewalk and a planted strip, at the top of the bank up to the neighbouring
/// tract.
const WAY_NORTH_WALL: f64 = SENDA_PARKWAY_HALF + 5.0;
/// How far (m) from Senda Parkway's middle the wall runs along the top of the bank on its
/// south side, past the wide path and a planted strip, where the Way's north-eastern
/// stretch lies outside the level.
const WAY_SOUTH_WALL: f64 = SENDA_PARKWAY_HALF + WAY_PARKWAY + WAY_PATH + 1.0;
/// Where the wall crosses Senda Parkway, square to it: where the Way's north verge comes
/// within this far (m) of the arena's north edge.
const WAY_CROSSING_EDGE: f64 = 0.2;
/// How far (m) from a corner spawn pad's middle the wall curves round it, and the most
/// each of its runs turns (degrees), so the wall bends round the pad rather than boxing
/// it in.
const PAD_WALL_ROUND: f64 = 7.0;
const PAD_WALL_STEP: f64 = 15.0;
/// How far (m) short of that circle the wall leaves its line, to meet the circle where it
/// touches it, so its first turn toward the pad is a gentle one.
const PAD_WALL_LEAD: f64 = 4.5;
/// How far (m) from the wall's line Senda Parkway's trees and lamps keep, so its planting
/// ends short of the wall either side of it rather than standing in it.
pub const WAY_WALL_ROOM: f64 = 3.0;
/// How far (m) short of each corner where the wall crosses Senda Parkway it leaves its line
/// to come round the corner in short runs, so neither the Way's paving nor its north
/// sidewalk carries a slanting run, and the most (degrees) each of those runs turns.
const CROSSING_ROUND: f64 = 2.0;
const CROSSING_STEP: f64 = 30.0;
/// The shortest run (m) the wall is built in: a point of its line nearer the one before
/// is passed over.
const SHORTEST_RUN: f64 = 1.0;
/// The longest piece (m) a slanting run is built in, so its box's bounds on the world's
/// axes, which the spawn and pickup clearance checks measure, stay near its line.
const LONGEST_SLANTING_RUN: f64 = 10.0;

/// The point `length` metres from `from` toward `to`, turned to the nearest whole degree:
/// a turned wall run's box turns by whole degrees, so its ends meet its neighbours'.
fn whole_degree_run(from: DVec2, to: DVec2) -> DVec2 {
    let heading = (to - from).to_angle().to_degrees().round().to_radians();
    from + DVec2::from_angle(heading) * from.distance(to)
}

/// The wall's runs through `points` in turn from the first, each turned to whole degrees,
/// passing over a point nearer the last than [`SHORTEST_RUN`]; the last run ends where it
/// meets the arena's edge at `x`.
fn whole_degree_chain(points: &[DVec2], x: f64) -> Vec<DVec2> {
    let n = points.len();
    let points: Vec<DVec2> = points
        .iter()
        .enumerate()
        .filter(|&(k, p)| k == 0 || k + 1 == n || p.distance(points[k - 1]) >= SHORTEST_RUN)
        .map(|(_, &p)| p)
        .collect();
    let mut chain = vec![points[0]];
    let mut last = points[0];
    for (k, &next) in points.iter().enumerate().skip(1) {
        if k + 1 == points.len() {
            let heading = (next - last).to_angle().to_degrees().round().to_radians();
            let direction = DVec2::from_angle(heading);
            last += direction * ((x - last.x) / direction.x);
        } else {
            last = whole_degree_run(last, next);
        }
        // A slanting run is built in even pieces of at most the longest run.
        let from = chain[chain.len() - 1];
        let slanting = (last.x - from.x).abs().min((last.y - from.y).abs()) > 1e-6;
        let pieces = if slanting {
            (from.distance(last) / LONGEST_SLANTING_RUN).ceil().max(1.0) as usize
        } else {
            1
        };
        chain.extend((1..pieces).map(|k| from.lerp(last, k as f64 / pieces as f64)));
        chain.push(last);
    }
    chain
}

/// Rounds the wall's corner at `points[corner]`: it leaves its line `tangent` metres short
/// of the corner and comes round in runs that each turn at most [`CROSSING_STEP`] to
/// meet the next line as far past it. Points of the line before it nearer the corner than
/// that are passed over.
fn round_corner(points: &mut Vec<DVec2>, corner: usize, tangent: f64) {
    let at = points[corner];
    let mut before = corner;
    while before > 0 && points[before - 1].distance(at) < tangent + SHORTEST_RUN {
        before -= 1;
    }
    let mut after = corner + 1;
    while after + 1 < points.len() && points[after].distance(at) < tangent + SHORTEST_RUN {
        after += 1;
    }
    let (Some(&from), Some(&to)) = (points.get(before.wrapping_sub(1)), points.get(after)) else {
        return;
    };
    let (a, b) = ((at - from).normalize(), (to - at).normalize());
    let turn = a.perp_dot(b).atan2(a.dot(b));
    let steps = (turn.abs().to_degrees() / CROSSING_STEP).ceil().max(1.0) as usize;
    // Each run's heading turns by an even step: its first half step from the line
    // before, its last half step onto the line after, so the runs close on both lines.
    let step = turn / steps as f64;
    // No run shorter than the wall is built in.
    let radius = SHORTEST_RUN * 1.05 / (2.0 * (step.abs() / 2.0).sin());
    let tangent = tangent.max(radius * (turn.abs() / 2.0).tan());
    let (start, end) = (at - a * tangent, at + b * tangent);
    let chord = |k: f64| DVec2::from_angle(step * k).rotate(a);
    let span: DVec2 = (0..steps).map(|k| chord(k as f64 + 0.5)).sum();
    let length = start.distance(end) / span.length();
    let mut run = vec![start];
    for k in 0..steps {
        let next = run[k] + chord(k as f64 + 0.5) * length;
        run.push(next);
    }
    run[steps] = end;
    points.splice(before..after, run);
}

/// Where the segment from `a` to `b` crosses the line where `coordinate` of a point
/// equals `value`, if it does.
fn crossing(a: DVec2, b: DVec2, coordinate: fn(DVec2) -> f64, value: f64) -> Option<DVec2> {
    let (ca, cb) = (coordinate(a), coordinate(b));
    ((ca - value) * (cb - value) <= 0.0 && ca != cb).then(|| a.lerp(b, (value - ca) / (cb - ca)))
}

/// The angle (degrees) where a run between `p` and the circle round `pad` touches it, for
/// a wall going round the circle clockwise (seen from above, +z south): `leaving` the
/// circle toward `p`, or coming to it from `p`.
fn touch_angle(pad: DVec2, p: DVec2, leaving: bool) -> f64 {
    let away = p - pad;
    let touch = (PAD_WALL_ROUND / away.length()).acos().to_degrees();
    away.to_angle().to_degrees() + if leaving { touch } else { -touch }
}

/// The points round `pad` from angle `from` down to `to` (degrees), in even steps of at
/// most [`PAD_WALL_STEP`].
fn round_pad(pad: DVec2, from: f64, to: f64) -> impl Iterator<Item = DVec2> {
    let steps = ((from - to) / PAD_WALL_STEP).ceil().max(1.0) as usize;
    (0..=steps).map(move |k| {
        let angle = from + (to - from) * k as f64 / steps as f64;
        pad + DVec2::from_angle(angle.to_radians()) * PAD_WALL_ROUND
    })
}

/// Where the run from `outside` to `inside` comes within `reach` of `pad`, by halving.
fn reach_point(mut outside: DVec2, mut inside: DVec2, pad: DVec2, reach: f64) -> DVec2 {
    for _ in 0..40 {
        let middle = (outside + inside) / 2.0;
        if middle.distance(pad) < reach {
            inside = middle;
        } else {
            outside = middle;
        }
    }
    outside
}

/// The spawn pads' middles, both teams'.
fn spawn_pads() -> Vec<DVec2> {
    [Team::Blue, Team::Red]
        .into_iter()
        .flat_map(|team| spawn_positions(team, 1.0))
        .map(|spawn| DVec2::new(spawn.x, spawn.z))
        .collect()
}

/// The points `past` metres behind the back walls of Gaviota's south row, west to east:
/// one behind each lot line, neighbours' halfway across their side yard, and one behind
/// each end of the row.
fn behind_gaviota_south(past: f64) -> Vec<DVec2> {
    let mut row: Vec<SendaHouse> = senda_houses()
        .into_iter()
        .filter(|house| house.row == Row::GaviotaSouth)
        .collect();
    row.sort_by(|a, b| a.x.total_cmp(&b.x));
    let mut behind: Vec<DVec2> = Vec::new();
    for house in &row {
        let design = house.design();
        let (_, out, _, half) = wall_frames(design.w, design.d)[design.door_wall];
        let out = design.to_world(out);
        let front = DVec2::new(out.x, out.z).normalize();
        // Every lot's back wall stands as far from the street as the row's furthest-set
        // house's: its yard makes up its step forward. The lot is a full house deep
        // whatever its plan.
        let yard = back_wall() - (house_front() - SETBACK + house.setback + HOUSE_DEPTH);
        let rear = house.lot_centre() - front * (HOUSE_DEPTH / 2.0 + yard + past);
        let across = left(front) * (half + SIDE_YARD / 2.0);
        let (mut west, mut east) = (rear - across, rear + across);
        if west.x > east.x {
            std::mem::swap(&mut west, &mut east);
        }
        match behind.last_mut() {
            Some(last) => *last = (*last + west) / 2.0,
            None => behind.push(west),
        }
        behind.push(east);
    }
    behind
}

/// The inside face of the wall round the level, each corner in turn with the inside on
/// the left of the way it runs (seen from above, +z south). It follows the tract's own
/// edges where the spawn lanes allow: west and east it keeps to the arena's edge, where
/// the spawn lanes stand against it. On the north it runs along the arena's edge at the
/// top of the bank, the Way just outside it above Rumbo's north row, then comes down the
/// bank's top along the Way's south verge, crosses the Way square to it and comes down
/// the top of the bank north of it, as the neighbouring tract's wall does, bending round
/// the north-western pad on the Way's verge to the west edge, where the Way leaves
/// through it again; every corner there turns in short runs. On the south it curves
/// round the south-western pad and runs a lane behind the back walls of Gaviota's south
/// row, past the slope's band of trees, then round the south-eastern pad to the east
/// edge.
pub fn senda_boundary() -> Vec<DVec2> {
    let pads = spawn_pads();
    let corner = |west: bool, north: bool| {
        pads.iter()
            .copied()
            .filter(|pad| (pad.x < 0.0) == west)
            .min_by(|a, b| {
                if north {
                    a.y.total_cmp(&b.y)
                } else {
                    b.y.total_cmp(&a.y)
                }
            })
            .expect("a corner spawn pad")
    };
    let mut boundary = north_wall(corner(true, true));
    boundary.extend(south_wall(corner(true, false), corner(false, false)));
    boundary.push(DVec2::new(ARENA, -ARENA));
    boundary
}

/// The wall's north side, from where it meets the arena's north edge on the bank south of
/// Senda Parkway to the west edge past the north-western pad.
fn north_wall(corner_pad: DVec2) -> Vec<DVec2> {
    let way = Street::SendaParkway.line();
    let verge = |s: f64, offset: f64| {
        let (point, along) = way.at(s);
        point + left(along) * offset
    };
    // The crossing: up the Way from the west, the first place its north verge comes
    // within the crossing's room of the north edge.
    let mut cross = 0.0;
    while verge(cross, WAY_NORTH_WALL).y > -ARENA + WAY_CROSSING_EDGE {
        cross += 0.1;
    }
    let mut along = vec![0.0];
    for pair in way.0.windows(2) {
        along.push(along[along.len() - 1] + pair[0].distance(pair[1]));
    }
    // Down the south verge from the north edge, back the way the Way's trace runs, to the
    // crossing.
    let south = way.offset(-WAY_SOUTH_WALL);
    let mut points: Vec<DVec2> = Vec::new();
    for k in (1..south.len()).rev() {
        if points.is_empty() {
            match crossing(south[k], south[k - 1], |p| p.y, -ARENA) {
                Some(entry) => points.push(entry),
                None => continue,
            }
        }
        if along[k - 1] <= cross {
            break;
        }
        points.push(south[k - 1]);
    }
    // Across the Way, square to it, and on down its north verge.
    let south_corner = points.len();
    points.push(verge(cross, -WAY_SOUTH_WALL));
    points.push(verge(cross, WAY_NORTH_WALL));
    let north = way.offset(WAY_NORTH_WALL);
    let leave = PAD_WALL_ROUND + PAD_WALL_LEAD;
    let (mut entry, mut lead) = (None, 0);
    for k in (0..north.len()).rev().filter(|&k| along[k] < cross) {
        let (last, next) = (points[points.len() - 1], north[k]);
        if next.distance(corner_pad) < leave {
            // From where the run comes within its lead of the circle round the pad, the
            // wall runs on to meet the circle where it touches it.
            let start = reach_point(last, next, corner_pad, leave);
            lead = points.len();
            points.push(start);
            entry = Some(touch_angle(corner_pad, start, false));
            break;
        }
        points.push(next);
    }
    // Round the pad's north side, west, to the arena's west edge.
    let entry = entry.expect("the wall along the Way comes round the north-western pad");
    points.extend(round_pad(corner_pad, entry, -180.0));
    // The corners where the wall crosses the Way and where it leaves the Way's verge for
    // the pad turn in short runs, as round the pads, the latest first so the earlier
    // keep their places.
    round_corner(&mut points, lead, CROSSING_ROUND);
    round_corner(&mut points, south_corner + 1, CROSSING_ROUND);
    round_corner(&mut points, south_corner, CROSSING_ROUND);
    whole_degree_chain(&points, -ARENA)
}

/// The wall's south side, from the west edge round the south-western pad, a lane behind
/// the back walls of Gaviota's south row, and round the south-eastern pad to the east
/// edge.
fn south_wall(west_pad: DVec2, east_pad: DVec2) -> Vec<DVec2> {
    let mut behind = behind_gaviota_south(SOUTH_WALL_FROM_STREET - back_wall());
    for point in &mut behind {
        point.y = point.y.clamp(SOUTH_WALL_LEAST, ARENA);
    }
    // Round the south-western pad from the west edge, leaving it toward the row.
    let mut points: Vec<DVec2> =
        round_pad(west_pad, 180.0, touch_angle(west_pad, behind[0], true)).collect();
    // Behind the row to where it comes within its lead of the south-eastern pad's circle.
    let leave = PAD_WALL_ROUND + PAD_WALL_LEAD;
    let row_start = points.len();
    for next in behind {
        let last = points[points.len() - 1];
        if next.distance(east_pad) < leave {
            points.push(reach_point(last, next, east_pad, leave));
            break;
        }
        points.push(next);
    }
    let entry = touch_angle(east_pad, points[points.len() - 1], false);
    points.extend(round_pad(east_pad, entry, 0.0));
    // Where it leaves the pad's circle for the row it turns in short runs, as where it
    // crosses the Way.
    round_corner(&mut points, row_start, CROSSING_ROUND);
    whole_degree_chain(&points, ARENA)
}

/// Whether `at` keeps `room` (m) from the wall's line.
pub fn clear_of_wall(boundary: &[DVec2], at: DVec2, room: f64) -> bool {
    let closed: Vec<DVec2> = boundary.iter().chain(boundary.first()).copied().collect();
    Line(closed).distance(at) > room
}

/// Whether `p` lies inside the wall round the level (a ray cast across its runs).
pub fn inside_boundary(boundary: &[DVec2], p: DVec2) -> bool {
    let n = boundary.len();
    (0..n)
        .filter(|&i| {
            let (a, b) = (boundary[i], boundary[(i + 1) % n]);
            (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x)
        })
        .count()
        % 2
        == 1
}

/// The thickness (m) of the tract's wall as it is drawn: a block wall standing on the
/// inside face of its run's cover, the rest of the cover's depth outside it left open
/// ground (the cover keeps its depth so no tank squeezes past a joint).
pub const WALL_THICKNESS: f64 = 0.2;
/// The step (m) in which a run's cover is grown past an outer corner, so its plan can
/// carry how far ([`WallFace`]), in three bits.
const WALL_REACH_STEP: f64 = 0.3;
const WALL_REACH_STEPS: u8 = 7;
/// The colour of a run of the level's wall drawn as a view fence rather than block:
/// along the top of the bank under Senda Parkway and on the hillside, where the lots look
/// out over open ground, as the real ones do.
pub const SENDA_FENCE_COLOR: u32 = 0x2b2d2f;

/// How a run of the level's wall stands in its cover's box, in the box's own frame (the
/// frame its plan turns): whether the run lies along the box's x (else its z; a short run
/// round a corner may be shallower than the box is deep), which side of the run's line
/// through the box's middle the level lies on (+1 or -1 across it), and how far (m) the
/// box runs on past the run's own ends at its negative and positive ends, where it is
/// grown round an outer corner. The wall is drawn on the box's inside face, from end to
/// end of the run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WallFace {
    pub along_x: bool,
    pub inside: f64,
    pub reach: [f64; 2],
}

impl WallFace {
    /// The plan that carries it for a run turned `angle` whole degrees: its garage side
    /// the inside, its style the two reaches in [`WALL_REACH_STEP`]s (three bits each)
    /// and whether it runs along x.
    fn plan(self, angle: i8) -> HousePlan {
        let steps = self.reach.map(|reach| {
            (reach / WALL_REACH_STEP)
                .round()
                .clamp(0.0, f64::from(WALL_REACH_STEPS)) as u8
        });
        HousePlan {
            style: steps[0] | (steps[1] << 3) | (u8::from(self.along_x) << 6),
            street: 0,
            angle,
            garage: if self.inside > 0.0 {
                GarageSide::Right
            } else {
                GarageSide::Left
            },
        }
    }

    /// The face a run's plan carries.
    pub fn of(plan: HousePlan) -> Self {
        WallFace {
            along_x: plan.style & (1 << 6) != 0,
            inside: match plan.garage {
                GarageSide::Right => 1.0,
                GarageSide::Left => -1.0,
            },
            reach: [
                plan.style & WALL_REACH_STEPS,
                (plan.style >> 3) & WALL_REACH_STEPS,
            ]
            .map(|steps| f64::from(steps) * WALL_REACH_STEP),
        }
    }
}

/// Whether the wall's run from `a` to `b` looks out over open ground and is drawn as a
/// view fence: on the hillside along the arena's east edge, and along the top of the bank
/// under Senda Parkway, from the arena's north edge down the Way's south verge.
fn looks_out(a: DVec2, b: DVec2) -> bool {
    let edge = ARENA - 0.01;
    let middle = (a + b) / 2.0;
    let way = Street::SendaParkway.line();
    let bank_top = (way.signed_distance(middle) + WAY_SOUTH_WALL).abs() < 1.5
        && middle.x > Street::NeblinaGaviota.line().0[0].x;
    (a.x > edge && b.x > edge) || (a.y < -edge && b.y < -edge) || bank_top
}

/// The wall's runs as boundary cover, each grown outward from the polygon's edge and on
/// past its outer corners as far as their outer faces meet, so the runs close round
/// them. A run along a world axis stands square; a slanting one (down Senda Parkway's
/// verge) turns by its whole degrees, as a lot's wall does
/// ([`cover_turn`](super::arena::cover_turn)). Each carries its [`WallFace`] in its plan.
fn boundary_cover(boundary: &[DVec2]) -> Vec<CoverDef> {
    let n = boundary.len();
    let direction = |i: usize| (boundary[(i + 1) % n] - boundary[i]).normalize();
    // An outer corner turns the way the polygon winds (left, inside on the left); an
    // inner one's runs meet behind it. Rounded up to the plan's step, so the plan says
    // exactly how far.
    let reach = |corner: usize| {
        let (before, after) = (direction((corner + n - 1) % n), direction(corner));
        let turn = before.perp_dot(after);
        if turn < 0.0 {
            let reach = BOUNDARY_THICKNESS * (turn.abs().atan2(before.dot(after)) / 2.0).tan();
            (reach / WALL_REACH_STEP).ceil() * WALL_REACH_STEP
        } else {
            0.0
        }
    };
    (0..n)
        .map(|i| {
            let (a, b, along) = (boundary[i], boundary[(i + 1) % n], direction(i));
            let (start, end) = (reach(i), reach((i + 1) % n));
            let (from, to) = (a - along * start, b + along * end);
            let out = -left(along) * BOUNDARY_THICKNESS;
            let middle = (from + to + out) / 2.0;
            let length = from.distance(to);
            // A turn about +y carries the box's x to (cos, -sin) in the world; the box's
            // long side lies along whichever world axis the run lies nearer.
            let turn = (-along.y).atan2(along.x).to_degrees();
            let quarter = (turn / 90.0).round();
            let angle = (turn - quarter * 90.0).round() as i8;
            let long_x = (quarter as i32).rem_euclid(2) == 0;
            let (w, d) = if long_x {
                (length, BOUNDARY_THICKNESS)
            } else {
                (BOUNDARY_THICKNESS, length)
            };
            // The run's direction and its inside in the box's own frame.
            let (sin, cos) = f64::from(angle).to_radians().sin_cos();
            let local = |v: DVec2| DVec2::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos);
            let on = |v: DVec2, long: bool| if long == long_x { v.x } else { v.y };
            let forward = on(local(along), true) > 0.0;
            let face = WallFace {
                along_x: long_x,
                inside: on(local(left(along)), false).signum(),
                reach: if forward { [start, end] } else { [end, start] },
            };
            CoverDef {
                house_plan: Some(face.plan(angle)),
                ..CoverDef::new(
                    CoverKind::Boundary,
                    (middle.x * 100.0).round() / 100.0,
                    (middle.y * 100.0).round() / 100.0,
                    w,
                    d,
                    WALL_HEIGHT,
                    f64::INFINITY,
                    if looks_out(a, b) {
                        SENDA_FENCE_COLOR
                    } else {
                        SENDA_WALL_COLOR
                    },
                )
            }
        })
        .collect()
}

/// The level's cover as it stands in the world: its design's ([`senda_design_layout`])
/// scaled by [`SENDA_SCALE`] about the arena's middle, place and size. A tree's family
/// follows from where it stands (`tree_proportions`), so each tree stands at the nearest
/// spot east of its scaled place that grows the family its design place grows: the
/// level's oaks stay oaks, within a few centimetres of their places.
fn senda_layout() -> Vec<CoverDef> {
    senda_design_layout()
        .into_iter()
        .map(|cover| {
            let mut world = CoverDef {
                x: cover.x * SENDA_SCALE,
                z: cover.z * SENDA_SCALE,
                w: cover.w * SENDA_SCALE,
                d: cover.d * SENDA_SCALE,
                h: cover.h * SENDA_SCALE,
                ..cover
            };
            if cover.kind == CoverKind::Tree {
                let family = tree_proportions(cover.x, cover.z, 1.0, 1.0, 1.0).family;
                (world.x, world.z) = tree_site(world.x, world.z, family);
            }
            world
        })
        .collect()
}

/// The level's cover in design metres, as its houses, lots and scenery are laid out
/// ([`senda_layout`] stands it in the world).
pub fn senda_design_layout() -> Vec<CoverDef> {
    let boundary = senda_boundary();
    let mut covers = boundary_cover(&boundary);
    let houses = senda_houses();
    for house in &houses {
        covers.push(CoverDef {
            house_plan: Some(house.plan),
            ..CoverDef::new(
                CoverKind::House,
                house.x,
                house.z,
                house.w,
                house.d,
                house.h,
                HOUSE_HEALTH,
                SENDA_STYLES[usize::from(house.plan.style)].siding,
            )
        });
    }
    covers.extend(super::senda_yards::yard_walls(&houses));
    covers.extend(super::senda_yards::patio_covers(&houses));
    covers.extend(super::senda_entries::entry_covers(&houses));
    covers.extend(street_trees(&houses));
    covers.extend(trash_bins(&houses));
    covers.extend(street_lamp_covers(&boundary));
    covers.extend(way_trees(&boundary));
    covers.extend(big_trees(&boundary, &houses));
    // One on each cul-de-sac's island.
    for middle in Street::ALL.into_iter().filter_map(Street::cul_de_sac) {
        let (x, z) = oak_site(middle.x, middle.y);
        covers.push(CoverDef::new(
            CoverKind::Tree,
            x,
            z,
            BIG_TREE,
            BIG_TREE,
            BIG_TREE_HEIGHTS[0],
            80.0,
            TREE_COLOR,
        ));
    }
    // A tree past the wall would stand out of reach, so the wall's outside keeps none.
    covers.retain(|c| {
        c.kind != CoverKind::Tree
            || [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                .into_iter()
                .all(|(u, v)| {
                    let corner = DVec2::new(c.x + u * c.w, c.z + v * c.d);
                    inside_boundary(&boundary, corner)
                })
    });
    covers
}

const fn pickup(kind: PickupKind, x: f64, z: f64) -> PickupPlacement {
    PickupPlacement { kind, x, z }
}

/// The pickups round the neighbourhood: the laser in the middle of the greenbelt, the
/// rest in pairs, one nearer each team's spawns, on the streets, the greenbelt, the
/// slope under Senda Parkway and the verges.
pub static SENDA_PICKUPS: [PickupPlacement; 19] = [
    pickup(PickupKind::Laser, 0.0, -3.5),
    pickup(PickupKind::Rapid, -14.0, -22.3),
    pickup(PickupKind::Rapid, 6.7, 16.1),
    pickup(PickupKind::Spread, -11.0, -1.0),
    pickup(PickupKind::Spread, 14.0, -3.0),
    pickup(PickupKind::Rocket, -34.0, -40.0),
    pickup(PickupKind::Rocket, 32.0, -50.0),
    pickup(PickupKind::Ricochet, -8.0, -42.0),
    pickup(PickupKind::Ricochet, 14.0, 41.0),
    pickup(PickupKind::Piercing, -21.1, -5.6),
    pickup(PickupKind::Piercing, 28.0, -44.0),
    pickup(PickupKind::Repair, -46.0, 30.0),
    pickup(PickupKind::Repair, 55.0, 12.5),
    pickup(PickupKind::Repair, -24.0, 37.0),
    pickup(PickupKind::Repair, 18.0, -46.0),
    pickup(PickupKind::Shield, -40.0, 40.0),
    pickup(PickupKind::Shield, 38.0, -52.0),
    pickup(PickupKind::Speed, 10.0, -23.7),
    pickup(PickupKind::Speed, -11.6, 9.7),
];

pub static SENDA_MAP: ArenaMap = ArenaMap {
    id: MapId::Neighborhood,
    theme: Some(MapTheme::Neighborhood),
    floor: None,
    outer_floor: None,
    outer_floor_extent: None,
    scale: Some(SENDA_SCALE),
    layout: senda_layout,
    // In design metres: the simulation places pickups at the map's scale.
    pickups: &SENDA_PICKUPS,
};

#[cfg(test)]
mod lamp_tests {
    use super::*;

    #[test]
    fn every_street_lamp_stands_in_a_parkway_strip_and_the_rows_apart() {
        let lines: Vec<(Street, Line)> = Street::ALL.map(|street| (street, street.line())).into();
        let sites = lamp_sites();
        assert!(
            sites.iter().filter(|(_, tall)| *tall).count() >= 6,
            "{sites:?}"
        );
        assert!(
            sites.iter().filter(|(_, tall)| !tall).count() >= 4,
            "{sites:?}"
        );
        for (k, &(at, tall)) in sites.iter().enumerate() {
            for (street, line) in &lines {
                let off = line.distance(at) - street.half();
                assert!(
                    off > LAMP_KERB_ROOM,
                    "a lamp at {at:?} on {street:?}'s asphalt"
                );
                if *street != Street::SendaParkway {
                    assert!(
                        !(PARKWAY..PARKWAY + SIDEWALK).contains(&off),
                        "a lamp at {at:?} on {street:?}'s sidewalk"
                    );
                }
            }
            if !tall {
                for &(other, _) in &sites[..k] {
                    assert!(
                        other.distance(at) >= LAMP_ROOM,
                        "lamps at {at:?} and {other:?}"
                    );
                }
            }
        }
    }
}
