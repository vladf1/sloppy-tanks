//! Burning wrecks: after a tank's death blast, flames roar up out of the openings
//! of the hull (`wreck_vents`: the open turret ring, the engine deck's grilles,
//! the hatches a burnout blew open, a Humvee's engine bay and roof ring), spitting
//! embers and lighting the ground and everything near it, and a thick black column
//! of smoke rises off the main blaze and leans away on the wind, thinning to a
//! smoulder until the wreck is cleared. The turret tossed by the blast trails fire
//! and smoke as it flies. Each wreck follows its hull fragment as it is thrown:
//! while the hull tumbles it only trails smoke, and the fire takes hold once it
//! has come to rest. Analytic streams of puffs cycling through the shared puff
//! pool plus short trail histories per wreck; bounded wreck slots.

use glam::{DVec2, DVec3, Mat4, Quat, Vec3};
use sloppy_core::geometry::math::smoothstep;
use sloppy_core::models::{WreckVent, wreck_vents};
use sloppy_core::sim::data::vehicle;
use sloppy_core::sim::debris_cleanup::{DEBRIS_CLEANUP_SECONDS, WRECK_FIRE_DIE_DOWN_SECONDS};
use sloppy_core::sim::render_state::RenderFragment;
use sloppy_core::sim::{DeathStyle, RenderState, SimEvent, SimEventType, VehicleKind, WreckPart};

use super::Flash;
use super::ease::EaseTable;
use super::explosions::{WIND, smoke_puff};
use super::pool::{PoolBuffer, record};
use super::random::CosmeticRandom;
use super::sparks::{EMBERS, Sparks, ground_glow};
use crate::color::hex_to_linear;

