//! Supply drop smoke (`presentation.ts` `pickupEffect`, reworked as military
//! supply): one thin wisp of signal smoke from the grenade at the back corner of
//! every stocked pallet, its kind's colour greyed toward smoke (`pickup_signal`),
//! billowing a little where it vents and leaned over by the one wind every wisp
//! shares, away from the overhead camera, each at its own height and phase, so a
//! field of drops marks where supply stands without a fence of coloured plumes. An
//! empty drop's grenade is out until its restock nears, when it starts to fizzle
//! again; a restocked drop's wisp grows from it. When a tank takes a supply its
//! crushed grenade vents one puff of its smoke out from under the hull, rolling
//! downwind, with a little of the pallet's dust kicked out low beside it, and what
//! the wisp gave off thins away within a breath. A restocked crate landing on its
//! pallet raises a little dust. A promotion pops a short burst of its colour over
//! the tank. Everything is smoke in the shared puff pool, drawn like the blasts'
//! puffs: no light, beams or rings.

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

const SUPPLY_TURN: f64 = 0.12;

/// The wisp: puffs in it, seconds each takes to climb it, how high it climbs
/// (each within `SIGNAL_RISE_SPREAD` of it, so a row of drops is not a fence) and
/// how its climb slows (the jet leaves the grenade fast and loses its push), the
/// puff size at the grenade and at the top and how soon it widens, how far the
/// wind has leaned it over by the top and how late in the climb the lean takes
/// hold, and its opacity at the grenade.
const SIGNAL_PUFFS: usize = 12;
const SIGNAL_CYCLE: f64 = 3.6;
const SIGNAL_RISE: f64 = 1.75;
const SIGNAL_RISE_SPREAD: f64 = 0.3;
const SIGNAL_SLOWING: f64 = 1.6;
const SIGNAL_SIZE: [f64; 2] = [0.24, 0.6];
const SIGNAL_WIDENING: f64 = 0.5;
const SIGNAL_SHEAR: f64 = 0.85;
const SIGNAL_SHEAR_ONSET: f64 = 1.4;
const SIGNAL_OPACITY: f64 = 0.7;
/// The wisp's colour along its climb: the signal colour where it vents, greying
/// toward smoke as it thins.
const SIGNAL_GREYING: [f32; 2] = [0.0, 0.4];
const SMOKE_GREY: [f32; 3] = [0.66, 0.65, 0.62];
/// Signal smoke is a little denser and brighter than the blasts' grey smoke, so a
/// wisp keeps its tint on its shaded side.
const SIGNAL_BRIGHTNESS: f32 = 1.5;
/// How the wisp frays: the puffs' erosion at the grenade and at the top (taking
/// hold past the middle of the climb), and how far the wisps it breaks into stray
/// across the wind by the top.
const SIGNAL_EROSION: [f64; 2] = [0.08, 0.66];
const SIGNAL_STRAY: f64 = 0.16;
/// The one wind every wisp is leaned by: away from the overhead camera (which
/// sits toward +z) and a little across, so a wisp trails up the screen behind
/// its own crate rather than over its neighbours.
const SIGNAL_WIND: [f64; 2] = [0.36, -0.93];
/// The wisp meanders: how far it wanders across the wind by the top, at one rate
/// for every wisp (rad/s of emission time, so neighbouring puffs follow one
/// another), each at its own phase.
const SIGNAL_MEANDER: f64 = 0.1;
const SIGNAL_MEANDER_RATE: f64 = 0.8;
/// The grenade gushes: one rate for every wisp (rad/s of emission time, so a
/// puff keeps its share as it climbs), each at its own phase, and how much density
/// and size the lulls lose.
const SIGNAL_GUSH_RATE: f64 = 2.3;
const SIGNAL_GUSH: f64 = 0.25;
/// An empty drop's grenade fizzles again once its restock is this far along: a
/// thin, low wisp of this share of the full one's density and height at first,
/// growing to the full wisp as the supply returns.
const FIZZLE_FROM: f64 = 0.7;
const FIZZLE: [f64; 2] = [0.15, 0.85];
/// Once the supply is taken the grenade goes out: the wisp it gave off thins
/// away over this long.
const SNUFF_SECONDS: f64 = 0.45;
/// A restocked crate landing on its pallet: dust puffed out low from under it
/// (puffs, start and end reach, size, opacity, seconds).
const LANDING_PUFFS: usize = 6;
const LANDING_REACH: [f64; 2] = [0.7, 1.4];
const LANDING_SIZE: [f64; 2] = [0.26, 0.5];
const LANDING_OPACITY: f64 = 0.32;
const LANDING_SECONDS: f64 = 0.55;
/// How far the ground's dust is lightened toward a pale haze.
const DUST_LIGHTEN: f32 = 0.4;
/// A crushed grenade's vent: puffs of the kind's smoke squeezed out from under the
/// hull on the drop's side, rolling out along it and downwind as they rise
/// (count, how far out they start from the drop and how far they roll, start and
/// end height, start and end size, opacity, seconds), and the pallet's dust
/// kicked out low beside them (count, reach, size, opacity, seconds).
const VENT_PUFFS: usize = 3;
const VENT_FROM: f64 = 0.7;
const VENT_ROLL: f64 = 1.3;
const VENT_HEIGHT: [f64; 2] = [0.3, 1.5];
const VENT_SIZE: [f64; 2] = [0.4, 1.05];
const VENT_OPACITY: f64 = 0.6;
const VENT_SECONDS: f64 = 1.4;
const VENT_DUST_PUFFS: usize = 3;
const VENT_DUST_REACH: [f64; 2] = [0.9, 1.6];
const VENT_DUST_SIZE: [f64; 2] = [0.3, 0.7];
const VENT_DUST_OPACITY: f64 = 0.34;
const VENT_DUST_SECONDS: f64 = 0.8;
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

