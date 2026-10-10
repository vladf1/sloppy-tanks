//! Conservative reflection culling to the texels visible water can sample.

use crate::camera::{Frustum, PerspectiveCamera, Sphere};
use glam::{Mat4, Vec2, Vec3, Vec4, Vec4Swizzles};
use sloppy_core::geometry::Mesh;

const CLIP_CAPACITY: usize = 16;
const CLIP_PLANES: [Vec4; 6] = [
    Vec4::new(1.0, 0.0, 0.0, 1.0),
    Vec4::new(-1.0, 0.0, 0.0, 1.0),
    Vec4::new(0.0, 1.0, 0.0, 1.0),
    Vec4::new(0.0, -1.0, 0.0, 1.0),
    Vec4::new(0.0, 0.0, 1.0, 0.0),
    Vec4::new(0.0, 0.0, -1.0, 1.0),
];
const WATER_DISTANCE_FLOOR: f32 = 0.001;
/// Added to a cluster's bounding radius before the frustum pre-test, so float
/// rounding never rejects a polygon that clipping would keep a sliver of.
const PRETEST_MARGIN: f32 = 0.01;
/// Consecutive polygons tested as one first: the mesh follows its shore (the creek
/// is a ribbon along its curve), so they lie together and mostly share the answer.
const CLUSTER: usize = 16;
/// Reflection texels around a sampled point that can reach the water's colour:
/// bilinear filtering blends texels whose samples lie up to 1.5 texels away, and a
/// water pixel's centre may sit a little outside the polygon that shades it.
const SAMPLED_TEXELS: f32 = 2.0;
/// Columns and rows of the grid of reflection cells that water samples, one bit
/// each. A cell spans 16 texels of a 512 reflection: fine enough to follow a creek
/// across the view, coarse enough that a sphere tests a few rows.
const MASK_CELLS: usize = u32::BITS as usize;

/// Where visible water samples its reflection, in reflection NDC: a rectangle around
/// all of it, whose crop of the reflection frustum culls first, and the cells of a
/// grid over the reflection that the rectangle of some water polygon touches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReflectionBounds {
    pub min: Vec2,
    pub max: Vec2,
    /// Bit `column` of `cells[row]`, counted from NDC (-1, -1).
    cells: [u32; MASK_CELLS],
}
impl ReflectionBounds {
    fn empty() -> Self {
        Self {
            min: Vec2::splat(f32::INFINITY),
            max: Vec2::splat(f32::NEG_INFINITY),
            cells: [0; MASK_CELLS],
        }
    }

    /// Add the rectangle one water polygon samples, inside the reflection.
    fn add(&mut self, min: Vec2, max: Vec2) {
        self.min = self.min.min(min);
        self.max = self.max.max(max);
        let columns = columns(cell(min.x), cell(max.x));
        for row in cell(min.y)..=cell(max.y) {
            self.cells[row] |= columns;
        }
    }

    pub fn frustum(&self, view_projection: Mat4) -> Frustum {
        let span = self.max - self.min;
        let scale = Vec2::splat(2.0) / span;
        let offset = -(self.max + self.min) / span;
        let crop = Mat4::from_cols(
            Vec4::new(scale.x, 0.0, 0.0, 0.0),
            Vec4::new(0.0, scale.y, 0.0, 0.0),
            Vec4::Z,
            Vec4::new(offset.x, offset.y, 0.0, 1.0),
        );
        Frustum::from_view_projection(&(crop * view_projection))
    }

    /// The sampled cells, for culling what `view_projection` (the reflection's,
    /// without its oblique near plane) draws.
    pub fn mask(&self, view_projection: Mat4) -> ReflectionMask {
        let rows = [0, 1, 3].map(|row| view_projection.row(row));
        ReflectionMask {
            rows,
            reach: Vec3::from_array(rows.map(|row| row.xyz().length())),
            cells: self.cells,
        }
    }
}

