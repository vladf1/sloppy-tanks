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
/// Air drag (1/s): sparks shoot out fast and arc down; embers hang in the air.
const SPARK_DRAG: f32 = 1.5;
const EMBER_DRAG: f32 = 2.2;
/// The ground bounces sparks back up with this share of their fall speed, and a
/// landed spark dies within this many seconds instead of lying there glowing.
const BOUNCE: f32 = 0.3;
const LANDED_LIFE: f32 = 0.09;
const GROUND: f32 = 0.05;
/// A streak is as long as some seconds of its flight (each spark draws its own
/// from this range, so a burst never reads as equal spokes), within these
/// bounds (m).
const STRETCH_SECONDS: [f32; 2] = [0.018, 0.07];
const STRETCH: [f32; 2] = [0.06, 1.3];

/// The streak shader's kinds (`streak.wgsl`).
const KIND_STREAK: f32 = 0.0;
const KIND_GLOW: f32 = 1.0;
const KIND_TONGUE: f32 = 2.0;
const KIND_TRACER: f32 = 3.0;
/// A licking flame, with its seed in the kind's fraction (0..0.49).
const KIND_FLAME: f32 = 4.0;
const KIND_GROUND_GLOW: f32 = 5.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spark {
    pub position: Vec3,
    pub velocity: Vec3,
    pub life: f32,
    max: f32,
    width: f32,
    /// Downward acceleration (m/s²); negative for embers carried up on the heat.
    gravity: f32,
    /// Flicker phase; zero for a steady spark.
    flicker: f32,
    /// Linear HDR color at birth; a hot spark cools toward deep red as it dies.
    color: [f32; 3],
    cools: bool,
    /// Seconds of flight its streak stretches over.
    stretch: f32,
    /// Seconds before it leaves the fire, unseen until then.
    delay: f32,
    /// Where it was thrown from and how far out the fire hides it (m), so a
    /// blast's sparks streak out of its fireball instead of across it.
    origin: Vec3,
    hide: f32,
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
    /// Downward acceleration (m/s²); negative makes buoyant embers.
    pub gravity: f32,
    /// Sparks leave from anywhere within this radius of the origin (m)...
    pub scatter: f32,
    /// ...over this many seconds.
    pub stagger: f32,
    /// The fire they leave hides them within this radius (m).
    pub hide: f32,
    /// Hot metal cools to red as it dies; light keeps its color and only fades.
    pub cools: bool,
}

