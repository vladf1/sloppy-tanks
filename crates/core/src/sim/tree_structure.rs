//! A cover tree as a structure (`structure.rs`): its rooted stump, the butt where it
//! snaps, the trunk and each limb with its branches and leaves, all shaped from the
//! skeleton its model draws ([`super::grown_layout`] for an oak, a birch or an aspen,
//! [`super::tree_layout`] for a conifer), so what falls is what is drawn.
//!
//! Shells meet its wood alone: a hit on the trunk strains the butt, which breaks to two
//! standard shells as a tree always has, and a hit on a broadleaf's limb snaps it off
//! whole; through its leaves, twigs and a conifer's thin boughs a shell flies on,
//! rustling them. Cut, the trunk and its limbs, fastened together and bearing on the
//! stump at the cut, tip over the way the crown leans and come down as one body; the
//! limbs that strike the ground snap off, and the parts lie on the ground as cottages'
//! and crates' fallen parts do. The stump stays, met by tanks alone.

use std::sync::Arc;

use glam::{DVec2, DVec3};

use super::debris_physics::DebrisMaterial;
use super::grown_layout::{BUTT_RINGS, GrownLayout, LIMB_SNAP_RING, grown_layout, grows};
pub use super::grown_layout::TreePart;
use super::structure::{
    Breaking, Breakup, Build, Contact, DebrisRules, HeldLayouts, Levelling, Lie, Material, Part,
    PartSet, PartShape, Role, Structure, Tipping,
};
use super::tree_layout::{Growth, LayoutDetail, TreeLayout, stem_radius, tree_layout};
use super::tree_proportions::{TreeProportions, tree_proportions};
use crate::geometry::math::transform_point;

pub const TREE_STUMP: usize = 0;
pub const TREE_BUTT: usize = 1;
pub const TREE_TRUNK: usize = 2;
const FIRST_LIMB: usize = 3;

/// Wood that never breaks: a hit on it standing strains the butt instead, and fallen it
/// is knocked aside whole, as a log or a limb is, never burst like a wall.
const WOOD: Material = Material {
    strength: f64::INFINITY,
    density: 0.6,
    surface: DebrisMaterial::Wood,
    breaking: Breaking::Dislodges,
    tear: 0.0,
};
/// The butt snaps to two standard shells (40 each), a tree cover's 80 hit points, in
/// splinters where it breaks.
const BUTT: Material = Material {
    strength: 80.0,
    breaking: Breaking::Splinters,
    ..WOOD
};
/// A limb weighs its wood and the twigs and leaves round its main branch's hull, and
/// snaps off whole to a standard shell on its main branch.
const LIMB: Material = Material {
    strength: 40.0,
    density: 2.0,
    ..WOOD
};
/// A conifer's boughs on one side: their hull is mostly the air between them.
const BOUGHS: Material = Material {
    density: 0.25,
    ..LIMB
};
/// Rooted, it stands whatever its balance; cut, it has nothing to stand on.
const TREE_TIPPING: Tipping = Tipping {
    margin: 0.0,
    rise: 0.0,
    snap: None,
};
/// A felled tree breaks apart only where it strikes the ground hard, never on its own
/// remains, and clears as other scenery's pieces do.
const TREE_DEBRIS: DebrisRules = DebrisRules {
    landing_speed: TREE_LANDING_SPEED,
    firm_only: true,
    one_clock: false,
    life: 9.5,
    life_spread: 2.0,
    deadline: 18.0,
    seam: 0.0,
};
const TREE_DUST: f64 = 2.5;
/// The change of speed (m/s) in a tick that breaks a felled tree where it lands: a crown
/// coming down from full height, not a trunk rolling over.
const TREE_LANDING_SPEED: f64 = 3.0;
/// The stump's tank-only footprint is this tall, so a hull cannot climb it.
const STUMP_HEIGHT: f64 = 1.6;
/// The least thickness (m from its axis) of a limb's hull: its main branch tapers to a
/// tip, and a fallen part's collider is shrunk a little more, too thin to rest on.
const BRANCH_HULL: f64 = 0.1;
/// The least thickness (m from its axis) of a trunk's hull at its top: a conifer's stem
/// tapers to a point.
const TRUNK_TIP: f64 = 0.06;
/// How far up a trunk (m) its butt reaches, where it snaps.
const BUTT_LENGTH: f64 = 0.5;
/// Points round a ring of the stump's footprint and the root plate.
const RING_POINTS: usize = 8;
/// Points round each end of a limb's hull.
const LIMB_RING_POINTS: usize = 6;
/// The root plate it bears on while rooted, as a share of its cover's half width.
const ROOT_PLATE: f64 = 0.9;

