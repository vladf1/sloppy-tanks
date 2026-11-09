//! Senda: Rumbo, Gaviota and Neblina as the neighbourhood lays them out, traced
//! from an aerial photograph and brought into the arena at under a third of their
//! length and half their breadth.
//!
//! Neblina comes down from Senda Parkway on a diagonal across the arena's north-west and
//! sweeps round a broad curve east, where it becomes Gaviota, which runs to its
//! cul-de-sac; no street goes on south. Rumbo leaves Neblina near the top and curves
//! east to its own. Their houses stand detached in rows along the curving streets, side
//! yards between them and their fronts stepping lot by lot, each turned to front its
//! street with a driveway to its garage: both sides of Rumbo and Gaviota, running
//! straight on beside their turning circles, Neblina's west side in one even run from
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
/// The turning circles at Rumbo's and Gaviota's ends.
const RUMBO_CUL_DE_SAC_TRACE: (f64, f64) = (1722.0, 322.0);
const GAVIOTA_CUL_DE_SAC_TRACE: (f64, f64) = (1810.0, 782.0);

/// Half widths (m): a residential street kerb to kerb, and Senda Parkway's four lanes
/// with their planted median.
pub const STREET_HALF: f64 = 2.8;
pub const SENDA_PARKWAY_HALF: f64 = 4.2;
pub const MEDIAN_HALF: f64 = 0.8;
/// The grass parkway (with its street trees) and sidewalk between the kerb and the
/// front yards.
pub const PARKWAY: f64 = 1.0;
pub const SIDEWALK: f64 = 0.8;
/// A cul-de-sac's kerb radius and the planted island in its middle.
pub const CUL_DE_SAC_RADIUS: f64 = 4.6;
pub const CUL_DE_SAC_ISLAND: f64 = 1.4;

