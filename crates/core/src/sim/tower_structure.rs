//! A watchtower as a structure: the members it is built of (two concrete footings,
//! four legs in two lifts, the X-braces and girts on its four faces, the two deck
//! halves, the railing, the cabin walls, the roof and the ladder), what each is joined
//! to, and whether what stands of it can stand.
//!
//! Each standing part is a collider of its own on the tower's fixed body, so a shell
//! between the legs and under the braces flies through untouched, and one that strikes
//! a leg strikes that leg. The tower stands as one braced frame: every part joined to
//! the frame goes with it, and the frame stands while it is anchored in its footings
//! and, carrying the deck, while its weight lies over the legs still in their shoes.
//! Shoot a leg out from under it and its weight is no longer over what holds it: it
//! tips toward the missing leg, the deck, cabin and roof with it. No count of hits
//! decides it, and no random draw.
//!
//! Everything is in tower coordinates: the tower's ground centre at the origin, x
//! across its footings (west -x), z along them (front +z, the ladder and door side).

use std::sync::LazyLock;

use glam::{DMat3, DQuat, DVec2, DVec3};

use super::house_structure::PartSet;
use super::tower_layout::{
    AWNING_BOTTOM, AWNING_REACH, AWNING_TOP, BENT_BRACE_LOW, CABIN_TOP, CABIN_WALL, CABIN_X,
    CABIN_Z, DECK_TOP, DECK_X, DECK_Z, FACE_BRACE_LOW, GIRT_Y, KNEE_LOW, LADDER_HALF, LADDER_TOP,
    LADDER_X, LADDER_Z, OUTRIGGER_Y, POST, POST_TOP, RAIL_HEIGHT, RAIL_INSET, ROOF_EDGE,
    ROOF_OVERHANG, TOWER_BASE, roof_apex,
};

/// What a part is: its strength, its weight and how the model draws it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TowerPartKind {
    /// A concrete footing with its steel shoes: it never breaks.
    Footing,
    /// A leg's lift from its shoe to the girts.
    LowerLeg,
    /// A leg's lift from the girts to the beams, with its knee brace.
    UpperLeg,
    /// One diagonal of a face's X-brace.
    Brace,
    /// The girt across a face at the lifts' joint.
    Girt,
    /// Half the deck: beams, joists, planks and rim.
    Deck,
    /// The railing round half the deck.
    Railing,
    /// A wall of the lookout cabin.
    Wall,
    Roof,
    Ladder,
}

impl TowerPartKind {
    /// Damage (in shell damage units) the part takes before it breaks. A standard
    /// shell (40) does not break a leg it strikes, but its burst there does with the
    /// next; an armour-piercing round breaks whatever it goes through.
    pub fn strength(self) -> f64 {
        match self {
            Self::Footing => f64::INFINITY,
            Self::LowerLeg | Self::UpperLeg => 90.0,
            // Two bursts at a leg (a shell that does not break it, then the one that
            // does) leave the braces' feet beside it standing.
            Self::Brace => 60.0,
            Self::Girt => 60.0,
            Self::Deck => 120.0,
            Self::Railing => 30.0,
            Self::Wall => 60.0,
            Self::Roof => 100.0,
            Self::Ladder => 40.0,
        }
    }
}

/// The four legs' corners, west-back, west-front, east-back, east-front, as (x, z)
/// signs.
pub const CORNERS: [(f64, f64); 4] = [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)];

/// Part indices.
pub const FOOTINGS: [usize; 2] = [0, 1];
pub const LOWER_LEGS: [usize; 4] = [2, 3, 4, 5];
pub const UPPER_LEGS: [usize; 4] = [6, 7, 8, 9];
/// Each face's outer diagonal, inner diagonal and girt: the west and east bents, then
/// the back and front faces.
pub const FACES: [[usize; 3]; 4] = [[10, 11, 12], [13, 14, 15], [16, 17, 18], [19, 20, 21]];
pub const DECKS: [usize; 2] = [22, 23];
pub const RAILINGS: [usize; 2] = [24, 25];
/// The cabin's front, back, west and east walls.
pub const WALLS: [usize; 4] = [26, 27, 28, 29];
pub const ROOF: usize = 30;
pub const LADDER: usize = 31;
pub const PART_COUNT: usize = 32;

/// The frame carrying the deck stands while its weight lies at least this far (m)
/// inside the legs in their shoes: a tower missing a leg has its weight on the line
/// between the two legs beside the gap, and tips.
pub const STAND_MARGIN: f64 = 0.35;

/// Mass per cubic metre of solid timber and steel, in the debris' units.
const TIMBER: f64 = 0.35;
const STEEL: f64 = 0.8;
/// The share of a deck half's box that is timber (planks, joists, beams), and the
/// thickness (m) of solid wood a cabin wall and the roof come to.
const DECK_SOLID: f64 = 0.25;
const WALL_SOLID: f64 = 0.05;
const ROOF_SOLID: f64 = 0.04;
/// Where each leg snaps when its tower lands (m up), one height a corner so the legs
/// break unalike.
pub const LEG_SNAPS: [f64; 4] = [1.9, 2.6, 3.2, 2.2];
/// How far (m) over its footing a torn-out leg's stub stands in its shoe; a landed leg
/// starts there.
pub const STUB_REACH: f64 = 0.45;
/// Where a deck half breaks across its planks (m out from the middle).
pub const DECK_CUT: f64 = 1.5;
/// The cabin's door: its middle along the front wall, width and height
/// (`tower_model::cabin_wall`), and the shutters a landing tears off: their middles
/// across x on the front and back walls and their width.
pub const DOOR_U: f64 = 1.35;
pub const DOOR_WIDTH: f64 = 0.8;
pub const DOOR_HEIGHT: f64 = 1.85;
pub const SHUTTER_X: [f64; 2] = [-1.25, 1.3];
pub const SHUTTER_WIDTH: f64 = 1.17;
/// How far (m) under the beams a knee brace's collider stops.
const KNEE_CLEAR: f64 = 0.1;
/// Parts reaching below this height block tanks on the ground.
pub const GROUND_BLOCK: f64 = 1.2;

