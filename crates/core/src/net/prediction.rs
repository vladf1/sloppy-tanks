//! Client-side prediction of the viewer's own hull.
//!
//! The host sends the viewer its hull ([`HullState`]: the position bit for bit, the rest in
//! units far finer than prediction's tolerances) with every snapshot batch. [`TankPredictor`] keeps a small Rapier world (the ground, the arena's
//! cover and the other tanks as the newest snapshot placed them) and drives one hull
//! through the simulation's own drive model at the fixed step, so replaying the inputs
//! the host has not applied yet reproduces what the host will compute. Everything else
//! the host simulates (shells, blasts, debris, pickups, other players' steering) is
//! absent; the client corrects for it when the next snapshot arrives.

use std::collections::HashMap;

use rapier3d::prelude::*;
use serde_json::{Value, json};

use super::own_shots::{CoverCast, CoverHit, Struck};
use super::scene_codec::{VEHICLE_KINDS, index_of};
use super::schema::{NUMBER_BOUND, ReadResult};
use super::wire::{WireReader, put_signed, put_varint, units};
use crate::sim::damage::tree_stump;
use crate::sim::data::vehicle;
use crate::sim::data::{STEP, group};
use crate::sim::debris_cleanup::DEBRIS_CLEANUP_SECONDS;
use crate::sim::debris_physics::{DebrisMaterial, debris_material};
use crate::sim::math::{Point3, Quat4, Vec2};
use crate::sim::physics::{
    from_vector, interaction_groups, query_filter, to_rotation, to_vector, vector,
};
use crate::sim::render_state::{RenderCover, RenderFragment, RenderState, RenderTank};
use crate::sim::simulation::{Simulation, arena_world, cover_parts};
use crate::sim::tank_driving::{Hull, drive_hull};
use crate::sim::tank_lifecycle::tank_body_parts;
use crate::sim::types::{CoverKind, FragmentShape, VehicleKind};
use crate::sim::wrecks::wreck_piece_box;

/// The viewer's hull after one host tick, as the host's physics holds it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HullState {
    pub tick: u64,
    pub life: u32,
    pub kind: VehicleKind,
    pub position: Point3,
    pub velocity: Point3,
    /// Angular velocity about Y, the only axis a hull turns on. Its rotation is not sent:
    /// the drive sets it from `heading` before every step.
    pub spin: f64,
    pub heading: f64,
    /// Remaining speed-pickup seconds.
    pub speed: f64,
}

/// The hull's wire units besides its position: a tenth of a millimetre per second, a
/// ten-thousandth of a radian per second of spin, a tenth of a microradian of heading and
/// a tenth of a millisecond of speed pickup, far inside the tolerances a prediction is
/// confirmed within (centimetres per second, milliradians). The position stays Rapier's
/// `f32` bit for bit: the ground contact a replay starts with moves a hull placed even a
/// hundredth of a millimetre off by a millimetre within 200 ms.
const HULL_VELOCITY_SCALE: f64 = 1e4;
const HULL_SPIN_SCALE: f64 = 1e4;
const HULL_HEADING_SCALE: f64 = 1e7;
const HULL_SPEED_SCALE: f64 = 1e4;
/// Moving bodies' velocity in millimetres per second and spin in milliradians per
/// second: their poses replicate in millimetres and ten-thousandths, so finer velocities
/// would not carry them on any better.
const BODY_MOTION_SCALE: f64 = 1e3;

fn put_fixed(out: &mut Vec<u8>, value: f64, scale: f64) {
    put_signed(out, units(value, scale));
}

fn put_point(out: &mut Vec<u8>, point: Point3, scale: f64) {
    for value in [point.x, point.y, point.z] {
        put_fixed(out, value, scale);
    }
}

fn put_f32s(out: &mut Vec<u8>, point: Point3) {
    for value in [point.x, point.y, point.z] {
        out.extend_from_slice(&(value as f32).to_le_bytes());
    }
}

