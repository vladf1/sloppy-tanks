//! Glowing streaks in one bounded additive draw: hot sparks and embers thrown by
//! blasts, hits and ricochets, stretched along their flight; plus the transient
//! glows other effects write into the same pool each frame after the sparks
//! (blast flash cores, muzzle flash tongues, shell tracers and their heads).

use glam::{Mat4, Quat, Vec3};

use super::pool::{PoolBuffer, record};
use super::random::CosmeticRandom;

/// Simulated sparks alive at once.
pub const MAX_SPARKS: usize = 900;
/// Every streak record a frame may hold: sparks, glows, tongues and tracers.
pub const STREAK_CAPACITY: usize = 2048;
const SPARK_GRAVITY: f32 = 11.0;
/// Air drag (1/s): sparks shoot out fast and hang a moment before they fall.
const SPARK_DRAG: f32 = 2.6;
/// The ground bounces sparks back up with this share of their fall speed.
const BOUNCE: f32 = 0.35;
const GROUND: f32 = 0.05;
/// A streak is as long as this many seconds of its flight, within these bounds (m).
const STRETCH_SECONDS: f32 = 0.045;
const STRETCH: [f32; 2] = [0.08, 1.6];

/// The streak shader's kinds (`streak.wgsl`).
const KIND_STREAK: f32 = 0.0;
const KIND_GLOW: f32 = 1.0;
const KIND_TONGUE: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spark {
    pub position: Vec3,
    pub velocity: Vec3,
    pub life: f32,
    max: f32,
    width: f32,
    /// Linear HDR color at birth; it cools toward deep red as the spark dies.
    color: [f32; 3],
}

/// How a burst of sparks flies: count, speed range (m/s), upward bias, life range
/// (s), width (m) and hot color.
#[derive(Clone, Copy, Debug)]
pub struct Burst {
    pub count: usize,
    pub speed: [f32; 2],
    pub lift: f32,
    pub life: [f32; 2],
    pub width: f32,
    pub color: [f32; 3],
}

/// A blast's hot sparks and embers.
pub const BLAST_SPARKS: Burst = Burst {
    count: 26,
    speed: [7.0, 19.0],
    lift: 0.75,
    life: [0.45, 0.9],
    width: 0.09,
    color: [5.0, 2.6, 0.7],
};
/// A shell striking steel: a bright, quick spray.
pub const METAL_SPARKS: Burst = Burst {
    count: 16,
    speed: [6.0, 15.0],
    lift: 0.45,
    life: [0.18, 0.4],
    width: 0.06,
    color: [6.0, 4.4, 1.8],
};
/// A shell striking stone or concrete.
pub const STONE_SPARKS: Burst = Burst {
    count: 9,
    speed: [4.0, 10.0],
    lift: 0.5,
    life: [0.15, 0.32],
    width: 0.055,
    color: [5.0, 3.2, 1.1],
};
/// A muzzle's burning propellant flecks.
pub const MUZZLE_SPARKS: Burst = Burst {
    count: 5,
    speed: [6.0, 13.0],
    lift: 0.2,
    life: [0.12, 0.26],
    width: 0.05,
    color: [5.5, 3.0, 0.9],
};

#[derive(Clone, Debug)]
pub struct Sparks {
    pub sparks: Vec<Spark>,
    pub records: PoolBuffer,
}

impl Default for Sparks {
    fn default() -> Self {
        Self {
            sparks: Vec::with_capacity(MAX_SPARKS),
            records: PoolBuffer::new(STREAK_CAPACITY),
        }
    }
}

impl Sparks {
    pub fn reset(&mut self) {
        self.sparks.clear();
        self.records.clear();
    }

    pub fn len(&self) -> usize {
        self.sparks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sparks.is_empty()
    }

    /// Throw `burst` from `origin`, fanned around `direction` (any length; zero
    /// throws every way) by `spread` (0 a tight cone .. 1 a hemisphere).
    pub fn burst(
        &mut self,
        origin: Vec3,
        direction: Vec3,
        spread: f32,
        burst: &Burst,
        random: &mut CosmeticRandom,
    ) {
        let aim = direction.normalize_or_zero();
        for _ in 0..burst.count {
            if self.sparks.len() >= MAX_SPARKS {
                return;
            }
            let mut next = || random.next_f64() as f32;
            let around = Vec3::new(next() * 2.0 - 1.0, next() * 2.0 - 1.0, next() * 2.0 - 1.0)
                .normalize_or(Vec3::Y);
            let mut heading = if aim == Vec3::ZERO {
                around
            } else {
                (aim + around * spread * 1.4).normalize_or(aim)
            };
            heading.y = heading.y.abs() * 0.6 + burst.lift * (0.4 + next() * 0.6);
            let speed = burst.speed[0] + (burst.speed[1] - burst.speed[0]) * next();
            let life = burst.life[0] + (burst.life[1] - burst.life[0]) * next();
            let shade = 0.75 + next() * 0.5;
            self.sparks.push(Spark {
                position: origin,
                velocity: heading.normalize_or(Vec3::Y) * speed,
                life,
                max: life,
                width: burst.width * (0.7 + next() * 0.6),
                color: burst.color.map(|c| c * shade),
            });
        }
    }

