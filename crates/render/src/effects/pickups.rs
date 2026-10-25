//! Supply drop smoke (`presentation.ts` `pickupEffect`, reworked as military
//! supply): one thin wisp of signal smoke from the grenade off the back corner of
//! every stocked pallet, in its kind's full colour (`pickup_signal`) for the first
//! third of its climb and then spreading, softening, greying and thinning into the
//! air, leaned over by the one wind every wisp shares, away from the overhead camera,
//! each at its own height and phase, so a field of drops marks where supply stands
//! without a fence of coloured plumes. The wisp is the drop's marker, so the sight
//! line to the player's tank never thins it (`PickupEffects::columns`). Once a drop
//! is taken its grenade goes out for a breath, the smoke it gave off climbing on and
//! thinning away as smoke does, then smoulders again in a faint, low wisp of the
//! returning kind that grows as the restock nears; a restocked drop's wisp carries
//! on from it.
//! When a tank takes a supply its crushed grenade vents one puff of its smoke out
//! from under the hull where the load bursts out, rolling downwind, with a kick of
//! the ground's dust round it. A restocked crate landing on its pallet raises a
//! little dust. A promotion pops a short burst of its colour over the tank.
//! Everything is smoke in the shared puff pool, drawn like the blasts' puffs: no
//! light, beams or rings.

use std::collections::VecDeque;
use std::f64::consts::TAU;

use glam::Vec3;
use sloppy_core::models::{SMOKE_MARKER, SUPPLY_SCALE, pickup_signal};
use sloppy_core::sim::RenderState;

use super::EffectView;
use super::ease::EaseTable;
use super::explosions::smoke_puff;
use super::ground::Ground;
use super::pool::PoolBuffer;
use crate::camera::Sphere;
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
/// puff size at the grenade and at the top and how soon it widens (a plume spreads
/// as it rises, so its top is a broad haze rather than the end of a stick), how far
/// the wind has leaned it over by the top and how late in the climb the lean takes
/// hold, and its opacity at the grenade.
const SIGNAL_PUFFS: usize = 12;
const SIGNAL_CYCLE: f64 = 3.6;
const SIGNAL_RISE: f64 = 2.6;
const SIGNAL_RISE_SPREAD: f64 = 0.3;
const SIGNAL_SLOWING: f64 = 1.6;
const SIGNAL_SIZE: [f64; 2] = [0.18, 0.9];
const SIGNAL_WIDENING: f64 = 0.55;
const SIGNAL_SHEAR: f64 = 1.1;
const SIGNAL_SHEAR_ONSET: f64 = 1.4;
const SIGNAL_OPACITY: f64 = 0.68;
/// The wisp's colour along its climb (share of the climb): the full signal colour
/// to `SIGNAL_SATURATED`, then greying toward smoke by `SIGNAL_GREYING` at the top.
const SIGNAL_SATURATED: f64 = 0.25;
const SIGNAL_GREYING: f32 = 0.8;
const SMOKE_GREY: [f32; 3] = [0.66, 0.65, 0.62];
/// Signal smoke is denser and brighter than the blasts' grey smoke, most where it
/// leaves the fuse, so the wisp keeps its colour on its shaded side and never reads
/// as soot (brightness at the grenade and at the top).
const SIGNAL_BRIGHTNESS: [f32; 2] = [1.5, 1.1];
/// How the wisp frays and softens: the puffs' erosion and haze at the grenade and
/// at the top (taking hold past the saturated part), and how far the wisps it
/// breaks into stray across the wind by the top.
const SIGNAL_EROSION: [f64; 2] = [0.06, 0.66];
const SIGNAL_SOFTNESS: [f64; 2] = [0.3, 0.8];
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
/// An empty drop's grenade smoulders again once its restock is this far along
/// (a breath after it was taken): a faint, low wisp of this share of the full
/// one's density and height at first, growing to the full wisp as the supply
/// returns, so a player can see what will come back there and roughly when.
const FIZZLE_FROM: f64 = 0.06;
const FIZZLE: [f64; 2] = [0.35, 0.9];
/// A restocked crate landing on its pallet: dust puffed out low from under it
/// (puffs, start and end reach, size, opacity, seconds).
const LANDING_PUFFS: usize = 6;
const LANDING_REACH: [f64; 2] = [0.7, 1.4];
const LANDING_SIZE: [f64; 2] = [0.26, 0.5];
const LANDING_OPACITY: f64 = 0.32;
const LANDING_SECONDS: f64 = 0.55;
/// How far the ground's dust is lightened toward a pale haze.
const DUST_LIGHTEN: f32 = 0.45;
/// A crushed grenade's vent: puffs of the kind's smoke squeezed out from under the
/// hull where the load bursts out, rolling out from it and downwind as they rise
/// (count, how far they roll, start and end height, start and end size, opacity,
/// seconds), and the ground's dust kicked up round the burst (count, start and
/// end reach, start and end height, start and end size, opacity, seconds).
const VENT_PUFFS: usize = 3;
const VENT_ROLL: f64 = 1.2;
const VENT_HEIGHT: [f64; 2] = [0.35, 1.5];
const VENT_SIZE: [f64; 2] = [0.4, 1.0];
const VENT_OPACITY: f64 = 0.55;
const VENT_SECONDS: f64 = 1.4;
const VENT_DUST_PUFFS: usize = 5;
const VENT_DUST_REACH: [f64; 2] = [0.15, 1.1];
const VENT_DUST_HEIGHT: [f64; 2] = [0.2, 0.75];
const VENT_DUST_SIZE: [f64; 2] = [0.45, 1.05];
const VENT_DUST_OPACITY: f64 = 0.62;
const VENT_DUST_SECONDS: f64 = 1.05;
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
    /// Its column's fixed shape and the puffs it is giving off.
    column: Column,
}

