//! The skeleton of a conifer, and of a background broadleaf (a full-detail one grows
//! limb by limb: [`super::grown_layout`]): its stump's roots, its trunk, and its crown
//! grown step by step (shedding boughs, bark limbs, needle sprays, leaf sprigs) in the
//! order its model draws them.
//!
//! The model draws it (`models::tree_models`) and the simulation builds a cover
//! conifer's parts from it (`tree_structure`), so what falls is what is drawn: its
//! boughs gather into a few limbs by side and height. It draws on the tree's own stream
//! after [`tree_proportions`], never on the match's.

use std::f64::consts::PI;

use glam::{DMat4, DVec3};

use super::grown_layout::TreeRoot;
use super::math::Random;
use super::tree_proportions::{TreeProportions, tree_proportions};
use crate::geometry::math::{quat_from_euler, quat_from_euler_yxz, quat_rotate_z};

/// How much of a tree is grown: a cover tree, background woods, or far woods.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutDetail {
    Full,
    Background,
    Distant,
}

/// Successive whorls and lobes turn by this angle (radians).
pub const GOLDEN_ANGLE: f64 = 2.39996;
/// A conifer's stem tapers to this share of its radius at its top, a broadleaf's to
/// [`BROADLEAF_STEM_TOP`] where it divides into its limbs.
pub const CONIFER_STEM_TOP: f64 = 0.025;
pub const BROADLEAF_STEM_TOP: f64 = 0.35;

/// A tree's stem radius `along` it (0 where it leaves the stump, 1 at its top).
pub fn stem_radius(proportions: &TreeProportions, along: f64) -> f64 {
    let top = if proportions.family < 3 {
        CONIFER_STEM_TOP
    } else {
        BROADLEAF_STEM_TOP
    };
    proportions.radius * (1.0 - along * (1.0 - top))
}

/// A conifer's boughs gather into limbs by side: this many round the trunk.
const CONIFER_SIDES: usize = 4;
/// And by height: the whorls with bark boughs split into this many bands.
const CONIFER_BANDS: usize = 2;

/// A broadleaf sprig card: its corners counter-clockwise from the bottom left, which of
/// its family's two sprigs it shows and whether mirrored, its two colour draws
/// (lightness, warmth) and the clump its shading bulges from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sprig {
    pub corners: [DVec3; 4],
    pub cell: usize,
    pub mirrored: bool,
    pub tint: [f64; 2],
    pub clump: DVec3,
}

/// One step of a crown's growth, in the order the model draws it. `bough` is the
/// shedding bough it hangs in (by the order they began), none for the crown itself;
/// `limb` the cover tree's limb it belongs to, none for the trunk.
#[derive(Clone, Debug, PartialEq)]
pub enum Growth {
    /// A shedding bough begins, dropped at damage `stage`.
    Bough { stage: u32 },
    /// A bark limb from `from` to `to`.
    Limb {
        from: DVec3,
        to: DVec3,
        radius: f64,
        bough: Option<usize>,
        limb: Option<usize>,
    },
    /// A needle spray of crossed cards: its local Z runs from the branch along the
    /// spray, X across it, and `transform` places it; its two colour draws.
    Spray {
        transform: DMat4,
        tint: [f64; 2],
        bough: Option<usize>,
        limb: Option<usize>,
    },
    Sprig {
        sprig: Sprig,
        bough: Option<usize>,
        limb: Option<usize>,
    },
}

impl Growth {
    /// The cover tree's limb it belongs to; none for the trunk or a bough's start.
    pub fn limb(&self) -> Option<usize> {
        match *self {
            Growth::Bough { .. } => None,
            Growth::Limb { limb, .. } | Growth::Spray { limb, .. } | Growth::Sprig { limb, .. } => {
                limb
            }
        }
    }
}

/// A conifer's crown volume: from `base` up, `width` across at its base.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cone {
    pub base: f64,
    pub width: f64,
}

/// A tree's skeleton about its ground centre.
#[derive(Clone, Debug)]
pub struct TreeLayout {
    pub proportions: TreeProportions,
    /// Full detail only: buttress roots.
    pub roots: Vec<TreeRoot>,
    /// The trunk's lean at its top.
    pub lean: (f64, f64),
    /// Where the trunk leaves the stump, and its top.
    pub trunk_base: f64,
    pub trunk_top: DVec3,
    pub growth: Vec<Growth>,
    /// A conifer's crown volume; none for a broadleaf, whose cards bound it.
    pub cone: Option<Cone>,
    /// How many limbs a cover tree's crown has.
    pub limbs: usize,
    /// The stream after the layout's draws: a far conifer draws its solid crown from it
    /// (`models::tree_models`), which the layout does not grow.
    pub rest: Random,
}

