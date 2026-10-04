//! Shared mesh pages, the CPU side: which large GPU buffer ("page") each mesh's
//! vertices and indices live in, and where.
//!
//! Meshes with one vertex layout share vertex pages, and every mesh's indices share
//! index pages. Indices are written absolute (already offset by the mesh's first
//! vertex in its page), so every draw passes `base_vertex` 0, which WebGL2 lacks, and
//! a mesh's draw is just an index range in its index page. Consecutive draws from one
//! page keep their vertex and index bindings, where a buffer per mesh made every mesh
//! switch two or three WebGPU commands (`setVertexBuffer`, `setIndexBuffer`), and
//! through wgpu's GL backend a re-specification of every vertex attribute.
//!
//! This module is pure bookkeeping, so it compiles natively and carries the tests:
//! a first-fit [`RangeAllocator`] per page, the [`PagePlanner`] that picks pages and
//! decides when one is created or destroyed, and the index rebasing. The renderer
//! (`gpu/resources.rs`) owns the buffers and mirrors the planner's pages.
//!
//! Pages never grow, move or copy: a fixed page holds one copy of its data, where a
//! buffer that doubles would briefly hold two, and on WebGL index data can only be
//! copied between index buffers. Freed ranges are reused as soon as the mesh is
//! freed. Earlier frames may still be in flight then, which queue order makes safe:
//! WebGPU runs `writeBuffer` after the submissions before it, and GL runs commands in
//! order.

use std::ops::Range;

use crate::model::Vertex;
use crate::shadow_merge::ShadowVertex;

const MIB: u64 = 1 << 20;

/// Vertex data a general vertex page holds, effect vec4s included: 175k `Vertex` or
/// 350k `ShadowVertex`. A larger page leaves more of the newest one as slack; a
/// smaller one means more pages, and every further page costs a binding switch per
/// pass that draws from it.
pub const GENERAL_VERTEX_PAGE_BYTES: u64 = 8 * MIB;

/// Indices a general index page holds: 524k. Index data is about an eighth of the
/// mesh bytes, so this pairs with a general vertex page.
pub const GENERAL_INDEX_PAGE_BYTES: u64 = 2 * MIB;

/// One registration whose meshes of a vertex family reach half a general page gets
/// exact-size batch pages of its own instead of filling general ones. That is themed
/// scenery above all, one model of tens of MB per theme: it neither crowds the
/// general pages nor, if it is ever released, leaves them as slack. One mesh larger
/// than this gets an exact own page ([`PagePlanner::place`]), so a general page
/// always takes at least two meshes.
pub const BATCH_PAGE_MIN_BYTES: u64 = GENERAL_VERTEX_PAGE_BYTES / 2;

/// The same for a registration's indices: half a general index page.
pub const BATCH_INDEX_PAGE_MIN_BYTES: u64 = GENERAL_INDEX_PAGE_BYTES / 2;

/// The largest batch page. It stays below the 128 MiB per-resource floor of D3D11,
/// which ANGLE uses for WebGL on Windows, and WebGPU's default 256 MiB
/// `max_buffer_size` (`context.rs` asks for the default limits). Batches split at
/// this size; a mesh never straddles pages, and one larger than this gets an exact
/// page of its own, like the buffer it had before pages.
pub const MAX_PAGE_BYTES: u64 = 64 * MIB;

/// The page of an empty mesh range, which binds nothing and draws nothing.
pub const NO_PAGE: u16 = u16::MAX;

/// What a page holds. Each family has its own pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PageFamily {
    /// The surface `Vertex` (48 B), with `extra` effect vec4s per vertex in a second
    /// buffer of the same vertex numbering.
    Surface { extra: u8 },
    /// The merged-shadow `ShadowVertex` (24 B).
    Shadow,
    /// `u32` indices of every family, absolute in their mesh's vertex page.
    Index,
}

impl PageFamily {
    /// Bytes per element of the page's main buffer.
    pub fn stride(self) -> u64 {
        match self {
            Self::Surface { .. } => size_of::<Vertex>() as u64,
            Self::Shadow => size_of::<ShadowVertex>() as u64,
            Self::Index => size_of::<u32>() as u64,
        }
    }

    /// Bytes per element of its effect attribute buffer; 0 when it has none.
    pub fn extra_stride(self) -> u64 {
        match self {
            Self::Surface { extra } => u64::from(extra) * size_of::<[f32; 4]>() as u64,
            Self::Shadow | Self::Index => 0,
        }
    }

    /// Bytes per element over both buffers.
    pub fn element_bytes(self) -> u64 {
        self.stride() + self.extra_stride()
    }

    fn general_page_bytes(self) -> u64 {
        match self {
            Self::Index => GENERAL_INDEX_PAGE_BYTES,
            Self::Surface { .. } | Self::Shadow => GENERAL_VERTEX_PAGE_BYTES,
        }
    }

    /// Elements in a general page.
    pub fn general_capacity(self) -> u32 {
        (self.general_page_bytes() / self.element_bytes()) as u32
    }

