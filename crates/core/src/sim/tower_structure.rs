//! A watchtower as a structure: the members it is built of (two concrete footings,
//! four legs in two lifts, the X-braces and girts on its four faces, the two deck
//! halves, the railing, the cabin walls, the roof and the ladder), what each is joined
//! to, and what a landing breaks them into: the watchtower's builder for the structure
//! engine ([`super::structure`]).
//!
//! Each standing part is a collider of its own, so a shell between the legs and under
//! the braces flies through untouched, and one that strikes a leg strikes that leg. The
//! tower stands as one braced frame by the engine's tipping rule: every part joined to
//! the frame goes with it, and the frame stands while it is anchored in its footings and,
//! carrying the deck, while its weight lies over the legs still in their shoes. Shoot a
//! leg out from under it and its weight is no longer over what holds it: it tips toward
//! the missing leg, the deck, cabin and roof with it.
//!
//! Everything is in tower coordinates: the tower's ground centre at the origin, x across
//! its footings (west -x), z along them (front +z, the ladder and door side).

use std::sync::{Arc, LazyLock};

use glam::{DMat3, DQuat, DVec2, DVec3};

use super::debris_physics::DebrisMaterial;
use super::structure::{
    Breakup, Breaking, Build, DebrisRules, Levelling, Material, Part, PartSet, PartShape, Role,
    Structure, Tipping,
};
use super::tower_layout::{
    AWNING_BOTTOM, AWNING_REACH, AWNING_TOP, BENT_BRACE_LOW, CABIN_TOP, CABIN_WALL, CABIN_X,
    CABIN_Z, DECK_TOP, DECK_X, DECK_Z, FACE_BRACE_LOW, GIRT_Y, KNEE_LOW, LADDER_HALF, LADDER_TOP,
    LADDER_X, LADDER_Z, OUTRIGGER_Y, POST, POST_TOP, RAIL_HEIGHT, RAIL_INSET, ROOF_EDGE,
    ROOF_OVERHANG, TOWER_BASE, roof_apex,
};

/// What a part is: its material, its weight and how the model draws it.
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
    /// The part's material: damage (in shell damage units) it takes before it breaks. A
    /// standard shell (40) does not break a leg it strikes, but its burst there does with
    /// the next; an armour-piercing round breaks whatever it goes through.
    pub fn material(self) -> Material {
        let strength = match self {
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
        };
        let (density, surface, breaking) = match self {
            Self::Footing => (0.0, DebrisMaterial::Concrete, Breaking::Never),
            Self::Ladder => (STEEL, DebrisMaterial::Metal, Breaking::Splinters),
            _ => (TIMBER, DebrisMaterial::Wood, Breaking::Splinters),
        };
        Material {
            strength,
            density,
            surface,
            breaking,
            tear: 0.0,
        }
    }

    /// The paint its splinters and dust take; none for the cabin's, the cover's colour.
    fn paint(self) -> Option<u32> {
        match self {
            Self::Footing => Some(CONCRETE_PAINT),
            Self::LowerLeg | Self::UpperLeg => Some(POST_PAINT),
            Self::Brace | Self::Girt | Self::Railing => Some(BRACE_PAINT),
            Self::Deck => Some(DECK_PAINT),
            Self::Wall => None,
            Self::Roof => Some(ROOF_PAINT),
            Self::Ladder => Some(STEEL_PAINT),
        }
    }
}

