//! Every tree but a full-detail oak drawn from its layout ([`crate::sim::tree_layout`]):
//! needle sprays round a conifer's bark boughs, sprig clumps round a broadleaf's limbs,
//! and a cover tree's fallen parts drawn on their own.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::DVec3;

use super::{
    BARK_COLORS, BIRCH_COLORS, Card, Crown, CrownShape, LEAF_COLORS, OAK_CELLS, OVATE_CELLS,
    RINGS_COLOR, TreeDetail, TreeGeometry, TreeShape, TreeSurface, add_mesh, batch, card_tint_of,
    geometry, into_atlas, limb,
};
use crate::geometry::math::quat_from_euler;
use crate::scene::{Material, Node};
use crate::sim::oak_layout::{OAK, TreePart};
use crate::sim::tree_layout::{Growth, LayoutDetail, TreeLayout, tree_layout};
use crate::sim::tree_proportions::TreeProportions;

/// Grow `tree`'s crown into `crown` step by step, each shedding bough a group of its
/// own when `boughs` (a standing tree's), drawing only the parts `keep` picks: a limb
/// whole, or the half of it its main limb snapped into (the outer half carries its
/// twigs and leaves). Returns the volume its foliage shades as, the whole crown's.
pub(super) fn add_tree_crown(
    crown: &mut Crown,
    tree: &TreeLayout,
    bark: &Arc<Material>,
    boughs: bool,
    shed: u32,
    keep: &dyn Fn(TreePart) -> bool,
) -> CrownShape {
    let TreeProportions { family, height, .. } = tree.proportions;
    let cells = if family == OAK {
        &OAK_CELLS
    } else {
        &OVATE_CELLS
    };
    // Whether a limb's twigs, sprays or sprigs are drawn: with its whole limb or with
    // the outer half it snapped into.
    let kept = |limb: Option<usize>| match limb {
        None => keep(TreePart::Trunk),
        Some(limb) => keep(TreePart::Limb(limb)) || keep(TreePart::LimbPiece { limb, outer: true }),
    };
    let mut groups: Vec<Option<usize>> = Vec::new();
    // Each shedding bough's damage stage, and whether it already fell on its own.
    let mut stages: Vec<u32> = Vec::new();
    let shed_off =
        |stages: &[u32], bough: Option<usize>| bough.is_some_and(|bough| stages[bough] <= shed);
    let (mut low, mut high) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
    for step in &tree.growth {
        match *step {
            Growth::Bough { stage } => {
                let group = if boughs {
                    crown.branch_parent(stage)
                } else {
                    None
                };
                groups.push(group);
                stages.push(stage);
            }
            Growth::Limb {
                from,
                to,
                radius,
                main,
                bough,
                limb: part,
            } => {
                let parent = bough.and_then(|bough| groups[bough]);
                let spans: &[(DVec3, DVec3)] = match part {
                    _ if shed_off(&stages, bough) => &[],
                    Some(limb) if main && !keep(TreePart::Limb(limb)) => {
                        let middle = from.lerp(to, 0.5);
                        let inner = keep(TreePart::LimbPiece { limb, outer: false });
                        let outer = keep(TreePart::LimbPiece { limb, outer: true });
                        match (inner, outer) {
                            (true, true) => &[(from, middle), (middle, to)],
                            (true, false) => &[(from, middle)],
                            (false, true) => &[(middle, to)],
                            (false, false) => &[],
                        }
                    }
                    _ if kept(part) => &[(from, to)],
                    _ => &[],
                };
                for &(from, to) in spans {
                    limb(crown.parent(parent), bark.clone(), from, to, radius, false);
                }
            }
            Growth::Spray {
                transform,
                tint,
                bough,
                limb,
            } => {
                if kept(limb) && !shed_off(&stages, bough) {
                    let parent = bough.and_then(|bough| groups[bough]);
                    crown.spray(parent, transform, card_tint_of(tint[0], tint[1]));
                }
            }
            Growth::Sprig { sprig, bough, limb } => {
                for corner in sprig.corners {
                    low = low.min(corner);
                    high = high.max(corner);
                }
                if kept(limb) && !shed_off(&stages, bough) {
                    let mut cell = cells[sprig.cell];
                    if sprig.mirrored {
                        cell.swap(0, 2);
                    }
                    let parent = bough.and_then(|bough| groups[bough]);
                    crown.card(
                        parent,
                        Card {
                            corners: sprig.corners,
                            cell,
                            tint: card_tint_of(sprig.tint[0], sprig.tint[1]),
                            clump: sprig.clump,
                        },
                    );
                }
            }
        }
    }
    match tree.cone {
        Some(cone) => CrownShape::Cone {
            lean: DVec3::new(tree.lean.0 / height, 0.0, tree.lean.1 / height),
            base: height * (cone.base - 0.08),
            top: height * 1.02,
            radius: cone.width * 1.05,
        },
        None => CrownShape::Round {
            center: (low + high) / 2.0,
            radii: (high - low) / 2.0,
        },
    }
}