/// A straight timber member from `from` to `to`: `width` in the plane it braces,
/// `depth` along `normal` (the plane's normal). The model draws it and its collider
/// is its box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Member {
    pub from: DVec3,
    pub to: DVec3,
    pub width: f64,
    pub depth: f64,
    pub normal: DVec3,
}

impl Member {
    /// The member's box: its centre, half size and turn (x along it, z along the
    /// normal).
    pub fn frame(&self) -> (DVec3, DVec3, DQuat) {
        let along = self.to - self.from;
        let x = along.normalize();
        let z = (self.normal - x * self.normal.dot(x)).normalize();
        let y = z.cross(x);
        (
            (self.from + self.to) / 2.0,
            DVec3::new(along.length(), self.width, self.depth) / 2.0,
            DQuat::from_mat3(&DMat3::from_cols(x, y, z)),
        )
    }

    fn shape(&self) -> PartShape {
        let (center, half, rotation) = self.frame();
        PartShape::Box {
            center,
            half,
            rotation,
        }
    }

    fn mass(&self) -> f64 {
        self.from.distance(self.to) * self.width * self.depth * TIMBER
    }
}

/// A piece of a part's collider in tower coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum PartShape {
    Box {
        center: DVec3,
        half: DVec3,
        rotation: DQuat,
    },
    /// A convex hull of points (the roof).
    Hull(Vec<DVec3>),
}

impl PartShape {
    fn aligned(min: DVec3, max: DVec3) -> Self {
        Self::Box {
            center: (min + max) / 2.0,
            half: (max - min) / 2.0,
            rotation: DQuat::IDENTITY,
        }
    }

    /// The shape's corner points.
    pub fn points(&self) -> Vec<DVec3> {
        match self {
            Self::Box {
                center,
                half,
                rotation,
            } => (0..8)
                .map(|k| {
                    let corner = DVec3::new(
                        if k & 1 == 0 { -half.x } else { half.x },
                        if k & 2 == 0 { -half.y } else { half.y },
                        if k & 4 == 0 { -half.z } else { half.z },
                    );
                    *center + *rotation * corner
                })
                .collect(),
            Self::Hull(points) => points.clone(),
        }
    }
}

/// One part of a watchtower.
#[derive(Clone, Debug, PartialEq)]
pub struct TowerPart {
    pub kind: TowerPartKind,
    /// The pieces of its collider.
    pub shapes: Vec<PartShape>,
    /// Its bounding box.
    pub min: DVec3,
    pub max: DVec3,
    /// Its weight and where it acts.
    pub mass: f64,
    pub center: DVec3,
    pub strength: f64,
    /// A leg's corner ([`CORNERS`]).
    pub corner: Option<usize>,
    /// The parts it is joined to, lowest first.
    pub ties: Vec<u16>,
}

impl TowerPart {
    fn new(kind: TowerPartKind, shapes: Vec<PartShape>, mass: f64) -> Self {
        let (min, max) = shapes.iter().flat_map(PartShape::points).fold(
            (DVec3::INFINITY, DVec3::NEG_INFINITY),
            |(min, max), p| (min.min(p), max.max(p)),
        );
        Self {
            kind,
            shapes,
            min,
            max,
            mass,
            center: (min + max) / 2.0,
            strength: kind.strength(),
            corner: None,
            ties: Vec::new(),
        }
    }

    /// How far `p` (tower coordinates) lies from the part's box.
    pub fn distance(&self, p: DVec3) -> f64 {
        (p.clamp(self.min, self.max) - p).length()
    }

    /// Whether the part blocks a tank on the ground once the tower's top is down.
    pub fn blocks_ground(&self) -> bool {
        self.min.y < GROUND_BLOCK
    }
}

/// A watchtower's parts and how they are joined. Every watchtower is built alike.
#[derive(Clone, Debug, PartialEq)]
pub struct TowerStructure {
    pub parts: Vec<TowerPart>,
    /// What a landing breaks the parts into ([`TowerStructure::landing`]), indexed from
    /// [`PART_COUNT`] on in the same sets as the parts.
    pub pieces: Vec<TowerPart>,
    /// How each piece is drawn, by piece (index less [`PART_COUNT`]).
    pub looks: Vec<PieceLook>,
}

/// How a piece a landing broke off is drawn. Members snap part way along, ragged where
/// they broke; an X-brace panel may stay joined with a diagonal bent; the cabin comes
/// apart at its corners with its glass out and its door and shutters torn off; the roof
/// lands crumpled with its rafters showing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PieceLook {
    /// A leg's run between `from` and `to` (heights), ragged at both ends, with its
    /// knee brace when it reaches the beams.
    Leg {
        corner: usize,
        from: f64,
        to: f64,
    },
    /// A snapped length of a face's outer diagonal: from its foot to the break or from
    /// the break to its head.
    Diagonal { face: usize, upper: bool },
    /// A face's X-brace still joined at its bolts with the outer diagonal bent at its
    /// middle, or (`snapped`) with that diagonal snapped there and its head gone.
    Panel { face: usize, snapped: bool },
    /// The inner or outer length of a deck half, its planks broken off raggedly.
    Deck { side: usize, outer: bool },
    /// A cabin wall torn from its corners, glass out, door and shutters gone.
    Wall { wall: usize },
    /// The cabin's door, torn off.
    Door,
    /// A window's shutter of the front (0) or back (1) wall, torn off.
    Shutter { wall: usize },
    /// The roof, crumpled across one slope, its underside open on its rafters.
    Roof,
}

