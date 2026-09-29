//! Per-frame draw lists. Visible part instances are collected per view with their
//! instance record, then sorted like Three's render lists: opaque draws by render
//! order and draw class (so every instance sharing a mesh and material becomes one
//! instanced draw), transparent draws by render order and then back to front, one
//! draw each. All buffers are reused between frames.

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
            let rank = |item: &Item| match item.source {
                Source::Record(index) => index,
                Source::Range { .. } => u32::MAX,
            };
            opaque.sort_unstable_by_key(|item| (item.render_order, item.class, rank(item)));
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
                        (base + self.records.len() as u32 - 1, 1)
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
        builder.clear();
        builder.finish(0, &mut views);
        assert!(views[MAIN_VIEW].is_empty());
    }
}
