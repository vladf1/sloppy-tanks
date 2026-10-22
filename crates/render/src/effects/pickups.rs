//! Supply drop smoke (`presentation.ts` `pickupEffect`, reworked as military
//! supply): one thin, restrained wisp of signal smoke from the grenade at the back
//! corner of every stocked pallet, vented dense and in the kind's milky colour
//! (`pickup_signal`) at the grenade, widening, thinning and greying as it climbs
//! and is sheared over by the one wind every wisp shares, away from the overhead
//! camera, so a field of drops reads as kit marked for collection rather than a
//! rainbow. A restocked drop's grenade is lit again and its wisp grows from it; an
//! empty drop gives off nothing. When a tank takes a supply its grenade goes out:
//! what it gave off thins away within a breath, and the load hauled off its pallet
//! raises a little of the ground's dust round it, as a restocked crate landing on
//! its pallet does. A promotion pops a short burst of its colour over the tank.
//! Everything is smoke in the shared puff pool, drawn like the blasts' puffs: no
//! light, beams or rings.

use std::collections::VecDeque;
use std::f64::consts::TAU;

use glam::Vec3;
use sloppy_core::models::{SMOKE_MARKER, SUPPLY_SCALE, pickup_signal};
use sloppy_core::sim::RenderState;

use super::explosions::smoke_puff;
use super::ground::Ground;
use super::pool::PoolBuffer;
use crate::color::hex_to_linear;

/// `FEEDBACK.maxPickupEffects` (view-settings.ts), and how long a promotion's
/// smoke pop lasts.
pub const MAX_PICKUP_EFFECTS: usize = 24;
pub const PICKUP_SECONDS: f64 = POP_SECONDS;

/// A supply's turn on its pallet (rad): fixed per pickup and within
/// `SUPPLY_TURN` of square, so drops do not stand in ranks yet never swing into a
/// neighbour; shared by the pallet, the item and its smoke. Cosmetic only.
pub fn supply_turn(id: u32) -> f64 {
    (hash(f64::from(id) + 0.5) - 0.5) * 2.0 * SUPPLY_TURN
}

/// A drop's weathering (0..1, the `pickup-face` effect's instance `w`): how its
/// pallet's boards and its paint differ from the next drop's, and whether its
/// pallet has lost a board. Cosmetic only.
pub fn supply_wear(id: u32) -> f32 {
    hash(f64::from(id) * 1.37 + 4.1) as f32
}

const SUPPLY_TURN: f64 = 0.3;

/// The wisp: puffs in it, seconds each takes to climb it, how high it climbs (each
/// within `SIGNAL_RISE_SPREAD` of it, so a row of drops is not a fence) and how its
/// climb slows (the jet leaves the grenade fast and loses its push), the puff size
/// at the grenade and at the top and how soon it widens, how far the wind has
/// sheared it over by the top and how late in the climb the shear takes hold, and
/// its opacity at the grenade.
const SIGNAL_PUFFS: usize = 18;
const SIGNAL_CYCLE: f64 = 3.4;
const SIGNAL_RISE: f64 = 2.4;
const SIGNAL_RISE_SPREAD: f64 = 0.16;
const SIGNAL_SLOWING: f64 = 1.6;
const SIGNAL_SIZE: [f64; 2] = [0.14, 0.5];
const SIGNAL_WIDENING: f64 = 0.7;
const SIGNAL_SHEAR: f64 = 1.2;
const SIGNAL_SHEAR_ONSET: f64 = 1.6;
const SIGNAL_OPACITY: f64 = 0.78;
/// The wisp's colour along its climb: the signal colour nearly whole where it
/// vents, and mostly smoke's pale grey by the top.
const SIGNAL_GREYING: [f32; 2] = [0.12, 0.7];
const SMOKE_GREY: [f32; 3] = [0.6, 0.59, 0.56];
/// How the wisp frays: the puffs' erosion at the grenade and at the top (taking
/// hold past the middle of the climb), and how far the wisps it breaks into stray
/// across the wind by the top.
const SIGNAL_EROSION: [f64; 2] = [0.1, 0.75];
const SIGNAL_STRAY: f64 = 0.22;
/// The one wind every wisp is sheared by: away from the overhead camera (which
/// sits toward +z) and a little across, so a wisp trails up the screen behind its
/// own crate rather than over its neighbours.
const SIGNAL_WIND: [f64; 2] = [0.36, -0.93];
/// The wisp meanders: how far it wanders across the wind by the top, at one rate
/// for every wisp (rad/s of emission time, so neighbouring puffs follow one
/// another), each at its own phase.
const SIGNAL_MEANDER: f64 = 0.1;
const SIGNAL_MEANDER_RATE: f64 = 0.8;
/// The grenade gushes: one rate for every wisp (rad/s of emission time, so a puff
/// keeps its share as it climbs), each at its own phase, and how much density and
/// size the lulls lose.
const SIGNAL_GUSH_RATE: f64 = 2.3;
const SIGNAL_GUSH: f64 = 0.2;
/// Once the supply is taken the grenade goes out: the wisp it gave off thins away
/// over this long.
const SNUFF_SECONDS: f64 = 0.45;
/// A restocked crate landing on its pallet, or a load hauled off it: a ring of dust
/// puffed out from under it (puffs, start and end reach, size, opacity, seconds).
const LANDING_PUFFS: usize = 8;
const LANDING_REACH: [f64; 2] = [0.8, 1.7];
const LANDING_SIZE: [f64; 2] = [0.3, 0.6];
const LANDING_OPACITY: f64 = 0.38;
const LANDING_SECONDS: f64 = 0.6;
/// How far the ground's dust is lightened toward a pale haze.
const DUST_LIGHTEN: f32 = 0.4;
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

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// Where a drop's grenade stands (its top, world), from the pallet's turn.
fn grenade(x: f64, z: f64, turn: f64) -> Vec3 {
    let (sin, cos) = turn.sin_cos();
    let [mx, top, mz] = SMOKE_MARKER.map(|v| v * SUPPLY_SCALE);
    Vec3::new(
        (x + mx * cos + mz * sin) as f32,
        (top + 0.03) as f32,
        (z - mx * sin + mz * cos) as f32,
    )
}

