//! A cottage as a structure: the parts it is built of (wall panels a bay wide, split
//! round their windows and door, corner posts, the plate course at the wall tops, the
//! gables, the attic floor, an inner partition, the roof sheets, the ridge and the
//! chimney), what each rests on, hangs from or is tied to, and what braces the parts
//! that stand only braced: the cottage's builder for the structure engine
//! ([`super::structure`]).
//!
//! A part's support is the best path of supports to the ground, each link passing on a
//! share of it: resting on a part passes all of it, hanging from one above or being tied
//! to one beside passes less. A part whose support falls under the engine's threshold
//! gives way, and what is left of the cottage tips over where it no longer stands on
//! enough of the ground (a lone tall strip of wall). No count of hits decides it: a
//! cottage stands as long as its frame carries it.
//!
//! Everything is measured in the build frame of [`HouseDesign`] (ridge along z, front
//! +z, right +x, ground at y = 0). Each part also names the cells of the cottage's model
//! it is drawn from: rectangles in the plane of a [`HousePart`] of the model.

use std::sync::Arc;

use glam::{DVec2, DVec3};

use super::debris_physics::DebrisMaterial;
use super::house_fracture::PartPlane;
use super::house_layout::{
    CHIMNEY, HouseDesign, HousePart, PLINTH, RAFTER_DEPTH, Roof, RoofForm, WALL_DEPTH, WallFinish,
    wall_frames, window_columns,
};
use super::structure::{
    Brace, Breakup, Breaking, Build, DebrisRules, End, GROUND_BLOCK, HeldLayouts, end_rank, Levelling, Lie, MAX_PARTS, Material,
    Part, PartSet, PartShape, Role, SEAT_NEEDED, Structure, Support, Tipping,
};

/// What a part is, which decides its strength, how it carries what it holds and how
/// the model draws it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PartKind {
    /// A bay of wall between studs, below a window, beside one or over it.
    Panel,
    Window,
    Door,
    /// A corner post, from the plinth to the wall top.
    Post,
    /// The course over the windows' heads up to the wall top (and a two-storey
    /// house's floor band): the plates and headers that spread the roof's weight.
    Plate,
    /// A piece of a gable over the plates.
    Gable,
    /// A strip of the attic floor (and a two-storey house's upper floor).
    Floor,
    /// A panel of the inner wall across the rooms.
    Partition,
    /// A roof sheet with its rafters.
    Sheet,
    Ridge,
    Chimney,
}

impl PartKind {
    /// The part's material: how much damage (in shell damage units) it takes before it
    /// breaks, before a brick cottage's walls double up ([`BRICK_STRENGTH`]). A standard
    /// shell (40) breaks a clapboard panel it strikes; a corner post or the chimney takes
    /// more. Glass shatters as it loses its seat, the chimney's brick lands as bricks.
    pub fn material(self) -> Material {
        let strength = match self {
            Self::Panel => 35.0,
            Self::Window => 12.0,
            Self::Door => 25.0,
            Self::Post => 90.0,
            Self::Plate => 70.0,
            Self::Gable => 40.0,
            Self::Floor => 50.0,
            Self::Partition => 30.0,
            Self::Sheet => 45.0,
            Self::Ridge => 80.0,
            Self::Chimney => 110.0,
        };
        let (density, breaking) = match self {
            Self::Window => (TIMBER_DENSITY, Breaking::Glass),
            Self::Chimney => (BRICK_DENSITY, Breaking::Bricks),
            _ => (TIMBER_DENSITY, Breaking::Splinters),
        };
        Material {
            strength,
            density,
            surface: DebrisMaterial::Wood,
            breaking,
            tear: 0.0,
        }
    }

    /// Whether the part belongs to an outer wall (brick on a brick cottage).
    pub fn outer_wall(self) -> bool {
        matches!(
            self,
            Self::Panel | Self::Window | Self::Door | Self::Post | Self::Plate | Self::Gable
        )
    }

    pub fn roof(self) -> bool {
        matches!(self, Self::Sheet | Self::Ridge | Self::Chimney)
    }

    /// Whether it stands on its edge, so it tips out over its foot as it gives way: a
    /// wall's parts, its top plate among them (a strip of wall with its plate tipping
    /// over once took the stubs it stood on down with it).
    fn upright(self) -> bool {
        matches!(
            self,
            Self::Panel
                | Self::Window
                | Self::Door
                | Self::Gable
                | Self::Partition
                | Self::Post
                | Self::Plate
        )
    }
}

/// Mass per cubic metre of a cottage's parts, as other wooden scenery pieces (the
/// chimney's brick heavier).
const TIMBER_DENSITY: f64 = 0.35;
const BRICK_DENSITY: f64 = 0.8;
/// The paint of a fallen chimney's brick and of the rooms' floors and inner walls.
pub const CHIMNEY_BRICK: u32 = 0x9c4a32;
const ROOM_PAINT: u32 = 0xc9b48f;

/// The chimney's index among a cottage's parts (its top, through the roof), after the
/// four corner posts (their lengths over the bottom course; the feet under them follow
/// the chimney).
pub const CHIMNEY_PART: usize = 4;
/// Corner post `k`'s foot (the post's length in the walls' bottom course) is part
/// `POST_FEET + k`.
pub const POST_FEET: usize = CHIMNEY_PART + 1;
/// The chimney's stack under the roof, after the posts' feet.
pub const CHIMNEY_STACK: usize = POST_FEET + 4;
/// The stack's top stays at least this far (m) under the roof sheets' rafters.
const STACK_CLEAR: f64 = 0.1;
/// The hole the chimney comes through the roof reaches this far (m) past the stack.
const HOLE_MARGIN: f64 = 0.04;
/// The damage the chimney's stack takes before it breaks: solid brickwork on its own
/// footing, five standard shells, and more than a roof section coming down past it
/// strikes it with.
const STACK_STRENGTH: f64 = 200.0;

/// A brick cottage's walls take this much more than clapboard ones.
const BRICK_STRENGTH: f64 = 1.6;

/// A floor strip's joists need this share of their length still seated on the plate:
/// boards cantilevered half a strip out past the last plate read as a flat card.
const FLOOR_SEAT_NEEDED: f32 = 0.8;
/// What is left of a cottage stands while its weight lies this far inside the ground
/// it stands on per metre up: a lone strip of wall a bay wide, or a post with a bay of
/// wall on it, tips over its stub.
const COTTAGE_TIPPING: Tipping = Tipping {
    margin: 0.0,
    rise: 0.1,
    snap: Some(STUB_TOP),
};
/// A wall's bottom course (its sill height and the plinth under it) reaches no higher
/// than this: a tipping strip of wall breaks off over it, leaving the stub.
const STUB_TOP: f64 = 1.5;
/// A cottage counts as levelled once its roof is down or less than this share of its
/// parts stands.
const LEVELLED: f64 = 0.45;
/// The dust a levelled cottage raises (m).
const COTTAGE_DUST: f64 = 7.0;
/// How a cottage's fallen parts land and clear: a body breaks up when its velocity
/// changes by this much (m/s) in a tick, a loose part lies from [`DebrisRules::life`]
/// to that plus its spread by its parts (so a pile clears one piece after another), and
/// a ruin is clear of them by its deadline. Standing parts' colliders reach a centimetre
/// into their neighbours'.
const COTTAGE_DEBRIS: DebrisRules = DebrisRules {
    landing_speed: 1.5,
    firm_only: false,
    one_clock: false,
    life: 11.0,
    life_spread: 6.0,
    deadline: 19.5,
    seam: 0.01,
};

/// The share of its support a part passes to one resting on it, hanging from it or
/// tied beside it ([`support_share`]). Resting passes all of it; the rest are named
/// by what they model.
const REST: f32 = 1.0;
/// A panel, a window or the door held by the course over it.
const HANG: f32 = 0.55;
/// A bay nailed to a corner post: held by it about as well as by what it stands on,
/// so it seats the plate over it like a bay standing on the plinth.
const POST_PANEL_TIE: f32 = 0.95;
/// Neighbouring panels nailed to the same studs.
const PANEL_TIE: f32 = 0.45;
/// A plate spanning on from its neighbour: the doubled top plate bridges two bays over
/// a gap and still seats the rafters on it (0.73² is over the engine's firm hold), three
/// only loosely, seating nothing (0.73³), four not at all (0.73⁴ falls under its
/// threshold): a wall shot out under its whole length lets its roof down.
const PLATE_TIE: f32 = 0.73;
/// The most bays a plate spans over a gap ([`Frame::link_plate_spans`]).
const PLATE_SPAN: usize = 3;
/// A plate framed into a corner post.
const POST_TIE: f32 = 0.9;
/// Gable pieces, attic floor strips: boarded to their neighbours.
const BOARD_TIE: f32 = 0.5;
/// A floor strip's joists sitting in the plate beside them.
const JOIST_SEAT: f32 = 1.0;
/// Roof sheets: decking between rafters, along the eave and down the slope. A sheet
/// whose own rafter seats are gone does not hang from its neighbours.
const SHEET_TIE: f32 = 0.3;
/// Opposite rafters meeting across the ridge.
const RIDGE_PAIR: f32 = 0.4;
/// The ridge beam spanning on from its neighbour, held in by the rafter pairs meeting
/// on it: two segments past a gone gable's peak, so the roof's end stands on its eave
/// walls without it, but not three (0.65³ falls under [`FALL_BELOW`]).
const RIDGE_TIE: f32 = 0.65;
/// The length (m) of ridge [`RIDGE_TIE`] is for: it holds the same length past a gone
/// gable whatever segments the sheets cut it into.
const RIDGE_TIE_SPAN: f64 = 2.4;
/// A hip's ridge held up by the hip end slopes.
const HIP_TIE: f32 = 0.7;
/// Whatever else touches.
const LOOSE_TIE: f32 = 0.3;

/// Wall bays: the widest a panel is, the narrowest one left between openings (a
/// narrower gap goes to the opening beside it), and corner posts' plan size.
const BAY: f64 = 0.95;
const NARROWEST_BAY: f64 = 0.3;
const POST: f64 = 0.3;
/// Half widths of a window's and the door's opening with their casings.
const WINDOW_HALF: f64 = 0.57;
const DOOR_HALF: f64 = 0.62;
/// Window and door heights, as the model builds them (`house_model::window_opening`).
const SILL: f64 = 0.9;
const DOOR_RISE: f64 = 2.0;
const BELT_OVER: f64 = 0.3;
const UPPER_SILL: f64 = 0.35;
const UPPER_WINDOW: f64 = 1.2;
/// How far (m) a ground-floor window's cells reach into the bays beside it for its
/// shutters, leaving at least [`NARROWEST_SIDE`] of a bay to the bay.
const SHUTTER_REACH: f64 = 0.4;
const NARROWEST_SIDE: f64 = 0.12;
/// The course of wall under a window's lower panel (m over the plinth).
const WINDOW_FOOT: f64 = 0.35;
/// A window's trim reaches this far (m) past its opening: the head casing and drip cap
/// over it and the sill and apron under it go with the window, not the wall round it.
const WINDOW_HEAD_TRIM: f64 = 0.2;
const WINDOW_SILL_TRIM: f64 = 0.12;
/// Roof sheets: about this long along the ridge, and the share of a slope's run the
/// upper row takes.
const SHEET_LENGTH: f64 = 1.6;
const UPPER_ROW: f64 = 0.5;
/// How deep a sheet's collider reaches under the deck: the deck and its rafters.
const SHEET_DEPTH: f64 = RAFTER_DEPTH;
/// The ridge beam's half width and depth under the deck.
const RIDGE_HALF: f64 = 0.08;
const RIDGE_DEPTH: f64 = 0.25;
/// Attic floor strips: about this wide along the ridge, and their thickness under
/// the wall top.
const FLOOR_STRIP: f64 = 2.0;
const FLOOR_DEPTH: f64 = 0.18;
/// The inner partition's thickness, where it stands along the ridge (a share of the
/// depth from the middle) and its doorway's half width.
const PARTITION_THICKNESS: f64 = 0.12;
const PARTITION_AT: f64 = -0.12;
const PARTITION_DOOR: f64 = 0.45;
/// How close (m) two parts' boxes are to count as touching.
const CONTACT: f64 = 0.06;
/// A gable end is braced by roof sheets this near (m) to it.
const ROOF_BRACE: f64 = 0.15;
/// An upper storey's wall part is held by a floor strip this near (m) in plan.
const FLOOR_REACH: f64 = 0.3;
/// A part reaching this near the plinth's top stands on the ground.
const GROUND_REACH: f64 = 0.05;
/// Cells of the model reach this far past a part's outer edges, so trims and
/// overhangs go with the part at the edge.
const FAR: f64 = 50.0;

