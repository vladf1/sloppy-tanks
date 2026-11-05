//! A structural cottage's model: the cottage's own walls, roof and chimney cut into
//! its parts ([`HouseStructure`]), drawn from the parts that stand (or, for a fallen
//! body, from the parts that fell together). Each part is its cells of the model
//! clipped from the part of the cottage they lie in; where a neighbour is gone the
//! cut edge is torn timber bristling with splinters (a wall's siding, sheathing and
//! lining, a roof's covering and deck), never a clean cut. The attic floor and the
//! partition, which the cottage's model does not draw, are plain lined boards.
//!
//! A cottage whose parts all stand is drawn uncut. Everything is batched, one draw per
//! material.

use glam::{DMat4, DVec2, DVec3};

use super::batching::batch;
use super::building_kit::Surface;
use super::house_collapse::{ShardClip, clip_mesh, clip_meshes, roof_breaks, wall_breaks};
use super::house_model::{CottagePieces, cottage_pieces, cottage_scale, cottage_shingles, paint_cottage};
use super::tank_kit::Kit;
use crate::scene::Node;
use crate::sim::house_fracture::{EdgeKind, ShardRegion, Side};
use crate::sim::house_layout::{HouseDesign, PLINTH, WALL_DEPTH};
use crate::sim::house_structure::{
    HouseStructure, PartCell, PartKind, PartSet, part_outline, part_plane,
};
use crate::sim::math::Random;

/// Cells reach this far or further past their part to its outer edges.
const OUTER: f64 = 25.0;
/// Two cell edges this close lie on one line.
const ON_LINE: f64 = 1e-6;
/// An attic floor's boards over its plastered ceiling (m).
const FLOOR_BOARDS: f64 = 0.04;

/// The standing cottage for a cottage cover box, drawn from its parts not in
/// `broken`, with its plinth and yard.
pub fn structural_house(group: &mut Node, x: f64, z: f64, w: f64, d: f64, h: f64, broken: &PartSet) {
    let design = HouseDesign::of(x, z, w, d, h);
    let layout = HouseStructure::new(&design);
    let pieces = cottage_pieces(&design, x, z);
    let standing = PartSet::from_indices(
        (0..layout.parts.len()).filter(|&index| !broken.contains(index)),
    );
    let mut cottage = parts_node(&layout, &pieces, &standing);
    // The ground floor's boards over the plinth, which a gutted cottage shows.
    let mut floor = Kit::new();
    floor.block(
        Surface::Wood,
        DVec3::new(design.w - 2.0 * WALL_DEPTH, FLOOR_BOARDS, design.d - 2.0 * WALL_DEPTH),
        DMat4::from_translation(DVec3::new(0.0, PLINTH + FLOOR_BOARDS / 2.0, 0.0)),
    );
    paint_cottage(&mut cottage, floor.finish_scaled(cottage_scale(&design)), &design, &pieces.palette);
    flatten_into(&mut cottage, pieces.ground);
    // One draw per finish for the whole cottage, its plinth and yard with it.
    batch(&mut cottage);
    cottage.set_rotation_euler(0.0, design.yaw(), 0.0);
    group.children.push(cottage);
}

/// The parts `parts` of the cottage for a cover box, fallen together: drawn about the
/// cottage's ground centre, turned into the world as the cottage stood.
pub fn house_debris(x: f64, z: f64, w: f64, d: f64, h: f64, parts: &PartSet) -> Node {
    let design = HouseDesign::of(x, z, w, d, h);
    let layout = HouseStructure::new(&design);
    let pieces = cottage_pieces(&design, x, z);
    let mut node = parts_node(&layout, &pieces, parts);
    batch(&mut node);
    node.set_rotation_euler(0.0, design.yaw(), 0.0);
    let mut root = Node::group("house-debris");
    root.children.push(node);
    root
}