/// A house's box along its street and from front to back (deeper than wide, as the
/// tract's footprints are, about 11 by 15 m), the least its front stands back from the
/// sidewalk (a car's length of driveway), and the lots' spacing along the street. The
/// streets are drawn at under a third of their length, so the houses are narrowed with
/// them (less than the streets, to keep a two-car garage and the front door), but the
/// side yards between them stay over a quarter of the lot, wider than the tract's, so
/// every house reads as detached from the overhead camera past its walls and eaves; and
/// their walls stay low ([`WALL_PLATES`]), so a front is wider than its eaves are high.
pub const HOUSE_WIDTH: f64 = 5.0;
pub const HOUSE_DEPTH: f64 = 7.0;
pub const SETBACK: f64 = 3.0;
pub const LOT_PITCH: f64 = 7.0;
/// How much further back (m) than [`SETBACK`] a lot's house stands, lot by lot, so the
/// fronts step as the tract's different plans do rather than lining up as a terrace;
/// Neblina's west row steps deeper, every other house set back, since the overhead
/// camera looks down that row and only a deep step shows its fronts apart; its corner
/// lot on Senda Parkway, hemmed in behind by the spawn lane, takes the shallow step.
const STAGGER: [f64; 3] = [0.0, 0.8, 0.4];
const WEST_ROW_STAGGER: [f64; 2] = [-0.5, 0.9];
/// The least side yard (m) between two houses' walls, where a row turns or meets
/// another, and how far along its street a row looks on for room when a lot is taken.
pub const SIDE_YARD: f64 = 2.2;
const LOT_SEARCH: f64 = 0.5;
/// How far (m) up Neblina from its bend Gaviota's north row begins: the greenbelt
/// opens onto Neblina above it.
const GREENBELT_MOUTH: f64 = 12.0;
/// Neblina's west row from Senda Parkway round to the bend, in [`senda_houses`]' rows,
/// its lots' spacing and its houses' width. It holds eight houses between Senda Parkway
/// and the park at the bend, two of them round the curve's outside, the first on the
/// corner of Senda Parkway, narrower than the curving rows' houses (still with a two-car garage and the
/// front door), so each pair keeps a side yard of about three metres: the overhead
/// camera looks down the row, and a narrower yard hides under the eaves in front of it.
const NEBLINA_WEST_ROW: usize = 3;
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
const WEST_ROW_PITCH: f64 = 7.0;
pub const WEST_ROW_WIDTH: f64 = 4.6;
/// Where along its street a row's first lot stands (m from the street's start).
const FIRST_LOT: f64 = 3.0;
/// How much nearer (m) than a lot's pitch two houses of a row may stand, for rounding.
const PITCH_TOLERANCE: f64 = 0.01;
/// Wall plate heights (m), varied lot by lot so the rows do not read as copies: two
/// low storeys (the tract's are about half as high to the eaves as the houses are
/// wide), the least that keeps the upper windows, under a shallow tile roof whose ridge
/// rises over them by half the house's width at [`GARAGE_HOUSE_PITCH`].
const WALL_PLATES: [f64; 4] = [3.15, 3.1, 3.25, 3.15];
/// The highest wall plate (m) of a house on a narrower lot, so its front stays wider
/// than its eaves are high.
const NARROW_PLATE: f64 = 3.15;
/// Room (m) a house keeps from another street's paving, from a spawn pad's rim, from a
/// pickup's centre, and from the arena's edge.
const STREET_ROOM: f64 = 0.4;
/// The room (m) Rumbo's corner lot on its north side keeps from Neblina's
/// sidewalk: it stands back past a broad planted side yard, its front on Rumbo.
const CORNER_LOT_ROOM: f64 = 2.5;
const SPAWN_ROOM: f64 = 2.5;
/// A spawn pad's radius (its rim's outer edge).
const SPAWN_PAD: f64 = 2.3;
const PICKUP_ROOM: f64 = 2.5;
const EDGE_ROOM: f64 = 3.0;
/// The rows run straight on past a street's end beside its cul-de-sac, as the tract's do,
/// two houses a side at the turning circle, up to this far (m) past the circle's middle:
/// beyond it the circle's far rim opens onto the hillside. A house there turns toward
/// the circle's middle, by at most this many degrees off square to its row.
const CUL_DE_SAC_ROW_END: f64 = 4.6;
const CUL_DE_SAC_TURN: f64 = 20.0;
/// How far (m) past the circle's middle a row on a street's south side runs on: the
/// tract's south rows hold a lot more there, under the circle's south-east rim.
const CUL_DE_SAC_SOUTH_ROW_END: f64 = 11.0;

/// The colour of the level's wall where it is drawn as the tract's capped split-face
/// block, which its model knows it by.
pub const SENDA_WALL_COLOR: u32 = 0xc8b28c;
const TREE_COLOR: u32 = 0x3f7a3a;
/// The young parkway trees, one before each house: their footprint (a slim pale trunk
/// under a thin, open crown that leaves the houses behind it in view, as the tract's
/// young street trees do) and height, and the least room (m) between two of them.
const STREET_TREE: f64 = 1.2;
const STREET_TREE_HEIGHT: f64 = 4.5;
const STREET_TREE_ROOM: f64 = 3.0;
/// How far (m) a parkway tree's trunk keeps inside its parkway's edges.
const TREE_KERB_ROOM: f64 = 0.25;

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
        match self {
            Street::Rumbo => Some(from_aerial(RUMBO_CUL_DE_SAC_TRACE)),
            Street::NeblinaGaviota => Some(from_aerial(GAVIOTA_CUL_DE_SAC_TRACE)),
            _ => None,
        }
    }
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

/// Whether `(x, z)` lies on Senda's paving: a street with its sidewalks or a
/// cul-de-sac. Ground effects throw concrete grit there, not earth.
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
}

impl SendaHouse {
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
        Plan::Small => 6.2,
        Plan::Medium => 6.6,
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
        let room = if Some(street) == own {
            street.half() + PARKWAY + SIDEWALK + SETBACK * 0.5
        } else {
            street.half() + PARKWAY + SIDEWALK + street_room
        };
        corners.iter().all(|&corner| line.distance(corner) > room)
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