/// The reflection cells that water samples, with the projection that draws into
/// them: it drops what the cropped frustum keeps between the bends of a creek or in
/// the dry middle of a basin's ring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReflectionMask {
    /// The view-projection rows that give clip x, y and w.
    rows: [Vec4; 3],
    /// Their lengths in xyz: how far one metre moves clip x, y and w.
    reach: Vec3,
    cells: [u32; MASK_CELLS],
}
impl ReflectionMask {
    /// Whether `sphere` may draw into a cell that water samples. Every point of it
    /// has clip x, y and w within its radius times `reach` of its centre's, so the
    /// quotients of those ranges' ends bound where it projects. A sphere that reaches
    /// the eye's plane, or is not finite, may cover any cell.
    pub fn covers(&self, sphere: &Sphere) -> bool {
        let center = sphere.center.extend(1.0);
        let [x, y, w] = self.rows.map(|row| row.dot(center));
        let reach = self.reach * sphere.radius;
        let (near, far) = (w - reach.z, w + reach.z);
        if near.is_nan() || near <= 0.0 || !Vec3::new(x, y, far).is_finite() {
            return true;
        }
        let cells = |clip: f32, reach: f32| {
            let (low, high) = (clip - reach, clip + reach);
            (
                cell((low / near).min(low / far)),
                cell((high / near).max(high / far)),
            )
        };
        let (left, right) = cells(x, reach.x);
        let (bottom, top) = cells(y, reach.y);
        let columns = columns(left, right);
        self.cells[bottom..=top]
            .iter()
            .any(|row| row & columns != 0)
    }
}

/// The grid cell holding `ndc` on one axis. It never decreases as `ndc` grows, so
/// overlapping ranges share a cell; values past the reflection's edge (and NaN,
/// which the cast takes to 0) land in the edge cell.
fn cell(ndc: f32) -> usize {
    (((ndc + 1.0) * (MASK_CELLS as f32 / 2.0)) as usize).min(MASK_CELLS - 1)
}

/// Bits `first..=last` of a row.
fn columns(first: usize, last: usize) -> u32 {
    (u32::MAX >> (MASK_CELLS - 1 - last)) & (u32::MAX << first)
}

/// Fixed water polygons outside the apron. Map loading clips the mesh once;
/// the per-frame projection and frustum clipping reuse stack storage.
pub struct WaterFootprint {
    polygons: Vec<Vec<Vec4>>,
    /// The bounds of each `CLUSTER` polygons in order: most lie outside the view,
    /// and their polygons are not projected.
    clusters: Vec<Sphere>,
    height: f32,
}
impl WaterFootprint {
    pub fn new(mesh: &Mesh, height: f32, calm_extent: f32) -> Self {
        let mut polygons = Vec::new();
        let sides = [
            Vec4::new(1.0, 0.0, 0.0, -calm_extent),
            Vec4::new(-1.0, 0.0, 0.0, -calm_extent),
            Vec4::new(0.0, 0.0, 1.0, -calm_extent),
            Vec4::new(0.0, 0.0, -1.0, -calm_extent),
        ];
        let count = mesh.indices.as_ref().map_or(mesh.positions.len(), Vec::len);
        for first in (0..count).step_by(3) {
            let triangle: [Vec4; 3] = std::array::from_fn(|corner| {
                let index = mesh
                    .indices
                    .as_ref()
                    .map_or(first + corner, |indices| indices[first + corner] as usize);
                (Vec3::from(mesh.positions[index]) + Vec3::Y * height).extend(1.0)
            });
            // These half planes cover the outside of the apron. A side that keeps
            // the whole triangle covers what the others would keep of it; otherwise
            // corner pieces overlap, which is harmless for a conservative query.
            let whole = sides
                .iter()
                .any(|side| triangle.iter().all(|&point| side.dot(point) >= 0.0));
            if calm_extent <= 0.0 || whole {
                polygons.push(triangle.to_vec());
                continue;
            }
            for side in sides {
                let mut clipped = [Vec4::ZERO; CLIP_CAPACITY];
                let count = clip_polygon(&triangle, side, &mut clipped);
                if count >= 3 {
                    polygons.push(clipped[..count].to_vec());
                }
            }
        }
        let clusters = polygons
            .chunks(CLUSTER)
            .map(|cluster| {
                let bounds = Sphere::from_points(cluster.iter().flatten().map(|point| point.xyz()));
                Sphere {
                    radius: bounds.radius + PRETEST_MARGIN,
                    ..bounds
                }
            })
            .collect();
        Self {
            polygons,
            clusters,
            height,
        }
    }