/// A rectangle of the model drawn as (part of) a part: `min`–`max` in the plane of
/// the model's `part` ([`part_plane`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartCell {
    pub part: HousePart,
    pub min: DVec2,
    pub max: DVec2,
}

/// What a cottage's part is to its model and its rules: its kind, the model cells drawn
/// as it, the wall it is in (in [`wall_frames`] order) for wall parts, a roof sheet's
/// slope and whether it is in the row under the ridge, and whether a wall part stands
/// above a two-storey house's upper floor.
#[derive(Clone, Debug, PartialEq)]
pub struct HouseRole {
    pub kind: PartKind,
    pub cells: Vec<PartCell>,
    pub wall: Option<usize>,
    pub slope: Option<(HousePart, bool)>,
    pub upper_storey: bool,
}

/// One part of a cottage as its builder lays it out: its kind, its collider as convex
/// hull points (a box's eight corners when `cuboid`), its bounding box and the model
/// cells drawn as it.
#[derive(Clone, Debug, PartialEq)]
struct FramePart {
    kind: PartKind,
    points: Vec<DVec3>,
    /// Axis aligned in the build frame: the collider is the box `min`–`max`.
    cuboid: bool,
    min: DVec3,
    max: DVec3,
    cells: Vec<PartCell>,
    /// Its collider's convex pieces, when it is not one hull: a sheet framed round the
    /// chimney's hole.
    hulls: Vec<Vec<DVec3>>,
    strength: f64,
    /// Stands on the plinth.
    grounded: bool,
    wall: Option<usize>,
    slope: Option<(HousePart, bool)>,
    upper_storey: bool,
}

impl FramePart {
    fn new(kind: PartKind, points: Vec<DVec3>, cuboid: bool, cells: Vec<PartCell>) -> Self {
        let (min, max) = points.iter().fold(
            (DVec3::INFINITY, DVec3::NEG_INFINITY),
            |(min, max), p| (min.min(*p), max.max(*p)),
        );
        Self {
            kind,
            points,
            cuboid,
            min,
            max,
            cells,
            hulls: Vec::new(),
            strength: kind.material().strength,
            grounded: min.y <= PLINTH + GROUND_REACH,
            wall: None,
            slope: None,
            upper_storey: false,
        }
    }

    fn cuboid(kind: PartKind, min: DVec3, max: DVec3, cells: Vec<PartCell>) -> Self {
        let points = (0..8)
            .map(|k| {
                DVec3::new(
                    if k & 1 == 0 { min.x } else { max.x },
                    if k & 2 == 0 { min.y } else { max.y },
                    if k & 4 == 0 { min.z } else { max.z },
                )
            })
            .collect();
        Self::new(kind, points, true, cells)
    }

    fn center(&self) -> DVec3 {
        (self.min + self.max) / 2.0
    }

    /// The engine's part: its shape (a wall's lowest parts and the posts reach down to
    /// the ground through their footing, which goes with them), material, paint, how it
    /// stands, lies and breaks apart, and its role.
    fn into_part(self, design: &HouseDesign, braces: Vec<Brace>) -> Part {
        let shapes = if self.cuboid && self.hulls.is_empty() {
            let footed = self.grounded && (self.wall.is_some() || self.kind == PartKind::Post);
            let bottom = if footed { self.min.with_y(0.0) } else { self.min };
            vec![PartShape::aligned(bottom, self.max)]
        } else if self.hulls.is_empty() {
            vec![PartShape::Hull(self.points.clone())]
        } else {
            self.hulls.iter().cloned().map(PartShape::Hull).collect()
        };
        let material = Material {
            strength: self.strength,
            ..self.kind.material()
        };
        let style = design.style;
        let paint = match self.kind {
            PartKind::Sheet | PartKind::Ridge => style.roof,
            PartKind::Chimney => CHIMNEY_BRICK,
            PartKind::Floor | PartKind::Partition => ROOM_PAINT,
            PartKind::Window => style.trim,
            _ => style.siding,
        };
        let pitch = design.roof().pitch;
        let lie = match (self.kind, self.slope, self.wall) {
            (PartKind::Chimney, ..) => Lie::Any,
            (PartKind::Post, ..) => Lie::Length(DVec3::Y),
            (PartKind::Ridge, ..) => Lie::Length(DVec3::Z),
            (PartKind::Floor, ..) => Lie::Face(DVec3::Y),
            (PartKind::Partition, ..) => Lie::Face(DVec3::Z),
            (PartKind::Sheet, Some((slope, _)), _) => Lie::Roof(
                match slope {
                    HousePart::RightRoof => DVec3::new(pitch, 1.0, 0.0),
                    HousePart::LeftRoof => DVec3::new(-pitch, 1.0, 0.0),
                    HousePart::FrontRoof => DVec3::new(0.0, 1.0, pitch),
                    _ => DVec3::new(0.0, 1.0, -pitch),
                }
                .normalize(),
            ),
            (_, _, Some(wall)) => Lie::Face(wall_frames(design.w, design.d)[wall].1),
            _ => Lie::Any,
        };
        // How a fallen body breaks apart: each roof slope, wall (and its upper storey),
        // gable end, floor, the ridge and the partition on its own, posts and the
        // chimney each alone.
        let element = match (self.kind, self.slope, self.wall) {
            (PartKind::Sheet, Some((slope, _)), _) => 1 + slope as u16,
            (PartKind::Ridge, ..) => 20,
            (PartKind::Partition, ..) => 21,
            (PartKind::Floor, ..) => 100 + (self.max.y * 10.0).round() as u16,
            (PartKind::Gable, _, Some(wall)) => 30 + 3 * wall as u16 + 2,
            (_, _, Some(wall)) => 30 + 3 * wall as u16 + u16::from(self.upper_storey),
            _ => 0,
        };
        let middle = self.center();
        let mut part = Part::new(
            shapes,
            material,
            Role::House(HouseRole {
                kind: self.kind,
                cells: self.cells,
                wall: self.wall,
                slope: self.slope,
                upper_storey: self.upper_storey,
            }),
        );
        part.min = self.min;
        part.max = self.max;
        part.mass_center = middle;
        part.paint = Some(paint);
        part.grounded = self.grounded;
        if self.grounded {
            part.contacts = part.base();
        }
        part.upright = self.kind.upright();
        // The stack stands on its footing whatever is left round it.
        part.anchor = self.kind == PartKind::Chimney && self.grounded;
        part.blocks_ground = self.min.y < GROUND_BLOCK && self.kind != PartKind::Floor;
        part.seat_needed = if self.kind == PartKind::Floor {
            FLOOR_SEAT_NEEDED
        } else {
            SEAT_NEEDED
        };
        part.braces = braces;
        // A plate over a gap is a beam: what it carries falls off with the whole span.
        part.beam = (self.kind == PartKind::Plate).then_some(PLATE_TIE);
        part.lie = lie;
        part.element = element;
        part
    }
}

/// The plane of the model's `part` that cells are measured in, as the collapse
/// pieces had it: along a wall and up it, along the ridge and out down a slope, across
/// a hip end and out down it, across the chimney and up it.
pub fn part_plane(design: &HouseDesign, part: HousePart) -> PartPlane {
    if let Some(index) = part.wall_index() {
        let (center, _, along, _) = wall_frames(design.w, design.d)[index];
        return PartPlane {
            origin: center,
            s: along,
            t: DVec3::Y,
        };
    }
    let (s, t) = match part {
        HousePart::RightRoof => (DVec3::Z, DVec3::X),
        HousePart::LeftRoof => (DVec3::Z, -DVec3::X),
        HousePart::FrontRoof => (DVec3::X, DVec3::Z),
        HousePart::BackRoof => (DVec3::X, -DVec3::Z),
        _ => (DVec3::X, DVec3::Y),
    };
    PartPlane {
        origin: DVec3::ZERO,
        s,
        t,
    }
}

/// The outline of the model's `part` in its plane ([`part_plane`]), as its collapse
/// pieces had it: where a torn edge along a missing neighbour ends.
pub fn part_outline(design: &HouseDesign, part: HousePart) -> Vec<DVec2> {
    let roof = design.roof();
    let (xe, ze, rh) = (roof.eave_x, roof.rake_z, roof.ridge_half);
    let gutter = xe + 0.17;
    if let Some(index) = part.wall_index() {
        let half = wall_frames(design.w, design.d)[index].3;
        let mut outline = vec![
            DVec2::new(-half, PLINTH),
            DVec2::new(half, PLINTH),
            DVec2::new(half, roof.wall),
        ];
        if roof.form == RoofForm::Gable && index < 2 {
            outline.push(DVec2::new(0.0, roof.ridge));
        }
        outline.push(DVec2::new(-half, roof.wall));
        return outline;
    }
    match (part, roof.form) {
        (HousePart::RightRoof | HousePart::LeftRoof, RoofForm::Gable) => vec![
            DVec2::new(-ze, 0.0),
            DVec2::new(ze, 0.0),
            DVec2::new(ze, gutter),
            DVec2::new(-ze, gutter),
        ],
        (HousePart::RightRoof | HousePart::LeftRoof, RoofForm::Hip) => vec![
            DVec2::new(-rh, 0.0),
            DVec2::new(rh, 0.0),
            DVec2::new(ze, xe),
            DVec2::new(ze, gutter),
            DVec2::new(-ze, gutter),
            DVec2::new(-ze, xe),
        ],
        (HousePart::FrontRoof | HousePart::BackRoof, _) => vec![
            DVec2::new(0.0, rh),
            DVec2::new(xe, ze),
            DVec2::new(xe, ze + 0.17),
            DVec2::new(-xe, ze + 0.17),
            DVec2::new(-xe, ze),
        ],
        _ => vec![
            DVec2::new(-FAR, -FAR),
            DVec2::new(FAR, -FAR),
            DVec2::new(FAR, FAR),
            DVec2::new(-FAR, FAR),
        ],
    }
}

/// A cottage's parts and how they hold each other up, as its builder lays them out.
struct Frame {
    design: HouseDesign,
    roof: Roof,
    parts: Vec<FramePart>,
    /// For each part, the parts that hold it up, lowest index first.
    supports: Vec<Vec<Support>>,
    /// For each wall part, its neighbours beside it in its wall.
    beside: Vec<Vec<u16>>,
}

/// A wall column: bays between the corner posts and the openings.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Column {
    Bay,
    Window,
    Door,
}

impl Frame {
    fn new(design: &HouseDesign) -> Self {
        let roof = design.roof();
        let mut structure = Self {
            design: *design,
            roof,
            parts: Vec::new(),
            supports: Vec::new(),
            beside: Vec::new(),
        };
        structure.build_posts(false);
        structure.build_chimney();
        debug_assert_eq!(structure.parts.len() - 1, CHIMNEY_PART);
        structure.build_posts(true);
        structure.build_stack();
        debug_assert_eq!(structure.parts.len() - 1, CHIMNEY_STACK);
        for index in 0..4 {
            structure.build_wall(index);
        }
        structure.build_floors();
        structure.build_partition();
        let walls = structure.parts.len();
        structure.supports = vec![Vec::new(); walls];
        structure.beside = vec![Vec::new(); walls];
        structure.link_by_contact();
        structure.link_plate_spans();
        structure.build_roof();
        structure.supports.resize(structure.parts.len(), Vec::new());
        structure.beside.resize(structure.parts.len(), Vec::new());
        structure.link_roof(walls);
        if design.style.walls == WallFinish::Brick {
            for part in &mut structure.parts {
                if part.kind.outer_wall() {
                    part.strength *= BRICK_STRENGTH;
                }
            }
        }
        for supports in &mut structure.supports {
            // The best share each supporter passes at each end, once.
            supports.sort_by(|a, b| {
                (a.by, end_rank(a.end))
                    .cmp(&(b.by, end_rank(b.end)))
                    .then(b.share.total_cmp(&a.share))
            });
            supports.dedup_by_key(|support| (support.by, support.end));
        }
        debug_assert!(structure.parts.len() <= MAX_PARTS);
        structure
    }