    /// Elements in the largest batch page.
    fn max_page_capacity(self) -> u32 {
        (MAX_PAGE_BYTES / self.element_bytes()) as u32
    }

    /// Half a general page ([`BATCH_PAGE_MIN_BYTES`], [`BATCH_INDEX_PAGE_MIN_BYTES`]):
    /// one mesh larger than this gets an own page, and one registration's meshes that
    /// add up to it get batch pages.
    fn large_bytes(self) -> u64 {
        match self {
            Self::Index => BATCH_INDEX_PAGE_MIN_BYTES,
            Self::Surface { .. } | Self::Shadow => BATCH_PAGE_MIN_BYTES,
        }
    }
}

/// A mesh as the pages see it: its vertex family and its vertex and index counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshSize {
    pub family: PageFamily,
    pub vertices: u32,
    pub indices: u32,
}

impl MeshSize {
    /// A mesh without vertices or indices draws nothing and gets no page
    /// ([`MeshRange::EMPTY`]). Reservation and upload both ask this, so a reserved
    /// range is always written.
    pub fn is_drawable(self) -> bool {
        self.vertices > 0 && self.indices > 0
    }
}

/// Whether one registration's meshes of `family` (their element counts) are big
/// enough for batch pages ([`PagePlanner::place_batch`]).
pub fn wants_batch(family: PageFamily, counts: impl IntoIterator<Item = u32>) -> bool {
    let elements: u64 = counts.into_iter().map(u64::from).sum();
    elements * family.element_bytes() >= family.large_bytes()
}

// ------------------------------------------------------------------ allocator

/// First-fit ranges of elements (vertices or indices) over `[0, capacity)`.
/// Counting in elements keeps every range aligned: vertices start at a multiple of
/// their stride, and 48, 24, 16 and 4 bytes are all multiples of WebGPU's 4-byte
/// copy alignment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RangeAllocator {
    capacity: u32,
    /// Free `(start, end)` ranges in address order. Frees coalesce, so no two touch.
    free: Vec<(u32, u32)>,
}

impl RangeAllocator {
    pub fn new(capacity: u32) -> Self {
        Self {
            capacity,
            free: if capacity > 0 {
                vec![(0, capacity)]
            } else {
                Vec::new()
            },
        }
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    /// The lowest-addressed `count` free elements in one piece, if any. Taking the
    /// lowest hole keeps live data low, and a round that frees and uploads the same
    /// sizes again fills the same holes.
    pub fn allocate(&mut self, count: u32) -> Option<u32> {
        debug_assert!(count > 0, "allocate at least one element");
        let index = self
            .free
            .iter()
            .position(|&(start, end)| end - start >= count)?;
        let (start, end) = self.free[index];
        if end - start == count {
            self.free.remove(index);
        } else {
            self.free[index].0 = start + count;
        }
        Some(start)
    }

    /// Return `[start, start + count)`, which must be allocated, merging it with the
    /// free ranges it touches.
    pub fn free(&mut self, start: u32, count: u32) {
        if count == 0 {
            return;
        }
        let end = start + count;
        debug_assert!(end <= self.capacity, "free past the end");
        let index = self
            .free
            .partition_point(|&(_, free_end)| free_end <= start);
        debug_assert!(
            self.free.get(index).is_none_or(|&(next, _)| end <= next),
            "free of a range that is not allocated"
        );
        let joins_left = index > 0 && self.free[index - 1].1 == start;
        let joins_right = self.free.get(index).is_some_and(|&(next, _)| next == end);
        match (joins_left, joins_right) {
            (true, true) => {
                self.free[index - 1].1 = self.free[index].1;
                self.free.remove(index);
            }
            (true, false) => self.free[index - 1].1 = end,
            (false, true) => self.free[index].0 = start,
            (false, false) => self.free.insert(index, (start, end)),
        }
    }

    /// Elements free now.
    pub fn free_elements(&self) -> u32 {
        self.free.iter().map(|&(start, end)| end - start).sum()
    }

    /// The free ranges, address-ordered.
    pub fn free_ranges(&self) -> &[(u32, u32)] {
        &self.free
    }
}

// ------------------------------------------------------------------ planner

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageKind {
    /// Shared by meshes of any lifetime, first-fit. Kept when it empties until the
    /// next [`PagePlanner::trim`]: a model rebuilt in place or the next round refills
    /// it, and a page nothing refilled goes.
    General,
    /// Exact size for one large registration's meshes; destroyed when the last of
    /// them is freed, which returns its GPU memory.
    Batch,
    /// Exact size for one mesh larger than half a general page; destroyed with it.
    Own,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub family: PageFamily,
    pub kind: PageKind,
    ranges: RangeAllocator,
    /// Elements allocated now.
    live: u32,
    /// The high-water mark of allocations: first-fit only ever extends the
    /// allocated span upward, so every element below it has been allocated, and so
    /// written, at least once.
    written: u32,
}

impl Page {
    fn new(family: PageFamily, kind: PageKind, capacity: u32) -> Self {
        Self {
            family,
            kind,
            ranges: RangeAllocator::new(capacity),
            live: 0,
            written: 0,
        }
    }

