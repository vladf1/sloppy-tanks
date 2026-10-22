//! Burning wrecks: after a tank's death blast, flames lick up out of the hull's
//! engine deck and open turret ring, spitting embers and lighting the ground and
//! everything near it, and a thick black column of smoke rises off the hull and
//! leans away on the wind, thinning to a smoulder until the tank respawns. The
//! turret tossed by the blast trails fire and smoke as it flies. Each wreck
//! follows its hull fragment as it is thrown and settles. Analytic streams of
//! puffs cycling through the shared puff pool plus a short trail history per
//! wreck; bounded wreck slots.

use std::f64::consts::PI;

use glam::{Quat, Vec3};
use sloppy_core::geometry::math::smoothstep;
use sloppy_core::sim::render_state::RenderFragment;
use sloppy_core::sim::{DeathStyle, RenderState, SimEvent, SimEventType, WreckPart};

use super::Flash;
use super::explosions::{GROUND_GLOW, WIND, smoke_puff};
use super::pool::{PoolBuffer, record};
use super::random::CosmeticRandom;
use super::sparks::{EMBERS, Sparks, tongue};
use crate::color::hex_to_linear;

pub const MAX_BURNING_WRECKS: usize = 12;
const SMOKE_PUFFS: usize = 7;
const FLAME_PUFFS: usize = 3;
const TRAIL_PUFFS: usize = 6;
pub const PUFFS_PER_WRECK: usize = SMOKE_PUFFS + FLAME_PUFFS + TRAIL_PUFFS;
/// Flames burn this long; smoke keeps rising for longer.
const BURN_SECONDS: f64 = 12.0;
const SMOULDER_SECONDS: f64 = 24.0;
/// A puff's trip up the column, and a flame's flicker (s).
const SMOKE_CYCLE: f64 = 2.8;
const FLAME_CYCLE: f64 = 0.55;
/// The smoke column's height over a puff's trip (m).
const COLUMN_HEIGHT: f64 = 6.5;
/// Where flames burn on a hull, in its own frame (m): the engine deck, the open
/// turret ring and a hatch.
const FLAME_SPOTS: [[f32; 3]; FLAME_PUFFS] =
    [[0.0, 0.3, -0.8], [0.1, 0.35, 0.1], [-0.4, 0.3, -0.35]];
/// A death's wreck pieces are looked up this far from it, this soon after it.
const ANCHOR_RADIUS: f64 = 7.0;
const ANCHOR_SECONDS: f64 = 1.5;
/// The turret's trail: samples of its flight this far apart (s), drawn while
/// it is this young.
const TRAIL_SAMPLE: f64 = 0.03;
const TRAIL_SECONDS: f64 = 2.6;
/// A burning wreck spits a few embers this often (s).
const EMBER_INTERVAL: f64 = 0.35;
/// The fire's glow on the ground (m across) and its light.
const GLOW_SIZE: f32 = 5.0;
const GLOW_COLOR: [f32; 3] = [2.4, 0.85, 0.2];
const FIRE_LIGHT: f64 = 16.0;
const FIRE_LIGHT_COLOR: u32 = 0xff7a30;
const FLAME_COLOR: [f32; 3] = [4.0, 1.45, 0.3];
/// A respawned tank's wreck smoke dies away this fast (1/s).
const CLEAR_RATE: f64 = 1.5;
const SOOT: u32 = 0x555456;
const SOOT_HIGH: u32 = 0x8a8886;
/// A dead tank's hull rests this high before its wreck piece is found.
const REST_HEIGHT: f32 = 0.65;

#[derive(Clone, Copy, Debug)]
struct Burner {
    active: bool,
    id: Option<u32>,
    x: f64,
    z: f64,
    age: f64,
    seed: f64,
    /// A burnout smokes without much fire.
    burnout: bool,
    /// 0 while the wreck stands; rises to 1 once the tank respawned.
    cleared: f64,
    /// The hull and turret wreck pieces this fire follows, once found.
    hull: Option<u32>,
    turret: Option<u32>,
    /// The hull's pose, or the death's place until the hull is found.
    position: Vec3,
    rotation: Quat,
    /// The turret's recent positions, newest first.
    trail: [Vec3; TRAIL_PUFFS],
    trail_count: usize,
    trail_clock: f64,
    ember_clock: f64,
    /// This frame's fire strength and flicker, for its light.
    fire: f64,
}