/// The skeleton of the tree at (x, z) sized w, d, h at `detail`: its roots (full detail),
/// its lean, then its crown, drawn in that order from the tree's own stream. A far
/// conifer's crown is left to its model.
pub fn tree_layout(x: f64, z: f64, w: f64, d: f64, h: f64, detail: LayoutDetail) -> TreeLayout {
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
    let full = detail == LayoutDetail::Full;
    let conifer = family < 3;
    let mut rng = proportions.rng.clone();
    let root_count = if full { 5 + seed % 3 } else { 0 };
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
    let mut grower = Grower {
        rng,
        detail,
        growth: Vec::new(),
        boughs: 0,
    };
    let cone = if conifer && detail == LayoutDetail::Distant {
        None
    } else if conifer {
        Some(grower.conifer(w, family, twist, height, radius, lean))
    } else {
        grower.broadleaf(w, d, family, twist, height, lean);
        None
    };
    let mut layout = TreeLayout {
        proportions: proportions.clone(),
        roots,
        lean,
        trunk_base: if full { stump_height * 0.88 } else { 0.0 },
        trunk_top: DVec3::new(lean.0, height * (if conifer { 0.98 } else { 0.78 }), lean.1),
        growth: grower.growth,
        cone,
        limbs: 0,
        rest: grower.rng,
    };
    if full && conifer {
        layout.limbs = group_conifer_boughs(&mut layout.growth);
    }
    layout
}

struct Grower {
    rng: Random,
    detail: LayoutDetail,
    growth: Vec<Growth>,
    boughs: usize,
}

impl Grower {
    /// A new shedding bough for damage `stage` (full detail only), or the crown itself.
    fn bough(&mut self, stage: u32) -> Option<usize> {
        if self.detail != LayoutDetail::Full || stage == 0 {
            return None;
        }
        self.growth.push(Growth::Bough { stage });
        self.boughs += 1;
        Some(self.boughs - 1)
    }

    fn spray(&mut self, bough: Option<usize>, transform: DMat4) {
        let tint = [self.rng.range(0.82, 1.12), self.rng.range(-0.05, 0.07)];
        self.growth.push(Growth::Spray {
            transform,
            tint,
            bough,
            limb: None,
        });
    }

