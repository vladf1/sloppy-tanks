//! Leaves and needle sprigs shaken from a tree's crown when a shell hits the tree
//! or it is felled: lit, folded leaf shapes in the tree's own foliage colours that
//! swing down under air drag, lie on the ground for a while and shrink away. One
//! bounded instanced draw; when it is full the oldest leaf makes room.

use std::f64::consts::TAU;

use glam::{Quat, Vec3};
use sloppy_core::geometry::Mesh;
use sloppy_core::models::TreeFoliage;
use sloppy_core::sim::{SimEvent, SimEventType};

use super::pool::{PoolBuffer, record};
use super::random::CosmeticRandom;
use crate::color::hex_to_linear;

pub const MAX_LEAVES: usize = 480;
const GRAVITY: f64 = 9.8;
/// Linear air drag per second: a leaf soon drifts down at `GRAVITY / DRAG`.
const DRAG: f64 = 7.5;
/// A landed leaf lies this high, above the ground decals.
const REST_HEIGHT: f64 = 0.025;
/// Seconds a landed leaf lies, [minimum, random span], then shrinks away.
const REST: [f64; 2] = [3.0, 4.0];
const SHRINK: f64 = 1.2;
/// Leaves a shell hit shakes loose, and a felled crown sheds as it goes over.
const HIT_LEAVES: usize = 26;
const FELL_LEAVES: usize = 44;
/// Of every hundred leaves, this many have turned yellow and this many brown.
const YELLOWED: f64 = 0.12;
const WITHERED: f64 = 0.05;
const YELLOW: u32 = 0xb8a548;
const BROWN: u32 = 0x7a5b34;

/// The crown of the tree an event belongs to: its trunk position and foliage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crown {
    pub x: f64,
    pub z: f64,
    pub foliage: TreeFoliage,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leaf {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub vx: f64,
    pub vy: f64,
    pub vz: f64,
    /// Sideways swing: its horizontal direction (the cosine and sine of its angle,
    /// fixed for the leaf's life), amplitude (m), rate (rad/s) and phase.
    swing_direction: [f64; 2],
    swing: f64,
    rate: f64,
    phase: f64,
    yaw: f64,
    spin: f64,
    pub age: f64,
    /// Seconds left lying on the ground once landed (negative while shrinking).
    pub rest: Option<f64>,
    /// The tilt the leaf landed with.
    tilt: f32,
    /// Its orientation once landed, which no longer changes.
    rest_rotation: Quat,
    /// Length and width in metres.
    length: f64,
    width: f64,
    /// Linear RGB.
    color: [f32; 3],
}

impl Leaf {
    fn tilt(&self) -> f32 {
        match self.rest {
            Some(_) => self.tilt,
            None => (0.75 * (self.age * self.rate + self.phase).sin()) as f32,
        }
    }

    fn rotation(&self) -> Quat {
        // A swinging leaf banks into its swing and rocks along its midrib.
        let roll = match self.rest {
            Some(_) => 0.0,
            None => (0.35 * (self.age * self.rate * 0.5 + self.phase).cos()) as f32,
        };
        let [cos, sin] = self.swing_direction;
        let across = Vec3::new(-sin as f32, 0.0, cos as f32);
        Quat::from_axis_angle(across, self.tilt())
            * Quat::from_rotation_y(self.yaw as f32)
            * Quat::from_rotation_x(roll)
    }

    fn record(&self) -> crate::draw_list::InstanceRecord {
        let shrink = self
            .rest
            .map_or(1.0, |rest| (1.0 + rest.min(0.0) / SHRINK).max(0.0));
        let length = (self.length * shrink) as f32;
        let scale = Vec3::new(length, length, length * self.width as f32);
        let rotation = match self.rest {
            Some(_) => self.rest_rotation,
            None => self.rotation(),
        };
        let world = glam::Mat4::from_scale_rotation_translation(
            scale,
            rotation,
            Vec3::new(self.x as f32, self.y as f32, self.z as f32),
        );
        let [r, g, b] = self.color;
        record(world, [r, g, b, 1.0], [0.0; 4])
    }
}

/// A leaf blade along local X, `[-0.5, 0.5]` long and about half as wide, folded
/// up along its midrib so it catches light from either side.
pub fn leaf_mesh() -> Mesh {
    let rib = [[-0.5, 0.0], [-0.15, 0.05], [0.2, 0.05], [0.5, 0.0]];
    let edge = [[-0.25, 0.2], [0.05, 0.24], [0.32, 0.14]];
    let mut positions = Vec::new();
    for side in [1.0, -1.0] {
        let m = |i: usize| [rib[i][0], rib[i][1], 0.0];
        let e = |i: usize| [edge[i][0], 0.0, side * edge[i][1]];
        let triangles = [
            [m(0), e(0), m(1)],
            [m(1), e(0), e(1)],
            [m(1), e(1), m(2)],
            [m(2), e(1), e(2)],
            [m(2), e(2), m(3)],
        ];
        for mut triangle in triangles {
            if side < 0.0 {
                triangle.swap(1, 2);
            }
            positions.extend(triangle.iter().flatten().copied());
        }
    }
    let mut mesh = Mesh::from_f64(&positions, &[], &[], None);
    mesh.compute_vertex_normals();
    mesh
}

