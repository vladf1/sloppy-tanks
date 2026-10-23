//! Chips, clods and flecks (`particle-effects.ts`): one bounded instanced draw
//! of small irregular shards, lit like the scene (hot clods and sparky flecks
//! glow through the chip effect's emissive), plus the pooled blasts and glowing
//! sparks it forwards events to.

use std::f64::consts::TAU;

use glam::{Mat4, Quat, Vec3};
use sloppy_core::geometry::{Mesh, icosahedron_geometry};
use sloppy_core::sim::{CoverKind, DeathStyle, SimEvent, SimEventType};

use super::explosions::ExplosionEffects;
use super::explosions::blast_scale;
use super::pool::{PoolBuffer, pose, record};
use super::random::CosmeticRandom;
use super::sparks::{BLAST_SPARKS, Burst, EMBERS, METAL_SPARKS, STONE_SPARKS, Sparks};
use super::walls::{Strike, WallMaterial};
use crate::color::hex_to_linear;
use crate::presentation::posing::euler_xyz;

pub const MAX_PARTICLES: usize = 1200;
/// Heavier than real gravity, like the simulation's: chips and plates drop and
/// land rather than drifting about like leaves.
const PARTICLE_GRAVITY: f64 = 15.0;
/// Particles never draw below the ground.
const MIN_HEIGHT: f64 = 0.1;
/// Splinters are long thin chips.
const SPLINTER_STRETCH: Vec3 = Vec3::new(0.4, 2.4, 0.4);
/// Chips and clods are flattened, uneven lumps rather than round beads.
const CHIP_STRETCH: Vec3 = Vec3::new(1.0, 0.62, 0.84);
/// Torn plate and flakes of paint are thin and wide.
const PLATE_STRETCH: Vec3 = Vec3::new(1.5, 0.26, 1.05);
/// A resting chip's or plate's centre stands this share of its size off the ground.
const CHIP_REST: f64 = 0.55;
const PLATE_REST: f64 = 0.26;
/// A shell striking a container's steel: a hard shower of sparks off the face,
/// and flakes of its paint and bare metal knocked off it ([minimum, span]).
const SHEET_SPARKS: Burst = Burst {
    count: 48,
    speed: [3.0, 18.0],
    lift: 0.6,
    life: [0.25, 0.95],
    width: 0.13,
    color: [3.8, 1.6, 0.36],
    gravity: 17.0,
    scatter: 0.12,
    stagger: 0.04,
    hide: 0.0,
    cools: true,
};
const FLAKES: usize = 7;
const FLAKE_SIZE: [f64; 2] = [0.13, 0.11];
const BARE_STEEL: u32 = 0x8d9296;
/// Concrete and stone spall knocked off a wall by a shell: count, life (s),
/// size (m) and the grey it is (mixed with the ground's dust at the hit).
const SPALL: usize = 20;
const SPALL_LIFE: [f64; 2] = [9.0, 5.0];
const SPALL_SIZE: [f64; 2] = [0.06, 0.12];
const SPALL_COLORS: [u32; 4] = [0x8a847a, 0xa49e93, 0x6c6862, 0x938b7f];
/// Spall comes in every size, from grit to fist-sized lumps.
const SPALL_SCALES: [f64; 5] = [2.8, 1.0, 1.7, 0.7, 1.25];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    pub splinter: bool,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub vx: f64,
    pub vy: f64,
    pub vz: f64,
    pub life: f64,
    pub max: f64,
    pub size: f64,
    /// Linear RGB.
    pub color: [f32; 3],
    /// Seconds a clod flung out of a fireball glows before it cools to its color.
    pub hot: f64,
    /// The share of its color a fleck shines with (sparks and pickup glitter),
    /// beside the light it catches.
    pub glow: f32,
    /// Lands, bounces and slides to rest on the ground instead of falling through.
    pub rests: bool,
    /// A flat plate or flake rather than a chunky chip.
    pub plate: bool,
    /// Its rotation once it lies still, which never changes after: most pieces in the
    /// pool lie on the ground, and posing each from its Euler angles every frame took
    /// most of the particles' update.
    pub lying_turn: Option<Quat>,
}

/// A piece's own turn (rad), fixed for its life by its random lifetime.
fn piece_turn(q: &Particle) -> f32 {
    ((q.max * 7919.0).fract() * 977.0) as f32
}

/// The particles' shared mesh: an icosahedron whose corners are pushed in and
/// out by a fixed hash of where they sit and then sheared off along two
/// fracture planes, so every chip is a lopsided, broken shard with flat freshly
/// split faces beside its rough skin, never a round bead or a hexagon in
/// silhouette however it lands.
pub fn shard_mesh() -> Mesh {
    let mut mesh = icosahedron_geometry(1.0, 0);
    for corner in &mut mesh.positions {
        let [x, y, z] = *corner;
        let h = ((x * 12.9898 + y * 78.233 + z * 37.719).sin() * 43_758.547)
            .fract()
            .abs();
        let k = 0.5 + 0.75 * h;
        let (x, y, z) = (x * k * 1.15, y * k * 0.85, z * k * 0.82);
        // A split face across the top and a chipped corner.
        let y = y.min(0.4 - 0.3 * x);
        let x = x.min(0.72 + 0.3 * z);
        *corner = [x, y, z];
    }
    mesh.compute_vertex_normals();
    mesh
}

