//! Senda's lots and their walls. Every house in the tract stands on a walled lot: a brown
//! split-face block wall runs down each side lot line between neighbours, and a short
//! return with a gate closes each side yard a little behind the house's front. Where a lot
//! backs onto open ground (the bank under Senda Parkway, the greenbelt) its back line is a
//! black tubular steel view fence on a low block base instead, as is a corner or end lot's
//! side toward the street or the hillside, so its yard shows through; the rows backing onto
//! Senda Parkway's west branch and the slope down to Gardenia keep solid block behind.
//!
//! A lot is measured in its house's own frame (across its front and toward its street).
//! Neighbours in a row share the lot line between them, its corners the mean of what
//! each house's frame puts there, so the back walls chain corner to corner round the
//! curving streets. The walls are cover, laid out in short runs a shell can breach, each
//! turned with its line ([`cover_turn`](super::arena::cover_turn)); runs that would stand
//! on paving, outside the arena or too near a spawn pad or pickup are left out.
//!
//! Shared by the simulation and the scenery that paves and plants the same yards.

use glam::DVec2;

use super::arena::{CoverDef, spawn_positions};
use super::house_layout::{GarageSide, HousePlan, wall_frames};
use super::kinds::{CoverKind, Team};
use super::model_data::ARENA;
use super::senda_layout::{
    BACK_YARD, HOUSE_DEPTH, LOT_PITCH, Line, PARKWAY, Row, SENDA_PICKUPS, SETBACK, SIDE_YARD,
    SIDEWALK, STREET_HALF, SendaHouse, Street, back_fence, bend_park, greenbelt_line, house_front,
    inside_boundary, is_senda_paved, left, senda_boundary,
};

/// The block walls' paint (split-face block, a darker brown-tan than the houses' stucco),
/// the view fences' (black tubular steel) and the gates' (stained boards): the cover's
/// colour picks its look.
pub const YARD_WALL_COLOR: u32 = 0x8f7762;
pub const YARD_VIEW_COLOR: u32 = 0x23262a;
pub const YARD_GATE_COLOR: u32 = 0x6b5442;
/// The tract's perimeter block behind Neblina's west row, along the top of Senda Parkway's
/// west branch's planted slope: a dark brown split-face block.
pub const YARD_PERIMETER_COLOR: u32 = 0x5e4a3c;
/// A wall's height and thickness (m): the tract's 1.8 m of 8-inch block at the tank's
/// scale, so a tank's turret looks over into the yards.
pub const YARD_WALL_HEIGHT: f64 = 1.0;
pub const YARD_WALL_THICKNESS: f64 = 0.2;
/// A run's health: one standard shell breaches it.
const YARD_WALL_HEALTH: f64 = 40.0;
/// The longest run (m) a wall is laid out in, so a shell breaches a stretch, not a lot.
const WALL_RUN: f64 = 3.5;
/// How far back from a house's front (as a share of its depth) the gate returns close
/// its side yards.
const GATE_SHARE: f64 = 1.0 / 3.0;
/// How deep (m) the back yard of the row's furthest-set house runs behind a large plan's
/// back wall to the lot's, the rest of the row's yards as much deeper as their houses
/// stand forward or are shallower, so a row's back walls run in one line, 14 m from the
/// kerb: the tract's run 1.5-6 m from the house to the block wall, paved, with a planter
/// along the wall.
const YARD: f64 = 2.2;
/// A yard is cut short of the greenbelt's lane, a pickup, a spawn pad and the level's
/// wall by the room each keeps from a lot's wall and this much more (m), but never under
/// the least yard (m).
const YARD_MARGIN: f64 = 0.3;
const LEAST_YARD: f64 = 1.0;
/// A back wall keeps this much room (m) from the level's wall behind it, a way through
/// for a tank.
const PASSAGE: f64 = 4.5;
/// How far (m) behind a house its yard looks for what cuts it short, and in what steps.
const YARD_SEARCH: f64 = 25.0;
const YARD_STEP: f64 = 0.25;
/// The little park on the outside of Neblina's bend is walled behind, from the free back
/// corner of the lot either side of it: those corners lie within this far (m) of its
/// middle.
const PARK_REACH: f64 = 14.0;
/// Two houses share a lot line when they face within this (the cosine of the angle
/// between their fronts) and stand this far apart (m) at most across their fronts.
const SAME_ROW: f64 = 0.5;
const NEIGHBOUR_REACH: f64 = 1.6 * LOT_PITCH;
/// The side yard (m) beside a house at the end of its row, out to its lot line.
const END_YARD: f64 = SIDE_YARD * 0.6;
/// Room (m) a wall keeps from a spawn pad's middle (its radius and the room a tank needs
/// to leave it), from a pickup, and from the level's wall.
const PAD_ROOM: f64 = 2.3 + 2.5;
const PICKUP_ROOM: f64 = 3.0;
const EDGE_ROOM: f64 = 1.5;
/// A run's end may meet a house's wall: only this far inside one is it standing in it.
const HOUSE_ROOM: f64 = 0.05;
/// A gate return shorter than this (m) is left out.
const LEAST_RETURN: f64 = 0.3;
/// The greenbelt's lane stays this clear (m) of walls either side of its middle, so it
/// stays a way through between the back walls from its mouth on Neblina to the
/// hillside.
const LANE_ROOM: f64 = 2.6;
/// How far apart (m) the greenbelt lane's points lie.
const LANE_STEP: f64 = 1.0;

