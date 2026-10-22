//! Burning wrecks: after a tank's death blast, flames lick up out of the hull and
//! a thick black column of smoke leans away on the wind, thinning to a smoulder
//! until the tank respawns. Analytic streams of puffs cycling through the shared
//! puff pool, so a wreck holds no particle state; bounded wreck slots.

use std::f64::consts::PI;

use glam::Vec3;
use sloppy_core::geometry::math::smoothstep;
use sloppy_core::sim::{DeathStyle, RenderState, SimEvent, SimEventType};

use super::explosions::smoke_puff;
use super::pool::PoolBuffer;
use super::random::CosmeticRandom;
use crate::color::hex_to_linear;

pub const MAX_BURNING_WRECKS: usize = 12;
pub const PUFFS_PER_WRECK: usize = 9;
const SMOKE_PUFFS: usize = 6;
const FLAME_PUFFS: usize = 3;
/// Flames burn this long; smoke keeps rising for longer.
const BURN_SECONDS: f64 = 9.0;
const SMOULDER_SECONDS: f64 = 22.0;
/// A puff's trip up the column, and a flame's flicker (s).
const SMOKE_CYCLE: f64 = 2.6;
const FLAME_CYCLE: f64 = 0.6;
/// The smoke column's height and its lean on the wind (m over a puff's trip).
const COLUMN_HEIGHT: f64 = 6.0;
const WIND: [f64; 2] = [1.7, 0.9];
/// A respawned tank's wreck smoke dies away this fast (1/s).
const CLEAR_RATE: f64 = 1.5;
const SOOT: u32 = 0x343335;
const SOOT_HIGH: u32 = 0x6a6866;

#[derive(Clone, Copy, Debug, Default)]
struct Burner {
    active: bool,
    id: Option<u32>,
    x: f64,
    z: f64,
    age: f64,
    seed: f64,
    /// A burnout smokes without much fire.
    burnout: bool,
    /// 0 while the wreck stands; rises to 1 once the tank respawned.
    cleared: f64,
}

#[derive(Clone, Debug, Default)]
pub struct WreckFires {
    burners: [Burner; MAX_BURNING_WRECKS],
    cursor: usize,
}

impl WreckFires {
    pub fn reset(&mut self) {
        for burner in &mut self.burners {
            burner.active = false;
        }
        self.cursor = 0;
    }

    pub fn active(&self) -> usize {
        self.burners.iter().filter(|burner| burner.active).count()
    }

    pub fn event(&mut self, event: &SimEvent, random: &mut CosmeticRandom) {
        if event.kind != SimEventType::Death {
            return;
        }
        self.burners[self.cursor] = Burner {
            active: true,
            id: event.id,
            x: event.x,
            z: event.z,
            age: 0.0,
            seed: random.next_f64(),
            burnout: event.death_style == Some(DeathStyle::Burnout),
            cleared: 0.0,
        };
        self.cursor = (self.cursor + 1) % MAX_BURNING_WRECKS;
    }