    pub fn capacity(&self) -> u32 {
        self.ranges.capacity()
    }

    /// Elements allocated now.
    pub fn live(&self) -> u32 {
        self.live
    }

    /// `[0, written)` has been written; draws bind only that prefix (see
    /// `MeshStore::vertex_buffers`).
    pub fn written(&self) -> u32 {
        self.written
    }

    pub fn ranges(&self) -> &RangeAllocator {
        &self.ranges
    }

    fn allocate(&mut self, count: u32) -> Option<u32> {
        let first = self.ranges.allocate(count)?;
        self.live += count;
        self.written = self.written.max(first + count);
        Some(first)
    }
}

/// Where a mesh's vertices or indices go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub page: u16,
    pub first: u32,
    /// The placement created its page: the caller creates the page's buffers, of
    /// [`Page::capacity`] elements, before writing.
    pub new_page: bool,
}

/// Where one mesh of a registration goes when [`PagePlanner::reserve`] gave it batch
/// pages; `None` leaves that part to a general or own page as it uploads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeshPlacement {
    pub vertex: Option<Placement>,
    pub index: Option<Placement>,
}

/// The pages of every family, by page id. Ids are reused after a page is destroyed,
/// so they stay small and a draw can key on them.
#[derive(Clone, Debug, Default)]
pub struct PagePlanner {
    pages: Vec<Option<Page>>,
}

impl PagePlanner {
    pub fn page(&self, page: u16) -> &Page {
        self.pages[page as usize].as_ref().expect("live mesh page")
    }

    /// Live pages and their ids.
    pub fn pages(&self) -> impl Iterator<Item = (u16, &Page)> {
        self.pages
            .iter()
            .enumerate()
            .filter_map(|(id, page)| page.as_ref().map(|page| (id as u16, page)))
    }

    fn create(&mut self, family: PageFamily, kind: PageKind, capacity: u32) -> u16 {
        let page = Some(Page::new(family, kind, capacity));
        let id = match self.pages.iter().position(Option::is_none) {
            Some(id) => {
                self.pages[id] = page;
                id
            }
            None => {
                self.pages.push(page);
                self.pages.len() - 1
            }
        };
        u16::try_from(id)
            .ok()
            .filter(|&id| id != NO_PAGE)
            .expect("fewer than 65535 mesh pages")
    }

    fn page_mut(&mut self, page: u16) -> &mut Page {
        self.pages[page as usize].as_mut().expect("live mesh page")
    }

    /// Place `count` (> 0) elements of `family`: in an own page when they fill more
    /// than half a general page, otherwise first-fit across the family's general
    /// pages in page order, adding a general page only when none has room.
    pub fn place(&mut self, family: PageFamily, count: u32) -> Placement {
        debug_assert!(count > 0, "place at least one element");
        if u64::from(count) * family.element_bytes() > family.large_bytes() {
            let page = self.create(family, PageKind::Own, count);
            let first = self.page_mut(page).allocate(count).expect("exact page");
            return Placement {
                page,
                first,
                new_page: true,
            };
        }
        for (id, slot) in self.pages.iter_mut().enumerate() {
            if let Some(page) = slot
                && page.family == family
                && page.kind == PageKind::General
                && let Some(first) = page.allocate(count)
            {
                return Placement {
                    page: id as u16,
                    first,
                    new_page: false,
                };
            }
        }
        let page = self.create(family, PageKind::General, family.general_capacity());
        let first = self
            .page_mut(page)
            .allocate(count)
            .expect("a new general page has room");
        Placement {
            page,
            first,
            new_page: true,
        }
    }

    /// Place one registration's meshes of `family` (element counts, each > 0) in
    /// order in new exact-size batch pages, starting a page whenever the next mesh
    /// would take it past [`MAX_PAGE_BYTES`]. A mesh never straddles pages.
    pub fn place_batch(&mut self, family: PageFamily, counts: &[u32]) -> Vec<Placement> {
        let max = u64::from(family.max_page_capacity());
        let mut placements = Vec::with_capacity(counts.len());
        let mut start = 0;
        while start < counts.len() {
            debug_assert!(counts[start] > 0, "place at least one element");
            let mut end = start + 1;
            let mut total = u64::from(counts[start]);
            while end < counts.len() && total + u64::from(counts[end]) <= max {
                total += u64::from(counts[end]);
                end += 1;
            }
            let page = self.create(family, PageKind::Batch, total as u32);
            for (index, &count) in counts[start..end].iter().enumerate() {
                let first = self.page_mut(page).allocate(count).expect("exact page");
                placements.push(Placement {
                    page,
                    first,
                    new_page: index == 0,
                });
            }
            start = end;
        }
        placements
    }

