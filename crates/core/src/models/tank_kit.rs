//! A builder that merges the static parts of an assembly (a tank's hull, turret
//! or gun, a building) into one flat-shaded mesh per finish, so armor plates,
//! hatches, window frames and trim cost one draw per finish rather than one per
//! part.
//!
//! Parts are added in the assembly's frame (metres, x left, y up, z forward).
//! Solids are lofted through closed rings of points; their faces are oriented
//! away from the ring centroid, so callers only describe convex solids.

use std::f64::consts::PI;
use std::marker::PhantomData;
use std::sync::Arc;

use glam::{DMat4, DVec2, DVec3};

use crate::geometry::math::{compose, quat_from_euler, quat_from_unit_vectors};
use crate::geometry::{Mesh, triangulate_shape};

/// Planar texture density of merged parts: the wear texture repeats about every
/// 2.2 m, close to the scale it had on the original unit-mapped armor blocks.
const UV_PER_METRE: f64 = 0.45;
/// Triangles smaller than this (squared doubled area) are dropped as degenerate.
const MIN_AREA: f64 = 1e-14;

/// The finish a merged part is painted with; the tank model maps each to its
/// chassis material.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Coat {
    /// Darkened team paint: lower hull, add-on armor, frames.
    Shade,
    /// Team paint: the main armor.
    Paint,
    /// Bare worn metal: tow eyes, track links, tool heads.
    Steel,
    /// Unpainted dark openings, grille recesses and rubber.
    Dark,
    /// Blued gun metal of machine guns and cable.
    Gunmetal,
    /// Sight and vision-block glass.
    Glass,
    /// Olive tarps, bags and dust covers.
    Canvas,
    /// Tool handles.
    Wood,
    /// White team insignia.
    Marking,
    /// Red tail-light lenses.
    TailLight,
}

fn finish_index<C: Finishes>(finish: C) -> usize {
    C::ALL
        .iter()
        .position(|listed| *listed == finish)
        .expect("ALL lists every finish")
}

/// The finishes one kind of assembly is painted with, in output order.
pub(super) trait Finishes: Copy + PartialEq + 'static {
    const ALL: &'static [Self];
}

impl Finishes for Coat {
    /// Output order of merged meshes. Shade comes first so the hull's lower armor
    /// and its team paint keep their places as the hull's first two children.
    const ALL: &'static [Self] = &[
        Coat::Shade,
        Coat::Paint,
        Coat::Steel,
        Coat::Dark,
        Coat::Gunmetal,
        Coat::Glass,
        Coat::Canvas,
        Coat::Wood,
        Coat::Marking,
        Coat::TailLight,
    ];
}

/// Merged meshes of one assembly, in [`Finishes::ALL`] order, empty finishes left
/// out.
pub(super) type KitMeshes<C = Coat> = Vec<(C, Arc<Mesh>)>;

/// Per-finish triangle soups: positions and normals, three vertices per triangle.
pub(super) struct Kit<C: Finishes = Coat> {
    buffers: Vec<Vec<(DVec3, DVec3)>>,
    finishes: PhantomData<C>,
}

/// A rigid placement: translate to `at` after rotating by Euler angles (XYZ).
pub(super) fn pose(at: DVec3, euler: DVec3) -> DMat4 {
    compose(at, quat_from_euler(euler.x, euler.y, euler.z), DVec3::ONE)
}

/// A placement that only moves to `at`.
pub(super) fn shift(at: DVec3) -> DMat4 {
    DMat4::from_translation(at)
}

/// A placement whose local +z points from `from` toward `to`, starting at `from`.
pub(super) fn aim(from: DVec3, to: DVec3) -> DMat4 {
    let direction = (to - from).normalize();
    compose(
        from,
        quat_from_unit_vectors(DVec3::Z, direction),
        DVec3::ONE,
    )
}