    /// Whorls of needle sprays round the leaning trunk, a bark bough inside each spray
    /// of the lower whorls at full detail, and a leader at the top.
    fn conifer(
        &mut self,
        w: f64,
        family: u32,
        twist: f64,
        height: f64,
        radius: f64,
        (lean_x, lean_z): (f64, f64),
    ) -> Cone {
        let full = self.detail == LayoutDetail::Full;
        let tiers = match self.detail {
            LayoutDetail::Full => 8,
            LayoutDetail::Background => 6,
            LayoutDetail::Distant => 5,
        };
        let arms = 6;
        // Pines carry a looser, higher crown; spruce and fir retain their lower boughs.
        let base = match family {
            0 => 0.36,
            1 => 0.17,
            _ => 0.23,
        };
        // Background forests keep their boughs lower and fuller, so the hills read
        // as dense woods rather than rows of bare poles.
        let (base, fullness) = match self.detail {
            LayoutDetail::Full => (base, 1.0),
            LayoutDetail::Background => (base * 0.6, 1.12),
            LayoutDetail::Distant => (base * 0.45, 1.22),
        };
        let width = w * (if family == 2 { 0.44 } else { 0.5 }) * fullness;
        for i in 0..tiers {
            let t = f64::from(i) / f64::from(tiers - 1);
            let y = height * (base + t * (0.87 - base));
            let span = width * (1.0 - t * 0.76);
            for j in 0..arms {
                let angle = twist
                    + f64::from(i) * GOLDEN_ANGLE
                    + (f64::from(j) * PI * 2.0) / f64::from(arms)
                    + self.rng.range(-0.24, 0.24);
                let reach = span * self.rng.range(0.78, 1.16);
                let start = DVec3::new(
                    lean_x * (base + t * (1.0 - base)),
                    y + self.rng.range(-0.05, 0.05) * height,
                    lean_z * (base + t * (1.0 - base)),
                );
                // An upright inner shoot fills the crown between whorls.
                if j == arms - 1 {
                    let transform = DMat4::from_scale_rotation_translation(
                        DVec3::new(span * 0.85, span * 0.85, height * (0.27 - t * 0.1)),
                        quat_rotate_z(quat_from_euler(-PI / 2.0, 0.0, 0.0), angle),
                        DVec3::new(start.x, start.y - height * 0.06, start.z),
                    );
                    self.spray(None, transform);
                    continue;
                }
                let stage = match (i, j) {
                    (1, 0) | (2, 3) => 1,
                    (0, 2) | (3, 1) => 2,
                    _ => 0,
                };
                let bough = self.bough(stage);
                let rise = reach
                    * match family {
                        0 => 0.24,
                        1 => -0.16,
                        _ => 0.06,
                    };
                if full && i < tiers - 2 {
                    // The limb stays inside its spray.
                    let end = DVec3::new(
                        start.x + angle.sin() * reach * 0.66,
                        start.y + rise * 0.7,
                        start.z + angle.cos() * reach * 0.66,
                    );
                    self.growth.push(Growth::Limb {
                        from: start,
                        to: end,
                        radius: radius * (0.13 - t * 0.08),
                        bough,
                        limb: None,
                    });
                }
                let transform = DMat4::from_scale_rotation_translation(
                    DVec3::new(
                        reach * (if family == 0 { 0.95 } else { 0.85 }),
                        height * (0.25 - t * 0.12),
                        reach * 1.1,
                    ),
                    quat_from_euler_yxz(-rise.atan2(reach), angle, self.rng.range(-0.2, 0.2)),
                    start,
                );
                self.spray(bough, transform);
            }
        }
        let leader = DMat4::from_scale_rotation_translation(
            DVec3::new(w * 0.2, w * 0.2, height * 0.2),
            quat_rotate_z(quat_from_euler(-PI / 2.0, 0.0, 0.0), twist),
            DVec3::new(lean_x * 0.9, height * 0.82, lean_z * 0.9),
        );
        self.spray(None, leader);
        Cone { base, width }
    }

    /// Lobes of sprig clumps up a background broadleaf's crown.
    #[allow(clippy::too_many_arguments)]
    fn broadleaf(
        &mut self,
        w: f64,
        d: f64,
        family: u32,
        twist: f64,
        height: f64,
        (lean_x, lean_z): (f64, f64),
    ) {
        let full = self.detail == LayoutDetail::Full;
        let count = 7;
        let oak = family == 3;
        // Oaks spread wide from a short bole; birch and aspen carry a taller oval crown
        // from a third of their height.
        let (spread_at, low_lobe, crown_rise) = if oak {
            (0.3, 0.42, 0.42)
        } else {
            (0.25, 0.38, 0.47)
        };
        // Background woods carry fuller crowns lower on the trunk.
        let (low_lobe, fullness) = if full {
            (low_lobe, 1.0)
        } else {
            (low_lobe - 0.12, 1.18)
        };
        for i in 0..count {
            let a = twist + f64::from(i) * GOLDEN_ANGLE;
            let t = f64::from(i) / f64::from(count - 1);
            let spread = (1.0 - t * 0.6) * spread_at;
            let center = DVec3::new(
                lean_x + a.sin() * w * spread,
                height * (low_lobe + t * crown_rise),
                lean_z + a.cos() * d * spread,
            );
            let size = w * (if oak { 0.3 } else { 0.27 }) * self.rng.range(0.84, 1.09) * fullness;
            // Overlapping clumps of sprigs give the crown an irregular, branching silhouette.
            let clumps = match self.detail {
                LayoutDetail::Full => 4,
                LayoutDetail::Background => 3,
                LayoutDetail::Distant => 2,
            };
            for j in 0..clumps {
                let angle = a + f64::from(j) * GOLDEN_ANGLE;
                let reach = if j == 0 { 0.0 } else { size * 0.55 };
                let tip = center
                    + DVec3::new(
                        angle.sin() * reach,
                        self.rng.range(-0.25, 0.35) * size,
                        angle.cos() * reach,
                    );
                // Damage sheds small outer twigs, never an entire section of the crown.
                let bough = if j == clumps - 1 && i < 4 {
                    self.bough(if i < 2 { 1 } else { 2 })
                } else {
                    None
                };
                let radii = DVec3::new(
                    size * self.rng.range(0.48, 0.66),
                    size * self.rng.range(0.55, 0.85),
                    size * self.rng.range(0.45, 0.65),
                );
                let cards = match self.detail {
                    LayoutDetail::Full => 9,
                    LayoutDetail::Background => 5,
                    LayoutDetail::Distant => 5,
                };
                let phase = self.rng.range(0.0, PI * 2.0);
                let card_size = size * if oak { 1.35 } else { 1.28 };
                for k in 0..cards {
                    let sprig = self.sprig(tip, radii, k, cards, phase, card_size);
                    self.growth.push(Growth::Sprig {
                        sprig,
                        bough,
                        limb: None,
                    });
                }
            }
        }
    }

