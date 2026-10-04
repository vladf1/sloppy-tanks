//! GPU meshes and materials. A shared `Arc<Mesh>` or interned `Arc<Material>` is
//! uploaded once, keyed by pointer identity (the store keeps a clone, so the
//! pointer cannot be reused while the entry lives). Entries used only by released
//! round resources are freed on `reset_round` once no caller still holds the
//! `Arc` — the Rust form of disposing only `userData.owned` resources. Meshes live
//! in shared mesh pages (`crate::mesh_pages`), not buffers of their own.

use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use sloppy_core::geometry::Mesh;
use sloppy_core::scene::{Effect, Material, TextureRef};
use wgpu::util::DeviceExt;

use crate::camera::Sphere;
use crate::color::hex_to_linear;
use crate::effects::EffectRegistry;
use crate::gpu::textures::TextureStore;
use crate::mesh_pages::{
    MeshPlacement, MeshRange, MeshSize, PageFamily, PagePlanner, Placement, rebase_indices,
    stream_shared_indices,
};
use crate::model::MeshData;
use crate::shader::MaterialFeatures;
use crate::shadow_merge::ShadowGroup;

/// Bind group layouts shared by every pipeline.
pub struct Layouts {
    pub frame: wgpu::BindGroupLayout,
    pub material: wgpu::BindGroupLayout,
    pub water: wgpu::BindGroupLayout,
    pub output: wgpu::BindGroupLayout,
}

const fn texture_entry(binding: u32, filterable: bool) -> wgpu::BindGroupLayoutEntry {
    staged_texture_entry(binding, filterable, wgpu::ShaderStages::FRAGMENT)
}

const fn staged_texture_entry(
    binding: u32,
    filterable: bool,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

const fn sampler_entry(binding: u32, ty: wgpu::SamplerBindingType) -> wgpu::BindGroupLayoutEntry {
    staged_sampler_entry(binding, ty, wgpu::ShaderStages::FRAGMENT)
}

const fn staged_sampler_entry(
    binding: u32,
    ty: wgpu::SamplerBindingType,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Sampler(ty),
        count: None,
    }
}

/// Secondary effect textures a material binds (`Material::extra_textures`).
pub const EXTRA_TEXTURE_SLOTS: usize = 2;

const fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

// The entries are data so the pipeline precompiler (`precompile.rs`) builds the
// same layouts as wgpu.
use wgpu::SamplerBindingType::{Comparison, Filtering};

/// Frame uniforms, sun shadow map, reflection and the instance records.
pub const FRAME_ENTRIES: &[wgpu::BindGroupLayoutEntry] = &[
    uniform_entry(0),
    wgpu::BindGroupLayoutEntry {
        binding: 1,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Depth,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    },
    sampler_entry(2, Comparison),
    texture_entry(3, true),
    sampler_entry(4, Filtering),
    super::instance_store::INSTANCE_ENTRY,
];

/// A uniform block and two filtered textures (the water).
pub const TEXTURED_ENTRIES: &[wgpu::BindGroupLayoutEntry] = &[
    uniform_entry(0),
    texture_entry(1, true),
    sampler_entry(2, Filtering),
    texture_entry(3, true),
    sampler_entry(4, Filtering),
];

pub const OUTPUT_ENTRIES: &[wgpu::BindGroupLayoutEntry] =
    &[texture_entry(0, false), uniform_entry(1)];

/// Map, bump and emissive map for the surface; effect textures for any stage.
pub const MATERIAL_ENTRIES: &[wgpu::BindGroupLayoutEntry] = &[
    uniform_entry(0),
    texture_entry(1, true),
    sampler_entry(2, Filtering),
    texture_entry(3, true),
    sampler_entry(4, Filtering),
    texture_entry(5, true),
    sampler_entry(6, Filtering),
    staged_texture_entry(7, true, wgpu::ShaderStages::VERTEX_FRAGMENT),
    staged_sampler_entry(8, Filtering, wgpu::ShaderStages::VERTEX_FRAGMENT),
    staged_texture_entry(9, true, wgpu::ShaderStages::VERTEX_FRAGMENT),
    staged_sampler_entry(10, Filtering, wgpu::ShaderStages::VERTEX_FRAGMENT),
];

