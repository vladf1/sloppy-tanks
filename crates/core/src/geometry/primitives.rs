//! Ports of Three.js r185 BoxGeometry, RoundedBoxGeometry (addon), PlaneGeometry,
//! CircleGeometry, RingGeometry, CylinderGeometry, ConeGeometry, SphereGeometry and
//! TorusGeometry. Parameter structs default to Three's constructor defaults; the
//! free functions cover the common call shapes. Material groups are not kept.

use std::f64::consts::PI;

use glam::DVec3;

use super::math::{angle_to, js_sign, normalize};
use super::mesh::{Mesh, widen};

/// Accumulates f64 attribute values as Three's generators push them into JS arrays.
#[derive(Default)]
struct Builder {
    positions: Vec<f64>,
    normals: Vec<f64>,
    uvs: Vec<f64>,
    indices: Vec<u32>,
}

impl Builder {
    fn vertex(&mut self, position: [f64; 3], normal: [f64; 3], uv: [f64; 2]) {
        self.positions.extend_from_slice(&position);
        self.normals.extend_from_slice(&normal);
        self.uvs.extend_from_slice(&uv);
    }

    fn triangle(&mut self, a: u32, b: u32, c: u32) {
        self.indices.extend_from_slice(&[a, b, c]);
    }

    fn vertex_count(&self) -> u32 {
        (self.positions.len() / 3) as u32
    }

    fn build(self) -> Mesh {
        Mesh::from_f64(
            &self.positions,
            &self.normals,
            &self.uvs,
            Some(self.indices),
        )
    }
}

/// `THREE.BoxGeometry`: six planes (+x, -x, +y, -y, +z, -z), each a grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxGeometry {
    pub width: f64,
    pub height: f64,
    pub depth: f64,
    pub width_segments: u32,
    pub height_segments: u32,
    pub depth_segments: u32,
}

impl Default for BoxGeometry {
    fn default() -> Self {
        Self {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
            width_segments: 1,
            height_segments: 1,
            depth_segments: 1,
        }
    }
}

impl BoxGeometry {
    pub fn build(&self) -> Mesh {
        let mut builder = Builder::default();
        let (w, h, d) = (self.width, self.height, self.depth);
        let (ws, hs, ds) = (
            self.width_segments,
            self.height_segments,
            self.depth_segments,
        );
        // Axis indices (u, v, w) and directions exactly as Three's buildPlane calls.
        box_plane(&mut builder, [2, 1, 0], -1.0, -1.0, [d, h, w], [ds, hs]);
        box_plane(&mut builder, [2, 1, 0], 1.0, -1.0, [d, h, -w], [ds, hs]);
        box_plane(&mut builder, [0, 2, 1], 1.0, 1.0, [w, d, h], [ws, ds]);
        box_plane(&mut builder, [0, 2, 1], 1.0, -1.0, [w, d, -h], [ws, ds]);
        box_plane(&mut builder, [0, 1, 2], 1.0, -1.0, [w, h, d], [ws, hs]);
        box_plane(&mut builder, [0, 1, 2], -1.0, -1.0, [w, h, -d], [ws, hs]);
        builder.build()
    }
}

