//! Client-side prediction of the viewer's own hull.
//!
//! The host sends the viewer its hull ([`HullState`]: the position bit for bit, the rest in
//! units far finer than prediction's tolerances) with every snapshot batch. [`TankPredictor`] keeps a small Rapier world (the ground, the arena's
//! cover and the other tanks as the newest snapshot placed them) and drives one hull
//! through the simulation's own drive model at the fixed step, so replaying the inputs
//! the host has not applied yet reproduces what the host will compute. Everything else
//! the host simulates (shells, blasts, debris, pickups, other players' steering) is
//! absent; the client corrects for it when the next snapshot arrives.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use rapier3d::prelude::*;
use serde_json::{Value, json};

use super::own_shots::{CoverCast, CoverHit, PartHit, Struck};
use super::scene_codec::{VEHICLE_KINDS, index_of};
use super::schema::{NUMBER_BOUND, ReadResult};
use super::wire::{WireReader, put_signed, put_varint, units};
use crate::sim::simulation::structure_parts;
use crate::sim::structure::{PartSet, Structure, structure_of};
use crate::sim::structure_physics::{
    WHOLE, cap_loose_speed, debris_colliders, part_collider, rest_body, structural, unrest_body,
};
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
        // A cottage's loose parts hang off the ground centre of the cottage they fell
        // from: they are as far as their centre of mass.
        let place = |handle: RigidBodyHandle, loose_parts: bool| {
            let body = &simulation.world.bodies[handle];
            from_vector(if loose_parts {
                body.center_of_mass()
            } else {
                body.translation()
            })
            .planar()
        };
        let mut near = |kind: BodyKind, id: u32, handle: RigidBodyHandle, loose_parts: bool| {
            let Some(body) = simulation.world.bodies.get(handle) else {
                return;
            };
            let gap = crate::sim::math::distance(place(handle, loose_parts), at);
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
                near(BodyKind::Cover, cover.id, cover.body, false);
            }
        }
        for fragment in &simulation.fragments {
            let loose_parts = fragment.structure.is_some();
            if fragment.wreck.is_some() || fragment.dimensions.is_some() || loose_parts {
                near(BodyKind::Fragment, fragment.id, fragment.body, loose_parts);
            }
        }
        let gap = |body: &MovingBody| {
            let handle = match body.kind {
                BodyKind::Cover => simulation
                    .covers
                    .iter()
                    .find(|cover| cover.id == body.id)
                    .map(|cover| (cover.body, false)),
                BodyKind::Fragment => simulation
                    .fragments
                    .iter()
                    .find(|fragment| fragment.id == body.id)
                    .map(|fragment| {
                        let loose_parts =
                            fragment.structure.is_some();
                        (fragment.body, loose_parts)
                    }),
            };
            handle.map_or(f64::INFINITY, |(handle, loose_parts)| {
                crate::sim::math::distance(place(handle, loose_parts), at)
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
    /// A cottage's standing parts: those not in the set.
    Parts(PartSet),
}

struct CoverBody {
    body: RigidBodyHandle,
    form: CoverForm,
    movable: bool,
    /// What a shell striking it shows.
    struck: Struck,
    /// A structure's layout, and its parts that took damage.
    layout: Option<Arc<Structure>>,
    /// Each part's standing shape and its pose on the body, built when a shell is first
    /// forecast through the intact structure; they depend on the layout alone.
    part_shapes: OnceLock<Box<[(Pose, SharedShape)]>>,
    cracked: PartSet,
    /// The scene sync that last found its record.
    seen: u64,
}

/// What a body of the predicted world stands for, kept in its user data, so a contact or
/// a query finds the record behind a body without searching for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner {
    Tank(u32),
    Cover(u32),
    Debris(u32),
}

const TANK_BODY: u128 = 1 << 64;
const COVER_BODY: u128 = 2 << 64;
const DEBRIS_BODY: u128 = 3 << 64;

impl Owner {
    fn user_data(self) -> u128 {
        match self {
            Owner::Tank(id) => TANK_BODY | u128::from(id),
            Owner::Cover(id) => COVER_BODY | u128::from(id),
            Owner::Debris(id) => DEBRIS_BODY | u128::from(id),
        }
    }

    /// The owner a body's user data names; none for the viewer's hull and the ground.
    fn of(user_data: u128) -> Option<Owner> {
        let id = user_data as u32;
        match user_data & !u128::from(u32::MAX) {
            TANK_BODY => Some(Owner::Tank(id)),
            COVER_BODY => Some(Owner::Cover(id)),
            DEBRIS_BODY => Some(Owner::Debris(id)),
            _ => None,
        }
    }
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
/// The farthest (m) a structure's fallen part reaches from the ground centre of the
/// structure it fell from, which its body is placed by: a watchtower's roof corner over
/// a footing, farther than a cottage's eaves.
const STRUCTURE_DEBRIS_OFFSET: f64 = 9.5;
/// A body slower than this (m/s and rad/s) counts as resting.
const RESTING_SPEED: f64 = 0.05;

/// A piece of solid debris in the predicted world.
struct DebrisBody {
    body: RigidBodyHandle,
    seen: u64,
    /// Held at rest for good, as the host holds a structure's piece.
    rested: bool,
    /// A structure's fallen parts, whose speed the host caps while they are loose.
    structure: bool,
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
    // Ordered by id: they are walked to remove bodies and drive the other tanks, and the
    // order bodies leave and join the world decides its handles and its solver's order,
    // so a hashed order made each run of the same frames predict a little differently.
    covers: BTreeMap<u32, CoverBody>,
    tanks: BTreeMap<u32, OtherTank>,
    debris: BTreeMap<u32, DebrisBody>,
    /// Counts scene syncs, to find replicated records that disappeared.
    sync: u64,
    /// Storage reused for their ids.
    gone: Vec<u32>,
    /// Storage reused for the colliders of a structure's parts that broke.
    broken: Vec<ColliderHandle>,
}

impl Default for TankPredictor {
    fn default() -> Self {
        Self {
            world: arena_world(),
            hull: None,
            covers: BTreeMap::new(),
            tanks: BTreeMap::new(),
            debris: BTreeMap::new(),
            sync: 0,
            gone: Vec::new(),
            broken: Vec::new(),
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
            if current != Some(form) && !self.break_parts(cover.id, form) {
                let (mut layout, mut part_shapes) = (None, OnceLock::new());
                if let Some(old) = self.covers.remove(&cover.id) {
                    self.world.remove_body(old.body);
                    (layout, part_shapes) = (old.layout, old.part_shapes);
                }
                // A structure's layout is built once; a break only changes its parts.
                let layout = layout.or_else(|| {
                    let (x, z, w, d) = (cover.x, cover.z, cover.w, cover.d);
                    structure_of(cover.kind, x, z, w, d, cover.h, cover.timber_join)
                });
                let body = self.insert_cover(cover, form, layout.as_deref());
                self.world.bodies[body].user_data = Owner::Cover(cover.id).user_data();
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
                        layout,
                        part_shapes,
                        cracked: PartSet::EMPTY,
                        seen: sync,
                    },
                );
            }
            let existing = self.covers.get_mut(&cover.id).expect("inserted above");
            existing.seen = sync;
            existing.cracked = cover.cracked;
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

    /// Takes the parts that broke out of a structure standing as its parts, as the host
    /// takes their colliders out, when `form` only adds broken parts to the ones it had.
    /// Returns false, changing nothing, when the structure needs building again: it stood
    /// whole (one collider) or a rebuild restored parts.
    fn break_parts(&mut self, id: u32, form: CoverForm) -> bool {
        let Some(existing) = self.covers.get_mut(&id) else {
            return false;
        };
        let (CoverForm::Parts(held), CoverForm::Parts(broken)) = (existing.form, form) else {
            return false;
        };
        let Some(layout) = existing.layout.as_deref() else {
            return false;
        };
        if (layout.whole && held.is_empty()) || held.union(&broken) != broken {
            return false;
        }
        self.broken.clear();
        self.broken.extend(
            self.world.bodies[existing.body]
                .colliders()
                .iter()
                .copied()
                .filter(|&collider| {
                    let part = self.world.colliders[collider].user_data as usize;
                    broken.contains(part) && !held.contains(part)
                }),
        );
        for collider in self.broken.drain(..) {
            self.world.remove_collider(collider);
        }
        existing.form = form;
        true
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
                    > DEBRIS_REACH + debris_offset(fragment)
            {
                continue;
            }
            if !self.debris.contains_key(&fragment.id) {
                let colliders = match (fragment.structure_id, fragment.structure_parts) {
                    // A structure's fallen parts: the replicated set gives their shapes,
                    // from the structure's layout.
                    (Some(structure), Some(parts)) => {
                        let Some(layout) = self
                            .covers
                            .get(&structure)
                            .and_then(|cover| cover.layout.as_deref())
                        else {
                            continue;
                        };
                        (structure_debris_body(), debris_colliders(layout, &parts))
                    }
                    _ => {
                        let Some((body, collider)) = debris_parts(fragment) else {
                            continue;
                        };
                        (body, vec![collider])
                    }
                };
                let body = self.world.insert_body(colliders.0);
                self.world.bodies[body].user_data = Owner::Debris(fragment.id).user_data();
                // As the host steps it: its own pieces with two more solver iterations.
                if fragment.structure_parts.is_none() {
                    self.world.bodies[body].set_additional_solver_iterations(2);
                }
                for collider in colliders.1 {
                    self.world.insert_collider(collider, Some(body));
                }
                self.debris.insert(
                    fragment.id,
                    DebrisBody {
                        body,
                        seen: sync,
                        rested: false,
                        structure: fragment.structure_parts.is_some(),
                    },
                );
            }
            let debris = self.debris.get_mut(&fragment.id).expect("inserted above");
            debris.seen = sync;
            // A piece comes to rest, or a blast throws it out of its rest again.
            if fragment.rested != debris.rested {
                debris.rested = fragment.rested;
                if fragment.rested {
                    rest_body(&mut self.world, debris.body);
                } else {
                    unrest_body(&mut self.world, debris.body);
                }
            }
            let body = &mut self.world.bodies[debris.body];
            if fragment.structure_parts.is_some() && !fragment.rested {
                // Damped as the host damps it this tick.
                let age = state.elapsed - fragment.created_at.unwrap_or(state.elapsed);
                let (linear, angular) = fragment.damping.unwrap_or_default().values(age);
                body.set_linear_damping(linear);
                body.set_angular_damping(angular);
            }
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

    fn insert_cover(
        &mut self,
        cover: &RenderCover,
        form: CoverForm,
        layout: Option<&Structure>,
    ) -> RigidBodyHandle {
        // Movable cover keeps its authored shape; its replicated footprint follows the nav.
        let (x, z, w, d) = match cover.motion {
            Some(motion) => (motion.origin_x, motion.origin_z, motion.w, motion.d),
            None => (cover.x, cover.z, cover.w, cover.d),
        };
        let parts = match (form, layout) {
            (CoverForm::Parts(broken), Some(layout)) => structure_parts(layout, x, z, &broken),
            _ => cover_parts(cover.kind, x, z, w, d, cover.h),
        };
        let body = self.world.insert_body(parts.body);
        match form {
            CoverForm::Parts(_) => {
                for collider in parts.colliders {
                    self.world.insert_collider(collider, Some(body));
                }
            }
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
            self.world.bodies[body].user_data = Owner::Tank(tank.id).user_data();
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
        // As the host does ahead of its step.
        for debris in self.debris.values().filter(|debris| debris.structure && !debris.rested) {
            cap_loose_speed(&mut self.world.bodies[debris.body]);
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
                match Owner::of(self.world.bodies[body].user_data) {
                    Some(Owner::Tank(id)) => touched(Some(Moved::Tank(id))),
                    Some(Owner::Cover(id)) => {
                        let movable = self.covers.get(&id).is_some_and(|cover| cover.movable);
                        touched(movable.then_some(Moved::Cover(id)));
                    }
                    Some(Owner::Debris(id)) => touched(Some(Moved::Fragment(id))),
                    None => {}
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
        self.cast_cover_past(origin, direction, distance, &[])
    }

    fn cast_cover_past(
        &self,
        origin: Point3,
        direction: Vec2,
        distance: f64,
        past: &[u64],
    ) -> Option<CoverHit> {
        let ray = Ray::new(to_vector(origin), vector(direction.x, 0.0, direction.z));
        // An intact crate or timber wall stands as one collider; a part of it the shell
        // broke through is named with its collider. Past one, the structure is queried by
        // the shapes of the parts still standing, as the host splits it and re-queries.
        let breached = past.iter().filter_map(|&key| whole_part(key));
        let through = |collider: ColliderHandle, _: &Collider| {
            let key = collider_key(collider);
            !past.contains(&key) && !breached.clone().any(|(whole, _)| whole == key)
        };
        let mut filter = query_filter(group::COVER_QUERY);
        if !past.is_empty() {
            filter.predicate = Some(&through);
        }
        let mut nearest = self
            .world
            .cast_ray_and_get_normal(&ray, distance as f32, true, filter)
            .map(|(collider, hit)| (collider, hit, None));
        // Each breached structure once, in the order of its collider's key.
        let mut last = None;
        while let Some(whole) = breached
            .clone()
            .map(|(whole, _)| whole)
            .filter(|&whole| last.is_none_or(|last| whole > last))
            .min()
        {
            last = Some(whole);
            let Some(collider) = collider_of_key(whole).filter(|&collider| {
                self.world.colliders.get(collider).is_some()
            }) else {
                continue;
            };
            let Some(cover) = self.cover_of(Some(collider)) else {
                continue;
            };
            let Some(layout) = cover.layout.as_deref() else {
                continue;
            };
            let gone = |index: usize| breached.clone().any(|part| part == (whole, index));
            let body = *self.world.bodies[cover.body].position();
            let shapes = cover.part_shapes.get_or_init(|| part_shapes(layout));
            for (index, (_, shape)) in shapes.iter().enumerate().filter(|&(index, _)| !gone(index)) {
                // A built collider has no parent yet: its shape was placed at the body.
                let pose = body * Pose::IDENTITY;
                let reach = nearest.as_ref().map_or(distance as f32, |(_, hit, _)| hit.time_of_impact);
                if let Some(hit) = shape.cast_ray_and_get_normal(&pose, &ray, reach, true) {
                    nearest = Some((collider, hit, Some(index)));
                }
            }
        }
        let (collider, hit, breached_part) = nearest?;
        let cover = self.cover_of(Some(collider));
        let part = cover.and_then(|cover| {
            let user = self.world.colliders[collider].user_data;
            let index = match (breached_part, user) {
                (Some(index), _) => index,
                // An intact structure's one collider: the part where the ray meets it.
                (None, WHOLE) => {
                    let at = ray.point_at(hit.time_of_impact);
                    let middle = self.world.bodies[cover.body].translation();
                    let local = glam::DVec3::new(
                        f64::from(at.x - middle.x),
                        f64::from(at.y),
                        f64::from(at.z - middle.z),
                    );
                    let layout = cover.layout.as_ref()?;
                    layout.part_at(layout.to_local(local), &PartSet::EMPTY)?
                }
                (None, index) => index as usize,
            };
            let cottage = cover.struck.kind == CoverKind::House;
            // A watchtower's footing takes a shell as other concrete does.
            let strength = Some(cover.layout.as_ref()?.parts.get(index)?.strength)
                .filter(|strength| strength.is_finite())?;
            let key = if user == WHOLE {
                whole_part_key(collider, index)
            } else {
                collider_key(collider)
            };
            Some(PartHit {
                cottage,
                key,
                strength,
                cracked: cover.cracked.contains(index),
            })
        });
        Some(CoverHit {
            distance: f64::from(hit.time_of_impact),
            normal: Vec2::new(f64::from(hit.normal.x), f64::from(hit.normal.z)),
            struck: cover.map(|cover| cover.struck),
            part,
        })
    }
}

impl TankPredictor {
    /// The replicated cover a collider of the predicted world belongs to.
    fn cover_of(&self, collider: Option<ColliderHandle>) -> Option<&CoverBody> {
        let body = self.world.colliders.get(collider?)?.parent()?;
        match Owner::of(self.world.bodies[body].user_data)? {
            Owner::Cover(id) => self.covers.get(&id),
            _ => None,
        }
    }
}

/// Every part's standing shape of a structure, and its pose on the structure's body.
fn part_shapes(layout: &Structure) -> Box<[(Pose, SharedShape)]> {
    (0..layout.parts.len())
        .map(|index| {
            let collider = part_collider(layout, index, -layout.debris.seam);
            (collider.position, collider.shape)
        })
        .collect()
}

/// The collider a key names ([`collider_key`]).
fn collider_of_key(key: u64) -> Option<ColliderHandle> {
    Some(ColliderHandle::from_raw_parts(
        u32::try_from(key & u64::from(u32::MAX)).ok()?,
        u32::try_from(key >> 32).ok()?,
    ))
}

/// Marks a key naming a part of an intact structure's one collider.
const WHOLE_PART_BIT: u64 = 1 << 63;
/// The bits of such a key that name the part.
const WHOLE_PART_BITS: u32 = 12;

/// The key of part `index` of the intact structure standing as `collider`.
fn whole_part_key(collider: ColliderHandle, index: usize) -> u64 {
    WHOLE_PART_BIT | (collider_key(collider) << WHOLE_PART_BITS) & !WHOLE_PART_BIT | index as u64
}

/// The whole collider's key and the part a key names, if it names one.
fn whole_part(key: u64) -> Option<(u64, usize)> {
    (key & WHOLE_PART_BIT != 0).then_some((
        (key & !WHOLE_PART_BIT) >> WHOLE_PART_BITS,
        (key & ((1 << WHOLE_PART_BITS) - 1)) as usize,
    ))
}

/// A collider's handle as one number.
fn collider_key(collider: ColliderHandle) -> u64 {
    let (index, generation) = collider.into_raw_parts();
    (u64::from(generation) << 32) | u64::from(index)
}

/// The host's body for a structure's fallen parts (`drop_fall`): its origin is the
/// structure's ground centre the parts fell from.
fn structure_debris_body() -> RigidBodyBuilder {
    RigidBodyBuilder::dynamic().can_sleep(true)
}

/// How far a fragment's solid parts may lie from its replicated position: a structure's
/// fallen parts hang off the ground centre of the structure they fell from.
pub fn debris_offset(fragment: &RenderFragment) -> f64 {
    if fragment.structure_parts.is_some() {
        STRUCTURE_DEBRIS_OFFSET
    } else {
        0.0
    }
}

/// How a cover record stands in the predicted world, or `None` when nothing is left of it.
fn cover_form(cover: &RenderCover) -> Option<CoverForm> {
    if structural(cover.kind) {
        Some(CoverForm::Parts(cover.broken))
    } else if cover.alive {
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
    let membership = group::PUSHABLE_DEBRIS;
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
/// pieces of broken scenery and the fallen parts of structures, which the
/// predicted world leaves out. Small shards only touch the ground and cover.
pub fn solid_debris(fragment: &RenderFragment) -> bool {
    fragment.wreck.is_some()
        || fragment.dimensions.is_some()
        || fragment.structure_parts.is_some()
}

/// The host's per-tick drive: the speed pickup runs down before the drive impulse.
fn drive(hull: &mut Hull, speed: &mut f64, body: &mut RigidBody, input: Vec2) {
    *speed = 0f64.max(*speed - STEP);
    hull.boosted = *speed > 0.0;
    drive_hull(hull, body, input.x, input.z, STEP);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::scene_codec::{MirrorScene, Scene};
    use crate::net::wire::WireReader;
    use crate::net::own_shots::CoverCast;
    use crate::sim::arena::CoverDef;
    use crate::sim::types::CoverKind;
    use crate::sim::{Simulation, VehicleCommand};

    fn mirrored(sim: &Simulation) -> RenderState {
        let mut bytes = Vec::new();
        Scene::capture(sim).write(&mut bytes);
        MirrorScene::read(&mut WireReader::new(&bytes))
            .unwrap()
            .render(sim.tanks[0].id)
            .unwrap()
    }

    fn turns(predictor: &TankPredictor, id: u32) -> bool {
        let body = &predictor.world.bodies[predictor.debris[&id].body];
        !body.locked_axes().contains(rapier3d::prelude::LockedAxes::ROTATION_LOCKED)
    }

    #[test]
    fn a_shell_through_a_board_of_an_intact_crate_meets_the_far_wall() {
        let mut sim = Simulation::with_seed(731.0);
        sim.start();
        sim.add_cover(&CoverDef::new(CoverKind::Cargo, 0.0, 40.0, 3.0, 3.0, 2.6, 80.0, 0xb47a49));
        let scene = mirrored(&sim);
        let mut predictor = TankPredictor::default();
        predictor.sync_scene(&scene);
        predictor.world.step();
        // A level shell along the crate's middle from the front (+z) toward the back.
        let origin = Point3::new(0.4, 1.1, 50.0);
        let direction = Vec2::new(0.0, -1.0);
        let first = predictor.cast_cover(origin, direction, 20.0).expect("the crate's front");
        let part = first.part.unwrap_or_else(|| panic!("a part of it: {} {:?}", first.distance, first.struck));
        assert!((first.distance - 8.5).abs() < 0.2, "the front at {}", first.distance);
        let next = predictor
            .cast_cover_past(origin, direction, 20.0, &[part.key])
            .expect("the shell through the board meets the crate's inside");
        assert!(
            next.distance > first.distance + 0.05 && next.distance < first.distance + 3.2,
            "past the board it meets {} m on, not beyond the crate",
            next.distance - first.distance
        );
        assert!(next.part.is_some_and(|hit| hit.key != part.key));
    }

    #[test]
    fn a_rested_piece_a_blast_throws_again_turns_in_the_predicted_world() {
        // A cottage's walls shot out at once; its pieces come to rest, then a blast
        // throws one out of its rest again.
        let mut sim = Simulation::with_seed(731.0);
        sim.start();
        let house = sim.add_cover(&CoverDef::house(0.0, 60.0, 7.0, 5.0, 4.9, 0xb87b4c));
        let layout = sim.covers[house].structure.as_ref().unwrap().layout.clone();
        let walls = PartSet::from_indices(
            (0..layout.parts.len()).filter(|&index| layout.parts[index].grounded),
        );
        sim.break_structure(house, walls, PartSet::EMPTY, None, None);
        for _ in 0..600 {
            sim.step(VehicleCommand::idle(), false);
        }
        let scene = mirrored(&sim);
        let piece = *scene
            .fragments
            .iter()
            .find(|fragment| fragment.structure_parts.is_some() && fragment.rested)
            .expect("a piece at rest");
        let mut predictor = TankPredictor::default();
        predictor.sync_scene(&scene);
        predictor.sync_bodies(&scene, &[], piece.position);
        assert!(!turns(&predictor, piece.id), "at rest it is held still");
        let mut thrown = scene.clone();
        for fragment in &mut thrown.fragments {
            if fragment.id == piece.id {
                fragment.rested = false;
            }
        }
        predictor.sync_scene(&thrown);
        predictor.sync_bodies(&thrown, &[], piece.position);
        assert!(turns(&predictor, piece.id), "thrown again, it turns as it flies");
        assert!(!predictor.debris[&piece.id].rested);
    }
}
