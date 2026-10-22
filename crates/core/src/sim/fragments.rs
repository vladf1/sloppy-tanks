//! Bounded cosmetic physics debris. Quantized sizes reuse model geometry.

use rapier3d::prelude::{ColliderBuilder, RigidBodyBuilder};

use super::data::group;
use super::debris_cleanup::DEBRIS_CLEANUP_SECONDS;
use super::math::js_round;
use super::physics::{interaction_groups, vector};
use super::simulation::Simulation;
use super::types::{Fragment, FragmentShape};

/// A fragment's throw: horizontal speed either way and upward speed (m/s); a
/// shard of armour clears the hull and lands a few metres off, not across the map.
const THROW_SPEED: f64 = 4.5;
const THROW_LIFT: [f64; 2] = [3.0, 7.5];
/// Seconds a fragment lies where it landed before it sinks away (times the
/// caller's scale), so a kill's remains outlast the smoke.
const FRAGMENT_REST: [f64; 2] = [5.0, 8.0];

impl Simulation {
    /// A small cosmetic physics fragment.
    pub fn fragment(
        &mut self,
        x: f64,
        z: f64,
        color: u32,
        size: f64,
        shape: FragmentShape,
        lifetime_scale: f64,
    ) {
        let size = js_round(size * 5.0) / 5.0;
        self.reserve_fragments(1);
        // Draw order is part of the seeded stream: height, velocity xyz, spin xyz, then life.
        let rng = &mut self.rng;
        let y = rng.range(1.0, 3.0);
        let linvel = vector(
            rng.range(-THROW_SPEED, THROW_SPEED),
            rng.range(THROW_LIFT[0], THROW_LIFT[1]),
            rng.range(-THROW_SPEED, THROW_SPEED),
        );
        let angvel = vector(
            rng.range(-6.0, 6.0),
            rng.range(-6.0, 6.0),
            rng.range(-6.0, 6.0),
        );
        let body = self.world.insert_body(
            RigidBodyBuilder::dynamic()
                .translation(vector(x, y, z))
                .linvel(linvel)
                .angvel(angvel),
        );
        let flat = matches!(
            shape,
            FragmentShape::Armor | FragmentShape::Track | FragmentShape::Wood
        );
        let wood = shape == FragmentShape::Wood;
        self.world.insert_collider(
            ColliderBuilder::cuboid(
                (size / 2.0) as f32,
                (size * if flat { 0.12 } else { 0.4 }) as f32,
                (size / 2.0) as f32,
            )
            .collision_groups(interaction_groups(group::FRAGMENT))
            .friction(if wood { 0.65 } else { 0.9 })
            .restitution(if wood { 0.16 } else { 0.12 })
            .mass(0.1),
            Some(body),
        );
        let id = self.allocate_id();
        let life = self.rng.range(FRAGMENT_REST[0], FRAGMENT_REST[1]) * lifetime_scale
            + DEBRIS_CLEANUP_SECONDS;
        let mut fragment = Fragment::new(id, body, life, size, color);
        fragment.shape = Some(shape);
        self.fragments.push(fragment);
    }
}
