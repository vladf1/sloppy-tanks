//! The game's camera modes from `presentation.ts`: the overhead follow camera
//! with zoom limits, the overview, the destroyed player's overhead spectator
//! view, and first person with its flight between the overhead pose and the
//! turret eye. Also the picking that turns the pointer into an aim point, the
//! wreck landing bounds, and the listener direction for stereo panning.

use glam::{DVec3, Mat3, Mat4, Quat, Vec2, Vec3};
use sloppy_core::geometry::math::smoothstep;
use sloppy_core::sim::VehicleKind;
use sloppy_core::sim::data::ARENA;
use sloppy_core::sim::math::angle_delta;
use sloppy_core::sim::simulation::WreckView;

use super::first_person::{FirstPersonLook, seat_flight, seat_turn};
use super::posing::{euler_yxz, lerp};
use super::view_settings::{AIM_PLANE_HEIGHT, CAMERA, FIRST_PERSON, first_person_eye};
use crate::camera::PerspectiveCamera;

/// Overhead camera offset per metre of zoom: up and back toward the player.
const OVERHEAD_HEIGHT: f64 = 0.93;
const OVERHEAD_BACK: f64 = 0.72;
/// The followed point sits this high on the tank.
const FOLLOW_HEIGHT: f64 = 0.7;
/// The overview frames the whole arena from this far.
const OVERVIEW_ZOOM: f64 = ARENA * 1.8;
/// NDC probes of the visible ground that wrecks land inside.
const WRECK_PROBES: [(f32, f32); 4] = [(-0.8, -0.7), (0.8, -0.7), (-0.8, 0.65), (0.8, 0.65)];
/// Touch aim projects this many CSS pixels of stick deflection around the tank.
const TOUCH_AIM_PIXELS: f64 = 180.0;

/// The followed tank's pose, as the camera needs it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewerPose {
    pub kind: VehicleKind,
    pub alive: bool,
    /// Planar position at the start of the tick.
    pub previous: DVec3,
    /// Current body position.
    pub position: DVec3,
    pub aim: f64,
}

pub struct CameraRig {
    pub zoom: f64,
    /// The interpolated followed point (tank position at `FOLLOW_HEIGHT`).
    pub follow: DVec3,
    /// The overhead pose, kept in first person too: aiming and wreck landing
    /// zones follow it.
    pub overhead: PerspectiveCamera,
    /// The drawn camera.
    pub camera: PerspectiveCamera,
    pub first_person: FirstPersonLook,
    /// Whether the last frame was drawn from inside the player's turret.
    pub in_first_person: bool,
    /// Whether the view is heading into the turret; the turret then follows the
    /// look and the cockpit HUD shows from the start of the flight.
    pub seat_wanted: bool,
    /// Camera progress from the overhead pose (0) to the turret eye (1).
    pub seat_blend: f64,
    /// A new round starts in the chosen view rather than flying into it.
    pub snap_seat: bool,
    /// The last turret eye the camera sat in. A player destroyed in first person
    /// keeps watching from it until the respawn, which seats the camera in the
    /// new tank directly instead of flying overhead and back.
    held_eye: Option<Vec3>,
    /// Progress (0..1) of the destroyed player's step out of the wreck.
    destroyed_step: f64,
    /// The drawn camera's world rotation.
    pub rotation: Quat,
    /// World X/Z of screen right, for stereo panning.
    pub listener_right: (f64, f64),
    pub wreck_view: WreckView,
    corners: [Vec3; 4],
    /// Last aim point on the aiming plane.
    pub aim_point: Vec3,
}

impl Default for CameraRig {
    fn default() -> Self {
        let camera = PerspectiveCamera::new(
            CAMERA.field_of_view as f32,
            CAMERA.near as f32,
            CAMERA.far as f32,
        );
        Self {
            zoom: CAMERA.default_zoom,
            follow: DVec3::ZERO,
            overhead: camera,
            camera,
            first_person: FirstPersonLook::default(),
            in_first_person: false,
            seat_wanted: false,
            seat_blend: 0.0,
            snap_seat: true,
            held_eye: None,
            destroyed_step: 0.0,
            rotation: Quat::IDENTITY,
            listener_right: (1.0, 0.0),
            wreck_view: WreckView {
                min_x: 0.0,
                max_x: 0.0,
                min_z: 0.0,
                max_z: 0.0,
            },
            corners: [Vec3::ZERO; 4],
            aim_point: Vec3::ZERO,
        }
    }
}

