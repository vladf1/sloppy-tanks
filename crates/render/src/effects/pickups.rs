//! Supply drop smoke (`presentation.ts` `pickupEffect`, reworked as military
//! supply): a column of signal smoke rising from the smoke grenade on the far side
//! of every stocked pallet, in one of the few colours real signal smoke comes in,
//! thin and dense at the grenade, swelling and paling as it climbs and drifting
//! away from the overhead camera so it marks the crate without hiding it. Once the
//! supply is taken the grenade fizzles out: the last smoke it gave keeps rising
//! and thins away. A promotion pops a short burst of its colour over the tank.
//! Everything is smoke in the shared puff pool, drawn like the blasts' puffs: no
//! light, beams or rings.

use std::collections::VecDeque;
use std::f64::consts::TAU;

use glam::Vec3;
use sloppy_core::models::SMOKE_MARKER;
use sloppy_core::sim::{PickupKind, RenderState};

use super::explosions::smoke_puff;
use super::pool::PoolBuffer;
use crate::color::hex_to_linear;

/// `FEEDBACK.maxPickupEffects` (view-settings.ts), and how long a promotion's
/// smoke pop lasts.
pub const MAX_PICKUP_EFFECTS: usize = 24;
pub const PICKUP_SECONDS: f64 = POP_SECONDS;

/// The signal smoke a kind's drop is marked with: red, orange, yellow, green,
/// violet or white, the colours smoke grenades come in. Kinds that share a colour
/// differ in silhouette (a shell rack and a rocket cradle, an ammo crate and a
/// white medical case, shell canisters and steel plate). Bright, so the plume stays
/// lit and keeps its hue in a warm desert sun.
pub fn signal_smoke(kind: PickupKind) -> u32 {
    match kind {
        PickupKind::Spread | PickupKind::Rocket => 0xf0442e,
        PickupKind::Rapid => 0xff8a1e,
        PickupKind::Speed => 0xffd83a,
        PickupKind::Piercing | PickupKind::Repair => 0x52d464,
        PickupKind::Ricochet | PickupKind::Shield => 0x8e5ce8,
        PickupKind::Laser => 0xf4f4f0,
    }
}

/// A supply's turn on its pallet (rad): fixed per pickup and within
/// `SUPPLY_TURN` of square, so drops do not stand in ranks yet never swing into a
/// neighbour and the grenade stays on the far side; shared by the pallet, the item
/// and its smoke. Cosmetic only.
pub fn supply_turn(id: u32) -> f64 {
    (hash(f64::from(id) + 0.5) - 0.5) * 2.0 * SUPPLY_TURN
}

const SUPPLY_TURN: f64 = 0.3;

/// The signal smoke: puffs per column, seconds each takes to climb it, how high it
/// climbs (each column within `SIGNAL_RISE_SPREAD` of it, so a row of drops does not
/// read as a fence) and how the climb is shaped (above one, puffs bunch at the
/// grenade so the column stays unbroken), the puff size at the grenade and at the top, how far it
/// has drifted by the top, its opacity at the grenade, and how far it pales toward
/// white at the grenade and at the top (pale enough at its base to stay lit in the
/// shade of a low sun).
const SIGNAL_PUFFS: usize = 18;
const SIGNAL_CYCLE: f64 = 4.0;
const SIGNAL_RISE: f64 = 3.2;
const SIGNAL_RISE_SPREAD: f64 = 0.2;
const SIGNAL_CLIMB: f64 = 1.2;
const SIGNAL_SIZE: [f64; 2] = [0.14, 0.8];
const SIGNAL_DRIFT: f64 = 1.5;
const SIGNAL_OPACITY: f64 = 1.0;
const SIGNAL_PALE: [f32; 2] = [0.0, 0.2];
/// Signal smoke is dense and bright: its albedo is lifted past the blasts' grey
/// smoke so a plume keeps its hue on its shaded side.
const SIGNAL_BRIGHTNESS: f32 = 1.25;
/// The drift: away from the overhead camera (which sits toward +z), a little
/// downwind, so a plume trails up the screen behind its own crate rather than
/// across its neighbours.
const SIGNAL_HEADING: [f64; 2] = [0.34, -0.94];
/// The grenade gushes: one rate for every column (rad/s of emission time, so a
/// puff keeps its share as it climbs), each column at its own phase, and how much
/// density and size the lulls lose.
const SIGNAL_GUSH_RATE: f64 = 2.3;
const SIGNAL_GUSH: f64 = 0.3;
/// After the supply is taken, the grenade gives out over this long: its last puffs
/// are thinner and smaller, then nothing leaves it.
const FIZZLE_SECONDS: f64 = 1.1;
/// A promotion's pop: a burst of its colour over the tank, puffs rolling out
/// (height, count, start and end reach, rise, start and end size, opacity) round a
/// core, gone in `POP_SECONDS`.
const POP_HEIGHT: f64 = 3.7;
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

    /// Pop smoke in `color` over a promoted tank.
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

    /// Write the signal smoke over every stocked pallet (and the last of it over a
    /// pallet just emptied) and the promotion pops into `puffs`, after the blasts
    /// have rebuilt it this frame.
    pub fn draw(&self, state: &RenderState, puffs: &mut PoolBuffer) {
        for supply in &state.pickups {
            // When the supply was taken: never while stocked, and long enough ago
            // for a drop whose cooldown has no known start.
            let taken = if supply.available {
                f64::INFINITY
            } else if supply.cooldown_duration > 0.0 {
                state.elapsed - (supply.cooldown_duration - supply.cooldown).max(0.0)
            } else {
                continue;
            };
            if state.elapsed - taken > SIGNAL_CYCLE {
                continue;
            }
            signal(
                supply.id,
                supply.x,
                supply.z,
                signal_smoke(supply.kind),
                state.elapsed,
                taken,
                puffs,
            );
        }
        for effect in &self.effects {
            pop(effect, puffs);
        }
    }
}