pub const MAX_BURNING_WRECKS: usize = 12;
const SMOKE_PUFFS: usize = 12;
const TRAIL_PUFFS: usize = 6;
/// The most openings a wreck burns out of (`wreck_vents`).
const MAX_VENTS: usize = 3;
pub const FLAME_CAPACITY: usize = MAX_BURNING_WRECKS * MAX_VENTS;
/// The smoke column and the turret and tumbling hull trails.
pub const PUFFS_PER_WRECK: usize = SMOKE_PUFFS + 2 * TRAIL_PUFFS;
/// Flames burn this long; smoke keeps rising for longer.
const BURN_SECONDS: f64 = 12.0;
const SMOULDER_SECONDS: f64 = 24.0;
/// A puff's trip up the column (s).
const SMOKE_CYCLE: f64 = 3.4;
/// The smoke column's height over a puff's trip (m), and how much faster than
/// its puffs the column first climbs off a fire that has just caught.
const COLUMN_HEIGHT: f64 = 8.5;
const COLUMN_GROWTH: f64 = 1.6;
/// A wreck whose hull piece is not found burns out of its middle.
static UNFOUND_VENTS: [WreckVent; 1] = [WreckVent {
    center: DVec3::new(0.0, 0.4, 0.0),
    half_size: DVec2::new(0.6, 0.6),
    depth: 0.0,
    main: true,
}];
/// Below this up-axis height the hull lies on its roof. A covered turret well
/// can still smoke, but cannot show a flame through the underside.
const UPTURNED: f32 = -0.3;
/// Flames stand this deep in their opening (m), below its floor when it has one
/// (the open turret ring's well, whose rim then hides their root along its
/// curve): just enough that no gap shows under them, while their ragged root
/// still rises clear of a flat deck, which would cut it along a straight line.
const FLAME_SINK: f32 = 0.04;
/// The baked plume (generate-wreck-fire.mjs). Its full width includes room
/// for eddies to roll outward above the lip. The aperture clips it below.
const PLUME_DIAMETERS: f32 = 2.0;
const MAIN_FLAME_HEIGHT: f32 = 2.30;
const VENT_FLAME_HEIGHT: f32 = 1.45;
const VENT_FIRE: f64 = 0.8;
const PLUME_FRAMES: f64 = 144.0;
const PLUME_FPS: f64 = 24.0;
/// A death's wreck pieces are looked up this far from it, this soon after it.
const ANCHOR_RADIUS: f64 = 7.0;
const ANCHOR_SECONDS: f64 = 1.5;
/// The turret's trail: samples of its flight this far apart (s), drawn while
/// it is this young.
const TRAIL_SAMPLE: f64 = 0.03;
const TRAIL_SECONDS: f64 = 2.6;
/// The hull counts as at rest below this speed (m/s), and the fire takes hold
/// and dies back at these rates (1/s) as it settles or is thrown again. Once it
/// has caught, only a throw blows it out: a hull rising or falling faster than
/// this (m/s), not one shoved along the ground.
const REST_SPEED: f32 = 1.2;
const THROWN_CLIMB: f32 = 2.0;
const CATCH_RATE: f64 = 3.0;
const DOUSE_RATE: f64 = 5.0;
/// A fire whose hull piece was never found takes hold this late (s).
const UNFOUND_CATCH_SECONDS: f64 = 1.2;
/// Seconds between intermittent releases of embers at full burn.
const EMBER_INTERVAL: [f64; 2] = [0.22, 1.05];
/// The fire's glow on the ground (m across) and its light. Added light turns
/// sunlit grass lime, so the glow stays small and faint; the light warms the rest.
const GLOW_SIZE: f32 = 4.5;
const GLOW_COLOR: [f32; 3] = [0.9, 0.34, 0.07];
const FIRE_LIGHT: f64 = 13.0;
const FIRE_LIGHT_COLOR: u32 = 0xff9a55;
/// The light's (and the ground glow's) share of the fire, steady whatever the
/// flames' flicker. Swinging with it (±20%) breathed over a wall facing the fire like
/// a failing fluorescent tube, and wrecks burning alike kept trading the few light
/// slots, which switched a whole pool of light off and on; the flames keep it.
const FIRE_LIGHT_SHARE: f64 = 0.8;
/// The firelight hangs this high over the hull (m), so the hull it burns on is
/// lit warm from above rather than painted orange.
const FIRE_LIGHT_HEIGHT: f32 = 3.2;
/// A cleared wreck's fire and smoke die away this fast (1/s).
const CLEAR_RATE: f64 = 0.9;
/// A hull's fire dies down at most this fast (1/s) when its cleanup comes early
/// (the debris budget fading it): out within the first moments of its sink.
const DIE_DOWN_RATE: f64 = 4.0;
/// The smoke left once the fire has died down, thinning on until the hull is gone.
const DIED_DOWN_SMOKE: f64 = 0.45;
/// Oily black smoke at the fire, paling to grey as it climbs and spreads.
const SOOT: u32 = 0x3b3734;
const SOOT_HIGH: u32 = 0x8c8a88;
/// A burnout's fire against a cook-off's: the hull burns from the inside, out
/// of every hatch, so it reads as dead from across the field.
const BURNOUT_FIRE: f64 = 0.85;
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
    /// 1 while the hull has time left; falls to 0 as its cleanup nears, so the
    /// fire is out before the hull sinks away.
    fuel: f64,
    /// The hull and turret wreck pieces this fire follows, once found.
    hull: Option<u32>,
    turret: Option<u32>,
    /// The openings the hull burns out of, and its vehicle's scale.
    vents: &'static [WreckVent],
    scale: f32,
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
    /// The fire has taken hold, and burns on until the hull is thrown again.
    caught: bool,
    /// Seconds the smoke column has been climbing off the resting hull: it
    /// grows up from the fire rather than appearing whole.
    column: f64,
    ember_delay: f64,
    /// This frame's fire strength, for its light.
    fire: f64,
    /// Each smoke puff's swirl changes only when it starts another lap.
    swirls: [(f64, f64); SMOKE_PUFFS],
}

/// A lap's value, worked out when the lap starts.
fn per_lap(slot: &mut (f64, f64), lap: f64, value: impl FnOnce() -> f64) -> f64 {
    if slot.0 != lap {
        *slot = (lap, value());
    }
    slot.1
}

