//! Client-side prediction of the viewer's own hull.
//!
//! The host sends the viewer its hull bit for bit ([`HullState`]) with every snapshot
//! batch. [`TankPredictor`] keeps a small Rapier world (the ground, the arena's
//! cover and the other tanks as the newest snapshot placed them) and drives one hull
//! through the simulation's own drive model at the fixed step, so replaying the inputs
//! the host has not applied yet reproduces what the host will compute. Everything else
//! the host simulates (shells, blasts, debris, pickups, other players' steering) is
//! absent; the client corrects for it when the next snapshot arrives.

use std::collections::HashMap;

use rapier3d::prelude::*;
use serde_json::{Value, json};

use super::scene_codec::VEHICLE_KINDS;
use super::schema::ReadResult;
use super::wire::{WireReader, put_varint};
use crate::sim::damage::tree_stump;
use crate::sim::data::{ARENA, STEP, group};
use crate::sim::math::{Point3, Quat4, Vec2};
use crate::sim::physics::{
    from_rotation, from_vector, interaction_groups, to_rotation, to_vector, vector,
};
use crate::sim::render_state::{RenderCover, RenderState, RenderTank};
use crate::sim::simulation::{Simulation, cover_parts};
use crate::sim::simulation_rules::GRAVITY;
use crate::sim::tank_driving::{Hull, drive_hull};
use crate::sim::tank_lifecycle::tank_body_parts;
use crate::sim::types::{CoverKind, VehicleKind};

/// Multiplayer always drives at the standard speed.
const SPEED_SCALE: f64 = 1.0;

/// The viewer's hull after one host tick, exactly as the host's physics holds it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HullState {
    pub tick: u64,
    pub life: u32,
    pub kind: VehicleKind,
    pub position: Point3,
    pub rotation: Quat4,
    pub velocity: Point3,
    pub angular_velocity: Point3,
    pub heading: f64,
    /// Remaining speed-pickup seconds.
    pub speed: f64,
}

fn put_f32s(out: &mut Vec<u8>, point: Point3) {
    for value in [point.x, point.y, point.z] {
        out.extend_from_slice(&(value as f32).to_le_bytes());
    }
}

fn read_f32(reader: &mut WireReader<'_>) -> ReadResult<f64> {
    let bytes = reader.bytes(4)?;
    let value = f32::from_le_bytes(bytes.try_into().expect("four bytes"));
    if value.is_finite() {
        Ok(value as f64)
    } else {
        Err("Invalid hull number".into())
    }
}

fn read_f64(reader: &mut WireReader<'_>) -> ReadResult<f64> {
    let bytes = reader.bytes(8)?;
    let value = f64::from_le_bytes(bytes.try_into().expect("eight bytes"));
    if value.is_finite() {
        Ok(value)
    } else {
        Err("Invalid hull number".into())
    }
}

fn read_point(reader: &mut WireReader<'_>) -> ReadResult<Point3> {
    Ok(Point3::new(
        read_f32(reader)?,
        read_f32(reader)?,
        read_f32(reader)?,
    ))
}

impl HullState {
    /// The live tank at `tank_index` after tick `tick`; `None` for a wreck.
    pub fn capture(simulation: &Simulation, tank_index: usize, tick: u64) -> Option<Self> {
        let tank = &simulation.tanks[tank_index];
        if !tank.alive {
            return None;
        }
        let body = &simulation.world.bodies[tank.body];
        Some(Self {
            tick,
            life: tank.life,
            kind: tank.kind,
            position: from_vector(body.translation()),
            rotation: from_rotation(*body.rotation()),
            velocity: from_vector(body.linvel()),
            angular_velocity: from_vector(body.angvel()),
            heading: tank.heading,
            speed: tank.speed,
        })
    }