/// A plan outline (x, z) as a ring at height `y`.
pub(super) fn ring(outline: &[DVec2], y: f64) -> Vec<DVec3> {
    outline.iter().map(|p| DVec3::new(p.x, y, p.y)).collect()
}

/// A plan outline at a height that varies along z: `y(z)`.
pub(super) fn sloped_ring(outline: &[DVec2], y: impl Fn(f64) -> f64) -> Vec<DVec3> {
    outline
        .iter()
        .map(|p| DVec3::new(p.x, y(p.y), p.y))
        .collect()
}

/// A polygon moved inward by `distance` (mitred corners). Works for either
/// winding; the polygon must stay simple at that distance.
pub(super) fn inset(polygon: &[DVec2], distance: f64) -> Vec<DVec2> {
    let n = polygon.len();
    let area: f64 = (0..n)
        .map(|i| polygon[i].perp_dot(polygon[(i + 1) % n]))
        .sum();
    let sign = area.signum();
    let inward = |a: DVec2, b: DVec2| (b - a).normalize().perp() * sign;
    (0..n)
        .map(|i| {
            let (prev, here, next) = (polygon[(i + n - 1) % n], polygon[i], polygon[(i + 1) % n]);
            let (n1, n2) = (inward(prev, here), inward(here, next));
            here + (n1 + n2) * (distance / (1.0 + n1.dot(n2)))
        })
        .collect()
}

/// Rings of a prism along x through a side profile of (z, y) points, with its
/// two end faces chamfered by `bevel`.
pub(super) fn prism_x(profile: &[DVec2], half_width: f64, bevel: f64) -> Vec<Vec<DVec3>> {
    let chamfered = inset(profile, bevel);
    let at = |points: &[DVec2], x: f64| -> Vec<DVec3> {
        points.iter().map(|p| DVec3::new(x, p.y, p.x)).collect()
    };
    vec![
        at(&chamfered, -half_width),
        at(profile, -half_width + bevel),
        at(profile, half_width - bevel),
        at(&chamfered, half_width),
    ]
}

/// Rings of a plan solid whose roof edge is chamfered by `bevel`: the outline at
/// each level, with the roof drawn `bevel` inside the top level and above it.
pub(super) fn chamfered_levels(levels: &[(Vec<DVec2>, f64)], bevel: f64) -> Vec<Vec<DVec3>> {
    let mut rings: Vec<Vec<DVec3>> = levels.iter().map(|(o, y)| ring(o, *y)).collect();
    let (top, y) = levels.last().expect("a solid has levels");
    rings.push(ring(&inset(top, bevel), y + bevel));
    rings
}

/// Planar texture coordinates of `p` on a face whose normal (or any vector along
/// it) is `facing`: the two coordinates across its dominant axis.
pub(super) fn planar_uv(p: DVec3, facing: DVec3) -> (f64, f64) {
    let n = facing.abs();
    if n.y >= n.x && n.y >= n.z {
        (p.x, p.z)
    } else if n.x >= n.z {
        (p.z, p.y)
    } else {
        (p.x, p.y)
    }
}

impl<C: Finishes> Kit<C> {
    pub(super) fn new() -> Self {
        Self {
            buffers: vec![Vec::new(); C::ALL.len()],
            finishes: PhantomData,
        }
    }

    /// One triangle with explicit vertex normals; reversed if its winding
    /// disagrees with them.
    fn triangle(&mut self, coat: C, mut p: [DVec3; 3], mut n: [DVec3; 3]) {
        let face = (p[1] - p[0]).cross(p[2] - p[0]);
        if face.length_squared() < MIN_AREA {
            return;
        }
        if face.dot(n[0] + n[1] + n[2]) < 0.0 {
            p.swap(1, 2);
            n.swap(1, 2);
        }
        let buffer = &mut self.buffers[finish_index(coat)];
        buffer.extend(p.into_iter().zip(n));
    }

