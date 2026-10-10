//! Per-frame draw lists. Visible part instances are collected per view with their
//! instance record, then ordered like Three's render lists: opaque draws by render
//! order and draw class in a linear pass, the classes grouped by the GPU state they
//! bind ([`ClassOrder`]), so every instance sharing a mesh and material becomes one
//! instanced draw; transparent draws by render order and then back to front, one draw
//! each. All buffers are reused between frames.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};

/// The per-instance GPU record (80 bytes), WGSL `InstanceRecord`, which
/// `instance_at` expands to an `Instance`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct InstanceRecord {
    /// The first three rows of the world transform. Instance transforms are affine,
    /// so the last row is always 0, 0, 0, 1 and is not stored: records are most of
    /// what a frame uploads, and the browser's buffer writes cost per byte.
    pub world_rows: [[f32; 4]; 3],
    /// rgb multiplies the base color; a is opacity.
    pub tint: [f32; 4],
    /// Free for effects.
    pub data: [f32; 4],
}

/// The WebGL build keeps records in an RGBA32F texture (`gpu/webgl/resources.rs`,
/// `instances_texture.wgsl`): one texel per vec4 of a record, this many records
/// per texture row.
pub const RECORD_TEXELS: u32 = (size_of::<InstanceRecord>() / 16) as u32;
pub const RECORDS_PER_ROW: u32 = 256;

impl InstanceRecord {
    pub fn new(world: &Mat4, tint: [f32; 4], data: [f32; 4]) -> Self {
        debug_assert!(
            world.row(3) == Vec4::W || world.is_nan(),
            "instance transforms are affine"
        );
        Self {
            world_rows: [
                world.row(0).to_array(),
                world.row(1).to_array(),
                world.row(2).to_array(),
            ],
            tint,
            data,
        }
    }

    pub fn world(&self) -> Mat4 {
        let [x, y, z] = self.world_rows.map(Vec4::from_array);
        Mat4::from_cols(x, y, z, Vec4::W).transpose()
    }

    pub fn translation(&self) -> Vec3 {
        let [x, y, z] = self.world_rows.map(|row| row[3]);
        Vec3::new(x, y, z)
    }

    pub const IDENTITY: InstanceRecord = InstanceRecord {
        world_rows: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
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
}

#[derive(Clone, Copy, Debug)]
enum Source {
    /// Index into `DrawListBuilder::pending_records`.
    Record(u32),
    /// A persistent range (static instanced scenery).
    Range { first: u32, count: u32 },
}

/// The GPU state an opaque draw of a class binds. Ordering classes by it
/// ([`Grouping`]) lets consecutive draws skip pipeline, mesh page, frame group and
/// material changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawState {
    /// A stable rank of the class's pipeline in the view; the pipeline itself may
    /// still be compiling when the lists are sorted.
    pub pipeline: u32,
    pub material: u32,
    /// The vertex page of its mesh (`crate::mesh_pages`).
    pub vertex_page: u32,
    /// The instance pool whose instance records it binds (`u32::MAX`: the view's own).
    pub pool: u32,
    /// The index page of its mesh, last: switching it is a single binding.
    pub index_page: u32,
}

/// What opaque draws group by after their pipeline, the costliest switch on both
/// backends. Each backend picks by what its bindings cost (`DRAW_GROUPING`); pools
/// and index pages break ties.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Grouping {
    /// Material, then vertex page. A WebGL material switch binds its uniform block
    /// and every texture and sampler that differs, up to a dozen calls, where a page
    /// switch rebinds one vertex array.
    #[default]
    MaterialFirst,
    /// Vertex page, then material. A WebGPU material switch is one bind group call,
    /// where a page switch sets one or two vertex buffers and often the index buffer.
    PageFirst,
}

impl DrawState {
    /// Where a class binding this state sorts, before its index.
    fn key(&self, grouping: Grouping) -> [u32; 5] {
        let (first, second) = match grouping {
            Grouping::MaterialFirst => (self.material, self.vertex_page),
            Grouping::PageFirst => (self.vertex_page, self.material),
        };
        [self.pipeline, first, second, self.pool, self.index_page]
    }
}

