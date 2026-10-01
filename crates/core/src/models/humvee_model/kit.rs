//! A parts kit for the Humvee: every part is moved into its assembly's frame and
//! merged per paint role, so a whole body, wheel or launcher is a handful of
//! shared meshes (one per material) instead of hundreds of tiny parts.
//!
//! Every triangle is checked against its intended normal and turned to face it,
//! so mirrored parts and hand-built faces never show their back sides.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::{DMat4, DVec2, DVec3, DVec4};

use crate::geometry::math::quat_from_euler;
use crate::geometry::{
    ExtrudeOptions, Mesh, Path, Shape, cylinder_geometry, extrude_geometry, merge_geometries,
    narrow, rounded_box_geometry, torus_geometry, widen,
};

/// The paint a part takes; the palette turns each role into a material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Role {
    /// Team paint (the armored body).
    Paint,
    /// Darker team paint (bumpers, rims, launcher, trim).
    Shade,
    /// Bare metal (hinges, bolts, hubs, racks).
    Steel,
    /// Tire rubber.
    Rubber,
    /// Matte black: grille backing, seals, wipers, antennas.
    Gap,
    /// Webbing and canvas stowage.
    Canvas,
    /// Armored glass and optics.
    Glass,
    Headlamp,
    Amber,
    Red,
}

pub(super) const ROLES: [Role; 10] = [
    Role::Paint,
    Role::Shade,
    Role::Steel,
    Role::Rubber,
    Role::Gap,
    Role::Canvas,
    Role::Glass,
    Role::Headlamp,
    Role::Amber,
    Role::Red,
];

/// Each part samples a window of the tileable wear image, offset per part so
/// neighbouring panels do not repeat the same rubbed patches.
const UV_WINDOW_START: [f64; 2] = [0.0, 0.0];
const UV_WINDOW_SIZE: [f64; 2] = [1.0, 1.0];
/// Texture units per model unit when a part is small enough to allow it.
const UV_DENSITY: f64 = 0.3;

/// A rotation from Euler angles (Three's XYZ order) and a translation.
pub(super) fn place(at: [f64; 3], euler: [f64; 3]) -> DMat4 {
    DMat4::from_rotation_translation(
        quat_from_euler(euler[0], euler[1], euler[2]),
        DVec3::from_array(at),
    )
}

/// The frame with the given axes and origin (local x, y, z map to `x`, `y`, `z`).
pub(super) fn frame(x: DVec3, y: DVec3, z: DVec3, origin: DVec3) -> DMat4 {
    DMat4::from_cols(
        x.extend(0.0),
        y.extend(0.0),
        z.extend(0.0),
        origin.extend(1.0),
    )
}

/// Mirror across the x = 0 plane.
pub(super) fn mirror_x() -> DMat4 {
    DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0))
}

/// Shear y along z: `y += slope * (z - z0)`, for surfaces that fall towards the front.
pub(super) fn slope_y(slope: f64, z0: f64) -> DMat4 {
    DMat4::from_cols(
        DVec4::X,
        DVec4::Y,
        DVec4::new(0.0, slope, 1.0, 0.0),
        DVec4::new(0.0, -slope * z0, 0.0, 1.0),
    )
}

/// Hand-built triangles with flat normals.
#[derive(Default)]
pub(super) struct Faces {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
}

impl Faces {
    /// A triangle facing `outward` (its winding is fixed to match).
    pub fn triangle(&mut self, a: DVec3, b: DVec3, c: DVec3, outward: DVec3) {
        let normal = (b - a).cross(c - a);
        let (b, c) = if normal.dot(outward) < 0.0 {
            (c, b)
        } else {
            (b, c)
        };
        let normal = (b - a).cross(c - a).normalize_or_zero();
        for p in [a, b, c] {
            self.positions.push(narrow(p));
            self.normals.push(narrow(normal));
        }
    }

