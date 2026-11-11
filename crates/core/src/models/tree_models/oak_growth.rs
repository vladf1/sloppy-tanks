//! Full-detail oaks grown limb by limb, after the branching of Daniel Greenheck's
//! EZ-Tree generator (MIT, `LICENSE-EZ-Tree.txt` beside this file): a gnarled, tapering trunk divides into limbs and the limbs
//! into branches, each a tube of ring sections that tilts a little at every section,
//! and every branch also carries on from its parent's tip, so the crown keeps one
//! continuous structure instead of straight spokes from the trunk. Sprig cards grow
//! along the outermost branches; their texture draws the twigs, so no tube does.
//!
//! The budget sits in the branching, not the leaves: a limb is a four-sided tube and an
//! outer branch a three-sided one, and a sprig card carries a whole cluster of leaves.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::{DQuat, DVec3};

use super::{Card, Crown, CrownShape, OAK_CELLS, card_tint, shadowed};
use crate::geometry::{Mesh, narrow};
use crate::scene::Material;
use crate::sim::math::Random;

/// One level of the oak's structure: the trunk, its limbs, their branches.
struct Level {
    /// Children each branch of the level above grows, beside the one that carries on
    /// from its tip.
    children: u32,
    /// How far along the parent the children start (0 at its base, 1 at its tip).
    start: f64,
    /// Angle between a child and its parent.
    angle: f64,
    /// Length as a fraction of the tree's height.
    length: f64,
    /// Radius as a fraction of the parent's where the child grows from it.
    radius: f64,
    sections: u32,
    sides: u32,
    /// Fraction of the radius lost from base to tip (1 ends in a point).
    taper: f64,
    /// Largest random tilt at each section.
    gnarl: f64,
    /// Turn toward the sky at each section; negative droops.
    rise: f64,
}

const OAK_LEVELS: [Level; 3] = [
    Level {
        children: 0,
        start: 0.0,
        angle: 0.0,
        length: 0.42,
        radius: 1.0,
        sections: 5,
        sides: 6,
        taper: 0.55,
        gnarl: 0.06,
        rise: 0.0,
    },
    Level {
        children: 6,
        start: 0.55,
        angle: 1.05,
        length: 0.36,
        radius: 0.62,
        sections: 3,
        sides: 4,
        taper: 0.5,
        gnarl: 0.14,
        rise: 0.02,
    },
    Level {
        children: 4,
        start: 0.25,
        angle: 0.8,
        length: 0.22,
        radius: 0.6,
        sections: 2,
        sides: 3,
        taper: 1.0,
        gnarl: 0.2,
        rise: 0.0,
    },
];

/// The length of the branch carrying on from a tip, as a fraction of its level's:
/// shorter, so the crown rounds off instead of ending in a thin leader.
const CARRY_ON_LENGTH: f64 = 0.7;
/// The least upward slope of a sprig card, so leaves never hang down from the crown
/// to a tank's height.
const SPRIG_LEAST_RISE: f64 = 0.3;
/// Sprig cards along each outermost branch, beside the one at its tip.
const SPRIGS_PER_BRANCH: u32 = 7;
/// How far along an outer branch its sprigs start.
const SPRIG_START: f64 = 0.2;
/// Angle between a sprig and its branch.
const SPRIG_ANGLE: f64 = 0.75;
/// A sprig card's size as a fraction of the tree's height.
const SPRIG_SIZE: f64 = 0.19;
/// Outer branches that shed when the tree is hit: two at each damage stage.
const SHED_BRANCHES: u32 = 4;

/// A branch about to grow: where, which way (its local +Y), how long and thick.
struct Branch {
    origin: DVec3,
    orientation: DQuat,
    length: f64,
    radius: f64,
    level: usize,
    /// The shedding bough it belongs to, if any (see [`Crown::branch_parent`]).
    bough: Option<usize>,
}

/// One ring of a grown branch.
#[derive(Clone, Copy)]
struct Section {
    origin: DVec3,
    orientation: DQuat,
    radius: f64,
}

/// The point, heading and radius `along` (0 to 1) a branch's sections.
fn along(sections: &[Section], along: f64) -> Section {
    let span = (sections.len() - 1) as f64;
    let index = ((along * span).floor() as usize).min(sections.len() - 1);
    let (a, b) = (
        sections[index],
        sections[(index + 1).min(sections.len() - 1)],
    );
    let t = (along * span - index as f64).clamp(0.0, 1.0);
    Section {
        origin: a.origin.lerp(b.origin, t),
        orientation: a.orientation.slerp(b.orientation, t),
        radius: a.radius + (b.radius - a.radius) * t,
    }
}