/// A beat round the drop at (x, z): the ground's dust as a crate lands on its
/// pallet, or a crushed grenade's vent of its signal colour toward `away` (a unit
/// ground direction, from the hull out over the drop); its age counts up from
/// below zero while it waits to start.
#[derive(Clone, Copy, Debug)]
struct Collection {
    x: f64,
    z: f64,
    age: f64,
    vent: Option<Vent>,
}

#[derive(Clone, Copy, Debug)]
struct Vent {
    color: [f32; 3],
    away: [f64; 2],
}

impl Collection {
    fn seconds(&self) -> f64 {
        if self.vent.is_some() {
            VENT_SECONDS
        } else {
            LANDING_SECONDS
        }
    }
}

/// Whether a drop was last seen stocked, and since when (negative infinity for a
/// drop first seen in that state, so a battle's drops start with full wisps).
#[derive(Clone, Copy, Debug)]
struct Supply {
    id: u32,
    stocked: bool,
    since: f64,
    /// When its grenade relit while it was last seen empty (infinity: not yet).
    relit: f64,
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

    /// The dust a crate raises landing on its pallet at (x, z), `delay` seconds
    /// from now.
    pub fn land(&mut self, x: f64, z: f64, delay: f64) {
        self.push(Collection {
            x,
            z,
            age: -delay,
            vent: None,
        });
    }

    /// The puff of `color` smoke a drop's grenade at (x, z) vents as a hull
    /// crushes it, out toward `away` (a ground direction from the hull).
    pub fn vent(&mut self, x: f64, z: f64, away: [f32; 2], color: u32) {
        let [ax, az] = away.map(f64::from);
        let length = ax.hypot(az).max(1e-6);
        self.push(Collection {
            x,
            z,
            age: 0.0,
            vent: Some(Vent {
                color: hex_to_linear(color),
                away: [ax / length, az / length],
            }),
        });
    }

    pub fn update(&mut self, dt: f64) {
        self.effects.retain_mut(|effect| {
            effect.age += dt;
            effect.age < PICKUP_SECONDS
        });
        self.collections.retain_mut(|beat| {
            beat.age += dt;
            beat.age < beat.seconds()
        });
    }