/// Life and size pairs are [minimum, random span]. Chosen once per event.
struct Style {
    count: u32,
    life: [f64; 2],
    size: [f64; 2],
    speed: f64,
    scatter: f64,
    height: f64,
    lift: f64,
}

/// Bark and wood chips from the trunk; the crown's leaves are `leaves.rs`.
const TREE_HIT: Style = Style {
    count: 7,
    life: [0.45, 0.35],
    size: [0.05, 0.06],
    speed: 3.8,
    scatter: 0.55,
    height: 0.7,
    lift: 1.8,
};
const WOOD_HIT: Style = Style {
    count: 12,
    life: [0.45, 0.4],
    size: [0.07, 0.1],
    speed: 4.0,
    scatter: 0.35,
    height: 0.8,
    lift: 1.5,
};
const PICKUP: Style = Style {
    count: 24,
    life: [0.5, 0.3],
    size: [0.12, 0.1],
    speed: 5.0,
    scatter: 0.0,
    height: 1.3,
    lift: 4.0,
};
const EXPLOSION: Style = Style {
    count: 18,
    life: [0.35, 0.45],
    size: [0.22, 0.5],
    speed: 1.2,
    scatter: 0.0,
    height: 0.8,
    lift: 0.0,
};
/// Clods of the ground a fiery blast throws, beside its sparks; they leave the
/// fireball glowing, cool to the ground's color, bounce and settle.
const CLODS: usize = 14;
/// A landing clod keeps this share of its fall speed, and this share of its
/// slide; it lies still for its last moments before it shrinks away.
const CLOD_BOUNCE: f64 = 0.32;
const CLOD_SLIDE: f64 = 0.55;
/// A piece lying on the ground slides to a stop at this rate (1/s): about
/// `CLOD_SLIDE` thirty times a second.
const SLIDE_DRAG: f64 = 18.0;
const CLOD_SHRINK: f64 = 0.2;
/// Seconds a clod lies where it landed ([minimum, random span]).
const CLOD_LIFE: [f64; 2] = [5.0, 3.0];
/// Bark and splinters a felled tree's trunk bursts from its stump as it snaps.
const SNAP_SPLINTERS: usize = 16;
/// Torn hull plate a kill scatters around its wreck: count, life (s), size (m),
/// and the scorched steel it is; it lies in the scorch long after the smoke.
const SCRAP: usize = 10;
const SCRAP_LIFE: [f64; 2] = [16.0, 8.0];
const SCRAP_SIZE: [f64; 2] = [0.1, 0.18];
/// Torn skin of a burst drum: curled plates of its red paint, blistered and
/// charred, lying round the scorch long after the smoke.
const DRUM_SCRAP: usize = 7;
const DRUM_SCRAP_SIZE: [f64; 2] = [0.16, 0.16];
const DRUM_SCRAP_COLORS: [u32; 3] = [0x8f2a1a, 0x2a2420, 0x5a2a1c];
const SCRAP_COLORS: [u32; 3] = [0x55534d, 0x6a5444, 0x45433f];
const CLOD_GLOW: [f32; 3] = [2.2, 0.62, 0.09];
const HURT: Style = Style {
    count: 12,
    life: [0.22, 0.16],
    size: [0.09, 0.09],
    speed: 6.0,
    scatter: 0.9,
    height: 2.1,
    lift: 1.5,
};
const IMPACT: Style = Style {
    count: 4,
    life: [0.1, 0.2],
    size: [0.04, 0.09],
    speed: 4.0,
    scatter: 0.0,
    height: 1.0,
    lift: 0.0,
};

/// Extra splinters a collapsing timber wall bursts, and how much longer than a
/// hit's chips they lie.
const TIMBER_COLLAPSE_SPLINTERS: i64 = 12;
const TIMBER_COLLAPSE_LIFE: f64 = 9.0;
const TIMBER_COLORS: [u32; 4] = [0x805336, 0xb47a49, 0xc99a65, 0x947958];
/// Dark bark and pale fresh wood torn from a trunk.
const TREE_CHIP_COLORS: [u32; 3] = [0x6b5038, 0x8a6a4a, 0xc9a978];
const EXPLOSION_COLORS: [u32; 6] = [0x536779, 0xff9250, 0xffc569, 0x536779, 0xffc569, 0xff9250];
const HURT_COLORS: [u32; 3] = [0xffd9a0, 0xffa040, 0xff7a2a];
const SPARK_COLOR: u32 = 0xffdf91;

#[derive(Clone, Debug)]
pub struct ParticleEffects {
    pub particles: Vec<Particle>,
    pub explosions: ExplosionEffects,
    pub sparks: Sparks,
    pub records: PoolBuffer,
}

impl Default for ParticleEffects {
    fn default() -> Self {
        Self {
            particles: Vec::with_capacity(MAX_PARTICLES),
            explosions: ExplosionEffects::default(),
            sparks: Sparks::default(),
            records: PoolBuffer::new(MAX_PARTICLES),
        }
    }
}

impl ParticleEffects {
    pub fn reset(&mut self) {
        self.explosions.reset();
        self.sparks.reset();
        self.particles.clear();
        self.records.clear();
    }