/// One side lot line: its corner at the gate line, its corner at the back wall, and the
/// neighbour who shares it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LotLine {
    pub gate: DVec2,
    pub rear: DVec2,
    pub neighbour: Option<usize>,
}

/// A house's lot in its own frame: `front` points to its street, `across` along its
/// front; `u` is measured across, `v` toward the street, both from the house's middle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lot {
    pub house: SendaHouse,
    pub centre: DVec2,
    pub front: DVec2,
    pub across: DVec2,
    pub half_width: f64,
    pub half_depth: f64,
    /// How deep (m) its back yard runs behind the house.
    pub yard: f64,
    /// Its lot lines, on the `-across` side and the `+across` side.
    pub sides: [LotLine; 2],
}

impl Lot {
    /// The world point at `(u, v)` in the lot's frame.
    pub fn at(&self, u: f64, v: f64) -> DVec2 {
        self.centre + self.across * u + self.front * v
    }

    /// `p` in the lot's frame.
    pub fn local(&self, p: DVec2) -> (f64, f64) {
        let offset = p - self.centre;
        (offset.dot(self.across), offset.dot(self.front))
    }

    /// Where (`v`) the gate returns cross the side yards.
    pub fn gate_v(&self) -> f64 {
        self.half_depth - HOUSE_DEPTH * GATE_SHARE
    }

    /// Where (`v`) the back wall runs.
    pub fn rear_v(&self) -> f64 {
        -(self.half_depth + self.yard)
    }

    /// The lot line's `u` on side `k` (0 the `-across` side), at the back wall.
    pub fn line_u(&self, k: usize) -> f64 {
        self.local(self.sides[k].rear).0
    }

    /// How far `p` lies inside the lot, between its lot lines, its back wall and the
    /// sidewalk (negative outside).
    pub fn inside(&self, p: DVec2) -> f64 {
        let (u, v) = self.local(p);
        let (low, high) = (self.line_u(0), self.line_u(1));
        let sidewalk = self.half_depth + self.house.setback;
        (u - low)
            .min(high - u)
            .min(v - self.rear_v())
            .min(sidewalk - v)
    }

    /// Whether `p` stands inside the house's walls, by more than `inset`.
    pub fn in_house(&self, p: DVec2, inset: f64) -> bool {
        let (u, v) = self.local(p);
        u.abs() < self.half_width - inset && v.abs() < self.half_depth - inset
    }
}

/// What cuts a back yard short: the spawn pads and pickups a wall keeps its room from,
/// the level's wall, and the streets whose rows back onto each other across the
/// greenbelt, whose lane runs halfway between them.
struct YardLimits {
    pads: Vec<DVec2>,
    pickups: Vec<DVec2>,
    boundary: Vec<DVec2>,
    streets: [Line; 2],
}