    /// Write the column over every stocked pallet (the last of it over a pallet
    /// just emptied, a fizzle over one about to restock) and the promotion pops
    /// into `puffs`, after the blasts have rebuilt it this frame.
    pub fn draw(&mut self, state: &RenderState, puffs: &mut PoolBuffer) {
        self.supplies
            .retain(|supply| state.pickups.iter().any(|pickup| pickup.id == supply.id));
        for pickup in &state.pickups {
            // An empty drop's grenade relights when its restock is `FIZZLE_FROM`
            // along: from then (when the cooldown is known), and how strongly.
            let relit = (!pickup.available && pickup.cooldown_duration > 0.0).then(|| {
                let progress = 1.0 - pickup.cooldown / pickup.cooldown_duration;
                let lit = state.elapsed - (progress - FIZZLE_FROM) * pickup.cooldown_duration;
                let near = smoothstep(progress, FIZZLE_FROM, 1.0);
                (lit, FIZZLE[0] + (FIZZLE[1] - FIZZLE[0]) * near)
            });
            let since = match self.supplies.iter_mut().find(|s| s.id == pickup.id) {
                Some(supply) => {
                    if supply.stocked != pickup.available {
                        supply.stocked = pickup.available;
                        // A restocked grenade carries on from its fizzle, if it was
                        // seen relit; otherwise its column starts afresh.
                        supply.since = supply.relit.min(state.elapsed);
                    }
                    supply.relit = relit.map_or(f64::INFINITY, |(lit, _)| lit);
                    supply.since
                }
                None => {
                    self.supplies.push(Supply {
                        id: pickup.id,
                        stocked: pickup.available,
                        since: f64::NEG_INFINITY,
                        relit: f64::INFINITY,
                    });
                    f64::NEG_INFINITY
                }
            };
            // When the supply was taken: never while stocked, and long enough ago
            // for a drop whose cooldown has no known start. An empty drop's grenade
            // fizzles again as its restock nears.
            let (lit, taken, strength) = if pickup.available {
                (since, f64::INFINITY, 1.0)
            } else if let Some((lit, strength)) = relit {
                let taken = state.elapsed - (pickup.cooldown_duration - pickup.cooldown).max(0.0);
                if lit <= state.elapsed {
                    (lit, f64::INFINITY, strength)
                } else {
                    (f64::NEG_INFINITY, taken, 1.0)
                }
            } else {
                continue;
            };
            if state.elapsed - taken > SNUFF_SECONDS {
                continue;
            }
            let base = grenade(pickup.x, pickup.z, supply_turn(pickup.id));
            let signal = Signal {
                color: pickup_signal(pickup.kind),
                lit,
                taken,
                strength,
            };
            column(pickup.id, base, signal, state.elapsed, puffs);
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
            match beat.vent {
                Some(vent) => vent_puffs(beat, vent, dust, puffs),
                None => landing(beat, dust, puffs),
            }
            any = true;
        }
        any
    }
}

/// What a drop's grenade gives off: its colour, since when (`lit`) and until when
/// (`taken`) it has vented, and how strongly (1 for the full column, less for the
/// fizzle of a grenade relighting).
struct Signal {
    color: u32,
    lit: f64,
    taken: f64,
    strength: f64,
}