impl Layouts {
    pub fn new(device: &wgpu::Device) -> Self {
        let layout = |label, entries| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries,
            })
        };
        Self {
            frame: layout("frame", FRAME_ENTRIES),
            material: layout("material", MATERIAL_ENTRIES),
            water: layout("water", TEXTURED_ENTRIES),
            output: layout("output", OUTPUT_ENTRIES),
        }
    }
}

pub struct GpuMesh {
    /// Where its vertices and absolute indices live in the mesh pages.
    pub range: MeshRange,
    pub extra_attributes: u8,
    pub bounds: Sphere,
    /// The caller's mesh for shared entries; `None` for model-owned merges.
    source: Option<(Arc<Mesh>, Vec<&'static str>)>,
    /// Live draw classes using this mesh.
    pub users: u32,
}

/// A mesh page's buffers: its vertices or indices, and for a surface page with
/// effect vec4s those, in the same vertex numbering. Draws bind them only through
/// [`MeshStore::vertex_buffers`] and [`MeshStore::index_buffer`], which bind the
/// written prefix: never `slice(..)` a page for a draw, or wgpu clears its unwritten
/// tail first, on WebGL through a page-sized vector of zeros in the Wasm heap.
struct PageBuffers {
    main: wgpu::Buffer,
    extra: Option<wgpu::Buffer>,
}

impl PageBuffers {
    fn destroy(&self) {
        self.main.destroy();
        if let Some(extra) = &self.extra {
            extra.destroy();
        }
    }
}

/// Batch placements for a registration's owned meshes and merged shadow groups, in
/// their order.
pub struct Reservation {
    pub owned: Vec<MeshPlacement>,
    pub shadow: Vec<MeshPlacement>,
}

/// GPU meshes: entries with their bounds and users, stored in shared mesh pages
/// (`crate::mesh_pages`).
#[derive(Default)]
pub struct MeshStore {
    slots: Vec<Option<GpuMesh>>,
    free: Vec<u32>,
    shared: HashMap<(usize, Vec<&'static str>), u32>,
    /// A shared mesh lost its last draw class since the last collection.
    released: bool,
    /// Which page each mesh's vertices and indices live in, and where.
    plan: PagePlanner,
    /// Each page's buffers, by page id.
    pages: Vec<Option<PageBuffers>>,
}

/// Vertices converted per write when a shared mesh streams to the GPU. The converted
/// chunk is a passing heap allocation (384 KiB of vertices, plus effect vec4s), small
/// enough to fit the free space loading leaves rather than grow the Wasm memory, which
/// keeps any growth for good.
const UPLOAD_CHUNK_VERTICES: usize = 8 * 1024;

/// Indices rebased per write when a shared mesh streams to the GPU: 16 KiB on the
/// stack, so the rebase takes no heap at all.
const UPLOAD_CHUNK_INDICES: usize = 4 * 1024;

impl MeshStore {
    fn insert(&mut self, mesh: GpuMesh) -> u32 {
        match self.free.pop() {
            Some(index) => {
                self.slots[index as usize] = Some(mesh);
                index
            }
            None => {
                self.slots.push(Some(mesh));
                self.slots.len() as u32 - 1
            }
        }
    }

    /// Create the buffers of a page the planner just added. Pages are filled only
    /// through the queue, never mapped at creation: the browser backend stages a
    /// mapped range in a Wasm-side copy of the whole buffer, and linear memory never
    /// shrinks.
    fn create_page(&mut self, device: &wgpu::Device, page: u16) {
        let info = self.plan.page(page);
        let family = info.family;
        let capacity = u64::from(info.capacity());
        let (label, usage) = match family {
            PageFamily::Surface { .. } => ("mesh vertex page", wgpu::BufferUsages::VERTEX),
            PageFamily::Shadow => ("shadow vertex page", wgpu::BufferUsages::VERTEX),
            PageFamily::Index => ("mesh index page", wgpu::BufferUsages::INDEX),
        };
        let buffer = |label, stride: u64| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: capacity * stride,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let buffers = PageBuffers {
            main: buffer(label, family.stride()),
            extra: (family.extra_stride() > 0)
                .then(|| buffer("mesh effect attribute page", family.extra_stride())),
        };
        let slot = page as usize;
        if self.pages.len() <= slot {
            self.pages.resize_with(slot + 1, || None);
        }
        self.pages[slot] = Some(buffers);
    }