    pub fn reflection_bounds(
        &self,
        camera: &PerspectiveCamera,
        distortion: f32,
        reflection_size: u32,
    ) -> Option<ReflectionBounds> {
        let matrix = camera.view_projection();
        let view = Frustum::from_view_projection(&matrix);
        // A normalized normal's x/z components are at most one, and distance
        // to a water point is at least distance to its plane. This bounds the
        // shader's distortion in texture coordinates, which span half of NDC.
        let distance = (camera.position.y - self.height)
            .abs()
            .max(WATER_DISTANCE_FLOOR);
        let padding = Vec2::splat(
            2.0 * (distortion.abs() * (0.001 + distance.recip())
                + SAMPLED_TEXELS / reflection_size.max(1) as f32),
        );
        let mut bounds = ReflectionBounds::empty();
        let mut points = [Vec4::ZERO; CLIP_CAPACITY];
        let mut scratch = [Vec4::ZERO; CLIP_CAPACITY];
        let clusters = self.clusters.iter().zip(self.polygons.chunks(CLUSTER));
        for (cluster, polygons) in clusters {
            if !view.intersects_sphere(cluster) {
                continue;
            }
            for polygon in polygons {
                let Some((min, max)) = screen_bounds(&matrix, polygon, &mut points, &mut scratch)
                else {
                    continue;
                };
                // Water at screen x samples the reflection at -x.
                bounds.add(
                    (Vec2::new(-max.x, min.y) - padding).max(Vec2::NEG_ONE),
                    (Vec2::new(-min.x, max.y) + padding).min(Vec2::ONE),
                );
            }
        }
        bounds.min.is_finite().then_some(bounds)
    }
}

/// The NDC rectangle of the part of `polygon` inside the view of `matrix`, if any.
/// Only the planes some corner lies outside clip it: most polygons lie wholly
/// inside the view or wholly outside one plane.
fn screen_bounds(
    matrix: &Mat4,
    polygon: &[Vec4],
    points: &mut [Vec4; CLIP_CAPACITY],
    scratch: &mut [Vec4; CLIP_CAPACITY],
) -> Option<(Vec2, Vec2)> {
    // Bit `i`: every corner (`all`) or some corner (`any`) lies outside plane `i`.
    let mut all = u8::MAX;
    let mut any = 0;
    for (into, &point) in points.iter_mut().zip(polygon) {
        *into = *matrix * point;
        let outside = outside_planes(*into);
        all &= outside;
        any |= outside;
    }
    if all != 0 {
        return None;
    }
    let (mut points, mut scratch) = (points, scratch);
    let mut count = polygon.len();
    for (index, &plane) in CLIP_PLANES.iter().enumerate() {
        if any & (1 << index) != 0 {
            count = clip_polygon(&points[..count], plane, scratch);
            std::mem::swap(&mut points, &mut scratch);
        }
    }
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    for point in &points[..count] {
        let screen = point.xy() / point.w;
        min = min.min(screen);
        max = max.max(screen);
    }
    (count > 0).then_some((min, max))
}

/// Bit `i` set when `point` lies outside `CLIP_PLANES[i]`, as `clip_polygon` decides.
fn outside_planes(point: Vec4) -> u8 {
    CLIP_PLANES
        .iter()
        .enumerate()
        .fold(0, |outside, (index, plane)| {
            outside | (u8::from(plane.dot(point) < 0.0) << index)
        })
}