/// One drop's column: puffs leaving the grenade since it was lit and before the
/// supply was taken, dense and in the kind's colour where they leave it, then
/// leaned over downwind, widening, fraying and greying a little as they climb. Each
/// puff keeps the gush, meander and stray it left the grenade with, so the column
/// moves as one body and never trembles; once the supply is taken it thins away.
fn column(id: u32, base: Vec3, signal: Signal, time: f64, puffs: &mut PoolBuffer) {
    let tint = hex_to_linear(signal.color);
    let seed = hash(f64::from(id));
    let rise = SIGNAL_RISE
        * (1.0 + SIGNAL_RISE_SPREAD * (2.0 * hash(f64::from(id) + 7.3) - 1.0))
        * signal.strength.sqrt();
    let snuff = 1.0 - smoothstep(time - signal.taken, 0.0, SNUFF_SECONDS);
    let [wx, wz] = SIGNAL_WIND;
    for j in 0..SIGNAL_PUFFS {
        let cycle = time / SIGNAL_CYCLE + j as f64 / SIGNAL_PUFFS as f64 + seed;
        let a = cycle.fract();
        // When this puff left the grenade; constant over its climb.
        let emitted = time - a * SIGNAL_CYCLE;
        if emitted < signal.lit || emitted > signal.taken {
            continue;
        }
        let gush =
            1.0 - SIGNAL_GUSH * 0.5 * (1.0 + (emitted * SIGNAL_GUSH_RATE + seed * TAU).sin());
        let height = rise * (1.0 - (1.0 - a).powf(SIGNAL_SLOWING));
        let shear = SIGNAL_SHEAR * a.powf(SIGNAL_SHEAR_ONSET);
        // Across the wind: the slow meander of the whole column, and each puff's
        // own stray as it breaks up.
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
            * (0.55 + 0.45 * signal.strength)
            * (1.0 - 0.25 * smoothstep(a, 0.6, 1.0) * hash(emitted * 5.1 + seed));
        let opacity = SIGNAL_OPACITY
            * signal.strength
            * gush
            * snuff
            * smoothstep(a, 0.0, 0.04)
            * (1.0 - a).powf(1.1);
        let [low, high] = SIGNAL_GREYING;
        let grey = low + (high - low) * smoothstep(a, 0.2, 1.0) as f32;
        let erosion =
            SIGNAL_EROSION[0] + (SIGNAL_EROSION[1] - SIGNAL_EROSION[0]) * smoothstep(a, 0.4, 1.0);
        smoke_puff(
            puffs,
            position,
            size as f32,
            mix(tint, SMOKE_GREY, grey).map(|c| c * SIGNAL_BRIGHTNESS),
            opacity as f32,
            (seed * 3.1 + emitted * 0.37).fract() as f32,
            0.0,
            erosion as f32,
            (0.45 + 0.3 * a) as f32,
            0.0,
        );
    }
}

