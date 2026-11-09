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

use std::sync::{Arc, Mutex, OnceLock, Weak};

use glam::{DMat3, DQuat, DVec2, DVec3};
use serde::{Deserialize, Serialize};

use super::crate_structure::{CratePart, crate_structure};
use super::debris_physics::DebrisMaterial;
use super::house_layout::{HouseDesign, HousePlan, Roof};
use super::house_structure::{HouseRole, shared_house_structure};
use super::kinds::CoverKind;
use super::structure_physics::ShapeCache;
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
    pub fn iter(&self) -> PartIndices {
        PartIndices {
            words: self.0,
            word: 0,
        }
    }

    /// The lowest index in the set.
    pub fn first(&self) -> Option<usize> {
        self.iter().next()
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

/// A [`PartSet`]'s indices, lowest first: each word's set bits by their trailing zeros
/// rather than a test of every bit.
#[derive(Clone, Debug)]
pub struct PartIndices {
    words: [u64; MAX_PARTS / 64],
    word: usize,
}

impl Iterator for PartIndices {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        while self.word < self.words.len() {
            let bits = self.words[self.word];
            if bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                self.words[self.word] = bits & (bits - 1);
                return Some(self.word * 64 + bit);
            }
            self.word += 1;
        }
        None
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
    /// A heavy timber held in slots: what would break it knocks it out whole instead,
    /// thrown from the hit (a timber wall's beam, far too thick to snap).
    Dislodges,
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
    /// A strike that breaks the part tears the parts of the same material, held by the
    /// same parts, within this reach (m) of where it struck along with it (the boards
    /// nailed to a crate wall's frame).
    pub tear: f64,
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
/// One `alone` applies only to what a landing leaves of `needs` as a body of its own
/// (a roof sheet that the slope it fell in broke away from its neighbours).
#[derive(Clone, Debug, PartialEq)]
pub struct Breakup {
    pub needs: PartSet,
    pub pieces: Vec<u16>,
    pub alone: bool,
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

    /// The middle of the first of the part's shapes that reaches height `y` (build
    /// frame), at that height: somewhere a level shell meets the part itself.
    pub fn point_at_height(&self, y: f64) -> Option<DVec3> {
        self.shapes.iter().find_map(|shape| {
            let (low, high, middle) = match shape {
                PartShape::Box {
                    center,
                    half,
                    rotation,
                } => {
                    let corners = [-1.0, 1.0].iter().flat_map(|&x| {
                        [-1.0, 1.0].iter().flat_map(move |&y| {
                            [-1.0, 1.0].map(move |z| *center + *rotation * (*half * DVec3::new(x, y, z)))
                        })
                    });
                    let (low, high) = corners.fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), p| {
                        (low.min(p.y), high.max(p.y))
                    });
                    (low, high, *center)
                }
                PartShape::Hull(points) => {
                    let (low, high) = points.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), p| {
                        (low.min(p.y), high.max(p.y))
                    });
                    (low, high, points.iter().copied().sum::<DVec3>() / points.len().max(1) as f64)
                }
            };
            (low <= y && y <= high).then(|| middle.with_y(y))
        })
    }

    /// How far `p` (build frame) lies from the part's own shapes: a frame of battens
    /// round a wall is near its edges only, where its box covers the whole wall.
    pub fn shape_distance(&self, p: DVec3) -> f64 {
        self.shapes
            .iter()
            .map(|shape| match shape {
                PartShape::Box {
                    center,
                    half,
                    rotation,
                } => {
                    let local = rotation.inverse() * (p - *center);
                    (local.clamp(-*half, *half) - local).length()
                }
                PartShape::Hull(_) => self.distance(p),
            })
            .fold(f64::INFINITY, f64::min)
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
/// plus `rise` times its height inside the ground it bears on. A group that tips snaps
/// off the parts it stands on that reach no higher than `snap` (m): the stubs stay.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tipping {
    pub margin: f64,
    pub rise: f64,
    pub snap: Option<f64>,
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
/// velocity changes by `landing_speed` (m/s) in a tick, or where it jams still; with
/// `firm_only` only where it strikes the ground or cover (not its own remains or other
/// debris), and never where it jams: a frame propped on something stays whole until a
/// new hit or a hard landing breaks it; a loose piece
/// lies `life` seconds (plus up to `life_spread` by its parts) before it clears, and is
/// gone by `deadline`. Standing parts' colliders reach `seam` metres past their faces,
/// so a shell along the seam between two parts meets one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DebrisRules {
    pub landing_speed: f64,
    pub firm_only: bool,
    /// The whole ruin clears on one clock, top first (a watchtower's frame of sticks);
    /// otherwise each piece keeps its own time, clearing before what it lies on.
    pub one_clock: bool,
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
    /// Whether the structure stands as one box collider while none of its parts is gone
    /// (its parts fill its box: a timber wall, a crate), a strike finding the part at the
    /// point it lands; it takes a collider for each part once one goes. The world keeps
    /// a slot for every collider, and the village's timber walls by the part pushed it
    /// over a doubling of its arenas.
    pub whole: bool,
    /// Whether a part that loses its other supports while one still stands hinges down
    /// off that one's top edge (a crate's lid off its last wall) rather than dropping.
    pub hinges: bool,
    /// Whether a bot shoots its way through it: a crate's frame of battens blocks a
    /// tank until it breaks, which takes several shells, so bots drive round a crate.
    pub breachable: bool,
    /// Its parts' collider shapes, each built the first time a collider asks for it.
    pub shapes: ShapeCache,
    /// Its supports turned round, built the first time the share rule runs.
    holds: HoldsCache,
}