/// The trees laid out that something still holds, by place and size.
static TREES: HeldLayouts<[u64; 5]> = HeldLayouts::new();

/// The structure of the cover tree at (x, z) sized w, d, h, shared with whatever still
/// holds it ([`HeldLayouts`]).
pub fn tree_structure(x: f64, z: f64, w: f64, d: f64, h: f64) -> Arc<Structure> {
    let key = [x, z, w, d, h].map(f64::to_bits);
    TREES.get_or_lay_out(key, || {
        if grows(tree_proportions(x, z, w, d, h).family) {
            lay_out_grown(x, z, w, d, h)
        } else {
            lay_out_conifer(x, z, w, d, h)
        }
    })
}

/// `RING_POINTS` points round the vertical axis at `radius` and height `y`.
fn ring(radius: f64, y: f64) -> impl Iterator<Item = DVec3> {
    (0..RING_POINTS).map(move |k| {
        let angle = std::f64::consts::TAU * k as f64 / RING_POINTS as f64;
        DVec3::new(angle.cos() * radius, y, angle.sin() * radius)
    })
}

/// `count` points round the axis from `from` to `to` at each end, `radius` (at least
/// `least`) from it: a hull round a straight branch or stem.
fn rod_points(
    from: DVec3,
    to: DVec3,
    radii: (f64, f64),
    least: f64,
    count: usize,
) -> impl Iterator<Item = DVec3> {
    let axis = (to - from).normalize_or(DVec3::Y);
    let (across, other) = axis.any_orthonormal_pair();
    [(from, radii.0), (to, radii.1)]
        .into_iter()
        .flat_map(move |(center, radius)| {
            (0..count).map(move |k| {
                let angle = std::f64::consts::TAU * k as f64 / count as f64;
                center + (across * angle.cos() + other * angle.sin()) * radius.max(least)
            })
        })
}

/// The points round rings `from..=to` of a grown tree's branch, `least` (m) from its axis at
/// least.
fn ring_points(
    tree: &GrownLayout,
    branch: usize,
    rings: std::ops::RangeInclusive<usize>,
    least: f64,
) -> Vec<DVec3> {
    let branch = &tree.branches[branch];
    branch.sections[rings]
        .iter()
        .flat_map(|section| {
            (0..branch.sides).map(move |k| {
                let angle = std::f64::consts::TAU * f64::from(k) / f64::from(branch.sides);
                section.origin
                    + section.orientation
                        * DVec3::new(angle.cos(), 0.0, angle.sin())
                        * section.radius.max(least)
            })
        })
        .collect()
}

/// The rooted stump, met by tanks alone, and the butt above it, which a shell on the
/// trunk strains and which bears the tree on its root plate.
fn stump_and_butt(proportions: &TreeProportions, w: f64, d: f64, butt: Vec<DVec3>) -> [Part; 2] {
    let stump_ring = || {
        ring(proportions.stump_radius, 0.0).chain(ring(proportions.stump_radius, STUMP_HEIGHT))
    };
    let mut stump = Part::new(
        vec![PartShape::Hull(stump_ring().collect())],
        WOOD,
        Role::Tree(TreePart::Stump),
    );
    stump.footing = true;
    stump.grounded = true;
    stump.contact = Contact::Tanks;
    stump.struck_as = Some(TREE_BUTT as u16);

    let mut butt = Part::new(
        vec![PartShape::Hull(butt)],
        BUTT,
        Role::Tree(TreePart::Butt),
    );
    butt.grounded = true;
    butt.anchor = true;
    butt.contacts = ring(w.min(d) / 2.0 * ROOT_PLATE, 0.0)
        .map(|p| DVec2::new(p.x, p.z))
        .collect();
    [stump, butt]
}

/// The trunk above its butt: cut, it bears on the stump at the cut and turns about it,
/// and comes to lie along its length.
fn trunk(points: Vec<DVec3>, length: DVec3) -> Part {
    let mut trunk = Part::new(
        vec![PartShape::Hull(points)],
        WOOD,
        Role::Tree(TreePart::Trunk),
    );
    trunk.load = true;
    trunk.contacts = vec![DVec2::ZERO];
    trunk.struck_as = Some(TREE_BUTT as u16);
    trunk.lie = Lie::Length(length);
    trunk
}