#[derive(Clone, Copy, Debug)]
struct PickupEffect {
    x: f64,
    z: f64,
    age: f64,
    color: [f32; 3],
}

/// A ring of the ground's dust round the drop at (x, z), as a crate lands on its
/// pallet or is hauled off it; its age counts up from below zero while it waits
/// to start.
#[derive(Clone, Copy, Debug)]
struct Collection {
    x: f64,
    z: f64,
    age: f64,
}

/// Whether a drop was last seen stocked, and since when (negative infinity for a
/// drop first seen in that state, so a battle's drops start with full wisps).
#[derive(Clone, Copy, Debug)]
struct Supply {
    id: u32,
    stocked: bool,
    since: f64,
}

#[derive(Clone, Debug)]
pub struct PickupEffects {
    effects: VecDeque<PickupEffect>,
    collections: VecDeque<Collection>,
    supplies: Vec<Supply>,
}

impl Default for PickupEffects {
    fn default() -> Self {
        Self {
            effects: VecDeque::with_capacity(MAX_PICKUP_EFFECTS),
            collections: VecDeque::with_capacity(MAX_PICKUP_EFFECTS),
            supplies: Vec::new(),
        }
    }
}

impl PickupEffects {
    pub fn len(&self) -> usize {
        self.effects.len() + self.collections.len()
    }

    pub fn is_empty(&self) -> bool {
        self.effects.is_empty() && self.collections.is_empty()
    }

