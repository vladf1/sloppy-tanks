//! Bounded cosmetic physics debris. Quantized sizes reuse model geometry.

use rapier3d::prelude::{ColliderBuilder, RigidBodyBuilder};

use super::data::group;
use super::debris_cleanup::DEBRIS_CLEANUP_SECONDS;
use super::math::js_round;
use super::physics::{interaction_groups, vector};
use super::simulation::Simulation;
use super::types::{Fragment, FragmentShape};

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
            rng.range(-7.0, 7.0),
            rng.range(5.0, 14.0),
            rng.range(-7.0, 7.0),
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
        let life = self.rng.range(1.6, 2.6) * lifetime_scale + DEBRIS_CLEANUP_SECONDS - 0.5;
        let mut fragment = Fragment::new(id, body, life, size, color);
        fragment.shape = Some(shape);
        self.fragments.push(fragment);
    }
}
