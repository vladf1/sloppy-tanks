//! Ports of Three.js r185 ShapeGeometry and ExtrudeGeometry (straight extrusion
//! along +z with optional bevels and Three's WorldUVGenerator). Extrusion along a
//! 3D path (`extrudePath`) and custom UV generators are not ported: the game never
//! used them.

use glam::DVec2;

use super::mesh::Mesh;
use super::shape::Shape;
use super::triangulate::{is_clockwise, triangulate_shape};

/// `THREE.ShapeGeometry(shapes, curveSegments)`: flat, indexed, facing +z, with the
/// shape's x/y as UVs. The outline is wound clockwise and holes counter-clockwise
/// before triangulation, as in Three.
pub fn shape_geometry(shapes: &[Shape], curve_segments: u32) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    for shape in shapes {
        let index_offset = (positions.len() / 3) as u32;
        let (mut outline, mut holes) = shape.extract_points(curve_segments);
        if !is_clockwise(&outline) {
            outline.reverse();
        }
        for hole in &mut holes {
            if is_clockwise(hole) {
                hole.reverse();
            }
        }
        let faces = triangulate_shape(&mut outline, &mut holes);
        for vertex in outline.iter().chain(holes.iter().flatten()) {
            positions.extend_from_slice(&[vertex.x, vertex.y, 0.0]);
            normals.extend_from_slice(&[0.0, 0.0, 1.0]);
            uvs.extend_from_slice(&[vertex.x, vertex.y]);
        }
        for face in faces {
            indices.extend(face.map(|i| i as u32 + index_offset));
        }
    }
    Mesh::from_f64(&positions, &normals, &uvs, Some(indices))
}

/// `ExtrudeGeometry` options with Three's defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExtrudeOptions {
    pub curve_segments: u32,
    pub steps: u32,
    pub depth: f64,
    pub bevel_enabled: bool,
    pub bevel_thickness: f64,
    /// `None` means Three's default of `bevel_thickness - 0.1`.
    pub bevel_size: Option<f64>,
    pub bevel_offset: f64,
    pub bevel_segments: u32,
}

impl Default for ExtrudeOptions {
    fn default() -> Self {
        Self {
            curve_segments: 12,
            steps: 1,
            depth: 1.0,
            bevel_enabled: true,
            bevel_thickness: 0.2,
            bevel_size: None,
            bevel_offset: 0.0,
            bevel_segments: 3,
        }
    }
}

/// `THREE.ExtrudeGeometry(shapes, options)`: a non-indexed prism from z = 0 to
/// `depth` (bevels extend beyond both ends), with flat normals from
/// `computeVertexNormals`. Vertex order: back lid, front lid, then side walls.
pub fn extrude_geometry(shapes: &[Shape], options: &ExtrudeOptions) -> Mesh {
    let mut vertices: Vec<f64> = Vec::new();
    let mut uvs: Vec<f64> = Vec::new();
    for shape in shapes {
        extrude_shape(shape, options, &mut vertices, &mut uvs);
    }
    let mut mesh = Mesh::from_f64(&vertices, &[], &uvs, None);
    mesh.compute_vertex_normals();
    mesh
}

/// Drops points within 1e-10 (scaled by coordinate magnitude) of their predecessor,
/// wrapping around so a closing duplicate is removed too.
fn merge_overlapping_points(points: &mut Vec<DVec2>) {
    const THRESHOLD_SQ: f64 = 1e-10 * 1e-10;
    if points.is_empty() {
        return;
    }
    let mut previous = points[0];
    let mut i = 1;
    while i <= points.len() {
        let current_index = i % points.len();
        let current = points[current_index];
        let (dx, dy) = (current.x - previous.x, current.y - previous.y);
        let dist_sq = dx * dx + dy * dy;
        let scale = current
            .x
            .abs()
            .max(current.y.abs())
            .max(previous.x.abs())
            .max(previous.y.abs());
        if dist_sq <= THRESHOLD_SQ * scale * scale {
            points.remove(current_index);
            if points.is_empty() {
                return;
            }
            continue;
        }
        previous = current;
        i += 1;
    }
}