    /// The chimney over its stack, through the attic and the roof, standing on the roof
    /// it comes through: it comes down with the roof round it.
    fn build_chimney(&mut self) {
        let flue = self.design.flue();
        let (_, top) = self.design.chimney_span();
        let bottom = self.stack_top();
        let reach = DVec3::new(CHIMNEY / 2.0, 0.0, CHIMNEY / 2.0);
        let chimney = FramePart::cuboid(
            PartKind::Chimney,
            DVec3::new(flue.x, bottom, flue.z) - reach,
            DVec3::new(flue.x, top, flue.z) + reach,
            vec![PartCell {
                part: HousePart::Chimney,
                min: DVec2::new(-FAR, self.stack_top()),
                max: DVec2::splat(FAR),
            }],
        );
        self.parts.push(chimney);
    }

    /// The chimney's stack from the floor up through the rooms to the wall tops, on its
    /// own footing: nothing is nailed to it and it carries nothing, so a ruin keeps it
    /// standing until it is struck itself. It stays well under the roof's rafters, so a
    /// slope coming down turns down past it rather than catching on it and lifting.
    fn build_stack(&mut self) {
        let flue = self.design.flue();
        let reach = DVec3::new(CHIMNEY / 2.0, 0.0, CHIMNEY / 2.0);
        let top = self.stack_top();
        let mut stack = FramePart::cuboid(
            PartKind::Chimney,
            DVec3::new(flue.x, PLINTH, flue.z) - reach,
            DVec3::new(flue.x, top, flue.z) + reach,
            vec![PartCell {
                part: HousePart::Chimney,
                min: DVec2::splat(-FAR),
                max: DVec2::new(FAR, top),
            }],
        );
        debug_assert!(stack.grounded);
        stack.strength = STACK_STRENGTH;
        self.parts.push(stack);
    }