#[derive(Clone, Debug)]
pub struct LeafFall {
    pub leaves: Vec<Leaf>,
    pub records: PoolBuffer,
}

impl Default for LeafFall {
    fn default() -> Self {
        Self {
            leaves: Vec::with_capacity(MAX_LEAVES),
            records: PoolBuffer::new(MAX_LEAVES),
        }
    }
}

impl LeafFall {
    pub fn reset(&mut self) {
        self.leaves.clear();
        self.records.clear();
    }

    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    /// Shake leaves from `crown` for a hit on its tree or its felling; other event
    /// kinds shed none.
    pub fn event(&mut self, event: &SimEvent, crown: &Crown, random: &mut CosmeticRandom) {
        let (count, burst) = match event.kind {
            SimEventType::Impact => (HIT_LEAVES, [0.4, 1.4]),
            SimEventType::Destroy => (FELL_LEAVES, [0.2, 0.7]),
            _ => return,
        };
        let foliage = &crown.foliage;
        let count = if foliage.conifer {
            count * 5 / 4
        } else {
            count
        };
        let base = hex_to_linear(foliage.color);
        let yellow = hex_to_linear(YELLOW);
        let brown = hex_to_linear(BROWN);
        let evicted = (self.leaves.len() + count).saturating_sub(MAX_LEAVES);
        if evicted > 0 {
            self.leaves.drain(..evicted);
            // Surviving leaves shifted slots; rebuild their records on update.
            self.records.clear();
        }
        for _ in 0..count {
            let mut next = || random.next_f64();
            let angle = next() * TAU;
            let distance = next().sqrt() * foliage.radius;
            let (out_x, out_z) = (angle.cos(), angle.sin());
            let speed = burst[0] + next() * (burst[1] - burst[0]);
            let light = 0.72 + next() * 0.5;
            let turn = next();
            let mut color = base.map(|channel| channel * light as f32);
            if turn < WITHERED {
                color = brown;
            } else if turn < WITHERED + YELLOWED {
                let k = 0.45 + next() as f32 * 0.4;
                color = [0, 1, 2].map(|i| color[i] + (yellow[i] - color[i]) * k);
            }
            let length = if foliage.conifer {
                0.1 + next() * 0.07
            } else {
                0.15 + next() * 0.09
            };
            self.leaves.push(Leaf {
                x: crown.x + out_x * distance,
                y: foliage.bottom + next() * (foliage.top - foliage.bottom),
                z: crown.z + out_z * distance,
                vx: out_x * speed,
                vy: -0.3 + next() * 1.2,
                vz: out_z * speed,
                swing_direction: {
                    let angle = next() * TAU;
                    [angle.cos(), angle.sin()]
                },
                swing: 0.18 + next() * 0.22,
                rate: 3.0 + next() * 2.5,
                phase: next() * TAU,
                yaw: next() * TAU,
                spin: (next() - 0.5) * 2.4,
                age: 0.0,
                rest: None,
                tilt: 0.0,
                rest_rotation: Quat::IDENTITY,
                length,
                width: if foliage.conifer { 0.3 } else { 1.0 },
                color,
            });
        }
    }