/// A limb shaped by `points`: shells meet it (a broadleaf's) or nothing does (a
/// conifer's thin boughs), and tanks pass under it either way.
fn limb(points: Vec<DVec3>, material: Material, limb: usize, contact: Contact) -> Part {
    let mut part = Part::new(
        vec![PartShape::Hull(points)],
        material,
        Role::Tree(TreePart::Limb(limb)),
    );
    part.contact = contact;
    part.blocks_ground = false;
    part
}

/// The structure of `parts` with the rules every tree shares: the trunk and its limbs
/// one element, joined trunk to butt and limb to trunk, the butt its health.
fn standing_tree(x: f64, z: f64, w: f64, d: f64, h: f64, parts: Vec<Part>, seed: u32) -> Structure {
    let mut structure = Structure::new(Build::Tree { x, z, w, d, h }, 0.0, parts, TREE_DEBRIS);
    structure.shares = false;
    structure.tipping = Some(TREE_TIPPING);
    structure.join(TREE_BUTT, TREE_TRUNK);
    for limb in FIRST_LIMB..structure.parts.len() {
        structure.join(TREE_TRUNK, limb);
    }
    // The trunk and its limbs are one piece of wood: falling, it snaps only the limbs
    // that strike the ground (`Structure::snaps_on_ground`). The stump and butt stand
    // apart.
    for (index, part) in structure.parts.iter_mut().enumerate() {
        part.element = index.min(TREE_TRUNK) as u16;
    }
    structure.breachable = false;
    structure.snaps_on_ground = true;
    structure.health_part = Some(TREE_BUTT);
    structure.variant = seed;
    structure.levelling = Levelling {
        keys: PartSet::from_indices([TREE_BUTT]),
        share: 0.0,
        announce: true,
        dust: TREE_DUST,
    };
    structure
}

/// A fallen limb a shell meets snaps in two (`pieces`, its inner and outer half); a
/// piece of it is knocked aside whole.
fn snap_in_two(structure: &mut Structure, limb: usize, pieces: [Vec<DVec3>; 2]) {
    let first = (structure.parts.len() + structure.pieces.len()) as u16;
    for (outer, points) in [false, true].into_iter().zip(pieces) {
        structure.pieces.push(Part::new(
            vec![PartShape::Hull(points)],
            LIMB,
            Role::Tree(TreePart::LimbPiece { limb, outer }),
        ));
    }
    structure.snaps.push(Breakup {
        needs: PartSet::from_indices([FIRST_LIMB + limb]),
        pieces: vec![first, first + 1],
        alone: true,
    });
}

/// The box round `points`, if any.
fn bounds(points: impl Iterator<Item = DVec3>) -> Option<(DVec3, DVec3)> {
    let (low, high) = points.fold((DVec3::INFINITY, DVec3::NEG_INFINITY), |(low, high), p| {
        (low.min(p), high.max(p))
    });
    (low.x <= high.x).then_some((low, high))
}

/// A broadleaf (an oak, a birch or an aspen), from its grown skeleton.
fn lay_out_grown(x: f64, z: f64, w: f64, d: f64, h: f64) -> Structure {
    let tree = grown_layout(x, z, w, d, h);
    let trunk_rings = tree.branches[0].sections.len() - 1;
    let mut parts = Vec::with_capacity(FIRST_LIMB + tree.limbs);
    let butt = ring_points(&tree, 0, 0..=BUTT_RINGS, 0.0);
    parts.extend(stump_and_butt(&tree.proportions, w, d, butt));
    let sections = &tree.branches[0].sections;
    parts.push(trunk(
        ring_points(&tree, 0, BUTT_RINGS..=trunk_rings, 0.0),
        sections[trunk_rings].origin - sections[BUTT_RINGS].origin,
    ));
    for index in 0..tree.limbs {
        // Its main branch bears it: its twigs and leaves give under it and never prop it
        // up off the ground.
        let points: Vec<DVec3> = tree
            .branches
            .iter()
            .enumerate()
            .filter(|(_, branch)| branch.limb == Some(index) && branch.level == 1)
            .flat_map(|(branch, grown)| {
                ring_points(&tree, branch, 0..=grown.sections.len() - 1, BRANCH_HULL)
            })
            .collect();
        // High over a tank's hull: shells meet it, tanks pass under.
        parts.push(limb(points, LIMB, index, Contact::Shells));
    }
    let mut structure = standing_tree(x, z, w, d, h, parts, tree.proportions.seed);
    for index in 0..tree.limbs {
        let Some((main, branch)) = tree
            .branches
            .iter()
            .enumerate()
            .find(|(_, branch)| branch.limb == Some(index) && branch.level == 1)
        else {
            continue;
        };
        let last = branch.sections.len() - 1;
        let snap = LIMB_SNAP_RING.min(last);
        snap_in_two(
            &mut structure,
            index,
            [0..=snap, snap..=last].map(|rings| ring_points(&tree, main, rings, BRANCH_HULL)),
        );
    }
    // Each limb's leaves, which a shell flies through.
    for index in 0..tree.limbs {
        let corners = tree
            .sprigs
            .iter()
            .filter(|sprig| tree.branches[sprig.branch].limb == Some(index))
            .flat_map(|sprig| sprig.corners);
        if let Some((low, high)) = bounds(corners) {
            structure.foliage.push((FIRST_LIMB + index, low, high));
        }
    }
    structure
}