/// Build the smoke column's ease tables now rather than on the frame the first wreck
/// catches fire (2,050 `powf`s: 0.2 ms on a desktop, 0.75 ms at a quarter the speed),
/// and measure the wrecks' openings.
pub fn warm_tables() {
    column_eases();
    wreck_vents(VehicleKind::Scout, WreckPart::Hull);
}

/// The smoke column's climb and widening powers, tabulated.
fn column_eases() -> &'static [EaseTable; 2] {
    static TABLES: std::sync::OnceLock<[EaseTable; 2]> = std::sync::OnceLock::new();
    TABLES.get_or_init(|| [COLUMN_CLIMB, COLUMN_WIDENING].map(EaseTable::new))
}

/// How a smoke puff's climb slows and its size grows over its trip (powers of the
/// share of the trip).
const COLUMN_CLIMB: f64 = 0.94;
const COLUMN_WIDENING: f64 = 0.85;

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
            fuel: 1.0,
            hull: None,
            turret: None,
            vents: &UNFOUND_VENTS,
            scale: 1.0,
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            trail: [Vec3::ZERO; TRAIL_PUFFS],
            trail_count: 0,
            trail_clock: 0.0,
            hull_trail: [Vec3::ZERO; TRAIL_PUFFS],
            hull_trail_count: 0,
            column: 0.0,
            hull_trail_clock: 0.0,
            settled: 0.0,
            caught: false,
            ember_delay: 0.0,
            fire: 0.0,
            swirls: [(f64::NAN, 0.0); SMOKE_PUFFS],
        }
    }
}

#[derive(Clone, Debug)]
pub struct WreckFires {
    pub flames: PoolBuffer,
    burners: [Burner; MAX_BURNING_WRECKS],
    cursor: usize,
}

