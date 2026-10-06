//! Instance pools: the CPU side of one bounded instanced draw (the former
//! `InstancedMesh` + `storageInstances` + `updateInstances`). An effect writes
//! instance records here; the renderer uploads only the dirty ranges into the
//! pool's own GPU buffer, which never moves, and draws `len()` instances (none
//! while empty).

use std::ops::Range;
use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};
use sloppy_core::geometry::Mesh;
use sloppy_core::scene::Material;

use crate::draw_list::InstanceRecord;

/// Upload lists longer than this collapse into one covering range; effects
/// append one span and relocate a few slots per frame.
const MAX_DIRTY_RANGES: usize = 8;

/// What a pool draws and how.
#[derive(Clone, Debug)]
pub struct PoolDesc {
    pub label: &'static str,
    pub mesh: Arc<Mesh>,
    pub material: Arc<Material>,
    /// Maximum instances; the GPU buffer is allocated at this size once.
    pub capacity: u32,
    pub render_order: i32,
    pub cast_shadow: bool,
    pub receive_shadow: bool,
    /// Drawn in the planar water reflection too (Three's default layer).
    pub reflected: bool,
}

impl PoolDesc {
    /// An unshadowed, reflected pool, like the game's effect InstancedMeshes.
    pub fn new(label: &'static str, mesh: Mesh, material: Material, capacity: usize) -> Self {
        Self {
            label,
            mesh: Arc::new(mesh),
            material: Arc::new(material),
            capacity: capacity as u32,
            render_order: 0,
            cast_shadow: false,
            receive_shadow: false,
            reflected: true,
        }
    }
}

/// Instance records plus the index ranges changed since the last upload.
#[derive(Clone, Debug)]
pub struct PoolBuffer {
    records: Vec<InstanceRecord>,
    capacity: usize,
    dirty: Vec<Range<u32>>,
}

impl PoolBuffer {
    /// The record storage is reserved once, so writing never reallocates.
    pub fn new(capacity: usize) -> Self {
        Self {
            records: Vec::with_capacity(capacity),
            capacity,
            dirty: Vec::with_capacity(MAX_DIRTY_RANGES + 1),
        }
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.records.len() >= self.capacity
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn records(&self) -> &[InstanceRecord] {
        &self.records
    }

    /// Drop every instance. Nothing needs uploading until records are written.
    pub fn clear(&mut self) {
        self.records.clear();
        self.dirty.clear();
    }

    /// Append a record; `false` (and nothing written) when the pool is full.
    pub fn push(&mut self, record: InstanceRecord) -> bool {
        if self.is_full() {
            return false;
        }
        let slot = self.records.len() as u32;
        self.records.push(record);
        self.mark(slot..slot + 1);
        true
    }

    pub fn set(&mut self, slot: usize, record: InstanceRecord) {
        self.records[slot] = record;
        self.mark(slot as u32..slot as u32 + 1);
    }

    /// Remove `slot` by moving the last record into it, keeping one dense draw.
    pub fn swap_remove(&mut self, slot: usize) {
        self.records.swap_remove(slot);
        if slot < self.records.len() {
            self.mark(slot as u32..slot as u32 + 1);
        }
    }

    /// Index ranges written since `take_dirty`, sorted and merged.
    pub fn dirty(&self) -> &[Range<u32>] {
        &self.dirty
    }

    /// Hand each pending upload range (its first slot and records) to `upload`
    /// and forget them. Ranges past the live count are clipped: those slots are
    /// not drawn.
    pub fn take_dirty(&mut self, mut upload: impl FnMut(u32, &[InstanceRecord])) {
        let records = &self.records;
        let live = records.len() as u32;
        for range in self.dirty.drain(..) {
            let end = range.end.min(live);
            if range.start < end {
                upload(range.start, &records[range.start as usize..end as usize]);
            }
        }
    }

    fn mark(&mut self, range: Range<u32>) {
        // Merge with an overlapping or adjacent range (the common append case).
        for existing in &mut self.dirty {
            if range.start <= existing.end && existing.start <= range.end {
                existing.start = existing.start.min(range.start);
                existing.end = existing.end.max(range.end);
                return;
            }
        }
        self.dirty.push(range);
        if self.dirty.len() > MAX_DIRTY_RANGES {
            let start = self.dirty.iter().map(|r| r.start).min().unwrap_or(0);
            let end = self.dirty.iter().map(|r| r.end).max().unwrap_or(0);
            self.dirty.clear();
            self.dirty.push(start..end);
        }
    }
}

/// Three's `Object3D` pose with an XYZ Euler rotation, as `updateMatrix` builds it.
pub fn pose(position: Vec3, euler: Vec3, scale: Vec3) -> Mat4 {
    Mat4::from_scale_rotation_translation(scale, euler_xyz(euler), position)
}

/// Three's default `Euler` order: R = Rx · Ry · Rz.
pub fn euler_xyz(euler: Vec3) -> Quat {
    Quat::from_rotation_x(euler.x) * Quat::from_rotation_y(euler.y) * Quat::from_rotation_z(euler.z)
}

/// A record with a linear tint (rgb multiplies the material color, a is opacity).
pub fn record(world: Mat4, tint: [f32; 4], data: [f32; 4]) -> InstanceRecord {
    InstanceRecord::new(&world, tint, data)
}

/// `THREE.MathUtils.smoothstep(x, min, max)`.
pub fn smoothstep(x: f64, min: f64, max: f64) -> f64 {
    if x <= min {
        return 0.0;
    }
    if x >= max {
        return 1.0;
    }
    let t = (x - min) / (max - min);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f32) -> InstanceRecord {
        record(
            Mat4::from_translation(Vec3::new(x, 0.0, 0.0)),
            [1.0; 4],
            [0.0; 4],
        )
    }

