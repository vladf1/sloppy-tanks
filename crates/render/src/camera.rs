//! Cameras, frusta and picking with Three.js conventions in WebGPU clip space:
//! right-handed view space looking down -Z, clip depth 0..1, NDC Y up.

use glam::camera::rh::{proj::directx, view::look_at_mat4};
use glam::{Mat4, Vec2, Vec3, Vec4, Vec4Swizzles};

/// A Three.js `PerspectiveCamera` posed with `lookAt`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerspectiveCamera {
    /// Vertical field of view in degrees, like Three's `fov`.
    pub fov_y_degrees: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
}

impl PerspectiveCamera {
    pub fn new(fov_y_degrees: f32, near: f32, far: f32) -> Self {
        Self {
            fov_y_degrees,
            aspect: 1.0,
            near,
            far,
            position: Vec3::new(0.0, 0.0, 1.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
        }
    }

    pub fn look_at(&mut self, position: Vec3, target: Vec3) {
        self.position = position;
        self.target = target;
    }

    /// World to view (Three's `matrixWorldInverse`).
    pub fn view(&self) -> Mat4 {
        look_at_mat4(self.position, self.target, self.up)
    }

    /// Three's `makePerspective` in the WebGPU coordinate system (depth 0..1).
    pub fn projection(&self) -> Mat4 {
        directx::perspective(
            self.fov_y_degrees.to_radians(),
            self.aspect,
            self.near,
            self.far,
        )
    }

    pub fn view_projection(&self) -> Mat4 {
        self.projection() * self.view()
    }

    /// Three's `Raycaster.setFromCamera`: a ray from the eye through an NDC point
    /// (x right, y up, both -1..1).
    pub fn ray(&self, ndc: Vec2) -> Ray {
        let inverse = self.view_projection().inverse();
        let point = inverse * Vec4::new(ndc.x, ndc.y, 0.5, 1.0);
        let point = point.xyz() / point.w;
        Ray {
            origin: self.position,
            direction: (point - self.position).normalize(),
        }
    }

    /// Screen pixel (origin top-left) to NDC for a canvas of `size` pixels.
    pub fn pixel_to_ndc(pixel: Vec2, size: Vec2) -> Vec2 {
        Vec2::new(pixel.x / size.x * 2.0 - 1.0, 1.0 - pixel.y / size.y * 2.0)
    }

    /// The ground point under a screen position, for mouse aiming: the ray through
    /// `ndc` meets the horizontal plane at height `y`.
    pub fn pick_ground(&self, ndc: Vec2, y: f32) -> Option<Vec3> {
        self.ray(ndc).intersect_horizontal_plane(y)
    }

    /// World point to NDC (Three's `Vector3.project`); z is clip depth 0..1.
    pub fn project(&self, point: Vec3) -> Vec3 {
        self.view_projection().project_point3(point)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

impl Ray {
    /// Three's `Ray.intersectPlane` for the plane `y = height`; `None` when the
    /// ray is parallel to it or points away.
    pub fn intersect_horizontal_plane(&self, height: f32) -> Option<Vec3> {
        if self.direction.y.abs() < 1e-8 {
            return (self.origin.y == height).then_some(self.origin);
        }
        let t = (height - self.origin.y) / self.direction.y;
        (t >= 0.0).then(|| self.origin + self.direction * t)
    }
}

/// A bounding sphere in some space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sphere {
    pub center: Vec3,
    pub radius: f32,
}

impl Sphere {
    /// Transform the sphere, scaling its radius by the largest axis scale like
    /// Three's `Sphere.applyMatrix4`.
    pub fn transformed(&self, matrix: &Mat4) -> Sphere {
        let scale = matrix
            .x_axis
            .xyz()
            .length_squared()
            .max(matrix.y_axis.xyz().length_squared())
            .max(matrix.z_axis.xyz().length_squared())
            .sqrt();
        Sphere {
            center: matrix.transform_point3(self.center),
            radius: self.radius * scale,
        }
    }

    pub fn from_points(points: impl Iterator<Item = Vec3>) -> Sphere {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        let mut any = false;
        for point in points {
            min = min.min(point);
            max = max.max(point);
            any = true;
        }
        if !any {
            return Sphere::default();
        }
        let center = (min + max) * 0.5;
        Sphere {
            center,
            radius: (max - center).length(),
        }
    }

    /// The smallest sphere around both (used to bound whole instance sets).
    pub fn union(&self, other: &Sphere) -> Sphere {
        let offset = other.center - self.center;
        let distance = offset.length();
        if distance + other.radius <= self.radius {
            return *self;
        }
        if distance + self.radius <= other.radius {
            return *other;
        }
        let radius = (distance + self.radius + other.radius) * 0.5;
        let center = self.center + offset * ((radius - self.radius) / distance.max(1e-12));
        Sphere { center, radius }
    }
}

/// Six clip planes extracted from a view-projection matrix with depth 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frustum {
    planes: [Vec4; 6],
}

impl Frustum {
    pub fn from_view_projection(matrix: &Mat4) -> Self {
        let row = |i: usize| matrix.row(i);
        let planes = [
            row(3) + row(0),
            row(3) - row(0),
            row(3) + row(1),
            row(3) - row(1),
            row(2),
            row(3) - row(2),
        ]
        .map(|plane| plane / plane.xyz().length().max(1e-20));
        Self { planes }
    }

