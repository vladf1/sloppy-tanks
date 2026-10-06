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
//! - Meshes have no buffers of their own: they share vertex and index pages
//!   (`crate::mesh_pages`, `resources.rs`) with absolute indices, so every draw
//!   passes `base_vertex` 0 and a pass rebinds only when the page changes. Draws
//!   bind a page's written prefix, never all of it. A general page left empty is
//!   destroyed at the next `reset_round` or frame collection, a batch or own page
//!   with its last mesh.
//! - Pipelines are cached forever. `prepare_step` compiles the scene's variants
//!   in the background (WebGPU `createRenderPipelineAsync`, WebGL
//!   `KHR_parallel_shader_compile`); call it until nothing remains, then `warm_up`,
//!   before the first gameplay frame. A variant first met while drawing compiles
//!   synchronously and counts as a late pipeline.
//!
//! # Backends
//!
//! Everything above is shared. The browser API is one module chosen at build time:
//! `webgpu` (wgpu on the browser's WebGPU) or `webgl` (WebGL2 through glow, with its
//! own GL state cache). Both offer the same few concrete types (`Gpu`, `Frame`,
//! `Pipelines`, mesh pages, material bindings, instance stores, textures), and
//! their `Frame` draws the draw lists this module builds.

mod context;
mod inspect;
mod lut;
mod pools;
mod resources;
mod textures;
#[cfg(feature = "webgl")]
mod webgl;
#[cfg(not(feature = "webgl"))]
mod webgpu;

#[cfg(feature = "webgl")]
use webgl as backend;
#[cfg(not(feature = "webgl"))]
use webgpu as backend;

pub use context::GRAPHICS_API;
pub use inspect::InstanceState;
pub use pools::PoolId;
pub use textures::image_data;

use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2, Vec3};
use sloppy_core::geometry::Mesh;
use sloppy_core::scene::{Blending, Node, Side, TextureRef};

use crate::camera::{Frustum, PerspectiveCamera, ShadowCamera, ShadowReach, Sphere, mirror_view};
use crate::color::{hex_to_linear, hex_to_linear_scaled};
use crate::draw_list::{
    ClassOrder, Draw, DrawListBuilder, DrawState, InstanceRecord, MAIN_VIEW, REFLECTION_VIEW,
    SHADOW_VIEW, VIEW_COUNT, ViewDraws,
};
use crate::effects::{EffectDefinition, EffectRegistry};
use crate::material::{MaterialInterner, is_transparent};
use crate::mesh_pages::MeshRange;
use crate::model::{
    InstanceData, ModelNode, PartMesh, PreparedModel, SceneryOptions, prepare_model,
    prepare_scenery,
};
use crate::reflection_cull::WaterFootprint;
use crate::shader::{PipelineKey, ShaderKey};
use crate::shadow_merge::{
    MergeKind, ShadowGroup, ShadowMerge, cache_scenery_shadows, merge_shadows, merged_draw_order,
    shadow_merge_kind,
};
use crate::target_memory::target_bytes;
use backend::{Frame, Gpu, Pipelines, WaterGpu};
use pools::PoolEntry;
use resources::{MaterialStore, MeshStore};
use textures::TextureStore;

/// MSAA samples of the main view and the water reflection. WebGL takes the most up
/// to this that its float targets support (`webgl/context.rs` `sample_count`).
pub const SAMPLE_COUNT: u32 = 4;

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
    pub shadow_triangles: u64,
    pub reflection_triangles: u64,
    pub main_triangles: u64,
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
    /// Estimated GPU bytes: mesh pages, textures, attachments, shadow map, instances.
    pub gpu_bytes: u64,
    /// Mesh page bytes no mesh uses (included in `gpu_bytes`).
    pub mesh_slack_bytes: u64,
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
    /// `Pipelines::rank` of the main and shadow keys: classes are created before
    /// their pipelines compile, and draw lists sort by these.
    main_rank: u32,
    shadow_rank: u32,
    /// The mesh pages of its mesh, fixed for the mesh's life.
    vertex_page: u16,
    index_page: u16,
    main: Option<u32>,
    back: Option<u32>,
    shadow: Option<u32>,
    /// Parts using this class that cast shadows.
    casters: u32,
    users: u32,
}

