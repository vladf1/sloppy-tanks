//! Burning wrecks: after a tank's death blast, flames lick up out of the hull's
//! engine deck and open turret ring, spitting embers and lighting the ground and
//! everything near it, and a thick black column of smoke rises off the hull and
//! leans away on the wind, thinning to a smoulder until the wreck is cleared. The
//! turret tossed by the blast trails fire and smoke as it flies. Each wreck
//! follows its hull fragment as it is thrown: while the hull tumbles it only
//! trails smoke, and the fire takes hold once it has come to rest, burning up
//! out of whatever side faces the sky. Analytic streams of puffs cycling through
//! the shared puff pool plus short trail histories per wreck; bounded wreck slots.

use glam::{Quat, Vec3};
use sloppy_core::geometry::math::smoothstep;
use sloppy_core::sim::render_state::RenderFragment;
use sloppy_core::sim::{DeathStyle, RenderState, SimEvent, SimEventType, WreckPart};

use super::Flash;
use super::explosions::{WIND, smoke_puff};
use super::pool::PoolBuffer;
use super::random::CosmeticRandom;
use super::sparks::{EMBERS, Sparks, flame, glow, ground_glow};
use crate::color::hex_to_linear;

pub const MAX_BURNING_WRECKS: usize = 12;
const SMOKE_PUFFS: usize = 10;
const FLAME_PUFFS: usize = 3;
const TRAIL_PUFFS: usize = 6;
/// The turret's trail and the tumbling hull's.
/// Two rolling balls of fire over each burning spot, half a flicker apart.
const FIRE_BALLS: usize = 2;
pub const PUFFS_PER_WRECK: usize = SMOKE_PUFFS + FIRE_BALLS * FLAME_PUFFS + 2 * TRAIL_PUFFS;
/// Flames burn this long; smoke keeps rising for longer.
const BURN_SECONDS: f64 = 12.0;
const SMOULDER_SECONDS: f64 = 24.0;
/// A puff's trip up the column, and a flame's flicker (s).
const SMOKE_CYCLE: f64 = 3.4;
const FLAME_CYCLE: f64 = 0.62;
/// The smoke column's height over a puff's trip (m).
const COLUMN_HEIGHT: f64 = 8.5;
/// Where flames burn over a hull, along its length and across it in its own
/// heading (m), how high over its centre they start whichever way up it lies,
/// and how big each fire is (times the wreck's own variation).
const FLAME_SPOTS: [[f32; 3]; FLAME_PUFFS] =
    [[0.0, 0.0, -0.85], [0.1, 0.0, 0.1], [-0.45, 0.0, -0.35]];
