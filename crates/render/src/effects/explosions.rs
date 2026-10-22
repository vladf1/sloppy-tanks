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
use sloppy_core::sim::{CoverKind, DeathStyle, SimEvent, SimEventType};

use super::Flash;
use super::ground::Ground;
use super::pool::{PoolBuffer, record};
use super::random::CosmeticRandom;
use super::sparks::{glow, streak, tongue};
use super::spawn_pad_decks::SpawnPadDecks;
use super::wood_chips::{CRATE_HEIGHT, HIT_HEIGHT};
use crate::color::hex_to_linear;

pub const MAX_EXPLOSIONS: usize = 40;
/// A plain blast's lifetime; tank and drum blasts take their look's.
pub const EXPLOSION_LIFETIME: f64 = 2.8;
pub const PUFFS_PER_BLAST: usize = 30;
/// Scorches left on the ground at once; the oldest makes room.
pub const MAX_SCORCHES: usize = 40;
/// Ground decals a frame may hold: a dust ring, a shockwave and a fire glow per
/// blast, every scorch, and a burning wreck's glow.
pub const DECAL_CAPACITY: usize =
    MAX_EXPLOSIONS * 3 + MAX_SCORCHES + super::wreck_fire::MAX_BURNING_WRECKS;
/// A scorch cools and then fades out over this many seconds.
pub const SCORCH_LIFETIME: f64 = 26.0;
/// A burnout's thin smoke outlasts a normal blast.
const BURNOUT_LIFETIME: f64 = 2.7;
/// A dust collapse (timber) and a shell hit's puff.
const COLLAPSE_LIFETIME: f64 = 2.2;
const HIT_LIFETIME: f64 = 1.1;
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
const FLASH_SECONDS: f64 = 0.13;
const FLASH_SIZE: f64 = 2.6;
const FLASH_COLOR: [f32; 3] = [4.2, 2.9, 1.5];
const BLOOM_SECONDS: f64 = 0.22;
const BLOOM_SIZE: f64 = 6.0;
const BLOOM_COLOR: [f32; 3] = [3.0, 1.1, 0.22];
/// Puffs are slightly flattened.
const PUFF_HEIGHT: f64 = 0.9;
/// The hottest a fireball puff starts (the puff shader's heat).
const FIRE_HEAT: f64 = 1.5;
/// Heat falls by this factor of e over a look's `fire_life`.
const COOLING: f64 = 3.0;
/// A fire puff burns out (thins to nothing from its rim) as its heat falls
/// through this range.
const BURN_OUT: [f64; 2] = [0.1, 0.42];
const FIRE_PUFFS: usize = 8;
const SMOKE_PUFFS: usize = 9;
const SKIRT_PUFFS: usize = 9;
/// The wind every blast's and wreck's smoke drifts on (m/s).
pub const WIND: [f64; 2] = [0.9, -0.35];
/// The light a blast throws on everything around it: peak intensity (times its
/// scale squared), reach (m) and height over the blast (m, times its scale).
const BLAST_LIGHT: f64 = 34.0;
pub const BLAST_LIGHT_DISTANCE: f32 = 15.0;
const LIGHT_HEIGHT: f64 = 2.4;
/// A shell hit's brief light.
const HIT_LIGHT: f64 = 12.0;
/// The light's color at the white flash and through the fire.
const FLASH_LIGHT_COLOR: u32 = 0xffd6a0;
const FIRE_LIGHT_COLOR: u32 = 0xff8436;
/// The fire's glow on the ground: diameter (m, times the scale) and HDR color.
const GROUND_GLOW_SIZE: f64 = 8.0;
const GROUND_GLOW_COLOR: [f32; 3] = [2.6, 0.95, 0.22];
/// A tank's cook-off jet: how long its tongue of flame roars up, and its color.
const JET_SECONDS: f64 = 0.45;
const JET_COLOR: [f32; 3] = [4.2, 1.6, 0.3];
/// The decal shader's kinds (`blast_ring.wgsl`).
const DUST_RING: f32 = 0.0;
const SHOCK_RING: f32 = 1.0;
const SCORCH: f32 = 2.0;
pub(super) const GROUND_GLOW: f32 = 3.0;