    /// A convex polygon facing `outward`, fanned from its first corner.
    pub fn polygon(&mut self, points: &[DVec3], outward: DVec3) {
        for i in 1..points.len() - 1 {
            self.triangle(points[0], points[i], points[i + 1], outward);
        }
    }

    pub fn mesh(self) -> Mesh {
        let count = self.positions.len();
        Mesh {
            positions: self.positions,
            normals: self.normals,
            uvs: vec![[0.0; 2]; count],
            ..Mesh::default()
        }
    }
}

/// A profile of (radius, axial) points revolved about the local x axis. Each band
/// between two profile points keeps its own normal (crisp machined edges) but is
/// smooth around the axis; `smooth` averages the normals at inner points instead
/// (molded rubber). The profile runs so its outside is on the right of travel in
/// (radius, axial), like `LatheGeometry`.
pub(super) fn revolve(profile: &[[f64; 2]], segments: u32, phase: f64, smooth: bool) -> Mesh {
    let band_normal = |j: usize| {
        let d = DVec2::from(profile[j + 1]) - DVec2::from(profile[j]);
        DVec2::new(d.y, -d.x).normalize_or_zero()
    };
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let point = |[r, a]: [f64; 2], angle: f64| DVec3::new(a, r * angle.cos(), r * angle.sin());
    let normal = |n: DVec2, angle: f64| DVec3::new(n.y, n.x * angle.cos(), n.x * angle.sin());
    for j in 0..profile.len() - 1 {
        let own = band_normal(j);
        let ends = if smooth {
            let before = if j > 0 { band_normal(j - 1) } else { own };
            let after = if j + 2 < profile.len() {
                band_normal(j + 1)
            } else {
                own
            };
            [(own + before).normalize(), (own + after).normalize()]
        } else {
            [own, own]
        };
        for i in 0..segments {
            let angles = [i, i + 1].map(|k| phase + f64::from(k) * 2.0 * PI / f64::from(segments));
            let corners = [
                (point(profile[j], angles[0]), normal(ends[0], angles[0])),
                (point(profile[j], angles[1]), normal(ends[0], angles[1])),
                (point(profile[j + 1], angles[1]), normal(ends[1], angles[1])),
                (point(profile[j + 1], angles[0]), normal(ends[1], angles[0])),
            ];
            for [a, b, c] in [[0, 1, 2], [0, 2, 3]] {
                let (pa, pb, pc) = (corners[a].0, corners[b].0, corners[c].0);
                let facing = corners[a].1 + corners[b].1 + corners[c].1;
                let order = if (pb - pa).cross(pc - pa).dot(facing) < 0.0 {
                    [a, c, b]
                } else {
                    [a, b, c]
                };
                for k in order {
                    positions.push(narrow(corners[k].0));
                    normals.push(narrow(corners[k].1));
                }
            }
        }
    }
    let count = positions.len();
    Mesh {
        positions,
        normals,
        uvs: vec![[0.0; 2]; count],
        ..Mesh::default()
    }
}

/// A flat outline in local x/y (with optional holes) extruded along +z from 0 to
/// `depth`, its edges chamfered by `bevel` without growing past the outline.
pub(super) fn slab(outline: &[[f64; 2]], holes: &[&[[f64; 2]]], depth: f64, bevel: f64) -> Mesh {
    let path = |points: &[[f64; 2]]| {
        let mut path = Path::new();
        for (i, [x, y]) in points.iter().enumerate() {
            if i == 0 {
                path.move_to(*x, *y);
            } else {
                path.line_to(*x, *y);
            }
        }
        path.close_path();
        path
    };
    let mut shape = Shape::new(path(outline));
    shape.holes = holes.iter().map(|hole| path(hole)).collect();
    let bevel = bevel.min(depth * 0.45);
    let options = ExtrudeOptions {
        depth: depth - 2.0 * bevel,
        steps: 1,
        bevel_enabled: bevel > 0.0,
        bevel_thickness: bevel,
        bevel_size: Some(bevel),
        bevel_offset: -bevel,
        bevel_segments: 1,
        ..ExtrudeOptions::default()
    };
    let mut mesh = extrude_geometry(&[shape], &options);
    mesh.translate(0.0, 0.0, bevel);
    mesh
}