/// The model of `draw`'s parts in the build frame, to batch.
fn parts_node(layout: &HouseStructure, pieces: &CottagePieces, draw: &PartSet) -> Node {
    let design = &layout.design;
    let mut group = Node::group("");
    for piece in &pieces.parts {
        let cells: Vec<(usize, PartCell)> = layout
            .parts
            .iter()
            .enumerate()
            .flat_map(|(index, part)| {
                part.cells
                    .iter()
                    .filter(|cell| cell.part == piece.part)
                    .map(move |cell| (index, *cell))
            })
            .collect();
        let drawn: Vec<(usize, PartCell)> = cells
            .iter()
            .copied()
            .filter(|(index, _)| draw.contains(*index))
            .collect();
        if drawn.is_empty() {
            continue;
        }
        if drawn.len() == cells.len() {
            // All of it: the part as built.
            paint_cottage(&mut group, piece.meshes.clone(), design, &pieces.palette);
            if let Some(shingles) = &piece.shingles {
                group.children.push(cottage_shingles(shingles.clone(), &pieces.palette));
            }
            group.children.extend(piece.nodes.iter().cloned());
            continue;
        }
        let plane = part_plane(design, piece.part);
        let outline = part_outline(design, piece.part);
        for &(index, cell) in &drawn {
            let region = ShardRegion {
                cells: vec![cell_sides(&cell, &drawn)],
            };
            let clip = ShardClip::new(&plane, &region);
            paint_cottage(
                &mut group,
                clip_meshes(&piece.meshes, &clip),
                design,
                &pieces.palette,
            );
            if let Some(shingles) = &piece.shingles
                && let Some(mesh) = clip_mesh(shingles, &clip)
            {
                group.children.push(cottage_shingles(mesh, &pieces.palette));
            }
            group.children.extend(
                piece
                    .nodes
                    .iter()
                    .filter(|node| region.contains(plane.project(node.position)))
                    .cloned(),
            );
            // Torn timber along the edges where its neighbours are gone.
            let mut rng = Random::new(f64::from(design.variant) * 7.0 + index as f64 * 13.0 + 3.0);
            let mut torn = Kit::new();
            if let Some(wall) = piece.part.wall_index() {
                wall_breaks(&mut torn, &plane, &outline, &region, &pieces.openings[wall], &mut rng);
            } else if piece.part.is_roof() {
                roof_breaks(&mut torn, &plane, &outline, &region, &pieces.roof, &mut rng);
            }
            paint_cottage(&mut group, torn.finish_scaled(cottage_scale(design)), design, &pieces.palette);
        }
    }
    let mut inside = Kit::new();
    for index in draw.iter() {
        let Some(part) = layout.parts.get(index) else {
            continue;
        };
        let size = part.max - part.min;
        let middle = part.center();
        match part.kind {
            // Boards over a plastered ceiling.
            PartKind::Floor => {
                let boards = DVec3::new(size.x, FLOOR_BOARDS, size.z);
                inside.block(
                    Surface::Wood,
                    boards,
                    DMat4::from_translation(DVec3::new(middle.x, part.max.y - FLOOR_BOARDS / 2.0, middle.z)),
                );
                let ceiling = DVec3::new(size.x, size.y - FLOOR_BOARDS, size.z);
                inside.block(
                    Surface::Lining,
                    ceiling,
                    DMat4::from_translation(DVec3::new(middle.x, part.min.y + ceiling.y / 2.0, middle.z)),
                );
            }
            PartKind::Partition => {
                inside.block(Surface::Lining, size, DMat4::from_translation(middle));
            }
            _ => {}
        }
    }
    paint_cottage(&mut group, inside.finish_scaled(cottage_scale(design)), design, &pieces.palette);
    group
}

/// `node`'s children into `group`, carrying its transform onto them: the yard's
/// props join the cottage's draws.
fn flatten_into(group: &mut Node, node: Node) {
    let matrix = node.local_matrix();
    for mut child in node.children {
        let (scale, rotation, translation) =
            (matrix * child.local_matrix()).to_scale_rotation_translation();
        child.position = translation;
        child.rotation = rotation;
        child.scale = scale;
        group.children.push(child);
    }
}

/// A drawn cell's sides in its part's plane: an edge where a drawn neighbour carries
/// on is no edge, one along a gone neighbour is broken (torn timber), and the part's
/// outer edges are left to the model.
fn cell_sides(cell: &PartCell, drawn: &[(usize, PartCell)]) -> Vec<Side> {
    let (min, max) = (cell.min, cell.max);
    let inside = (min + max) / 2.0;
    let inside = DVec2::new(
        inside.x.clamp(-OUTER, OUTER),
        inside.y.clamp(-OUTER, OUTER),
    );
    let mut sides = Vec::new();
    // (the edge's line, which axis it crosses, the span along it)
    let edges = [
        (min.x, 0, (min.y, max.y), -1.0),
        (max.x, 0, (min.y, max.y), 1.0),
        (min.y, 1, (min.x, max.x), -1.0),
        (max.y, 1, (min.x, max.x), 1.0),
    ];
    for (at, axis, (from, to), out) in edges {
        if at.abs() >= OUTER {
            continue;
        }
        // How much of the edge drawn neighbours across it cover.
        let covered: f64 = drawn
            .iter()
            .filter(|(_, other)| other != cell)
            .filter(|(_, other)| {
                let near = if out < 0.0 { other.max[axis] } else { other.min[axis] };
                (near - at).abs() < ON_LINE
            })
            .map(|(_, other)| {
                let along = 1 - axis;
                (to.min(other.max[along]) - from.max(other.min[along])).max(0.0)
            })
            .sum();
        let (span_from, span_to) = (from.max(-OUTER), to.min(OUTER));
        let kind = if covered >= (span_to - span_from) - ON_LINE {
            EdgeKind::Band
        } else {
            EdgeKind::Broken
        };
        let (a, b) = if axis == 0 {
            (DVec2::new(at, 0.0), DVec2::new(at, 1.0))
        } else {
            (DVec2::new(0.0, at), DVec2::new(1.0, at))
        };
        sides.push(Side::through(a, b, inside, kind));
    }
    sides
}