fn box_plane(
    builder: &mut Builder,
    [u, v, w]: [usize; 3],
    udir: f64,
    vdir: f64,
    [width, height, depth]: [f64; 3],
    [grid_x, grid_y]: [u32; 2],
) {
    let segment_width = width / f64::from(grid_x);
    let segment_height = height / f64::from(grid_y);
    let (width_half, height_half, depth_half) = (width / 2.0, height / 2.0, depth / 2.0);
    let grid_x1 = grid_x + 1;
    let first = builder.vertex_count();
    for iy in 0..=grid_y {
        let y = f64::from(iy) * segment_height - height_half;
        for ix in 0..=grid_x {
            let x = f64::from(ix) * segment_width - width_half;
            let mut position = [0.0; 3];
            position[u] = x * udir;
            position[v] = y * vdir;
            position[w] = depth_half;
            let mut normal = [0.0; 3];
            normal[w] = if depth > 0.0 { 1.0 } else { -1.0 };
            let uv = [
                f64::from(ix) / f64::from(grid_x),
                1.0 - f64::from(iy) / f64::from(grid_y),
            ];
            builder.vertex(position, normal, uv);
        }
    }
    for iy in 0..grid_y {
        for ix in 0..grid_x {
            let a = first + ix + grid_x1 * iy;
            let b = first + ix + grid_x1 * (iy + 1);
            let c = first + (ix + 1) + grid_x1 * (iy + 1);
            let d = first + (ix + 1) + grid_x1 * iy;
            builder.triangle(a, b, d);
            builder.triangle(b, c, d);
        }
    }
}

pub fn box_geometry(width: f64, height: f64, depth: f64) -> Mesh {
    BoxGeometry {
        width,
        height,
        depth,
        ..BoxGeometry::default()
    }
    .build()
}

/// `RoundedBoxGeometry` (three/addons): a unit box with `2 * segments + 1` grid
/// divisions whose vertices are pushed onto rounded edges of `radius` (clamped to
/// half the smallest side). With `segments == 0` it is a plain indexed unit box,
/// as in Three, where the early return skips the rounding and ignores the size.
/// Otherwise the result is non-indexed.
pub fn rounded_box_geometry(
    width: f64,
    height: f64,
    depth: f64,
    segments: u32,
    radius: f64,
) -> Mesh {
    let total_segments = segments * 2 + 1;
    let radius = (width / 2.0).min(height / 2.0).min(depth / 2.0).min(radius);
    let unit = BoxGeometry {
        width_segments: total_segments,
        height_segments: total_segments,
        depth_segments: total_segments,
        ..BoxGeometry::default()
    }
    .build();
    if total_segments == 1 {
        return unit;
    }
    let mut mesh = unit.to_non_indexed();
    let half = DVec3::new(width, height, depth) / 2.0 - radius;
    let half_segment_size = 0.5 / f64::from(total_segments);
    let face_vertices = mesh.positions.len() / 6;
    for i in 0..mesh.positions.len() {
        let position = widen(mesh.positions[i]);
        let normal = normalize(DVec3::new(
            position.x - js_sign(position.x) * half_segment_size,
            position.y - js_sign(position.y) * half_segment_size,
            position.z - js_sign(position.z) * half_segment_size,
        ));
        mesh.positions[i] = [
            (half.x * js_sign(position.x) + normal.x * radius) as f32,
            (half.y * js_sign(position.y) + normal.y * radius) as f32,
            (half.z * js_sign(position.z) + normal.z * radius) as f32,
        ];
        mesh.normals[i] = [normal.x as f32, normal.y as f32, normal.z as f32];
        let rounded_uv = |face: DVec3, uv_axis: usize, projection_axis: usize, side: f64| {
            rounded_box_uv(face, normal, uv_axis, projection_axis, radius, side)
        };
        let (x, y, z) = (0, 1, 2);
        let uv = match i / face_vertices {
            0 => {
                let face = DVec3::X;
                [
                    rounded_uv(face, z, y, depth),
                    1.0 - rounded_uv(face, y, z, height),
                ]
            }
            1 => {
                let face = DVec3::NEG_X;
                [
                    1.0 - rounded_uv(face, z, y, depth),
                    1.0 - rounded_uv(face, y, z, height),
                ]
            }
            2 => {
                let face = DVec3::Y;
                [
                    1.0 - rounded_uv(face, x, z, width),
                    rounded_uv(face, z, x, depth),
                ]
            }
            3 => {
                let face = DVec3::NEG_Y;
                [
                    1.0 - rounded_uv(face, x, z, width),
                    1.0 - rounded_uv(face, z, x, depth),
                ]
            }
            4 => {
                let face = DVec3::Z;
                [
                    1.0 - rounded_uv(face, x, y, width),
                    1.0 - rounded_uv(face, y, x, height),
                ]
            }
            _ => {
                let face = DVec3::NEG_Z;
                [
                    rounded_uv(face, x, y, width),
                    1.0 - rounded_uv(face, y, x, height),
                ]
            }
        };
        mesh.uvs[i] = [uv[0] as f32, uv[1] as f32];
    }
    mesh
}

