//! The smoke trail of rockets and TOW missiles: soft puffs laid evenly along the
//! flight path behind the nozzle. A puff starts small and lit by the motor, then
//! grows, rises and drifts as grey smoke and fades; the trail outlives the missile
//! for a moment. One bounded draw; when it is full the oldest puff makes room.

use std::collections::HashMap;

use glam::{Mat4, Vec3};
use sloppy_core::sim::Weapon;
use sloppy_core::sim::data::STEP;
use sloppy_core::sim::render_state::RenderShot;

use super::pool::{PoolBuffer, record};
use super::projectiles::model_scale;
use super::random::CosmeticRandom;
use crate::color::hex_to_linear;

pub const ROCKET_SMOKE_CAPACITY: usize = 720;
/// Metres of flight between puffs, by munition (the TOW's motor smokes less).
const ROCKET_SPACING: f64 = 0.22;
const TOW_SPACING: f64 = 0.3;
/// The nozzle sits this far behind the shot's centre, in model units (×scale).
const NOZZLE: f64 = 0.55;
/// Puff life in seconds, [minimum, random span].
const LIFE: [f64; 2] = [0.8, 0.7];
/// Diameter at birth and at the end of its life (m).
const START_SIZE: f64 = 0.3;
const END_SIZE: f64 = 1.15;
/// Upward drift (m/s) and random sideways drift.
const RISE: f64 = 0.35;
const DRIFT: f64 = 0.4;
const OPACITY: f64 = 0.32;
/// The share of a puff's life it glows with the motor's flame.
const GLOW: f64 = 0.07;
const FLAME_COLOR: u32 = 0xffb35a;
const SMOKE_COLOR: u32 = 0xc4c0b8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Puff {
    pub position: Vec3,
    velocity: Vec3,
    pub age: f64,
    life: f64,
    /// Shade of grey and growth, so a trail is neither one flat colour nor a tube.
    shade: f32,
    grow: f32,
}

#[derive(Clone, Copy, Debug)]
struct Trail {
    position: Vec3,
    seen: bool,
}

#[derive(Clone, Debug)]
pub struct RocketSmoke {
    pub puffs: Vec<Puff>,
    /// Where each missile in flight last laid a puff, by shot id.
    trails: HashMap<u32, Trail>,
    pub records: PoolBuffer,
}

impl Default for RocketSmoke {
    fn default() -> Self {
        Self {
            puffs: Vec::with_capacity(ROCKET_SMOKE_CAPACITY),
            trails: HashMap::new(),
            records: PoolBuffer::new(ROCKET_SMOKE_CAPACITY),
        }
    }
}

impl RocketSmoke {
    pub fn reset(&mut self) {
        self.puffs.clear();
        self.trails.clear();
        self.records.clear();
    }

    pub fn len(&self) -> usize {
        self.puffs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.puffs.is_empty()
    }

    /// Lay puffs behind every missile in flight (placed like
    /// `ProjectileVisuals::update`, `alpha` between physics poses), age the trail
    /// by `dt` and write the records.
    pub fn update(
        &mut self,
        shots: &[RenderShot],
        alpha: f64,
        dt: f64,
        random: &mut CosmeticRandom,
    ) {
        for puff in &mut self.puffs {
            puff.age += dt;
            puff.position += puff.velocity * dt as f32;
            // The puff's push from the exhaust dies away; its rise stays.
            puff.velocity.x *= (-2.0 * dt).exp() as f32;
            puff.velocity.z *= (-2.0 * dt).exp() as f32;
        }
        self.puffs.retain(|puff| puff.age < puff.life);
        let behind = (1.0 - alpha.clamp(0.0, 1.0)) * STEP;
        for trail in self.trails.values_mut() {
            trail.seen = false;
        }
        for shot in shots {
            let spacing = match shot.weapon {
                Weapon::Rocket => ROCKET_SPACING,
                Weapon::Tow => TOW_SPACING,
                _ => continue,
            };
            let speed = shot.vx.hypot(shot.vz);
            if speed <= 0.0 {
                continue;
            }
            let back = Vec3::new((-shot.vx / speed) as f32, 0.0, (-shot.vz / speed) as f32);
            let nozzle = Vec3::new(
                (shot.x - shot.vx * behind) as f32,
                shot.visual_y.or(shot.y).unwrap_or(1.0) as f32,
                (shot.z - shot.vz * behind) as f32,
            ) + back * (NOZZLE * model_scale(shot.weapon)) as f32;
            let mut last = self
                .trails
                .get(&shot.id)
                .map(|trail| trail.position)
                .unwrap_or(nozzle - back * spacing as f32);
            // Puffs every `spacing` metres from the last one up to the nozzle.
            let mut gap = last.distance(nozzle);
            while gap >= spacing as f32 {
                last = last.move_towards(nozzle, spacing as f32);
                gap -= spacing as f32;
                self.lay(last, back, random);
            }
            self.trails.insert(
                shot.id,
                Trail {
                    position: last,
                    seen: true,
                },
            );
        }
        // Keep the allocation and surviving trails; spent or stopped missiles
        // must not leave history behind for the next frame.
        self.trails.retain(|_, trail| trail.seen);
        self.records.clear();
        let flame = hex_to_linear(FLAME_COLOR);
        let smoke = hex_to_linear(SMOKE_COLOR);
        for puff in &self.puffs {
            let t = (puff.age / puff.life) as f32;
            let size =
                (START_SIZE + (END_SIZE - START_SIZE) * f64::from(t.sqrt())) as f32 * puff.grow;
            let glow = (1.0 - t / GLOW as f32).max(0.0);
            let color = [0, 1, 2]
                .map(|i| smoke[i] * puff.shade + (flame[i] - smoke[i] * puff.shade) * glow);
            // Fade in over the first moment, out over the rest.
            let opacity = OPACITY as f32 * (t / 0.04).min(1.0) * (1.0 - t).powi(2);
            let world = Mat4::from_scale_rotation_translation(
                Vec3::splat(size),
                glam::Quat::IDENTITY,
                puff.position,
            );
            let [r, g, b] = color;
            self.records
                .push(record(world, [r, g, b, opacity], [0.0; 4]));
        }
    }