/// The unit direction (scaled so corners keep the bevel width) that moves a
/// contour point outwards for bevels.
fn bevel_vector(point: DVec2, previous: DVec2, next: DVec2) -> DVec2 {
    let (v_prev_x, v_prev_y) = (point.x - previous.x, point.y - previous.y);
    let (v_next_x, v_next_y) = (next.x - point.x, next.y - point.y);
    let v_prev_lensq = v_prev_x * v_prev_x + v_prev_y * v_prev_y;
    let collinear0 = v_prev_x * v_next_y - v_prev_y * v_next_x;
    let (v_trans_x, v_trans_y, shrink_by);
    if collinear0.abs() > f64::EPSILON {
        let v_prev_len = v_prev_lensq.sqrt();
        let v_next_len = (v_next_x * v_next_x + v_next_y * v_next_y).sqrt();
        let prev_shift_x = previous.x - v_prev_y / v_prev_len;
        let prev_shift_y = previous.y + v_prev_x / v_prev_len;
        let next_shift_x = next.x - v_next_y / v_next_len;
        let next_shift_y = next.y + v_next_x / v_next_len;
        let sf = ((next_shift_x - prev_shift_x) * v_next_y
            - (next_shift_y - prev_shift_y) * v_next_x)
            / (v_prev_x * v_next_y - v_prev_y * v_next_x);
        v_trans_x = prev_shift_x + v_prev_x * sf - point.x;
        v_trans_y = prev_shift_y + v_prev_y * sf - point.y;
        let v_trans_lensq = v_trans_x * v_trans_x + v_trans_y * v_trans_y;
        if v_trans_lensq <= 2.0 {
            return DVec2::new(v_trans_x, v_trans_y);
        }
        shrink_by = (v_trans_lensq / 2.0).sqrt();
    } else {
        let direction_eq = if v_prev_x > f64::EPSILON {
            v_next_x > f64::EPSILON
        } else if v_prev_x < -f64::EPSILON {
            v_next_x < -f64::EPSILON
        } else {
            super::math::js_sign(v_prev_y) == super::math::js_sign(v_next_y)
        };
        if direction_eq {
            v_trans_x = -v_prev_y;
            v_trans_y = v_prev_x;
            shrink_by = v_prev_lensq.sqrt();
        } else {
            v_trans_x = v_prev_x;
            v_trans_y = v_prev_y;
            shrink_by = (v_prev_lensq / 2.0).sqrt();
        }
    }
    DVec2::new(v_trans_x / shrink_by, v_trans_y / shrink_by)
}

fn ring_movements(ring: &[DVec2]) -> Vec<DVec2> {
    let count = ring.len();
    (0..count)
        .map(|i| {
            let previous = ring[(i + count - 1) % count];
            let next = ring[(i + 1) % count];
            bevel_vector(ring[i], previous, next)
        })
        .collect()
}

/// `pt.clone().addScaledVector(vec, size)`.
fn scale_point(point: DVec2, direction: DVec2, size: f64) -> DVec2 {
    DVec2::new(point.x + direction.x * size, point.y + direction.y * size)
}