/// The addon's `getUv`: arc-length UVs across a rounded edge and the flat center.
fn rounded_box_uv(
    face: DVec3,
    normal: DVec3,
    uv_axis: usize,
    projection_axis: usize,
    radius: f64,
    side_length: f64,
) -> f64 {
    let arc_length = 2.0 * PI * radius / 4.0;
    let center_length = (side_length - 2.0 * radius).max(0.0);
    let half_arc = PI / 4.0;
    let mut projected = normal;
    projected[projection_axis] = 0.0;
    let projected = normalize(projected);
    let arc_uv_ratio = 0.5 * arc_length / (arc_length + center_length);
    let arc_angle_ratio = 1.0 - angle_to(projected, face) / half_arc;
    if js_sign(projected[uv_axis]) == 1.0 {
        arc_angle_ratio * arc_uv_ratio
    } else {
        let length_uv = center_length / (arc_length + center_length);
        length_uv + arc_uv_ratio + arc_uv_ratio * (1.0 - arc_angle_ratio)
    }
}

/// `THREE.PlaneGeometry`: a grid in the XY plane facing +z.
pub fn plane_geometry_segments(
    width: f64,
    height: f64,
    width_segments: u32,
    height_segments: u32,
) -> Mesh {
    let mut builder = Builder::default();
    let (width_half, height_half) = (width / 2.0, height / 2.0);
    let (grid_x, grid_y) = (width_segments, height_segments);
    let grid_x1 = grid_x + 1;
    let segment_width = width / f64::from(grid_x);
    let segment_height = height / f64::from(grid_y);
    for iy in 0..=grid_y {
        let y = f64::from(iy) * segment_height - height_half;
        for ix in 0..=grid_x {
            let x = f64::from(ix) * segment_width - width_half;
            builder.vertex(
                [x, -y, 0.0],
                [0.0, 0.0, 1.0],
                [
                    f64::from(ix) / f64::from(grid_x),
                    1.0 - f64::from(iy) / f64::from(grid_y),
                ],
            );
        }
    }
    for iy in 0..grid_y {
        for ix in 0..grid_x {
            let a = ix + grid_x1 * iy;
            let b = ix + grid_x1 * (iy + 1);
            let c = (ix + 1) + grid_x1 * (iy + 1);
            let d = (ix + 1) + grid_x1 * iy;
            builder.triangle(a, b, d);
            builder.triangle(b, c, d);
        }
    }
    builder.build()
}

pub fn plane_geometry(width: f64, height: f64) -> Mesh {
    plane_geometry_segments(width, height, 1, 1)
}

/// `THREE.CircleGeometry`: a fan around a center vertex, facing +z.
pub fn circle_geometry_arc(
    radius: f64,
    segments: u32,
    theta_start: f64,
    theta_length: f64,
) -> Mesh {
    let segments = segments.max(3);
    let mut builder = Builder::default();
    builder.vertex([0.0; 3], [0.0, 0.0, 1.0], [0.5, 0.5]);
    for s in 0..=segments {
        let segment = theta_start + f64::from(s) / f64::from(segments) * theta_length;
        let x = radius * segment.cos();
        let y = radius * segment.sin();
        builder.vertex(
            [x, y, 0.0],
            [0.0, 0.0, 1.0],
            [(x / radius + 1.0) / 2.0, (y / radius + 1.0) / 2.0],
        );
    }
    for i in 1..=segments {
        builder.triangle(i, i + 1, 0);
    }
    builder.build()
}

