//! The structure engine: every destructible object built of parts (cottages,
//! watchtowers, timber walls and cargo crates) as one kind of thing.
//!
//! A [`Structure`] is a set of parts, each a shape of a material that sets its strength,
//! weight, what it sounds like and how it breaks, and the joints between them: which
//! parts hold which up ([`Support`]), which are fastened together ([`Structure::joints`])
//! and which brace a part that stands only braced ([`Brace`]). What stands is decided by
//! two standing rules, either or both of which a structure uses:
//!
//! - support shares ([`Structure::stability`]): a part's support is the best path of
//!   supports to the ground, each link passing on a share of it (resting on a part passes
//!   all of it, hanging from one or being tied to one passes less); a part whose support
//!   falls under [`FALL_BELOW`] gives way;
//! - tipping ([`tipping`]): each group of standing parts fastened together stands while
//!   its weight lies far enough inside the ground it bears on, unless it is light enough
//!   for the anchors it stands in ([`Part::anchor`]) to hold it upright.
//!
//! No count of hits decides it, and no random draw. Each object kind supplies its parts,
//! materials and joints as data from a builder (`house_structure`, `tower_structure`,
//! `timber_structure`, `crate_structure`); `structure_physics` plays any structure.
//!
//! Parts are measured in the structure's build frame: its ground centre at the origin, y
//! up, turned [`Structure::yaw`] about y from the world's axes.

use std::sync::Arc;

use glam::{DMat3, DQuat, DVec2, DVec3};
use serde::{Deserialize, Serialize};

use super::crate_structure::{CratePart, crate_structure};
use super::debris_physics::DebrisMaterial;
use super::house_layout::{HouseDesign, Roof};
use super::house_structure::{HouseRole, house_structure};
use super::kinds::CoverKind;
use super::timber_layout::TimberJoin;
use super::timber_structure::{TimberRole, timber_structure};
use super::tower_structure::{PieceLook, TowerPartKind, tower_structure};

/// The most parts and landing pieces a structure has (the width of [`PartSet`]).
pub const MAX_PARTS: usize = 256;

/// A set of a structure's parts by index: the parts broken or fallen from it, or those
/// falling as one body.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PartSet(pub [u64; MAX_PARTS / 64]);

/// A part set reads and writes as its indices, lowest first, as the scene codec's JSON
/// form has it.
impl Serialize for PartSet {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter())
    }
}

impl<'de> Deserialize<'de> for PartSet {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let indices = Vec::<usize>::deserialize(deserializer)?;
        if indices.iter().any(|&index| index >= MAX_PARTS) {
            return Err(serde::de::Error::custom("part index out of range"));
        }
        Ok(Self::from_indices(indices))
    }
}

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
        self.0.iter().enumerate().flat_map(|(word, &bits)| {
            (0..64)
                .filter(move |bit| bits & (1 << bit) != 0)
                .map(move |bit| word * 64 + bit)
        })
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

/// A piece of a part's collider in the build frame.
#[derive(Clone, Debug, PartialEq)]
pub enum PartShape {
    Box {
        center: DVec3,
        half: DVec3,
        rotation: DQuat,
    },
    /// A convex hull of points.
    Hull(Vec<DVec3>),
}

impl PartShape {
    /// The box `min`–`max`, square to the build frame.
    pub fn aligned(min: DVec3, max: DVec3) -> Self {
        Self::Box {
            center: (min + max) / 2.0,
            half: (max - min) / 2.0,
            rotation: DQuat::IDENTITY,
        }
    }