    /// A flat triangle facing away from `inside` (degenerate ones are dropped by
    /// `triangle`).
    fn flat(&mut self, coat: C, p: [DVec3; 3], inside: DVec3) {
        let face = (p[1] - p[0]).cross(p[2] - p[0]);
        let center = (p[0] + p[1] + p[2]) / 3.0;
        let normal = face.normalize() * (center - inside).dot(face).signum();
        self.triangle(coat, p, [normal; 3]);
    }

    /// A convex solid lofted through closed rings of equal length, capped at both
    /// ends. `skip` leaves out one side quad (band, edge) for a custom face.
    pub(super) fn solid_skipping(
        &mut self,
        coat: C,
        rings: &[Vec<DVec3>],
        skip: Option<(usize, usize)>,
    ) {
        let count: usize = rings.iter().map(Vec::len).sum();
        let inside = rings.iter().flatten().copied().sum::<DVec3>() / count as f64;
        for band in 0..rings.len() - 1 {
            let (lower, upper) = (&rings[band], &rings[band + 1]);
            let n = lower.len();
            for i in 0..n {
                if skip == Some((band, i)) {
                    continue;
                }
                let j = (i + 1) % n;
                self.flat(coat, [lower[i], lower[j], upper[j]], inside);
                self.flat(coat, [lower[i], upper[j], upper[i]], inside);
            }
        }
        for cap in [&rings[0], &rings[rings.len() - 1]] {
            for i in 1..cap.len() - 1 {
                self.flat(coat, [cap[0], cap[i], cap[i + 1]], inside);
            }
        }
    }

    pub(super) fn solid(&mut self, coat: C, rings: &[Vec<DVec3>]) {
        self.solid_skipping(coat, rings, None);
    }

    /// A single-sided quad (two triangles) facing along `facing`.
    pub(super) fn quad(&mut self, coat: C, corners: [DVec3; 4], facing: DVec3) {
        let normal = facing.normalize();
        let [a, b, c, d] = corners;
        self.triangle(coat, [a, b, c], [normal; 3]);
        self.triangle(coat, [a, c, d], [normal; 3]);
    }

    /// A horizontal rectangle at height `y` facing up, with a round hole.
    pub(super) fn plate_with_hole(
        &mut self,
        coat: C,
        corners: [DVec2; 2],
        y: f64,
        hole_center: DVec2,
        hole_radius: f64,
        hole_points: u32,
    ) {
        let [lo, hi] = corners;
        let mut contour = vec![
            DVec2::new(lo.x, lo.y),
            DVec2::new(hi.x, lo.y),
            DVec2::new(hi.x, hi.y),
            DVec2::new(lo.x, hi.y),
        ];
        let hole: Vec<DVec2> = (0..hole_points)
            .map(|i| {
                let angle = f64::from(i) / f64::from(hole_points) * PI * 2.0;
                hole_center + DVec2::new(angle.cos(), angle.sin()) * hole_radius
            })
            .collect();
        let points: Vec<DVec2> = contour.iter().chain(&hole).copied().collect();
        for [a, b, c] in triangulate_shape(&mut contour, &mut [hole]) {
            let p = [points[a], points[b], points[c]].map(|p| DVec3::new(p.x, y, p.y));
            self.triangle(coat, p, [DVec3::Y; 3]);
        }
    }

    /// A flat face in `placement`'s local xy plane, facing local +z: `outline`
    /// with `holes` cut through it (both simple polygons).
    pub(super) fn face_with_holes(
        &mut self,
        coat: C,
        outline: &[DVec2],
        holes: &[Vec<DVec2>],
        placement: DMat4,
    ) {
        let mut contour = outline.to_vec();
        let mut holes = holes.to_vec();
        let triangles = triangulate_shape(&mut contour, &mut holes);
        let points: Vec<DVec2> = contour
            .iter()
            .chain(holes.iter().flatten())
            .copied()
            .collect();
        let normal = placement.transform_vector3(DVec3::Z).normalize();
        for [a, b, c] in triangles {
            let p = [points[a], points[b], points[c]]
                .map(|p| placement.transform_point3(DVec3::new(p.x, p.y, 0.0)));
            self.triangle(coat, p, [normal; 3]);
        }
    }