    /// Spawn the event's particles and blast. Returns whether it deserves the
    /// explosion light flash.
    pub fn event(&mut self, event: &SimEvent, random: &mut CosmeticRandom) -> bool {
        use SimEventType as T;
        self.explosions.event(event, random);
        let pickup = matches!(event.kind, T::Pickup | T::Promotion);
        let hurt = event.kind == T::Hurt;
        let explosion = matches!(event.kind, T::Explosion | T::Death | T::Destroy);
        let cover_effect = matches!(event.kind, T::Destroy | T::Impact);
        // A crate's splinters and packing are lit wood (`wood_chips.rs`).
        if cover_effect && event.cover_kind == Some(CoverKind::Cargo) {
            return false;
        }
        let timber = cover_effect && event.cover_kind == Some(CoverKind::Timber);
        let tree = cover_effect && event.cover_kind == Some(CoverKind::Tree);
        let chip_hit = event.kind == T::Impact && (tree || timber);
        // A felled tree's falling trunk and crown carry the moment: the snap
        // only bursts bark and splinters from the stump.
        if tree && !chip_hit {
            if event.kind == T::Destroy {
                self.snap(event, random);
            }
            return false;
        }
        let style = if timber {
            &WOOD_HIT
        } else if chip_hit {
            if tree { &TREE_HIT } else { &WOOD_HIT }
        } else if pickup {
            &PICKUP
        } else if explosion {
            &EXPLOSION
        } else if hurt {
            &HURT
        } else {
            &IMPACT
        };
        // The fire/smoke billboards carry blast volume; sparks and clods the rest.
        let tank_death = event.kind == T::Death;
        let burnout = tank_death && event.death_style == Some(DeathStyle::Burnout);
        let fiery = event.kind == T::Explosion || tank_death;
        if explosion && !tree && !timber && !fiery {
            return false;
        }
        // A gun's own flash and smoke are the muzzle flashes' (`muzzle.rs`).
        if event.kind == T::Shot {
            return false;
        }
        self.sparks_for(event, fiery, burnout, chip_hit, random);
        // A pickup's light streaks carry the moment; no confetti.
        if event.kind == T::Pickup {
            return false;
        }
        if fiery {
            if !burnout {
                self.clods(event, random);
            }
            return !burnout;
        }
        // The real beams carry a collapse; add only a hit-sized handful of chips.
        // A wall torn down bursts splinters from every snapped joint; they land
        // and lie in the grass by the wreck.
        let collapse = timber && explosion;
        let count = if timber {
            style.count as i64 - 2
                + (random.next_f64() * 5.0).floor() as i64
                + if collapse {
                    TIMBER_COLLAPSE_SPLINTERS
                } else {
                    0
                }
        } else {
            style.count as i64
        };
        let speed = style.speed;
        let accent = event.color;
        let color_at = |i: usize| -> u32 {
            if timber {
                TIMBER_COLORS[i % TIMBER_COLORS.len()]
            } else if tree {
                TREE_CHIP_COLORS[i % TREE_CHIP_COLORS.len()]
            } else if pickup {
                if i.is_multiple_of(4) {
                    0xffffff
                } else {
                    accent.unwrap_or(0xffffff)
                }
            } else if explosion {
                EXPLOSION_COLORS[i % EXPLOSION_COLORS.len()]
            } else if hurt {
                HURT_COLORS[i % HURT_COLORS.len()]
            } else {
                accent.unwrap_or(SPARK_COLOR)
            }
        };
        let life_scale = if collapse {
            TIMBER_COLLAPSE_LIFE
        } else if tree {
            1.6
        } else if timber {
            2.0
        } else {
            1.0
        };
        let mut i = 0usize;
        while (i as i64) < count && self.particles.len() < MAX_PARTICLES {
            // Draw order matches the former object literal: life, x, y, z, vx, vy, vz, size.
            let life = (style.life[0] + random.next_f64() * style.life[1]) * life_scale;
            let x = event.x + (random.next_f64() - 0.5) * style.scatter;
            let y = if chip_hit && tree {
                0.7 + random.next_f64() * 0.4
            } else {
                style.height
            };
            let z = event.z + (random.next_f64() - 0.5) * style.scatter;
            let vx = (random.next_f64() - 0.5) * speed;
            let vy = style.lift + random.next_f64() * speed;
            let vz = (random.next_f64() - 0.5) * speed;
            let size = style.size[0] + random.next_f64() * style.size[1];
            self.particles.push(Particle {
                splinter: timber || tree,
                x,
                y,
                z,
                vx,
                vy,
                vz,
                life,
                max: life,
                size,
                color: hex_to_linear(color_at(i)),
                hot: 0.0,
                glow: if timber || tree { 0.0 } else { 0.9 },
                rests: collapse,
                plate: false,
                lying_turn: None,
            });
            i += 1;
        }
        let stone_hit =
            (event.kind == T::Impact && event.cover_kind.is_none() && event.height.is_none())
                || event.kind == T::Ricochet;
        if stone_hit {
            let strike = self.wall_strike(event);
            match strike.map(|strike| strike.material) {
                Some(WallMaterial::Steel) => self.flakes(event, strike, random),
                // A cottage's painted boards and plaster are lit wood
                // (`wood_chips.rs`).
                Some(WallMaterial::Siding) => {}
                _ => self.spall(event, strike, random),
            }
        }
        false
    }

    /// The hard wall an impact naming no cover struck, if one stands there.
    fn wall_strike(&self, event: &SimEvent) -> Option<Strike> {
        if event.kind != SimEventType::Impact || event.height.is_some() {
            return None;
        }
        self.explosions.walls.strike(event.x, 1.0, event.z)
    }