fn extrude_shape(shape: &Shape, options: &ExtrudeOptions, out: &mut Vec<f64>, uvs: &mut Vec<f64>) {
    let steps = options.steps;
    let depth = options.depth;
    let bevel_enabled = options.bevel_enabled;
    let (bevel_segments, bevel_thickness, bevel_size, bevel_offset) = if bevel_enabled {
        (
            options.bevel_segments,
            options.bevel_thickness,
            options.bevel_size.unwrap_or(options.bevel_thickness - 0.1),
            options.bevel_offset,
        )
    } else {
        (0, 0.0, 0.0, 0.0)
    };

    let (mut contour, mut holes) = shape.extract_points(options.curve_segments);
    if !is_clockwise(&contour) {
        contour.reverse();
        for hole in &mut holes {
            if is_clockwise(hole) {
                hole.reverse();
            }
        }
    }
    merge_overlapping_points(&mut contour);
    holes.iter_mut().for_each(merge_overlapping_points);

    let vertices: Vec<DVec2> = contour
        .iter()
        .chain(holes.iter().flatten())
        .copied()
        .collect();
    let vlen = vertices.len();
    let contour_movements = ring_movements(&contour);
    let holes_movements: Vec<Vec<DVec2>> = holes.iter().map(|hole| ring_movements(hole)).collect();
    let vertices_movements: Vec<DVec2> = contour_movements
        .iter()
        .chain(holes_movements.iter().flatten())
        .copied()
        .collect();

    // Vertex layers along z, addressed by face indices below.
    let mut layers: Vec<f64> = Vec::new();
    let mut push = |x: f64, y: f64, z: f64| layers.extend_from_slice(&[x, y, z]);
    let bevel_layer = |b: u32| {
        let t = f64::from(b) / f64::from(bevel_segments);
        let z = bevel_thickness * (t * std::f64::consts::PI / 2.0).cos();
        let size = bevel_size * (t * std::f64::consts::PI / 2.0).sin() + bevel_offset;
        (t, z, size)
    };

    let faces = if bevel_segments == 0 {
        triangulate_shape(&mut contour.clone(), &mut holes.clone())
    } else {
        let mut contracted_contour = Vec::new();
        let mut expanded_holes = Vec::new();
        for b in 0..bevel_segments {
            let (t, z, size) = bevel_layer(b);
            for (point, movement) in contour.iter().zip(&contour_movements) {
                let vert = scale_point(*point, *movement, size);
                push(vert.x, vert.y, -z);
                if t == 0.0 {
                    contracted_contour.push(vert);
                }
            }
            for (hole, movements) in holes.iter().zip(&holes_movements) {
                let mut hole_vertices = Vec::new();
                for (point, movement) in hole.iter().zip(movements) {
                    let vert = scale_point(*point, *movement, size);
                    push(vert.x, vert.y, -z);
                    if t == 0.0 {
                        hole_vertices.push(vert);
                    }
                }
                if t == 0.0 {
                    expanded_holes.push(hole_vertices);
                }
            }
        }
        triangulate_shape(&mut contracted_contour, &mut expanded_holes)
    };

    let full_size = bevel_size + bevel_offset;
    let layer_point = |i: usize| {
        if bevel_enabled {
            scale_point(vertices[i], vertices_movements[i], full_size)
        } else {
            vertices[i]
        }
    };
    for i in 0..vlen {
        let vert = layer_point(i);
        push(vert.x, vert.y, 0.0);
    }
    for s in 1..=steps {
        for i in 0..vlen {
            let vert = layer_point(i);
            push(vert.x, vert.y, depth / f64::from(steps) * f64::from(s));
        }
    }
    for b in (0..bevel_segments).rev() {
        let (_, z, size) = bevel_layer(b);
        for (point, movement) in contour.iter().zip(&contour_movements) {
            let vert = scale_point(*point, *movement, size);
            push(vert.x, vert.y, depth + z);
        }
        for (hole, movements) in holes.iter().zip(&holes_movements) {
            for (point, movement) in hole.iter().zip(movements) {
                let vert = scale_point(*point, *movement, size);
                push(vert.x, vert.y, depth + z);
            }
        }
    }

    let mut writer = FaceWriter {
        layers: &layers,
        out,
        uvs,
    };
    // Lids: the back is reversed so both face outwards.
    let top_layer = if bevel_enabled {
        (steps + bevel_segments * 2) as usize
    } else {
        steps as usize
    };
    for face in &faces {
        writer.triangle(face[2], face[1], face[0]);
    }
    for face in &faces {
        let offset = vlen * top_layer;
        writer.triangle(face[0] + offset, face[1] + offset, face[2] + offset);
    }
    // Side walls, contour first, then each hole.
    let wall_layers = (steps + bevel_segments * 2) as usize;
    let mut layer_offset = 0;
    for ring_len in std::iter::once(contour.len()).chain(holes.iter().map(Vec::len)) {
        for j in (0..ring_len).rev() {
            let k = if j == 0 { ring_len - 1 } else { j - 1 };
            for s in 0..wall_layers {
                let (slen1, slen2) = (vlen * s, vlen * (s + 1));
                writer.quad(
                    layer_offset + j + slen1,
                    layer_offset + k + slen1,
                    layer_offset + k + slen2,
                    layer_offset + j + slen2,
                );
            }
        }
        layer_offset += ring_len;
    }
}

/// Emits de-indexed triangles and Three's WorldUVGenerator UVs, which read the
/// f64 vertex values before they are narrowed to f32.
struct FaceWriter<'a> {
    layers: &'a [f64],
    out: &'a mut Vec<f64>,
    uvs: &'a mut Vec<f64>,
}

impl FaceWriter<'_> {
    fn vertex(&self, index: usize) -> [f64; 3] {
        [
            self.layers[index * 3],
            self.layers[index * 3 + 1],
            self.layers[index * 3 + 2],
        ]
    }

    fn triangle(&mut self, a: usize, b: usize, c: usize) {
        for index in [a, b, c] {
            let v = self.vertex(index);
            self.out.extend_from_slice(&v);
            self.uvs.extend_from_slice(&[v[0], v[1]]);
        }
    }

    fn quad(&mut self, a: usize, b: usize, c: usize, d: usize) {
        let [va, vb, vc, vd] = [a, b, c, d].map(|index| self.vertex(index));
        for v in [va, vb, vd, vb, vc, vd] {
            self.out.extend_from_slice(&v);
        }
        let uv = if (va[1] - vb[1]).abs() < (va[0] - vb[0]).abs() {
            [va, vb, vc, vd].map(|v| [v[0], 1.0 - v[2]])
        } else {
            [va, vb, vc, vd].map(|v| [v[1], 1.0 - v[2]])
        };
        for corner in [0, 1, 3, 1, 2, 3] {
            self.uvs.extend_from_slice(&uv[corner]);
        }
    }
}
