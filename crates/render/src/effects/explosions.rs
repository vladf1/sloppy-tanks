//! Analytic, pooled cosmetic blasts (`explosion-effects.ts`), layered like a
//! film explosion: a white flash core, a fireball of boiling puffs that burns out
//! from its rim while separate smoke rolls off its top, rises on its own heat,
//! spreads, thins and drifts on the wind, a skirt of the ground's own dust racing
//! out low and a lingering haze; on the ground a fire glow, a hot shockwave, a
//! dusty ring and a scorch whose ember rim cools and fades over many seconds.
//! Each blast also lights its surroundings (`lights`). Shell hits raise a small
//! flash, a star of glints and a puff of smoke or grit in the same slots, and
//! cargo crates raise wood dust: a puff from a struck face, a rolling cloud when
//! one bursts. Every puff, ring and scorch is rebuilt each frame into three
//! shared instanced draws; no textures or physics bodies.

use std::f64::consts::{PI, TAU};

use glam::{Mat4, Vec3};
use sloppy_core::geometry::math::smoothstep;
use sloppy_core::sim::render_state::RenderFragment;
use sloppy_core::sim::{CoverKind, DeathStyle, SimEvent, SimEventType};

use super::Flash;
use super::ground::Ground;
use super::pool::{PoolBuffer, record};
use super::random::CosmeticRandom;
use super::sparks::{flame, glow, ground_glow};

use super::spawn_pad_decks::SpawnPadDecks;
use super::walls::{Strike, WallMaterial, Walls};
use super::wood_chips::{CRATE_HEIGHT, HIT_HEIGHT};
use crate::color::hex_to_linear;

pub const MAX_EXPLOSIONS: usize = 40;
/// A plain blast's lifetime; tank and drum blasts take their look's.
pub const EXPLOSION_LIFETIME: f64 = 2.8;
pub const PUFFS_PER_BLAST: usize = 34;
/// Scorches left on the ground at once; the oldest makes room.
pub const MAX_SCORCHES: usize = 40;
/// Marks shells leave on hard walls at once; the oldest makes room.
pub const MAX_WALL_MARKS: usize = 24;
/// A wall mark lasts this long, fading over its last part (s).
const WALL_MARK_LIFETIME: f64 = 40.0;
/// A mark's diameter (m), never more than its face has room for, and how far it
/// stands off the face (a container's ribs stand proud of its skin).
const WALL_MARK_SIZE: f64 = 2.4;
const WALL_MARK_STANDOFF: [f32; 2] = [0.035, 0.16];
/// Ground decals a frame may hold: a dust ring and a shockwave per blast, every
/// scorch and every wall mark.
pub const DECAL_CAPACITY: usize = MAX_EXPLOSIONS * 2 + MAX_SCORCHES + MAX_WALL_MARKS;
/// A scorch cools and then fades out over this many seconds.
pub const SCORCH_LIFETIME: f64 = 45.0;
/// A burnout's thin smoke outlasts a normal blast.
const BURNOUT_LIFETIME: f64 = 3.6;
/// A burnout's fireball against a cook-off's: the hull holds most of it in.
const BURNOUT_FIRE_SCALE: f64 = 0.72;
/// Billows of fire and smoke a burnout rolls up off the hull.
const BURNOUT_BILLOWS: usize = 4;
/// A dust collapse (timber) and a shell hit's puff.
const COLLAPSE_LIFETIME: f64 = 3.2;
const COLLAPSE_DUST: f64 = 1.35;
/// A cottage's collapse dust: how long it hangs, when its walls start to strike
/// the ground (s after the break) and over how long they go on landing, how many
/// small bursts the landings raise round the footprint, how far (m) from its
/// centre they burst and roll out, and the puffs of the thin column rising off
/// the heap. Each burst is small and short, so the falling walls and roof stay
/// in sight through the dust instead of behind one cloud.
const HOUSE_DUST_LIFETIME: f64 = 7.0;
const HOUSE_IMPACT: f64 = 0.45;
const HOUSE_LANDINGS: f64 = 1.1;
const HOUSE_BILLOWS: usize = 12;
const HOUSE_FOOT: f64 = 2.7;
const HOUSE_ROLL: f64 = 1.5;
const HOUSE_PALL: usize = 3;
/// The ground surge the roof drives out as it lands on the wreck: when it lands
/// (s after the break) and over how long its slabs go on landing, its billows, how
/// fast (per s) they roll out and how far (m) past the footprint, and how high
/// (m) they climb as they thin. Lumpy and darker at the foot, it rises and thins
/// over about a second and a half, so the pile shows through it.
const SURGE_AT: (f64, f64) = (0.55, 0.35);
const SURGE_BILLOWS: usize = 14;
const SURGE_RATE: f64 = 2.6;
const SURGE_REACH: f64 = 3.0;
const SURGE_RISE: f64 = 0.8;
/// How squat the surge's billows are against a plain puff: the ones along the
/// ground and the ones that climb off it, so the skirt hugs the ground as a low,
/// lumpy roll rather than a ring of round blobs.
const SURGE_SQUAT: (f64, f64) = (0.5, 0.8);
/// A collapsed cottage's footprint stains: the churned earth under the heap (two
/// overlapping, m across, and how far apart), and the lobes of plaster dust round
/// it, overlapping into a ragged ring: how far (m) from its centre they lie and
/// how wide they are.
const FOOTPRINT_CHURN: (f64, f64) = (11.0, 1.2);
const FOOTPRINT_LOBES: usize = 5;
const FOOTPRINT_REACH: f64 = 4.2;
const FOOTPRINT_LOBE: f64 = 7.5;
/// Darker puffs of plaster and soot out of the pile as the roof lands on it.
const HOUSE_CORE: usize = 3;
/// Plaster and mortar dust, paler than the ground's.
const PLASTER_DUST: u32 = 0xb9ab95;
const HIT_LIFETIME: f64 = 1.1;
/// Grit off a wall hangs and settles longer than a hull's puff of smoke.
const STONE_HIT_LIFETIME: f64 = 3.0;
/// A holed container smokes from its breach a while after the hit: how long,
/// and the wisps cycling up out of it (s per trip).
const SHEET_HIT_LIFETIME: f64 = 3.2;
const BREACH_WISPS: usize = 3;
const WISP_CYCLE: f64 = 1.4;
/// Dust rings show for the first part of a blast only; the shockwave for less.
const RING_SECONDS: f64 = 0.75;
const SHOCK_SECONDS: f64 = 0.42;
const RING_HEIGHT: f64 = 0.07;
const SCORCH_HEIGHT: f64 = 0.062;
/// A ring spreads from `RING_START` to `RING_START + RING_SPREAD` metres (times
/// the blast scale); it clears every spawn pad deck it can reach.
const RING_START: f64 = 0.65;
const RING_SPREAD: f64 = 3.6;
const SHOCK_START: f64 = 0.6;
const SHOCK_SPREAD: f64 = 5.4;
/// The flash core: how long it burns (s), its diameter (m, times the scale) and
/// the wider orange bloom around it.
/// Kept small and below white so it never blows out a hull beside it.
const FLASH_SECONDS: f64 = 0.06;
const FLASH_SIZE: f64 = 2.2;
const FLASH_COLOR: [f32; 3] = [3.4, 2.9, 2.2];
/// Heat every fireball lobe adds over its first frames, falling away with this
/// time constant (s): the blast is white-hot before it is orange.
const FLASH_HEAT: f64 = 1.3;
const FLASH_HEAT_SECONDS: f64 = 0.04;
/// The flash's light on the ground round the blast: diameter (m, times the
/// scale) and HDR colour, gone within a few frames.
const FLASH_GROUND_SIZE: f64 = 10.0;
const FLASH_GROUND_COLOR: [f32; 3] = [1.6, 1.25, 0.8];
/// The white-hot seams a vehicle's flash shows round its hull for its first
/// frames: each glow's diameter (m, times the scale) and colour.
const SEAM_FLASH_SECONDS: f64 = 0.08;
const SEAM_FLASH_SIZE: f64 = 1.3;
const SEAM_FLASH_COLOR: [f32; 3] = [3.2, 2.6, 1.7];
const BLOOM_SECONDS: f64 = 0.15;
const BLOOM_SIZE: f64 = 3.4;
const BLOOM_COLOR: [f32; 3] = [1.8, 0.62, 0.12];
/// Puffs are slightly flattened.
const PUFF_HEIGHT: f64 = 0.9;
/// The hottest a fireball puff starts (the puff shader's heat).
const FIRE_HEAT: f64 = 1.6;
/// Heat falls by this factor of e over a look's `fire_life`.
const COOLING: f64 = 2.8;
/// Upward acceleration of a blast's soot once its fire is out (m/s²).
const SOOT_BUOYANCY: f64 = 5.0;
/// A blast's soot thins away over this many seconds once its fire is out.
const SOOT_SECONDS: f64 = 1.7;
/// A fire puff burns out (thins to nothing from its rim) as its heat falls
/// through this range.
const BURN_OUT: [f64; 2] = [0.1, 0.42];
const FIRE_PUFFS: usize = 12;
const SMOKE_PUFFS: usize = 6;
/// A vehicle's fireball bursts out of its hull's seams and hatches, round its
/// middle rather than over it, so the hull stays in sight inside the blast:
/// the ring its seam lobes start on (m, times the scale) and how high on the
/// hull, and where its hatch lobes start, over the deck.
const HULL_SEAMS: f64 = 1.3;
const HULL_SEAM_HEIGHT: f64 = 0.45;
const HULL_HATCH_HEIGHT: f64 = 1.35;
/// How high a vehicle's hatch lobes jet, as a share of the eruption's column: a
/// burst out of the hatches, never a pillar standing on the hull.
const HULL_COLUMN: f64 = 0.7;
/// A blast on open ground bursts from a fist-sized heart low over the ground,
/// its lobes bigger and closer packed than a vehicle's: one rolling ball with
/// a broken rim rather than a ring of pops.
const GROUND_SEAMS: f64 = 0.4;
const GROUND_LOBES: [f64; 2] = [0.8, 1.35];
/// How high a ground blast's middle jets, against a vehicle's.
const GROUND_COLUMN: f64 = 0.75;
const GROUND_SEAM_HEIGHT: f64 = 0.3;
const GROUND_HATCH_HEIGHT: f64 = 0.55;
const SKIRT_PUFFS: usize = 7;
/// The wind every blast's and wreck's smoke drifts on (m/s).
pub const WIND: [f64; 2] = [0.9, -0.35];
/// The light a blast throws on everything around it: peak intensity (times its
/// scale squared), reach (m) and height over the blast (m, times its scale).
const BLAST_LIGHT: f64 = 34.0;
pub const BLAST_LIGHT_DISTANCE: f32 = 15.0;
/// The light hangs high over the fire, so a hull beside or under it is lit
/// warm, not blown out.
const LIGHT_HEIGHT: f64 = 4.2;
const LIGHT_LIFT: f64 = 0.8;
/// A shell hit's brief light.
const HIT_LIGHT: f64 = 26.0;
/// The light's color at the white flash and through the fire.
const FLASH_LIGHT_COLOR: u32 = 0xffe4c8;
const FIRE_LIGHT_COLOR: u32 = 0xffa465;
/// The fire's additive light on the ground: diameter (m, times the scale) and
/// HDR color.
const GROUND_GLOW_SIZE: f64 = 7.0;
const GROUND_GLOW_COLOR: [f32; 3] = [0.95, 0.5, 0.16];
/// A drum's spilled fuel burns in this many licks between these times (s).
const FUEL_FLAMES: usize = 4;
/// Balls of fire rolling up off the burning fuel, each this long a climb (s);
/// they use the puff slots a tank's turret jet would.
const FUEL_BALLS: usize = 3;
const FUEL_BALL_CYCLE: f64 = 0.9;
const FUEL_FIRE: [f64; 2] = [0.25, 2.6];
/// The oily plume the burning fuel feeds: puffs cycling up it, a puff's trip
/// (s) and the plume's height (m, times the blast's scale).
const FUEL_PLUME: usize = 5;
const FUEL_PLUME_CYCLE: f64 = 2.6;
const FUEL_PLUME_HEIGHT: f64 = 5.5;
/// A tank's cook-off jet: how long its tongue of flame roars up, and its color.
const JET_SECONDS: f64 = 0.45;
const JET_COLOR: [f32; 3] = [1.3, 1.1, 1.0];
/// The decal shader's kinds (`blast_ring.wgsl`).
const DUST_RING: f32 = 0.0;
const SHOCK_RING: f32 = 1.0;
const SCORCH: f32 = 2.0;
const STONE_MARK: f32 = 3.0;
const STEEL_MARK: f32 = 4.0;
const POWDER: f32 = 5.0;
/// A concrete hit's powder stain: how far out from the face it lies and how
/// wide it is (m).
const POWDER_REACH: f64 = 0.55;
const POWDER_SIZE: f64 = 2.0;
/// Soot off a shell striking a steel wall.
const STEEL_SOOT: u32 = 0x3d3a37;

const SMOKE: u32 = 0x76787c;
const TANK_SMOKE: u32 = 0x57534f;
const DUST: u32 = 0xaa9273;
/// A struck tank's oily smoke and a struck wall's grit.
const HIT_SMOKE: u32 = 0x55524f;
const GRIT: u32 = 0xb4ada2;
/// Crushed concrete: a pale powder, paler than the wall's weathered skin.
const CONCRETE_POWDER: u32 = 0xd9d5cc;
/// A hit's white-hot point and its bloom (linear color and opacity): warm on
/// steel, a cold white crack of light on concrete.
const HIT_FLASH: [f32; 4] = [2.6, 1.7, 0.75, 1.0];
const HIT_BLOOM: [f32; 4] = [1.8, 0.65, 0.14, 0.4];
const STONE_FLASH: [f32; 4] = [2.5, 2.3, 2.0, 0.9];
const STONE_BLOOM: [f32; 4] = [1.1, 0.95, 0.75, 0.2];

/// Dust a heavy piece of debris kicks up where it lands: how long it hangs,
/// the contact force (N) below which a landing raises none and at which it
/// raises the most, how low (m) the piece's centre must be to be landing rather
/// than knocking against other debris in the air, and how soon (s) the same piece
/// or any piece at all may raise dust again.
const LANDING_LIFETIME: f64 = 1.7;
const LANDING_FORCE: [f64; 2] = [140.0, 2500.0];
const LANDING_HEIGHT: f64 = 2.4;
const LANDING_REPEAT: f64 = 4.0;
const LANDING_GAP: f64 = 0.06;
/// A landing's dust skirt scale: [lightest, added for the hardest].
const LANDING_DUST: [f64; 2] = [0.45, 0.85];
/// The dust a felled tree's snap shakes up round its stump (a landing's scale).
const TREE_SNAP_DUST: f64 = 0.55;
/// Pieces remembered for `LANDING_REPEAT`.
const LANDING_MEMORY: usize = 24;
/// A felled trunk has landed once its axis tilts this low (its up's height),
/// and dust bursts at these shares of its half-length from its middle (the
/// crown end positive), at this scale. The last trunks that did are kept.
const FELLED_TILT: f64 = 0.3;
const FELLED_DUST: [f64; 3] = [-0.3, 0.45, 1.05];
const FELLED_DUST_SCALE: f64 = 0.85;
const FELLED_MEMORY: usize = 8;
/// About where a trunk breaks off its stump (m).
const TRUNK_STUMP: f64 = 0.4;

