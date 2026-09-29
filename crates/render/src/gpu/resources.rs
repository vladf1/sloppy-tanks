//! GPU meshes and materials. A shared `Arc<Mesh>` or interned `Arc<Material>` is
//! uploaded once, keyed by pointer identity (the store keeps a clone, so the
//! pointer cannot be reused while the entry lives). Entries used only by released
//! round resources are freed on `reset_round` once no caller still holds the
//! `Arc` — the Rust form of disposing only `userData.owned` resources.

use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use sloppy_core::geometry::Mesh;
use sloppy_core::scene::{Effect, Material};
use wgpu::util::DeviceExt;

use crate::camera::Sphere;
use crate::color::hex_to_linear;
use crate::effects::EffectRegistry;
use crate::gpu::textures::TextureStore;
use crate::model::{MeshData, Vertex};

/// Bind group layouts shared by every pipeline.
pub struct Layouts {
    pub frame: wgpu::BindGroupLayout,
    pub material: wgpu::BindGroupLayout,
    pub water: wgpu::BindGroupLayout,
    pub output: wgpu::BindGroupLayout,
}

fn texture_entry(binding: u32, filterable: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32, ty: wgpu::SamplerBindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(ty),
        count: None,
    }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
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

impl Layouts {
    pub fn new(device: &wgpu::Device) -> Self {
        use wgpu::SamplerBindingType::{Comparison, Filtering};
        let frame = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame"),
            entries: &[
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
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let textured = |label| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries: &[
                    uniform_entry(0),
                    texture_entry(1, true),
                    sampler_entry(2, Filtering),
                    texture_entry(3, true),
                    sampler_entry(4, Filtering),
                ],
            })
        };
        let output = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("output"),
            entries: &[texture_entry(0, false), uniform_entry(1)],
        });
        Self {
            frame,
            material: textured("material"),
            water: textured("water"),
            output,
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
}

fn buffer(
    device: &wgpu::Device,
    label: &str,
    contents: &[u8],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    // Pad so empty meshes still get a valid, 4-byte aligned buffer.
    let mut padded;
    let contents = if !contents.len().is_multiple_of(4) || contents.is_empty() {
        padded = contents.to_vec();
        padded.resize(contents.len().div_ceil(4).max(1) * 4, 0);
        &padded[..]
    } else {
        contents
    };
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents,
        usage,
    })
}

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

    fn upload(device: &wgpu::Device, data: &MeshData) -> GpuMesh {
        let vertex = buffer(
            device,
            "mesh vertices",
            bytemuck::cast_slice(&data.vertices),
            wgpu::BufferUsages::VERTEX,
        );
        let index = buffer(
            device,
            "mesh indices",
            bytemuck::cast_slice(&data.indices),
            wgpu::BufferUsages::INDEX,
        );
        let extra = (data.extra_attributes > 0).then(|| {
            buffer(
                device,
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

    /// The GPU mesh for a shared `Arc<Mesh>`, uploading it on first use.
    pub fn shared(
        &mut self,
        device: &wgpu::Device,
        mesh: &Arc<Mesh>,
        attributes: &'static [&'static str],
    ) -> u32 {
        let key = (Arc::as_ptr(mesh) as usize, attributes.to_vec());
        if let Some(&index) = self.shared.get(&key) {
            return index;
        }
        let mut gpu = Self::upload(device, &crate::model::mesh_data(mesh, attributes));
        gpu.source = Some((mesh.clone(), attributes.to_vec()));
        let index = self.insert(gpu);
        self.shared.insert(key, index);
        index
    }

    /// Upload merged geometry owned by one model; release it with `release`.
    pub fn owned(&mut self, device: &wgpu::Device, data: &MeshData) -> u32 {
        let gpu = Self::upload(device, data);
        self.insert(gpu)
    }

    pub fn get(&self, index: u32) -> &GpuMesh {
        self.slots[index as usize].as_ref().expect("live mesh")
    }

    pub fn get_mut(&mut self, index: u32) -> &mut GpuMesh {
        self.slots[index as usize].as_mut().expect("live mesh")
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
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MaterialUniform {
    pub color: [f32; 4],
    pub emissive: [f32; 4],
    pub surface: [f32; 4],
    pub map_transform: [f32; 4],
    pub bump_transform: [f32; 4],
    pub params: [[f32; 4]; 4],
}

impl MaterialUniform {
    pub fn of(material: &Material) -> Self {
        let [r, g, b] = hex_to_linear(material.color.0);
        let [er, eg, eb] = hex_to_linear(material.emissive.0);
        let i = material.emissive_intensity;
        let transform = |t: &Option<sloppy_core::scene::TextureRef>| {
            t.as_ref().map_or([1.0, 1.0, 0.0, 0.0], |t| {
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
            map_transform: transform(&material.map),
            bump_transform: transform(&material.bump_map),
            params,
        }
    }
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
        let map_sampler = textures.sampler(device, material.map.as_ref());
        let bump_sampler = textures.sampler(device, material.bump_map.as_ref());
        let (map, map_ready) = match &material.map {
            Some(texture) => textures.view(texture),
            None => (textures.placeholder(), true),
        };
        let (bump, bump_ready) = match &material.bump_map {
            Some(texture) => textures.view(texture),
            None => (textures.placeholder(), true),
        };
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("material"),
            layout: &layouts.material,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(map),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&map_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(bump),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&bump_sampler),
                },
            ],
        });
        (bind_group, !(map_ready && bump_ready))
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
        for texture in material.map.iter().chain(&material.bump_map) {
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
            usage: wgpu::BufferUsages::UNIFORM,
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
