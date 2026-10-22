//! Supply drop smoke (`presentation.ts` `pickupEffect`, reworked as military
//! supply): a column of signal smoke in the kind's colour rising from the smoke
//! grenade beside every stocked pallet, dense and saturated at the grenade and
//! thinning as it climbs before the wind shears it, so its colour reads from across
//! the field; and on collection (or promotion) a short pop of that smoke where the
//! supply bursts open over the tank. Everything is smoke in the shared puff pool,
//! drawn like the blasts' puffs: no light, beams or rings.

use std::collections::VecDeque;
use std::f64::consts::TAU;

use glam::Vec3;
use sloppy_core::models::{SMOKE_MARKER, pickup_paint};
use sloppy_core::sim::data::pickup;
use sloppy_core::sim::{PickupKind, RenderState};

use super::explosions::{WIND, smoke_puff};
use super::pool::PoolBuffer;
use crate::color::hex_to_linear;

/// `FEEDBACK.maxPickupEffects` (view-settings.ts), and how long a collection's
/// smoke lasts: until the supply bursts, then the pop.
pub const MAX_PICKUP_EFFECTS: usize = 24;
pub const PICKUP_SECONDS: f64 = SUPPLY_BURST_SECONDS + POP_SECONDS;

/// A collected supply's flight (presentation throws the item, the effects pop its
/// smoke and spill its pieces): it springs out of its pallet at the launch height,
/// clear of the hull, climbs to its apex, and bursts open there this long after the
/// collection.
pub const SUPPLY_LAUNCH_HEIGHT: f64 = 2.6;
pub const SUPPLY_APEX_HEIGHT: f64 = 3.7;
pub const SUPPLY_BURST_SECONDS: f64 = 0.3;

/// What a burst supply throws (`WoodChips::spill_supply`): pieces of its surface,
/// its paint and steel, and whether it was packed in excelsior (the wooden crates).
pub fn supply_spill(kind: PickupKind) -> ([u32; 3], bool) {
    const STEEL: u32 = 0x4a4e52;
    const PINE: u32 = 0xa87e50;
    const OLIVE: u32 = 0x5c643e;
    let (surface, packing) = match kind {
        PickupKind::Spread | PickupKind::Piercing => (PINE, true),
        PickupKind::Rocket | PickupKind::Rapid | PickupKind::Speed | PickupKind::Repair => {
            (OLIVE, false)
        }
        PickupKind::Ricochet => (0x3a3028, false),
        PickupKind::Shield => (STEEL, false),
        PickupKind::Laser => (0x9c8a64, false),
    };
    ([surface, pickup_paint(kind), STEEL], packing)
}

/// A supply's turn on its pallet (rad): fixed per pickup and within
/// `SUPPLY_TURN` of square, so drops do not stand in ranks yet never swing into a
/// neighbour; shared by the pallet, the item and its smoke. Cosmetic only.
pub fn supply_turn(id: u32) -> f64 {
    (hash(f64::from(id) + 0.5) - 0.5) * 2.0 * SUPPLY_TURN
}

const SUPPLY_TURN: f64 = 0.35;

/// The signal smoke: puffs per column, seconds each takes to climb it, how high it
/// climbs and how the climb is shaped (above one, puffs bunch at the grenade so the
/// column stays unbroken), the puff size at the grenade and at the top, how far the
/// wind shears the top of the column, its opacity at the grenade, and how far it
/// pales toward white at the top.
const SIGNAL_PUFFS: usize = 14;
const SIGNAL_CYCLE: f64 = 5.2;
const SIGNAL_RISE: f64 = 4.4;
const SIGNAL_CLIMB: f64 = 1.25;
const SIGNAL_SIZE: [f64; 2] = [0.16, 0.72];
const SIGNAL_SHEAR: f64 = 1.1;
const SIGNAL_OPACITY: f64 = 0.78;
const SIGNAL_PALE: f32 = 0.28;
/// The collection pop: a burst of the kind's smoke where the supply opens, puffs
/// rolling out (count, start and end reach, rise, start and end size, opacity) round
/// a core, gone in `POP_SECONDS`.
const POP_SECONDS: f64 = 0.7;
const POP_PUFFS: usize = 7;
const POP_REACH: [f64; 2] = [0.2, 1.25];
const POP_RISE: f64 = 0.55;
const POP_SIZE: [f64; 2] = [0.32, 0.95];
const POP_OPACITY: f64 = 0.9;

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

    /// Pop smoke in `color` over a collected pallet (or a promoted tank) once its
    /// supply bursts.
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

    /// Write the signal smoke over every stocked pallet and the collection pops
    /// into `puffs`, after the blasts have rebuilt it this frame.
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
        for effect in &self.effects {
            pop(effect, puffs);
        }
    }
}

