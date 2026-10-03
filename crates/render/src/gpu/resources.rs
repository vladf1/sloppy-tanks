//! GPU meshes and materials. A shared `Arc<Mesh>` or interned `Arc<Material>` is
//! uploaded once, keyed by pointer identity (the store keeps a clone, so the
//! pointer cannot be reused while the entry lives). Entries used only by released
//! round resources are freed on `reset_round` once no caller still holds the
//! `Arc` — the Rust form of disposing only `userData.owned` resources.

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
use crate::model::{MeshData, Vertex};
use crate::shader::MaterialFeatures;

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
    pub vertex: wgpu::Buffer,
    pub index: wgpu::Buffer,
    pub extra: Option<wgpu::Buffer>,
    pub index_count: u32,
    pub extra_attributes: u8,
    pub bounds: Sphere,
    pub bytes: u64,
    /// The caller's mesh for shared entries; `None` for model-owned merges.
    source: Option<(Arc<Mesh>, Vec<&'static str>)>,
    /// Live draw classes using this mesh.
    pub users: u32,
}

impl GpuMesh {
    fn destroy(&self) {
        self.vertex.destroy();
        self.index.destroy();
        if let Some(extra) = &self.extra {
            extra.destroy();
        }
    }
}

#[derive(Default)]
pub struct MeshStore {
    slots: Vec<Option<GpuMesh>>,
    free: Vec<u32>,
    shared: HashMap<(usize, Vec<&'static str>), u32>,
    /// A shared mesh lost its last draw class since the last collection.
    released: bool,
}

/// A buffer holding `contents`. It is written through the queue rather than mapped
/// at creation: the browser backend stages a mapped range in a Wasm-side copy of the
/// whole buffer, and linear memory never shrinks, so a large scenery mesh would
/// leave the heap that much bigger for the rest of the page.
pub fn buffer_with_contents(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    contents: &[u8],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    // Pad so empty meshes still get a valid, 4-byte aligned buffer.
    let aligned = contents.len() / 4 * 4;
    let size = contents.len().div_ceil(4).max(1) * 4;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size as u64,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    if aligned > 0 {
        queue.write_buffer(&buffer, 0, &contents[..aligned]);
    }
    if aligned < contents.len() {
        let mut tail = [0; 4];
        tail[..contents.len() - aligned].copy_from_slice(&contents[aligned..]);
        queue.write_buffer(&buffer, aligned as u64, &tail);
    }
    buffer
}

/// Vertices converted per write when a shared mesh streams to the GPU.
const UPLOAD_CHUNK_VERTICES: usize = 16 * 1024;

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

    fn upload(device: &wgpu::Device, queue: &wgpu::Queue, data: &MeshData) -> GpuMesh {
        let vertex = buffer_with_contents(
            device,
            queue,
            "mesh vertices",
            bytemuck::cast_slice(&data.vertices),
            wgpu::BufferUsages::VERTEX,
        );
        let index = buffer_with_contents(
            device,
            queue,
            "mesh indices",
            bytemuck::cast_slice(&data.indices),
            wgpu::BufferUsages::INDEX,
        );
        let extra = (data.extra_attributes > 0).then(|| {
            buffer_with_contents(
                device,
                queue,
                "mesh effect attributes",
                bytemuck::cast_slice(&data.extra),
                wgpu::BufferUsages::VERTEX,
            )
        });
        let bytes = (data.vertices.len() * size_of::<Vertex>()
            + data.indices.len() * 4
            + data.extra.len() * 16) as u64;
        GpuMesh {
            vertex,
            index,
            extra,
            index_count: data.indices.len() as u32,
            extra_attributes: data.extra_attributes,
            bounds: data.bounds,
            bytes,
            source: None,
            users: 0,
        }
    }

    /// Stream an unmodified shared mesh to the GPU a chunk of vertices at a time:
    /// the quarry's merged walls alone would need a 20 MB upload copy in linear
    /// memory, which never shrinks. Indices go straight from the mesh.
    fn upload_shared(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &Mesh,
        attributes: &[&str],
    ) -> GpuMesh {
        let count = mesh.positions.len();
        let empty = |label, size: usize, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size.max(4) as u64,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let vertex_size = size_of::<Vertex>();
        let extra_size = attributes.len() * size_of::<[f32; 4]>();
        let vertex = empty(
            "mesh vertices",
            count * vertex_size,
            wgpu::BufferUsages::VERTEX,
        );
        let extra = (!attributes.is_empty()).then(|| {
            empty(
                "mesh effect attributes",
                count * extra_size,
                wgpu::BufferUsages::VERTEX,
            )
        });
        let index = match &mesh.indices {
            Some(indices) => buffer_with_contents(
                device,
                queue,
                "mesh indices",
                bytemuck::cast_slice(indices),
                wgpu::BufferUsages::INDEX,
            ),
            None => empty("mesh indices", count * 4, wgpu::BufferUsages::INDEX),
        };
        for start in (0..count).step_by(UPLOAD_CHUNK_VERTICES) {
            let range = start..(start + UPLOAD_CHUNK_VERTICES).min(count);
            let (vertices, extras) = crate::model::shared_vertices(mesh, attributes, range.clone());
            queue.write_buffer(
                &vertex,
                (start * vertex_size) as u64,
                bytemuck::cast_slice(&vertices),
            );
            if let Some(extra) = &extra {
                queue.write_buffer(
                    extra,
                    (start * extra_size) as u64,
                    bytemuck::cast_slice(&extras),
                );
            }
            if mesh.indices.is_none() {
                let indices: Vec<u32> = (range.start as u32..range.end as u32).collect();
                queue.write_buffer(&index, (start * 4) as u64, bytemuck::cast_slice(&indices));
            }
        }
        let index_count = mesh.indices.as_ref().map_or(count, Vec::len);
        GpuMesh {
            vertex,
            index,
            extra,
            index_count: index_count as u32,
            extra_attributes: attributes.len() as u8,
            bounds: Sphere::from_points(mesh.positions.iter().map(|p| Vec3::from(*p))),
            bytes: (count * (vertex_size + extra_size) + index_count * 4) as u64,
            source: None,
            users: 0,
        }
    }

    /// The GPU mesh for a shared `Arc<Mesh>`, uploading it on first use.
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
        let mut gpu = Self::upload_shared(device, queue, mesh, attributes);
        gpu.source = Some((mesh.clone(), attributes.to_vec()));
        let index = self.insert(gpu);
        self.shared.insert(key, index);
        index
    }

    /// Upload merged geometry owned by one model; release it with `release`.
    pub fn owned(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &MeshData) -> u32 {
        let gpu = Self::upload(device, queue, data);
        self.insert(gpu)
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

    /// Free the shared meshes released since the last call that no caller holds.
    pub fn collect_released(&mut self) {
        if std::mem::take(&mut self.released) {
            self.collect_unused();
        }
    }

    pub fn release(&mut self, index: u32) {
        if let Some(mesh) = self.slots[index as usize].take() {
            mesh.destroy();
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

    pub fn bytes(&self) -> u64 {
        self.slots.iter().flatten().map(|mesh| mesh.bytes).sum()
    }

    pub fn buffers(&self) -> usize {
        self.slots
            .iter()
            .flatten()
            .map(|mesh| 2 + mesh.extra.is_some() as usize)
            .sum()
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
