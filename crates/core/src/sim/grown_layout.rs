//! A full-detail broadleaf's skeleton (an oak, a birch or an aspen), grown limb by limb
//! after the branching of Daniel Greenheck's EZ-Tree generator (MIT,
//! `models/tree_models/LICENSE-EZ-Tree.txt`): a tapering trunk divides into limbs and
//! the limbs into branches, each a tube of ring sections that tilts a little at every
//! section, and every branch also carries on from its parent's tip. Sprig cards grow
//! along the outermost branches. Each family grows by its own [`Habit`]: the oak a
//! gnarled, spreading crown on a short bole, the birch ascending limbs with drooping
//! twigs up a tall pale trunk, the aspen a narrow crown of short limbs.
//!
//! The simulation builds the tree's parts from it (`tree_structure`) and the model draws
//! it (`models::tree_models`), so what falls is what is drawn. It draws on the tree's own
//! stream after [`tree_proportions`] (its roots, lean and growth, in the order the model
//! drew them), never on the match's.

use std::f64::consts::PI;

use glam::{DQuat, DVec3};

use super::math::Random;
use super::tree_proportions::{TreeProportions, tree_proportions};

/// One level of a tree's structure: the trunk, its limbs, their branches.
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

/// How a family grows: its three levels and its sprigs.
struct Habit {
    levels: [Level; 3],
    /// The length of the branch carrying on from a tip, as a fraction of its level's:
    /// under 1 the crown rounds off; near 1 the trunk goes on up as a leader.
    carry_on: f64,
    /// Sprig cards along each outermost branch, beside the one at its tip.
    sprigs: u32,
    /// A sprig card's size as a fraction of the tree's height.
    sprig_size: f64,
}

/// The oak: a short, gnarled bole spreading into wide, crooked limbs.
const OAK_HABIT: Habit = Habit {
    levels: [
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
    ],
    carry_on: 0.7,
    sprigs: 7,
    sprig_size: 0.19,
};

/// The birch: a tall, nearly straight pale trunk going on up as a leader, limbs rising
/// steeply from its upper half, and their twigs drooping.
const BIRCH_HABIT: Habit = Habit {
    levels: [
        Level {
            children: 0,
            start: 0.0,
            angle: 0.0,
            length: 0.55,
            radius: 1.0,
            sections: 6,
            sides: 6,
            taper: 0.6,
            gnarl: 0.03,
            rise: 0.0,
        },
        Level {
            children: 7,
            start: 0.45,
            angle: 0.7,
            length: 0.3,
            radius: 0.55,
            sections: 3,
            sides: 4,
            taper: 0.6,
            gnarl: 0.12,
            rise: 0.03,
        },
        Level {
            children: 3,
            start: 0.3,
            angle: 0.9,
            length: 0.2,
            radius: 0.6,
            sections: 2,
            sides: 3,
            taper: 1.0,
            gnarl: 0.18,
            rise: -0.12,
        },
    ],
    carry_on: 0.85,
    sprigs: 6,
    sprig_size: 0.14,
};

/// The aspen: a tall, straight trunk under a narrow crown of short, upturned limbs.
const ASPEN_HABIT: Habit = Habit {
    levels: [
        Level {
            children: 0,
            start: 0.0,
            angle: 0.0,
            length: 0.6,
            radius: 1.0,
            sections: 6,
            sides: 6,
            taper: 0.6,
            gnarl: 0.025,
            rise: 0.0,
        },
        Level {
            children: 8,
            start: 0.4,
            angle: 0.55,
            length: 0.24,
            radius: 0.5,
            sections: 3,
            sides: 4,
            taper: 0.6,
            gnarl: 0.1,
            rise: 0.04,
        },
        Level {
            children: 3,
            start: 0.3,
            angle: 0.75,
            length: 0.15,
            radius: 0.6,
            sections: 2,
            sides: 3,
            taper: 1.0,
            gnarl: 0.16,
            rise: 0.02,
        },
    ],
    carry_on: 0.9,
    sprigs: 6,
    sprig_size: 0.13,
};