/// A body of a structure's parts coming down together: its parts, and for a group that
/// tips over, the point on the ground (build frame) it turns about and the way it falls
/// (build frame x and z, unit length).
#[derive(Clone, Debug, PartialEq)]
pub struct Fall {
    pub parts: PartSet,
    pub tip: Option<(DVec3, DVec2)>,
}

/// Layouts that something still holds, by what they were laid out from. A layout is
/// never changed once built and follows from that alone, so the simulation's cover,
/// its model and a reset's next round (which holds the last round's until its own
/// covers are in) all take one instead of each laying it out again, with its part
/// shapes already built. Kept only while held, never for good.
pub(crate) struct HeldLayouts<K>(Mutex<Vec<(K, Weak<Structure>)>>);

impl<K: PartialEq> HeldLayouts<K> {
    pub(crate) const fn new() -> Self {
        Self(Mutex::new(Vec::new()))
    }

    /// The layout held for `key`, or the one `lay_out` builds.
    pub(crate) fn get_or_lay_out(&self, key: K, lay_out: impl FnOnce() -> Structure) -> Arc<Structure> {
        let held = |layouts: &[(K, Weak<Structure>)]| {
            layouts
                .iter()
                .find(|(other, _)| *other == key)
                .and_then(|(_, layout)| layout.upgrade())
        };
        if let Some(layout) = held(&self.0.lock().expect("held layouts")) {
            return layout;
        }
        // Laid out outside the lock: another thread's structures need not wait for it.
        let layout = Arc::new(lay_out());
        let mut layouts = self.0.lock().expect("held layouts");
        if let Some(layout) = held(&layouts) {
            return layout;
        }
        layouts.retain(|(_, layout)| layout.strong_count() > 0);
        layouts.push((key, Arc::downgrade(&layout)));
        layout
    }
}