const SMOKE: u32 = 0x76787c;
const TANK_SMOKE: u32 = 0x5c5e62;
const DUST: u32 = 0xaa9273;
/// A struck tank's oily smoke and a struck wall's grit.
const HIT_SMOKE: u32 = 0x55524f;
const GRIT: u32 = 0xb4ada2;

/// Wood dust: a struck face's puff and a burst crate's cloud last this long.
const WOOD_HIT_LIFETIME: f64 = 0.9;
const WOOD_BURST_LIFETIME: f64 = 2.1;
/// Warm sawdust rolling low, and the paler dust that rises and blows out.
const SAWDUST: u32 = 0xbda483;
const SAWDUST_LIGHT: u32 = 0xcfbc9d;
/// A burst's ring spreads at this scale; its cloud scales with the crate's height.
const WOOD_RING_SCALE: f64 = 0.6;

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
    Wood(WoodDust),
    Hit(Struck),
}

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
    /// Half the fireball bursts a beat later.
    double_burst: bool,
}

const BLAST: Look = Look {
    lifetime: EXPLOSION_LIFETIME,
    fire_size: 1.32,
    fire_spread: 1.15,
    fire_life: 0.62,
    fire_rise: 1.3,
    smoke: SMOKE,
    smoke_size: 1.0,
    smoke_rise: 2.6,
    drift: 0.35,
    double_burst: false,
};

// Distinct silhouettes and timing: a tall column, a wide rolling cloud and a
// drifting plume, cycled so consecutive deaths differ.
const TANK_BLASTS: [Look; 3] = [
    Look {
        lifetime: 3.4,
        fire_size: 1.15,
        fire_spread: 0.8,
        fire_life: 0.85,
        fire_rise: 2.0,
        smoke: TANK_SMOKE,
        smoke_size: 1.15,
        smoke_rise: 3.6,
        drift: 0.3,
        double_burst: true,
    },
    Look {
        lifetime: 3.0,
        fire_size: 1.3,
        fire_spread: 1.3,
        fire_life: 0.8,
        fire_rise: 0.9,
        smoke: 0x605c58,
        smoke_size: 1.25,
        smoke_rise: 2.4,
        drift: 0.25,
        double_burst: false,
    },
    Look {
        lifetime: 3.2,
        fire_size: 1.1,
        fire_spread: 1.0,
        fire_life: 0.8,
        fire_rise: 1.4,
        smoke: 0x5e6266,
        smoke_size: 1.1,
        smoke_rise: 3.0,
        drift: 1.4,
        double_burst: false,
    },
];