pub fn circle_geometry(radius: f64, segments: u32) -> Mesh {
    circle_geometry_arc(radius, segments, 0.0, PI * 2.0)
}

/// `THREE.RingGeometry`, facing +z.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RingGeometry {
    pub inner_radius: f64,
    pub outer_radius: f64,
    pub theta_segments: u32,
    pub phi_segments: u32,
    pub theta_start: f64,
    pub theta_length: f64,
}

impl Default for RingGeometry {
    fn default() -> Self {
        Self {
            inner_radius: 0.5,
            outer_radius: 1.0,
            theta_segments: 32,
            phi_segments: 1,
            theta_start: 0.0,
            theta_length: PI * 2.0,
        }
    }
}

impl RingGeometry {
    pub fn build(&self) -> Mesh {
        let theta_segments = self.theta_segments.max(3);
        let phi_segments = self.phi_segments.max(1);
        let mut builder = Builder::default();
        let mut radius = self.inner_radius;
        let radius_step = (self.outer_radius - self.inner_radius) / f64::from(phi_segments);
        for _ in 0..=phi_segments {
            for i in 0..=theta_segments {
                let segment =
                    self.theta_start + f64::from(i) / f64::from(theta_segments) * self.theta_length;
                let x = radius * segment.cos();
                let y = radius * segment.sin();
                builder.vertex(
                    [x, y, 0.0],
                    [0.0, 0.0, 1.0],
                    [
                        (x / self.outer_radius + 1.0) / 2.0,
                        (y / self.outer_radius + 1.0) / 2.0,
                    ],
                );
            }
            radius += radius_step;
        }
        for j in 0..phi_segments {
            let level = j * (theta_segments + 1);
            for i in 0..theta_segments {
                let segment = i + level;
                let (a, b, c, d) = (
                    segment,
                    segment + theta_segments + 1,
                    segment + theta_segments + 2,
                    segment + 1,
                );
                builder.triangle(a, b, d);
                builder.triangle(b, c, d);
            }
        }
        builder.build()
    }
}

pub fn ring_geometry(inner_radius: f64, outer_radius: f64, theta_segments: u32) -> Mesh {
    RingGeometry {
        inner_radius,
        outer_radius,
        theta_segments,
        ..RingGeometry::default()
    }
    .build()
}

/// `THREE.CylinderGeometry` (and ConeGeometry with `radius_top == 0`): the torso,
/// then the top cap, then the bottom cap, along the y axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CylinderGeometry {
    pub radius_top: f64,
    pub radius_bottom: f64,
    pub height: f64,
    pub radial_segments: u32,
    pub height_segments: u32,
    pub open_ended: bool,
    pub theta_start: f64,
    pub theta_length: f64,
}

impl Default for CylinderGeometry {
    fn default() -> Self {
        Self {
            radius_top: 1.0,
            radius_bottom: 1.0,
            height: 1.0,
            radial_segments: 32,
            height_segments: 1,
            open_ended: false,
            theta_start: 0.0,
            theta_length: PI * 2.0,
        }
    }
}

impl CylinderGeometry {
    pub fn build(&self) -> Mesh {
        let mut builder = Builder::default();
        self.torso(&mut builder);
        if !self.open_ended {
            if self.radius_top > 0.0 {
                self.cap(&mut builder, true);
            }
            if self.radius_bottom > 0.0 {
                self.cap(&mut builder, false);
            }
        }
        builder.build()
    }

    fn theta(&self, x: u32) -> (f64, f64) {
        let u = f64::from(x) / f64::from(self.radial_segments);
        (u, u * self.theta_length + self.theta_start)
    }