    /// A member's box from `from` to `to`: `width` across it in the plane whose normal
    /// is `normal`, `depth` along that normal.
    pub fn member(from: DVec3, to: DVec3, width: f64, depth: f64, normal: DVec3) -> Self {
        let along = to - from;
        let x = along.normalize();
        let z = (normal - x * normal.dot(x)).normalize();
        let y = z.cross(x);
        Self::Box {
            center: (from + to) / 2.0,
            half: DVec3::new(along.length(), width, depth) / 2.0,
            rotation: DQuat::from_mat3(&DMat3::from_cols(x, y, z)),
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

/// How a material breaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Breaking {
    /// Into splinters where it breaks; whole when it comes loose.
    Splinters,
    /// Glass in its frame: it does not come out whole, it shatters as it loses its seat.
    Glass,
    /// Mortared brick: a fallen body of it bursts into bricks on its first knock.
    Bricks,
    /// Concrete with its steel shoes, which never breaks: part of the ground.
    Never,
}

/// What a part is made of: how much damage (in shell damage units) it takes before it
/// breaks, how heavy it is per cubic metre of its shapes, how it rings and slides as
/// debris and how it breaks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    pub strength: f64,
    pub density: f64,
    pub surface: DebrisMaterial,
    pub breaking: Breaking,
}

/// A part holding another up: `by` passes on `share` of its support. A part it rests on
/// also seats it over `seat` (an area or a length): a part keeps all its support only
/// while what still stands under it, firmly held itself ([`FIRM`]), seats at least its
/// [`Part::seat_needed`] share of what did. `end` says which end of a span it holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Support {
    pub by: u16,
    pub share: f32,
    pub seat: f32,
    pub end: End,
}

/// Which end of a part spanning between two supports a support holds; a span holds by the
/// weaker of its two ends, or by any other support (a roof sheet's rafters from the ridge
/// to the eave plate, a plate from the part on one side to the one on the other, a beam
/// from post to post).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Any,
    A,
    B,
}

/// An end's place in a part's offers.
pub fn end_rank(end: End) -> usize {
    match end {
        End::Any => 0,
        End::A => 1,
        End::B => 2,
    }
}

/// What a part needs standing beside it however well it is seated: one of `any`, every
/// one of `all` (when there are any), or one of `via` braced in turn by one of the parts
/// listed with it. A one-bay strip of wall with nothing beside it, a bare post or a gable
/// end braced by nothing are slivers that topple; a pier between two openings stands
/// braced by the plate over it while that plate is braced by its neighbours.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Brace {
    pub any: Vec<u16>,
    pub all: Vec<u16>,
    pub via: Vec<(u16, Vec<u16>)>,
}

impl Brace {
    pub fn any(parts: impl IntoIterator<Item = usize>) -> Self {
        Self {
            any: parts.into_iter().map(|part| part as u16).collect(),
            ..Self::default()
        }
    }

    fn holds(&self, standing: &dyn Fn(usize) -> bool) -> bool {
        let up = |part: &u16| standing(*part as usize);
        self.any.iter().any(up)
            || (!self.all.is_empty() && self.all.iter().all(up))
            || self
                .via
                .iter()
                .any(|(part, braces)| up(part) && braces.iter().any(up))
    }
}

/// How a loose piece lies, from its biggest part, in the build frame: a plate's face
/// normal, a roof's face (it lies shingles up) or a stick's length. Nothing rests on a
/// thin edge: a piece come to rest on one is laid down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lie {
    /// It lies however it lands (a lump of brick, a crumpled roof).
    Any,
    Face(DVec3),
    Roof(DVec3),
    Length(DVec3),
}

/// What a body that lands hard breaks into besides its parts: when it holds all of
/// `needs`, those parts land as `pieces` (indices from [`Structure::parts`]' length on).
#[derive(Clone, Debug, PartialEq)]
pub struct Breakup {
    pub needs: PartSet,
    pub pieces: Vec<u16>,
}

/// One part of a structure, or one landing piece.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub shapes: Vec<PartShape>,
    /// Its bounding box.
    pub min: DVec3,
    pub max: DVec3,
    /// Its weight and where it acts.
    pub mass: f64,
    pub mass_center: DVec3,
    pub material: Material,
    /// Damage it takes before it breaks; infinite for a part that never breaks.
    pub strength: f64,
    /// Its splinters' and dust's paint; none for the cover's own colour.
    pub paint: Option<u32>,
    /// Stands on the ground: the share rule holds it whole.
    pub grounded: bool,
    /// Where it bears on the ground, for the tipping rule (none off the ground).
    pub contacts: Vec<DVec2>,
    /// Held in a shoe or planted: a light frame standing in one stands whatever its
    /// balance ([`Part::load`]).
    pub anchor: bool,
    /// A heavy load (a deck) its anchors do not hold off balance.
    pub load: bool,
    /// Part of the ground (a concrete footing): no group of parts includes it.
    pub footing: bool,
    /// Carries nothing: it tears off a frame that tips and goes over on its own.
    pub detaches: bool,
    /// Stands on its edge: coming loose, it tips out over its foot.
    pub upright: bool,
    /// Blocks a tank on the ground.
    pub blocks_ground: bool,
    /// The share of its seat it needs still standing under it to rest fully.
    pub seat_needed: f32,
    /// A beam spanning a gap holds by its two ends together, its hold falling off with
    /// the whole span as if each end were this tie further away, not by the weaker end
    /// alone.
    pub beam: Option<f32>,
    pub braces: Vec<Brace>,
    pub lie: Lie,
    /// The element it belongs to (a wall, a roof slope, a face's bracing): a body landing
    /// hard breaks apart at its elements first.
    pub element: u16,
    /// What the kind's model draws it as.
    pub role: Role,
}

