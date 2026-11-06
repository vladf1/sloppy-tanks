//! A structural cottage's model: the cottage's own walls, roof and chimney cut into
//! its parts ([`Structure`]), drawn from the parts that stand (or, for a fallen
//! body, from the parts that fell together). Each part is its cells of the model
//! clipped from the part of the cottage they lie in; where a neighbour is gone the
//! cut edge is torn timber bristling with splinters (a wall's siding, sheathing and
//! lining, a roof's covering and deck), never a clean cut. The attic floor and the
//! partition, which the cottage's model does not draw, are plain lined boards.
//!
//! A cottage whose parts all stand is drawn uncut. Everything is batched, one draw per
//! material.

use std::sync::Arc;

use glam::{DMat4, DVec2, DVec3};

use super::batching::batch;
use super::building_kit::Surface;
use super::house_collapse::{ShardClip, clip_mesh, clip_meshes, roof_breaks, wall_breaks};
use super::house_model::{standing_house, CottagePiece, CottagePieces, cottage_pieces, cottage_scale, cottage_shingles, paint_cottage, paint_cottage_whole};
use super::tank_kit::Kit;
use crate::scene::Node;
use crate::sim::house_fracture::{EdgeKind, ShardRegion, Side};
use crate::sim::house_layout::{HouseDesign, PLINTH, WALL_DEPTH};
use crate::sim::house_structure::{
    PartCell, PartKind, part_outline, part_plane, shared_house_structure,
};
use crate::sim::math::Random;
use crate::sim::structure::{PartSet, Structure};

/// Cells reach this far or further past their part to its outer edges.
const OUTER: f64 = 25.0;
/// Two cell edges this close lie on one line.
const ON_LINE: f64 = 1e-6;
/// An attic floor's boards over its plastered ceiling (m), and the ground floor's.
const FLOOR_BOARDS: f64 = 0.04;
/// Floorboards' width (m), and their seams' width and how proud of them they are
/// drawn, as are a partition's studs of its plaster.
const BOARD_WIDTH: f64 = 0.16;
const SEAM_WIDTH: f64 = 0.012;
const SEAM_PROUD: f64 = 0.003;
/// A partition's studs: their spacing and width (m).
const STUD_SPACING: f64 = 0.45;
const STUD_WIDTH: f64 = 0.045;
/// How far (m) the footing stands proud of the wall's face.
const PLINTH_LIP: f64 = 0.05;
/// A part's side this close (m) to the cottage's outline is on its face.
const ON_FACE: f64 = 0.01;

/// The standing cottage for a cottage cover box, drawn from its parts not in
/// `broken`, with its plinth and yard.
pub fn structural_house(group: &mut Node, x: f64, z: f64, w: f64, d: f64, h: f64, broken: &PartSet) {
    let layout = shared_house_structure(&HouseDesign::of(x, z, w, d, h));
    let mut build = HouseBuild::new(layout, x, z, broken);
    while !build.step() {}
    group.children.push(build.finish_cottage());
}

/// A cottage's model built a step at a time, so a page can spread a damaged cottage's
/// rebuild over frames: its pieces first, then one wall or roof slope a step, then its
/// rooms, plinth and yard.
pub struct HouseBuild {
    x: f64,
    z: f64,
    layout: Arc<Structure>,
    standing: PartSet,
    /// Whether a gap shows its rooms.
    open: bool,
    pieces: Option<CottagePieces>,
    next: usize,
    group: Node,
}

impl HouseBuild {
    /// The build of the cottage at `(x, z)` laid out as `layout` (its
    /// [`shared_house_structure`], which the caller keeps for the cottage rather than laying
    /// it out again for every rebuild), with its parts in `broken` gone.
    pub fn new(layout: Arc<Structure>, x: f64, z: f64, broken: &PartSet) -> Self {
        let standing = PartSet::from_indices(
            (0..layout.parts.len()).filter(|&index| !broken.contains(index)),
        );
        Self {
            x,
            z,
            layout,
            standing,
            // A whole cottage shows nothing of its rooms: they are drawn once a gap
            // opens.
            open: !broken.is_empty(),
            pieces: None,
            next: 0,
            group: Node::group(""),
        }
    }

    /// The cottage's layout the build draws.
    pub fn layout(&self) -> &Arc<Structure> {
        &self.layout
    }

