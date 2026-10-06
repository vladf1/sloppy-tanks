//! Analytic, pooled cosmetic blasts (`explosion-effects.ts`): smoke and fire
//! billboards plus a ground ring per blast, two instanced draws in total, with no
//! textures, lights or physics bodies.

use std::f64::consts::TAU;

use glam::{Mat4, Vec3};
use sloppy_core::sim::{CoverKind, DeathStyle, SimEvent, SimEventType};

use super::pool::{PoolBuffer, record, smoothstep};
use super::random::CosmeticRandom;
use super::spawn_pad_decks::SpawnPadDecks;
use crate::color::hex_to_linear;

pub const MAX_EXPLOSIONS: usize = 24;
pub const EXPLOSION_LIFETIME: f64 = 1.15;
pub const PUFFS_PER_BLAST: usize = 8;
/// A burnout's thin smoke outlasts a normal blast.
const BURNOUT_LIFETIME: f64 = 2.7;
/// Rings show for the first part of a blast only.
const RING_SECONDS: f64 = 0.55;
const RING_HEIGHT: f64 = 0.07;
/// A ring spreads from `RING_START` to `RING_START + RING_SPREAD` metres (times
/// the blast scale); it clears every spawn pad deck it can reach.
const RING_START: f64 = 0.65;
const RING_SPREAD: f64 = 3.6;
/// Puffs are slightly flattened unless a profile stretches its fire.
const PUFF_HEIGHT: f64 = 0.86;

/// Linear colors; `HOT` exceeds 1 on purpose (an HDR fire core).
const HOT: [f64; 3] = [2.4, 1.25, 0.2];
const FLAME: u32 = 0xf87924;
const SMOKE: u32 = 0x62666a;
const TANK_SMOKE: u32 = 0x34373a;
const DUST: u32 = 0xaa9273;

/// A blast silhouette: smoke column, rolling cloud or drifting plume, and its fire.
#[derive(Clone, Copy, Debug)]
struct Profile {
    lifetime: f64,
    stagger: f64,
    spread: f64,
    step: f64,
    rise: f64,
    drift: f64,
    smoke_size: f64,
    smoke: u32,
    fire_end: f64,
    fire_spread: f64,
    fire_step: f64,
    fire_rise: f64,
    fire_size: f64,
    fire_stretch: f64,
    double_burst: bool,
}

// Distinct silhouettes and timing, using the same eight slots per tank death.
const TANK_BLASTS: [Profile; 3] = [
    Profile {
        lifetime: 2.1,
        stagger: 0.075,
        spread: 0.32,
        step: 0.58,
        rise: 1.65,
        drift: 0.3,
        smoke_size: 0.9,
        smoke: TANK_SMOKE,
        fire_end: 0.61,
        fire_spread: 0.24,
        fire_step: 0.65,
        fire_rise: 2.2,
        fire_size: 0.64,
        fire_stretch: 1.65,
        double_burst: true,
    },
    Profile {
        lifetime: 1.65,
        stagger: 0.025,
        spread: 1.65,
        step: 0.08,
        rise: 0.85,
        drift: 0.2,
        smoke_size: 1.12,
        smoke: 0x635448,
        fire_end: 0.45,
        fire_spread: 0.85,
        fire_step: 0.16,
        fire_rise: 0.7,
        fire_size: 1.15,
        fire_stretch: 0.78,
        double_burst: false,
    },
    Profile {
        lifetime: 1.95,
        stagger: 0.055,
        spread: 0.55,
        step: 0.26,
        rise: 1.05,
        drift: 1.8,
        smoke_size: 0.8,
        smoke: 0x41474d,
        fire_end: 0.52,
        fire_spread: 0.45,
        fire_step: 0.3,
        fire_rise: 1.1,
        fire_size: 0.75,
        fire_stretch: 1.2,
        double_burst: false,
    },
];

