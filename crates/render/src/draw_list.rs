//! Per-frame draw lists. Visible part instances are collected per view with their
//! instance record, then ordered like Three's render lists: opaque draws by render
//! order and draw class (so every instance sharing a mesh and material becomes one
//! instanced draw) in a linear pass, transparent draws by render order and then back
//! to front, one draw each. All buffers are reused between frames.

use bytemuck::{Pod, Zeroable};
use glam::Mat4;

/// The per-instance GPU record (96 bytes), WGSL `Instance`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct InstanceRecord {
    pub world: [f32; 16],
    /// rgb multiplies the base color; a is opacity.
    pub tint: [f32; 4],
    /// Free for effects.
    pub data: [f32; 4],
}

/// The WebGL build keeps records in an RGBA32F texture (`gpu/instance_store.rs`,
/// `instances_texture.wgsl`): one texel per vec4 of a record, this many records
/// per texture row.
pub const RECORD_TEXELS: u32 = (size_of::<InstanceRecord>() / 16) as u32;
pub const RECORDS_PER_ROW: u32 = 256;

impl InstanceRecord {
    pub fn new(world: &Mat4, tint: [f32; 4], data: [f32; 4]) -> Self {
        Self {
            world: world.to_cols_array(),
            tint,
            data,
        }
    }

    pub const IDENTITY: InstanceRecord = InstanceRecord {
        world: [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ],
        tint: [1.0; 4],
        data: [0.0; 4],
    };
}

/// Views drawn each frame.
pub const MAIN_VIEW: usize = 0;
pub const REFLECTION_VIEW: usize = 1;
pub const SHADOW_VIEW: usize = 2;
pub const VIEW_COUNT: usize = 3;

/// One draw call: a class (mesh + material + pipeline) over a record range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Draw {
    pub class: u32,
    pub first_instance: u32,
    pub instance_count: u32,
}

#[derive(Clone, Debug, Default)]
pub struct ViewDraws {
    pub opaque: Vec<Draw>,
    pub transparent: Vec<Draw>,
}

impl ViewDraws {
    pub fn clear(&mut self) {
        self.opaque.clear();
        self.transparent.clear();
    }
    pub fn len(&self) -> usize {
        self.opaque.len() + self.transparent.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Clone, Copy, Debug)]
enum Source {
    /// Index into `DrawListBuilder::pending_records`.
    Record(u32),
    /// A persistent range (static instanced scenery).
    Range { first: u32, count: u32 },
}

#[derive(Clone, Copy, Debug)]
struct Item {
    render_order: i32,
    class: u32,
    /// View-space depth (larger is farther), for transparent sorting.
    depth: f32,
    source: Source,
}

#[derive(Default)]
pub struct DrawListBuilder {
    pending_records: Vec<InstanceRecord>,
    opaque: [Vec<Item>; VIEW_COUNT],
    transparent: [Vec<Item>; VIEW_COUNT],
    /// This frame's dynamic records, laid out so each draw's range is contiguous.
    pub records: Vec<InstanceRecord>,
    /// Scratch for ordering opaque items ([`sort_opaque`]).
    counts: Vec<u32>,
    sorted: Vec<Item>,
}

impl DrawListBuilder {
    pub fn clear(&mut self) {
        self.pending_records.clear();
        self.records.clear();
        for list in self.opaque.iter_mut().chain(self.transparent.iter_mut()) {
            list.clear();
        }
    }

    /// Store a record once; the same record may be pushed to several views.
    pub fn record(&mut self, record: InstanceRecord) -> u32 {
        self.pending_records.push(record);
        self.pending_records.len() as u32 - 1
    }