    /// Whether the two stand closer than a side yard (separating axes).
    fn crowds(&self, other: &Footprint) -> bool {
        let (a, b) = (
            self.corners(SIDE_YARD / 2.0),
            other.corners(SIDE_YARD / 2.0),
        );
        let axes = [
            self.facing,
            left(self.facing),
            other.facing,
            left(other.facing),
        ];
        axes.into_iter().all(|axis| {
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
/// Rumbo's and Gaviota's rows run on straight past their streets' ends beside the
/// turning circles. Each row runs on from lot to lot; where a lot is taken (a street, a
/// spawn, a neighbour round a bend) it looks a little further on. A house left with no
/// neighbour in its row stands alone on the field and is dropped.
pub fn senda_houses() -> Vec<SendaHouse> {
    let mut houses: Vec<(SendaHouse, Footprint)> = Vec::new();
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
        (Street::Rumbo, -1.0, FIRST_LOT, 1.0, STREET_ROOM),
        (
            Street::NeblinaGaviota,
            1.0,
            greenbelt_mouth,
            1.0,
            STREET_ROOM,
        ),
        (Street::NeblinaGaviota, -1.0, bend, -1.0, STREET_ROOM),
        (Street::NeblinaGaviota, -1.0, bend, 1.0, STREET_ROOM),
    ];
    let park = bend_park();
    let gaviota = Street::NeblinaGaviota.line();
    let gaviota_heading = (gaviota.at(gaviota.length()).0 - gaviota.at(bend).0).normalize();
    for (row, &(street, side, start, way, street_room)) in rows.iter().enumerate() {
        let line = street.line();
        let length = line.length();
        // A row running toward its street's cul-de-sac carries on straight past the
        // street's end beside the turning circle.
        let (end, reach, bulb) = match street.cul_de_sac() {
            Some(middle) if way > 0.0 => {
                let (end, along) = line.at(length);
                let row_end = if side < 0.0 {
                    CUL_DE_SAC_SOUTH_ROW_END
                } else {
                    CUL_DE_SAC_ROW_END
                };
                let past = (middle - end).dot(along) + row_end;
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
        while s >= FIRST_LOT && s < length + reach {
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
            for setback in staggers.map(|stagger| SETBACK + stagger) {
                let distance = house_front() - SETBACK + setback + HOUSE_DEPTH / 2.0;
                let mut at = point + out * distance;
                // Past the circle's middle the street's kerb is the circle's: a house
                // there stands no further back from it than from a street.
                if let Some(middle) = bulb.filter(|_| s > length) {
                    let most = CUL_DE_SAC_RADIUS + PARKWAY + SIDEWALK + setback + HOUSE_DEPTH / 2.0;
                    let away = at - middle;
                    if away.length() > most {
                        at = middle + away.normalize() * most;
                    }
                }
                // Beside the turning circle a house turns a little toward its middle.
                let facing = match bulb {
                    Some(middle) if s > length => {
                        let square = -out;
                        let toward = (middle - at).normalize();
                        let most = CUL_DE_SAC_TURN.to_radians();
                        let turn = square
                            .perp_dot(toward)
                            .atan2(square.dot(toward))
                            .clamp(-most, most);
                        DVec2::from_angle(turn).rotate(square)
                    }
                    _ if row == GAVIOTA_NORTH_ROW => -left(gaviota_heading),
                    _ => -out,
                };
                let footprint = Footprint {
                    at,
                    facing,
                    w: width,
                };
                // A lot's pitch is measured between the houses, so the lots fan out
                // round the outside of a curve as the real ones do; the park takes a lot
                // of its own.
                let spaced = last
                    .is_none_or(|at| at.distance(footprint.at) > pitch - PITCH_TOLERANCE)
                    && footprint.at.distance(park) > LOT_PITCH - PITCH_TOLERANCE;
                let room = houses.iter().all(|(_, other)| !other.crowds(&footprint));
                if spaced && room && clear(&footprint, Some(street), street_room) {
                    houses.push((house(&footprint, lot, row, setback), footprint));
                    last = Some(footprint.at);
                    lot += 1;
                    break;
                }
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
        .filter(|(_, footprint)| neighboured(footprint.at))
        .map(|&(house, _)| house)
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

/// The parkway trees, young slender broadleaves: one in front of each house, in the parkway before its front walk
/// (clear of its driveway), wherever that is clear of the pickups, the asphalt and the
/// sidewalks of every street, the turning circles and the trees already placed.
fn street_trees(houses: &[SendaHouse]) -> Vec<CoverDef> {
    let mut trees: Vec<CoverDef> = Vec::new();
    let lines: Vec<(Street, Line)> = Street::ALL.map(|street| (street, street.line())).into();
    for house in houses {
        let design = house.design();
        let (centre, out, along, _) = wall_frames(design.w, design.d)[design.door_wall];
        let Some(door) = design.wall_plan(design.door_wall).door() else {
            continue;
        };
        let face = design.to_world(centre + along * door);
        let out = design.to_world(out);
        let reach = house.setback + SIDEWALK + PARKWAY / 2.0;
        let (x, z) = tree_site(
            house.x + face.x + out.x * reach,
            house.z + face.z + out.z * reach,
            BIRCH,
        );
        let spot = DVec2::new(x, z);
        let crowded = SENDA_PICKUPS
            .iter()
            .any(|p| DVec2::new(p.x, p.z).distance(spot) < PICKUP_ROOM + 1.5)
            || lines.iter().any(|(street, line)| {
                let off = line.distance(spot) - street.half();
                // On the asphalt or a sidewalk, not in a parkway.
                off < TREE_KERB_ROOM
                    || (off > PARKWAY - TREE_KERB_ROOM && off < PARKWAY + SIDEWALK + TREE_KERB_ROOM)
            })
            || Street::ALL
                .into_iter()
                .filter_map(Street::cul_de_sac)
                .any(|middle| middle.distance(spot) < CUL_DE_SAC_RADIUS + STREET_TREE)
            || trees
                .iter()
                .any(|tree| DVec2::new(tree.x, tree.z).distance(spot) < STREET_TREE_ROOM);
        if !crowded && in_arena(spot) {
            trees.push(CoverDef::new(
                CoverKind::Tree,
                x,
                z,
                STREET_TREE,
                STREET_TREE,
                STREET_TREE_HEIGHT,
                80.0,
                TREE_COLOR,
            ));
        }
    }
    trees
}

/// Senda's wheelie bins out for collection: explosive drums drawn as a dark green
/// trash bin and a blue recycling bin ([`is_trash_bin`] picks the look by colour), their
/// footprint across and front to back and their height (m), and their health.
pub const TRASH_BIN_COLORS: [u32; 2] = [0x3e4b3c, 0x2d5c8a];
pub const TRASH_BIN: (f64, f64, f64) = (0.66, 0.74, 1.08);
const TRASH_BIN_HEALTH: f64 = 20.0;
/// One house in this many puts its bins out, starting with the first.
const TRASH_BIN_HOUSES: usize = 3;
/// Where (m) the bins stand: past the kerb into the street, and from the driveway's
/// edge on its side away from the front door, and apart from each other.
const TRASH_BIN_KERB_ROOM: f64 = 0.5;
const TRASH_BIN_DRIVEWAY_ROOM: f64 = 0.6;
const TRASH_BIN_GAP: f64 = 0.85;

/// Whether a drum is one of Senda's wheelie bins.
pub fn is_trash_bin(color: u32) -> bool {
    TRASH_BIN_COLORS.contains(&color)
}

/// The bins of every [`TRASH_BIN_HOUSES`]th house, a trash and a recycling bin side by
/// side in the street against the kerb beside its driveway, turned to face the street,
/// wherever both stand on a straight run of asphalt clear of the turning circles, the
/// spawn pads and the pickups.
fn trash_bins(houses: &[SendaHouse]) -> Vec<CoverDef> {
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
            at_kerb && off_circles && in_arena(spot) && off_pads_and_pickups(spot, w)
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
const LAMP_KERB_ROOM: f64 = 0.35;
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
    for house in senda_houses().iter().step_by(LAMP_EVERY) {
        let design = house.design();
        let (centre, out, along, half) = wall_frames(design.w, design.d)[design.door_wall];
        let reach = house.setback + SIDEWALK + PARKWAY / 2.0;
        let spot = design.to_world(centre + along * (half + SIDE_YARD / 2.0) + out * reach);
        let at = DVec2::new(house.x + spot.x, house.z + spot.z);
        let crowded = sites.iter().any(|(lamp, _)| lamp.distance(at) < LAMP_ROOM);
        if lamp_clear(at, &lines) && !crowded {
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
const MEDIAN_TREE_HEIGHT: f64 = 4.8;
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
const GREENBELT_TREE_OFFSET: f64 = 1.6;
const GREENBELT_TREE_HEIGHTS: [f64; 3] = [6.2, 5.6, 6.6];
/// The trees on the bank below Senda Parkway (arena x, z, height).
const BANK_OAKS: [(f64, f64, f64); 3] = [(-2.0, -44.0, 6.0), (9.0, -47.0, 6.2), (30.0, -47.0, 5.8)];
/// The band of oaks and peppers on the top of the slope down to Gardenia, behind the back
/// walls of Gaviota's south row: one behind each lot line, this far (m) past the walls,
/// their crowns over the gardens' corners, wherever it leaves a tank's lane (m) along the
/// level's wall.
const BACK_TREE_REACH: f64 = 1.5;
pub const SOUTH_LANE: f64 = 4.5;
const BIG_TREE: f64 = 2.6;

/// The greenbelt's trees, the bank's and the band behind Gaviota's south row: oaks, at
/// spots clear of the spawn pads, the pickups, the paving and each other, the band a lane
/// short of the level's wall at `boundary`.
fn big_trees(boundary: &[DVec2]) -> Vec<CoverDef> {
    let mut wanted: Vec<(DVec2, f64)> = Vec::new();
    let middle = Line(greenbelt_line(0.0));
    if middle.0.len() > 1 {
        let mut s = GREENBELT_TREE_SPACING / 2.0;
        let mut k = 0;
        while s < middle.length() {
            let (point, along) = middle.at(s);
            let side = if k % 2 == 0 { 1.0 } else { -1.0 };
            let height = GREENBELT_TREE_HEIGHTS[k % GREENBELT_TREE_HEIGHTS.len()];
            wanted.push((point + left(along) * side * GREENBELT_TREE_OFFSET, height));
            s += GREENBELT_TREE_SPACING;
            k += 1;
        }
    }
    wanted.extend(BANK_OAKS.map(|(x, z, h)| (DVec2::new(x, z), h)));
    for (k, at) in behind_gaviota_south(BACK_TREE_REACH)
        .into_iter()
        .enumerate()
    {
        let height = GREENBELT_TREE_HEIGHTS[k % GREENBELT_TREE_HEIGHTS.len()] - 0.4;
        if clear_of_wall(boundary, at, BIG_TREE / 2.0 + SOUTH_LANE) {
            wanted.push((at, height));
        }
    }
    let way = Street::SendaParkway.line();
    let mut trees: Vec<CoverDef> = Vec::new();
    for (at, height) in wanted {
        let (x, z) = oak_site(at.x, at.y);
        let at = DVec2::new(x, z);
        // Off the paving and Senda Parkway's wide path along its south side.
        let off_way = way.distance(at) > Street::SendaParkway.half() + WAY_PARKWAY + WAY_PATH + 1.0;
        let clear = in_arena(at)
            && off_pads_and_pickups(at, BIG_TREE)
            && off_way
            && !is_senda_paved(x, z)
            && trees
                .iter()
                .all(|tree| DVec2::new(tree.x, tree.z).distance(at) > BIG_TREE * 1.5);
        if clear {
            trees.push(CoverDef::new(
                CoverKind::Tree,
                x,
                z,
                BIG_TREE,
                BIG_TREE,
                height,
                80.0,
                TREE_COLOR,
            ));
        }
    }
    trees
}

/// The tract's wall: its height (m), and how far (m) its south run stands behind the back
/// walls of Gaviota's south row, across the planted top of the slope down to Gardenia with
/// its one band of trees, a tank's lane between them and the wall.
const WALL_HEIGHT: f64 = 1.8;
const SOUTH_WALL_ROOM: f64 = 7.5;
/// The least distance (m) south of the arena's middle the south run keeps behind the bend
/// park and the row's west end, where the row stands furthest north: the bots' southern
/// flank points (38 m south) stay well inside it.
const SOUTH_WALL_LEAST: f64 = 40.0;
/// How far (m) from Senda Parkway's middle the wall runs along its north side: past the
/// north sidewalk and a planted strip, at the top of the bank up to the neighbouring
/// tract.
const WAY_NORTH_WALL: f64 = SENDA_PARKWAY_HALF + PARKWAY + SIDEWALK + 3.2;
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
    let mut behind = behind_gaviota_south(SOUTH_WALL_ROOM);
    for point in &mut behind {
        point.y = point.y.clamp(SOUTH_WALL_LEAST, ARENA);
    }
    // Round the south-western pad from the west edge, leaving it toward the row.
    let mut points: Vec<DVec2> =
        round_pad(west_pad, 180.0, touch_angle(west_pad, behind[0], true)).collect();
    // Behind the row to where it comes within its lead of the south-eastern pad's circle.
    let leave = PAD_WALL_ROUND + PAD_WALL_LEAD;
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

fn senda_layout() -> Vec<CoverDef> {
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
    covers.extend(street_trees(&houses));
    covers.extend(trash_bins(&houses));
    covers.extend(street_lamp_covers(&boundary));
    covers.extend(way_trees(&boundary));
    covers.extend(big_trees(&boundary));
    // One on each cul-de-sac's island.
    for middle in Street::ALL.into_iter().filter_map(Street::cul_de_sac) {
        let (x, z) = oak_site(middle.x, middle.y);
        covers.push(CoverDef::new(
            CoverKind::Tree,
            x,
            z,
            BIG_TREE,
            BIG_TREE,
            5.6,
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
    pickup(PickupKind::Rapid, 14.0, 18.6),
    pickup(PickupKind::Spread, -11.0, -1.0),
    pickup(PickupKind::Spread, 34.0, -2.0),
    pickup(PickupKind::Rocket, -34.0, -40.0),
    pickup(PickupKind::Rocket, 48.0, -48.0),
    pickup(PickupKind::Ricochet, -8.0, -42.0),
    pickup(PickupKind::Ricochet, 20.0, 40.5),
    pickup(PickupKind::Piercing, -13.0, 24.0),
    pickup(PickupKind::Piercing, 28.0, -44.0),
    pickup(PickupKind::Repair, -46.0, 30.0),
    pickup(PickupKind::Repair, 47.0, 6.0),
    pickup(PickupKind::Repair, -24.0, 37.0),
    pickup(PickupKind::Repair, 18.0, -46.0),
    pickup(PickupKind::Shield, -40.0, 40.0),
    pickup(PickupKind::Shield, 52.0, -50.0),
    pickup(PickupKind::Speed, 10.0, -23.7),
    pickup(PickupKind::Speed, -10.0, 15.9),
];

pub static SENDA_MAP: ArenaMap = ArenaMap {
    id: MapId::Neighborhood,
    theme: Some(MapTheme::Neighborhood),
    floor: None,
    outer_floor: None,
    outer_floor_extent: None,
    scale: None,
    layout: senda_layout,
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