    /// One step of the build; true once it is done.
    pub fn step(&mut self) -> bool {
        if !self.open {
            // A whole cottage is the standing model, built in one step.
            if self.next == 0 {
                self.group = standing_house(self.layout.design().expect("a cottage"), self.x, self.z);
                self.next = 1;
            }
            return true;
        }
        let Some(pieces) = &mut self.pieces else {
            self.pieces = Some(cottage_pieces(self.layout.design().expect("a cottage"), self.x, self.z, true));
            return false;
        };
        if self.next < pieces.parts.len() {
            piece_into(&mut self.group, &self.layout, pieces, &pieces.parts[self.next], &self.standing);
            // Each piece is drawn once: the model now holds what it kept of its meshes.
            let piece = &mut pieces.parts[self.next];
            piece.meshes = Vec::new();
            piece.shingles = None;
            piece.nodes = Vec::new();
            self.next += 1;
            return false;
        }
        true
    }

    /// The built cottage, turned into the world about its ground centre.
    fn finish_cottage(self) -> Node {
        if !self.open {
            return self.group;
        }
        let Self {
            layout,
            standing,
            pieces,
            mut group,
            ..
        } = self;
        let pieces = pieces.expect("built");
        let design = layout.design().expect("a cottage");
        rooms_into(&mut group, &layout, &pieces, &standing, true);
        // The ground floor's boards laid on the ground inside the footing, which a
        // gutted cottage shows and a tank driving in rolls over, and the footing under
        // the parts still standing.
        let mut floor = Kit::new();
        floor.block(
            Surface::Wood,
            DVec3::new(design.w - 2.0 * WALL_DEPTH, FLOOR_BOARDS, design.d - 2.0 * WALL_DEPTH),
            DMat4::from_translation(DVec3::new(0.0, FLOOR_BOARDS / 2.0, 0.0)),
        );
        footings(&mut floor, &layout, &standing);
        paint_cottage(&mut group, floor.finish_scaled(cottage_scale(design)), design, &pieces.palette);
        flatten_into(&mut group, pieces.ground);
        // One draw per finish for the whole cottage, its plinth and yard with it.
        batch(&mut group);
        group.set_rotation_euler(0.0, design.yaw(), 0.0);
        group
    }

    /// The built cottage as its cover's model (`cover_model`): placed at its ground
    /// centre.
    pub fn finish(self) -> Node {
        let position = DVec3::new(self.x, 0.0, self.z);
        Node {
            position,
            children: vec![self.finish_cottage()],
            ..Node::default()
        }
    }
}

/// The fieldstone footing under each of `standing`'s parts on the ground (the outer walls'
/// lowest parts and the corner posts), from the ground to the plinth's top and a little
/// proud of the wall's face: one course of the plinth that goes with its part.
fn footings(kit: &mut Kit<Surface>, layout: &Structure, standing: &PartSet) {
    let half = DVec2::new(layout.design().expect("a cottage").w / 2.0, layout.design().expect("a cottage").d / 2.0);
    for index in standing.iter() {
        let part = &layout.parts[index];
        if !part.grounded || !(part.house().wall.is_some() || part.house().kind == PartKind::Post) {
            continue;
        }
        let (mut min, mut max) = (part.min, part.max);
        for (axis, plan) in [(0, 0), (2, 1)] {
            if max[axis] >= half[plan] - ON_FACE {
                max[axis] += PLINTH_LIP;
            }
            if min[axis] <= -half[plan] + ON_FACE {
                min[axis] -= PLINTH_LIP;
            }
        }
        let (min, max) = (min.with_y(0.0), max.with_y(PLINTH));
        kit.block(Surface::Stone, max - min, DMat4::from_translation((min + max) / 2.0));
    }
}

/// The parts `parts` of the cottage for a cover box, fallen together: drawn about the
/// cottage's ground centre, turned into the world as the cottage stood. A page drawing
/// several of a cottage's bodies at once builds one [`HouseDebris`] for them all.
pub fn house_debris(x: f64, z: f64, w: f64, d: f64, h: f64, parts: &PartSet) -> Node {
    let layout = shared_house_structure(&HouseDesign::of(x, z, w, d, h));
    HouseDebris::new(layout, x, z).body(parts)
}

/// What a cottage's fallen bodies are cut from: its layout and its finished pieces
/// (without its plinth and yard, which never fall). Building the pieces is most of a
/// body's cost, so a collapse's bodies share one; it is dropped once they are built,
/// never kept, since the bodies' models hold what they draw.
pub struct HouseDebris {
    layout: Arc<Structure>,
    pieces: CottagePieces,
}