    pub fn update(&mut self, dt: f64, random: &mut CosmeticRandom) {
        let drag = (-DRAG * dt).exp();
        let terminal = -GRAVITY / DRAG;
        let mut slot = 0;
        let mut rewrite_from = self.records.len();
        self.leaves.retain_mut(|leaf| {
            let moving = leaf.rest.is_none();
            leaf.age += dt;
            let alive = match &mut leaf.rest {
                Some(rest) => {
                    *rest -= dt;
                    *rest > -SHRINK
                }
                None => {
                    // Exact linear-drag step toward the terminal fall speed.
                    leaf.vx *= drag;
                    leaf.vz *= drag;
                    leaf.vy = terminal + (leaf.vy - terminal) * drag;
                    let swing = leaf.swing * leaf.rate * (leaf.age * leaf.rate + leaf.phase).cos();
                    let [cos, sin] = leaf.swing_direction;
                    leaf.x += (leaf.vx + cos * swing) * dt;
                    leaf.z += (leaf.vz + sin * swing) * dt;
                    leaf.y += leaf.vy * dt;
                    leaf.yaw += leaf.spin * dt;
                    if leaf.y <= REST_HEIGHT {
                        leaf.y = REST_HEIGHT;
                        leaf.tilt = leaf.tilt() * 0.15;
                        leaf.rest = Some(REST[0] + random.next_f64() * REST[1]);
                        leaf.rest_rotation = leaf.rotation();
                    }
                    true
                }
            };
            if !alive || (moving && leaf.rest.is_some()) {
                rewrite_from = rewrite_from.min(slot);
            }
            if alive {
                slot += 1;
            }
            alive
        });
        for (slot, leaf) in self.leaves.iter().enumerate() {
            if slot >= rewrite_from || leaf.rest.is_none_or(|rest| rest < 0.0) {
                let record = leaf.record();
                if slot < self.records.len() {
                    self.records.set(slot, record);
                } else {
                    self.records.push(record);
                }
            }
        }
        self.records.truncate(slot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crown() -> Crown {
        Crown {
            x: 0.0,
            z: 0.0,
            foliage: TreeFoliage {
                conifer: false,
                color: 0x6a8a3c,
                bottom: 2.5,
                top: 5.5,
                radius: 1.1,
            },
        }
    }

    fn tree_event(kind: SimEventType) -> SimEvent {
        SimEvent::at(kind, 0.4, 1.0)
    }

    #[test]
    fn hits_and_fellings_shed_leaves_that_drift_down_land_and_go() {
        let mut random = CosmeticRandom::seeded(7);
        let mut fall = LeafFall::default();
        fall.event(&tree_event(SimEventType::Impact), &crown(), &mut random);
        assert_eq!(fall.len(), HIT_LEAVES);
        // Drag holds the fall to a drift: well under a free fall's speed.
        for _ in 0..30 {
            fall.update(1.0 / 60.0, &mut random);
        }
        assert!(
            fall.leaves
                .iter()
                .all(|leaf| leaf.vy > -1.5 && leaf.vy < 0.5)
        );
        // Every leaf reaches the ground within the crown height at drift speed...
        for _ in 0..(60 * 6) {
            fall.update(1.0 / 60.0, &mut random);
        }
        assert!(fall.leaves.iter().all(|leaf| leaf.rest.is_some()));
        assert!(fall.leaves.iter().all(|leaf| leaf.y == REST_HEIGHT));
        // A landed leaf keeps the orientation it landed with.
        assert!(
            fall.leaves
                .iter()
                .all(|leaf| leaf.rest_rotation == leaf.rotation())
        );
        // ...lies there for a while, then shrinks away.
        for _ in 0..(60 * 9) {
            fall.update(1.0 / 60.0, &mut random);
        }
        assert!(fall.is_empty());
        assert!(fall.records.is_empty());
    }

    #[test]
    fn resting_leaves_upload_only_when_they_shrink_or_shift_slots() {
        let mut random = CosmeticRandom::seeded(17);
        let mut fall = LeafFall::default();
        fall.event(&tree_event(SimEventType::Impact), &crown(), &mut random);
        for leaf in &mut fall.leaves {
            leaf.rest = Some(3.0);
            leaf.y = REST_HEIGHT;
        }
        fall.update(1.0 / 60.0, &mut random);
        fall.records.take_dirty(|_, _| {});
        let resting = fall.records.records().to_vec();
        fall.update(1.0 / 60.0, &mut random);
        assert!(fall.records.dirty().is_empty());
        assert_eq!(fall.records.records(), resting);

        fall.leaves[2].rest = Some(0.0);
        fall.update(1.0 / 60.0, &mut random);
        let mut uploaded = Vec::new();
        fall.records
            .take_dirty(|slot, records| uploaded.push((slot, records.len())));
        assert_eq!(uploaded, [(2, 1)]);
        assert_ne!(fall.records.records()[2], resting[2]);

        fall.leaves[1].rest = Some(-SHRINK);
        fall.update(1.0 / 60.0, &mut random);
        assert_eq!(fall.records.len(), HIT_LEAVES - 1);
        assert_eq!(
            fall.records.records(),
            fall.leaves.iter().map(Leaf::record).collect::<Vec<_>>()
        );
    }

    #[test]
    fn records_follow_leaves_through_bursts_eviction_expiry_and_reset() {
        let mut random = CosmeticRandom::seeded(41);
        let mut fall = LeafFall::default();
        for frame in 0..1200 {
            if frame < 30 || frame == 360 {
                let before = fall.leaves.clone();
                fall.event(&tree_event(SimEventType::Destroy), &crown(), &mut random);
                let evicted = (before.len() + FELL_LEAVES).saturating_sub(MAX_LEAVES);
                assert_eq!(&fall.leaves[..before.len() - evicted], &before[evicted..]);
            }
            fall.update(1.0 / 60.0, &mut random);
            assert_eq!(
                fall.records.records(),
                fall.leaves.iter().map(Leaf::record).collect::<Vec<_>>()
            );
            fall.records.take_dirty(|_, _| {});
            if frame == 400 {
                fall.reset();
                assert!(fall.records.is_empty() && fall.records.dirty().is_empty());
                fall.event(&tree_event(SimEventType::Impact), &crown(), &mut random);
            }
        }
        assert!(fall.is_empty());
    }

    #[test]
    fn the_pool_stays_bounded_by_dropping_the_oldest_leaves() {
        let mut random = CosmeticRandom::seeded(3);
        let mut fall = LeafFall::default();
        for _ in 0..40 {
            fall.event(&tree_event(SimEventType::Destroy), &crown(), &mut random);
        }
        assert_eq!(fall.len(), MAX_LEAVES);
        fall.update(1.0 / 60.0, &mut random);
        assert_eq!(fall.records.len(), MAX_LEAVES);
    }
}