    pub fn update(&mut self, state: &RenderState, dt: f64, puffs: &mut PoolBuffer) {
        let soot = hex_to_linear(SOOT);
        let soot_high = hex_to_linear(SOOT_HIGH);
        for burner in &mut self.burners {
            if !burner.active {
                continue;
            }
            burner.age += dt.max(0.0);
            let standing = burner
                .id
                .is_none_or(|id| state.tanks.iter().any(|tank| tank.id == id && !tank.alive));
            if !standing {
                burner.cleared = (burner.cleared + dt * CLEAR_RATE).min(1.0);
            }
            if burner.age >= SMOULDER_SECONDS || burner.cleared >= 1.0 {
                burner.active = false;
                continue;
            }
            let age = burner.age;
            let left = 1.0 - burner.cleared;
            let fire = smoothstep(age, 0.3, 1.0)
                * (1.0 - smoothstep(age, BURN_SECONDS * 0.55, BURN_SECONDS))
                * left
                * if burner.burnout { 0.45 } else { 1.0 };
            let smoke = smoothstep(age, 0.4, 1.4)
                * (1.0 - smoothstep(age, SMOULDER_SECONDS * 0.55, SMOULDER_SECONDS))
                * left
                * (0.55 + 0.45 * (1.0 - smoothstep(age, BURN_SECONDS * 0.6, BURN_SECONDS)));
            for j in 0..SMOKE_PUFFS {
                let cycle = age / SMOKE_CYCLE + j as f64 / SMOKE_PUFFS as f64 + burner.seed;
                let a = cycle.fract();
                let lap = cycle.floor();
                let swirl = (lap * 2.3 + j as f64 + burner.seed * 7.0).sin() * 0.35;
                let rise = a.powf(0.8);
                let position = Vec3::new(
                    (burner.x + WIND[0] * a * a + swirl) as f32,
                    (0.9 + COLUMN_HEIGHT * rise) as f32,
                    (burner.z + WIND[1] * a * a - swirl * 0.6) as f32,
                );
                let color = [0, 1, 2].map(|i| soot[i] + (soot_high[i] - soot[i]) * a as f32);
                smoke_puff(
                    puffs,
                    position,
                    (0.7 + 2.0 * a) as f32,
                    color,
                    (smoke * (PI * a).sin().powf(0.7) * 0.45) as f32,
                    (burner.seed * 3.1 + lap * 0.37 + j as f64 * 0.19) as f32,
                    (fire * 0.6 * (1.0 - a).powi(4)) as f32,
                    (a * a * 0.8) as f32,
                    0.15,
                );
            }
            if fire <= 0.0 {
                continue;
            }
            for j in 0..FLAME_PUFFS {
                let cycle = age / FLAME_CYCLE + j as f64 / FLAME_PUFFS as f64 + burner.seed;
                let a = cycle.fract();
                let lap = cycle.floor();
                let angle = burner.seed * 11.0 + j as f64 * 2.1 + lap * 0.9;
                let spot = 0.25 + 0.35 * ((lap + j as f64) * 1.7).sin().abs();
                smoke_puff(
                    puffs,
                    Vec3::new(
                        (burner.x + angle.cos() * spot) as f32,
                        (0.8 + 1.5 * a) as f32,
                        (burner.z + angle.sin() * spot) as f32,
                    ),
                    (1.0 - 0.45 * a) as f32,
                    soot,
                    (fire * (a / 0.1).min(1.0) * (1.0 - a) * 0.95) as f32,
                    (burner.seed * 5.3 + lap * 0.61 + j as f64 * 0.29) as f32,
                    (fire * 1.25 * (1.0 - a * 0.85)) as f32,
                    (a * 0.6) as f32,
                    0.0,
                );
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use sloppy_core::sim::render_state::RenderTank;

    #[test]
    fn wrecks_burn_until_their_tank_respawns_then_clear() {
        let mut state = RenderState::default();
        state.tanks.push(RenderTank {
            id: 7,
            alive: false,
            ..RenderTank::default()
        });
        let mut death = SimEvent::at(SimEventType::Death, 3.0, 4.0);
        death.id = Some(7);
        let mut fires = WreckFires::default();
        let mut random = CosmeticRandom::seeded(2);
        fires.event(&death, &mut random);
        let mut puffs = PoolBuffer::new(64);
        for _ in 0..120 {
            puffs.clear();
            fires.update(&state, 1.0 / 60.0, &mut puffs);
        }
        assert_eq!(fires.active(), 1);
        assert!(puffs.len() > SMOKE_PUFFS, "smoke and flames");
        assert!(puffs.len() <= PUFFS_PER_WRECK);
        state.tanks[0].alive = true;
        for _ in 0..60 {
            puffs.clear();
            fires.update(&state, 1.0 / 60.0, &mut puffs);
        }
        assert_eq!(fires.active(), 0);
        for _ in 0..40 {
            fires.event(&death, &mut random);
        }
        assert_eq!(fires.active(), MAX_BURNING_WRECKS);
        fires.reset();
        assert_eq!(fires.active(), 0);
    }
}