/// A body of a tower's parts coming down together: its parts, and for a frame that
/// tips over, the point on the ground it turns about and the way it falls (planar x
/// and z, unit length).
#[derive(Clone, Debug, PartialEq)]
pub struct TowerFall {
    pub parts: PartSet,
    pub tip: Option<(DVec3, DVec2)>,
}

/// The structure all watchtowers share.
pub fn tower_structure() -> &'static TowerStructure {
    static STRUCTURE: LazyLock<TowerStructure> = LazyLock::new(TowerStructure::build);
    &STRUCTURE
}

/// A bent's (west `side` -1, east +1) members: its outer and inner diagonals and
/// its girt.
pub fn bent_members(side: f64) -> [Member; 3] {
    let x = side * TOWER_BASE.offset;
    let diagonal = |face: f64| {
        let bx = x + side * face * (POST / 2.0 + 0.04);
        let z = TOWER_BASE.post_z * face;
        Member {
            from: DVec3::new(bx, BENT_BRACE_LOW, -z),
            to: DVec3::new(bx, POST_TOP - 0.2, z),
            width: 0.18,
            depth: 0.08,
            normal: DVec3::X,
        }
    };
    let span = 2.0 * TOWER_BASE.post_z - POST;
    [
        diagonal(1.0),
        diagonal(-1.0),
        Member {
            from: DVec3::new(x, GIRT_Y, -span / 2.0),
            to: DVec3::new(x, GIRT_Y, span / 2.0),
            width: 0.2,
            depth: 0.15,
            normal: DVec3::X,
        },
    ]
}

/// The back (`edge` -1) or front face's members: its outer and inner diagonals and
/// its girt.
pub fn face_members(edge: f64) -> [Member; 3] {
    let z = edge * TOWER_BASE.post_z;
    let diagonal = |face: f64| {
        let bz = z + edge * face * (POST / 2.0 + 0.04);
        let x = TOWER_BASE.offset * face;
        Member {
            from: DVec3::new(-x, FACE_BRACE_LOW, bz),
            to: DVec3::new(x, POST_TOP - 0.25, bz),
            width: 0.2,
            depth: 0.08,
            normal: DVec3::Z,
        }
    };
    let span = 2.0 * TOWER_BASE.offset - POST;
    [
        diagonal(1.0),
        diagonal(-1.0),
        Member {
            from: DVec3::new(-span / 2.0, GIRT_Y, z),
            to: DVec3::new(span / 2.0, GIRT_Y, z),
            width: 0.2,
            depth: 0.15,
            normal: DVec3::Z,
        },
    ]
}

/// The knee brace from the leg at `corner` out to the outrigger beam under the
/// deck's overhang.
pub fn knee_member(corner: usize) -> Member {
    let (sx, edge) = CORNERS[corner];
    let x = sx * TOWER_BASE.offset;
    let z = edge * TOWER_BASE.post_z;
    Member {
        from: DVec3::new(x, KNEE_LOW, z + edge * (POST / 2.0 - 0.02)),
        to: DVec3::new(x, OUTRIGGER_Y - 0.06, edge * (DECK_Z - 0.45) - edge * 0.08),
        width: 0.15,
        depth: 0.15,
        normal: DVec3::X,
    }
}

/// A knee brace's collider: the member, stopping under the beams where the deck's
/// collider begins.
fn knee_collider(corner: usize) -> Member {
    let knee = knee_member(corner);
    let reach = (POST_TOP - KNEE_CLEAR - knee.from.y) / (knee.to.y - knee.from.y);
    Member {
        to: knee.from.lerp(knee.to, reach),
        ..knee
    }
}

/// Face `face`'s outer diagonal, inner diagonal and girt ([`FACES`] order).
pub fn face_members_of(face: usize) -> [Member; 3] {
    match face {
        0 => bent_members(-1.0),
        1 => bent_members(1.0),
        2 => face_members(-1.0),
        _ => face_members(1.0),
    }
}

/// The corner `(sx, sz)` signs name.
fn corner_at(sx: f64, sz: f64) -> usize {
    CORNERS
        .iter()
        .position(|&corner| corner == (sx, sz))
        .expect("a corner")
}

/// The corner at which a face's diagonal stands low and the one it reaches up to: an
/// outer diagonal rises toward +z on a bent and toward +x on a face, an inner one the
/// other way ([`bent_members`], [`face_members`]).
fn diagonal_corners(face: usize, outer: bool) -> (usize, usize) {
    let rise = if outer { 1.0 } else { -1.0 };
    if face < 2 {
        let sx = if face == 0 { -1.0 } else { 1.0 };
        (corner_at(sx, -rise), corner_at(sx, rise))
    } else {
        let sz = if face == 2 { -1.0 } else { 1.0 };
        (corner_at(-rise, sz), corner_at(rise, sz))
    }
}

/// The two corners a face's girt joins.
fn face_corners(face: usize) -> [usize; 2] {
    match face {
        0 => [0, 1],
        1 => [2, 3],
        2 => [0, 2],
        _ => [1, 3],
    }
}