/// The oak being grown: its crown, random stream and bark, and what it has grown.
struct Grower<'a> {
    crown: &'a mut Crown,
    rng: &'a mut Random,
    bark: &'a Arc<Material>,
    height: f64,
    shed: u32,
    low: DVec3,
    high: DVec3,
}

impl Grower<'_> {
    fn grow(&mut self, branch: Branch) {
        let level = &OAK_LEVELS[branch.level];
        let sections = self.sections(&branch, level);
        let tube = tube_mesh(&sections, level.sides);
        self.crown
            .parent(branch.bough)
            .children
            .push(shadowed(Arc::new(tube), self.bark.clone()));
        if branch.level + 1 < OAK_LEVELS.len() {
            self.branch_out(&branch, &sections);
        } else {
            self.sprigs(&branch, &sections);
        }
    }

    /// The rings of `branch`: each tapers, then the branch turns by its gnarl and
    /// rise before the next.
    fn sections(&mut self, branch: &Branch, level: &Level) -> Vec<Section> {
        let step = branch.length / f64::from(level.sections);
        let mut origin = branch.origin;
        let mut orientation = branch.orientation;
        let mut sections = Vec::with_capacity(level.sections as usize + 1);
        for i in 0..=level.sections {
            let t = f64::from(i) / f64::from(level.sections);
            sections.push(Section {
                origin,
                orientation,
                radius: branch.radius * (1.0 - level.taper * t),
            });
            origin += orientation * DVec3::Y * step;
            orientation = orientation
                * DQuat::from_rotation_x(self.rng.range(-level.gnarl, level.gnarl))
                * DQuat::from_rotation_z(self.rng.range(-level.gnarl, level.gnarl));
            let heading = orientation * DVec3::Y;
            let axis = heading.cross(DVec3::Y);
            if axis.length_squared() > 1e-12 {
                orientation = DQuat::from_axis_angle(axis.normalize(), level.rise) * orientation;
            }
        }
        sections
    }

    /// The next level: one branch carrying on from the tip, and the level's children
    /// spread round the parent from `start` to its tip.
    fn branch_out(&mut self, parent: &Branch, sections: &[Section]) {
        let child = &OAK_LEVELS[parent.level + 1];
        let tip = sections[sections.len() - 1];
        // A felled crown's limbs (the trunk's children) each break off on their own.
        let limb = parent.level == 0;
        let bough = if limb {
            self.crown.limb_parent().or(parent.bough)
        } else {
            parent.bough
        };
        self.grow(Branch {
            origin: tip.origin,
            orientation: tip.orientation,
            length: self.height * child.length * CARRY_ON_LENGTH,
            radius: tip.radius,
            level: parent.level + 1,
            bough,
        });
        let offset = self.rng.next();
        for i in 0..child.children {
            let from = along(sections, self.rng.range(child.start, 1.0));
            let radial = 2.0 * PI * (offset + f64::from(i) / f64::from(child.children));
            let orientation = from.orientation
                * DQuat::from_rotation_y(radial)
                * DQuat::from_rotation_x(child.angle);
            // Damage sheds an outer branch of the first limbs, never a whole limb.
            let outermost = parent.level + 2 == OAK_LEVELS.len();
            let bough = if outermost && i == 0 && self.shed < SHED_BRANCHES {
                self.shed += 1;
                self.crown
                    .branch_parent(if self.shed <= SHED_BRANCHES / 2 { 1 } else { 2 })
            } else if limb {
                self.crown.limb_parent().or(parent.bough)
            } else {
                parent.bough
            };
            let length = self.height * child.length * self.rng.range(0.8, 1.1);
            self.grow(Branch {
                origin: from.origin,
                orientation,
                length,
                radius: from.radius * child.radius,
                level: parent.level + 1,
                bough,
            });
        }
    }

    /// Sprig cards round an outermost branch and one at its tip, each with its twig on
    /// the branch and its leaves reaching out along the sprig's direction, its face
    /// turned out of the crown so the crown shows leaves rather than card edges.
    fn sprigs(&mut self, branch: &Branch, sections: &[Section]) {
        let clump = along(sections, 0.6).origin;
        let offset = self.rng.next();
        let tip = sections[sections.len() - 1];
        for k in 0..=SPRIGS_PER_BRANCH {
            let (origin, heading) = if k == SPRIGS_PER_BRANCH {
                (tip.origin, tip.orientation * DVec3::Y)
            } else {
                let at = along(sections, self.rng.range(SPRIG_START, 1.0));
                let radial = 2.0 * PI * (offset + f64::from(k) / f64::from(SPRIGS_PER_BRANCH));
                let orientation = at.orientation
                    * DQuat::from_rotation_y(radial)
                    * DQuat::from_rotation_x(SPRIG_ANGLE);
                (at.origin, orientation * DVec3::Y)
            };
            let card = self.sprig_card(origin, heading, clump);
            for corner in card.corners {
                self.low = self.low.min(corner);
                self.high = self.high.max(corner);
            }
            self.crown.card(branch.bough, card);
        }
    }

    fn sprig_card(&mut self, stem: DVec3, heading: DVec3, clump: DVec3) -> Card {
        let outward =
            (DVec3::new(stem.x, 0.0, stem.z).normalize_or(DVec3::X) + DVec3::Y * 0.3).normalize();
        // Leaves lean up whatever way the branch points.
        let up = DVec3::new(heading.x, heading.y.max(SPRIG_LEAST_RISE), heading.z)
            .normalize_or(DVec3::Y);
        let across = up.cross(outward).normalize_or(up.any_orthonormal_vector());
        let size = self.height * SPRIG_SIZE * self.rng.range(0.85, 1.15);
        // The sprig's drawn twig starts a little behind the branch point.
        let bottom = stem - up * size * 0.08;
        let half = across * size * 0.5;
        let mut cell = OAK_CELLS[usize::from(self.rng.next() < 0.5)];
        if self.rng.next() < 0.5 {
            cell.swap(0, 2);
        }
        Card {
            corners: [
                bottom - half,
                bottom + half,
                bottom + half + up * size,
                bottom - half + up * size,
            ],
            cell,
            tint: card_tint(self.rng),
            clump,
        }
    }
}