    pub fn intersects_sphere(&self, sphere: &Sphere) -> bool {
        self.planes
            .iter()
            .all(|plane| plane.xyz().dot(sphere.center) + plane.w >= -sphere.radius)
    }

    /// Whether `sphere` lies wholly inside every plane.
    pub fn contains_sphere(&self, sphere: &Sphere) -> bool {
        self.planes
            .iter()
            .all(|plane| plane.xyz().dot(sphere.center) + plane.w >= sphere.radius)
    }

    /// Whether `sphere`, moved in a straight line by `sweep`, may touch the
    /// frustum on the way. Conservative: a plane rejects it only when both ends
    /// of the sweep lie wholly outside that plane.
    pub fn intersects_swept_sphere(&self, sphere: &Sphere, sweep: Vec3) -> bool {
        let end = sphere.center + sweep;
        self.planes.iter().all(|plane| {
            let distance = |point: Vec3| plane.xyz().dot(point) + plane.w;
            distance(sphere.center).max(distance(end)) >= -sphere.radius
        })
    }
}

/// Added to a caster's radius by [`ShadowReach::reaches`]: a receiver reads the
/// shadow map a normal bias and a filter footprint away from itself, a few
/// texels of about 7 cm, so a shadow that ends just outside a view still counts.
const SHADOW_REACH_MARGIN: f32 = 0.5;

/// Where the sun's shadows can show this frame. A caster matters only when its
/// bounds, swept along the light down to the lowest surface that receives a
/// shadow, reach a view that samples the shadow map; the rest of the arena's
/// casters would fill shadow-map texels no drawn pixel reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowReach {
    /// Unit direction the sunlight travels.
    pub light: Vec3,
    /// No receiving surface lies below this height (`SunShadow::receiver_floor`).
    pub floor: f32,
    /// The views that sample the shadow map: the main view and, while it draws,
    /// the water reflection.
    pub views: [Option<Frustum>; 2],
}

impl ShadowReach {
    /// Every caster reaches (no views known yet, or no floor).
    pub fn everywhere() -> Self {
        Self {
            light: Vec3::NEG_Y,
            floor: f32::NEG_INFINITY,
            views: [None, None],
        }
    }

    pub fn reaches(&self, sphere: &Sphere) -> bool {
        // A sun at or below the horizon never meets the floor: keep every caster.
        if self.light.y >= -1e-3 || !self.floor.is_finite() {
            return true;
        }
        // The sphere's top is the last of its points to fall to the floor.
        let travel = ((sphere.center.y + sphere.radius - self.floor) / -self.light.y).max(0.0);
        let padded = Sphere {
            center: sphere.center,
            radius: sphere.radius + SHADOW_REACH_MARGIN,
        };
        self.views
            .iter()
            .flatten()
            .any(|view| view.intersects_swept_sphere(&padded, self.light * travel))
    }
}