/// A cabin wall's box and its awning's: the front and back walls (`along_x`) run
/// between the side walls, `side` -1 for the back and west.
fn cabin_wall(along_x: bool, side: f64) -> [PartShape; 2] {
    if along_x {
        let x = CABIN_X - CABIN_WALL;
        let z = side * (CABIN_Z - CABIN_WALL / 2.0);
        let face = side * CABIN_Z;
        let (z0, z1) = if side < 0.0 {
            (face - AWNING_REACH, face - 0.02)
        } else {
            (face + 0.02, face + AWNING_REACH)
        };
        [
            PartShape::aligned(
                DVec3::new(-x, DECK_TOP, z - CABIN_WALL / 2.0),
                DVec3::new(x, CABIN_TOP, z + CABIN_WALL / 2.0),
            ),
            PartShape::aligned(
                DVec3::new(-CABIN_X + 0.2, AWNING_BOTTOM, z0),
                DVec3::new(CABIN_X - 0.2, AWNING_TOP, z1),
            ),
        ]
    } else {
        let x = side * (CABIN_X - CABIN_WALL / 2.0);
        let face = side * CABIN_X;
        let (x0, x1) = if side < 0.0 {
            (face - AWNING_REACH, face - 0.02)
        } else {
            (face + 0.02, face + AWNING_REACH)
        };
        [
            PartShape::aligned(
                DVec3::new(x - CABIN_WALL / 2.0, DECK_TOP, -CABIN_Z),
                DVec3::new(x + CABIN_WALL / 2.0, CABIN_TOP, CABIN_Z),
            ),
            PartShape::aligned(
                DVec3::new(x0, AWNING_BOTTOM, -CABIN_Z + 0.2),
                DVec3::new(x1, AWNING_TOP, CABIN_Z - 0.2),
            ),
        ]
    }
}

impl TowerStructure {
    fn build() -> Self {
        use TowerPartKind::*;
        let mut parts = Vec::with_capacity(PART_COUNT);
        let foot = TOWER_BASE.height;
        for side in [-1.0, 1.0] {
            let x = side * TOWER_BASE.offset;
            let (hw, hd) = (TOWER_BASE.width / 2.0, TOWER_BASE.depth / 2.0);
            parts.push(TowerPart::new(
                Footing,
                vec![PartShape::aligned(
                    DVec3::new(x - hw, 0.0, -hd),
                    DVec3::new(x + hw, foot, hd),
                )],
                0.0,
            ));
        }
        let post = |corner: usize, low: f64, high: f64| {
            let (sx, sz) = CORNERS[corner];
            let (x, z) = (sx * TOWER_BASE.offset, sz * TOWER_BASE.post_z);
            PartShape::aligned(
                DVec3::new(x - POST / 2.0, low, z - POST / 2.0),
                DVec3::new(x + POST / 2.0, high, z + POST / 2.0),
            )
        };
        for corner in 0..4 {
            let mut leg = TowerPart::new(
                LowerLeg,
                vec![post(corner, foot, GIRT_Y)],
                POST * POST * (GIRT_Y - foot) * TIMBER,
            );
            leg.corner = Some(corner);
            parts.push(leg);
        }
        for corner in 0..4 {
            // The knee brace's collider stops under the beams, where the deck's begins:
            // a leg falling apart from the deck starts clear of it.
            let knee = knee_member(corner);
            let mut leg = TowerPart::new(
                UpperLeg,
                vec![post(corner, GIRT_Y, POST_TOP), knee_collider(corner).shape()],
                POST * POST * (POST_TOP - GIRT_Y) * TIMBER + knee.mass(),
            );
            leg.corner = Some(corner);
            parts.push(leg);
        }
        let faces = [
            bent_members(-1.0),
            bent_members(1.0),
            face_members(-1.0),
            face_members(1.0),
        ];
        for members in &faces {
            for (k, member) in members.iter().enumerate() {
                let kind = if k == 2 { Girt } else { Brace };
                parts.push(TowerPart::new(kind, vec![member.shape()], member.mass()));
            }
        }
        for side in [-1.0, 1.0] {
            let (x0, x1) = if side < 0.0 { (-DECK_X, 0.0) } else { (0.0, DECK_X) };
            let min = DVec3::new(x0, POST_TOP, -DECK_Z);
            let max = DVec3::new(x1, DECK_TOP, DECK_Z);
            parts.push(TowerPart::new(
                Deck,
                vec![PartShape::aligned(min, max)],
                (max - min).element_product() * DECK_SOLID * TIMBER,
            ));
        }
        for side in [-1.0, 1.0] {
            let rail = (DECK_TOP, DECK_TOP + RAIL_HEIGHT + 0.06);
            let (rx, rz) = (DECK_X - RAIL_INSET, DECK_Z - RAIL_INSET);
            let (x0, x1) = if side < 0.0 { (-rx, 0.0) } else { (0.0, rx) };
            let runs = vec![
                PartShape::aligned(
                    DVec3::new(x0, rail.0, -rz - 0.06),
                    DVec3::new(x1, rail.1, -rz + 0.06),
                ),
                PartShape::aligned(
                    DVec3::new(x0, rail.0, rz - 0.06),
                    DVec3::new(x1, rail.1, rz + 0.06),
                ),
                PartShape::aligned(
                    DVec3::new(side * rx - 0.06, rail.0, -rz + 0.07),
                    DVec3::new(side * rx + 0.06, rail.1, rz - 0.07),
                ),
            ];
            let length = 2.0 * rx + 2.0 * rz;
            parts.push(TowerPart::new(
                Railing,
                runs,
                length * RAIL_HEIGHT * 0.02 * TIMBER,
            ));
        }
        let wall_height = CABIN_TOP - DECK_TOP;
        for (along_x, side) in [(true, 1.0), (true, -1.0), (false, -1.0), (false, 1.0)] {
            let length = 2.0 * if along_x { CABIN_X } else { CABIN_Z };
            parts.push(TowerPart::new(
                Wall,
                cabin_wall(along_x, side).to_vec(),
                length * wall_height * WALL_SOLID * TIMBER,
            ));
        }
        let (x, z) = (CABIN_X + ROOF_OVERHANG, CABIN_Z + ROOF_OVERHANG);
        let ridge = x - z;
        let top = CABIN_TOP + ROOF_EDGE;
        let mut points = Vec::new();
        for (px, pz) in [(x, z), (x, -z), (-x, z), (-x, -z)] {
            points.push(DVec3::new(px, CABIN_TOP, pz));
            points.push(DVec3::new(px, top, pz));
        }
        points.push(DVec3::new(ridge, roof_apex(), 0.0));
        points.push(DVec3::new(-ridge, roof_apex(), 0.0));
        let mut roof = TowerPart::new(
            Roof,
            vec![PartShape::Hull(points)],
            4.0 * x * z * ROOF_SOLID * TIMBER,
        );
        // A hipped roof's weight lies a third of the way up it.
        roof.center.y = top + (roof_apex() - top) / 3.0;
        parts.push(roof);
        parts.push(TowerPart::new(
            Ladder,
            vec![PartShape::aligned(
                DVec3::new(LADDER_X - LADDER_HALF - 0.03, 0.0, LADDER_Z - 0.04),
                DVec3::new(LADDER_X + LADDER_HALF + 0.03, LADDER_TOP, LADDER_Z + 0.04),
            )],
            0.03 * LADDER_TOP * STEEL,
        ));
        let mut structure = Self {
            parts,
            pieces: Vec::new(),
            looks: Vec::new(),
        };
        structure.tie_all();
        structure.add_pieces();
        structure
    }

