//! Pooled, cosmetic dust behind moving tracks (`track-dust.ts`): two triangles
//! per puff, no textures, lights or physics bodies. It advances on simulation
//! time only, so it freezes while paused and between rounds.

use glam::{Mat4, Vec3};
use sloppy_core::sim::data::{ARENA, vehicle};
use sloppy_core::sim::maps::GroundKind;
use sloppy_core::sim::math::angle_delta;
use sloppy_core::sim::{MatchPhase, RenderState, VehicleKind};

use super::pool::{PoolBuffer, record};
use super::random::CosmeticRandom;
use super::track_gravel::TrackGravel;
use crate::color::hex_to_linear;

pub const TRACK_DUST_CAPACITY: usize = 384;
/// A suspended tab or respawn must not generate a long catch-up trail.
const MAX_STEP: f64 = 0.1;
/// Contact speeds (m/s) below this raise no dust.
const MIN_CONTACT_SPEED: f64 = 1.5;
const TELEPORT_DISTANCE: f64 = 5.0;
const TELEPORT_TURN: f64 = 0.8;
const AIRBORNE_HEIGHT: f64 = 1.25;
const GRAVEL_COOLDOWN: f64 = 0.12;

// `village-roads.ts`: the dirt roads' footprints and grass-blended shoulder.
const ROAD_SHOULDER: f64 = 0.7;
struct Road {
    x: f64,
    z: f64,
    w: f64,
    d: f64,
}
const VILLAGE_ROADS: [Road; 6] = [
    Road {
        x: -52.0,
        z: 0.0,
        w: 10.0,
        d: ARENA * 2.0 - 2.0,
    },
    Road {
        x: 0.0,
        z: 0.0,
        w: 18.0,
        d: ARENA * 2.0 - 2.0,
    },
    Road {
        x: 52.0,
        z: 0.0,
        w: 10.0,
        d: ARENA * 2.0 - 2.0,
    },
    Road {
        x: 0.0,
        z: -38.0,
        w: ARENA * 2.0 - 2.0,
        d: 8.0,
    },
    Road {
        x: 0.0,
        z: 0.0,
        w: ARENA * 2.0 - 2.0,
        d: 12.0,
    },
    Road {
        x: 0.0,
        z: 38.0,
        w: ARENA * 2.0 - 2.0,
        d: 8.0,
    },
];

/// Dust starts on the solid dirt, leaving the grass-blended shoulders quiet.
pub fn is_village_dirt(x: f64, z: f64) -> bool {
    VILLAGE_ROADS.iter().any(|road| {
        (x - road.x).abs() <= road.w / 2.0 - ROAD_SHOULDER
            && (z - road.z).abs() <= road.d / 2.0 - ROAD_SHOULDER
    })
}

#[derive(Clone, Copy, Debug, Default)]
struct Puff {
    x: f64,
    y: f64,
    z: f64,
    vx: f64,
    vz: f64,
    life: f64,
    max: f64,
    size: f64,
}

#[derive(Clone, Copy, Debug)]
struct Pose {
    x: f64,
    z: f64,
    heading: f64,
    pending: f64,
    gravel_cooldown: f64,
}

#[derive(Clone, Debug)]
pub struct TrackDust {
    pub records: PoolBuffer,
    pub gravel: TrackGravel,
    puffs: Vec<Puff>,
    /// Each live tank's last pose, by tank id: a few dozen tanks are found faster
    /// by scanning than by hashing their ids every frame.
    poses: Vec<(u32, Pose)>,
    previous_time: Option<f64>,
    /// Linear dust color of the current map.
    color: [f32; 3],
}

impl Default for TrackDust {
    fn default() -> Self {
        Self {
            records: PoolBuffer::new(TRACK_DUST_CAPACITY),
            gravel: TrackGravel::default(),
            puffs: Vec::with_capacity(TRACK_DUST_CAPACITY),
            poses: Vec::new(),
            previous_time: None,
            color: hex_to_linear(0xe1caa2),
        }
    }
}

impl TrackDust {
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn reset(&mut self) {
        self.gravel.reset();
        self.puffs.clear();
        self.poses.clear();
        self.previous_time = None;
        self.records.clear();
    }

