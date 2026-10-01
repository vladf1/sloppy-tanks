//! The browser renderer: WebGPU device and canvas, resources, per-frame culling
//! and draw lists, and the shadow → reflection → main → output passes.
//!
//! # Drawing model
//!
//! - `add_model` prepares a `Node` tree once (see `crate::model`); `add_instance`
//!   places it any number of times. Per instance: world transform, visibility,
//!   opacity (below 1 it draws blended in the transparent list and its shadow
//!   dithers away), tint, four effect floats, and local transform/visibility
//!   overrides for named joints (turret yaw, barrel pitch/recoil, wheels,
//!   suspension). Every visible part is culled by its bounding sphere per view,
//!   and instances sharing a mesh and material draw as one instanced call.
//! - `add_scenery` bakes static scenery into world-space batches per material and
//!   spatial cell; repeated meshes and `Drawable.instances` become static
//!   instanced draws whose records upload once.
//! - `Lifetime::Round` models, instances and scenery are released by
//!   `reset_round`; `Lifetime::Shared` ones persist. GPU meshes and materials keyed
//!   by `Arc` identity outlive resets while their `Arc` is still held elsewhere.
//! - Pipelines are cached forever. `prepare_step` compiles the scene's variants
//!   on the browser's background threads (`precompile.rs`); call it until nothing
//!   remains, then `warm_up`, before the first gameplay frame. A variant first met
//!   while drawing compiles synchronously and counts as a late pipeline.

mod context;
mod inspect;
mod lut;
mod pipelines;
mod pools;
mod precompile;
mod resources;
mod textures;

pub use inspect::InstanceState;
pub use pools::PoolId;
pub use textures::image_data;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2, Vec3};
use sloppy_core::geometry::Mesh;
use sloppy_core::scene::{Blending, Node, Side, TextureRef};
use wgpu::util::DeviceExt;

use crate::camera::{Frustum, PerspectiveCamera, ShadowCamera, ShadowReach, Sphere, mirror_view};
use crate::color::{hex_to_linear, hex_to_linear_scaled};
use crate::draw_list::{
    Draw, DrawListBuilder, InstanceRecord, MAIN_VIEW, REFLECTION_VIEW, SHADOW_VIEW, VIEW_COUNT,
    ViewDraws,
};
use crate::effects::{EffectDefinition, EffectRegistry};
use crate::material::{MaterialInterner, is_transparent};
use crate::model::{
    InstanceData, ModelNode, PartMesh, PreparedModel, SceneryOptions, prepare_model,
    prepare_scenery,
};
use crate::shader::{PipelineKey, ShaderKey};
use crate::shadow_merge::{MergeKind, ShadowGroup, merge_shadows};
use context::{ColorTarget, Context, DEPTH_FORMAT};
use pipelines::{Pipelines, SAMPLE_COUNT, shadow_merged_index};
use pools::PoolEntry;
use resources::{Layouts, MaterialStore, MeshStore, buffer_with_contents};
use textures::TextureStore;

/// Whether a resource survives `reset_round`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lifetime {
    /// Kept across rounds (cached models, themed scenery).
    Shared,
    /// Released by `reset_round` (per-round tanks, cover, effects).
    Round,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ModelId {
    index: u32,
    generation: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InstanceId {
    index: u32,
    generation: u32,
}

/// Three linear `Fog`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fog {
    pub color: u32,
    pub near: f32,
    pub far: f32,
}

/// Background, fog and lights, with Three's conventions: sRGB hex colors,
/// intensities multiply colors without π, the hemisphere light points up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Environment {
    pub background: u32,
    pub fog: Option<Fog>,
    pub sky_color: u32,
    pub ground_color: u32,
    pub hemisphere_intensity: f32,
    pub sun_color: u32,
    pub sun_intensity: f32,
    pub sun_position: Vec3,
    pub sun_target: Vec3,
    /// Tone-mapping exposure (Three `toneMappingExposure`).
    pub exposure: f32,
    /// Strength of the sky's specular reflection on lit surfaces; 0 keeps
    /// Three's hemisphere-only fill, which the labs' reference frames use.
    pub reflections: f32,
}

impl Default for Environment {
    /// The village theme from `presentation.ts` `reset()`.
    fn default() -> Self {
        Self {
            background: 0xaacbc2,
            fog: Some(Fog {
                color: 0xaacbc2,
                near: 210.0,
                far: 380.0,
            }),
            sky_color: 0xbdd5f5,
            ground_color: 0x75859b,
            hemisphere_intensity: 1.65,
            sun_color: 0xffd59b,
            sun_intensity: 2.8,
            sun_position: Vec3::new(-45.0, 68.0, 25.0),
            sun_target: Vec3::ZERO,
            exposure: 1.0,
            reflections: 0.0,
        }
    }
}

/// The sun's shadow map. The camera's position and target are replaced by the
/// sun's each frame, like Three's `DirectionalLightShadow.updateMatrices`; its box
/// and depth range come from here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SunShadow {
    pub enabled: bool,
    pub map_size: u32,
    pub camera: ShadowCamera,
    pub bias: f32,
    pub normal_bias: f32,
    /// PCF filter radius in texels (Three `shadow.radius`).
    pub radius: f32,
    /// No surface that receives the shadow lies below this height, so a caster
    /// matters only when its shadow reaches a view above it ([`ShadowReach`]).
    /// Negative infinity draws every caster in the box.
    pub receiver_floor: f32,
}

/// `SHADOW_DEPTH` in scenery.ts.
pub const SHADOW_DEPTH: f32 = 219.5;

impl Default for SunShadow {
    /// `createLighting` + `defaultSunShadow` (arena half 60 + 10).
    fn default() -> Self {
        Self {
            enabled: true,
            map_size: 2048,
            camera: ShadowCamera::square(
                Vec3::new(-45.0, 68.0, 25.0),
                Vec3::ZERO,
                70.0,
                0.5,
                SHADOW_DEPTH,
            ),
            bias: -0.0002,
            normal_bias: 0.05,
            radius: 1.0,
            receiver_floor: f32::NEG_INFINITY,
        }
    }
}

/// Three `PointLight` (muzzle flashes, explosions).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointLight {
    pub position: Vec3,
    pub color: u32,
    pub intensity: f32,
    /// Cutoff distance; 0 is unlimited.
    pub distance: f32,
    pub decay: f32,
}

pub const MAX_POINT_LIGHTS: usize = 4;

/// Half-extent of the arena apron that hides the water (`waterInView`).
const CALM_EXTENT: f32 = 62.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WaterShore {
    /// Harbor: shallows begin `half_size` metres from the origin on either axis.
    Basin { half_size: f32 },
    /// Village creek: shallows along both banks of `uv.x`.
    Creek,
}

/// Planar-reflection water (`water-surface.ts`).
#[derive(Clone, Debug)]
pub struct WaterSettings {
    /// The surface in world XZ at y = 0; drawn at `height`.
    pub mesh: Arc<Mesh>,
    pub height: f32,
    pub ripple_scale: f32,
    pub distortion_scale: f32,
    pub normal_strength: f32,
    /// Ripple clock = time × this.
    pub time_scale: f32,
    pub sun_direction: Vec3,
    pub sun_color: u32,
    pub deep_color: u32,
    pub shallow_color: u32,
    pub shore: WaterShore,
    pub reflection: bool,
    /// Reflection target edge in pixels (the original Water used 512).
    pub reflection_size: u32,
    /// Skip the reflection while the view's four corner rays all land on the
    /// plane within this half-extent (only the apron is in view,
    /// `WaterSurface.waterInView`); 0 always reflects.
    pub calm_extent: f32,
}

impl WaterSettings {
    pub fn harbor(mesh: Arc<Mesh>) -> Self {
        Self {
            mesh,
            height: -2.2,
            ripple_scale: 4.0,
            distortion_scale: 1.8,
            normal_strength: 1.3,
            time_scale: 0.7,
            sun_direction: Vec3::new(-45.0, 55.0, 25.0).normalize(),
            sun_color: 0xffdcc0,
            deep_color: 0x164956,
            shallow_color: 0x3a807d,
            shore: WaterShore::Basin { half_size: 62.0 },
            reflection: true,
            reflection_size: 512,
            calm_extent: CALM_EXTENT,
        }
    }