impl Part {
    pub fn new(shapes: Vec<PartShape>, material: Material, role: Role) -> Self {
        let (min, max) = shapes.iter().flat_map(PartShape::points).fold(
            (DVec3::INFINITY, DVec3::NEG_INFINITY),
            |(min, max), p| (min.min(p), max.max(p)),
        );
        let mass = shapes.iter().map(shape_volume).sum::<f64>() * material.density;
        Self {
            shapes,
            min,
            max,
            mass,
            mass_center: (min + max) / 2.0,
            material,
            strength: material.strength,
            paint: None,
            grounded: false,
            contacts: Vec::new(),
            anchor: false,
            load: false,
            footing: false,
            detaches: false,
            upright: false,
            blocks_ground: min.y < GROUND_BLOCK,
            seat_needed: SEAT_NEEDED,
            beam: None,
            braces: Vec::new(),
            lie: Lie::Any,
            element: 0,
            role,
        }
    }

    /// The middle of its bounding box.
    pub fn middle(&self) -> DVec3 {
        (self.min + self.max) / 2.0
    }

    /// How far `p` (build frame) lies from the part's box.
    pub fn distance(&self, p: DVec3) -> f64 {
        (p.clamp(self.min, self.max) - p).length()
    }

    /// Its footprint's corners, as contacts with the ground.
    pub fn base(&self) -> Vec<DVec2> {
        vec![
            DVec2::new(self.min.x, self.min.z),
            DVec2::new(self.max.x, self.min.z),
            DVec2::new(self.max.x, self.max.z),
            DVec2::new(self.min.x, self.max.z),
        ]
    }

    /// A cottage part's role.
    pub fn house(&self) -> &HouseRole {
        match &self.role {
            Role::House(role) => role,
            _ => panic!("not a cottage's part"),
        }
    }

    /// A watchtower part's kind.
    pub fn tower_kind(&self) -> TowerPartKind {
        match self.role {
            Role::Tower { kind, .. } => kind,
            _ => panic!("not a watchtower's part"),
        }
    }
}

/// The volume (m³) of a shape.
fn shape_volume(shape: &PartShape) -> f64 {
    match shape {
        PartShape::Box { half, .. } => 8.0 * half.x * half.y * half.z,
        PartShape::Hull(points) => {
            let points: Vec<_> = points
                .iter()
                .map(|p| rapier3d::prelude::Vector::new(p.x as f32, p.y as f32, p.z as f32))
                .collect();
            rapier3d::prelude::SharedShape::convex_hull(&points)
                .map_or(0.0, |hull| f64::from(hull.mass_properties(1.0).mass()))
        }
    }
}

/// What a kind's model draws a part as.
#[derive(Clone, Debug, PartialEq)]
pub enum Role {
    House(HouseRole),
    Tower {
        kind: TowerPartKind,
        /// A leg's corner.
        corner: Option<usize>,
        /// How a landing piece is drawn.
        look: Option<PieceLook>,
    },
    Timber(TimberRole),
    Crate(CratePart),
}

/// Parts reaching below this height block tanks on the ground.
pub const GROUND_BLOCK: f64 = 1.2;
/// A part's support below which it gives way.
pub const FALL_BELOW: f32 = 0.33;
/// A part's support from which it seats what rests on it.
pub const FIRM: f32 = 0.5;
/// The share of its seat a part needs still standing under it to rest fully.
pub const SEAT_NEEDED: f32 = 0.5;

/// A structure's tipping rule: a group stands while its weight lies at least `margin`
/// plus `rise` times its height inside the ground it bears on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tipping {
    pub margin: f64,
    pub rise: f64,
}