/// The structure a cover of `kind` stands for, if it is built of parts.
#[allow(clippy::too_many_arguments)]
pub fn structure_of(
    kind: CoverKind,
    x: f64,
    z: f64,
    w: f64,
    d: f64,
    h: f64,
    join: Option<TimberJoin>,
    plan: Option<HousePlan>,
) -> Option<Arc<Structure>> {
    match kind {
        CoverKind::House => Some(shared_house_structure(&HouseDesign::for_cover(
            x, z, w, d, h, plan,
        ))),
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
            whole: false,
            hinges: false,
            breachable: true,
            shapes: ShapeCache::default(),
            holds: HoldsCache::default(),
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
    /// The standing part (not in `gone`) nearest the build-frame point `local`, the
    /// lowest index among those as near.
    pub fn part_at(&self, local: DVec3, gone: &PartSet) -> Option<usize> {
        (0..self.parts.len())
            .filter(|&index| !gone.contains(index))
            .min_by(|&a, &b| {
                self.parts[a]
                    .shape_distance(local)
                    .total_cmp(&self.parts[b].shape_distance(local))
            })
    }

    /// The build-frame box round every part.
    pub fn bounds(&self) -> (DVec3, DVec3) {
        self.parts.iter().fold(
            (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
            |(min, max), part| (min.min(part.min), max.max(part.max)),
        )
    }

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
    /// Indexed by part; the entries past the structure's parts are 0.
    pub fn stability(&self, standing: &dyn Fn(usize) -> bool) -> [f32; MAX_PARTS] {
        self.stability_with(self.holds(), standing)
    }

    fn holds(&self) -> &Holds {
        self.holds.0.get_or_init(|| Holds::of(self))
    }

    /// [`Self::stability`] over the structure's [`Holds`].
    fn stability_with(&self, holds: &Holds, standing: &dyn Fn(usize) -> bool) -> [f32; MAX_PARTS] {
        let count = self.parts.len();
        let mut firm = PartSet::from_indices((0..count).filter(|&index| standing(index)));
        let mut support = [0.0f32; MAX_PARTS];
        loop {
            self.settle(holds, standing, &firm, &mut support);
            let next = PartSet::from_indices(
                (0..count).filter(|&index| standing(index) && support[index] >= FIRM),
            );
            if next == firm {
                return support;
            }
            firm = next;
        }
    }

    /// [`Self::stability`] into `support` with the parts that seat others given.
    fn settle(
        &self,
        holds: &Holds,
        standing: &dyn Fn(usize) -> bool,
        firm: &PartSet,
        support: &mut [f32; MAX_PARTS],
    ) {
        let count = self.parts.len();
        // Each part's best offer at each end (in [`end_rank`] order).
        let mut best = [[0.0f32; 3]; MAX_PARTS];
        let held_by = |best: &[f32; 3], spans: bool, beam: Option<f32>| {
            let ends = match beam {
                Some(tie) => best[1].min(best[2]).min(best[1] * best[2] / tie),
                None => best[1].min(best[2]),
            };
            if spans { best[0].max(ends) } else { best[0] }
        };
        support[..count].fill(0.0);
        let mut settled = [false; MAX_PARTS];
        // The open parts with some support: the only ones the best can be among.
        let mut open = [0u16; MAX_PARTS];
        let mut open_count = 0;
        for (index, part) in self.parts.iter().enumerate() {
            if part.grounded && standing(index) {
                support[index] = 1.0;
                open[open_count] = index as u16;
                open_count += 1;
            }
        }
        // How much of the share each part's seats at each end pass on, from what still
        // stands firmly under it.
        let mut seating = [[1.0f32; 3]; MAX_PARTS];
        for (index, supports) in self.supports.iter().enumerate() {
            for end in [End::Any, End::A, End::B] {
                let (seated, seat) = supports
                    .iter()
                    .filter(|support| support.end == end && support.seat > 0.0)
                    .fold((0.0f32, 0.0f32), |(seated, seat), support| {
                        let left = if firm.contains(support.by as usize) {
                            support.seat
                        } else {
                            0.0
                        };
                        (seated + left, seat + support.seat)
                    });
                // Short of the seat it needs a part loses its hold fast: a sheet seated
                // on a corner post alone is not a sheet held at a sixth of its eave.
                seating[index][end_rank(end)] = if seat > 0.0 {
                    (seated / seat / self.parts[index].seat_needed)
                        .min(1.0)
                        .powi(2)
                } else {
                    1.0
                };
            }
        }
        // Dijkstra on the best product of shares: every share is at most 1 and a span
        // takes its weaker end, so a part's support is settled once it is the best open
        // part.
        loop {
            // The best open part (the lowest index on a tie), found by a linear scan of
            // the open parts: a structure has at most a few hundred parts, and few of them
            // are open at once.
            let mut chosen = None;
            let mut value = 0.0f32;
            for (slot, &index) in open[..open_count].iter().enumerate() {
                let index = index as usize;
                if support[index] > value
                    || (support[index] == value && chosen.is_some_and(|(_, best)| index < best))
                {
                    value = support[index];
                    chosen = Some((slot, index));
                }
            }
            let Some((slot, index)) = chosen else {
                break;
            };
            open_count -= 1;
            open[slot] = open[open_count];
            settled[index] = true;
            for hold in holds.of_part(index) {
                let held = hold.held as usize;
                if !settled[held] && standing(held) {
                    let end = hold.end as usize;
                    let share = if hold.seated {
                        hold.share * seating[held][end]
                    } else {
                        hold.share
                    };
                    let offered = value * share;
                    if offered > best[held][end] {
                        best[held][end] = offered;
                        let before = support[held];
                        support[held] = before.max(held_by(
                            &best[held],
                            holds.spans.contains(held),
                            self.parts[held].beam,
                        ));
                        if before == 0.0 && support[held] > 0.0 {
                            open[open_count] = held as u16;
                            open_count += 1;
                        }
                    }
                }
            }
        }
    }

    /// The standing parts (not in `gone`) that can no longer stand, and the groups among
    /// them that tipped over with where they turn. Whether a part falls does not depend
    /// on `toward`, only which way a group tips.
    pub fn giving_way(&self, gone: &PartSet, toward: DVec3) -> GivingWay {
        let holds = self.shares.then(|| self.holds());
        let mut falling = PartSet::EMPTY;
        let mut tips = Vec::new();
        // Room for every part's contacts at once (and a hull's chains round them), so the
        // groups share it without growing it.
        let room = match self.tipping {
            Some(_) => self.parts.iter().map(|part| part.contacts.len()).sum(),
            None => 0,
        };
        let mut contacts = Vec::with_capacity(room);
        let mut hull = Hull {
            sorted: Vec::with_capacity(room),
            points: Vec::with_capacity(2 * room),
        };
        loop {
            let down = gone.union(&falling);
            let standing = |index: usize| !down.contains(index);
            let mut more = PartSet::EMPTY;
            if let Some(holds) = holds {
                let stability = self.stability_with(holds, &standing);
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
                for group in self.groups(&up, true) {
                    contacts.clear();
                    contacts.extend(
                        group
                            .iter()
                            .flat_map(|index| self.parts[index].contacts.iter().copied()),
                    );
                    let anchored = group.iter().any(|index| self.parts[index].anchor);
                    let loaded = group.iter().any(|index| self.parts[index].load);
                    // Off the ground a group stands on what the share rule found holding
                    // it (a chimney on its roof); anchored, a light frame stands.
                    if (contacts.is_empty() && self.shares) || (anchored && !loaded) {
                        continue;
                    }
                    let (_, weight) = self.balance(&group);
                    let Some(tip) = tipping_with(&contacts, weight, toward, rule, &mut hull) else {
                        continue;
                    };
                    // A strip of wall alone that tips over snaps off its stubs on the
                    // ground, which stand on as they can by themselves; a section that
                    // carries roof comes down whole.
                    let over = PartSet::from_indices(group.iter().filter(|&index| {
                        let part = &self.parts[index];
                        !(part.grounded && rule.snap.is_some_and(|snap| part.max.y <= snap))
                    }));
                    let over = if over.is_empty()
                        || over.iter().any(|index| !self.parts[index].upright)
                    {
                        group
                    } else {
                        over
                    };
                    more = more.union(&over);
                    if loaded || !contacts.is_empty() {
                        tips.push((over, tip));
                    }
                }
            }
            if more.is_empty() {
                return GivingWay { falling, tips };
            }
            falling = falling.union(&more);
        }
    }

    /// The standing parts (not in `gone`) the structure no longer holds up.
    pub fn unsupported(&self, gone: &PartSet) -> PartSet {
        self.giving_way(gone, DVec3::ZERO).falling
    }

    /// What comes down once the parts in `gone` are out (broken or fallen) and those in
    /// `loose` are torn free: every part that can no longer stand and the loose parts, in
    /// the groups still fastened together, each in order of its lowest part. A group that
    /// tipped over turns toward the side its weight lies out over, or toward `toward`
    /// (build frame) when its weight lies on the line of its contacts; a part that
    /// detaches from it goes over the same way about its own foot.
    pub fn falls(&self, gone: &PartSet, loose: &PartSet, toward: DVec3) -> Vec<Fall> {
        self.falls_after(gone, loose, self.giving_way(&gone.union(loose), toward))
    }

    /// [`Self::falls`] from what gives way with `gone` and `loose` out, `way`
    /// ([`Self::giving_way`] of their union).
    pub fn falls_after(&self, gone: &PartSet, loose: &PartSet, way: GivingWay) -> Vec<Fall> {
        let GivingWay { falling, tips } = way;
        let mut falling = falling.union(loose);
        let way = tips.first().map(|(_, (_, way))| *way);
        let detached = match way {
            Some(_) => PartSet::from_indices(
                falling
                    .iter()
                    .filter(|&index| index < self.parts.len() && self.parts[index].detaches),
            ),
            None => PartSet::EMPTY,
        };
        for index in detached.iter() {
            falling.remove(index);
        }
        let down = gone.union(&falling);
        let mut falls: Vec<Fall> = self
            .groups(&falling, true)
            .map(|parts| {
                let tip = tips
                    .iter()
                    .find(|(group, _)| group.iter().any(|index| parts.contains(index)))
                    .map(|(_, tip)| *tip)
                    .or_else(|| self.hinge(&parts, &down));
                Fall { parts, tip }
            })
            .collect();
        if let Some(way) = way {
            for index in detached.iter() {
                let middle = self.parts[index].middle();
                falls.push(Fall {
                    parts: PartSet::from_indices([index]),
                    tip: Some((DVec3::new(middle.x, 0.0, middle.z), way)),
                });
            }
        }
        falls
    }

    /// Where falling `parts` hinge down off a supporter still standing (not in `down`),
    /// for a structure that [`Structure::hinges`]: the top edge of the supporter nearest
    /// their middle, turning away from it.
    fn hinge(&self, parts: &PartSet, down: &PartSet) -> Option<(DVec3, DVec2)> {
        if !self.hinges {
            return None;
        }
        let (_, middle) = self.balance(parts);
        let by = parts
            .iter()
            .flat_map(|index| self.supports[index].iter().map(|support| support.by as usize))
            .filter(|&by| !down.contains(by) && !parts.contains(by))
            .min_by(|&a, &b| {
                self.parts[a]
                    .distance(middle)
                    .total_cmp(&self.parts[b].distance(middle))
                    .then(a.cmp(&b))
            })?;
        let holder = &self.parts[by];
        let pivot = DVec3::new(
            middle.x.clamp(holder.min.x, holder.max.x),
            holder.max.y,
            middle.z.clamp(holder.min.z, holder.max.z),
        );
        let way = DVec2::new(middle.x - pivot.x, middle.z - pivot.z);
        (way.length() > 1e-6).then(|| (pivot, way.normalize()))
    }

    /// The groups of `parts` fastened to each other through `parts` alone, each in order
    /// of its lowest part. Footings belong to none: they are the ground.
    pub fn joined(&self, parts: &PartSet) -> Vec<PartSet> {
        self.groups(parts, true).collect()
    }

    /// The groups of `parts` fastened together, as [`Self::joined`] finds them, footings
    /// left out with `skip_footings`, found one at a time.
    fn groups(&self, parts: &PartSet, skip_footings: bool) -> Groups<'_> {
        Groups {
            structure: self,
            parts: *parts,
            starts: parts.iter(),
            seen: PartSet::EMPTY,
            skip_footings,
        }
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
            for breakup in self.breakups.iter().filter(|breakup| !breakup.alone) {
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
        let landed = rest;
        for index in landed.iter().filter(|&index| index >= self.parts.len()) {
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
        let mut members: Vec<usize> = Vec::new();
        for group in elements.iter().flat_map(|(_, set)| self.groups(set, false)) {
            if coarse {
                bodies.push(group);
                continue;
            }
            members.clear();
            members.extend(group.iter());
            let count = members.len();
            if count <= 2 {
                bodies.extend(members.iter().map(|&part| PartSet::from_indices([part])));
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
            let mut shares = [4.0, 2.5, 1.5, 1.0];
            let weights = &mut shares[..pieces];
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
        // A part a landing left on its own breaks along its own pieces.
        let alone = |body: &PartSet| {
            self.breakups
                .iter()
                .find(|breakup| breakup.alone && breakup.needs == *body)
        };
        if !coarse && bodies.iter().any(|body| alone(body).is_some()) {
            let mut split = Vec::with_capacity(bodies.len());
            for body in bodies {
                match alone(&body) {
                    Some(breakup) => split.extend(
                        breakup
                            .pieces
                            .iter()
                            .map(|&piece| PartSet::from_indices([piece as usize])),
                    ),
                    None => split.push(body),
                }
            }
            bodies = split;
        }
        bodies.sort_by_key(|body| body.iter().next().unwrap_or(0));
        bodies
    }
}

/// What gives way once some parts are out ([`Structure::giving_way`]): the standing
/// parts that can no longer stand, and the groups among them that tipped over with where
/// they turn.
#[derive(Clone, Debug, PartialEq)]
pub struct GivingWay {
    pub falling: PartSet,
    pub tips: Vec<(PartSet, (DVec3, DVec2))>,
}

/// The share rule's supports turned round: for each part, the parts it holds up, at
/// which end and the share it passes them (before its seats are counted), so a settled
/// part offers them its support. They follow from the supports alone, so one break builds
/// them once for every pass of the rule.
struct Holds {
    /// Where each part's holds start in `links`, and past the last the end of them.
    start: Vec<u32>,
    links: Vec<Hold>,
    /// The parts that span between two ends.
    spans: PartSet,
}

#[derive(Clone, Copy, Default)]
struct Hold {
    held: u16,
    end: u8,
    share: f32,
    /// It seats the part it holds: the share falls with what still stands firmly there.
    seated: bool,
}

impl Holds {
    /// Each holder's holds in the order a list per holder had them: by the part held,
    /// then its end in [`end_rank`] order, then its supports' order.
    fn of(structure: &Structure) -> Self {
        let count = structure.parts.len();
        debug_assert!(count <= MAX_PARTS);
        let ends = [End::Any, End::A, End::B];
        let mut start = vec![0u32; count + 1];
        for supports in &structure.supports {
            for support in supports {
                start[support.by as usize + 1] += 1;
            }
        }
        for index in 0..count {
            start[index + 1] += start[index];
        }
        let mut links = vec![Hold::default(); start[count] as usize];
        // Fill each holder's run with its start as a cursor, which ends at the next
        // holder's start; shifted back, the starts are restored.
        for (index, supports) in structure.supports.iter().enumerate() {
            for end in ends {
                for support in supports.iter().filter(|support| support.end == end) {
                    let cursor = &mut start[support.by as usize];
                    links[*cursor as usize] = Hold {
                        held: index as u16,
                        end: end_rank(end) as u8,
                        share: support.share,
                        seated: support.seat > 0.0,
                    };
                    *cursor += 1;
                }
            }
        }
        for index in (1..=count).rev() {
            start[index] = start[index - 1];
        }
        start[0] = 0;
        let spans = PartSet::from_indices(
            structure
                .supports
                .iter()
                .enumerate()
                .filter(|(_, supports)| supports.iter().any(|support| support.end != End::Any))
                .map(|(index, _)| index),
        );
        Self {
            start,
            links,
            spans,
        }
    }

    /// The parts `holder` holds.
    fn of_part(&self, holder: usize) -> &[Hold] {
        &self.links[self.start[holder] as usize..self.start[holder + 1] as usize]
    }
}

/// A structure's [`Holds`], built the first time the share rule runs: a built structure's
/// supports no longer change.
#[derive(Default)]
struct HoldsCache(OnceLock<Holds>);

/// A copy of a structure starts with no holds: it may be changed.
impl Clone for HoldsCache {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl PartialEq for HoldsCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl std::fmt::Debug for HoldsCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HoldsCache")
    }
}

/// The groups of a set of parts fastened together, each in order of its lowest part
/// ([`Structure::joined`]).
struct Groups<'a> {
    structure: &'a Structure,
    parts: PartSet,
    starts: PartIndices,
    seen: PartSet,
    skip_footings: bool,
}

impl Iterator for Groups<'_> {
    type Item = PartSet;

    fn next(&mut self) -> Option<PartSet> {
        let structure = self.structure;
        let skip_footings = self.skip_footings;
        let footing = |index: usize| {
            skip_footings && index < structure.parts.len() && structure.parts[index].footing
        };
        for start in self.starts.by_ref() {
            if self.seen.contains(start) || footing(start) {
                continue;
            }
            // What `start` reaches through the parts no group has taken: the same set
            // whatever order the open parts are taken in.
            let mut group = PartSet::EMPTY;
            let mut open = PartSet::from_indices([start]);
            self.seen.insert(start);
            while let Some(index) = open.first() {
                open.remove(index);
                group.insert(index);
                let Some(joints) = structure.joints.get(index) else {
                    continue;
                };
                for &next in joints {
                    let next = next as usize;
                    if self.parts.contains(next) && !self.seen.contains(next) && !footing(next) {
                        self.seen.insert(next);
                        open.insert(next);
                    }
                }
            }
            return Some(group);
        }
        None
    }
}

/// Room for a convex hull and the sorted points it is taken from, kept across the groups
/// one break tests for tipping.
#[derive(Default)]
struct Hull {
    sorted: Vec<DVec2>,
    points: Vec<DVec2>,
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
    tipping_with(contacts, weight, toward, rule, &mut Hull::default())
}

/// [`tipping`] with room for its hull.
fn tipping_with(
    contacts: &[DVec2],
    weight: DVec3,
    toward: DVec3,
    rule: Tipping,
    room: &mut Hull,
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
    convex_hull(contacts, room);
    let hull = &room.points;
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

/// The convex hull of `points` into `room.points`, counter-clockwise, without collinear
/// points: the two ends of a line of them, or the one place they all are.
fn convex_hull(points: &[DVec2], room: &mut Hull) {
    let Hull { sorted, points: hull } = room;
    sorted.clear();
    sorted.extend_from_slice(points);
    sorted.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    sorted.dedup_by(|a, b| a.distance(*b) < 1e-9);
    hull.clear();
    if sorted.len() < 3 {
        hull.extend_from_slice(sorted);
        return;
    }
    let turns_left = |o: DVec2, a: DVec2, b: DVec2| (a - o).perp_dot(b - o) > 1e-9;
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
                chain(p, hull);
            }
        } else {
            for &p in sorted.iter() {
                chain(p, hull);
            }
        }
        hull.pop();
    }
    if hull.len() < 3 {
        hull.clear();
        hull.extend([sorted[0], sorted[sorted.len() - 1]]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOWER: Tipping = Tipping {
        margin: 0.35,
        rise: 0.0,
        snap: None,
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
            snap: None,
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
