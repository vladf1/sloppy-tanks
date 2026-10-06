//! Chips, sparks and embers (`particle-effects.ts`): one bounded instanced
//! draw of small icosahedra, plus the pooled blasts it forwards events to.

use glam::{Mat4, Vec3};
use sloppy_core::sim::{CoverKind, DeathStyle, SimEvent, SimEventType};

use super::explosions::ExplosionEffects;
use super::pool::{PoolBuffer, euler_xyz, pose, record};
use super::random::CosmeticRandom;
use crate::color::hex_to_linear;

pub const MAX_PARTICLES: usize = 1200;
const PARTICLE_GRAVITY: f64 = 8.0;
/// Particles never draw below the ground.
const MIN_HEIGHT: f64 = 0.1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleShape {
    Splinter,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    pub shape: Option<ParticleShape>,
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
    count: 8,
    life: [0.1, 0.2],
    size: [0.04, 0.09],
    speed: 4.0,
    scatter: 0.0,
    height: 1.0,
    lift: 0.0,
};

const TIMBER_COLORS: [u32; 4] = [0x805336, 0xb47a49, 0xc99a65, 0x947958];
/// Dark bark and pale fresh wood torn from a trunk.
const TREE_CHIP_COLORS: [u32; 3] = [0x6b5038, 0x8a6a4a, 0xc9a978];
const EXPLOSION_COLORS: [u32; 6] = [0x536779, 0xff9250, 0xffc569, 0x536779, 0xffc569, 0xff9250];
const HURT_COLORS: [u32; 3] = [0xffffff, 0xffcb58, 0xffcb58];
const EMBER_COLORS: [u32; 2] = [0xffde82, 0xffa238];
const SPARK_COLOR: u32 = 0xffdf91;

/// Cosmetic randomness is deliberately independent from the seeded simulation.
#[derive(Clone, Debug)]
pub struct ParticleEffects {
    pub particles: Vec<Particle>,
    pub explosions: ExplosionEffects,
    pub records: PoolBuffer,
}

impl Default for ParticleEffects {
    fn default() -> Self {
        Self {
            particles: Vec::with_capacity(MAX_PARTICLES),
            explosions: ExplosionEffects::default(),
            records: PoolBuffer::new(MAX_PARTICLES),
        }
    }
}