    /// The wire form: the physics values bit for bit, as Rapier's `f32` and the drive
    /// model's `f64`, little-endian.
    pub fn write(&self, out: &mut Vec<u8>) {
        put_varint(out, self.tick);
        put_varint(out, u64::from(self.life));
        out.push(
            VEHICLE_KINDS
                .iter()
                .position(|(_, kind)| *kind == self.kind)
                .expect("every chassis has a wire name") as u8,
        );
        put_f32s(out, self.position);
        let q = self.rotation;
        for value in [q.x, q.y, q.z, q.w] {
            out.extend_from_slice(&(value as f32).to_le_bytes());
        }
        put_f32s(out, self.velocity);
        put_f32s(out, self.angular_velocity);
        out.extend_from_slice(&self.heading.to_le_bytes());
        out.extend_from_slice(&self.speed.to_le_bytes());
    }

    pub fn read(reader: &mut WireReader<'_>) -> ReadResult<Self> {
        let tick = reader.varint()?;
        let life = reader.varint32()?;
        let kind = VEHICLE_KINDS
            .get(reader.byte()? as usize)
            .ok_or("Invalid hull kind")?
            .1;
        let position = read_point(reader)?;
        let rotation = Quat4 {
            x: read_f32(reader)?,
            y: read_f32(reader)?,
            z: read_f32(reader)?,
            w: read_f32(reader)?,
        };
        Ok(Self {
            tick,
            life,
            kind,
            position,
            rotation,
            velocity: read_point(reader)?,
            angular_velocity: read_point(reader)?,
            heading: read_f64(reader)?,
            speed: read_f64(reader)?,
        })
    }

    /// A readable form for the development wire log.
    pub fn to_json(&self) -> Value {
        let point = |p: Point3| json!([p.x, p.y, p.z]);
        let q = self.rotation;
        json!({
            "tick": self.tick,
            "life": self.life,
            "kind": self.kind.as_str(),
            "p": point(self.position),
            "q": [q.x, q.y, q.z, q.w],
            "v": point(self.velocity),
            "w": point(self.angular_velocity),
            "heading": self.heading,
            "speed": self.speed,
        })
    }
}

/// The predicted hull after a step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PredictedPose {
    pub position: Point3,
    pub velocity: Point3,
    pub heading: f64,
}

struct PredictedHull {
    body: RigidBodyHandle,
    life: u32,
    hull: Hull,
    speed: f64,
}

/// How a replicated cover stands in the predicted world; a change rebuilds its body.
#[derive(Clone, Copy, Debug, PartialEq)]
enum CoverForm {
    Standing,
    Stump,
}

struct CoverBody {
    body: RigidBodyHandle,
    form: CoverForm,
}

struct OtherTank {
    body: RigidBodyHandle,
    life: u32,
    hull: Hull,
    speed: f64,
    /// The move input of the newest snapshot; prediction holds it.
    drive: Vec2,
}

/// The viewer's hull in a world of the arena's cover and the other tanks.
pub struct TankPredictor {
    world: PhysicsWorld,
    hull: Option<PredictedHull>,
    covers: HashMap<u32, CoverBody>,
    tanks: HashMap<u32, OtherTank>,
    /// Storage reused to find replicated records that disappeared.
    stale: Vec<u32>,
}

impl Default for TankPredictor {
    fn default() -> Self {
        Self::new()
    }
}

impl TankPredictor {
    pub fn new() -> Self {
        let mut world = PhysicsWorld::new();
        world.gravity = vector(0.0, -GRAVITY, 0.0);
        world.integration_parameters.dt = STEP as f32;
        // The same ground slab as `Simulation::reset`.
        let ground =
            world.insert_body(RigidBodyBuilder::fixed().translation(vector(0.0, -0.5, 0.0)));
        world.insert_collider(
            ColliderBuilder::cuboid((ARENA + 2.0) as f32, 0.5, (ARENA + 2.0) as f32)
                .collision_groups(interaction_groups(group::GROUND)),
            Some(ground),
        );
        Self {
            world,
            hull: None,
            covers: HashMap::new(),
            tanks: HashMap::new(),
            stale: Vec::new(),
        }
    }