/// Wood dust: a struck face's puff and a burst crate's cloud last this long.
const WOOD_HIT_LIFETIME: f64 = 0.9;
const WOOD_BURST_LIFETIME: f64 = 3.6;
/// Warm sawdust rolling low, and the paler dust that rises and blows out.
const SAWDUST: u32 = 0xbda483;
const SAWDUST_LIGHT: u32 = 0xcfbc9d;
/// A burst's ring spreads at this scale; its cloud scales with the crate's height.
const WOOD_RING_SCALE: f64 = 0.6;
/// Low billows of sawdust that settle round a burst crate and linger.
const SETTLING_DUST: usize = 4;
const SETTLING_BLOWN: f64 = 1.2;

/// The dust a cargo crate raises.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WoodDust {
    /// A puff of sawdust blown out of a struck face, back toward the gun, or on
    /// with the shell from a loose board it burst.
    Hit,
    /// A crate bursting apart: a cloud rolling out low, a jet blowing on along the
    /// killing shot and a plume rising over the wreck.
    Burst,
}

/// What a shell struck, for its hit puff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Struck {
    /// A tank's armour: a flash and a gout of dark smoke.
    Steel,
    /// A wall, rock or building: a flash and a puff of grit.
    Stone,
    /// A container's sheet steel: a flash, a shower of sparks and a little soot.
    Sheet,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Kind {
    #[default]
    Blast,
    Tank,
    Barrel,
    Burnout,
    /// A collapse without fire (timber): dust only.
    Collapse,
    /// A cottage coming down: a burst at its foot, then the dust its roof and walls
    /// drive out as they hit, rolling out low round the footprint and hanging over
    /// the heap for seconds.
    HouseFall,
    Wood(WoodDust),
    Hit(Struck),
    /// Heavy debris slamming down: a skirt of the ground's dust.
    Landing,
}

/// How a fireball erupts: lobes bursting out round its source ("seams") and the
/// rest jetting up out of its middle ("hatches"), some of them a beat late.
#[derive(Clone, Copy, Debug)]
struct Shape {
    seams: usize,
    /// The arc the seam lobes span (rad): all round, or out of one side.
    arc: f64,
    /// How far the seam lobes fly out and how fast they rise (times the scale).
    reach: f64,
    rise: f64,
    /// How high the middle lobes jet.
    column: f64,
    /// Lobes from this index burst this many seconds late: a second pop.
    late_from: usize,
    late: f64,
    size: f64,
}

/// Four eruptions, picked at random and never twice running: a ring of fire
/// out of every seam, a flank blown out of one side with a pop on the other, a
/// geyser up the middle, and a small burst swallowed by a bigger second one.
const SHAPES: [Shape; 4] = [
    Shape {
        seams: 8,
        arc: TAU,
        reach: 1.0,
        rise: 0.8,
        column: 1.0,
        late_from: FIRE_PUFFS,
        late: 0.0,
        size: 1.0,
    },
    Shape {
        seams: 8,
        arc: 2.5,
        reach: 1.4,
        rise: 0.7,
        column: 0.8,
        late_from: 10,
        late: 0.13,
        size: 1.08,
    },
    Shape {
        seams: 5,
        arc: TAU,
        reach: 0.75,
        rise: 0.6,
        column: 1.9,
        late_from: FIRE_PUFFS,
        late: 0.0,
        size: 0.92,
    },
    Shape {
        seams: 7,
        arc: TAU,
        reach: 0.85,
        rise: 0.9,
        column: 1.25,
        late_from: 6,
        late: 0.11,
        size: 1.0,
    },
];

/// A fiery blast's silhouette and timing.
#[derive(Clone, Copy, Debug)]
struct Look {
    lifetime: f64,
    fire_size: f64,
    fire_spread: f64,
    /// Seconds over which the fireball cools into smoke.
    fire_life: f64,
    fire_rise: f64,
    smoke: u32,
    smoke_size: f64,
    smoke_rise: f64,
    /// Sideways drift of the whole cloud (m/s).
    drift: f64,
}

const BLAST: Look = Look {
    lifetime: EXPLOSION_LIFETIME,
    fire_size: 0.98,
    fire_spread: 0.85,
    fire_life: 0.5,
    fire_rise: 1.2,
    smoke: SMOKE,
    smoke_size: 0.92,
    smoke_rise: 2.6,
    drift: 0.35,
};

// Distinct silhouettes and timing: a tall column, a wide rolling cloud and a
// drifting plume, cycled so consecutive deaths differ. A kill's fireball is
// bigger than a hull; it bursts within a few frames, burns through the first
// half second and rolls over into a column of soot.
const TANK_BLASTS: [Look; 3] = [
    Look {
        lifetime: 4.2,
        fire_size: 1.15,
        fire_spread: 0.9,
        fire_life: 0.6,
        fire_rise: 1.8,
        smoke: TANK_SMOKE,
        smoke_size: 1.25,
        smoke_rise: 3.4,
        drift: 0.3,
    },
    Look {
        lifetime: 3.9,
        fire_size: 1.22,
        fire_spread: 1.12,
        fire_life: 0.56,
        fire_rise: 1.1,
        smoke: 0x5c554e,
        smoke_size: 1.3,
        smoke_rise: 2.8,
        drift: 0.25,
    },
    Look {
        lifetime: 4.0,
        fire_size: 1.1,
        fire_spread: 1.0,
        fire_life: 0.58,
        fire_rise: 1.4,
        smoke: 0x5a5856,
        smoke_size: 1.22,
        smoke_rise: 3.1,
        drift: 1.2,
    },
];

// A drum's fuel flashes over in a ball smaller than a tank's ammunition but well
// over the drum's own size, and its oily smoke lingers.
const BARREL_BLASTS: [Look; 3] = [
    Look {
        lifetime: 6.0,
        fire_size: 1.02,
        fire_spread: 0.76,
        fire_life: 0.5,
        fire_rise: 1.9,
        smoke: 0x4d4844,
        smoke_size: 1.1,
        smoke_rise: 3.0,
        drift: 0.3,
    },
    Look {
        lifetime: 6.0,
        fire_size: 0.97,
        fire_spread: 0.68,
        fire_life: 0.5,
        fire_rise: 2.1,
        smoke: 0x4a4542,
        smoke_size: 1.05,
        smoke_rise: 3.2,
        drift: 1.3,
    },
    Look {
        lifetime: 6.0,
        fire_size: 0.99,
        fire_spread: 0.72,
        fire_life: 0.5,
        fire_rise: 2.2,
        smoke: 0x524a44,
        smoke_size: 1.08,
        smoke_rise: 3.3,
        drift: 0.6,
    },
];

#[derive(Clone, Copy, Debug, Default)]
struct Blast {
    active: bool,
    kind: Kind,
    x: f64,
    z: f64,
    age: f64,
    scale: f64,
    phase: f64,
    variant: usize,
    /// The eruption (`SHAPES`) of a fiery blast.
    shape: usize,
    tempo: f64,
    /// Linear color of the ground's dust where it went off.
    dust: [f64; 3],
    /// A wood blast's or hit's height (the struck point, or the crate's top) and
    /// its unit horizontal direction (out of the struck face, or along the
    /// killing shot), zero when unknown.
    y: f64,
    direction: [f64; 2],
}

/// A shell's mark on a hard wall: a chipped crater in stone, a sooty dent with a
/// hot heart in steel.
#[derive(Clone, Copy, Debug, Default)]
struct WallMark {
    position: Vec3,
    normal: Vec3,
    diameter: f32,
    steel: bool,
    /// The wall's color, linear.
    color: [f32; 3],
    age: f64,
    phase: f64,
}

impl Blast {
    fn look(&self) -> Look {
        match self.kind {
            Kind::Tank | Kind::Burnout => TANK_BLASTS[self.variant],
            Kind::Barrel => BARREL_BLASTS[self.variant],
            _ => BLAST,
        }
    }

    fn lifetime(&self) -> f64 {
        match self.kind {
            Kind::Wood(WoodDust::Hit) => WOOD_HIT_LIFETIME,
            Kind::Wood(WoodDust::Burst) => WOOD_BURST_LIFETIME,
            Kind::Burnout => BURNOUT_LIFETIME,
            Kind::Collapse => COLLAPSE_LIFETIME,
            Kind::HouseFall => HOUSE_DUST_LIFETIME,
            Kind::Hit(Struck::Stone) => STONE_HIT_LIFETIME,
            Kind::Hit(Struck::Steel) => HIT_LIFETIME,
            Kind::Hit(Struck::Sheet) => SHEET_HIT_LIFETIME,
            Kind::Landing => LANDING_LIFETIME,
            Kind::Blast | Kind::Tank | Kind::Barrel => self.look().lifetime,
        }
    }