/// A branch's tube: a ring of `sides` per section (the first corner repeated, so the
/// bark wraps once round it), bark running once along its length.
fn tube_mesh(sections: &[Section], sides: u32) -> Mesh {
    let ring = sides + 1;
    let mut positions = Vec::with_capacity(sections.len() * ring as usize);
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut uvs = Vec::with_capacity(positions.capacity());
    let last = (sections.len() - 1) as f64;
    for (i, section) in sections.iter().enumerate() {
        for j in 0..ring {
            let angle = 2.0 * PI * f64::from(j % sides) / f64::from(sides);
            let radial = section.orientation * DVec3::new(angle.cos(), 0.0, angle.sin());
            positions.push(narrow(section.origin + radial * section.radius));
            normals.push(narrow(radial));
            uvs.push([
                (f64::from(j) / f64::from(sides)) as f32,
                (i as f64 / last) as f32,
            ]);
        }
    }
    let mut indices = Vec::with_capacity((sections.len() - 1) * sides as usize * 6);
    for i in 0..sections.len() as u32 - 1 {
        for j in 0..sides {
            let a = i * ring + j;
            let (b, c) = (a + 1, a + ring);
            indices.extend_from_slice(&[a, c, b, b, c, c + 1]);
        }
    }
    Mesh {
        positions,
        normals,
        uvs,
        indices: Some(indices),
        ..Mesh::default()
    }
}

/// Grow a full-detail oak's trunk, limbs, branches and sprigs into `crown` from the
/// top of its stump, the trunk leaning toward `lean` at the crown's height; returns the
/// volume its foliage shades as.
#[allow(clippy::too_many_arguments)]
pub(super) fn add_grown_oak_crown(
    crown: &mut Crown,
    rng: &mut Random,
    bark: &Arc<Material>,
    twist: f64,
    height: f64,
    radius: f64,
    trunk_base: f64,
    (lean_x, lean_z): (f64, f64),
) -> CrownShape {
    let lean = DVec3::new(lean_x, height * 0.78 - trunk_base, lean_z).normalize();
    let orientation = DQuat::from_rotation_arc(DVec3::Y, lean) * DQuat::from_rotation_y(twist);
    let mut grower = Grower {
        crown,
        rng,
        bark,
        height,
        shed: 0,
        low: DVec3::splat(f64::MAX),
        high: DVec3::splat(f64::MIN),
    };
    grower.grow(Branch {
        origin: DVec3::new(0.0, trunk_base, 0.0),
        orientation,
        length: height * OAK_LEVELS[0].length,
        radius,
        level: 0,
        bough: None,
    });
    CrownShape::Round {
        center: (grower.low + grower.high) / 2.0,
        radii: (grower.high - grower.low) / 2.0,
    }
}