/// The paints of the tower's timber, its roof, its concrete and its steel.
const POST_PAINT: u32 = 0x887454;
const BRACE_PAINT: u32 = 0x96734c;
const DECK_PAINT: u32 = 0x887d59;
const ROOF_PAINT: u32 = 0x2b6a4c;
const CONCRETE_PAINT: u32 = 0x9a968c;
const STEEL_PAINT: u32 = 0x5d6266;

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
/// How a watchtower's fallen pieces land and clear: a body breaks up when its velocity
/// changes by this much (m/s) in a tick on the ground (a frame caught on cover stays
/// whole until it comes down), and a piece lies a while before it clears, the ruin gone
/// but for the footings by its deadline.
const TOWER_DEBRIS: DebrisRules = DebrisRules {
    landing_speed: 2.5,
    firm_only: true,
    one_clock: true,
    life: 16.0,
    life_spread: 0.0,
    deadline: 24.0,
    seam: 0.01,
};

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

    pub fn shape(&self) -> PartShape {
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

/// The structure all watchtowers share.
pub fn tower_structure() -> Arc<Structure> {
    static STRUCTURE: LazyLock<Arc<Structure>> = LazyLock::new(|| Arc::new(build()));
    STRUCTURE.clone()
}

/// A tower part of `kind` of `shapes` weighing `mass`.
fn part(kind: TowerPartKind, shapes: Vec<PartShape>, mass: f64) -> Part {
    let mut part = Part::new(
        shapes,
        kind.material(),
        Role::Tower {
            kind,
            corner: None,
            look: None,
        },
    );
    part.mass = mass;
    part.paint = kind.paint();
    part
}

/// A landing piece of `kind`, drawn as `look`.
fn piece(kind: TowerPartKind, look: PieceLook, shapes: Vec<PartShape>, mass: f64) -> Part {
    let mut piece = part(kind, shapes, mass);
    piece.role = Role::Tower {
        kind,
        corner: None,
        look: Some(look),
    };
    piece
}

fn build() -> Structure {
    use TowerPartKind::*;
    let mut parts = Vec::with_capacity(PART_COUNT);
    let foot = TOWER_BASE.height;
    for side in [-1.0, 1.0] {
        let x = side * TOWER_BASE.offset;
        let (hw, hd) = (TOWER_BASE.width / 2.0, TOWER_BASE.depth / 2.0);
        let mut footing = part(
            Footing,
            vec![PartShape::aligned(
                DVec3::new(x - hw, 0.0, -hd),
                DVec3::new(x + hw, foot, hd),
            )],
            0.0,
        );
        footing.footing = true;
        footing.grounded = true;
        parts.push(footing);
    }
    let post = |corner: usize, low: f64, high: f64| {
        let (sx, sz) = CORNERS[corner];
        let (x, z) = (sx * TOWER_BASE.offset, sz * TOWER_BASE.post_z);
        PartShape::aligned(
            DVec3::new(x - POST / 2.0, low, z - POST / 2.0),
            DVec3::new(x + POST / 2.0, high, z + POST / 2.0),
        )
    };
    let leg = |mut part: Part, corner: usize| {
        let kind = part.tower_kind();
        part.role = Role::Tower {
            kind,
            corner: Some(corner),
            look: None,
        };
        part
    };
    for (corner, &(sx, sz)) in CORNERS.iter().enumerate() {
        let mut lower = leg(
            part(
                LowerLeg,
                vec![post(corner, foot, GIRT_Y)],
                POST * POST * (GIRT_Y - foot) * TIMBER,
            ),
            corner,
        );
        // A leg in its shoe: the frame bears on the ground there, and the shoe holds a
        // light frame upright.
        lower.anchor = true;
        lower.contacts = vec![DVec2::new(sx * TOWER_BASE.offset, sz * TOWER_BASE.post_z)];
        parts.push(lower);
    }
    for corner in 0..4 {
        // The knee brace's collider stops under the beams, where the deck's begins: a
        // leg falling apart from the deck starts clear of it.
        let knee = knee_member(corner);
        parts.push(leg(
            part(
                UpperLeg,
                vec![post(corner, GIRT_Y, POST_TOP), knee_collider(corner).shape()],
                POST * POST * (POST_TOP - GIRT_Y) * TIMBER + knee.mass(),
            ),
            corner,
        ));
    }
    for face in 0..4 {
        for (k, member) in face_members_of(face).iter().enumerate() {
            let kind = if k == 2 { Girt } else { Brace };
            parts.push(part(kind, vec![member.shape()], member.mass()));
        }
    }
    for side in [-1.0, 1.0] {
        let (x0, x1) = if side < 0.0 { (-DECK_X, 0.0) } else { (0.0, DECK_X) };
        let min = DVec3::new(x0, POST_TOP, -DECK_Z);
        let max = DVec3::new(x1, DECK_TOP, DECK_Z);
        let mut deck = part(
            Deck,
            vec![PartShape::aligned(min, max)],
            (max - min).element_product() * DECK_SOLID * TIMBER,
        );
        deck.load = true;
        parts.push(deck);
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
        parts.push(part(Railing, runs, length * RAIL_HEIGHT * 0.02 * TIMBER));
    }
    let wall_height = CABIN_TOP - DECK_TOP;
    for (along_x, side) in [(true, 1.0), (true, -1.0), (false, -1.0), (false, 1.0)] {
        let length = 2.0 * if along_x { CABIN_X } else { CABIN_Z };
        parts.push(part(
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
    let mut roof = part(
        Roof,
        vec![PartShape::Hull(points)],
        4.0 * x * z * ROOF_SOLID * TIMBER,
    );
    // A hipped roof's weight lies a third of the way up it.
    roof.mass_center.y = top + (roof_apex() - top) / 3.0;
    parts.push(roof);
    let mut ladder = part(
        Ladder,
        vec![PartShape::aligned(
            DVec3::new(LADDER_X - LADDER_HALF - 0.03, 0.0, LADDER_Z - 0.04),
            DVec3::new(LADDER_X + LADDER_HALF + 0.03, LADDER_TOP, LADDER_Z + 0.04),
        )],
        0.03 * LADDER_TOP * STEEL,
    );
    // The ladder carries nothing: it tears off a frame that tips and goes over the same
    // way about its own foot, rather than propping the frame up.
    ladder.detaches = true;
    parts.push(ladder);
    // A landing body breaks apart at its sections first: each leg (both its lifts), each
    // face's bracing, each deck half with its railing, then the rest alone.
    for (index, part) in parts.iter_mut().enumerate() {
        part.element = match part.role {
            Role::Tower {
                corner: Some(corner),
                ..
            } => corner as u16,
            _ => match FACES.iter().position(|members| members.contains(&index)) {
                Some(face) => 4 + face as u16,
                None => match (DECKS.iter().position(|&deck| deck == index))
                    .or(RAILINGS.iter().position(|&railing| railing == index))
                {
                    Some(side) => 8 + side as u16,
                    None => 10 + index as u16,
                },
            },
        };
    }
    let mut structure = Structure::new(Build::Tower, 0.0, parts, TOWER_DEBRIS);
    structure.shares = false;
    structure.tipping = Some(Tipping {
        margin: STAND_MARGIN,
        rise: 0.0,
        snap: None,
    });
    // It counts as destroyed as its deck starts down; its dust and noise come when the
    // deck lands.
    structure.levelling = Levelling {
        keys: PartSet::from_indices(DECKS),
        share: 0.0,
        announce: false,
        dust: 0.0,
    };
    tie_all(&mut structure);
    add_pieces(&mut structure);
    structure
}

/// Joins the parts: footings to the legs in their shoes, each leg's lifts, the braces
/// and girts to the legs at their ends, the legs to the deck halves over them, the deck
/// halves to each other, the railing and the cabin to the deck, the cabin's walls at
/// their corners, the roof to the walls and the ladder to the deck rim.
fn tie_all(structure: &mut Structure) {
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
        structure.join(a, b);
    }
    for joints in &mut structure.joints {
        joints.sort_unstable();
    }
}

/// The pieces a landing breaks the parts into, in [`PieceLook`] order, and the breakups
/// that use them: each leg in two at a height of its own (with both its lifts there),
/// the outer diagonal of each face snapped (the west and back panels stay joined, bent;
/// with all three of the face's members there), the deck halves across their planks,
/// each cabin wall with the door and a shutter torn off it, and the roof crumpled.
/// Whatever is left of a part whose pieces are not all there (a lone lift, a brace) and
/// the railing and ladder land whole.
fn add_pieces(structure: &mut Structure) {
    use TowerPartKind::*;
    let first = PART_COUNT as u16;
    let mut pieces: Vec<Part> = Vec::new();
    let mut add = |piece: Part| {
        pieces.push(piece);
        first + pieces.len() as u16 - 1
    };
    let mut breakups: Vec<Breakup> = Vec::new();
    let foot = TOWER_BASE.height + STUB_REACH;
    for (corner, &(sx, sz)) in CORNERS.iter().enumerate() {
        let (x, z) = (sx * TOWER_BASE.offset, sz * TOWER_BASE.post_z);
        let snap = LEG_SNAPS[corner];
        let mut made = Vec::new();
        for (from, to) in [(foot, snap), (snap, POST_TOP)] {
            let mut shapes = vec![PartShape::aligned(
                DVec3::new(x - POST / 2.0, from, z - POST / 2.0),
                DVec3::new(x + POST / 2.0, to, z + POST / 2.0),
            )];
            if to == POST_TOP {
                shapes.push(knee_collider(corner).shape());
            }
            let look = PieceLook::Leg { corner, from, to };
            made.push(add(piece(
                LowerLeg,
                look,
                shapes,
                POST * POST * (to - from) * TIMBER,
            )));
        }
        breakups.push(Breakup {
            needs: PartSet::from_indices([LOWER_LEGS[corner], UPPER_LEGS[corner]]),
            pieces: made,
            alone: false,
        });
    }
    for (face, members) in FACES.iter().enumerate() {
        let [outer, inner, girt] = face_members_of(face);
        let middle = outer.from.lerp(outer.to, 0.5);
        let lower = Member { to: middle, ..outer };
        let upper = Member {
            from: middle,
            ..outer
        };
        let joined = face % 2 == 0;
        let made = if joined {
            let shapes = vec![outer.shape(), inner.shape(), girt.shape()];
            let mass = outer.mass() + inner.mass() + girt.mass();
            let look = PieceLook::Panel {
                face,
                snapped: false,
            };
            vec![add(piece(Brace, look, shapes, mass))]
        } else {
            let shapes = vec![lower.shape(), inner.shape(), girt.shape()];
            let mass = lower.mass() + inner.mass() + girt.mass();
            let look = PieceLook::Panel {
                face,
                snapped: true,
            };
            let panel = add(piece(Brace, look, shapes, mass));
            let look = PieceLook::Diagonal { face, upper: true };
            vec![panel, add(piece(Brace, look, vec![upper.shape()], upper.mass()))]
        };
        breakups.push(Breakup {
            needs: PartSet::from_indices(*members),
            pieces: made,
            alone: false,
        });
    }
    for (side, &deck) in DECKS.iter().enumerate() {
        let sign = if side == 0 { -1.0 } else { 1.0 };
        let mut made = Vec::new();
        for outer in [false, true] {
            let (a, b) = if outer { (DECK_CUT, DECK_X) } else { (0.0, DECK_CUT) };
            let (x0, x1) = if sign < 0.0 { (-b, -a) } else { (a, b) };
            let min = DVec3::new(x0, POST_TOP, -DECK_Z);
            let max = DVec3::new(x1, DECK_TOP, DECK_Z);
            let mass = (max - min).element_product() * DECK_SOLID * TIMBER;
            made.push(add(piece(
                Deck,
                PieceLook::Deck { side, outer },
                vec![PartShape::aligned(min, max)],
                mass,
            )));
        }
        breakups.push(Breakup {
            needs: PartSet::from_indices([deck]),
            pieces: made,
            alone: false,
        });
    }
    let wall_height = CABIN_TOP - DECK_TOP;
    let mut walls = Vec::new();
    for (wall, (along_x, side)) in [(true, 1.0), (true, -1.0), (false, -1.0), (false, 1.0)]
        .into_iter()
        .enumerate()
    {
        let [panel, _] = cabin_wall(along_x, side);
        let length = 2.0 * if along_x { CABIN_X } else { CABIN_Z };
        walls.push(add(piece(
            Wall,
            PieceLook::Wall { wall },
            vec![panel],
            length * wall_height * WALL_SOLID * TIMBER,
        )));
    }
    // The door, in the front wall's opening (`tower_model::cabin_wall`).
    let door = PartShape::aligned(
        DVec3::new(DOOR_U - DOOR_WIDTH / 2.0, DECK_TOP, CABIN_Z - 0.12),
        DVec3::new(DOOR_U + DOOR_WIDTH / 2.0, DECK_TOP + DOOR_HEIGHT, CABIN_Z - 0.04),
    );
    let door = add(piece(
        Wall,
        PieceLook::Door,
        vec![door],
        DOOR_WIDTH * DOOR_HEIGHT * 0.04 * TIMBER,
    ));
    // A shutter of the front and the back wall, propped out under the eaves.
    let mut shutters = Vec::new();
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
        shutters.push(add(piece(Wall, PieceLook::Shutter { wall }, vec![shape], mass)));
    }
    for (wall, &part) in WALLS.iter().enumerate() {
        let mut made = vec![walls[wall]];
        match wall {
            0 => made.extend([door, shutters[0]]),
            1 => made.push(shutters[1]),
            _ => {}
        }
        breakups.push(Breakup {
            needs: PartSet::from_indices([part]),
            pieces: made,
            alone: false,
        });
    }
    let roof = &structure.parts[ROOF];
    let (shapes, mass, center) = (roof.shapes.clone(), roof.mass, roof.mass_center);
    let mut crumpled = piece(Roof, PieceLook::Roof, shapes, mass);
    crumpled.mass_center = center;
    let roof = add(crumpled);
    breakups.push(Breakup {
        needs: PartSet::from_indices([ROOF]),
        pieces: vec![roof],
        alone: false,
    });
    for piece in &mut pieces {
        // A piece breaks no further: it lands as its own element.
        piece.element = u16::MAX;
    }
    structure.pieces = pieces;
    structure.breakups = breakups;
}

/// How landing piece `index` (from [`PART_COUNT`]) is drawn.
pub fn piece_look(structure: &Structure, index: usize) -> Option<PieceLook> {
    match structure.part(index).role {
        Role::Tower { look, .. } => look,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_part_is_joined_into_one_frame() {
        let tower = tower_structure();
        assert_eq!(tower.parts.len(), PART_COUNT);
        for (index, part) in tower.parts.iter().enumerate() {
            assert!(!tower.joints[index].is_empty(), "part {index} is joined to something");
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
    fn a_landing_snaps_the_legs_and_tears_the_cabin_apart_when_all_of_it_lands() {
        let tower = tower_structure();
        let whole = PartSet::from_indices((2..PART_COUNT).filter(|&index| index != LADDER));
        let bodies = tower.landing(&whole, false);
        let looks = bodies
            .iter()
            .filter(|body| body.iter().all(|index| piece_look(&tower, index).is_some()))
            .count();
        assert!(looks >= 20, "{looks} of {} bodies are pieces", bodies.len());
        let coarse = tower.landing(&whole, true);
        assert!(coarse.len() < bodies.len());
        assert!(coarse.iter().all(|body| body.iter().all(|index| index < PART_COUNT)));
    }
}
