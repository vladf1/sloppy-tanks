//! The game's camera modes from `presentation.ts`: the overhead follow camera
//! with zoom limits, the overview, the destroyed player's overhead spectator
//! view, and first person with its flight between the overhead pose and the
//! turret eye. Also the picking that turns the pointer into an aim point, the
//! wreck landing bounds, and the listener direction for stereo panning.
//!
//! The follow camera has weight: its focus rides a critically damped spring that
//! leads the tank toward its aim and travel, and it eases back at speed. Shots,
//! hits and blasts shake and kick only the drawn camera; the overhead pose that
//! aiming and wreck landing read stays steady.

use glam::{DVec3, Mat3, Mat4, Quat, Vec2, Vec3};
use sloppy_core::geometry::math::smoothstep;
use sloppy_core::sim::VehicleKind;
use sloppy_core::sim::data::ARENA;
use sloppy_core::sim::math::angle_delta;
use sloppy_core::sim::simulation::WreckView;

use super::first_person::{FirstPersonLook, seat_flight, seat_turn};
use super::posing::{euler_yxz, lerp};
use super::view_settings::{AIM_PLANE_HEIGHT, CAMERA, CAMERA_FEEL, FIRST_PERSON, first_person_eye};
use crate::camera::PerspectiveCamera;

/// Overhead camera offset per metre of zoom: up and back toward the player, at
/// about 47 degrees, low enough to show the sides of tanks and houses.
const OVERHEAD_HEIGHT: f64 = 0.76;
const OVERHEAD_BACK: f64 = 0.7;
/// The overview looks down more steeply, to frame the whole arena. Wrecks land
/// inside the same steady framing centred on the tank (`landing`), so the
/// camera's lead, lag and shake never reach the simulation.
const OVERVIEW_HEIGHT: f64 = 0.93;
const OVERVIEW_BACK: f64 = 0.72;
/// Independent shake channels sample the noise this far apart in time.
const SHAKE_CHANNEL_SPACING: f64 = 37.1;
/// Below this a camera punch has played out.
const PUNCH_REST: f64 = 1e-3;
/// The followed point sits this high on the tank.
const FOLLOW_HEIGHT: f64 = 0.7;
/// The overview frames the whole arena from this far.
const OVERVIEW_ZOOM: f64 = ARENA * 1.8;
/// NDC probes of the visible ground that wrecks land inside.
const WRECK_PROBES: [(f32, f32); 4] = [(-0.8, -0.7), (0.8, -0.7), (-0.8, 0.65), (0.8, 0.65)];

/// The followed tank's pose, as the camera needs it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewerPose {
    pub kind: VehicleKind,
    pub alive: bool,
    /// Planar position at the start of the tick.
    pub previous: DVec3,
    /// Current body position.
    pub position: DVec3,
    /// Planar body velocity (m/s).
    pub velocity: DVec3,
    pub aim: f64,
    /// The nearest live enemy within `threat_range`, on the X/Z plane.
    pub threat: Option<DVec3>,
}

/// Trauma shake plus a sprung kick offset, applied to the drawn camera only.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CameraShake {
    /// 0..1; the shake grows with its square.
    pub trauma: f64,
    clock: f64,
    offset: DVec3,
    velocity: DVec3,
}

/// Smooth noise in -1..1: two incommensurate sines.
fn shake_noise(t: f64) -> f64 {
    0.62 * (t * std::f64::consts::TAU).sin() + 0.38 * (t * 2.31 * std::f64::consts::TAU + 1.3).sin()
}

impl CameraShake {
    pub fn add_trauma(&mut self, amount: f64) {
        self.trauma = (self.trauma + amount.max(0.0)).min(1.0);
    }

    /// Push the camera with velocity `kick` (m/s, world).
    pub fn kick(&mut self, kick: DVec3) {
        self.velocity += kick;
    }

    fn step(&mut self, dt: f64) {
        self.trauma = (self.trauma - CAMERA_FEEL.trauma_decay * dt).max(0.0);
        self.clock += dt;
        let (frequency, damping) = (CAMERA_FEEL.kick_frequency, CAMERA_FEEL.kick_damping);
        let mut remaining = dt.min(0.1);
        while remaining > 0.0 {
            let h = remaining.min(1.0 / 240.0);
            let pull =
                -frequency * frequency * self.offset - 2.0 * damping * frequency * self.velocity;
            self.velocity += pull * h;
            self.offset += self.velocity * h;
            remaining -= h;
        }
        let max = CAMERA_FEEL.max_kick_offset;
        if self.offset.length() > max {
            self.offset = self.offset.normalize() * max;
        }
    }

