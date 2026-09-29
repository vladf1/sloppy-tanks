//! Tiny cosmetic chips thrown by quarry tracks (`track-gravel.ts`): one bounded
//! draw, no collision bodies or scene queries.

use glam::Vec3;

use super::pool::{PoolBuffer, pose, record};
use super::random::CosmeticRandom;

pub const TRACK_GRAVEL_CAPACITY: usize = 192;
const GRAVITY: f64 = 12.0;
const REST_HEIGHT: f64 = 0.055;
/// Pebbles shrink away over their last moments.
const SHRINK_SECONDS: f64 = 0.14;

#[derive(Clone, Copy, Debug, Default)]
struct Pebble {
    x: f64,
    y: f64,
    z: f64,
    vx: f64,
    vy: f64,
    vz: f64,
    life: f64,
    size: f64,
    spin: f64,
    bounce: bool,
}

#[derive(Clone, Debug)]
pub struct TrackGravel {
    live: Vec<Pebble>,
    pub records: PoolBuffer,
}

impl Default for TrackGravel {
    fn default() -> Self {
        Self {
            live: Vec::with_capacity(TRACK_GRAVEL_CAPACITY),
            records: PoolBuffer::new(TRACK_GRAVEL_CAPACITY),
        }
    }
}

impl TrackGravel {
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn emit(
        &mut self,
        x: f64,
        z: f64,
        vx: f64,
        vz: f64,
        strength: f64,
        random: &mut CosmeticRandom,
    ) {
        for _ in 0..if strength > 0.45 { 2 } else { 1 } {
            if self.live.len() >= TRACK_GRAVEL_CAPACITY {
                return;
            }
            // Draw order: vx, vz, vy, life, size, spin.
            let vx = vx + (random.next() - 0.5) * 0.8;
            let vz = vz + (random.next() - 0.5) * 0.8;
            let vy = 1.6 + random.next() * 1.4 + strength * 0.6;
            let life = 0.5 + random.next() * 0.22;
            let size = 0.055 + random.next() * 0.05;
            let spin = random.next() * std::f64::consts::TAU;
            self.live.push(Pebble {
                x,
                y: 0.15,
                z,
                vx,
                vy,
                vz,
                life,
                size,
                spin,
                bounce: false,
            });
        }
    }

    pub fn reset(&mut self) {
        self.live.clear();
        self.records.clear();
    }

    pub fn update(&mut self, elapsed: f64) {
        let dt = elapsed.min(0.1);
        self.records.clear();
        let records = &mut self.records;
        self.live.retain_mut(|p| {
            p.life -= elapsed;
            if p.life <= 0.0 {
                return false;
            }
            p.x += p.vx * dt;
            p.z += p.vz * dt;
            p.y += p.vy * dt;
            p.vy -= GRAVITY * dt;
            p.spin += dt * 12.0;
            if p.y < REST_HEIGHT {
                p.y = REST_HEIGHT;
                p.vy = if p.bounce { 0.0 } else { p.vy.abs() * 0.25 };
                p.vx *= 0.6;
                p.vz *= 0.6;
                p.bounce = true;
            }
            let size = (p.size * (p.life / SHRINK_SECONDS).min(1.0)) as f32;
            let world = pose(
                Vec3::new(p.x as f32, p.y as f32, p.z as f32),
                Vec3::new(p.spin as f32, (p.spin * 0.7) as f32, 0.0),
                Vec3::splat(size),
            );
            records.push(record(world, [1.0; 4], [0.0; 4]));
            true
        });
    }
}