fn read_f32(reader: &mut WireReader<'_>) -> ReadResult<f64> {
    let value = f32::from_le_bytes(reader.bytes(4)?.try_into().expect("four bytes"));
    if value.is_finite() {
        Ok(value as f64)
    } else {
        Err("Invalid hull number".into())
    }
}

fn read_fixed(reader: &mut WireReader<'_>, scale: f64) -> ReadResult<f64> {
    let value = reader.signed()? as f64 / scale;
    if value.abs() <= NUMBER_BOUND {
        Ok(value)
    } else {
        Err("Invalid hull number".into())
    }
}

fn read_point(reader: &mut WireReader<'_>, scale: f64) -> ReadResult<Point3> {
    Ok(Point3::new(
        read_fixed(reader, scale)?,
        read_fixed(reader, scale)?,
        read_fixed(reader, scale)?,
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
            velocity: from_vector(body.linvel()),
            spin: body.angvel().y as f64,
            heading: tank.heading,
            speed: tank.speed,
        })
    }

    /// The wire form: tick, life and kind, the position as little-endian `f32`, then the
    /// rest as signed whole units (`HULL_*_SCALE`).
    pub fn write(&self, out: &mut Vec<u8>) {
        put_varint(out, self.tick);
        put_varint(out, u64::from(self.life));
        out.push(index_of(&VEHICLE_KINDS, self.kind) as u8);
        put_f32s(out, self.position);
        put_point(out, self.velocity, HULL_VELOCITY_SCALE);
        put_fixed(out, self.spin, HULL_SPIN_SCALE);
        put_fixed(out, self.heading, HULL_HEADING_SCALE);
        put_fixed(out, self.speed, HULL_SPEED_SCALE);
    }

    pub fn read(reader: &mut WireReader<'_>) -> ReadResult<Self> {
        let tick = reader.varint()?;
        let life = reader.varint32()?;
        let kind = VEHICLE_KINDS
            .get(reader.byte()? as usize)
            .ok_or("Invalid hull kind")?
            .1;
        Ok(Self {
            tick,
            life,
            kind,
            position: Point3::new(read_f32(reader)?, read_f32(reader)?, read_f32(reader)?),
            velocity: read_point(reader, HULL_VELOCITY_SCALE)?,
            spin: read_fixed(reader, HULL_SPIN_SCALE)?,
            heading: read_fixed(reader, HULL_HEADING_SCALE)?,
            speed: read_fixed(reader, HULL_SPEED_SCALE)?,
        })
    }

    /// A readable form for the development wire log.
    pub fn to_json(&self) -> Value {
        let point = |p: Point3| json!([p.x, p.y, p.z]);
        json!({
            "tick": self.tick,
            "life": self.life,
            "kind": self.kind.as_str(),
            "p": point(self.position),
            "v": point(self.velocity),
            "spin": self.spin,
            "heading": self.heading,
            "speed": self.speed,
        })
    }
}

/// Which replicated record a [`MovingBody`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    /// Movable cover: a drum, a crate, a tank trap.
    Cover,
    /// A piece of solid debris.
    Fragment,
}

/// Bodies this close to the viewer's hull, and awake, travel with its hull, metres.
pub const MOVING_BODY_REACH: f64 = 8.0;
/// Most moving bodies one snapshot carries.
pub const MAX_MOVING_BODIES: usize = 16;

/// A body moving near the viewer's hull, with its velocity as the host's physics holds
/// it, so prediction carries it on from its replicated pose instead of from rest. The
/// replicated scene sends poses, not velocities.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovingBody {
    pub kind: BodyKind,
    pub id: u32,
    pub velocity: Point3,
    pub spin: Point3,
}

