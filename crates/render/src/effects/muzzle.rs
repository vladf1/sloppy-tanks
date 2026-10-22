//! Muzzle flashes: when a gun fires, a white-hot tongue of flame and a glow
//! leap from the barrel's tip with side jets from the muzzle brake, burning
//! flecks spray ahead, a brief warm light falls on everything nearby, and
//! gunsmoke rolls forward and out from the muzzle and kicks dust off the ground
//! below. The shot event names the tank; its turret aim and visual muzzle place
//! the flash when the frame's poses are known. Bounded: the oldest flash slot
//! is reused.

use std::f64::consts::TAU;

use glam::Vec3;
use sloppy_core::sim::tank_dimensions::tank_visual_muzzle;
use sloppy_core::sim::{RenderState, SimEvent, SimEventType, Weapon};

use super::Flash;
use super::explosions::smoke_puff;
use super::ground::Ground;
use super::laser::HULL_OFFSET;
use super::pool::PoolBuffer;
use super::random::CosmeticRandom;
use super::sparks::{MUZZLE_SPARKS, Sparks, glow, tongue};
use crate::color::hex_to_linear;

pub const MAX_MUZZLE_FLASHES: usize = 32;
pub const PUFFS_PER_MUZZLE: usize = 6;
/// Shots waiting for the frame's poses; more in one frame are dropped.
const MAX_PENDING: usize = 64;
/// The flame lasts a few frames; the smoke longer.
const FLAME_SECONDS: f64 = 0.08;
const SMOKE_SECONDS: f64 = 1.2;
const GUNSMOKE: u32 = 0xd2cdc4;
/// The muzzle light's peak intensity (times the shot size) and reach.
pub const MUZZLE_LIGHT: f32 = 28.0;
pub const MUZZLE_LIGHT_DISTANCE: f32 = 10.0;
/// The light dies this fast (1/s).
const LIGHT_DECAY: f64 = 32.0;
const FLAME_COLOR: [f32; 3] = [5.0, 2.7, 0.9];

#[derive(Clone, Copy, Debug)]
struct Pending {
    id: Option<u32>,
    x: f64,
    z: f64,
    size: f64,
}

#[derive(Clone, Copy, Debug, Default)]
struct Muzzle {
    active: bool,
    origin: Vec3,
    /// Unit planar direction the barrel points; zero when unknown.
    direction: Vec3,
    age: f64,
    scale: f32,
    seed: f32,
    dust: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct MuzzleFlashes {
    flashes: [Muzzle; MAX_MUZZLE_FLASHES],
    cursor: usize,
    pending: Vec<Pending>,
    /// The latest flash's light, decaying.
    pub light: Flash,
}

impl Default for MuzzleFlashes {
    fn default() -> Self {
        Self {
            flashes: [Muzzle::default(); MAX_MUZZLE_FLASHES],
            cursor: 0,
            pending: Vec::with_capacity(MAX_PENDING),
            light: Flash::default(),
        }
    }
}

impl MuzzleFlashes {
    pub fn reset(&mut self) {
        for flash in &mut self.flashes {
            flash.active = false;
        }
        self.cursor = 0;
        self.pending.clear();
        self.light = Flash::default();
    }

    pub fn active(&self) -> usize {
        self.flashes.iter().filter(|flash| flash.active).count()
    }

    pub fn event(&mut self, event: &SimEvent) {
        if event.kind != SimEventType::Shot || self.pending.len() >= MAX_PENDING {
            return;
        }
        let size = match event.weapon {
            Some(Weapon::Spread) => 1.15,
            Some(Weapon::Piercing) => 1.1,
            _ => event.size.unwrap_or(1.0),
        };
        self.pending.push(Pending {
            id: event.id,
            x: event.x,
            z: event.z,
            size,
        });
    }

