//! A cottage as a structure: the parts it is built of (wall panels a bay wide, split
//! round their windows and door, corner posts, the plate course at the wall tops, the
//! gables, the attic floor, an inner partition, the roof sheets, the ridge and the
//! chimney), what each rests on, hangs from or is tied to, and how a part's support
//! reaches the ground.
//!
//! Each standing part is a collider of its own on the cottage's fixed body, so a part
//! that breaks leaves a real gap for shells, sight and, low enough, tanks. A part's
//! support is the best path of supports to the ground, each link passing on a share of
//! it ([`HouseStructure::stability`]): resting on a part passes all of it, hanging from
//! one above or being tied to one beside passes less. A part whose support falls under
//! [`FALL_BELOW`] gives way, and the parts giving way together fall as one body. No
//! count of hits decides it: a cottage stands as long as its frame carries it.
//!
//! Everything is measured in the build frame of [`HouseDesign`] (ridge along z, front
//! +z, right +x, ground at y = 0). Each part also names the cells of the cottage's
//! model it is drawn from: rectangles in the plane of a [`HousePart`] of the model.

use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

use super::house_fracture::PartPlane;
use super::house_layout::{
    CHIMNEY, HouseDesign, HousePart, PLINTH, RAFTER_DEPTH, Roof, RoofForm, WALL_DEPTH, WallFinish,
    wall_frames, window_columns,
};

/// The most parts a cottage has (the width of [`PartSet`]).
pub const MAX_PARTS: usize = 256;

/// A set of a cottage's parts by index: the parts broken or fallen from a cottage, or
/// those falling as one body.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PartSet(pub [u64; MAX_PARTS / 64]);

impl PartSet {
    pub const EMPTY: Self = Self([0; MAX_PARTS / 64]);

    pub fn contains(&self, index: usize) -> bool {
        index < MAX_PARTS && self.0[index / 64] & (1 << (index % 64)) != 0
    }

    pub fn insert(&mut self, index: usize) {
        self.0[index / 64] |= 1 << (index % 64);
    }

    pub fn remove(&mut self, index: usize) {
        self.0[index / 64] &= !(1 << (index % 64));
    }