impl MovingBody {
    /// The awake movable cover and solid debris within [`MOVING_BODY_REACH`] of the live
    /// tank at `tank_index`, nearest first, at most [`MAX_MOVING_BODIES`].
    pub fn capture_near(simulation: &Simulation, tank_index: usize, out: &mut Vec<MovingBody>) {
        out.clear();
        let tank = &simulation.tanks[tank_index];
        if !tank.alive {
            return;
        }
        let at = simulation.body_translation(tank.body).planar();
        let mut near = |kind: BodyKind, id: u32, handle: RigidBodyHandle| {
            let Some(body) = simulation.world.bodies.get(handle) else {
                return;
            };
            let gap = crate::sim::math::distance(from_vector(body.translation()).planar(), at);
            if body.is_sleeping() || gap > MOVING_BODY_REACH {
                return;
            }
            out.push(MovingBody {
                kind,
                id,
                velocity: from_vector(body.linvel()),
                spin: from_vector(body.angvel()),
            });
        };
        for &index in &simulation.movable_covers {
            let cover = &simulation.covers[index];
            if cover.alive {
                near(BodyKind::Cover, cover.id, cover.body);
            }
        }
        for fragment in &simulation.fragments {
            if fragment.wreck.is_some() || fragment.dimensions.is_some() {
                near(BodyKind::Fragment, fragment.id, fragment.body);
            }
        }
        let gap = |body: &MovingBody| {
            let handle = match body.kind {
                BodyKind::Cover => simulation
                    .covers
                    .iter()
                    .find(|cover| cover.id == body.id)
                    .map(|cover| cover.body),
                BodyKind::Fragment => simulation
                    .fragments
                    .iter()
                    .find(|fragment| fragment.id == body.id)
                    .map(|fragment| fragment.body),
            };
            handle.map_or(f64::INFINITY, |handle| {
                crate::sim::math::distance(simulation.body_translation(handle).planar(), at)
            })
        };
        if out.len() > MAX_MOVING_BODIES {
            out.sort_by(|a, b| gap(a).total_cmp(&gap(b)));
            out.truncate(MAX_MOVING_BODIES);
        }
    }

    /// The wire form: kind, id, then velocity and spin as signed whole units
    /// (`BODY_MOTION_SCALE`).
    pub fn write(&self, out: &mut Vec<u8>) {
        out.push(match self.kind {
            BodyKind::Cover => 0,
            BodyKind::Fragment => 1,
        });
        put_varint(out, u64::from(self.id));
        put_point(out, self.velocity, BODY_MOTION_SCALE);
        put_point(out, self.spin, BODY_MOTION_SCALE);
    }

    pub fn read(reader: &mut WireReader<'_>) -> ReadResult<Self> {
        let kind = match reader.byte()? {
            0 => BodyKind::Cover,
            1 => BodyKind::Fragment,
            _ => return Err("Invalid moving body".into()),
        };
        Ok(Self {
            kind,
            id: reader.varint32()?,
            velocity: read_point(reader, BODY_MOTION_SCALE)?,
            spin: read_point(reader, BODY_MOTION_SCALE)?,
        })
    }

    /// A readable form for the development wire log.
    pub fn to_json(&self) -> Value {
        let point = |p: Point3| json!([p.x, p.y, p.z]);
        json!({
            "kind": match self.kind {
                BodyKind::Cover => "cover",
                BodyKind::Fragment => "fragment",
            },
            "id": self.id,
            "v": point(self.velocity),
            "spin": point(self.spin),
        })
    }
}

/// The predicted hull after a step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PredictedPose {
    pub position: Point3,
    pub velocity: Point3,
    pub heading: f64,
    /// Remaining speed-pickup seconds.
    pub speed: f64,
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
    movable: bool,
    /// What a shell striking it shows.
    struck: Struck,
    /// The scene sync that last found its record.
    seen: u64,
}

/// What the viewer's hull touched in a predicted step, for the correction statistics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HullContacts {
    pub tank: bool,
    pub movable_cover: bool,
    pub debris: bool,
    /// Standing cover or a stump.
    pub cover: bool,
}

impl HullContacts {
    pub fn add(&mut self, other: HullContacts) {
        self.tank |= other.tank;
        self.movable_cover |= other.movable_cover;
        self.debris |= other.debris;
        self.cover |= other.cover;
    }
}

/// A replicated record whose body the predicted world moves: another tank, movable cover
/// or a piece of solid debris.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Moved {
    Tank(u32),
    Cover(u32),
    Fragment(u32),
}