/// A closed rectangle outline centred on (`x`, `y`).
pub(super) fn rect(x: f64, y: f64, width: f64, height: f64) -> [[f64; 2]; 4] {
    let (w, h) = (width / 2.0, height / 2.0);
    [
        [x - w, y - h],
        [x + w, y - h],
        [x + w, y + h],
        [x - w, y + h],
    ]
}

/// Parts gathered per role, merged on `finish`.
#[derive(Default)]
pub(super) struct Kit {
    parts: Vec<(Role, Mesh)>,
}

impl Kit {
    /// Add `mesh` moved by `transform`; mirrored transforms keep outward winding.
    pub fn add(&mut self, role: Role, mesh: &Mesh, transform: DMat4) {
        let mut mesh = moved(mesh.to_non_indexed(), transform);
        mesh.colors.clear();
        mesh.attributes.clear();
        part_uvs(&mut mesh, self.parts.len());
        self.parts.push((role, mesh));
    }

    /// Take over another kit's parts (keeping their UVs), moved by `transform`.
    pub fn absorb(&mut self, other: &Kit, transform: DMat4) {
        for (role, mesh) in &other.parts {
            self.parts.push((*role, moved(mesh.clone(), transform)));
        }
    }

    /// A box with chamfered edges (`bevel` 0 gives a plain box).
    pub fn block(&mut self, role: Role, size: [f64; 3], transform: DMat4) {
        let [w, h, d] = size;
        let mut mesh = slab(&rect(0.0, 0.0, w, h), &[], d, 0.0);
        mesh.translate(0.0, 0.0, -d / 2.0);
        self.add(role, &mesh, transform);
    }

    /// A box with chamfered edges.
    pub fn chamfered(&mut self, role: Role, size: [f64; 3], bevel: f64, transform: DMat4) {
        let [w, h, d] = size;
        let mut mesh = slab(&rect(0.0, 0.0, w, h), &[], d, bevel);
        mesh.translate(0.0, 0.0, -d / 2.0);
        self.add(role, &mesh, transform);
    }

    /// A box with rounded edges.
    pub fn rounded(&mut self, role: Role, size: [f64; 3], radius: f64, transform: DMat4) {
        let mesh = rounded_box_geometry(size[0], size[1], size[2], 1, radius);
        self.add(role, &mesh, transform);
    }

    /// A y-axis cylinder.
    pub fn cylinder(&mut self, role: Role, radius: f64, height: f64, sides: u32, transform: DMat4) {
        self.add(
            role,
            &cylinder_geometry(radius, radius, height, sides),
            transform,
        );
    }

    /// A hex bolt head standing on +y from the local origin (no hidden bottom).
    pub fn bolt(&mut self, role: Role, radius: f64, height: f64, transform: DMat4) {
        let mut faces = Faces::default();
        let corner = |i: u32, y: f64| {
            let angle = f64::from(i) * PI / 3.0;
            DVec3::new(radius * angle.cos(), y, radius * angle.sin())
        };
        let top: Vec<DVec3> = (0..6).map(|i| corner(i, height)).collect();
        faces.polygon(&top, DVec3::Y);
        for i in 0..6 {
            let quad = [
                corner(i, 0.0),
                corner(i + 1, 0.0),
                corner(i + 1, height),
                corner(i, height),
            ];
            let mid = quad.iter().sum::<DVec3>() / 4.0;
            faces.polygon(&quad, mid - DVec3::Y * mid.y);
        }
        self.add(role, &faces.mesh(), transform);
    }

    /// A ring in the local x/y plane.
    pub fn torus(&mut self, role: Role, radius: f64, tube: f64, transform: DMat4) {
        self.add(role, &torus_geometry(radius, tube, 4, 8), transform);
    }