/// The world rotation of a camera posed with `lookAt`.
fn look_rotation(camera: &PerspectiveCamera) -> Quat {
    let world = camera.view().inverse();
    Quat::from_mat3(&Mat3::from_mat4(world)).normalize()
}

impl CameraRig {
    pub fn set_aspect(&mut self, aspect: f32) {
        self.overhead.aspect = aspect;
        self.camera.aspect = aspect;
    }

    pub fn zoom_by(&mut self, amount: f64) {
        self.set_zoom(self.zoom + amount);
    }

    /// Saved preferences and user input share the renderer's camera limits.
    pub fn set_zoom(&mut self, zoom: f64) {
        if zoom.is_finite() {
            self.zoom = zoom.clamp(CAMERA.min_zoom, CAMERA.max_zoom);
        }
    }

    pub fn restore_preferences(&mut self, first_person: bool, zoom: Option<f64>) {
        self.first_person.enabled = first_person;
        if let Some(zoom) = zoom {
            self.set_zoom(zoom);
        }
        self.snap_seat = true;
    }

    /// The chosen view stays enabled while death or a menu changes the drawn camera.
    pub fn preferences(&self) -> [f64; 2] {
        [f64::from(u8::from(self.first_person.enabled)), self.zoom]
    }

    /// Pose the overhead camera, advance the seat flight and orient the drawn
    /// camera. The eye position in first person waits for `place_eye`, once the
    /// turret is posed. `has_model` is false before the viewer's model exists.
    pub fn update(
        &mut self,
        viewer: &ViewerPose,
        alpha: f64,
        dt: f64,
        overview: bool,
        has_model: bool,
    ) {
        let position = if viewer.alive {
            viewer.position
        } else {
            viewer.previous
        };
        // Follow the same interpolated pose as the tank, with no edge clamp or lag.
        self.follow = if overview {
            DVec3::ZERO
        } else {
            DVec3::new(
                lerp(viewer.previous.x, position.x, alpha),
                FOLLOW_HEIGHT,
                lerp(viewer.previous.z, position.z, alpha),
            )
        };
        let zoom = if overview { OVERVIEW_ZOOM } else { self.zoom };
        let eye = self.follow + DVec3::new(0.0, zoom * OVERHEAD_HEIGHT, zoom * OVERHEAD_BACK);
        self.overhead.look_at(eye.as_vec3(), self.follow.as_vec3());
        // Destroyed in first person, the player keeps the seat's view; one who
        // switches to first person while destroyed waits for the respawn.
        let seated_while_destroyed = !viewer.alive && self.held_eye.is_some();
        self.seat_wanted =
            self.first_person.enabled && !overview && (viewer.alive || seated_while_destroyed);
        if self.snap_seat || overview || !has_model {
            self.snap_seat = false;
            self.seat_blend = if self.seat_wanted { 1.0 } else { 0.0 };
        } else {
            let step = dt / FIRST_PERSON.transition_seconds;
            let direction = if self.seat_wanted { step } else { -step };
            self.seat_blend = (self.seat_blend + direction).clamp(0.0, 1.0);
        }
        if self.seat_blend == 0.0 {
            self.held_eye = None;
        }
        self.destroyed_step = if seated_while_destroyed && self.seat_wanted {
            (self.destroyed_step + dt / FIRST_PERSON.destroyed_step_seconds).min(1.0)
        } else {
            0.0
        };
        self.in_first_person = self.seat_blend == 1.0;
        self.camera.fov_y_degrees = lerp(
            CAMERA.field_of_view,
            FIRST_PERSON.field_of_view,
            seat_flight(self.seat_blend),
        ) as f32;
        if self.seat_blend > 0.0 {
            // In flight the camera keeps the overhead gaze on the tank, then turns
            // to the turret. Its position arrives in `place_eye`.
            let yaw = if self.first_person.enabled {
                self.first_person.yaw
            } else {
                viewer.aim
            };
            let gaze = (self.overhead.target - self.overhead.position).normalize();
            let overhead_pitch = (gaze.y as f64).asin();
            let overhead_yaw = (-gaze.x as f64).atan2(-gaze.z as f64);
            let turn = seat_turn(self.seat_blend);
            let step = smoothstep(self.destroyed_step, 0.0, 1.0);
            let seat_pitch = lerp(FIRST_PERSON.pitch, FIRST_PERSON.destroyed_pitch, step);
            self.rotation = euler_yxz(
                lerp(overhead_pitch, seat_pitch, turn) as f32,
                (overhead_yaw + angle_delta(overhead_yaw, yaw + std::f64::consts::PI) * turn)
                    as f32,
                0.0,
            );
            let listener_yaw = if turn < 0.5 {
                std::f64::consts::PI
            } else {
                yaw
            };
            self.listener_right = (-listener_yaw.cos(), listener_yaw.sin());
            // Until `place_eye` runs, stay at the overhead position. A destroyed
            // tank has no turret to seat the camera in: it steps back and up from
            // the last eye, out of the wreck, still facing the look.
            match self.held_eye {
                Some(eye) if !viewer.alive => {
                    let back = Vec3::new(-(yaw.sin() as f32), 0.0, -(yaw.cos() as f32));
                    let offset = back * FIRST_PERSON.destroyed_back as f32
                        + Vec3::Y * FIRST_PERSON.destroyed_rise as f32;
                    self.orient(eye + offset * step as f32);
                }
                _ => self.orient(self.overhead.position),
            }
        } else {
            self.camera.position = self.overhead.position;
            self.camera.target = self.overhead.target;
            self.camera.up = Vec3::Y;
            self.rotation = look_rotation(&self.overhead);
            self.listener_right = (1.0, 0.0);
        }
        for (corner, (x, y)) in self.corners.iter_mut().zip(WRECK_PROBES) {
            if let Some(hit) = self.overhead.pick_ground(Vec2::new(x, y), 0.0) {
                *corner = hit;
            }
        }
        let c = &self.corners;
        self.wreck_view = WreckView {
            min_x: c[0].x.max(c[2].x) as f64,
            max_x: c[1].x.min(c[3].x) as f64,
            min_z: c[2].z as f64,
            max_z: c[0].z as f64,
        };
    }