    fn page_buffers(&self, page: u16) -> &PageBuffers {
        self.pages[page as usize].as_ref().expect("live mesh page")
    }

    /// Where `count` elements of `family` go: the reserved batch placement, whose
    /// page [`reserve`](Self::reserve) created, or a general or own page now.
    fn place(
        &mut self,
        device: &wgpu::Device,
        family: PageFamily,
        count: u32,
        reserved: Option<Placement>,
    ) -> Placement {
        if let Some(placement) = reserved {
            return placement;
        }
        let placement = self.plan.place(family, count);
        if placement.new_page {
            self.create_page(device, placement.page);
        }
        placement
    }

    /// Give a registration's owned meshes and merged shadow groups the batch pages
    /// `PagePlanner::reserve` decides on, creating them; the meshes then upload into
    /// their placements in order.
    pub fn reserve(
        &mut self,
        device: &wgpu::Device,
        owned: &[MeshData],
        shadow: &[ShadowGroup],
    ) -> Reservation {
        // Every mesh in upload order: owned meshes, then shadow groups.
        let sizes: Vec<MeshSize> = owned
            .iter()
            .map(|data| MeshSize {
                family: PageFamily::Surface {
                    extra: data.extra_attributes,
                },
                vertices: data.vertices.len() as u32,
                indices: data.indices.len() as u32,
            })
            .chain(shadow.iter().map(|group| MeshSize {
                family: PageFamily::Shadow,
                vertices: group.vertices.len() as u32,
                indices: group.indices.len() as u32,
            }))
            .collect();
        let mut placements = self.plan.reserve(&sizes);
        let created: Vec<u16> = placements
            .iter()
            .flat_map(|placement| [placement.vertex, placement.index])
            .flatten()
            .filter(|placement| placement.new_page)
            .map(|placement| placement.page)
            .collect();
        for page in created {
            self.create_page(device, page);
        }
        let shadow = placements.split_off(owned.len());
        Reservation {
            owned: placements,
            shadow,
        }
    }

    /// Place and write one mesh: `vertices`, and `extra` (its effect vec4s), at its
    /// vertex range, and `indices`, made absolute in place, at its index range. A
    /// mesh without vertices or indices gets [`MeshRange::EMPTY`] and draws nothing.
    #[allow(clippy::too_many_arguments)]
    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        family: PageFamily,
        vertices: &[u8],
        extra: &[u8],
        indices: &mut [u32],
        reserved: MeshPlacement,
    ) -> MeshRange {
        let size = MeshSize {
            family,
            vertices: (vertices.len() as u64 / family.stride()) as u32,
            indices: indices.len() as u32,
        };
        if !size.is_drawable() {
            return MeshRange::EMPTY;
        }
        let vertex_count = size.vertices;
        debug_assert_eq!(
            extra.len() as u64,
            u64::from(vertex_count) * family.extra_stride()
        );
        let vertex = self.place(device, family, vertex_count, reserved.vertex);
        let index = self.place(
            device,
            PageFamily::Index,
            indices.len() as u32,
            reserved.index,
        );
        rebase_indices(indices, vertex.first);
        let page = self.page_buffers(vertex.page);
        let at = u64::from(vertex.first);
        queue.write_buffer(&page.main, at * family.stride(), vertices);
        if let Some(buffer) = &page.extra {
            queue.write_buffer(buffer, at * family.extra_stride(), extra);
        }
        queue.write_buffer(
            &self.page_buffers(index.page).main,
            u64::from(index.first) * 4,
            bytemuck::cast_slice(indices),
        );
        MeshRange {
            vertex_page: vertex.page,
            first_vertex: vertex.first,
            vertex_count,
            index_page: index.page,
            first_index: index.first,
            index_count: indices.len() as u32,
        }
    }