const BARREL_BLASTS: [Profile; 3] = [
    Profile {
        lifetime: 1.15,
        stagger: 0.018,
        spread: 1.55,
        step: 0.05,
        rise: 0.8,
        drift: 0.15,
        smoke_size: 0.85,
        smoke: 0x716357,
        fire_end: 0.38,
        fire_spread: 0.9,
        fire_step: 0.1,
        fire_rise: 0.7,
        fire_size: 1.0,
        fire_stretch: 0.68,
        double_burst: false,
    },
    Profile {
        lifetime: 1.35,
        stagger: 0.04,
        spread: 0.6,
        step: 0.18,
        rise: 1.1,
        drift: 2.0,
        smoke_size: 0.7,
        smoke: 0x575b61,
        fire_end: 0.48,
        fire_spread: 0.35,
        fire_step: 0.25,
        fire_rise: 1.0,
        fire_size: 0.7,
        fire_stretch: 1.15,
        double_burst: false,
    },
    Profile {
        lifetime: 1.2,
        stagger: 0.035,
        spread: 1.0,
        step: 0.12,
        rise: 1.25,
        drift: 0.55,
        smoke_size: 0.75,
        smoke: 0x66615b,
        fire_end: 0.61,
        fire_spread: 0.5,
        fire_step: 0.23,
        fire_rise: 1.25,
        fire_size: 0.65,
        fire_stretch: 0.95,
        double_burst: true,
    },
];

#[derive(Clone, Copy, Debug, Default)]
struct Blast {
    active: bool,
    x: f64,
    z: f64,
    age: f64,
    scale: f64,
    phase: f64,
    fire: bool,
    tank: bool,
    barrel: bool,
    burnout: bool,
    variant: usize,
    tempo: f64,
}

fn linear(hex: u32) -> [f64; 3] {
    hex_to_linear(hex).map(f64::from)
}

fn scaled(color: [f64; 3], factor: f64) -> [f64; 3] {
    color.map(|c| c * factor)
}