    #[test]
    fn pools_are_bounded_and_track_merged_upload_ranges() {
        let mut pool = PoolBuffer::new(4);
        for i in 0..6 {
            pool.push(at(i as f32));
        }
        assert_eq!(pool.len(), 4, "a full pool refuses records");
        assert_eq!(pool.dirty().first(), Some(&(0..4)));
        let mut uploads = Vec::new();
        pool.take_dirty(|first, records| uploads.push((first, records.len())));
        assert_eq!(uploads, [(0, 4)]);
        assert!(pool.dirty().is_empty());
        pool.swap_remove(1);
        assert_eq!(
            pool.records()[1].translation().x,
            3.0,
            "the last record fills the hole"
        );
        pool.swap_remove(2);
        assert_eq!(pool.len(), 2);
        let mut uploads = Vec::new();
        pool.take_dirty(|first, records| uploads.push((first, records.len())));
        assert_eq!(
            uploads,
            [(1, 1)],
            "slots past the live count are not uploaded"
        );
        let capacity = pool.records.capacity();
        pool.clear();
        assert!(pool.is_empty() && pool.dirty().is_empty());
        assert_eq!(pool.records.capacity(), capacity, "clearing keeps storage");
    }

    #[test]
    fn scattered_writes_collapse_into_one_range() {
        let mut pool = PoolBuffer::new(64);
        for i in 0..64 {
            pool.push(at(i as f32));
        }
        pool.take_dirty(|_, _| {});
        for slot in (0..40).step_by(4) {
            pool.set(slot, at(0.0));
        }
        // The ninth range collapsed the first nine; the tenth starts a new list.
        assert_eq!(pool.dirty(), [0..33, 36..37]);
    }

    #[test]
    fn euler_order_matches_three() {
        let q = euler_xyz(Vec3::new(0.3, 0.5, 0.7));
        let m =
            Mat4::from_rotation_x(0.3) * Mat4::from_rotation_y(0.5) * Mat4::from_rotation_z(0.7);
        assert!(Mat4::from_quat(q).abs_diff_eq(m, 1e-6));
        assert_eq!(smoothstep(0.5, 0.0, 1.0), 0.5);
        assert_eq!(smoothstep(-1.0, 0.0, 1.0), 0.0);
    }
}