/// The least upward slope of a sprig card, so leaves never hang down from the crown
/// to a tank's height.
const SPRIG_LEAST_RISE: f64 = 0.3;
/// How far along an outer branch its sprigs start.
const SPRIG_START: f64 = 0.2;
/// Angle between a sprig and its branch.
const SPRIG_ANGLE: f64 = 0.75;
/// Outer branches the model sheds as the tree is hit: two at each damage stage.
const SHED_BRANCHES: u32 = 4;
/// The broadleaf families ([`TreeProportions::family`]); a full-detail one grows this way.
pub const OAK: u32 = 3;
pub const BIRCH: u32 = 4;
pub const ASPEN: u32 = 5;

/// Whether a full-detail tree of `family` grows limb by limb (a broadleaf).
pub fn grows(family: u32) -> bool {
    matches!(family, OAK | BIRCH | ASPEN)
}

fn habit(family: u32) -> &'static Habit {
    match family {
        BIRCH => &BIRCH_HABIT,
        ASPEN => &ASPEN_HABIT,
        _ => &OAK_HABIT,
    }
}
/// The trunk's rings below this one make its butt, where a felled tree breaks.
pub const BUTT_RINGS: usize = 1;

/// What a cover tree's part is (`tree_structure`), which its model draws on its own as
/// it falls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreePart {
    Stump,
    Butt,
    Trunk,
    /// The trunk's limb by its growth order ([`GrownBranch::limb`]).
    Limb(usize),
    /// A limb's piece once a shell snapped it where it lay: its main branch up to the
    /// snap ([`LIMB_SNAP_RING`]) with the twigs that grow there, or the rest.
    LimbPiece {
        limb: usize,
        outer: bool,
    },
}

/// A fallen limb's main branch snaps at this ring where a shell meets it.
pub const LIMB_SNAP_RING: usize = 2;

/// One ring of a grown branch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Section {
    pub origin: DVec3,
    pub orientation: DQuat,
    pub radius: f64,
}

/// A grown branch: its rings, sides, level (0 the trunk) and what it belongs to.
#[derive(Clone, Debug, PartialEq)]
pub struct GrownBranch {
    pub level: usize,
    pub sections: Vec<Section>,
    pub sides: u32,
    /// The trunk's limb it grows on (the trunk's children, in growth order); none for
    /// the trunk.
    pub limb: Option<usize>,
    /// The shedding bough it belongs to (in growth order), and whether it starts it.
    pub bough: Option<(usize, bool)>,
}

/// A buttress root: a tapered stem from `from` on the stump into the ground at `to`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeRoot {
    pub from: DVec3,
    pub to: DVec3,
    pub radius: f64,
}

/// A sprig card: its corners counter-clockwise from the bottom left, which of the two
/// oak sprigs it shows and whether mirrored, its two colour draws (lightness, warmth),
/// the clump its shading bulges from and the branch it grows on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrownSprig {
    pub branch: usize,
    pub corners: [DVec3; 4],
    pub cell: usize,
    pub mirrored: bool,
    pub tint: [f64; 2],
    pub clump: DVec3,
}

/// A full-detail oak's skeleton about its ground centre.
#[derive(Clone, Debug, PartialEq)]
pub struct GrownLayout {
    pub proportions: TreeProportions,
    pub roots: Vec<TreeRoot>,
    /// The trunk's lean at the crown's height.
    pub lean: (f64, f64),
    /// Where the trunk leaves the stump.
    pub trunk_base: f64,
    /// Every branch in growth order, the trunk first.
    pub branches: Vec<GrownBranch>,
    pub sprigs: Vec<GrownSprig>,
    /// How many limbs the trunk grows.
    pub limbs: usize,
}

impl GrownLayout {
    /// The limb an outer branch grows on, and whether past the ring its limb's main
    /// branch snaps at; none for the trunk and a limb's main branch, which spans both.
    pub fn limb_side(&self, branch: usize) -> Option<(usize, bool)> {
        let grown = &self.branches[branch];
        let limb = grown.limb?;
        if grown.level < 2 {
            return None;
        }
        let main = self
            .branches
            .iter()
            .find(|main| main.limb == Some(limb) && main.level == 1)?;
        let base = main.sections[0].origin;
        let snap = main.sections[LIMB_SNAP_RING.min(main.sections.len() - 1)].origin;
        let start = grown.sections[0].origin;
        Some((limb, start.distance(base) >= snap.distance(base)))
    }
}