/// The parts of the full-detail tree at `c` (not an oak: [`super::oak_pieces`]) that
/// `keep` picks, drawn where they stood about its ground centre and still (no sway),
/// without the boughs it shed at damage stages up to `shed`: what a felled tree's bodies
/// look like as they fall and lie (`structure_debris`).
pub fn tree_pieces(c: &TreeShape, keep: &dyn Fn(TreePart) -> bool, shed: u32) -> Node {
    let tree = tree_layout(c.x, c.z, c.w, c.d, c.h, LayoutDetail::Full);
    let TreeProportions {
        seed,
        family,
        twist,
        radius,
        stump_height,
        ..
    } = tree.proportions;
    let variant = seed % 24;
    let conifer = family < 3;
    let pale = family >= 4;
    let bark = if pale {
        super::surface(TreeSurface::Birch, BIRCH_COLORS[family as usize - 4], false)
    } else {
        super::surface(TreeSurface::Bark, BARK_COLORS[family as usize], false)
    };
    let foliage_surface = if conifer {
        TreeSurface::ConiferSpray
    } else {
        TreeSurface::LeafSprigs
    };
    let foliage = super::surface(foliage_surface, LEAF_COLORS[family as usize], false);
    let rings = super::surface(TreeSurface::Rings, RINGS_COLOR, false);
    let mut crown = Crown {
        node: Node::group("tree-pieces"),
        detail: TreeDetail::Full,
        cards: Vec::new(),
    };
    if keep(TreePart::Trunk) {
        let trunk = limb(
            &mut crown.node,
            bark.clone(),
            DVec3::new(0.0, tree.trunk_base, 0.0),
            tree.trunk_top,
            radius,
            false,
        );
        if let Some(drawable) = &mut trunk.drawable {
            drawable.mesh = geometry(if conifer {
                TreeGeometry::ConiferStem
            } else {
                TreeGeometry::BroadleafStem
            });
        }
        if !keep(TreePart::Butt) {
            // The splintered end the trunk tore from its butt with.
            let end = add_mesh(
                &mut crown.node,
                geometry(TreeGeometry::StumpCut((variant + 7) % 24)),
                rings.clone(),
                DVec3::new(0.0, tree.trunk_base + stump_height * 0.55, 0.0),
                DVec3::new(radius * 0.85, stump_height * 0.55, radius * 0.85),
            );
            end.rotation = quat_from_euler(PI, twist, 0.0);
        }
    }
    if keep(TreePart::Stump) {
        add_mesh(
            &mut crown.node,
            geometry(TreeGeometry::StumpBark(variant)),
            bark.clone(),
            DVec3::ZERO,
            DVec3::new(radius, stump_height, radius),
        )
        .set_rotation_euler(0.0, twist, 0.0);
        for root in &tree.roots {
            limb(
                &mut crown.node,
                bark.clone(),
                root.from,
                root.to,
                root.radius,
                true,
            );
        }
    }
    let shape = add_tree_crown(&mut crown, &tree, &bark, false, shed, keep);
    crown.grow_foliage(&foliage, &shape, [1.0; 3]);
    let mut node = crown.node;
    batch(&mut node);
    into_atlas(&mut node, false);
    node
}