    /// Stream an unmodified shared mesh to its pages a chunk at a time: the quarry's
    /// merged walls alone would need a 20 MB upload copy in linear memory, which
    /// never shrinks. Its indices are rebased a chunk at a time on the stack.
    fn upload_shared(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &Mesh,
        attributes: &[&str],
    ) -> MeshRange {
        let vertex_count = mesh.positions.len() as u32;
        let family = PageFamily::Surface {
            extra: attributes.len() as u8,
        };
        let size = MeshSize {
            family,
            vertices: vertex_count,
            indices: mesh
                .indices
                .as_ref()
                .map_or(vertex_count, |indices| indices.len() as u32),
        };
        if !size.is_drawable() {
            return MeshRange::EMPTY;
        }
        let index_count = size.indices;
        let vertex = self.place(device, family, vertex_count, None);
        let index = self.place(device, PageFamily::Index, index_count, None);
        let page = self.page_buffers(vertex.page);
        for start in (0..vertex_count as usize).step_by(UPLOAD_CHUNK_VERTICES) {
            let range = start..(start + UPLOAD_CHUNK_VERTICES).min(vertex_count as usize);
            let (vertices, extras) = crate::model::shared_vertices(mesh, attributes, range);
            let at = u64::from(vertex.first) + start as u64;
            queue.write_buffer(
                &page.main,
                at * family.stride(),
                bytemuck::cast_slice(&vertices),
            );
            if let Some(extra) = &page.extra {
                queue.write_buffer(
                    extra,
                    at * family.extra_stride(),
                    bytemuck::cast_slice(&extras),
                );
            }
        }
        let indices = &self.pages[index.page as usize]
            .as_ref()
            .expect("live mesh page")
            .main;
        stream_shared_indices(
            mesh.indices.as_deref(),
            vertex_count,
            vertex.first,
            &mut [0; UPLOAD_CHUNK_INDICES],
            |offset, chunk| {
                let at = u64::from(index.first + offset) * 4;
                queue.write_buffer(indices, at, bytemuck::cast_slice(chunk));
            },
        );
        MeshRange {
            vertex_page: vertex.page,
            first_vertex: vertex.first,
            vertex_count,
            index_page: index.page,
            first_index: index.first,
            index_count,
        }
    }

    /// The GPU mesh for a shared `Arc<Mesh>`, uploading it on first use. It goes to
    /// general pages, or an own page when large, never to a registration's batch
    /// pages: deduplicated by `Arc` identity, it can outlive the model that
    /// registered it.
    pub fn shared(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &Arc<Mesh>,
        attributes: &'static [&'static str],
    ) -> u32 {
        let key = (Arc::as_ptr(mesh) as usize, attributes.to_vec());
        if let Some(&index) = self.shared.get(&key) {
            return index;
        }
        let range = self.upload_shared(device, queue, mesh, attributes);
        let index = self.insert(GpuMesh {
            range,
            extra_attributes: attributes.len() as u8,
            bounds: Sphere::from_points(mesh.positions.iter().map(|p| Vec3::from(*p))),
            source: Some((mesh.clone(), attributes.to_vec())),
            users: 0,
        });
        self.shared.insert(key, index);
        index
    }

    /// Upload merged geometry owned by one model, at its reserved placement if it has
    /// one; release it with `release`. Its indices are rebased in place.
    pub fn owned(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &mut MeshData,
        reserved: MeshPlacement,
    ) -> u32 {
        let family = PageFamily::Surface {
            extra: data.extra_attributes,
        };
        let range = self.upload(
            device,
            queue,
            family,
            bytemuck::cast_slice(&data.vertices),
            bytemuck::cast_slice(&data.extra),
            &mut data.indices,
            reserved,
        );
        self.insert(GpuMesh {
            range,
            extra_attributes: data.extra_attributes,
            bounds: data.bounds,
            source: None,
            users: 0,
        })
    }

    /// Upload a model's merged shadow group, at its reserved placement if it has
    /// one; free it with [`free_range`](Self::free_range). Its indices are rebased in
    /// place.
    pub fn shadow(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        group: &mut ShadowGroup,
        reserved: MeshPlacement,
    ) -> MeshRange {
        self.upload(
            device,
            queue,
            PageFamily::Shadow,
            bytemuck::cast_slice(&group.vertices),
            &[],
            &mut group.indices,
            reserved,
        )
    }