    /// The shake's view-space rotation (yaw, pitch, roll) and world offset, scaled.
    fn sample(&self, scale: f64) -> (Quat, Vec3) {
        let shake = self.trauma * self.trauma * scale;
        let t = self.clock * CAMERA_FEEL.shake_frequency;
        let [yaw, pitch, roll] = CAMERA_FEEL.shake_angles;
        let channel = |index: f64| shake_noise(t + index * SHAKE_CHANNEL_SPACING);
        let rotation = Quat::from_rotation_y((yaw * shake * channel(0.0)) as f32)
            * Quat::from_rotation_x((pitch * shake * channel(1.0)) as f32)
            * Quat::from_rotation_z((roll * shake * channel(2.0)) as f32);
        (rotation, (self.offset * scale).as_vec3())
    }
}

/// A fast-in, slow-out envelope (0..1): an event raises its level, which the
/// drawn value chases at the attack rate while the level eases away.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CameraPunch {
    level: f64,
    pub drawn: f64,
}

impl CameraPunch {
    fn add(&mut self, amount: f64) {
        self.level = (self.level + amount).min(1.0);
    }

    fn step(&mut self, attack: f64, release: f64, dt: f64) {
        self.drawn += (self.level - self.drawn) * (1.0 - (-attack * dt).exp());
        self.level *= (-release * dt).exp();
        // A spent punch rests at exactly zero, leaving the plain lens and pose.
        if self.level < PUNCH_REST && self.drawn < PUNCH_REST {
            self.level = 0.0;
            self.drawn = 0.0;
        }
    }
}

/// One step of an exact critically damped spring toward `target`.
fn follow_spring(position: &mut DVec3, velocity: &mut DVec3, target: DVec3, omega: f64, dt: f64) {
    let offset = *position - target;
    let impulse = (*velocity + offset * omega) * dt;
    let decay = (-omega * dt).exp();
    *position = target + (offset + impulse) * decay;
    *velocity = (*velocity - impulse * omega) * decay;
}