/// What a drop's column keeps between frames: its grenade, its own phase and height,
/// and what each puff took from the moment it left the grenade. A puff's gush,
/// meander, stray and size jitter depend only on when it was emitted, so they are
/// worked out once per climb rather than every frame (the transcendentals were most
/// of the column's cost).
#[derive(Clone, Copy, Debug)]
struct Column {
    /// The pallet position the grenade was placed for.
    at: [f64; 2],
    base: Vec3,
    seed: f64,
    /// The climb before the fizzle's strength scales it.
    rise: f64,
    puffs: [Emission; SIGNAL_PUFFS],
}

impl Column {
    fn new(id: u32, x: f64, z: f64) -> Self {
        let seed = hash(f64::from(id));
        Self {
            at: [x, z],
            base: grenade(x, z, supply_turn(id)),
            seed,
            rise: SIGNAL_RISE
                * (1.0 + SIGNAL_RISE_SPREAD * (2.0 * hash(f64::from(id) + 7.3) - 1.0)),
            puffs: [Emission::default(); SIGNAL_PUFFS],
        }
    }
}

/// The powers that shape a puff's climb (slowing, lean onset, widening and fading),
/// tabulated: four of them for every puff of every column each frame.
struct EaseTables([EaseTable; 4]);

/// The fading power of the column's opacity toward its top: dense where it leaves
/// the grenade, then thinning faster than it climbs, as smoke diluting into air does.
const SIGNAL_FADING: f64 = 1.8;

/// Build the columns' ease tables now rather than on the frame the first column shows.
pub fn warm_tables() {
    ease_tables();
}

fn ease_tables() -> &'static EaseTables {
    static TABLES: std::sync::OnceLock<EaseTables> = std::sync::OnceLock::new();
    TABLES.get_or_init(|| {
        EaseTables(
            [
                SIGNAL_SLOWING,
                SIGNAL_SHEAR_ONSET,
                SIGNAL_WIDENING,
                SIGNAL_FADING,
            ]
            .map(EaseTable::new),
        )
    })
}

/// One puff's draws from its emission time, valid for the climb numbered `climb`.
#[derive(Clone, Copy, Debug)]
struct Emission {
    climb: f64,
    gush: f64,
    meander: f64,
    stray: f64,
    shrink: f64,
    seed: f32,
}