impl HouseDebris {
    /// The pieces of the cottage at `(x, z)` laid out as `layout` (its
    /// [`shared_house_structure`]).
    pub fn new(layout: Arc<Structure>, x: f64, z: f64) -> Self {
        let pieces = cottage_pieces(layout.design().expect("a cottage"), x, z, false);
        Self { layout, pieces }
    }

    /// The body of its parts `parts`, as [`house_debris`] draws it.
    pub fn body(&self, parts: &PartSet) -> Node {
        let layout = &*self.layout;
        let design = layout.design().expect("a cottage");
        let mut node = parts_node(layout, &self.pieces, parts, true);
        batch(&mut node);
        // A small piece casts no shadow: a collapse brings down dozens, each a shadow draw
        // per finish, and only the big sections' shadows show from the battlefield.
        let (low, high) = parts.iter().fold(
            (DVec3::INFINITY, DVec3::NEG_INFINITY),
            |(low, high), index| (low.min(layout.part(index).min), high.max(layout.part(index).max)),
        );
        if (high - low).max_element() < SHADOWLESS_UNDER {
            uncast(&mut node);
        }
        node.set_rotation_euler(0.0, design.yaw(), 0.0);
        let mut root = Node::group("house-debris");
        root.children.push(node);
        root
    }
}

/// Loose pieces smaller than this (m) across cast no shadow.
const SHADOWLESS_UNDER: f64 = 1.8;

fn uncast(node: &mut Node) {
    if let Some(drawable) = &mut node.drawable {
        drawable.cast_shadow = false;
    }
    for child in &mut node.children {
        uncast(child);
    }
}

/// The model of `draw`'s parts in the build frame, to batch.
fn parts_node(layout: &Structure, pieces: &CottagePieces, draw: &PartSet, inside: bool) -> Node {
    let mut group = Node::group("");
    for piece in &pieces.parts {
        piece_into(&mut group, layout, pieces, piece, draw);
    }
    rooms_into(&mut group, layout, pieces, draw, inside);
    group
}

/// `piece`'s cells of `draw`'s parts into `group`: the piece as built when all of them
/// are drawn, else cut to them with torn edges where their neighbours are gone.
fn piece_into(
    group: &mut Node,
    layout: &Structure,
    pieces: &CottagePieces,
    piece: &CottagePiece,
    draw: &PartSet,
) {
    let design = layout.design().expect("a cottage");
    let cells: Vec<(usize, PartCell)> = layout
        .parts
        .iter()
        .enumerate()
        .flat_map(|(index, part)| {
            part.house().cells
                .iter()
                .filter(|cell| cell.part == piece.part)
                .map(move |cell| (index, *cell))
        })
        .collect();
    let mut drawn: Vec<(usize, PartCell)> = cells
        .iter()
        .copied()
        .filter(|(index, _)| draw.contains(*index))
        .collect();
    // The pieces a landing broke a part into, drawn as their own cells of it.
    let whole = drawn.len() == cells.len();
    let first = layout.parts.len();
    let before = drawn.len();
    drawn.extend(layout.pieces.iter().enumerate().flat_map(|(k, part)| {
        let index = first + k;
        let cells: Vec<(usize, PartCell)> = if draw.contains(index) {
            part.house()
                .cells
                .iter()
                .filter(|cell| cell.part == piece.part)
                .map(|cell| (index, *cell))
                .collect()
        } else {
            Vec::new()
        };
        cells
    }));
    if drawn.is_empty() {
        return;
    }
    if whole && drawn.len() == before {
        // All of it: the part as built, sharing its meshes.
        paint_cottage_whole(group, piece.meshes.clone(), design, &pieces.palette);
        if let Some(shingles) = &piece.shingles {
            group.children.push(cottage_shingles(shingles.clone(), &pieces.palette));
        }
        group.children.extend(piece.nodes.iter().cloned());
        return;
    }
    let plane = part_plane(design, piece.part);
    let outline = part_outline(design, piece.part);
    // The drawn cells cut out of the piece in one pass over its meshes, each
    // triangle projected once and clipped by the cells it reaches.
    let region = ShardRegion {
        cells: drawn.iter().map(|(_, cell)| cell_sides(cell, &drawn)).collect(),
    };
    let clip = ShardClip::new(&plane, &region);
    paint_cottage(
        group,
        clip_meshes(&piece.meshes, &clip),
        design,
        &pieces.palette,
    );
    if let Some(shingles) = &piece.shingles
        && let Some(mesh) = clip_mesh(shingles, &clip)
    {
        group.children.push(cottage_shingles(Arc::new(mesh), &pieces.palette));
    }
    group.children.extend(
        piece
            .nodes
            .iter()
            .filter(|node| region.contains(plane.project(node.position)))
            .cloned(),
    );
    // Torn timber along each cell's edges where its neighbours are gone, all of the
    // piece's in one kit. Board ends keep clear of a window or door still in the wall,
    // but tear on into one that went.
    let openings = pieces.openings.clone().map(|openings| {
        openings
            .into_iter()
            .filter(|o| {
                let middle = DVec2::new((o[0] + o[2]) / 2.0, (o[1] + o[3]) / 2.0);
                drawn.iter().any(|(_, cell)| {
                    middle.x > cell.min.x && middle.x < cell.max.x && middle.y > cell.min.y && middle.y < cell.max.y
                })
            })
            .collect::<Vec<_>>()
    });
    let mut torn = Kit::new();
    for (&(index, _), sides) in drawn.iter().zip(&region.cells) {
        let cell = ShardRegion {
            cells: vec![sides.clone()],
        };
        let mut rng = Random::new(f64::from(design.variant) * 7.0 + index as f64 * 13.0 + 3.0);
        if let Some(wall) = piece.part.wall_index() {
            wall_breaks(&mut torn, &plane, &outline, &cell, &openings[wall], &mut rng);
        } else if piece.part.is_roof() {
            roof_breaks(&mut torn, &plane, &outline, &cell, &pieces.roof, &mut rng);
        }
    }
    paint_cottage(group, torn.finish_scaled(cottage_scale(design)), design, &pieces.palette);
}