    pub fn update(&mut self, state: &RenderState, random: &mut CosmeticRandom) {
        let elapsed = state.elapsed - self.previous_time.unwrap_or(state.elapsed);
        self.previous_time = Some(state.elapsed);
        if elapsed < 0.0 {
            self.reset();
            return;
        }
        if elapsed == 0.0 || state.match_state.phase != MatchPhase::Playing {
            return;
        }
        let dt = elapsed.min(MAX_STEP);
        self.gravel.update(elapsed);
        let theme = state.map_theme.as_str();
        let quarry = theme == "quarry";
        let harbor = theme == "harbor";
        let village = theme == "village";
        let grass_floor = state.map_floor == Some(GroundKind::DryGrass);
        self.color = hex_to_linear(if quarry {
            0xd9bc8b
        } else if harbor {
            0xaeb0ab
        } else {
            0xe1caa2
        });
        self.puffs.retain_mut(|puff| {
            puff.life -= elapsed;
            if puff.life <= 0.0 {
                return false;
            }
            puff.x += puff.vx * dt;
            puff.z += puff.vz * dt;
            puff.y += dt * 0.3;
            true
        });
        self.poses
            .retain(|(id, _)| state.tanks.iter().any(|tank| tank.id == *id && tank.alive));
        for tank in &state.tanks {
            if !tank.alive {
                continue;
            }
            let p = tank.position;
            let slot = match self.poses.iter().position(|(id, _)| *id == tank.id) {
                Some(slot) => slot,
                None => {
                    self.poses.push((
                        tank.id,
                        Pose {
                            x: p.x,
                            z: p.z,
                            heading: tank.heading,
                            pending: 0.0,
                            gravel_cooldown: 0.0,
                        },
                    ));
                    self.poses.len() - 1
                }
            };
            let previous = self.poses[slot].1;
            let mut next = previous;
            let distance = (p.x - previous.x).hypot(p.z - previous.z);
            let velocity = tank.velocity;
            let speed = velocity.x.hypot(velocity.z);
            let scale = vehicle(tank.kind).scale;
            let turn = angle_delta(previous.heading, tank.heading);
            let (sin, cos) = tank.heading.sin_cos();
            let forward = velocity.x * sin + velocity.z * cos;
            let lateral = (velocity.x * cos - velocity.z * sin).abs();
            let strength =
                (turn.abs() / dt / 2.4 + (lateral / (speed + 1.0)) * 0.6).clamp(0.0, 1.0);
            let half_length = if tank.kind == VehicleKind::Scout {
                2.1
            } else {
                2.6
            } * scale;
            // Use the faster of translation and belt travel during a pivot, not
            // both added: turns stir the same surface, they do not multiply dust.
            let contact_travel = distance.max(turn.abs() * scale);
            let spacing = if quarry {
                1.0
            } else if harbor {
                3.0
            } else {
                2.1
            } * scale;
            next.gravel_cooldown -= dt;
            if elapsed > MAX_STEP
                || distance > TELEPORT_DISTANCE
                || turn.abs() > TELEPORT_TURN
                || p.y > AIRBORNE_HEIGHT
                || contact_travel / dt < MIN_CONTACT_SPEED
            {
                next.pending = 0.0;
            } else if contact_travel > 1e-6 {
                let mut threw_gravel = false;
                let mut d = spacing - previous.pending;
                while d <= contact_travel {
                    let u = (d / contact_travel).clamp(0.0, 1.0);
                    let heading = previous.heading + turn * u;
                    let (sin, cos) = heading.sin_cos();
                    for side in [-1.0, 1.0] {
                        let track_speed = forward - ((side * turn) / dt) * scale;
                        let direction = if track_speed >= 0.0 { 1.0 } else { -1.0 };
                        let x = previous.x + (p.x - previous.x) * u - sin * half_length * direction
                            + cos * side * scale;
                        let z = previous.z + (p.z - previous.z) * u
                            - cos * half_length * direction
                            - sin * side * scale;
                        if grass_floor || (village && !is_village_dirt(x, z)) {
                            continue;
                        }
                        let spread = 0.45 + strength * 0.35;
                        let vx = -sin * direction * 0.4 + cos * side * spread;
                        let vz = -cos * direction * 0.4 - sin * side * spread;
                        if quarry
                            && next.gravel_cooldown <= 0.0
                            && (strength > 0.25 || speed > 10.0)
                        {
                            self.gravel.emit(x, z, vx * 1.5, vz * 1.5, strength, random);
                            threw_gravel = true;
                        }
                        if self.puffs.len() >= TRACK_DUST_CAPACITY {
                            break;
                        }
                        let life = (if quarry { 0.55 } else { 0.45 }) + random.next_f64() * 0.2;
                        let size = (0.8 + random.next_f64() * 0.2)
                            * scale
                            * if quarry { 1.15 } else { 1.0 }
                            * (1.0 + strength * 0.1);
                        self.puffs.push(Puff {
                            x,
                            y: 0.25,
                            z,
                            vx,
                            vz,
                            life,
                            max: life,
                            size,
                        });
                    }
                    if threw_gravel {
                        next.gravel_cooldown = GRAVEL_COOLDOWN;
                    }
                    d += spacing;
                }
                next.pending = (previous.pending + contact_travel) % spacing;
            }
            next.x = p.x;
            next.z = p.z;
            next.heading = tank.heading;
            self.poses[slot].1 = next;
        }
        let peak = if quarry {
            0.46
        } else if harbor {
            0.12
        } else {
            0.38
        };
        self.records.clear();
        let [r, g, b] = self.color;
        for puff in &self.puffs {
            let age = 1.0 - puff.life / puff.max;
            let size = (puff.size * (0.7 + age * 1.8)) as f32;
            let world =
                Mat4::from_translation(Vec3::new(puff.x as f32, puff.y as f32, puff.z as f32))
                    * Mat4::from_scale(Vec3::new(size, size * 0.65, 1.0));
            let opacity = (age * 12.0).min(1.0) * (1.0 - age) * peak;
            self.records
                .push(record(world, [r, g, b, opacity as f32], [0.0; 4]));
        }
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::effects::track_gravel::TRACK_GRAVEL_CAPACITY;
    use sloppy_core::sim::render_state::RenderTank;
    use sloppy_core::sim::{Point3, VehicleKind};

    struct Fixture {
        state: RenderState,
        dust: TrackDust,
        random: CosmeticRandom,
    }

    impl Fixture {
        fn new(count: u32, theme: &str, grass: bool) -> Self {
            let mut state = RenderState::default();
            state.map_theme = theme.into();
            if grass {
                state.map_floor = Some(GroundKind::DryGrass);
            }
            state.match_state.phase = MatchPhase::Playing;
            for i in 0..count {
                state.tanks.push(RenderTank {
                    id: i + 1,
                    kind: VehicleKind::Balanced,
                    alive: true,
                    position: Point3 {
                        x: f64::from(i) * 5.0,
                        y: 0.65,
                        z: 0.0,
                    },
                    ..RenderTank::default()
                });
            }
            let mut fixture = Self {
                state,
                dust: TrackDust::default(),
                random: CosmeticRandom::default(),
            };
            fixture.update();
            fixture.step(0.0);
            fixture
        }

        fn update(&mut self) {
            self.dust.update(&self.state, &mut self.random);
        }

        fn step_with(&mut self, speed: f64, dt: f64, height: f64) {
            self.state.elapsed += dt;
            for tank in &mut self.state.tanks {
                tank.position.y = height;
                tank.position.z += speed * dt;
                tank.velocity = Point3 {
                    x: 0.0,
                    y: 0.0,
                    z: speed,
                };
            }
            self.update();
        }

        fn step(&mut self, speed: f64) {
            self.step_with(speed, 1.0 / 60.0, 0.65);
        }

        fn place(&mut self, x: f64, z: f64) {
            let tank = &mut self.state.tanks[0];
            tank.position.x = x;
            tank.position.z = z;
            self.step(0.0);
        }

        fn origins(&self) -> Vec<Vec3> {
            self.dust
                .records
                .records()
                .iter()
                .map(|r| r.translation())
                .collect()
        }
    }

    #[test]
    fn dust_comes_from_both_trailing_tracks_in_forward_and_reverse() {
        for speed in [12.0, -12.0] {
            let mut f = Fixture::new(1, "village", false);
            for _ in 0..15 {
                f.step(speed);
            }
            assert!(f.dust.len() >= 2);
            let points = f.origins();
            assert!(points[0].x < 0.0 && points[1].x > 0.0);
            let z = f.state.tanks[0].position.z;
            for p in &points[..2] {
                assert!((f64::from(p.z) - z) * speed < 0.0);
            }
        }
    }

    #[test]
    fn dust_freezes_without_ticks_expires_after_stopping_and_clears_on_reset() {
        let mut f = Fixture::new(1, "village", false);
        for _ in 0..60 {
            f.step(12.0);
        }
        let count = f.dust.len();
        let records = f.dust.records.records().to_vec();
        for _ in 0..120 {
            f.update();
        }
        assert_eq!(f.dust.len(), count);
        assert_eq!(f.dust.records.records(), &records[..]);
        f.state.match_state.phase = MatchPhase::Paused;
        f.step(12.0);
        assert_eq!(
            f.dust.records.records(),
            &records[..],
            "paused dust freezes"
        );
        f.state.match_state.phase = MatchPhase::Playing;
        for _ in 0..70 {
            f.step(0.0);
        }
        assert_eq!(f.dust.len(), 0);
        for _ in 0..30 {
            f.step(12.0);
        }
        assert!(!f.dust.is_empty());
        f.dust.reset();
        assert_eq!(f.dust.len(), 0);
    }

    #[test]
    fn idle_airborne_dead_tanks_and_teleports_raise_no_dust() {
        let mut f = Fixture::new(1, "village", false);
        for _ in 0..60 {
            f.step(0.0);
        }
        assert_eq!(f.dust.len(), 0);
        for _ in 0..60 {
            f.step_with(12.0, 1.0 / 60.0, 2.0);
        }
        assert_eq!(f.dust.len(), 0);
        f.step(1000.0);
        assert_eq!(f.dust.len(), 0);
        f.state.tanks[0].alive = false;
        for _ in 0..30 {
            f.step(12.0);
        }
        assert_eq!(f.dust.len(), 0);
    }

    #[test]
    fn crowded_scenes_stay_within_the_pool_and_recover_after_saturation() {
        let mut f = Fixture::new(30, "quarry", false);
        let mut peak = 0;
        for _ in 0..120 {
            f.step(30.0);
            peak = peak.max(f.dust.len());
            assert!(f.dust.len() <= TRACK_DUST_CAPACITY);
        }
        assert_eq!(peak, TRACK_DUST_CAPACITY);
        for _ in 0..70 {
            f.step(0.0);
        }
        assert_eq!(f.dust.len(), 0);
        for _ in 0..60 {
            f.step(30.0);
        }
        assert!(!f.dust.is_empty());
    }

    #[test]
    fn village_grass_emits_no_dust_while_roads_and_other_maps_do() {
        for theme in ["village", "quarry", "harbor"] {
            let mut f = Fixture::new(1, theme, false);
            f.place(20.0, 12.0);
            for _ in 0..45 {
                f.step(12.0);
            }
            if theme == "village" {
                assert_eq!(f.dust.len(), 0, "grass stays clean");
                f.place(0.0, 12.0);
                for _ in 0..30 {
                    f.step(12.0);
                }
                assert!(!f.dust.is_empty(), "dirt road raises dust");
                f.place(20.0, 12.0);
                for _ in 0..45 {
                    f.step(12.0);
                }
                assert_eq!(f.dust.len(), 0, "dust fades after leaving the road");
            } else {
                assert!(!f.dust.is_empty(), "{theme} keeps its dust");
            }
        }
    }

    #[test]
    fn each_track_checks_its_own_surface_at_the_road_edge() {
        for speed in [12.0, -12.0] {
            let mut f = Fixture::new(1, "village", false);
            f.place(8.3, 20.0);
            for _ in 0..30 {
                f.step(speed);
            }
            assert!(!f.dust.is_empty());
            for p in f.origins() {
                assert!(p.x < 8.3, "only the track on dirt emits");
            }
        }
    }

    #[test]
    fn turns_on_grass_and_grass_floors_stay_clean() {
        for grass in [false, true] {
            let mut f = Fixture::new(1, "village", grass);
            if !grass {
                f.place(20.0, 20.0);
            }
            for _ in 0..60 {
                f.state.tanks[0].heading += 2.4 / 60.0;
                f.step(0.0);
            }
            assert_eq!(f.dust.len(), 0);
            assert_eq!(f.dust.gravel.len(), 0);
        }
    }

    #[test]
    fn gravel_is_quarry_only_bounded_frozen_on_pause_and_expires() {
        for theme in ["village", "quarry", "harbor"] {
            let mut f = Fixture::new(30, theme, false);
            let mut peak = 0;
            for _ in 0..120 {
                for tank in &mut f.state.tanks {
                    tank.heading += 2.4 / 60.0;
                }
                f.step(12.0);
                peak = peak.max(f.dust.gravel.len());
                assert!(f.dust.gravel.len() <= TRACK_GRAVEL_CAPACITY);
            }
            let expected = if theme == "quarry" {
                TRACK_GRAVEL_CAPACITY
            } else {
                0
            };
            assert_eq!(peak, expected, "{theme}");
            let records = f.dust.gravel.records.records().to_vec();
            for _ in 0..30 {
                f.update();
            }
            assert_eq!(f.dust.gravel.records.records(), &records[..]);
            for _ in 0..70 {
                f.step(0.0);
            }
            assert_eq!(f.dust.gravel.len(), 0);
            for _ in 0..30 {
                f.step(12.0);
            }
            f.dust.reset();
            assert_eq!(f.dust.gravel.len(), 0);
        }
    }

    #[test]
    fn village_dirt_matches_the_road_footprints() {
        assert!(is_village_dirt(0.0, 20.0));
        assert!(is_village_dirt(8.3, 20.0));
        assert!(!is_village_dirt(8.31, 20.0));
        assert!(!is_village_dirt(20.0, 12.0));
        assert!(is_village_dirt(20.0, 0.0));
        assert!(is_village_dirt(-52.0, 30.0));
    }
}