impl Default for Burner {
    fn default() -> Self {
        Self {
            active: false,
            id: None,
            x: 0.0,
            z: 0.0,
            age: 0.0,
            seed: 0.0,
            burnout: false,
            cleared: 0.0,
            hull: None,
            turret: None,
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            trail: [Vec3::ZERO; TRAIL_PUFFS],
            trail_count: 0,
            trail_clock: 0.0,
            ember_clock: 0.0,
            fire: 0.0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct WreckFires {
    burners: [Burner; MAX_BURNING_WRECKS],
    cursor: usize,
}

fn fragment_pose(fragment: &RenderFragment) -> (Vec3, Quat) {
    let p = fragment.position;
    let r = fragment.rotation;
    (
        Vec3::new(p.x as f32, p.y as f32, p.z as f32),
        Quat::from_xyzw(r.x as f32, r.y as f32, r.z as f32, r.w as f32).normalize(),
    )
}

impl WreckFires {
    pub fn reset(&mut self) {
        for burner in &mut self.burners {
            burner.active = false;
        }
        self.cursor = 0;
    }

    pub fn active(&self) -> usize {
        self.burners.iter().filter(|burner| burner.active).count()
    }

    pub fn event(&mut self, event: &SimEvent, random: &mut CosmeticRandom) {
        if event.kind != SimEventType::Death {
            return;
        }
        self.burners[self.cursor] = Burner {
            active: true,
            id: event.id,
            x: event.x,
            z: event.z,
            seed: random.next_f64(),
            burnout: event.death_style == Some(DeathStyle::Burnout),
            position: Vec3::new(event.x as f32, REST_HEIGHT, event.z as f32),
            ..Burner::default()
        };
        self.cursor = (self.cursor + 1) % MAX_BURNING_WRECKS;
    }

    /// Offer each burning wreck's flickering firelight to `lights`.
    pub fn lights(&self, lights: &mut [Flash]) {
        for burner in self.burners.iter().filter(|burner| burner.active) {
            super::offer_light(
                lights,
                Flash {
                    position: burner.position + Vec3::Y * 1.3,
                    intensity: (FIRE_LIGHT * burner.fire) as f32,
                    color: FIRE_LIGHT_COLOR,
                },
            );
        }
    }

    /// Advance every wreck fire and write its smoke and flames into `puffs`, its
    /// ground glow into `decals` and its flame tongues and embers into `sparks`.
    pub fn update(
        &mut self,
        state: &RenderState,
        dt: f64,
        puffs: &mut PoolBuffer,
        decals: &mut PoolBuffer,
        sparks: &mut Sparks,
        random: &mut CosmeticRandom,
    ) {
        for burner in &mut self.burners {
            if !burner.active {
                continue;
            }
            burner.age += dt.max(0.0);
            let standing = burner
                .id
                .is_none_or(|id| state.tanks.iter().any(|tank| tank.id == id && !tank.alive));
            if !standing {
                burner.cleared = (burner.cleared + dt * CLEAR_RATE).min(1.0);
            }
            if burner.age >= SMOULDER_SECONDS || burner.cleared >= 1.0 {
                burner.active = false;
                continue;
            }
            follow(burner, state, dt);
            draw(burner, state.elapsed, dt, puffs, decals, sparks, random);
        }
    }
}

/// Find the death's hull and turret pieces, then follow them.
fn follow(burner: &mut Burner, state: &RenderState, dt: f64) {
    if burner.age < ANCHOR_SECONDS && (burner.hull.is_none() || burner.turret.is_none()) {
        let born = state.elapsed - burner.age - 0.5;
        let mut nearest = [(f64::MAX, None); 2];
        for fragment in &state.fragments {
            let (Some(_), Some(part)) = (fragment.wreck, fragment.part) else {
                continue;
            };
            if fragment.created_at.is_some_and(|at| at < born) {
                continue;
            }
            let distance = (fragment.position.x - burner.x).hypot(fragment.position.z - burner.z);
            if distance > ANCHOR_RADIUS {
                continue;
            }
            let slot = match part {
                WreckPart::Intact | WreckPart::Hull => 0,
                WreckPart::Turret | WreckPart::TurretBarrel => 1,
                WreckPart::Barrel => continue,
            };
            if distance < nearest[slot].0 {
                nearest[slot] = (distance, Some(fragment.id));
            }
        }
        burner.hull = burner.hull.or(nearest[0].1);
        burner.turret = burner.turret.or(nearest[1].1);
    }
    let find = |id: Option<u32>| {
        id.and_then(|id| state.fragments.iter().find(|fragment| fragment.id == id))
    };
    if let Some(hull) = find(burner.hull) {
        (burner.position, burner.rotation) = fragment_pose(hull);
    }
    if let Some(turret) = find(burner.turret) {
        let (position, _) = fragment_pose(turret);
        burner.trail_clock += dt;
        if burner.trail_count == 0 || burner.trail_clock >= TRAIL_SAMPLE {
            burner.trail_clock = 0.0;
            burner.trail.copy_within(0..TRAIL_PUFFS - 1, 1);
            burner.trail_count = (burner.trail_count + 1).min(TRAIL_PUFFS);
        }
        burner.trail[0] = position;
    }
}

fn draw(
    burner: &mut Burner,
    time: f64,
    dt: f64,
    puffs: &mut PoolBuffer,
    decals: &mut PoolBuffer,
    sparks: &mut Sparks,
    random: &mut CosmeticRandom,
) {
    let soot = hex_to_linear(SOOT);
    let soot_high = hex_to_linear(SOOT_HIGH);
    let age = burner.age;
    let left = 1.0 - burner.cleared;
    let fire = smoothstep(age, 0.2, 0.8)
        * (1.0 - smoothstep(age, BURN_SECONDS * 0.55, BURN_SECONDS))
        * left
        * if burner.burnout { 0.45 } else { 1.0 };
    let flicker = 0.8
        + 0.2 * (time * 13.0 + burner.seed * 40.0).sin() * (time * 7.3 + burner.seed * 9.0).sin();
    burner.fire = fire * flicker;
    let smoke = smoothstep(age, 0.3, 1.2)
        * (1.0 - smoothstep(age, SMOULDER_SECONDS * 0.55, SMOULDER_SECONDS))
        * left
        * (0.55 + 0.45 * (1.0 - smoothstep(age, BURN_SECONDS * 0.6, BURN_SECONDS)));
    let base = burner.position;
    let deck = |spot: [f32; 3]| base + burner.rotation * Vec3::from(spot);
    let top = deck([0.0, 0.4, -0.3]);
    for j in 0..SMOKE_PUFFS {
        let cycle = age / SMOKE_CYCLE + j as f64 / SMOKE_PUFFS as f64 + burner.seed;
        let a = cycle.fract();
        let lap = cycle.floor();
        let swirl = (lap * 2.3 + j as f64 + burner.seed * 7.0).sin() * 0.35;
        let rise = a.powf(0.75);
        let lean = a * a * SMOKE_CYCLE;
        let position = Vec3::new(
            top.x + (WIND[0] * lean + swirl) as f32,
            top.y + (COLUMN_HEIGHT * rise) as f32,
            top.z + (WIND[1] * lean - swirl * 0.6) as f32,
        );
        let color = [0, 1, 2].map(|i| soot[i] + (soot_high[i] - soot[i]) * a as f32);
        let size = 0.6 + 2.1 * a;
        smoke_puff(
            puffs,
            position,
            size as f32,
            color,
            (smoke * (PI * a.min(0.85) / 0.85).sin().powf(0.6) * 0.62 * (1.0 - 0.45 * a)) as f32,
            (burner.seed * 3.1 + lap * 0.37 + j as f64 * 0.19) as f32,
            0.0,
            (a * a * 0.8) as f32,
            0.12,
            (fire * (1.0 - a).powi(3)) as f32,
        );
    }
    trail(burner, puffs, soot);
    if fire <= 0.0 {
        return;
    }
    for (j, spot) in FLAME_SPOTS.into_iter().enumerate() {
        let cycle = age / FLAME_CYCLE + j as f64 / FLAME_PUFFS as f64 + burner.seed;
        let a = cycle.fract();
        let lap = cycle.floor();
        let at = deck(spot);
        let jitter = ((lap + j as f64) * 1.7).sin() * 0.12;
        let heat = fire * 1.3 * (1.0 - a * 0.8);
        smoke_puff(
            puffs,
            Vec3::new(
                at.x + (WIND[0] * a * 0.5 + jitter) as f32,
                at.y + (0.1 + 1.3 * a) as f32,
                at.z + (WIND[1] * a * 0.5 - jitter) as f32,
            ),
            (0.7 - 0.35 * a) as f32,
            soot,
            (fire * (a / 0.1).min(1.0) * (1.0 - a) * 0.9) as f32,
            (burner.seed * 5.3 + lap * 0.61 + j as f64 * 0.29) as f32,
            heat as f32,
            (a * 0.7) as f32,
            0.0,
            0.0,
        );
        // A licking tongue of flame over each burning spot, leaning on the wind.
        let lick = (time * 11.0 + j as f64 * 2.1 + burner.seed * 20.0).sin();
        let up = Vec3::new(WIND[0] as f32 * 0.25, 1.0, WIND[1] as f32 * 0.25).normalize();
        let [r, g, b] = FLAME_COLOR;
        tongue(
            &mut sparks.records,
            at,
            up,
            (0.55 + 0.1 * lick) as f32,
            ((0.9 + 0.35 * lick) * (0.6 + 0.4 * fire)) as f32,
            [r, g, b, (fire * (0.75 + 0.25 * flicker)) as f32],
        );
    }
    burner.ember_clock += dt;
    if burner.ember_clock >= EMBER_INTERVAL {
        burner.ember_clock = 0.0;
        let spot = FLAME_SPOTS[(random.next_f64() * FLAME_PUFFS as f64) as usize % FLAME_PUFFS];
        let mut embers = EMBERS;
        embers.count = 3;
        sparks.burst(deck(spot), Vec3::Y, 0.7, &embers, random);
    }
    let size = GLOW_SIZE * (0.9 + 0.1 * flicker as f32);
    let world = glam::Mat4::from_translation(Vec3::new(base.x, 0.08, base.z))
        * glam::Mat4::from_scale(Vec3::new(size, 1.0, size));
    let [r, g, b] = GLOW_COLOR;
    decals.push(record(
        world,
        [r, g, b, 1.0],
        [
            age as f32,
            burner.seed as f32,
            GROUND_GLOW,
            (burner.fire * 0.55) as f32,
        ],
    ));
}

/// Fire and smoke streaming behind the tossed turret, burning out as it flies.
fn trail(burner: &Burner, puffs: &mut PoolBuffer, soot: [f32; 3]) {
    if burner.trail_count == 0 || burner.age >= TRAIL_SECONDS {
        return;
    }
    let fade = 1.0 - smoothstep(burner.age, TRAIL_SECONDS * 0.55, TRAIL_SECONDS);
    let hot = 1.0 - smoothstep(burner.age, 0.2, 0.8);
    for (i, position) in burner.trail[..burner.trail_count].iter().enumerate() {
        let k = i as f64 / TRAIL_PUFFS as f64;
        let heat = if i == 0 { hot * 0.9 } else { 0.0 };
        smoke_puff(
            puffs,
            *position + Vec3::new(WIND[0] as f32, 0.6, WIND[1] as f32) * (k as f32 * 0.4),
            (0.4 + 0.75 * k) as f32,
            soot,
            (fade * (0.62 - 0.4 * k)) as f32,
            (burner.seed * 7.7 + i as f64 * 0.37) as f32,
            heat as f32,
            (0.2 + k * 0.6) as f32,
            0.3,
            if i == 0 { (hot * 0.5) as f32 } else { 0.0 },
        );
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use sloppy_core::sim::Point3;
    use sloppy_core::sim::render_state::RenderTank;

    fn step(fires: &mut WreckFires, state: &RenderState, puffs: &mut PoolBuffer) {
        let mut decals = PoolBuffer::new(64);
        let mut sparks = Sparks::default();
        let mut random = CosmeticRandom::seeded(5);
        puffs.clear();
        fires.update(
            state,
            1.0 / 60.0,
            puffs,
            &mut decals,
            &mut sparks,
            &mut random,
        );
    }

    #[test]
    fn wrecks_burn_until_their_tank_respawns_then_clear() {
        let mut state = RenderState::default();
        state.tanks.push(RenderTank {
            id: 7,
            alive: false,
            ..RenderTank::default()
        });
        let mut death = SimEvent::at(SimEventType::Death, 3.0, 4.0);
        death.id = Some(7);
        let mut fires = WreckFires::default();
        let mut random = CosmeticRandom::seeded(2);
        fires.event(&death, &mut random);
        let mut puffs = PoolBuffer::new(64);
        for _ in 0..120 {
            step(&mut fires, &state, &mut puffs);
        }
        assert_eq!(fires.active(), 1);
        assert!(puffs.len() > SMOKE_PUFFS, "smoke and flames");
        assert!(puffs.len() <= PUFFS_PER_WRECK);
        let mut lights = [Flash::default(); 3];
        fires.lights(&mut lights);
        assert!(lights.iter().any(|light| light.intensity > 1.0));
        state.tanks[0].alive = true;
        for _ in 0..60 {
            step(&mut fires, &state, &mut puffs);
        }
        assert_eq!(fires.active(), 0);
        for _ in 0..40 {
            fires.event(&death, &mut random);
        }
        assert_eq!(fires.active(), MAX_BURNING_WRECKS);
        fires.reset();
        assert_eq!(fires.active(), 0);
    }

    #[test]
    fn the_fire_follows_its_thrown_hull_and_the_turret_trails_smoke() {
        let mut state = RenderState::default();
        state.elapsed = 10.0;
        let mut death = SimEvent::at(SimEventType::Death, 0.0, 0.0);
        death.id = Some(3);
        let mut fires = WreckFires::default();
        fires.event(&death, &mut CosmeticRandom::seeded(1));
        state.fragments.push(RenderFragment {
            id: 40,
            wreck: Some(sloppy_core::sim::VehicleKind::default()),
            part: Some(WreckPart::Hull),
            created_at: Some(10.0),
            position: Point3::new(2.0, 0.6, 1.0),
            ..RenderFragment::default()
        });
        state.fragments.push(RenderFragment {
            id: 41,
            wreck: Some(sloppy_core::sim::VehicleKind::default()),
            part: Some(WreckPart::Turret),
            created_at: Some(10.0),
            position: Point3::new(-1.0, 3.0, 0.0),
            ..RenderFragment::default()
        });
        let mut puffs = PoolBuffer::new(64);
        for frame in 0..30 {
            state.fragments[0].position.x = 2.0 + f64::from(frame) * 0.1;
            state.fragments[1].position.y = 3.0 + f64::from(frame) * 0.05;
            step(&mut fires, &state, &mut puffs);
        }
        let burner = &fires.burners[0];
        assert_eq!((burner.hull, burner.turret), (Some(40), Some(41)));
        assert!((burner.position.x - 4.9).abs() < 1e-4);
        assert_eq!(burner.trail_count, TRAIL_PUFFS);
        assert!(puffs.len() >= SMOKE_PUFFS + TRAIL_PUFFS);
    }
}
