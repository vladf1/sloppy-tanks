//! Full-detail oaks drawn from the skeleton the simulation grows
//! ([`crate::sim::grown_layout`], after the branching of Daniel Greenheck's EZ-Tree
//! generator, MIT, `LICENSE-EZ-Tree.txt` beside this file): each branch a tube of its
//! rings, and sprig cards along the outermost branches, whose texture draws the twigs.
//!
//! The budget sits in the branching, not the leaves: a limb is a four-sided tube and an
//! outer branch a three-sided one, and a sprig card carries a whole cluster of leaves.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::DVec3;

use super::{
    Card, Crown, CrownShape, OAK_CELLS, OVATE_CELLS, RINGS_COLOR, TreeDetail, TreeGeometry,
    TreeLook, TreeShape, TreeSurface, add_mesh, batch, card_tint_of, geometry, into_atlas, limb,
    look_colors, shadowed, surface,
};
use crate::geometry::math::quat_from_euler;
use crate::geometry::{Mesh, narrow};
use crate::scene::{Material, Node};
use crate::sim::grown_layout::{
    BUTT_RINGS, GrownLayout, GrownSprig, LIMB_SNAP_RING, OAK, Section, TreePart, grown_layout,
};
use crate::sim::tree_proportions::TreeProportions;

/// The damage stage that sheds bough `index`: the first two at the first damage, the
/// next two at 35% health.
fn shed_stage(index: usize) -> u32 {
    if index < 2 { 1 } else { 2 }
}