/// When a structure counts as levelled (destroyed, for the score and the levels that
/// rebuild it): once every one of `keys` is gone, or less than `share` of its parts
/// stands. `announce` sends its destruction, raising `dust` (m), as it is levelled;
/// otherwise its load crashing down does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Levelling {
    pub keys: PartSet,
    pub share: f64,
    pub announce: bool,
    pub dust: f64,
}

/// How a structure's fallen parts land and clear: a falling body breaks up when its
/// velocity changes by `landing_speed` (m/s) in a tick, only on the ground when
/// `ground_only` (a frame caught on cover stays whole until it comes down); a loose piece
/// lies `life` seconds (plus up to `life_spread` by its parts) before it clears, and is
/// gone by `deadline`. Standing parts' colliders reach `seam` metres past their faces,
/// so a shell along the seam between two parts meets one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DebrisRules {
    pub landing_speed: f64,
    pub ground_only: bool,
    pub life: f64,
    pub life_spread: f64,
    pub deadline: f64,
    pub seam: f64,
}

/// The kind a structure is, with what its builder keeps for its model.
#[derive(Clone, Debug, PartialEq)]
pub enum Build {
    House { design: HouseDesign, roof: Roof },
    Tower,
    Timber,
    Crate,
}

/// A structure's parts and how they hold each other up.
#[derive(Clone, Debug, PartialEq)]
pub struct Structure {
    pub build: Build,
    /// The build frame's turn from the world's axes, about y.
    pub yaw: f64,
    pub parts: Vec<Part>,
    /// What a landing breaks the parts into, indexed from `parts.len()` on.
    pub pieces: Vec<Part>,
    pub breakups: Vec<Breakup>,
    /// For each part, the parts that hold it up, lowest index first (the share rule).
    pub supports: Vec<Vec<Support>>,
    /// For each part, the parts it is fastened to: they fall together, and a group of
    /// them stands or tips as one.
    pub joints: Vec<Vec<u16>>,
    /// Whether the share rule decides what stands.
    pub shares: bool,
    pub tipping: Option<Tipping>,
    pub levelling: Levelling,
    pub debris: DebrisRules,
    /// Varies how a landed body breaks apart, from the design.
    pub variant: u32,
}

/// A body of a structure's parts coming down together: its parts, and for a group that
/// tips over, the point on the ground (build frame) it turns about and the way it falls
/// (build frame x and z, unit length).
#[derive(Clone, Debug, PartialEq)]
pub struct Fall {
    pub parts: PartSet,
    pub tip: Option<(DVec3, DVec2)>,
}

/// The structure a cover of `kind` stands for, if it is built of parts.
pub fn structure_of(
    kind: CoverKind,
    x: f64,
    z: f64,
    w: f64,
    d: f64,
    h: f64,
    join: Option<TimberJoin>,
) -> Option<Arc<Structure>> {
    match kind {
        CoverKind::House => Some(Arc::new(house_structure(&HouseDesign::of(x, z, w, d, h)))),
        CoverKind::Tower => Some(tower_structure()),
        CoverKind::Timber => Some(timber_structure(w, d, h, join)),
        CoverKind::Cargo => Some(crate_structure(w, d, h)),
        _ => None,
    }
}

impl Structure {
    /// A structure of `parts` with no joints yet, standing by the share rule alone.
    pub fn new(build: Build, yaw: f64, parts: Vec<Part>, debris: DebrisRules) -> Self {
        let count = parts.len();
        Self {
            build,
            yaw,
            parts,
            pieces: Vec::new(),
            breakups: Vec::new(),
            supports: vec![Vec::new(); count],
            joints: vec![Vec::new(); count],
            shares: true,
            tipping: None,
            levelling: Levelling {
                keys: PartSet::EMPTY,
                share: 0.0,
                announce: true,
                dust: 2.0,
            },
            debris,
            variant: 0,
        }
    }

    /// Fasten parts `a` and `b` together.
    pub fn join(&mut self, a: usize, b: usize) {
        if a != b && !self.joints[a].contains(&(b as u16)) {
            self.joints[a].push(b as u16);
            self.joints[b].push(a as u16);
        }
    }

    /// Joints from the supports: every part fastened to what holds it up, but for the
    /// parts in `apart` (a chimney's masonry is not nailed to the roof).
    pub fn join_supports(&mut self, apart: &PartSet) {
        for index in 0..self.parts.len() {
            for k in 0..self.supports[index].len() {
                let by = self.supports[index][k].by as usize;
                if !apart.contains(index) && !apart.contains(by) {
                    self.join(index, by);
                }
            }
        }
        for joints in &mut self.joints {
            joints.sort_unstable();
        }
    }