    /// A sprig card in a clump at `tip` with ellipsoid `radii`: the `k`th of `count`
    /// spread over the clump from `phase`, its twig toward the inside and its leaves
    /// reaching out and up, `size` metres across.
    fn sprig(
        &mut self,
        tip: DVec3,
        radii: DVec3,
        k: u32,
        count: u32,
        phase: f64,
        size: f64,
    ) -> Sprig {
        let rng = &mut self.rng;
        let y = 1.0 - (f64::from(k) + 0.5) / f64::from(count) * 1.7;
        let ring = (1.0 - y * y).max(0.0).sqrt();
        let theta = phase + f64::from(k) * GOLDEN_ANGLE;
        let direction = DVec3::new(theta.cos() * ring, y, theta.sin() * ring);
        let center = tip + direction * radii * rng.range(0.3, 0.75);
        let outward = (direction + DVec3::Y * 0.3).normalize();
        let wander = DVec3::new(
            rng.range(-1.0, 1.0),
            rng.range(-1.0, 1.0),
            rng.range(-1.0, 1.0),
        );
        // Sprigs face out of the clump like shingles, so the crown shows leaves rather
        // than card edges, and grow up and outward from a twig inside it.
        let face = (outward + wander * 0.4).normalize_or(outward);
        let rise = outward * 0.6 + DVec3::Y + wander * 0.3;
        let up = (rise - face * rise.dot(face)).normalize_or(face.any_orthonormal_vector());
        let across = up.cross(face);
        let size = size * rng.range(0.88, 1.18);
        // The leafy mass sits a little below the card's middle.
        let bottom = center - up * size * 0.42;
        let half = across * size * 0.5;
        let cell = usize::from(rng.next() < 0.5);
        let mirrored = rng.next() < 0.5;
        let tint = [rng.range(0.82, 1.12), rng.range(-0.05, 0.07)];
        Sprig {
            corners: [
                bottom - half,
                bottom + half,
                bottom + half + up * size,
                bottom - half + up * size,
            ],
            cell,
            mirrored,
            tint,
            clump: tip,
        }
    }
}

/// Gather a full-detail conifer's bark boughs, with their sprays, into limbs by the
/// side of the trunk they grow from and by height; the sprays with no bough (the inner
/// shoots, the top whorls, the leader) stay with the trunk. Returns how many limbs.
fn group_conifer_boughs(growth: &mut [Growth]) -> usize {
    let lowest = growth.iter().filter_map(|step| match step {
        Growth::Limb { from, .. } => Some(from.y),
        _ => None,
    });
    let (low, high) = lowest.fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), y| {
        (low.min(y), high.max(y))
    });
    let mut limbs: Vec<usize> = Vec::new();
    let mut last = None;
    for step in growth.iter_mut() {
        match step {
            Growth::Limb { from, to, limb, .. } => {
                let heading = (to.z - from.z).atan2(to.x - from.x).rem_euclid(2.0 * PI);
                let side =
                    ((heading / (2.0 * PI) * CONIFER_SIDES as f64) as usize).min(CONIFER_SIDES - 1);
                let band = (((from.y - low) / (high - low).max(1e-9) * CONIFER_BANDS as f64)
                    as usize)
                    .min(CONIFER_BANDS - 1);
                let key = band * CONIFER_SIDES + side;
                let index = match limbs.iter().position(|&known| known == key) {
                    Some(index) => index,
                    None => {
                        limbs.push(key);
                        limbs.len() - 1
                    }
                };
                *limb = Some(index);
                last = Some(index);
            }
            // A bough's spray follows its bark bough; a spray with none is the trunk's.
            Growth::Spray { limb, .. } => *limb = last.take(),
            _ => {}
        }
    }
    limbs.len()
}