/// The point, heading and radius `along` (0 to 1) a branch's sections.
pub fn along(sections: &[Section], along: f64) -> Section {
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

/// A branch about to grow: where, which way (its local +Y), how long and thick.
struct Bud {
    origin: DVec3,
    orientation: DQuat,
    length: f64,
    radius: f64,
    level: usize,
    limb: Option<usize>,
    bough: Option<usize>,
}

struct Grower<'a> {
    rng: &'a mut Random,
    habit: &'static Habit,
    height: f64,
    layout: GrownLayout,
    boughs: usize,
}

impl Grower<'_> {
    fn grow(&mut self, bud: Bud, starts_bough: bool) {
        let level = &self.habit.levels[bud.level];
        let sections = self.sections(&bud, level);
        let index = self.layout.branches.len();
        self.layout.branches.push(GrownBranch {
            level: bud.level,
            sections: sections.clone(),
            sides: level.sides,
            limb: bud.limb,
            bough: bud.bough.map(|bough| (bough, starts_bough)),
        });
        if bud.level + 1 < self.habit.levels.len() {
            self.branch_out(&bud, &sections);
        } else {
            self.sprigs(index, &sections);
        }
    }

    /// The rings of `bud`: each tapers, then the branch turns by its gnarl and rise
    /// before the next.
    fn sections(&mut self, bud: &Bud, level: &Level) -> Vec<Section> {
        let step = bud.length / f64::from(level.sections);
        let mut origin = bud.origin;
        let mut orientation = bud.orientation;
        let mut sections = Vec::with_capacity(level.sections as usize + 1);
        for i in 0..=level.sections {
            let t = f64::from(i) / f64::from(level.sections);
            sections.push(Section {
                origin,
                orientation,
                radius: bud.radius * (1.0 - level.taper * t),
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
    fn branch_out(&mut self, parent: &Bud, sections: &[Section]) {
        let child = &self.habit.levels[parent.level + 1];
        let tip = sections[sections.len() - 1];
        // The trunk's children are its limbs.
        let limb = |grower: &mut Self| match parent.level {
            0 => {
                grower.layout.limbs += 1;
                Some(grower.layout.limbs - 1)
            }
            _ => parent.limb,
        };
        let carry_on = limb(self);
        self.grow(
            Bud {
                origin: tip.origin,
                orientation: tip.orientation,
                length: self.height * child.length * self.habit.carry_on,
                radius: tip.radius,
                level: parent.level + 1,
                limb: carry_on,
                bough: parent.bough,
            },
            false,
        );
        let offset = self.rng.next();
        for i in 0..child.children {
            let from = along(sections, self.rng.range(child.start, 1.0));
            let radial = 2.0 * PI * (offset + f64::from(i) / f64::from(child.children));
            let orientation = from.orientation
                * DQuat::from_rotation_y(radial)
                * DQuat::from_rotation_x(child.angle);
            // Damage sheds an outer branch of the first limbs, never a whole limb.
            let outermost = parent.level + 2 == self.habit.levels.len();
            let sheds = outermost && i == 0 && self.boughs < SHED_BRANCHES as usize;
            let bough = if sheds {
                self.boughs += 1;
                Some(self.boughs - 1)
            } else {
                parent.bough
            };
            let limb = limb(self);
            let length = self.height * child.length * self.rng.range(0.8, 1.1);
            self.grow(
                Bud {
                    origin: from.origin,
                    orientation,
                    length,
                    radius: from.radius * child.radius,
                    level: parent.level + 1,
                    limb,
                    bough,
                },
                sheds,
            );
        }
    }

    /// Sprig cards round an outermost branch and one at its tip, each with its twig on
    /// the branch and its leaves reaching out along the sprig's direction, its face
    /// turned out of the crown so the crown shows leaves rather than card edges.
    fn sprigs(&mut self, branch: usize, sections: &[Section]) {
        let clump = along(sections, 0.6).origin;
        let offset = self.rng.next();
        let tip = sections[sections.len() - 1];
        let sprigs = self.habit.sprigs;
        for k in 0..=sprigs {
            let (origin, heading) = if k == sprigs {
                (tip.origin, tip.orientation * DVec3::Y)
            } else {
                let at = along(sections, self.rng.range(SPRIG_START, 1.0));
                let radial = 2.0 * PI * (offset + f64::from(k) / f64::from(sprigs));
                let orientation = at.orientation
                    * DQuat::from_rotation_y(radial)
                    * DQuat::from_rotation_x(SPRIG_ANGLE);
                (at.origin, orientation * DVec3::Y)
            };
            let sprig = self.sprig(branch, origin, heading, clump);
            self.layout.sprigs.push(sprig);
        }
    }

    fn sprig(&mut self, branch: usize, stem: DVec3, heading: DVec3, clump: DVec3) -> GrownSprig {
        let outward =
            (DVec3::new(stem.x, 0.0, stem.z).normalize_or(DVec3::X) + DVec3::Y * 0.3).normalize();
        // Leaves lean up whatever way the branch points.
        let up = DVec3::new(heading.x, heading.y.max(SPRIG_LEAST_RISE), heading.z)
            .normalize_or(DVec3::Y);
        let across = up.cross(outward).normalize_or(up.any_orthonormal_vector());
        let size = self.height * self.habit.sprig_size * self.rng.range(0.85, 1.15);
        // The sprig's drawn twig starts a little behind the branch point.
        let bottom = stem - up * size * 0.08;
        let half = across * size * 0.5;
        let cell = usize::from(self.rng.next() < 0.5);
        let mirrored = self.rng.next() < 0.5;
        let light = self.rng.range(0.82, 1.12);
        let warm = self.rng.range(-0.05, 0.07);
        GrownSprig {
            branch,
            corners: [
                bottom - half,
                bottom + half,
                bottom + half + up * size,
                bottom - half + up * size,
            ],
            cell,
            mirrored,
            tint: [light, warm],
            clump,
        }
    }
}

/// The skeleton of the full-detail oak at (x, z) sized w, d, h: its stump's roots, then
/// its trunk's lean, then its growth, drawn in that order from the tree's own stream.
pub fn grown_layout(x: f64, z: f64, w: f64, d: f64, h: f64) -> GrownLayout {
    let proportions = tree_proportions(x, z, w, d, h);
    let TreeProportions {
        seed,
        family,
        twist,
        height,
        radius,
        stump_height,
        ..
    } = proportions;
    let habit = habit(family);
    let mut rng = proportions.rng.clone();
    // Buttress roots flare from the stump and dive into the ground.
    let root_count = 5 + seed % 3;
    let roots = (0..root_count)
        .map(|i| {
            let angle =
                twist + (f64::from(i) * PI * 2.0) / f64::from(root_count) + rng.range(-0.2, 0.2);
            let reach = radius * rng.range(1.5, 2.1);
            let from = DVec3::new(
                angle.sin() * radius * 0.55,
                stump_height * rng.range(0.45, 0.6),
                angle.cos() * radius * 0.55,
            );
            let to = DVec3::new(angle.sin() * reach, -0.08, angle.cos() * reach);
            TreeRoot {
                from,
                to,
                radius: radius * rng.range(0.38, 0.52),
            }
        })
        .collect();
    let lean = (rng.range(-0.1, 0.1) * w, rng.range(-0.07, 0.07) * d);
    let trunk_base = stump_height * 0.88;
    let toward = DVec3::new(lean.0, height * 0.78 - trunk_base, lean.1).normalize();
    let orientation = DQuat::from_rotation_arc(DVec3::Y, toward) * DQuat::from_rotation_y(twist);
    let mut grower = Grower {
        rng: &mut rng,
        habit,
        height,
        layout: GrownLayout {
            proportions: proportions.clone(),
            roots,
            lean,
            trunk_base,
            branches: Vec::new(),
            sprigs: Vec::new(),
            limbs: 0,
        },
        boughs: 0,
    };
    grower.grow(
        Bud {
            origin: DVec3::new(0.0, trunk_base, 0.0),
            orientation,
            length: height * habit.levels[0].length,
            radius,
            level: 0,
            limb: None,
            bough: None,
        },
        false,
    );
    grower.layout
}