/// One pallet's column: puffs climbing from its smoke grenade, densest and most
/// saturated there, swelling, paling and thinning as they rise and drifting away
/// from the camera. A puff keeps the gush it left the grenade with; none leave it
/// once the supply is `taken` and its fizzle is over.
fn signal(id: u32, x: f64, z: f64, color: u32, time: f64, taken: f64, puffs: &mut PoolBuffer) {
    let (sin, cos) = supply_turn(id).sin_cos();
    let [mx, top, mz] = SMOKE_MARKER;
    let base = Vec3::new(
        (x + mx * cos + mz * sin) as f32,
        (top + 0.03) as f32,
        (z - mx * sin + mz * cos) as f32,
    );
    let tint = hex_to_linear(color);
    let seed = hash(f64::from(id));
    let rise = SIGNAL_RISE * (1.0 + SIGNAL_RISE_SPREAD * (2.0 * hash(f64::from(id) + 7.3) - 1.0));
    let [hx, hz] = SIGNAL_HEADING;
    for j in 0..SIGNAL_PUFFS {
        let cycle = time / SIGNAL_CYCLE + j as f64 / SIGNAL_PUFFS as f64 + seed;
        let a = cycle.fract();
        let lap = cycle.floor();
        // When this puff left the grenade; constant over its climb.
        let emitted = time - a * SIGNAL_CYCLE;
        let fizzle = 1.0 - ((emitted - taken) / FIZZLE_SECONDS).max(0.0);
        if fizzle <= 0.0 {
            continue;
        }
        let gush =
            1.0 - SIGNAL_GUSH * 0.5 * (1.0 + (emitted * SIGNAL_GUSH_RATE + seed * TAU).sin());
        let strength = gush * fizzle.powf(0.7);
        let height = rise * a.powf(SIGNAL_CLIMB);
        let drift = SIGNAL_DRIFT * a.powf(1.5);
        // A slow sideways sway, the same for every puff of a lap, grows with height.
        let sway = (lap * 2.3 + j as f64 * 0.7 + seed * 7.0).sin() * 0.14 * a;
        let position = base
            + Vec3::new(
                (hx * drift - hz * sway) as f32,
                height as f32,
                (hz * drift + hx * sway) as f32,
            );
        let size = (SIGNAL_SIZE[0] + (SIGNAL_SIZE[1] - SIGNAL_SIZE[0]) * a.powf(0.85))
            * (0.8 + 0.2 * strength);
        let opacity = SIGNAL_OPACITY * strength * smoothstep(a, 0.0, 0.03) * (1.0 - a).powf(1.2);
        smoke_puff(
            puffs,
            position,
            size as f32,
            pale(
                tint,
                SIGNAL_PALE[0] + (SIGNAL_PALE[1] - SIGNAL_PALE[0]) * a as f32,
            )
            .map(|c| c * SIGNAL_BRIGHTNESS),
            opacity as f32,
            (seed * 3.1 + lap * 0.37 + j as f64 * 0.19) as f32,
            0.0,
            (smoothstep(a, 0.4, 1.0) * 0.6) as f32,
            (0.15 + 0.4 * a) as f32,
            0.0,
        );
    }
}

/// A promotion's pop: the colour rolls out over the tank round a core that rises a
/// little, and clears.
fn pop(effect: &PickupEffect, puffs: &mut PoolBuffer) {
    let age = effect.age;
    let progress = age / POP_SECONDS;
    let spread = 1.0 - (1.0 - progress).powi(3);
    let turn = hash(effect.x * 0.731 + effect.z * 0.377) * TAU;
    let tint = pale(effect.color, 0.12);
    let opacity = POP_OPACITY * smoothstep(age, 0.0, 0.03) * (1.0 - progress).powf(1.6);
    let center = Vec3::new(
        effect.x as f32,
        (POP_HEIGHT + POP_RISE * spread) as f32,
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
    fn stocked_pallets_smoke_fizzle_out_once_taken_and_promotions_pop() {
        let mut state = RenderState {
            elapsed: 20.0,
            ..Default::default()
        };
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
        let stocked = puffs.len();
        assert!(
            stocked > 0 && stocked <= SIGNAL_PUFFS,
            "a column over the pallet"
        );
        // Just taken: the column already given off keeps rising.
        state.pickups[0].available = false;
        state.pickups[0].cooldown_duration = 15.0;
        state.pickups[0].cooldown = 14.9;
        puffs.clear();
        effects.draw(&state, &mut puffs);
        assert!(
            !puffs.is_empty() && puffs.len() <= stocked,
            "the last smoke"
        );
        // Long after: nothing.
        state.pickups[0].cooldown = 15.0 - SIGNAL_CYCLE - 0.1;
        puffs.clear();
        effects.draw(&state, &mut puffs);
        assert!(puffs.is_empty(), "no smoke from an empty pallet");
        for i in 0..30 {
            effects.add(f64::from(i), 0.0, 0xffb02a);
        }
        assert_eq!(effects.len(), MAX_PICKUP_EFFECTS);
        effects.update(0.1);
        effects.draw(&state, &mut puffs);
        assert_eq!(puffs.len(), MAX_PICKUP_EFFECTS * POP_PUFFS);
        assert!(
            puffs.records()[0].translation().y > POP_HEIGHT as f32,
            "the pop hangs over the tank"
        );
        effects.update(PICKUP_SECONDS);
        assert!(effects.is_empty());
    }
}