impl ParticleEffects {
    pub fn reset(&mut self) {
        self.explosions.reset();
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
        let timber =
            cover_effect && matches!(event.cover_kind, Some(CoverKind::Timber | CoverKind::Cargo));
        let timber_wall = cover_effect && event.cover_kind == Some(CoverKind::Timber);
        let tree = cover_effect && event.cover_kind == Some(CoverKind::Tree);
        let chip_hit = event.kind == T::Impact && (tree || timber);
        // A felled tree's falling trunk and crown carry the moment; no leaf burst.
        if tree && !chip_hit {
            return false;
        }
        let style = if timber_wall {
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
        // The fire/smoke billboards carry blast volume; keep a few hot flecks.
        let tank_death = event.kind == T::Death;
        let burnout = tank_death && event.death_style == Some(DeathStyle::Burnout);
        let fiery = event.kind == T::Explosion || tank_death;
        if explosion && !tree && !timber && !fiery {
            return false;
        }
        // The real beams carry a collapse; add only a hit-sized handful of chips.
        let count = if timber_wall {
            style.count as i64 - 2
                + (random.next_f64() * 5.0).floor() as i64
                + if explosion { 2 } else { 0 }
        } else if burnout {
            3
        } else if fiery {
            8
        } else if event.kind == T::Shot {
            5
        } else {
            style.count as i64
        };
        let speed = style.speed
            * if explosion && !tree && !timber_wall {
                event.size.unwrap_or(3.0)
            } else {
                1.0
            };
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
        let life_scale = if tree {
            1.6
        } else if timber {
            2.0
        } else {
            1.0
        };
        let mut i = 0usize;
        while (i as i64) < count && self.particles.len() < MAX_PARTICLES {
            // Draw order matches the former object literal: life, x, y, z, vx, vy, vz, size.
            let life = if tank_death {
                0.8 + random.next_f64() * 0.4
            } else {
                (style.life[0] + random.next_f64() * style.life[1]) * life_scale
            };
            let x = event.x + (random.next_f64() - 0.5) * style.scatter;
            let y = if chip_hit && tree {
                0.7 + random.next_f64() * 0.4
            } else {
                style.height
            };
            let z = event.z + (random.next_f64() - 0.5) * style.scatter;
            let vx = (random.next_f64() - 0.5) * speed;
            let vy = if tank_death {
                (if burnout { 1.5 } else { 3.5 }) + random.next_f64() * 2.8
            } else {
                style.lift + random.next_f64() * speed
            };
            let vz = (random.next_f64() - 0.5) * speed;
            let size = if fiery {
                0.05 + random.next_f64() * 0.06
            } else {
                style.size[0] + random.next_f64() * style.size[1]
            };
            let shape = (timber || tree).then_some(ParticleShape::Splinter);
            let hex = if fiery {
                EMBER_COLORS[i % 2]
            } else {
                color_at(i)
            };
            self.particles.push(Particle {
                shape,
                x,
                y,
                z,
                vx,
                vy,
                vz,
                life,
                max: life,
                size,
                color: hex_to_linear(hex),
            });
            i += 1;
        }
        fiery && !burnout
    }

    pub fn update(&mut self, dt: f64, time: f64) {
        self.explosions.update(dt);
        self.particles.retain_mut(|q| {
            q.life -= dt;
            if q.life <= 0.0 {
                return false;
            }
            q.x += q.vx * dt;
            q.y += q.vy * dt;
            q.z += q.vz * dt;
            q.vy -= PARTICLE_GRAVITY * dt;
            true
        });
        self.records.clear();
        let time = time as f32;
        // Every chip turns the same way this frame.
        let chip_turn = euler_xyz(Vec3::new(0.0, time, 0.0));
        for (i, q) in self.particles.iter().enumerate() {
            let position = Vec3::new(q.x as f32, q.y.max(MIN_HEIGHT) as f32, q.z as f32);
            let size = (q.size * q.life / q.max) as f32;
            let world = match q.shape {
                None => {
                    Mat4::from_scale_rotation_translation(Vec3::splat(size), chip_turn, position)
                }
                Some(shape) => {
                    let spin = i as f32;
                    let stretch = match shape {
                        ParticleShape::Splinter => Vec3::new(0.4, 2.4, 0.4),
                    };
                    pose(
                        position,
                        Vec3::new(time * 5.0 + spin, time * 3.0 + spin, time * 4.0),
                        stretch * size,
                    )
                }
            };
            let [r, g, b] = q.color;
            self.records.push(record(world, [r, g, b, 1.0], [0.0; 4]));
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
        assert_eq!(particles.explosions.rings.len(), 1);
        assert_eq!(particles.particles.len(), 8);
        assert_eq!(particles.records.len(), 8);
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
        assert!(
            particles
                .particles
                .iter()
                .all(|p| p.shape == Some(ParticleShape::Splinter))
        );
    }

    #[test]
    fn chips_and_splinters_are_posed_by_their_euler_turn() {
        let mut random = CosmeticRandom::default();
        let mut particles = ParticleEffects::default();
        particles.event(&at(SimEventType::Impact), &mut random);
        particles.event(
            &with_cover(SimEventType::Impact, CoverKind::Tree),
            &mut random,
        );
        let time = 7.3;
        particles.update(0.02, time);
        assert!(particles.particles.iter().any(|q| q.shape.is_none()));
        assert!(particles.particles.iter().any(|q| q.shape.is_some()));
        let time = time as f32;
        for (i, (q, record)) in particles
            .particles
            .iter()
            .zip(particles.records.records())
            .enumerate()
        {
            let position = Vec3::new(q.x as f32, q.y.max(MIN_HEIGHT) as f32, q.z as f32);
            let size = (q.size * q.life / q.max) as f32;
            let spin = i as f32;
            let (euler, scale) = match q.shape {
                None => (Vec3::new(0.0, time, 0.0), Vec3::splat(size)),
                Some(_) => (
                    Vec3::new(time * 5.0 + spin, time * 3.0 + spin, time * 4.0),
                    Vec3::new(0.4, 2.4, 0.4) * size,
                ),
            };
            assert_eq!(record.world(), pose(position, euler, scale));
        }
    }

    #[test]
    fn tank_deaths_throw_upward_embers() {
        let mut random = CosmeticRandom::constant(0.5);
        let mut particles = ParticleEffects::default();
        particles.event(
            &SimEvent {
                size: Some(3.0),
                ..at(SimEventType::Death)
            },
            &mut random,
        );
        assert_eq!(particles.particles.len(), 8);
        assert!(
            particles
                .particles
                .iter()
                .all(|p| p.vy >= 3.5 && p.life >= 0.8)
        );
    }

    #[test]
    fn burnout_uses_thin_smoke_with_no_shockwave_or_light() {
        let mut random = CosmeticRandom::default();
        let mut particles = ParticleEffects::default();
        let burnout = SimEvent {
            death_style: Some(DeathStyle::Burnout),
            size: Some(3.0),
            ..at(SimEventType::Death)
        };
        assert!(!particles.event(&burnout, &mut random));
        assert_eq!(particles.particles.len(), 3);
        particles.update(0.5, 0.5);
        assert_eq!(particles.explosions.rings.len(), 0);
        assert_eq!(particles.explosions.puffs.len(), 3);
    }

    /// `cover-hit-effects.test.ts`, particle part: hits chip, a fatal hit bursts.
    /// A tree's leaves are the leaf fall's (`leaves.rs`), not particles.
    #[test]
    fn cover_hits_chip_trees_timber_and_cargo() {
        let mut random = CosmeticRandom::constant(0.5);
        let chips = |p: &ParticleEffects| {
            p.particles
                .iter()
                .filter(|q| q.shape == Some(ParticleShape::Splinter))
                .count()
        };
        for cover in [CoverKind::Tree, CoverKind::Timber, CoverKind::Cargo] {
            let mut hit = ParticleEffects::default();
            hit.event(&with_cover(SimEventType::Impact, cover), &mut random);
            assert!(chips(&hit) > 0, "{cover:?} hit chips");
            assert_eq!(chips(&hit), hit.particles.len());
            let mut destroyed = ParticleEffects::default();
            destroyed.event(&with_cover(SimEventType::Destroy, cover), &mut random);
            if cover == CoverKind::Tree {
                assert_eq!(
                    destroyed.particles.len(),
                    0,
                    "falling tree parts replace the burst"
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
        assert_eq!(particles.particles.len(), 5);
        particles.event(&at(SimEventType::Ricochet), &mut random);
        assert_eq!(particles.particles.len(), 13);
        particles.event(&at(SimEventType::Hurt), &mut random);
        assert_eq!(particles.particles.len(), 25);
        particles.event(&at(SimEventType::Pickup), &mut random);
        assert_eq!(particles.particles.len(), 49);
        for _ in 0..200 {
            particles.event(&at(SimEventType::Pickup), &mut random);
        }
        assert_eq!(particles.particles.len(), MAX_PARTICLES);
        particles.update(1.0 / 60.0, 1.0);
        assert!(particles.records.len() <= MAX_PARTICLES);
        assert!(particles.records.records().iter().all(|r| {
            r.world_rows.as_flattened().iter().all(|v| v.is_finite()) && r.translation().y >= 0.1
        }));
        for _ in 0..120 {
            particles.update(1.0 / 60.0, 1.0);
        }
        assert!(particles.particles.is_empty());
        assert!(particles.records.is_empty(), "an empty pool draws nothing");
    }
}
