//! Supply drop smoke (`presentation.ts` `pickupEffect`, reworked as military
//! supply): one plume of signal smoke from the grenade beside every stocked pallet,
//! in the kind's signal colour (`pickup_signal`), pale where it jets from the
//! grenade, then billowing, widening, greying and thinning as it climbs and leaning
//! in the one wind every plume shares, away from the overhead camera so it marks
//! the crate without hiding it. A restocked drop's grenade is lit again and its
//! plume grows from it. When a supply is taken its grenade is snuffed: no more
//! smoke leaves it and what it gave off thins away within a breath, while a low
//! burst of its colour and the ground's dust rolls out over the drop. A promotion
//! pops a short burst of its colour over the tank. Everything is smoke in the
//! shared puff pool, drawn like the blasts' puffs: no light, beams or rings.

use std::collections::VecDeque;
use std::f64::consts::TAU;

use glam::Vec3;
use sloppy_core::models::{SMOKE_MARKER, pickup_signal};
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
/// neighbour; shared by the pallet, its marking, the item and its smoke. Cosmetic
/// only.
pub fn supply_turn(id: u32) -> f64 {
    (hash(f64::from(id) + 0.5) - 0.5) * 2.0 * SUPPLY_TURN
}

const SUPPLY_TURN: f64 = 0.3;

/// The plume: puffs in it, seconds each takes to climb it, how high it climbs
/// (each plume within `SIGNAL_RISE_SPREAD` of it, so a row of drops is not a
/// fence) and how its climb slows (the jet leaves the grenade fast and loses its
/// push), the puff size at the grenade and at the top and how soon it billows out,
/// how far the wind has leant it by the top, its opacity where it is densest.
const SIGNAL_PUFFS: usize = 22;
const SIGNAL_CYCLE: f64 = 3.6;
const SIGNAL_RISE: f64 = 2.1;
const SIGNAL_RISE_SPREAD: f64 = 0.16;
const SIGNAL_SLOWING: f64 = 1.7;
const SIGNAL_SIZE: [f64; 2] = [0.2, 1.15];
const SIGNAL_BILLOW: f64 = 0.6;
const SIGNAL_LEAN: f64 = 2.3;
const SIGNAL_OPACITY: f64 = 0.6;
/// The plume's colour along its climb: a pale core where the jet leaves the
/// grenade (how pale, and the share of the climb it takes to reach full colour),
/// then greying and lightening as the smoke dilutes (how much by the top).
const SIGNAL_CORE: f32 = 0.38;
const SIGNAL_CORE_SPAN: f64 = 0.14;
const SIGNAL_GREYING: f32 = 0.78;
/// Signal smoke is dense and bright: its albedo is lifted past the blasts' grey
/// smoke so a plume keeps its hue on its shaded side.
const SIGNAL_BRIGHTNESS: f32 = 1.15;
/// The one wind every plume leans in: across the screen and away from the overhead
/// camera (which sits toward +z), so a plume reads as blown smoke and trails off
/// behind its own crate rather than over it.
const SIGNAL_WIND: [f64; 2] = [0.72, -0.69];
/// The plume meanders: how far it wanders across the wind by the top, at one rate
/// for every plume (rad/s of emission time, so neighbouring puffs follow one
/// another), each plume at its own phase.
const SIGNAL_MEANDER: f64 = 0.24;
const SIGNAL_MEANDER_RATE: f64 = 0.8;
/// The grenade gushes: one rate for every plume (rad/s of emission time, so a puff
/// keeps its share as it climbs), each at its own phase, and how much density and
/// size the lulls lose.
const SIGNAL_GUSH_RATE: f64 = 2.3;
const SIGNAL_GUSH: f64 = 0.25;
/// Once the supply is taken the grenade is snuffed: the plume it gave off thins
/// away over this long.
const SNUFF_SECONDS: f64 = 0.6;
/// The collection's ground beat, gone in `COLLECT_SECONDS`: puffs rolling out low
/// from the pallet past the painted circle, out from under the hull that took it
/// (count, start and end reach, height, start
/// and end size, opacity), every other one the kind's colour and the rest the
/// ground's dust; and the grenade's last puff (size, rise).
const COLLECT_SECONDS: f64 = 0.7;
const COLLECT_PUFFS: usize = 14;
const COLLECT_REACH: [f64; 2] = [0.5, 1.75];
const COLLECT_HEIGHT: [f64; 2] = [0.25, 0.6];
const COLLECT_SIZE: [f64; 2] = [0.45, 0.95];
const COLLECT_OPACITY: f64 = 0.55;
const LAST_PUFF_SIZE: [f64; 2] = [0.16, 0.42];
const LAST_PUFF_RISE: f64 = 0.7;
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