    /// How brightly the blast lights its surroundings `t` seconds in: the
    /// flash's spike, then the flickering fireball until it burns out.
    fn fire_light(&self, t: f64, look: &Look) -> f64 {
        // A crisp flash of a frame or two, then the fire's glow.
        let flash = (-t * 30.0).exp();
        let fire = burning(fire_heat(t, COOLING / look.fire_life));
        let flicker =
            0.85 + 0.15 * (t * 37.0 + self.phase * 5.0).sin() * (t * 23.0 + self.phase).sin();
        1.3 * flash + 0.6 * fire * flicker
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Scorch {
    x: f64,
    z: f64,
    y: f64,
    age: f64,
    diameter: f64,
    phase: f64,
    /// The ground's dust, for the rim of dirt the blast threw out; a powder
    /// stain's own powder.
    dust: [f64; 3],
    /// The decal's look: a blast's scorch, or the powder a concrete hit spills.
    kind: f32,
}

fn linear(hex: u32) -> [f64; 3] {
    hex_to_linear(hex).map(f64::from)
}

fn scaled(color: [f64; 3], factor: f64) -> [f64; 3] {
    color.map(|c| c * factor)
}

fn mixed(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// A burst drum's spilled fuel: a few low licks of flame on the ground round
/// it, catching just after the fireball and guttering out, with their glow.
fn fuel_fire(b: &Blast, t: f64, lifetime: f64, streaks: &mut PoolBuffer) {
    let fire = smoothstep(t, FUEL_FIRE[0], FUEL_FIRE[0] + 0.2)
        * (1.0 - smoothstep(t, FUEL_FIRE[1], lifetime * 0.95));
    if fire <= 0.01 {
        return;
    }
    let s = b.scale as f32;
    let up = Vec3::new(WIND[0] as f32 * 0.25, 1.0, WIND[1] as f32 * 0.25).normalize();
    for j in 0..FUEL_FLAMES {
        let jf = j as f64;
        let angle = b.phase + jf * 2.399;
        let reach = (0.25 + 0.55 * hash(b.phase + jf * 0.7)) * b.scale;
        let base = Vec3::new(
            (b.x + angle.cos() * reach) as f32,
            0.05,
            (b.z + angle.sin() * reach) as f32,
        );
        // One rate for every lick, each with its own phase.
        let breathe = (t * 7.0 + b.phase * 3.0 + jf * 2.1).sin() as f32;
        let size = (0.55 + 0.25 * hash(b.phase * 1.3 + jf)) as f32 * s;
        flame(
            streaks,
            base,
            (up + Vec3::new(0.12 * breathe, 0.0, -0.08 * breathe)).normalize(),
            size,
            size * (0.85 + 0.25 * breathe) * (0.5 + 0.5 * fire as f32),
            (b.phase * 0.17 + jf * 0.23).fract() as f32,
            [1.05, 1.0, 1.0, (fire * 0.65) as f32],
        );
    }
    let [r, g, bl] = GROUND_GLOW_COLOR;
    ground_glow(
        streaks,
        Vec3::new(b.x as f32, 0.08, b.z as f32),
        3.2 * s,
        [r, g, bl, (fire * 0.55) as f32],
    );
}

/// A plain blast's scale from its event's radius: a shell's interception stays a
/// pop, a rocket or a mine a fist of fire no bigger than a tank's own death.
pub fn blast_scale(size: f64) -> f64 {
    (size / 5.2).clamp(0.55, 1.1)
}

/// `1 - e^(-rate·age)`: fast at first, settling toward 1.
fn settle(age: f64, rate: f64) -> f64 {
    1.0 - (-age * rate).exp()
}

/// A repeatable 0..1 value for a puff, so every puff of a blast differs in size,
/// timing and reach without drawing cosmetic randomness each frame.
fn hash(value: f64) -> f64 {
    ((value * 12.9898 + 78.233).sin() * 43_758.545_3)
        .fract()
        .abs()
}

/// How hot a fireball puff burns `age` seconds in, with `rate` its cooling.
fn fire_heat(age: f64, rate: f64) -> f64 {
    FIRE_HEAT * (-age * rate).exp()
}

/// 1 while a puff at `heat` burns, falling to 0 as it burns out.
fn burning(heat: f64) -> f64 {
    smoothstep(heat, BURN_OUT[0], BURN_OUT[1])
}

/// One puff's look for the puff shader.
#[derive(Clone, Copy, Debug)]
struct Puff {
    position: [f64; 3],
    size: f64,
    color: [f64; 3],
    opacity: f64,
    seed: f64,
    heat: f64,
    erosion: f64,
    softness: f64,
    /// Fire below lighting the smoke's underside (0..1).
    underglow: f64,
}

/// The puff shader's underglow levels, packed above the seed's fraction.
const UNDERGLOW_LEVELS: f64 = 15.0;

/// Push a puff record into `pool` (`puff.wgsl`).
fn push_puff(pool: &mut PoolBuffer, puff: Puff) {
    push_stretched_puff(pool, puff, 1.0);
}

/// Push a puff `stretch` times as tall as a plain one: a jet of fire or a
/// lobe flattened as it bursts out.
fn push_stretched_puff(pool: &mut PoolBuffer, puff: Puff, stretch: f64) {
    if puff.size <= 0.0 || puff.opacity <= 0.003 {
        return;
    }
    // Billboards ignore rotation, so the pose is translation × scale only.
    let world = Mat4::from_scale_rotation_translation(
        Vec3::new(
            puff.size as f32,
            (puff.size * PUFF_HEIGHT * stretch) as f32,
            puff.size as f32,
        ),
        glam::Quat::IDENTITY,
        Vec3::new(
            puff.position[0] as f32,
            puff.position[1] as f32,
            puff.position[2] as f32,
        ),
    );
    let [r, g, b] = puff.color.map(|c| c as f32);
    pool.push(record(
        world,
        [r, g, b, puff.opacity.min(1.0) as f32],
        [
            (puff.seed.fract().abs() + (puff.underglow.clamp(0.0, 1.0) * UNDERGLOW_LEVELS).round())
                as f32,
            puff.heat as f32,
            puff.erosion.clamp(0.0, 1.0) as f32,
            puff.softness as f32,
        ],
    ));
}

/// Pooled blasts and scorches; `puffs` and `rings` are rebuilt each update, and
/// other effects append their own puffs after it.
#[derive(Clone, Debug)]
pub struct ExplosionEffects {
    blasts: [Blast; MAX_EXPLOSIONS],
    cursor: usize,
    tank_sequence: usize,
    barrel_sequence: usize,
    last_shape: usize,
    scorches: [Scorch; MAX_SCORCHES],
    scorch_count: usize,
    scorch_cursor: usize,
    pub puffs: PoolBuffer,
    pub rings: PoolBuffer,
    /// The current map's spawn pads, which rings on or beside them ride over.
    pub pads: SpawnPadDecks,
    /// The current map's ground, for the dust blasts kick up.
    pub ground: Ground,
    /// The round's solid cover: what a shell struck, and what smoke rolls round.
    pub walls: Walls,
    marks: [WallMark; MAX_WALL_MARKS],
    mark_count: usize,
    mark_cursor: usize,
    /// Seconds of effect time, and the debris pieces that last raised landing
    /// dust (id, when), so a piece settling or resting does not keep puffing.
    clock: f64,
    landings: [(u32, f64); LANDING_MEMORY],
    landing_cursor: usize,
    last_landing: f64,
    felled: [u32; FELLED_MEMORY],
    felled_cursor: usize,
}

impl Default for ExplosionEffects {
    fn default() -> Self {
        Self {
            blasts: [Blast::default(); MAX_EXPLOSIONS],
            cursor: 0,
            tank_sequence: 0,
            barrel_sequence: 0,
            last_shape: 0,
            scorches: [Scorch::default(); MAX_SCORCHES],
            scorch_count: 0,
            scorch_cursor: 0,
            puffs: PoolBuffer::new(super::PUFF_CAPACITY),
            rings: PoolBuffer::new(DECAL_CAPACITY),
            pads: SpawnPadDecks::default(),
            ground: Ground::default(),
            walls: Walls::default(),
            marks: [WallMark::default(); MAX_WALL_MARKS],
            mark_count: 0,
            mark_cursor: 0,
            clock: 0.0,
            landings: [(0, f64::NEG_INFINITY); LANDING_MEMORY],
            landing_cursor: 0,
            last_landing: f64::NEG_INFINITY,
            felled: [u32::MAX; FELLED_MEMORY],
            felled_cursor: 0,
        }
    }
}

impl ExplosionEffects {
    /// A piece of debris striking the ground or other debris (`DebrisImpact`):
    /// a hard landing raises the ground's dust under it, once per piece.
    pub fn landing(&mut self, event: &SimEvent) {
        let (Some(force), Some(height), Some(id)) = (event.force, event.height, event.id) else {
            return;
        };
        if height > LANDING_HEIGHT
            || force < LANDING_FORCE[0]
            || self.clock - self.last_landing < LANDING_GAP
            || self
                .landings
                .iter()
                .any(|&(piece, at)| piece == id && self.clock - at < LANDING_REPEAT)
        {
            return;
        }
        self.landings[self.landing_cursor] = (id, self.clock);
        self.landing_cursor = (self.landing_cursor + 1) % LANDING_MEMORY;
        self.last_landing = self.clock;
        let strength = (force / LANDING_FORCE[0]).ln() / (LANDING_FORCE[1] / LANDING_FORCE[0]).ln();
        self.dust_skirt(
            event.x,
            event.z,
            LANDING_DUST[0] + LANDING_DUST[1] * strength.clamp(0.0, 1.0),
            (f64::from(id) * 2.399).rem_euclid(TAU),
        );
    }

    /// A felled trunk slamming down: once it lies nearly flat, dust bursts up
    /// along its length where the crown and the log strike the ground.
    pub fn felled_trees(&mut self, fragments: &[RenderFragment]) {
        for fragment in fragments {
            let (Some(_), Some(center)) = (fragment.tree_cover_id, fragment.tree_center_y) else {
                continue;
            };
            let r = fragment.rotation;
            let up = glam::DQuat::from_xyzw(r.x, r.y, r.z, r.w).normalize() * glam::DVec3::Y;
            if up.y > FELLED_TILT || self.felled.contains(&fragment.id) {
                continue;
            }
            self.felled[self.felled_cursor] = fragment.id;
            self.felled_cursor = (self.felled_cursor + 1) % FELLED_MEMORY;
            let half = (center - TRUNK_STUMP).max(1.0);
            let p = fragment.position;
            for (j, along) in FELLED_DUST.into_iter().enumerate() {
                let x = p.x + up.x * half * along;
                let z = p.z + up.z * half * along;
                let phase = (f64::from(fragment.id) * 1.7 + j as f64 * 2.399).rem_euclid(TAU);
                self.dust_skirt(x, z, FELLED_DUST_SCALE * (0.8 + 0.25 * along), phase);
            }
        }
    }

    /// A skirt of the ground's dust rolling out low from `(x, z)` at `scale`.
    fn dust_skirt(&mut self, x: f64, z: f64, scale: f64, phase: f64) {
        let blast = &mut self.blasts[self.cursor];
        self.cursor = (self.cursor + 1) % MAX_EXPLOSIONS;
        *blast = Blast {
            active: true,
            kind: Kind::Landing,
            x,
            z,
            scale,
            phase,
            tempo: 1.0,
            dust: self.ground.dust(x, z),
            ..Blast::default()
        };
    }

    pub fn event(&mut self, event: &SimEvent, random: &mut CosmeticRandom) {
        // A felled tree's snap shakes dust off the ground round its stump.
        if event.kind == SimEventType::Destroy && event.cover_kind == Some(CoverKind::Tree) {
            let phase = random.next_f64() * TAU;
            self.dust_skirt(event.x, event.z, TREE_SNAP_DUST, phase);
            return;
        }
        let fire = matches!(event.kind, SimEventType::Explosion | SimEventType::Death);
        let wood = match (event.kind, event.cover_kind) {
            (SimEventType::Impact, Some(CoverKind::Cargo))
            | (SimEventType::Shatter, Some(CoverKind::Cargo | CoverKind::Tower)) => {
                Some(WoodDust::Hit)
            }
            (SimEventType::Destroy, Some(CoverKind::Cargo)) => Some(WoodDust::Burst),
            _ => None,
        };
        // A shell's hit on a tank (it names a height) or on hard cover (none):
        // a container's steel or stone, on the face nearest the impact.
        let strike = (event.kind == SimEventType::Impact
            && event.cover_kind.is_none()
            && event.height.is_none())
        .then(|| self.walls.strike(event.x, 1.0, event.z))
        .flatten();
        let struck = match event.kind {
            SimEventType::Impact if event.cover_kind.is_none() => Some(if event.height.is_some() {
                Struck::Steel
            } else if strike.is_some_and(|strike| strike.material == WallMaterial::Steel) {
                Struck::Sheet
            } else {
                Struck::Stone
            }),
            SimEventType::Ricochet => Some(Struck::Stone),
            _ => None,
        };
        // Barrels also emit an explosion; do not double their fireball/dust budget.
        if !fire
            && wood.is_none()
            && struck.is_none()
            && (event.kind != SimEventType::Destroy
                || matches!(event.cover_kind, Some(CoverKind::Tree | CoverKind::Drum)))
        {
            return;
        }
        let size = event.size.unwrap_or(3.0);
        let tank = event.kind == SimEventType::Death;
        let barrel =
            event.kind == SimEventType::Explosion && event.cover_kind == Some(CoverKind::Drum);
        let kind = if let Some(wood) = wood {
            Kind::Wood(wood)
        } else if let Some(struck) = struck {
            Kind::Hit(struck)
        } else if tank && event.death_style == Some(DeathStyle::Burnout) {
            Kind::Burnout
        } else if tank {
            Kind::Tank
        } else if barrel {
            Kind::Barrel
        } else if fire {
            Kind::Blast
        } else if event.cover_kind == Some(CoverKind::House) {
            Kind::HouseFall
        } else {
            Kind::Collapse
        };
        let blast = &mut self.blasts[self.cursor];
        self.cursor = (self.cursor + 1) % MAX_EXPLOSIONS;
        *blast = Blast {
            active: true,
            kind,
            x: event.x,
            z: event.z,
            dust: self.ground.dust(event.x, event.z),
            ..Blast::default()
        };
        blast.variant = if tank {
            self.tank_sequence += 1;
            (self.tank_sequence - 1) % TANK_BLASTS.len()
        } else if barrel {
            self.barrel_sequence += 1;
            (self.barrel_sequence - 1) % BARREL_BLASTS.len()
        } else {
            0
        };
        // Cosmetic draw order: tempo, scale, barrel scale, phase.
        blast.tempo = if tank || barrel {
            0.9 + random.next_f64() * 0.2
        } else {
            1.0
        };
        // Kills are the hero blasts; ordnance stays compact so the scale reads
        // from a shell's interception up to a drum.
        blast.scale = if tank {
            (size / 3.0) * (1.0 + random.next_f64() * 0.15)
        } else {
            blast_scale(size)
        };
        if barrel {
            blast.scale *= 0.74 + random.next_f64() * 0.1;
        }
        // Timber and a tower going down raise more dust than a shell's pop.
        if kind == Kind::Collapse {
            blast.scale *= COLLAPSE_DUST;
        }
        blast.phase = random.next_f64() * TAU;
        if fire {
            // A different eruption from the last one.
            let step = 1 + (random.next_f64() * (SHAPES.len() - 1) as f64) as usize;
            self.last_shape = (self.last_shape + step) % SHAPES.len();
            blast.shape = self.last_shape;
        }
        let mut wall_mark = None;
        if let Kind::Hit(_) = kind {
            blast.scale = 1.0;
            blast.y = event.height.unwrap_or(1.0);
            if let Some(from) = event.from {
                let (dx, dz) = (from.x - event.x, from.z - event.z);
                let length = dx.hypot(dz);
                if length > 1e-6 {
                    blast.direction = [dx / length, dz / length];
                }
            }
            // A hard wall's hit bursts out of the face it struck, and marks it.
            if let Some(strike) = strike {
                blast.x = f64::from(strike.point.x);
                blast.z = f64::from(strike.point.z);
                blast.y = f64::from(strike.point.y);
                blast.direction = [f64::from(strike.normal.x), f64::from(strike.normal.z)];
                // The dust is the wall's own: grey concrete grit, not the ground's.
                blast.dust = if strike.material == WallMaterial::Steel {
                    mixed(linear(strike.color), linear(GRIT), 0.45)
                } else {
                    mixed(linear(strike.color), linear(CONCRETE_POWDER), 0.75)
                };
                wall_mark = Some((strike, blast.phase, blast.dust));
            }
        }
        if let Some(wood) = wood {
            let (y, direction) = match wood {
                WoodDust::Hit => (
                    event.height.unwrap_or(HIT_HEIGHT),
                    // A struck face puffs back toward the gun; a burst board's dust
                    // flies on with the shell.
                    event.from.map(|from| {
                        let back = [from.x - event.x, from.z - event.z];
                        if event.kind == SimEventType::Shatter {
                            back.map(|v| -v)
                        } else {
                            back
                        }
                    }),
                ),
                WoodDust::Burst => (
                    event.height.unwrap_or(CRATE_HEIGHT),
                    event.from.map(|from| [event.x - from.x, event.z - from.z]),
                ),
            };
            blast.y = y;
            blast.scale = match wood {
                WoodDust::Hit => 1.0,
                WoodDust::Burst => (y / CRATE_HEIGHT).clamp(0.8, 1.3),
            };
            if let Some([dx, dz]) = direction {
                let length = dx.hypot(dz);
                if length > 1e-6 {
                    blast.direction = [dx / length, dz / length];
                }
            }
        }
        let footprint =
            (kind == Kind::HouseFall).then_some((blast.x, blast.z, blast.phase, blast.dust));
        // Fiery blasts char the ground where they went off.
        let scorch = match kind {
            Kind::Blast => Some(5.2),
            Kind::Barrel => Some(6.2),
            Kind::Tank => Some(7.4),
            Kind::Burnout => Some(4.4),
            _ => None,
        };
        if let Some(diameter) = scorch {
            let (x, z, phase, dust) = (blast.x, blast.z, blast.phase, blast.dust);
            let diameter = diameter * blast.scale;
            self.stain(x, z, diameter, phase, dust, SCORCH);
        }
        if let Some((x, z, phase, ground)) = footprint {
            self.house_footprint(x, z, phase, ground);
        }
        if let Some((strike, phase, powder)) = wall_mark {
            // Crushed concrete spills pale powder on the ground at the wall's foot.
            if strike.material == WallMaterial::Stone {
                let out = POWDER_REACH + 0.5 * hash(phase);
                self.stain(
                    f64::from(strike.point.x + strike.normal.x * out as f32),
                    f64::from(strike.point.z + strike.normal.z * out as f32),
                    POWDER_SIZE * (0.8 + 0.4 * hash(phase * 1.7)),
                    phase,
                    powder,
                    POWDER,
                );
            }
            self.mark(strike, phase);
        }
    }

    /// The dust and churned earth a cottage's collapse leaves round its footprint:
    /// a dark stain of soil and rubble under the heap, and lobes of pale plaster
    /// dust where the walls fell and the surge rolled out, feathered into the
    /// ground and lasting as long as a scorch.
    fn house_footprint(&mut self, x: f64, z: f64, phase: f64, ground: [f64; 3]) {
        let plaster = linear(PLASTER_DUST);
        let (churn, apart) = FOOTPRINT_CHURN;
        for side in [-0.5, 0.5] {
            self.stain(
                x + phase.cos() * apart * side,
                z + phase.sin() * apart * side,
                churn,
                phase + side,
                scaled(ground, 0.45),
                POWDER,
            );
        }
        for k in 0..FOOTPRINT_LOBES {
            let kf = k as f64;
            let angle =
                phase * 1.3 + kf * TAU / FOOTPRINT_LOBES as f64 + (hash(phase + kf) - 0.5) * 0.6;
            let reach = FOOTPRINT_REACH * (0.85 + 0.3 * hash(phase * 1.7 + kf * 0.9));
            let diameter = FOOTPRINT_LOBE * (0.8 + 0.4 * hash(phase * 2.3 + kf * 1.3));
            self.stain(
                x + angle.cos() * reach,
                z + angle.sin() * reach,
                diameter,
                phase + kf * 0.37,
                scaled(plaster, 1.08 - 0.12 * hash(phase * 0.7 + kf)),
                POWDER,
            );
        }
    }

    /// Leave a ground decal that lasts: a scorch or a powder stain.
    fn stain(&mut self, x: f64, z: f64, diameter: f64, phase: f64, dust: [f64; 3], kind: f32) {
        let y = self.pads.decal_height(x, z, diameter * 0.5, SCORCH_HEIGHT);
        self.scorches[self.scorch_cursor] = Scorch {
            x,
            z,
            y,
            age: 0.0,
            diameter,
            phase,
            dust,
            kind,
        };
        self.scorch_cursor = (self.scorch_cursor + 1) % MAX_SCORCHES;
        self.scorch_count = (self.scorch_count + 1).min(MAX_SCORCHES);
    }

    pub fn reset(&mut self) {
        for blast in &mut self.blasts {
            blast.active = false;
        }
        self.cursor = 0;
        self.landings = [(0, f64::NEG_INFINITY); LANDING_MEMORY];
        self.last_landing = f64::NEG_INFINITY;
        self.felled = [u32::MAX; FELLED_MEMORY];
        self.felled_cursor = 0;
        self.tank_sequence = 0;
        self.barrel_sequence = 0;
        self.scorch_count = 0;
        self.scorch_cursor = 0;
        self.mark_count = 0;
        self.mark_cursor = 0;
        self.walls.clear();
        self.puffs.clear();
        self.rings.clear();
    }

    /// Offer every live blast's and hit's light to `lights`, which keeps the
    /// brightest.
    pub fn lights(&self, lights: &mut [Flash]) {
        for blast in self.blasts.iter().filter(|blast| blast.active) {
            let t = blast.age * blast.tempo;
            let s = blast.scale;
            let (intensity, height) = match blast.kind {
                Kind::Blast | Kind::Tank | Kind::Barrel => (
                    BLAST_LIGHT * s * blast.fire_light(t, &blast.look()),
                    LIGHT_HEIGHT * s + LIGHT_LIFT,
                ),
                Kind::Burnout => (
                    BLAST_LIGHT * BURNOUT_FIRE_SCALE * s * blast.fire_light(t, &blast.look()),
                    LIGHT_HEIGHT * s * BURNOUT_FIRE_SCALE + LIGHT_LIFT,
                ),
                Kind::Hit(_) => (HIT_LIGHT * (-t * 22.0).exp(), blast.y + 0.6),
                _ => continue,
            };
            super::offer_light(
                lights,
                Flash {
                    position: Vec3::new(blast.x as f32, height as f32, blast.z as f32),
                    intensity: intensity as f32,
                    color: if t < 0.07 {
                        FLASH_LIGHT_COLOR
                    } else {
                        FIRE_LIGHT_COLOR
                    },
                },
            );
        }
    }

    /// Live blasts, for tests and stats.
    pub fn active(&self) -> usize {
        self.blasts.iter().filter(|blast| blast.active).count()
    }

    /// Scorches still on the ground.
    pub fn scorches(&self) -> usize {
        self.scorch_count
    }

    /// Age every blast and scorch and rebuild the puffs and decals; flash cores
    /// go into `streaks`, the additive pool.
    pub fn update(&mut self, dt: f64, streaks: &mut PoolBuffer) {
        self.clock += dt.max(0.0);
        self.puffs.clear();
        self.rings.clear();
        self.scorch_decals(dt);
        self.wall_marks(dt);
        for index in 0..MAX_EXPLOSIONS {
            let mut blast = self.blasts[index];
            if blast.active {
                self.advance(&mut blast, dt, streaks);
                self.blasts[index] = blast;
            }
        }
    }

    fn scorch_decals(&mut self, dt: f64) {
        // Oldest first, so younger scorches draw over older ones.
        let first = (self.scorch_cursor + MAX_SCORCHES - self.scorch_count) % MAX_SCORCHES;
        let mut expired = 0;
        for i in 0..self.scorch_count {
            let scorch = &mut self.scorches[(first + i) % MAX_SCORCHES];
            scorch.age += dt.max(0.0);
            if scorch.age >= SCORCH_LIFETIME {
                expired += 1;
                continue;
            }
            let strength = 1.0 - smoothstep(scorch.age, SCORCH_LIFETIME * 0.55, SCORCH_LIFETIME);
            let appear = smoothstep(scorch.age, 0.0, 0.08);
            let world = Mat4::from_translation(Vec3::new(
                scorch.x as f32,
                scorch.y as f32,
                scorch.z as f32,
            )) * Mat4::from_scale(Vec3::new(
                scorch.diameter as f32,
                1.0,
                scorch.diameter as f32,
            ));
            let [r, g, b] = scorch.dust.map(|c| c as f32);
            self.rings.push(record(
                world,
                [r, g, b, 1.0],
                [
                    scorch.age as f32,
                    scorch.phase as f32,
                    scorch.kind,
                    (strength * appear) as f32,
                ],
            ));
        }
        // Expired scorches are always the oldest ones.
        self.scorch_count -= expired;
    }

    /// Leave a shell's mark where it struck a hard wall.
    fn mark(&mut self, strike: Strike, phase: f64) {
        let steel = strike.material == WallMaterial::Steel;
        let diameter = (WALL_MARK_SIZE as f32).min(strike.room * 2.3);
        if diameter < 0.2 {
            return;
        }
        let standoff = WALL_MARK_STANDOFF[usize::from(steel)];
        self.marks[self.mark_cursor] = WallMark {
            position: strike.point + strike.normal * standoff,
            normal: strike.normal,
            diameter,
            steel,
            color: hex_to_linear(strike.color),
            age: 0.0,
            phase,
        };
        self.mark_cursor = (self.mark_cursor + 1) % MAX_WALL_MARKS;
        self.mark_count = (self.mark_count + 1).min(MAX_WALL_MARKS);
    }

    /// Every wall mark, as a decal standing on its face, oldest first; the tint is
    /// the wall's own color for the crater's broken stone.
    fn wall_marks(&mut self, dt: f64) {
        let first = (self.mark_cursor + MAX_WALL_MARKS - self.mark_count) % MAX_WALL_MARKS;
        let mut expired = 0;
        for i in 0..self.mark_count {
            let mark = &mut self.marks[(first + i) % MAX_WALL_MARKS];
            mark.age += dt.max(0.0);
            if mark.age >= WALL_MARK_LIFETIME {
                expired += 1;
                continue;
            }
            let strength = 1.0 - smoothstep(mark.age, WALL_MARK_LIFETIME * 0.6, WALL_MARK_LIFETIME);
            let appear = smoothstep(mark.age, 0.0, 0.04);
            let world = Mat4::from_scale_rotation_translation(
                Vec3::new(mark.diameter, 1.0, mark.diameter),
                glam::Quat::from_rotation_arc(Vec3::Y, mark.normal),
                mark.position,
            );
            let kind = if mark.steel { STEEL_MARK } else { STONE_MARK };
            let [r, g, b] = mark.color;
            self.rings.push(record(
                world,
                [r, g, b, 1.0],
                [
                    mark.age as f32,
                    mark.phase as f32,
                    kind,
                    (strength * appear) as f32,
                ],
            ));
        }
        self.mark_count -= expired;
    }

    /// Wall marks on hard cover, for tests.
    pub fn wall_mark_count(&self) -> usize {
        self.mark_count
    }

    fn puff(&mut self, puff: Puff) {
        push_puff(&mut self.puffs, puff);
    }

    fn advance(&mut self, b: &mut Blast, dt: f64, streaks: &mut PoolBuffer) {
        b.age += dt.max(0.0);
        let look = b.look();
        let lifetime = b.lifetime();
        let t = b.age * b.tempo;
        if t >= lifetime {
            b.active = false;
            return;
        }
        match b.kind {
            Kind::Wood(wood) => self.wood_dust(b, wood, t, lifetime),
            Kind::Hit(struck) => self.hit(b, struck, t, streaks),
            Kind::Burnout => {
                // The ammunition goes inside the hull: a smaller fireball bursts
                // out of its seams and hatches, lighting the ground, and the
                // hull smokes on.
                let mut inner = *b;
                inner.scale *= BURNOUT_FIRE_SCALE;
                self.ground_glow(&inner, t, &look, streaks);
                self.skirt(&inner, t, b.dust);
                self.fireball(&inner, t, &look, linear(TANK_SMOKE), streaks);
                self.burnout(b, t);
                self.burnout_flare(b, t, streaks);
                self.ring(b, t, inner.scale);
            }
            Kind::Landing => self.skirt(b, t, scaled(b.dust, 1.05)),
            Kind::Collapse => {
                let dust = linear(DUST);
                self.haze(b, t, lifetime, dust);
                self.skirt(b, t, mixed(b.dust, dust, 0.5));
                self.column(b, t, lifetime, &BLAST, scaled(dust, 1.1), false);
                self.ring(b, t, b.scale);
            }
            Kind::HouseFall => {
                // Plaster and mortar off the walls, not the ground the cottage stood on.
                let dust = mixed(b.dust, linear(PLASTER_DUST), 0.9);
                self.skirt(b, t, dust);
                self.house_dust(b, t, lifetime, dust);
                if t > HOUSE_IMPACT {
                    self.ring(b, t - HOUSE_IMPACT, b.scale);
                }
            }
            Kind::Blast | Kind::Tank | Kind::Barrel => {
                let smoke = linear(look.smoke);
                // No sitting haze: the column carries the smoke up and away,
                // and a broad faint disc over the ground read as a flat decal.
                self.ground_glow(b, t, &look, streaks);
                self.skirt(b, t, b.dust);
                self.column(b, t, lifetime, &look, smoke, true);
                if b.kind == Kind::Tank {
                    self.turret_jet(b, t, smoke, streaks);
                }
                if b.kind == Kind::Barrel {
                    fuel_fire(b, t, lifetime, streaks);
                    self.fuel_smoke(b, t, lifetime, smoke);
                    self.fuel_plume(b, t, lifetime, smoke);
                }
                self.fireball(b, t, &look, smoke, streaks);
                let s = b.scale;
                // Low in the hull's middle: the seams' glows carry the flash round
                // it, and the hull stands dark against them.
                let center = Vec3::new(b.x as f32, (0.9 * s) as f32, b.z as f32);
                if t < BLOOM_SECONDS {
                    let k = t / BLOOM_SECONDS;
                    let [r, g, bl] = BLOOM_COLOR;
                    glow(
                        streaks,
                        center,
                        (BLOOM_SIZE * s * (0.7 + 0.5 * k)) as f32,
                        [r, g, bl, ((1.0 - k) * (1.0 - k) * 0.3) as f32],
                    );
                }
                if t < FLASH_SECONDS {
                    let k = t / FLASH_SECONDS;
                    let [r, g, bl] = FLASH_COLOR;
                    glow(
                        streaks,
                        center,
                        (FLASH_SIZE * s * (0.7 + 0.5 * k)) as f32,
                        [r, g, bl, ((1.0 - k) * (1.0 - k) * 0.7) as f32],
                    );
                }
                self.shock(b, t, b.scale);
                self.ring(b, t, b.scale);
            }
        }
    }

    /// The fire's light on the ground around it: a flash, then a flickering
    /// glow that dies as the fireball burns out, added onto the ground.
    fn ground_glow(&self, b: &Blast, t: f64, look: &Look, streaks: &mut PoolBuffer) {
        let strength = b.fire_light(t, look);
        if strength <= 0.01 {
            return;
        }
        let s = b.scale;
        let diameter =
            (GROUND_GLOW_SIZE * s * look.fire_size * (0.75 + 0.35 * settle(t, 8.0))) as f32;
        let y = self
            .pads
            .decal_height(b.x, b.z, f64::from(diameter) * 0.5, RING_HEIGHT + 0.03)
            as f32;
        let [r, g, bl] = GROUND_GLOW_COLOR;
        ground_glow(
            streaks,
            Vec3::new(b.x as f32, y, b.z as f32),
            diameter,
            [r, g, bl, (0.5 * strength).min(0.55) as f32],
        );
        // The flash lights the ground white round the blast for a frame or two.
        if t < SEAM_FLASH_SECONDS {
            let k = t / SEAM_FLASH_SECONDS;
            let [r, g, bl] = FLASH_GROUND_COLOR;
            ground_glow(
                streaks,
                Vec3::new(b.x as f32, y + 0.005, b.z as f32),
                (FLASH_GROUND_SIZE * s * (0.8 + 0.3 * k)) as f32,
                [r, g, bl, ((1.0 - k) * (1.0 - k) * 0.85) as f32],
            );
        }
    }

    /// A broad, faint haze that settles over the blast and lingers, drifting.
    fn haze(&mut self, b: &Blast, t: f64, lifetime: f64, color: [f64; 3]) {
        let s = b.scale;
        for j in 0..2 {
            let age = t - 0.35 - j as f64 * 0.25;
            if age <= 0.0 {
                continue;
            }
            let angle = b.phase + PI * j as f64 + 0.8;
            let reach = (0.6 + 1.2 * settle(age, 1.2)) * s;
            self.puff(Puff {
                position: [
                    b.x + angle.cos() * reach + WIND[0] * age * 0.8,
                    (0.7 + 0.25 * age) * s,
                    b.z + angle.sin() * reach + WIND[1] * age * 0.8,
                ],
                size: (2.4 + 1.3 * age) * s,
                color,
                opacity: (age / 0.5).min(1.0)
                    * 0.1
                    * (1.0 - smoothstep(t, lifetime * 0.45, lifetime)),
                seed: b.phase * 0.37 + j as f64 * 0.21,
                heat: 0.0,
                erosion: smoothstep(t, lifetime * 0.3, lifetime) * 0.75,
                softness: 1.0,
                underglow: 0.0,
            });
        }
    }

    /// The ground's dust racing out low from under the blast: billows of
    /// different sizes and timing that roll out, spread and thin away.
    fn skirt(&mut self, b: &Blast, t: f64, color: [f64; 3]) {
        let s = b.scale;
        for j in 0..SKIRT_PUFFS {
            let jf = j as f64;
            let r1 = hash(b.phase + jf * 1.913);
            let r2 = hash(b.phase * 1.7 + jf * 0.577);
            let r3 = hash(b.phase * 2.3 + jf * 2.719);
            let age = t - r1 * 0.07;
            if age <= 0.0 {
                continue;
            }
            // Uneven gaps and reaches: a burst of billows, not a stamped ring.
            let angle = b.phase + jf * 2.399 + (r2 - 0.5) * 1.1;
            let reach = (0.5 + (1.8 + 2.4 * r3) * settle(age, 3.0 + 3.0 * r2)) * s;
            let size = (0.45 + 0.35 * r1 + (0.8 + 0.6 * r2) * age.sqrt()) * s;
            let end = 1.1 + 0.8 * r3;
            self.puff(Puff {
                position: [
                    b.x + angle.cos() * reach + WIND[0] * age * 0.5,
                    0.2 * s + size * 0.3,
                    b.z + angle.sin() * reach + WIND[1] * age * 0.5,
                ],
                size,
                color: scaled(color, 0.8 + 0.25 * r2),
                opacity: (age / 0.05).min(1.0)
                    * (0.36 + 0.24 * r3)
                    * (1.0 - smoothstep(age, 0.25, end))
                    * (0.9 * s / size).clamp(0.35, 1.0),
                seed: b.phase * 0.53 + jf * 0.137,
                heat: 0.0,
                erosion: smoothstep(age, 0.2, end) * 0.9,
                softness: 0.45 + 0.25 * r1,
                underglow: 0.0,
            });
        }
    }

    /// The smoke rolling off the top of the fireball: puffs born small just
    /// above the fire that rise on its heat, swell, thin out and drift on the
    /// wind; `glows` lights their undersides with the fire while it burns.
    fn column(
        &mut self,
        b: &Blast,
        t: f64,
        lifetime: f64,
        look: &Look,
        smoke: [f64; 3],
        glows: bool,
    ) {
        let s = b.scale;
        let fire = if glows {
            burning(fire_heat(t, COOLING / look.fire_life))
        } else {
            0.0
        };
        for j in 0..SMOKE_PUFFS {
            let jf = j as f64;
            let r1 = hash(b.phase * 0.91 + jf * 3.137);
            let r2 = hash(b.phase * 1.37 + jf * 1.618);
            let r3 = hash(b.phase * 2.11 + jf * 0.433);
            let born = 0.1 + jf * 0.05 + r1 * 0.04;
            let age = t - born;
            if age <= 0.0 {
                continue;
            }
            let life = lifetime - born;
            // Golden-angle placement keeps the column's billows from lining up.
            let angle = b.phase + jf * 2.399;
            let spread = (0.25 + 0.95 * settle(age, 1.6)) * s * (0.55 + 0.6 * r3);
            let rise = look.smoke_rise * s.sqrt() * (0.7 + 0.5 * r2);
            let y = (1.3 + 0.5 * r1) * s
                + rise * settle(age, 0.8) / 0.8
                + 0.5 * SOOT_BUOYANCY * 0.5 * age * age;
            let carry = age * (0.6 + 0.3 * y);
            let size = (0.55 + 1.4 * age.sqrt()) * s * look.smoke_size * (0.8 + 0.4 * r2);
            // The same smoke spread over a bigger billow is thinner.
            let thin = (1.15 * s * look.smoke_size / size)
                .clamp(0.3, 1.0)
                .powf(0.8);
            self.puff(Puff {
                position: [
                    b.x + angle.cos() * spread
                        + WIND[0] * carry
                        + b.phase.cos() * age * look.drift * 0.4,
                    y,
                    b.z + angle.sin() * spread
                        + WIND[1] * carry
                        + b.phase.sin() * age * look.drift * 0.4,
                ],
                size,
                color: scaled(smoke, 0.85 + 0.3 * r3),
                // Thinning as it climbs, so the column frays out overhead rather
                // than standing as one dark mass.
                opacity: (age / 0.2).min(1.0)
                    * 0.55
                    * thin
                    * (1.0 - 0.55 * smoothstep(y, 2.5 * s, 7.5 * s))
                    * (1.0 - smoothstep(age, life * 0.3, life * 0.85)),
                seed: b.phase * 0.71 + jf * 0.173,
                heat: 0.0,
                erosion: smoothstep(age, life * 0.25, life) * 0.85,
                softness: 0.32 + 0.22 * r1,
                underglow: fire * (1.0 - smoothstep(age, 0.2, 1.4)),
            });
        }
    }

    /// The fireball: lobes of white-hot fire burst out of the source within a few
    /// frames, a vehicle's out of its hull's seams and hatches round its middle so
    /// the hull stays in sight, and fly out and up. Each lobe cools from its rim
    /// through orange and deep red into the soot it carries, so a dark sooty rim
    /// takes over within a third of a second while the hearts burn on a beat
    /// longer. The soot then climbs on its own heat, each lobe at its own rate so
    /// the cloud tears apart as it rises, swells, pales and thins on the wind:
    /// the blast leaves rising wisps, never a sitting ball.
    fn fireball(
        &mut self,
        b: &Blast,
        t: f64,
        look: &Look,
        smoke: [f64; 3],
        streaks: &mut PoolBuffer,
    ) {
        let s = b.scale;
        let shape = SHAPES[b.shape % SHAPES.len()];
        let cooling = COOLING / look.fire_life;
        let vehicle = matches!(b.kind, Kind::Tank | Kind::Burnout);
        let (source, seam_height, hatch_height, [spread, lobe], column) = if vehicle {
            (
                HULL_SEAMS,
                HULL_SEAM_HEIGHT,
                HULL_HATCH_HEIGHT,
                [1.0, 1.0],
                shape.column * HULL_COLUMN,
            )
        } else {
            (
                GROUND_SEAMS,
                GROUND_SEAM_HEIGHT,
                GROUND_HATCH_HEIGHT,
                GROUND_LOBES,
                shape.column * GROUND_COLUMN,
            )
        };
        // Every blast its own build: some squat and wide, some tall and narrow.
        let build = hash(b.phase * 4.13 + 0.37);
        let spread = spread * (0.78 + 0.44 * build);
        let column = column * (1.15 - 0.4 * build);
        for j in 0..FIRE_PUFFS {
            let jf = j as f64;
            let r1 = hash(b.phase * 0.53 + jf * 1.371);
            let r2 = hash(b.phase * 1.91 + jf * 0.713);
            let r3 = hash(b.phase * 2.71 + jf * 2.239);
            let r4 = hash(b.phase * 3.17 + jf * 0.389);
            let late = j >= shape.late_from;
            let age = t - r1 * 0.035 - if late { shape.late } else { 0.0 };
            if age <= 0.0 {
                continue;
            }
            let seam = j < shape.seams;
            // Most of the way out within a tenth of a second.
            let out = settle(age, 22.0 + 10.0 * r2);
            let (angle, reach, height, size) = if seam {
                let k = (jf + 0.5) / shape.seams as f64 - 0.5;
                let angle =
                    b.phase + k * shape.arc + (r3 - 0.5) * shape.arc / shape.seams as f64 * 0.7;
                let reach =
                    source + (0.3 + 0.5 * r3) * shape.reach * look.fire_spread * spread * out;
                let height = seam_height + 0.3 * r2 + 1.0 * shape.rise * out;
                // A few big lobes among small ones.
                let size = 0.25 + (0.55 + 0.85 * r1 * r1) * out;
                (angle, reach, height, size)
            } else {
                let angle = b.phase + jf * 2.399;
                let reach = 0.1 + 0.5 * r3 * out;
                let height = hatch_height + column * (1.0 + 1.3 * r3) * settle(age, 6.0 + 3.0 * r2);
                let size = 0.22 + (0.5 + 0.6 * r1 * r1) * out;
                (angle, reach, height, size)
            };
            // White-hot for the flash's first frames, then the lobe's own cooling.
            let heat = fire_heat(age, cooling * (0.8 + 0.45 * r4))
                + FLASH_HEAT * (-age / FLASH_HEAT_SECONDS).exp();
            let fire = burning(heat);
            // Soot rises on its own heat once the fire is out, each lobe at its own
            // rate, so the cloud tears apart into rising wisps.
            let soot_age = (age - look.fire_life * 0.5).max(0.0);
            let buoyancy = SOOT_BUOYANCY * (0.55 + 0.9 * r2);
            let climb = 0.5 * buoyancy * soot_age * soot_age;
            let lift = (height + look.fire_rise * age * 0.6) * s + climb * s.sqrt();
            let reach = (reach + 0.35 * soot_age) * s;
            // The burning part shrinks back a little as it cools; the soot it
            // leaves keeps spreading.
            let size = size * lobe * (0.84 + 0.16 * fire) * s * look.fire_size * shape.size
                + (0.35 + 0.3 * r2) * soot_age * s.sqrt();
            let pale = smoothstep(soot_age, 0.1, 0.1 + SOOT_SECONDS);
            let carry = soot_age * soot_age * 0.5;
            let x = b.x
                + angle.cos() * reach
                + b.phase.cos() * age * look.drift * 0.5
                + WIND[0] * carry;
            let z = b.z
                + angle.sin() * reach
                + b.phase.sin() * age * look.drift * 0.5
                + WIND[1] * carry;
            // Each seam glows white-hot round the hull for the flash's first frames,
            // outlining the hull rather than hiding it.
            if vehicle && seam && age < SEAM_FLASH_SECONDS {
                let k = age / SEAM_FLASH_SECONDS;
                let [r, g, bl] = SEAM_FLASH_COLOR;
                glow(
                    streaks,
                    Vec3::new(x as f32, lift as f32, z as f32),
                    (SEAM_FLASH_SIZE * s * (0.6 + 0.6 * k)) as f32,
                    [r, g, bl, ((1.0 - k) * (1.0 - k) * 0.55) as f32],
                );
            }
            // Fire is dense; its soot is a veil that thins as it climbs and
            // spreads, never a sitting balloon.
            let veil = 0.5 + 0.48 * fire;
            let gone = 1.0 - smoothstep(soot_age, 0.25, 1.3 + 0.7 * r3);
            // Jets up the middle burst tall, lobes out of the seams wide; both
            // round out as they turn to soot.
            let burst = match (seam, vehicle) {
                (true, true) => 0.8 + 0.15 * r4,
                (true, false) => 1.0 + 0.2 * r4,
                (false, true) => 1.05 + 0.25 * r4,
                (false, false) => 1.0 + 0.2 * r4,
            };
            // Soot climbing on its own heat draws out into rising billows.
            let stretch = 1.0 + (burst - 1.0) * fire + 0.4 * smoothstep(soot_age, 0.15, 1.2);
            push_stretched_puff(
                &mut self.puffs,
                Puff {
                    position: [x, lift, z],
                    size,
                    color: scaled(smoke, 0.42 + 0.75 * pale),
                    opacity: (age / 0.015).min(1.0) * veil * gone,
                    seed: b.phase * 0.29 + jf * 0.311,
                    heat,
                    erosion: (smoothstep(soot_age, 0.0, 1.4 + 0.6 * r3) * 0.85)
                        .max(0.25 * (1.0 - fire)),
                    softness: (1.0 - fire) * (0.2 + 0.35 * pale),
                    underglow: (heat / 0.45).min(1.0) * 0.85,
                },
                stretch,
            );
        }
    }

    /// A tank's ammunition cooking off: a jet of fire shooting up from the turret
    /// ring, burning out as it climbs.
    fn turret_jet(&mut self, b: &Blast, t: f64, smoke: [f64; 3], streaks: &mut PoolBuffer) {
        let s = b.scale;
        if t < JET_SECONDS {
            let k = t / JET_SECONDS;
            let [r, g, bl] = JET_COLOR;
            for i in 0..2 {
                let lean = (b.phase + i as f64 * 2.1).cos() as f32 * 0.15;
                flame(
                    streaks,
                    Vec3::new(b.x as f32, (0.8 * s) as f32, b.z as f32),
                    Vec3::new(
                        WIND[0] as f32 * 0.1 + lean,
                        1.0,
                        WIND[1] as f32 * 0.1 - lean,
                    )
                    .normalize(),
                    (1.5 * s * (1.0 - 0.3 * k)) as f32 * (1.0 - 0.3 * i as f32),
                    ((2.4 + 3.2 * settle(t, 9.0)) * s) as f32 * (1.0 - 0.25 * i as f32),
                    (b.phase * 0.37 + i as f64 * 0.41) as f32,
                    [r, g, bl, ((1.0 - k) * (1.0 - k).sqrt()) as f32],
                );
            }
        }
        for j in 0..3 {
            let jf = j as f64;
            let age = t - 0.05 - jf * 0.07;
            if age <= 0.0 {
                continue;
            }
            let heat = fire_heat(age, 4.5 + jf * 0.8);
            // Each gout leans its own way, climbs its own height and swells to
            // its own size, so the jet never reads as a stack of equal balls.
            let r1 = hash(b.phase * 1.21 + jf * 0.917);
            let r2 = hash(b.phase * 0.67 + jf * 1.733);
            let lean = b.phase + jf * 2.399;
            let off = (0.15 + 0.55 * r1) * jf.min(1.0);
            self.puff(Puff {
                position: [
                    b.x + lean.cos() * off + WIND[0] * age * 0.6,
                    (1.2 + (3.2 + 3.0 * r2) * settle(age, 3.0) * (1.0 - 0.22 * jf)) * s,
                    b.z + lean.sin() * off + WIND[1] * age * 0.6,
                ],
                size: (0.4 + 0.35 * r1 + (0.6 + 0.5 * r2) * age.sqrt()) * s,
                color: scaled(smoke, 0.7),
                opacity: (age / 0.04).min(1.0) * 0.95 * burning(heat),
                seed: b.phase * 0.43 + jf * 0.5,
                heat,
                erosion: (1.0 - smoothstep(heat, 0.3, 1.2)) * 0.8,
                softness: 0.0,
                underglow: 0.0,
            });
        }
    }

    /// The spilled fuel's body of fire: balls of flame rolling up off the pool
    /// that cool into oily black smoke, so the drum's spot keeps burning after
    /// the fireball (in the puff slots a tank's turret jet would use).
    /// Black oily smoke the burning fuel feeds: a plume that climbs off the
    /// puddle, leans on the wind and pales and spreads as it rises, for as
    /// long as the fuel burns.
    fn fuel_plume(&mut self, b: &Blast, t: f64, lifetime: f64, smoke: [f64; 3]) {
        let fed = smoothstep(t, 0.5, 1.1) * (1.0 - smoothstep(t, FUEL_FIRE[1], lifetime));
        if fed <= 0.01 {
            return;
        }
        let s = b.scale;
        for j in 0..FUEL_PLUME {
            let jf = j as f64;
            // One rate for every puff, each with its own phase.
            let cycle = (t - 0.5) / FUEL_PLUME_CYCLE + jf / FUEL_PLUME as f64;
            if cycle < 0.0 {
                continue;
            }
            let a = cycle.fract();
            let lap = cycle.floor();
            let swirl = (lap * 2.3 + jf + b.phase * 3.0).sin() * (0.2 + 0.5 * a);
            let lean = a * a * FUEL_PLUME_CYCLE * 0.8;
            let size = (0.7 + 2.1 * a.powf(0.8)) * s;
            self.puff(Puff {
                position: [
                    b.x + WIND[0] * lean + swirl,
                    (0.6 + FUEL_PLUME_HEIGHT * a.powf(0.85)) * s,
                    b.z + WIND[1] * lean - swirl * 0.6,
                ],
                size,
                color: scaled(smoke, 0.5 + 0.6 * a),
                opacity: fed * smoothstep(a, 0.0, 0.12) * (1.0 - smoothstep(a, 0.35, 1.0)) * 0.7,
                seed: b.phase * 0.61 + lap * 0.37 + jf * 0.19,
                heat: 0.0,
                erosion: smoothstep(a, 0.3, 1.0) * 0.8,
                softness: 0.12 + 0.3 * a,
                underglow: fed * (1.0 - a).powi(4) * 0.5,
            });
        }
    }

    fn fuel_smoke(&mut self, b: &Blast, t: f64, lifetime: f64, smoke: [f64; 3]) {
        let fire = smoothstep(t, FUEL_FIRE[0], FUEL_FIRE[0] + 0.25)
            * (1.0 - smoothstep(t, FUEL_FIRE[1], lifetime * 0.95));
        if fire <= 0.01 {
            return;
        }
        let s = b.scale;
        for j in 0..FUEL_BALLS {
            let jf = j as f64;
            // One rate for every ball, each with its own phase.
            let cycle = t / FUEL_BALL_CYCLE + jf / FUEL_BALLS as f64 + b.phase;
            let a = cycle.fract();
            let lap = cycle.floor();
            let angle = b.phase + (lap + jf) * 2.399;
            let reach = 0.35 * s;
            self.puff(Puff {
                position: [
                    b.x + angle.cos() * reach + WIND[0] * a * 0.9,
                    (0.25 + 2.2 * a) * s,
                    b.z + angle.sin() * reach + WIND[1] * a * 0.9,
                ],
                size: (0.35 + 0.6 * a) * s,
                color: scaled(smoke, 0.55),
                opacity: fire * (a / 0.1).min(1.0) * (1.0 - a * a) * 0.6,
                seed: b.phase * 0.47 + lap * 0.61 + jf * 0.3,
                heat: fire * (1.0 - a) * 0.95,
                erosion: smoothstep(a, 0.35, 1.0) * 0.8,
                softness: 0.15,
                underglow: 0.0,
            });
        }
    }

    /// A burnout's moment: the ammunition goes inside the hull, so fire jets out
    /// of its hatches and a gout of fire and black smoke rolls up off it, much
    /// smaller than a cook-off's fireball.
    fn burnout_flare(&mut self, b: &Blast, t: f64, streaks: &mut PoolBuffer) {
        let s = b.scale;
        if t < JET_SECONDS {
            let k = t / JET_SECONDS;
            let [r, g, bl] = JET_COLOR;
            flame(
                streaks,
                Vec3::new(b.x as f32, (0.9 * s) as f32, b.z as f32),
                Vec3::new(WIND[0] as f32 * 0.1, 1.0, WIND[1] as f32 * 0.1).normalize(),
                (1.1 * s * (1.0 - 0.3 * k)) as f32,
                ((1.6 + 2.0 * settle(t, 9.0)) * s) as f32,
                (b.phase * 0.29).fract() as f32,
                [r, g, bl, ((1.0 - k) * (1.0 - k).sqrt() * 0.85) as f32],
            );
        }
        let smoke = linear(TANK_SMOKE);
        for j in 0..BURNOUT_BILLOWS {
            let jf = j as f64;
            let age = t - 0.02 - jf * 0.05;
            if age <= 0.0 {
                continue;
            }
            let angle = b.phase + jf * 2.399;
            let heat = fire_heat(age, 4.2 + jf * 0.6);
            let reach = (0.3 + 0.5 * settle(age, 6.0)) * s;
            self.puff(Puff {
                position: [
                    b.x + angle.cos() * reach + WIND[0] * age * 0.5,
                    (1.0 + 2.2 * settle(age, 2.5) + 0.3 * jf) * s,
                    b.z + angle.sin() * reach + WIND[1] * age * 0.5,
                ],
                size: (0.75 + 0.9 * age.sqrt()) * s,
                color: scaled(smoke, 0.6),
                opacity: (age / 0.03).min(1.0)
                    * 0.9
                    * (1.0 - smoothstep(age, 0.6, BURNOUT_LIFETIME - 0.3)),
                seed: b.phase * 0.51 + jf * 0.37,
                heat,
                erosion: smoothstep(age, 0.4, BURNOUT_LIFETIME) * 0.8,
                softness: 0.15,
                underglow: (heat / 0.45).min(1.0) * 0.8,
            });
        }
    }

    /// A burnout: a small internal flash, then thin exhaust-like smoke; no
    /// shockwave.
    fn burnout(&mut self, b: &Blast, t: f64) {
        let s = b.scale;
        for j in 0..3 {
            let age = t - j as f64 * 0.18;
            if age <= 0.0 {
                continue;
            }
            self.puff(Puff {
                position: [
                    b.x + WIND[0] * age * 0.6,
                    (1.1 + j as f64 * 0.3 + age * 0.85) * s,
                    b.z + WIND[1] * age * 0.6,
                ],
                size: (0.4 + age.sqrt() * 0.5) * s,
                color: linear(TANK_SMOKE),
                opacity: (age * 7.0).min(1.0) * (1.0 - smoothstep(t, 1.5, 2.7)) * 0.6,
                seed: b.phase * 0.3 + j as f64 * 0.27,
                heat: 0.0,
                erosion: smoothstep(t, 1.0, 2.7) * 0.8,
                softness: 0.3,
                underglow: 0.6 * (-age * 3.0).exp(),
            });
        }
        if t < 0.2 {
            let heat = fire_heat(t, 7.0);
            self.puff(Puff {
                position: [b.x, 0.9 * s, b.z],
                size: 0.65 * s,
                color: linear(TANK_SMOKE),
                opacity: burning(heat),
                seed: b.phase,
                heat,
                erosion: 0.0,
                softness: 0.0,
                underglow: 0.0,
            });
        }
    }

    /// A shell's hit: a white-hot point in an orange bloom, a star of glints, a
    /// burst of fire that burns out at once and a gout of smoke or grit.
    fn hit(&mut self, b: &Blast, struck: Struck, t: f64, streaks: &mut PoolBuffer) {
        let y = b.y.max(0.3);
        let at = Vec3::new(b.x as f32, y as f32, b.z as f32);
        // A small hot point in a warm bloom, kept below white so the struck
        // hull keeps its silhouette; the sparks carry the rest.
        // A wall takes the full force of the shell: a bigger, longer flash.
        let wall = struck != Struck::Steel;
        let (flash_seconds, flare) = match struck {
            Struck::Sheet => (0.14, 2.3),
            Struck::Stone => (0.12, 1.8),
            Struck::Steel => (0.09, 1.0),
        };
        // Concrete flashes white and bursts into powder; only steel burns.
        let (core, bloom) = if struck == Struck::Stone {
            (STONE_FLASH, STONE_BLOOM)
        } else {
            (HIT_FLASH, HIT_BLOOM)
        };
        if t < flash_seconds {
            let k = t / flash_seconds;
            let fade = (1.0 - k) as f32;
            let [r, g, bl, a] = core;
            glow(
                streaks,
                at,
                ((0.45 + 0.3 * k) * flare) as f32,
                [r, g, bl, fade * a],
            );
            let [r, g, bl, a] = bloom;
            glow(
                streaks,
                at,
                ((1.6 + 0.8 * k) * flare) as f32,
                [r, g, bl, fade * fade * a],
            );
        }
        let (color, softness, rise) = match struck {
            Struck::Steel => (linear(HIT_SMOKE), 0.2, 1.5),
            Struck::Sheet => (linear(STEEL_SOOT), 0.3, 1.2),
            Struck::Stone => (b.dust, 0.85, 0.8),
        };
        let heat = fire_heat(t, 9.0);
        if struck != Struck::Stone && burning(heat) > 0.0 {
            // A wall's burst blows out of its face, not back into it.
            let [ox, oz] = if wall { b.direction } else { [0.0; 2] };
            let size = (0.4 + 0.8 * t.sqrt()) * if wall { 1.5 } else { 1.0 };
            self.puff(Puff {
                position: [
                    b.x + ox * (0.2 + size * 0.6),
                    y + t * 0.6,
                    b.z + oz * (0.2 + size * 0.6),
                ],
                size,
                color: scaled(color, 0.6),
                opacity: (t / 0.02).min(1.0) * burning(heat),
                seed: b.phase * 0.91,
                heat,
                erosion: (1.0 - smoothstep(heat, 0.3, 1.2)) * 0.8,
                softness: 0.0,
                underglow: 0.0,
            });
        }
        let stone = struck == Struck::Stone;
        let life = if stone {
            STONE_HIT_LIFETIME
        } else {
            HIT_LIFETIME
        };
        if struck == Struck::Sheet {
            self.breach_smoke(b, t, y);
        }
        // A wall blows its grit or soot out of the face it struck, each billow
        // its own distance and size; a hull's smoke just rolls.
        let [bx, bz] = if wall { b.direction } else { [0.0; 2] };
        if stone {
            self.grit_fall(b, t, color);
        }
        // What the hit shakes off the wall settles round its foot in a ring.
        if wall {
            self.ring(b, t, if stone { 0.42 } else { 0.3 });
        }
        let billows = if wall { 4 } else { 3 };
        for j in 0..billows {
            let jf = j as f64;
            let age = t - 0.03 - jf * 0.04;
            if age <= 0.0 {
                continue;
            }
            let r = hash(b.phase * 1.3 + jf * 0.77);
            let angle = b.phase + jf * TAU / 3.0;
            let reach = (0.1 + 0.3 * settle(age, 4.0)) * if wall { 0.6 } else { 1.0 };
            let blown = (0.5 + 0.9 * r) * settle(age, 5.0) * (1.0 - 0.2 * jf);
            let size = match struck {
                Struck::Stone => (0.45 + 1.25 * age.sqrt()) * (0.75 + 0.5 * r),
                Struck::Sheet => (0.4 + 0.95 * age.sqrt()) * (0.8 + 0.4 * r),
                Struck::Steel => 0.4 + 0.9 * age.sqrt(),
            };
            // Held a little over its own radius out of the face, so the wall's
            // soft fade thins only the billow's far edge, never the whole of it.
            let out = blown + size * 0.55;
            self.puff(Puff {
                position: [
                    b.x + angle.cos() * reach + bx * out + WIND[0] * age * 0.6,
                    y + age * rise * (0.7 + 0.3 * jf),
                    b.z + angle.sin() * reach + bz * out + WIND[1] * age * 0.6,
                ],
                size,
                color: scaled(color, 0.9 + jf * 0.07),
                opacity: (age / 0.05).min(1.0)
                    * if stone { 0.7 } else { 0.6 }
                    * (1.0 - smoothstep(age, 0.25, life)),
                seed: b.phase * 0.61 + jf * 0.23,
                heat: 0.0,
                erosion: smoothstep(age, 0.2, life) * 0.9,
                softness,
                underglow: if j == 0 {
                    0.7 * (-age * 6.0).exp()
                } else {
                    0.0
                },
            });
        }
    }

    /// Thin smoke curling out of a container's breach as its scorched paint
    /// smoulders: wisps that rise along the face and drift off on the wind.
    fn breach_smoke(&mut self, b: &Blast, t: f64, y: f64) {
        let fade = smoothstep(t, 0.25, 0.6) * (1.0 - smoothstep(t, 1.6, SHEET_HIT_LIFETIME));
        if fade <= 0.01 {
            return;
        }
        let [bx, bz] = b.direction;
        let soot = linear(STEEL_SOOT);
        for j in 0..BREACH_WISPS {
            let jf = j as f64;
            // One rate for every wisp, each with its own phase.
            let a = (t / WISP_CYCLE + jf / BREACH_WISPS as f64).fract();
            let size = 0.25 + 0.7 * a;
            self.puff(Puff {
                position: [
                    b.x + bx * (0.15 + size * 0.5) + WIND[0] * a * 1.2,
                    y + 0.1 + 1.6 * a,
                    b.z + bz * (0.15 + size * 0.5) + WIND[1] * a * 1.2,
                ],
                size,
                color: scaled(soot, 0.9 + 0.5 * a),
                opacity: fade * smoothstep(a, 0.0, 0.15) * (1.0 - smoothstep(a, 0.3, 1.0)) * 0.55,
                seed: b.phase * 0.83 + jf * 0.29,
                heat: 0.0,
                erosion: smoothstep(a, 0.2, 1.0) * 0.8,
                softness: 0.5,
                underglow: 0.0,
            });
        }
    }

    /// Grit shaken off a struck wall pouring down its face and pooling at its
    /// foot.
    fn grit_fall(&mut self, b: &Blast, t: f64, color: [f64; 3]) {
        let [bx, bz] = b.direction;
        for j in 0..2 {
            let jf = j as f64;
            let age = t - 0.12 - jf * 0.1;
            if age <= 0.0 {
                continue;
            }
            let side = if j == 0 { 1.0 } else { -1.0 };
            let spread = 0.3 + 0.6 * settle(age, 2.5);
            let size = 0.45 + 0.9 * age.sqrt();
            self.puff(Puff {
                position: [
                    b.x + bx * (0.35 + 0.3 * settle(age, 3.0)) - bz * side * spread,
                    0.15 + size * 0.35,
                    b.z + bz * (0.35 + 0.3 * settle(age, 3.0)) + bx * side * spread,
                ],
                size,
                color: scaled(color, 1.05),
                opacity: (age / 0.15).min(1.0)
                    * 0.5
                    * (1.0 - smoothstep(age, 0.4, STONE_HIT_LIFETIME - 0.22)),
                seed: b.phase * 0.37 + jf * 0.41,
                heat: 0.0,
                erosion: smoothstep(age, 0.3, STONE_HIT_LIFETIME) * 0.85,
                softness: 0.6,
                underglow: 0.0,
            });
        }
    }

    /// The shockwave's dust front racing out over the ground.
    fn shock(&mut self, b: &Blast, t: f64, s: f64) {
        if t >= SHOCK_SECONDS {
            return;
        }
        let radius = ((SHOCK_START + SHOCK_SPREAD * settle(t, 7.0)) * s) as f32;
        let reach = (SHOCK_START + SHOCK_SPREAD) * s;
        let y = self.pads.decal_height(b.x, b.z, reach, RING_HEIGHT + 0.004) as f32;
        let world = Mat4::from_translation(Vec3::new(b.x as f32, y, b.z as f32))
            * Mat4::from_scale(Vec3::new(radius * 2.0, 1.0, radius * 2.0));
        let strength = (1.0 - t / SHOCK_SECONDS).powi(2);
        let [r, g, bl] = b.dust.map(|c| c as f32);
        self.rings.push(record(
            world,
            [r, g, bl, 1.0],
            [t as f32, b.phase as f32, SHOCK_RING, strength as f32],
        ));
    }

    /// The blast's dusty ground ring over its first moments, spreading at scale
    /// `s`, in the ground's dust color.
    fn ring(&mut self, b: &Blast, t: f64, s: f64) {
        if t >= RING_SECONDS {
            return;
        }
        let radius = ((RING_START + RING_SPREAD * settle(t, 5.0)) * s) as f32;
        // One height for the ring's whole spread, so it never steps mid-blast.
        let reach = (RING_START + RING_SPREAD) * s;
        let y = self.pads.decal_height(b.x, b.z, reach, RING_HEIGHT) as f32;
        let world = Mat4::from_translation(Vec3::new(b.x as f32, y, b.z as f32))
            * Mat4::from_scale(Vec3::new(radius * 2.0, 1.0, radius * 2.0));
        let [r, g, bl] = b.dust.map(|c| c as f32);
        self.rings.push(record(
            world,
            [r, g, bl, 1.0],
            [t as f32, b.phase as f32, DUST_RING, 1.0],
        ));
    }

    /// The dust a falling cottage drives out as its roof comes down: a skirt of
    /// low billows born round the footprint as the roof lands, rolling outward
    /// along the ground and spreading as they thin, a darker billow of plaster
    /// and soot boiling up inside the footprint and hanging over the pile, and a
    /// column of plaster dust rising slowly off the heap and drifting downwind.
    /// Everything settles over seconds; the wreck shows through from the first.
    fn house_dust(&mut self, b: &Blast, t: f64, lifetime: f64, dust: [f64; 3]) {
        for j in 0..SURGE_BILLOWS {
            let jf = j as f64;
            let r1 = hash(b.phase * 0.71 + jf * 1.913);
            let r2 = hash(b.phase * 1.37 + jf * 0.557);
            let r3 = hash(b.phase * 2.29 + jf * 1.181);
            let age = t - SURGE_AT.0 - r1 * SURGE_AT.1;
            if age <= 0.0 {
                continue;
            }
            // Every other billow rolls along the ground, dark with the soil and
            // rubble it scours up; the rest climb off it paler and thinner.
            let ground = j % 2 == 0;
            // Spread round the footprint, bunched a little so the skirt is lumpy
            // rather than an even ring.
            let angle = b.phase * 0.9 + jf * TAU / SURGE_BILLOWS as f64 + (r2 - 0.5) * 0.9;
            let roll = settle(age, SURGE_RATE * (0.8 + 0.4 * r3));
            let reach = HOUSE_FOOT * (0.55 + 0.25 * r3)
                + SURGE_REACH * (0.6 + 0.4 * r2) * roll * if ground { 1.0 } else { 0.7 };
            let size = if ground {
                1.0 + 0.5 * r2 + 1.4 * roll
            } else {
                0.8 + 0.4 * r2 + 1.0 * roll
            };
            let squat = if ground { SURGE_SQUAT.0 } else { SURGE_SQUAT.1 };
            let end = 2.6 + 0.8 * r3;
            let climb = if ground { 0.15 } else { 1.0 };
            push_stretched_puff(
                &mut self.puffs,
                Puff {
                    position: [
                        b.x + angle.cos() * reach + WIND[0] * age * 0.3,
                        0.08 + size * squat * 0.32
                            + SURGE_RISE * climb * settle(age, 0.9) * (0.5 + 0.5 * r1),
                        b.z + angle.sin() * reach + WIND[1] * age * 0.3,
                    ],
                    size,
                    // Darker where it boils off the ground, paler as it climbs.
                    color: scaled(
                        dust,
                        if ground { 0.58 } else { 0.8 } + 0.14 * r3 + 0.12 * settle(age, 1.2),
                    ),
                    opacity: (age / 0.08).min(1.0)
                        * if ground {
                            0.36 + 0.1 * r1
                        } else {
                            0.22 + 0.08 * r1
                        }
                        * (1.0 - smoothstep(age, 0.6, end)),
                    seed: b.phase * 0.53 + jf * 0.173,
                    heat: 0.0,
                    erosion: smoothstep(age, 0.2, end) * 0.9,
                    // Lumpy, rim-lit billows rather than soft cotton.
                    softness: if ground { 0.3 } else { 0.45 } + 0.15 * r1,
                    underglow: 0.0,
                },
                squat,
            );
        }
        for j in 0..HOUSE_BILLOWS {
            let jf = j as f64;
            let r1 = hash(b.phase * 1.31 + jf * 2.113);
            let r2 = hash(b.phase * 0.77 + jf * 0.919);
            let r3 = hash(b.phase * 2.03 + jf * 1.447);
            // Burst where a wall strikes the ground, one after another as the
            // walls go down round the footprint.
            let age = t - HOUSE_IMPACT - r1 * HOUSE_LANDINGS;
            if age <= 0.0 {
                continue;
            }
            let angle = b.phase + jf * TAU / HOUSE_BILLOWS as f64 + (r2 - 0.5) * 0.5;
            let roll = settle(age, 1.2 + 0.4 * r2);
            let reach = HOUSE_FOOT * (0.8 + 0.3 * r3) + HOUSE_ROLL * roll;
            let size = 0.5 + 0.25 * r2 + 0.9 * roll;
            let end = 3.2 + 1.2 * r3;
            push_stretched_puff(
                &mut self.puffs,
                Puff {
                    position: [
                        b.x + angle.cos() * reach + WIND[0] * age * 0.3,
                        0.1 + size * 0.24 + 0.35 * settle(age, 0.8),
                        b.z + angle.sin() * reach + WIND[1] * age * 0.3,
                    ],
                    size,
                    color: scaled(dust, 0.78 + 0.22 * r3),
                    opacity: (age / 0.1).min(1.0)
                        * (0.24 + 0.1 * r1)
                        * (1.0 - smoothstep(age, 0.8, end)),
                    seed: b.phase * 0.43 + jf * 0.191,
                    heat: 0.0,
                    erosion: smoothstep(age, 0.4, end) * 0.85,
                    softness: 0.4 + 0.2 * r1,
                    underglow: 0.0,
                },
                SURGE_SQUAT.1,
            );
        }
        for j in 0..HOUSE_CORE {
            let jf = j as f64;
            let r1 = hash(b.phase * 0.83 + jf * 1.771);
            let r2 = hash(b.phase * 1.57 + jf * 0.613);
            let r3 = hash(b.phase * 2.41 + jf * 2.293);
            // Born as the walls fold and the roof lands, over the pile.
            let age = t - HOUSE_IMPACT * (0.4 + 0.7 * r1);
            if age <= 0.0 {
                continue;
            }
            let angle = b.phase * 1.7 + jf * TAU / HOUSE_CORE as f64 + (r2 - 0.5) * 0.6;
            let spread = HOUSE_FOOT * (0.25 + 0.35 * r3) + 0.5 * settle(age, 0.8);
            let size = 0.6 + 0.3 * r2 + 0.5 * settle(age, 0.7);
            let end = 3.0 + 1.0 * r3;
            self.puff(Puff {
                position: [
                    b.x + angle.cos() * spread + WIND[0] * age * 0.35,
                    0.35 + size * 0.25 + 0.6 * settle(age, 0.5) * r1,
                    b.z + angle.sin() * spread + WIND[1] * age * 0.35,
                ],
                size,
                color: scaled(dust, 0.62 + 0.14 * r2),
                opacity: (age / 0.2).min(1.0)
                    * (0.18 + 0.06 * r3)
                    * (1.0 - smoothstep(age, 0.8, end)),
                seed: b.phase * 0.61 + jf * 0.237,
                heat: 0.0,
                erosion: smoothstep(age, 0.4, end) * 0.85,
                softness: 0.5 + 0.2 * r1,
                underglow: 0.0,
            });
        }
        for j in 0..HOUSE_PALL {
            let jf = j as f64;
            let r1 = hash(b.phase * 0.59 + jf * 3.71);
            let r2 = hash(b.phase * 1.93 + jf * 0.37);
            let age = t - HOUSE_IMPACT * 0.8 - jf * 0.16;
            if age <= 0.0 {
                continue;
            }
            let angle = b.phase * 2.0 + jf * 2.399;
            let spread = (0.4 + 1.1 * r1) * (0.6 + 0.4 * settle(age, 1.0));
            let end = lifetime - jf * 0.3;
            self.puff(Puff {
                position: [
                    b.x + angle.cos() * spread + WIND[0] * age * 0.6,
                    1.0 + 0.5 * age + 0.4 * r2,
                    b.z + angle.sin() * spread + WIND[1] * age * 0.6,
                ],
                size: 0.9 + 0.4 * age + 0.4 * r2,
                color: scaled(dust, 0.92 + 0.15 * r1),
                opacity: (age / 0.35).min(1.0) * 0.12 * (1.0 - smoothstep(age, 1.2, end)),

                seed: b.phase * 0.29 + jf * 0.311,
                heat: 0.0,
                erosion: smoothstep(age, 0.6, end) * 0.85,
                softness: 0.8,
                underglow: 0.0,
            });
        }
    }

    /// A cargo crate's dust, in the blast's puff slots.
    fn wood_dust(&mut self, b: &Blast, wood: WoodDust, t: f64, lifetime: f64) {
        let s = b.scale;
        // Without a known direction, the blast's own phase picks one.
        let [dx, dz] = if b.direction == [0.0; 2] {
            [b.phase.cos(), b.phase.sin()]
        } else {
            b.direction
        };
        let fade = 1.0 - smoothstep(t, lifetime * 0.25, lifetime);
        let erosion = smoothstep(t, lifetime * 0.2, lifetime) * 0.8;
        match wood {
            WoodDust::Hit => {
                // Three small puffs blown out of the face, fanning along it.
                for j in 0..3 {
                    let side = j as f64 - 1.0;
                    let age = t - j as f64 * 0.025;
                    if age <= 0.0 {
                        continue;
                    }
                    let reach = 0.1 + 0.7 * settle(age, 4.5) * (1.0 - 0.25 * side.abs());
                    let fan = side * (0.1 + age * 0.4);
                    self.puff(Puff {
                        position: [
                            b.x + dx * reach - dz * fan,
                            b.y + age * 0.3 - side.abs() * 0.08,
                            b.z + dz * reach + dx * fan,
                        ],
                        size: 0.2 + 0.45 * age.sqrt(),
                        color: scaled(linear(SAWDUST), 0.95 + side * 0.04),
                        opacity: (age / 0.04).min(1.0) * fade * 0.5,
                        seed: b.phase * 0.4 + j as f64 * 0.3,
                        heat: 0.0,
                        erosion,
                        softness: 0.6,
                        underglow: 0.0,
                    });
                }
            }
            WoodDust::Burst => {
                // Five billows roll out low from under the crate, held clear of the
                // ground they would cut into...
                for j in 0..5 {
                    let jf = j as f64;
                    let age = t - jf * 0.02;
                    if age <= 0.0 {
                        continue;
                    }
                    let angle = b.phase + jf * TAU / 5.0 + (j % 2) as f64 * 0.3;
                    let roll = (0.8 + 1.7 * settle(age, 3.5)) * s;
                    let drift = age * 0.45 * s;
                    let size = (0.95 + 0.9 * age.sqrt()) * s;
                    self.puff(Puff {
                        position: [
                            b.x + angle.cos() * roll + dx * drift,
                            0.3 * s + size * 0.45,
                            b.z + angle.sin() * roll + dz * drift,
                        ],
                        size,
                        color: scaled(linear(SAWDUST), 0.88 + jf * 0.035),
                        opacity: (age / 0.08).min(1.0) * fade * 0.58,
                        seed: b.phase * 0.2 + jf * 0.19,
                        heat: 0.0,
                        erosion,
                        softness: 0.5,
                        underglow: 0.0,
                    });
                }
                // ...a jet blows on out of the far side along the shot...
                let jet = (0.5 + 2.6 * settle(t, 3.2)) * s;
                self.puff(Puff {
                    position: [b.x + dx * jet, (0.45 + t * 0.15) * b.y, b.z + dz * jet],
                    size: (0.55 + 0.8 * t.sqrt()) * s,
                    color: linear(SAWDUST_LIGHT),
                    opacity: (t / 0.05).min(1.0) * fade * 0.45,
                    seed: b.phase * 0.9,
                    heat: 0.0,
                    erosion,
                    softness: 0.5,
                    underglow: 0.0,
                });
                // ...and a plume rises over the wreck.
                for j in 0..2 {
                    let jf = j as f64;
                    let age = t - 0.06 - jf * 0.08;
                    if age <= 0.0 {
                        continue;
                    }
                    let angle = b.phase + PI * jf;
                    self.puff(Puff {
                        position: [
                            b.x + angle.cos() * 0.35 * s + dx * age * 0.6,
                            b.y * (0.5 + jf * 0.2) + age * 0.65 * s,
                            b.z + angle.sin() * 0.35 * s + dz * age * 0.6,
                        ],
                        size: (0.5 + 0.7 * age.sqrt()) * s,
                        color: scaled(linear(SAWDUST_LIGHT), 0.97 - jf * 0.05),
                        opacity: (age / 0.1).min(1.0) * fade * 0.36,
                        seed: b.phase * 0.6 + jf * 0.41,
                        heat: 0.0,
                        erosion,
                        softness: 0.6,
                        underglow: 0.0,
                    });
                }
                // ...and a low ring of it settles on the ground round the wreck,
                // lingering and drifting off on the wind.
                for j in 0..SETTLING_DUST {
                    let jf = j as f64;
                    let age = t - 0.25 - jf * 0.06;
                    if age <= 0.0 {
                        continue;
                    }
                    let angle = b.phase + 0.7 + jf * TAU / SETTLING_DUST as f64;
                    let roll = (1.4 + 1.2 * settle(age, 1.4)) * s;
                    let size = (1.0 + 0.55 * age.sqrt()) * s;
                    // Its ring is blown on along the shot like the rest of the cloud.
                    let blown = (SETTLING_BLOWN + 0.4 * age) * s;
                    self.puff(Puff {
                        position: [
                            b.x + angle.cos() * roll + dx * blown + WIND[0] * age * 0.35,
                            0.25 * s + size * 0.3,
                            b.z + angle.sin() * roll + dz * blown + WIND[1] * age * 0.35,
                        ],
                        size,
                        color: scaled(linear(SAWDUST), 0.92 + 0.04 * jf),
                        opacity: (age / 0.3).min(1.0)
                            * 0.3
                            * (1.0 - smoothstep(t, lifetime * 0.55, lifetime)),
                        seed: b.phase * 0.33 + jf * 0.29,
                        heat: 0.0,
                        erosion: smoothstep(t, lifetime * 0.35, lifetime) * 0.8,
                        softness: 0.8,
                        underglow: 0.0,
                    });
                }
                self.ring(b, t, WOOD_RING_SCALE * s);
            }
        }
    }
}

/// Puffs between the camera and the player's tank thin out to this share of
/// their opacity, so a blast's smoke never hides the player.
const SEE_THROUGH: f32 = 0.08;
/// The sight line is kept clear this wide around its axis (m): a hull and the
/// ground around it.
const SIGHT_RADIUS: f32 = 3.0;

/// Thin every puff in `puffs` that covers the sight line from `eye` to `target`
/// (the player's tank), most where the line passes through its middle.
pub fn clear_sight_line(puffs: &mut PoolBuffer, eye: Vec3, target: Vec3) {
    let sight = target - eye;
    let length_squared = sight.length_squared();
    if length_squared < 1e-6 {
        return;
    }
    for puff in puffs.records_mut() {
        let center = puff.translation();
        let along = ((center - eye).dot(sight) / length_squared).clamp(0.0, 1.0);
        let miss = center.distance(eye + sight * along);
        // The card spans its scale either side of the centre.
        let radius = Vec3::from_slice(&puff.world_rows[0][..3]).length();
        let t = ((miss - SIGHT_RADIUS) / radius).clamp(0.0, 1.0);
        let clear = t * t * (3.0 - 2.0 * t);
        puff.tint[3] *= SEE_THROUGH + (1.0 - SEE_THROUGH) * clear;
    }
}

/// A puff written by another effect (muzzle smoke, burning wrecks) into the
/// shared puff pool, with the same look as the blasts' puffs.
#[allow(clippy::too_many_arguments)]
pub fn smoke_puff(
    pool: &mut PoolBuffer,
    position: Vec3,
    size: f32,
    color: [f32; 3],
    opacity: f32,
    seed: f32,
    heat: f32,
    erosion: f32,
    softness: f32,
    underglow: f32,
) {
    push_puff(
        pool,
        Puff {
            position: position.to_array().map(f64::from),
            size: f64::from(size),
            color: color.map(f64::from),
            opacity: f64::from(opacity),
            seed: f64::from(seed),
            heat: f64::from(heat),
            erosion: f64::from(erosion),
            softness: f64::from(softness),
            underglow: f64::from(underglow),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: SimEventType, size: Option<f64>) -> SimEvent {
        SimEvent {
            size,
            ..SimEvent::at(kind, 0.0, 0.0)
        }
    }

    fn y(pool: &PoolBuffer, index: usize) -> f32 {
        pool.records()[index].translation().y
    }

    fn update(effects: &mut ExplosionEffects, dt: f64) {
        let mut streaks = PoolBuffer::new(256);
        effects.update(dt, &mut streaks);
    }

    fn heat(pool: &PoolBuffer) -> f32 {
        pool.records().iter().map(|r| r.data[1]).fold(0.0, f32::max)
    }

    fn rings_of(pool: &PoolBuffer, kind: f32) -> usize {
        pool.records().iter().filter(|r| r.data[2] == kind).count()
    }

    #[test]
    fn blast_sequence_burns_cools_into_smoke_then_completely_retires() {
        let mut random = CosmeticRandom::default();
        let mut effects = ExplosionEffects::default();
        let mut streaks = PoolBuffer::new(16);
        effects.event(
            &SimEvent {
                size: Some(5.0),
                ..SimEvent::at(SimEventType::Explosion, 4.0, -2.0)
            },
            &mut random,
        );
        effects.update(0.03, &mut streaks);
        assert_eq!(
            streaks.len(),
            4,
            "the flash core, its bloom, the flash's and the fire's light on the ground"
        );
        assert!(heat(&effects.puffs) > 1.0, "a white-hot fireball");
        assert_eq!(rings_of(&effects.rings, SHOCK_RING), 1);
        assert_eq!(rings_of(&effects.rings, DUST_RING), 1);
        assert_eq!(rings_of(&effects.rings, SCORCH), 1);
        let mut lights = [Flash::default(); 3];
        effects.lights(&mut lights);
        assert!(lights.iter().any(|light| light.intensity > 10.0));
        streaks.clear();
        effects.update(0.6, &mut streaks);
        assert!(heat(&effects.puffs) < 0.3, "the fire has cooled into smoke");
        assert_eq!(rings_of(&effects.rings, SHOCK_RING), 0);
        assert!(streaks.is_empty(), "the fire has burnt out");
        assert!(effects.puffs.len() > 10);
        // The smoke has risen well clear of the ground on the fire's heat.
        let top = (0..effects.puffs.len())
            .map(|i| y(&effects.puffs, i))
            .fold(0.0, f32::max);
        assert!(top > 2.2, "{top}");
        update(&mut effects, EXPLOSION_LIFETIME);
        assert_eq!(effects.puffs.len(), 0);
        assert_eq!(effects.active(), 0);
        assert_eq!(rings_of(&effects.rings, SCORCH), 1, "the scorch lingers");
        update(&mut effects, SCORCH_LIFETIME);
        assert_eq!(effects.rings.len(), 0);
        assert_eq!(effects.scorches(), 0);
    }

    #[test]
    fn rings_ride_over_the_spawn_pads_they_reach_for_their_whole_spread() {
        for theme in ["village", "harbor", "quarry"] {
            let mut random = CosmeticRandom::default();
            let mut effects = ExplosionEffects {
                pads: SpawnPadDecks::new(theme, 1.0),
                ..ExplosionEffects::default()
            };
            // On a blue pad, and in the open centre of the arena.
            for (x, z) in [(-53.0, 0.0), (0.0, 30.0)] {
                effects.event(
                    &SimEvent {
                        size: Some(4.5),
                        ..SimEvent::at(SimEventType::Explosion, x, z)
                    },
                    &mut random,
                );
            }
            let deck = effects.pads.top_within(-53.0, 0.0, 0.0).expect("pad deck");
            let mut heights = Vec::new();
            for _ in 0..8 {
                update(&mut effects, 0.06);
                let dust: Vec<_> = effects
                    .rings
                    .records()
                    .iter()
                    .filter(|r| r.data[2] == DUST_RING)
                    .collect();
                assert_eq!(dust.len(), 2);
                let on_pad = dust[0].translation().y;
                assert!(f64::from(on_pad) > deck + 0.015, "{theme} ring {on_pad}");
                heights.push(on_pad);
                assert!((f64::from(dust[1].translation().y) - RING_HEIGHT).abs() < 1e-6);
            }
            assert!(
                heights.iter().all(|&h| h == heights[0]),
                "{theme} ring steps"
            );
        }
    }

    #[test]
    fn chain_reactions_reuse_bounded_buffers_and_reset_without_leftovers() {
        let mut random = CosmeticRandom::default();
        let mut effects = ExplosionEffects::default();
        for i in 0..1000 {
            effects.event(
                &SimEvent {
                    size: Some(100.0),
                    ..SimEvent::at(SimEventType::Explosion, i as f64, 0.0)
                },
                &mut random,
            );
        }
        update(&mut effects, 0.18);
        assert_eq!(rings_of(&effects.rings, DUST_RING), MAX_EXPLOSIONS);
        assert_eq!(effects.scorches(), MAX_SCORCHES);
        assert!(effects.rings.len() <= DECAL_CAPACITY);
        assert!(effects.puffs.len() <= MAX_EXPLOSIONS * PUFFS_PER_BLAST);
        assert!(effects.puffs.len() >= MAX_EXPLOSIONS * 15);
        assert!(effects.puffs.records().iter().all(|r| {
            r.world_rows
                .as_flattened()
                .iter()
                .chain(&r.tint)
                .chain(&r.data)
                .all(|v| v.is_finite())
        }));
        effects.reset();
        update(&mut effects, 0.0);
        assert_eq!(effects.puffs.len(), 0);
        assert_eq!(effects.rings.len(), 0);
        assert_eq!(effects.puffs.capacity(), super::super::PUFF_CAPACITY);
    }

    #[test]
    fn tank_deaths_have_taller_darker_longer_smoke() {
        let mut random = CosmeticRandom::constant(0.5);
        let mut tank = ExplosionEffects::default();
        let mut shell = ExplosionEffects::default();
        tank.event(&event(SimEventType::Death, Some(3.0)), &mut random);
        shell.event(&event(SimEventType::Explosion, Some(3.0)), &mut random);
        update(&mut tank, 0.75);
        update(&mut shell, 0.75);
        let top = |pool: &PoolBuffer| (0..pool.len()).map(|i| y(pool, i)).fold(0.0, f32::max);
        assert!(top(&tank.puffs) > top(&shell.puffs) + 0.5);
        let darkest = |pool: &PoolBuffer| {
            pool.records()
                .iter()
                .map(|r| r.tint[0])
                .fold(f32::MAX, f32::min)
        };
        assert!(darkest(&tank.puffs) < darkest(&shell.puffs));
        let shell_cleanup = EXPLOSION_LIFETIME - 0.75 + 0.01;
        update(&mut tank, shell_cleanup);
        update(&mut shell, shell_cleanup);
        assert_eq!(shell.puffs.len(), 0);
        assert!(!tank.puffs.is_empty());
        let longest_tank_blast = TANK_BLASTS
            .iter()
            .map(|look| look.lifetime)
            .fold(0.0, f64::max);
        update(&mut tank, longest_tank_blast);
        assert_eq!(tank.puffs.len(), 0);
    }

    #[test]
    fn mixed_tank_deaths_and_blasts_still_fit_the_shared_pool() {
        let mut random = CosmeticRandom::default();
        let mut effects = ExplosionEffects::default();
        for i in 0..100 {
            let kind = if i % 2 == 1 {
                SimEventType::Death
            } else {
                SimEventType::Explosion
            };
            effects.event(&SimEvent::at(kind, i as f64, 0.0), &mut random);
        }
        update(&mut effects, 0.37);
        assert!(effects.puffs.len() <= MAX_EXPLOSIONS * PUFFS_PER_BLAST);
        assert!(effects.puffs.len() >= MAX_EXPLOSIONS * 15);
        assert_eq!(rings_of(&effects.rings, DUST_RING), MAX_EXPLOSIONS);
        effects.reset();
        effects.event(
            &SimEvent::at(SimEventType::Explosion, 0.0, 0.0),
            &mut random,
        );
        update(&mut effects, EXPLOSION_LIFETIME);
        assert_eq!(
            effects.puffs.len(),
            0,
            "reused tank slot must not retain its long lifetime"
        );
    }

    #[test]
    fn consecutive_tank_and_barrel_blasts_have_different_silhouettes() {
        for kind in [SimEventType::Death, SimEventType::Explosion] {
            let mut random = CosmeticRandom::default();
            let mut effects = ExplosionEffects::default();
            let mut signatures = Vec::new();
            for _ in 0..3 {
                let mut blast = event(kind, Some(3.0));
                if kind == SimEventType::Explosion {
                    blast.cover_kind = Some(CoverKind::Drum);
                }
                effects.event(&blast, &mut random);
                update(&mut effects, 0.45);
                signatures.push(
                    effects
                        .puffs
                        .records()
                        .iter()
                        .flat_map(|r| r.world_rows.into_iter().flatten())
                        .collect::<Vec<_>>(),
                );
                update(&mut effects, 4.0);
            }
            assert_ne!(signatures[0], signatures[1]);
            assert_ne!(signatures[1], signatures[2]);
        }
    }

    #[test]
    fn shell_hits_flash_and_puff_without_scorch_and_only_walls_shed_dust_rings() {
        let mut random = CosmeticRandom::constant(0.5);
        let mut effects = ExplosionEffects::default();
        let mut streaks = PoolBuffer::new(16);
        let mut steel = SimEvent::at(SimEventType::Impact, 0.0, 0.0);
        steel.height = Some(1.0);
        effects.event(&steel, &mut random);
        effects.event(&SimEvent::at(SimEventType::Ricochet, 3.0, 0.0), &mut random);
        effects.update(0.02, &mut streaks);
        assert_eq!(streaks.len(), 4, "each a hot point and its bloom");
        assert_eq!(
            effects.puffs.len(),
            1,
            "steel bursts into fire; stone only into powder"
        );
        assert_eq!(
            rings_of(&effects.rings, DUST_RING),
            1,
            "the wall's grit settles round its foot; the hull sheds none"
        );
        assert_eq!(effects.scorches(), 0);
        update(&mut effects, STONE_HIT_LIFETIME);
        assert_eq!(effects.active(), 0);
    }

    fn cargo(kind: SimEventType, height: f64) -> SimEvent {
        SimEvent {
            cover_kind: Some(CoverKind::Cargo),
            height: Some(height),
            // The shell flew toward +x.
            from: Some(sloppy_core::sim::Point3::new(-1.0, height, 0.0)),
            ..SimEvent::at(kind, 0.0, 0.0)
        }
    }

    #[test]
    fn crate_hits_puff_toward_the_gun_and_bursts_roll_a_lasting_cloud() {
        let mut random = CosmeticRandom::constant(0.5);
        let mut effects = ExplosionEffects::default();
        effects.event(&cargo(SimEventType::Impact, 1.2), &mut random);
        update(&mut effects, 0.3);
        assert_eq!(effects.rings.len(), 0, "a hit raises no ground ring");
        assert_eq!(effects.puffs.len(), 3);
        assert!(effects.puffs.records().iter().all(|r| {
            let at = r.translation();
            at.x < 0.0 && at.y > 1.0 && r.tint[3] < 0.6
        }));
        update(&mut effects, WOOD_HIT_LIFETIME);
        assert_eq!(effects.active(), 0);

        effects.event(&cargo(SimEventType::Destroy, 2.4), &mut random);
        update(&mut effects, 0.3);
        assert_eq!(
            effects.puffs.len(),
            8 + 1,
            "and the first of the settling dust"
        );
        assert_eq!(effects.rings.len(), 1);
        let drift = |effects: &ExplosionEffects| {
            let puffs = effects.puffs.records();
            puffs.iter().map(|r| r.translation().x).sum::<f32>() / puffs.len() as f32
        };
        assert!(drift(&effects) > 0.3, "the dust blows on along the shot");
        update(&mut effects, 1.15);
        assert_eq!(
            effects.puffs.len(),
            8 + SETTLING_DUST,
            "the cloud lingers and settles round the wreck"
        );
        assert!(drift(&effects) > 0.6);
        update(&mut effects, WOOD_BURST_LIFETIME);
        assert_eq!(effects.puffs.len(), 0);
        assert_eq!(effects.active(), 0);
    }

    #[test]
    fn trees_and_drums_destroyed_raise_no_fiery_blast() {
        let mut random = CosmeticRandom::default();
        let mut effects = ExplosionEffects::default();
        for kind in [CoverKind::Tree, CoverKind::Drum] {
            let mut destroy = SimEvent::at(SimEventType::Destroy, 0.0, 0.0);
            destroy.cover_kind = Some(kind);
            effects.event(&destroy, &mut random);
        }
        effects.event(&SimEvent::at(SimEventType::Hurt, 0.0, 0.0), &mut random);
        assert_eq!(effects.active(), 1, "only the felled tree's dust");
        assert!(
            effects
                .blasts
                .iter()
                .any(|b| b.active && b.kind == Kind::Landing)
        );
        let mut timber = SimEvent::at(SimEventType::Destroy, 0.0, 0.0);
        timber.cover_kind = Some(CoverKind::Timber);
        effects.event(&timber, &mut random);
        assert_eq!(effects.active(), 2);
    }

    #[test]
    fn a_collapsed_cottage_leaves_its_footprint_in_dust() {
        let mut random = CosmeticRandom::default();
        let mut effects = ExplosionEffects::default();
        let mut destroy = SimEvent::at(SimEventType::Destroy, 0.0, 0.0);
        destroy.cover_kind = Some(CoverKind::House);
        destroy.size = Some(7.0);
        effects.event(&destroy, &mut random);
        assert_eq!(effects.scorches(), FOOTPRINT_LOBES + 2);
        update(&mut effects, HOUSE_DUST_LIFETIME);
        assert_eq!(effects.active(), 0);
        assert_eq!(rings_of(&effects.rings, POWDER), FOOTPRINT_LOBES + 2);
    }
}