/// The sun's orthographic shadow camera (Three `DirectionalLight.shadow.camera`):
/// it sits at the light position, looks at the light target, and keeps its box
/// fixed in light space, so the map does not swim when the view moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowCamera {
    pub position: Vec3,
    pub target: Vec3,
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
    pub top: f32,
    pub near: f32,
    pub far: f32,
}

impl ShadowCamera {
    /// The square box shared by the village and harbor (`defaultSunShadow`).
    pub fn square(position: Vec3, target: Vec3, half: f32, near: f32, depth: f32) -> Self {
        Self {
            position,
            target,
            left: -half,
            right: half,
            bottom: -half,
            top: half,
            near,
            far: near + depth,
        }
    }

    pub fn view(&self) -> Mat4 {
        look_at_mat4(self.position, self.target, Vec3::Y)
    }

    pub fn projection(&self) -> Mat4 {
        directx::orthographic(
            self.left,
            self.right,
            self.bottom,
            self.top,
            self.near,
            self.far,
        )
    }

    pub fn view_projection(&self) -> Mat4 {
        self.projection() * self.view()
    }

    /// Port of `fitSunShadow`: fit the box around the ground square
    /// `|x|,|z| <= half`, heights `low..high`, for the current sun direction.
    /// The depth span stays `depth` so the tuned bias holds.
    pub fn fit_square(
        position: Vec3,
        target: Vec3,
        half: f32,
        low: f32,
        high: f32,
        depth: f32,
    ) -> Self {
        let view = look_at_mat4(position, target, Vec3::Y);
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for x in [-half, half] {
            for y in [low, high] {
                for z in [-half, half] {
                    let corner = view.transform_point3(Vec3::new(x, y, z));
                    min = min.min(corner);
                    max = max.max(corner);
                }
            }
        }
        let near = -max.z - 2.0;
        Self {
            position,
            target,
            left: min.x,
            right: max.x,
            bottom: min.y,
            top: max.y,
            near,
            far: near + depth,
        }
    }
}