impl YardLimits {
    fn new() -> Self {
        YardLimits {
            pads: spawn_pads(),
            pickups: pickup_spots(),
            boundary: senda_boundary(),
            streets: [Street::Rumbo.line(), Street::NeblinaGaviota.line()],
        }
    }

    /// How deep `lot`'s back yard may run: as deep as it wants, short of the greenbelt's
    /// lane where a row backs onto another across it, of a pickup or a pad behind it and
    /// of the level's wall, but never under [`LEAST_YARD`].
    fn cut(&self, lot: &Lot) -> f64 {
        let rear = lot.at(0.0, -lot.half_depth);
        let mut yard = lot.yard;
        // The street the lot fronts and the other: halfway between them lies the lane.
        let (own, other) =
            if self.streets[0].distance(lot.centre) < self.streets[1].distance(lot.centre) {
                (&self.streets[0], &self.streets[1])
            } else {
                (&self.streets[1], &self.streets[0])
            };
        let reach = lot.half_width + SIDE_YARD;
        let mut t = 0.0;
        while t < YARD_SEARCH {
            let behind = rear - lot.front * t;
            if other.distance(behind) < own.distance(behind) {
                yard = yard.min(t - LANE_ROOM - YARD_MARGIN);
                break;
            }
            t += YARD_STEP;
        }
        // Its back wall's middle and corners each keep a tank's way through from the
        // level's wall, whichever way that runs.
        let clear = |yard: f64| {
            [-reach, 0.0, reach].into_iter().all(|u| {
                let corner = rear + lot.across * u - lot.front * yard;
                (0..8).all(|k| {
                    let angle = f64::from(k) * std::f64::consts::FRAC_PI_4;
                    let way = DVec2::new(angle.cos(), angle.sin());
                    inside_boundary(&self.boundary, corner + way * PASSAGE)
                })
            })
        };
        while yard > LEAST_YARD && !clear(yard) {
            yard -= YARD_STEP;
        }
        for (spots, room) in [(&self.pickups, PICKUP_ROOM), (&self.pads, PAD_ROOM)] {
            for &spot in spots {
                let (u, v) = lot.local(spot);
                if u.abs() < reach + room && v < -lot.half_depth {
                    yard = yard.min(-v - lot.half_depth - room - YARD_MARGIN);
                }
            }
        }
        yard.max(LEAST_YARD)
    }
}

/// The spawn pads' middles, both teams'.
fn spawn_pads() -> Vec<DVec2> {
    [Team::Blue, Team::Red]
        .into_iter()
        .flat_map(|team| spawn_positions(team, 1.0))
        .map(|spawn| DVec2::new(spawn.x, spawn.z))
        .collect()
}

/// The pickups' spots.
fn pickup_spots() -> Vec<DVec2> {
    SENDA_PICKUPS
        .iter()
        .map(|pickup| DVec2::new(pickup.x, pickup.z))
        .collect()
}

/// The free back corners of the two lots either side of the little park on the outside
/// of Neblina's bend ([`bend_park`]), the last of Neblina's west row and the first of
/// Gaviota's south row: its back wall runs between them.
pub fn park_corners(lots: &[Lot]) -> Option<(DVec2, DVec2)> {
    let park = bend_park();
    let mut corners: Vec<(f64, DVec2, DVec2)> = lots
        .iter()
        .flat_map(|lot| {
            lot.sides
                .iter()
                .filter(|side| side.neighbour.is_none())
                .map(move |side| (side.rear.distance(park), side.rear, lot.front))
        })
        .filter(|(reach, _, _)| *reach < PARK_REACH)
        .collect();
    corners.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (_, first, front) = *corners.first()?;
    // The other side's lot faces another way round the bend.
    let (_, second, _) = *corners
        .iter()
        .skip(1)
        .find(|(_, _, other)| other.dot(front) < 0.95)?;
    Some((first, second))
}

/// The furthest (m) a house steps back from its row's front, which its back yard makes
/// up so the row's back walls run in one line.
fn deepest_step() -> f64 {
    back_fence() - house_front() - HOUSE_DEPTH - BACK_YARD
}

/// The distance (m) from a street's centre line to the back walls of the lots along it.
pub fn back_wall() -> f64 {
    house_front() + deepest_step() + HOUSE_DEPTH + YARD
}