    /// Settle each part's supports: the best share each supporter passes at each end,
    /// once.
    pub fn tidy_supports(&mut self) {
        for supports in &mut self.supports {
            supports.sort_by(|a, b| {
                (a.by, end_rank(a.end))
                    .cmp(&(b.by, end_rank(b.end)))
                    .then(b.share.total_cmp(&a.share))
            });
            supports.dedup_by_key(|support| (support.by, support.end));
        }
    }

    /// A part or landing piece by index.
    pub fn part(&self, index: usize) -> &Part {
        if index < self.parts.len() {
            &self.parts[index]
        } else {
            &self.pieces[index - self.parts.len()]
        }
    }

    /// Every part, as a set.
    pub fn all(&self) -> PartSet {
        PartSet::from_indices(0..self.parts.len())
    }

    /// How many parts the standing share counts: all but footings.
    pub fn breakable(&self) -> usize {
        self.parts.iter().filter(|part| !part.footing).count()
    }

    /// A build-frame offset as a world offset from the structure's ground centre.
    pub fn to_world(&self, local: DVec3) -> DVec3 {
        let (sin, cos) = self.yaw.sin_cos();
        DVec3::new(
            local.x * cos + local.z * sin,
            local.y,
            -local.x * sin + local.z * cos,
        )
    }

    /// A world offset from the structure's ground centre in the build frame.
    pub fn to_local(&self, world: DVec3) -> DVec3 {
        let (sin, cos) = self.yaw.sin_cos();
        DVec3::new(
            world.x * cos - world.z * sin,
            world.y,
            world.x * sin + world.z * cos,
        )
    }

    /// Part `index`'s box as world offsets (min, max) from the ground centre.
    pub fn world_box(&self, index: usize) -> (DVec3, DVec3) {
        let part = self.part(index);
        let (a, b) = (self.to_world(part.min), self.to_world(part.max));
        (a.min(b), a.max(b))
    }

    pub fn design(&self) -> Option<&HouseDesign> {
        match &self.build {
            Build::House { design, .. } => Some(design),
            _ => None,
        }
    }

    pub fn roof(&self) -> Option<&Roof> {
        match &self.build {
            Build::House { roof, .. } => Some(roof),
            _ => None,
        }
    }

    /// Each part's support: 1 for a grounded standing part, otherwise the best share of
    /// support any path of standing parts passes to it from the ground (0 for a part not
    /// standing), a span holding by the weaker of its ends. What a part rests on passes
    /// less once less than its seat share still stands firmly held: a part held by less
    /// than [`FIRM`] seats nothing, so this settles until the firm parts no longer change.
    pub fn stability(&self, standing: &dyn Fn(usize) -> bool) -> Vec<f32> {
        let mut firm: Vec<bool> = (0..self.parts.len()).map(standing).collect();
        loop {
            let support = self.settle(standing, &firm);
            let next: Vec<bool> = (0..self.parts.len())
                .map(|index| standing(index) && support[index] >= FIRM)
                .collect();
            if next == firm {
                return support;
            }
            firm = next;
        }
    }