/// A branch's tube: a ring of `sides` per section (the first corner repeated, so the
/// bark wraps once round it), bark running once along its length.
pub(super) fn tube_mesh(sections: &[Section], sides: u32) -> Mesh {
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

/// A sprig's card, cut from the oak's lobed sprigs or the birch's and aspen's ovate ones.
pub(super) fn sprig_card(sprig: &GrownSprig, family: u32) -> Card {
    let cells = if family == OAK {
        &OAK_CELLS
    } else {
        &OVATE_CELLS
    };
    let mut cell = cells[sprig.cell];
    if sprig.mirrored {
        cell.swap(0, 2);
    }
    Card {
        corners: sprig.corners,
        cell,
        tint: card_tint_of(sprig.tint[0], sprig.tint[1]),
        clump: sprig.clump,
    }
}

/// Grow a full-detail tree's trunk, limbs, branches and sprigs into `crown` from its
/// skeleton, each shedding bough a group of its own, leaving out the limbs `keep` does
/// not pick (snapped off it); returns the volume its foliage shades as.
pub(super) fn add_grown_crown(
    crown: &mut Crown,
    tree: &GrownLayout,
    bark: &Arc<Material>,
    keep: &dyn Fn(TreePart) -> bool,
) -> CrownShape {
    let kept = |branch: usize| {
        keep(
            tree.branches[branch]
                .limb
                .map_or(TreePart::Trunk, TreePart::Limb),
        )
    };
    // Each bough's group among the crown's children, made as its first branch grows.
    let mut boughs: Vec<Option<usize>> = Vec::new();
    let mut parents = Vec::with_capacity(tree.branches.len());
    for branch in &tree.branches {
        let parent = match branch.bough {
            Some((bough, true)) => {
                let group = crown.branch_parent(shed_stage(bough));
                boughs.push(group);
                group
            }
            Some((bough, false)) => boughs[bough],
            None => None,
        };
        parents.push(parent);
        if !kept(parents.len() - 1) {
            continue;
        }
        let tube = tube_mesh(&branch.sections, branch.sides);
        crown
            .parent(parent)
            .children
            .push(shadowed(Arc::new(tube), bark.clone()));
    }
    let (mut low, mut high) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
    for sprig in &tree.sprigs {
        for corner in sprig.corners {
            low = low.min(corner);
            high = high.max(corner);
        }
        if kept(sprig.branch) {
            crown.card(
                parents[sprig.branch],
                sprig_card(sprig, tree.proportions.family),
            );
        }
    }
    CrownShape::Round {
        center: (low + high) / 2.0,
        radii: (high - low) / 2.0,
    }
}

/// The parts of the full-detail tree at `c` that `keep` picks, drawn where they stood
/// about its ground centre and still (no sway), without the boughs it shed at damage
/// stages up to `shed`: what a felled tree's bodies look like as they fall and lie
/// (`structure_debris`). Its leaves shade as the whole crown did.
pub fn grown_pieces(
    c: &TreeShape,
    look: TreeLook,
    keep: &dyn Fn(TreePart) -> bool,
    shed: u32,
) -> Node {
    let tree = grown_layout(c.x, c.z, c.w, c.d, c.h);
    let TreeProportions {
        seed,
        family,
        twist,
        radius,
        stump_height,
        ..
    } = tree.proportions;
    let variant = seed % 24;
    let (bark_surface, bark_color, leaf_color, leaf_tint) = look_colors(look, family, seed);
    let bark = surface(bark_surface, bark_color, false);
    let foliage = surface(TreeSurface::LeafSprigs, leaf_color, false);
    let rings = surface(TreeSurface::Rings, RINGS_COLOR, false);
    let mut crown = Crown {
        node: Node::group("grown-pieces"),
        detail: TreeDetail::Full,
        cards: Vec::new(),
    };
    // Whether an outer branch (and its leaves) is drawn: with its whole limb, or with
    // the piece of it it grows on once the limb snapped.
    let kept = |branch: usize| {
        let shed_off = tree.branches[branch]
            .bough
            .is_some_and(|(bough, _)| shed_stage(bough) <= shed);
        !shed_off
            && match tree.limb_side(branch) {
                Some((limb, outer)) => {
                    keep(TreePart::Limb(limb)) || keep(TreePart::LimbPiece { limb, outer })
                }
                None => keep(
                    tree.branches[branch]
                        .limb
                        .map_or(TreePart::Trunk, TreePart::Limb),
                ),
            }
    };
    let tube = |crown: &mut Crown, sections: &[Section], sides: u32| {
        crown
            .node
            .children
            .push(shadowed(Arc::new(tube_mesh(sections, sides)), bark.clone()));
    };
    for (index, branch) in tree.branches.iter().enumerate() {
        if index == 0 {
            // The trunk's tube: its butt below the cut, the trunk above.
            let last = branch.sections.len() - 1;
            if keep(TreePart::Butt) {
                tube(&mut crown, &branch.sections[..=BUTT_RINGS], branch.sides);
            }
            if keep(TreePart::Trunk) {
                tube(
                    &mut crown,
                    &branch.sections[BUTT_RINGS..=last],
                    branch.sides,
                );
            }
        } else if let (1, Some(limb)) = (branch.level, branch.limb) {
            // A limb's main branch, whole or as the pieces it snapped into.
            let last = branch.sections.len() - 1;
            let snap = LIMB_SNAP_RING.min(last);
            if keep(TreePart::Limb(limb)) {
                tube(&mut crown, &branch.sections, branch.sides);
            } else {
                for (outer, rings) in [(false, 0..=snap), (true, snap..=last)] {
                    if keep(TreePart::LimbPiece { limb, outer }) {
                        tube(&mut crown, &branch.sections[rings], branch.sides);
                    }
                }
            }
        } else if kept(index) {
            tube(&mut crown, &branch.sections, branch.sides);
        }
    }
    if keep(TreePart::Trunk) && !keep(TreePart::Butt) {
        // The splintered end the trunk tore from its butt with.
        let cut = tree.branches[0].sections[BUTT_RINGS];
        let end = add_mesh(
            &mut crown.node,
            geometry(TreeGeometry::StumpCut((variant + 7) % 24)),
            rings.clone(),
            cut.origin + DVec3::Y * stump_height * 0.55,
            DVec3::new(cut.radius * 0.85, stump_height * 0.55, cut.radius * 0.85),
        );
        end.rotation = quat_from_euler(PI, twist, 0.0);
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
    let (mut low, mut high) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
    for sprig in &tree.sprigs {
        for corner in sprig.corners {
            low = low.min(corner);
            high = high.max(corner);
        }
        if kept(sprig.branch) {
            crown.card(None, sprig_card(sprig, tree.proportions.family));
        }
    }
    let shape = CrownShape::Round {
        center: (low + high) / 2.0,
        radii: (high - low) / 2.0,
    };
    crown.grow_foliage(&foliage, &shape, leaf_tint);
    let mut node = crown.node;
    batch(&mut node);
    into_atlas(&mut node, false);
    node
}