/// The rooms of `draw`'s parts (attic floor strips and the partition) into `group`,
/// when `inside` shows them.
fn rooms_into(group: &mut Node, layout: &Structure, pieces: &CottagePieces, draw: &PartSet, inside: bool) {
    let design = layout.design().expect("a cottage");
    let mut rooms = Kit::new();
    for index in draw.iter().filter(|_| inside) {
        let Some(part) = layout.parts.get(index) else {
            continue;
        };
        let size = part.max - part.min;
        let middle = part.middle();
        match part.house().kind {
            // Boards over a plastered ceiling, their seams dark between them.
            PartKind::Floor => {
                let boards = DVec3::new(size.x, FLOOR_BOARDS, size.z);
                rooms.block(
                    Surface::Wood,
                    boards,
                    DMat4::from_translation(DVec3::new(middle.x, part.max.y - FLOOR_BOARDS / 2.0, middle.z)),
                );
                let seams = (size.z / BOARD_WIDTH).floor() as usize;
                for k in 1..seams {
                    let z = part.min.z + size.z * k as f64 / seams as f64;
                    rooms.block(
                        Surface::Sheathing,
                        DVec3::new(size.x, SEAM_PROUD, SEAM_WIDTH),
                        DMat4::from_translation(DVec3::new(middle.x, part.max.y + SEAM_PROUD / 2.0, z)),
                    );
                }
                let ceiling = DVec3::new(size.x, size.y - FLOOR_BOARDS, size.z);
                rooms.block(
                    Surface::Lining,
                    ceiling,
                    DMat4::from_translation(DVec3::new(middle.x, part.min.y + ceiling.y / 2.0, middle.z)),
                );
            }
            // Lath and plaster on studs, which stand proud of it where it is torn.
            PartKind::Partition => {
                rooms.block(Surface::Lining, size, DMat4::from_translation(middle));
                let studs = (size.x / STUD_SPACING).round().max(1.0) as usize;
                for k in 0..=studs {
                    let x = part.min.x + size.x * k as f64 / studs as f64;
                    rooms.block(
                        Surface::Wood,
                        DVec3::new(STUD_WIDTH, size.y, size.z + 2.0 * SEAM_PROUD),
                        DMat4::from_translation(DVec3::new(x, middle.y, middle.z)),
                    );
                }
            }
            _ => {}
        }
    }
    paint_cottage(group, rooms.finish_scaled(cottage_scale(design)), design, &pieces.palette);
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