    pub fn creek(mesh: Arc<Mesh>) -> Self {
        Self {
            mesh,
            height: -2.65,
            ripple_scale: 7.0,
            distortion_scale: 0.65,
            normal_strength: 0.75,
            time_scale: 0.65,
            sun_direction: Vec3::new(-45.0, 68.0, 25.0).normalize(),
            sun_color: 0xffebce,
            deep_color: 0x244b3f,
            shallow_color: 0x638466,
            shore: WaterShore::Creek,
            reflection: true,
            reflection_size: 512,
            calm_extent: CALM_EXTENT,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct RendererOptions {
    /// URL prefix for `TextureSource::File` paths (the page's `BASE_URL`).
    pub asset_base: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PrepareProgress {
    /// Pipelines compiled by this call.
    pub compiled: u32,
    /// Pipelines the registered scene still needs.
    pub remaining: u32,
    /// Pipelines compiling in the background; later calls create them.
    pub compiling: u32,
}

/// Counters for "Stats for nerds". Draw calls and triangles cover every pass of
/// the last frame; resource counts and bytes are current allocations.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RenderStats {
    pub draw_calls: u32,
    pub triangles: u64,
    pub shadow_draw_calls: u32,
    pub reflection_draw_calls: u32,
    /// Instance records written this frame.
    pub instance_records: u32,
    pub pipelines: u32,
    pub shader_modules: u32,
    /// Pipelines compiled while drawing (a warm-up miss if nonzero after prepare).
    pub late_pipelines: u32,
    pub meshes: u32,
    /// Shared meshes nothing draws or holds; zero after a frame.
    pub unused_meshes: u32,
    pub materials: u32,
    pub textures: u32,
    pub textures_pending: u32,
    pub buffers: u32,
    pub models: u32,
    pub instances: u32,
    pub draw_classes: u32,
    /// Estimated GPU bytes: meshes, textures, attachments, shadow map, instances.
    pub gpu_bytes: u64,
    /// Instance pools registered, and the instances they drew last frame.
    pub pools: u32,
    pub pool_instances: u32,
}

/// WGSL `Frame`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct FrameUniform {
    view_projection: [f32; 16],
    view: [f32; 16],
    shadow_matrix: [f32; 16],
    camera_position: [f32; 4],
    fog_color: [f32; 4],
    fog_range: [f32; 4],
    sky_color: [f32; 4],
    ground_color: [f32; 4],
    sun_direction: [f32; 4],
    sun_color: [f32; 4],
    shadow: [f32; 4],
    viewport: [f32; 4],
    camera_right: [f32; 4],
    camera_up: [f32; 4],
    point_lights: [[f32; 8]; MAX_POINT_LIGHTS],
}

/// WGSL `Water`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct WaterUniform {
    sun_direction: [f32; 4],
    sun_color: [f32; 4],
    deep_color: [f32; 4],
    shallow_color: [f32; 4],
    settings: [f32; 4],
    shore: [f32; 4],
}

fn rgb4(rgb: [f32; 3], w: f32) -> [f32; 4] {
    [rgb[0], rgb[1], rgb[2], w]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ClassKey {
    mesh: u32,
    material: u32,
    receive_shadow: bool,
    faded: bool,
    /// The instance pool that owns this class, `NO_POOL` for model parts.
    pool: u32,
}

const NO_POOL: u32 = u32::MAX;

/// A mesh + material + pipeline combination that instances batch under.
struct ClassEntry {
    key: ClassKey,
    /// Pool classes draw from their pool's own instance buffer.
    pool: Option<u32>,
    transparent: bool,
    main_key: PipelineKey,
    /// Transparent double-sided materials draw back faces first, then front
    /// faces, like Three's two-pass `DoubleSide` transparency.
    back_key: Option<PipelineKey>,
    shadow_key: PipelineKey,
    main: Option<u32>,
    back: Option<u32>,
    shadow: Option<u32>,
    /// Parts using this class that cast shadows.
    casters: u32,
    users: u32,
}

struct PartEntry {
    node: usize,
    local: Mat4,
    class: u32,
    faded_class: Option<u32>,
    cast_shadow: bool,
    render_order: i32,
    frustum_culled: bool,
    /// Mesh-space bounds.
    bounds: Sphere,
    instances: Option<Vec<InstanceData>>,
    /// Its shadow draws from the model's merged casters (unless faded).
    merged_shadow: bool,
}

/// A merged, depth-only caster mesh (`shadow_merge.rs`).
struct ShadowMesh {
    vertex: wgpu::Buffer,
    index: wgpu::Buffer,
    index_count: u32,
    /// Index into `Pipelines::shadow_merged`.
    pipeline: usize,
    /// Cutout groups: the material (store index) whose map and cutoff apply.
    material: Option<u32>,
    /// World bounds (scenery only; models cull per instance).
    bounds: Sphere,
    bytes: u64,
}

/// Where a merged shadow draw's records start.
#[derive(Clone, Copy)]
enum MergedBase {
    /// An absolute record (static scenery reads the identity record 0).
    Static(u32),
    /// An offset into this frame's merged records.
    Dynamic(u32),
}

#[derive(Clone, Copy)]
struct MergedItem {
    pipeline: usize,
    model: u32,
    group: u32,
    base: MergedBase,
}

/// One merged shadow draw: instances `first..first + count` of the bases buffer.
#[derive(Clone, Copy)]
struct MergedDraw {
    model: u32,
    group: u32,
    first: u32,
    count: u32,
}

/// Shadow merge cells for static scenery, in metres.
const SHADOW_MERGE_CELL: f32 = 60.0;

const ZERO_RECORD: InstanceRecord = InstanceRecord {
    world: [0.0; 16],
    tint: [0.0; 4],
    data: [0.0; 4],
};

struct ModelEntry {
    skeleton: PreparedModel,
    parts: Vec<PartEntry>,
    /// Merged shadow casters, and for movable models the parts (entry indices)
    /// whose posed records fill slots 0.. (`None`: a part that was skipped).
    shadow: Vec<ShadowMesh>,
    shadow_slots: Vec<Option<usize>>,
    owned_meshes: Vec<u32>,
    lifetime: Lifetime,
    scenery: bool,
    instances: u32,
}

#[derive(Clone, Copy)]
struct StaticRange {
    first: u32,
    count: u32,
    bounds: Sphere,
}

struct InstanceEntry {
    model: u32,
    lifetime: Lifetime,
    world: Mat4,
    visible: bool,
    opacity: f32,
    tint: [f32; 3],
    data: [f32; 4],
    reflected: bool,
    overrides: Vec<Option<Mat4>>,
    node_visible: Vec<bool>,
    /// Scenery only: persistent records of its instanced parts.
    static_ranges: Vec<Option<StaticRange>>,
}

/// Generational slots, so a stale id never reaches a reused entry.
struct Slab<T> {
    slots: Vec<(u32, Option<T>)>,
    free: Vec<u32>,
}

impl<T> Default for Slab<T> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
        }
    }
}

impl<T> Slab<T> {
    fn insert(&mut self, value: T) -> (u32, u32) {
        match self.free.pop() {
            Some(index) => {
                let slot = &mut self.slots[index as usize];
                slot.0 += 1;
                slot.1 = Some(value);
                (index, slot.0)
            }
            None => {
                self.slots.push((0, Some(value)));
                (self.slots.len() as u32 - 1, 0)
            }
        }
    }
    fn get(&self, index: u32, generation: u32) -> Option<&T> {
        self.slots
            .get(index as usize)
            .filter(|slot| slot.0 == generation)
            .and_then(|slot| slot.1.as_ref())
    }
    fn get_mut(&mut self, index: u32, generation: u32) -> Option<&mut T> {
        self.slots
            .get_mut(index as usize)
            .filter(|slot| slot.0 == generation)
            .and_then(|slot| slot.1.as_mut())
    }
    fn remove(&mut self, index: u32) -> Option<T> {
        let value = self.slots.get_mut(index as usize)?.1.take();
        if value.is_some() {
            self.free.push(index);
        }
        value
    }
    fn at(&self, index: u32) -> Option<&T> {
        self.slots
            .get(index as usize)
            .and_then(|slot| slot.1.as_ref())
    }
    fn iter(&self) -> impl Iterator<Item = (u32, &T)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.1.as_ref().map(|value| (index as u32, value)))
    }
    fn len(&self) -> usize {
        self.slots.iter().filter(|slot| slot.1.is_some()).count()
    }
}

struct Water {
    settings: WaterSettings,
    mesh: u32,
    normals: TextureRef,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    target: ColorTarget,
    waiting: bool,
    generation: u64,
    bounds: Sphere,
}

struct ViewCull {
    active: bool,
    frustum: Frustum,
    origin: Vec3,
    forward: Vec3,
}

impl ViewCull {
    fn inactive() -> Self {
        Self {
            active: false,
            frustum: Frustum::from_view_projection(&Mat4::IDENTITY),
            origin: Vec3::ZERO,
            forward: Vec3::NEG_Z,
        }
    }
}

const INITIAL_INSTANCE_CAPACITY: u32 = 4096;
const RECORD_SIZE: u64 = size_of::<InstanceRecord>() as u64;

pub struct Renderer {
    ctx: Context,
    layouts: Layouts,
    pipelines: Pipelines,
    effects: EffectRegistry,
    interner: MaterialInterner,
    textures: TextureStore,
    meshes: MeshStore,
    materials: MaterialStore,
    classes: Vec<Option<ClassEntry>>,
    class_index: HashMap<ClassKey, u32>,
    free_classes: Vec<u32>,
    models: Slab<ModelEntry>,
    instances: Slab<InstanceEntry>,
    pools: Slab<PoolEntry>,

    main_target: ColorTarget,
    shadow_map: wgpu::Texture,
    shadow_view: wgpu::TextureView,
    dummy_depth: wgpu::TextureView,
    shadow_sampler: wgpu::Sampler,
    lut_view: wgpu::TextureView,
    lut_sampler: wgpu::Sampler,
    reflection_sampler: wgpu::Sampler,
    view_uniforms: [wgpu::Buffer; VIEW_COUNT],
    view_groups: Vec<wgpu::BindGroup>,
    instance_buffer: wgpu::Buffer,
    instance_capacity: u32,
    static_records: Vec<InstanceRecord>,
    static_dirty: bool,
    output_uniform: wgpu::Buffer,
    output_group: wgpu::BindGroup,

    environment: Environment,
    sun_shadow: SunShadow,
    point_lights: [Option<PointLight>; MAX_POINT_LIGHTS],
    camera: PerspectiveCamera,
    water: Option<Water>,
    time: f32,

    builder: DrawListBuilder,
    views: [ViewDraws; VIEW_COUNT],
    joints: Vec<Mat4>,
    joint_visible: Vec<bool>,
    culls: [ViewCull; VIEW_COUNT],
    reflection_active: bool,
    shadow_reach: ShadowReach,
    /// Merged shadow casters this frame: records, items, draws and their bases.
    merged_records: Vec<InstanceRecord>,
    merged_items: Vec<MergedItem>,
    merged_draws: Vec<MergedDraw>,
    shadow_bases: Vec<u32>,
    shadow_base_buffer: wgpu::Buffer,
    shadow_base_capacity: u32,
    stats: RenderStats,
    /// Set once the GPU has run everything submitted before the last [`Renderer::await_gpu`].
    gpu_idle: Arc<AtomicBool>,
}