    /// Where a wall hit's debris leaves from and the way it flies: out of the
    /// struck face, else back toward the gun.
    fn burst_from(event: &SimEvent, strike: Option<Strike>) -> ([f64; 3], Option<(f64, f64)>) {
        match strike {
            Some(strike) => (
                strike.point.to_array().map(f64::from),
                Some((f64::from(strike.normal.x), f64::from(strike.normal.z))),
            ),
            None => (
                [event.x, 1.0, event.z],
                event.from.map(|from| {
                    let (dx, dz) = (from.x - event.x, from.z - event.z);
                    let length = dx.hypot(dz).max(1e-6);
                    (dx / length, dz / length)
                }),
            ),
        }
    }

    /// Chips of concrete or stone a shell knocks off a wall: lumps of every size,
    /// their broken faces paler than the weathered skin, burst out of the face,
    /// fall, bounce and settle in the grit at its foot.
    fn spall(&mut self, event: &SimEvent, strike: Option<Strike>, random: &mut CosmeticRandom) {
        let dust = self.explosions.ground.dust(event.x, event.z);
        let skin = strike.map(|strike| hex_to_linear(strike.color));
        let ([x, y, z], back) = Self::burst_from(event, strike);
        for i in 0..SPALL {
            if self.particles.len() >= MAX_PARTICLES {
                return;
            }
            let life = SPALL_LIFE[0] + random.next_f64() * SPALL_LIFE[1];
            let (bx, bz) = back.unwrap_or((random.next_f64() - 0.5, random.next_f64() - 0.5));
            // Knocked out of the face and dropping close under it.
            let spread = 2.2;
            let vx = bx * 2.6 + (random.next_f64() - 0.5) * spread;
            let vz = bz * 2.6 + (random.next_f64() - 0.5) * spread;
            let vy = 1.5 + random.next_f64() * 3.5;
            // A few heavy chunks among the grit.
            let size = (SPALL_SIZE[0] + random.next_f64() * SPALL_SIZE[1])
                * SPALL_SCALES[i % SPALL_SCALES.len()];
            let grey = hex_to_linear(SPALL_COLORS[i % SPALL_COLORS.len()]);
            let color = match skin {
                Some(skin) => [0, 1, 2].map(|c| {
                    // Broken stone is darker and greyer than the weathered skin.
                    let fresh = 0.6 + 0.17 * (i % 3) as f32;
                    (skin[c] * 0.45 + grey[c] * 0.55) * fresh
                }),
                None => [0, 1, 2].map(|c| grey[c] * 0.7 + dust[c] as f32 * 0.3),
            };
            self.particles.push(Particle {
                splinter: false,
                x,
                y,
                z,
                vx,
                vy,
                vz,
                life,
                max: life,
                size,
                color,
                hot: 0.0,
                glow: 0.0,
                rests: true,
                plate: false,
                lying_turn: None,
            });
        }
    }

    /// Flakes of paint and bare steel a shell knocks off a container: thin,
    /// spinning, and they settle flat at its foot.
    fn flakes(&mut self, event: &SimEvent, strike: Option<Strike>, random: &mut CosmeticRandom) {
        let paint = strike.map_or(hex_to_linear(BARE_STEEL), |strike| {
            hex_to_linear(strike.color)
        });
        let bare = hex_to_linear(BARE_STEEL);
        let ([x, y, z], back) = Self::burst_from(event, strike);
        let (bx, bz) = back.unwrap_or((0.0, 0.0));
        for i in 0..FLAKES {
            if self.particles.len() >= MAX_PARTICLES {
                return;
            }
            let life = SPALL_LIFE[0] + random.next_f64() * SPALL_LIFE[1];
            let vx = bx * 2.2 + (random.next_f64() - 0.5) * 3.0;
            let vz = bz * 2.2 + (random.next_f64() - 0.5) * 3.0;
            let vy = 1.0 + random.next_f64() * 2.5;
            let color = if i % 3 == 0 {
                bare
            } else {
                paint.map(|c| c * 0.85)
            };
            self.particles.push(Particle {
                splinter: false,
                x,
                y,
                z,
                vx,
                vy,
                vz,
                life,
                max: life,
                size: FLAKE_SIZE[0] + random.next_f64() * FLAKE_SIZE[1],
                color,
                hot: if i % 3 == 0 { 0.35 } else { 0.0 },
                glow: 0.0,
                rests: true,
                plate: true,
                lying_turn: None,
            });
        }
    }