/// A moved body's pose in the predicted world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyPose {
    pub position: Point3,
    pub rotation: Quat4,
    /// A tank's hull heading.
    pub heading: f64,
    pub velocity: Point3,
    /// Asleep or all but still.
    pub resting: bool,
}

/// Debris this far from the viewer's hull (metres) joins the predicted world: farther
/// than the hull can drive in a prediction window.
const DEBRIS_REACH: f64 = 12.0;
/// A body slower than this (m/s and rad/s) counts as resting.
const RESTING_SPEED: f64 = 0.05;

/// A piece of solid debris in the predicted world.
struct DebrisBody {
    body: RigidBodyHandle,
    seen: u64,
}

struct OtherTank {
    body: RigidBodyHandle,
    life: u32,
    hull: Hull,
    speed: f64,
    /// The move input of the newest snapshot; prediction holds it.
    drive: Vec2,
    /// The scene sync that last found it alive.
    seen: u64,
}

/// The viewer's hull in a world of the arena's cover and the other tanks.
pub struct TankPredictor {
    world: PhysicsWorld,
    hull: Option<PredictedHull>,
    covers: HashMap<u32, CoverBody>,
    tanks: HashMap<u32, OtherTank>,
    debris: HashMap<u32, DebrisBody>,
    /// Counts scene syncs, to find replicated records that disappeared.
    sync: u64,
    /// Storage reused for their ids.
    gone: Vec<u32>,
}

impl Default for TankPredictor {
    fn default() -> Self {
        Self {
            world: arena_world(),
            hull: None,
            covers: HashMap::new(),
            tanks: HashMap::new(),
            debris: HashMap::new(),
            sync: 0,
            gone: Vec::new(),
        }
    }
}

impl TankPredictor {
    /// Matches the obstacles to a received scene: standing cover and stumps, movable
    /// cover at its replicated pose, and every live tank other than the viewer's.
    pub fn sync_scene(&mut self, state: &RenderState) {
        self.sync_scene_ahead(state, 0.0);
    }

    /// Like [`Self::sync_scene`] for a world `seconds` ahead of the scene: the other tanks
    /// are carried that far along their velocities.
    pub fn sync_scene_ahead(&mut self, state: &RenderState, seconds: f64) {
        self.sync_covers(state);
        self.sync_tanks(state, seconds);
    }

    /// Matches the cover to a received scene: standing cover and stumps, and movable cover
    /// at its replicated pose.
    pub fn sync_covers(&mut self, state: &RenderState) {
        self.sync += 1;
        let sync = self.sync;
        for cover in &state.covers {
            let Some(form) = cover_form(cover) else {
                continue;
            };
            let current = self.covers.get(&cover.id).map(|existing| existing.form);
            if current != Some(form) {
                if let Some(old) = self.covers.remove(&cover.id) {
                    self.world.remove_body(old.body);
                }
                let body = self.insert_cover(cover, form);
                let movable = cover.kind.movable();
                self.covers.insert(
                    cover.id,
                    CoverBody {
                        body,
                        form,
                        movable,
                        struck: Struck {
                            kind: cover.kind,
                            color: cover.color,
                            height: cover.h,
                        },
                        seen: sync,
                    },
                );
            }
            let existing = self.covers.get_mut(&cover.id).expect("inserted above");
            existing.seen = sync;
            if existing.movable {
                // Movable cover rests where the host last had it; contact wakes it.
                let body = &mut self.world.bodies[self.covers[&cover.id].body];
                body.set_translation(to_vector(cover.position), false);
                body.set_rotation(to_rotation(cover.rotation), false);
                body.set_linvel(Vector::ZERO, false);
                body.set_angvel(Vector::ZERO, false);
                body.sleep();
            }
        }
        self.gone.extend(
            self.covers
                .iter()
                .filter(|(_, cover)| cover.seen != sync)
                .map(|(id, _)| *id),
        );
        for id in self.gone.drain(..) {
            if let Some(old) = self.covers.remove(&id) {
                self.world.remove_body(old.body);
            }
        }
    }

