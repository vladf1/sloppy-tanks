//! Supply drop smoke (`presentation.ts` `pickupEffect`, reworked as military
//! supply): a wisp of signal smoke in the kind's colour rising from the smoke
//! grenade beside every stocked pallet, so its colour reads from across the field, and
//! on collection (or promotion) a burst of that smoke and a kick of dust off the
//! pallet. Everything is smoke in the shared puff pool, drawn like the blasts'
//! puffs: no light, beams or rings.

use std::collections::VecDeque;
use std::f64::consts::TAU;

use glam::Vec3;
use sloppy_core::models::SMOKE_MARKER;
use sloppy_core::sim::RenderState;
use sloppy_core::sim::data::pickup;

use super::explosions::{WIND, smoke_puff};
use super::pool::PoolBuffer;
use crate::color::hex_to_linear;

/// `FEEDBACK.maxPickupEffects` and `FEEDBACK.pickupSeconds` (view-settings.ts).
pub const MAX_PICKUP_EFFECTS: usize = 24;
pub const PICKUP_SECONDS: f64 = 1.4;

/// A supply's turn on its pallet (rad): fixed per pickup and within
/// `SUPPLY_TURN` of square, so drops do not stand in ranks yet never swing into a
/// neighbour; shared by the pallet, the item and its smoke. Cosmetic only.
pub fn supply_turn(id: u32) -> f64 {
    (hash(f64::from(id) + 0.5) - 0.5) * 2.0 * SUPPLY_TURN
}

const SUPPLY_TURN: f64 = 0.35;

/// The signal smoke: puffs per stream, seconds each takes to rise and thin out,
/// how high it climbs, its size at the grenade and at the top, how far the wind
/// leans it per second squared, its opacity, and how much it pales toward white.
const SIGNAL_PUFFS: usize = 7;
const SIGNAL_CYCLE: f64 = 4.6;
const SIGNAL_RISE: f64 = 3.2;
const SIGNAL_SIZE: [f64; 2] = [0.16, 0.78];
const SIGNAL_LEAN: f64 = 0.14;
const SIGNAL_OPACITY: f64 = 0.5;
const SIGNAL_PALE: f32 = 0.3;
/// The collection burst: a ring of the kind's smoke rolling out from the pallet
/// (count, start and end radius, rise, start and end size, opacity) with dust
/// kicked out low ahead of it. It rolls clear of the collecting tank, whose sight
/// line the puff pool keeps thin (`explosions::clear_sight_line`).
const BURST_PUFFS: usize = 16;
const BURST_RADIUS: [f64; 2] = [1.4, 5.0];
const BURST_RISE: [f64; 2] = [0.45, 1.1];
const BURST_SIZE: [f64; 2] = [0.7, 1.6];
const BURST_OPACITY: f64 = 0.85;
const DUST_PUFFS: usize = 8;
const DUST_RADIUS: [f64; 2] = [1.8, 5.4];
const DUST_SIZE: [f64; 2] = [0.5, 1.5];
const DUST_OPACITY: f64 = 0.5;
const DUST: u32 = 0x9c8f7c;

/// A repeatable 0..1 value, so puffs differ without drawing cosmetic randomness.
fn hash(value: f64) -> f64 {
    ((value * 12.9898 + 78.233).sin() * 43_758.545_3)
        .fract()
        .abs()
}