    /// A box of `size` centred on the placement's origin.
    pub(super) fn block(&mut self, coat: C, size: DVec3, placement: DMat4) {
        let h = size / 2.0;
        let outline =
            [(-h.x, -h.z), (h.x, -h.z), (h.x, h.z), (-h.x, h.z)].map(|(x, z)| DVec2::new(x, z));
        let rings = [ring(&outline, -h.y), ring(&outline, h.y)].map(|r| {
            r.into_iter()
                .map(|p| placement.transform_point3(p))
                .collect()
        });
        self.solid(coat, &rings);
    }

    /// `block` placed at a point with Euler angles.
    pub(super) fn block_at(&mut self, coat: C, size: DVec3, at: DVec3, euler: DVec3) {
        self.block(coat, size, pose(at, euler));
    }

    /// A box whose top and bottom edges are chamfered by `bevel`, catching a
    /// highlight along the lid and keeping soft loads from looking boxed.
    pub(super) fn chamfer_block(&mut self, coat: C, size: DVec3, bevel: f64, placement: DMat4) {
        let h = size / 2.0;
        let outline =
            [(h.x, -h.z), (h.x, h.z), (-h.x, h.z), (-h.x, -h.z)].map(|(x, z)| DVec2::new(x, z));
        let narrow = inset(&outline, bevel);
        let rings = [
            ring(&narrow, -h.y),
            ring(&outline, -h.y + bevel),
            ring(&outline, h.y - bevel),
            ring(&narrow, h.y),
        ]
        .map(|r| {
            r.into_iter()
                .map(|p| placement.transform_point3(p))
                .collect()
        });
        self.solid(coat, &rings);
    }

