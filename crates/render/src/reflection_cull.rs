//! Conservative reflection culling to the texels visible water can sample.

use crate::camera::{Frustum, PerspectiveCamera, Sphere};
use glam::{Mat4, Vec2, Vec3, Vec4};
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
/// Added to a polygon's bounding radius before the frustum pre-test, so float
/// rounding never rejects a polygon that clipping would keep a sliver of.
const PRETEST_MARGIN: f32 = 0.01;

/// A rectangle in reflection NDC; only the cull query uses its projection crop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReflectionBounds {
    pub min: Vec2,
    pub max: Vec2,
}
impl ReflectionBounds {
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
}

/// Fixed water polygons outside the apron. Map loading clips the mesh once;
/// the per-frame projection and frustum clipping reuse stack storage.
pub struct WaterFootprint {
    polygons: Vec<Vec<Vec4>>,
    /// Each polygon's bounds: most lie outside the view and skip clipping.
    bounds: Vec<Sphere>,
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
            if calm_extent <= 0.0 {
                polygons.push(triangle.to_vec());
                continue;
            }
            // These half planes cover the outside of the apron. Overlapping
            // corner polygons are harmless for a conservative bounds query.
            for plane in sides {
                let mut clipped = [Vec4::ZERO; CLIP_CAPACITY];
                let count = clip_polygon(&triangle, plane, &mut clipped);
                if count >= 3 {
                    polygons.push(clipped[..count].to_vec());
                }
            }
        }
        let bounds = polygons
            .iter()
            .map(|polygon| {
                let center = polygon.iter().map(|point| point.truncate()).sum::<Vec3>()
                    / polygon.len() as f32;
                let radius = polygon
                    .iter()
                    .map(|point| point.truncate().distance(center))
                    .fold(0.0, f32::max);
                Sphere {
                    center,
                    radius: radius + PRETEST_MARGIN,
                }
            })
            .collect();
        Self {
            polygons,
            bounds,
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
        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        for (polygon, bounds) in self.polygons.iter().zip(&self.bounds) {
            if !view.intersects_sphere(bounds) {
                continue;
            }
            let mut points = [Vec4::ZERO; CLIP_CAPACITY];
            let mut scratch = points;
            let mut count = polygon.len();
            for (into, &point) in points.iter_mut().zip(polygon) {
                *into = matrix * point;
            }
            for plane in CLIP_PLANES {
                count = clip_polygon(&points[..count], plane, &mut scratch);
                std::mem::swap(&mut points, &mut scratch);
                if count == 0 {
                    break;
                }
            }
            for point in &points[..count] {
                let screen = Vec2::new(point.x, point.y) / point.w;
                min = min.min(screen);
                max = max.max(screen);
            }
        }
        if !min.is_finite() {
            return None;
        }
        // A normalized normal's x/z components are at most one, and distance
        // to a water point is at least distance to its plane. This bounds the
        // shader's distortion; one reflection texel covers filtering and edges.
        let distance = (camera.position.y - self.height)
            .abs()
            .max(WATER_DISTANCE_FLOOR);
        let padding = 2.0
            * (distortion.abs() * (0.001 + distance.recip()) + 1.0 / reflection_size.max(1) as f32);
        Some(ReflectionBounds {
            min: (Vec2::new(-max.x, min.y) - Vec2::splat(padding)).max(Vec2::splat(-1.0)),
            max: (Vec2::new(-min.x, max.y) + Vec2::splat(padding)).min(Vec2::ONE),
        })
    }
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
    use crate::camera::Sphere;
    use sloppy_core::geometry::plane_geometry;
    fn water() -> Mesh {
        let mut mesh = plane_geometry(100.0, 100.0);
        mesh.rotate_x(-std::f64::consts::FRAC_PI_2);
        mesh
    }
    fn camera(position: Vec3, target: Vec3) -> PerspectiveCamera {
        let mut camera = PerspectiveCamera::new(43.0, 0.1, 320.0);
        camera.position = position;
        camera.target = target;
        camera.aspect = 1.6;
        camera
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
    fn a_dry_view_has_no_water_bounds() {
        let water = WaterFootprint::new(&water(), -2.0, 40.0);
        let camera = camera(Vec3::new(0.0, 12.0, 0.0), Vec3::new(0.0, 0.0, -0.1));
        assert_eq!(water.reflection_bounds(&camera, 1.8, 512), None);
    }
    #[test]
    fn cropping_keeps_the_rectangle_and_rejects_other_receivers() {
        let bounds = ReflectionBounds {
            min: Vec2::new(-1.0, 0.0),
            max: Vec2::new(0.0, 1.0),
        };
        let frustum = bounds.frustum(Mat4::IDENTITY);
        let sphere = |x, y| Sphere {
            center: Vec3::new(x, y, 0.5),
            radius: 0.01,
        };
        assert!(frustum.intersects_sphere(&sphere(-0.5, 0.5)));
        assert!(!frustum.intersects_sphere(&sphere(0.5, 0.5)));
        assert!(!frustum.intersects_sphere(&sphere(-0.5, -0.5)));
    }
    #[test]
    fn water_behind_the_eye_is_clipped_before_dividing_by_w() {
        let water = WaterFootprint::new(&water(), -2.0, 0.0);
        let camera = camera(Vec3::new(0.0, 8.0, 100.0), Vec3::new(0.0, 8.0, 120.0));
        assert_eq!(water.reflection_bounds(&camera, 1.8, 512), None);
    }
    #[test]
    fn skipping_polygons_outside_the_view_keeps_the_bounds() {
        // A creek-like strip of small triangles, mostly outside any one view.
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
        let mesh = Mesh {
            positions,
            ..Mesh::default()
        };
        let water = WaterFootprint::new(&mesh, -2.0, 40.0);
        let mut everything = WaterFootprint::new(&mesh, -2.0, 40.0);
        for bounds in &mut everything.bounds {
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