    /// Free a mesh's ranges at once: nothing in the frame being built draws it, and
    /// frames in flight finish before a later write reuses the range (queue order).
    /// A batch or own page that empties is destroyed.
    pub fn free_range(&mut self, range: MeshRange) {
        if range.is_empty() {
            return;
        }
        let ranges = [
            (range.vertex_page, range.first_vertex, range.vertex_count),
            (range.index_page, range.first_index, range.index_count),
        ];
        for (page, first, count) in ranges {
            if self.plan.free(page, first, count) {
                let buffers = self.pages[page as usize].take().expect("live mesh page");
                buffers.destroy();
            }
        }
    }

    /// Destroy the general pages no mesh uses (`PagePlanner::trim`).
    fn trim_pages(&mut self) {
        for page in self.plan.trim() {
            let buffers = self.pages[page as usize].take().expect("live mesh page");
            buffers.destroy();
        }
    }

    /// A vertex page's buffers to bind: only the prefix written so far. Every placed
    /// range is written before anything can draw from it and a freed range keeps its
    /// old contents, so the prefix holds every mesh in the page and is always
    /// initialized. Binding the never-written tail as well would make wgpu zero-fill
    /// it before the pass, and on WebGL an index page cannot be cleared on the GPU:
    /// wgpu-hal would upload a CPU vector of zeros as large as the page, and the Wasm
    /// heap would keep that size for good.
    pub fn vertex_buffers(
        &self,
        page: u16,
    ) -> (wgpu::BufferSlice<'_>, Option<wgpu::BufferSlice<'_>>) {
        let info = self.plan.page(page);
        let written = u64::from(info.written());
        let buffers = self.page_buffers(page);
        (
            buffers.main.slice(..written * info.family.stride()),
            buffers
                .extra
                .as_ref()
                .map(|extra| extra.slice(..written * info.family.extra_stride())),
        )
    }

    /// An index page to bind: only its written prefix, as in
    /// [`vertex_buffers`](Self::vertex_buffers).
    pub fn index_buffer(&self, page: u16) -> wgpu::BufferSlice<'_> {
        let written = u64::from(self.plan.page(page).written());
        self.page_buffers(page).main.slice(..written * 4)
    }

    pub fn get(&self, index: u32) -> &GpuMesh {
        self.slots[index as usize].as_ref().expect("live mesh")
    }

    pub fn get_mut(&mut self, index: u32) -> &mut GpuMesh {
        self.slots[index as usize].as_mut().expect("live mesh")
    }

    /// A draw class stopped using a mesh. Models come and go mid-round (cover
    /// looks, timber members, falling crowns and boughs), so a mesh that loses its
    /// last user is freed at the next [`collect_released`](Self::collect_released)
    /// rather than lingering until the round resets.
    pub fn remove_user(&mut self, index: u32) {
        let mesh = self.get_mut(index);
        mesh.users -= 1;
        if mesh.users == 0 {
            self.released = true;
        }
    }

    /// Free the shared meshes released since the last call that no caller holds,
    /// then destroy the general pages left empty (`PagePlanner::trim`). The renderer
    /// collects once a frame, before it builds draws, so a page emptied and refilled
    /// within one frame (a model rebuilt in place, or a round reset's old and new
    /// round) stays.
    pub fn collect_released(&mut self) {
        if std::mem::take(&mut self.released) {
            self.collect_unused();
        }
        self.trim_pages();
    }

    pub fn release(&mut self, index: u32) {
        if let Some(mesh) = self.slots[index as usize].take() {
            self.free_range(mesh.range);
            if let Some((source, attributes)) = mesh.source {
                self.shared
                    .remove(&(Arc::as_ptr(&source) as usize, attributes));
            }
            self.free.push(index);
        }
    }

    /// Free shared meshes no draw uses and no caller still holds.
    pub fn collect_unused(&mut self) {
        let unused: Vec<u32> = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                let mesh = slot.as_ref()?;
                let (source, _) = mesh.source.as_ref()?;
                (mesh.users == 0 && Arc::strong_count(source) == 1).then_some(index as u32)
            })
            .collect();
        for index in unused {
            self.release(index);
        }
    }

    pub fn count(&self) -> usize {
        self.slots.iter().flatten().count()
    }

    /// Shared meshes that nothing draws and no caller holds, awaiting collection.
    pub fn unused(&self) -> usize {
        self.slots
            .iter()
            .flatten()
            .filter(|mesh| {
                mesh.users == 0
                    && mesh
                        .source
                        .as_ref()
                        .is_some_and(|(source, _)| Arc::strong_count(source) == 1)
            })
            .count()
    }

    /// GPU bytes of the mesh pages (merged shadows included): their capacity, which
    /// is what they allocate.
    pub fn bytes(&self) -> u64 {
        self.plan.capacity_bytes()
    }

    /// Page bytes no mesh uses: holes, and the free tails of general pages.
    pub fn slack_bytes(&self) -> u64 {
        self.plan.capacity_bytes() - self.plan.live_bytes()
    }

    pub fn buffers(&self) -> usize {
        self.plan.buffer_count()
    }
}