    /// The pieces a landing breaks the parts into, in [`PieceLook`] order: each leg in
    /// two at a height of its own, the outer diagonal of each face snapped (the west
    /// and back panels stay joined, bent), the deck halves across their planks, the
    /// cabin walls, door, two shutters and the roof.
    fn add_pieces(&mut self) {
        use TowerPartKind::*;
        let mut add = |kind: TowerPartKind, look: PieceLook, shapes: Vec<PartShape>, mass: f64| {
            self.pieces.push(TowerPart::new(kind, shapes, mass));
            self.looks.push(look);
        };
        let foot = TOWER_BASE.height + STUB_REACH;
        for (corner, &(sx, sz)) in CORNERS.iter().enumerate() {
            let (x, z) = (sx * TOWER_BASE.offset, sz * TOWER_BASE.post_z);
            let snap = LEG_SNAPS[corner];
            for (from, to) in [(foot, snap), (snap, POST_TOP)] {
                let mut shapes = vec![PartShape::aligned(
                    DVec3::new(x - POST / 2.0, from, z - POST / 2.0),
                    DVec3::new(x + POST / 2.0, to, z + POST / 2.0),
                )];
                if to == POST_TOP {
                    shapes.push(knee_collider(corner).shape());
                }
                let look = PieceLook::Leg { corner, from, to };
                add(LowerLeg, look, shapes, POST * POST * (to - from) * TIMBER);
            }
        }
        for face in 0..4 {
            let [outer, inner, girt] = face_members_of(face);
            let middle = outer.from.lerp(outer.to, 0.5);
            let lower = Member { to: middle, ..outer };
            let upper = Member {
                from: middle,
                ..outer
            };
            let joined = face % 2 == 0;
            if joined {
                let shapes = vec![outer.shape(), inner.shape(), girt.shape()];
                let mass = outer.mass() + inner.mass() + girt.mass();
                add(Brace, PieceLook::Panel { face, snapped: false }, shapes, mass);
            } else {
                let shapes = vec![lower.shape(), inner.shape(), girt.shape()];
                let mass = lower.mass() + inner.mass() + girt.mass();
                add(Brace, PieceLook::Panel { face, snapped: true }, shapes, mass);
                let look = PieceLook::Diagonal { face, upper: true };
                add(Brace, look, vec![upper.shape()], upper.mass());
            }
        }
        for side in 0..2 {
            let sign = if side == 0 { -1.0 } else { 1.0 };
            for outer in [false, true] {
                let (a, b) = if outer {
                    (DECK_CUT, DECK_X)
                } else {
                    (0.0, DECK_CUT)
                };
                let (x0, x1) = if sign < 0.0 { (-b, -a) } else { (a, b) };
                let min = DVec3::new(x0, POST_TOP, -DECK_Z);
                let max = DVec3::new(x1, DECK_TOP, DECK_Z);
                let mass = (max - min).element_product() * DECK_SOLID * TIMBER;
                add(
                    Deck,
                    PieceLook::Deck { side, outer },
                    vec![PartShape::aligned(min, max)],
                    mass,
                );
            }
        }
        let wall_height = CABIN_TOP - DECK_TOP;
        for (wall, (along_x, side)) in [(true, 1.0), (true, -1.0), (false, -1.0), (false, 1.0)]
            .into_iter()
            .enumerate()
        {
            let [panel, _] = cabin_wall(along_x, side);
            let length = 2.0 * if along_x { CABIN_X } else { CABIN_Z };
            add(
                Wall,
                PieceLook::Wall { wall },
                vec![panel],
                length * wall_height * WALL_SOLID * TIMBER,
            );
        }
        // The door, in the front wall's opening (`tower_model::cabin_wall`).
        let door = PartShape::aligned(
            DVec3::new(DOOR_U - DOOR_WIDTH / 2.0, DECK_TOP, CABIN_Z - 0.12),
            DVec3::new(DOOR_U + DOOR_WIDTH / 2.0, DECK_TOP + DOOR_HEIGHT, CABIN_Z - 0.04),
        );
        add(Wall, PieceLook::Door, vec![door], DOOR_WIDTH * DOOR_HEIGHT * 0.04 * TIMBER);
        // A shutter of the front and the back wall, propped out under the eaves.
        for (wall, (x, side)) in [(SHUTTER_X[0], 1.0), (SHUTTER_X[1], -1.0)]
            .into_iter()
            .enumerate()
        {
            let half = SHUTTER_WIDTH / 2.0;
            let face = side * CABIN_Z;
            let (z0, z1) = if side < 0.0 {
                (face - AWNING_REACH, face - 0.02)
            } else {
                (face + 0.02, face + AWNING_REACH)
            };
            let shape = PartShape::aligned(
                DVec3::new(x - half, AWNING_BOTTOM, z0),
                DVec3::new(x + half, AWNING_TOP, z1),
            );
            let mass = SHUTTER_WIDTH * 0.9 * 0.04 * TIMBER;
            add(Wall, PieceLook::Shutter { wall }, vec![shape], mass);
        }
        let roof = &self.parts[ROOF];
        let (shapes, mass, center) = (roof.shapes.clone(), roof.mass, roof.center);
        add(Roof, PieceLook::Roof, shapes, mass);
        self.pieces.last_mut().expect("the roof").center = center;
    }