impl ClassEntry {
    /// What its opaque draws bind in `view`, by which the draw lists sort them.
    fn draw_state(&self, view: usize) -> DrawState {
        DrawState {
            pipeline: if view == SHADOW_VIEW {
                self.shadow_rank
            } else {
                self.main_rank
            },
            vertex_page: u32::from(self.vertex_page),
            pool: self.key.pool,
            material: self.key.material,
            index_page: u32::from(self.index_page),
        }
    }
}

struct PartEntry {
    node: usize,
    local: Mat4,
    class: u32,
    faded_class: Option<u32>,
    /// Whether `class` draws blended (the faded class always does), kept here so
    /// building the draw lists reads no class.
    transparent: bool,
    cast_shadow: bool,
    render_order: i32,
    frustum_culled: bool,
    /// Mesh-space bounds.
    bounds: Sphere,
    instances: Option<Vec<InstanceData>>,
    /// Its shadow draws from the model's merged casters (unless faded).
    merged_shadow: bool,
    fixed_shadow: bool,
}

/// Merged shadow-caster pipelines by drawn side (see `shadow_merge.rs`).
const SHADOW_MERGED_SIDES: [Side; 3] = [Side::Front, Side::Back, Side::Double];

/// The merged shadow pipeline for a drawn side; cutout pipelines follow the
/// depth-only ones.
fn shadow_merged_index(side: Side, cutout: bool) -> usize {
    let side = match side {
        Side::Front => 0,
        Side::Back => 1,
        Side::Double => 2,
    };
    side + if cutout { SHADOW_MERGED_SIDES.len() } else { 0 }
}

/// A merged, depth-only caster mesh (`shadow_merge.rs`).
struct ShadowMesh {
    /// Its `ShadowVertex` vertices and absolute indices in the mesh pages.
    range: MeshRange,
    /// Index into `Pipelines::shadow_merged`.
    pipeline: usize,
    /// Cutout groups: the material (store index) whose map and cutoff apply.
    material: Option<u32>,
    /// World bounds (scenery only; models cull per instance).
    bounds: Sphere,
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
    /// Its place in draw order, from its pipeline, the caster's mesh pages, model
    /// and group (`merged_draw_order`).
    order: u128,
    model: u32,
    group: u32,
    base: MergedBase,
}