    /// Glowing sparks: a blast's spray, steel struck by a shell, or grit off a
    /// wall or a ricochet.
    fn sparks_for(
        &mut self,
        event: &SimEvent,
        fiery: bool,
        burnout: bool,
        chip_hit: bool,
        random: &mut CosmeticRandom,
    ) {
        use SimEventType as T;
        let at = |y: f64| Vec3::new(event.x as f32, y as f32, event.z as f32);
        if fiery {
            let size = event.size.unwrap_or(3.0) as f32;
            let mut burst = BLAST_SPARKS;
            burst.speed = burst.speed.map(|v| v * (size / 4.5).clamp(0.7, 1.25));
            // The fireball that hides them is as big as the blast.
            burst.hide *= blast_scale(f64::from(size)) as f32
                * if event.kind == T::Death { 1.3 } else { 1.0 };
            let mut embers = EMBERS;
            if burnout {
                burst.count = 6;
                embers.count = 5;
            } else if event.kind == T::Death {
                burst.count = 40;
                embers.count = 22;
            }
            self.sparks.burst(at(0.9), Vec3::ZERO, 1.0, &burst, random);
            self.sparks.burst(at(1.3), Vec3::Y, 0.9, &embers, random);
            return;
        }
        match event.kind {
            T::Hurt if event.size.unwrap_or(0.0) > 0.0 => {
                let mut burst = METAL_SPARKS;
                burst.count = 8;
                self.sparks.burst(at(1.4), Vec3::ZERO, 1.0, &burst, random);
            }
            T::Impact if !chip_hit && event.cover_kind.is_none() => {
                // Glancing back off the struck face, a wall's or toward the gun.
                let back = event.from.map_or(Vec3::ZERO, |from| {
                    Vec3::new((from.x - event.x) as f32, 0.4, (from.z - event.z) as f32)
                });
                match (event.height, self.wall_strike(event)) {
                    (Some(y), _) => self.sparks.burst(at(y), back, 0.85, &METAL_SPARKS, random),
                    (None, Some(strike)) => {
                        let burst = match strike.material {
                            WallMaterial::Steel => &SHEET_SPARKS,
                            WallMaterial::Stone | WallMaterial::Siding => &STONE_SPARKS,
                        };
                        let out = strike.normal + Vec3::Y * 0.35;
                        self.sparks.burst(strike.point, out, 0.8, burst, random);
                    }
                    (None, None) => self
                        .sparks
                        .burst(at(1.0), back, 0.85, &STONE_SPARKS, random),
                }
            }
            T::Ricochet => {
                self.sparks
                    .burst(at(1.0), Vec3::ZERO, 1.0, &STONE_SPARKS, random);
            }
            _ => {}
        }
    }

    /// Clods of the ground a blast throws up and out; they fall back and roll.
    fn clods(&mut self, event: &SimEvent, random: &mut CosmeticRandom) {
        let soil = self.explosions.ground.soil(event.x, event.z);
        let speed = 2.0 * event.size.unwrap_or(3.0).min(7.0);
        for i in 0..CLODS {
            if self.particles.len() >= MAX_PARTICLES {
                return;
            }
            let life = CLOD_LIFE[0] + random.next_f64() * CLOD_LIFE[1];
            let vx = (random.next_f64() - 0.5) * speed;
            let vy = 3.5 + random.next_f64() * 5.0;
            let vz = (random.next_f64() - 0.5) * speed;
            let size = 0.1 + random.next_f64() * 0.2;
            self.particles.push(Particle {
                splinter: false,
                x: event.x,
                y: 0.6,
                z: event.z,
                vx,
                vy,
                vz,
                life,
                max: life,
                size,
                color: hex_to_linear(soil.clods[i % soil.clods.len()]),
                // A few embers among dark clods, so they read as earth flung out
                // of the fire, not glowing confetti inside it.
                hot: if i % 4 == 0 { 0.4 } else { 0.0 },
                glow: 0.0,
                rests: true,
                plate: false,
                lying_turn: None,
            });
        }
        if event.kind == SimEventType::Death {
            self.scrap(event, random);
        }
        if event.cover_kind == Some(CoverKind::Drum) {
            self.drum_scrap(event, random);
        }
    }

    /// The burst drum's skin: a few big curled plates thrown out low, still
    /// hot, that land and lie round the scorch.
    fn drum_scrap(&mut self, event: &SimEvent, random: &mut CosmeticRandom) {
        for i in 0..DRUM_SCRAP {
            if self.particles.len() >= MAX_PARTICLES {
                return;
            }
            let life = SCRAP_LIFE[0] + random.next_f64() * SCRAP_LIFE[1];
            let angle = (i as f64 + random.next_f64() * 0.8) * TAU / DRUM_SCRAP as f64;
            let speed = 1.5 + random.next_f64() * 3.0;
            self.particles.push(Particle {
                splinter: false,
                x: event.x,
                y: 0.8,
                z: event.z,
                vx: angle.cos() * speed,
                vy: 2.5 + random.next_f64() * 3.5,
                vz: angle.sin() * speed,
                life,
                max: life,
                size: DRUM_SCRAP_SIZE[0] + random.next_f64() * DRUM_SCRAP_SIZE[1],
                color: hex_to_linear(DRUM_SCRAP_COLORS[i % DRUM_SCRAP_COLORS.len()]),
                hot: 0.6,
                glow: 0.0,
                rests: true,
                plate: true,
                lying_turn: None,
            });
        }
    }

    /// The snap of a felled trunk: bark and pale splinters burst out round the
    /// stump, fall, and lie in the grass.
    fn snap(&mut self, event: &SimEvent, random: &mut CosmeticRandom) {
        for i in 0..SNAP_SPLINTERS {
            if self.particles.len() >= MAX_PARTICLES {
                return;
            }
            let life = 4.0 + random.next_f64() * 3.0;
            let angle = (i as f64 + random.next_f64() * 0.9) * TAU / SNAP_SPLINTERS as f64;
            let speed = 1.5 + random.next_f64() * 3.5;
            self.particles.push(Particle {
                splinter: true,
                x: event.x + angle.cos() * 0.2,
                y: 0.4 + random.next_f64() * 0.8,
                z: event.z + angle.sin() * 0.2,
                vx: angle.cos() * speed,
                vy: 1.5 + random.next_f64() * 3.0,
                vz: angle.sin() * speed,
                life,
                max: life,
                size: 0.06 + random.next_f64() * 0.08,
                color: hex_to_linear(TREE_CHIP_COLORS[i % TREE_CHIP_COLORS.len()]),
                hot: 0.0,
                glow: 0.0,
                rests: true,
                plate: false,
                lying_turn: None,
            });
        }
    }

