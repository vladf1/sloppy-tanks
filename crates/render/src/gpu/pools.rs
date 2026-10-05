//! Instance pools: bounded, per-frame effect instances (particles, puffs, tread
//! marks, shells in flight) drawn as one instanced call each, without culling,
//! like the former `InstancedMesh` + `frustumCulled = false` pools.
//!
//! Each pool owns a GPU record store sized to its capacity. It never moves or
//! grows, so only the ranges an effect changed are uploaded (`sync_pool`), and a
//! mark written once (a tread print) is uploaded once. A pool has its own draw
//! class, and its draws bind a copy of the view's frame group whose instance
//! buffer is the pool's; they sort among the scene's other draws by render order
//! and, when transparent, by the depth of the scene origin (Three sorted an
//! InstancedMesh by its object position). Empty pools cost no draws.

use glam::Vec3;

use super::backend::{FrameGroups, InstanceStore};
use super::{Lifetime, Renderer};
use crate::draw_list::{InstanceRecord, REFLECTION_VIEW, SHADOW_VIEW};
use crate::effects::pool::{PoolBuffer, PoolDesc};

/// A registered pool; stale ids are ignored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PoolId {
    index: u32,
    generation: u32,
}

pub(super) struct PoolEntry {
    pub label: &'static str,
    pub class: u32,
    pub material: u32,
    pub render_order: i32,
    pub cast_shadow: bool,
    pub reflected: bool,
    pub capacity: u32,
    /// Instances drawn (the pool's live record count at the last sync).
    pub count: u32,
    pub lifetime: Lifetime,
    pub records: InstanceStore,
    /// The frame bindings per view, with this pool's records as `instances`.
    pub groups: FrameGroups,
}

impl Renderer {
    /// Register a pool. Its pipelines join the prepare/warm-up set at once, so
    /// the first effect never compiles mid-round.
    pub fn add_pool(&mut self, desc: &PoolDesc, lifetime: Lifetime) -> PoolId {
        let gpu = &self.gpu;
        let material = self.interner.intern(&desc.material);
        let material =
            self.materials
                .get_or_create(gpu, &mut self.textures, &self.effects, &material);
        let attributes = self.effects.attributes(self.materials.get(material).effect);
        let mesh = self.meshes.shared(gpu, &desc.mesh, attributes);
        let capacity = desc.capacity.max(1);
        let records = InstanceStore::new(gpu, desc.label, capacity);
        let groups = self.frame.frame_groups(gpu, &records);
        // Reserve the slot first: the class key names the pool it draws.
        let (index, generation) = self.pools.insert(PoolEntry {
            label: desc.label,
            class: u32::MAX,
            material,
            render_order: desc.render_order,
            cast_shadow: desc.cast_shadow,
            reflected: desc.reflected,
            capacity,
            count: 0,
            lifetime,
            records,
            groups,
        });
        let class =
            self.class_for_pool(mesh, material, desc.receive_shadow, desc.cast_shadow, index);
        self.pools
            .get_mut(index, generation)
            .expect("just inserted")
            .class = class;
        PoolId { index, generation }
    }

    pub fn has_pool(&self, id: PoolId) -> bool {
        self.pools.get(id.index, id.generation).is_some()
    }

    pub fn remove_pool(&mut self, id: PoolId) {
        if self.pools.get(id.index, id.generation).is_none() {
            return;
        }
        // Dropping the entry destroys its records.
        let entry = self.pools.remove(id.index).expect("checked");
        self.release_class(entry.class, entry.cast_shadow);
    }

    /// Upload the records an effect changed and set the drawn count.
    pub fn sync_pool(&mut self, id: PoolId, records: &mut PoolBuffer) {
        let Some(entry) = self.pools.get_mut(id.index, id.generation) else {
            records.take_dirty(|_, _| {});
            return;
        };
        // A pool buffer and its GPU buffer share one capacity.
        let capacity = entry.capacity as usize;
        entry.count = records.len().min(capacity) as u32;
        let gpu = &self.gpu;
        let store = &entry.records;
        records.take_dirty(|first, slice: &[InstanceRecord]| {
            let end = (first as usize + slice.len()).min(capacity);
            let Some(count) = end.checked_sub(first as usize).filter(|&n| n > 0) else {
                return;
            };
            store.write(gpu, first, &slice[..count]);
        });
    }

    /// Overwrite the effect params (16 floats) the pool's material uniform
    /// carries, for per-frame effect clocks. The GPU material is shared by every
    /// user of an equal material, so pools that animate params use a material
    /// no model shares (their own effect).
    pub fn set_pool_params(&mut self, id: PoolId, params: [[f32; 4]; 4]) {
        if let Some(entry) = self.pools.get(id.index, id.generation) {
            let material = self.materials.get(entry.material);
            material.binding.write_params(&self.gpu, &params);
        }
    }

    /// Rebind every pool's frame groups after the frame's own resources changed.
    pub(super) fn rebuild_pool_groups(&mut self) {
        for (_, entry) in self.pools.slots.iter_mut() {
            if let Some(entry) = entry {
                entry.groups = self.frame.frame_groups(&self.gpu, &entry.records);
            }
        }
    }

    /// Queue the non-empty pools into this frame's draw lists.
    pub(super) fn push_pool_draws(&mut self) {
        let Self {
            pools,
            classes,
            builder,
            culls,
            ..
        } = self;
        for (_, pool) in pools.iter() {
            if pool.count == 0 {
                continue;
            }
            let transparent = classes[pool.class as usize]
                .as_ref()
                .is_some_and(|class| class.transparent);
            for (view, cull) in culls.iter().enumerate() {
                if !cull.active
                    || (view == SHADOW_VIEW && !pool.cast_shadow)
                    || (view == REFLECTION_VIEW && !pool.reflected)
                {
                    continue;
                }
                let depth = (Vec3::ZERO - cull.origin).dot(cull.forward);
                builder.push_range(
                    view,
                    pool.class,
                    transparent && view != SHADOW_VIEW,
                    pool.render_order,
                    depth,
                    0,
                    pool.count,
                );
            }
        }
    }

    pub(super) fn release_round_pools(&mut self) {
        let round: Vec<(u32, u32)> = self
            .pools
            .iter()
            .filter(|(_, pool)| pool.lifetime == Lifetime::Round)
            .map(|(index, _)| (index, self.pools.slots[index as usize].0))
            .collect();
        for (index, generation) in round {
            self.remove_pool(PoolId { index, generation });
        }
    }

    /// What the pools' stores allocate: on WebGL a store rounds its capacity up to
    /// whole texture rows, so this can exceed the pools' capacities.
    pub(super) fn pool_bytes(&self) -> u64 {
        self.pools
            .iter()
            .map(|(_, pool)| pool.records.bytes())
            .sum()
    }

    /// `(pools, instances drawn)` for stats.
    pub(super) fn pool_totals(&self) -> (u32, u32) {
        self.pools
            .iter()
            .fold((0, 0), |(pools, instances), (_, pool)| {
                (pools + 1, instances + pool.count)
            })
    }

    /// Pool labels and drawn counts, for debugging views.
    pub fn pool_summary(&self) -> Vec<(&'static str, u32)> {
        self.pools
            .iter()
            .map(|(_, pool)| (pool.label, pool.count))
            .collect()
    }
}