/// Every house's lot, in the order of `houses`.
pub fn senda_lots(houses: &[SendaHouse]) -> Vec<Lot> {
    let deepest = deepest_step();
    let limits = YardLimits::new();
    let frames: Vec<(DVec2, DVec2, f64, f64)> = houses
        .iter()
        .map(|house| {
            let design = house.design();
            let (_, out, _, half) = wall_frames(design.w, design.d)[design.door_wall];
            let out = design.to_world(out);
            (
                DVec2::new(out.x, out.z).normalize(),
                house.lot_centre(),
                half,
                // The lot is a full house deep whatever its plan: a smaller house stands in
                // its front part, its back yard the deeper.
                HOUSE_DEPTH / 2.0,
            )
        })
        .collect();
    let mut lots: Vec<Lot> = houses
        .iter()
        .zip(&frames)
        .map(|(house, &(front, centre, half_width, half_depth))| {
            let stagger = house.setback - SETBACK;
            let mut lot = Lot {
                house: *house,
                centre,
                front,
                across: left(front),
                half_width,
                half_depth,
                yard: YARD + deepest - stagger,
                sides: [LotLine {
                    gate: centre,
                    rear: centre,
                    neighbour: None,
                }; 2],
            };
            lot.yard = limits.cut(&lot);
            lot
        })
        .collect();
    // Each side's own corners, from its nearest neighbour in the row: halfway across
    // the side yard between their walls.
    for i in 0..lots.len() {
        for (k, sign) in [-1.0, 1.0].into_iter().enumerate() {
            let lot = lots[i];
            let neighbour = (0..lots.len())
                .filter(|&j| j != i && lots[j].front.dot(lot.front) > SAME_ROW)
                .filter_map(|j| {
                    let (u, v) = lot.local(lots[j].centre);
                    (u * sign > 0.0 && u.abs() < NEIGHBOUR_REACH && v.abs() < lot.half_depth)
                        .then_some((j, u))
                })
                .min_by(|a, b| a.1.abs().total_cmp(&b.1.abs()));
            let u = match neighbour {
                Some((j, u)) => (sign * lot.half_width + u - sign * lots[j].half_width) / 2.0,
                None => sign * (lot.half_width + END_YARD),
            };
            lots[i].sides[k] = LotLine {
                gate: lot.at(u, lot.gate_v()),
                rear: lot.at(u, lot.rear_v()),
                neighbour: neighbour.map(|(j, _)| j),
            };
        }
    }
    // Neighbours who name each other share their line: its corners halfway between what
    // each one's frame puts there.
    let own: Vec<[LotLine; 2]> = lots.iter().map(|lot| lot.sides).collect();
    for (i, lot) in lots.iter_mut().enumerate() {
        for side in &mut lot.sides {
            let Some(j) = side.neighbour else {
                continue;
            };
            match own[j].iter().find(|other| other.neighbour == Some(i)) {
                Some(other) => {
                    side.gate = (side.gate + other.gate) / 2.0;
                    side.rear = (side.rear + other.rear) / 2.0;
                }
                None => side.neighbour = None,
            }
        }
    }
    lots
}

/// What a stretch of a lot's wall is built as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallLook {
    /// Split-face block, between neighbours and behind the rows that back onto the slope
    /// down to Gardenia.
    Block,
    /// The tract's dark brown perimeter block, behind the row that backs onto Senda
    /// Way's west branch.
    Perimeter,
    /// A steel view fence on a low block base, toward open ground or a street.
    View,
    /// A gate return: boards between two block piers.
    Gate,
}

impl WallLook {
    /// The cover's paint, which picks its look.
    pub fn color(self) -> u32 {
        match self {
            WallLook::Block => YARD_WALL_COLOR,
            WallLook::Perimeter => YARD_PERIMETER_COLOR,
            WallLook::View => YARD_VIEW_COLOR,
            WallLook::Gate => YARD_GATE_COLOR,
        }
    }
}

/// A wall's line: from `a` to `b` on the ground, and what it is built as.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct YardWall {
    pub a: DVec2,
    pub b: DVec2,
    pub look: WallLook,
}