const FLAME_HEIGHT: f32 = 0.42;
const FLAME_SIZES: [f32; FLAME_PUFFS] = [1.0, 0.85, 0.6];
/// Licks of flame per burning spot, and where each stands on the deck around
/// it (m, in the hull's frame, times the fire's size).
const LICKS: usize = 3;
const LICK_SPREAD: [[f32; 2]; LICKS] = [[-0.35, 0.15], [0.0, -0.12], [0.38, 0.2]];
/// A death's wreck pieces are looked up this far from it, this soon after it.
const ANCHOR_RADIUS: f64 = 7.0;
const ANCHOR_SECONDS: f64 = 1.5;
/// The turret's trail: samples of its flight this far apart (s), drawn while
/// it is this young.
const TRAIL_SAMPLE: f64 = 0.03;
const TRAIL_SECONDS: f64 = 2.6;
/// The hull counts as at rest below this speed (m/s), and the fire takes hold
/// and dies back at these rates (1/s) as it settles or is thrown again.
const REST_SPEED: f32 = 1.2;
const CATCH_RATE: f64 = 1.6;
const DOUSE_RATE: f64 = 5.0;
/// A fire whose hull piece was never found takes hold this late (s).
const UNFOUND_CATCH_SECONDS: f64 = 1.2;
/// The fire's soft glow behind its tongues (diameter per flame size, m).
const FLAME_GLOW: f32 = 1.7;
/// A burning wreck spits a few embers this often (s).
const EMBER_INTERVAL: f64 = 0.35;
/// The fire's glow on the ground (m across) and its light.
const GLOW_SIZE: f32 = 7.0;
const GLOW_COLOR: [f32; 3] = [0.9, 0.34, 0.07];
const FIRE_LIGHT: f64 = 13.0;
const FIRE_LIGHT_COLOR: u32 = 0xff9a55;
/// The firelight hangs this high over the hull (m), so the hull it burns on is
/// lit warm from above rather than painted orange.
const FIRE_LIGHT_HEIGHT: f32 = 3.2;
/// Burning fuel spilled on the ground beside the hull: where (m, across and
/// along the hull in its own heading) and how big each pool's licks are.
const GROUND_FIRES: [[f32; 3]; 2] = [[1.45, -0.6, 0.62], [-1.35, 0.9, 0.5]];
/// The flames' brightness multiplier (the shader shades yellow root to red tip).
const FLAME_COLOR: [f32; 3] = [0.82, 0.78, 0.78];
/// A cleared wreck's fire and smoke die away this fast (1/s).
const CLEAR_RATE: f64 = 0.9;
/// Oily black smoke at the fire, paling to grey as it climbs and spreads.
const SOOT: u32 = 0x2c2a29;
const SOOT_HIGH: u32 = 0x6c6c6e;
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
    /// The turret's and the flying hull's recent positions, newest first.
    trail: [Vec3; TRAIL_PUFFS],
    trail_count: usize,
    trail_clock: f64,
    hull_trail: [Vec3; TRAIL_PUFFS],
    hull_trail_count: usize,
    hull_trail_clock: f64,
    /// 0 while the hull flies, rising to 1 once it has come to rest.
    settled: f64,
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
            hull_trail: [Vec3::ZERO; TRAIL_PUFFS],
            hull_trail_count: 0,
            hull_trail_clock: 0.0,
            settled: 0.0,
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
                    position: burner.position + Vec3::Y * FIRE_LIGHT_HEIGHT,
                    intensity: (FIRE_LIGHT * burner.fire) as f32,
                    color: FIRE_LIGHT_COLOR,
                },
            );
        }
    }

    /// Advance every wreck fire and write its smoke and fireballs into `puffs`
    /// and its flames, ground glow and embers into `sparks`.
    pub fn update(
        &mut self,
        state: &RenderState,
        dt: f64,
        puffs: &mut PoolBuffer,
        sparks: &mut Sparks,
        random: &mut CosmeticRandom,
    ) {
        for burner in &mut self.burners {
            if !burner.active {
                continue;
            }
            burner.age += dt.max(0.0);
            // A fire burns as long as its hull lies there, and dies with it
            // rather than burning on over empty ground; a fire whose hull was
            // never found goes out when its tank respawns.
            let standing = match burner.hull {
                Some(hull) => state.fragments.iter().any(|fragment| fragment.id == hull),
                None => burner
                    .id
                    .is_none_or(|id| state.tanks.iter().any(|tank| tank.id == id && !tank.alive)),
            };
            if !standing {
                burner.cleared = (burner.cleared + dt * CLEAR_RATE).min(1.0);
            }
            if burner.age >= SMOULDER_SECONDS || burner.cleared >= 1.0 {
                burner.active = false;
                continue;
            }
            follow(burner, state, dt);
            draw(burner, state.elapsed, dt, puffs, sparks, random);
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
    let mut speed = 0.0;
    if let Some(hull) = find(burner.hull) {
        let (position, rotation) = fragment_pose(hull);
        if dt > 0.0 {
            speed = (position - burner.position).length() / dt as f32;
        }
        (burner.position, burner.rotation) = (position, rotation);
        if speed >= REST_SPEED {
            sample(
                &mut burner.hull_trail,
                &mut burner.hull_trail_count,
                &mut burner.hull_trail_clock,
                position,
                dt,
            );
        }
    }
    // Fire takes hold on a hull at rest and is blown out when it is thrown again.
    let resting = match burner.hull {
        Some(_) => speed < REST_SPEED,
        None => burner.age >= UNFOUND_CATCH_SECONDS,
    };
    burner.settled = if resting {
        (burner.settled + dt * CATCH_RATE).min(1.0)
    } else {
        (burner.settled - dt * DOUSE_RATE).max(0.0)
    };
    if let Some(turret) = find(burner.turret) {
        let (position, _) = fragment_pose(turret);
        sample(
            &mut burner.trail,
            &mut burner.trail_count,
            &mut burner.trail_clock,
            position,
            dt,
        );
    }
}

/// Keep `position` as the newest of a flight's samples, `TRAIL_SAMPLE` apart.
fn sample(
    trail: &mut [Vec3; TRAIL_PUFFS],
    count: &mut usize,
    clock: &mut f64,
    position: Vec3,
    dt: f64,
) {
    *clock += dt;
    if *count == 0 || *clock >= TRAIL_SAMPLE {
        *clock = 0.0;
        trail.copy_within(0..TRAIL_PUFFS - 1, 1);
        *count = (*count + 1).min(TRAIL_PUFFS);
    }
    trail[0] = position;
}

fn draw(
    burner: &mut Burner,
    time: f64,
    dt: f64,
    puffs: &mut PoolBuffer,
    sparks: &mut Sparks,
    random: &mut CosmeticRandom,
) {
    let soot = hex_to_linear(SOOT);
    let soot_high = hex_to_linear(SOOT_HIGH);
    let age = burner.age;
    let left = 1.0 - burner.cleared;
    let settled = smoothstep(burner.settled, 0.0, 1.0);
    let fire = settled
        * (1.0 - smoothstep(age, BURN_SECONDS * 0.55, BURN_SECONDS))
        * left
        * if burner.burnout { 0.45 } else { 1.0 };
    let flicker = 0.8
        + 0.2 * (time * 13.0 + burner.seed * 40.0).sin() * (time * 7.3 + burner.seed * 9.0).sin();
    burner.fire = fire * flicker;
    // The column stands over a wreck at rest; a flying hull trails its smoke.
    let smoke = settled
        * (1.0 - smoothstep(age, SMOULDER_SECONDS * 0.55, SMOULDER_SECONDS))
        * left
        * (0.55 + 0.45 * (1.0 - smoothstep(age, BURN_SECONDS * 0.6, BURN_SECONDS)));
    let base = burner.position;
    // Fires sit over the hull along its heading, on whichever side faces up:
    // a wreck on its roof burns out of its belly, never out of the ground.
    let forward = burner.rotation * Vec3::Z;
    let heading = Quat::from_rotation_y(forward.x.atan2(forward.z));
    let deck = |spot: [f32; 3]| base + heading * Vec3::from(spot) + Vec3::Y * FLAME_HEIGHT;
    let top = deck([0.0, 0.0, -0.3]);
    // Each wreck burns its own way: how big its fires are and how its column
    // leans and billows.
    let vigour = 0.85 + 0.35 * burner.seed;
    for j in 0..SMOKE_PUFFS {
        let cycle = age / SMOKE_CYCLE + j as f64 / SMOKE_PUFFS as f64 + burner.seed;
        let a = cycle.fract();
        let lap = cycle.floor();
        let swirl = (lap * 2.3 + j as f64 + burner.seed * 7.0).sin() * (0.25 + 0.4 * a);
        let rise = a.powf(0.8);
        // The wind shears the column more the higher it climbs.
        let lean = a * a * SMOKE_CYCLE * 1.3;
        let position = Vec3::new(
            top.x + (WIND[0] * lean + swirl) as f32,
            top.y + (0.3 + COLUMN_HEIGHT * rise) as f32,
            top.z + (WIND[1] * lean - swirl * 0.6) as f32,
        );
        let pale = smoothstep(a, 0.05, 0.9) as f32;
        let color = [0, 1, 2].map(|i| soot[i] + (soot_high[i] - soot[i]) * pale);
        let size = (0.8 + 2.5 * a.powf(0.85)) * vigour;
        // Born quickly at the fire, densest low, thinning out as it spreads.
        let opacity = smoke * smoothstep(a, 0.0, 0.08) * (1.0 - smoothstep(a, 0.4, 1.0)) * 0.86;
        smoke_puff(
            puffs,
            position,
            size as f32,
            color,
            opacity as f32,
            (burner.seed * 3.1 + lap * 0.37 + j as f64 * 0.19) as f32,
            0.0,
            (smoothstep(a, 0.35, 1.0) * 0.8) as f32,
            (0.08 + 0.3 * a) as f32,
            (fire * (1.0 - a).powi(6) * 0.6) as f32,
        );
    }
    trail(burner, &burner.trail, burner.trail_count, puffs, soot);
    trail(
        burner,
        &burner.hull_trail,
        burner.hull_trail_count,
        puffs,
        soot,
    );
    if fire <= 0.0 {
        return;
    }
    let up = Vec3::new(WIND[0] as f32 * 0.22, 1.0, WIND[1] as f32 * 0.22).normalize();
    let side = Vec3::new(-WIND[1] as f32, 0.0, WIND[0] as f32).normalize();
    for (j, spot) in FLAME_SPOTS.into_iter().enumerate() {
        let size = FLAME_SIZES[j] * vigour as f32;
        let at = deck(spot);
        // Rolling balls of fire over each spot, born hot in the opening and
        // cooling through orange into the soot of the column as they climb: the
        // blaze's body, which the licks only fringe.
        for ball in 0..FIRE_BALLS {
            let cycle = age / FLAME_CYCLE
                + j as f64 / FLAME_PUFFS as f64
                + ball as f64 / FIRE_BALLS as f64
                + burner.seed;
            let a = cycle.fract();
            let lap = cycle.floor();
            let jitter = ((lap + j as f64 + ball as f64 * 0.5) * 1.7).sin() * 0.18;
            let heat = fire * (1.05 - a * 0.95);
            smoke_puff(
                puffs,
                Vec3::new(
                    at.x + (WIND[0] * a * 0.6 + jitter) as f32,
                    at.y + (0.2 + 1.7 * a) as f32 * size,
                    at.z + (WIND[1] * a * 0.6 - jitter) as f32,
                ),
                (0.7 + 0.35 * a) as f32 * size,
                soot,
                (fire * (a / 0.1).min(1.0) * (1.0 - a * a) * 0.92) as f32,
                (burner.seed * 5.3 + lap * 0.61 + j as f64 * 0.29 + ball as f64 * 0.47) as f32,
                heat as f32,
                (smoothstep(a, 0.3, 1.0) * 0.75) as f32,
                0.0,
                0.0,
            );
        }
        // Licks of flame over each burning spot, each its own height and sway,
        // leaning on the wind.
        let [r, g, b] = FLAME_COLOR;
        for (k, [ox, oz]) in LICK_SPREAD.into_iter().enumerate() {
            let kf = k as f32;
            let phase = burner.seed as f32 * 31.0 + j as f32 * 4.7 + kf * 2.3;
            let breathe = (time as f32 * (5.0 + kf * 1.7) + phase).sin();
            // Spread over the hull's own deck, so a fire fills the opening it
            // burns out of rather than standing in a row.
            let spread = heading * Vec3::new(ox, 0.0, oz) * size;
            let lean = up + side * ((kf - 1.0) * 0.2 + 0.1 * breathe);
            flame(
                &mut sparks.records,
                at + spread + Vec3::Y * 0.05,
                lean.normalize(),
                (1.5 - 0.2 * (kf - 1.0).abs()) * size,
                ((0.85 + 0.2 * breathe)
                    * (1.0 - 0.25 * (kf - 1.0).abs())
                    * (0.55 + 0.45 * fire as f32))
                    * size,
                (burner.seed as f32 * 3.7 + j as f32 * 0.31 + kf * 0.17).fract(),
                [r, g, b, (fire * (0.5 + 0.15 * flicker)) as f32],
            );
        }
        // A soft heat glow behind the tongues gives the fire body, so it reads
        // as a blaze rather than a row of cut-out licks.
        glow(
            &mut sparks.records,
            at + Vec3::Y * (0.55 * size),
            FLAME_GLOW * size,
            [1.0, 0.42, 0.09, (fire * 0.26 * flicker) as f32],
        );
    }
    // Fuel spilled round the hull burns low on the ground beside it.
    let [r, g, b] = FLAME_COLOR;
    for (j, [across, along, size]) in GROUND_FIRES.into_iter().enumerate() {
        let jf = j as f32;
        let at = base + heading * Vec3::new(across, 0.0, along);
        let breathe = (time as f32 * 6.1 + burner.seed as f32 * 17.0 + jf * 2.9).sin();
        let size = size * vigour as f32;
        flame(
            &mut sparks.records,
            Vec3::new(at.x, 0.04, at.z),
            (up + side * 0.12 * breathe).normalize(),
            1.5 * size,
            (0.95 + 0.2 * breathe) * size * (0.5 + 0.5 * fire as f32),
            (burner.seed as f32 * 5.9 + jf * 0.37).fract(),
            [r, g, b, (fire * 0.85) as f32],
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
    let [r, g, b] = GLOW_COLOR;
    ground_glow(
        &mut sparks.records,
        Vec3::new(base.x, 0.1, base.z),
        size,
        [r, g, b, (burner.fire * 0.45) as f32],
    );
}

/// Fire and smoke streaming behind a tossed piece, burning out as it flies.
fn trail(
    burner: &Burner,
    points: &[Vec3; TRAIL_PUFFS],
    count: usize,
    puffs: &mut PoolBuffer,
    soot: [f32; 3],
) {
    if count == 0 || burner.age >= TRAIL_SECONDS {
        return;
    }
    let fade = 1.0 - smoothstep(burner.age, TRAIL_SECONDS * 0.55, TRAIL_SECONDS);
    let hot = 1.0 - smoothstep(burner.age, 0.2, 0.8);
    for (i, position) in points[..count].iter().enumerate() {
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
        let mut sparks = Sparks::default();
        let mut random = CosmeticRandom::seeded(5);
        puffs.clear();
        fires.update(state, 1.0 / 60.0, puffs, &mut sparks, &mut random);
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
        for _ in 0..80 {
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
    fn a_thrown_hull_trails_smoke_and_catches_fire_once_at_rest() {
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
        assert_eq!(burner.hull_trail_count, TRAIL_PUFFS);
        // In flight: trails only, no fire and no standing column.
        assert_eq!(burner.fire, 0.0);
        assert!(puffs.len() <= 2 * TRAIL_PUFFS);
        for _ in 0..60 {
            step(&mut fires, &state, &mut puffs);
        }
        let burner = &fires.burners[0];
        assert!(burner.fire > 0.3, "the hull at rest burns");
        assert!(puffs.len() >= SMOKE_PUFFS, "and smokes");
    }
}