    /// [`Self::stability`] with the parts that seat others given.
    fn settle(&self, standing: &dyn Fn(usize) -> bool, firm: &[bool]) -> Vec<f32> {
        let count = self.parts.len();
        // Each part's best offer at each end (in [`end_rank`] order), and whether it spans.
        let mut best = vec![[0.0f32; 3]; count];
        let spans: Vec<bool> = self
            .supports
            .iter()
            .map(|supports| supports.iter().any(|support| support.end != End::Any))
            .collect();
        let held_by = |best: &[f32; 3], spans: bool, beam: Option<f32>| {
            let ends = match beam {
                Some(tie) => best[1].min(best[2]).min(best[1] * best[2] / tie),
                None => best[1].min(best[2]),
            };
            if spans { best[0].max(ends) } else { best[0] }
        };
        let mut support = vec![0.0f32; count];
        let mut settled = vec![false; count];
        for (index, part) in self.parts.iter().enumerate() {
            if part.grounded && standing(index) {
                support[index] = 1.0;
            }
        }
        // The parts each part holds, at which end and the share it passes them, so a
        // settled part offers them its support.
        let mut holds: Vec<Vec<(u16, usize, f32)>> = vec![Vec::new(); count];
        for (index, supports) in self.supports.iter().enumerate() {
            for end in [End::Any, End::A, End::B] {
                let (seated, seat) = supports
                    .iter()
                    .filter(|support| support.end == end && support.seat > 0.0)
                    .fold((0.0f32, 0.0f32), |(seated, seat), support| {
                        let left = if firm[support.by as usize] {
                            support.seat
                        } else {
                            0.0
                        };
                        (seated + left, seat + support.seat)
                    });
                // Short of the seat it needs a part loses its hold fast: a sheet seated
                // on a corner post alone is not a sheet held at a sixth of its eave.
                let seating = if seat > 0.0 {
                    (seated / seat / self.parts[index].seat_needed)
                        .min(1.0)
                        .powi(2)
                } else {
                    1.0
                };
                for support in supports.iter().filter(|support| support.end == end) {
                    let share = if support.seat > 0.0 {
                        support.share * seating
                    } else {
                        support.share
                    };
                    holds[support.by as usize].push((index as u16, end_rank(end), share));
                }
            }
        }
        // Dijkstra on the best product of shares: every share is at most 1 and a span
        // takes its weaker end, so a part's support is settled once it is the best open
        // part.
        loop {
            // The best open part (the lowest index on a tie), found by a linear scan: a
            // structure has at most a few hundred parts.
            let mut chosen = None;
            let mut value = 0.0f32;
            for index in 0..count {
                if !settled[index] && support[index] > value {
                    value = support[index];
                    chosen = Some(index);
                }
            }
            let Some(index) = chosen else {
                break;
            };
            settled[index] = true;
            for &(held, end, share) in &holds[index] {
                let held = held as usize;
                if !settled[held] && standing(held) {
                    let offered = value * share;
                    if offered > best[held][end] {
                        best[held][end] = offered;
                        support[held] =
                            support[held].max(held_by(&best[held], spans[held], self.parts[held].beam));
                    }
                }
            }
        }
        support
    }

    /// The standing parts (not in `gone`) that can no longer stand, and the groups among
    /// them that tipped over with where they turn.
    #[allow(clippy::type_complexity)]
    fn giving_way(
        &self,
        gone: &PartSet,
        toward: DVec3,
    ) -> (PartSet, Vec<(PartSet, (DVec3, DVec2))>) {
        let mut falling = PartSet::EMPTY;
        let mut tips = Vec::new();
        loop {
            let down = gone.union(&falling);
            let standing = |index: usize| !down.contains(index);
            let mut more = PartSet::EMPTY;
            if self.shares {
                let stability = self.stability(&standing);
                for (index, (part, support)) in self.parts.iter().zip(&stability).enumerate() {
                    if standing(index) && !part.footing && *support < FALL_BELOW {
                        more.insert(index);
                    }
                }
            }
            for (index, part) in self.parts.iter().enumerate() {
                if standing(index) && part.braces.iter().any(|brace| !brace.holds(&standing)) {
                    more.insert(index);
                }
            }
            if more.is_empty()
                && let Some(rule) = self.tipping
            {
                let up = PartSet::from_indices((0..self.parts.len()).filter(|&index| standing(index)));
                for group in self.joined(&up) {
                    let contacts: Vec<DVec2> = group
                        .iter()
                        .flat_map(|index| self.parts[index].contacts.iter().copied())
                        .collect();
                    let anchored = group.iter().any(|index| self.parts[index].anchor);
                    let loaded = group.iter().any(|index| self.parts[index].load);
                    // Off the ground a group stands on what the share rule found holding
                    // it (a chimney on its roof); anchored, a light frame stands.
                    if (contacts.is_empty() && self.shares) || (anchored && !loaded) {
                        continue;
                    }
                    let (_, weight) = self.balance(&group);
                    let Some(tip) = tipping(&contacts, weight, toward, rule) else {
                        continue;
                    };
                    more = more.union(&group);
                    if loaded || !contacts.is_empty() {
                        tips.push((group, tip));
                    }
                }
            }
            if more.is_empty() {
                return (falling, tips);
            }
            falling = falling.union(&more);
        }
    }

    /// The standing parts (not in `gone`) the structure no longer holds up.
    pub fn unsupported(&self, gone: &PartSet) -> PartSet {
        self.giving_way(gone, DVec3::ZERO).0
    }