fn smoothstep(x: f64, edge0: f64, edge1: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn pale(color: [f32; 3], amount: f32) -> [f32; 3] {
    color.map(|c| c + (1.0 - c) * amount)
}

#[derive(Clone, Copy, Debug)]
struct PickupEffect {
    x: f64,
    z: f64,
    age: f64,
    color: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct PickupEffects {
    effects: VecDeque<PickupEffect>,
}

impl Default for PickupEffects {
    fn default() -> Self {
        Self {
            effects: VecDeque::with_capacity(MAX_PICKUP_EFFECTS),
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
    }

    /// Burst smoke off a pallet (or a promoted tank) in `color`.
    pub fn add(&mut self, x: f64, z: f64, color: u32) {
        if self.effects.len() >= MAX_PICKUP_EFFECTS {
            self.effects.pop_front();
        }
        self.effects.push_back(PickupEffect {
            x,
            z,
            age: 0.0,
            color: hex_to_linear(color),
        });
    }

    pub fn update(&mut self, dt: f64) {
        self.effects.retain_mut(|effect| {
            effect.age += dt;
            effect.age < PICKUP_SECONDS
        });
    }

    /// Write the signal smoke over every stocked pallet and the collection
    /// bursts into `puffs`, after the blasts have rebuilt it this frame.
    pub fn draw(&self, state: &RenderState, puffs: &mut PoolBuffer) {
        for supply in state.pickups.iter().filter(|supply| supply.available) {
            signal(
                supply.id,
                supply.x,
                supply.z,
                pickup(supply.kind).color,
                state.elapsed,
                puffs,
            );
        }
        for (index, effect) in self.effects.iter().enumerate() {
            burst(effect, index as f64, puffs);
        }
    }
}

/// One pallet's wisp: puffs cycling up from its smoke grenade, leaning downwind,
/// growing and paling as they go.
fn signal(id: u32, x: f64, z: f64, color: u32, time: f64, puffs: &mut PoolBuffer) {
    let (sin, cos) = supply_turn(id).sin_cos();
    let [mx, top, mz] = SMOKE_MARKER;
    let base = Vec3::new(
        (x + mx * cos + mz * sin) as f32,
        (top + 0.05) as f32,
        (z - mx * sin + mz * cos) as f32,
    );
    let tint = pale(hex_to_linear(color), SIGNAL_PALE);
    let seed = hash(f64::from(id));
    for j in 0..SIGNAL_PUFFS {
        let cycle = time / SIGNAL_CYCLE + j as f64 / SIGNAL_PUFFS as f64 + seed;
        let a = cycle.fract();
        let lap = cycle.floor();
        let lean = a * a * SIGNAL_CYCLE * SIGNAL_CYCLE * SIGNAL_LEAN;
        let swirl = (lap * 2.3 + j as f64 + seed * 7.0).sin() * 0.25 * a;
        let position = base
            + Vec3::new(
                (WIND[0] * lean + swirl) as f32,
                (SIGNAL_RISE * a.powf(0.8)) as f32,
                (WIND[1] * lean - swirl * 0.6) as f32,
            );
        let size = SIGNAL_SIZE[0] + (SIGNAL_SIZE[1] - SIGNAL_SIZE[0]) * a.powf(0.7);
        let opacity = SIGNAL_OPACITY * smoothstep(a, 0.0, 0.1) * (1.0 - smoothstep(a, 0.35, 1.0));
        smoke_puff(
            puffs,
            position,
            size as f32,
            pale(tint, (a * 0.3) as f32),
            opacity as f32,
            (seed * 3.1 + lap * 0.37 + j as f64 * 0.19) as f32,
            0.0,
            (smoothstep(a, 0.3, 1.0) * 0.7) as f32,
            (0.12 + 0.3 * a) as f32,
            0.0,
        );
    }
}

/// A collection's burst: the kind's smoke rolling off the pallet and up, and dust
/// kicked out low around it, both settling out over the effect.
fn burst(effect: &PickupEffect, index: f64, puffs: &mut PoolBuffer) {
    let progress = effect.age / PICKUP_SECONDS;
    let spread = 1.0 - (1.0 - progress).powi(3);
    let turn = hash(effect.x * 0.731 + effect.z * 0.377) * TAU;
    let tint = pale(effect.color, 0.2);
    for j in 0..BURST_PUFFS {
        let bearing = turn + TAU * j as f64 / BURST_PUFFS as f64;
        let reach = (BURST_RADIUS[0] + (BURST_RADIUS[1] - BURST_RADIUS[0]) * spread)
            * (0.85 + 0.15 * hash(j as f64 + index));
        let rise = BURST_RISE[0] + (BURST_RISE[1] - BURST_RISE[0]) * spread;
        let position = Vec3::new(
            (effect.x + bearing.cos() * reach) as f32,
            (rise * (0.75 + 0.25 * hash(j as f64 * 3.0))) as f32,
            (effect.z + bearing.sin() * reach) as f32,
        );
        let size = BURST_SIZE[0] + (BURST_SIZE[1] - BURST_SIZE[0]) * spread;
        let opacity =
            BURST_OPACITY * smoothstep(effect.age, 0.0, 0.06) * (1.0 - progress).powf(1.5);
        smoke_puff(
            puffs,
            position,
            size as f32,
            tint,
            opacity as f32,
            (turn + j as f64 * 0.41) as f32,
            0.0,
            (progress * 0.8) as f32,
            (0.15 + 0.3 * progress) as f32,
            0.0,
        );
    }
    let dust = hex_to_linear(DUST);
    for j in 0..DUST_PUFFS {
        let bearing = turn + TAU * (j as f64 + 0.5) / DUST_PUFFS as f64;
        let reach = DUST_RADIUS[0] + (DUST_RADIUS[1] - DUST_RADIUS[0]) * spread;
        let position = Vec3::new(
            (effect.x + bearing.cos() * reach) as f32,
            (0.25 + 0.3 * spread) as f32,
            (effect.z + bearing.sin() * reach) as f32,
        );
        let size = DUST_SIZE[0] + (DUST_SIZE[1] - DUST_SIZE[0]) * spread;
        let opacity = DUST_OPACITY * smoothstep(effect.age, 0.0, 0.05) * (1.0 - progress).powi(2);
        smoke_puff(
            puffs,
            position,
            size as f32,
            dust,
            opacity as f32,
            (turn * 1.7 + j as f64 * 0.23) as f32,
            0.0,
            (progress * 0.9) as f32,
            (0.2 + 0.3 * progress) as f32,
            0.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::sim::PickupKind;
    use sloppy_core::sim::types::Pickup;

    #[test]
    fn stocked_pallets_smoke_and_collections_burst_then_clear() {
        let mut state = RenderState::default();
        state.pickups.push(Pickup {
            id: 3,
            kind: PickupKind::Speed,
            x: 4.0,
            z: -2.0,
            available: true,
            cooldown: 0.0,
            cooldown_duration: 0.0,
        });
        let mut effects = PickupEffects::default();
        let mut puffs = PoolBuffer::new(1024);
        effects.draw(&state, &mut puffs);
        let signal = puffs.len();
        assert!(
            signal > 0 && signal <= SIGNAL_PUFFS,
            "a wisp over the pallet"
        );
        state.pickups[0].available = false;
        puffs.clear();
        effects.draw(&state, &mut puffs);
        assert!(puffs.is_empty(), "no smoke from an empty pallet");
        for i in 0..30 {
            effects.add(f64::from(i), 0.0, 0xffb02a);
        }
        assert_eq!(effects.len(), MAX_PICKUP_EFFECTS);
        effects.update(0.2);
        effects.draw(&state, &mut puffs);
        assert_eq!(puffs.len(), MAX_PICKUP_EFFECTS * (BURST_PUFFS + DUST_PUFFS));
        assert!(
            puffs.records()[0].translation().y > 0.0,
            "the burst lifts off the pallet"
        );
        effects.update(PICKUP_SECONDS);
        assert!(effects.is_empty());
    }
}