    /// Matches the obstacles to a received scene: standing cover and stumps, movable
    /// cover at its replicated pose, and every live tank other than the viewer's.
    pub fn sync_scene(&mut self, state: &RenderState) {
        self.stale.clear();
        self.stale.extend(self.covers.keys().copied());
        for cover in &state.covers {
            let Some(form) = cover_form(cover) else {
                continue;
            };
            self.stale.retain(|id| *id != cover.id);
            let current = self.covers.get(&cover.id).map(|existing| existing.form);
            if current != Some(form) {
                if let Some(old) = self.covers.remove(&cover.id) {
                    self.world.remove_body(old.body);
                }
                let body = self.insert_cover(cover, form);
                self.covers.insert(cover.id, CoverBody { body, form });
            }
            if cover.kind.movable() {
                // Movable cover rests where the host last had it; contact wakes it.
                let body = &mut self.world.bodies[self.covers[&cover.id].body];
                body.set_translation(to_vector(cover.position), false);
                body.set_rotation(to_rotation(cover.rotation), false);
                body.set_linvel(Vector::ZERO, false);
                body.set_angvel(Vector::ZERO, false);
                body.sleep();
            }
        }
        for id in self.stale.drain(..) {
            if let Some(old) = self.covers.remove(&id) {
                self.world.remove_body(old.body);
            }
        }

        self.stale.extend(self.tanks.keys().copied());
        for tank in &state.tanks {
            if tank.id == state.viewer_id || !tank.alive {
                continue;
            }
            self.stale.retain(|id| *id != tank.id);
            self.place_other_tank(tank);
        }
        for id in self.stale.drain(..) {
            if let Some(old) = self.tanks.remove(&id) {
                self.world.remove_body(old.body);
            }
        }
    }

    fn insert_cover(&mut self, cover: &RenderCover, form: CoverForm) -> RigidBodyHandle {
        // Movable cover keeps its authored shape; its replicated footprint follows the nav.
        let (x, z, w, d) = match cover.motion {
            Some(motion) => (motion.origin_x, motion.origin_z, motion.w, motion.d),
            None => (cover.x, cover.z, cover.w, cover.d),
        };
        let parts = cover_parts(cover.kind, x, z, w, d, cover.h);
        let body = self.world.insert_body(parts.body);
        match form {
            CoverForm::Standing => {
                for collider in parts.colliders {
                    self.world.insert_collider(collider, Some(body));
                }
                if let Some(footprint) = parts.tank_footprint {
                    self.world.insert_collider(footprint, Some(body));
                }
            }
            CoverForm::Stump => {
                let stump = tree_stump(x, z, w, d, cover.h);
                self.world.insert_collider(
                    ColliderBuilder::new(stump.shape)
                        .translation(stump.offset)
                        .collision_groups(interaction_groups(group::STUMP_CONTACT))
                        .friction(0.4),
                    Some(body),
                );
            }
        }
        body
    }

    fn place_other_tank(&mut self, tank: &RenderTank) {
        let rebuild = self
            .tanks
            .get(&tank.id)
            .is_none_or(|existing| existing.life != tank.life);
        if rebuild {
            if let Some(old) = self.tanks.remove(&tank.id) {
                self.world.remove_body(old.body);
            }
            let (body, colliders) = tank_body_parts(
                tank.kind,
                Vec2::new(tank.position.x, tank.position.z),
                SPEED_SCALE,
            );
            let body = self.world.insert_body(body);
            for collider in colliders {
                self.world.insert_collider(collider, Some(body));
            }
            self.tanks.insert(
                tank.id,
                OtherTank {
                    body,
                    life: tank.life,
                    hull: Hull {
                        kind: tank.kind,
                        heading: tank.heading,
                        boosted: false,
                        // Only a stuck bot HMMWV reverses; the client cannot tell when.
                        reverse_gear: tank.kind != VehicleKind::Humvee || tank.human,
                    },
                    speed: 0.0,
                    drive: Vec2::ZERO,
                },
            );
        }
        let other = self.tanks.get_mut(&tank.id).expect("inserted above");
        other.hull.heading = tank.heading;
        other.speed = tank.speed;
        other.drive = tank.drive;
        let body = &mut self.world.bodies[other.body];
        body.set_translation(to_vector(tank.position), true);
        body.set_rotation(to_rotation(Quat4::yaw(tank.heading)), true);
        body.set_linvel(to_vector(tank.velocity), true);
        body.set_angvel(Vector::ZERO, true);
    }

