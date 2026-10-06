//! Pickup and promotion feedback (`presentation.ts` `pickupEffect`): an additive
//! ring expanding over the ground and a glow shell around the collecting tank.

use std::collections::VecDeque;
use std::f32::consts::FRAC_PI_2;

use glam::{Mat4, Vec3};
use sloppy_core::sim::RenderState;
use sloppy_core::sim::data::vehicle;

use super::pool::{PoolBuffer, record};
use crate::color::hex_to_linear;

/// `FEEDBACK.maxPickupEffects` and `FEEDBACK.pickupSeconds` (view-settings.ts).
pub const MAX_PICKUP_EFFECTS: usize = 24;
pub const PICKUP_SECONDS: f64 = 0.8;
const RING_HEIGHT: f32 = 0.08;
const RING_GROWTH: f64 = 3.0;
const RING_OPACITY: f64 = 0.85;
const GLOW_OPACITY: f64 = 0.2;
const GLOW_HEIGHT: f32 = 1.1;
/// The glow ellipsoid around a tank of scale 1, and how much it swells.
const GLOW_SIZE: Vec3 = Vec3::new(1.65, 1.25, 1.9);
const GLOW_GROWTH: f64 = 0.15;

#[derive(Clone, Copy, Debug)]
struct PickupEffect {
    x: f64,
    z: f64,
    age: f64,
    tank: Option<u32>,
    color: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct PickupEffects {
    effects: VecDeque<PickupEffect>,
    pub rings: PoolBuffer,
    pub glows: PoolBuffer,
}

impl Default for PickupEffects {
    fn default() -> Self {
        Self {
            effects: VecDeque::with_capacity(MAX_PICKUP_EFFECTS),
            rings: PoolBuffer::new(MAX_PICKUP_EFFECTS),
            glows: PoolBuffer::new(MAX_PICKUP_EFFECTS),
        }
    }
}

impl PickupEffects {
    pub fn len(&self) -> usize {
        self.effects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    pub fn reset(&mut self) {
        self.effects.clear();
        self.rings.clear();
        self.glows.clear();
    }

    /// Start a ring at a pickup, following `tank` with a glow while it lives.
    pub fn add(&mut self, x: f64, z: f64, color: u32, tank: Option<u32>) {
        if self.effects.len() >= MAX_PICKUP_EFFECTS {
            self.effects.pop_front();
        }
        self.effects.push_back(PickupEffect {
            x,
            z,
            age: 0.0,
            tank,
            color: hex_to_linear(color),
        });
    }

    pub fn update(&mut self, state: &RenderState, alpha: f64, dt: f64) {
        self.rings.clear();
        self.glows.clear();
        self.effects.retain_mut(|effect| {
            effect.age += dt;
            effect.age < PICKUP_SECONDS
        });
        for effect in &self.effects {
            let progress = effect.age / PICKUP_SECONDS;
            let fade = (1.0 - progress).powi(2);
            let [r, g, b] = effect.color;
            let ring =
                Mat4::from_translation(Vec3::new(effect.x as f32, RING_HEIGHT, effect.z as f32))
                    * Mat4::from_rotation_x(-FRAC_PI_2)
                    * Mat4::from_scale(Vec3::splat((1.0 + progress * RING_GROWTH) as f32));
            self.rings.push(record(
                ring,
                [r, g, b, (RING_OPACITY * fade) as f32],
                [0.0; 4],
            ));
            let Some(tank) = effect
                .tank
                .and_then(|id| state.tanks.iter().find(|t| t.id == id && t.alive))
            else {
                continue;
            };
            let p = tank.position;
            let center = Vec3::new(
                (tank.previous.x + (p.x - tank.previous.x) * alpha) as f32,
                GLOW_HEIGHT,
                (tank.previous.z + (p.z - tank.previous.z) * alpha) as f32,
            );
            let size =
                GLOW_SIZE * (vehicle(tank.kind).scale * (1.0 + progress * GLOW_GROWTH)) as f32;
            self.glows.push(record(
                Mat4::from_translation(center) * Mat4::from_scale(size),
                [r, g, b, (GLOW_OPACITY * fade) as f32],
                [0.0; 4],
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::sim::render_state::RenderTank;

    #[test]
    fn rings_expand_fade_and_expire_and_the_pool_keeps_the_newest() {
        let mut state = RenderState::default();
        state.tanks.push(RenderTank {
            id: 7,
            alive: true,
            ..RenderTank::default()
        });
        let mut effects = PickupEffects::default();
        for i in 0..30 {
            effects.add(i as f64, 0.0, 0xffcf54, Some(7));
        }
        assert_eq!(effects.len(), MAX_PICKUP_EFFECTS);
        effects.update(&state, 1.0, 0.4);
        assert_eq!(effects.rings.len(), MAX_PICKUP_EFFECTS);
        assert_eq!(effects.glows.len(), MAX_PICKUP_EFFECTS);
        let first = &effects.rings.records()[0];
        assert_eq!(
            first.translation().x,
            6.0,
            "the oldest effects were replaced"
        );
        assert!((first.tint[3] - 0.85 * 0.25).abs() < 1e-6);
        state.tanks[0].alive = false;
        effects.update(&state, 1.0, 0.1);
        assert!(effects.glows.is_empty(), "no glow without a living tank");
        effects.update(&state, 1.0, 0.3);
        assert!(effects.is_empty() && effects.rings.is_empty());
    }
}