    /// Give one registration's meshes, in upload order, batch pages
    /// ([`place_batch`](Self::place_batch)): for each vertex family whose drawable
    /// meshes add up to a batch ([`wants_batch`]), and for all their indices if those
    /// do. The whole registration is known before anything uploads, so a large one
    /// (themed scenery above all) neither fills general pages nor leaves them as slack
    /// when it goes. Returns each mesh's placements in the same order; an undrawable
    /// mesh gets none.
    pub fn reserve(&mut self, meshes: &[MeshSize]) -> Vec<MeshPlacement> {
        let drawable: Vec<usize> = (0..meshes.len())
            .filter(|&mesh| meshes[mesh].is_drawable())
            .collect();
        let mut placements = vec![MeshPlacement::default(); meshes.len()];
        let mut families: Vec<PageFamily> = Vec::new();
        for &mesh in &drawable {
            if !families.contains(&meshes[mesh].family) {
                families.push(meshes[mesh].family);
            }
        }
        for family in families {
            let members: Vec<usize> = drawable
                .iter()
                .copied()
                .filter(|&mesh| meshes[mesh].family == family)
                .collect();
            let counts: Vec<u32> = members.iter().map(|&mesh| meshes[mesh].vertices).collect();
            if wants_batch(family, counts.iter().copied()) {
                let batch = self.place_batch(family, &counts);
                for (mesh, placement) in members.into_iter().zip(batch) {
                    placements[mesh].vertex = Some(placement);
                }
            }
        }
        let counts: Vec<u32> = drawable.iter().map(|&mesh| meshes[mesh].indices).collect();
        if wants_batch(PageFamily::Index, counts.iter().copied()) {
            let batch = self.place_batch(PageFamily::Index, &counts);
            for (&mesh, placement) in drawable.iter().zip(batch) {
                placements[mesh].index = Some(placement);
            }
        }
        placements
    }

    /// Free `count` elements at `first` of `page`. Returns whether that destroyed the
    /// page (an emptied batch or own page); the caller destroys its buffers.
    pub fn free(&mut self, page: u16, first: u32, count: u32) -> bool {
        let slot = &mut self.pages[page as usize];
        let entry = slot.as_mut().expect("live mesh page");
        entry.ranges.free(first, count);
        entry.live -= count;
        if entry.live == 0 && entry.kind != PageKind::General {
            *slot = None;
            return true;
        }
        false
    }

    /// Destroy the general pages no mesh uses and return their ids; the caller
    /// destroys their buffers. The renderer trims once a frame, after the frame's
    /// frees and uploads: a round reset frees the old round and uploads the new one
    /// in between, so the new round refills the pages the old one emptied, and no page
    /// a round, a map or a mid-round removal left empty outlives the next frame.
    pub fn trim(&mut self) -> Vec<u16> {
        let mut trimmed = Vec::new();
        for (id, slot) in self.pages.iter_mut().enumerate() {
            if slot
                .as_ref()
                .is_some_and(|page| page.kind == PageKind::General && page.live == 0)
            {
                *slot = None;
                trimmed.push(id as u16);
            }
        }
        trimmed
    }

    /// GPU bytes the pages reserve.
    pub fn capacity_bytes(&self) -> u64 {
        self.pages()
            .map(|(_, page)| u64::from(page.capacity()) * page.family.element_bytes())
            .sum()
    }

    /// Bytes of the meshes the pages hold.
    pub fn live_bytes(&self) -> u64 {
        self.pages()
            .map(|(_, page)| u64::from(page.live) * page.family.element_bytes())
            .sum()
    }

    /// GPU buffers: one per page, plus the effect attribute buffer of surface pages
    /// with effect vec4s.
    pub fn buffer_count(&self) -> usize {
        self.pages()
            .map(|(_, page)| 1 + (page.family.extra_stride() > 0) as usize)
            .sum()
    }
}

// ------------------------------------------------------------------ meshes

/// Where a mesh lives: its vertices in a vertex page and its absolute indices in an
/// index page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshRange {
    pub vertex_page: u16,
    pub first_vertex: u32,
    pub vertex_count: u32,
    pub index_page: u16,
    pub first_index: u32,
    pub index_count: u32,
}

impl MeshRange {
    /// A mesh without vertices or indices: no page, nothing to draw.
    pub const EMPTY: MeshRange = MeshRange {
        vertex_page: NO_PAGE,
        first_vertex: 0,
        vertex_count: 0,
        index_page: NO_PAGE,
        first_index: 0,
        index_count: 0,
    };

    /// The `draw_indexed` index range, with `base_vertex` 0.
    pub fn indices(&self) -> Range<u32> {
        self.first_index..self.first_index + self.index_count
    }

    pub fn is_empty(&self) -> bool {
        self.index_count == 0
    }
}

/// Make a mesh's indices absolute in its vertex page, in place: add the page vertex
/// its first vertex sits at.
pub fn rebase_indices(indices: &mut [u32], first_vertex: u32) {
    if first_vertex != 0 {
        for index in indices {
            *index += first_vertex;
        }
    }
}