    /// Matches what moves to a received scene: the bodies the host reported moving near the
    /// hull carry on at their velocities (the rest of the movable cover rests), and the
    /// solid debris within [`DEBRIS_REACH`] of `around` joins the world at its replicated
    /// pose. Call after [`sync_scene`](Self::sync_scene) with the same scene.
    pub fn sync_bodies(&mut self, state: &RenderState, bodies: &[MovingBody], around: Point3) {
        let sync = self.sync;
        let moving = |kind: BodyKind, id: u32| {
            bodies
                .iter()
                .find(|body| body.kind == kind && body.id == id)
        };
        for moved in bodies.iter().filter(|body| body.kind == BodyKind::Cover) {
            if let Some(cover) = self.covers.get(&moved.id).filter(|cover| cover.movable) {
                let body = &mut self.world.bodies[cover.body];
                body.set_linvel(to_vector(moved.velocity), true);
                body.set_angvel(to_vector(moved.spin), true);
            }
        }
        for fragment in &state.fragments {
            // Replicated life stops at the cleanup's length: below it the piece is sinking
            // and no longer blocks tanks.
            if !solid_debris(fragment)
                || fragment.life < DEBRIS_CLEANUP_SECONDS
                || crate::sim::math::distance(fragment.position.planar(), around.planar())
                    > DEBRIS_REACH
            {
                continue;
            }
            if !self.debris.contains_key(&fragment.id) {
                let Some((body, collider)) = debris_parts(fragment) else {
                    continue;
                };
                let body = self.world.insert_body(body);
                self.world.bodies[body].set_additional_solver_iterations(2);
                self.world.insert_collider(collider, Some(body));
                self.debris
                    .insert(fragment.id, DebrisBody { body, seen: sync });
            }
            let debris = self.debris.get_mut(&fragment.id).expect("inserted above");
            debris.seen = sync;
            let body = &mut self.world.bodies[debris.body];
            body.set_translation(to_vector(fragment.position), false);
            body.set_rotation(to_rotation(fragment.rotation), false);
            match moving(BodyKind::Fragment, fragment.id) {
                Some(moved) => {
                    body.set_linvel(to_vector(moved.velocity), true);
                    body.set_angvel(to_vector(moved.spin), true);
                }
                None => {
                    body.set_linvel(Vector::ZERO, false);
                    body.set_angvel(Vector::ZERO, false);
                    body.sleep();
                }
            }
        }
        self.gone.extend(
            self.debris
                .iter()
                .filter(|(_, debris)| debris.seen != sync)
                .map(|(id, _)| *id),
        );
        for id in self.gone.drain(..) {
            if let Some(old) = self.debris.remove(&id) {
                self.world.remove_body(old.body);
            }
        }
    }

    /// Places every live tank other than the viewer's as a received scene has it, carried
    /// `seconds` along its velocity.
    fn sync_tanks(&mut self, state: &RenderState, seconds: f64) {
        let sync = self.sync;
        for tank in &state.tanks {
            if tank.id == state.viewer_id || !tank.alive {
                continue;
            }
            self.place_other_tank(tank, sync, seconds);
        }
        self.gone.extend(
            self.tanks
                .iter()
                .filter(|(_, tank)| tank.seen != sync)
                .map(|(id, _)| *id),
        );
        for id in self.gone.drain(..) {
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

    /// A tank body at `position` with its colliders, as the host builds one.
    fn insert_tank(&mut self, kind: VehicleKind, position: Point3) -> RigidBodyHandle {
        let (body, colliders) = tank_body_parts(kind, position.planar());
        let body = self.world.insert_body(body);
        for collider in colliders {
            self.world.insert_collider(collider, Some(body));
        }
        body
    }

    fn place_other_tank(&mut self, tank: &RenderTank, sync: u64, seconds: f64) {
        let rebuild = self
            .tanks
            .get(&tank.id)
            .is_none_or(|existing| existing.life != tank.life);
        if rebuild {
            if let Some(old) = self.tanks.remove(&tank.id) {
                self.world.remove_body(old.body);
            }
            let body = self.insert_tank(tank.kind, tank.position);
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
                    seen: sync,
                },
            );
        }
        let other = self.tanks.get_mut(&tank.id).expect("inserted above");
        other.seen = sync;
        other.hull.heading = tank.heading;
        other.speed = tank.speed;
        other.drive = tank.drive;
        let mut position = tank.position;
        position.x += tank.velocity.x * seconds;
        position.z += tank.velocity.z * seconds;
        let body = &mut self.world.bodies[other.body];
        body.set_translation(to_vector(position), true);
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
            let body = self.insert_tank(state.kind, state.position);
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
        body.set_rotation(to_rotation(Quat4::yaw(state.heading)), true);
        body.set_linvel(to_vector(state.velocity), true);
        body.set_angvel(vector(0.0, state.spin, 0.0), true);
    }