impl MergedItem {
    fn new(mesh: &ShadowMesh, model: u32, group: usize, base: MergedBase) -> Self {
        let group = group as u32;
        Self {
            // A handful of merged pipelines (`shadow_merged_index`).
            order: merged_draw_order(
                mesh.pipeline as u32,
                mesh.range.vertex_page,
                mesh.range.index_page,
                model,
                group,
            ),
            model,
            group,
            base,
        }
    }
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

/// The record of a hidden part among a model's merged shadow slots: it moves every
/// vertex to the world origin, so the part's triangles have no area.
const ZERO_RECORD: InstanceRecord = InstanceRecord {
    world_rows: [[0.0; 4]; 3],
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
    /// Its model is baked scenery (`ModelEntry::scenery`).
    scenery: bool,
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
    /// Its uniform, reflection target and bindings.
    gpu: WaterGpu,
    /// The texture store's generation its normal map was bound at.
    generation: u64,
    bounds: Sphere,
    footprint: WaterFootprint,
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
    gpu: Gpu,
    /// The main view, shadow maps, view uniforms and instance records.
    frame: Frame,
    pipelines: Pipelines,
    /// A number per pipeline key ever named ([`Self::rank`]).
    ranks: HashMap<PipelineKey, u32>,
    effects: EffectRegistry,
    interner: MaterialInterner,
    textures: TextureStore,
    meshes: MeshStore,
    materials: MaterialStore,
    classes: Vec<Option<ClassEntry>>,
    class_index: HashMap<ClassKey, u32>,
    free_classes: Vec<u32>,
    /// Opaque draws group by the GPU state their class binds, so consecutive draws
    /// skip pipeline, material, mesh page and pool changes
    /// (`backend::DRAW_GROUPING`). The order for the main and reflection views, which
    /// bind a class's main pipeline, and for the shadow view.
    class_order: [ClassOrder; 2],
    models: Slab<ModelEntry>,
    instances: Slab<InstanceEntry>,
    pools: Slab<PoolEntry>,

    static_shadow_dirty: bool,
    cache_static_shadow: bool,
    static_shadow_draws: Vec<Draw>,
    static_merged_draws: Vec<MergedDraw>,
    static_records: Vec<InstanceRecord>,
    static_dirty: bool,

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
    stats: RenderStats,
}

/// What one frame draws, for the backend's passes: the draw lists of every view and
/// what their classes bind.
struct Scene<'a> {
    classes: &'a [Option<ClassEntry>],
    meshes: &'a MeshStore,
    materials: &'a MaterialStore,
    pipelines: &'a Pipelines,
    pools: &'a Slab<PoolEntry>,
    models: &'a Slab<ModelEntry>,
    views: &'a [ViewDraws; VIEW_COUNT],
    merged_draws: &'a [MergedDraw],
    static_shadow_draws: &'a [Draw],
    static_merged_draws: &'a [MergedDraw],
    water: Option<&'a Water>,
    /// Linear clear color of the main view and the reflection.
    background: [f32; 3],
    /// Redraw the cached fixed-scenery shadow first.
    rebuild_static: bool,
    /// Start the sun shadow from the cached fixed-scenery shadow instead of clearing it.
    copy_static: bool,
    reflection: bool,
}

fn water_normals() -> TextureRef {
    TextureRef {
        srgb: false,
        anisotropy: 4,
        ..TextureRef::file("textures/water/normals.webp")
    }
}

impl Renderer {
    /// Create the device and canvas context. Fails when the build's graphics API is
    /// unavailable.
    pub async fn new(
        canvas: web_sys::HtmlCanvasElement,
        options: RendererOptions,
    ) -> Result<Renderer, String> {
        let (gpu, canvas) = Gpu::new(canvas).await?;
        let pipelines = Pipelines::new(&gpu);
        let textures = TextureStore::new(&gpu, options.asset_base);
        let sun_shadow = SunShadow::default();
        let frame = Frame::new(&gpu, canvas, sun_shadow.map_size, INITIAL_INSTANCE_CAPACITY);
        // A target the browser cannot draw into fails here, where the page can tell
        // an unavailable API from other errors.
        if let Some(error) = gpu.error() {
            return Err(error);
        }
        Ok(Renderer {
            pipelines,
            ranks: HashMap::new(),
            effects: EffectRegistry::default(),
            interner: MaterialInterner::default(),
            textures,
            meshes: MeshStore::default(),
            materials: MaterialStore::default(),
            classes: Vec::new(),
            class_index: HashMap::new(),
            free_classes: Vec::new(),
            class_order: [0, 1].map(|_| ClassOrder::new(backend::DRAW_GROUPING)),
            models: Slab::default(),
            instances: Slab::default(),
            pools: Slab::default(),
            static_shadow_dirty: true,
            cache_static_shadow: false,
            static_shadow_draws: Vec::new(),
            static_merged_draws: Vec::new(),
            static_records: vec![InstanceRecord::IDENTITY],
            static_dirty: true,
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
            stats: RenderStats::default(),
            frame,
            gpu,
        })
    }

    // ---------------------------------------------------------------- setup