fn uniform_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn depth_texture(device: &wgpu::Device, label: &str, size: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

const INITIAL_SHADOW_BASES: u32 = 1024;

fn base_buffer(device: &wgpu::Device, capacity: u32) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("shadow bases"),
        size: capacity as u64 * 4,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// How a material's shadow can come from a merged caster: depth only, or an
/// alpha-tested card; never with an effect that moves vertices, dithers the
/// shadow or (for cards) could change the cut-out alpha.
fn shadow_merge_kind(
    effects: &EffectRegistry,
    material: &sloppy_core::scene::Material,
) -> MergeKind {
    let effect = match &material.effect {
        sloppy_core::scene::Effect::None => None,
        sloppy_core::scene::Effect::Custom { name, .. } => {
            effects.id(name).and_then(|id| effects.get(id))
        }
    };
    let moves = effect.is_some_and(|effect| {
        effect.has_vertex() || effect.has_world() || effect.has_clip() || effect.shadow_fade
    });
    if moves {
        MergeKind::Separate
    } else if material.alpha_test > 0.0 {
        if effect.is_some() {
            MergeKind::Separate
        } else {
            MergeKind::Cutout
        }
    } else {
        MergeKind::Opaque
    }
}

fn water_normals() -> TextureRef {
    TextureRef {
        srgb: false,
        anisotropy: 4,
        ..TextureRef::file("textures/water/normals.webp")
    }
}

impl Renderer {
    /// Create the device and canvas context. Fails when WebGPU is unavailable.
    pub async fn new(
        canvas: web_sys::HtmlCanvasElement,
        options: RendererOptions,
    ) -> Result<Renderer, String> {
        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let ctx = Context::new(canvas).await?;
        let device = &ctx.device;
        let layouts = Layouts::new(device);
        let pipelines = Pipelines::new(device, &layouts, ctx.config.format);
        let textures = TextureStore::new(device, &ctx.queue, options.asset_base);
        let sun_shadow = SunShadow::default();
        let shadow_map = depth_texture(device, "sun shadow map", sun_shadow.map_size);
        let dummy_depth =
            depth_texture(device, "shadow pass placeholder", 1).create_view(&Default::default());
        let lut = device.create_texture_with_data(
            &ctx.queue,
            &wgpu::TextureDescriptor {
                label: Some("DFG LUT"),
                size: wgpu::Extent3d {
                    width: lut::DFG_LUT_SIZE,
                    height: lut::DFG_LUT_SIZE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rg16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            bytemuck::cast_slice(&lut::DFG_LUT),
        );
        let linear_clamp = |label| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some(label),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            })
        };
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow compare"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let view_uniforms =
            [0, 1, 2].map(|_| uniform_buffer(device, "frame", size_of::<FrameUniform>() as u64));
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: INITIAL_INSTANCE_CAPACITY as u64 * RECORD_SIZE,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let main_target = ColorTarget::new(device, "main view", width, height, SAMPLE_COUNT);
        let output_uniform = uniform_buffer(device, "output", 16);
        let output_group = Self::output_group(device, &layouts, &main_target, &output_uniform);
        let mut renderer = Renderer {
            layouts,
            pipelines,
            effects: EffectRegistry::default(),
            interner: MaterialInterner::default(),
            textures,
            meshes: MeshStore::default(),
            materials: MaterialStore::default(),
            classes: Vec::new(),
            class_index: HashMap::new(),
            free_classes: Vec::new(),
            models: Slab::default(),
            instances: Slab::default(),
            pools: Slab::default(),
            shadow_view: shadow_map.create_view(&Default::default()),
            shadow_map,
            dummy_depth,
            shadow_sampler,
            lut_view: lut.create_view(&Default::default()),
            lut_sampler: linear_clamp("DFG LUT"),
            reflection_sampler: linear_clamp("water reflection"),
            view_uniforms,
            view_groups: Vec::new(),
            instance_buffer,
            instance_capacity: INITIAL_INSTANCE_CAPACITY,
            static_records: vec![InstanceRecord::IDENTITY],
            static_dirty: true,
            output_uniform,
            output_group,
            main_target,
            environment: Environment::default(),
            sun_shadow,
            point_lights: [None; MAX_POINT_LIGHTS],
            camera: PerspectiveCamera::new(43.0, 0.1, 320.0),
            water: None,
            time: 0.0,
            builder: DrawListBuilder::default(),
            views: Default::default(),
            joints: Vec::new(),
            joint_visible: Vec::new(),
            culls: [
                ViewCull::inactive(),
                ViewCull::inactive(),
                ViewCull::inactive(),
            ],
            reflection_active: false,
            shadow_reach: ShadowReach::everywhere(),
            merged_records: Vec::new(),
            merged_items: Vec::new(),
            merged_draws: Vec::new(),
            shadow_bases: Vec::new(),
            shadow_base_buffer: base_buffer(device, INITIAL_SHADOW_BASES),
            shadow_base_capacity: INITIAL_SHADOW_BASES,
            stats: RenderStats::default(),
            gpu_idle: Arc::new(AtomicBool::new(true)),
            ctx,
        };
        renderer.rebuild_view_groups();
        Ok(renderer)
    }

    fn output_group(
        device: &wgpu::Device,
        layouts: &Layouts,
        target: &ColorTarget,
        uniform: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("output"),
            layout: &layouts.output,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&target.resolved_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform.as_entire_binding(),
                },
            ],
        })
    }

    fn rebuild_view_groups(&mut self) {
        self.view_groups = self.frame_groups(&self.instance_buffer);
        self.rebuild_pool_groups();
    }

    /// The frame bind group of a view with `instances` as its instance records.
    fn frame_group(&self, view: usize, instances: &wgpu::Buffer) -> wgpu::BindGroup {
        let shadow = if view == SHADOW_VIEW {
            &self.dummy_depth
        } else {
            &self.shadow_view
        };
        self.ctx
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("frame"),
                layout: &self.layouts.frame,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.view_uniforms[view].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(shadow),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.shadow_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&self.lut_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(&self.lut_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: instances.as_entire_binding(),
                    },
                ],
            })
    }

    // ---------------------------------------------------------------- setup

    /// Resize the canvas drawing buffer (device pixels). Superseded attachments
    /// are destroyed immediately.
    pub fn resize(&mut self, width: u32, height: u32) {
        let limit = self.ctx.device.limits().max_texture_dimension_2d;
        let width = width.clamp(1, limit);
        let height = height.clamp(1, limit);
        if self.ctx.config.width == width && self.ctx.config.height == height {
            return;
        }
        self.ctx.config.width = width;
        self.ctx.config.height = height;
        self.ctx
            .surface
            .configure(&self.ctx.device, &self.ctx.config);
        self.main_target.destroy();
        self.main_target =
            ColorTarget::new(&self.ctx.device, "main view", width, height, SAMPLE_COUNT);
        self.output_group = Self::output_group(
            &self.ctx.device,
            &self.layouts,
            &self.main_target,
            &self.output_uniform,
        );
    }

    pub fn size(&self) -> (u32, u32) {
        (self.ctx.config.width, self.ctx.config.height)
    }

    /// The first GPU validation error or device loss, if any.
    pub fn error(&self) -> Option<String> {
        self.ctx.error.get()
    }

    /// Add or replace a custom material effect; returns its id.
    pub fn register_effect(&mut self, effect: EffectDefinition) -> u16 {
        self.effects.register(effect)
    }

    pub fn set_environment(&mut self, environment: Environment) {
        self.environment = environment;
    }

    pub fn environment(&self) -> &Environment {
        &self.environment
    }

    pub fn set_sun_shadow(&mut self, shadow: SunShadow) {
        if shadow.map_size != self.sun_shadow.map_size {
            self.shadow_map.destroy();
            self.shadow_map =
                depth_texture(&self.ctx.device, "sun shadow map", shadow.map_size.max(1));
            self.shadow_view = self.shadow_map.create_view(&Default::default());
            self.rebuild_view_groups();
        }
        self.sun_shadow = shadow;
    }

    pub fn set_point_light(&mut self, index: usize, light: Option<PointLight>) {
        if let Some(slot) = self.point_lights.get_mut(index) {
            *slot = light;
        }
    }

    /// The camera's aspect follows the canvas at draw time.
    pub fn set_camera(&mut self, camera: PerspectiveCamera) {
        self.camera = camera;
    }

    pub fn camera(&self) -> PerspectiveCamera {
        let mut camera = self.camera;
        let (width, height) = self.size();
        camera.aspect = width as f32 / height as f32;
        camera
    }

    /// The ground point under a canvas pixel (device pixels, origin top-left).
    pub fn pick_ground(&self, pixel: Vec2, height: f32) -> Option<Vec3> {
        let (width, h) = self.size();
        let ndc = PerspectiveCamera::pixel_to_ndc(pixel, Vec2::new(width as f32, h as f32));
        self.camera().pick_ground(ndc, height)
    }

    /// Show or remove the planar-reflection water.
    pub fn set_water(&mut self, settings: Option<WaterSettings>) {
        if let Some(water) = self.water.take() {
            water.target.destroy();
            water.uniform.destroy();
            self.meshes.remove_user(water.mesh);
            drop(water);
            // The surface mesh is usually rebuilt per map; free it now if unused.
            self.meshes.collect_unused();
        }
        let Some(settings) = settings else {
            return;
        };
        let device = &self.ctx.device;
        let mesh = self
            .meshes
            .shared(device, &self.ctx.queue, &settings.mesh, &[]);
        self.meshes.get_mut(mesh).users += 1;
        let normals = water_normals();
        self.textures.request(&normals);
        let uniform = uniform_buffer(device, "water", size_of::<WaterUniform>() as u64);
        let size = settings.reflection_size.max(1);
        let target = ColorTarget::new(device, "water reflection", size, size, SAMPLE_COUNT);
        let mut bounds = self.meshes.get(mesh).bounds;
        bounds.center.y += settings.height;
        let (bind_group, waiting) = self.water_group(&normals, &uniform, &target);
        self.water = Some(Water {
            settings,
            mesh,
            normals,
            uniform,
            bind_group,
            target,
            waiting,
            generation: self.textures.generation,
            bounds,
        });
    }

    fn water_group(
        &mut self,
        normals: &TextureRef,
        uniform: &wgpu::Buffer,
        target: &ColorTarget,
    ) -> (wgpu::BindGroup, bool) {
        let device = &self.ctx.device;
        let sampler = self.textures.sampler(device, Some(normals));
        let (view, ready) = self.textures.view(normals);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("water"),
            layout: &self.layouts.water,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&target.resolved_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.reflection_sampler),
                },
            ],
        });
        (bind_group, !ready)
    }

    /// Enable or pause reflection updates; while paused the water keeps showing
    /// the last reflection (`WaterSurface.reflectionEnabled` / `waterInView`).
    pub fn set_water_reflection(&mut self, enabled: bool) {
        if let Some(water) = &mut self.water {
            water.settings.reflection = enabled;
        }
    }

    /// Supply pixels for `TextureSource::Generated(name)` (rows top to bottom;
    /// see [`image_data`]). Materials using it pick it up on the next frame.
    pub fn set_generated_texture(&mut self, name: &'static str, image: web_sys::ImageData) {
        self.textures.set_generated(name, image);
    }

    /// Textures still loading; wait for 0 before the warm-up frame.
    pub fn textures_pending(&self) -> usize {
        self.textures.pending()
    }

    /// Texture load failures so far (missing files, decode errors).
    pub fn texture_failures(&self) -> &[String] {
        &self.textures.failures
    }

    // ---------------------------------------------------------------- models

    fn class(
        &mut self,
        mesh: u32,
        material: u32,
        receive_shadow: bool,
        faded: bool,
        cast: bool,
        pool: Option<u32>,
    ) -> u32 {
        let key = ClassKey {
            mesh,
            material,
            receive_shadow,
            faded,
            pool: pool.unwrap_or(NO_POOL),
        };
        let index = match self.class_index.get(&key) {
            Some(&index) => index,
            None => {
                let gpu = self.materials.get(material);
                let extra = self.meshes.get(mesh).extra_attributes;
                let source = &gpu.material;
                let transparent = is_transparent(source) || faded;
                let main_key = |material: &sloppy_core::scene::Material| {
                    let shader = ShaderKey::main(material, gpu.effect, extra, receive_shadow);
                    PipelineKey::main(shader, material, faded)
                };
                let two_pass = transparent && source.side == Side::Double;
                let face = |side| sloppy_core::scene::Material {
                    side,
                    ..(**source).clone()
                };
                let dithered = faded || self.effects.get(gpu.effect).is_some_and(|e| e.shadow_fade);
                let shadow_shader =
                    ShaderKey::shadow(source, gpu.effect, extra, dithered, &self.effects);
                let entry = ClassEntry {
                    key,
                    pool,
                    transparent,
                    main_key: if two_pass {
                        main_key(&face(Side::Front))
                    } else {
                        main_key(source)
                    },
                    back_key: two_pass.then(|| main_key(&face(Side::Back))),
                    shadow_key: PipelineKey::shadow(shadow_shader, source),
                    main: None,
                    back: None,
                    shadow: None,
                    casters: 0,
                    users: 0,
                };
                self.meshes.get_mut(mesh).users += 1;
                self.materials.get_mut(material).users += 1;
                let index = match self.free_classes.pop() {
                    Some(index) => {
                        self.classes[index as usize] = Some(entry);
                        index
                    }
                    None => {
                        self.classes.push(Some(entry));
                        self.classes.len() as u32 - 1
                    }
                };
                self.class_index.insert(key, index);
                index
            }
        };
        let class = self.classes[index as usize].as_mut().expect("live class");
        class.users += 1;
        class.casters += cast as u32;
        index
    }

    /// The draw class of an instance pool (its own, keyed by the pool slot).
    fn class_for_pool(
        &mut self,
        mesh: u32,
        material: u32,
        receive_shadow: bool,
        cast: bool,
        pool: u32,
    ) -> u32 {
        self.class(mesh, material, receive_shadow, false, cast, Some(pool))
    }

    fn release_class(&mut self, index: u32, cast: bool) {
        let class = self.classes[index as usize].as_mut().expect("live class");
        class.users -= 1;
        class.casters -= cast as u32;
        if class.users == 0 {
            let class = self.classes[index as usize].take().expect("live class");
            self.class_index.remove(&class.key);
            self.meshes.remove_user(class.key.mesh);
            self.materials.get_mut(class.key.material).users -= 1;
            self.free_classes.push(index);
        }
    }

    fn register(&mut self, prepared: PreparedModel, lifetime: Lifetime, scenery: bool) -> ModelId {
        let device = self.ctx.device.clone();
        let queue = self.ctx.queue.clone();
        let merge = {
            let effects = &self.effects;
            merge_shadows(
                &prepared,
                |index| shadow_merge_kind(effects, &prepared.parts[index].material),
                scenery,
                SHADOW_MERGE_CELL,
            )
        };
        let mut entry_of = vec![None; prepared.parts.len()];
        let owned: Vec<u32> = prepared
            .meshes
            .iter()
            .map(|data| self.meshes.owned(&device, &queue, data))
            .collect();
        let mut parts = Vec::with_capacity(prepared.parts.len());
        for (prepared_index, part) in prepared.parts.iter().enumerate() {
            let material = self.materials.get_or_create(
                &device,
                &self.layouts,
                &mut self.textures,
                &self.effects,
                &part.material,
            );
            let attributes = self.effects.attributes(self.materials.get(material).effect);
            let mesh = match &part.mesh {
                PartMesh::Shared(mesh) => self.meshes.shared(&device, &queue, mesh, attributes),
                PartMesh::Owned(index) => owned[*index],
            };
            if self.meshes.get(mesh).index_count == 0 {
                continue;
            }
            let class = self.class(
                mesh,
                material,
                part.receive_shadow,
                false,
                part.cast_shadow,
                None,
            );
            // Movable models may fade; give them the blended variant up front so
            // warm-up compiles it.
            let fadeable = !scenery
                && !part.material.transparent
                && part.material.blending == Blending::Normal;
            let faded_class = fadeable.then(|| {
                self.class(
                    mesh,
                    material,
                    part.receive_shadow,
                    true,
                    part.cast_shadow,
                    None,
                )
            });
            entry_of[prepared_index] = Some(parts.len());
            parts.push(PartEntry {
                node: part.node,
                local: part.local,
                class,
                faded_class,
                cast_shadow: part.cast_shadow,
                render_order: part.render_order,
                frustum_culled: part.frustum_culled,
                bounds: self.meshes.get(mesh).bounds,
                instances: part.instances.clone(),
                merged_shadow: merge.merged[prepared_index],
            });
        }
        let shadow_slots = merge.slots.iter().map(|&index| entry_of[index]).collect();
        let shadow = merge
            .groups
            .into_iter()
            .filter(|group| !group.indices.is_empty())
            .map(|group| {
                let material = group.cutout.as_ref().map(|material| {
                    let index = self.materials.get_or_create(
                        &device,
                        &self.layouts,
                        &mut self.textures,
                        &self.effects,
                        material,
                    );
                    self.materials.get_mut(index).users += 1;
                    index
                });
                upload_shadow_mesh(&device, &queue, &group, material)
            })
            .collect();
        let skeleton = PreparedModel {
            nodes: prepared.nodes,
            parts: Vec::new(),
            meshes: Vec::new(),
        };
        let (index, generation) = self.models.insert(ModelEntry {
            skeleton,
            parts,
            shadow,
            shadow_slots,
            owned_meshes: owned,
            lifetime,
            scenery,
            instances: 0,
        });
        ModelId { index, generation }
    }

    fn attributes_for(
        effects: &EffectRegistry,
    ) -> impl Fn(&sloppy_core::scene::Material) -> &'static [&'static str] + '_ {
        move |material| match &material.effect {
            sloppy_core::scene::Effect::Custom { name, .. } => effects
                .id(name)
                .map_or(&[][..], |id| effects.attributes(id)),
            sloppy_core::scene::Effect::None => &[],
        }
    }

    /// Prepare a movable model (tanks, pickups, props). Named nodes become joints
    /// that instances can pose; the root's transform comes from each instance.
    pub fn add_model(&mut self, root: &Node, lifetime: Lifetime) -> ModelId {
        let prepared = {
            let attributes = Self::attributes_for(&self.effects);
            prepare_model(root, &mut self.interner, &attributes)
        };
        self.register(prepared, lifetime, false)
    }

    /// Joint index for a named node of a model.
    pub fn model_node(&self, model: ModelId, name: &str) -> Option<usize> {
        self.models
            .get(model.index, model.generation)?
            .skeleton
            .node(name)
    }

    /// Joint names and rest transforms relative to their parent joint.
    pub fn model_nodes(&self, model: ModelId) -> &[ModelNode] {
        self.models
            .get(model.index, model.generation)
            .map_or(&[], |entry| &entry.skeleton.nodes)
    }

    /// Release a model and every instance of it.
    pub fn remove_model(&mut self, model: ModelId) {
        if self.models.get(model.index, model.generation).is_none() {
            return;
        }
        let users: Vec<u32> = self
            .instances
            .iter()
            .filter(|(_, instance)| instance.model == model.index)
            .map(|(index, _)| index)
            .collect();
        for index in users {
            self.instances.remove(index);
        }
        self.release_model(model.index);
    }

    fn release_model(&mut self, index: u32) {
        let Some(entry) = self.models.remove(index) else {
            return;
        };
        for part in &entry.parts {
            self.release_class(part.class, part.cast_shadow);
            if let Some(faded) = part.faded_class {
                self.release_class(faded, part.cast_shadow);
            }
        }
        for mesh in entry.owned_meshes {
            self.meshes.release(mesh);
        }
        for mesh in &entry.shadow {
            mesh.vertex.destroy();
            mesh.index.destroy();
            if let Some(material) = mesh.material {
                self.materials.get_mut(material).users -= 1;
            }
        }
        if entry.scenery {
            self.static_dirty = true;
        }
    }

    /// Place a model. Instances start visible, opaque and untinted.
    pub fn add_instance(
        &mut self,
        model: ModelId,
        world: Mat4,
        lifetime: Lifetime,
    ) -> Option<InstanceId> {
        let entry = self.models.get_mut(model.index, model.generation)?;
        entry.instances += 1;
        let joints = entry.skeleton.nodes.len();
        let node_visible = entry
            .skeleton
            .nodes
            .iter()
            .map(|node| node.visible)
            .collect();
        let (index, generation) = self.instances.insert(InstanceEntry {
            model: model.index,
            lifetime,
            world,
            visible: true,
            opacity: 1.0,
            tint: [1.0; 3],
            data: [0.0; 4],
            reflected: true,
            overrides: vec![None; joints],
            node_visible,
            static_ranges: Vec::new(),
        });
        Some(InstanceId { index, generation })
    }

    /// Bake static scenery (world transforms as authored) into batches.
    pub fn add_scenery(&mut self, root: &Node, lifetime: Lifetime) -> InstanceId {
        self.add_scenery_with(root, lifetime, SceneryOptions::default())
    }

    pub fn add_scenery_with(
        &mut self,
        root: &Node,
        lifetime: Lifetime,
        options: SceneryOptions,
    ) -> InstanceId {
        let prepared = {
            let attributes = Self::attributes_for(&self.effects);
            prepare_scenery(root, &mut self.interner, &attributes, options)
        };
        let model = self.register(prepared, lifetime, true);
        let id = self
            .add_instance(model, Mat4::IDENTITY, lifetime)
            .expect("model just registered");
        self.static_dirty = true;
        id
    }

    /// Remove an instance; scenery also releases its baked batches.
    pub fn remove_instance(&mut self, id: InstanceId) {
        if self.instances.get(id.index, id.generation).is_none() {
            return;
        }
        let instance = self.instances.remove(id.index).expect("checked");
        let model = instance.model;
        let scenery = self.models.at(model).is_some_and(|m| m.scenery);
        if let Some(entry) = self
            .models
            .slots
            .get_mut(model as usize)
            .and_then(|s| s.1.as_mut())
        {
            entry.instances -= 1;
        }
        if scenery {
            self.release_model(model);
        }
    }

    fn instance_mut(&mut self, id: InstanceId) -> Option<&mut InstanceEntry> {
        self.instances.get_mut(id.index, id.generation)
    }

    pub fn set_transform(&mut self, id: InstanceId, world: Mat4) {
        if let Some(instance) = self.instance_mut(id) {
            instance.world = world;
        }
    }

    pub fn set_visible(&mut self, id: InstanceId, visible: bool) {
        if let Some(instance) = self.instance_mut(id) {
            instance.visible = visible;
        }
    }

    /// Below 1 the instance draws blended (transparent list) and its shadow
    /// dithers out, like `debris-fade.ts`.
    pub fn set_opacity(&mut self, id: InstanceId, opacity: f32) {
        if let Some(instance) = self.instance_mut(id) {
            instance.opacity = opacity.clamp(0.0, 1.0);
        }
    }

    /// Linear RGB multiplier on the base color (damage darkening, instance color).
    pub fn set_tint(&mut self, id: InstanceId, tint: [f32; 3]) {
        if let Some(instance) = self.instance_mut(id) {
            instance.tint = tint;
        }
    }

    /// Four floats effects read as `instance_data`.
    pub fn set_instance_data(&mut self, id: InstanceId, data: [f32; 4]) {
        if let Some(instance) = self.instance_mut(id) {
            instance.data = data;
        }
    }

    /// Whether the instance appears in water reflections (false for HUD-like
    /// world objects, Three's `HUD_LAYER`).
    pub fn set_reflected(&mut self, id: InstanceId, reflected: bool) {
        if let Some(instance) = self.instance_mut(id) {
            instance.reflected = reflected;
        }
    }

    /// Replace a joint's local transform (relative to its parent joint), or
    /// restore the rest pose with `None`.
    pub fn set_node_transform(&mut self, id: InstanceId, node: usize, local: Option<Mat4>) {
        if let Some(slot) = self
            .instance_mut(id)
            .and_then(|i| i.overrides.get_mut(node))
        {
            *slot = local;
        }
    }

    pub fn set_node_visible(&mut self, id: InstanceId, node: usize, visible: bool) {
        if let Some(slot) = self
            .instance_mut(id)
            .and_then(|i| i.node_visible.get_mut(node))
        {
            *slot = visible;
        }
    }

    /// Replace the instances of an InstancedMesh part of a model instance's model
    /// (for example a debris pool). Affects every instance of that model.
    pub fn set_part_instances(&mut self, model: ModelId, part: usize, instances: &[InstanceData]) {
        if let Some(entry) = self.models.get_mut(model.index, model.generation)
            && let Some(part) = entry.parts.get_mut(part)
        {
            let list = part.instances.get_or_insert_with(Vec::new);
            list.clear();
            list.extend_from_slice(instances);
        }
    }

    /// Release everything created with `Lifetime::Round`, then free GPU meshes and
    /// materials that nothing uses and no caller still holds.
    pub fn reset_round(&mut self) {
        self.release_round_pools();
        let round: Vec<u32> = self
            .instances
            .iter()
            .filter(|(_, instance)| instance.lifetime == Lifetime::Round)
            .map(|(index, _)| index)
            .collect();
        for index in round {
            let generation = self.instances.slots[index as usize].0;
            self.remove_instance(InstanceId { index, generation });
        }
        let models: Vec<u32> = self
            .models
            .iter()
            .filter(|(_, model)| model.lifetime == Lifetime::Round)
            .map(|(index, _)| index)
            .collect();
        for index in models {
            let users: Vec<u32> = self
                .instances
                .iter()
                .filter(|(_, instance)| instance.model == index)
                .map(|(i, _)| i)
                .collect();
            for i in users {
                self.instances.remove(i);
            }
            self.release_model(index);
        }
        self.meshes.collect_unused();
        self.materials.collect_unused();
        self.interner.retain_used();
        self.static_dirty = true;
    }

    // ---------------------------------------------------------------- warm-up

    /// Distinct pipelines the registered scene still lacks. Many draw classes share
    /// one pipeline (every opaque textured mesh, say), so progress counts keys, not
    /// class slots: the scene's shaders, as the player sees them.
    fn missing_pipelines(&self) -> u32 {
        let mut missing = std::collections::HashSet::new();
        for class in self.classes.iter().flatten() {
            if class.main.is_none() {
                missing.insert(class.main_key);
            }
            if let (Some(key), None) = (&class.back_key, class.back) {
                missing.insert(*key);
            }
            if class.casters > 0 && class.shadow.is_none() {
                missing.insert(class.shadow_key);
            }
        }
        missing.len() as u32
    }

    /// Create up to `budget` of the pipelines the registered scene needs whose
    /// background compile has finished, and queue the compiles of the rest. Yield to
    /// the page between calls (a short timer while only `compiling` remains).
    pub fn prepare_step(&mut self, budget: u32) -> PrepareProgress {
        self.update_textures();
        let fixed_pending = !self.pipelines.fixed_ready(&self.ctx.device);
        let mut compiled = 0;
        let device = &self.ctx.device;
        for class in self.classes.iter_mut().flatten() {
            let slots = [
                (&mut class.main, Some(&class.main_key)),
                (&mut class.back, class.back_key.as_ref()),
                (
                    &mut class.shadow,
                    (class.casters > 0).then_some(&class.shadow_key),
                ),
            ];
            for (slot, key) in slots {
                if slot.is_none()
                    && let Some(key) = key
                {
                    // Another class already made this pipeline: sharing it is free and
                    // must not use up the budget meant for real compiles.
                    if let Some(index) = self.pipelines.find(key) {
                        *slot = Some(index);
                        continue;
                    }
                    *slot = self
                        .pipelines
                        .request(device, &self.effects, key, compiled < budget);
                    compiled += slot.is_some() as u32;
                }
            }
        }
        PrepareProgress {
            compiled,
            remaining: self.missing_pipelines() + u32::from(fixed_pending),
            compiling: self.pipelines.compiling() + u32::from(fixed_pending),
        }
    }

    /// Create every pipeline the scene still needs, synchronously for any whose
    /// background compile has not finished.
    fn complete_pipelines(&mut self) {
        let device = &self.ctx.device;
        self.pipelines.ensure_fixed(device);
        for class in self.classes.iter_mut().flatten() {
            if class.main.is_none() {
                class.main = Some(
                    self.pipelines
                        .ensure(device, &self.effects, &class.main_key),
                );
            }
            if let (Some(key), None) = (&class.back_key, class.back) {
                class.back = Some(self.pipelines.ensure(device, &self.effects, key));
            }
            if class.casters > 0 && class.shadow.is_none() {
                class.shadow = Some(self.pipelines.ensure(
                    device,
                    &self.effects,
                    &class.shadow_key,
                ));
            }
        }
    }

    /// Ask to be told when the GPU has run everything submitted so far, including the
    /// compilation of every pipeline created before; [`Self::gpu_idle`] turns true then.
    pub fn await_gpu(&mut self) {
        let idle = Arc::new(AtomicBool::new(false));
        self.gpu_idle = idle.clone();
        self.ctx
            .queue
            .on_submitted_work_done(move || idle.store(true, Ordering::Release));
    }

    pub fn gpu_idle(&self) -> bool {
        self.gpu_idle.load(Ordering::Acquire)
    }

    /// Draw every prepared variant once offscreen (shadow, main, water and output
    /// passes) so the browser finishes compiling before gameplay needs them.
    pub fn warm_up(&mut self) -> Result<(), String> {
        self.update_textures();
        self.complete_pipelines();
        self.upload_static();
        self.write_view_uniforms();
        for draws in &mut self.views {
            draws.clear();
        }
        for (index, class) in self.classes.iter().enumerate() {
            let Some(class) = class else { continue };
            let draw = Draw {
                class: index as u32,
                first_instance: 0,
                instance_count: 1,
            };
            if class.transparent {
                self.views[MAIN_VIEW].transparent.push(draw);
            } else {
                self.views[MAIN_VIEW].opaque.push(draw);
            }
            if class.casters > 0 {
                self.views[SHADOW_VIEW].opaque.push(draw);
            }
        }
        // One draw per merged shadow pipeline in use, reading static records.
        self.merged_draws.clear();
        let mut warmed = [false; 6];
        for (index, model) in self.models.iter() {
            for (group, mesh) in model.shadow.iter().enumerate() {
                if !std::mem::replace(&mut warmed[mesh.pipeline], true) {
                    self.merged_draws.push(MergedDraw {
                        model: index,
                        group: group as u32,
                        first: 0,
                        count: 1,
                    });
                }
            }
        }
        self.shadow_bases.clear();
        self.shadow_bases.push(0);
        self.upload_shadow_bases();
        let device = &self.ctx.device;
        let probe = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("warm-up output"),
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.ctx.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let probe_view = probe.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("warm-up"),
        });
        self.encode_scene(&mut encoder, false);
        self.encode_output(&mut encoder, &probe_view);
        self.ctx.queue.submit([encoder.finish()]);
        probe.destroy();
        for draws in &mut self.views {
            draws.clear();
        }
        self.merged_draws.clear();
        self.error().map_or(Ok(()), Err)
    }

    // ---------------------------------------------------------------- frame

    fn update_textures(&mut self) {
        if self.textures.drain(&self.ctx.device, &self.ctx.queue) {
            self.materials
                .refresh(&self.ctx.device, &self.layouts, &mut self.textures);
            if let Some(water) = self.water.take() {
                let mut water = water;
                if water.waiting && water.generation != self.textures.generation {
                    let (group, waiting) =
                        self.water_group(&water.normals, &water.uniform, &water.target);
                    water.bind_group = group;
                    water.waiting = waiting;
                    water.generation = self.textures.generation;
                }
                self.water = Some(water);
            }
        }
    }

    fn ensure_capacity(&mut self, records: u32) {
        if records <= self.instance_capacity {
            return;
        }
        let capacity = records.next_power_of_two();
        self.instance_buffer.destroy();
        self.instance_buffer = self.ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: capacity as u64 * RECORD_SIZE,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.instance_capacity = capacity;
        // The static records are current (uploaded earlier this frame); keep them.
        self.ctx.queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&self.static_records),
        );
        self.rebuild_view_groups();
    }

    /// Rebuild the persistent records of static instanced scenery.
    fn upload_static(&mut self) {
        if !self.static_dirty {
            return;
        }
        self.static_dirty = false;
        self.static_records.truncate(1);
        let instance_ids: Vec<u32> = self.instances.iter().map(|(index, _)| index).collect();
        for index in instance_ids {
            let instance = self.instances.slots[index as usize]
                .1
                .as_ref()
                .expect("live");
            let Some(model) = self.models.at(instance.model).filter(|m| m.scenery) else {
                continue;
            };
            let mut ranges = Vec::with_capacity(model.parts.len());
            for part in &model.parts {
                let Some(list) = &part.instances else {
                    ranges.push(None);
                    continue;
                };
                let first = self.static_records.len() as u32;
                let mut bounds: Option<Sphere> = None;
                let base = instance.world * part.local;
                for item in list {
                    let world = base * item.matrix;
                    let sphere = part.bounds.transformed(&world);
                    bounds = Some(bounds.map_or(sphere, |b| b.union(&sphere)));
                    self.static_records.push(InstanceRecord::new(
                        &world,
                        [item.color[0], item.color[1], item.color[2], 1.0],
                        item.data.unwrap_or([0.0; 4]),
                    ));
                }
                ranges.push(bounds.map(|bounds| StaticRange {
                    first,
                    count: list.len() as u32,
                    bounds,
                }));
            }
            self.instances.slots[index as usize]
                .1
                .as_mut()
                .expect("live")
                .static_ranges = ranges;
        }
        self.ensure_capacity(self.static_records.len() as u32);
        self.ctx.queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&self.static_records),
        );
    }

    fn frame_uniform(
        &self,
        view_projection: Mat4,
        view: Mat4,
        eye: Vec3,
        size: (u32, u32),
    ) -> FrameUniform {
        let env = &self.environment;
        let shadow = self.shadow_camera();
        let shadow_matrix = Mat4::from_cols_array(&[
            0.5, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.5, 0.5, 0.0, 1.0,
        ]) * shadow.view_projection();
        let world = view.inverse();
        let mut point_lights = [[0.0; 8]; MAX_POINT_LIGHTS];
        for (slot, light) in point_lights.iter_mut().zip(&self.point_lights) {
            if let Some(light) = light {
                let [r, g, b] = hex_to_linear_scaled(light.color, light.intensity);
                *slot = [
                    light.position.x,
                    light.position.y,
                    light.position.z,
                    light.distance,
                    r,
                    g,
                    b,
                    light.decay,
                ];
            }
        }
        let (fog_color, fog_range) = match env.fog {
            Some(fog) => (
                rgb4(hex_to_linear(fog.color), 1.0),
                [fog.near, fog.far, 0.0, 0.0],
            ),
            None => ([0.0; 4], [0.0; 4]),
        };
        FrameUniform {
            view_projection: view_projection.to_cols_array(),
            view: view.to_cols_array(),
            shadow_matrix: shadow_matrix.to_cols_array(),
            camera_position: [eye.x, eye.y, eye.z, self.time],
            fog_color,
            fog_range,
            sky_color: rgb4(
                hex_to_linear_scaled(env.sky_color, env.hemisphere_intensity),
                env.reflections,
            ),
            ground_color: rgb4(
                hex_to_linear_scaled(env.ground_color, env.hemisphere_intensity),
                0.0,
            ),
            sun_direction: rgb4(
                (env.sun_position - env.sun_target)
                    .normalize_or_zero()
                    .into(),
                0.0,
            ),
            sun_color: rgb4(hex_to_linear_scaled(env.sun_color, env.sun_intensity), 0.0),
            shadow: [
                self.sun_shadow.bias,
                self.sun_shadow.normal_bias,
                self.sun_shadow.map_size as f32,
                self.sun_shadow.radius,
            ],
            viewport: [
                size.0 as f32,
                size.1 as f32,
                1.0 / size.0 as f32,
                1.0 / size.1 as f32,
            ],
            camera_right: rgb4(world.x_axis.truncate().into(), 0.0),
            camera_up: rgb4(world.y_axis.truncate().into(), 0.0),
            point_lights,
        }
    }

    fn shadow_camera(&self) -> ShadowCamera {
        ShadowCamera {
            position: self.environment.sun_position,
            target: self.environment.sun_target,
            ..self.sun_shadow.camera
        }
    }

    fn mirror(&self) -> Option<(Mat4, Mat4)> {
        let water = self.water.as_ref()?;
        mirror_view(&self.camera(), water.settings.height)
    }

    fn write_view_uniforms(&mut self) {
        let camera = self.camera();
        let size = self.size();
        let main = self.frame_uniform(
            camera.view_projection(),
            camera.view(),
            camera.position,
            size,
        );
        let shadow_camera = self.shadow_camera();
        let shadow = self.frame_uniform(
            shadow_camera.view_projection(),
            shadow_camera.view(),
            shadow_camera.position,
            (self.sun_shadow.map_size, self.sun_shadow.map_size),
        );
        let reflection = match (self.mirror(), &self.water) {
            (Some((view, projection)), Some(water)) => {
                let eye = view.inverse().w_axis.truncate();
                let size = water.target.width;
                self.frame_uniform(projection * view, view, eye, (size, size))
            }
            _ => main,
        };
        let queue = &self.ctx.queue;
        for (view, uniform) in [
            (MAIN_VIEW, main),
            (REFLECTION_VIEW, reflection),
            (SHADOW_VIEW, shadow),
        ] {
            queue.write_buffer(&self.view_uniforms[view], 0, bytemuck::bytes_of(&uniform));
        }
        queue.write_buffer(
            &self.output_uniform,
            0,
            bytemuck::bytes_of(&[self.environment.exposure, 0.0, 0.0, 0.0]),
        );
        if let Some(water) = &self.water {
            let s = &water.settings;
            let shore = match s.shore {
                WaterShore::Basin { half_size } => [0.0, half_size, s.height, 0.0],
                WaterShore::Creek => [1.0, 0.0, s.height, 0.0],
            };
            let uniform = WaterUniform {
                sun_direction: rgb4(s.sun_direction.normalize_or_zero().into(), 0.0),
                sun_color: rgb4(hex_to_linear(s.sun_color), 0.0),
                deep_color: rgb4(hex_to_linear(s.deep_color), 0.0),
                shallow_color: rgb4(hex_to_linear(s.shallow_color), 0.0),
                settings: [
                    s.ripple_scale,
                    s.distortion_scale,
                    s.normal_strength,
                    self.time * s.time_scale,
                ],
                shore,
            };
            queue.write_buffer(&water.uniform, 0, bytemuck::bytes_of(&uniform));
        }
    }

    fn update_culls(&mut self) {
        let camera = self.camera();
        let forward = (camera.target - camera.position).normalize_or_zero();
        self.culls[MAIN_VIEW] = ViewCull {
            active: true,
            frustum: Frustum::from_view_projection(&camera.view_projection()),
            origin: camera.position,
            forward,
        };
        let shadow = self.shadow_camera();
        self.culls[SHADOW_VIEW] = ViewCull {
            active: self.sun_shadow.enabled,
            frustum: Frustum::from_view_projection(&shadow.view_projection()),
            origin: shadow.position,
            forward: (shadow.target - shadow.position).normalize_or_zero(),
        };
        self.reflection_active = false;
        self.culls[REFLECTION_VIEW].active = false;
        if let Some(water) = &self.water
            && water.settings.reflection
            && self.culls[MAIN_VIEW]
                .frustum
                .intersects_sphere(&water.bounds)
            && water_in_view(&camera, &water.settings)
            && let Some((view, projection)) = self.mirror()
        {
            let world = view.inverse();
            // Cull with the plain projection: the oblique near plane also skews
            // the far plane, which would reject most of the reflected scene.
            let _ = projection;
            let plain = self.camera().projection() * view;
            self.culls[REFLECTION_VIEW] = ViewCull {
                active: true,
                frustum: Frustum::from_view_projection(&plain),
                origin: world.w_axis.truncate(),
                forward: -world.z_axis.truncate(),
            };
            self.reflection_active = true;
        }
        self.shadow_reach = ShadowReach {
            light: self.culls[SHADOW_VIEW].forward,
            floor: self.sun_shadow.receiver_floor,
            views: [
                Some(self.culls[MAIN_VIEW].frustum),
                self.reflection_active
                    .then_some(self.culls[REFLECTION_VIEW].frustum),
            ],
        };
    }

    fn build_draws(&mut self) {
        let Self {
            instances,
            models,
            classes,
            builder,
            joints,
            joint_visible,
            culls,
            shadow_reach,
            merged_records,
            merged_items,
            ..
        } = self;
        builder.clear();
        merged_records.clear();
        merged_items.clear();
        for (_, instance) in instances.iter() {
            if !instance.visible || instance.opacity <= 0.0 {
                continue;
            }
            let Some(model) = models.at(instance.model) else {
                continue;
            };
            model
                .skeleton
                .joint_transforms(instance.world, &instance.overrides, joints);
            joint_visible.clear();
            for (index, node) in model.skeleton.nodes.iter().enumerate() {
                let parent = node.parent.is_none_or(|parent| joint_visible[parent]);
                joint_visible.push(parent && instance.node_visible[index]);
            }
            let faded = instance.opacity < 1.0;
            let tint = [
                instance.tint[0],
                instance.tint[1],
                instance.tint[2],
                instance.opacity,
            ];
            // Faded instances keep per-part shadows for their dither.
            let merged = !model.shadow.is_empty() && !faded && culls[SHADOW_VIEW].active;
            for (part_index, part) in model.parts.iter().enumerate() {
                if !joint_visible[part.node] {
                    continue;
                }
                let class_index = match (faded, part.faded_class) {
                    (true, Some(faded)) => faded,
                    _ => part.class,
                };
                let transparent = classes[class_index as usize]
                    .as_ref()
                    .is_some_and(|c| c.transparent);
                let transform = joints[part.node] * part.local;
                let in_view = |view: usize, sphere: &Sphere| -> bool {
                    let cull = &culls[view];
                    cull.active
                        && (view != SHADOW_VIEW
                            || (part.cast_shadow && !(merged && part.merged_shadow)))
                        && (view != REFLECTION_VIEW || instance.reflected)
                        && (!part.frustum_culled
                            || (cull.frustum.intersects_sphere(sphere)
                                && (view != SHADOW_VIEW || shadow_reach.reaches(sphere))))
                };
                let depth = |view: usize, sphere: &Sphere| {
                    (sphere.center - culls[view].origin).dot(culls[view].forward)
                };
                if let Some(Some(range)) = instance.static_ranges.get(part_index) {
                    for view in 0..VIEW_COUNT {
                        if in_view(view, &range.bounds) {
                            builder.push_range(
                                view,
                                class_index,
                                transparent && view != SHADOW_VIEW,
                                part.render_order,
                                depth(view, &range.bounds),
                                range.first,
                                range.count,
                            );
                        }
                    }
                    continue;
                }
                let mut emit = |world: Mat4, color: [f32; 3], data: [f32; 4]| {
                    let sphere = part.bounds.transformed(&world);
                    let mut record = None;
                    for view in 0..VIEW_COUNT {
                        if !in_view(view, &sphere) {
                            continue;
                        }
                        let index = *record.get_or_insert_with(|| {
                            builder.record(InstanceRecord::new(
                                &world,
                                [
                                    tint[0] * color[0],
                                    tint[1] * color[1],
                                    tint[2] * color[2],
                                    tint[3],
                                ],
                                data,
                            ))
                        });
                        builder.push(
                            view,
                            class_index,
                            transparent && view != SHADOW_VIEW,
                            part.render_order,
                            depth(view, &sphere),
                            index,
                        );
                    }
                };
                match &part.instances {
                    Some(list) => {
                        for item in list {
                            emit(
                                transform * item.matrix,
                                item.color,
                                item.data.unwrap_or(instance.data),
                            );
                        }
                    }
                    None => emit(transform, [1.0; 3], instance.data),
                }
            }
            if merged {
                let shadow = &culls[SHADOW_VIEW].frustum;
                if model.scenery {
                    for (group, mesh) in model.shadow.iter().enumerate() {
                        if shadow.intersects_sphere(&mesh.bounds)
                            && shadow_reach.reaches(&mesh.bounds)
                        {
                            merged_items.push(MergedItem {
                                pipeline: mesh.pipeline,
                                model: instance.model,
                                group: group as u32,
                                base: MergedBase::Static(0),
                            });
                        }
                    }
                } else {
                    let first = merged_records.len();
                    let mut sphere: Option<Sphere> = None;
                    for slot in &model.shadow_slots {
                        let record = match slot.map(|index| &model.parts[index]) {
                            Some(part) if joint_visible[part.node] => {
                                let world = joints[part.node] * part.local;
                                let bounds = part.bounds.transformed(&world);
                                sphere = Some(sphere.map_or(bounds, |s| s.union(&bounds)));
                                InstanceRecord::new(&world, tint, instance.data)
                            }
                            _ => ZERO_RECORD,
                        };
                        merged_records.push(record);
                    }
                    match sphere {
                        Some(sphere)
                            if shadow.intersects_sphere(&sphere)
                                && shadow_reach.reaches(&sphere) =>
                        {
                            for (group, mesh) in model.shadow.iter().enumerate() {
                                merged_items.push(MergedItem {
                                    pipeline: mesh.pipeline,
                                    model: instance.model,
                                    group: group as u32,
                                    base: MergedBase::Dynamic(first as u32),
                                });
                            }
                        }
                        _ => merged_records.truncate(first),
                    }
                }
            }
        }
        self.push_pool_draws();
        let base = self.static_records.len() as u32;
        self.builder.finish(base, &mut self.views);
        self.finish_merged(base + self.builder.records.len() as u32);
    }

    /// Group merged shadow items into instanced draws (by pipeline, model and
    /// group) and lay out their record bases; dynamic records start at `dynamic`.
    fn finish_merged(&mut self, dynamic: u32) {
        self.merged_items
            .sort_by_key(|item| (item.pipeline, item.model, item.group));
        self.shadow_bases.clear();
        self.merged_draws.clear();
        for item in &self.merged_items {
            let at = self.shadow_bases.len() as u32;
            self.shadow_bases.push(match item.base {
                MergedBase::Static(record) => record,
                MergedBase::Dynamic(offset) => dynamic + offset,
            });
            match self.merged_draws.last_mut() {
                Some(last) if last.model == item.model && last.group == item.group => {
                    last.count += 1;
                }
                _ => self.merged_draws.push(MergedDraw {
                    model: item.model,
                    group: item.group,
                    first: at,
                    count: 1,
                }),
            }
        }
    }

    /// Upload this frame's merged shadow bases.
    fn upload_shadow_bases(&mut self) {
        let needed = self.shadow_bases.len() as u32;
        if needed > self.shadow_base_capacity {
            self.shadow_base_buffer.destroy();
            self.shadow_base_capacity = needed.next_power_of_two();
            self.shadow_base_buffer = base_buffer(&self.ctx.device, self.shadow_base_capacity);
        }
        if needed > 0 {
            self.ctx.queue.write_buffer(
                &self.shadow_base_buffer,
                0,
                bytemuck::cast_slice(&self.shadow_bases),
            );
        }
    }

    /// Compile pipelines for any class drawn this frame that warm-up missed.
    fn ensure_frame_pipelines(&mut self) {
        let device = &self.ctx.device;
        if self.pipelines.ensure_fixed(device) {
            self.stats.late_pipelines += 1;
        }
        for (view, draws) in self.views.iter().enumerate() {
            for draw in draws.opaque.iter().chain(&draws.transparent) {
                let class = self.classes[draw.class as usize]
                    .as_mut()
                    .expect("live class");
                let (slot, key) = if view == SHADOW_VIEW {
                    (&mut class.shadow, &class.shadow_key)
                } else {
                    (&mut class.main, &class.main_key)
                };
                if slot.is_none() {
                    // Only a real compile counts; a new class may reuse a cached one.
                    if self.pipelines.find(key).is_none() {
                        self.stats.late_pipelines += 1;
                    }
                    *slot = Some(self.pipelines.ensure(device, &self.effects, key));
                }
                if view != SHADOW_VIEW
                    && let (Some(key), None) = (&class.back_key, class.back)
                {
                    if self.pipelines.find(key).is_none() {
                        self.stats.late_pipelines += 1;
                    }
                    class.back = Some(self.pipelines.ensure(device, &self.effects, key));
                }
            }
        }
    }

    /// Draw one frame at `time` seconds (the effect and water clock).
    pub fn render(&mut self, time: f32) -> Result<(), String> {
        if let Some(error) = self.error() {
            return Err(error);
        }
        self.time = time;
        self.meshes.collect_released();
        self.update_textures();
        self.upload_static();
        self.update_culls();
        self.build_draws();
        self.ensure_frame_pipelines();
        let base = self.static_records.len() as u32;
        let dynamic = base + self.builder.records.len() as u32;
        self.ensure_capacity(dynamic + self.merged_records.len() as u32);
        if !self.builder.records.is_empty() {
            self.ctx.queue.write_buffer(
                &self.instance_buffer,
                base as u64 * RECORD_SIZE,
                bytemuck::cast_slice(&self.builder.records),
            );
        }
        if !self.merged_records.is_empty() {
            self.ctx.queue.write_buffer(
                &self.instance_buffer,
                dynamic as u64 * RECORD_SIZE,
                bytemuck::cast_slice(&self.merged_records),
            );
        }
        self.upload_shadow_bases();
        self.write_view_uniforms();
        // The scene and the canvas go in separate command buffers. WebKit paces a
        // WebGPU canvas by the GPU time of the command buffers that write its texture
        // (`WebGPUFramePacer`) and lowers the frame rate when that exceeds a display
        // frame; with the whole frame in one buffer the scene counted against the
        // canvas and Safari settled at 30 fps. The output buffer is one full-screen
        // triangle.
        let mut encoder = self
            .ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("scene"),
            });
        self.encode_scene(&mut encoder, self.reflection_active);
        self.ctx.queue.submit([encoder.finish()]);
        let output = match self.ctx.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.ctx
                    .surface
                    .configure(&self.ctx.device, &self.ctx.config);
                return Ok(());
            }
            error => return Err(format!("Canvas unavailable: {error:?}. Reload to restart.")),
        };
        let view = output.texture.create_view(&Default::default());
        let mut encoder = self
            .ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("output"),
            });
        self.encode_output(&mut encoder, &view);
        self.ctx.queue.submit([encoder.finish()]);
        self.ctx.queue.present(output);
        self.stats.instance_records = dynamic + self.merged_records.len() as u32;
        Ok(())
    }

    /// The sun shadow, water reflection and main view passes, into `main_target`.
    fn encode_scene(&mut self, encoder: &mut wgpu::CommandEncoder, reflection: bool) {
        let stats = &mut self.stats;
        stats.draw_calls = 0;
        stats.triangles = 0;
        let background = hex_to_linear(self.environment.background);
        let clear = wgpu::Color {
            r: background[0] as f64,
            g: background[1] as f64,
            b: background[2] as f64,
            a: 1.0,
        };
        let draws = DrawContext {
            classes: &self.classes,
            meshes: &self.meshes,
            materials: &self.materials,
            pipelines: &self.pipelines,
            pools: &self.pools,
            frame_groups: &self.view_groups,
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sun shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.view_groups[SHADOW_VIEW], &[]);
            let mut count = draws.encode(
                &mut pass,
                &self.views[SHADOW_VIEW].opaque,
                SHADOW_VIEW,
                stats,
            );
            if !self.merged_draws.is_empty() {
                pass.set_bind_group(0, &self.view_groups[SHADOW_VIEW], &[]);
                pass.set_vertex_buffer(1, self.shadow_base_buffer.slice(..));
                let mut pipeline = usize::MAX;
                for draw in &self.merged_draws {
                    let Some(mesh) = self
                        .models
                        .at(draw.model)
                        .and_then(|model| model.shadow.get(draw.group as usize))
                    else {
                        continue;
                    };
                    if mesh.pipeline != pipeline {
                        pass.set_pipeline(&self.pipelines.fixed().shadow_merged[mesh.pipeline]);
                        pipeline = mesh.pipeline;
                    }
                    if let Some(material) = mesh.material {
                        pass.set_bind_group(1, &self.materials.get(material).bind_group, &[]);
                    }
                    pass.set_vertex_buffer(0, mesh.vertex.slice(..));
                    pass.set_index_buffer(mesh.index.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.index_count, 0, draw.first..draw.first + draw.count);
                    count += 1;
                    stats.draw_calls += 1;
                    stats.triangles += (mesh.index_count / 3) as u64 * draw.count as u64;
                }
            }
            stats.shadow_draw_calls = count;
        }
        stats.reflection_draw_calls = 0;
        if let (true, Some(water)) = (reflection, &self.water) {
            let mut pass = scene_pass(encoder, "water reflection", &water.target, clear);
            pass.set_bind_group(0, &self.view_groups[REFLECTION_VIEW], &[]);
            let view = &self.views[REFLECTION_VIEW];
            let count = draws.encode(&mut pass, &view.opaque, REFLECTION_VIEW, stats)
                + draws.encode(&mut pass, &view.transparent, REFLECTION_VIEW, stats);
            stats.reflection_draw_calls = count;
        }
        {
            let mut pass = scene_pass(encoder, "main view", &self.main_target, clear);
            pass.set_bind_group(0, &self.view_groups[MAIN_VIEW], &[]);
            let view = &self.views[MAIN_VIEW];
            draws.encode(&mut pass, &view.opaque, MAIN_VIEW, stats);
            if let Some(water) = &self.water {
                let mesh = self.meshes.get(water.mesh);
                pass.set_pipeline(&self.pipelines.fixed().water);
                pass.set_bind_group(1, &water.bind_group, &[]);
                pass.set_vertex_buffer(0, mesh.vertex.slice(..));
                pass.set_index_buffer(mesh.index.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
                stats.draw_calls += 1;
                stats.triangles += mesh.index_count as u64 / 3;
            }
            draws.encode(&mut pass, &view.transparent, MAIN_VIEW, stats);
        }
    }

    /// Draw `main_target` into the canvas through the output transform.
    fn encode_output(&mut self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView) {
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("output"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipelines.fixed().output);
            pass.set_bind_group(0, &self.output_group, &[]);
            pass.draw(0..3, 0..1);
            self.stats.draw_calls += 1;
        }
    }

    /// Counters for the last frame and current allocations.
    pub fn stats(&self) -> RenderStats {
        let mut stats = self.stats;
        stats.pipelines = self.pipelines.count() as u32;
        stats.shader_modules = self.pipelines.module_count() as u32;
        stats.meshes = self.meshes.count() as u32;
        stats.unused_meshes = self.meshes.unused() as u32;
        stats.materials = self.materials.count() as u32;
        stats.textures = self.textures.count() as u32;
        stats.textures_pending = self.textures.pending() as u32;
        stats.models = self.models.len() as u32;
        stats.instances = self.instances.len() as u32;
        stats.draw_classes = self.classes.iter().flatten().count() as u32;
        // Mesh buffers, material uniforms, 3 view uniforms, instance buffer,
        // output and water uniforms.
        let (pools, pool_instances) = self.pool_totals();
        stats.pools = pools;
        stats.pool_instances = pool_instances;
        stats.buffers = (self.meshes.buffers()
            + self.materials.count()
            + VIEW_COUNT
            + 2
            + self.water.is_some() as usize
            + pools as usize) as u32;
        let shadow = self.sun_shadow.map_size as u64;
        let (width, height) = self.size();
        stats.gpu_bytes = self.meshes.bytes()
            + self.textures.bytes()
            + self.main_target.bytes(SAMPLE_COUNT)
            + self
                .water
                .as_ref()
                .map_or(0, |w| w.target.bytes(SAMPLE_COUNT))
            + shadow * shadow * 4
            + self.instance_capacity as u64 * RECORD_SIZE
            + self.pool_bytes()
            + self
                .models
                .iter()
                .flat_map(|(_, model)| &model.shadow)
                .map(|mesh| mesh.bytes)
                .sum::<u64>()
            + width as u64 * height as u64 * 4;
        stats
    }
}

