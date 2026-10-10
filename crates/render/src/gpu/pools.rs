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
use super::{RECORD_SIZE, Renderer};
use crate::draw_list::SHADOW_VIEW;
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
    /// Instances drawn (the pool's live record count at the last sync).
    pub count: u32,
    pub records: InstanceStore,
    /// The frame bindings per view, with this pool's records as `instances`.
    pub groups: FrameGroups,
}

impl Renderer {
    /// Register a pool for the renderer's lifetime. Its pipelines join the
    /// prepare/warm-up set at once, so the first effect never compiles mid-round.
    pub fn add_pool(&mut self, desc: &PoolDesc) -> PoolId {
        let gpu = &self.gpu;
        let material = self.interner.intern(&desc.material);
        let material =
            self.materials
                .get_or_create(gpu, &mut self.textures, &self.effects, &material);
        let mesh = self.meshes.shared(gpu, &desc.mesh);
        let capacity = desc.capacity.max(1);
        let records = InstanceStore::new(gpu, desc.label, capacity);
        let groups = self.frame.frame_groups(gpu, &records);
        // Reserve the slot first: the class key names the pool it draws.
        let (index, generation) = self.pools.insert(PoolEntry {
            label: desc.label,
            class: u32::MAX,
            material,
            render_order: desc.render_order,
            count: 0,
            records,
            groups,
        });
        // Effect pools cast no shadows.
        let class = self.class(
            mesh,
            material,
            desc.receive_shadow,
            false,
            false,
            Some(index),
        );
        self.pools
            .get_mut(index, generation)
            .expect("just inserted")
            .class = class;
        PoolId { index, generation }
    }

    /// Upload the records an effect changed and set the drawn count.
    pub fn sync_pool(&mut self, id: PoolId, records: &mut PoolBuffer) {
        let Some(entry) = self.pools.get_mut(id.index, id.generation) else {
            records.take_dirty(|_, _| {});
            return;
        };
        // A pool buffer and its GPU store share one capacity, so every record fits.
        entry.count = records.len() as u32;
        let (gpu, store) = (&self.gpu, &entry.records);
        records.take_dirty(|first, slice| store.write(gpu, first, slice));
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
        for entry in self.pools.iter_mut() {
            entry.groups = self.frame.frame_groups(&self.gpu, &entry.records);
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
            let transparent = classes
                .at(pool.class)
                .is_some_and(|class| class.transparent);
            // Pools cast no shadows; they draw in the main view and reflection.
            for (view, cull) in culls.iter().enumerate() {
                if !cull.active || view == SHADOW_VIEW {
                    continue;
                }
                let depth = (Vec3::ZERO - cull.origin).dot(cull.forward);
                builder.push_range(
                    view,
                    pool.class,
                    transparent,
                    pool.render_order,
                    depth,
                    0,
                    pool.count,
                );
            }
        }
    }

    /// What the pools' stores allocate: on WebGL a store rounds its capacity up to
    /// whole texture rows, so this can exceed the pools' capacities.
    pub(super) fn pool_bytes(&self) -> u64 {
        self.pools
            .iter()
            .map(|(_, pool)| u64::from(pool.records.capacity()) * RECORD_SIZE)
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