/// The corners of a needle spray's three crossed cards' box.
fn spray_corners(transform: &glam::DMat4) -> impl Iterator<Item = DVec3> + '_ {
    (0..8).map(move |k| {
        let corner = DVec3::new(
            if k & 1 == 0 { -0.5 } else { 0.5 },
            if k & 2 == 0 { -0.5 } else { 0.5 },
            if k & 4 == 0 { 0.0 } else { 1.0 },
        );
        transform_point(transform, corner)
    })
}

/// A conifer, from its layout: its limbs are its boughs gathered by side and height,
/// too thin for a shell to meet.
fn lay_out_conifer(x: f64, z: f64, w: f64, d: f64, h: f64) -> Structure {
    let tree = tree_layout(x, z, w, d, h, LayoutDetail::Full);
    let TreeLayout {
        proportions,
        trunk_base,
        trunk_top,
        growth,
        limbs,
        ..
    } = &tree;
    let base = DVec3::new(0.0, *trunk_base, 0.0);
    let length = (*trunk_top - base).length();
    let cut = (BUTT_LENGTH / length).min(0.2);
    let radius_at = |along: f64| stem_radius(proportions, along);
    let at = |along: f64| base.lerp(*trunk_top, along);
    let mut parts = Vec::with_capacity(FIRST_LIMB + limbs);
    let butt = rod_points(base, at(cut), (radius_at(0.0), radius_at(cut)), 0.0, RING_POINTS);
    parts.extend(stump_and_butt(proportions, w, d, butt.collect()));
    let trunk_points = rod_points(
        at(cut),
        *trunk_top,
        (radius_at(cut), radius_at(1.0)),
        TRUNK_TIP,
        RING_POINTS,
    );
    parts.push(trunk(trunk_points.collect(), *trunk_top - at(cut)));
    for index in 0..*limbs {
        // Every bough on its side and height bears it.
        let points: Vec<DVec3> = growth
            .iter()
            .filter_map(|step| match *step {
                Growth::Limb {
                    from,
                    to,
                    radius,
                    limb,
                    ..
                } if limb == Some(index) => Some(rod_points(
                    from,
                    to,
                    (radius, radius),
                    BRANCH_HULL,
                    LIMB_RING_POINTS,
                )),
                _ => None,
            })
            .flatten()
            .collect();
        parts.push(limb(points, BOUGHS, index, Contact::Nothing));
    }
    let mut structure = standing_tree(x, z, w, d, h, parts, proportions.seed);
    // The needles of each limb, and of the trunk, which a shell flies through.
    let foliage = |limb: Option<usize>| {
        bounds(
            growth
                .iter()
                .filter(|step| step.limb() == limb)
                .flat_map(|step| match step {
                    Growth::Spray { transform, .. } => Some(spray_corners(transform)),
                    _ => None,
                })
                .flatten(),
        )
    };
    for (part, limb) in std::iter::once((TREE_TRUNK, None))
        .chain((0..*limbs).map(|index| (FIRST_LIMB + index, Some(index))))
    {
        if let Some((low, high)) = foliage(limb) {
            structure.foliage.push((part, low, high));
        }
    }
    structure
}