/// Each live class's position when classes are listed by the state they bind, then
/// by index. Opaque draws follow these positions. Classes come and go one at a time
/// and their state is fixed for their life, so each change moves one entry of the
/// sorted list and renumbers the classes after it, where re-sorting every class
/// whenever debris added one cost the Stress Grid about 24 µs a frame.
#[derive(Default)]
pub struct ClassOrder {
    grouping: Grouping,
    /// Live classes in draw order, by their [`DrawState::key`] and index.
    by_state: Vec<([u32; 5], u32)>,
    /// Each class's position in `by_state`, indexed by class; free slots keep a
    /// stale one, which no draw reads.
    positions: Vec<u32>,
}

impl ClassOrder {
    pub fn new(grouping: Grouping) -> Self {
        Self {
            grouping,
            ..Self::default()
        }
    }

    /// The positions [`DrawListBuilder::finish`] sorts by, indexed by class.
    pub fn positions(&self) -> &[u32] {
        &self.positions
    }

    pub fn insert(&mut self, class: u32, state: DrawState) {
        let entry = (state.key(self.grouping), class);
        let at = self.by_state.partition_point(|&other| other < entry);
        self.by_state.insert(at, entry);
        if self.positions.len() <= class as usize {
            self.positions.resize(class as usize + 1, 0);
        }
        self.renumber(at);
    }

    pub fn remove(&mut self, class: u32) {
        let at = self.positions[class as usize] as usize;
        debug_assert_eq!(self.by_state[at].1, class, "a live class");
        self.by_state.remove(at);
        self.renumber(at);
    }

    fn renumber(&mut self, from: usize) {
        for (position, &(_, class)) in self.by_state.iter().enumerate().skip(from) {
            self.positions[class as usize] = position as u32;
        }
    }
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
    buckets: Buckets,
}

/// Counting-sort scratch for [`sort_opaque`], kept between frames. The counts are
/// zero between sorts and a bit per bucket marks the buckets in use, so a sort visits
/// only those: a view draws a few hundred of the scene's thousands of classes (the
/// shadow view a few dozen), and visiting every class's buckets cost more than
/// sorting the items.
#[derive(Default)]
struct Buckets {
    counts: Vec<u32>,
    used: Vec<u64>,
    sorted: Vec<Item>,
}

impl Buckets {
    /// Call `visit` with every bucket marked in use, in increasing order.
    fn for_each_used(used: &[u64], mut visit: impl FnMut(usize)) {
        for (word, &bits) in used.iter().enumerate() {
            let mut bits = bits;
            while bits != 0 {
                visit(word * 64 + bits.trailing_zeros() as usize);
                bits &= bits - 1;
            }
        }
    }
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
    /// `order[view]` gives each class's position in that view
    /// ([`ClassOrder::positions`]); every class pushed this frame must have one.
    ///
    /// Within a render order, opaque draws follow their class's position. Opaque
    /// materials write depth with a less-equal test, so the order only decides depth
    /// ties, which the later draw wins. Coplanar layers, and parts closer together
    /// than the depth buffer resolves at the camera's distance, tie under any order,
    /// the class index included: freed classes are reused in reverse after a round
    /// reset. A part that must cover another needs depth between them, no hidden face
    /// beneath it (the cottage roof deck has no top face) or a render order of its own.
    pub fn finish(
        &mut self,
        base: u32,
        views: &mut [ViewDraws; VIEW_COUNT],
        order: [&[u32]; VIEW_COUNT],
    ) {
        let Self {
            pending_records,
            opaque,
            transparent,
            records,
            buckets,
        } = self;
        records.clear();
        for (view, draws) in views.iter_mut().enumerate() {
            draws.clear();
            for item in sort_opaque(&mut opaque[view], buckets, order[view]) {
                emit(&mut draws.opaque, item, base, records, pending_records);
            }
            let transparent = &mut transparent[view];
            transparent.sort_unstable_by(|a, b| {
                a.render_order
                    .cmp(&b.render_order)
                    .then(b.depth.total_cmp(&a.depth))
                    .then(a.class.cmp(&b.class))
            });
            for item in transparent.iter() {
                emit(&mut draws.transparent, item, base, records, pending_records);
            }
        }
    }
}