/// A grenade crushed under a hull: a few puffs of its colour squeezed out on the
/// drop's side, rolling out and downwind as they rise and thin, each at its own
/// pace; and the pallet's dust kicked out low beside them. One lopsided cloud,
/// never a ring round the tank.
fn vent_puffs(beat: &Collection, vent: Vent, dust: [f32; 3], puffs: &mut PoolBuffer) {
    let seed = hash(beat.x * 0.613 + beat.z * 0.271);
    let [ax, az] = vent.away;
    let [wx, wz] = SIGNAL_WIND;
    let origin = Vec3::new(beat.x as f32, 0.0, beat.z as f32);
    for j in 0..VENT_PUFFS {
        let pace = 0.75 + 0.5 * hash(seed * 7.0 + j as f64);
        let age = beat.age * pace;
        let progress = (age / VENT_SECONDS).min(1.0);
        let roll = 1.0 - (1.0 - progress).powi(3);
        // Each puff leaves along its own bearing either side of `away`.
        let bearing = (j as f64 - (VENT_PUFFS as f64 - 1.0) / 2.0) * 0.55 + (seed - 0.5) * 0.4;
        let (sin, cos) = bearing.sin_cos();
        let [dx, dz] = [ax * cos - az * sin, ax * sin + az * cos];
        let reach = VENT_FROM + VENT_ROLL * roll * (0.7 + 0.5 * hash(seed + j as f64 * 3.1));
        let drift = 0.6 * progress * progress;
        let height = VENT_HEIGHT[0] + (VENT_HEIGHT[1] - VENT_HEIGHT[0]) * roll * pace;
        let position = origin
            + Vec3::new(
                (dx * reach + wx * drift) as f32,
                height as f32,
                (dz * reach + wz * drift) as f32,
            );
        let size = (VENT_SIZE[0] + (VENT_SIZE[1] - VENT_SIZE[0]) * roll)
            * (0.75 + 0.4 * hash(seed * 3.0 + j as f64));
        let fade = smoothstep(beat.age, 0.0, 0.06) * (1.0 - progress).powf(1.4);
        let tint = mix(vent.color, SMOKE_GREY, (0.15 + 0.45 * progress) as f32)
            .map(|c| c * SIGNAL_BRIGHTNESS);
        smoke_puff(
            puffs,
            position,
            size as f32,
            tint,
            (VENT_OPACITY * fade) as f32,
            (seed + j as f64 * 0.37).fract() as f32,
            0.0,
            (0.15 + progress * 0.6) as f32,
            (0.45 + 0.3 * progress) as f32,
            0.0,
        );
    }
    let progress = beat.age / VENT_DUST_SECONDS;
    if progress >= 1.0 {
        return;
    }
    let spread = 1.0 - (1.0 - progress).powi(3);
    let fade = smoothstep(beat.age, 0.0, 0.03) * (1.0 - progress).powf(1.5);
    for j in 0..VENT_DUST_PUFFS {
        let bearing = (j as f64 - 1.0) * 0.8 + (seed - 0.5) * 0.5 + 0.4;
        let (sin, cos) = bearing.sin_cos();
        let [dx, dz] = [ax * cos - az * sin, ax * sin + az * cos];
        let reach = VENT_DUST_REACH[0] + (VENT_DUST_REACH[1] - VENT_DUST_REACH[0]) * spread;
        let position = origin
            + Vec3::new(
                (dx * reach) as f32,
                (0.1 + 0.12 * spread) as f32,
                (dz * reach) as f32,
            );
        let size = (VENT_DUST_SIZE[0] + (VENT_DUST_SIZE[1] - VENT_DUST_SIZE[0]) * spread)
            * (0.8 + 0.4 * hash(seed * 5.0 + j as f64));
        smoke_puff(
            puffs,
            position,
            size as f32,
            dust,
            (VENT_DUST_OPACITY * fade) as f32,
            (seed * 2.0 + j as f64 * 0.41).fract() as f32,
            0.0,
            (0.25 + progress * 0.6) as f32,
            (0.5 + 0.3 * progress) as f32,
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
        // Out from under the pallet's sides, unevenly, so it is not a ring.
        let bearing = turn + TAU * (j as f64 + 0.6 * hash(turn + j as f64)) / LANDING_PUFFS as f64;
        let reach = (LANDING_REACH[0] + (LANDING_REACH[1] - LANDING_REACH[0]) * spread)
            * (0.75 + 0.5 * hash(turn * 3.0 + j as f64));
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
            "a full column over a drop first seen stocked"
        );
        // Just taken: the smoke already given off is still there, thinning.
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
        // Restocked: the column grows from the grenade.
        state.pickups[0].available = true;
        puffs.clear();
        effects.draw(&state, &mut puffs);
        assert!(puffs.len() <= 1, "a relit grenade starts its column afresh");
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

    #[test]
    fn an_empty_grenade_fizzles_before_its_restock_and_a_crushed_one_vents() {
        let mut state = RenderState {
            elapsed: 50.0,
            ..Default::default()
        };
        state.pickups.push(Pickup {
            id: 5,
            kind: PickupKind::Rocket,
            x: 0.0,
            z: 0.0,
            available: false,
            cooldown: 10.0,
            cooldown_duration: 15.0,
        });
        let mut effects = PickupEffects::default();
        let mut puffs = PoolBuffer::new(1024);
        effects.draw(&state, &mut puffs);
        assert!(puffs.is_empty(), "out for most of the cooldown");
        // Near its restock the grenade has relit and fizzles.
        state.elapsed += 9.0;
        state.pickups[0].cooldown = 1.0;
        effects.draw(&state, &mut puffs);
        let fizzle = puffs.len();
        assert!(fizzle > 2, "a fizzle near the restock");
        // Restocked, the column carries on from the fizzle instead of restarting.
        state.elapsed += 1.0;
        state.pickups[0].available = true;
        puffs.clear();
        effects.draw(&state, &mut puffs);
        assert!(puffs.len() >= fizzle, "the column carries on");
        // A crushed grenade vents its colour out on the drop's side, and it clears.
        effects.vent(0.0, 0.0, [1.0, 0.0], 0xd2622a);
        effects.update(0.2);
        puffs.clear();
        assert!(effects.draw_collections(&state, &mut puffs));
        assert_eq!(puffs.len(), VENT_PUFFS + VENT_DUST_PUFFS);
        assert!(
            puffs
                .records()
                .iter()
                .all(|puff| puff.translation().x > 0.0),
            "the vent leaves on the drop's side, not round the hull"
        );
        assert!(
            puffs
                .records()
                .iter()
                .all(|puff| puff.translation().y < 2.5),
            "the vent stays low"
        );
        effects.update(VENT_SECONDS);
        assert!(effects.is_empty());
    }
}