    /// Place this frame's new flashes, age the rest and write their flames into
    /// `streaks` and their smoke into `puffs`.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        state: &RenderState,
        alpha: f64,
        dt: f64,
        ground: Ground,
        sparks: &mut Sparks,
        puffs: &mut PoolBuffer,
        random: &mut CosmeticRandom,
    ) {
        self.light.intensity *= (-dt * LIGHT_DECAY).exp() as f32;
        for index in 0..self.pending.len() {
            let shot = self.pending[index];
            let tank = shot
                .id
                .and_then(|id| state.tanks.iter().find(|tank| tank.id == id));
            let (origin, direction) = match tank {
                Some(tank) => {
                    let (sin, cos) = tank.aim.sin_cos();
                    let muzzle = tank_visual_muzzle(tank.kind);
                    let x = tank.previous.x + (tank.position.x - tank.previous.x) * alpha;
                    let z = tank.previous.z + (tank.position.z - tank.previous.z) * alpha;
                    (
                        Vec3::new(
                            (x + sin * muzzle.z) as f32,
                            (tank.position.y - HULL_OFFSET + muzzle.y) as f32,
                            (z + cos * muzzle.z) as f32,
                        ),
                        Vec3::new(sin as f32, 0.0, cos as f32),
                    )
                }
                None => (Vec3::new(shot.x as f32, 1.0, shot.z as f32), Vec3::ZERO),
            };
            let dust = hex_to_linear(ground.soil(f64::from(origin.x), f64::from(origin.z)).dust);
            let flash = &mut self.flashes[self.cursor];
            self.cursor = (self.cursor + 1) % MAX_MUZZLE_FLASHES;
            *flash = Muzzle {
                active: true,
                origin,
                direction,
                age: 0.0,
                scale: shot.size as f32,
                seed: random.next_f64() as f32,
                dust,
            };
            sparks.burst(origin, direction, 0.3, &MUZZLE_SPARKS, random);
            self.light = Flash {
                position: origin + direction * 0.6 + Vec3::Y * 0.4,
                intensity: MUZZLE_LIGHT * shot.size as f32,
            };
        }
        self.pending.clear();
        for flash in &mut self.flashes {
            if !flash.active {
                continue;
            }
            flash.age += dt.max(0.0);
            if flash.age >= SMOKE_SECONDS {
                flash.active = false;
                continue;
            }
            draw(flash, sparks, puffs);
        }
    }
}

fn settle(age: f32, rate: f32) -> f32 {
    1.0 - (-age * rate).exp()
}