/// Toward a light grey of the colour's own luminance: diluted smoke.
fn greyed(color: [f32; 3], amount: f32) -> [f32; 3] {
    let [r, g, b] = color;
    let grey = (0.2126 * r + 0.7152 * g + 0.0722 * b) * 1.1 + 0.06;
    color.map(|c| c + (grey - c) * amount)
}

/// Where a drop's grenade stands (its top, world), from the pallet's turn.
fn grenade(x: f64, z: f64, turn: f64) -> Vec3 {
    let (sin, cos) = turn.sin_cos();
    let [mx, top, mz] = SMOKE_MARKER;
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

/// A collection's ground beat over the drop at (x, z), its pallet turned `turn`.
#[derive(Clone, Copy, Debug)]
struct Collection {
    x: f64,
    z: f64,
    turn: f64,
    age: f64,
    color: [f32; 3],
}

/// Whether a drop was last seen stocked, and since when (negative infinity for a
/// drop first seen in that state, so a battle's drops start with full plumes).
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

    /// The ground beat of a supply taken from the drop at (x, z), its pallet turned
    /// `turn`, in its signal `color`.
    pub fn collect(&mut self, x: f64, z: f64, turn: f64, color: u32) {
        if self.collections.len() >= MAX_PICKUP_EFFECTS {
            self.collections.pop_front();
        }
        self.collections.push_back(Collection {
            x,
            z,
            turn,
            age: 0.0,
            color: hex_to_linear(color),
        });
    }

    pub fn update(&mut self, dt: f64) {
        self.effects.retain_mut(|effect| {
            effect.age += dt;
            effect.age < PICKUP_SECONDS
        });
        self.collections.retain_mut(|beat| {
            beat.age += dt;
            beat.age < COLLECT_SECONDS
        });
    }

    /// Write the plume over every stocked pallet (and the last of it over a pallet
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
            plume(
                pickup.id,
                grenade(pickup.x, pickup.z, supply_turn(pickup.id)),
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

    /// Write the collections' ground beats into `puffs`. They play round the tank
    /// that took the supply, usually the viewer's, so they are written after the
    /// pool has cleared the sight line to it; returns whether any were.
    pub fn draw_collections(&self, state: &RenderState, puffs: &mut PoolBuffer) -> bool {
        let ground = Ground::from_theme(&state.map_theme);
        for beat in &self.collections {
            let dust = ground.dust(beat.x, beat.z).map(|c| c as f32);
            collection(beat, dust, puffs);
        }
        !self.collections.is_empty()
    }
}

/// One drop's plume: puffs leaving the grenade since it was `lit` and before the
/// supply was `taken`, palest where they leave it, billowing, greying and thinning
/// as they climb and lean downwind. Each puff keeps the gush and the meander it
/// left the grenade with, so the plume moves as one body; once the supply is taken
/// the whole plume thins away.
fn plume(id: u32, base: Vec3, color: u32, time: f64, lit: f64, taken: f64, puffs: &mut PoolBuffer) {
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
        let lean = SIGNAL_LEAN * a.powf(1.4);
        let meander = SIGNAL_MEANDER * a * (emitted * SIGNAL_MEANDER_RATE + seed * 11.0).sin();
        let position = base
            + Vec3::new(
                (wx * lean - wz * meander) as f32,
                height as f32,
                (wz * lean + wx * meander) as f32,
            );
        let size = (SIGNAL_SIZE[0] + (SIGNAL_SIZE[1] - SIGNAL_SIZE[0]) * a.powf(SIGNAL_BILLOW))
            * (0.85 + 0.15 * gush);
        let opacity =
            SIGNAL_OPACITY * gush * snuff * smoothstep(a, 0.0, 0.035) * (1.0 - a).powf(1.8);
        let core = SIGNAL_CORE * (1.0 - smoothstep(a, 0.0, SIGNAL_CORE_SPAN)) as f32;
        let grey = SIGNAL_GREYING * smoothstep(a, SIGNAL_CORE_SPAN, 1.0) as f32;
        smoke_puff(
            puffs,
            position,
            size as f32,
            greyed(pale(tint, core), grey).map(|c| c * SIGNAL_BRIGHTNESS),
            opacity as f32,
            (seed * 3.1 + emitted * 0.37) as f32,
            0.0,
            (smoothstep(a, 0.35, 1.0) * 0.6) as f32,
            (0.2 + 0.45 * a) as f32,
            0.0,
        );
    }
}

