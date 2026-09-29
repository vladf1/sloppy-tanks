//! The Rapier boundary. Game math is f64 like the previous JS numbers; Rapier works in f32,
//! so values narrow exactly where the old code crossed into the physics engine.

use std::sync::Mutex;

use rapier3d::prelude::*;

use super::math::{Point3, Quat4};

/// Packed 32-bit collision groups (memberships high, filter low), tested with AND like the
/// previous engine.
pub fn interaction_groups(bits: u32) -> InteractionGroups {
    InteractionGroups::new(
        Group::from_bits_retain(bits >> 16),
        Group::from_bits_retain(bits & 0xffff),
        InteractionTestMode::And,
    )
}

/// A scene-query filter accepting colliders compatible with the packed `groups`.
pub fn query_filter(groups: u32) -> QueryFilter<'static> {
    QueryFilter::default().groups(interaction_groups(groups))
}

pub fn vector(x: f64, y: f64, z: f64) -> Vector {
    Vector::new(x as f32, y as f32, z as f32)
}

pub fn to_vector(point: Point3) -> Vector {
    vector(point.x, point.y, point.z)
}

pub fn from_vector(value: Vector) -> Point3 {
    Point3::new(value.x as f64, value.y as f64, value.z as f64)
}

pub fn from_rotation(value: Rotation) -> Quat4 {
    Quat4 {
        x: value.x as f64,
        y: value.y as f64,
        z: value.z as f64,
        w: value.w as f64,
    }
}

/// Rapier normalizes a quaternion given from outside, as the JS bindings did.
pub fn to_rotation(value: Quat4) -> Rotation {
    Rotation::from_xyzw(value.x as f32, value.y as f32, value.z as f32, value.w as f32).normalize()
}

/// A convex hull from xyz triples.
pub fn convex_hull(points: &[f32]) -> ColliderBuilder {
    let points: Vec<Vector> = points.chunks_exact(3).map(|p| Vector::new(p[0], p[1], p[2])).collect();
    ColliderBuilder::convex_hull(&points).expect("authored hull points are not degenerate")
}

/// Contact-force events collected during a world step, drained in report order.
#[derive(Default)]
pub struct ContactForces {
    events: Mutex<Vec<ContactForce>>,
}

#[derive(Clone, Copy, Debug)]
pub struct ContactForce {
    pub collider1: ColliderHandle,
    pub collider2: ColliderHandle,
    pub total_force_magnitude: f32,
}

impl ContactForces {
    pub fn drain(&self) -> Vec<ContactForce> {
        std::mem::take(&mut *self.events.lock().unwrap_or_else(|poison| poison.into_inner()))
    }

    pub fn clear(&self) {
        self.events.lock().unwrap_or_else(|poison| poison.into_inner()).clear();
    }
}

impl EventHandler for ContactForces {
    fn handle_collision_event(
        &self,
        _bodies: &RigidBodySet,
        _colliders: &ColliderSet,
        _event: CollisionEvent,
        _contact_pair: Option<&ContactPair>,
    ) {
    }

    fn handle_contact_force_event(
        &self,
        _dt: Real,
        _bodies: &RigidBodySet,
        _colliders: &ColliderSet,
        contact_pair: &ContactPair,
        total_force_magnitude: Real,
    ) {
        self.events.lock().unwrap_or_else(|poison| poison.into_inner()).push(ContactForce {
            collider1: contact_pair.collider1,
            collider2: contact_pair.collider2,
            total_force_magnitude,
        });
    }

    fn handle_soft_body_tear_event(&self, _soft_bodies: &SoftBodySet, _event: &SoftBodyTearEvent) {}
}