    /// Forgets the hull, as when the viewer's tank dies.
    pub fn clear_hull(&mut self) {
        if let Some(old) = self.hull.take() {
            self.world.remove_body(old.body);
        }
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

    /// What the viewer's hull touches after the last step: other tanks, cover and debris it
    /// is in contact with. The ground does not count.
    pub fn hull_contacts(&self) -> HullContacts {
        let mut contacts = HullContacts::default();
        self.touching(|moved| match moved {
            Some(Moved::Tank(_)) => contacts.tank = true,
            Some(Moved::Cover(_)) => contacts.movable_cover = true,
            Some(Moved::Fragment(_)) => contacts.debris = true,
            None => contacts.cover = true,
        });
        contacts
    }

    /// Calls `touched` for every body the viewer's hull is in contact with after the last
    /// step: a moved record, or `None` for standing cover and stumps.
    pub fn touching(&self, mut touched: impl FnMut(Option<Moved>)) {
        let Some(hull) = self.hull.as_ref() else {
            return;
        };
        for &collider in self.world.bodies[hull.body].colliders() {
            for pair in self.world.contact_pairs_with(collider) {
                if !pair.has_any_active_contact() {
                    continue;
                }
                let other = if pair.collider1 == collider {
                    pair.collider2
                } else {
                    pair.collider1
                };
                let Some(body) = self.world.colliders[other].parent() else {
                    continue;
                };
                if let Some((&id, _)) = self.tanks.iter().find(|(_, tank)| tank.body == body) {
                    touched(Some(Moved::Tank(id)));
                } else if let Some((&id, cover)) =
                    self.covers.iter().find(|(_, cover)| cover.body == body)
                {
                    touched(cover.movable.then_some(Moved::Cover(id)));
                } else if let Some((&id, _)) =
                    self.debris.iter().find(|(_, debris)| debris.body == body)
                {
                    touched(Some(Moved::Fragment(id)));
                }
            }
        }
    }

    /// Other tanks within `reach` of the viewer's hull.
    pub fn tanks_near(&self, reach: f64, mut near: impl FnMut(Moved)) {
        let Some(hull) = self.pose() else {
            return;
        };
        for (&id, tank) in &self.tanks {
            let at = from_vector(self.world.bodies[tank.body].translation());
            if crate::sim::math::distance(at.planar(), hull.position.planar()) <= reach {
                near(Moved::Tank(id));
            }
        }
    }

    /// Where the predicted world has a moved body now.
    pub fn body_pose(&self, moved: Moved) -> Option<BodyPose> {
        let (handle, heading) = match moved {
            Moved::Tank(id) => self
                .tanks
                .get(&id)
                .map(|tank| (tank.body, tank.hull.heading))?,
            Moved::Cover(id) => (
                self.covers.get(&id).filter(|cover| cover.movable)?.body,
                0.0,
            ),
            Moved::Fragment(id) => (self.debris.get(&id)?.body, 0.0),
        };
        let body = &self.world.bodies[handle];
        let (linvel, angvel) = (body.linvel(), body.angvel());
        Some(BodyPose {
            position: from_vector(body.translation()),
            rotation: crate::sim::physics::from_rotation(*body.rotation()),
            heading,
            velocity: from_vector(linvel),
            resting: body.is_sleeping()
                || (f64::from(linvel.length()) < RESTING_SPEED
                    && f64::from(angvel.length()) < RESTING_SPEED),
        })
    }

    pub fn pose(&self) -> Option<PredictedPose> {
        let hull = self.hull.as_ref()?;
        let body = &self.world.bodies[hull.body];
        Some(PredictedPose {
            position: from_vector(body.translation()),
            velocity: from_vector(body.linvel()),
            heading: hull.hull.heading,
            speed: hull.speed,
        })
    }
}

/// The cover as the host's shell query sees it.
impl CoverCast for TankPredictor {
    fn cast_cover(&self, origin: Point3, direction: Vec2, distance: f64) -> Option<CoverHit> {
        let ray = Ray::new(to_vector(origin), vector(direction.x, 0.0, direction.z));
        let (collider, hit) = self.world.cast_ray_and_get_normal(
            &ray,
            distance as f32,
            true,
            query_filter(group::COVER_QUERY),
        )?;
        let body = self.world.colliders[collider].parent();
        Some(CoverHit {
            distance: f64::from(hit.time_of_impact),
            normal: Vec2::new(f64::from(hit.normal.x), f64::from(hit.normal.z)),
            struck: self
                .covers
                .values()
                .find(|cover| Some(cover.body) == body)
                .map(|cover| cover.struck),
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

/// The body and collider the host gives a piece of solid debris (`add_wreck_piece`,
/// `Breakup::piece_with`), built from its replicated record; `None` for other fragments.
fn debris_parts(fragment: &RenderFragment) -> Option<(RigidBodyBuilder, ColliderBuilder)> {
    if let (Some(kind), Some(part)) = (fragment.wreck, fragment.part) {
        let (half, share) = wreck_piece_box(kind, part);
        let [x, y, z] = half.map(|extent| extent as f32);
        let mass = vehicle(kind).mass * 1.6 * share;
        return Some((
            RigidBodyBuilder::dynamic().ccd_enabled(true),
            ColliderBuilder::cuboid(x, y, z)
                .collision_groups(interaction_groups(group::WRECK))
                .mass(mass as f32)
                .friction(0.95)
                .restitution(0.05),
        ));
    }
    let size = fragment.dimensions?;
    let (w, h, d) = (size.x, size.y, size.z);
    let builder = if fragment.shape == Some(FragmentShape::Log) {
        ColliderBuilder::cylinder((h / 2.0) as f32, (w / 2.0) as f32)
    } else {
        ColliderBuilder::cuboid((w / 2.0) as f32, (h / 2.0) as f32, (d / 2.0) as f32)
    };
    let material = fragment.material.unwrap_or(DebrisMaterial::Wood);
    let density = if material == DebrisMaterial::Metal {
        0.65
    } else {
        0.35
    };
    let surface = debris_material(material);
    let membership = if fragment.timber_part.is_some() {
        group::TIMBER_DEBRIS
    } else if fragment.tower_piece.is_some() {
        group::CRUSHED_FRAME
    } else if fragment.house_piece.is_some() {
        group::HOUSE_SHARD
    } else {
        group::PUSHABLE_DEBRIS
    };
    Some((
        RigidBodyBuilder::dynamic()
            .linear_damping(0.12)
            .angular_damping(0.25)
            .can_sleep(true),
        builder
            .collision_groups(interaction_groups(membership))
            .mass(0.18f64.max(w * h * d * density) as f32)
            .friction(surface.friction as f32)
            .restitution(surface.restitution as f32),
    ))
}

/// Whether a replicated fragment blocks tanks on the host: wreck parts and the authored
/// pieces of broken scenery (timber, crate sections, tower and house pieces), which the
/// predicted world leaves out. Small shards only touch the ground and cover.
pub fn solid_debris(fragment: &RenderFragment) -> bool {
    fragment.wreck.is_some() || fragment.dimensions.is_some()
}

/// The host's per-tick drive: the speed pickup runs down before the drive impulse.
fn drive(hull: &mut Hull, speed: &mut f64, body: &mut RigidBody, input: Vec2) {
    *speed = 0f64.max(*speed - STEP);
    hull.boosted = *speed > 0.0;
    drive_hull(hull, body, input.x, input.z, STEP);
}