    /// Advance the sparks and rewrite the pool with them; the frame's glows,
    /// tongues and tracers are pushed after this.
    pub fn update(&mut self, dt: f64) {
        let dt = dt as f32;
        let drag = (-SPARK_DRAG * dt).exp();
        self.sparks.retain_mut(|spark| {
            spark.life -= dt;
            if spark.life <= 0.0 {
                return false;
            }
            spark.velocity *= drag;
            spark.velocity.y -= SPARK_GRAVITY * dt;
            spark.position += spark.velocity * dt;
            if spark.position.y < GROUND {
                spark.position.y = GROUND;
                spark.velocity.y = -spark.velocity.y * BOUNCE;
                spark.velocity.x *= 0.6;
                spark.velocity.z *= 0.6;
            }
            true
        });
        self.records.clear();
        for spark in &self.sparks {
            let fade = spark.life / spark.max;
            let speed = spark.velocity.length();
            let length = (speed * STRETCH_SECONDS).clamp(STRETCH[0], STRETCH[1]);
            let direction = spark.velocity / speed.max(1e-4);
            // White-hot sparks cool to orange and deep red as they die.
            let cool = [1.0, 0.45 + 0.55 * fade, 0.15 + 0.85 * fade * fade];
            let color = [0, 1, 2].map(|i| spark.color[i] * cool[i]);
            streak(
                &mut self.records,
                spark.position - direction * (length * 0.5),
                direction,
                spark.width * (0.5 + 0.5 * fade),
                length,
                [color[0], color[1], color[2], fade.sqrt()],
            );
        }
    }
}

/// A streak centred on `center`, `length` metres along `direction`.
pub fn streak(
    records: &mut PoolBuffer,
    center: Vec3,
    direction: Vec3,
    width: f32,
    length: f32,
    tint: [f32; 4],
) -> bool {
    let world = Mat4::from_scale_rotation_translation(
        Vec3::new(width, length, 1.0),
        Quat::IDENTITY,
        center,
    );
    records.push(record(
        world,
        tint,
        [direction.x, direction.y, direction.z, KIND_STREAK],
    ))
}

/// A round camera-facing glow `diameter` metres across.
pub fn glow(records: &mut PoolBuffer, center: Vec3, diameter: f32, tint: [f32; 4]) -> bool {
    let world = Mat4::from_scale_rotation_translation(
        Vec3::new(diameter, diameter, 1.0),
        Quat::IDENTITY,
        center,
    );
    records.push(record(world, tint, [0.0, 0.0, 0.0, KIND_GLOW]))
}

/// A flame tongue from `base`, `length` metres along `direction`. The shader's
/// raggedness reads `seed` from the direction's x, so it is passed through.
pub fn tongue(
    records: &mut PoolBuffer,
    base: Vec3,
    direction: Vec3,
    width: f32,
    length: f32,
    tint: [f32; 4],
) -> bool {
    let world =
        Mat4::from_scale_rotation_translation(Vec3::new(width, length, 1.0), Quat::IDENTITY, base);
    records.push(record(
        world,
        tint,
        [direction.x, direction.y, direction.z, KIND_TONGUE],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparks_fly_cool_and_retire_within_their_pool() {
        let mut random = CosmeticRandom::seeded(3);
        let mut sparks = Sparks::default();
        for _ in 0..100 {
            sparks.burst(Vec3::Y, Vec3::ZERO, 1.0, &BLAST_SPARKS, &mut random);
        }
        assert_eq!(sparks.len(), MAX_SPARKS);
        sparks.update(1.0 / 60.0);
        assert_eq!(sparks.records.len(), MAX_SPARKS);
        assert!(sparks.records.records().iter().all(|r| {
            r.world_rows.as_flattened().iter().all(|v| v.is_finite())
                && r.translation().y >= GROUND - STRETCH[1]
        }));
        for _ in 0..80 {
            sparks.update(1.0 / 60.0);
        }
        assert!(sparks.is_empty() && sparks.records.is_empty());
    }

    #[test]
    fn aimed_bursts_fly_along_their_direction() {
        let mut random = CosmeticRandom::seeded(9);
        let mut sparks = Sparks::default();
        sparks.burst(Vec3::Y, Vec3::X, 0.3, &METAL_SPARKS, &mut random);
        let forward = sparks.sparks.iter().filter(|s| s.velocity.x > 0.0).count();
        assert!(
            forward * 4 >= sparks.len() * 3,
            "{forward}/{}",
            sparks.len()
        );
    }
}