impl Default for WreckFires {
    fn default() -> Self {
        Self {
            burners: [Burner::default(); MAX_BURNING_WRECKS],
            cursor: 0,
            flames: PoolBuffer::new(FLAME_CAPACITY),
        }
    }
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
        self.flames.clear();
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
                    point: false,
                    blocker: None,
                },
            );
        }
    }

    /// Rebuild the visible plumes, smoke, embers and ground glow.
    pub fn update(
        &mut self,
        state: &RenderState,
        dt: f64,
        puffs: &mut PoolBuffer,
        sparks: &mut Sparks,
        random: &mut CosmeticRandom,
    ) {
        self.flames.clear();
        for burner in &mut self.burners {
            if !burner.active {
                continue;
            }
            burner.age += dt.max(0.0);
            // A fire burns as long as its hull lies there, and dies with it
            // rather than burning on over empty ground; a fire whose hull was
            // never found goes out when its tank respawns.
            let hull = burner
                .hull
                .map(|hull| state.fragments.iter().find(|fragment| fragment.id == hull));
            let standing = match hull {
                Some(hull) => hull.is_some(),
                None => burner
                    .id
                    .is_none_or(|id| state.tanks.iter().any(|tank| tank.id == id && !tank.alive)),
            };
            if let Some(Some(hull)) = hull {
                let ahead = hull.life - DEBRIS_CLEANUP_SECONDS;
                let fuel = smoothstep(ahead, 0.0, WRECK_FIRE_DIE_DOWN_SECONDS);
                burner.fuel = fuel.max(burner.fuel - dt * DIE_DOWN_RATE);
            }
            if !standing {
                burner.cleared = (burner.cleared + dt * CLEAR_RATE).min(1.0);
            }
            if burner.age >= SMOULDER_SECONDS || burner.cleared >= 1.0 {
                burner.active = false;
                continue;
            }
            follow(burner, state, dt);
            draw(
                burner,
                state.elapsed,
                dt,
                puffs,
                &mut self.flames,
                sparks,
                random,
            );
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
        // The fire burns out of the hull's own openings once the hull is found.
        if burner.hull.is_none()
            && let Some(hull) = nearest[0].1
            && let Some(piece) = state.fragments.iter().find(|fragment| fragment.id == hull)
            && let (Some(kind), Some(part)) = (piece.wreck, piece.part)
        {
            let vents = wreck_vents(kind, part);
            if !vents.is_empty() {
                burner.vents = &vents[..vents.len().min(MAX_VENTS)];
                burner.scale = vehicle(kind).scale as f32;
            }
        }
        burner.hull = burner.hull.or(nearest[0].1);
        burner.turret = burner.turret.or(nearest[1].1);
    }
    let find = |id: Option<u32>| {
        id.and_then(|id| state.fragments.iter().find(|fragment| fragment.id == id))
    };
    let mut velocity = Vec3::ZERO;
    if let Some(hull) = find(burner.hull) {
        let (position, rotation) = fragment_pose(hull);
        if dt > 0.0 {
            velocity = (position - burner.position) / dt as f32;
        }
        let speed = velocity.length();
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
    // Fire takes hold on a hull at rest and is blown out when it is thrown again;
    // a burning hull shoved along the ground burns on.
    let resting = match burner.hull {
        Some(_) if burner.caught => velocity.y.abs() < THROWN_CLIMB,
        Some(_) => velocity.length() < REST_SPEED,
        None => burner.age >= UNFOUND_CATCH_SECONDS,
    };
    burner.settled = if resting {
        (burner.settled + dt * CATCH_RATE).min(1.0)
    } else {
        (burner.settled - dt * DOUSE_RATE).max(0.0)
    };
    if burner.settled >= 1.0 {
        burner.caught = true;
    } else if burner.settled <= 0.0 {
        burner.caught = false;
    }
    burner.column += dt.max(0.0) * burner.settled * COLUMN_GROWTH;
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
    flames: &mut PoolBuffer,
    sparks: &mut Sparks,
    random: &mut CosmeticRandom,
) {
    let soot = hex_to_linear(SOOT);
    let soot_high = hex_to_linear(SOOT_HIGH);
    let age = burner.age;
    let left = 1.0 - burner.cleared;
    let settled = smoothstep(burner.settled, 0.0, 1.0);
    let normal = burner.rotation * Vec3::Y;
    let upturned = normal.y < UPTURNED;
    let ring = burner.vents[0].depth > 0.0;
    let fire = if ring && upturned {
        0.0
    } else {
        settled
            * (1.0 - smoothstep(age, BURN_SECONDS * 0.55, BURN_SECONDS))
            * left
            * burner.fuel
            * if burner.burnout { BURNOUT_FIRE } else { 1.0 }
    };
    let flicker = 0.8
        + 0.2 * (time * 13.0 + burner.seed * 40.0).sin() * (time * 7.3 + burner.seed * 9.0).sin();
    burner.fire = fire * FIRE_LIGHT_SHARE;
    // The column stands over a wreck at rest; a flying hull trails its smoke.
    let smoke = settled
        * (1.0 - smoothstep(age, SMOULDER_SECONDS * 0.55, SMOULDER_SECONDS))
        * left
        * (DIED_DOWN_SMOKE + (1.0 - DIED_DOWN_SMOKE) * burner.fuel)
        * (0.55 + 0.45 * (1.0 - smoothstep(age, BURN_SECONDS * 0.6, BURN_SECONDS)));
    let base = burner.position;
    // The turret well follows the hull even when it rolls onto its roof.
    let (rotation, scale) = (burner.rotation, burner.scale);
    let opening = |vent: &WreckVent| {
        let mut local = vent.center.as_vec3();
        if upturned && !ring {
            local.y = -local.y;
        }
        base + rotation * (local * scale)
    };
    let top = opening(&burner.vents[0]);
    // Each wreck burns its own way: how big its fires are and how its column
    // leans and billows.
    let vigour = 0.85 + 0.35 * burner.seed;
    let [climb, widening] = column_eases();
    let seed = burner.seed;
    for j in 0..SMOKE_PUFFS {
        let cycle = age / SMOKE_CYCLE + j as f64 / SMOKE_PUFFS as f64 + seed;
        let a = cycle.fract();
        let lap = cycle.floor();
        let swirl = per_lap(&mut burner.swirls[j], lap, || {
            (lap * 2.3 + j as f64 + seed * 7.0).sin()
        }) * (0.25 + 0.4 * a);
        let rise = climb.at(a);
        // The wind shears the column more the higher it climbs.
        let lean = a * a * SMOKE_CYCLE * 0.9;
        let position = Vec3::new(
            top.x + (WIND[0] * lean + swirl) as f32,
            top.y + (0.6 + COLUMN_HEIGHT * rise) as f32,
            top.z + (WIND[1] * lean - swirl * 0.6) as f32,
        );
        let pale = smoothstep(a, 0.05, 0.9) as f32;
        let color = [0, 1, 2].map(|i| soot[i] + (soot_high[i] - soot[i]) * pale);
        let size = (1.0 + 2.9 * widening.at(a)) * vigour;
        // Born quickly at the fire, densest low, thinning out as it spreads; a
        // puff shows only once the column has climbed to it.
        let grown = smoothstep(burner.column - a * SMOKE_CYCLE, 0.0, 0.6);
        let opacity =
            smoke * grown * smoothstep(a, 0.0, 0.08) * (1.0 - smoothstep(a, 0.35, 1.0)) * 0.93;
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
            (fire * (1.0 - a).powi(8) * 0.32) as f32,
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
    for (j, vent) in burner.vents.iter().enumerate() {
        let radius = vent.half_size.min_element() as f32 * scale;
        let well_depth = vent.depth as f32 * scale + FLAME_SINK;
        let axis = if ring { normal } else { Vec3::Y };
        let floor = opening(vent) - axis * well_depth;
        let level = (fire * if vent.main { 1.0 } else { VENT_FIRE }) as f32;
        let reach = (2.0 * radius).sqrt()
            * vigour as f32
            * if vent.main {
                MAIN_FLAME_HEIGHT
            } else {
                VENT_FLAME_HEIGHT
            };
        let height = well_depth + reach * (0.18 + 0.82 * level);
        let width = 2.0 * radius * PLUME_DIAMETERS;
        let world = Mat4::from_scale_rotation_translation(
            Vec3::new(width, height, 1.0),
            Quat::IDENTITY,
            floor,
        );
        let phase = (seed + 0.38197 * j as f64).fract();
        let frame = age * PLUME_FPS + phase * PLUME_FRAMES;
        let mouth = axis * well_depth;
        // The vertex hook keeps the base along the well's axis. The surface
        // hook masks it to the aperture until the rising gas clears the lip.
        flames.push(record(
            world,
            [
                0.90 * radius / width,
                well_depth / height,
                phase as f32,
                level,
            ],
            [mouth.x, mouth.y, mouth.z, frame as f32],
        ));
    }
    burner.ember_delay -= dt * fire;
    if burner.ember_delay <= 0.0 {
        burner.ember_delay =
            EMBER_INTERVAL[0] + random.next_f64() * (EMBER_INTERVAL[1] - EMBER_INTERVAL[0]);
        let vents = burner.vents;
        let vent = &vents[(random.next_f64() * vents.len() as f64) as usize % vents.len()];
        let mut embers = EMBERS;
        embers.count = 1 + (random.next_f64().powi(2) * 5.0) as usize;
        sparks.burst(opening(vent), Vec3::Y, 0.7, &embers, random);
    }
    let size = GLOW_SIZE * (0.9 + 0.1 * flicker as f32);
    let [r, g, b] = GLOW_COLOR;
    ground_glow(
        &mut sparks.records,
        Vec3::new(top.x, 0.1, top.z),
        size,
        [r, g, b, (burner.fire * 0.28) as f32],
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
        // Long enough for the smoke column to have climbed off the fire.
        for _ in 0..300 {
            step(&mut fires, &state, &mut puffs);
        }
        assert_eq!(fires.active(), 1);
        assert!(
            !puffs.is_empty() && !fires.flames.is_empty(),
            "smoke and flames"
        );
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
            life: 10.0,
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
        // Shoved along the ground by a tank, it burns on.
        for _ in 0..60 {
            state.fragments[0].position.z += 4.0 / 60.0;
            step(&mut fires, &state, &mut puffs);
        }
        assert!(fires.burners[0].fire > 0.3, "a pushed wreck keeps burning");
        // Thrown into the air again, the fire is blown out.
        for _ in 0..30 {
            state.fragments[0].position.y += 5.0 / 60.0;
            step(&mut fires, &state, &mut puffs);
        }
        assert_eq!(fires.burners[0].fire, 0.0, "a thrown wreck's fire goes out");
    }

    /// A wreck at rest with its hull piece, burning in full.
    fn burning_hull(life: f64) -> (WreckFires, RenderState, PoolBuffer) {
        let mut state = RenderState::default();
        let mut death = SimEvent::at(SimEventType::Death, 0.0, 0.0);
        death.id = Some(3);
        let mut fires = WreckFires::default();
        fires.event(&death, &mut CosmeticRandom::seeded(1));
        state.fragments.push(RenderFragment {
            id: 40,
            wreck: Some(sloppy_core::sim::VehicleKind::default()),
            part: Some(WreckPart::Hull),
            created_at: Some(0.0),
            life,
            position: Point3::new(0.0, 0.6, 0.0),
            ..RenderFragment::default()
        });
        let mut puffs = PoolBuffer::new(64);
        for _ in 0..60 {
            step(&mut fires, &state, &mut puffs);
        }
        assert!(fires.burners[0].fire > 0.3, "the hull at rest burns");
        (fires, state, puffs)
    }

    #[test]
    fn plume_starts_in_the_well_and_fits_through_the_tilted_turret_ring() {
        let (mut fires, _, mut puffs) = burning_hull(8.0);
        let burner = &mut fires.burners[0];
        burner.rotation = Quat::from_rotation_y(1.1) * Quat::from_rotation_z(0.45);
        let mut sparks = Sparks::default();
        fires.flames.clear();
        draw(
            burner,
            2.0,
            0.0,
            &mut puffs,
            &mut fires.flames,
            &mut sparks,
            &mut CosmeticRandom::seeded(1),
        );
        let [plume] = fires.flames.records() else {
            panic!("one plume through the turret opening")
        };
        let vent = burner.vents[0];
        let axis = Vec3::from_slice(&plume.data[..3]).normalize();
        let lip = plume.translation() + axis * plume.world_rows[1][1] * plume.tint[1];
        let expected = burner.position + burner.rotation * (vent.center.as_vec3() * burner.scale);
        assert!(lip.distance(expected) < 1e-4);
        assert!(plume.tint[0] * plume.world_rows[0][0] < vent.half_size.x as f32 * burner.scale);
        assert!(plume.translation().distance(lip) > vent.depth as f32 * burner.scale);
    }

    #[test]
    fn a_hulls_fire_dies_down_before_it_sinks_away() {
        let (mut fires, mut state, mut puffs) = burning_hull(4.0);
        let mut last = f64::MAX;
        while state.fragments[0].life > DEBRIS_CLEANUP_SECONDS {
            state.fragments[0].life -= 1.0 / 60.0;
            step(&mut fires, &state, &mut puffs);
            let fire = fires.burners[0].fuel;
            assert!(fire <= last, "the fire only dies down");
            last = fire;
        }
        // The hull is about to sink: its flames are out, and smoke still rises.
        assert!(fires.burners[0].fire < 1e-3, "{}", fires.burners[0].fire);
        assert!(!puffs.is_empty(), "the fire smoulders on");
        // A hull the debris budget fades early still puts its fire out quickly.
        let (mut fires, mut state, mut puffs) = burning_hull(2.8);
        state.fragments[0].life = DEBRIS_CLEANUP_SECONDS;
        for _ in 0..20 {
            state.fragments[0].life -= 1.0 / 60.0;
            step(&mut fires, &state, &mut puffs);
        }
        assert_eq!(fires.burners[0].fire, 0.0);
    }
}