/// The mirrored camera for a horizontal planar reflection, ported from Three
/// r185 `ReflectorNode.updateBefore`: the eye and gaze are reflected in the plane
/// and the projection's near plane is replaced by the water plane (oblique
/// clipping), so geometry below the surface never reaches the reflection.
/// Returns `None` when the camera looks at the plane from below.
pub fn mirror_view(camera: &PerspectiveCamera, plane_height: f32) -> Option<(Mat4, Mat4)> {
    let normal = Vec3::Y;
    let reflector = Vec3::new(0.0, plane_height, 0.0);
    let reflect = |v: Vec3| v - 2.0 * v.dot(normal) * normal;
    let world = camera.view().inverse();
    let camera_position = world.w_axis.xyz();
    let view_vector = reflector - camera_position;
    if view_vector.dot(normal) > 0.0 {
        return None;
    }
    let eye = -reflect(view_vector) + reflector;
    let rotation = glam::Mat3::from_mat4(world);
    let look_at = rotation * Vec3::NEG_Z + camera_position;
    let target = -reflect(reflector - look_at) + reflector;
    let up = reflect(rotation * Vec3::Y);
    let view = look_at_mat4(eye, target, up);
    let mut projection = camera.projection();

    // The plane in the mirrored camera's view space.
    let plane_normal = glam::Mat3::from_mat4(view) * normal;
    let point = view.transform_point3(reflector);
    let clip = Vec4::new(
        plane_normal.x,
        plane_normal.y,
        plane_normal.z,
        -point.dot(plane_normal),
    );
    let sign = |v: f32| {
        if v > 0.0 {
            1.0
        } else if v < 0.0 {
            -1.0
        } else {
            0.0
        }
    };
    let e = projection.to_cols_array();
    let q = Vec4::new(
        (sign(clip.x) + e[8]) / e[0],
        (sign(clip.y) + e[9]) / e[5],
        -1.0,
        (1.0 + e[10]) / e[14],
    );
    let clip = clip * (1.0 / clip.dot(q));
    projection.x_axis.z = clip.x;
    projection.y_axis.z = clip.y;
    projection.z_axis.z = clip.z;
    projection.w_axis.z = clip.w;
    Some((view, projection))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game_camera() -> PerspectiveCamera {
        // The overhead pose from presentation.ts at the default zoom.
        let mut camera = PerspectiveCamera::new(43.0, 0.1, 320.0);
        camera.aspect = 16.0 / 9.0;
        let follow = Vec3::new(0.0, 0.7, 0.0);
        camera.look_at(follow + Vec3::new(0.0, 34.0 * 0.93, 34.0 * 0.72), follow);
        camera
    }

    #[test]
    fn projection_uses_webgpu_depth_range() {
        let camera = game_camera();
        let near =
            camera.project(camera.position + (camera.target - camera.position).normalize() * 0.1);
        let far =
            camera.project(camera.position + (camera.target - camera.position).normalize() * 320.0);
        assert!(near.z.abs() < 1e-4, "{near:?}");
        assert!((far.z - 1.0).abs() < 1e-4, "{far:?}");
    }

    #[test]
    fn center_pick_hits_the_look_target() {
        let camera = game_camera();
        let hit = camera.pick_ground(Vec2::ZERO, 0.7).unwrap();
        assert!(hit.distance(camera.target) < 1e-3, "{hit:?}");
        // Screen up picks farther away (-z), screen right picks +x.
        let up = camera.pick_ground(Vec2::new(0.0, 0.5), 1.0).unwrap();
        assert!(up.z < -1.0);
        let right = camera.pick_ground(Vec2::new(0.5, 0.0), 1.0).unwrap();
        assert!(right.x > 1.0);
        assert_eq!(
            PerspectiveCamera::pixel_to_ndc(Vec2::new(0.0, 0.0), Vec2::new(200.0, 100.0)),
            Vec2::new(-1.0, 1.0)
        );
    }

    #[test]
    fn frustum_culls_spheres_outside_the_view() {
        let camera = game_camera();
        let frustum = Frustum::from_view_projection(&camera.view_projection());
        let sphere = |x, y, z, r| Sphere {
            center: Vec3::new(x, y, z),
            radius: r,
        };
        assert!(frustum.intersects_sphere(&sphere(0.0, 0.0, 0.0, 1.0)));
        assert!(!frustum.intersects_sphere(&sphere(200.0, 0.0, 0.0, 1.0)));
        assert!(frustum.intersects_sphere(&sphere(200.0, 0.0, 0.0, 190.0)));
        assert!(!frustum.intersects_sphere(&sphere(0.0, 0.0, -600.0, 5.0)));
        assert!(!frustum.intersects_sphere(&sphere(0.0, 60.0, 40.0, 1.0)));
    }

    #[test]
    fn swept_spheres_count_anywhere_along_the_sweep() {
        let camera = game_camera();
        let frustum = Frustum::from_view_projection(&camera.view_projection());
        let outside = Sphere {
            center: Vec3::new(200.0, 0.0, 0.0),
            radius: 1.0,
        };
        assert!(!frustum.intersects_swept_sphere(&outside, Vec3::ZERO));
        assert!(!frustum.intersects_swept_sphere(&outside, Vec3::new(50.0, 0.0, 0.0)));
        // Ending inside, or passing straight through, both touch the view.
        assert!(frustum.intersects_swept_sphere(&outside, Vec3::new(-200.0, 0.0, 0.0)));
        assert!(frustum.intersects_swept_sphere(&outside, Vec3::new(-400.0, 0.0, 0.0)));
    }

    #[test]
    fn shadow_reach_keeps_casters_whose_shadow_lands_in_a_view() {
        let camera = game_camera();
        let view = Frustum::from_view_projection(&camera.view_projection());
        let reach = ShadowReach {
            light: Vec3::new(1.0, -1.0, 0.0).normalize(),
            floor: -10.0,
            views: [Some(view), None],
        };
        let at = |x: f32, y: f32| Sphere {
            center: Vec3::new(x, y, 0.0),
            radius: 1.0,
        };
        assert!(reach.reaches(&at(0.0, 1.0)));
        // Off the left of the view, but tall enough that its shadow falls into it.
        assert!(!view.intersects_sphere(&at(-60.0, 40.0)));
        assert!(reach.reaches(&at(-60.0, 40.0)));
        // Off the right: its shadow falls farther right still.
        assert!(!reach.reaches(&at(60.0, 1.0)));
        // A second view (the reflection) keeps what it sees.
        let reflected = ShadowReach {
            views: [Some(view), Some(view_from(Vec3::new(60.0, 20.0, 20.0)))],
            ..reach
        };
        assert!(reflected.reaches(&at(60.0, 1.0)));
        // No floor, or a sun at the horizon, keeps everything.
        assert!(ShadowReach::everywhere().reaches(&at(60.0, 1.0)));
        let level = ShadowReach {
            light: Vec3::X,
            ..reach
        };
        assert!(level.reaches(&at(60.0, 1.0)));
    }

    fn view_from(position: Vec3) -> Frustum {
        let mut camera = game_camera();
        camera.look_at(position, Vec3::new(60.0, 0.0, 0.0));
        Frustum::from_view_projection(&camera.view_projection())
    }

    #[test]
    fn shadow_box_maps_the_arena_inside_clip_space() {
        let sun = ShadowCamera::square(Vec3::new(-45.0, 85.0, 25.0), Vec3::ZERO, 70.0, 0.5, 219.5);
        let matrix = sun.view_projection();
        // The square box covers the arena's middle; like the game's, it misses two
        // corners under a diagonal sun, which is why the quarry fits its box.
        for corner in [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(40.0, 0.0, 40.0),
            Vec3::new(-40.0, 8.0, -40.0),
        ] {
            let clip = matrix.project_point3(corner);
            assert!(clip.x.abs() <= 1.0 && clip.y.abs() <= 1.0, "{clip:?}");
            assert!((0.0..=1.0).contains(&clip.z), "{clip:?}");
        }
        let fitted = ShadowCamera::fit_square(
            Vec3::new(-50.0, 43.0, 28.0),
            Vec3::ZERO,
            68.0,
            -2.0,
            9.0,
            219.5,
        );
        let matrix = fitted.view_projection();
        for x in [-68.0, 68.0] {
            for z in [-68.0, 68.0] {
                let clip = matrix.project_point3(Vec3::new(x, 0.0, z));
                assert!(clip.x.abs() <= 1.0001 && clip.y.abs() <= 1.0001, "{clip:?}");
                assert!((0.0..=1.0).contains(&clip.z), "{clip:?}");
            }
        }
    }

    #[test]
    fn mirror_camera_reflects_the_eye_and_clips_below_the_water() {
        let camera = game_camera();
        let (view, projection) = mirror_view(&camera, -2.2).unwrap();
        let eye = view.inverse().w_axis.xyz();
        let expected = Vec3::new(
            camera.position.x,
            -2.0 * 2.2 - camera.position.y,
            camera.position.z,
        );
        assert!(eye.distance(expected) < 1e-3, "{eye:?}");
        // A point above the water projects inside the depth range; one below it
        // falls behind the oblique near plane.
        let above = (projection * view).project_point3(Vec3::new(0.0, 3.0, 0.0));
        let below = (projection * view).project_point3(Vec3::new(0.0, -6.0, 0.0));
        assert!((0.0..=1.0).contains(&above.z), "{above:?}");
        assert!(below.z < 0.0, "{below:?}");
        let mut under = camera;
        under.position.y = -10.0;
        assert!(mirror_view(&under, -2.2).is_none());
    }

    #[test]
    fn sphere_union_contains_both() {
        let a = Sphere {
            center: Vec3::ZERO,
            radius: 1.0,
        };
        let b = Sphere {
            center: Vec3::new(10.0, 0.0, 0.0),
            radius: 2.0,
        };
        let u = a.union(&b);
        assert!((u.radius - 6.5).abs() < 1e-5);
        assert!((u.center.x - 5.5).abs() < 1e-5);
    }
}