/// Whether a lot's back line looks onto open ground: the bank under Senda Parkway behind
/// Rumbo's north row, the greenbelt behind Rumbo's south row and Gaviota's north row.
/// Neblina's west row backs onto Senda Parkway's west branch and Gaviota's south row onto
/// the slope down to Gardenia, behind the tract's solid perimeter block.
fn backs_onto_open_ground(row: Row) -> bool {
    matches!(row, Row::RumboNorth | Row::RumboSouth | Row::GaviotaNorth)
}

/// Every lot's walls as whole lines: its back wall corner to corner (a view fence where
/// it backs onto open ground), each side lot line once from its back wall to its gate
/// line (block between neighbours, a view fence on a row's end toward the street or the
/// hillside), and its two gate returns from the house's side walls to the lot lines; and
/// the bend park's back wall.
pub fn yard_wall_lines(lots: &[Lot]) -> Vec<YardWall> {
    let mut walls = Vec::new();
    for (i, lot) in lots.iter().enumerate() {
        let back = if backs_onto_open_ground(lot.house.row) {
            WallLook::View
        } else if lot.house.row == Row::NeblinaWest {
            WallLook::Perimeter
        } else {
            WallLook::Block
        };
        walls.push(YardWall {
            a: lot.sides[0].rear,
            b: lot.sides[1].rear,
            look: back,
        });
        for (side, sign) in lot.sides.iter().zip([-1.0, 1.0]) {
            // A shared line belongs to the first of its two lots.
            if side.neighbour.is_none_or(|j| i < j) {
                walls.push(YardWall {
                    a: side.rear,
                    b: side.gate,
                    look: if side.neighbour.is_some() {
                        WallLook::Block
                    } else {
                        WallLook::View
                    },
                });
            }
            let wall = lot.at(sign * lot.half_width, lot.gate_v());
            if wall.distance(side.gate) > LEAST_RETURN {
                walls.push(YardWall {
                    a: wall,
                    b: side.gate,
                    look: WallLook::Gate,
                });
            }
        }
    }
    // The bend park's back wall closes it from the field behind.
    if let Some((a, b)) = park_corners(lots) {
        walls.push(YardWall {
            a,
            b,
            look: WallLook::Block,
        });
    }
    walls
}

/// The lots' walls as cover: each line in runs of at most [`WALL_RUN`], leaving out a
/// run that would stand on paving, in a house, outside the level's wall or too near a spawn pad
/// or a pickup.
pub fn yard_walls(houses: &[SendaHouse]) -> Vec<CoverDef> {
    let lots = senda_lots(houses);
    let pads = spawn_pads();
    let pickups = pickup_spots();
    let lane = Line(greenbelt_lane());
    // The level's wall runs at right angles, so a point this far inside it every way
    // along the axes keeps its room from every run.
    let boundary = senda_boundary();
    let off_edge = |p: DVec2| {
        [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)]
            .into_iter()
            .all(|(u, v)| inside_boundary(&boundary, p + DVec2::new(u, v) * EDGE_ROOM))
    };
    let clear = |a: DVec2, b: DVec2| {
        let samples = ((a.distance(b) / 0.5).ceil() as usize).max(1);
        let points = (0..=samples).map(|k| a.lerp(b, k as f64 / samples as f64));
        let on_ground = points.clone().all(|p| {
            !is_senda_paved(p.x, p.y)
                && (lane.0.len() < 2 || lane.distance(p) > LANE_ROOM)
                && off_edge(p)
                && !lots.iter().any(|lot| lot.in_house(p, HOUSE_ROOM))
        });
        on_ground
            && pads
                .iter()
                .all(|&pad| segment_distance(pad, a, b) > PAD_ROOM)
            && pickups
                .iter()
                .all(|&pickup| segment_distance(pickup, a, b) > PICKUP_ROOM)
    };
    let mut covers = Vec::new();
    for wall in yard_wall_lines(&lots) {
        let length = wall.a.distance(wall.b);
        let runs = (length / WALL_RUN).ceil().max(1.0) as usize;
        for k in 0..runs {
            let a = wall.a.lerp(wall.b, k as f64 / runs as f64);
            let b = wall.a.lerp(wall.b, (k + 1) as f64 / runs as f64);
            if clear(a, b) {
                covers.push(wall_run(a, b, wall.look));
            }
        }
    }
    covers
}