fn lerp(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// Pooled blasts; `puffs` and `rings` are rebuilt each update.
#[derive(Clone, Debug)]
pub struct ExplosionEffects {
    blasts: [Blast; MAX_EXPLOSIONS],
    cursor: usize,
    tank_sequence: usize,
    barrel_sequence: usize,
    pub puffs: PoolBuffer,
    pub rings: PoolBuffer,
    /// The current map's spawn pads, which rings on or beside them ride over.
    pub pads: SpawnPadDecks,
}

impl Default for ExplosionEffects {
    fn default() -> Self {
        Self {
            blasts: [Blast::default(); MAX_EXPLOSIONS],
            cursor: 0,
            tank_sequence: 0,
            barrel_sequence: 0,
            puffs: PoolBuffer::new(MAX_EXPLOSIONS * PUFFS_PER_BLAST),
            rings: PoolBuffer::new(MAX_EXPLOSIONS),
            pads: SpawnPadDecks::default(),
        }
    }
}

impl ExplosionEffects {
    pub fn event(&mut self, event: &SimEvent, random: &mut CosmeticRandom) {
        let fire = matches!(event.kind, SimEventType::Explosion | SimEventType::Death);
        // Barrels also emit an explosion; do not double their fireball/dust budget.
        if !fire
            && (event.kind != SimEventType::Destroy
                || matches!(event.cover_kind, Some(CoverKind::Tree | CoverKind::Drum)))
        {
            return;
        }
        let size = event.size.unwrap_or(3.0);
        let blast = &mut self.blasts[self.cursor];
        self.cursor = (self.cursor + 1) % MAX_EXPLOSIONS;
        blast.active = true;
        blast.x = event.x;
        blast.z = event.z;
        blast.age = 0.0;
        blast.tank = event.kind == SimEventType::Death;
        blast.burnout = blast.tank && event.death_style == Some(DeathStyle::Burnout);
        blast.barrel =
            event.kind == SimEventType::Explosion && event.cover_kind == Some(CoverKind::Drum);
        blast.variant = if blast.tank {
            self.tank_sequence += 1;
            (self.tank_sequence - 1) % TANK_BLASTS.len()
        } else if blast.barrel {
            self.barrel_sequence += 1;
            (self.barrel_sequence - 1) % BARREL_BLASTS.len()
        } else {
            0
        };
        // Cosmetic draw order: tempo, scale, barrel scale, phase.
        blast.tempo = if blast.tank || blast.barrel {
            0.9 + random.next_f64() * 0.2
        } else {
            1.0
        };
        blast.scale = if blast.tank {
            (size / 3.0) * (0.9 + random.next_f64() * 0.25)
        } else {
            (size / 4.5).clamp(0.55, 1.45)
        };
        if blast.barrel {
            blast.scale *= 0.88 + random.next_f64() * 0.25;
        }
        blast.phase = random.next_f64() * TAU;
        blast.fire = fire;
    }

    pub fn reset(&mut self) {
        for blast in &mut self.blasts {
            blast.active = false;
        }
        self.cursor = 0;
        self.tank_sequence = 0;
        self.barrel_sequence = 0;
        self.puffs.clear();
        self.rings.clear();
    }

    /// Live blasts, for tests and stats.
    pub fn active(&self) -> usize {
        self.blasts.iter().filter(|blast| blast.active).count()
    }

    pub fn update(&mut self, dt: f64) {
        self.puffs.clear();
        self.rings.clear();
        for index in 0..MAX_EXPLOSIONS {
            let mut blast = self.blasts[index];
            if blast.active {
                self.advance(&mut blast, dt);
                self.blasts[index] = blast;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn puff(&mut self, position: [f64; 3], size: f64, color: [f64; 3], opacity: f64, height: f64) {
        if size <= 0.0 || opacity <= 0.0 {
            return;
        }
        // Billboards ignore rotation, so the pose is translation × scale only.
        let world = Mat4::from_scale_rotation_translation(
            Vec3::new(size as f32, (size * height) as f32, size as f32),
            glam::Quat::IDENTITY,
            Vec3::new(position[0] as f32, position[1] as f32, position[2] as f32),
        );
        let tint = [
            color[0] as f32,
            color[1] as f32,
            color[2] as f32,
            opacity as f32,
        ];
        self.puffs.push(record(world, tint, [0.0; 4]));
    }

    fn advance(&mut self, b: &mut Blast, dt: f64) {
        b.age += dt.max(0.0);
        let profile = if b.tank {
            Some(TANK_BLASTS[b.variant])
        } else if b.barrel {
            Some(BARREL_BLASTS[b.variant])
        } else {
            None
        };
        let lifetime = if b.burnout {
            BURNOUT_LIFETIME
        } else {
            profile.map_or(EXPLOSION_LIFETIME, |p| p.lifetime)
        };
        let t = b.age * b.tempo;
        if t >= lifetime {
            b.active = false;
            return;
        }
        let s = b.scale;
        let (cos_phase, sin_phase) = (b.phase.cos(), b.phase.sin());
        if b.burnout {
            // A small internal flash, then thin exhaust-like smoke; no shockwave.
            for j in 0..3 {
                let age = t - j as f64 * 0.18;
                if age <= 0.0 {
                    continue;
                }
                let alpha = (age * 7.0).min(1.0) * (1.0 - smoothstep(t, 1.5, 2.7)) * 0.6;
                self.puff(
                    [
                        b.x + cos_phase * age * 0.35,
                        (1.1 + j as f64 * 0.3 + age * 0.75) * s,
                        b.z + sin_phase * age * 0.35,
                    ],
                    (0.22 + age.sqrt() * 0.3) * s,
                    linear(TANK_SMOKE),
                    alpha,
                    PUFF_HEIGHT,
                );
            }
            if t < 0.12 {
                self.puff(
                    [b.x, 0.8 * s, b.z],
                    0.32 * s,
                    linear(FLAME),
                    1.0 - t / 0.12,
                    PUFF_HEIGHT,
                );
            }
            return;
        }
        // A column, a low rolling cloud, or a drifting side plume. All profiles
        // reuse the same five smoke and three fire slots.
        let smoke = linear(profile.map_or(if b.fire { SMOKE } else { DUST }, |p| p.smoke));
        for j in 0..5 {
            let jf = j as f64;
            let age =
                t - if b.fire { 0.08 } else { 0.0 } - jf * profile.map_or(0.018, |p| p.stagger);
            if age <= 0.0 {
                continue;
            }
            let angle = b.phase + jf * TAU / 5.0;
            let spread = match profile {
                Some(p) => 0.18 + age * p.spread,
                None => 0.35 + age * 1.15,
            } * s;
            let alpha = (age / 0.1).min(1.0)
                * (1.0 - smoothstep(t, if b.tank { 1.05 } else { 0.45 }, lifetime))
                * 0.8;
            let (x, y, z) = match profile {
                Some(p) => (
                    b.x + angle.cos() * spread + cos_phase * t * p.drift,
                    ((if b.tank { 0.9 } else { 0.55 }) + jf * p.step + age * p.rise) * s,
                    b.z + angle.sin() * spread + sin_phase * t * p.drift,
                ),
                None => (
                    b.x + angle.cos() * spread + t * 0.35,
                    (0.65 + age * 1.45 + (j % 2) as f64 * 0.25) * s,
                    b.z + angle.sin() * spread,
                ),
            };
            let size = (0.45 + age.sqrt() * profile.map_or(0.9, |p| p.smoke_size)) * s;
            self.puff(
                [x, y, z],
                size,
                scaled(smoke, 0.86 + jf * 0.055),
                alpha,
                PUFF_HEIGHT,
            );
        }
        let fire_end = profile.map_or(0.38, |p| p.fire_end);
        let double_burst = profile.is_some_and(|p| p.double_burst);
        let fire_age = if double_burst && t >= 0.34 {
            t - 0.34
        } else {
            t
        };
        let pulse_length = if double_burst { 0.27 } else { fire_end };
        if b.fire && t < fire_end && fire_age < pulse_length {
            let growth = (fire_age / 0.055).min(1.0);
            let fade_start = if profile.is_some() {
                pulse_length * 0.47
            } else {
                0.18
            };
            let fade = 1.0 - smoothstep(fire_age, fade_start, pulse_length);
            let flame = linear(FLAME);
            for j in 0..3 {
                let jf = j as f64;
                let angle = b.phase + jf * TAU / 3.0;
                let heat = (fire_age * if j == 0 { 2.0 } else { 5.0 } + jf * 0.3).min(1.0);
                let color = lerp(HOT, flame, heat);
                let (spread, drift) = match profile {
                    Some(p) => (p.fire_spread, p.drift * fire_age * 0.7),
                    None => (0.55, 0.0),
                };
                let (y, size) = match profile {
                    Some(p) => (
                        (if b.tank { 1.0 } else { 0.65 })
                            + jf * p.fire_step
                            + fire_age * p.fire_rise,
                        p.fire_size + jf * 0.07 + fire_age * 0.3,
                    ),
                    None => (0.8 + jf * 0.25 + t * 1.2, 0.85 + jf * 0.12 + t * 0.7),
                };
                self.puff(
                    [
                        b.x + angle.cos() * spread * s + cos_phase * drift,
                        y * s,
                        b.z + angle.sin() * spread * s + sin_phase * drift,
                    ],
                    size * growth * s,
                    color,
                    fade,
                    profile.map_or(PUFF_HEIGHT, |p| p.fire_stretch),
                );
            }
        }
        if t < RING_SECONDS {
            let radius = ((RING_START + RING_SPREAD * (1.0 - (-t * 5.0).exp())) * s) as f32;
            // One height for the ring's whole spread, so it never steps mid-blast.
            let reach = (RING_START + RING_SPREAD) * s;
            let y = self.pads.decal_height(b.x, b.z, reach, RING_HEIGHT) as f32;
            let world = Mat4::from_translation(Vec3::new(b.x as f32, y, b.z as f32))
                * Mat4::from_scale(Vec3::new(radius * 2.0, 1.0, radius * 2.0));
            self.rings.push(record(
                world,
                [1.0; 4],
                [t as f32, b.phase as f32, 0.0, 0.0],
            ));
        }
    }
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

    #[test]
    fn blast_sequence_expands_into_smoke_then_completely_retires() {
        let mut random = CosmeticRandom::default();
        let mut effects = ExplosionEffects::default();
        effects.event(
            &SimEvent {
                size: Some(5.0),
                ..SimEvent::at(SimEventType::Explosion, 4.0, -2.0)
            },
            &mut random,
        );
        effects.update(0.06);
        assert_eq!(effects.puffs.len(), 3);
        assert_eq!(effects.rings.len(), 1);
        effects.update(0.34);
        assert_eq!(effects.puffs.len(), 5);
        effects.update(0.25);
        assert_eq!(effects.rings.len(), 0);
        assert_eq!(effects.puffs.len(), 5);
        effects.update(EXPLOSION_LIFETIME);
        assert_eq!(effects.puffs.len(), 0);
        assert_eq!(effects.rings.len(), 0);
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
            let deck = effects.pads.top(-53.0, 0.0).expect("pad deck");
            let mut heights = Vec::new();
            for _ in 0..8 {
                effects.update(0.06);
                assert_eq!(effects.rings.len(), 2);
                let on_pad = y(&effects.rings, 0);
                assert!(f64::from(on_pad) > deck + 0.015, "{theme} ring {on_pad}");
                heights.push(on_pad);
                assert!((f64::from(y(&effects.rings, 1)) - RING_HEIGHT).abs() < 1e-6);
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
        effects.update(0.18);
        assert_eq!(effects.rings.len(), MAX_EXPLOSIONS);
        assert_eq!(effects.puffs.len(), MAX_EXPLOSIONS * PUFFS_PER_BLAST);
        assert!(effects.puffs.records().iter().all(|r| {
            r.world_rows
                .as_flattened()
                .iter()
                .chain(&r.tint)
                .all(|v| v.is_finite())
        }));
        effects.reset();
        effects.update(0.0);
        assert_eq!(effects.puffs.len(), 0);
        assert_eq!(effects.rings.len(), 0);
        assert_eq!(effects.puffs.capacity(), MAX_EXPLOSIONS * PUFFS_PER_BLAST);
    }

    #[test]
    fn tank_deaths_have_taller_darker_longer_smoke() {
        let mut random = CosmeticRandom::constant(0.5);
        let mut tank = ExplosionEffects::default();
        let mut shell = ExplosionEffects::default();
        tank.event(&event(SimEventType::Death, Some(3.0)), &mut random);
        shell.event(&event(SimEventType::Explosion, Some(3.0)), &mut random);
        tank.update(0.75);
        shell.update(0.75);
        assert!(y(&tank.puffs, 4) > y(&shell.puffs, 4) + 1.0);
        assert!(tank.puffs.records()[0].tint[0] < shell.puffs.records()[0].tint[0]);
        let shell_cleanup = EXPLOSION_LIFETIME - 0.75 + 0.01;
        tank.update(shell_cleanup);
        shell.update(shell_cleanup);
        assert_eq!(shell.puffs.len(), 0);
        assert_eq!(tank.puffs.len(), 5);
        let longest_tank_blast = TANK_BLASTS
            .iter()
            .map(|profile| profile.lifetime)
            .fold(0.0, f64::max);
        tank.update(longest_tank_blast);
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
        effects.update(0.37);
        assert!(effects.puffs.len() <= MAX_EXPLOSIONS * PUFFS_PER_BLAST);
        assert!(effects.puffs.len() >= MAX_EXPLOSIONS * 5);
        assert_eq!(effects.rings.len(), MAX_EXPLOSIONS);
        effects.reset();
        effects.event(
            &SimEvent::at(SimEventType::Explosion, 0.0, 0.0),
            &mut random,
        );
        effects.update(EXPLOSION_LIFETIME);
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
                effects.update(0.45);
                signatures.push(
                    effects
                        .puffs
                        .records()
                        .iter()
                        .flat_map(|r| r.world_rows.into_iter().flatten())
                        .collect::<Vec<_>>(),
                );
                effects.update(4.0);
            }
            assert_ne!(signatures[0], signatures[1]);
            assert_ne!(signatures[1], signatures[2]);
            assert_eq!(effects.puffs.capacity(), MAX_EXPLOSIONS * PUFFS_PER_BLAST);
        }
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
