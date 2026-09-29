//! Cosmetic, distance-spaced twin vehicle trails in one bounded draw
//! (`tracks.ts`). Each mark stores its birth time and strength in the record's
//! effect data; the shader fades it against the trail clock, so a mark is written
//! once and uploaded once.

use std::collections::HashMap;

use glam::{Mat4, Vec3};
use sloppy_core::sim::arena::spawn_positions;
use sloppy_core::sim::data::vehicle;
use sloppy_core::sim::math::angle_delta;
use sloppy_core::sim::{RenderState, Team, VehicleKind};

use super::pool::{PoolBuffer, record};

// Thirty boosted scouts can leave about 70,000 marks during the 24-second fade.
// Reserve that lifetime budget so busy scenes do not stop drawing new trails.
pub const TRACK_CAPACITY: usize = 81920;
pub const TRACK_LIFETIME: f64 = 24.0;
const SPACING: f64 = 0.42;
/// HMMWV wheels leave overlapping narrow lines.
const HUMVEE_SPACING: f64 = 0.16;
pub const HUMVEE_TRACK_STRENGTH: f32 = 0.18;
/// Planar travel (m), turn (rad) or height (m) beyond which a pose is a teleport
/// or a jump rather than driving.
const TELEPORT_DISTANCE: f64 = 5.0;
const TELEPORT_TURN: f64 = 0.8;
const AIRBORNE_HEIGHT: f64 = 1.25;
const MARK_HEIGHT: f32 = 0.075;
/// Quarry pads stand proud of the dirt, so prints on them ride on top.
const PAD_MARK_HEIGHT: f32 = 0.16;
const PAD_RADIUS: f64 = 2.75;

#[derive(Clone, Copy, Debug)]
struct Pose {
    x: f64,
    z: f64,
    heading: f64,
    pending: f64,
}

#[derive(Clone, Debug)]
pub struct TrackTrails {
    pub records: PoolBuffer,
    /// The trail clock (simulation seconds) the shader fades marks against.
    pub clock: f64,
    poses: HashMap<u32, Pose>,
    // Expiry order is a ring; render slots stay dense so the draw count excludes
    // dead marks. `slots[queue]` is a mark's slot, `queue_indices[slot]` its queue.
    oldest: usize,
    slots: Vec<u32>,
    queue_indices: Vec<u32>,
}

impl Default for TrackTrails {
    fn default() -> Self {
        Self {
            records: PoolBuffer::new(TRACK_CAPACITY),
            clock: 0.0,
            poses: HashMap::new(),
            oldest: 0,
            slots: vec![0; TRACK_CAPACITY],
            queue_indices: vec![0; TRACK_CAPACITY],
        }
    }
}

fn birth(records: &PoolBuffer, slot: usize) -> f64 {
    f64::from(records.records()[slot].data[0])
}

fn mark_height(state: &RenderState, x: f64, z: f64) -> f32 {
    if state.map_theme == "quarry" {
        for team in [Team::Blue, Team::Red] {
            for p in spawn_positions(team, state.map_scale) {
                if (x - p.x).hypot(z - p.z) < PAD_RADIUS {
                    return PAD_MARK_HEIGHT;
                }
            }
        }
    }
    MARK_HEIGHT
}