const BARREL_BLASTS: [Look; 3] = [
    Look {
        lifetime: 2.6,
        fire_size: 1.05,
        fire_spread: 1.15,
        fire_life: 0.8,
        fire_rise: 0.9,
        smoke: 0x5e5852,
        smoke_size: 1.15,
        smoke_rise: 2.8,
        drift: 0.3,
        double_burst: false,
    },
    Look {
        lifetime: 2.6,
        fire_size: 0.92,
        fire_spread: 0.85,
        fire_life: 0.72,
        fire_rise: 1.3,
        smoke: 0x5c5c5e,
        smoke_size: 1.0,
        smoke_rise: 3.0,
        drift: 1.5,
        double_burst: false,
    },
    Look {
        lifetime: 2.8,
        fire_size: 0.95,
        fire_spread: 0.9,
        fire_life: 0.8,
        fire_rise: 1.5,
        smoke: 0x625e5a,
        smoke_size: 1.05,
        smoke_rise: 3.2,
        drift: 0.6,
        double_burst: true,
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
    tempo: f64,
    /// Linear color of the ground's dust where it went off.
    dust: [f64; 3],
    /// A wood blast's or hit's height (the struck point, or the crate's top) and
    /// its unit horizontal direction (out of the struck face, or along the
    /// killing shot), zero when unknown.
    y: f64,
    direction: [f64; 2],
}

impl Blast {
    fn look(&self) -> Look {
        match self.kind {
            Kind::Tank => TANK_BLASTS[self.variant],
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
            Kind::Hit(_) => HIT_LIFETIME,
            Kind::Blast | Kind::Tank | Kind::Barrel => self.look().lifetime,
        }
    }

    /// How brightly the blast lights its surroundings `t` seconds in: the
    /// flash's spike, then the flickering fireball until it burns out.
    fn fire_light(&self, t: f64, look: &Look) -> f64 {
        let flash = (-t * 16.0).exp();
        let fire = burning(fire_heat(t, COOLING / look.fire_life));
        let flicker =
            0.85 + 0.15 * (t * 37.0 + self.phase * 5.0).sin() * (t * 23.0 + self.phase).sin();
        0.7 * flash + 0.6 * fire * flicker
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
    /// The ground's dust, for the rim of dirt the blast threw out.
    dust: [f64; 3],
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
    if puff.size <= 0.0 || puff.opacity <= 0.003 {
        return;
    }
    // Billboards ignore rotation, so the pose is translation × scale only.
    let world = Mat4::from_scale_rotation_translation(
        Vec3::new(
            puff.size as f32,
            (puff.size * PUFF_HEIGHT) as f32,
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
    scorches: [Scorch; MAX_SCORCHES],
    scorch_count: usize,
    scorch_cursor: usize,
    pub puffs: PoolBuffer,
    pub rings: PoolBuffer,
    /// The current map's spawn pads, which rings on or beside them ride over.
    pub pads: SpawnPadDecks,
    /// The current map's ground, for the dust blasts kick up.
    pub ground: Ground,
}

impl Default for ExplosionEffects {
    fn default() -> Self {
        Self {
            blasts: [Blast::default(); MAX_EXPLOSIONS],
            cursor: 0,
            tank_sequence: 0,
            barrel_sequence: 0,
            scorches: [Scorch::default(); MAX_SCORCHES],
            scorch_count: 0,
            scorch_cursor: 0,
            puffs: PoolBuffer::new(super::PUFF_CAPACITY),
            rings: PoolBuffer::new(DECAL_CAPACITY),
            pads: SpawnPadDecks::default(),
            ground: Ground::default(),
        }
    }
}

impl ExplosionEffects {
    pub fn event(&mut self, event: &SimEvent, random: &mut CosmeticRandom) {
        let fire = matches!(event.kind, SimEventType::Explosion | SimEventType::Death);
        let wood = match (event.kind, event.cover_kind) {
            (SimEventType::Impact, Some(CoverKind::Cargo))
            | (SimEventType::Shatter, Some(CoverKind::Cargo | CoverKind::Tower)) => {
                Some(WoodDust::Hit)
            }
            (SimEventType::Destroy, Some(CoverKind::Cargo)) => Some(WoodDust::Burst),
            _ => None,
        };
        // A shell's hit on a tank (it names a height) or on hard cover (none).
        let struck = match event.kind {
            SimEventType::Impact if event.cover_kind.is_none() => Some(if event.height.is_some() {
                Struck::Steel
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
            (size / 3.0) * (1.15 + random.next_f64() * 0.25)
        } else {
            (0.42 + 0.1 * size).clamp(0.55, 1.2)
        };
        if barrel {
            blast.scale *= 0.88 + random.next_f64() * 0.25;
        }
        blast.phase = random.next_f64() * TAU;
        if let Kind::Hit(_) = kind {
            blast.scale = 1.0;
            blast.y = event.height.unwrap_or(1.0);
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
        // Fiery blasts char the ground where they went off.
        let scorch = match kind {
            Kind::Blast | Kind::Barrel => Some(4.2),
            Kind::Tank => Some(5.0),
            Kind::Burnout => Some(2.6),
            _ => None,
        };
        if let Some(diameter) = scorch {
            let (x, z, phase, dust) = (blast.x, blast.z, blast.phase, blast.dust);
            let diameter = diameter * blast.scale;
            let y = self.pads.decal_height(x, z, diameter * 0.5, SCORCH_HEIGHT);
            self.scorches[self.scorch_cursor] = Scorch {
                x,
                z,
                y,
                age: 0.0,
                diameter,
                phase,
                dust,
            };
            self.scorch_cursor = (self.scorch_cursor + 1) % MAX_SCORCHES;
            self.scorch_count = (self.scorch_count + 1).min(MAX_SCORCHES);
        }
    }

    pub fn reset(&mut self) {
        for blast in &mut self.blasts {
            blast.active = false;
        }
        self.cursor = 0;
        self.tank_sequence = 0;
        self.barrel_sequence = 0;
        self.scorch_count = 0;
        self.scorch_cursor = 0;
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
                    BLAST_LIGHT * s * s * blast.fire_light(t, &blast.look()),
                    LIGHT_HEIGHT * s,
                ),
                Kind::Burnout => (BLAST_LIGHT * 0.25 * s * s * (-t * 5.0).exp(), 1.2 * s),
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
        self.puffs.clear();
        self.rings.clear();
        self.scorch_decals(dt);
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
                    SCORCH,
                    (strength * appear) as f32,
                ],
            ));
        }
        // Expired scorches are always the oldest ones.
        self.scorch_count -= expired;
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
            Kind::Burnout => self.burnout(b, t),
            Kind::Collapse => {
                let dust = linear(DUST);
                self.haze(b, t, lifetime, dust);
                self.skirt(b, t, mixed(b.dust, dust, 0.5));
                self.column(b, t, lifetime, &BLAST, scaled(dust, 1.1), false);
                self.ring(b, t, b.scale);
            }
            Kind::Blast | Kind::Tank | Kind::Barrel => {
                let smoke = linear(look.smoke);
                self.ground_glow(b, t, &look);
                self.haze(b, t, lifetime, mixed(b.dust, smoke, 0.45));
                self.skirt(b, t, b.dust);
                self.column(b, t, lifetime, &look, smoke, true);
                if b.kind == Kind::Tank {
                    self.turret_jet(b, t, smoke, streaks);
                }
                self.fireball(b, t, &look, smoke);
                let s = b.scale;
                let center = Vec3::new(b.x as f32, (1.0 * s) as f32, b.z as f32);
                if t < BLOOM_SECONDS {
                    let k = t / BLOOM_SECONDS;
                    let [r, g, bl] = BLOOM_COLOR;
                    glow(
                        streaks,
                        center,
                        (BLOOM_SIZE * s * (0.7 + 0.5 * k)) as f32,
                        [r, g, bl, ((1.0 - k) * (1.0 - k) * 0.45) as f32],
                    );
                }
                if t < FLASH_SECONDS {
                    let k = t / FLASH_SECONDS;
                    let [r, g, bl] = FLASH_COLOR;
                    glow(
                        streaks,
                        center,
                        (FLASH_SIZE * s * (0.7 + 0.5 * k)) as f32,
                        [r, g, bl, ((1.0 - k) * (1.0 - k) * 0.9) as f32],
                    );
                }
                self.shock(b, t, b.scale);
                self.ring(b, t, b.scale);
            }
        }
    }

    /// The fire's light on the ground around it: a flash, then a flickering
    /// glow that dies as the fireball burns out. Drawn under the smoke.
    fn ground_glow(&mut self, b: &Blast, t: f64, look: &Look) {
        let strength = b.fire_light(t, look);
        if strength <= 0.01 {
            return;
        }
        let s = b.scale;
        let diameter =
            (GROUND_GLOW_SIZE * s * look.fire_size * (0.75 + 0.35 * settle(t, 8.0))) as f32;
        let y = self
            .pads
            .decal_height(b.x, b.z, f64::from(diameter) * 0.5, RING_HEIGHT + 0.008)
            as f32;
        let world = Mat4::from_translation(Vec3::new(b.x as f32, y, b.z as f32))
            * Mat4::from_scale(Vec3::new(diameter, 1.0, diameter));
        let [r, g, bl] = GROUND_GLOW_COLOR;
        self.rings.push(record(
            world,
            [r, g, bl, 1.0],
            [
                t as f32,
                b.phase as f32,
                GROUND_GLOW,
                strength.min(1.0) as f32,
            ],
        ));
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
            let angle = b.phase + jf * TAU / SKIRT_PUFFS as f64 + (r2 - 0.5) * 0.55;
            let reach = (0.5 + (2.4 + 1.8 * r3) * settle(age, 3.0 + 3.0 * r2)) * s;
            let size = (0.45 + 0.35 * r1 + (0.8 + 0.6 * r2) * age.sqrt()) * s;
            let end = 1.1 + 0.8 * r3;
            self.puff(Puff {
                position: [
                    b.x + angle.cos() * reach + WIND[0] * age * 0.5,
                    0.2 * s + size * 0.3,
                    b.z + angle.sin() * reach + WIND[1] * age * 0.5,
                ],
                size,
                color: scaled(color, 0.88 + 0.22 * r2),
                opacity: (age / 0.05).min(1.0)
                    * (0.3 + 0.22 * r3)
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
            let y = (1.2 + 0.5 * r1) * s + rise * settle(age, 0.8) / 0.8 + 0.3 * age;
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
                opacity: (age / 0.2).min(1.0)
                    * 0.82
                    * thin
                    * (1.0 - smoothstep(age, life * 0.35, life)),
                seed: b.phase * 0.71 + jf * 0.173,
                heat: 0.0,
                erosion: smoothstep(age, life * 0.25, life) * 0.85,
                softness: 0.12 + 0.15 * r1,
                underglow: fire * (1.0 - smoothstep(age, 0.2, 1.4)),
            });
        }
    }

    /// The fireball: boiling puffs flung out and up, white hot at first, cooling
    /// through yellow and orange and burning out from their rims; the smoke is
    /// the column's.
    fn fireball(&mut self, b: &Blast, t: f64, look: &Look, smoke: [f64; 3]) {
        let s = b.scale;
        let cooling = COOLING / look.fire_life;
        for j in 0..FIRE_PUFFS {
            let jf = j as f64;
            let r1 = hash(b.phase * 0.53 + jf * 1.371);
            let r2 = hash(b.phase * 1.91 + jf * 0.713);
            let r3 = hash(b.phase * 2.71 + jf * 2.239);
            let late = look.double_burst && j >= 5;
            let age = t - r1 * 0.035 - if late { 0.24 } else { 0.0 };
            if age <= 0.0 {
                continue;
            }
            let out = settle(age, 7.0 + 5.0 * r2);
            let angle = b.phase + jf * TAU / (FIRE_PUFFS - 1) as f64 + (r3 - 0.5) * 0.8;
            let reach = if j == 0 {
                0.1
            } else {
                (0.25 + (0.85 + 0.55 * r2) * out) * look.fire_spread
            } * s;
            let lift = (0.5 + 0.55 * r3 + look.fire_rise * age) * s;
            let size = (0.6 + 1.4 * out + 0.3 * age)
                * s
                * look.fire_size
                * if j == 0 { 1.25 } else { 0.7 + 0.45 * r1 };
            let heat =
                fire_heat(age, cooling * (0.85 + 0.35 * r3)) * if j == 0 { 1.1 } else { 1.0 };
            let burn = burning(heat);
            self.puff(Puff {
                position: [
                    b.x + angle.cos() * reach + b.phase.cos() * age * look.drift * 0.5,
                    lift,
                    b.z + angle.sin() * reach + b.phase.sin() * age * look.drift * 0.5,
                ],
                size,
                color: scaled(smoke, 0.55),
                opacity: (age / 0.025).min(1.0) * 0.95 * burn,
                seed: b.phase * 0.29 + jf * 0.311,
                heat,
                erosion: (1.0 - smoothstep(heat, 0.3, 1.2)) * 0.8,
                softness: 0.0,
                underglow: 0.0,
            });
        }
    }

    /// A tank's ammunition cooking off: a jet of fire shooting up from the turret
    /// ring, burning out as it climbs.
    fn turret_jet(&mut self, b: &Blast, t: f64, smoke: [f64; 3], streaks: &mut PoolBuffer) {
        let s = b.scale;
        if t < JET_SECONDS {
            let k = t / JET_SECONDS;
            let [r, g, bl] = JET_COLOR;
            tongue(
                streaks,
                Vec3::new(b.x as f32, (0.9 * s) as f32, b.z as f32),
                Vec3::new(WIND[0] as f32 * 0.1, 1.0, WIND[1] as f32 * 0.1).normalize(),
                (1.1 * s * (1.0 - 0.4 * k)) as f32,
                ((2.0 + 3.5 * settle(t, 9.0)) * s) as f32,
                [r, g, bl, ((1.0 - k) * (1.0 - k)) as f32],
            );
        }
        for j in 0..3 {
            let jf = j as f64;
            let age = t - 0.05 - jf * 0.07;
            if age <= 0.0 {
                continue;
            }
            let heat = fire_heat(age, 4.5 + jf * 0.8);
            self.puff(Puff {
                position: [
                    b.x + b.phase.cos() * 0.2 * jf + WIND[0] * age * 0.6,
                    (1.2 + 5.0 * settle(age, 3.0) * (1.0 - 0.22 * jf)) * s,
                    b.z + b.phase.sin() * 0.2 * jf + WIND[1] * age * 0.6,
                ],
                size: (0.55 + 0.8 * age.sqrt()) * s,
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
        if t < 0.1 {
            let k = t / 0.1;
            let fade = (1.0 - k) as f32;
            glow(streaks, at, (0.6 + 0.4 * k) as f32, [4.4, 2.9, 1.4, fade]);
            glow(
                streaks,
                at,
                (2.2 + 1.0 * k) as f32,
                [3.0, 1.15, 0.25, fade * fade * 0.45],
            );
            // Glints thrown out of the hit in a ragged star.
            for i in 0..4 {
                let fi = i as f64;
                let angle = b.phase + fi * TAU / 4.0 + (hash(b.phase + fi) - 0.5) * 0.9;
                let up = 0.25 + 0.5 * hash(b.phase * 3.0 + fi);
                let direction =
                    Vec3::new(angle.cos() as f32, up as f32, angle.sin() as f32).normalize();
                let length = ((0.5 + 0.7 * hash(b.phase * 5.0 + fi)) * (0.6 + 0.9 * k)) as f32;
                streak(
                    streaks,
                    at + direction * (length * 0.5 + 0.1),
                    direction,
                    0.08,
                    length,
                    [4.0, 1.6, 0.32, fade],
                );
            }
        }
        let (color, softness, rise) = match struck {
            Struck::Steel => (linear(HIT_SMOKE), 0.2, 1.5),
            Struck::Stone => (mixed(linear(GRIT), b.dust, 0.35), 0.45, 0.8),
        };
        let heat = fire_heat(t, 9.0);
        if burning(heat) > 0.0 {
            self.puff(Puff {
                position: [b.x, y + t * 0.6, b.z],
                size: 0.35 + 0.7 * t.sqrt(),
                color: scaled(color, 0.6),
                opacity: (t / 0.02).min(1.0) * burning(heat),
                seed: b.phase * 0.91,
                heat,
                erosion: (1.0 - smoothstep(heat, 0.3, 1.2)) * 0.8,
                softness: 0.0,
                underglow: 0.0,
            });
        }
        for j in 0..3 {
            let jf = j as f64;
            let age = t - 0.03 - jf * 0.04;
            if age <= 0.0 {
                continue;
            }
            let angle = b.phase + jf * TAU / 3.0;
            let reach = 0.15 + 0.5 * settle(age, 4.0);
            self.puff(Puff {
                position: [
                    b.x + angle.cos() * reach + WIND[0] * age * 0.6,
                    y + age * rise * (0.7 + 0.3 * jf),
                    b.z + angle.sin() * reach + WIND[1] * age * 0.6,
                ],
                size: 0.4 + 0.9 * age.sqrt(),
                color: scaled(color, 0.9 + jf * 0.07),
                opacity: (age / 0.05).min(1.0) * 0.6 * (1.0 - smoothstep(age, 0.25, HIT_LIFETIME)),
                seed: b.phase * 0.61 + jf * 0.23,
                heat: 0.0,
                erosion: smoothstep(age, 0.2, HIT_LIFETIME) * 0.9,
                softness,
                underglow: if j == 0 {
                    0.7 * (-age * 6.0).exp()
                } else {
                    0.0
                },
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
                    let size = (0.85 + 0.8 * age.sqrt()) * s;
                    self.puff(Puff {
                        position: [
                            b.x + angle.cos() * roll + dx * drift,
                            0.3 * s + size * 0.45,
                            b.z + angle.sin() * roll + dz * drift,
                        ],
                        size,
                        color: scaled(linear(SAWDUST), 0.88 + jf * 0.035),
                        opacity: (age / 0.08).min(1.0) * fade * 0.45,
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
                self.ring(b, t, WOOD_RING_SCALE * s);
            }
        }
    }
}

/// Puffs between the camera and the player's tank thin out to this share of
/// their opacity, so a blast's smoke never hides the player.
const SEE_THROUGH: f32 = 0.06;
/// The sight line is kept clear this wide around its axis (m): about a hull.
const SIGHT_RADIUS: f32 = 2.0;

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
        let t = ((miss - SIGHT_RADIUS) / (radius * 0.9)).clamp(0.0, 1.0);
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
        effects.update(0.06, &mut streaks);
        assert_eq!(streaks.len(), 2, "the flash core and its bloom");
        assert!(heat(&effects.puffs) > 1.0, "a white-hot fireball");
        assert_eq!(rings_of(&effects.rings, SHOCK_RING), 1);
        assert_eq!(rings_of(&effects.rings, DUST_RING), 1);
        assert_eq!(rings_of(&effects.rings, SCORCH), 1);
        assert_eq!(
            rings_of(&effects.rings, GROUND_GLOW),
            1,
            "the fire lights the ground"
        );
        let mut lights = [Flash::default(); 3];
        effects.lights(&mut lights);
        assert!(lights.iter().any(|light| light.intensity > 10.0));
        update(&mut effects, 0.6);
        assert!(heat(&effects.puffs) < 0.3, "the fire has cooled into smoke");
        assert_eq!(rings_of(&effects.rings, SHOCK_RING), 0);
        assert_eq!(
            rings_of(&effects.rings, GROUND_GLOW),
            0,
            "the fire has burnt out"
        );
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
    fn shell_hits_flash_and_puff_without_scorch_or_rings() {
        let mut random = CosmeticRandom::constant(0.5);
        let mut effects = ExplosionEffects::default();
        let mut streaks = PoolBuffer::new(16);
        let mut steel = SimEvent::at(SimEventType::Impact, 0.0, 0.0);
        steel.height = Some(1.0);
        effects.event(&steel, &mut random);
        effects.event(&SimEvent::at(SimEventType::Ricochet, 3.0, 0.0), &mut random);
        effects.update(0.02, &mut streaks);
        assert_eq!(
            streaks.len(),
            12,
            "each a hot point, its bloom and four glints"
        );
        assert_eq!(effects.puffs.len(), 2, "each a burst of fire");
        assert!(effects.rings.is_empty() && effects.scorches() == 0);
        update(&mut effects, HIT_LIFETIME);
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
        assert_eq!(effects.puffs.len(), 8);
        assert_eq!(effects.rings.len(), 1);
        let drift = |effects: &ExplosionEffects| {
            let puffs = effects.puffs.records();
            puffs.iter().map(|r| r.translation().x).sum::<f32>() / puffs.len() as f32
        };
        assert!(drift(&effects) > 0.3, "the dust blows on along the shot");
        update(&mut effects, 1.15);
        assert_eq!(effects.puffs.len(), 8, "the cloud lingers");
        assert!(drift(&effects) > 0.6);
        update(&mut effects, WOOD_BURST_LIFETIME);
        assert_eq!(effects.puffs.len(), 0);
        assert_eq!(effects.active(), 0);
    }

    #[test]
    fn trees_and_drums_destroyed_raise_no_blast() {
        let mut random = CosmeticRandom::default();
        let mut effects = ExplosionEffects::default();
        for kind in [CoverKind::Tree, CoverKind::Drum] {
            let mut destroy = SimEvent::at(SimEventType::Destroy, 0.0, 0.0);
            destroy.cover_kind = Some(kind);
            effects.event(&destroy, &mut random);
        }
        effects.event(&SimEvent::at(SimEventType::Hurt, 0.0, 0.0), &mut random);
        assert_eq!(effects.active(), 0);
        let mut timber = SimEvent::at(SimEventType::Destroy, 0.0, 0.0);
        timber.cover_kind = Some(CoverKind::Timber);
        effects.event(&timber, &mut random);
        assert_eq!(effects.active(), 1);
    }
}