pub struct CameraRig {
    pub zoom: f64,
    /// The interpolated followed point (tank position at `FOLLOW_HEIGHT`).
    pub follow: DVec3,
    /// What the overhead camera looks at: the followed point with its lead, on a
    /// spring (`None` until the first follow).
    focus: Option<DVec3>,
    focus_velocity: DVec3,
    /// The eased speed fraction (0..1) that pulls the camera back.
    speed_zoom: f64,
    pub shake: CameraShake,
    /// A kill's dolly toward the focus and a shot's lens kick (drawn camera only).
    pub punch: CameraPunch,
    fov_kick: CameraPunch,
    /// A recent blast the view leans toward, and how much of its pull is left (0..1).
    interest: DVec3,
    interest_weight: f64,
    /// Metres the viewer has driven, phasing the seated view's sway.
    sway_travel: f64,
    /// The eased speed fraction (0..1) the seated sway scales with.
    sway_speed: f64,
    /// This frame's shake, applied wherever the drawn camera is placed.
    shake_rotation: Quat,
    shake_offset: Vec3,
    /// The hull tilt the seated view rides.
    seat_tilt: Quat,
    /// The overhead pose, kept in first person too: aiming and wreck landing
    /// zones follow it.
    pub overhead: PerspectiveCamera,
    /// The steady tank-centred frame the wreck landing bounds are measured in.
    landing: PerspectiveCamera,
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
            focus: None,
            focus_velocity: DVec3::ZERO,
            speed_zoom: 0.0,
            shake: CameraShake::default(),
            punch: CameraPunch::default(),
            fov_kick: CameraPunch::default(),
            interest: DVec3::ZERO,
            interest_weight: 0.0,
            sway_travel: 0.0,
            sway_speed: 0.0,
            shake_rotation: Quat::IDENTITY,
            shake_offset: Vec3::ZERO,
            seat_tilt: Quat::IDENTITY,
            overhead: camera,
            landing: camera,
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
        self.landing.aspect = aspect;
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
        let focus = self.weighted_focus(viewer, overview, dt);
        let zoom = if overview {
            OVERVIEW_ZOOM
        } else {
            self.zoom * (1.0 + CAMERA_FEEL.speed_zoom * self.speed_zoom)
        };
        let (height, back) = if overview {
            (OVERVIEW_HEIGHT, OVERVIEW_BACK)
        } else {
            (OVERHEAD_HEIGHT, OVERHEAD_BACK)
        };
        let eye = focus + DVec3::new(0.0, zoom * height, zoom * back);
        self.overhead.look_at(eye.as_vec3(), focus.as_vec3());
        let landing_zoom = if overview { OVERVIEW_ZOOM } else { self.zoom };
        let landing_eye = self.follow
            + DVec3::new(
                0.0,
                landing_zoom * OVERVIEW_HEIGHT,
                landing_zoom * OVERVIEW_BACK,
            );
        self.landing
            .look_at(landing_eye.as_vec3(), self.follow.as_vec3());
        self.shake.step(dt);
        self.punch
            .step(CAMERA_FEEL.punch_attack, CAMERA_FEEL.punch_release, dt);
        self.fov_kick
            .step(CAMERA_FEEL.punch_attack, CAMERA_FEEL.fov_kick_release, dt);
        self.interest_weight =
            (self.interest_weight - dt / CAMERA_FEEL.blast_look_seconds).max(0.0);
        let speed = viewer.velocity.x.hypot(viewer.velocity.z);
        if viewer.alive {
            self.sway_travel += speed * dt;
        }
        let sway_target = if viewer.alive {
            (speed / CAMERA_FEEL.full_speed).min(1.0)
        } else {
            0.0
        };
        self.sway_speed += (sway_target - self.sway_speed)
            * (1.0 - (-CAMERA_FEEL.speed_zoom_rate * 2.0 * dt).exp());
        let shake_scale = lerp(1.0, CAMERA_FEEL.first_person_shake, self.seat_blend);
        (self.shake_rotation, self.shake_offset) = self.shake.sample(shake_scale);
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
        let flight = seat_flight(self.seat_blend);
        // Speed widens a live seat's view; a destroyed one shows the plain lens.
        let speed_fov = if viewer.alive {
            CAMERA_FEEL.first_person_speed_fov * self.speed_zoom
        } else {
            0.0
        };
        let seated_fov = FIRST_PERSON.field_of_view + speed_fov;
        let kick = CAMERA_FEEL.fire_fov_kick * self.fov_kick.drawn;
        self.camera.fov_y_degrees = (lerp(CAMERA.field_of_view, seated_fov, flight) - kick) as f32;
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
            self.rotation = look_rotation(&self.overhead);
            self.seat_tilt = Quat::IDENTITY;
            // A kill dollies the drawn camera in toward the focus for a beat.
            let dolly = (self.overhead.target - self.overhead.position)
                * (CAMERA_FEEL.kill_punch_in * self.punch.drawn) as f32;
            self.orient(self.overhead.position + dolly);
            self.listener_right = (1.0, 0.0);
        }
        for (corner, (x, y)) in self.corners.iter_mut().zip(WRECK_PROBES) {
            if let Some(hit) = self.landing.pick_ground(Vec2::new(x, y), 0.0) {
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

    /// The overhead focus: the interpolated tank, led toward its aim and travel,
    /// on a spring. A far jump snaps.
    fn weighted_focus(&mut self, viewer: &ViewerPose, overview: bool, dt: f64) -> DVec3 {
        let speed = viewer.velocity.x.hypot(viewer.velocity.z);
        let speed_target = if viewer.alive && !overview {
            (speed / CAMERA_FEEL.full_speed).min(1.0)
        } else {
            0.0
        };
        self.speed_zoom +=
            (speed_target - self.speed_zoom) * (1.0 - (-CAMERA_FEEL.speed_zoom_rate * dt).exp());
        // The overview leaves the spring where it was, so a glance at the arena
        // returns to the same framing (or snaps when the tank moved far meanwhile).
        if overview {
            return self.follow;
        }
        let omega = CAMERA_FEEL.follow_frequency;
        let mut target = self.follow;
        if viewer.alive {
            // The view leans toward the aim in proportion to its reach: a pointer
            // near the tank leaves it centred. A fraction below one keeps the lead
            // from chasing the pointer it moves.
            let aim = DVec3::new(viewer.aim.sin(), 0.0, viewer.aim.cos());
            let reach = f64::from(self.aim_point.x - self.follow.x as f32)
                .hypot(f64::from(self.aim_point.z - self.follow.z as f32));
            let lead = (reach * CAMERA_FEEL.aim_look_ahead)
                .min(CAMERA_FEEL.max_aim_look_ahead * self.zoom);
            let travel = DVec3::new(viewer.velocity.x, 0.0, viewer.velocity.z);
            let ahead = travel * CAMERA_FEEL.travel_look_ahead_seconds;
            let ahead = ahead.clamp_length_max(CAMERA_FEEL.max_travel_look_ahead);
            // And toward the nearest enemy, so the fight shares the frame with the
            // tank, plus a constant nudge up the screen: the view shows more of what
            // lies ahead than of the ground under the camera.
            let threat = viewer.threat.map_or(DVec3::ZERO, |enemy| {
                let toward = DVec3::new(enemy.x - self.follow.x, 0.0, enemy.z - self.follow.z);
                (toward * CAMERA_FEEL.threat_look_ahead)
                    .clamp_length_max(CAMERA_FEEL.max_threat_look_ahead * self.zoom)
            });
            let frame = DVec3::new(0.0, 0.0, -CAMERA_FEEL.frame_ahead * self.zoom);
            // A blast nearby draws the eye for a moment.
            let toward_blast = DVec3::new(
                self.interest.x - self.follow.x,
                0.0,
                self.interest.z - self.follow.z,
            );
            let blast = toward_blast
                * (CAMERA_FEEL.blast_look_ahead * smoothstep(self.interest_weight, 0.0, 1.0));
            let lead = (aim * lead + threat + frame + blast)
                .clamp_length_max(CAMERA_FEEL.max_look_ahead * self.zoom);
            // The spring trails a steady target by 2v/ω; leading by as much keeps a
            // cruising tank centred while starts, stops and turns carry weight.
            target += lead + ahead + travel * (2.0 / omega);
        }
        let focus = match self.focus {
            Some(focus) if focus.distance(target) < CAMERA_FEEL.snap_distance => {
                let mut focus = focus;
                follow_spring(&mut focus, &mut self.focus_velocity, target, omega, dt);
                focus
            }
            _ => {
                self.focus_velocity = DVec3::ZERO;
                target
            }
        };
        self.focus = Some(focus);
        focus
    }

    /// Place the drawn camera at `position` facing `rotation`, with this frame's
    /// shake and, when seated, the hull's tilt.
    fn orient(&mut self, position: Vec3) {
        let rotation = self.seat_tilt * self.rotation * self.shake_rotation;
        self.camera.position = position + self.shake_offset;
        self.camera.target = self.camera.position + rotation * Vec3::NEG_Z;
        self.camera.up = rotation * Vec3::Y;
    }

    /// A new round: the next follow snaps to the tank, with no shake left over.
    pub fn settle(&mut self) {
        self.focus = None;
        self.focus_velocity = DVec3::ZERO;
        self.speed_zoom = 0.0;
        self.shake = CameraShake::default();
        self.punch = CameraPunch::default();
        self.fov_kick = CameraPunch::default();
        self.interest_weight = 0.0;
        self.sway_speed = 0.0;
    }

    /// The viewer fired toward world direction `(dx, dz)`; rockets pass `scale` > 1.
    pub fn fired(&mut self, dx: f64, dz: f64, scale: f64) {
        let length = dx.hypot(dz).max(1e-6);
        self.shake.add_trauma(CAMERA_FEEL.fire_trauma * scale);
        self.fov_kick.add(scale.min(1.0));
        let kick = CAMERA_FEEL.fire_kick * scale / length;
        self.shake.kick(DVec3::new(-dx * kick, 0.0, -dz * kick));
    }

    /// One of the viewer's shots destroyed an enemy at `(x, z)`: a hard knock and a
    /// dolly in, while the battle holds its breath.
    pub fn killed(&mut self, x: f64, z: f64) {
        self.shake.add_trauma(CAMERA_FEEL.kill_trauma);
        self.punch.add(1.0);
        let (dx, dz) = (x - self.follow.x, z - self.follow.z);
        let distance = dx.hypot(dz).max(1.0);
        // The view lurches a little toward the kill.
        let kick = CAMERA_FEEL.fire_kick / distance;
        self.shake.kick(DVec3::new(dx * kick, 0.0, dz * kick));
    }

    /// A reticle driven by the turret rather than the pointer (autoplay) glides to
    /// `target` on the aiming plane; a far jump snaps.
    pub fn track_aim(&mut self, target: Vec3, dt: f64) {
        let target = Vec3::new(target.x, AIM_PLANE_HEIGHT as f32, target.z);
        if self.aim_point.distance(target) > CAMERA_FEEL.snap_distance as f32 * 2.0 {
            self.aim_point = target;
        } else {
            let blend = 1.0 - (-CAMERA_FEEL.aim_track_rate * dt).exp();
            self.aim_point = self.aim_point.lerp(target, blend as f32);
        }
    }

    /// The viewer took `damage`, pushed along world `(dx, dz)` (zero when unknown).
    pub fn hurt(&mut self, dx: f64, dz: f64, damage: f64) {
        self.shake.add_trauma(
            (damage * CAMERA_FEEL.hurt_trauma_per_damage).min(CAMERA_FEEL.max_hurt_trauma),
        );
        let length = dx.hypot(dz);
        if length > 1e-6 {
            let kick = CAMERA_FEEL.hurt_kick / length;
            self.shake.kick(DVec3::new(dx * kick, 0.0, dz * kick));
        }
    }

    /// A blast of `radius` at `(x, z)`: trauma, a slight pull in and a kick down
    /// and away, fading with its distance from the followed point.
    pub fn blast(&mut self, x: f64, z: f64, radius: f64) {
        let (dx, dz) = (self.follow.x - x, self.follow.z - z);
        let distance = dx.hypot(dz);
        let reach = CAMERA_FEEL.blast_reach + radius;
        if distance >= reach {
            return;
        }
        let falloff = (1.0 - distance / reach).powi(2);
        self.shake
            .add_trauma(CAMERA_FEEL.blast_trauma_per_radius * radius * falloff);
        let away = if distance > 1e-3 {
            DVec3::new(dx / distance, 0.0, dz / distance)
        } else {
            DVec3::ZERO
        };
        let strength = falloff * (radius / 6.0).min(1.0);
        self.punch.add(CAMERA_FEEL.blast_punch * strength);
        if strength >= self.interest_weight * 0.5 {
            self.interest = DVec3::new(x, 0.0, z);
            self.interest_weight = (self.interest_weight + strength * 2.0).min(1.0);
        }
        let kick = CAMERA_FEEL.blast_kick * falloff * (radius / 3.0).min(2.0);
        self.shake
            .kick((away * 0.6 + DVec3::new(0.0, -1.0, 0.0)) * kick);
    }

    /// A tank destroyed at `(x, z)` near the viewer pulls the camera in a little,
    /// fading with distance like a blast; the viewer's own kills pull harder
    /// (`killed`).
    pub fn tank_destroyed(&mut self, x: f64, z: f64) {
        let distance = (self.follow.x - x).hypot(self.follow.z - z);
        let falloff = (1.0 - distance / CAMERA_FEEL.blast_reach).max(0.0);
        self.punch
            .add(CAMERA_FEEL.nearby_death_punch * falloff * falloff);
    }

    /// The turret eye the seated camera last sat in, while fully seated. (The drawn
    /// camera waits at the overhead pose until `place_eye` each frame.)
    pub fn seated_eye(&self) -> Option<Vec3> {
        self.held_eye.filter(|_| self.in_first_person)
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
        // The seated view rides part of the hull's pitch and roll: the turret's up
        // axis leans with the suspension.
        let up = turret.y_axis.truncate().normalize_or(Vec3::Y);
        let share = (CAMERA_FEEL.first_person_tilt * f64::from(flight)) as f32;
        // Driving sways the seat side to side with the treads, bobs it twice a
        // cycle and shudders it with the engine, all growing with speed.
        let phase = self.sway_travel / CAMERA_FEEL.first_person_bob_metres * std::f64::consts::TAU;
        let sway = self.sway_speed * f64::from(flight);
        let roll = CAMERA_FEEL.first_person_sway_roll * sway * phase.sin();
        let shudder =
            CAMERA_FEEL.first_person_shudder * sway * shake_noise(self.shake.clock * 17.0);
        let bob = CAMERA_FEEL.first_person_bob * sway * (phase * 2.0).cos().abs() + shudder;
        let forward = turret.z_axis.truncate().normalize_or(Vec3::Z);
        self.seat_tilt = Quat::IDENTITY.slerp(Quat::from_rotation_arc(Vec3::Y, up), share)
            * Quat::from_axis_angle(forward, roll as f32);
        let eye_swayed = eye + up * bob as f32;
        self.orient(self.overhead.position.lerp(eye_swayed, flight));
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
            velocity: DVec3::ZERO,
            aim: 0.0,
            threat: None,
        }
    }

    #[test]
    fn overhead_camera_follows_the_tank_and_aims_at_the_pointer() {
        let mut rig = CameraRig::default();
        rig.set_aspect(1440.0 / 900.0);
        // A pointer resting on the tank leaves the view on it, framed a little
        // ahead (up the screen).
        rig.aim_point = Vec3::new(10.0, 1.0, -5.0);
        rig.update(&viewer(10.0, -5.0), 1.0, 0.0, false, true);
        let zoom = CAMERA.default_zoom as f32;
        let ahead = (CAMERA_FEEL.frame_ahead * CAMERA.default_zoom) as f32;
        let expected = Vec3::new(
            10.0,
            0.7 + zoom * OVERHEAD_HEIGHT as f32,
            -5.0 - ahead + zoom * OVERHEAD_BACK as f32,
        );
        assert!(
            rig.camera.position.distance(expected) < 1e-4,
            "{:?}",
            rig.camera.position
        );
        // The screen centre aims just ahead of the tank (the aim plane is above the
        // follow point).
        let aim = rig.aim(Vec2::ZERO);
        assert!(
            (aim.x - 10.0).abs() < 1e-3 && (aim.z + 5.0 + ahead).abs() < 0.5,
            "{aim:?}"
        );
        // A far pointer leads the view toward the aim (+Z at aim 0), within its cap.
        let mut led = CameraRig::default();
        led.set_aspect(1440.0 / 900.0);
        led.aim_point = Vec3::new(10.0, 1.0, 25.0);
        led.update(&viewer(10.0, -5.0), 1.0, 0.0, false, true);
        let lead = led.overhead.target.z - rig.overhead.target.z;
        assert!(
            lead > 1.0
                && f64::from(lead) <= CAMERA_FEEL.max_aim_look_ahead * CAMERA.default_zoom + 1e-4
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
    fn the_focus_carries_weight_and_shake_spares_the_aim() {
        let mut rig = CameraRig::default();
        rig.set_aspect(1.6);
        rig.update(&viewer(0.0, 0.0), 1.0, 0.0, false, true);
        let start = rig.overhead.position;
        // A sudden step trails, then settles on the new focus.
        rig.update(&viewer(3.0, 0.0), 1.0, 1.0 / 60.0, false, true);
        let moved = rig.overhead.position.x - start.x;
        assert!(moved > 0.0 && moved < 3.0, "{moved}");
        for _ in 0..120 {
            rig.update(&viewer(3.0, 0.0), 1.0, 1.0 / 60.0, false, true);
        }
        assert!((rig.overhead.position.x - start.x - 3.0).abs() < 1e-3);
        // A blast shakes the drawn camera; the overhead pose aiming reads only
        // eases toward it on the focus spring, without the shake's tilt.
        let steady = rig.overhead.position;
        let steady_gaze = (rig.overhead.target - rig.overhead.position).normalize();
        rig.blast(3.0, 4.0, 6.0);
        rig.update(&viewer(3.0, 0.0), 1.0, 1.0 / 60.0, false, true);
        assert!(rig.shake.trauma > 0.0);
        let gaze = (rig.overhead.target - rig.overhead.position).normalize();
        assert!(gaze.distance(steady_gaze) < 1e-5);
        assert!(rig.overhead.position.distance(steady) < 0.1);
        let drawn_gaze = (rig.camera.target - rig.camera.position).normalize();
        assert!(drawn_gaze.distance(gaze) > 1e-5);
        // Its pull fades, and the focus settles back on the tank.
        for _ in 0..240 {
            rig.update(&viewer(3.0, 0.0), 1.0, 1.0 / 60.0, false, true);
        }
        assert!(rig.overhead.position.distance(steady) < 1e-3);
        // Far blasts do nothing.
        let mut calm = CameraRig::default();
        calm.blast(200.0, 0.0, 6.0);
        assert_eq!(calm.shake.trauma, 0.0);
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
}