/// Append a sorted item's draw. A persistent range is a draw of its own; a record is
/// copied to `records`, which start at `base` in the instance buffer, and neighbours
/// in the sorted order that share a class draw as one instanced call: instances
/// rasterize in order, so the blend order is unchanged (tank bars, pickup glows).
fn emit(
    draws: &mut Vec<Draw>,
    item: &Item,
    base: u32,
    records: &mut Vec<InstanceRecord>,
    pending: &[InstanceRecord],
) {
    let (first_instance, instance_count) = match item.source {
        Source::Range { first, count } => (first, count),
        Source::Record(index) => {
            let at = base + records.len() as u32;
            records.push(pending[index as usize]);
            if let Some(last) = draws.last_mut()
                && last.class == item.class
                && last.first_instance + last.instance_count == at
            {
                last.instance_count += 1;
                return;
            }
            (at, 1)
        }
    };
    draws.push(Draw {
        class: item.class,
        first_instance,
        instance_count,
    });
}

/// Where an opaque item sorts within its render order: each class's records, then
/// its persistent ranges, class after class in `order`'s positions.
fn bucket(item: &Item, order: &[u32]) -> usize {
    2 * order[item.class as usize] as usize + matches!(item.source, Source::Range { .. }) as usize
}

/// A record's place within its bucket: records in record (push) order; ranges last.
fn rank(item: &Item) -> u32 {
    match item.source {
        Source::Record(index) => index,
        Source::Range { .. } => u32::MAX,
    }
}