impl Default for Emission {
    fn default() -> Self {
        Self {
            climb: f64::NAN,
            gush: 0.0,
            meander: 0.0,
            stray: 0.0,
            shrink: 0.0,
            seed: 0.0,
        }
    }
}

impl Emission {
    fn new(climb: f64, emitted: f64, seed: f64) -> Self {
        Self {
            climb,
            gush: 1.0 - SIGNAL_GUSH * 0.5 * (1.0 + (emitted * SIGNAL_GUSH_RATE + seed * TAU).sin()),
            meander: (emitted * SIGNAL_MEANDER_RATE + seed * 11.0).sin(),
            stray: hash(emitted * 3.7 + seed) - 0.5,
            shrink: hash(emitted * 5.1 + seed),
            seed: (seed * 3.1 + emitted * 0.37).fract() as f32,
        }
    }
}

/// Every puff of a column lies within this far of the point `COLUMN_MIDDLE` over its
/// grenade: the highest climb, the lean and the stray of the widest puff, its card
/// included, with room to spare.
const COLUMN_REACH: f32 = 4.0;
const COLUMN_MIDDLE: f32 = 1.6;

#[derive(Clone, Debug)]
pub struct PickupEffects {
    effects: VecDeque<PickupEffect>,
    collections: VecDeque<Collection>,
    supplies: Vec<Supply>,
    /// The puffs this frame's columns wrote into the pool.
    columns: std::ops::Range<usize>,
}

