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
use super::house_collapse::{ShardClip, clip_meshes, clip_settled, roof_breaks, wall_breaks};
use super::house_model::{CottagePieces, cottage_pieces, cottage_scale, cottage_shingles, paint_cottage};
use super::tank_kit::Kit;
use crate::scene::Node;
use crate::sim::house_fracture::{EdgeKind, ShardRegion, Side};
use crate::sim::house_layout::{HouseDesign, HousePiece, PieceShape};
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
    cottage.children.push(pieces.ground);
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
    node.set_rotation_euler(0.0, design.yaw(), 0.0);
    let mut root = Node::group("house-debris");
    root.children.push(node);
    root
}

/// The model of `draw`'s parts in the build frame, batched.
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
                clip_meshes(&piece.meshes, &piece.meshes, &clip),
                design,
                &pieces.palette,
            );
            if let Some(shingles) = &piece.shingles
                && let Some(mesh) = clip_settled(shingles, shingles, &clip)
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
            let shape = PieceShape {
                piece: HousePiece::new(piece.part, 0),
                plane,
                outline: outline.clone(),
                region,
                hull: Vec::new(),
                center: DVec3::ZERO,
                size: DVec3::ZERO,
                overhang: Vec::new(),
            };
            let mut rng = Random::new(f64::from(design.variant) * 7.0 + index as f64 * 13.0 + 3.0);
            let mut torn = Kit::new();
            if let Some(wall) = piece.part.wall_index() {
                wall_breaks(&mut torn, &shape, &pieces.openings[wall], &mut rng);
            } else if piece.part.is_roof() {
                roof_breaks(&mut torn, &shape, &pieces.roof, &mut rng);
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
    batch(&mut group);
    group
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

#[cfg(test)]
mod cleanup_pins {
    //! Temporary: the cottages' models, hashed bit for bit, kept unchanged while the
    //! superseded cottage machinery is removed.

    use super::*;
    use crate::geometry::Mesh;
    use crate::models::house_model::house_facing;
    use crate::sim::extra_levels::extra_level;
    use crate::sim::level_rules::single_player_rules;
    use crate::sim::map_options::MapId;
    use crate::sim::types::CoverKind;
    use crate::sim::{RenderState, Simulation};

    struct Pin(u64);

    impl Pin {
        fn word(&mut self, word: u32) {
            self.0 = (self.0 ^ u64::from(word)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        fn words(&mut self, words: impl IntoIterator<Item = u32>) {
            let mut count = 0u32;
            for word in words {
                self.word(word);
                count += 1;
            }
            self.word(count);
        }
        fn floats<'a>(&mut self, values: impl IntoIterator<Item = &'a f32>) {
            self.words(values.into_iter().map(|v| v.to_bits()));
        }
        fn doubles(&mut self, values: impl IntoIterator<Item = f64>) {
            for value in values {
                let bits = value.to_bits();
                self.word(bits as u32);
                self.word((bits >> 32) as u32);
            }
        }
        fn text(&mut self, text: &str) {
            self.words(text.bytes().map(u32::from));
        }
        fn mesh(&mut self, mesh: &Mesh) {
            self.floats(mesh.positions.as_flattened());
            self.floats(mesh.normals.as_flattened());
            self.floats(mesh.uvs.as_flattened());
            self.floats(mesh.colors.as_flattened());
            match &mesh.indices {
                Some(indices) => self.words(indices.iter().copied()),
                None => self.word(u32::MAX),
            }
            self.word(mesh.attributes.len() as u32);
            for attribute in &mesh.attributes {
                self.text(attribute.name);
                self.word(u32::from(attribute.item_size));
                self.word(u32::from(attribute.per_instance));
                self.floats(&attribute.data);
            }
        }
        fn node(&mut self, node: &Node) {
            self.text(&node.name);
            self.doubles(node.position.to_array());
            self.doubles(node.rotation.to_array());
            self.doubles(node.scale.to_array());
            self.word(u32::from(node.visible));
            match &node.drawable {
                Some(drawable) => {
                    self.word(1);
                    self.mesh(&drawable.mesh);
                    self.text(&format!("{:?}", drawable.material));
                    self.word(u32::from(drawable.cast_shadow));
                    self.word(u32::from(drawable.receive_shadow));
                    self.word(drawable.render_order as u32);
                    self.word(u32::from(drawable.frustum_culled));
                    match &drawable.instances {
                        Some(instances) => {
                            self.word(instances.len() as u32);
                            for instance in instances {
                                self.doubles(instance.matrix.to_cols_array());
                                match instance.color {
                                    Some(color) => self.floats(&color),
                                    None => self.word(u32::MAX),
                                }
                            }
                        }
                        None => self.word(u32::MAX),
                    }
                }
                None => self.word(0),
            }
            self.word(node.children.len() as u32);
            for child in &node.children {
                self.node(child);
            }
        }
    }

    fn pin() -> Pin {
        Pin(0xcbf2_9ce4_8422_2325)
    }

    fn cottages() -> Vec<[f64; 5]> {
        let mut boxes = Vec::new();
        for map in MapId::ALL {
            let simulation = match extra_level(map) {
                Some(level) => Simulation::new(417.0, single_player_rules(level)),
                None => {
                    let mut simulation = Simulation::with_seed(417.0);
                    simulation.map_mode = map;
                    simulation.reset(None);
                    simulation
                }
            };
            let mut view = RenderState::default();
            simulation.fill_render_state(&mut view, None);
            boxes.extend(
                view.covers
                    .iter()
                    .filter(|cover| cover.kind == CoverKind::House)
                    .map(|c| [c.x, c.z, c.w, c.d, c.h]),
            );
        }
        boxes
    }

    fn broken_sets(layout: &HouseStructure) -> Vec<PartSet> {
        let count = layout.parts.len();
        vec![
            PartSet::EMPTY,
            PartSet::from_indices((5..14).filter(|&i| i < count)),
            PartSet::from_indices((0..count).filter(|&i| layout.parts[i].wall == Some(0))),
            PartSet::from_indices((0..count).filter(|&i| layout.parts[i].wall == Some(3))),
            PartSet::from_indices(
                (0..count).filter(|&i| layout.parts[i].slope.is_some() && i % 3 == 0),
            ),
            PartSet::from_indices(0..count),
        ]
    }

    #[test]
    fn cottage_models_are_unchanged() {
        let mut facing = pin();
        for (k, [x, z, w, d, h]) in [
            [12.0, -40.0, 6.0, 7.0, 5.4],
            [-31.0, 17.5, 7.5, 6.0, 6.2],
            [55.0, 61.0, 8.0, 9.0, 7.4],
            [-3.0, -88.0, 5.6, 6.4, 5.0],
            [140.0, 9.0, 9.0, 7.0, 8.0],
        ]
        .into_iter()
        .enumerate()
        {
            for street in 0..4 {
                let mut group = Node::group("");
                house_facing(&mut group, x, z, w, d, h, 0xd8cdb4, street + k as i32);
                facing.node(&group);
            }
        }
        let boxes = cottages();
        assert!(boxes.len() > 4, "{} cottages", boxes.len());
        let mut pieces_pin = pin();
        let mut standing = pin();
        let mut debris = pin();
        for &[x, z, w, d, h] in &boxes {
            let design = HouseDesign::of(x, z, w, d, h);
            let pieces = cottage_pieces(&design, x, z);
            pieces_pin.text(&format!("{:?} {:?} {:?}", pieces.roof, pieces.palette, pieces.openings));
            for piece in &pieces.parts {
                pieces_pin.text(&format!("{:?}", piece.part));
                for (surface, mesh) in &piece.meshes {
                    pieces_pin.text(&format!("{surface:?}"));
                    pieces_pin.mesh(mesh);
                }
                match &piece.shingles {
                    Some(mesh) => pieces_pin.mesh(mesh),
                    None => pieces_pin.word(u32::MAX),
                }
                for node in &piece.nodes {
                    pieces_pin.node(node);
                }
            }
            pieces_pin.node(&pieces.ground);
            let layout = HouseStructure::new(&design);
            for set in broken_sets(&layout) {
                let mut group = Node::group("");
                structural_house(&mut group, x, z, w, d, h, &set);
                standing.node(&group);
                debris.node(&house_debris(x, z, w, d, h, &set));
            }
        }
        let found = [facing.0, pieces_pin.0, standing.0, debris.0];
        println!("cottage pins: {found:#018x?} over {} cottages", boxes.len());
        let expected: Option<[u64; 4]> = Some([
            0x00d1_d06d_55b6_3f6f,
            0xce91_cf7e_75fe_7986,
            0xbd07_8f5e_cbc6_e47f,
            0x644f_ecbd_6a6f_6e94,
        ]);
        if let Some(expected) = expected {
            assert_eq!(found, expected);
        }
    }
}