    /// A (z, y) side profile extruded across x from `x0` to `x1`, with holes.
    pub fn side_profile(
        &mut self,
        role: Role,
        profile: &[[f64; 2]],
        holes: &[&[[f64; 2]]],
        x0: f64,
        x1: f64,
        bevel: f64,
    ) {
        // Local x/y of the slab become z/y; its depth runs along +x.
        let mesh = slab(profile, holes, x1 - x0, bevel);
        let to_side = frame(DVec3::Z, DVec3::Y, DVec3::X, DVec3::new(x0, 0.0, 0.0));
        self.add(role, &mesh, to_side);
    }

    /// A (x, y) cross-section extruded along z from `z0` to `z1`.
    pub fn section(
        &mut self,
        role: Role,
        profile: &[[f64; 2]],
        z0: f64,
        z1: f64,
        bevel: f64,
        transform: DMat4,
    ) {
        let mut mesh = slab(profile, &[], z1 - z0, bevel);
        mesh.translate(0.0, 0.0, z0);
        self.add(role, &mesh, transform);
    }

    /// One merged mesh per role.
    pub fn finish(self) -> Vec<(Role, Arc<Mesh>)> {
        ROLES
            .iter()
            .filter_map(|role| {
                let parts: Vec<&Mesh> = self
                    .parts
                    .iter()
                    .filter(|(r, _)| r == role)
                    .map(|(_, mesh)| mesh)
                    .collect();
                let merged = merge_geometries(&parts)?;
                (!merged.positions.is_empty()).then(|| (*role, Arc::new(merged)))
            })
            .collect()
    }
}

/// A non-indexed mesh moved by `transform`; mirrored transforms keep outward
/// winding.
fn moved(mut mesh: Mesh, transform: DMat4) -> Mesh {
    mesh.apply_matrix4(&transform);
    if transform.determinant() < 0.0 {
        for corner in (0..mesh.positions.len()).step_by(3) {
            mesh.positions.swap(corner + 1, corner + 2);
            mesh.normals.swap(corner + 1, corner + 2);
            mesh.uvs.swap(corner + 1, corner + 2);
        }
    }
    mesh
}

/// Planar UVs for one part: each face is projected onto the plane it faces most
/// (sides use z/y, tops z/x, ends x/y), scaled so the whole part fits the
/// texture window, and shifted by a per-part offset so neighbours differ.
fn part_uvs(mesh: &mut Mesh, index: usize) {
    let points: Vec<DVec3> = mesh.positions.iter().map(|p| widen(*p)).collect();
    let min = points.iter().copied().fold(DVec3::INFINITY, DVec3::min);
    let max = points.iter().copied().fold(DVec3::NEG_INFINITY, DVec3::max);
    let extent = max - min;
    let (u_extent, v_extent) = (extent.z.max(extent.x), extent.y.max(extent.x));
    let scale = UV_DENSITY
        .min(UV_WINDOW_SIZE[0] / u_extent.max(1e-6))
        .min(UV_WINDOW_SIZE[1] / v_extent.max(1e-6));
    // A golden-ratio walk spreads the parts' samples over the window.
    let walk = |k: f64| (index as f64 * k).fract();
    let start = [
        UV_WINDOW_START[0] + walk(0.618_034) * (UV_WINDOW_SIZE[0] - u_extent * scale),
        UV_WINDOW_START[1] + walk(0.754_878) * (UV_WINDOW_SIZE[1] - v_extent * scale),
    ];
    mesh.uvs = points
        .iter()
        .zip(&mesh.normals)
        .map(|(p, n)| {
            let (d, n) = (*p - min, widen(*n).abs());
            let (u, v) = if n.y >= n.x && n.y >= n.z {
                (d.z, d.x)
            } else if n.x > n.z {
                (d.z, d.y)
            } else {
                (d.x, d.y)
            };
            [(start[0] + u * scale) as f32, (start[1] + v * scale) as f32]
        })
        .collect();
}