/// `WaterSurface.waterInView`: whether any view corner ray misses the calm
/// square of the water plane (or the plane itself), so open water shows.
fn water_in_view(camera: &PerspectiveCamera, settings: &WaterSettings) -> bool {
    if settings.calm_extent <= 0.0 {
        return true;
    }
    [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
        .into_iter()
        .any(|(x, y)| {
            camera
                .pick_ground(Vec2::new(x, y), settings.height)
                .is_none_or(|hit| hit.x.abs().max(hit.z.abs()) > settings.calm_extent)
        })
}

fn scene_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    label: &str,
    target: &ColorTarget,
    clear: wgpu::Color,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &target.color_view,
            depth_slice: None,
            resolve_target: Some(&target.resolved_view),
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(clear),
                store: wgpu::StoreOp::Discard,
            },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: &target.depth_view,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Discard,
            }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

struct DrawContext<'a> {
    classes: &'a [Option<ClassEntry>],
    meshes: &'a MeshStore,
    materials: &'a MaterialStore,
    pipelines: &'a Pipelines,
    pools: &'a Slab<PoolEntry>,
    frame_groups: &'a [wgpu::BindGroup],
}

impl DrawContext<'_> {
    /// Encode draws, skipping redundant state changes. Returns the draw count.
    /// The caller binds the view's frame group; pool draws swap in their own
    /// instance buffer and the frame group is restored afterwards.
    fn encode(
        &self,
        pass: &mut wgpu::RenderPass,
        draws: &[Draw],
        view: usize,
        stats: &mut RenderStats,
    ) -> u32 {
        let shadow = view == SHADOW_VIEW;
        let mut last_pipeline = u32::MAX;
        let mut last_material = u32::MAX;
        let mut last_mesh = u32::MAX;
        let mut bound_pool: Option<u32> = None;
        let mut count = 0;
        for draw in draws {
            let Some(class) = &self.classes[draw.class as usize] else {
                continue;
            };
            if class.pool != bound_pool {
                let group = match class.pool {
                    Some(pool) => match self.pools.at(pool) {
                        Some(entry) => &entry.groups[view],
                        None => continue,
                    },
                    None => &self.frame_groups[view],
                };
                pass.set_bind_group(0, group, &[]);
                bound_pool = class.pool;
            }
            let Some(pipeline) = (if shadow { class.shadow } else { class.main }) else {
                continue;
            };
            let back = if shadow { None } else { class.back };
            if class.key.material != last_material {
                pass.set_bind_group(1, &self.materials.get(class.key.material).bind_group, &[]);
                last_material = class.key.material;
            }
            let mesh = self.meshes.get(class.key.mesh);
            if class.key.mesh != last_mesh {
                pass.set_vertex_buffer(0, mesh.vertex.slice(..));
                if let Some(extra) = &mesh.extra {
                    pass.set_vertex_buffer(1, extra.slice(..));
                }
                pass.set_index_buffer(mesh.index.slice(..), wgpu::IndexFormat::Uint32);
                last_mesh = class.key.mesh;
            }
            for pipeline in back.into_iter().chain([pipeline]) {
                if pipeline != last_pipeline {
                    pass.set_pipeline(self.pipelines.get(pipeline));
                    last_pipeline = pipeline;
                }
                pass.draw_indexed(
                    0..mesh.index_count,
                    0,
                    draw.first_instance..draw.first_instance + draw.instance_count,
                );
                count += 1;
                stats.triangles += (mesh.index_count / 3) as u64 * draw.instance_count as u64;
            }
        }
        if bound_pool.is_some() {
            pass.set_bind_group(0, &self.frame_groups[view], &[]);
        }
        stats.draw_calls += count;
        count
    }
}

fn upload_shadow_mesh(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    group: &ShadowGroup,
    material: Option<u32>,
) -> ShadowMesh {
    let vertex = buffer_with_contents(
        device,
        queue,
        "shadow merged vertices",
        bytemuck::cast_slice(&group.vertices),
        wgpu::BufferUsages::VERTEX,
    );
    let index = buffer_with_contents(
        device,
        queue,
        "shadow merged indices",
        bytemuck::cast_slice(&group.indices),
        wgpu::BufferUsages::INDEX,
    );
    ShadowMesh {
        vertex,
        index,
        index_count: group.indices.len() as u32,
        pipeline: shadow_merged_index(group.side, material.is_some()),
        material,
        bounds: group.bounds,
        bytes: (group.vertices.len() * size_of::<crate::shadow_merge::ShadowVertex>()
            + group.indices.len() * 4) as u64,
    }
}