/// Put a view's opaque items in draw order: by render order, then by their class's
/// position in `order`, each class's records in push order before its ranges (in push
/// order too), so a class's records are contiguous and become one instanced draw.
/// With one render order, the usual case, that is a counting sort over the buckets in
/// use ([`Buckets`]): linear in the items, where a comparison sort of thousands of
/// items a frame showed in WebGPU CPU profiles of the Stress Grid. Its result is in
/// the scratch. A list that mixes render orders (no opaque draw sets one today) takes
/// a stable comparison sort to the same order, in place.
fn sort_opaque<'a>(items: &'a mut [Item], scratch: &'a mut Buckets, order: &[u32]) -> &'a [Item] {
    let bucket = |item: &Item| bucket(item, order);
    let Some(&first) = items.first() else {
        return items;
    };
    if items
        .iter()
        .any(|item| item.render_order != first.render_order)
    {
        items.sort_by_key(|item| (item.render_order, bucket(item), rank(item)));
        return items;
    }
    let Buckets {
        counts,
        used,
        sorted,
    } = scratch;
    // Every class's position is below `order.len()`.
    let buckets = 2 * order.len();
    if counts.len() < buckets {
        counts.resize(buckets, 0);
        used.resize(buckets.div_ceil(64), 0);
    }
    for item in items.iter() {
        let at = bucket(item);
        counts[at] += 1;
        used[at / 64] |= 1u64 << (at % 64);
    }
    // Each bucket's count becomes the slot of its first item.
    let mut next = 0;
    Buckets::for_each_used(used, |at| {
        let count = counts[at];
        counts[at] = next;
        next += count;
    });
    sorted.clear();
    sorted.resize(items.len(), first);
    for item in items.iter() {
        let at = &mut counts[bucket(item)];
        sorted[*at as usize] = *item;
        *at += 1;
    }
    // Leave the scratch clear for the next view.
    Buckets::for_each_used(used, |at| counts[at] = 0);
    used.fill(0);
    sorted
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

    /// Every class binds the same state: opaque draws go by class.
    fn same_state(_view: usize, _class: u32) -> DrawState {
        DrawState::default()
    }

    /// Each view's class positions for `classes` live classes that bind
    /// `state(view, class)`.
    fn orders(classes: u32, state: impl Fn(usize, u32) -> DrawState) -> [Vec<u32>; VIEW_COUNT] {
        std::array::from_fn(|view| {
            let mut order = ClassOrder::default();
            for class in 0..classes {
                order.insert(class, state(view, class));
            }
            order.positions().to_vec()
        })
    }

    fn finish(
        builder: &mut DrawListBuilder,
        base: u32,
        views: &mut [ViewDraws; VIEW_COUNT],
        orders: &[Vec<u32>; VIEW_COUNT],
    ) {
        builder.finish(base, views, [&orders[0], &orders[1], &orders[2]]);
    }

    fn draws(list: &[Draw]) -> Vec<(u32, u32, u32)> {
        list.iter()
            .map(|d| (d.class, d.first_instance, d.instance_count))
            .collect()
    }

    #[test]
    fn records_keep_the_rows_of_an_affine_world() {
        // WGSL `expand_instance` rebuilds the world from three rows, translation in w.
        let world = Mat4::from_scale_rotation_translation(
            glam::Vec3::new(1.0, 2.0, 0.5),
            glam::Quat::from_rotation_y(0.7),
            glam::Vec3::new(3.0, -1.0, 8.0),
        );
        let record = InstanceRecord::new(&world, [1.0; 4], [0.0; 4]);
        assert_eq!(size_of_val(&record), 80);
        assert_eq!(record.world_rows[1], world.row(1).to_array());
        assert_eq!(record.translation(), glam::Vec3::new(3.0, -1.0, 8.0));
        assert!(record.world().abs_diff_eq(world, 1e-6));
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
        finish(&mut builder, 10, &mut views, &orders(512, same_state));
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
        assert_eq!(builder.records[0].translation().x, 1.0);
        assert_eq!(builder.records[1].translation().x, 3.0);
    }

    /// Main view: odd classes use pipeline 0 and even ones pipeline 1; class 4 draws
    /// from pool 7. The shadow view has its own pipelines: every class shares
    /// pipeline 0 except class 1.
    fn two_pipelines(view: usize, class: u32) -> DrawState {
        let material = [5, 9, 2, 9, 9, 5][class as usize];
        let pipeline = if view == SHADOW_VIEW {
            u32::from(class == 1)
        } else {
            1 - class % 2
        };
        DrawState {
            pipeline,
            pool: if class == 4 { 7 } else { u32::MAX },
            material,
            ..DrawState::default()
        }
    }

    #[test]
    fn opaque_draws_group_by_pipeline_material_and_pool() {
        let mut builder = DrawListBuilder::default();
        let mut views: [ViewDraws; VIEW_COUNT] = Default::default();
        for (i, class) in [5, 0, 3, 1, 2, 3, 0, 1, 5, 2].into_iter().enumerate() {
            let r = builder.record(record(i as f32));
            builder.push(MAIN_VIEW, class, false, 0, 0.0, r);
            builder.push(SHADOW_VIEW, class, false, 0, 0.0, r);
        }
        // A pool's range, and an earlier render order that still goes first.
        builder.push_range(MAIN_VIEW, 4, false, 0, 0.0, 0, 50);
        let r = builder.record(record(10.0));
        builder.push(MAIN_VIEW, 0, false, -1, 0.0, r);
        finish(&mut builder, 100, &mut views, &orders(6, two_pipelines));
        // Pipeline 0: material 5 (class 5), material 9 (classes 1, 3); pipeline 1:
        // material 2 (class 2), material 5 (class 0), material 9 (the pool's class 4).
        // Each class stays one instanced draw over contiguous records.
        assert_eq!(
            draws(&views[MAIN_VIEW].opaque),
            [
                (0, 100, 1),
                (5, 101, 2),
                (1, 103, 2),
                (3, 105, 2),
                (2, 107, 2),
                (0, 109, 2),
                (4, 0, 50),
            ]
        );
        // Records follow draw order: class 1 drew instances 3 and 7.
        assert_eq!(builder.records[0].translation().x, 10.0);
        assert_eq!(builder.records[3].translation().x, 3.0);
        assert_eq!(builder.records[4].translation().x, 7.0);
        assert_eq!(
            draws(&views[SHADOW_VIEW].opaque),
            [
                (2, 111, 2),
                (0, 113, 2),
                (5, 115, 2),
                (3, 117, 2),
                (1, 119, 2),
            ]
        );
        assert_eq!(builder.records.len(), 21);
    }

    #[test]
    fn opaque_draws_group_by_material_then_mesh_page_within_the_pipeline() {
        // Classes 0-5 in pipelines [0, 0, 1, 0, 0, 1], vertex pages [2, 1, 1, 2, 1, 0],
        // materials [3, 4, 3, 3, 3, 4] and index pages [0, 1, 0, 0, 0, 0].
        let state = |_view: usize, class: u32| {
            let class = class as usize;
            DrawState {
                pipeline: [0, 0, 1, 0, 0, 1][class],
                vertex_page: [2, 1, 1, 2, 1, 0][class],
                pool: u32::MAX,
                material: [3, 4, 3, 3, 3, 4][class],
                index_page: [0, 1, 0, 0, 0, 0][class],
            }
        };
        let mut builder = DrawListBuilder::default();
        let mut views: [ViewDraws; VIEW_COUNT] = Default::default();
        for (i, class) in [0, 1, 2, 3, 4, 5, 4, 3, 2, 1, 0].into_iter().enumerate() {
            let r = builder.record(record(i as f32));
            builder.push(MAIN_VIEW, class, false, 0, 0.0, r);
        }
        // An earlier render order still goes first, whatever its page.
        let r = builder.record(record(11.0));
        builder.push(MAIN_VIEW, 5, false, -1, 0.0, r);
        // Transparent draws keep depth order, switching pages back and forth.
        for (class, depth) in [(5, 1.0), (0, 2.0), (5, 3.0)] {
            let r = builder.record(record(depth));
            builder.push(MAIN_VIEW, class, true, 0, depth, r);
        }
        finish(&mut builder, 0, &mut views, &orders(6, state));
        // Pipeline 0: material 3 (page 1: class 4; page 2: classes 0 and 3, by index),
        // then material 4 (class 1); pipeline 1: material 3 (class 2), then material 4
        // (class 5). Each class stays one draw over contiguous records.
        assert_eq!(
            draws(&views[MAIN_VIEW].opaque),
            [
                (5, 0, 1),
                (4, 1, 2),
                (0, 3, 2),
                (3, 5, 2),
                (1, 7, 2),
                (2, 9, 2),
                (5, 11, 1),
            ]
        );
        let transparent: Vec<u32> = views[MAIN_VIEW]
            .transparent
            .iter()
            .map(|d| d.class)
            .collect();
        assert_eq!(transparent, [5, 0, 5]);
        // The index page only breaks ties: within one pipeline, material and vertex
        // page, a class on a later index page goes after the others.
        let mut order = ClassOrder::default();
        for (class, index_page) in [1, 0, 1].into_iter().enumerate() {
            let state = DrawState {
                index_page,
                ..DrawState::default()
            };
            order.insert(class as u32, state);
        }
        assert_eq!(order.positions(), [1, 0, 2]);
    }

    #[test]
    fn page_first_grouping_orders_by_vertex_page_before_material() {
        // Classes 0-3 in one pipeline: (vertex page, material) = (1, 0), (0, 1),
        // (0, 0), (1, 1).
        let states = [(1, 0), (0, 1), (0, 0), (1, 1)].map(|(vertex_page, material)| DrawState {
            vertex_page,
            material,
            ..DrawState::default()
        });
        let positions = |grouping| {
            let mut order = ClassOrder::new(grouping);
            for (class, state) in states.iter().enumerate() {
                order.insert(class as u32, *state);
            }
            order.positions().to_vec()
        };
        assert_eq!(positions(Grouping::PageFirst), [2, 1, 0, 3]);
        assert_eq!(positions(Grouping::MaterialFirst), [1, 2, 0, 3]);
    }

    #[test]
    fn classes_order_by_state_then_index_as_they_come_and_go() {
        let state = |pipeline| DrawState {
            pipeline,
            ..DrawState::default()
        };
        let mut order = ClassOrder::default();
        for (class, pipeline) in [(3, 1), (0, 2), (1, 9), (2, 1), (4, 2)] {
            order.insert(class, state(pipeline));
        }
        assert_eq!(order.positions(), [2, 4, 0, 1, 3]);
        // Class 1 goes and its slot comes back with another state; class 3 goes.
        order.remove(1);
        order.remove(3);
        order.insert(1, state(0));
        let live = [0, 1, 2, 4];
        let positions: Vec<_> = live.iter().map(|&class| order.positions()[class]).collect();
        assert_eq!(positions, [2, 0, 1, 3]);
    }

    #[test]
    fn class_order_matches_a_sort_of_the_live_classes() {
        let mut random = crate::effects::random::CosmeticRandom::seeded(11);
        let mut pick = |n: f64| (random.next_f64() * n) as u32;
        let grouping = Grouping::PageFirst;
        let mut order = ClassOrder::new(grouping);
        let mut live: Vec<Option<DrawState>> = vec![None; 60];
        for _ in 0..2000 {
            let class = pick(60.0);
            match live[class as usize] {
                Some(_) => {
                    order.remove(class);
                    live[class as usize] = None;
                }
                None => {
                    let state = DrawState {
                        pipeline: pick(4.0),
                        material: pick(6.0),
                        vertex_page: pick(3.0),
                        pool: pick(2.0),
                        index_page: pick(3.0),
                    };
                    order.insert(class, state);
                    live[class as usize] = Some(state);
                }
            }
            let mut expected: Vec<([u32; 5], u32)> = (0..60)
                .filter_map(|class| live[class as usize].map(|state| (state.key(grouping), class)))
                .collect();
            expected.sort();
            for (position, (_, class)) in expected.into_iter().enumerate() {
                assert_eq!(order.positions()[class as usize], position as u32);
            }
        }
    }

    #[test]
    fn transparent_draws_sort_by_order_then_back_to_front() {
        let mut builder = DrawListBuilder::default();
        let mut views: [ViewDraws; VIEW_COUNT] = Default::default();
        for (class, order, depth) in [(1, 0, 5.0), (2, 0, 50.0), (3, -1, 1.0), (4, 0, 20.0)] {
            let r = builder.record(record(depth));
            builder.push(MAIN_VIEW, class, true, order, depth, r);
        }
        finish(&mut builder, 0, &mut views, &orders(512, same_state));
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
        finish(&mut builder, 0, &mut views, &orders(512, same_state));
        let draws: Vec<_> = views[MAIN_VIEW]
            .transparent
            .iter()
            .map(|d| (d.class, d.instance_count))
            .collect();
        assert_eq!(draws, [(5, 2), (6, 1), (5, 1)]);
        builder.clear();
        finish(&mut builder, 0, &mut views, &orders(512, same_state));
        assert!(views[MAIN_VIEW].opaque.is_empty() && views[MAIN_VIEW].transparent.is_empty());
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
        finish(&mut builder, 20, &mut views, &orders(512, same_state));
        assert_eq!(
            draws(&views[MAIN_VIEW].opaque),
            [(0, 20, 2), (0, 0, 8), (1, 22, 2)]
        );
        let xs: Vec<_> = builder.records.iter().map(|r| r.translation().x).collect();
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
        finish(&mut builder, 200, &mut views, &orders(512, same_state));
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
        let xs: Vec<_> = builder.records.iter().map(|r| r.translation().x).collect();
        assert_eq!(xs, [2.0, 5.0, 1.0, 4.0, 0.0, 3.0]);
    }

    #[test]
    fn opaque_order_matches_a_stable_comparison_sort() {
        // Both paths: one render order (the counting sort) and mixed render orders
        // (the comparison sort), with many ranges per class. One scratch serves every
        // round, as one serves every view: class counts grow and shrink between rounds,
        // and a sort must leave nothing behind for the next.
        let mut random = crate::effects::random::CosmeticRandom::seeded(7);
        let mut pick = |n: f64| (random.next_f64() * n) as u32;
        // What an item is: its class, whether it is a range, and its record or first.
        let identity = |item: &Item| match item.source {
            Source::Record(index) => (item.class, false, index),
            Source::Range { first, .. } => (item.class, true, first),
        };
        let mut scratch = Buckets::default();
        for round in 0..40 {
            let orders = if round % 2 == 0 { 1.0 } else { 3.0 };
            let classes = [40, 700, 9, 130][round / 2 % 4];
            let len = [300, 5, 1000][round % 3];
            // Classes in reverse positions, so the order is not just the class index.
            let order: Vec<u32> = (0..classes).rev().collect();
            let mut items: Vec<Item> = (0..len)
                .map(|index| Item {
                    render_order: pick(orders) as i32 - 1,
                    class: pick(f64::from(classes)),
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
            expected.sort_by_key(|item| (item.render_order, bucket(item, &order), rank(item)));
            let got: Vec<_> = sort_opaque(&mut items, &mut scratch, &order)
                .iter()
                .map(identity)
                .collect();
            let expected: Vec<_> = expected.iter().map(identity).collect();
            assert_eq!(got, expected);
            assert!(scratch.counts.iter().all(|&count| count == 0));
            assert!(scratch.used.iter().all(|&bits| bits == 0));
        }
    }
}