    /// What comes down once the parts in `gone` are out (broken or fallen) and those in
    /// `loose` are torn free: every part that can no longer stand and the loose parts, in
    /// the groups still fastened together, each in order of its lowest part. A group that
    /// tipped over turns toward the side its weight lies out over, or toward `toward`
    /// (build frame) when its weight lies on the line of its contacts; a part that
    /// detaches from it goes over the same way about its own foot.
    pub fn falls(&self, gone: &PartSet, loose: &PartSet, toward: DVec3) -> Vec<Fall> {
        let (falling, tips) = self.giving_way(&gone.union(loose), toward);
        let mut falling = falling.union(loose);
        let way = tips.first().map(|(_, (_, way))| *way);
        let detached: Vec<usize> = match way {
            Some(_) => falling
                .iter()
                .filter(|&index| index < self.parts.len() && self.parts[index].detaches)
                .collect(),
            None => Vec::new(),
        };
        for &index in &detached {
            falling.remove(index);
        }
        let mut falls: Vec<Fall> = self
            .joined(&falling)
            .into_iter()
            .map(|parts| {
                let tip = tips
                    .iter()
                    .find(|(group, _)| group.iter().any(|index| parts.contains(index)))
                    .map(|(_, tip)| *tip);
                Fall { parts, tip }
            })
            .collect();
        if let Some(way) = way {
            for index in detached {
                let middle = self.parts[index].middle();
                falls.push(Fall {
                    parts: PartSet::from_indices([index]),
                    tip: Some((DVec3::new(middle.x, 0.0, middle.z), way)),
                });
            }
        }
        falls
    }

    /// The groups of `parts` fastened to each other through `parts` alone, each in order
    /// of its lowest part. Footings belong to none: they are the ground.
    pub fn joined(&self, parts: &PartSet) -> Vec<PartSet> {
        self.groups(parts, true)
    }

    fn groups(&self, parts: &PartSet, skip_footings: bool) -> Vec<PartSet> {
        let footing =
            |index: usize| skip_footings && index < self.parts.len() && self.parts[index].footing;
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
                let Some(joints) = self.joints.get(index) else {
                    continue;
                };
                for &next in joints {
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
            let part = self.part(index);
            (mass + part.mass, moment + part.mass_center * part.mass)
        });
        (mass, if mass > 0.0 { moment / mass } else { DVec3::ZERO })
    }

    /// The bodies a fallen body of `parts` breaks into when it lands hard: the parts a
    /// breakup covers as its pieces, the rest apart at their elements, and each element's
    /// group still fastened together cut across by a slanting break line into two to four
    /// runs of unequal size (a group of one or two parts falls apart into its parts). With
    /// `coarse` (the debris budget is short), each element's group lands whole instead.
    /// Pieces already broken off land as they are. The cuts follow from the structure's
    /// variant and the part indices: no random draw.
    pub fn landing(&self, parts: &PartSet, coarse: bool) -> Vec<PartSet> {
        let mut rest = *parts;
        let mut bodies: Vec<PartSet> = Vec::new();
        if !coarse {
            for breakup in &self.breakups {
                if !breakup.needs.is_empty() && breakup.needs.iter().all(|index| rest.contains(index))
                {
                    for index in breakup.needs.iter() {
                        rest.remove(index);
                    }
                    bodies.extend(
                        breakup
                            .pieces
                            .iter()
                            .map(|&piece| PartSet::from_indices([piece as usize])),
                    );
                }
            }
        }
        let pieces: Vec<usize> = rest.iter().filter(|&index| index >= self.parts.len()).collect();
        for index in pieces {
            bodies.push(PartSet::from_indices([index]));
            rest.remove(index);
        }
        let mut elements: Vec<(u16, PartSet)> = Vec::new();
        for index in rest.iter() {
            let key = self.parts[index].element;
            match elements.iter_mut().find(|(other, _)| *other == key) {
                Some((_, set)) => set.insert(index),
                None => elements.push((key, PartSet::from_indices([index]))),
            }
        }
        for group in elements.iter().flat_map(|(_, set)| self.groups(set, false)) {
            if coarse {
                bodies.push(group);
                continue;
            }
            let mut members: Vec<usize> = group.iter().collect();
            let count = members.len();
            if count <= 2 {
                bodies.extend(members.into_iter().map(|part| PartSet::from_indices([part])));
                continue;
            }
            let seed = members[0] + self.variant as usize;
            let pieces = (2 + seed % 3).min(count);
            // A break line slanting 20 to 65 degrees across the build frame's z.
            let angle = 0.35 + (seed % 5) as f64 * 0.19;
            let across = DVec3::new(angle.sin(), 0.0, angle.cos());
            members.sort_by(|&a, &b| {
                let at = |part: usize| self.parts[part].middle().dot(across);
                at(a).total_cmp(&at(b)).then(a.cmp(&b))
            });
            // Unequal shares, the largest first or last as the seed has it.
            let mut weights: Vec<f64> = [4.0, 2.5, 1.5, 1.0][..pieces].to_vec();
            if seed % 2 == 1 {
                weights.reverse();
            }
            let total: f64 = weights.iter().sum();
            let mut start = 0;
            for (k, weight) in weights.iter().enumerate() {
                let left = pieces - k - 1;
                let size = if left == 0 {
                    count - start
                } else {
                    ((count as f64 * weight / total).round() as usize)
                        .max(1)
                        .min(count - start - left)
                };
                bodies.push(PartSet::from_indices(members[start..start + size].iter().copied()));
                start += size;
            }
        }
        bodies.sort_by_key(|body| body.iter().next().unwrap_or(0));
        bodies
    }
}