    pub fn reset(&mut self) {
        self.effects.clear();
        self.collections.clear();
        self.supplies.clear();
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

    fn push(&mut self, beat: Collection) {
        if self.collections.len() >= MAX_PICKUP_EFFECTS {
            self.collections.pop_front();
        }
        self.collections.push_back(beat);
    }

    /// The dust a crate raises landing on its pallet at (x, z) or hauled off it,
    /// `delay` seconds from now.
    pub fn land(&mut self, x: f64, z: f64, delay: f64) {
        self.push(Collection { x, z, age: -delay });
    }

    pub fn update(&mut self, dt: f64) {
        self.effects.retain_mut(|effect| {
            effect.age += dt;
            effect.age < PICKUP_SECONDS
        });
        self.collections.retain_mut(|beat| {
            beat.age += dt;
            beat.age < LANDING_SECONDS
        });
    }

    /// Write the wisp over every stocked pallet (and the last of it over a pallet
    /// just emptied) and the promotion pops into `puffs`, after the blasts have
    /// rebuilt it this frame.
    pub fn draw(&mut self, state: &RenderState, puffs: &mut PoolBuffer) {
        self.supplies
            .retain(|supply| state.pickups.iter().any(|pickup| pickup.id == supply.id));
        for pickup in &state.pickups {
            let since = match self.supplies.iter_mut().find(|s| s.id == pickup.id) {
                Some(supply) => {
                    if supply.stocked != pickup.available {
                        supply.stocked = pickup.available;
                        supply.since = state.elapsed;
                    }
                    supply.since
                }
                None => {
                    self.supplies.push(Supply {
                        id: pickup.id,
                        stocked: pickup.available,
                        since: f64::NEG_INFINITY,
                    });
                    f64::NEG_INFINITY
                }
            };
            // When the supply was taken: never while stocked, and long enough ago
            // for a drop whose cooldown has no known start.
            let (lit, taken) = if pickup.available {
                (since, f64::INFINITY)
            } else if pickup.cooldown_duration > 0.0 {
                let taken = state.elapsed - (pickup.cooldown_duration - pickup.cooldown).max(0.0);
                (f64::NEG_INFINITY, taken)
            } else {
                continue;
            };
            if state.elapsed - taken > SNUFF_SECONDS {
                continue;
            }
            let base = grenade(pickup.x, pickup.z, supply_turn(pickup.id));
            wisp(
                pickup.id,
                base,
                pickup_signal(pickup.kind),
                state.elapsed,
                lit,
                taken,
                puffs,
            );
        }
        for effect in &self.effects {
            pop(effect, puffs);
        }
    }

    /// Write the collections' beats into `puffs`. They play round the tank that
    /// took the supply, usually the viewer's, so they are written after the pool
    /// has cleared the sight line to it; returns whether any were.
    pub fn draw_collections(&self, state: &RenderState, puffs: &mut PoolBuffer) -> bool {
        let ground = Ground::from_theme(&state.map_theme);
        let mut any = false;
        for beat in self.collections.iter().filter(|beat| beat.age >= 0.0) {
            let dust = pale(ground.dust(beat.x, beat.z).map(|c| c as f32), DUST_LIGHTEN);
            landing(beat, dust, puffs);
            any = true;
        }
        any
    }
}

/// One drop's wisp: puffs leaving the grenade since it was `lit` and before the
/// supply was `taken`, thin and only tinted where they leave it, then sheared over
/// downwind, widening a little, greying and fraying as they climb. Each puff keeps
/// the gush, meander and stray it left the grenade with, so the wisp moves as one
/// body and never trembles; once the supply is taken it thins away.
fn wisp(id: u32, base: Vec3, color: u32, time: f64, lit: f64, taken: f64, puffs: &mut PoolBuffer) {
    let tint = hex_to_linear(color);
    let seed = hash(f64::from(id));
    let rise = SIGNAL_RISE * (1.0 + SIGNAL_RISE_SPREAD * (2.0 * hash(f64::from(id) + 7.3) - 1.0));
    let snuff = 1.0 - smoothstep(time - taken, 0.0, SNUFF_SECONDS);
    let [wx, wz] = SIGNAL_WIND;
    for j in 0..SIGNAL_PUFFS {
        let cycle = time / SIGNAL_CYCLE + j as f64 / SIGNAL_PUFFS as f64 + seed;
        let a = cycle.fract();
        // When this puff left the grenade; constant over its climb.
        let emitted = time - a * SIGNAL_CYCLE;
        if emitted < lit || emitted > taken {
            continue;
        }
        let gush =
            1.0 - SIGNAL_GUSH * 0.5 * (1.0 + (emitted * SIGNAL_GUSH_RATE + seed * TAU).sin());
        let height = rise * (1.0 - (1.0 - a).powf(SIGNAL_SLOWING));
        let shear = SIGNAL_SHEAR * a.powf(SIGNAL_SHEAR_ONSET);
        // Across the wind: the slow meander of the whole wisp, and each puff's own
        // stray as it breaks up.
        let stray = SIGNAL_STRAY * (hash(emitted * 3.7 + seed) - 0.5) * 2.0 * a * a;
        let meander =
            SIGNAL_MEANDER * a * (emitted * SIGNAL_MEANDER_RATE + seed * 11.0).sin() + stray;
        let position = base
            + Vec3::new(
                (wx * shear - wz * meander) as f32,
                height as f32,
                (wz * shear + wx * meander) as f32,
            );
        let size = (SIGNAL_SIZE[0] + (SIGNAL_SIZE[1] - SIGNAL_SIZE[0]) * a.powf(SIGNAL_WIDENING))
            * (0.88 + 0.12 * gush)
            * (1.0 - 0.25 * smoothstep(a, 0.6, 1.0) * hash(emitted * 5.1 + seed));
        let opacity =
            SIGNAL_OPACITY * gush * snuff * smoothstep(a, 0.0, 0.04) * (1.0 - a).powf(1.4);
        let [low, high] = SIGNAL_GREYING;
        let grey = low + (high - low) * smoothstep(a, 0.0, 0.7) as f32;
        let erosion =
            SIGNAL_EROSION[0] + (SIGNAL_EROSION[1] - SIGNAL_EROSION[0]) * smoothstep(a, 0.4, 1.0);
        smoke_puff(
            puffs,
            position,
            size as f32,
            mix(tint, SMOKE_GREY, grey),
            opacity as f32,
            (seed * 3.1 + emitted * 0.37).fract() as f32,
            0.0,
            erosion as f32,
            (0.45 + 0.25 * a) as f32,
            0.0,
        );
    }
}

/// A crate landing on its pallet or hauled off it: a ring of the ground's `dust`
/// puffed out low from under it.
fn landing(beat: &Collection, dust: [f32; 3], puffs: &mut PoolBuffer) {
    let progress = beat.age / LANDING_SECONDS;
    let spread = 1.0 - (1.0 - progress).powi(3);
    let fade = smoothstep(beat.age, 0.0, 0.03) * (1.0 - progress).powf(1.5);
    let turn = hash(beat.x * 0.531 + beat.z * 0.917) * TAU;
    for j in 0..LANDING_PUFFS {
        let bearing = turn + TAU * j as f64 / LANDING_PUFFS as f64;
        let reach = LANDING_REACH[0] + (LANDING_REACH[1] - LANDING_REACH[0]) * spread;
        let position = Vec3::new(
            (beat.x + bearing.cos() * reach) as f32,
            (0.12 + 0.12 * spread) as f32,
            (beat.z + bearing.sin() * reach) as f32,
        );
        let size = LANDING_SIZE[0] + (LANDING_SIZE[1] - LANDING_SIZE[0]) * spread;
        smoke_puff(
            puffs,
            position,
            size as f32,
            dust,
            (LANDING_OPACITY * fade) as f32,
            (turn + j as f64 * 0.37).fract() as f32,
            0.0,
            (0.2 + progress * 0.6) as f32,
            (0.5 + 0.3 * progress) as f32,
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
    use sloppy_core::sim::PickupKind;
    use sloppy_core::sim::types::Pickup;

    #[test]
    fn stocked_pallets_smoke_snuff_out_once_taken_and_promotions_pop() {
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
            stocked > SIGNAL_PUFFS / 2 && stocked <= SIGNAL_PUFFS,
            "a full wisp over a drop first seen stocked"
        );
        // Just taken: the wisp already given off is still there, thinning.
        state.pickups[0].available = false;
        state.pickups[0].cooldown_duration = 15.0;
        state.pickups[0].cooldown = 14.9;
        puffs.clear();
        effects.draw(&state, &mut puffs);
        assert!(
            !puffs.is_empty() && puffs.len() <= stocked,
            "the last smoke"
        );
        // Soon after: nothing over the empty pallet.
        state.pickups[0].cooldown = 15.0 - SNUFF_SECONDS * 3.0;
        puffs.clear();
        effects.draw(&state, &mut puffs);
        assert!(puffs.is_empty(), "no smoke from an empty pallet");
        // Restocked: the wisp grows from the grenade.
        state.pickups[0].available = true;
        puffs.clear();
        effects.draw(&state, &mut puffs);
        assert!(puffs.len() <= 1, "a relit grenade starts its wisp afresh");
        state.elapsed += SIGNAL_CYCLE / 2.0;
        puffs.clear();
        effects.draw(&state, &mut puffs);
        assert!(puffs.len() > 2 && puffs.len() < stocked, "and it climbs");
        puffs.clear();
        effects.land(4.0, -2.0, 0.0);
        effects.land(0.0, 0.0, 0.3);
        effects.update(0.1);
        assert!(effects.draw_collections(&state, &mut puffs));
        assert_eq!(
            puffs.len(),
            LANDING_PUFFS,
            "the landing waits for the crate to fall"
        );
        assert!(
            puffs
                .records()
                .iter()
                .all(|puff| puff.translation().y < 0.6),
            "the dust stays low"
        );
        effects.update(0.3);
        puffs.clear();
        effects.draw_collections(&state, &mut puffs);
        assert_eq!(puffs.len(), 2 * LANDING_PUFFS);
        effects.update(LANDING_SECONDS);
        assert!(effects.is_empty(), "the beat is over in a breath");
        puffs.clear();
        for i in 0..30 {
            effects.add(f64::from(i), 0.0, 0xffb02a);
        }
        assert_eq!(effects.len(), MAX_PICKUP_EFFECTS);
        effects.update(0.1);
        state.pickups.clear();
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