    fn torso(&self, builder: &mut Builder) {
        let half_height = self.height / 2.0;
        let slope = (self.radius_bottom - self.radius_top) / self.height;
        let columns = self.radial_segments + 1;
        for y in 0..=self.height_segments {
            let v = f64::from(y) / f64::from(self.height_segments);
            let radius = v * (self.radius_bottom - self.radius_top) + self.radius_top;
            for x in 0..=self.radial_segments {
                let (u, theta) = self.theta(x);
                let (sin_theta, cos_theta) = (theta.sin(), theta.cos());
                let normal = normalize(DVec3::new(sin_theta, slope, cos_theta));
                builder.vertex(
                    [
                        radius * sin_theta,
                        -v * self.height + half_height,
                        radius * cos_theta,
                    ],
                    normal.to_array(),
                    [u, 1.0 - v],
                );
            }
        }
        let index = |x: u32, y: u32| y * columns + x;
        for x in 0..self.radial_segments {
            for y in 0..self.height_segments {
                let (a, b, c, d) = (
                    index(x, y),
                    index(x, y + 1),
                    index(x + 1, y + 1),
                    index(x + 1, y),
                );
                if self.radius_top > 0.0 || y != 0 {
                    builder.triangle(a, b, d);
                }
                if self.radius_bottom > 0.0 || y != self.height_segments - 1 {
                    builder.triangle(b, c, d);
                }
            }
        }
    }

    fn cap(&self, builder: &mut Builder, top: bool) {
        let center_start = builder.vertex_count();
        let radius = if top {
            self.radius_top
        } else {
            self.radius_bottom
        };
        let sign = if top { 1.0 } else { -1.0 };
        let y = self.height / 2.0 * sign;
        for _ in 1..=self.radial_segments {
            builder.vertex([0.0, y, 0.0], [0.0, sign, 0.0], [0.5, 0.5]);
        }
        let center_end = builder.vertex_count();
        for x in 0..=self.radial_segments {
            let (_, theta) = self.theta(x);
            let (cos_theta, sin_theta) = (theta.cos(), theta.sin());
            builder.vertex(
                [radius * sin_theta, y, radius * cos_theta],
                [0.0, sign, 0.0],
                [cos_theta * 0.5 + 0.5, sin_theta * 0.5 * sign + 0.5],
            );
        }
        for x in 0..self.radial_segments {
            let c = center_start + x;
            let i = center_end + x;
            if top {
                builder.triangle(i, i + 1, c);
            } else {
                builder.triangle(i + 1, i, c);
            }
        }
    }
}

pub fn cylinder_geometry(
    radius_top: f64,
    radius_bottom: f64,
    height: f64,
    radial_segments: u32,
) -> Mesh {
    CylinderGeometry {
        radius_top,
        radius_bottom,
        height,
        radial_segments,
        ..CylinderGeometry::default()
    }
    .build()
}

/// `THREE.ConeGeometry(radius, height, radialSegments)`.
pub fn cone_geometry(radius: f64, height: f64, radial_segments: u32) -> Mesh {
    cylinder_geometry(0.0, radius, height, radial_segments)
}

/// `THREE.SphereGeometry`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SphereGeometry {
    pub radius: f64,
    pub width_segments: u32,
    pub height_segments: u32,
    pub phi_start: f64,
    pub phi_length: f64,
    pub theta_start: f64,
    pub theta_length: f64,
}

impl Default for SphereGeometry {
    fn default() -> Self {
        Self {
            radius: 1.0,
            width_segments: 32,
            height_segments: 16,
            phi_start: 0.0,
            phi_length: PI * 2.0,
            theta_start: 0.0,
            theta_length: PI,
        }
    }
}