/// WGSL `MaterialUniform`.
/// Byte offset of `MaterialUniform::params`, for pools that animate them.
pub const MATERIAL_PARAMS_OFFSET: u64 = 5 * 16;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MaterialUniform {
    pub color: [f32; 4],
    pub emissive: [f32; 4],
    pub surface: [f32; 4],
    pub map_transform: [f32; 4],
    pub bump_transform: [f32; 4],
    pub emissive_transform: [f32; 4],
    pub extra_transforms: [[f32; 4]; EXTRA_TEXTURE_SLOTS],
    pub params: [[f32; 4]; 4],
    /// x: `MaterialFeatures` bits.
    pub features: [u32; 4],
}

impl MaterialUniform {
    pub fn of(material: &Material) -> Self {
        let [r, g, b] = hex_to_linear(material.color.0);
        let [er, eg, eb] = hex_to_linear(material.emissive.0);
        let i = material.emissive_intensity;
        let transform = |t: Option<&TextureRef>| {
            t.map_or([1.0, 1.0, 0.0, 0.0], |t| {
                [t.repeat[0], t.repeat[1], t.offset[0], t.offset[1]]
            })
        };
        let mut params = [[0.0; 4]; 4];
        if let Effect::Custom { params: values, .. } = &material.effect {
            for (i, value) in values.iter().take(16).enumerate() {
                params[i / 4][i % 4] = *value;
            }
        }
        Self {
            color: [r, g, b, material.opacity],
            emissive: [er * i, eg * i, eb * i, 0.0],
            surface: [
                material.roughness,
                material.metalness,
                material.alpha_test,
                material.bump_scale,
            ],
            map_transform: transform(material.map.as_ref()),
            bump_transform: transform(material.bump_map.as_ref()),
            emissive_transform: transform(material.emissive_map.as_ref()),
            extra_transforms: std::array::from_fn(|slot| transform(extra_texture(material, slot))),
            params,
            features: [MaterialFeatures::of(material).0, 0, 0, 0],
        }
    }
}

/// The texture in an effect's extra slot, if the material names one.
fn extra_texture(material: &Material, slot: usize) -> Option<&TextureRef> {
    material
        .extra_textures
        .get(slot)
        .map(|(_, texture)| texture)
}

/// Every texture a material samples.
fn material_textures(material: &Material) -> impl Iterator<Item = &TextureRef> {
    [&material.map, &material.bump_map, &material.emissive_map]
        .into_iter()
        .flatten()
        .chain(
            material
                .extra_textures
                .iter()
                .take(EXTRA_TEXTURE_SLOTS)
                .map(|(_, texture)| texture),
        )
}

pub struct GpuMaterial {
    pub material: Arc<Material>,
    pub uniform: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub effect: u16,
    /// Built while a texture was still a placeholder.
    waiting: bool,
    generation: u64,
    /// Live draw classes using this material.
    pub users: u32,
}

#[derive(Default)]
pub struct MaterialStore {
    slots: Vec<Option<GpuMaterial>>,
    free: Vec<u32>,
    by_ptr: HashMap<usize, u32>,
    warned: Vec<&'static str>,
}