    fn lay(&mut self, position: Vec3, back: Vec3, random: &mut CosmeticRandom) {
        if self.puffs.len() == ROCKET_SMOKE_CAPACITY {
            self.puffs.remove(0);
        }
        let mut next = || random.next_f64() as f32;
        let drift = Vec3::new(next() - 0.5, 0.0, next() - 0.5) * (DRIFT * 2.0) as f32;
        self.puffs.push(Puff {
            position,
            velocity: back * 0.5 + drift + Vec3::Y * RISE as f32,
            age: 0.0,
            life: LIFE[0] + f64::from(next()) * LIFE[1],
            shade: 0.8 + next() * 0.25,
            grow: 0.7 + next() * 0.6,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::sim::Team;

    fn missile(id: u32, weapon: Weapon, x: f64) -> RenderShot {
        RenderShot {
            id,
            weapon,
            team: Team::Blue,
            x,
            z: 0.0,
            y: Some(1.0),
            visual_y: None,
            vx: 20.0,
            vz: 0.0,
        }
    }

    #[test]
    fn missiles_lay_an_even_trail_that_fades_after_they_are_gone() {
        let mut random = CosmeticRandom::seeded(5);
        let mut smoke = RocketSmoke::default();
        // A shell lays nothing; a rocket lays one puff per spacing of flight.
        smoke.update(&[missile(1, Weapon::Standard, 0.0)], 1.0, STEP, &mut random);
        assert!(smoke.is_empty());
        let mut x = 0.0;
        for _ in 0..30 {
            x += 20.0 * STEP;
            smoke.update(&[missile(2, Weapon::Rocket, x)], 1.0, STEP, &mut random);
        }
        let flown = 20.0 * STEP * 29.0;
        let expected = (flown / ROCKET_SPACING) as usize;
        assert!(smoke.len().abs_diff(expected) <= 1, "{} puffs", smoke.len());
        assert_eq!(smoke.records.len(), smoke.len());
        // Every puff lies behind the rocket's nozzle.
        let nozzle = x as f32 - (NOZZLE * model_scale(Weapon::Rocket)) as f32;
        assert!(
            smoke
                .puffs
                .iter()
                .all(|puff| puff.position.x <= nozzle + 1e-3)
        );
        // Gone with the rocket, the trail fades within its puffs' longest life.
        for _ in 0..((LIFE[0] + LIFE[1]) / STEP) as usize + 1 {
            smoke.update(&[], 1.0, STEP, &mut random);
        }
        assert!(smoke.is_empty() && smoke.records.is_empty());
    }

    #[test]
    fn trail_history_reuses_storage_and_drops_spent_or_stopped_missiles() {
        let mut random = CosmeticRandom::seeded(5);
        let mut smoke = RocketSmoke::default();
        let volley = [
            missile(1, Weapon::Rocket, 0.0),
            missile(2, Weapon::Tow, 0.0),
        ];
        smoke.update(&volley, 1.0, STEP, &mut random);
        let capacity = smoke.trails.capacity();
        let continuing = smoke.trails[&1].position;

        let stopped = RenderShot {
            vx: 0.0,
            ..missile(2, Weapon::Tow, 1.0)
        };
        smoke.update(&[volley[0], stopped], 1.0, STEP, &mut random);
        assert_eq!(smoke.trails.len(), 1);
        assert_eq!(smoke.trails[&1].position, continuing);
        assert_eq!(smoke.trails.capacity(), capacity);

        smoke.update(&[], 1.0, STEP, &mut random);
        assert!(smoke.trails.is_empty());
        assert_eq!(
            smoke.trails.capacity(),
            capacity,
            "reuse storage between volleys"
        );
        assert!(!smoke.puffs.is_empty(), "spent missiles leave fading smoke");
        smoke.reset();
        assert!(smoke.trails.is_empty() && smoke.puffs.is_empty() && smoke.records.is_empty());
    }

    #[test]
    fn the_pool_stays_bounded() {
        let mut random = CosmeticRandom::seeded(9);
        let mut smoke = RocketSmoke::default();
        let mut x = 0.0;
        for _ in 0..400 {
            x += 20.0 * STEP;
            let volley: Vec<_> = (0..8).map(|id| missile(id, Weapon::Rocket, x)).collect();
            smoke.update(&volley, 1.0, STEP, &mut random);
        }
        assert_eq!(smoke.len(), ROCKET_SMOKE_CAPACITY);
        assert_eq!(smoke.records.len(), ROCKET_SMOKE_CAPACITY);
    }
}