impl TrackTrails {
    pub fn reset(&mut self) {
        self.records.clear();
        self.oldest = 0;
        self.poses.clear();
        self.clock = 0.0;
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn update(&mut self, state: &RenderState, alpha: f64) {
        let elapsed = state.elapsed;
        self.clock = elapsed;
        // Retire only expired entries, not a scan of every live mark each frame.
        // Moving the last live slot into each hole keeps a single compact draw.
        while !self.records.is_empty() {
            let slot = self.slots[self.oldest] as usize;
            if elapsed - birth(&self.records, slot) < TRACK_LIFETIME {
                break;
            }
            let last = self.records.len() - 1;
            self.records.swap_remove(slot);
            if slot != last {
                let queue = self.queue_indices[last];
                self.queue_indices[slot] = queue;
                self.slots[queue as usize] = slot as u32;
            }
            self.oldest = (self.oldest + 1) % TRACK_CAPACITY;
        }
        for tank in &state.tanks {
            if !tank.alive {
                self.poses.remove(&tank.id);
                continue;
            }
            let position = tank.position;
            let x = tank.previous.x + (position.x - tank.previous.x) * alpha;
            let z = tank.previous.z + (position.z - tank.previous.z) * alpha;
            let Some(previous) = self.poses.get(&tank.id).copied() else {
                self.poses.insert(
                    tank.id,
                    Pose {
                        x,
                        z,
                        heading: tank.heading,
                        pending: 0.0,
                    },
                );
                continue;
            };
            let mut next = previous;
            let distance = (x - previous.x).hypot(z - previous.z);
            let turn = angle_delta(previous.heading, tank.heading);
            let scale = vehicle(tank.kind).scale;
            let humvee = tank.kind == VehicleKind::Humvee;
            // Tracked vehicles keep the separated tread-pad rhythm.
            let spacing = if humvee { HUMVEE_SPACING } else { SPACING } * scale;
            let length = distance + turn.abs() * 1.5 * scale;
            if distance > TELEPORT_DISTANCE
                || turn.abs() > TELEPORT_TURN
                || position.y > AIRBORNE_HEIGHT
            {
                next.pending = 0.0;
            } else if length > 1e-6 {
                // Subdivide travel so marks stay evenly spaced at any frame rate.
                let mut d = spacing - previous.pending;
                while d <= length {
                    let u = d / length;
                    let angle = previous.heading + turn * u;
                    let (sin, cos) = angle.sin_cos();
                    let cx = previous.x + (x - previous.x) * u - sin * 1.1 * scale;
                    let cz = previous.z + (z - previous.z) * u - cos * 1.1 * scale;
                    // Capacity pressure must not erase marks before they finish fading.
                    if !self.records.is_full() {
                        for side in [-1.0, 1.0] {
                            let mx = cx + cos * side * scale;
                            let mz = cz - sin * side * scale;
                            self.lay(state, mx, mz, angle, scale, humvee);
                        }
                    }
                    d += spacing;
                }
                next.pending = (previous.pending + length) % spacing;
            }
            next.x = x;
            next.z = z;
            next.heading = tank.heading;
            self.poses.insert(tank.id, next);
        }
    }

    fn lay(&mut self, state: &RenderState, x: f64, z: f64, angle: f64, scale: f64, humvee: bool) {
        let (width, length) = if humvee { (0.18, 0.3) } else { (0.48, 0.16) };
        let world = Mat4::from_translation(Vec3::new(x as f32, mark_height(state, x, z), z as f32))
            * Mat4::from_rotation_y(angle as f32)
            * Mat4::from_scale(Vec3::new(
                (width * scale) as f32,
                1.0,
                (length * scale) as f32,
            ));
        let strength = if humvee { HUMVEE_TRACK_STRENGTH } else { 1.0 };
        let slot = self.records.len();
        if self.records.push(record(
            world,
            [1.0; 4],
            [state.elapsed as f32, strength, 0.0, 0.0],
        )) {
            let queue = (self.oldest + slot) % TRACK_CAPACITY;
            self.slots[queue] = slot as u32;
            self.queue_indices[slot] = queue as u32;
        }
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use sloppy_core::sim::render_state::RenderTank;
    use sloppy_core::sim::{MatchPhase, Point3, Vec2};

    fn state(kind: VehicleKind) -> RenderState {
        let mut state = RenderState::default();
        state.map_theme = "village".into();
        state.match_state.phase = MatchPhase::Playing;
        state.tanks.push(RenderTank {
            id: 1,
            kind,
            alive: true,
            position: Point3 {
                x: 0.0,
                y: 0.65,
                z: 0.0,
            },
            ..RenderTank::default()
        });
        state
    }

    fn place(state: &mut RenderState, x: f64, z: f64) {
        let tank = &mut state.tanks[0];
        tank.position.x = x;
        tank.position.z = z;
    }

    /// Moves with `previous` equal to the new position (alpha 1), like the TS fixture.
    fn step(trails: &mut TrackTrails, state: &mut RenderState, x: f64) {
        place(state, x, 0.0);
        trails.update(state, 1.0);
    }

    fn origin(records: &PoolBuffer, slot: usize) -> Vec3 {
        let w = &records.records()[slot].world;
        Vec3::new(w[12], w[13], w[14])
    }

    #[test]
    fn quarry_prints_ride_on_top_of_spawn_pads() {
        for theme in ["quarry", "village"] {
            let mut s = state(VehicleKind::Balanced);
            s.map_theme = theme.into();
            s.tanks[0].heading = std::f64::consts::FRAC_PI_2;
            place(&mut s, -58.0, -23.0);
            let mut trails = TrackTrails::default();
            let dt = 1.0 / 60.0;
            for _ in 0..120 {
                if s.tanks[0].position.x >= -48.0 {
                    break;
                }
                s.elapsed += dt;
                let x = s.tanks[0].position.x + 8.0 * dt;
                place(&mut s, x, -23.0);
                trails.update(&s, 1.0);
            }
            let (mut pad, mut dirt) = (0, 0);
            for slot in 0..trails.len() {
                let p = origin(&trails.records, slot);
                let on_pad = f64::from(p.x + 53.0).hypot(f64::from(p.z + 23.0)) < PAD_RADIUS;
                let expected = if on_pad && theme == "quarry" {
                    pad += 1;
                    PAD_MARK_HEIGHT
                } else {
                    dirt += 1;
                    MARK_HEIGHT
                };
                assert!((p.y - expected).abs() < 1e-6, "{theme} print at {}", p.y);
            }
            assert!(dirt > 0);
            assert_eq!(pad > 0, theme == "quarry");
        }
    }

    #[test]
    fn tracks_are_distance_spaced_and_skip_stationary_tanks_and_teleports() {
        let mut counts = Vec::new();
        for fps in [30, 120] {
            let mut s = state(VehicleKind::Balanced);
            let mut trails = TrackTrails::default();
            for i in 0..=fps * 2 {
                let x = (i * 6) as f64 / fps as f64;
                s.tanks[0].previous = Vec2 { x, z: 0.0 };
                s.elapsed = i as f64 / fps as f64;
                step(&mut trails, &mut s, x);
            }
            let count = trails.len();
            counts.push(count);
            for _ in 0..10 {
                trails.update(&s, 1.0);
            }
            assert_eq!(trails.len(), count, "a stationary tank adds nothing");
            step(&mut trails, &mut s, 50.0);
            assert_eq!(trails.len(), count, "a teleport adds nothing");
        }
        assert_eq!(counts[0], counts[1]);
    }

    #[test]
    fn stationary_pivots_leave_curved_marks_without_filling_the_pool() {
        let mut s = state(VehicleKind::Balanced);
        let mut trails = TrackTrails::default();
        trails.update(&s, 1.0);
        for _ in 0..60 {
            s.tanks[0].heading += 2.4 / 60.0;
            s.elapsed += 1.0 / 60.0;
            trails.update(&s, 1.0);
        }
        let count = trails.len();
        assert!(count > 4);
        for _ in 0..60 {
            s.elapsed += 1.0 / 60.0;
            trails.update(&s, 1.0);
        }
        assert_eq!(trails.len(), count);
    }

    #[test]
    fn a_full_buffer_keeps_visible_marks_reuses_faded_ones_and_empties_on_reset() {
        let mut s = state(VehicleKind::Balanced);
        let mut trails = TrackTrails::default();
        let fill_steps = TRACK_CAPACITY / 2;
        for i in 0..fill_steps {
            step(&mut trails, &mut s, i as f64);
        }
        assert_eq!(trails.len(), TRACK_CAPACITY);
        let before = trails.records.records().to_vec();
        for i in fill_steps..fill_steps + 10 {
            step(&mut trails, &mut s, i as f64);
        }
        assert_eq!(trails.records.records(), &before[..]);
        s.elapsed = TRACK_LIFETIME + 0.1;
        step(&mut trails, &mut s, (fill_steps + 10) as f64);
        assert_ne!(trails.records.records(), &before[..]);
        trails.reset();
        assert_eq!(trails.len(), 0);
        trails.update(&s, 1.0);
        assert_eq!(
            trails.len(),
            0,
            "the first pose after a reset is only a reference"
        );
    }

    #[test]
    fn expiry_compacts_live_marks_and_uploads_only_changed_slots() {
        let mut s = state(VehicleKind::Balanced);
        let mut trails = TrackTrails::default();
        step(&mut trails, &mut s, 0.0);
        step(&mut trails, &mut s, 1.0);
        let old_count = trails.len();
        s.elapsed = 10.0;
        step(&mut trails, &mut s, 2.0);
        let live_count = trails.len() - old_count;
        let mut expected: Vec<_> = trails.records.records()[old_count..]
            .iter()
            .map(|r| r.world)
            .collect();
        trails.records.take_dirty(|_, _| {});
        s.elapsed = TRACK_LIFETIME - 0.001;
        trails.update(&s, 1.0);
        assert_eq!(
            trails.len(),
            old_count + live_count,
            "keep marks until completely faded"
        );
        s.elapsed = TRACK_LIFETIME;
        trails.update(&s, 1.0);
        assert_eq!(trails.len(), live_count);
        let mut actual: Vec<_> = trails.records.records().iter().map(|r| r.world).collect();
        for r in trails.records.records() {
            assert_eq!(r.data[0], 10.0, "moving a slot preserves its fade age");
        }
        let key = |w: &[f32; 16]| w.map(|v| v.to_bits());
        expected.sort_by_key(key);
        actual.sort_by_key(key);
        assert_eq!(actual, expected, "expiry preserves every younger mark");
        let mut uploaded = 0;
        trails
            .records
            .take_dirty(|_, records| uploaded += records.len());
        assert!(uploaded <= old_count);
        s.elapsed = 10.0 + TRACK_LIFETIME;
        trails.update(&s, 1.0);
        assert_eq!(trails.len(), 0, "no expired instances remain in the draw");
        step(&mut trails, &mut s, 3.0);
        assert!(
            !trails.is_empty(),
            "new tracks resume after all marks expire"
        );
    }

    #[test]
    fn upload_ranges_stay_bounded_through_multiple_lifetimes() {
        let mut s = state(VehicleKind::Balanced);
        let mut trails = TrackTrails::default();
        for i in 0..3000 {
            s.elapsed = i as f64 / 60.0;
            step(&mut trails, &mut s, (i % 300) as f64 * 0.1 - 15.0);
            // One tank appends one span and relocates at most two expired marks.
            assert!(trails.records.dirty().len() <= 3);
            trails.records.take_dirty(|_, _| {});
        }
        assert!(!trails.is_empty());
    }

    #[test]
    fn thirty_boosted_scouts_keep_laying_fresh_tracks_through_buffer_wraps() {
        let mut s = RenderState::default();
        s.map_theme = "village".into();
        let speed = vehicle(VehicleKind::Scout).speed * 1.5;
        for i in 0..30 {
            s.tanks.push(RenderTank {
                id: i,
                kind: VehicleKind::Scout,
                alive: true,
                ..RenderTank::default()
            });
        }
        let mut trails = TrackTrails::default();
        for frame in 0..=60 * 60 {
            s.elapsed = frame as f64 / 60.0;
            for (i, tank) in s.tanks.iter_mut().enumerate() {
                tank.position = Point3 {
                    x: s.elapsed * speed,
                    y: 0.65,
                    z: i as f64 * 3.0,
                };
                tank.previous = Vec2 {
                    x: tank.position.x,
                    z: tank.position.z,
                };
            }
            trails.update(&s, 1.0);
            trails.records.take_dirty(|_, _| {});
            if frame > 0 && frame % 60 == 0 {
                let fresh = (0..trails.len())
                    .filter(|&slot| s.elapsed - birth(&trails.records, slot) < 1.0)
                    .count();
                assert!(
                    fresh > 2500,
                    "fresh trails stalled at {}s: {fresh}",
                    s.elapsed
                );
            }
        }
        assert!(trails.len() > 50000 && trails.len() < TRACK_CAPACITY);
        for slot in 0..trails.len() {
            assert!(
                s.elapsed - birth(&trails.records, slot) < TRACK_LIFETIME,
                "expired marks must not be submitted"
            );
        }
    }

    #[test]
    fn humvees_leave_denser_fainter_wheel_trails() {
        let trail = |kind| {
            let mut s = state(kind);
            let mut trails = TrackTrails::default();
            for i in 0..=60 {
                s.tanks[0].previous = Vec2 {
                    x: i as f64 * 0.5 - 0.5,
                    z: 0.0,
                };
                s.elapsed = i as f64 / 60.0;
                step(&mut trails, &mut s, i as f64 * 0.5);
            }
            (trails.len(), trails.records.records()[0].data[1])
        };
        let humvee = trail(VehicleKind::Humvee);
        let scout = trail(VehicleKind::Scout);
        assert!(humvee.0 as f64 > scout.0 as f64 * 1.8);
        assert!((humvee.1 - HUMVEE_TRACK_STRENGTH).abs() < 1e-6);
        assert_eq!(scout.1, 1.0);
    }
}