    pub fn len(&self) -> usize {
        self.0.iter().map(|word| word.count_ones() as usize).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|&word| word == 0)
    }

    /// The indices in the set, lowest first.
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        (0..MAX_PARTS).filter(|&index| self.contains(index))
    }

    pub fn union(&self, other: &Self) -> Self {
        let mut words = self.0;
        for (word, other) in words.iter_mut().zip(other.0) {
            *word |= other;
        }
        Self(words)
    }

    pub fn from_indices(indices: impl IntoIterator<Item = usize>) -> Self {
        let mut set = Self::EMPTY;
        for index in indices {
            set.insert(index);
        }
        set
    }
}

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
    /// Damage (in shell damage units) the part takes before it breaks, before a brick
    /// cottage's walls double up ([`BRICK_STRENGTH`]). A standard shell (40) breaks a
    /// clapboard panel it strikes; a corner post or the chimney takes more.
    pub fn strength(self) -> f64 {
        match self {
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
}

/// The chimney's index among a cottage's parts, after the four corner posts.
pub const CHIMNEY_PART: usize = 4;

/// A brick cottage's walls take this much more than clapboard ones.
const BRICK_STRENGTH: f64 = 1.6;

/// A part's support below which it gives way.
pub const FALL_BELOW: f32 = 0.33;
/// The share of its seat a part needs still standing under it to rest fully: a roof
/// sheet whose eave wall is shot out from under it except at a corner post comes down.
const SEAT_NEEDED: f32 = 0.5;

/// The share of its support a part passes to one resting on it, hanging from it or
/// tied beside it ([`support_share`]). Resting passes all of it; the rest are named
/// by what they model.
const REST: f32 = 1.0;
/// A panel, a window or the door held by the course over it.
const HANG: f32 = 0.55;
/// Neighbouring panels nailed to the same studs.
const PANEL_TIE: f32 = 0.45;
/// A plate spanning on from its neighbour: a header bridges one bay over a gap, not
/// two (0.55² falls under [`FALL_BELOW`]).
const PLATE_TIE: f32 = 0.55;
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
/// The ridge beam spanning on from its neighbour.
const RIDGE_TIE: f32 = 0.8;
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
/// Roof sheets: about this long along the ridge, and the share of a slope's run the
/// upper row takes.
const SHEET_LENGTH: f64 = 2.4;
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

/// One part of a cottage: its kind, its collider as convex hull points (a box's
/// eight corners when `cuboid`), its bounding box and the model cells drawn as it.
#[derive(Clone, Debug, PartialEq)]
pub struct StructurePart {
    pub kind: PartKind,
    pub points: Vec<DVec3>,
    /// Axis aligned in the build frame: the collider is the box `min`–`max`.
    pub cuboid: bool,
    pub min: DVec3,
    pub max: DVec3,
    pub cells: Vec<PartCell>,
    pub strength: f64,
    /// Stands on the plinth (or, the chimney, on its own footing).
    pub grounded: bool,
    /// The wall it is in (in [`wall_frames`] order), for wall parts.
    pub wall: Option<usize>,
    /// A roof sheet's slope, and whether it is in the row under the ridge.
    pub slope: Option<(HousePart, bool)>,
}

impl StructurePart {
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
            strength: kind.strength(),
            grounded: min.y <= PLINTH + GROUND_REACH,
            wall: None,
            slope: None,
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

    pub fn center(&self) -> DVec3 {
        (self.min + self.max) / 2.0
    }

    /// How far `p` (build frame) lies from the part's box.
    pub fn distance(&self, p: DVec3) -> f64 {
        (p.clamp(self.min, self.max) - p).length()
    }

    /// Whether the part blocks a tank on the ground: it reaches down to hull height.
    pub fn blocks_ground(&self) -> bool {
        self.min.y < GROUND_BLOCK && self.kind != PartKind::Floor
    }
}

/// Parts reaching below this height block tanks on the ground.
pub const GROUND_BLOCK: f64 = 1.2;

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

/// A cottage's parts and how they hold each other up.
#[derive(Clone, Debug, PartialEq)]
pub struct HouseStructure {
    pub design: HouseDesign,
    pub roof: Roof,
    pub parts: Vec<StructurePart>,
    /// For each part, the parts that hold it up, lowest index first.
    pub supports: Vec<Vec<Support>>,
}

/// A part holding another up: `by` passes on `share` of its support. A part it rests
/// on also seats it over `seat` (an area or a length): a part keeps all its support
/// only while what still stands under it seats at least [`SEAT_NEEDED`] of what did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Support {
    pub by: u16,
    pub share: f32,
    pub seat: f32,
}

/// A wall column: bays between the corner posts and the openings.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Column {
    Bay,
    Window,
    Door,
}

impl HouseStructure {
    pub fn new(design: &HouseDesign) -> Self {
        let roof = design.roof();
        let mut structure = Self {
            design: *design,
            roof,
            parts: Vec::new(),
            supports: Vec::new(),
        };
        structure.build_posts();
        structure.build_chimney();
        debug_assert_eq!(structure.parts.len() - 1, CHIMNEY_PART);
        for index in 0..4 {
            structure.build_wall(index);
        }
        structure.build_floors();
        structure.build_partition();
        let walls = structure.parts.len();
        structure.supports = vec![Vec::new(); walls];
        structure.link_by_contact();
        structure.build_roof();
        structure.supports.resize(structure.parts.len(), Vec::new());
        structure.link_roof(walls);
        if design.style.walls == WallFinish::Brick {
            for part in &mut structure.parts {
                if part.kind.outer_wall() {
                    part.strength *= BRICK_STRENGTH;
                }
            }
        }
        for supports in &mut structure.supports {
            // The best share each supporter passes, once.
            supports.sort_by(|a, b| a.by.cmp(&b.by).then(b.share.total_cmp(&a.share)));
            supports.dedup_by_key(|support| support.by);
        }
        debug_assert!(structure.parts.len() <= MAX_PARTS);
        structure
    }

    /// The chimney above the deck; its stack stands on its own footing.
    fn build_chimney(&mut self) {
        let flue = self.design.flue();
        let (bottom, top) = self.design.chimney_span();
        let reach = DVec3::new(CHIMNEY / 2.0, 0.0, CHIMNEY / 2.0);
        let mut chimney = StructurePart::cuboid(
            PartKind::Chimney,
            DVec3::new(flue.x, bottom, flue.z) - reach,
            DVec3::new(flue.x, top, flue.z) + reach,
            vec![PartCell {
                part: HousePart::Chimney,
                min: DVec2::splat(-FAR),
                max: DVec2::splat(FAR),
            }],
        );
        chimney.grounded = true;
        self.parts.push(chimney);
    }