/// A blast's hot sparks: fast yellow-orange streaks that leave the fireball and
/// arc down under gravity.
pub const BLAST_SPARKS: Burst = Burst {
    count: 20,
    speed: [6.0, 22.0],
    lift: 1.0,
    life: [0.35, 1.2],
    width: 0.12,
    color: [3.4, 1.15, 0.2],
    gravity: 14.0,
    scatter: 0.9,
    stagger: 0.14,
    hide: 3.0,
    cools: true,
};
/// Embers carried up on a fire's heat, flickering as they die.
pub const EMBERS: Burst = Burst {
    count: 14,
    speed: [1.2, 4.5],
    lift: 1.4,
    life: [0.9, 2.0],
    width: 0.08,
    color: [3.6, 0.95, 0.14],
    gravity: -1.8,
    scatter: 0.6,
    stagger: 0.3,
    hide: 0.0,
    cools: true,
};
/// A shell striking steel: a bright, quick spray glancing off the armour.
pub const METAL_SPARKS: Burst = Burst {
    count: 11,
    speed: [7.0, 15.0],
    lift: 0.45,
    life: [0.2, 0.42],
    width: 0.07,
    color: [3.6, 1.4, 0.3],
    gravity: 13.0,
    scatter: 0.1,
    stagger: 0.03,
    hide: 0.0,
    cools: true,
};
/// A shell striking stone or concrete.
pub const STONE_SPARKS: Burst = Burst {
    count: 9,
    speed: [5.0, 11.0],
    lift: 0.5,
    life: [0.15, 0.32],
    width: 0.055,
    color: [3.8, 1.8, 0.45],
    gravity: 13.0,
    scatter: 0.1,
    stagger: 0.03,
    hide: 0.0,
    cools: true,
};
/// Light thrown off a collected pickup: quick streaks fanning up and out in the
/// kind's color (set per event), slowing to hang a moment before they fade.
pub const PICKUP_SPARKS: Burst = Burst {
    count: 16,
    speed: [4.0, 9.0],
    lift: 1.2,
    life: [0.3, 0.6],
    width: 0.075,
    color: [1.0, 1.0, 1.0],
    gravity: 3.0,
    scatter: 0.7,
    stagger: 0.06,
    hide: 0.0,
    cools: false,
};
/// A muzzle's burning propellant flecks.
pub const MUZZLE_SPARKS: Burst = Burst {
    count: 4,
    speed: [7.0, 14.0],
    lift: 0.2,
    life: [0.1, 0.22],
    width: 0.045,
    color: [4.0, 1.6, 0.35],
    gravity: 13.0,
    scatter: 0.05,
    stagger: 0.02,
    hide: 0.0,
    cools: true,
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
            let flicker = if burst.gravity < 0.0 {
                1.0 + next() * 40.0
            } else {
                0.0
            };
            let stretch = STRETCH_SECONDS[0] + (STRETCH_SECONDS[1] - STRETCH_SECONDS[0]) * next();
            let delay = burst.stagger * next() * next();
            let offset = around * burst.scatter * next().sqrt();
            self.sparks.push(Spark {
                position: origin + offset,
                velocity: heading.normalize_or(Vec3::Y) * speed,
                life,
                max: life,
                width: burst.width * (0.6 + next() * 0.8),
                gravity: burst.gravity,
                flicker,
                color: burst.color.map(|c| c * shade),
                cools: burst.cools,
                stretch,
                delay,
                origin,
                hide: burst.hide,
            });
        }
    }

    /// Advance the sparks and rewrite the pool with them; the frame's glows,
    /// tongues and tracers are pushed after this.
    pub fn update(&mut self, dt: f64) {
        let dt = dt as f32;
        let spark_drag = (-SPARK_DRAG * dt).exp();
        let ember_drag = (-EMBER_DRAG * dt).exp();
        self.sparks.retain_mut(|spark| {
            if spark.delay > 0.0 {
                spark.delay -= dt;
                return true;
            }
            spark.life -= dt;
            if spark.life <= 0.0 {
                return false;
            }
            let ember = spark.flicker > 0.0;
            spark.velocity *= if ember { ember_drag } else { spark_drag };
            spark.velocity.y -= spark.gravity * dt;
            if ember {
                // Embers wander on the eddies of the hot air they ride.
                let swirl = spark.flicker + spark.life * 3.0;
                spark.velocity.x += swirl.sin() * 3.0 * dt;
                spark.velocity.z += (swirl * 1.3).cos() * 3.0 * dt;
            }
            spark.position += spark.velocity * dt;
            if spark.position.y < GROUND {
                spark.position.y = GROUND;
                spark.velocity.y = -spark.velocity.y * BOUNCE;
                spark.velocity.x *= 0.5;
                spark.velocity.z *= 0.5;
                spark.life = spark.life.min(LANDED_LIFE);
            }
            true
        });
        self.records.clear();
        for spark in self.sparks.iter().filter(|spark| spark.delay <= 0.0) {
            let fade = spark.life / spark.max;
            let speed = spark.velocity.length();
            // A spark's trail grows over its first moments in the air.
            let flown = ((spark.max - spark.life) / spark.stretch).min(1.0);
            let length = (speed * spark.stretch * flown).clamp(STRETCH[0], STRETCH[1]);
            let direction = spark.velocity / speed.max(1e-4);
            // Hot sparks cool to orange and deep red as they die; embers flicker.
            let cool = if spark.cools {
                [1.0, 0.4 + 0.6 * fade, 0.1 + 0.9 * fade * fade]
            } else {
                [1.0; 3]
            };
            let flicker = if spark.flicker > 0.0 {
                0.55 + 0.45 * (spark.flicker + spark.life * 23.0).sin()
            } else {
                1.0
            };
            let color = [0, 1, 2].map(|i| spark.color[i] * cool[i] * flicker);
            let out = if spark.hide > 0.0 {
                let t = (((spark.position - spark.origin).length() - spark.hide * 0.5)
                    / (spark.hide * 0.5))
                    .clamp(0.0, 1.0);
                t * t * (3.0 - 2.0 * t)
            } else {
                1.0
            };
            streak(
                &mut self.records,
                spark.position - direction * (length * 0.5),
                direction,
                spark.width * (0.5 + 0.5 * fade),
                length,
                [color[0], color[1], color[2], fade.sqrt() * out],
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

/// A shell tracer: a tapered glowing tail `length` metres behind `head`, hottest
/// at its head.
pub fn tracer_streak(
    records: &mut PoolBuffer,
    head: Vec3,
    direction: Vec3,
    width: f32,
    length: f32,
    tint: [f32; 4],
) -> bool {
    let world = Mat4::from_scale_rotation_translation(
        Vec3::new(width, length, 1.0),
        Quat::IDENTITY,
        head - direction * (length * 0.5),
    );
    records.push(record(
        world,
        tint,
        [direction.x, direction.y, direction.z, KIND_TRACER],
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

/// A licking flame from `base`, `length` metres along `direction`: yellow at its
/// root, burning out through orange to deep red licks at its tip. `seed` (0..1)
/// sets its licks apart from its neighbours'.
pub fn flame(
    records: &mut PoolBuffer,
    base: Vec3,
    direction: Vec3,
    width: f32,
    length: f32,
    seed: f32,
    tint: [f32; 4],
) -> bool {
    let world =
        Mat4::from_scale_rotation_translation(Vec3::new(width, length, 1.0), Quat::IDENTITY, base);
    records.push(record(
        world,
        tint,
        [
            direction.x,
            direction.y,
            direction.z,
            KIND_FLAME + seed.fract().abs() * 0.49,
        ],
    ))
}

/// Firelight on the ground: an additive glow `diameter` metres across lying
/// flat at `center`, brightest under the fire.
pub fn ground_glow(records: &mut PoolBuffer, center: Vec3, diameter: f32, tint: [f32; 4]) -> bool {
    let world = Mat4::from_scale_rotation_translation(
        Vec3::new(diameter, diameter, 1.0),
        Quat::IDENTITY,
        center,
    );
    records.push(record(world, tint, [0.0, 1.0, 0.0, KIND_GROUND_GLOW]))
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
        // Staggered sparks leave the fire over its first moments.
        for _ in 0..12 {
            sparks.update(1.0 / 60.0);
        }
        assert!(sparks.records.len() > MAX_SPARKS * 3 / 4);
        assert!(sparks.records.records().iter().all(|r| {
            r.world_rows.as_flattened().iter().all(|v| v.is_finite())
                && r.translation().y >= GROUND - STRETCH[1]
        }));
        for _ in 0..80 {
            sparks.update(1.0 / 60.0);
        }
        assert!(
            sparks.len() < MAX_SPARKS / 4,
            "most have landed or burnt out"
        );
        for _ in 0..10 {
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