/// The greenbelt's lane: its middle between the back gardens of Rumbo's south row and
/// Gaviota's north row, carried on straight at its west end out through its mouth to
/// Neblina's sidewalk and at its east end on past the cul-de-sacs' lots onto the
/// hillside; empty where the greenbelt has no room for one.
pub fn greenbelt_lane() -> Vec<DVec2> {
    let middle = greenbelt_line(0.0);
    if middle.len() < 2 {
        return middle;
    }
    let neblina = Street::NeblinaGaviota.line();
    let walk = STREET_HALF + PARKWAY + SIDEWALK;
    let n = middle.len();
    let west = (middle[0] - middle[1.min(n - 1)]).normalize();
    let east = (middle[n - 1] - middle[n.saturating_sub(4)]).normalize();
    let mut lead: Vec<DVec2> = Vec::new();
    let mut p = middle[0] + west * LANE_STEP;
    while neblina.distance(p) > walk && p.x.abs().max(p.y.abs()) < ARENA && lead.len() < 40 {
        lead.push(p);
        p += west * LANE_STEP;
    }
    lead.reverse();
    let mut lane: Vec<DVec2> = lead.into_iter().chain(middle).collect();
    let mut p = lane[lane.len() - 1] + east * LANE_STEP;
    while p.x.abs().max(p.y.abs()) < ARENA {
        lane.push(p);
        p += east * LANE_STEP;
    }
    lane
}

/// The least distance from `p` to the segment from `a` to `b`.
fn segment_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let span = b - a;
    let u = ((p - a).dot(span) / span.length_squared().max(1e-9)).clamp(0.0, 1.0);
    p.distance(a + span * u)
}

/// A run of wall from `a` to `b` as a turned cover. Its box is its own, the long side
/// along whichever world axis it lies nearer, turned the rest of the way by its plan's
/// whole degrees, as a house's is; a run overlaps its neighbours by half its thickness
/// at each end so the corners close.
fn wall_run(a: DVec2, b: DVec2, look: WallLook) -> CoverDef {
    let middle = (a + b) / 2.0;
    let length = a.distance(b) + YARD_WALL_THICKNESS;
    let along = (b - a).normalize();
    // A turn about +y carries the build frame's x to (cos, -sin) in the world.
    let mut turn = (-along.y).atan2(along.x).to_degrees();
    if turn > 90.0 {
        turn -= 180.0;
    } else if turn <= -90.0 {
        turn += 180.0;
    }
    let (w, d, angle) = if turn.abs() <= 45.0 {
        (length, YARD_WALL_THICKNESS, turn)
    } else {
        // Along the build frame's z, a quarter turn back toward the world's.
        (YARD_WALL_THICKNESS, length, turn - 90.0f64.copysign(turn))
    };
    let color = look.color();
    CoverDef {
        // The plan carries only the run's turn; a wall has no house's style or garage.
        house_plan: Some(HousePlan {
            style: 0,
            street: 0,
            angle: angle.round() as i8,
            garage: GarageSide::Left,
        }),
        ..CoverDef::new(
            CoverKind::Concrete,
            (middle.x * 100.0).round() / 100.0,
            (middle.y * 100.0).round() / 100.0,
            w,
            d,
            YARD_WALL_HEIGHT,
            YARD_WALL_HEALTH,
            color,
        )
    }
}