/// One pallet's column: puffs climbing from its smoke grenade, densest and most
/// saturated there, swelling, paling and thinning as they rise, the wind shearing
/// the top of the column downwind. Every column leans the one way.
fn signal(id: u32, x: f64, z: f64, color: u32, time: f64, puffs: &mut PoolBuffer) {
    let (sin, cos) = supply_turn(id).sin_cos();
    let [mx, top, mz] = SMOKE_MARKER;
    let base = Vec3::new(
        (x + mx * cos + mz * sin) as f32,
        (top + 0.04) as f32,
        (z - mx * sin + mz * cos) as f32,
    );
    let tint = hex_to_linear(color);
    let seed = hash(f64::from(id));
    for j in 0..SIGNAL_PUFFS {
        let cycle = time / SIGNAL_CYCLE + j as f64 / SIGNAL_PUFFS as f64 + seed;
        let a = cycle.fract();
        let lap = cycle.floor();
        let height = SIGNAL_RISE * a.powf(SIGNAL_CLIMB);
        let shear = SIGNAL_SHEAR * a.powf(2.2);
        // A slow sideways sway, the same for every puff of a lap, grows with height.
        let sway = (lap * 2.3 + j as f64 * 0.7 + seed * 7.0).sin() * 0.16 * a;
        let position = base
            + Vec3::new(
                (WIND[0] * shear + sway * WIND[1]) as f32,
                height as f32,
                (WIND[1] * shear - sway * WIND[0]) as f32,
            );
        let size = SIGNAL_SIZE[0] + (SIGNAL_SIZE[1] - SIGNAL_SIZE[0]) * a.powf(0.8);
        let opacity = SIGNAL_OPACITY * smoothstep(a, 0.0, 0.035) * (1.0 - a).powf(1.3);
        smoke_puff(
            puffs,
            position,
            size as f32,
            pale(tint, SIGNAL_PALE * a as f32),
            opacity as f32,
            (seed * 3.1 + lap * 0.37 + j as f64 * 0.19) as f32,
            0.0,
            (smoothstep(a, 0.45, 1.0) * 0.6) as f32,
            (0.18 + 0.4 * a) as f32,
            0.0,
        );
    }
}

/// A collection's pop: once the supply bursts, the kind's smoke rolls out from
/// where it opened, round a core that rises a little, and clears.
fn pop(effect: &PickupEffect, puffs: &mut PoolBuffer) {
    let age = effect.age - SUPPLY_BURST_SECONDS;
    if age < 0.0 {
        return;
    }
    let progress = age / POP_SECONDS;
    let spread = 1.0 - (1.0 - progress).powi(3);
    let turn = hash(effect.x * 0.731 + effect.z * 0.377) * TAU;
    let tint = pale(effect.color, 0.12);
    let opacity = POP_OPACITY * smoothstep(age, 0.0, 0.03) * (1.0 - progress).powf(1.6);
    let center = Vec3::new(
        effect.x as f32,
        (SUPPLY_APEX_HEIGHT + POP_RISE * spread) as f32,
        effect.z as f32,
    );
    for j in 0..POP_PUFFS {
        let (reach, size) = if j == 0 {
            (0.0, POP_SIZE[1] * 1.15)
        } else {
            (
                POP_REACH[0] + (POP_REACH[1] - POP_REACH[0]) * spread,
                POP_SIZE[1],
            )
        };
        let bearing = turn + TAU * j as f64 / (POP_PUFFS - 1) as f64;
        let lift = 0.25 * (j as f64 * 1.7).sin() * spread;
        let position = center
            + Vec3::new(
                (bearing.cos() * reach) as f32,
                lift as f32,
                (bearing.sin() * reach) as f32,
            );
        let size = POP_SIZE[0] + (size - POP_SIZE[0]) * spread;
        smoke_puff(
            puffs,
            position,
            size as f32,
            tint,
            opacity as f32,
            (turn + j as f64 * 0.41) as f32,
            0.0,
            (progress * 0.8) as f32,
            (0.2 + 0.35 * progress) as f32,
            0.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::sim::types::Pickup;

    #[test]
    fn stocked_pallets_smoke_and_collections_pop_then_clear() {
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
            "a column over the pallet"
        );
        state.pickups[0].available = false;
        puffs.clear();
        effects.draw(&state, &mut puffs);
        assert!(puffs.is_empty(), "no smoke from an empty pallet");
        for i in 0..30 {
            effects.add(f64::from(i), 0.0, 0xffb02a);
        }
        assert_eq!(effects.len(), MAX_PICKUP_EFFECTS);
        effects.update(SUPPLY_BURST_SECONDS * 0.5);
        effects.draw(&state, &mut puffs);
        assert!(puffs.is_empty(), "no pop before the supply bursts");
        effects.update(SUPPLY_BURST_SECONDS * 0.5 + 0.1);
        effects.draw(&state, &mut puffs);
        assert_eq!(puffs.len(), MAX_PICKUP_EFFECTS * POP_PUFFS);
        assert!(
            puffs.records()[0].translation().y > SUPPLY_APEX_HEIGHT as f32,
            "the pop hangs where the supply burst"
        );
        effects.update(PICKUP_SECONDS);
        assert!(effects.is_empty());
    }
}