    fn orient(&mut self, position: Vec3) {
        self.camera.position = position;
        self.camera.target = position + self.rotation * Vec3::NEG_Z;
        self.camera.up = self.rotation * Vec3::Y;
    }

    /// Seat the first-person camera in the posed turret (`turret` is its world
    /// matrix), so it rides the hull's suspension. Between views the camera flies
    /// on the line from the overhead pose to that eye. Returns the eye.
    pub fn place_eye(&mut self, turret: &Mat4, kind: VehicleKind) -> Option<Vec3> {
        if self.seat_blend == 0.0 {
            return None;
        }
        let eye = first_person_eye(kind);
        let eye = turret.transform_point3(Vec3::new(0.0, eye.height as f32, eye.forward as f32));
        let flight = seat_flight(self.seat_blend) as f32;
        self.orient(self.overhead.position.lerp(eye, flight));
        self.held_eye = Some(eye);
        Some(eye)
    }

    /// The first-person reticle floats on the shell lane at a typical range:
    /// shells fly level at muzzle height, below the eye, so on screen they climb
    /// toward the horizon. Returns its world transform without scale.
    pub fn first_person_reticle(&self, muzzle_height: f64) -> Mat4 {
        let yaw = self.first_person.yaw;
        let lane = Vec3::new(
            (self.follow.x + yaw.sin() * FIRST_PERSON.reticle_range) as f32,
            muzzle_height as f32,
            (self.follow.z + yaw.cos() * FIRST_PERSON.reticle_range) as f32,
        );
        let to_lane = (lane - self.camera.position).normalize();
        let position = self.camera.position + to_lane * FIRST_PERSON.reticle_distance as f32;
        // The reticle's rings lie flat; stand them up to face the viewer.
        let rotation = self.rotation * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
        Mat4::from_rotation_translation(rotation, position)
    }

    /// Overhead aim reads the overhead pose, which a view transition leaves in
    /// place. `ndc` is the pointer (x right, y up, -1..1).
    pub fn aim(&mut self, ndc: Vec2) -> Vec3 {
        if let Some(point) = self.overhead.pick_ground(ndc, AIM_PLANE_HEIGHT as f32) {
            self.aim_point = point;
        }
        self.aim_point
    }