/// The planting strip (m) along a back yard's walls; the rest of the yard is paved.
pub const YARD_PLANTER: f64 = 0.5;
/// The one-storey rear loggias over part of the patio on most lots, and the pergolas on a
/// few: their posts' side, the roof's height at its outer beam and where it meets the
/// house (m), how wide across the lot and how far out from the house their roofs reach at
/// most (m), and the least depth (m) one is built at.
pub const PATIO_POST: f64 = 0.12;
pub const PATIO_ROOF: (f64, f64) = (1.75, 2.1);
pub const PATIO_COVER: (f64, f64) = (3.6, 2.0);
const LEAST_PATIO_COVER: f64 = 1.2;
/// The patch the house model's single-storey projecting room and entry porch take before
/// its front, on the half of the front away from the garage (the model's is about 2.8 by
/// 1.6): its width along the front and how far it stands out from the wall (m), the least
/// wall (m) it keeps to the house's corner, and the least room (m) it leaves before the
/// sidewalk. The front yard keeps that patch clear of its driveway, beds and shrubs.
pub const PORCH: (f64, f64) = (3.0, 1.8);
const PORCH_CORNER: f64 = 0.05;
const PORCH_WALK_ROOM: f64 = 0.2;
/// The rear loggias' square stucco columns and the depth of the beam they carry (m), and
/// how far (m) their tile roof overhangs past the beam and the ends.
pub const LOGGIA_COLUMN: (f64, f64) = (0.3, 0.3);
pub const LOGGIA_EAVE: f64 = 0.3;
/// The pergolas' stained wood.
pub const PERGOLA_WOOD: u32 = 0x8a6c50;
/// A loggia's colour as a cover: its look's mark (its own paint is its house's style's)
/// and the colour its rubble falls in, a cream stucco no other cover wears.
pub const LOGGIA_COLOR: u32 = 0xd9c9a7;
/// How much (hp) a loggia and a pergola take to bring down.
const LOGGIA_HEALTH: f64 = 140.0;
const PERGOLA_HEALTH: f64 = 70.0;

/// Where the house's back wall stands in its lot (`v` from the lot's centre): a full
/// house deep's back, or nearer the street for a smaller plan, whose patio runs out from
/// its own wall.
pub fn house_back(lot: &Lot) -> f64 {
    -lot.half_depth + (HOUSE_DEPTH - lot.house.d)
}

/// How deep (m) the back yard runs from the house's back wall to the lot's.
pub fn back_yard(lot: &Lot) -> f64 {
    house_back(lot) - lot.rear_v()
}

/// How deep (m) a lot's patio runs out from the house's back wall: up to the planting
/// strip along the back wall.
pub fn patio_depth(lot: &Lot) -> f64 {
    (back_yard(lot) - YARD_PLANTER - 0.1).max(0.0)
}

/// Where the front door and the garage door stand across `lot`'s front (`u`).
pub fn front_openings(lot: &Lot) -> Option<(f64, f64)> {
    let design = lot.house.design();
    let (centre, _, along, _) = wall_frames(design.w, design.d)[design.door_wall];
    let plan = design.wall_plan(design.door_wall);
    let in_lot = |u: f64| {
        let at = design.to_world(centre + along * u);
        lot.local(DVec2::new(lot.house.x + at.x, lot.house.z + at.z))
            .0
    };
    Some((in_lot(plan.door()?), in_lot(plan.garage()?)))
}

/// The patch a lot's entry porch takes: its middle across the front (`u`), its width and
/// how far it stands out from the house's front. It stands over the front door, as far
/// as the half of the front away from the garage holds it.
pub fn porch(lot: &Lot) -> Option<(f64, f64, f64)> {
    let (door, garage) = front_openings(lot)?;
    let side = (door - garage).signum();
    let width = PORCH.0;
    // From the middle of the front, or the driveway's edge where the garage door reaches
    // past it, to the corner.
    let design = lot.house.design();
    let driveway = design.wall_plan(design.door_wall).garage_width() / 2.0;
    let start = ((garage + side * driveway) * side).max(0.0);
    let width = width.min(lot.half_width - PORCH_CORNER - start);
    let near = start + width / 2.0;
    let far = (lot.half_width - PORCH_CORNER - width / 2.0).max(near);
    let middle = (door * side).clamp(near, far) * side;
    Some((middle, width, porch_depth(lot.house.setback)))
}

/// How far (m) the porch patch stands out from a house's front that stands `setback` (m)
/// behind the sidewalk.
pub fn porch_depth(setback: f64) -> f64 {
    PORCH.1.min(setback - PORCH_WALK_ROOM)
}

/// The side of a lot (+1 its `+across` side) its turf patch and patio cover take; its
/// spa or potted tree takes the other.
pub fn turf_side(k: usize) -> f64 {
    if k.is_multiple_of(2) { 1.0 } else { -1.0 }
}