    /// Where the chimney's stack ends: at the wall tops, or under the lowest rafters
    /// over it if they come lower.
    fn stack_top(&self) -> f64 {
        let flue = self.design.flue();
        let rafters = [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)]
            .map(|(sx, sz)| self.roof.top(flue.x + sx * CHIMNEY / 2.0, flue.z + sz * CHIMNEY / 2.0))
            .into_iter()
            .fold(f64::INFINITY, f64::min)
            - self.roof.thick
            - SHEET_DEPTH;
        self.roof.wall.min(rafters - STACK_CLEAR)
    }

    /// The four corner posts (front right, front left, back right, back left) over the
    /// walls' bottom course, or with `feet` the posts' feet in it, each its own part: a
    /// wall that tips with its post snaps the post off over its foot as it snaps off its
    /// own stubs, so a ruin keeps its corners waist high.
    fn build_posts(&mut self, feet: bool) {
        let (hw, hd) = (self.design.w / 2.0, self.design.d / 2.0);
        let split = PLINTH + SILL;
        let (bottom, top) = if feet {
            (PLINTH, split)
        } else {
            (split, self.roof.wall)
        };
        let frames = wall_frames(self.design.w, self.design.d);
        for (sx, sz) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
            let a = DVec3::new(sx * hw, bottom, sz * hd);
            let b = DVec3::new(sx * (hw - POST), top, sz * (hd - POST));
            // Drawn as each wall's end at this corner: the end column of the front or
            // back wall and of the side wall that meet here.
            let cells = [if sz > 0.0 { 0 } else { 1 }, if sx > 0.0 { 2 } else { 3 }]
                .into_iter()
                .map(|index: usize| {
                    let (_, _, along, half) = frames[index];
                    // Which end of the wall the corner is at.
                    let end = along.dot(DVec3::new(sx, 0.0, sz)).signum();
                    let inner = half - POST;
                    let (min_s, max_s) = if end > 0.0 {
                        (inner, FAR)
                    } else {
                        (-FAR, -inner)
                    };
                    let gable = self.gable_wall(index);
                    let (low, high) = match (feet, gable) {
                        (true, _) => (-FAR, split),
                        (false, true) => (split, self.roof.wall),
                        (false, false) => (split, FAR),
                    };
                    PartCell {
                        part: HousePart::wall(index),
                        min: DVec2::new(min_s, low),
                        max: DVec2::new(max_s, high),
                    }
                })
                .collect();
            let post = FramePart::cuboid(PartKind::Post, a.min(b), a.max(b), cells);
            debug_assert_eq!(post.grounded, feet);
            self.parts.push(post);
        }
    }

    fn gable_wall(&self, index: usize) -> bool {
        self.roof.form == RoofForm::Gable && index < 2
    }

    /// A wall's columns between its corner posts: its openings and bays a little
    /// under [`BAY`] wide between them, as `(from, to, column)` along the wall.
    fn columns(&self, index: usize) -> Vec<(f64, f64, Column)> {
        let half = wall_frames(self.design.w, self.design.d)[index].3;
        let door = index == self.design.door_wall;
        let mut openings: Vec<(f64, f64, Column)> =
            window_columns(door, self.gable_wall(index), half)
                .into_iter()
                .map(|u| (u - WINDOW_HALF, u + WINDOW_HALF, Column::Window))
                .collect();
        if door {
            openings.push((-DOOR_HALF, DOOR_HALF, Column::Door));
        }
        openings.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (start, end) = (-half + POST, half - POST);
        let mut columns: Vec<(f64, f64, Column)> = Vec::new();
        let mut at = start;
        for &(from, to, column) in &openings {
            let (from, to) = (from.max(start), to.min(end));
            if from - at < NARROWEST_BAY {
                // Too narrow for a bay of its own: the opening takes it.
                columns.push((at, to, column));
            } else {
                bays(&mut columns, at, from);
                columns.push((from, to, column));
            }
            at = to;
        }
        if end - at < NARROWEST_BAY
            && let Some(last) = columns.last_mut()
        {
            last.1 = end;
        } else {
            bays(&mut columns, at, end);
        }
        columns
    }

    /// The bands of a column up the wall, `(bottom, top, kind)`: the panel under the
    /// sill, the window or the panel beside it, the plates (and, on two floors, the
    /// upper floor's the same again).
    fn bands(&self, column: Column) -> Vec<(f64, f64, PartKind)> {
        let wall = self.roof.wall;
        let sill = PLINTH + SILL;
        let head = sill + (wall - PLINTH - 1.25).min(1.3);
        let door_top = PLINTH + DOOR_RISE;
        let mut bands = match column {
            Column::Door => vec![(PLINTH, head.max(door_top), PartKind::Door)],
            // Under a window, a low course of wall over the footing that a shell at
            // hull height passes over: a shot under the sill leaves a ragged hole over
            // it, not a doorway.
            Column::Window => vec![
                (PLINTH, PLINTH + WINDOW_FOOT, PartKind::Panel),
                (PLINTH + WINDOW_FOOT, sill, PartKind::Panel),
                (sill, head, PartKind::Window),
            ],
            Column::Bay => vec![(PLINTH, sill, PartKind::Panel), (sill, head, PartKind::Panel)],
        };
        let floor = bands.last().map_or(head, |band| band.1);
        if self.design.storeys() == 2 {
            let upper_sill = head + BELT_OVER + UPPER_SILL;
            let upper_head = upper_sill + (wall - 0.35 - upper_sill).min(UPPER_WINDOW);
            let upper = if column == Column::Bay {
                PartKind::Panel
            } else {
                PartKind::Window
            };
            bands.push((floor, upper_sill, PartKind::Plate));
            bands.push((upper_sill, upper_head, upper));
            bands.push((upper_head, wall, PartKind::Plate));
        } else {
            bands.push((floor, wall, PartKind::Plate));
        }
        bands
    }

    fn build_wall(&mut self, index: usize) {
        let (center, out, along, _) = wall_frames(self.design.w, self.design.d)[index];
        let part = HousePart::wall(index);
        let gable = self.gable_wall(index);
        let upper_floor = self.upper_floor();
        let columns = self.columns(index);
        let sill = PLINTH + SILL;
        // How far a ground-floor window's cells reach into the bay beside it, so the
        // shutters hung there go with the window: (into the left bay, the right).
        let reach = |c: usize| -> (f64, f64) {
            if columns[c].2 != Column::Window {
                return (0.0, 0.0);
            }
            let into = |n: Option<&(f64, f64, Column)>| {
                n.filter(|bay| bay.2 == Column::Bay)
                    .map_or(0.0, |bay| SHUTTER_REACH.min(bay.1 - bay.0 - NARROWEST_SIDE).max(0.0))
            };
            (into(c.checked_sub(1).and_then(|k| columns.get(k))), into(columns.get(c + 1)))
        };
        for (c, &(from, to, column)) in columns.iter().enumerate() {
            let bands = self.bands(column);
            let last_band = bands.len() - 1;
            // Where each band's cell starts and ends up the wall: a window's takes its
            // trim from the bands over and under it.
            let trim = |b: usize, edge: f64, upward: bool| {
                let window = |k: Option<usize>| k.and_then(|k| bands.get(k)).is_some_and(|band| band.2 == PartKind::Window);
                match upward {
                    // A band's top: under a window it stops short of the sill trim; a
                    // window's reaches over its head trim.
                    true if window(Some(b + 1)) => edge - WINDOW_SILL_TRIM,
                    true if window(Some(b)) => edge + WINDOW_HEAD_TRIM,
                    // A band's bottom: over a window it starts above the head trim; a
                    // window's reaches under its sill trim.
                    false if window(b.checked_sub(1)) => edge + WINDOW_HEAD_TRIM,
                    false if window(Some(b)) => edge - WINDOW_SILL_TRIM,
                    _ => edge,
                }
            };
            for (b, &(bottom, top, kind)) in bands.iter().enumerate() {
                let corner =
                    |s: f64, t: f64, depth: f64| center + along * s + DVec3::Y * t - out * depth;
                let (p, q) = (corner(from, bottom, 0.0), corner(to, top, WALL_DEPTH));
                // The model's cell: the bottom band reaches on down past the plinth
                // and the top one up into the eaves (on a gable wall, up to the gable).
                let mut cell = PartCell {
                    part,
                    min: DVec2::new(from, if b == 0 { -FAR } else { trim(b, bottom, false) }),
                    max: DVec2::new(
                        to,
                        if b < last_band || gable { trim(b, top, true) } else { FAR },
                    ),
                };
                let mut cells = Vec::new();
                let window_band = (bottom - sill).abs() < CONTACT;
                if kind == PartKind::Window && window_band {
                    // The shutters beside it, in the bays' window-high bands.
                    let (left, right) = reach(c);
                    for (a, z) in [(from - left, from), (to, to + right)] {
                        if z - a > 1e-6 {
                            cells.push(PartCell {
                                part,
                                min: DVec2::new(a, bottom),
                                max: DVec2::new(z, top),
                            });
                        }
                    }
                } else if column == Column::Bay && window_band {
                    // What the windows beside it took of this band for their shutters.
                    let left = c.checked_sub(1).map_or(0.0, |k| reach(k).1);
                    let right = if c + 1 < columns.len() { reach(c + 1).0 } else { 0.0 };
                    cell.min.x += left;
                    cell.max.x -= right;
                }
                cells.insert(0, cell);
                let mut part = FramePart::cuboid(kind, p.min(q), p.max(q), cells);
                part.wall = Some(index);
                part.upper_storey = upper_floor.is_some_and(|floor| bottom >= floor - CONTACT);
                self.parts.push(part);
            }
        }
        if gable {
            self.build_gable(index);
        }
    }

    /// A gable over the plates, in three pieces: the middle one under the ridge.
    fn build_gable(&mut self, index: usize) {
        let (center, out, along, half) = wall_frames(self.design.w, self.design.d)[index];
        let roof = self.roof;
        let third = half / 3.0;
        for (from, to) in [(-half, -third), (-third, third), (third, half)] {
            let rise = |s: f64| roof.ridge - roof.pitch * s.abs();
            let mut outline = vec![
                DVec2::new(from, roof.wall),
                DVec2::new(to, roof.wall),
                DVec2::new(to, rise(to)),
            ];
            if from < 0.0 && to > 0.0 {
                outline.push(DVec2::new(0.0, roof.ridge));
            }
            outline.push(DVec2::new(from, rise(from)));
            let points = outline
                .iter()
                .flat_map(|q| {
                    let face = center + along * q.x + DVec3::Y * q.y;
                    [face, face - out * WALL_DEPTH]
                })
                .collect();
            let cell = PartCell {
                part: HousePart::wall(index),
                min: DVec2::new(if from <= -half { -FAR } else { from }, roof.wall),
                max: DVec2::new(if to >= half { FAR } else { to }, FAR),
            };
            let mut part = FramePart::new(PartKind::Gable, points, false, vec![cell]);
            part.wall = Some(index);
            self.parts.push(part);
        }
    }

    /// A two-storey house's upper floor level.
    fn upper_floor(&self) -> Option<f64> {
        (self.design.storeys() == 2).then(|| {
            let head = PLINTH + SILL + (self.roof.wall - PLINTH - 1.25).min(1.3);
            head + BELT_OVER
        })
    }

    /// The attic floor under the wall tops (and a two-storey house's upper floor over
    /// the ground floor's plates), in strips across the cottage.
    fn build_floors(&mut self) {
        let inside = DVec2::new(
            self.design.w / 2.0 - WALL_DEPTH,
            self.design.d / 2.0 - WALL_DEPTH,
        );
        let mut levels = vec![self.roof.wall];
        levels.extend(self.upper_floor());
        let strips = ((2.0 * inside.y) / FLOOR_STRIP).round().max(1.0) as usize;
        // The chimney's stack comes up through the floors: their boards are framed round
        // it, so a floor coming down drops past it.
        let flue = self.design.flue();
        let reach = DVec2::splat(CHIMNEY / 2.0 + HOLE_MARGIN);
        let (hole_low, hole_high) = (DVec2::new(flue.x, flue.z) - reach, DVec2::new(flue.x, flue.z) + reach);
        for level in levels {
            for k in 0..strips {
                let z0 = -inside.y + 2.0 * inside.y * k as f64 / strips as f64;
                let z1 = -inside.y + 2.0 * inside.y * (k + 1) as f64 / strips as f64;
                let (bottom, top) = (level - FLOOR_DEPTH, level);
                let mut floor = FramePart::cuboid(
                    PartKind::Floor,
                    DVec3::new(-inside.x, bottom, z0),
                    DVec3::new(inside.x, top, z1),
                    Vec::new(),
                );
                let plan = [
                    DVec2::new(-inside.x, z0),
                    DVec2::new(inside.x, z0),
                    DVec2::new(inside.x, z1),
                    DVec2::new(-inside.x, z1),
                ];
                floor.hulls = framed(&plan, hole_low, hole_high)
                    .into_iter()
                    .map(|strip| {
                        strip
                            .iter()
                            .flat_map(|q| [DVec3::new(q.x, bottom, q.y), DVec3::new(q.x, top, q.y)])
                            .collect()
                    })
                    .collect();
                self.parts.push(floor);
            }
        }
    }

    /// The wall across the rooms of the ground floor, with a doorway in the middle,
    /// from the ground floor's boards (laid on the ground inside the footing) up to the
    /// floor over it.
    fn build_partition(&mut self) {
        let inside_x = self.design.w / 2.0 - WALL_DEPTH;
        let z = self.design.d * PARTITION_AT;
        let top = self
            .parts
            .iter()
            .filter(|part| part.kind == PartKind::Floor)
            .map(|part| part.min.y)
            .fold(f64::INFINITY, f64::min);
        for side in [-1.0, 1.0] {
            let span = inside_x - PARTITION_DOOR;
            let panels = (span / BAY).round().max(1.0) as usize;
            for k in 0..panels {
                let a = PARTITION_DOOR + span * k as f64 / panels as f64;
                let b = PARTITION_DOOR + span * (k + 1) as f64 / panels as f64;
                let (x0, x1) = (side * a, side * b);
                self.parts.push(FramePart::cuboid(
                    PartKind::Partition,
                    DVec3::new(x0.min(x1), 0.0, z - PARTITION_THICKNESS / 2.0),
                    DVec3::new(x0.max(x1), top, z + PARTITION_THICKNESS / 2.0),
                    Vec::new(),
                ));
            }
        }
    }

    /// Supports between the walls, posts, floors and partition from how their boxes
    /// touch: what lies on a part rests on it, what lies under it hangs from it and
    /// what touches its side is tied to it.
    fn link_by_contact(&mut self) {
        let count = self.parts.len();
        for i in 0..count {
            for j in 0..count {
                if i == j {
                    continue;
                }
                let (a, b) = (&self.parts[i], &self.parts[j]);
                if a.kind == PartKind::Chimney || b.kind == PartKind::Chimney {
                    continue;
                }
                let apart = (b.min - a.max).max(a.min - b.max);
                if apart.max_element() > CONTACT {
                    continue;
                }
                let relation = if a.min.y >= b.max.y - CONTACT {
                    Relation::Below
                } else if b.min.y >= a.max.y - CONTACT {
                    Relation::Above
                } else if a.max.y.min(b.max.y) - a.min.y.max(b.min.y) > 0.1 {
                    Relation::Beside
                } else {
                    continue;
                };
                // Touching only at an edge or a corner holds nothing.
                let plan_overlap =
                    |axis: usize| a.max[axis].min(b.max[axis]) - a.min[axis].max(b.min[axis]);
                if relation != Relation::Beside
                    && (plan_overlap(0) <= CONTACT || plan_overlap(2) <= CONTACT)
                {
                    continue;
                }
                let mut share = support_share(a.kind, b.kind, relation);
                // A floor's joists run across the cottage (along x) and bear on the
                // eave walls' plates; a gable wall's plate runs along a strip's side.
                let joists = relation == Relation::Beside
                    && (a.kind, b.kind) == (PartKind::Floor, PartKind::Plate)
                    && b.wall.is_some_and(|wall| wall >= 2);
                if relation == Relation::Beside
                    && (a.kind, b.kind) == (PartKind::Floor, PartKind::Plate)
                    && !joists
                {
                    share = LOOSE_TIE;
                }
                // What seats it: the plan it lies on, or the length a floor's
                // joists sit along a plate.
                let seat = match relation {
                    Relation::Below => plan_overlap(0) * plan_overlap(2),
                    Relation::Beside if joists => plan_overlap(0).max(plan_overlap(2)),
                    _ => 0.0,
                };
                let end = match (a.kind, relation) {
                    // A plate's sides, toward the wall's start or its end.
                    (PartKind::Plate, Relation::Beside)
                        if matches!(b.kind, PartKind::Plate | PartKind::Post)
                            && a.wall.is_some() =>
                    {
                        let along = wall_frames(self.design.w, self.design.d)
                            [a.wall.expect("checked")]
                        .2;
                        if (b.center() - a.center()).dot(along) < 0.0 {
                            End::A
                        } else {
                            End::B
                        }
                    }
                    // A floor strip's joists span from one eave wall's plate to the
                    // other's: off one, it does not cantilever off the other.
                    (PartKind::Floor, Relation::Beside) if joists => {
                        if b.center().x < a.center().x {
                            End::A
                        } else {
                            End::B
                        }
                    }
                    (PartKind::Window, Relation::Below) => End::B,
                    (PartKind::Window, _) => {
                        // Its frame holds it whole while it stands on its sill.
                        share = REST;
                        End::A
                    }
                    _ => End::Any,
                };
                if relation == Relation::Beside
                    && a.wall.is_some()
                    && a.wall == b.wall
                    && a.kind != PartKind::Gable
                {
                    self.beside[i].push(j as u16);
                }
                if share > 0.0 {
                    self.supports[i].push(Support {
                        by: j as u16,
                        share,
                        seat: seat.max(0.0) as f32,
                        end,
                    });
                }
            }
        }
    }

    /// A wall's plates (and the corner posts at its ends) carry each other on across a
    /// gap of more than one bay: the top plate runs on past a broken bay to the next
    /// one standing, weaker for each bay it spans ([`PLATE_TIE`] a bay, up to
    /// [`PLATE_SPAN`] bays), so two plates over a two-bay hole hold each other's ends.
    fn link_plate_spans(&mut self) {
        let frames = wall_frames(self.design.w, self.design.d);
        for (wall, &(_, _, along, _)) in frames.iter().enumerate() {
            // The wall's plate courses, each in order along the wall.
            let mut courses: Vec<(i64, Vec<(f64, usize)>)> = Vec::new();
            for (index, part) in self.parts.iter().enumerate() {
                if part.kind != PartKind::Plate || part.wall != Some(wall) {
                    continue;
                }
                let key = (part.min.y * 100.0).round() as i64;
                let at = part.center().dot(along);
                match courses.iter_mut().find(|(course, _)| *course == key) {
                    Some((_, plates)) => plates.push((at, index)),
                    None => courses.push((key, vec![(at, index)])),
                }
            }
            for (_, plates) in &mut courses {
                plates.sort_by(|a, b| a.0.total_cmp(&b.0));
                // The posts the course's end plates are framed into.
                let post_of = |plate: usize| {
                    self.supports[plate].iter().find_map(|support| {
                        (self.parts[support.by as usize].kind == PartKind::Post).then_some(support.by)
                    })
                };
                let first_post = plates.first().and_then(|&(_, plate)| post_of(plate));
                let last_post = plates.last().and_then(|&(_, plate)| post_of(plate));
                let count = plates.len();
                let mut links: Vec<(usize, Support)> = Vec::new();
                for (i, &(_, plate)) in plates.iter().enumerate() {
                    for k in 2..=PLATE_SPAN {
                        let share = PLATE_TIE.powi(k as i32);
                        if i >= k {
                            links.push((plate, Support { by: plates[i - k].1 as u16, share, seat: 0.0, end: End::A }));
                        } else if i + 1 == k
                            && let Some(post) = first_post
                        {
                            links.push((plate, Support { by: post, share, seat: 0.0, end: End::A }));
                        }
                        if i + k < count {
                            links.push((plate, Support { by: plates[i + k].1 as u16, share, seat: 0.0, end: End::B }));
                        } else if i + k == count
                            && let Some(post) = last_post
                        {
                            links.push((plate, Support { by: post, share, seat: 0.0, end: End::B }));
                        }
                    }
                }
                for (plate, support) in links {
                    self.supports[plate].push(support);
                }
            }
        }
    }

    /// The roof: each slope in sheets (columns along the ridge, an upper and a lower
    /// row), hip ends in two sheets each, the ridge in segments and the chimney.
    fn build_roof(&mut self) {
        let roof = self.roof;
        let columns = ((2.0 * roof.rake_z) / SHEET_LENGTH).round().max(2.0) as usize;
        let cut = |k: usize| -roof.rake_z + 2.0 * roof.rake_z * k as f64 / columns as f64;
        let row = roof.eave_x * UPPER_ROW;
        for side in [1.0, -1.0] {
            let part = if side > 0.0 {
                HousePart::RightRoof
            } else {
                HousePart::LeftRoof
            };
            // The slope in plan as (along the ridge, out from it).
            let outline: Vec<DVec2> = match roof.form {
                RoofForm::Gable => vec![
                    DVec2::new(-roof.rake_z, 0.0),
                    DVec2::new(roof.rake_z, 0.0),
                    DVec2::new(roof.rake_z, roof.eave_x),
                    DVec2::new(-roof.rake_z, roof.eave_x),
                ],
                RoofForm::Hip => vec![
                    DVec2::new(-roof.ridge_half, 0.0),
                    DVec2::new(roof.ridge_half, 0.0),
                    DVec2::new(roof.ridge_half + roof.eave_x, roof.eave_x),
                    DVec2::new(-roof.ridge_half - roof.eave_x, roof.eave_x),
                ],
            };
            for upper in [true, false] {
                for k in 0..columns {
                    let (s0, s1) = (cut(k), cut(k + 1));
                    let (t0, t1) = if upper { (0.0, row) } else { (row, roof.eave_x) };
                    let plan = clip_rect(&outline, DVec2::new(s0, t0), DVec2::new(s1, t1));
                    if plan.len() < 3 {
                        continue;
                    }
                    let cell = PartCell {
                        part,
                        min: DVec2::new(
                            if k == 0 { -FAR } else { s0 },
                            if upper { -FAR } else { t0 },
                        ),
                        max: DVec2::new(
                            if k == columns - 1 { FAR } else { s1 },
                            if upper { t1 } else { FAR },
                        ),
                    };
                    self.push_sheet((part, upper), &plan, cell, &|q| DVec3::new(side * q.y, 0.0, q.x));
                }
            }
        }
        if roof.form == RoofForm::Hip {
            for end in [1.0, -1.0] {
                let part = if end > 0.0 {
                    HousePart::FrontRoof
                } else {
                    HousePart::BackRoof
                };
                // The end slope in plan as (across, out along the ridge).
                let outline = vec![
                    DVec2::new(0.0, roof.ridge_half),
                    DVec2::new(roof.eave_x, roof.rake_z),
                    DVec2::new(-roof.eave_x, roof.rake_z),
                ];
                for half in [-1.0, 1.0] {
                    let (s0, s1) = if half < 0.0 {
                        (-roof.eave_x, 0.0)
                    } else {
                        (0.0, roof.eave_x)
                    };
                    let plan = clip_rect(
                        &outline,
                        DVec2::new(s0, roof.ridge_half),
                        DVec2::new(s1, roof.rake_z),
                    );
                    if plan.len() < 3 {
                        continue;
                    }
                    let cell = PartCell {
                        part,
                        min: DVec2::new(if half < 0.0 { -FAR } else { 0.0 }, -FAR),
                        max: DVec2::new(if half < 0.0 { 0.0 } else { FAR }, FAR),
                    };
                    self.push_sheet((part, false), &plan, cell, &|q| DVec3::new(q.x, 0.0, end * q.y));
                }
            }
        }
        // The ridge beam, cut where the sheets are.
        let reach = match roof.form {
            RoofForm::Gable => self.design.d / 2.0,
            RoofForm::Hip => roof.ridge_half,
        };
        for k in 0..columns {
            let (z0, z1) = (cut(k).max(-reach), cut(k + 1).min(reach));
            if z1 - z0 < 0.3 {
                continue;
            }
            self.parts.push(FramePart::cuboid(
                PartKind::Ridge,
                DVec3::new(-RIDGE_HALF, roof.ridge - RIDGE_DEPTH, z0),
                DVec3::new(RIDGE_HALF, roof.ridge + roof.thick * 0.5, z1),
                Vec::new(),
            ));
        }
    }

    /// A roof sheet of `slope` with plan `plan` (in its part's plane, placed in the build
    /// frame by `place`) drawn as `cell`, framed round the hole the chimney comes through
    /// where it does: its collider is then the strips of deck round the hole, so a roof
    /// coming down slides down past the stack rather than landing on it like a table.
    fn push_sheet(&mut self, slope: (HousePart, bool), plan: &[DVec2], cell: PartCell, place: &dyn Fn(DVec2) -> DVec3) {
        let points = slab(&self.roof, plan.iter().map(|&q| place(q)));
        let mut sheet = FramePart::new(PartKind::Sheet, points, false, vec![cell]);
        sheet.slope = Some(slope);
        let (hole_low, hole_high) = chimney_hole(&self.design, slope.0);
        sheet.hulls = framed(plan, hole_low, hole_high)
            .into_iter()
            .map(|strip| slab(&self.roof, strip.iter().map(|&q| place(q))))
            .collect();
        self.parts.push(sheet);
    }

    /// Supports of the roof (sheets, ridge, chimney: parts from `first` on): the
    /// rafters' feet on the eave walls' plates, their tops at the ridge, the ridge on
    /// the gables or the hips, and the decking between sheets.
    fn link_roof(&mut self, first: usize) {
        let roof = self.roof;
        let (hw, hd) = (self.design.w / 2.0, self.design.d / 2.0);
        let count = self.parts.len();
        let overlap = |a: (f64, f64), b: (f64, f64)| a.1.min(b.1) - a.0.max(b.0);
        let touching = |a: &FramePart, b: &FramePart| {
            (b.min - a.max).max(a.min - b.max).max_element() <= CONTACT
        };
        let z = |p: &FramePart| (p.min.z, p.max.z);
        let x = |p: &FramePart| (p.min.x, p.max.x);
        let mut links: Vec<(usize, usize, f32, f64)> = Vec::new();
        for i in first..count {
            let a = &self.parts[i];
            match (a.kind, a.slope) {
                (PartKind::Sheet, Some((slope, upper))) => {
                    let side = match slope {
                        HousePart::RightRoof => Some(1.0),
                        HousePart::LeftRoof => Some(-1.0),
                        _ => None,
                    };
                    for (j, b) in self.parts.iter().enumerate() {
                        if i == j {
                            continue;
                        }
                        let wall_top = (b.max.y - roof.wall).abs() < CONTACT;
                        let feet = matches!(b.kind, PartKind::Plate | PartKind::Post) && wall_top;
                        match (b.kind, side) {
                            // Rafters' feet: an eave sheet on the plates (and posts)
                            // along its eave.
                            (_, Some(side)) if feet && !upper => {
                                let eave = (b.max.x - side * hw).abs() < POST + CONTACT
                                    || (b.min.x - side * hw).abs() < POST + CONTACT;
                                let seat = overlap(z(a), z(b));
                                if eave && b.wall.is_none_or(|w| w >= 2) && seat > CONTACT {
                                    links.push((i, j, REST, seat));
                                }
                            }
                            // A hip end's on its end wall's.
                            (_, None) if feet => {
                                let end = a.center().z.signum();
                                let wall = (b.max.z - end * hd).abs() < POST + CONTACT
                                    || (b.min.z - end * hd).abs() < POST + CONTACT;
                                let seat = overlap(x(a), x(b));
                                if wall && b.wall.is_none_or(|w| w < 2) && seat > CONTACT {
                                    links.push((i, j, REST, seat));
                                }
                            }
                            // Rafters' tops on the ridge: rafter pairs bear on
                            // each other there, so the ridge need not seat them firmly.
                            (PartKind::Ridge, Some(_)) if upper => {
                                let seat = overlap(z(a), z(b));
                                if seat > CONTACT {
                                    links.push((i, j, REST, 0.0));
                                    links.push((j, i, LOOSE_TIE, 0.0));
                                }
                            }
                            // A hip end carries the ridge's end.
                            (PartKind::Ridge, None) => {
                                let end = if a.center().z > 0.0 { b.max.z } else { b.min.z };
                                if (end.abs() - roof.ridge_half).abs() < 0.4 {
                                    links.push((j, i, HIP_TIE, 0.0));
                                    links.push((i, j, LOOSE_TIE, 0.0));
                                }
                            }
                            (PartKind::Sheet, _) if j > i && touching(a, b) => {
                                let across = matches!(
                                    (slope, b.slope),
                                    (HousePart::RightRoof, Some((HousePart::LeftRoof, _)))
                                        | (HousePart::LeftRoof, Some((HousePart::RightRoof, _)))
                                );
                                let both_upper = upper && b.slope.is_some_and(|(_, up)| up);
                                let down_slope = b.slope == Some((slope, false))
                                    && upper
                                    && overlap(z(a), z(b)) > CONTACT;
                                if down_slope {
                                    // Its rafters run on down to the plate: a sheet
                                    // clear of the ridge (a hip's jack rafters) rests on
                                    // the sheet under it, one under the ridge needs both.
                                    let ridge = self.parts.iter().any(|r| {
                                        r.kind == PartKind::Ridge && overlap(z(a), z(r)) > CONTACT
                                    });
                                    let share = if ridge { RIDGE_PAIR } else { REST };
                                    links.push((i, j, share, 0.0));
                                    links.push((j, i, SHEET_TIE, 0.0));
                                    continue;
                                }
                                let share = if !across {
                                    SHEET_TIE
                                } else if both_upper && overlap(z(a), z(b)) > CONTACT {
                                    RIDGE_PAIR
                                } else {
                                    continue;
                                };
                                links.push((i, j, share, 0.0));
                                links.push((j, i, share, 0.0));
                            }
                            // The chimney's top stands on the roof it comes through,
                            // and carries none of it.
                            (PartKind::Chimney, _) if j == CHIMNEY_PART && touching(a, b) => {
                                links.push((j, i, REST, 0.0));
                            }
                            _ => {}
                        }
                    }
                }
                (PartKind::Ridge, _) => {
                    for (j, b) in self.parts.iter().enumerate() {
                        if i == j || overlap(z(a), z(b)) < -CONTACT {
                            continue;
                        }
                        match b.kind {
                            // Per length of ridge spanned, as the sheets' columns cut it.
                            PartKind::Ridge => {
                                let span = (a.max.z - a.min.z) / RIDGE_TIE_SPAN;
                                links.push((i, j, RIDGE_TIE.powf(span as f32), 0.0));
                            }
                            PartKind::Chimney if j == CHIMNEY_PART && touching(a, b) => {
                                links.push((j, i, REST, 0.0));
                            }
                            // A gable's middle piece carries the ridge's end.
                            PartKind::Gable if b.min.x < 0.0 && b.max.x > 0.0 => {
                                links.push((i, j, REST, 1.0));
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        for (i, j, share, seat) in links {
            self.supports[i].push(Support {
                by: j as u16,
                share,
                seat: seat as f32,
                end: End::Any,
            });
        }
        self.link_rafters(first);
    }

    /// The two ends of each rafter span under the ridge: an upper sheet's top on the
    /// ridge (and the rafters it meets across it), its foot on the eave plates under the
    /// sheet below it, and that lower sheet the same, so both come down when either end
    /// of their rafters is gone.
    fn link_rafters(&mut self, first: usize) {
        let mut added: Vec<(usize, Support)> = Vec::new();
        let mut feet_of: Vec<(usize, Vec<u16>)> = Vec::new();
        for upper in first..self.parts.len() {
            let Some((slope, true)) = self.parts[upper].slope else {
                continue;
            };
            let tops: Vec<Support> = self.supports[upper]
                .iter()
                .filter(|support| {
                    let by = &self.parts[support.by as usize];
                    by.kind == PartKind::Ridge
                        || (support.share == RIDGE_PAIR && by.slope.is_some_and(|(_, up)| up))
                })
                .copied()
                .collect();
            if !tops
                .iter()
                .any(|top| self.parts[top.by as usize].kind == PartKind::Ridge)
            {
                continue;
            }
            // The sheets below it down the slope, which its rafters run on through.
            let lowers: Vec<usize> = self.supports[upper]
                .iter()
                .map(|support| support.by as usize)
                .filter(|&by| self.parts[by].slope == Some((slope, false)))
                .collect();
            let feet: Vec<Support> = lowers
                .iter()
                .flat_map(|&lower| self.supports[lower].iter())
                .filter(|support| {
                    support.seat > 0.0
                        && matches!(
                            self.parts[support.by as usize].kind,
                            PartKind::Plate | PartKind::Post
                        )
                })
                .copied()
                .collect();
            if feet.is_empty() {
                continue;
            }
            for top in &tops {
                added.push((upper, Support { end: End::A, ..*top }));
                for &lower in &lowers {
                    // The lower sheet holds at its top by the same ridge, unseated:
                    // the rafters' seat there is the upper sheet's.
                    added.push((
                        lower,
                        Support {
                            end: End::A,
                            seat: 0.0,
                            ..*top
                        },
                    ));
                }
            }
            for foot in &feet {
                added.push((upper, Support { end: End::B, ..*foot }));
            }
            for &lower in &lowers {
                feet_of.push((lower, feet.iter().map(|foot| foot.by).collect()));
            }
        }
        // The lower sheets' own seats on the plates are their feet.
        for (lower, feet) in feet_of {
            for support in &mut self.supports[lower] {
                if support.seat > 0.0 && feet.contains(&support.by) {
                    support.end = End::B;
                }
            }
        }
        // A span's ends replace the plain links they came from.
        for (part, support) in &added {
            self.supports[*part].retain(|old| old.end != End::Any || old.by != support.by);
        }
        for (part, support) in added {
            self.supports[part].push(support);
        }
    }

    /// What braces part `index` however well it is seated: a wall part above the bottom
    /// course stands only with a neighbour beside it in its wall or the corner post it is
    /// framed into (a one-bay strip of wall standing alone breaks off over its stub), an
    /// upper storey's wall part only with its corner post and the floor behind it (the
    /// remains of a two-storey house stand one storey high), a corner post only with a
    /// wall part above the bottom course on it (a bare post), a gable end only with the
    /// roof bracing it or its whole wall under it (a gable triangle over a broken wall is
    /// a stage flat), and the inner wall only with the floor over it.
    fn braces(&self, index: usize) -> Vec<Brace> {
        let part = &self.parts[index];
        let count = self.parts.len();
        match part.kind {
            PartKind::Post if !part.grounded => vec![Brace::any((0..count).filter(|&other| {
                let wall = &self.parts[other];
                wall.wall.is_some()
                    && !wall.grounded
                    && self.supports[other]
                        .iter()
                        .any(|support| support.by as usize == index)
            }))],
            // The door stands on the ground but reaches up to the plates: a doorway's
            // leaf with no wall left beside it falls like the bays above a stub.
            PartKind::Panel | PartKind::Window | PartKind::Door | PartKind::Plate
                if part.wall.is_some() && (!part.grounded || part.kind == PartKind::Door) =>
            {
                // Its neighbours in the wall, the corner post it is framed into, or (a pier
                // between two openings) the plate over it, while that is braced.
                let posts = self.supports[index]
                    .iter()
                    .map(|support| support.by as usize)
                    .filter(|&by| self.parts[by].kind == PartKind::Post);
                let beside = self.beside[index].iter().map(|&other| other as usize);
                let mut brace = Brace::any(beside.chain(posts));
                if part.kind != PartKind::Plate {
                    brace.via = self.supports[index]
                        .iter()
                        .map(|support| support.by as usize)
                        .filter(|&by| {
                            let over = &self.parts[by];
                            over.kind == PartKind::Plate && over.min.y >= part.max.y - CONTACT
                        })
                        .map(|plate| (plate as u16, self.beside[plate].clone()))
                        .collect();
                }
                let mut braces = vec![brace];
                if part.upper_storey
                    && let Some(floor) = self.upper_floor()
                {
                    let middle = part.center().with_y(0.0);
                    let corner = (0..4)
                        .min_by(|&a, &b| {
                            let gap =
                                |post: usize| (self.parts[post].center().with_y(0.0) - middle).length();
                            gap(a).total_cmp(&gap(b))
                        })
                        .expect("four corner posts");
                    braces.push(Brace::any([corner]));
                    braces.push(Brace::any((0..count).filter(|&other| {
                        let strip = &self.parts[other];
                        strip.kind == PartKind::Floor
                            && (strip.max.y - floor).abs() < CONTACT
                            && (strip.min - part.max)
                                .max(part.min - strip.max)
                                .with_y(0.0)
                                .max_element()
                                < FLOOR_REACH
                    })));
                }
                braces
            }
            PartKind::Gable => vec![Brace {
                via: Vec::new(),
                any: (0..count)
                    .filter(|&other| {
                        self.parts[other].kind == PartKind::Sheet && self.touch(index, other, ROOF_BRACE)
                    })
                    .map(|other| other as u16)
                    .collect(),
                all: (0..count)
                    .filter(|&other| {
                        let below = &self.parts[other];
                        below.wall == part.wall && below.kind != PartKind::Gable
                    })
                    .map(|other| other as u16)
                    .collect(),
            }],
            PartKind::Partition => vec![Brace::any((0..count).filter(|&other| {
                self.parts[other].kind == PartKind::Floor && self.touch(index, other, CONTACT)
            }))],
            _ => Vec::new(),
        }
    }

    /// Whether parts `a` and `b`'s boxes come within `gap` (m) of each other.
    fn touch(&self, a: usize, b: usize, gap: f64) -> bool {
        let (a, b) = (&self.parts[a], &self.parts[b]);
        (b.min - a.max).max(a.min - b.max).max_element() <= gap
    }

    /// The cottage as a structure: its parts fastened to what holds them up (all but the
    /// chimney, whose masonry is not nailed to the frame), standing by the share rule and
    /// tipping, levelled once its roof is down.
    fn into_structure(self) -> Structure {
        let braces: Vec<Vec<Brace>> = (0..self.parts.len()).map(|index| self.braces(index)).collect();
        let outlines: Vec<Vec<DVec3>> = self.parts.iter().map(|part| part.points.clone()).collect();
        let design = self.design;
        let sheets = PartSet::from_indices(
            (0..self.parts.len()).filter(|&index| self.parts[index].kind == PartKind::Sheet),
        );
        let parts: Vec<Part> = self
            .parts
            .into_iter()
            .zip(braces)
            .map(|(part, braces)| part.into_part(&design, braces))
            .collect();
        let mut structure = Structure::new(
            Build::House {
                design,
                roof: self.roof,
            },
            design.yaw(),
            parts,
            COTTAGE_DEBRIS,
        );
        structure.supports = self.supports;
        structure.join_supports(&PartSet::from_indices([CHIMNEY_PART, CHIMNEY_STACK]));
        structure.tipping = Some(COTTAGE_TIPPING);
        structure.levelling = Levelling {
            keys: sheets,
            share: LEVELLED,
            announce: true,
            dust: COTTAGE_DUST,
        };
        structure.variant = design.variant;
        add_sheet_pieces(&mut structure, &design, &outlines);
        structure
    }
}

/// A roof sheet a landing leaves on its own breaks across its rafters, in two at an
/// uneven place down the slope (a long one also across its length, in two), its torn
/// edges where the model shows snapped rafters and broken boards: a slab of shingles
/// never lies whole in a ruin, nor stands on its edge as a card the height of the slope.
fn add_sheet_pieces(structure: &mut Structure, design: &HouseDesign, outlines: &[Vec<DVec3>]) {
    let roof = design.roof();
    let first = structure.parts.len() + structure.pieces.len();
    let mut pieces: Vec<Part> = Vec::new();
    let mut breakups = Vec::new();
    // Each sheet with its whole slab, the hole for the chimney aside.
    for (index, (part, points)) in structure.parts.iter().zip(outlines).enumerate() {
        let role = part.house().clone();
        let Some(cell) = role.cells.first().copied() else {
            continue;
        };
        if role.kind != PartKind::Sheet || role.cells.len() != 1 {
            continue;
        }
        // The sheet in plan in its plane's (along, down the slope) coordinates: the slab's
        // top corners, in order.
        let plane = part_plane(design, cell.part);
        let plan: Vec<DVec2> = points
            .iter()
            .step_by(2)
            .map(|p| DVec2::new(p.dot(plane.s), p.dot(plane.t)))
            .collect();
        let (low, high) = plan.iter().fold(
            (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
            |(low, high), q| (low.min(*q), high.max(*q)),
        );
        let cuts = |from: f64, to: f64, shares: &[f64]| -> Vec<f64> {
            std::iter::once(from)
                .chain(shares.iter().map(|share| from + (to - from) * share))
                .chain(std::iter::once(to))
                .collect()
        };
        let along = if high.x - low.x > LONG_SHEET {
            cuts(low.x, high.x, &[if index % 2 == 0 { 0.45 } else { 0.55 }])
        } else {
            cuts(low.x, high.x, &[])
        };
        let down = cuts(low.y, high.y, &[if index % 2 == 0 { 0.38 } else { 0.62 }]);
        let mut made = Vec::new();
        for s_span in along.windows(2) {
            for t_span in down.windows(2) {
                let (min, max) = (DVec2::new(s_span[0], t_span[0]), DVec2::new(s_span[1], t_span[1]));
                let piece_plan = clip_rect(&plan, min, max);
                if piece_plan.len() < 3 {
                    continue;
                }
                let (hole_low, hole_high) = chimney_hole(design, cell.part);
                let mut plans = framed(&piece_plan, hole_low, hole_high);
                if plans.is_empty() {
                    plans.push(piece_plan);
                }
                let shapes: Vec<PartShape> = plans
                    .iter()
                    .map(|plan| PartShape::Hull(slab(&roof, plan.iter().map(|q| plane.s * q.x + plane.t * q.y))))
                    .collect();
                let mut piece_cell = cell;
                for (axis, edge_low, edge_high) in [(0, low.x, high.x), (1, low.y, high.y)] {
                    if min[axis] > edge_low + 1e-9 {
                        piece_cell.min[axis] = min[axis];
                    }
                    if max[axis] < edge_high - 1e-9 {
                        piece_cell.max[axis] = max[axis];
                    }
                }
                let mut piece = Part::new(
                    shapes,
                    part.material,
                    Role::House(HouseRole {
                        cells: vec![piece_cell],
                        ..role.clone()
                    }),
                );
                piece.paint = part.paint;
                piece.lie = part.lie;
                piece.element = u16::MAX;
                made.push((first + pieces.len()) as u16);
                pieces.push(piece);
            }
        }
        breakups.push(Breakup {
            needs: PartSet::from_indices([index]),
            pieces: made,
            alone: true,
        });
    }
    debug_assert!(first + pieces.len() <= MAX_PARTS);
    structure.pieces.extend(pieces);
    structure.breakups.extend(breakups);
}

/// A roof sheet longer than this (m) along the ridge breaks across its length too.
const LONG_SHEET: f64 = 2.6;

/// The structure of the cottage `design` lays out.
pub fn house_structure(design: &HouseDesign) -> Structure {
    Frame::new(design).into_structure()
}

/// The cottages laid out that something still holds, by design.
static COTTAGES: HeldLayouts<HouseDesign> = HeldLayouts::new();

/// [`house_structure`], shared with whatever still holds the same design's
/// ([`HeldLayouts`]): most of a village reset, and of each cottage's model, was laying
/// its cottages out again.
pub fn shared_house_structure(design: &HouseDesign) -> Arc<Structure> {
    COTTAGES.get_or_lay_out(*design, || house_structure(design))
}

/// Where a touching part lies from the one it supports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Relation {
    /// Under it: the part rests on it.
    Below,
    /// Over it: the part hangs from it.
    Above,
    Beside,
}

/// The share of `by`'s support passed to `of` touching it as `relation` says.
fn support_share(of: PartKind, by: PartKind, relation: Relation) -> f32 {
    use PartKind::*;
    match relation {
        // A floor strip's joists bear on the inner wall in passing: they span from
        // plate to plate, and do not stand on the partition alone like a table.
        Relation::Below if (of, by) == (Floor, Partition) => LOOSE_TIE,
        Relation::Below => REST,
        // A beam carries what lies on it; it does not hang from it.
        Relation::Above => match of {
            Panel | Window | Door | Partition | Gable => HANG,
            Floor => BOARD_TIE,
            _ => 0.0,
        },
        Relation::Beside => match (of, by) {
            (Post, _) => 0.0,
            (Floor, Plate) => JOIST_SEAT,
            // Joists sit on the plate; they do not hold it up.
            (Plate, Floor) => LOOSE_TIE,
            (Plate, Plate) => PLATE_TIE,
            (Plate, Post) => POST_TIE,
            // A gable's pieces stand on their plates: a third of a gable does not
            // cantilever off the middle one.
            (Gable, Gable) => LOOSE_TIE,
            (Floor, Floor) => BOARD_TIE,
            // A bay framed into a corner post is held by it, as by its plate.
            (Panel | Window | Door, Post) => POST_PANEL_TIE,
            (Panel | Window | Door | Partition, Panel | Window | Door | Plate | Post) => {
                PANEL_TIE
            }
            _ => LOOSE_TIE,
        },
    }
}

/// Columns of bays at most [`BAY`] wide filling `from`–`to`.
fn bays(columns: &mut Vec<(f64, f64, Column)>, from: f64, to: f64) {
    let span = to - from;
    if span <= 1e-6 {
        return;
    }
    let count = (span / BAY).ceil().max(1.0) as usize;
    for k in 0..count {
        columns.push((
            from + span * k as f64 / count as f64,
            from + span * (k + 1) as f64 / count as f64,
            Column::Bay,
        ));
    }
}

/// A convex polygon cut to the rectangle `min`–`max`.
fn clip_rect(polygon: &[DVec2], min: DVec2, max: DVec2) -> Vec<DVec2> {
    let mut points = polygon.to_vec();
    for (axis, bound, keep_above) in [
        (0, min.x, true),
        (0, max.x, false),
        (1, min.y, true),
        (1, max.y, false),
    ] {
        let inside = |p: DVec2| {
            if keep_above {
                p[axis] >= bound - 1e-9
            } else {
                p[axis] <= bound + 1e-9
            }
        };
        let mut next = Vec::with_capacity(points.len() + 2);
        for i in 0..points.len() {
            let (a, b) = (points[i], points[(i + 1) % points.len()]);
            if inside(a) {
                next.push(a);
            }
            if inside(a) != inside(b) {
                let k = (bound - a[axis]) / (b[axis] - a[axis]);
                next.push(a + (b - a) * k);
            }
        }
        points = next;
    }
    points
}

/// Where the chimney comes through the roof part `part`: the stack's plan with a margin,
/// in the part's plane (along, out down the slope).
fn chimney_hole(design: &HouseDesign, part: HousePart) -> (DVec2, DVec2) {
    let plane = part_plane(design, part);
    let flue = design.flue();
    let reach = CHIMNEY / 2.0 + HOLE_MARGIN;
    [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)]
        .map(|(sx, sz)| {
            let p = DVec3::new(flue.x + sx * reach, 0.0, flue.z + sz * reach);
            DVec2::new(p.dot(plane.s), p.dot(plane.t))
        })
        .into_iter()
        .fold((DVec2::INFINITY, DVec2::NEG_INFINITY), |(low, high), q| (low.min(q), high.max(q)))
}

/// The convex plans `plan` (in a roof part's plane) is cut into round the hole
/// `hole_low`–`hole_high`: the strips beside, under and over the hole where it reaches
/// into the plan, none where it does not.
fn framed(plan: &[DVec2], hole_low: DVec2, hole_high: DVec2) -> Vec<Vec<DVec2>> {
    let (low, high) = plan.iter().fold((DVec2::INFINITY, DVec2::NEG_INFINITY), |(low, high), q| {
        (low.min(*q), high.max(*q))
    });
    let (hole_low, hole_high) = (hole_low.max(low), hole_high.min(high));
    if (hole_high - hole_low).min_element() <= 1e-6 {
        return Vec::new();
    }
    [
        (low, DVec2::new(hole_low.x, high.y)),
        (DVec2::new(hole_high.x, low.y), high),
        (DVec2::new(hole_low.x, low.y), DVec2::new(hole_high.x, hole_low.y)),
        (DVec2::new(hole_low.x, hole_high.y), DVec2::new(hole_high.x, high.y)),
    ]
    .into_iter()
    .filter(|(min, max)| (*max - *min).min_element() > 1e-6)
    .map(|(min, max)| clip_rect(plan, min, max))
    .filter(|strip| strip.len() >= 3)
    .collect()
}

/// A roof sheet's hull from its plan corners (y ignored): the deck's top and its
/// rafters' undersides.
fn slab(roof: &Roof, plan: impl Iterator<Item = DVec3>) -> Vec<DVec3> {
    plan.flat_map(|p| {
        let top = roof.top(p.x, p.z);
        [
            DVec3::new(p.x, top, p.z),
            DVec3::new(p.x, top - roof.thick - SHEET_DEPTH, p.z),
        ]
    })
    .collect()
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::structure::FALL_BELOW;

    fn village() -> Vec<HouseDesign> {
        let mut designs = Vec::new();
        for s in [-1.0, 1.0] {
            for (z, h) in [(-39.0, 6.8), (-13.0, 4.6), (13.0, 4.3), (39.0, 5.0)] {
                designs.push(HouseDesign::of(s * 45.0, z, 5.0, 6.0, h));
            }
            for (z, h) in [(-46.0, 6.8), (46.0, 5.2)] {
                designs.push(HouseDesign::of(s * 17.0, z, 7.0, 5.0, h));
            }
            designs.push(HouseDesign::of(s * 24.0, 0.0, 5.0, 7.0, 4.9));
        }
        designs
    }

    #[test]
    fn every_village_cottage_stands_on_its_own_frame() {
        for design in village() {
            let structure = house_structure(&design);
            assert!(structure.parts.len() <= MAX_PARTS);
            assert!(structure.parts.len() > 60, "{}", structure.parts.len());
            let support = structure.stability(&|_| true);
            for (index, part) in structure.parts.iter().enumerate() {
                assert!(
                    support[index] >= FALL_BELOW,
                    "{:?} {index} {:?} at {:?}..{:?}: {}",
                    design.style.form,
                    part.house().kind,
                    part.min,
                    part.max,
                    support[index]
                );
            }
        }
    }

    #[test]
    fn a_lone_panel_shot_out_drops_nothing_but_the_window_on_it() {
        for design in village() {
            let structure = house_structure(&design);
            for (index, part) in structure.parts.iter().enumerate() {
                // A low panel a shell at hull height strikes (over a window's foot).
                if part.house().kind != PartKind::Panel
                    || part.min.y > PLINTH + WINDOW_FOOT + 0.01
                    || part.max.y < 1.0
                {
                    continue;
                }
                let gone = PartSet::from_indices([index]);
                let falling: Vec<usize> = structure
                    .unsupported(&gone)
                    .iter()
                    .filter(|&other| {
                        structure.parts[other].house().kind != PartKind::Window
                            || under(&structure, other) != [index]
                    })
                    .collect();
                assert!(falling.is_empty(), "{:?} {index}: {falling:?}", design.style.form);
            }
        }
    }

    /// Every village cottage with eave wall `side` (2 or 3) shot out below its plates
    /// (`plates`: or with them): the parts left falling.
    fn eave_gutted(design: &HouseDesign, side: usize, plates: bool) -> (Structure, PartSet) {
        let structure = house_structure(design);
        let gone = PartSet::from_indices((0..structure.parts.len()).filter(|&index| {
            let part = &structure.parts[index];
            part.house().wall == Some(side) && (plates || part.house().kind != PartKind::Plate)
        }));
        let falling = structure.unsupported(&gone);
        (structure, falling)
    }

    #[test]
    fn a_slope_whose_eave_wall_is_shot_out_comes_down_whole() {
        for design in village() {
            for side in [2, 3] {
                for plates in [true, false] {
                    let (structure, falling) = eave_gutted(&design, side, plates);
                    let slope = if side == 2 {
                        HousePart::RightRoof
                    } else {
                        HousePart::LeftRoof
                    };
                    for (index, part) in structure.parts.iter().enumerate() {
                        if part.house().slope.is_some_and(|(over, _)| over == slope) {
                            assert!(
                                falling.contains(index),
                                "{:?} side {side} plates {plates}: sheet {index} at {:?}..{:?} stays up",
                                design.style.form,
                                part.min,
                                part.max
                            );
                        }
                    }
                    // The corner posts still stand.
                    assert!((0..4).all(|post| !falling.contains(post)));
                }
            }
        }
    }

    /// The standing parts under `index` it rests on.
    fn under(structure: &Structure, index: usize) -> Vec<usize> {
        let part = &structure.parts[index];
        (0..structure.parts.len())
            .filter(|&other| {
                let below = &structure.parts[other];
                other != index
                    && below.house().wall == part.house().wall
                    && (below.max.y - part.min.y).abs() < CONTACT
                    && below.max.x.min(part.max.x) - below.min.x.max(part.min.x) > CONTACT
                    && below.max.z.min(part.max.z) - below.min.z.max(part.min.z) > CONTACT
            })
            .collect()
    }

    #[test]
    fn a_window_whose_sill_is_shot_out_drops_out_of_its_frame() {
        for design in village() {
            let structure = house_structure(&design);
            for (index, part) in structure.parts.iter().enumerate() {
                if part.house().kind != PartKind::Window {
                    continue;
                }
                let gone = PartSet::from_indices(under(&structure, index));
                assert!(!gone.is_empty(), "window {index} stands on something");
                let falling = structure.unsupported(&gone);
                assert!(falling.contains(index), "window {index} hangs over its hole");
            }
        }
    }

    #[test]
    fn a_plate_bridges_one_gap_but_never_cantilevers() {
        for design in village() {
            let structure = house_structure(&design);
            // A corner post and the lowest bays beside it shot out.
            for post in 0..4 {
                let corner = structure.parts[post].middle();
                let gone = PartSet::from_indices((0..structure.parts.len()).filter(|&index| {
                    let part = &structure.parts[index];
                    index == post
                        || index == POST_FEET + post
                        || (part.house().wall.is_some()
                            && part.min.y < GROUND_BLOCK
                            && (part.middle() - corner).with_y(0.0).length() < 2.2)
                }));
                let down = gone.union(&structure.unsupported(&gone));
                for (index, part) in structure.parts.iter().enumerate() {
                    if part.house().kind != PartKind::Plate || down.contains(index) {
                        continue;
                    }
                    let seated = under(&structure, index).iter().any(|&below| !down.contains(below));
                    let sides = structure.supports[index]
                        .iter()
                        .filter(|support| support.end != End::Any && !down.contains(support.by as usize))
                        .map(|support| support.end)
                        .collect::<Vec<_>>();
                    assert!(
                        seated || (sides.contains(&End::A) && sides.contains(&End::B)),
                        "{:?} post {post}: plate {index} at {:?}..{:?} cantilevers",
                        design.style.form,
                        part.min,
                        part.max
                    );
                }
            }
        }
    }

    #[test]
    fn a_lone_bay_of_an_upper_storey_breaks_off_at_the_floor() {
        let mut checked = 0;
        for design in village() {
            let structure = house_structure(&design);
            if design.storeys() < 2 {
                continue;
            }
            // Wall 0 shot out but for one column, and its posts with it.
            let column = structure
                .parts
                .iter()
                .position(|part| part.house().wall == Some(0) && part.house().upper_storey)
                .expect("an upper storey");
            let span = (structure.parts[column].min.x, structure.parts[column].max.x);
            let kept = |part: &Part| {
                part.house().wall == Some(0) && (part.min.x - span.0).abs() < 1e-6 && (part.max.x - span.1).abs() < 1e-6
            };
            let gone = PartSet::from_indices((0..structure.parts.len()).filter(|&index| {
                let part = &structure.parts[index];
                index < 4 || (part.house().wall == Some(0) && !kept(part))
            }));
            let falling = structure.unsupported(&gone);
            for (index, part) in structure.parts.iter().enumerate() {
                if kept(part) && part.house().upper_storey {
                    assert!(falling.contains(index), "upper part {index} of the lone bay stands");
                }
            }
            checked += 1;
        }
        assert!(checked > 0, "a two-storey cottage in the village");
    }

    #[test]
    fn a_gable_end_shot_out_round_its_window_leaves_the_roof_on_its_eave_walls() {
        let mut checked = 0;
        for design in village() {
            let structure = house_structure(&design);
            if structure.roof().unwrap().form != RoofForm::Gable {
                continue;
            }
            for wall in [0, 1] {
                // The gable's peak pieces and a window's column with the bays beside it.
                let Some(window) = (0..structure.parts.len()).find(|&index| {
                    structure.parts[index].house().kind == PartKind::Window
                        && structure.parts[index].house().wall == Some(wall)
                        && !structure.parts[index].house().upper_storey
                }) else {
                    continue;
                };
                let (from, to) = (structure.parts[window].min.x - 1.0, structure.parts[window].max.x + 1.0);
                let gone = PartSet::from_indices((0..structure.parts.len()).filter(|&index| {
                    let part = &structure.parts[index];
                    part.house().wall == Some(wall)
                        && (part.house().kind == PartKind::Gable
                            || (part.max.x > from && part.min.x < to && part.house().kind != PartKind::Plate))
                }));
                let falling = structure.unsupported(&gone);
                let sheets: Vec<usize> = falling
                    .iter()
                    .filter(|&index| structure.parts[index].house().kind == PartKind::Sheet)
                    .collect();
                assert!(sheets.is_empty(), "{:?} wall {wall}: sheets {sheets:?} fell", design.style.form);
                checked += 1;
            }
        }
        assert!(checked > 0);
    }

    #[test]
    fn a_lone_strip_of_wall_and_a_bare_post_topple_over_their_stubs() {
        for design in village() {
            let structure = house_structure(&design);
            // Wall 2 shot out but for one middle column, and its posts with it.
            let middle = (0..structure.parts.len())
                .filter(|&index| {
                    let part = &structure.parts[index];
                    part.house().wall == Some(2) && part.house().kind == PartKind::Panel && part.grounded
                })
                .min_by(|&a, &b| {
                    let z = |index: usize| structure.parts[index].middle().z.abs();
                    z(a).total_cmp(&z(b))
                })
                .expect("a middle bay");
            let span = (structure.parts[middle].min.z, structure.parts[middle].max.z);
            let kept = |part: &Part| {
                part.house().wall == Some(2) && (part.min.z - span.0).abs() < 1e-6 && (part.max.z - span.1).abs() < 1e-6
            };
            let gone = PartSet::from_indices((0..structure.parts.len()).filter(|&index| {
                let part = &structure.parts[index];
                [0, 2, POST_FEET, POST_FEET + 2].contains(&index) || (part.house().wall == Some(2) && !kept(part))
            }));
            let down = gone.union(&structure.unsupported(&gone));
            for (index, part) in structure.parts.iter().enumerate() {
                if kept(part) {
                    assert_eq!(down.contains(index), !part.grounded, "{:?} {index} {:?}", design.style.form, part.house().kind);
                }
            }
            // A post with only stubs of wall left on it goes too.
            let bare = PartSet::from_indices((0..structure.parts.len()).filter(|&index| {
                let part = &structure.parts[index];
                matches!(part.house().wall, Some(0) | Some(2)) && !part.grounded
            }));
            assert!(structure.unsupported(&bare).contains(0), "{:?}: the bare corner post stands", design.style.form);
        }
    }

    #[test]
    fn a_door_left_alone_in_its_wall_falls() {
        for design in village() {
            let structure = house_structure(&design);
            let door = (0..structure.parts.len())
                .find(|&index| structure.parts[index].house().kind == PartKind::Door)
                .expect("a door");
            let wall = structure.parts[door].house().wall;
            let gone = PartSet::from_indices(
                (0..structure.parts.len()).filter(|&index| index != door && structure.parts[index].house().wall == wall),
            );
            assert!(structure.unsupported(&gone).contains(door), "{:?}", design.style.form);
        }
    }

    #[test]
    fn an_upper_storey_stands_only_on_its_corner_and_floor() {
        let mut checked = 0;
        for design in village() {
            let structure = house_structure(&design);
            if design.storeys() < 2 {
                continue;
            }
            // A corner post shot out: the upper storey's wall nearest it comes down; the
            // ground floor's parts there stand on.
            let gone = PartSet::from_indices([0, POST_FEET]);
            let falling = structure.unsupported(&gone);
            let corner = structure.parts[0].middle().with_y(0.0);
            let near = |part: &Part| (part.middle().with_y(0.0) - corner).length() < 1.0;
            for (index, part) in structure.parts.iter().enumerate() {
                if part.house().wall.is_some() && part.house().upper_storey && near(part) {
                    assert!(falling.contains(index), "upper part {index} by the gone corner stands");
                }
                if part.house().wall.is_some() && part.grounded && near(part) {
                    assert!(!falling.contains(index), "ground floor part {index} fell");
                }
            }
            checked += 1;
        }
        assert!(checked > 0);
    }

    #[test]
    fn a_gable_end_with_its_roof_gone_stands_only_on_its_whole_wall() {
        for design in village() {
            let structure = house_structure(&design);
            if structure.roof().unwrap().form != RoofForm::Gable {
                continue;
            }
            let roof = PartSet::from_indices(
                (0..structure.parts.len()).filter(|&index| structure.parts[index].house().kind.roof()),
            );
            let parts = &structure.parts;
            let gables = |wall: usize| {
                (0..parts.len())
                    .filter(|&index| parts[index].house().kind == PartKind::Gable && parts[index].house().wall == Some(wall))
                    .collect::<Vec<_>>()
            };
            // Roof gone, walls whole: the gables stand.
            let falling = structure.unsupported(&roof);
            assert!(gables(0).iter().all(|&g| !falling.contains(g)), "{:?}", design.style.form);
            // And a bay of wall 0 shot out under them as well: wall 0's gable goes.
            let bay = (0..structure.parts.len())
                .find(|&index| structure.parts[index].house().wall == Some(0) && structure.parts[index].house().kind == PartKind::Panel)
                .unwrap();
            let gone = roof.union(&PartSet::from_indices([bay]));
            let falling = structure.unsupported(&gone);
            assert!(gables(0).iter().all(|&g| falling.contains(g)), "{:?}", design.style.form);
            assert!(gables(1).iter().all(|&g| !falling.contains(g)), "{:?}", design.style.form);
        }
    }

    #[test]
    fn bare_gable_ends_and_their_ridge_come_down_off_broken_walls() {
        for design in village() {
            let structure = house_structure(&design);
            if structure.roof().unwrap().form != RoofForm::Gable {
                continue;
            }
            // The roof sheets gone, both long walls with them and a bay out of each gable
            // wall: the gable ends stand on what is left of their walls, the ridge between.
            let bays: Vec<usize> = [0, 1]
                .into_iter()
                .filter_map(|wall| {
                    (0..structure.parts.len()).find(|&index| {
                        structure.parts[index].house().wall == Some(wall) && structure.parts[index].house().kind == PartKind::Panel
                    })
                })
                .collect();
            let gone = PartSet::from_indices((0..structure.parts.len()).filter(|&index| {
                let part = &structure.parts[index];
                part.house().kind == PartKind::Sheet || matches!(part.house().wall, Some(2) | Some(3)) || bays.contains(&index)
            }));
            let falling = structure.unsupported(&gone);
            for (index, part) in structure.parts.iter().enumerate() {
                if matches!(part.house().kind, PartKind::Ridge | PartKind::Gable) {
                    assert!(falling.contains(index), "{:?}: {:?} {index} stands", design.style.form, part.house().kind);
                }
            }
        }
    }

    #[test]
    fn a_slope_over_a_gone_eave_wall_and_gable_does_not_hang_from_the_chimney() {
        for design in village() {
            let structure = house_structure(&design);
            for (side, slope) in [(2, HousePart::RightRoof), (3, HousePart::LeftRoof)] {
                for gable in [0, 1] {
                    // The eave wall gone but for one column in its middle, and one end wall
                    // with its gable.
                    let middle = (0..structure.parts.len())
                        .filter(|&index| structure.parts[index].house().wall == Some(side))
                        .min_by(|&a, &b| {
                            let z = |index: usize| structure.parts[index].middle().z.abs();
                            z(a).total_cmp(&z(b))
                        })
                        .unwrap();
                    let column = (structure.parts[middle].min.z, structure.parts[middle].max.z);
                    let gone = PartSet::from_indices((0..structure.parts.len()).filter(|&index| {
                        let part = &structure.parts[index];
                        let in_column = (part.min.z - column.0).abs() < 1e-6 && (part.max.z - column.1).abs() < 1e-6;
                        (part.house().wall == Some(side) && !in_column) || part.house().wall == Some(gable)
                    }));
                    let down = gone.union(&structure.unsupported(&gone));
                    // The chimney carries nothing of the roof round it.
                    assert!(
                        structure
                            .supports
                            .iter()
                            .flatten()
                            .all(|support| support.by as usize != CHIMNEY_PART),
                        "the chimney holds a part up"
                    );
                    for (index, part) in structure.parts.iter().enumerate() {
                        if part.house().slope.is_some_and(|(over, _)| over == slope) {
                            assert!(
                                down.contains(index),
                                "{:?} side {side} gable {gable}: sheet {index} at {:?}..{:?} hangs",
                                design.style.form,
                                part.min,
                                part.max
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_chimney_comes_down_with_the_roof_it_stands_on_and_its_stack_stands() {
        for design in village() {
            let structure = house_structure(&design);
            let roof = PartSet::from_indices((0..structure.parts.len()).filter(|&index| {
                matches!(structure.parts[index].house().kind, PartKind::Sheet | PartKind::Ridge)
            }));
            assert!(structure.unsupported(&roof).contains(CHIMNEY_PART));
            let all_else = PartSet::from_indices((0..structure.parts.len()).filter(|&index| index != CHIMNEY_STACK));
            assert!(!structure.unsupported(&all_else).contains(CHIMNEY_STACK), "{:?}", design.style.form);
            // Clear of the roof's rafters over it: nothing holds it or rests on it.
            let (stack, roof) = (&structure.parts[CHIMNEY_STACK], design.roof());
            for (x, z) in [(stack.min.x, stack.min.z), (stack.max.x, stack.max.z), (stack.min.x, stack.max.z), (stack.max.x, stack.min.z)] {
                assert!(roof.top(x, z) - roof.thick - SHEET_DEPTH > stack.max.y);
            }
            assert!(structure.supports[CHIMNEY_STACK].is_empty());
            assert!(structure.supports.iter().flatten().all(|support| support.by as usize != CHIMNEY_STACK));
        }
    }

    #[test]
    fn the_roof_and_floors_are_framed_round_the_chimney_stack() {
        for design in village() {
            let structure = house_structure(&design);
            let stack = &structure.parts[CHIMNEY_STACK];
            let mut framed = 0;
            for part in structure.parts.iter().chain(&structure.pieces) {
                if !matches!(part.house().kind, PartKind::Sheet | PartKind::Floor) {
                    continue;
                }
                framed += usize::from(part.shapes.len() > 1);
                for shape in &part.shapes {
                    let (low, high) = shape.points().iter().fold(
                        (DVec3::INFINITY, DVec3::NEG_INFINITY),
                        |(low, high), p| (low.min(*p), high.max(*p)),
                    );
                    let apart = (stack.min - high).max(low - stack.max).with_y(f64::NEG_INFINITY);
                    assert!(apart.max_element() > 0.0, "{:?}: a {:?} reaches over the stack", design.style.form, part.house().kind);
                }
            }
            assert!(framed > 0, "{:?}: no sheet is framed round the chimney", design.style.form);
        }
    }

    #[test]
    fn a_wall_that_tips_with_its_corner_post_leaves_the_post_foot_and_stubs() {
        for design in village() {
            let structure = house_structure(&design);
            // Everything gone but the front wall (with no gable over it) and its posts.
            let kept = |index: usize| {
                let part = &structure.parts[index];
                [0, 1, POST_FEET, POST_FEET + 1].contains(&index)
                    || (part.house().wall == Some(0) && part.house().kind != PartKind::Gable)
            };
            let gone = PartSet::from_indices((0..structure.parts.len()).filter(|&index| !kept(index)));
            let down = structure.unsupported(&gone);
            assert!(down.contains(0) && down.contains(1), "{:?}: the posts stand", design.style.form);
            for index in (0..structure.parts.len()).filter(|&index| kept(index)) {
                let part = &structure.parts[index];
                let stub = part.grounded && part.max.y <= STUB_TOP;
                assert_eq!(down.contains(index), !stub, "{:?} {index} {:?}", design.style.form, part.house().kind);
            }
        }
    }

    #[test]
    fn a_gutted_eave_wall_lets_the_roof_over_it_down() {
        let design = HouseDesign::of(24.0, 0.0, 5.0, 7.0, 4.9);
        let structure = house_structure(&design);
        let side = 2;
        let gone = PartSet::from_indices(
            (0..structure.parts.len()).filter(|&index| structure.parts[index].house().wall == Some(side)),
        );
        let falling = structure.unsupported(&gone);
        assert!(
            falling
                .iter()
                .any(|index| structure.parts[index].house().kind == PartKind::Sheet),
            "{falling:?}"
        );
    }

    #[test]
    fn a_corner_post_left_with_a_bay_of_wall_tips_over() {
        for design in village() {
            let structure = house_structure(&design);
            let corner = structure.parts[0].middle().with_y(0.0);
            // Everything but the front-right post and the front wall's bay beside it.
            let kept = |index: usize| {
                let part = &structure.parts[index];
                index == 0
                    || index == POST_FEET
                    || (part.house().wall == Some(0)
                        && part.house().kind != PartKind::Gable
                        && (part.middle().with_y(0.0) - corner).length() < 1.4)
            };
            let gone = PartSet::from_indices((0..structure.parts.len()).filter(|&index| !kept(index)));
            let falls = structure.falls(&gone, &PartSet::EMPTY, DVec3::ZERO);
            let down = falls.iter().fold(PartSet::EMPTY, |down, fall| down.union(&fall.parts));
            assert!(down.contains(0), "{:?}: the post stands", design.style.form);
            assert!(falls.iter().any(|fall| fall.tip.is_some()), "it tips over");
        }
    }

    #[test]
    fn a_whole_cottage_stands_by_every_rule() {
        for design in village() {
            let structure = house_structure(&design);
            assert!(structure.unsupported(&PartSet::EMPTY).is_empty(), "{:?}", design.style.form);
        }
    }

}