    /// A surface of revolution about local +z through a (radius, z) profile, with
    /// smooth normals around the axis and hard edges between profile segments.
    /// Profile points run from back to front; a radius of zero closes an end.
    pub(super) fn turned(&mut self, coat: C, profile: &[DVec2], sides: u32, placement: DMat4) {
        for pair in profile.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let d = b - a;
            if d.length_squared() < MIN_AREA {
                continue;
            }
            // Outward when the profile runs forward along the outer wall.
            let normal = DVec2::new(d.y, -d.x).normalize();
            for k in 0..sides {
                let angles = [k, k + 1].map(|s| f64::from(s) / f64::from(sides) * PI * 2.0);
                let corner =
                    |p: DVec2, angle: f64| DVec3::new(p.x * angle.cos(), p.x * angle.sin(), p.y);
                let normal_at = |angle: f64| {
                    DVec3::new(normal.x * angle.cos(), normal.x * angle.sin(), normal.y)
                };
                let points = [
                    corner(a, angles[0]),
                    corner(a, angles[1]),
                    corner(b, angles[1]),
                    corner(b, angles[0]),
                ]
                .map(|p| placement.transform_point3(p));
                let normals = [
                    normal_at(angles[0]),
                    normal_at(angles[1]),
                    normal_at(angles[1]),
                    normal_at(angles[0]),
                ]
                .map(|n| placement.transform_vector3(n).normalize());
                self.triangle(
                    coat,
                    [points[0], points[1], points[2]],
                    [normals[0], normals[1], normals[2]],
                );
                self.triangle(
                    coat,
                    [points[0], points[2], points[3]],
                    [normals[0], normals[2], normals[3]],
                );
            }
        }
    }

    /// A closed cylinder of `radius` from `from` to `to`.
    pub(super) fn rod(&mut self, coat: C, radius: f64, from: DVec3, to: DVec3, sides: u32) {
        let length = (to - from).length();
        let profile = [(0.0, 0.0), (radius, 0.0), (radius, length), (0.0, length)]
            .map(|(r, z)| DVec2::new(r, z));
        self.turned(coat, &profile, sides, aim(from, to));
    }

    /// An upright closed cylinder standing on `base`.
    pub(super) fn post(&mut self, coat: C, radius: f64, height: f64, base: DVec3, sides: u32) {
        self.rod(coat, radius, base, base + DVec3::Y * height, sides);
    }

    /// Merge every coat into one non-indexed mesh with planar UVs projected along
    /// each triangle's dominant axis.
    pub(super) fn finish(self) -> KitMeshes<C> {
        self.finish_scaled(|_| UV_PER_METRE)
    }

    /// [`Self::finish`] with each finish's own texture density (repeats per metre),
    /// for finishes whose texture has a real-world scale (siding, brick, stone).
    pub(super) fn finish_scaled(self, uv_per_metre: impl Fn(C) -> f64) -> KitMeshes<C> {
        let mut meshes = Vec::new();
        for (&coat, buffer) in C::ALL.iter().zip(self.buffers) {
            let scale = uv_per_metre(coat);
            if buffer.is_empty() {
                continue;
            }
            let mut mesh = Mesh::default();
            for triangle in buffer.as_chunks::<3>().0 {
                let [a, b, c] = [triangle[0].0, triangle[1].0, triangle[2].0];
                let face = (b - a).cross(c - a);
                for (p, n) in triangle {
                    let (u, v) = planar_uv(*p, face);
                    mesh.positions.push([p.x as f32, p.y as f32, p.z as f32]);
                    mesh.normals.push([n.x as f32, n.y as f32, n.z as f32]);
                    mesh.uvs
                        .push([(u * scale + 0.5) as f32, (v * scale + 0.5) as f32]);
                }
            }
            meshes.push((coat, Arc::new(mesh)));
        }
        meshes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outward_faces(mesh: &Mesh, center: DVec3) -> bool {
        mesh.positions.as_chunks::<3>().0.iter().all(|t| {
            let p = t.map(|v| DVec3::new(f64::from(v[0]), f64::from(v[1]), f64::from(v[2])));
            let face = (p[1] - p[0]).cross(p[2] - p[0]);
            face.dot((p[0] + p[1] + p[2]) / 3.0 - center) > 0.0
        })
    }

    #[test]
    fn chamfered_blocks_are_closed_and_face_outward() {
        let mut kit = Kit::new();
        kit.chamfer_block(
            Coat::Paint,
            DVec3::new(1.0, 0.5, 2.0),
            0.05,
            pose(DVec3::new(1.0, 2.0, 3.0), DVec3::new(0.0, 0.4, 0.0)),
        );
        let meshes = kit.finish();
        assert_eq!(meshes.len(), 1);
        let mesh = &meshes[0].1;
        // 4 sides x 3 bands x 2 + 2 caps x 2.
        assert_eq!(mesh.triangle_count(), 28);
        assert!(outward_faces(mesh, DVec3::new(1.0, 2.0, 3.0)));
        let bounds = mesh.bounding_box();
        assert!((bounds.max.y - 2.25).abs() < 1e-6 && (bounds.min.y - 1.75).abs() < 1e-6);
    }

    #[test]
    fn turned_profiles_face_outward() {
        let mut kit = Kit::new();
        kit.rod(Coat::Steel, 0.1, DVec3::ZERO, DVec3::new(0.0, 0.0, 1.0), 8);
        let meshes = kit.finish();
        let mesh = &meshes[0].1;
        // 8 sides plus two 8-triangle caps.
        assert_eq!(mesh.triangle_count(), 32);
        assert!(outward_faces(mesh, DVec3::new(0.0, 0.0, 0.5)));
    }

    #[test]
    fn insets_move_edges_inward_by_the_distance() {
        let square =
            [(1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0)].map(|(x, z)| DVec2::new(x, z));
        let inner = inset(&square, 0.1);
        for p in inner {
            assert!((p.x.abs() - 0.9).abs() < 1e-9 && (p.y.abs() - 0.9).abs() < 1e-9);
        }
    }
}