/// What covers a lot's patio, by its place among the lots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatioCover {
    /// A one-storey rear loggia: a shed roof in the house's tile on stucco columns.
    Loggia,
    /// A slatted pergola on timber posts.
    Pergola,
}

impl PatioCover {
    pub fn of_lot(k: usize) -> Option<Self> {
        match k % 10 {
            0 | 2 | 3 | 5 | 7 | 8 => Some(Self::Loggia),
            9 => Some(Self::Pergola),
            _ => None,
        }
    }

    /// Which a cover is, by its colour (both are concrete covers drawn their own way).
    pub fn of_color(color: u32) -> Option<Self> {
        match color {
            LOGGIA_COLOR => Some(Self::Loggia),
            PERGOLA_WOOD => Some(Self::Pergola),
            _ => None,
        }
    }
}

/// The loggias and pergolas over the lots' patios, as destructible covers: each a box
/// against the house's back wall, from its posts out past its roof's overhang, turned
/// square to its lot (the plan carries the turn and, for a loggia, the house's style,
/// whose paint and tile it wears), short of the planting strip along the back wall; a
/// yard too shallow for one keeps none. Its box's own +z points at the house.
pub fn patio_covers(houses: &[SendaHouse]) -> Vec<CoverDef> {
    let (low, eave) = PATIO_ROOF;
    senda_lots(houses)
        .iter()
        .enumerate()
        .filter_map(|(k, lot)| {
            let look = PatioCover::of_lot(k)?;
            let width = PATIO_COVER.0.min(2.0 * lot.half_width - 0.4);
            let middle = turf_side(k) * (lot.half_width - width / 2.0 - 0.2);
            let reach = (back_yard(lot) - YARD_PLANTER - 0.2).min(PATIO_COVER.1);
            if reach < LEAST_PATIO_COVER {
                return None;
            }
            let (h, color, health) = match look {
                PatioCover::Loggia => (eave + 0.2, LOGGIA_COLOR, LOGGIA_HEALTH),
                PatioCover::Pergola => (low + 0.2, PERGOLA_WOOD, PERGOLA_HEALTH),
            };
            let at = lot.at(middle, house_back(lot) - reach / 2.0);
            Some(roofed_cover(
                lot,
                at,
                lot.front,
                (width, reach, h),
                color,
                health,
            ))
        })
        .collect()
}

/// A loggia's or pergola's cover box at `at`, `size` across its lot, out from the house
/// and high, turned so its own +z points along `toward_house`; its plan carries the turn
/// and its house's style.
fn roofed_cover(
    lot: &Lot,
    at: DVec2,
    toward_house: DVec2,
    (w, d, h): (f64, f64, f64),
    color: u32,
    health: f64,
) -> CoverDef {
    let turn = toward_house.x.atan2(toward_house.y).to_degrees();
    let quarter = (turn / 90.0).round();
    CoverDef {
        house_plan: Some(HousePlan {
            style: lot.house.plan.style,
            street: quarter as i8,
            angle: (turn - quarter * 90.0).round() as i8,
            garage: GarageSide::Left,
        }),
        ..CoverDef::new(
            CoverKind::Concrete,
            (at.x * 100.0).round() / 100.0,
            (at.y * 100.0).round() / 100.0,
            w,
            d,
            h,
            health,
            color,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::senda_layout::senda_houses;

    #[test]
    fn neblina_west_backs_onto_the_dark_perimeter_block_and_no_other_row_does() {
        let lots = senda_lots(&senda_houses());
        let walls = yard_wall_lines(&lots);
        let mut west = 0;
        for lot in &lots {
            let back = walls
                .iter()
                .find(|wall| wall.a == lot.sides[0].rear && wall.b == lot.sides[1].rear)
                .expect("every lot has a back wall");
            let perimeter = back.look == WallLook::Perimeter;
            assert_eq!(perimeter, lot.house.row == Row::NeblinaWest);
            west += usize::from(perimeter);
        }
        assert!(west > 0, "Neblina's west row has lots");
        assert!(
            walls
                .iter()
                .filter(|wall| wall.look == WallLook::Perimeter)
                .all(|wall| wall.a.x < 0.0),
            "the perimeter block runs along the tract's west side"
        );
    }
}