impl SphereGeometry {
    pub fn build(&self) -> Mesh {
        let width_segments = self.width_segments.max(3);
        let height_segments = self.height_segments.max(2);
        let theta_end = (self.theta_start + self.theta_length).min(PI);
        let mut builder = Builder::default();
        for iy in 0..=height_segments {
            let v = f64::from(iy) / f64::from(height_segments);
            let theta = self.theta_start + v * self.theta_length;
            let y = self.radius * theta.cos();
            let ring_radius = (self.radius * self.radius - y * y).sqrt();
            let u_offset = if iy == 0 && self.theta_start == 0.0 {
                0.5 / f64::from(width_segments)
            } else if iy == height_segments && theta_end == PI {
                -0.5 / f64::from(width_segments)
            } else {
                0.0
            };
            for ix in 0..=width_segments {
                let u = f64::from(ix) / f64::from(width_segments);
                let phi = self.phi_start + u * self.phi_length;
                let vertex = DVec3::new(-ring_radius * phi.cos(), y, ring_radius * phi.sin());
                builder.vertex(
                    vertex.to_array(),
                    normalize(vertex).to_array(),
                    [u + u_offset, 1.0 - v],
                );
            }
        }
        let columns = width_segments + 1;
        for iy in 0..height_segments {
            for ix in 0..width_segments {
                let a = iy * columns + ix + 1;
                let b = iy * columns + ix;
                let c = (iy + 1) * columns + ix;
                let d = (iy + 1) * columns + ix + 1;
                if iy != 0 || self.theta_start > 0.0 {
                    builder.triangle(a, b, d);
                }
                if iy != height_segments - 1 || theta_end < PI {
                    builder.triangle(b, c, d);
                }
            }
        }
        builder.build()
    }
}

pub fn sphere_geometry(radius: f64, width_segments: u32, height_segments: u32) -> Mesh {
    SphereGeometry {
        radius,
        width_segments,
        height_segments,
        ..SphereGeometry::default()
    }
    .build()
}

/// `THREE.TorusGeometry`, lying in the XY plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TorusGeometry {
    pub radius: f64,
    pub tube: f64,
    pub radial_segments: u32,
    pub tubular_segments: u32,
    pub arc: f64,
    pub theta_start: f64,
    pub theta_length: f64,
}

impl Default for TorusGeometry {
    fn default() -> Self {
        Self {
            radius: 1.0,
            tube: 0.4,
            radial_segments: 12,
            tubular_segments: 48,
            arc: PI * 2.0,
            theta_start: 0.0,
            theta_length: PI * 2.0,
        }
    }
}

impl TorusGeometry {
    pub fn build(&self) -> Mesh {
        let mut builder = Builder::default();
        let (radial, tubular) = (self.radial_segments, self.tubular_segments);
        for j in 0..=radial {
            let v = self.theta_start + (f64::from(j) / f64::from(radial)) * self.theta_length;
            for i in 0..=tubular {
                let u = f64::from(i) / f64::from(tubular) * self.arc;
                let vertex = DVec3::new(
                    (self.radius + self.tube * v.cos()) * u.cos(),
                    (self.radius + self.tube * v.cos()) * u.sin(),
                    self.tube * v.sin(),
                );
                let center = DVec3::new(self.radius * u.cos(), self.radius * u.sin(), 0.0);
                builder.vertex(
                    vertex.to_array(),
                    normalize(vertex - center).to_array(),
                    [
                        f64::from(i) / f64::from(tubular),
                        f64::from(j) / f64::from(radial),
                    ],
                );
            }
        }
        for j in 1..=radial {
            for i in 1..=tubular {
                let a = (tubular + 1) * j + i - 1;
                let b = (tubular + 1) * (j - 1) + i - 1;
                let c = (tubular + 1) * (j - 1) + i;
                let d = (tubular + 1) * j + i;
                builder.triangle(a, b, d);
                builder.triangle(b, c, d);
            }
        }
        builder.build()
    }
}

pub fn torus_geometry(radius: f64, tube: f64, radial_segments: u32, tubular_segments: u32) -> Mesh {
    TorusGeometry {
        radius,
        tube,
        radial_segments,
        tubular_segments,
        ..TorusGeometry::default()
    }
    .build()
}