/// A collection's ground beat: the kind's colour and the ground's `dust` rolling
/// out low from the pallet to the painted circle and settling, and the grenade's
/// last puff rising from it.
fn collection(beat: &Collection, dust: [f32; 3], puffs: &mut PoolBuffer) {
    let progress = beat.age / COLLECT_SECONDS;
    let spread = 1.0 - (1.0 - progress).powi(3);
    let fade = smoothstep(beat.age, 0.0, 0.03) * (1.0 - progress).powf(1.4);
    let turn = hash(beat.x * 0.731 + beat.z * 0.377) * TAU;
    let tint = pale(beat.color, 0.15);
    for j in 0..COLLECT_PUFFS {
        let bearing = turn + TAU * j as f64 / COLLECT_PUFFS as f64;
        let reach = COLLECT_REACH[0] + (COLLECT_REACH[1] - COLLECT_REACH[0]) * spread;
        let height = COLLECT_HEIGHT[0] + (COLLECT_HEIGHT[1] - COLLECT_HEIGHT[0]) * spread;
        let position = Vec3::new(
            (beat.x + bearing.cos() * reach) as f32,
            height as f32,
            (beat.z + bearing.sin() * reach) as f32,
        );
        let size = COLLECT_SIZE[0] + (COLLECT_SIZE[1] - COLLECT_SIZE[0]) * spread;
        let color = if j % 2 == 0 { tint } else { dust };
        smoke_puff(
            puffs,
            position,
            size as f32,
            color,
            (COLLECT_OPACITY * fade) as f32,
            (turn + j as f64 * 0.53) as f32,
            0.0,
            (progress * 0.7) as f32,
            (0.3 + 0.3 * progress) as f32,
            0.0,
        );
    }
    let last = grenade(beat.x, beat.z, beat.turn) + Vec3::Y * (LAST_PUFF_RISE * spread) as f32;
    smoke_puff(
        puffs,
        last,
        (LAST_PUFF_SIZE[0] + (LAST_PUFF_SIZE[1] - LAST_PUFF_SIZE[0]) * spread) as f32,
        greyed(beat.color, 0.4),
        (COLLECT_OPACITY * fade) as f32,
        (turn + 0.3) as f32,
        0.0,
        (progress * 0.8) as f32,
        (0.25 + 0.4 * progress) as f32,
        0.0,
    );
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
            "a full plume over a drop first seen stocked"
        );
        // Just taken: the plume already given off is still there, thinning.
        state.pickups[0].available = false;
        state.pickups[0].cooldown_duration = 15.0;
        state.pickups[0].cooldown = 14.9;
        puffs.clear();
        effects.draw(&state, &mut puffs);
        assert!(
            !puffs.is_empty() && puffs.len() <= stocked,
            "the last smoke"
        );
        // Soon after: nothing.
        state.pickups[0].cooldown = 15.0 - SNUFF_SECONDS - 0.1;
        puffs.clear();
        effects.draw(&state, &mut puffs);
        assert!(puffs.is_empty(), "no smoke from an empty pallet");
        // Restocked: the plume grows from the grenade.
        state.pickups[0].available = true;
        puffs.clear();
        effects.draw(&state, &mut puffs);
        assert!(puffs.len() <= 1, "a relit grenade starts its plume afresh");
        state.elapsed += SIGNAL_CYCLE / 2.0;
        puffs.clear();
        effects.draw(&state, &mut puffs);
        assert!(puffs.len() > 2 && puffs.len() < stocked, "and it climbs");
        puffs.clear();
        effects.collect(4.0, -2.0, 0.0, 0xffffff);
        effects.update(0.1);
        assert!(effects.draw_collections(&state, &mut puffs));
        assert_eq!(puffs.len(), COLLECT_PUFFS + 1);
        assert!(
            puffs.records()[..COLLECT_PUFFS]
                .iter()
                .all(|puff| puff.translation().y < 0.7),
            "the beat stays low"
        );
        effects.update(COLLECT_SECONDS);
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