/// Whether a group resting on `contacts` with its weight at `weight` tips under `rule`,
/// and if so the point on the ground it turns about and the way it falls: it stands while
/// its weight lies far enough inside the polygon of its contacts. Off a line of contacts
/// (or a single one) it falls the way its weight lies off it, or toward `toward` when it
/// lies on it; with no contact it drops where it is, turning toward `toward`.
pub fn tipping(
    contacts: &[DVec2],
    weight: DVec3,
    toward: DVec3,
    rule: Tipping,
) -> Option<(DVec3, DVec2)> {
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
            // clockwise. The group tips over the edge its weight lies nearest inside (or
            // outside) of.
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
            if nearest >= rule.margin + rule.rise * weight.y.max(0.0) {
                return None;
            }
            let (a, b) = (hull[edge], hull[(edge + 1) % count]);
            let along = (b - a).normalize();
            let foot = a + along * (at - a).dot(along).clamp(0.0, a.distance(b));
            Some((ground(foot), -along.perp()))
        }
    }
}

/// The convex hull of `points`, counter-clockwise, without collinear points: the two ends
/// of a line of them, or the one place they all are.
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

    const TOWER: Tipping = Tipping {
        margin: 0.35,
        rise: 0.0,
    };

    #[test]
    fn a_frame_on_three_corners_tips_toward_the_fourth() {
        let contacts = [
            DVec2::new(1.0, 1.0),
            DVec2::new(-1.0, 1.0),
            DVec2::new(1.0, -1.0),
        ];
        let (_, way) = tipping(&contacts, DVec3::ZERO, DVec3::ZERO, TOWER).expect("tips");
        assert!(way.x < 0.0 && way.y < 0.0, "{way:?}");
        let square = [
            DVec2::new(1.0, 1.0),
            DVec2::new(-1.0, 1.0),
            DVec2::new(1.0, -1.0),
            DVec2::new(-1.0, -1.0),
        ];
        assert!(tipping(&square, DVec3::ZERO, DVec3::ZERO, TOWER).is_none());
    }

    #[test]
    fn a_tall_narrow_group_tips_where_a_low_one_on_the_same_footing_stands() {
        let slab = [
            DVec2::new(-1.0, -0.15),
            DVec2::new(1.0, -0.15),
            DVec2::new(1.0, 0.15),
            DVec2::new(-1.0, 0.15),
        ];
        let rule = Tipping {
            margin: 0.0,
            rise: 0.12,
        };
        assert!(tipping(&slab, DVec3::new(0.0, 0.5, 0.0), DVec3::ZERO, rule).is_none());
        assert!(tipping(&slab, DVec3::new(0.0, 2.0, 0.0), DVec3::ZERO, rule).is_some());
    }

    #[test]
    fn part_sets_iterate_their_members_in_order() {
        let set = PartSet::from_indices([200, 3, 64, 63]);
        assert_eq!(set.iter().collect::<Vec<_>>(), [3, 63, 64, 200]);
        assert_eq!(set.len(), 4);
    }
}