    /// Touch aim projects relative to the tank so aiming follows the screen
    /// direction at any zoom. `stick` is the aim stick vector; `client` is the
    /// canvas size in CSS pixels.
    pub fn touch_aim(&mut self, position: DVec3, stick: Vec2, client: Vec2) -> Vec3 {
        let origin = self
            .overhead
            .project(Vec3::new(position.x as f32, 0.0, position.z as f32));
        let pixels = TOUCH_AIM_PIXELS as f32;
        self.aim(Vec2::new(
            origin.x + stick.x * pixels / client.x.max(1.0),
            origin.y - stick.y * pixels / client.y.max(1.0),
        ))
    }

    /// Screen angle (clockwise from up, radians) of damage arriving at `at` from
    /// `origin`, or `None` when they coincide.
    pub fn damage_angle(&self, at: (f64, f64), origin: (f64, f64)) -> Option<f64> {
        let (dx, dz) = (origin.0 - at.0, origin.1 - at.1);
        if dx.hypot(dz) < 0.001 {
            return None;
        }
        if self.in_first_person {
            return Some(self.first_person.screen_angle(dx.atan2(dz)));
        }
        let direction = self
            .overhead
            .view()
            .transform_vector3(Vec3::new(dx as f32, 0.0, dz as f32))
            .normalize();
        Some((direction.x as f64).atan2(direction.y as f64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewer(x: f64, z: f64) -> ViewerPose {
        ViewerPose {
            kind: VehicleKind::Balanced,
            alive: true,
            previous: DVec3::new(x, 0.0, z),
            position: DVec3::new(x, 0.65, z),
            aim: 0.0,
        }
    }

    #[test]
    fn overhead_camera_follows_the_tank_and_aims_at_the_pointer() {
        let mut rig = CameraRig::default();
        rig.set_aspect(1440.0 / 900.0);
        rig.update(&viewer(10.0, -5.0), 1.0, 0.0, false, true);
        let expected = Vec3::new(10.0, 0.7 + 34.0 * 0.93, -5.0 + 34.0 * 0.72);
        assert!(rig.camera.position.distance(expected) < 1e-4);
        // The screen centre aims near the tank (the aim plane is above the follow point).
        let aim = rig.aim(Vec2::ZERO);
        assert!(
            (aim.x - 10.0).abs() < 1e-3 && (aim.z + 5.0).abs() < 0.5,
            "{aim:?}"
        );
        // Screen up aims away from the camera (-Z).
        assert!(rig.aim(Vec2::new(0.0, 0.8)).z < -10.0);
        let view = rig.wreck_view;
        assert!(view.min_x < 10.0 && view.max_x > 10.0);
        assert!(view.min_z < -5.0 && view.max_z > -5.0);
        // Straight up on screen is angle 0, right is +π/2.
        let up = rig.damage_angle((10.0, -5.0), (10.0, -15.0)).unwrap();
        assert!(up.abs() < 1e-3, "{up}");
        let right = rig.damage_angle((10.0, -5.0), (20.0, -5.0)).unwrap();
        assert!((right - std::f64::consts::FRAC_PI_2).abs() < 1e-3);
    }

    #[test]
    fn first_person_flies_into_the_turret() {
        let mut rig = CameraRig::default();
        rig.update(&viewer(0.0, 0.0), 1.0, 0.0, false, true);
        assert_eq!(rig.seat_blend, 0.0);
        rig.first_person.toggle(0.0);
        for _ in 0..10 {
            rig.update(&viewer(0.0, 0.0), 1.0, 0.04, false, true);
        }
        assert!(rig.seat_blend > 0.0 && rig.seat_blend < 1.0);
        assert!(!rig.in_first_person);
        for _ in 0..20 {
            rig.update(&viewer(0.0, 0.0), 1.0, 0.04, false, true);
        }
        assert!(rig.in_first_person);
        let turret = Mat4::from_translation(Vec3::new(0.0, 0.25, 0.0));
        let eye = rig.place_eye(&turret, VehicleKind::Balanced).unwrap();
        assert!(rig.camera.position.distance(eye) < 1e-5);
        // Facing yaw 0 looks along +Z, tilted slightly down.
        let forward = (rig.camera.target - rig.camera.position).normalize();
        assert!(forward.z > 0.99 && forward.y < 0.0, "{forward:?}");
        assert!((rig.camera.fov_y_degrees - 58.0).abs() < 1e-4);
        // Destroyed, the player stays in first person, steps back and up out of
        // the wreck along the look, and can still look around.
        let mut dead = viewer(0.0, 0.0);
        dead.alive = false;
        rig.first_person.yaw = std::f64::consts::FRAC_PI_2;
        for _ in 0..30 {
            rig.update(&dead, 1.0, 0.04, false, true);
        }
        assert!(rig.in_first_person);
        let stepped = eye
            + Vec3::new(
                -FIRST_PERSON.destroyed_back as f32,
                FIRST_PERSON.destroyed_rise as f32,
                0.0,
            );
        assert!(
            rig.camera.position.distance(stepped) < 1e-4,
            "{:?}",
            rig.camera.position
        );
        let forward = (rig.camera.target - rig.camera.position).normalize();
        assert!(forward.x > 0.8 && forward.y < -0.4, "{forward:?}");
        // The respawned tank seats the camera directly, with no flight.
        let respawned = Mat4::from_translation(Vec3::new(30.0, 0.25, -12.0));
        rig.update(&viewer(30.0, -12.0), 1.0, 0.016, false, true);
        let eye = rig.place_eye(&respawned, VehicleKind::Balanced).unwrap();
        assert!(rig.in_first_person);
        assert!(rig.camera.position.distance(eye) < 1e-5);
    }

    #[test]
    fn switching_to_first_person_while_destroyed_waits_for_the_respawn() {
        let mut rig = CameraRig::default();
        let mut dead = viewer(0.0, 0.0);
        dead.alive = false;
        rig.update(&dead, 1.0, 0.0, false, true);
        rig.first_person.toggle(0.0);
        rig.update(&dead, 1.0, 0.5, false, true);
        assert!(!rig.seat_wanted);
        assert_eq!(rig.seat_blend, 0.0);
    }

    #[test]
    fn zoom_stays_within_limits() {
        let mut rig = CameraRig::default();
        rig.zoom_by(-100.0);
        assert_eq!(rig.zoom, CAMERA.min_zoom);
        rig.zoom_by(100.0);
        assert_eq!(rig.zoom, CAMERA.max_zoom);
    }

    #[test]
    fn saved_camera_preferences_use_renderer_limits_and_keep_the_chosen_view() {
        let mut rig = CameraRig::default();
        rig.restore_preferences(true, Some(-100.0));
        assert_eq!(rig.preferences(), [1.0, CAMERA.min_zoom]);
        rig.restore_preferences(true, Some(100.0));
        assert_eq!(rig.preferences(), [1.0, CAMERA.max_zoom]);
        for malformed in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            rig.restore_preferences(true, Some(malformed));
            assert_eq!(rig.preferences(), [1.0, CAMERA.max_zoom]);
        }
        let mut dead = viewer(0.0, 0.0);
        dead.alive = false;
        rig.update(&dead, 1.0, 0.0, false, true);
        assert!(!rig.seat_wanted);
        assert_eq!(rig.preferences(), [1.0, CAMERA.max_zoom]);
        rig.restore_preferences(false, None);
        assert_eq!(rig.preferences(), [0.0, CAMERA.max_zoom]);
    }

    #[test]
    fn touch_aim_follows_the_stick_around_the_tank() {
        let mut rig = CameraRig::default();
        rig.set_aspect(1024.0 / 768.0);
        rig.update(&viewer(-20.0, 10.0), 1.0, 0.0, false, true);
        let client = Vec2::new(1024.0, 768.0);
        let tank = DVec3::new(-20.0, 0.0, 10.0);
        let right = rig.touch_aim(tank, Vec2::new(1.0, 0.0), client);
        assert!(
            right.x > -20.0 + 3.0 && (right.z - 10.0).abs() < 1.5,
            "{right:?}"
        );
        // Stick up (negative y) aims up the screen, away from the camera.
        let up = rig.touch_aim(tank, Vec2::new(0.0, -1.0), client);
        assert!(up.z < 10.0 - 3.0, "{up:?}");
    }
}
