//! A full-detail oak as a structure (`structure.rs`): its rooted stump, the butt where
//! it snaps, the trunk and each limb with its branches and leaves, all shaped from the
//! skeleton its model draws ([`super::oak_layout`]), so what falls is what is drawn.
//!
//! It stands as its cover's box, as every tree does, and a hit anywhere on it strains
//! the butt, which breaks to two standard shells as a tree always has. Then the trunk
//! and its limbs, fastened together and bearing on the stump at the cut, tip over the
//! way the crown leans and come down as one body; landing breaks them apart, the trunk
//! and each limb a body of its own, and they lie on the ground as cottages' and crates'
//! fallen parts do. The stump stays, met by tanks alone.

use std::sync::Arc;

use glam::{DVec2, DVec3};

use super::debris_physics::DebrisMaterial;
use super::oak_layout::{BUTT_RINGS, OAK, OakLayout, oak_layout};
pub use super::oak_layout::TreePart;
use super::structure::{
    Breaking, Build, DebrisRules, HeldLayouts, Levelling, Lie, Material, Part, PartSet, PartShape,
    Role, Structure, Tipping,
};
use super::tree_proportions::tree_proportions;

pub const OAK_STUMP: usize = 0;
pub const OAK_BUTT: usize = 1;
pub const OAK_TRUNK: usize = 2;
const FIRST_LIMB: usize = 3;

/// Wood that never breaks: a hit on it strains the butt instead.
const WOOD: Material = Material {
    strength: f64::INFINITY,
    density: 0.6,
    surface: DebrisMaterial::Wood,
    breaking: Breaking::Splinters,
    tear: 0.0,
};
/// The butt snaps to two standard shells (40 each), a tree cover's 80 hit points.
const BUTT: Material = Material {
    strength: 80.0,
    ..WOOD
};
/// A limb's hulls are mostly leaves and air round its wood.
const LIMB: Material = Material {
    density: 0.04,
    ..WOOD
};
/// Rooted, it stands whatever its balance; cut, it has nothing to stand on.
const OAK_TIPPING: Tipping = Tipping {
    margin: 0.0,
    rise: 0.0,
    snap: None,
};
/// A felled oak breaks apart where it strikes the ground, never on its own remains, and
/// clears as other scenery's pieces do.
const OAK_DEBRIS: DebrisRules = DebrisRules {
    landing_speed: 1.5,
    firm_only: true,
    one_clock: false,
    life: 9.5,
    life_spread: 2.0,
    deadline: 18.0,
    seam: 0.0,
};
const OAK_DUST: f64 = 2.5;
/// The stump's tank-only footprint is this tall, so a hull cannot climb it.
const STUMP_HEIGHT: f64 = 1.6;
/// The least thickness (m from its axis) of a limb's branch hull: a twig's own tapers to
/// nothing, and a fallen part's collider is shrunk a little more, too thin to rest on.
const BRANCH_HULL: f64 = 0.12;
/// Points round a ring of the stump's footprint and the root plate.
const RING_POINTS: usize = 8;
/// The root plate it bears on while rooted, as a share of its cover's half width.
const ROOT_PLATE: f64 = 0.9;

/// The oaks laid out that something still holds, by place and size.
static OAKS: HeldLayouts<[u64; 5]> = HeldLayouts::new();

/// The structure of the tree at (x, z) sized w, d, h if it grows as one (an oak),
/// shared with whatever still holds it ([`HeldLayouts`]).
pub fn oak_structure(x: f64, z: f64, w: f64, d: f64, h: f64) -> Option<Arc<Structure>> {
    if tree_proportions(x, z, w, d, h).family != OAK {
        return None;
    }
    let key = [x, z, w, d, h].map(f64::to_bits);
    Some(OAKS.get_or_lay_out(key, || lay_out_oak(x, z, w, d, h)))
}

fn ring(radius: f64, y: f64) -> impl Iterator<Item = DVec3> {
    (0..RING_POINTS).map(move |k| {
        let angle = std::f64::consts::TAU * k as f64 / RING_POINTS as f64;
        DVec3::new(angle.cos() * radius, y, angle.sin() * radius)
    })
}