impl MaterialStore {
    fn bind_group(
        device: &wgpu::Device,
        layouts: &Layouts,
        textures: &mut TextureStore,
        material: &Material,
        uniform: &wgpu::Buffer,
    ) -> (wgpu::BindGroup, bool) {
        // Texture slots in binding order: map, bump, emissive, then effect extras.
        let slots = [
            material.map.as_ref(),
            material.bump_map.as_ref(),
            material.emissive_map.as_ref(),
            extra_texture(material, 0),
            extra_texture(material, 1),
        ];
        let samplers = slots.map(|texture| textures.sampler(device, texture));
        let mut ready = true;
        let views = slots.map(|texture| match texture {
            Some(texture) => {
                let (view, loaded) = textures.view(texture);
                ready &= loaded;
                view
            }
            None => textures.placeholder(),
        });
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }];
        for (slot, (view, sampler)) in views.iter().zip(&samplers).enumerate() {
            let binding = 1 + slot as u32 * 2;
            entries.push(wgpu::BindGroupEntry {
                binding,
                resource: wgpu::BindingResource::TextureView(view),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: binding + 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            });
        }
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("material"),
            layout: &layouts.material,
            entries: &entries,
        });
        (bind_group, !ready)
    }

    /// The GPU material for an interned material, creating it on first use.
    pub fn get_or_create(
        &mut self,
        device: &wgpu::Device,
        layouts: &Layouts,
        textures: &mut TextureStore,
        effects: &EffectRegistry,
        material: &Arc<Material>,
    ) -> u32 {
        let key = Arc::as_ptr(material) as usize;
        if let Some(&index) = self.by_ptr.get(&key) {
            return index;
        }
        for texture in material_textures(material) {
            textures.request(texture);
        }
        let effect = match &material.effect {
            Effect::None => 0,
            Effect::Custom { name, .. } => effects.id(name).unwrap_or_else(|| {
                if !self.warned.contains(name) {
                    self.warned.push(name);
                    web_sys::console::warn_1(
                        &format!("Unknown material effect `{name}`; drawing without it").into(),
                    );
                }
                0
            }),
        };
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("material uniform"),
            contents: bytemuck::bytes_of(&MaterialUniform::of(material)),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let (bind_group, waiting) = Self::bind_group(device, layouts, textures, material, &uniform);
        let gpu = GpuMaterial {
            material: material.clone(),
            uniform,
            bind_group,
            effect,
            waiting,
            generation: textures.generation,
            users: 0,
        };
        let index = match self.free.pop() {
            Some(index) => {
                self.slots[index as usize] = Some(gpu);
                index
            }
            None => {
                self.slots.push(Some(gpu));
                self.slots.len() as u32 - 1
            }
        };
        self.by_ptr.insert(key, index);
        index
    }

    /// Rebind materials whose textures arrived since their bind group was built.
    pub fn refresh(
        &mut self,
        device: &wgpu::Device,
        layouts: &Layouts,
        textures: &mut TextureStore,
    ) {
        for gpu in self.slots.iter_mut().flatten() {
            if gpu.waiting && gpu.generation != textures.generation {
                let (bind_group, waiting) =
                    Self::bind_group(device, layouts, textures, &gpu.material, &gpu.uniform);
                gpu.bind_group = bind_group;
                gpu.waiting = waiting;
                gpu.generation = textures.generation;
            }
        }
    }

    pub fn get(&self, index: u32) -> &GpuMaterial {
        self.slots[index as usize].as_ref().expect("live material")
    }

    pub fn get_mut(&mut self, index: u32) -> &mut GpuMaterial {
        self.slots[index as usize].as_mut().expect("live material")
    }

    /// Free materials no draw uses and nobody but the store and the interner holds.
    pub fn collect_unused(&mut self) {
        for index in 0..self.slots.len() {
            let unused = self.slots[index]
                .as_ref()
                .is_some_and(|gpu| gpu.users == 0 && Arc::strong_count(&gpu.material) <= 2);
            if unused {
                let gpu = self.slots[index].take().expect("checked");
                gpu.uniform.destroy();
                self.by_ptr.remove(&(Arc::as_ptr(&gpu.material) as usize));
                self.free.push(index as u32);
            }
        }
    }

    pub fn count(&self) -> usize {
        self.slots.iter().flatten().count()
    }
}