    /// A part ([`PART_COUNT`] and up: a piece a landing broke off) by index.
    pub fn member(&self, index: usize) -> &TowerPart {
        if index < PART_COUNT {
            &self.parts[index]
        } else {
            &self.pieces[index - PART_COUNT]
        }
    }

    /// How piece `index` (from [`PART_COUNT`]) is drawn.
    pub fn look(&self, index: usize) -> Option<PieceLook> {
        index
            .checked_sub(PART_COUNT)
            .and_then(|piece| self.looks.get(piece).copied())
    }

    /// The pieces `parts`, a falling body that landed hard, breaks into, each a body of
    /// its own: a leg with both its lifts in two at its snap, a face with all its
    /// members as its panel (and the snapped diagonal's head), a deck half in two, each
    /// cabin wall with the door and a shutter torn off it, the roof crumpled. Whatever
    /// is left of a part whose pieces are not all there (a lone lift, a brace) and the
    /// railing and ladder land whole. Pieces already broken stay as they are.
    pub fn landing(&self, parts: &PartSet) -> Vec<PartSet> {
        let mut pieces: Vec<usize> = Vec::new();
        let mut piece = 0;
        let mut next = |count: usize| {
            let first = PART_COUNT + piece;
            piece += count;
            first
        };
        for corner in 0..4 {
            let first = next(2);
            if parts.contains(LOWER_LEGS[corner]) && parts.contains(UPPER_LEGS[corner]) {
                pieces.extend([first, first + 1]);
            } else {
                pieces.extend(
                    [LOWER_LEGS[corner], UPPER_LEGS[corner]]
                        .into_iter()
                        .filter(|&part| parts.contains(part)),
                );
            }
        }
        for (face, members) in FACES.iter().enumerate() {
            let count = if face % 2 == 0 { 1 } else { 2 };
            let first = next(count);
            if members.iter().all(|&member| parts.contains(member)) {
                pieces.extend(first..first + count);
            } else {
                pieces.extend(members.iter().copied().filter(|&member| parts.contains(member)));
            }
        }
        for deck in DECKS {
            let first = next(2);
            if parts.contains(deck) {
                pieces.extend([first, first + 1]);
            }
        }
        let walls = next(4);
        let (door, shutters) = (next(1), next(2));
        for (wall, &part) in WALLS.iter().enumerate() {
            if parts.contains(part) {
                pieces.push(walls + wall);
                match wall {
                    0 => pieces.extend([door, shutters]),
                    1 => pieces.push(shutters + 1),
                    _ => {}
                }
            }
        }
        let roof = next(1);
        if parts.contains(ROOF) {
            pieces.push(roof);
        }
        pieces.extend(
            RAILINGS
                .into_iter()
                .chain([LADDER])
                .filter(|&part| parts.contains(part)),
        );
        // Pieces already broken off land as they are.
        pieces.extend(parts.iter().filter(|&index| index >= PART_COUNT));
        pieces.sort_unstable();
        pieces
            .into_iter()
            .map(|index| PartSet::from_indices([index]))
            .collect()
    }

    /// Joins the parts: footings to the legs in their shoes, each leg's lifts, the
    /// braces and girts to the legs at their ends, the legs to the deck halves over
    /// them, the deck halves to each other, the railing and the cabin to the deck, the
    /// cabin's walls at their corners, the roof to the walls and the ladder to the deck
    /// rim.
    fn tie_all(&mut self) {
        let mut ties: Vec<(usize, usize)> = Vec::new();
        for (corner, &(sx, _)) in CORNERS.iter().enumerate() {
            let side = usize::from(sx > 0.0);
            ties.push((FOOTINGS[side], LOWER_LEGS[corner]));
            ties.push((LOWER_LEGS[corner], UPPER_LEGS[corner]));
            ties.push((UPPER_LEGS[corner], DECKS[side]));
        }
        for (face, [outer, inner, girt]) in FACES.into_iter().enumerate() {
            for (member, is_outer) in [(outer, true), (inner, false)] {
                let (low, high) = diagonal_corners(face, is_outer);
                ties.push((member, LOWER_LEGS[low]));
                ties.push((member, UPPER_LEGS[high]));
            }
            for corner in face_corners(face) {
                ties.push((girt, LOWER_LEGS[corner]));
                ties.push((girt, UPPER_LEGS[corner]));
            }
        }
        ties.push((DECKS[0], DECKS[1]));
        ties.push((RAILINGS[0], DECKS[0]));
        ties.push((RAILINGS[1], DECKS[1]));
        ties.push((RAILINGS[0], RAILINGS[1]));
        let [front, back, west, east] = WALLS;
        for wall in [front, back] {
            ties.extend([(wall, DECKS[0]), (wall, DECKS[1]), (wall, west), (wall, east)]);
        }
        ties.push((west, DECKS[0]));
        ties.push((east, DECKS[1]));
        ties.extend(WALLS.map(|wall| (ROOF, wall)));
        ties.push((LADDER, DECKS[1]));
        for (a, b) in ties {
            self.parts[a].ties.push(b as u16);
            self.parts[b].ties.push(a as u16);
        }
        for part in &mut self.parts {
            part.ties.sort_unstable();
        }
    }

    /// Every part, as a set.
    pub fn all(&self) -> PartSet {
        PartSet::from_indices(0..self.parts.len())
    }