/// Stream a shared mesh's absolute indices to `write` a chunk at a time, with each
/// chunk's offset among the mesh's indices. The mesh is immutable, so a rebased chunk
/// is built in `scratch`, whose length is the chunk size; with `first_vertex` 0 the
/// mesh's own indices go out uncopied, in chunks of the same size. `indices: None` is
/// a non-indexed mesh, drawn as vertices `0..vertex_count`.
pub fn stream_shared_indices(
    indices: Option<&[u32]>,
    vertex_count: u32,
    first_vertex: u32,
    scratch: &mut [u32],
    mut write: impl FnMut(u32, &[u32]),
) {
    let chunk = scratch.len();
    assert!(chunk > 0, "stream through a scratch of at least one index");
    match indices {
        Some(indices) => {
            for (number, piece) in indices.chunks(chunk).enumerate() {
                let offset = (number * chunk) as u32;
                if first_vertex == 0 {
                    write(offset, piece);
                } else {
                    let rebased = &mut scratch[..piece.len()];
                    for (out, &index) in rebased.iter_mut().zip(piece) {
                        *out = index + first_vertex;
                    }
                    write(offset, rebased);
                }
            }
        }
        None => {
            for start in (0..vertex_count).step_by(chunk) {
                let end = (start + chunk as u32).min(vertex_count);
                let rebased = &mut scratch[..(end - start) as usize];
                for (out, vertex) in rebased.iter_mut().zip(first_vertex + start..) {
                    *out = vertex;
                }
                write(start, rebased);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::random::CosmeticRandom;

    const SURFACE: PageFamily = PageFamily::Surface { extra: 0 };

    #[test]
    fn allocation_is_first_fit_and_exact_fits_remove_the_hole() {
        let mut ranges = RangeAllocator::new(100);
        assert_eq!(ranges.allocate(10), Some(0));
        assert_eq!(ranges.allocate(20), Some(10));
        assert_eq!(ranges.allocate(30), Some(30));
        ranges.free(10, 20);
        assert_eq!(ranges.free_ranges(), [(10, 30), (60, 100)]);
        // The first hole that fits, not the best one.
        assert_eq!(ranges.allocate(5), Some(10));
        assert_eq!(ranges.allocate(40), Some(60));
        assert_eq!(ranges.free_ranges(), [(15, 30)]);
        // An exact fit removes the hole.
        assert_eq!(ranges.allocate(15), Some(15));
        assert_eq!(ranges.free_ranges(), []);
        assert_eq!(ranges.allocate(1), None);
    }

    #[test]
    fn frees_coalesce_with_the_left_the_right_and_both_neighbours() {
        let mut ranges = RangeAllocator::new(40);
        for expected in [0, 10, 20, 30] {
            assert_eq!(ranges.allocate(10), Some(expected));
        }
        ranges.free(0, 10);
        ranges.free(20, 10);
        assert_eq!(ranges.free_ranges(), [(0, 10), (20, 30)]);
        // Right neighbour: 10..20 joins 20..30 and then the left one.
        ranges.free(10, 10);
        assert_eq!(ranges.free_ranges(), [(0, 30)]);
        // Left neighbour.
        ranges.free(30, 10);
        assert_eq!(ranges.free_ranges(), [(0, 40)]);
        assert_eq!(ranges.free_elements(), 40);
        // Right only.
        assert_eq!(ranges.allocate(40), Some(0));
        ranges.free(35, 5);
        ranges.free(30, 5);
        assert_eq!(ranges.free_ranges(), [(30, 40)]);
    }

    #[test]
    fn exhaustion_returns_none_and_freeing_everything_leaves_one_range() {
        let mut ranges = RangeAllocator::new(64);
        let firsts: Vec<u32> = (0..8).map(|_| ranges.allocate(8).unwrap()).collect();
        assert_eq!(ranges.allocate(1), None);
        for &first in firsts.iter().rev().step_by(2) {
            ranges.free(first, 8);
        }
        assert_eq!(ranges.allocate(9), None, "no hole holds 9");
        for &first in firsts.iter().step_by(2) {
            ranges.free(first, 8);
        }
        assert_eq!(ranges.free_ranges(), [(0, 64)]);
    }

    #[test]
    fn random_allocations_and_frees_match_a_reference_model() {
        const CAPACITY: u32 = 2048;
        let mut random = CosmeticRandom::seeded(56);
        let mut ranges = RangeAllocator::new(CAPACITY);
        // The reference: one flag per element and its own first-fit scan.
        let mut used = vec![false; CAPACITY as usize];
        let mut live: Vec<(u32, u32)> = Vec::new();
        let reference_fit = |used: &[bool], count: u32| {
            let count = count as usize;
            (0..=used.len().saturating_sub(count))
                .find(|&start| used[start..start + count].iter().all(|&taken| !taken))
                .map(|start| start as u32)
        };
        for step in 0..10_000 {
            if live.is_empty() || random.next_f64() < 0.55 {
                // Mostly small sizes, now and then a large one.
                let count = if random.next_f64() < 0.1 {
                    1 + (random.next_f64() * 400.0) as u32
                } else {
                    1 + (random.next_f64() * 40.0) as u32
                };
                let got = ranges.allocate(count);
                assert_eq!(got, reference_fit(&used, count), "step {step}");
                if let Some(first) = got {
                    used[first as usize..(first + count) as usize].fill(true);
                    live.push((first, count));
                }
            } else {
                let (first, count) =
                    live.swap_remove((random.next_f64() * live.len() as f64) as usize);
                ranges.free(first, count);
                used[first as usize..(first + count) as usize].fill(false);
            }
            // Totals are conserved, and the free list is exactly the free runs,
            // coalesced and in order.
            let mut runs = Vec::new();
            let mut start = None;
            for (index, &taken) in used.iter().chain([&true]).enumerate() {
                match (taken, start) {
                    (false, None) => start = Some(index as u32),
                    (true, Some(begin)) => {
                        runs.push((begin, index as u32));
                        start = None;
                    }
                    _ => {}
                }
            }
            assert_eq!(ranges.free_ranges(), runs, "step {step}");
            let allocated: u32 = live.iter().map(|&(_, count)| count).sum();
            assert_eq!(ranges.free_elements() + allocated, CAPACITY);
        }
    }

    #[test]
    fn small_meshes_share_general_pages_first_fit_in_page_order() {
        let mut planner = PagePlanner::default();
        let half = SURFACE.general_capacity() / 2;
        let a = planner.place(SURFACE, half);
        let b = planner.place(SURFACE, half);
        assert_eq!((a.page, a.first, a.new_page), (0, 0, true));
        assert_eq!((b.page, b.first, b.new_page), (0, half, false));
        // Full: the next mesh opens a second general page.
        let c = planner.place(SURFACE, 100);
        assert_eq!((c.page, c.first, c.new_page), (1, 0, true));
        // A hole in the older page is filled before the newer page.
        assert!(!planner.free(0, a.first, half));
        let d = planner.place(SURFACE, 10);
        assert_eq!((d.page, d.first), (0, 0));
        // Families never share pages.
        let shadow = planner.place(PageFamily::Shadow, 10);
        let indices = planner.place(PageFamily::Index, 10);
        assert_eq!((shadow.page, indices.page), (2, 3));
        assert_eq!(planner.page(2).family, PageFamily::Shadow);
        assert_eq!(planner.page(3).capacity(), (2 << 20) / 4);
    }

    #[test]
    fn a_mesh_over_half_a_general_page_gets_an_exact_own_page() {
        let mut planner = PagePlanner::default();
        let half = PageFamily::Index.general_capacity() / 2;
        let general = planner.place(PageFamily::Index, half);
        let own = planner.place(PageFamily::Index, half + 1);
        assert_eq!(planner.page(general.page).kind, PageKind::General);
        assert_eq!(planner.page(own.page).kind, PageKind::Own);
        assert_eq!(planner.page(own.page).capacity(), half + 1);
        // Larger than the largest batch page: still one exact page.
        let huge = PageFamily::Index.max_page_capacity() + 5;
        let placement = planner.place(PageFamily::Index, huge);
        assert_eq!(planner.page(placement.page).capacity(), huge);
        // Own pages go with their mesh; the emptied general page stays.
        assert!(planner.free(placement.page, 0, huge));
        assert!(planner.free(own.page, 0, half + 1));
        assert!(!planner.free(general.page, 0, half));
        assert_eq!(planner.pages().count(), 1);
        assert_eq!(planner.page(general.page).live(), 0);
        // The destroyed pages' ids come back.
        assert_eq!(planner.place(PageFamily::Shadow, 1).page, own.page);
    }

    #[test]
    fn the_batch_threshold_is_per_family() {
        // The fewest vertices of 48 bytes that reach the threshold.
        let enough = BATCH_PAGE_MIN_BYTES.div_ceil(48) as u32;
        assert!(!wants_batch(SURFACE, [enough - 1]));
        // A registration's meshes add up.
        assert!(wants_batch(SURFACE, [enough / 2, enough - enough / 2]));
        assert!(!wants_batch(PageFamily::Shadow, [enough]));
        assert!(!wants_batch(PageFamily::Index, [(MIB / 4) as u32 - 1]));
        assert!(wants_batch(PageFamily::Index, [(MIB / 4) as u32]));
        // Effect vec4s count toward the bytes.
        let effect = PageFamily::Surface { extra: 2 };
        assert!(!wants_batch(effect, [enough / 2]));
        assert!(wants_batch(
            effect,
            [BATCH_PAGE_MIN_BYTES.div_ceil(80) as u32]
        ));
    }

    #[test]
    fn batches_split_at_the_page_limit_and_never_straddle() {
        let mut planner = PagePlanner::default();
        let family = PageFamily::Shadow;
        let max = family.max_page_capacity();
        let counts = [max / 2, max / 3, max / 4, 10, max + 7, 20, 30];
        let placements = planner.place_batch(family, &counts);
        assert_eq!(placements.len(), counts.len());
        let pages: Vec<u16> = placements.iter().map(|p| p.page).collect();
        // [max/2, max/3] fit one page; max/4 starts the next with 10; the oversized
        // mesh gets one to itself; the last two share the fourth.
        assert_eq!(pages, [0, 0, 1, 1, 2, 3, 3]);
        let new_pages: Vec<bool> = placements.iter().map(|p| p.new_page).collect();
        assert_eq!(new_pages, [true, false, true, false, true, true, false]);
        for (placement, &count) in placements.iter().zip(&counts) {
            let page = planner.page(placement.page);
            assert_eq!(page.kind, PageKind::Batch);
            assert!(placement.first + count <= page.capacity());
        }
        // Exact sizes, filled in order, all written.
        assert_eq!(planner.page(0).capacity(), max / 2 + max / 3);
        assert_eq!(planner.page(1).capacity(), max / 4 + 10);
        assert_eq!(planner.page(2).capacity(), max + 7);
        assert_eq!(placements[1].first, max / 2);
        assert_eq!(planner.page(3).written(), 50);
        assert_eq!(planner.capacity_bytes(), planner.live_bytes());
        // A batch page lives until its last mesh is freed.
        assert!(!planner.free(3, placements[5].first, 20));
        assert!(planner.free(3, placements[6].first, 30));
        assert_eq!(planner.pages().count(), 3);
    }

    #[test]
    fn the_written_prefix_only_grows() {
        let mut planner = PagePlanner::default();
        let a = planner.place(SURFACE, 100);
        let b = planner.place(SURFACE, 50);
        assert_eq!(planner.page(0).written(), 150);
        planner.free(0, b.first, 50);
        planner.free(0, a.first, 100);
        // Freed ranges keep their (written) contents.
        assert_eq!(planner.page(0).written(), 150);
        planner.place(SURFACE, 120);
        assert_eq!(planner.page(0).written(), 150);
        planner.place(SURFACE, 40);
        assert_eq!(planner.page(0).written(), 160);
    }

    #[test]
    fn effect_pages_size_and_count_both_buffers() {
        let mut planner = PagePlanner::default();
        let family = PageFamily::Surface { extra: 1 };
        assert_eq!(family.general_capacity(), (8 << 20) / 64);
        planner.place(family, 10);
        planner.place(SURFACE, 10);
        assert_eq!(planner.buffer_count(), 3);
        assert_eq!(planner.capacity_bytes(), 2 * (8 << 20) - (8 << 20) % 48);
        assert_eq!(planner.live_bytes(), 10 * 64 + 10 * 48);
    }

    #[test]
    fn indices_rebase_in_place() {
        let mut indices = vec![0, 1, 2, 2, 1, 3];
        rebase_indices(&mut indices, 0);
        assert_eq!(indices, [0, 1, 2, 2, 1, 3]);
        rebase_indices(&mut indices, 1000);
        assert_eq!(indices, [1000, 1001, 1002, 1002, 1001, 1003]);
    }

    #[test]
    fn shared_indices_stream_the_whole_rebase_in_pieces() {
        const CHUNK: u32 = 17;
        // Shorter than a chunk, exactly one, exactly two and a ragged tail: where the
        // chunk arithmetic would drop or repeat a piece.
        for length in [CHUNK - 1, CHUNK, 2 * CHUNK, 103] {
            let mesh: Vec<u32> = (0..length).map(|i| (i * 7) % 40).collect();
            for first_vertex in [0, 5000] {
                for (indices, vertex_count) in [(Some(&mesh[..]), 40), (None, length)] {
                    let mut whole: Vec<u32> = match indices {
                        Some(indices) => indices.to_vec(),
                        None => (0..vertex_count).collect(),
                    };
                    rebase_indices(&mut whole, first_vertex);
                    let mut streamed = Vec::new();
                    let mut offsets = Vec::new();
                    let mut scratch = [0; CHUNK as usize];
                    stream_shared_indices(
                        indices,
                        vertex_count,
                        first_vertex,
                        &mut scratch,
                        |offset, chunk| {
                            assert_eq!(offset as usize, streamed.len(), "chunks in order");
                            assert!(!chunk.is_empty() && chunk.len() <= CHUNK as usize);
                            offsets.push(offset);
                            streamed.extend_from_slice(chunk);
                        },
                    );
                    let case = format!("{length} indices from vertex {first_vertex}");
                    assert_eq!(streamed, whole, "{case}");
                    let expected: Vec<u32> = (0..length).step_by(CHUNK as usize).collect();
                    assert_eq!(offsets, expected, "{case}");
                }
            }
        }
    }

    #[test]
    fn trimming_destroys_only_empty_general_pages() {
        let mut planner = PagePlanner::default();
        let half = SURFACE.general_capacity() / 2;
        let kept = planner.place(SURFACE, 10);
        let round = [planner.place(SURFACE, half), planner.place(SURFACE, half)];
        let indices = planner.place(PageFamily::Index, 10);
        let own = planner.place(PageFamily::Shadow, PageFamily::Shadow.general_capacity());
        assert_eq!((kept.page, round[1].page, indices.page), (0, 1, 2));
        // Nothing is empty yet.
        assert!(planner.trim().is_empty());
        for placement in round {
            planner.free(placement.page, placement.first, half);
        }
        assert!(!planner.free(indices.page, indices.first, 10));
        // Pages 1 and 2 emptied (the round's second surface page and the index page);
        // page 0 keeps a mesh and the own page goes only with its mesh.
        assert_eq!(planner.pages().count(), 4);
        assert_eq!(planner.trim(), [1, 2]);
        let left: Vec<u16> = planner.pages().map(|(id, _)| id).collect();
        assert_eq!(left, [0, own.page]);
        assert_eq!(planner.page(0).live(), 10);
        assert!(planner.trim().is_empty());
        // The trimmed ids come back for new pages.
        assert_eq!(planner.place(PageFamily::Index, 5).page, 1);
    }

    #[test]
    fn a_large_registration_reserves_batch_pages_per_family() {
        let mut planner = PagePlanner::default();
        // Enough of a family for a batch, in two meshes: 48 and 64 bytes a vertex.
        let surface = BATCH_PAGE_MIN_BYTES.div_ceil(2 * 48) as u32;
        let effect = PageFamily::Surface { extra: 1 };
        let effect_vertices = BATCH_PAGE_MIN_BYTES.div_ceil(64) as u32;
        let size = |family, vertices, indices| MeshSize {
            family,
            vertices,
            indices,
        };
        // A registration's owned meshes, then its shadow groups, as `MeshStore` lists
        // them: two surface meshes that add up to a batch, an undrawable one between
        // them, one effect mesh that is a batch alone, and a shadow group too small for
        // one. Their indices together reach an index batch.
        let meshes = [
            size(SURFACE, surface, 3 * surface),
            size(SURFACE, 500, 0),
            size(SURFACE, surface, 3 * surface),
            size(effect, effect_vertices, 6),
            size(PageFamily::Shadow, 1000, 3000),
        ];
        let placements = planner.reserve(&meshes);
        assert_eq!(
            placements.len(),
            meshes.len(),
            "one entry per mesh, in order"
        );
        // Undrawable: nothing reserved, and its vertices count toward no batch.
        assert_eq!(placements[1], MeshPlacement::default());
        let vertex: Vec<Option<(u16, u32)>> = placements
            .iter()
            .map(|p| p.vertex.map(|v| (v.page, v.first)))
            .collect();
        assert_eq!(
            vertex,
            [Some((0, 0)), None, Some((0, surface)), Some((1, 0)), None]
        );
        assert_eq!(planner.page(0).capacity(), 2 * surface);
        assert_eq!(planner.page(1).family, effect);
        // The four drawable meshes' indices share one index batch, in order.
        let index: Vec<Option<(u16, u32)>> = placements
            .iter()
            .map(|p| p.index.map(|i| (i.page, i.first)))
            .collect();
        assert_eq!(
            index,
            [
                Some((2, 0)),
                None,
                Some((2, 3 * surface)),
                Some((2, 6 * surface)),
                Some((2, 6 * surface + 6)),
            ]
        );
        assert_eq!(planner.page(2).capacity(), 6 * surface + 6 + 3000);
        // Each batch page is created by its first placement and holds exactly its
        // meshes, all of which the upload writes.
        let created = placements
            .iter()
            .flat_map(|p| [p.vertex, p.index])
            .flatten()
            .filter(|p| p.new_page)
            .count();
        assert_eq!(created, 3);
        for (_, page) in planner.pages() {
            assert_eq!(page.kind, PageKind::Batch);
            assert_eq!(page.live(), page.capacity());
            assert_eq!(page.written(), page.capacity());
        }
        // The shadow group's vertices go to a general page when it uploads.
        assert_eq!(planner.place(PageFamily::Shadow, 1000).page, 3);
    }

    #[test]
    fn indices_can_reserve_a_batch_without_their_vertices() {
        let mut planner = PagePlanner::default();
        // Under half a general vertex page of vertices with over half a general index
        // page of indices.
        let vertices = BATCH_PAGE_MIN_BYTES as u32 / 48 - 1;
        let indices = BATCH_INDEX_PAGE_MIN_BYTES as u32 / 4;
        let meshes = [
            MeshSize {
                family: SURFACE,
                vertices: vertices / 2,
                indices: indices / 2,
            },
            MeshSize {
                family: SURFACE,
                vertices: vertices / 2,
                indices: indices - indices / 2,
            },
        ];
        let placements = planner.reserve(&meshes);
        assert!(placements.iter().all(|p| p.vertex.is_none()));
        assert_eq!(placements[1].index.map(|i| i.first), Some(indices / 2));
        assert_eq!(planner.pages().count(), 1);
        assert_eq!(planner.page(0).family, PageFamily::Index);
        // Small registrations reserve nothing.
        let small = [MeshSize {
            family: PageFamily::Shadow,
            vertices: 10,
            indices: 30,
        }];
        assert_eq!(planner.reserve(&small), [MeshPlacement::default()]);
        assert!(planner.reserve(&[]).is_empty());
    }
}
