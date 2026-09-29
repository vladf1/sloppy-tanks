//! Sparse windblown wisps along the quarry apron, plus rare faint sheets high
//! over the combat floor (`quarry-dust.ts`): one bounded draw, frozen while the
//! match is not playing. Cosmetic randomness only.

use std::f64::consts::{PI, TAU};

use glam::{Mat4, Vec3};
use sloppy_core::sim::{MatchPhase, RenderState};

use super::pool::{PoolBuffer, record};
use super::random::CosmeticRandom;
use crate::color::hex_to_linear;

pub const QUARRY_DUST_CAPACITY: usize = 48;
pub const QUARRY_DUST_MAX_OPACITY: f64 = 0.1;
const COLOR: u32 = 0xe3cfa5;
const MAX_STEP: f64 = 0.1;
/// One spawn in five drifts high over the combat floor instead.
const HIGH_SHEET_CHANCE: f64 = 0.2;

#[derive(Clone, Copy, Debug, Default)]
struct Wisp {
    x: f64,
    y: f64,
    z: f64,
    vx: f64,
    life: f64,
    max: f64,
    size: f64,
    phase: f64,
    alpha: f64,
}

#[derive(Clone, Debug)]
pub struct QuarryDust {
    pub records: PoolBuffer,
    wisps: Vec<Wisp>,
    timer: f64,
    /// Shown only on the quarry (the former `mesh.visible`).
    pub visible: bool,
}

impl Default for QuarryDust {
    fn default() -> Self {
        Self {
            records: PoolBuffer::new(QUARRY_DUST_CAPACITY),
            wisps: Vec::with_capacity(QUARRY_DUST_CAPACITY),
            timer: 0.0,
            visible: false,
        }
    }
}

impl QuarryDust {
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn reset(&mut self) {
        self.wisps.clear();
        self.timer = 0.0;
        self.records.clear();
        self.visible = false;
    }

    fn spawn(&mut self, random: &mut CosmeticRandom) {
        if self.wisps.len() >= QUARRY_DUST_CAPACITY {
            return;
        }
        // Larger, fainter sheets stay well above the tanks so readability never suffers.
        if random.next() < HIGH_SHEET_CHANCE {
            let x = -50.0 + random.next() * 100.0;
            let z = -50.0 + random.next() * 100.0;
            let y = 2.4 + random.next() * 2.4;
            let vx = 1.5 + random.next() * 1.5;
            let life = 6.0 + random.next() * 3.0;
            let size = 5.0 + random.next() * 3.5;
            let phase = random.next() * TAU;
            self.wisps.push(Wisp {
                x,
                y,
                z,
                vx,
                life,
                max: life,
                size,
                phase,
                alpha: 0.45,
            });
            return;
        }
        let side = if random.next() < 0.5 { -1.0 } else { 1.0 };
        let z = side * (63.0 + random.next() * 8.0);
        // Rest on the dipped apron outside the wall, the grade the terrain bakes.
        let ground = -((z.abs() - 60.0) * 0.3).min(1.8);
        let x = -70.0 + random.next() * 140.0;
        let y = ground + 0.5 + random.next() * 0.9;
        let vx = 1.2 + random.next() * 1.2;
        let life = 5.0 + random.next() * 3.0;
        let size = 2.5 + random.next() * 2.0;
        let phase = random.next() * TAU;
        self.wisps.push(Wisp {
            x,
            y,
            z,
            vx,
            life,
            max: life,
            size,
            phase,
            alpha: 1.0,
        });
    }

    pub fn update(&mut self, state: &RenderState, dt: f64, random: &mut CosmeticRandom) {
        if state.map_theme != "quarry" {
            if !self.records.is_empty() || self.visible {
                self.reset();
            }
            return;
        }
        self.visible = true;
        // Frozen while paused or between rounds, like the track dust pool.
        if state.match_state.phase != MatchPhase::Playing {
            return;
        }
        let step = dt.min(MAX_STEP);
        self.timer -= step;
        if self.timer <= 0.0 {
            self.timer = 0.35 + random.next() * 0.6;
            self.spawn(random);
        }
        let elapsed = state.elapsed;
        self.wisps.retain_mut(|wisp| {
            wisp.life -= step;
            if wisp.life <= 0.0 {
                return false;
            }
            wisp.x += wisp.vx * step;
            wisp.z += (elapsed * 0.6 + wisp.phase).sin() * 0.5 * step;
            true
        });
        self.records.clear();
        let [r, g, b] = hex_to_linear(COLOR);
        for wisp in &self.wisps {
            let age = 1.0 - wisp.life / wisp.max;
            let size = (wisp.size * (0.8 + age * 1.2)) as f32;
            let world =
                Mat4::from_translation(Vec3::new(wisp.x as f32, wisp.y as f32, wisp.z as f32))
                    * Mat4::from_scale(Vec3::new(size, size * 0.55, 1.0));
            let opacity = (age * PI).sin() * QUARRY_DUST_MAX_OPACITY * wisp.alpha;
            self.records
                .push(record(world, [r, g, b, opacity as f32], [0.0; 4]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quarry(phase: MatchPhase) -> RenderState {
        let mut state = RenderState::default();
        state.map_theme = "quarry".into();
        state.match_state.phase = phase;
        state
    }

    #[test]
    fn wisps_drift_stay_bounded_faint_and_freeze_when_paused() {
        let mut random = CosmeticRandom::default();
        let mut dust = QuarryDust::default();
        let mut state = quarry(MatchPhase::Playing);
        for frame in 0..60 * 120 {
            state.elapsed = frame as f64 / 60.0;
            dust.update(&state, 1.0 / 60.0, &mut random);
            assert!(dust.len() <= QUARRY_DUST_CAPACITY);
        }
        assert!(dust.visible && dust.len() > 5);
        for r in dust.records.records() {
            assert!(r.tint[3] >= 0.0 && f64::from(r.tint[3]) <= QUARRY_DUST_MAX_OPACITY);
        }
        let frozen = dust.records.records().to_vec();
        state.match_state.phase = MatchPhase::Paused;
        for _ in 0..600 {
            dust.update(&state, 1.0 / 60.0, &mut random);
        }
        assert_eq!(dust.records.records(), &frozen[..]);
        let mut village = state.clone();
        village.map_theme = "village".into();
        dust.update(&village, 1.0 / 60.0, &mut random);
        assert!(!dust.visible && dust.is_empty());
    }
}