    /// Restarts the viewer's hull from the host's state. A new life or chassis gets a new
    /// body, as a respawn does on the host.
    pub fn reset(&mut self, state: &HullState) {
        let rebuild = self
            .hull
            .as_ref()
            .is_none_or(|hull| hull.life != state.life || hull.hull.kind != state.kind);
        if rebuild {
            if let Some(old) = self.hull.take() {
                self.world.remove_body(old.body);
            }
            let (body, colliders) = tank_body_parts(
                state.kind,
                Vec2::new(state.position.x, state.position.z),
                SPEED_SCALE,
            );
            let body = self.world.insert_body(body);
            for collider in colliders {
                self.world.insert_collider(collider, Some(body));
            }
            self.hull = Some(PredictedHull {
                body,
                life: state.life,
                hull: Hull {
                    kind: state.kind,
                    heading: state.heading,
                    boosted: false,
                    reverse_gear: true,
                },
                speed: state.speed,
            });
        }
        let hull = self.hull.as_mut().expect("created above");
        hull.hull.heading = state.heading;
        hull.speed = state.speed;
        let body = &mut self.world.bodies[hull.body];
        body.set_translation(to_vector(state.position), true);
        body.set_rotation(to_rotation(state.rotation), true);
        body.set_linvel(to_vector(state.velocity), true);
        body.set_angvel(to_vector(state.angular_velocity), true);
    }

    /// Forgets the hull, as when the viewer's tank dies.
    pub fn clear_hull(&mut self) {
        if let Some(old) = self.hull.take() {
            self.world.remove_body(old.body);
        }
    }

    pub fn has_hull(&self) -> bool {
        self.hull.is_some()
    }

    /// One fixed tick with the host's per-tank order: timers, then the drive impulse, then
    /// the world step. Other tanks keep driving with their replicated input.
    pub fn step(&mut self, move_x: f64, move_z: f64) {
        let Some(hull) = self.hull.as_mut() else {
            return;
        };
        drive(
            &mut hull.hull,
            &mut hull.speed,
            &mut self.world.bodies[hull.body],
            Vec2::new(move_x, move_z),
        );
        for other in self.tanks.values_mut() {
            drive(
                &mut other.hull,
                &mut other.speed,
                &mut self.world.bodies[other.body],
                other.drive,
            );
        }
        self.world.step();
    }

    pub fn pose(&self) -> Option<PredictedPose> {
        let hull = self.hull.as_ref()?;
        let body = &self.world.bodies[hull.body];
        Some(PredictedPose {
            position: from_vector(body.translation()),
            velocity: from_vector(body.linvel()),
            heading: hull.hull.heading,
        })
    }
}

/// How a cover record stands in the predicted world, or `None` when nothing is left of it.
fn cover_form(cover: &RenderCover) -> Option<CoverForm> {
    if cover.alive {
        Some(CoverForm::Standing)
    } else if cover.kind == CoverKind::Tree {
        Some(CoverForm::Stump)
    } else {
        None
    }
}

/// The host's per-tick drive: the speed pickup runs down before the drive impulse.
fn drive(hull: &mut Hull, speed: &mut f64, body: &mut RigidBody, input: Vec2) {
    *speed = 0f64.max(*speed - STEP);
    hull.boosted = *speed > 0.0;
    drive_hull(hull, body, input.x, input.z, STEP, SPEED_SCALE);
}