    /// The groups of `parts` joined to each other through `parts` alone, each in order
    /// of its lowest part. Footings belong to none: they are the ground.
    pub fn joined(&self, parts: &PartSet) -> Vec<PartSet> {
        let footing = |index: usize| self.parts[index].kind == TowerPartKind::Footing;
        let mut seen = PartSet::EMPTY;
        let mut groups = Vec::new();
        for start in parts.iter() {
            if seen.contains(start) || footing(start) {
                continue;
            }
            let mut group = PartSet::EMPTY;
            let mut open = vec![start];
            seen.insert(start);
            while let Some(index) = open.pop() {
                group.insert(index);
                for &next in &self.parts[index].ties {
                    let next = next as usize;
                    if parts.contains(next) && !seen.contains(next) && !footing(next) {
                        seen.insert(next);
                        open.push(next);
                    }
                }
            }
            groups.push(group);
        }
        groups
    }

    /// The weight of `parts` and the point it acts at.
    pub fn balance(&self, parts: &PartSet) -> (f64, DVec3) {
        let (mass, moment) = parts.iter().fold((0.0, DVec3::ZERO), |(mass, moment), index| {
            let part = &self.parts[index];
            (mass + part.mass, moment + part.center * part.mass)
        });
        (mass, if mass > 0.0 { moment / mass } else { DVec3::ZERO })
    }

    /// Where `group` rests on the ground: the corners of its legs standing in their
    /// shoes.
    fn contacts(&self, group: &PartSet) -> Vec<DVec2> {
        LOWER_LEGS
            .iter()
            .filter(|&&leg| group.contains(leg))
            .map(|&leg| {
                let (sx, sz) = CORNERS[self.parts[leg].corner.expect("a leg's corner")];
                DVec2::new(sx * TOWER_BASE.offset, sz * TOWER_BASE.post_z)
            })
            .collect()
    }

    /// What comes down once the parts in `gone` are out (broken or fallen) and those in
    /// `loose` are torn free: every group of the rest no longer anchored in a footing,
    /// and the group carrying the deck once its weight is no longer over its legs, each
    /// with the loose parts joined to it. A frame that tips turns toward the side its
    /// weight lies out over, or toward `toward` (tower coordinates) when its weight lies
    /// on the line of its legs. Groups in order of their lowest part.
    pub fn falls(&self, gone: &PartSet, loose: &PartSet, toward: DVec3) -> Vec<TowerFall> {
        let standing = PartSet::from_indices(
            (0..self.parts.len()).filter(|&index| !gone.contains(index) && !loose.contains(index)),
        );
        let mut falling = *loose;
        let mut tips: Vec<(PartSet, (DVec3, DVec2))> = Vec::new();
        for group in self.joined(&standing) {
            let anchored = LOWER_LEGS.iter().any(|&leg| group.contains(leg));
            let carries = DECKS.iter().any(|&deck| group.contains(deck));
            // A frame anchored in its shoes stands by itself; carrying the deck it needs
            // its weight over its legs too.
            if anchored && !carries {
                continue;
            }
            let (_, weight) = self.balance(&group);
            let tip = tipping(&self.contacts(&group), weight, toward);
            if anchored && tip.is_none() {
                continue;
            }
            falling = falling.union(&group);
            if let Some(tip) = tip.filter(|_| carries) {
                tips.push((group, tip));
            }
        }
        // The ladder carries nothing: it tears off a frame that tips and goes over the
        // same way about its own foot, rather than propping the frame up.
        let ladder = tips.first().map(|(_, (_, way))| *way).filter(|_| falling.contains(LADDER));
        if ladder.is_some() {
            falling.remove(LADDER);
        }
        let mut falls: Vec<TowerFall> = self
            .joined(&falling)
            .into_iter()
            .map(|parts| {
                let tip = tips
                    .iter()
                    .find(|(group, _)| group.iter().any(|index| parts.contains(index)))
                    .map(|(_, tip)| *tip);
                TowerFall { parts, tip }
            })
            .collect();
        if let Some(way) = ladder {
            let foot = self.parts[LADDER].min.with_y(0.0) + DVec3::new(LADDER_HALF, 0.0, 0.04);
            falls.push(TowerFall {
                parts: PartSet::from_indices([LADDER]),
                tip: Some((foot, way)),
            });
        }
        falls
    }
}

/// Whether a frame resting on `contacts` with its weight at `weight` tips, and if so
/// the point on the ground it turns about and the way it falls: it stands while its
/// weight lies [`STAND_MARGIN`] inside the polygon of its contacts. Off a line of
/// contacts (or a single one) it falls the way its weight lies off it, or toward
/// `toward` when it lies on it; with no contact it drops where it is, turning toward
/// `toward`.
pub fn tipping(contacts: &[DVec2], weight: DVec3, toward: DVec3) -> Option<(DVec3, DVec2)> {
    let at = DVec2::new(weight.x, weight.z);
    let fallback = {
        let toward = DVec2::new(toward.x, toward.z) - at;
        if toward.length() > 1e-6 {
            toward.normalize()
        } else {
            DVec2::X
        }
    };
    let ground = |p: DVec2| DVec3::new(p.x, 0.0, p.y);
    let hull = convex_hull(contacts);
    match hull.len() {
        0 => Some((ground(at), fallback)),
        1 => {
            let off = at - hull[0];
            let way = if off.length() > 1e-6 {
                off.normalize()
            } else {
                fallback
            };
            Some((ground(hull[0]), way))
        }
        2 => {
            let (a, b) = (hull[0], hull[1]);
            let along = (b - a).normalize();
            let foot = a + along * (at - a).dot(along).clamp(0.0, a.distance(b));
            let off = at - foot;
            let way = if off.length() > 1e-6 {
                off.normalize()
            } else {
                // On the line: off toward `toward`'s side of it.
                let across = along.perp();
                if across.dot(fallback) >= 0.0 {
                    across
                } else {
                    -across
                }
            };
            Some((ground(foot), way))
        }
        count => {
            // Counter-clockwise, so an edge's outward normal is its direction turned
            // clockwise. The frame tips over the edge its weight lies nearest inside
            // (or outside) of.
            let (mut nearest, mut edge) = (f64::INFINITY, 0);
            for i in 0..count {
                let (a, b) = (hull[i], hull[(i + 1) % count]);
                let outward = -(b - a).normalize().perp();
                let inside = -(at - a).dot(outward);
                if inside < nearest {
                    nearest = inside;
                    edge = i;
                }
            }
            if nearest >= STAND_MARGIN {
                return None;
            }
            let (a, b) = (hull[edge], hull[(edge + 1) % count]);
            let along = (b - a).normalize();
            let foot = a + along * (at - a).dot(along).clamp(0.0, a.distance(b));
            Some((ground(foot), -along.perp()))
        }
    }
}