impl Default for PickupEffects {
    fn default() -> Self {
        Self {
            effects: VecDeque::with_capacity(MAX_PICKUP_EFFECTS),
            collections: VecDeque::with_capacity(MAX_PICKUP_EFFECTS),
            supplies: Vec::new(),
            columns: 0..0,
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

    /// Where this frame's columns lie in the puff pool, as written (before the pool
    /// is sorted): the drops' markers, which the sight line to the player's tank
    /// leaves alone. A thin wisp never hid a hull, and thinned it blinked out as a
    /// tank drove up to the drop it marks.
    pub fn columns(&self) -> std::ops::Range<usize> {
        self.columns.clone()
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

    /// The puff of `color` smoke a drop's grenade vents as a hull crushes it, and
    /// the dust its load kicks up bursting out, at (x, z) beside the hull, rolling
    /// out toward `away` (a ground direction from the hull).
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
    /// into `puffs`, after the blasts have rebuilt it this frame. Columns no view
    /// shows are left out.
    pub fn draw(&mut self, state: &RenderState, view: &EffectView, puffs: &mut PoolBuffer) {
        self.supplies
            .retain(|supply| state.pickups.iter().any(|pickup| pickup.id == supply.id));
        let first = puffs.len();
        for pickup in &state.pickups {
            // An empty drop's grenade relights when its restock is `FIZZLE_FROM`
            // along: from then (when the cooldown is known), and how strongly.
            let relit = (!pickup.available && pickup.cooldown_duration > 0.0).then(|| {
                let progress = 1.0 - pickup.cooldown / pickup.cooldown_duration;
                let lit = state.elapsed - (progress - FIZZLE_FROM) * pickup.cooldown_duration;
                let near = smoothstep(progress, FIZZLE_FROM, 1.0);
                (lit, FIZZLE[0] + (FIZZLE[1] - FIZZLE[0]) * near)
            });
            let at = match self.supplies.iter().position(|s| s.id == pickup.id) {
                Some(at) => {
                    let supply = &mut self.supplies[at];
                    if supply.stocked != pickup.available {
                        supply.stocked = pickup.available;
                        // A restocked grenade carries on from its fizzle, if it was
                        // seen relit; otherwise its column starts afresh.
                        supply.since = supply.relit.min(state.elapsed);
                    }
                    supply.relit = relit.map_or(f64::INFINITY, |(lit, _)| lit);
                    if supply.column.at != [pickup.x, pickup.z] {
                        supply.column = Column::new(pickup.id, pickup.x, pickup.z);
                    }
                    at
                }
                None => {
                    self.supplies.push(Supply {
                        id: pickup.id,
                        stocked: pickup.available,
                        since: f64::NEG_INFINITY,
                        relit: f64::INFINITY,
                        column: Column::new(pickup.id, pickup.x, pickup.z),
                    });
                    self.supplies.len() - 1
                }
            };
            let supply = &mut self.supplies[at];
            let since = supply.since;
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
            // The smoke given off before the grenade went out climbs on until the last
            // of it has thinned away at the top.
            if state.elapsed - taken > SIGNAL_CYCLE {
                continue;
            }
            let bounds = Sphere {
                center: supply.column.base + Vec3::new(0.0, COLUMN_MIDDLE, 0.0),
                radius: COLUMN_REACH,
            };
            if !view.sees(&bounds) {
                continue;
            }
            let signal = Signal {
                color: pickup_signal(pickup.kind),
                lit,
                taken,
                strength,
            };
            column(&mut supply.column, signal, state.elapsed, puffs);
        }
        self.columns = first..puffs.len();
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
/// moves as one body and never trembles; once the supply is taken no more leave the
/// grenade, and those in the air finish their climb.
fn column(column: &mut Column, signal: Signal, time: f64, puffs: &mut PoolBuffer) {
    let tint = hex_to_linear(signal.color);
    let (base, seed) = (column.base, column.seed);
    let rise = column.rise * signal.strength.sqrt();
    let [wx, wz] = SIGNAL_WIND;
    for (j, emission) in column.puffs.iter_mut().enumerate() {
        let cycle = time / SIGNAL_CYCLE + j as f64 / SIGNAL_PUFFS as f64 + seed;
        let a = cycle.fract();
        // When this puff left the grenade; constant over its climb.
        let emitted = time - a * SIGNAL_CYCLE;
        if emitted < signal.lit || emitted > signal.taken {
            continue;
        }
        let climb = cycle - a;
        if emission.climb != climb {
            *emission = Emission::new(climb, emitted, seed);
        }
        let gush = emission.gush;
        let [slowing, onset, widening, fading] = &ease_tables().0;
        let height = rise * (1.0 - slowing.at(1.0 - a));
        let shear = SIGNAL_SHEAR * onset.at(a);
        // Across the wind: the slow meander of the whole column, and each puff's
        // own stray as it breaks up.
        let stray = SIGNAL_STRAY * emission.stray * 2.0 * a * a;
        let meander = SIGNAL_MEANDER * a * emission.meander + stray;
        let position = base
            + Vec3::new(
                (wx * shear - wz * meander) as f32,
                height as f32,
                (wz * shear + wx * meander) as f32,
            );
        let size = (SIGNAL_SIZE[0] + (SIGNAL_SIZE[1] - SIGNAL_SIZE[0]) * widening.at(a))
            * (0.88 + 0.12 * gush)
            * (0.55 + 0.45 * signal.strength)
            * (1.0 - 0.25 * smoothstep(a, 0.6, 1.0) * emission.shrink);
        let opacity =
            SIGNAL_OPACITY * signal.strength * gush * smoothstep(a, 0.0, 0.04) * fading.at(1.0 - a);
        let thinning = smoothstep(a, SIGNAL_SATURATED, 1.0);
        let grey = SIGNAL_GREYING * thinning as f32;
        let [bright, dim] = SIGNAL_BRIGHTNESS;
        let brightness = bright + (dim - bright) * thinning as f32;
        let erosion = SIGNAL_EROSION[0] + (SIGNAL_EROSION[1] - SIGNAL_EROSION[0]) * thinning;
        let softness = SIGNAL_SOFTNESS[0] + (SIGNAL_SOFTNESS[1] - SIGNAL_SOFTNESS[0]) * thinning;
        smoke_puff(
            puffs,
            position,
            size as f32,
            mix(tint, SMOKE_GREY, grey).map(|c| c * brightness),
            opacity as f32,
            emission.seed,
            0.0,
            erosion as f32,
            softness as f32,
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
        let reach = VENT_ROLL * roll * (0.7 + 0.5 * hash(seed + j as f64 * 3.1));
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
        let tint = mix(vent.color, SMOKE_GREY, (0.1 + 0.45 * progress) as f32)
            .map(|c| c * SIGNAL_BRIGHTNESS[0]);
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
    let middle = (VENT_DUST_PUFFS as f64 - 1.0) / 2.0;
    for j in 0..VENT_DUST_PUFFS {
        // Fanned out round the burst on the side away from the hull.
        let bearing = (j as f64 - middle) * 0.62 + (seed - 0.5) * 0.5;
        let (sin, cos) = bearing.sin_cos();
        let [dx, dz] = [ax * cos - az * sin, ax * sin + az * cos];
        let reach = (VENT_DUST_REACH[0] + (VENT_DUST_REACH[1] - VENT_DUST_REACH[0]) * spread)
            * (0.75 + 0.5 * hash(seed * 9.0 + j as f64));
        let height = VENT_DUST_HEIGHT[0] + (VENT_DUST_HEIGHT[1] - VENT_DUST_HEIGHT[0]) * spread;
        let position = origin + Vec3::new((dx * reach) as f32, height as f32, (dz * reach) as f32);
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
    use crate::camera::PerspectiveCamera;
    use sloppy_core::sim::PickupKind;
    use sloppy_core::sim::types::Pickup;

    fn stocked(id: u32, x: f64, z: f64) -> Pickup {
        Pickup {
            id,
            kind: PickupKind::Speed,
            x,
            z,
            available: true,
            cooldown: 0.0,
            cooldown_duration: 0.0,
        }
    }

    #[test]
    fn a_column_drawn_every_frame_matches_one_drawn_afresh() {
        // Each puff keeps what it took from its emission for its whole climb.
        let mut state = RenderState::default();
        state.pickups.push(stocked(4, 3.0, -7.0));
        state.pickups.push(stocked(9, -12.0, 5.0));
        let mut running = PickupEffects::default();
        let mut puffs = PoolBuffer::new(1024);
        for frame in 0..600 {
            state.elapsed = 30.0 + f64::from(frame) / 60.0;
            puffs.clear();
            running.draw(&state, &EffectView::default(), &mut puffs);
        }
        let mut fresh = PoolBuffer::new(1024);
        PickupEffects::default().draw(&state, &EffectView::default(), &mut fresh);
        assert_eq!(puffs.len(), fresh.len());
        assert!(puffs.len() > SIGNAL_PUFFS);
        for (a, b) in puffs.records().iter().zip(fresh.records()) {
            assert!(a.world().abs_diff_eq(b.world(), 1e-5));
            for (x, y) in a
                .tint
                .iter()
                .chain(&a.data)
                .zip(b.tint.iter().chain(&b.data))
            {
                assert!((x - y).abs() < 1e-5, "{x} {y}");
            }
        }
    }

    #[test]
    fn the_climb_tables_follow_their_powers() {
        let tables = ease_tables();
        for (table, power) in tables.0.iter().zip([
            SIGNAL_SLOWING,
            SIGNAL_SHEAR_ONSET,
            SIGNAL_WIDENING,
            SIGNAL_FADING,
        ]) {
            for step in 0..=4000 {
                let x = f64::from(step) / 4000.0;
                let error = (table.at(x) - x.powf(power)).abs();
                // A puff fades in over the first 4% of its climb, and is not drawn
                // at all for the first half percent.
                let bound = match x {
                    x if x < 0.005 => 1e-2,
                    x if x < 0.04 => 1e-3,
                    _ => 1e-5,
                };
                assert!(error < bound, "{power} at {x}: {error}");
            }
        }
    }

    #[test]
    fn columns_no_view_shows_are_left_out() {
        let mut state = RenderState {
            elapsed: 20.0,
            ..Default::default()
        };
        state.pickups.push(stocked(3, 0.0, 0.0));
        let draw = |view: &EffectView| {
            let mut puffs = PoolBuffer::new(64);
            PickupEffects::default().draw(&state, view, &mut puffs);
            puffs.len()
        };
        let full = draw(&EffectView::default());
        assert!(full > 0);
        let mut camera = PerspectiveCamera::new(20.0, 0.1, 500.0);
        // Steeply down at the ground beyond the drop, which stands above the view.
        camera.look_at(Vec3::new(0.0, 10.0, 50.0), Vec3::new(0.0, -40.0, 0.0));
        assert_eq!(draw(&EffectView::new(&camera.view_projection(), None)), 0);
        // Water below mirrors the column into the view.
        let mirrored = EffectView::new(&camera.view_projection(), Some(-15.0));
        assert_eq!(draw(&mirrored), full);
        camera.look_at(Vec3::new(0.0, 30.0, 25.0), Vec3::ZERO);
        assert_eq!(
            draw(&EffectView::new(&camera.view_projection(), None)),
            full
        );
    }

    #[test]
    fn stocked_pallets_smoke_until_taken_and_promotions_pop() {
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
        effects.draw(&state, &EffectView::default(), &mut puffs);
        let stocked = puffs.len();
        assert!(
            stocked > SIGNAL_PUFFS / 2 && stocked <= SIGNAL_PUFFS,
            "a full column over a drop first seen stocked"
        );
        // Just taken: the smoke already given off is still there.
        state.pickups[0].available = false;
        state.pickups[0].cooldown_duration = 120.0;
        state.pickups[0].cooldown = 119.9;
        puffs.clear();
        effects.draw(&state, &EffectView::default(), &mut puffs);
        assert!(
            !puffs.is_empty() && puffs.len() <= stocked,
            "the last smoke"
        );
        // A second on, none has left the grenade since: the rest climbs on clear of it.
        let grenade_top = (SMOKE_MARKER[1] * SUPPLY_SCALE) as f32;
        state.pickups[0].cooldown = 119.0;
        puffs.clear();
        effects.draw(&state, &EffectView::default(), &mut puffs);
        assert!(!puffs.is_empty(), "the smoke in the air finishes its climb");
        assert!(
            puffs
                .records()
                .iter()
                .all(|puff| puff.translation().y > grenade_top + 0.3),
            "no smoke leaves the grenade once it is out"
        );
        // Once that has thinned away, before it smoulders again: nothing over the
        // empty pallet.
        state.pickups[0].cooldown = 120.0 - (SIGNAL_CYCLE + FIZZLE_FROM * 120.0) / 2.0;
        puffs.clear();
        effects.draw(&state, &EffectView::default(), &mut puffs);
        assert!(puffs.is_empty(), "the grenade goes out for a breath");
        // Restocked: the column grows from the grenade.
        state.pickups[0].available = true;
        puffs.clear();
        effects.draw(&state, &EffectView::default(), &mut puffs);
        assert!(puffs.len() <= 1, "a relit grenade starts its column afresh");
        state.elapsed += SIGNAL_CYCLE / 2.0;
        puffs.clear();
        effects.draw(&state, &EffectView::default(), &mut puffs);
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
        effects.draw(&state, &EffectView::default(), &mut puffs);
        assert_eq!(puffs.len(), MAX_PICKUP_EFFECTS * POP_PUFFS);
        assert!(
            puffs.records()[0].translation().y > POP_HEIGHT as f32,
            "the pop hangs over the tank"
        );
        effects.update(PICKUP_SECONDS);
        assert!(effects.is_empty());
    }

    #[test]
    fn an_empty_grenade_smoulders_until_its_restock_and_a_crushed_one_vents() {
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
        effects.draw(&state, &EffectView::default(), &mut puffs);
        let faint = |puffs: &PoolBuffer| puffs.records().iter().map(|p| p.tint[3]).sum::<f32>();
        let early = faint(&puffs);
        assert!(!puffs.is_empty(), "a faint wisp through the cooldown");
        // Near its restock the wisp has grown.
        state.elapsed += 9.0;
        state.pickups[0].cooldown = 1.0;
        puffs.clear();
        effects.draw(&state, &EffectView::default(), &mut puffs);
        let fizzle = puffs.len();
        assert!(
            fizzle > 2 && faint(&puffs) > early,
            "a stronger wisp near the restock"
        );
        // Restocked, the column carries on from the fizzle instead of restarting.
        state.elapsed += 1.0;
        state.pickups[0].available = true;
        puffs.clear();
        effects.draw(&state, &EffectView::default(), &mut puffs);
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