    /// Resize the canvas drawing buffer (device pixels). Superseded attachments
    /// are destroyed immediately.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.frame.resize(&self.gpu, width, height);
    }

    pub fn size(&self) -> (u32, u32) {
        self.frame.size()
    }

    /// The first GPU validation error or device loss, if any.
    pub fn error(&self) -> Option<String> {
        self.gpu.error()
    }

    /// Add or replace a custom material effect; returns its id.
    pub fn register_effect(&mut self, effect: EffectDefinition) -> u16 {
        self.effects.register(effect)
    }

    pub fn set_environment(&mut self, environment: Environment) {
        // The sun places the shadow camera that the fixed scenery cache used.
        if environment.sun_position != self.environment.sun_position
            || environment.sun_target != self.environment.sun_target
        {
            self.static_shadow_dirty = true;
        }
        self.environment = environment;
    }

    pub fn environment(&self) -> &Environment {
        &self.environment
    }

    pub fn set_sun_shadow(&mut self, shadow: SunShadow) {
        if shadow.map_size != self.sun_shadow.map_size {
            self.frame
                .set_shadow_size(&self.gpu, shadow.map_size.max(1));
            self.rebuild_pool_groups();
        }
        self.sun_shadow = shadow;
        self.static_shadow_dirty = true;
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
            self.meshes.remove_user(water.mesh);
            // Dropping it destroys its uniform and reflection target.
            drop(water);
            // The surface mesh is usually rebuilt per map; free it now if unused.
            self.meshes.collect_unused();
        }
        let Some(settings) = settings else {
            return;
        };
        let mesh = self.meshes.shared(&self.gpu, &settings.mesh, &[]);
        self.meshes.get_mut(mesh).users += 1;
        let normals = water_normals();
        self.textures.request(&normals);
        let size = settings.reflection_size.max(1);
        let mut bounds = self.meshes.get(mesh).bounds;
        bounds.center.y += settings.height;
        let sampler = self.textures.sampler(&self.gpu, Some(&normals));
        let (view, _) = self.textures.view(&normals);
        let gpu = WaterGpu::new(&self.gpu, &self.frame, size, view, sampler);
        let footprint = WaterFootprint::new(&settings.mesh, settings.height, settings.calm_extent);
        self.water = Some(Water {
            footprint,
            settings,
            mesh,
            normals,
            gpu,
            generation: self.textures.generation,
            bounds,
        });
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
                let range = self.meshes.get(mesh).range;
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
                let main = if two_pass {
                    main_key(&face(Side::Front))
                } else {
                    main_key(source)
                };
                let shadow = PipelineKey::shadow(shadow_shader, source);
                let entry = ClassEntry {
                    key,
                    pool,
                    transparent,
                    main_key: main,
                    back_key: two_pass.then(|| main_key(&face(Side::Back))),
                    shadow_key: shadow,
                    main_rank: pipeline_rank(&mut self.ranks, &main),
                    shadow_rank: pipeline_rank(&mut self.ranks, &shadow),
                    vertex_page: range.vertex_page,
                    index_page: range.index_page,
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
                let class = self.classes[index as usize].as_ref().expect("live class");
                for (order, view) in self.class_order.iter_mut().zip([MAIN_VIEW, SHADOW_VIEW]) {
                    order.insert(index, class.draw_state(view));
                }
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
            for order in &mut self.class_order {
                order.remove(index);
            }
        }
    }

    fn register(
        &mut self,
        mut prepared: PreparedModel,
        lifetime: Lifetime,
        scenery: bool,
    ) -> ModelId {
        let gpu = self.gpu.clone();
        let ShadowMerge {
            groups,
            slots,
            merged,
        } = {
            let effects = &self.effects;
            merge_shadows(
                &prepared,
                |index| shadow_merge_kind(effects, &prepared.parts[index].material),
                scenery,
                SHADOW_MERGE_CELL,
            )
        };
        let groups: Vec<ShadowGroup> = groups
            .into_iter()
            .filter(|group| !group.indices.is_empty())
            .collect();
        // The whole model is known before anything uploads, so a large one gets batch
        // pages of its own.
        let reservation = self.meshes.reserve(&gpu, &prepared.meshes, &groups);
        let mut entry_of = vec![None; prepared.parts.len()];
        let owned: Vec<u32> = prepared
            .meshes
            .iter_mut()
            .zip(reservation.owned)
            .map(|(data, placement)| self.meshes.owned(&gpu, data, placement))
            .collect();
        let mut parts = Vec::with_capacity(prepared.parts.len());
        for (prepared_index, part) in prepared.parts.iter().enumerate() {
            let material = self.materials.get_or_create(
                &gpu,
                &mut self.textures,
                &self.effects,
                &part.material,
            );
            let attributes = self.effects.attributes(self.materials.get(material).effect);
            let mesh = match &part.mesh {
                PartMesh::Shared(mesh) => self.meshes.shared(&gpu, mesh, attributes),
                PartMesh::Owned(index) => owned[*index],
            };
            if self.meshes.get(mesh).range.is_empty() {
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
                transparent: self.classes[class as usize]
                    .as_ref()
                    .is_some_and(|class| class.transparent),
                cast_shadow: part.cast_shadow,
                render_order: part.render_order,
                frustum_culled: part.frustum_culled,
                bounds: self.meshes.get(mesh).bounds,
                instances: part.instances.clone(),
                merged_shadow: merged[prepared_index],
                fixed_shadow: shadow_merge_kind(&self.effects, &part.material)
                    != MergeKind::Separate,
            });
        }
        let shadow_slots = slots.iter().map(|&index| entry_of[index]).collect();
        let shadow = groups
            .into_iter()
            .zip(reservation.shadow)
            .map(|(mut group, placement)| {
                let material = group.cutout.as_ref().map(|material| {
                    let index = self.materials.get_or_create(
                        &gpu,
                        &mut self.textures,
                        &self.effects,
                        material,
                    );
                    self.materials.get_mut(index).users += 1;
                    index
                });
                ShadowMesh {
                    range: self.meshes.shadow(&gpu, &mut group, placement),
                    pipeline: shadow_merged_index(group.side, material.is_some()),
                    material,
                    bounds: group.bounds,
                }
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
            self.meshes.free_range(mesh.range);
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
        if entry.scenery {
            self.static_dirty = true;
        }
        let joints = entry.skeleton.nodes.len();
        let node_visible = entry
            .skeleton
            .nodes
            .iter()
            .map(|node| node.visible)
            .collect();
        let scenery = entry.scenery;
        let (index, generation) = self.instances.insert(InstanceEntry {
            model: model.index,
            scenery,
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
        let instance = self.instances.get_mut(id.index, id.generation)?;
        // Scenery bakes its instance state into the static records.
        self.static_dirty |= instance.scenery;
        Some(instance)
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
            if entry.scenery {
                self.static_dirty = true;
            }
        }
    }

    /// Release everything created with `Lifetime::Round`, then free GPU meshes and
    /// materials that nothing uses and no caller still holds, and destroy the general
    /// mesh pages that left empty, so the next round starts from the pages that still
    /// hold meshes.
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
        // General pages this empties stay for the next round's uploads, which
        // follow at once (`View::reset`); the next frame's `collect_released` trims
        // the ones those leave empty.
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
        let fixed_pending = !self.pipelines.fixed_ready(&self.gpu);
        let mut compiled = 0;
        let gpu = &self.gpu;
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
                        .request(gpu, &self.effects, key, compiled < budget);
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
        let gpu = &self.gpu;
        self.pipelines.ensure_fixed(gpu);
        for class in self.classes.iter_mut().flatten() {
            if class.main.is_none() {
                class.main = Some(self.pipelines.ensure(gpu, &self.effects, &class.main_key));
            }
            if let (Some(key), None) = (&class.back_key, class.back) {
                class.back = Some(self.pipelines.ensure(gpu, &self.effects, key));
            }
            if class.casters > 0 && class.shadow.is_none() {
                class.shadow = Some(self.pipelines.ensure(gpu, &self.effects, &class.shadow_key));
            }
        }
    }

    /// Ask to be told when the GPU has run everything submitted so far, including the
    /// compilation of every pipeline created before; [`Self::gpu_idle`] turns true then.
    pub fn await_gpu(&mut self) {
        self.frame.await_gpu(&self.gpu);
    }

    pub fn gpu_idle(&self) -> bool {
        self.frame.gpu_idle(&self.gpu)
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
        self.draw_frame(false, true)?;
        // Warm-up uses representative draws, not the complete fixed scenery.
        self.static_shadow_dirty = true;
        for draws in &mut self.views {
            draws.clear();
        }
        self.merged_draws.clear();
        self.error().map_or(Ok(()), Err)
    }

    // ---------------------------------------------------------------- frame

    fn update_textures(&mut self) {
        if self.textures.drain(&self.gpu) {
            self.static_shadow_dirty = true;
            self.materials.refresh(&self.gpu, &mut self.textures);
            // Like the materials, also after a replaced texture.
            if let Some(water) = &mut self.water
                && water.generation != self.textures.generation
            {
                let sampler = self.textures.sampler(&self.gpu, Some(&water.normals));
                let (view, _) = self.textures.view(&water.normals);
                water.gpu.rebind(&self.gpu, &self.frame, view, sampler);
                water.generation = self.textures.generation;
            }
        }
    }

    fn ensure_capacity(&mut self, records: u32) {
        if records <= self.frame.instances().capacity() {
            return;
        }
        self.frame
            .grow_instances(&self.gpu, records.next_power_of_two());
        // The static records are current (uploaded earlier this frame); keep them.
        self.frame
            .instances()
            .write(&self.gpu, 0, &self.static_records);
        self.rebuild_pool_groups();
    }

    /// Rebuild the persistent records of static instanced scenery.
    fn upload_static(&mut self) {
        if !self.static_dirty {
            return;
        }
        self.static_dirty = false;
        self.static_shadow_dirty = true;
        let triangles = self
            .instances
            .iter()
            // Themes hide other maps' shared scenery; only shown sets cast.
            .filter(|(_, instance)| instance.visible)
            .filter_map(|(_, instance)| {
                self.models.at(instance.model).filter(|model| model.scenery)
            })
            .map(|model| {
                let merged: u64 = model
                    .shadow
                    .iter()
                    .map(|mesh| u64::from(mesh.range.index_count / 3))
                    .sum();
                let separate: u64 = model
                    .parts
                    .iter()
                    .filter(|part| part.cast_shadow && part.fixed_shadow && !part.merged_shadow)
                    .map(|part| {
                        let class = self.classes[part.class as usize]
                            .as_ref()
                            .expect("live class");
                        u64::from(self.meshes.get(class.key.mesh).range.index_count / 3)
                            * part.instances.as_ref().map_or(1, |list| list.len() as u64)
                    })
                    .sum();
                merged + separate
            })
            .sum();
        self.cache_static_shadow = cache_scenery_shadows(triangles);
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
        self.frame
            .instances()
            .write(&self.gpu, 0, &self.static_records);
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
                let size = water.settings.reflection_size.max(1);
                self.frame_uniform(projection * view, view, eye, (size, size))
            }
            _ => main,
        };
        let mut views = [main; VIEW_COUNT];
        views[REFLECTION_VIEW] = reflection;
        views[SHADOW_VIEW] = shadow;
        self.frame.write_views(&self.gpu, &views);
        self.frame
            .write_output(&self.gpu, self.environment.exposure);
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
            water.gpu.write(&self.gpu, &uniform);
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
        let mut reflection_shadow_view = None;
        if let Some(water) = &self.water
            && water.settings.reflection
            && self.culls[MAIN_VIEW]
                .frustum
                .intersects_sphere(&water.bounds)
            && water_in_view(&camera, &water.settings)
            && let Some((view, projection)) = self.mirror()
            && let Some(bounds) = water.footprint.reflection_bounds(
                &camera,
                water.settings.distortion_scale,
                water.settings.reflection_size,
            )
        {
            let world = view.inverse();
            // Cull with the plain projection: the oblique near plane also skews
            // the far plane, which would reject most of the reflected scene.
            let _ = projection;
            let plain = self.camera().projection() * view;
            reflection_shadow_view = Some(Frustum::from_view_projection(&plain));
            self.culls[REFLECTION_VIEW] = ViewCull {
                active: true,
                frustum: bounds.frustum(plain),
                origin: world.w_axis.truncate(),
                forward: -world.z_axis.truncate(),
            };
            self.reflection_active = true;
        }
        self.shadow_reach = ShadowReach {
            light: self.culls[SHADOW_VIEW].forward,
            floor: self.sun_shadow.receiver_floor,
            views: [Some(self.culls[MAIN_VIEW].frustum), reflection_shadow_view],
        };
    }

    fn build_draws(&mut self) {
        let Self {
            instances,
            models,
            builder,
            joints,
            joint_visible,
            culls,
            shadow_reach,
            static_shadow_dirty,
            cache_static_shadow,
            static_shadow_draws,
            merged_records,
            merged_items,
            ..
        } = self;
        builder.clear();
        static_shadow_draws.clear();
        self.static_merged_draws.clear();
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
                let (class_index, transparent) = match (faded, part.faded_class) {
                    (true, Some(faded)) => (faded, true),
                    _ => (part.class, part.transparent),
                };
                // The views the part may enter before its bounds are culled, decided
                // once rather than per instance of an instanced part.
                let candidate: [bool; VIEW_COUNT] = std::array::from_fn(|view| {
                    culls[view].active
                        && match view {
                            SHADOW_VIEW => {
                                part.cast_shadow
                                    && !(merged && part.merged_shadow)
                                    && !(*cache_static_shadow
                                        && model.scenery
                                        && part.fixed_shadow
                                        && !faded)
                            }
                            REFLECTION_VIEW => instance.reflected,
                            _ => true,
                        }
                });
                let in_view = |view: usize, sphere: &Sphere| -> bool {
                    candidate[view]
                        && (!part.frustum_culled
                            || (culls[view].frustum.intersects_sphere(sphere)
                                && (view != SHADOW_VIEW || shadow_reach.reaches(sphere))))
                };
                // Only blended draws sort by depth; opaque draws ignore it.
                let depth = |view: usize, sphere: &Sphere| {
                    if transparent && view != SHADOW_VIEW {
                        (sphere.center - culls[view].origin).dot(culls[view].forward)
                    } else {
                        0.0
                    }
                };
                if let Some(Some(range)) = instance.static_ranges.get(part_index) {
                    if *cache_static_shadow
                        && *static_shadow_dirty
                        && model.scenery
                        && part.fixed_shadow
                        && !faded
                        && part.cast_shadow
                        && !part.merged_shadow
                        && culls[SHADOW_VIEW].active
                        && (!part.frustum_culled
                            || culls[SHADOW_VIEW].frustum.intersects_sphere(&range.bounds))
                    {
                        static_shadow_draws.push(Draw {
                            class: class_index,
                            first_instance: range.first,
                            instance_count: range.count,
                        });
                    }
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
                let transform = joints[part.node] * part.local;
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
                    if *cache_static_shadow && !*static_shadow_dirty {
                        continue;
                    }
                    for (group, mesh) in model.shadow.iter().enumerate() {
                        // Fixed scenery must cover every camera, including future views.
                        if shadow.intersects_sphere(&mesh.bounds)
                            && (*cache_static_shadow || shadow_reach.reaches(&mesh.bounds))
                        {
                            merged_items.push(MergedItem::new(
                                mesh,
                                instance.model,
                                group,
                                MergedBase::Static(0),
                            ));
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
                                merged_items.push(MergedItem::new(
                                    mesh,
                                    instance.model,
                                    group,
                                    MergedBase::Dynamic(first as u32),
                                ));
                            }
                        }
                        _ => merged_records.truncate(first),
                    }
                }
            }
        }
        self.push_pool_draws();
        let base = self.static_records.len() as u32;
        let [main, shadow] = [0, 1].map(|view| self.class_order[view].positions());
        self.builder
            .finish(base, &mut self.views, [main, main, shadow]);
        self.finish_merged(base + self.builder.records.len() as u32);
        self.merged_draws.retain(|draw| {
            if self.cache_static_shadow
                && self
                    .models
                    .at(draw.model)
                    .is_some_and(|model| model.scenery)
            {
                self.static_merged_draws.push(*draw);
                false
            } else {
                true
            }
        });
    }

    /// Group merged shadow items into instanced draws (by pipeline, mesh page, model
    /// and group) and lay out their record bases; dynamic records start at `dynamic`.
    fn finish_merged(&mut self, dynamic: u32) {
        // Depth-only casters: order changes nothing. A group's pages follow from its
        // model and group, so each group's items stay together.
        self.merged_items.sort_unstable_by_key(|item| item.order);
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
        self.frame.write_shadow_bases(&self.gpu, &self.shadow_bases);
    }

    /// Compile pipelines for any class drawn this frame that warm-up missed.
    fn ensure_frame_pipelines(&mut self) {
        let gpu = &self.gpu;
        if self.pipelines.ensure_fixed(gpu) {
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
                    *slot = Some(self.pipelines.ensure(gpu, &self.effects, key));
                }
                if view != SHADOW_VIEW
                    && let (Some(key), None) = (&class.back_key, class.back)
                {
                    if self.pipelines.find(key).is_none() {
                        self.stats.late_pipelines += 1;
                    }
                    class.back = Some(self.pipelines.ensure(gpu, &self.effects, key));
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
            self.frame
                .instances()
                .write(&self.gpu, base, &self.builder.records);
        }
        if !self.merged_records.is_empty() {
            self.frame
                .instances()
                .write(&self.gpu, dynamic, &self.merged_records);
        }
        self.upload_shadow_bases();
        self.write_view_uniforms();
        self.draw_frame(self.reflection_active, false)?;
        self.stats.instance_records = dynamic + self.merged_records.len() as u32;
        Ok(())
    }

    /// Draw the sun shadow, water reflection and main view passes, then the output:
    /// into the canvas, or for `warm_up` into an offscreen probe.
    fn draw_frame(&mut self, reflection: bool, warm_up: bool) -> Result<(), String> {
        let copy_static = self.sun_shadow.enabled && self.cache_static_shadow;
        let scene = Scene {
            classes: &self.classes,
            meshes: &self.meshes,
            materials: &self.materials,
            pipelines: &self.pipelines,
            pools: &self.pools,
            models: &self.models,
            views: &self.views,
            merged_draws: &self.merged_draws,
            static_shadow_draws: &self.static_shadow_draws,
            static_merged_draws: &self.static_merged_draws,
            water: self.water.as_ref(),
            background: hex_to_linear(self.environment.background),
            rebuild_static: copy_static && self.static_shadow_dirty,
            copy_static,
            reflection,
        };
        let stats = &mut self.stats;
        stats.draw_calls = 0;
        stats.triangles = 0;
        let result = if warm_up {
            self.frame.warm_up(&self.gpu, &scene, stats)
        } else {
            self.frame.draw(&self.gpu, &scene, stats)
        };
        stats.main_triangles =
            stats.triangles - stats.shadow_triangles - stats.reflection_triangles;
        if copy_static {
            self.static_shadow_dirty = false;
        }
        result
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
        // Mesh pages, material uniforms, 3 view uniforms, instance buffer, output
        // and water uniforms.
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
            + target_bytes(width, height, self.gpu.samples)
            + self.water.as_ref().map_or(0, |w| {
                let size = w.settings.reflection_size.max(1);
                target_bytes(size, size, self.gpu.samples)
            })
            + shadow * shadow * 8
            + self.frame.instances().bytes()
            + self.pool_bytes()
            + width as u64 * height as u64 * 4;
        stats.mesh_slack_bytes = self.meshes.slack_bytes();
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

/// A number for `key`, the same for the page's lifetime and known before the pipeline
/// exists, by which opaque draws sort (`DrawState::pipeline`).
fn pipeline_rank(ranks: &mut HashMap<PipelineKey, u32>, key: &PipelineKey) -> u32 {
    let next = ranks.len() as u32;
    *ranks.entry(*key).or_insert(next)
}