fn clip_polygon(points: &[Vec4], plane: Vec4, output: &mut [Vec4; CLIP_CAPACITY]) -> usize {
    let Some(&last) = points.last() else {
        return 0;
    };
    let mut previous = last;
    let mut before = plane.dot(previous);
    let mut count = 0;
    for &point in points {
        let after = plane.dot(point);
        if (before >= 0.0) != (after >= 0.0) {
            output[count] = previous.lerp(point, before / (before - after));
            count += 1;
        }
        if after >= 0.0 {
            output[count] = point;
            count += 1;
        }
        previous = point;
        before = after;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::{Sphere, mirror_view};
    use sloppy_core::geometry::plane_geometry;
    fn water() -> Mesh {
        let mut mesh = plane_geometry(100.0, 100.0);
        mesh.rotate_x(-std::f64::consts::FRAC_PI_2);
        mesh
    }
    /// A creek-like strip of small triangles winding along x.
    fn creek() -> Mesh {
        let mut positions = Vec::new();
        for step in 0..120 {
            let x = -150.0 + step as f32 * 2.5;
            let z = 30.0 * (x * 0.03).sin();
            positions.extend([
                [x, 0.0, z - 4.0],
                [x + 2.5, 0.0, z - 4.0],
                [x, 0.0, z + 4.0],
            ]);
            positions.extend([
                [x + 2.5, 0.0, z - 4.0],
                [x + 2.5, 0.0, z + 4.0],
                [x, 0.0, z + 4.0],
            ]);
        }
        Mesh {
            positions,
            ..Mesh::default()
        }
    }
    fn camera(position: Vec3, target: Vec3) -> PerspectiveCamera {
        let mut camera = PerspectiveCamera::new(43.0, 0.1, 320.0);
        camera.position = position;
        camera.target = target;
        camera.aspect = 1.6;
        camera
    }
    /// The game's overview of the arena and a lower follow camera at its default
    /// zoom (`camera_rig.rs`).
    fn overview() -> PerspectiveCamera {
        camera(Vec3::new(0.0, 100.4, 77.8), Vec3::ZERO)
    }
    fn follow(target: Vec3) -> PerspectiveCamera {
        camera(target + Vec3::new(0.0, 31.6, 24.5), target)
    }
    /// The reflection's cropped frustum and mask, as the renderer culls with them.
    fn reflection_cull(
        water: &WaterFootprint,
        view: &PerspectiveCamera,
    ) -> Option<(Frustum, ReflectionMask, Mat4)> {
        let bounds = water.reflection_bounds(view, 0.65, 512)?;
        let (mirror, _) = mirror_view(view, water.height)?;
        let plain = view.projection() * mirror;
        Some((bounds.frustum(plain), bounds.mask(plain), plain))
    }
    fn reflected(cull: &(Frustum, ReflectionMask, Mat4), sphere: &Sphere) -> bool {
        cull.0.intersects_sphere(sphere) && cull.1.covers(sphere)
    }
    fn sphere(x: f32, y: f32, z: f32, radius: f32) -> Sphere {
        Sphere {
            center: Vec3::new(x, y, z),
            radius,
        }
    }
    #[test]
    fn sampling_mirrors_the_visible_water_rectangle_in_x() {
        let mesh = Mesh {
            positions: vec![[10.0, 0.0, 0.0], [20.0, 0.0, 0.0], [15.0, 0.0, -5.0]],
            ..Mesh::default()
        };
        let water = WaterFootprint::new(&mesh, 0.0, 0.0);
        let view = camera(Vec3::new(0.0, 30.0, 0.0), Vec3::new(0.0, 0.0, -5.0));
        let bounds = water.reflection_bounds(&view, 0.0, 512).unwrap();
        assert!(
            bounds.max.x < 0.0,
            "water to the right samples the left of the reflection"
        );
    }
    #[test]
    fn a_triangle_crossing_the_near_plane_is_clipped_to_a_quad() {
        let triangle = [
            Vec4::new(-0.5, -0.5, -0.1, 1.0),
            Vec4::new(0.5, -0.5, 0.5, 1.0),
            Vec4::new(0.0, 0.5, 0.5, 1.0),
        ];
        let mut clipped = [Vec4::ZERO; CLIP_CAPACITY];
        let count = clip_polygon(&triangle, CLIP_PLANES[4], &mut clipped);
        assert_eq!(count, 4);
        assert!(
            clipped[..count]
                .iter()
                .all(|point| point.z >= -f32::EPSILON)
        );
    }
    #[test]
    fn a_triangle_past_two_sides_of_the_apron_is_kept_once() {
        let corner = Mesh {
            positions: vec![[50.0, 0.0, 50.0], [60.0, 0.0, 50.0], [50.0, 0.0, 60.0]],
            ..Mesh::default()
        };
        assert_eq!(WaterFootprint::new(&corner, 0.0, 40.0).polygons.len(), 1);
        // One straddling a side keeps only its outer piece.
        let edge = Mesh {
            positions: vec![[30.0, 0.0, 0.0], [50.0, 0.0, 0.0], [30.0, 0.0, 10.0]],
            ..Mesh::default()
        };
        let water = WaterFootprint::new(&edge, 0.0, 40.0);
        assert_eq!(water.polygons.len(), 1);
        assert!(water.polygons[0].iter().all(|point| point.x >= 40.0));
    }
    #[test]
    fn a_dry_view_has_no_water_bounds() {
        let water = WaterFootprint::new(&water(), -2.0, 40.0);
        let camera = camera(Vec3::new(0.0, 12.0, 0.0), Vec3::new(0.0, 0.0, -0.1));
        assert_eq!(water.reflection_bounds(&camera, 1.8, 512), None);
    }
    #[test]
    fn cropping_and_masking_keep_the_sampled_rectangles_only() {
        let mut bounds = ReflectionBounds::empty();
        bounds.add(Vec2::new(-1.0, 0.0), Vec2::new(-0.8, 1.0));
        bounds.add(Vec2::new(-0.2, 0.0), Vec2::new(0.0, 1.0));
        // Clip space is this "world": w is 1 everywhere.
        let frustum = bounds.frustum(Mat4::IDENTITY);
        let mask = bounds.mask(Mat4::IDENTITY);
        let at = |x, y| sphere(x, y, 0.5, 0.01);
        assert!(frustum.intersects_sphere(&at(-0.9, 0.5)) && mask.covers(&at(-0.9, 0.5)));
        assert!(frustum.intersects_sphere(&at(-0.1, 0.5)) && mask.covers(&at(-0.1, 0.5)));
        assert!(!frustum.intersects_sphere(&at(0.5, 0.5)));
        assert!(!frustum.intersects_sphere(&at(-0.5, -0.5)));
        // Between the rectangles: inside the crop, outside every sampled cell.
        assert!(frustum.intersects_sphere(&at(-0.5, 0.5)) && !mask.covers(&at(-0.5, 0.5)));
        assert!(mask.covers(&sphere(-0.5, 0.5, 0.5, 0.35)));
        assert!(mask.covers(&at(f32::INFINITY, 0.5)) && mask.covers(&at(f32::NAN, 0.5)));
    }
    #[test]
    fn water_behind_the_eye_is_clipped_before_dividing_by_w() {
        let water = WaterFootprint::new(&water(), -2.0, 0.0);
        let camera = camera(Vec3::new(0.0, 8.0, 100.0), Vec3::new(0.0, 8.0, 120.0));
        assert_eq!(water.reflection_bounds(&camera, 1.8, 512), None);
    }
    #[test]
    fn skipping_clusters_outside_the_view_keeps_the_bounds() {
        // The strip lies mostly outside any one view.
        let water = WaterFootprint::new(&creek(), -2.0, 40.0);
        let mut everything = WaterFootprint::new(&creek(), -2.0, 40.0);
        for bounds in &mut everything.clusters {
            bounds.radius = f32::INFINITY;
        }
        let mut compared = 0;
        for step in 0..64 {
            let angle = step as f32 * 0.37;
            let eye = Vec3::new(
                90.0 * angle.cos(),
                20.0 + (step % 5) as f32 * 9.0,
                90.0 * angle.sin(),
            );
            let target = Vec3::new(30.0 * (angle * 1.7).sin(), 0.0, 20.0 * angle.cos());
            let view = camera(eye, target);
            let expected = everything.reflection_bounds(&view, 0.65, 512);
            assert_eq!(water.reflection_bounds(&view, 0.65, 512), expected);
            compared += expected.is_some() as usize;
        }
        assert!(compared > 16, "most poses see water");
    }
    #[test]
    fn the_reflection_keeps_whatever_visible_water_samples() {
        // Every visible water point samples the reflection at its mirrored screen
        // position; whatever the reflection draws along that ray must stay.
        let water = WaterFootprint::new(&creek(), -2.0, 40.0);
        let mut checked = 0;
        for view in [
            overview(),
            follow(Vec3::new(-60.0, 0.7, -20.0)),
            follow(Vec3::new(70.0, 0.7, 30.0)),
            camera(Vec3::new(-120.0, 25.0, 60.0), Vec3::new(-40.0, 0.0, 0.0)),
        ] {
            let cull = reflection_cull(&water, &view).expect("the view sees water");
            let main = view.view_projection();
            let unproject = cull.2.inverse();
            for polygon in &water.polygons {
                let center = polygon.iter().sum::<Vec4>() / polygon.len() as f32;
                for point in polygon.iter().map(|&corner| corner.lerp(center, 0.01)) {
                    let clip = main * point;
                    let ndc = clip.xyz() / clip.w;
                    if clip.w <= 0.0 || ndc.x.abs() > 1.0 || ndc.y.abs() > 1.0 || ndc.z > 1.0 {
                        continue;
                    }
                    let near = unproject.project_point3(Vec3::new(-ndc.x, ndc.y, 0.0));
                    let far = unproject.project_point3(Vec3::new(-ndc.x, ndc.y, 1.0));
                    for along in [0.001, 0.01, 0.1, 0.5, 0.95] {
                        let point = near.lerp(far, along);
                        assert!(
                            reflected(&cull, &sphere(point.x, point.y, point.z, 0.0)),
                            "{point} reflects onto water {ndc}"
                        );
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 2000, "{checked}");
    }
    #[test]
    fn the_mask_drops_what_lies_between_creek_bends() {
        let water = WaterFootprint::new(&creek(), -2.0, 0.0);
        let cull = reflection_cull(&water, &overview()).unwrap();
        // The creek peaks at z = 30 here and dips at x = -52 and 157.
        let between = sphere(52.0, 2.0, -25.0, 3.0);
        assert!(cull.0.intersects_sphere(&between), "inside the rectangle");
        assert!(!cull.1.covers(&between));
        // Looking down, the water mirrors its far bank.
        assert!(
            reflected(&cull, &sphere(52.0, 4.0, 21.0, 4.0)),
            "on the bank"
        );
        assert!(
            reflected(&cull, &sphere(0.0, 2.0, 0.0, 3.0)),
            "over the water"
        );
        assert!(
            reflected(&cull, &sphere(52.0, 2.0, -25.0, 60.0)),
            "reaching it"
        );
    }
    #[test]
    fn the_mask_drops_the_dry_middle_of_a_basin_ring() {
        let mut harbor = plane_geometry(340.0, 340.0);
        harbor.rotate_x(-std::f64::consts::FRAC_PI_2);
        let water = WaterFootprint::new(&harbor, -2.2, 62.0);
        let cull = reflection_cull(&water, &overview()).unwrap();
        let middle = sphere(0.0, 2.0, 0.0, 4.0);
        assert!(cull.0.intersects_sphere(&middle) && !cull.1.covers(&middle));
        assert!(
            reflected(&cull, &sphere(0.0, 2.0, -75.0, 4.0)),
            "past the far quay"
        );
        assert!(
            reflected(&cull, &sphere(-70.0, 2.0, 0.0, 4.0)),
            "past the side quay"
        );
    }

    #[test]
    fn distortion_expands_the_rectangle_and_close_water_keeps_everything() {
        let water = WaterFootprint::new(&water(), 0.0, 0.0);
        let view = camera(Vec3::new(0.0, 10.0, 70.0), Vec3::ZERO);
        let plain = water.reflection_bounds(&view, 0.0, 512).unwrap();
        let padded = water.reflection_bounds(&view, 1.8, 512).unwrap();
        assert!(padded.min.cmple(plain.min).all());
        assert!(padded.max.cmpge(plain.max).all());
        let view = camera(Vec3::new(0.0, 0.01, 40.0), Vec3::ZERO);
        let padded = water.reflection_bounds(&view, 1.8, 512).unwrap();
        assert_eq!(padded.min, Vec2::splat(-1.0));
        assert_eq!(padded.max, Vec2::ONE);
    }
}