fn draw(flash: &Muzzle, sparks: &mut Sparks, puffs: &mut PoolBuffer) {
    let t = flash.age as f32;
    let s = flash.scale;
    let forward = flash.direction;
    let side = Vec3::new(forward.z, 0.0, -forward.x);
    let base = flash.origin;
    if f64::from(t) < FLAME_SECONDS {
        let k = t / FLAME_SECONDS as f32;
        let fade = (1.0 - k) * (1.0 - k);
        let [r, g, b] = FLAME_COLOR;
        let streaks = &mut sparks.records;
        if forward != Vec3::ZERO {
            tongue(
                streaks,
                base - forward * 0.1,
                forward,
                0.95 * s,
                (1.7 + 0.9 * k) * s,
                [r, g, b, fade],
            );
            // The muzzle brake throws two short jets sideways.
            for sign in [-1.0, 1.0] {
                tongue(
                    streaks,
                    base - forward * 0.12,
                    (side * sign + forward * 0.35).normalize(),
                    0.42 * s,
                    0.75 * s,
                    [r, g * 0.9, b, fade * 0.85],
                );
            }
        }
        glow(
            streaks,
            base + forward * 0.25,
            (2.3 + 1.0 * k) * s,
            [5.0, 3.4, 1.6, fade * 0.9],
        );
    }
    let smoke = hex_to_linear(GUNSMOKE);
    let fade = 1.0 - smoothstep(t, 0.15, SMOKE_SECONDS as f32);
    let erosion = smoothstep(t, 0.1, SMOKE_SECONDS as f32) * 0.9;
    let heat = 0.9 * (-t * 22.0).exp();
    for j in 0..PUFFS_PER_MUZZLE {
        let jf = j as f32;
        let seed = flash.seed + jf * 0.137;
        let wobble = (seed * TAU as f32).sin() * 0.15;
        let (position, size, color, opacity, softness) = match j {
            // Rolling forward out of the barrel, each a little farther and bigger.
            0..=2 => (
                base + forward * ((0.35 + jf * 0.55) * settle(t, 6.5 - jf) * s)
                    + side * wobble
                    + Vec3::Y * (t * 0.35),
                (0.4 + 0.15 * jf + 0.85 * t.sqrt()) * s,
                smoke,
                0.6 - jf * 0.08,
                0.3,
            ),
            // Blown sideways out of the muzzle brake.
            3 | 4 => {
                let sign = if j == 3 { -1.0 } else { 1.0 };
                (
                    base + side * (sign * (0.25 + 0.75 * settle(t, 7.0)) * s) - forward * 0.1
                        + Vec3::Y * (t * 0.3),
                    (0.3 + 0.7 * t.sqrt()) * s,
                    smoke,
                    0.45,
                    0.35,
                )
            }
            // The blast kicks the ground's dust up under the muzzle.
            _ => (
                Vec3::new(base.x, 0.35, base.z) + forward * (0.6 + 1.1 * settle(t, 5.0)) * s,
                (0.6 + 1.0 * t.sqrt()) * s,
                flash.dust,
                0.32,
                0.8,
            ),
        };
        smoke_puff(
            puffs,
            position,
            size,
            color,
            (t / 0.02).min(1.0) * opacity * fade,
            seed,
            if j < 3 { heat } else { heat * 0.5 },
            erosion,
            softness,
        );
    }
}

fn smoothstep(x: f32, edge0: f32, edge1: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use sloppy_core::sim::Point3;
    use sloppy_core::sim::render_state::RenderTank;

    #[test]
    fn a_shot_flashes_at_its_tanks_muzzle_and_its_smoke_clears() {
        let mut state = RenderState::default();
        state.tanks.push(RenderTank {
            id: 4,
            alive: true,
            aim: std::f64::consts::FRAC_PI_2,
            position: Point3::new(10.0, 0.65, 0.0),
            previous: sloppy_core::sim::Vec2::new(10.0, 0.0),
            ..RenderTank::default()
        });
        let mut shot = SimEvent::at(SimEventType::Shot, 12.0, 0.0);
        shot.id = Some(4);
        let mut muzzles = MuzzleFlashes::default();
        let mut sparks = Sparks::default();
        let mut puffs = PoolBuffer::new(64);
        let mut random = CosmeticRandom::seeded(1);
        muzzles.event(&shot);
        sparks.update(0.0);
        muzzles.update(
            &state,
            1.0,
            1.0 / 60.0,
            Ground::Village,
            &mut sparks,
            &mut puffs,
            &mut random,
        );
        assert_eq!(muzzles.active(), 1);
        assert!(muzzles.light.intensity > 0.0);
        assert!(
            muzzles.light.position.x > 10.0,
            "the flash is ahead of the hull"
        );
        assert!(!sparks.is_empty());
        // The burst's sparks draw from the next update; these are the flames.
        let flames = sparks.records.len();
        assert_eq!(flames, 4, "a tongue, two brake jets and a glow");
        assert_eq!(puffs.len(), PUFFS_PER_MUZZLE);
        for _ in 0..90 {
            puffs.clear();
            sparks.update(1.0 / 60.0);
            muzzles.update(
                &state,
                1.0,
                1.0 / 60.0,
                Ground::Village,
                &mut sparks,
                &mut puffs,
                &mut random,
            );
        }
        assert_eq!(muzzles.active(), 0);
        assert!(puffs.is_empty());
        assert!(muzzles.light.intensity < 0.01);
    }
}