/// The points round rings `from..=to` of a branch, `least` (m) from its axis at least.
fn ring_points(
    oak: &OakLayout,
    branch: usize,
    rings: std::ops::RangeInclusive<usize>,
    least: f64,
) -> Vec<DVec3> {
    let branch = &oak.branches[branch];
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

/// [`oak_structure`], laid out.
fn lay_out_oak(x: f64, z: f64, w: f64, d: f64, h: f64) -> Structure {
    let oak = oak_layout(x, z, w, d, h);
    let proportions = &oak.proportions;
    let trunk_rings = oak.branches[0].sections.len() - 1;
    let mut parts = Vec::with_capacity(FIRST_LIMB + oak.limbs);

    let stump_ring = || ring(proportions.stump_radius, 0.0).chain(ring(proportions.stump_radius, STUMP_HEIGHT));
    let mut stump = Part::new(
        vec![PartShape::Hull(stump_ring().collect())],
        WOOD,
        Role::Tree(TreePart::Stump),
    );
    stump.footing = true;
    stump.grounded = true;
    stump.tank_only = true;
    stump.struck_as = Some(OAK_BUTT as u16);
    parts.push(stump);

    let mut butt = Part::new(
        vec![PartShape::Hull(ring_points(&oak, 0, 0..=BUTT_RINGS, 0.0))],
        BUTT,
        Role::Tree(TreePart::Butt),
    );
    butt.grounded = true;
    butt.anchor = true;
    butt.contacts = ring(w.min(d) / 2.0 * ROOT_PLATE, 0.0)
        .map(|p| DVec2::new(p.x, p.z))
        .collect();
    parts.push(butt);

    let mut trunk = Part::new(
        vec![PartShape::Hull(ring_points(&oak, 0, BUTT_RINGS..=trunk_rings, 0.0))],
        WOOD,
        Role::Tree(TreePart::Trunk),
    );
    // Cut, it bears on the stump at the cut and turns about it.
    trunk.load = true;
    trunk.contacts = vec![DVec2::ZERO];
    trunk.struck_as = Some(OAK_BUTT as u16);
    let sections = &oak.branches[0].sections;
    trunk.lie = Lie::Length(sections[trunk_rings].origin - sections[BUTT_RINGS].origin);
    parts.push(trunk);

    for limb in 0..oak.limbs {
        // One hull round its outer branches and their leaves: round its bare inner end as
        // well, it stood a fallen limb up on that end and the far edge of its leaves.
        let mut points: Vec<DVec3> = oak
            .branches
            .iter()
            .enumerate()
            .filter(|(_, branch)| branch.limb == Some(limb) && branch.level > 1)
            .flat_map(|(index, branch)| {
                ring_points(&oak, index, 0..=branch.sections.len() - 1, BRANCH_HULL)
            })
            .collect();
        points.extend(
            oak.sprigs
                .iter()
                .filter(|sprig| oak.branches[sprig.branch].limb == Some(limb))
                .flat_map(|sprig| sprig.corners),
        );
        let mut part = Part::new(
            vec![PartShape::Hull(points)],
            LIMB,
            Role::Tree(TreePart::Limb(limb)),
        );
        part.struck_as = Some(OAK_BUTT as u16);
        parts.push(part);
    }

    let mut structure = Structure::new(Build::Tree { x, z, w, d, h }, 0.0, parts, OAK_DEBRIS);
    structure.shares = false;
    structure.tipping = Some(OAK_TIPPING);
    structure.join(OAK_BUTT, OAK_TRUNK);
    for limb in FIRST_LIMB..structure.parts.len() {
        structure.join(OAK_TRUNK, limb);
    }
    for (index, part) in structure.parts.iter_mut().enumerate() {
        part.element = index as u16;
    }
    structure.whole = true;
    structure.stand_box = Some((DVec3::new(-w / 2.0, 0.0, -d / 2.0), DVec3::new(w / 2.0, h, d / 2.0)));
    structure.breachable = false;
    structure.health_part = Some(OAK_BUTT);
    structure.variant = proportions.seed;
    structure.levelling = Levelling {
        keys: PartSet::from_indices([OAK_BUTT]),
        share: 0.0,
        announce: true,
        dust: OAK_DUST,
    };
    structure
}