/// The convex hull of `points`, counter-clockwise, without collinear points: the two
/// ends of a line of them, or the one place they all are.
fn convex_hull(points: &[DVec2]) -> Vec<DVec2> {
    let mut sorted: Vec<DVec2> = points.to_vec();
    sorted.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    sorted.dedup_by(|a, b| a.distance(*b) < 1e-9);
    if sorted.len() < 3 {
        return sorted;
    }
    let turns_left = |o: DVec2, a: DVec2, b: DVec2| (a - o).perp_dot(b - o) > 1e-9;
    let mut hull: Vec<DVec2> = Vec::new();
    // Andrew's monotone chain: the lower hull left to right, then the upper back.
    for pass in [false, true] {
        let start = hull.len();
        let chain = |p: DVec2, hull: &mut Vec<DVec2>| {
            while hull.len() >= start + 2 && !turns_left(hull[hull.len() - 2], hull[hull.len() - 1], p)
            {
                hull.pop();
            }
            hull.push(p);
        };
        if pass {
            for &p in sorted.iter().rev() {
                chain(p, &mut hull);
            }
        } else {
            for &p in &sorted {
                chain(p, &mut hull);
            }
        }
        hull.pop();
    }
    if hull.len() < 3 {
        return vec![sorted[0], sorted[sorted.len() - 1]];
    }
    hull
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_part_is_joined_into_one_frame() {
        let tower = tower_structure();
        assert_eq!(tower.parts.len(), PART_COUNT);
        for (index, part) in tower.parts.iter().enumerate() {
            assert!(!part.ties.is_empty(), "part {index} is joined to something");
            assert!(part.min.cmplt(part.max).all(), "part {index} has a size");
        }
        assert_eq!(tower.joined(&tower.all()).len(), 1, "one frame");
    }

    #[test]
    fn a_whole_tower_stands_and_one_missing_leg_tips_it_toward_that_corner() {
        let tower = tower_structure();
        assert!(tower.falls(&PartSet::EMPTY, &PartSet::EMPTY, DVec3::ZERO).is_empty());
        for (corner, &(sx, sz)) in CORNERS.iter().enumerate() {
            let gone = PartSet::from_indices([LOWER_LEGS[corner]]);
            let falls = tower.falls(&gone, &PartSet::EMPTY, DVec3::ZERO);
            assert_eq!(falls.len(), 2, "corner {corner}: the frame, and the ladder off it");
            assert_eq!(
                falls[0].parts.len(),
                PART_COUNT - 4,
                "everything but the footings, the leg and the ladder"
            );
            assert_eq!(falls[1].parts, PartSet::from_indices([LADDER]));
            let (_, way) = falls[0].tip.expect("it tips");
            assert!(
                way.x * sx > 0.3 && way.y * sz > 0.3,
                "corner {corner} falls toward ({sx}, {sz}): {way:?}"
            );
            assert_eq!(falls[1].tip.map(|(_, ladder)| ladder), Some(way));
        }
    }

    #[test]
    fn a_shot_out_brace_lift_wall_or_ladder_leaves_the_frame_standing() {
        let tower = tower_structure();
        for gone in [FACES[0][0], FACES[3][2], UPPER_LEGS[1], LADDER, WALLS[0]] {
            let falls = tower.falls(&PartSet::from_indices([gone]), &PartSet::EMPTY, DVec3::ZERO);
            assert!(falls.is_empty(), "{gone}: {falls:?}");
        }
    }

    #[test]
    fn both_legs_of_a_bent_gone_tip_it_off_that_side() {
        let tower = tower_structure();
        let gone = PartSet::from_indices([LOWER_LEGS[0], LOWER_LEGS[1]]);
        let falls = tower.falls(&gone, &PartSet::EMPTY, DVec3::ZERO);
        let (pivot, way) = falls[0].tip.expect("it tips");
        assert!(way.x < -0.9, "west: {way:?}");
        assert!((pivot.x - TOWER_BASE.offset).abs() < 1e-9, "{pivot:?}");
    }

    #[test]
    fn a_frame_on_three_corners_tips_toward_the_fourth() {
        let contacts = [
            DVec2::new(1.0, 1.0),
            DVec2::new(-1.0, 1.0),
            DVec2::new(1.0, -1.0),
        ];
        let (_, way) = tipping(&contacts, DVec3::ZERO, DVec3::ZERO).expect("tips");
        assert!(way.x < 0.0 && way.y < 0.0, "{way:?}");
        let square = [
            DVec2::new(1.0, 1.0),
            DVec2::new(-1.0, 1.0),
            DVec2::new(1.0, -1.0),
            DVec2::new(-1.0, -1.0),
        ];
        assert!(tipping(&square, DVec3::ZERO, DVec3::ZERO).is_none());
    }
}