    /// Plates and bolts torn off a killed hull: thrown low, glowing at first,
    /// they land and lie scattered round the wreck.
    fn scrap(&mut self, event: &SimEvent, random: &mut CosmeticRandom) {
        for i in 0..SCRAP {
            if self.particles.len() >= MAX_PARTICLES {
                return;
            }
            let life = SCRAP_LIFE[0] + random.next_f64() * SCRAP_LIFE[1];
            let angle = (i as f64 + random.next_f64() * 0.8) * TAU / SCRAP as f64;
            let speed = 2.5 + random.next_f64() * 3.5;
            self.particles.push(Particle {
                splinter: false,
                x: event.x,
                y: 1.0,
                z: event.z,
                vx: angle.cos() * speed,
                vy: 2.0 + random.next_f64() * 3.0,
                vz: angle.sin() * speed,
                life,
                max: life,
                size: SCRAP_SIZE[0] + random.next_f64() * SCRAP_SIZE[1],
                color: hex_to_linear(SCRAP_COLORS[i % SCRAP_COLORS.len()]),
                hot: 0.3,
                glow: 0.0,
                rests: true,
                plate: i % 2 == 0,
                lying_turn: None,
            });
        }
    }

    pub fn update(&mut self, dt: f64, time: f64) {
        self.sparks.update(dt);
        self.explosions.update(dt, &mut self.sparks.records);
        // Resting pieces slide to a stop by the same share every frame.
        let keep = (-SLIDE_DRAG * dt).exp();
        self.particles.retain_mut(|q| {
            q.life -= dt;
            if q.life <= 0.0 {
                return false;
            }
            let rest = q.size
                * if q.plate || q.splinter {
                    PLATE_REST * 2.0
                } else {
                    CHIP_REST
                };
            q.x += q.vx * dt;
            q.z += q.vz * dt;
            // A piece lying on the ground stays on it. Gravity sank it a hair every
            // frame and the landing below lifted it back without fall speed, so it
            // swapped between its lying and tumbling poses every other frame and
            // shook and flickered where it lay.
            if q.rests && q.vy == 0.0 && q.y <= rest {
                q.vx *= keep;
                q.vz *= keep;
                return true;
            }
            q.y += q.vy * dt;
            q.vy -= PARTICLE_GRAVITY * dt;
            // Clods and spall land, bounce and slide to rest on the ground.
            if q.rests && q.y < rest && q.vy < 0.0 {
                q.y = rest;
                q.vy = if q.vy < -1.5 {
                    -q.vy * CLOD_BOUNCE
                } else {
                    0.0
                };
                q.vx *= CLOD_SLIDE;
                q.vz *= CLOD_SLIDE;
            }
            true
        });
        self.records.clear();
        let time = time as f32;
        for q in &mut self.particles {
            let position = Vec3::new(q.x as f32, q.y.max(MIN_HEIGHT) as f32, q.z as f32);
            let size = if q.rests {
                (q.size * (q.life / q.max / CLOD_SHRINK).min(1.0)) as f32
            } else {
                (q.size * q.life / q.max) as f32
            };
            let lying = q.rests && q.vy == 0.0 && q.vx.abs() + q.vz.abs() < 0.05;
            // Each piece's own turn, fixed for its life: its slot in the pool moves
            // as older pieces expire, and a turn taken from that would jolt pieces
            // lying still.
            let turn = piece_turn(q);
            let world = if q.splinter && lying {
                // A splinter lies along the ground where it fell.
                let rotation = *q.lying_turn.get_or_insert_with(|| {
                    let spin = turn * 1.7;
                    euler_xyz(0.08 * spin.sin(), spin, std::f32::consts::FRAC_PI_2)
                });
                Mat4::from_scale_rotation_translation(SPLINTER_STRETCH * size, rotation, position)
            } else if q.splinter {
                let spin = turn;
                pose(
                    position,
                    Vec3::new(time * 5.0 + spin, time * 3.0 + spin, time * 4.0),
                    SPLINTER_STRETCH * size,
                )
            } else if q.rests {
                // A clod, chip or plate tumbles while it flies and lies flat where
                // it comes to rest, never balanced on an edge.
                let spin = turn * 1.7;
                let stretch = if q.plate { PLATE_STRETCH } else { CHIP_STRETCH };
                if lying {
                    let rotation = *q.lying_turn.get_or_insert_with(|| {
                        let tilt = 0.12 * spin.sin();
                        euler_xyz(tilt, spin, tilt * 0.7)
                    });
                    Mat4::from_scale_rotation_translation(stretch * size, rotation, position)
                } else {
                    let tumble = (q.vx.abs() + q.vz.abs() + q.vy.abs()).min(6.0) as f32;
                    let turn = time * tumble * 0.8;
                    pose(
                        position,
                        Vec3::new(turn + spin, turn * 0.7 + spin * 0.5, spin),
                        stretch * size,
                    )
                }
            } else {
                // Every fleck tumbles on its own axes.
                let spin = turn * 2.3;
                pose(
                    position,
                    Vec3::new(time * 4.0 + spin, time * 3.0 + spin * 0.7, spin),
                    CHIP_STRETCH * size,
                )
            };
            let mut color = q.color;
            let mut emissive = color.map(|c| c * q.glow);
            if q.hot > 0.0 {
                // A hot clod shines through its own color and cools to it.
                let glow = (1.0 - (q.max - q.life) / q.hot).clamp(0.0, 1.0) as f32;
                let heat = glow * glow;
                color = color.map(|c| c * (1.0 - 0.8 * heat));
                emissive = CLOD_GLOW.map(|c| c * heat);
            }
            let [r, g, b] = color;
            let [er, eg, eb] = emissive;
            self.records
                .push(record(world, [r, g, b, 1.0], [er, eg, eb, 0.0]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(kind: SimEventType) -> SimEvent {
        SimEvent::at(kind, 0.0, 0.0)
    }

    fn with_cover(kind: SimEventType, cover: CoverKind) -> SimEvent {
        SimEvent {
            cover_kind: Some(cover),
            ..at(kind)
        }
    }

    #[test]
    fn barrel_destruction_is_not_counted_twice_and_wood_collapse_has_no_fireball() {
        let mut random = CosmeticRandom::default();
        let mut particles = ParticleEffects::default();
        assert!(!particles.event(
            &with_cover(SimEventType::Destroy, CoverKind::Drum),
            &mut random
        ));
        particles.update(0.06, 0.06);
        assert_eq!(particles.explosions.puffs.len(), 0);
        assert_eq!(particles.particles.len(), 0);
        let blast = SimEvent {
            size: Some(5.0),
            ..at(SimEventType::Explosion)
        };
        assert!(particles.event(&blast, &mut random));
        particles.update(0.06, 0.12);
        // A dust ring, a shock front and a scorch; clods and glowing sparks.
        assert_eq!(particles.explosions.rings.len(), 3);
        assert_eq!(particles.particles.len(), CLODS);
        assert_eq!(particles.records.len(), CLODS);
        assert!(!particles.sparks.is_empty());
        particles.reset();
        assert!(!particles.event(
            &with_cover(SimEventType::Destroy, CoverKind::Timber),
            &mut random
        ));
        particles.update(0.06, 0.06);
        assert!(
            particles.explosions.puffs.records()[0].tint[0] < 1.0,
            "collapse produces dust, not the bright fire core"
        );
        assert!(particles.particles.iter().all(|p| p.splinter));
    }

    #[test]
    fn chips_and_splinters_are_posed_by_their_euler_turn() {
        let mut random = CosmeticRandom::default();
        let mut particles = ParticleEffects::default();
        let mut steel = at(SimEventType::Impact);
        steel.height = Some(1.0);
        particles.event(&steel, &mut random);
        particles.event(
            &with_cover(SimEventType::Impact, CoverKind::Tree),
            &mut random,
        );
        let time = 7.3;
        particles.update(0.02, time);
        assert!(particles.particles.iter().any(|q| !q.splinter));
        assert!(particles.particles.iter().any(|q| q.splinter));
        let time = time as f32;
        for (q, record) in particles.particles.iter().zip(particles.records.records()) {
            let position = Vec3::new(q.x as f32, q.y.max(MIN_HEIGHT) as f32, q.z as f32);
            let size = (q.size * q.life / q.max) as f32;
            let spin = piece_turn(q);
            let (euler, scale) = if q.splinter {
                (
                    Vec3::new(time * 5.0 + spin, time * 3.0 + spin, time * 4.0),
                    SPLINTER_STRETCH * size,
                )
            } else {
                let spin = piece_turn(q) * 2.3;
                (
                    Vec3::new(time * 4.0 + spin, time * 3.0 + spin * 0.7, spin),
                    CHIP_STRETCH * size,
                )
            };
            assert_eq!(record.world(), pose(position, euler, scale));
        }
    }

    #[test]
    fn landed_spall_and_clods_lie_still() {
        let mut random = CosmeticRandom::default();
        let mut particles = ParticleEffects::default();
        // A shell striking hard cover (spall, the path steel's paint flakes share)
        // and a blast's clods.
        particles.event(&at(SimEventType::Impact), &mut random);
        particles.event(
            &SimEvent {
                size: Some(3.0),
                ..at(SimEventType::Death)
            },
            &mut random,
        );
        let step = 1.0 / 60.0;
        let mut time = 0.0;
        for _ in 0..120 {
            time += step;
            particles.update(step, time);
        }
        let lying: Vec<usize> = (0..particles.particles.len())
            .filter(|&i| particles.particles[i].rests)
            .collect();
        assert!(!lying.is_empty());
        // Two seconds on, every piece that rests has landed; frame after frame its
        // pose holds still (only its shrink at the end of its life may change it).
        for _ in 0..30 {
            let before: Vec<_> = lying
                .iter()
                .map(|&i| particles.records.records()[i].world())
                .collect();
            time += step;
            particles.update(step, time);
            for (&i, before) in lying.iter().zip(&before) {
                let q = &particles.particles[i];
                assert!(q.vy == 0.0, "a landed piece keeps no fall speed");
                assert!(
                    particles.records.records()[i]
                        .world()
                        .abs_diff_eq(*before, 1e-3),
                    "a landed piece moved or turned"
                );
            }
        }
    }

    #[test]
    fn tank_deaths_throw_clods_and_sparks() {
        let mut random = CosmeticRandom::constant(0.5);
        let mut particles = ParticleEffects::default();
        particles.event(
            &SimEvent {
                size: Some(3.0),
                ..at(SimEventType::Death)
            },
            &mut random,
        );
        assert_eq!(particles.particles.len(), CLODS + SCRAP);
        assert!(
            particles
                .particles
                .iter()
                .all(|p| p.vy >= 2.5 && p.life >= 0.9)
        );
        assert_eq!(particles.sparks.len(), 40 + 22, "sparks and embers");
    }

    #[test]
    fn burnout_bursts_a_contained_fireball_with_no_shockwave_or_clods() {
        let mut random = CosmeticRandom::default();
        let mut particles = ParticleEffects::default();
        let burnout = SimEvent {
            death_style: Some(DeathStyle::Burnout),
            size: Some(3.0),
            ..at(SimEventType::Death)
        };
        assert!(!particles.event(&burnout, &mut random));
        assert!(particles.particles.is_empty());
        assert_eq!(
            particles.sparks.len(),
            6 + 5,
            "a few sparks and embers, no clods"
        );
        particles.update(0.5, 0.5);
        assert!(
            particles
                .explosions
                .rings
                .records()
                .iter()
                .all(|r| r.data[2] != 1.0),
            "a scorch and a ring of dust, but no shock front"
        );
        let puffs = particles.explosions.puffs.len();
        assert!(
            puffs > 3 + 4 && puffs <= super::super::explosions::PUFFS_PER_BLAST,
            "a fireball out of the hull's seams beside the smoke and the gout: {puffs}"
        );
    }

    /// `cover-hit-effects.test.ts`, particle part: hits chip, a fatal hit bursts.
    /// A tree's leaves are the leaf fall's (`leaves.rs`), and a crate's splinters
    /// lit wood chips (`wood_chips.rs`), not particles.
    #[test]
    fn cover_hits_chip_trees_and_timber() {
        let mut random = CosmeticRandom::constant(0.5);
        let chips = |p: &ParticleEffects| p.particles.iter().filter(|q| q.splinter).count();
        for kind in [SimEventType::Impact, SimEventType::Destroy] {
            let mut cargo = ParticleEffects::default();
            assert!(!cargo.event(&with_cover(kind, CoverKind::Cargo), &mut random));
            assert!(
                cargo.particles.is_empty(),
                "{kind:?}: crate wood is not particles"
            );
        }
        for cover in [CoverKind::Tree, CoverKind::Timber] {
            let mut hit = ParticleEffects::default();
            hit.event(&with_cover(SimEventType::Impact, cover), &mut random);
            assert!(chips(&hit) > 0, "{cover:?} hit chips");
            assert_eq!(chips(&hit), hit.particles.len());
            let mut destroyed = ParticleEffects::default();
            destroyed.event(&with_cover(SimEventType::Destroy, cover), &mut random);
            if cover == CoverKind::Tree {
                assert_eq!(
                    chips(&destroyed),
                    SNAP_SPLINTERS,
                    "the felled trunk snaps off its stump in splinters"
                );
            } else {
                assert!(
                    chips(&destroyed) > chips(&hit),
                    "{cover:?}: collapse adds more chips than a hit"
                );
            }
        }
    }

    #[test]
    fn event_styles_have_their_counts_and_the_pool_is_bounded() {
        let mut random = CosmeticRandom::default();
        let mut particles = ParticleEffects::default();
        let mut shot = at(SimEventType::Shot);
        shot.color = Some(0x008cff);
        particles.event(&shot, &mut random);
        assert_eq!(
            particles.particles.len(),
            0,
            "muzzle flashes are muzzle.rs's"
        );
        particles.event(&at(SimEventType::Ricochet), &mut random);
        let stone = IMPACT.count as usize + SPALL;
        assert_eq!(particles.particles.len(), stone, "sparks and spall");
        assert_eq!(particles.sparks.len(), STONE_SPARKS.count);
        particles.event(&at(SimEventType::Hurt), &mut random);
        assert_eq!(particles.particles.len(), stone + HURT.count as usize);
        let sparks = particles.sparks.len();
        particles.event(&at(SimEventType::Pickup), &mut random);
        assert_eq!(
            particles.particles.len(),
            stone + HURT.count as usize,
            "a pickup throws no confetti"
        );
        assert_eq!(particles.sparks.len(), sparks, "nor light");
        particles.event(&at(SimEventType::Promotion), &mut random);
        assert_eq!(
            particles.particles.len(),
            stone + (HURT.count + PICKUP.count) as usize
        );
        for _ in 0..200 {
            particles.event(&at(SimEventType::Promotion), &mut random);
        }
        assert_eq!(particles.particles.len(), MAX_PARTICLES);
        particles.update(1.0 / 60.0, 1.0);
        assert!(particles.records.len() <= MAX_PARTICLES);
        assert!(particles.records.records().iter().all(|r| {
            r.world_rows.as_flattened().iter().all(|v| v.is_finite()) && r.translation().y >= 0.1
        }));
        // Spall lies longest.
        let longest = SPALL_LIFE[0] + SPALL_LIFE[1];
        for _ in 0..(longest * 60.0) as usize + 1 {
            particles.update(1.0 / 60.0, 1.0);
        }
        assert!(particles.particles.is_empty());
        assert!(particles.records.is_empty(), "an empty pool draws nothing");
    }
}