    pub fn push(
        &mut self,
        view: usize,
        class: u32,
        transparent: bool,
        render_order: i32,
        depth: f32,
        record: u32,
    ) {
        let item = Item {
            render_order,
            class,
            depth,
            source: Source::Record(record),
        };
        if transparent {
            self.transparent[view].push(item);
        } else {
            self.opaque[view].push(item);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push_range(
        &mut self,
        view: usize,
        class: u32,
        transparent: bool,
        render_order: i32,
        depth: f32,
        first: u32,
        count: u32,
    ) {
        let item = Item {
            render_order,
            class,
            depth,
            source: Source::Range { first, count },
        };
        if transparent {
            self.transparent[view].push(item);
        } else {
            self.opaque[view].push(item);
        }
    }

    /// Sort every view and emit draws. Dynamic records start at `base` in the GPU
    /// instance buffer; `self.records` holds them in draw order afterwards.
    pub fn finish(&mut self, base: u32, views: &mut [ViewDraws; VIEW_COUNT]) {
        self.records.clear();
        for (view, draws) in views.iter_mut().enumerate() {
            draws.clear();
            let opaque = &mut self.opaque[view];
            sort_opaque(opaque, &mut self.sorted, &mut self.counts);
            for item in opaque.iter() {
                match item.source {
                    Source::Range { first, count } => draws.opaque.push(Draw {
                        class: item.class,
                        first_instance: first,
                        instance_count: count,
                    }),
                    Source::Record(index) => {
                        let at = base + self.records.len() as u32;
                        self.records.push(self.pending_records[index as usize]);
                        match draws.opaque.last_mut() {
                            Some(last)
                                if last.class == item.class
                                    && last.first_instance + last.instance_count == at =>
                            {
                                last.instance_count += 1;
                            }
                            _ => draws.opaque.push(Draw {
                                class: item.class,
                                first_instance: at,
                                instance_count: 1,
                            }),
                        }
                    }
                }
            }
            let transparent = &mut self.transparent[view];
            transparent.sort_unstable_by(|a, b| {
                a.render_order
                    .cmp(&b.render_order)
                    .then(b.depth.total_cmp(&a.depth))
                    .then(a.class.cmp(&b.class))
            });
            for item in transparent.iter() {
                let (first_instance, instance_count) = match item.source {
                    Source::Range { first, count } => (first, count),
                    Source::Record(index) => {
                        self.records.push(self.pending_records[index as usize]);
                        let at = base + self.records.len() as u32 - 1;
                        // Neighbours in the sorted order that share a class draw as
                        // one instanced call: instances rasterize in order, so the
                        // blend order is unchanged (tank bars, pickup glows).
                        if let Some(last) = draws.transparent.last_mut()
                            && last.class == item.class
                            && last.first_instance + last.instance_count == at
                        {
                            last.instance_count += 1;
                            continue;
                        }
                        (at, 1)
                    }
                };
                draws.transparent.push(Draw {
                    class: item.class,
                    first_instance,
                    instance_count,
                });
            }
        }
    }
}

/// Where an opaque item sorts within its render order: each class's records, then
/// its persistent ranges, class after class.
fn bucket(item: &Item) -> usize {
    2 * item.class as usize + matches!(item.source, Source::Range { .. }) as usize
}

/// A record's place within its bucket: records in record (push) order; ranges last.
fn rank(item: &Item) -> u32 {
    match item.source {
        Source::Record(index) => index,
        Source::Range { .. } => u32::MAX,
    }
}

/// Put a view's opaque items in draw order: by render order, then by class, each
/// class's records in push order before its ranges (in push order too), so a class's
/// records are contiguous and become one instanced draw. With one render order, the
/// usual case, that is a counting sort over the class buckets: linear in the items,
/// where a comparison sort of thousands of items a frame showed in WebGPU CPU
/// profiles of the Stress Grid. A list that mixes render orders (no opaque draw sets
/// one today) takes a stable comparison sort to the same order.
fn sort_opaque(items: &mut [Item], sorted: &mut Vec<Item>, counts: &mut Vec<u32>) {
    let Some(&first) = items.first() else {
        return;
    };
    if items
        .iter()
        .any(|item| item.render_order != first.render_order)
    {
        items.sort_by_key(|item| (item.render_order, bucket(item), rank(item)));
        return;
    }
    let buckets = items.iter().map(bucket).max().map_or(0, |last| last + 1);
    counts.clear();
    counts.resize(buckets + 1, 0);
    for item in items.iter() {
        counts[bucket(item) + 1] += 1;
    }
    for i in 1..counts.len() {
        counts[i] += counts[i - 1];
    }
    sorted.clear();
    sorted.resize(items.len(), first);
    for item in items.iter() {
        let at = &mut counts[bucket(item)];
        sorted[*at as usize] = *item;
        *at += 1;
    }
    // Copied back rather than swapped, so each view's list keeps only its own
    // capacity (Wasm memory never shrinks).
    items.copy_from_slice(sorted);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(x: f32) -> InstanceRecord {
        InstanceRecord::new(
            &Mat4::from_translation(glam::Vec3::new(x, 0.0, 0.0)),
            [1.0; 4],
            [0.0; 4],
        )
    }

    fn draws(list: &[Draw]) -> Vec<(u32, u32, u32)> {
        list.iter()
            .map(|d| (d.class, d.first_instance, d.instance_count))
            .collect()
    }

    #[test]
    fn opaque_instances_of_a_class_become_one_draw() {
        let mut builder = DrawListBuilder::default();
        let mut views: [ViewDraws; VIEW_COUNT] = Default::default();
        for (i, class) in [3, 1, 3, 1, 3].into_iter().enumerate() {
            let r = builder.record(record(i as f32));
            builder.push(MAIN_VIEW, class, false, 0, 0.0, r);
            builder.push(SHADOW_VIEW, class, false, 0, 0.0, r);
        }
        builder.push_range(MAIN_VIEW, 1, false, 0, 0.0, 0, 500);
        builder.finish(10, &mut views);
        assert_eq!(
            views[MAIN_VIEW].opaque,
            [
                Draw {
                    class: 1,
                    first_instance: 10,
                    instance_count: 2
                },
                Draw {
                    class: 1,
                    first_instance: 0,
                    instance_count: 500
                },
                Draw {
                    class: 3,
                    first_instance: 12,
                    instance_count: 3
                },
            ]
        );
        assert_eq!(views[SHADOW_VIEW].opaque.len(), 2);
        assert_eq!(builder.records.len(), 10);
        // Records follow draw order, so each range is the class's instances.
        assert_eq!(builder.records[0].world[12], 1.0);
        assert_eq!(builder.records[1].world[12], 3.0);
    }

    #[test]
    fn transparent_draws_sort_by_order_then_back_to_front() {
        let mut builder = DrawListBuilder::default();
        let mut views: [ViewDraws; VIEW_COUNT] = Default::default();
        for (class, order, depth) in [(1, 0, 5.0), (2, 0, 50.0), (3, -1, 1.0), (4, 0, 20.0)] {
            let r = builder.record(record(depth));
            builder.push(MAIN_VIEW, class, true, order, depth, r);
        }
        builder.finish(0, &mut views);
        let classes: Vec<_> = views[MAIN_VIEW]
            .transparent
            .iter()
            .map(|d| d.class)
            .collect();
        assert_eq!(classes, [3, 2, 4, 1]);
        // Neighbours of one class merge; a class between them keeps them apart.
        builder.clear();
        for (class, order, depth) in [(5, 1, 30.0), (5, 1, 20.0), (6, 1, 10.0), (5, 1, 5.0)] {
            let r = builder.record(record(depth));
            builder.push(MAIN_VIEW, class, true, order, depth, r);
        }
        builder.finish(0, &mut views);
        let draws: Vec<_> = views[MAIN_VIEW]
            .transparent
            .iter()
            .map(|d| (d.class, d.instance_count))
            .collect();
        assert_eq!(draws, [(5, 2), (6, 1), (5, 1)]);
        builder.clear();
        builder.finish(0, &mut views);
        assert!(views[MAIN_VIEW].is_empty());
    }

    #[test]
    fn opaque_records_keep_push_order_before_ranges() {
        // One render order takes the counting sort; a range pushed before its class's
        // records still draws after them, and records stay in push order.
        let mut builder = DrawListBuilder::default();
        let mut views: [ViewDraws; VIEW_COUNT] = Default::default();
        builder.push_range(MAIN_VIEW, 0, false, 0, 0.0, 0, 8);
        for (i, class) in [1, 0, 1, 0].into_iter().enumerate() {
            let r = builder.record(record(i as f32));
            builder.push(MAIN_VIEW, class, false, 0, 0.0, r);
        }
        builder.finish(20, &mut views);
        assert_eq!(
            draws(&views[MAIN_VIEW].opaque),
            [(0, 20, 2), (0, 0, 8), (1, 22, 2)]
        );
        let xs: Vec<_> = builder.records.iter().map(|r| r.world[12]).collect();
        assert_eq!(xs, [1.0, 3.0, 0.0, 2.0]);
    }

    #[test]
    fn mixed_render_orders_sort_by_order_then_class() {
        // The comparison sort a mixed list takes keeps the counting sort's order
        // within each render order, several ranges of a class in push order included.
        let mut builder = DrawListBuilder::default();
        let mut views: [ViewDraws; VIEW_COUNT] = Default::default();
        builder.push_range(MAIN_VIEW, 2, false, 0, 0.0, 0, 50);
        builder.push_range(MAIN_VIEW, 1, false, 0, 0.0, 100, 7);
        builder.push_range(MAIN_VIEW, 2, false, 0, 0.0, 60, 30);
        let items = [(2, 0), (1, 0), (2, -1), (0, 1), (1, 0), (2, -1)];
        for (i, (class, order)) in items.into_iter().enumerate() {
            let r = builder.record(record(i as f32));
            builder.push(MAIN_VIEW, class, false, order, 0.0, r);
        }
        builder.finish(200, &mut views);
        assert_eq!(
            draws(&views[MAIN_VIEW].opaque),
            [
                (2, 200, 2),
                (1, 202, 2),
                (1, 100, 7),
                (2, 204, 1),
                (2, 0, 50),
                (2, 60, 30),
                (0, 205, 1)
            ]
        );
        let xs: Vec<_> = builder.records.iter().map(|r| r.world[12]).collect();
        assert_eq!(xs, [2.0, 5.0, 1.0, 4.0, 0.0, 3.0]);
    }

    #[test]
    fn opaque_order_matches_a_stable_comparison_sort() {
        // Both paths: one render order (the counting sort) and mixed render orders
        // (the comparison sort), with many ranges per class.
        let mut random = crate::effects::random::CosmeticRandom::seeded(7);
        let mut pick = |n: f64| (random.next_f64() * n) as u32;
        // What an item is: its class, whether it is a range, and its record or first.
        let identity = |item: &Item| match item.source {
            Source::Record(index) => (item.class, false, index),
            Source::Range { first, .. } => (item.class, true, first),
        };
        let (mut sorted, mut counts) = (Vec::new(), Vec::new());
        for round in 0..20 {
            let orders = if round % 2 == 0 { 1.0 } else { 3.0 };
            let mut items: Vec<Item> = (0..300)
                .map(|index| Item {
                    render_order: pick(orders) as i32 - 1,
                    class: pick(40.0),
                    depth: 0.0,
                    source: if pick(10.0) == 0 {
                        Source::Range {
                            first: index,
                            count: 1,
                        }
                    } else {
                        Source::Record(index)
                    },
                })
                .collect();
            // Stable, so several ranges of one class keep their push order too.
            let mut expected = items.clone();
            expected.sort_by_key(|item| (item.render_order, bucket(item), rank(item)));
            sort_opaque(&mut items, &mut sorted, &mut counts);
            let got: Vec<_> = items.iter().map(identity).collect();
            let expected: Vec<_> = expected.iter().map(identity).collect();
            assert_eq!(got, expected);
        }
    }
}