    /// The four corner posts: front right, front left, back right, back left.
    fn build_posts(&mut self) {
        let (hw, hd, top) = (self.design.w / 2.0, self.design.d / 2.0, self.roof.wall);
        let frames = wall_frames(self.design.w, self.design.d);
        for (sx, sz) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
            let a = DVec3::new(sx * hw, PLINTH, sz * hd);
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
                    PartCell {
                        part: HousePart::wall(index),
                        min: DVec2::new(min_s, -FAR),
                        max: DVec2::new(max_s, if gable { self.roof.wall } else { FAR }),
                    }
                })
                .collect();
            let mut post = StructurePart::cuboid(PartKind::Post, a.min(b), a.max(b), cells);
            post.grounded = true;
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
            Column::Window => vec![
                (PLINTH, sill, PartKind::Panel),
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
        for (from, to, column) in self.columns(index) {
            let bands = self.bands(column);
            let last_band = bands.len() - 1;
            for (b, &(bottom, top, kind)) in bands.iter().enumerate() {
                let corner =
                    |s: f64, t: f64, depth: f64| center + along * s + DVec3::Y * t - out * depth;
                let (p, q) = (corner(from, bottom, 0.0), corner(to, top, WALL_DEPTH));
                // The model's cell: the bottom band reaches on down past the plinth
                // and the top one up into the eaves (on a gable wall, up to the gable).
                let cell = PartCell {
                    part,
                    min: DVec2::new(from, if b == 0 { -FAR } else { bottom }),
                    max: DVec2::new(to, if b < last_band || gable { top } else { FAR }),
                };
                let mut part = StructurePart::cuboid(kind, p.min(q), p.max(q), vec![cell]);
                part.wall = Some(index);
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
            let mut part = StructurePart::new(PartKind::Gable, points, false, vec![cell]);
            part.wall = Some(index);
            self.parts.push(part);
        }
    }

    /// The attic floor under the wall tops (and a two-storey house's upper floor over
    /// the ground floor's plates), in strips across the cottage.
    fn build_floors(&mut self) {
        let inside = DVec2::new(
            self.design.w / 2.0 - WALL_DEPTH,
            self.design.d / 2.0 - WALL_DEPTH,
        );
        let mut levels = vec![self.roof.wall];
        if self.design.storeys() == 2 {
            let head = PLINTH + SILL + (self.roof.wall - PLINTH - 1.25).min(1.3);
            levels.push(head + BELT_OVER);
        }
        let strips = ((2.0 * inside.y) / FLOOR_STRIP).round().max(1.0) as usize;
        for level in levels {
            for k in 0..strips {
                let z0 = -inside.y + 2.0 * inside.y * k as f64 / strips as f64;
                let z1 = -inside.y + 2.0 * inside.y * (k + 1) as f64 / strips as f64;
                self.parts.push(StructurePart::cuboid(
                    PartKind::Floor,
                    DVec3::new(-inside.x, level - FLOOR_DEPTH, z0),
                    DVec3::new(inside.x, level, z1),
                    Vec::new(),
                ));
            }
        }
    }

    /// The wall across the rooms of the ground floor, with a doorway in the middle,
    /// up to the floor over it.
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
                self.parts.push(StructurePart::cuboid(
                    PartKind::Partition,
                    DVec3::new(x0.min(x1), PLINTH, z - PARTITION_THICKNESS / 2.0),
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
                let share = support_share(a.kind, b.kind, relation);
                // What seats it: the plan it lies on, or the length a floor's
                // joists sit along a plate.
                let seat = match relation {
                    Relation::Below => plan_overlap(0) * plan_overlap(2),
                    Relation::Beside if (a.kind, b.kind) == (PartKind::Floor, PartKind::Plate) => plan_overlap(0).max(plan_overlap(2)),
                    _ => 0.0,
                };
                if share > 0.0 {
                    self.supports[i].push(Support {
                        by: j as u16,
                        share,
                        seat: seat.max(0.0) as f32,
                    });
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
                    let points = slab(
                        &roof,
                        plan.iter().map(|q| DVec3::new(side * q.y, 0.0, q.x)),
                    );
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
                    let mut sheet = StructurePart::new(PartKind::Sheet, points, false, vec![cell]);
                    sheet.slope = Some((part, upper));
                    self.parts.push(sheet);
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
                    let points = slab(
                        &roof,
                        plan.iter().map(|q| DVec3::new(q.x, 0.0, end * q.y)),
                    );
                    let cell = PartCell {
                        part,
                        min: DVec2::new(if half < 0.0 { -FAR } else { 0.0 }, -FAR),
                        max: DVec2::new(if half < 0.0 { 0.0 } else { FAR }, FAR),
                    };
                    let mut sheet = StructurePart::new(PartKind::Sheet, points, false, vec![cell]);
                    sheet.slope = Some((part, false));
                    self.parts.push(sheet);
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
            self.parts.push(StructurePart::cuboid(
                PartKind::Ridge,
                DVec3::new(-RIDGE_HALF, roof.ridge - RIDGE_DEPTH, z0),
                DVec3::new(RIDGE_HALF, roof.ridge + roof.thick * 0.5, z1),
                Vec::new(),
            ));
        }
    }

    /// Supports of the roof (sheets, ridge, chimney: parts from `first` on): the
    /// rafters' feet on the eave walls' plates, their tops at the ridge, the ridge on
    /// the gables or the hips, and the decking between sheets.
    fn link_roof(&mut self, first: usize) {
        let roof = self.roof;
        let (hw, hd) = (self.design.w / 2.0, self.design.d / 2.0);
        let count = self.parts.len();
        let overlap = |a: (f64, f64), b: (f64, f64)| a.1.min(b.1) - a.0.max(b.0);
        let touching = |a: &StructurePart, b: &StructurePart| {
            (b.min - a.max).max(a.min - b.max).max_element() <= CONTACT
        };
        let z = |p: &StructurePart| (p.min.z, p.max.z);
        let x = |p: &StructurePart| (p.min.x, p.max.x);
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
                            // Rafters' tops on the ridge.
                            (PartKind::Ridge, Some(_)) if upper => {
                                let seat = overlap(z(a), z(b));
                                if seat > CONTACT {
                                    links.push((i, j, REST, seat));
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
                                let across = match (slope, b.slope) {
                                    (HousePart::RightRoof, Some((HousePart::LeftRoof, _)))
                                    | (HousePart::LeftRoof, Some((HousePart::RightRoof, _))) => true,
                                    _ => false,
                                };
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
                            (PartKind::Chimney, _) if touching(a, b) => {
                                links.push((i, j, LOOSE_TIE, 0.0));
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
                            PartKind::Ridge => links.push((i, j, RIDGE_TIE, 0.0)),
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
            });
        }
    }

    /// Each part's support: 1 for a grounded standing part, otherwise the best share
    /// of support any path of standing parts passes to it from the ground (0 for a
    /// part not standing). What a part rests on passes less once less than
    /// [`SEAT_NEEDED`] of its seat still stands.
    pub fn stability(&self, standing: &dyn Fn(usize) -> bool) -> Vec<f32> {
        let count = self.parts.len();
        let mut support = vec![0.0f32; count];
        // Dijkstra on the best product of shares: every share is at most 1, so a
        // part's best path is settled once it is the best open part.
        let mut settled = vec![false; count];
        for (index, part) in self.parts.iter().enumerate() {
            if part.grounded && standing(index) {
                support[index] = 1.0;
            }
        }
        // The parts each part holds and the share it passes them, so a settled part
        // offers them its support.
        let mut holds: Vec<Vec<(u16, f32)>> = vec![Vec::new(); count];
        for (index, supports) in self.supports.iter().enumerate() {
            let (seated, seat) = supports
                .iter()
                .filter(|support| support.seat > 0.0)
                .fold((0.0f32, 0.0f32), |(seated, seat), support| {
                    let left = if standing(support.by as usize) {
                        support.seat
                    } else {
                        0.0
                    };
                    (seated + left, seat + support.seat)
                });
            let seating = if seat > 0.0 {
                (seated / seat / SEAT_NEEDED).min(1.0)
            } else {
                1.0
            };
            for support in supports {
                let share = if support.seat > 0.0 {
                    support.share * seating
                } else {
                    support.share
                };
                holds[support.by as usize].push((index as u16, share));
            }
        }
        loop {
            // The best open part (the lowest index on a tie), found by a linear scan:
            // a cottage has under two hundred parts.
            let mut best = None;
            let mut value = 0.0f32;
            for index in 0..count {
                if !settled[index] && support[index] > value {
                    value = support[index];
                    best = Some(index);
                }
            }
            let Some(index) = best else {
                break;
            };
            settled[index] = true;
            for &(held, share) in &holds[index] {
                let held = held as usize;
                if !settled[held] && standing(held) {
                    let offered = value * share;
                    if offered > support[held] {
                        support[held] = offered;
                    }
                }
            }
        }
        support
    }

    /// The standing parts (not in `gone`) the frame no longer holds up.
    pub fn unsupported(&self, gone: &PartSet) -> PartSet {
        let stability = self.stability(&|index| !gone.contains(index));
        PartSet::from_indices(
            (0..self.parts.len())
                .filter(|&index| !gone.contains(index) && stability[index] < FALL_BELOW),
        )
    }

    /// `falling` in the groups that come down together: parts linked to each other
    /// by any support, each group in order of its lowest part.
    pub fn clusters(&self, falling: &PartSet) -> Vec<PartSet> {
        let count = self.parts.len();
        let mut neighbours: Vec<Vec<usize>> = vec![Vec::new(); count];
        for (index, supports) in self.supports.iter().enumerate() {
            for support in supports {
                neighbours[index].push(support.by as usize);
                neighbours[support.by as usize].push(index);
            }
        }
        let mut seen = PartSet::EMPTY;
        let mut clusters = Vec::new();
        for start in falling.iter() {
            if seen.contains(start) {
                continue;
            }
            let mut cluster = PartSet::EMPTY;
            let mut open = vec![start];
            seen.insert(start);
            while let Some(index) = open.pop() {
                cluster.insert(index);
                for &next in &neighbours[index] {
                    if falling.contains(next) && !seen.contains(next) {
                        seen.insert(next);
                        open.push(next);
                    }
                }
            }
            clusters.push(cluster);
        }
        clusters
    }

    /// Every part, as a set.
    pub fn all(&self) -> PartSet {
        PartSet::from_indices(0..self.parts.len())
    }
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
        Relation::Below => REST,
        Relation::Above => match of {
            Panel | Window | Door | Partition | Gable | Plate => HANG,
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
            (Gable, Gable) | (Floor, Floor) => BOARD_TIE,
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
            let structure = HouseStructure::new(&design);
            assert!(structure.parts.len() <= MAX_PARTS);
            assert!(structure.parts.len() > 60, "{}", structure.parts.len());
            let support = structure.stability(&|_| true);
            for (index, part) in structure.parts.iter().enumerate() {
                assert!(
                    support[index] >= FALL_BELOW,
                    "{:?} {index} {:?} at {:?}..{:?}: {}",
                    design.style.form,
                    part.kind,
                    part.min,
                    part.max,
                    support[index]
                );
            }
        }
    }

    #[test]
    fn a_lone_panel_shot_out_drops_nothing_else() {
        for design in village() {
            let structure = HouseStructure::new(&design);
            for (index, part) in structure.parts.iter().enumerate() {
                if part.kind != PartKind::Panel || part.min.y > PLINTH + 0.1 {
                    continue;
                }
                let gone = PartSet::from_indices([index]);
                let falling: Vec<usize> = structure.unsupported(&gone).iter().collect();
                assert!(falling.is_empty(), "{:?} {index}: {falling:?}", design.style.form);
            }
        }
    }

    #[test]
    fn a_gutted_eave_wall_lets_the_roof_over_it_down() {
        let design = HouseDesign::of(24.0, 0.0, 5.0, 7.0, 4.9);
        let structure = HouseStructure::new(&design);
        let side = 2;
        let gone = PartSet::from_indices(
            (0..structure.parts.len()).filter(|&index| structure.parts[index].wall == Some(side)),
        );
        let falling = structure.unsupported(&gone);
        assert!(
            falling
                .iter()
                .any(|index| structure.parts[index].kind == PartKind::Sheet),
            "{falling:?}"
        );
    }
}
