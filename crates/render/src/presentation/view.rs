//! The browser presentation (`presentation.ts` `Presentation`): owns the
//! renderer and runtime effects, keeps one renderer instance per simulation
//! entity, and poses them from each frame's `RenderState`.

use std::collections::{HashMap, VecDeque};
use std::f64::consts::PI;
use std::sync::Arc;

use glam::{DMat4, DVec3, Mat4, Quat, Vec3};
use sloppy_core::geometry::{Aabb, node_bounds};
use sloppy_core::models::{
    FLAG_CLOTH_NODE, aged_wreck_material, custom_floor, custom_spawn_pads, flags_model, part,
    pickup_cube, tank_model, tank_visual_muzzle, wreck_brightness, wreck_model,
};
use sloppy_core::scene::Node;
use sloppy_core::sim::ammunition::AMMO_RESPAWN_SECONDS;
use sloppy_core::sim::data::{ARENA, LASER_DEFENSE, vehicle};
use sloppy_core::sim::debris_cleanup::debris_cleanup_progress;
use sloppy_core::sim::render_state::{RenderCover, RenderFragment, RenderTank};
use sloppy_core::sim::simulation::WreckView;
use sloppy_core::sim::simulation_rules::FRAGMENT_CAPACITY;
use sloppy_core::sim::timber_layout::{TimberWall, timber_parts};
use sloppy_core::sim::veterancy::rank_index;
use sloppy_core::sim::{
    CoverKind, FragmentShape, MatchPhase, PickupKind, RenderState, SimEvent, SimEventType, Team,
    VehicleKind, WreckPart,
};

use super::camera_rig::{CameraRig, ViewerPose};
use super::generated::{GeneratedTextures, SOIL_ROWS_PER_STEP};
use super::hud::{HealthColor, health_bar_state, protection_meters, spawn_pulse};
use super::model_catalog::{
    self, CoverModel, SceneryMover, SceneryWater, TreeParts, cover_damage_stage, cover_key,
    tree_branch_stage,
};
use super::models::{self as own, joint};
use super::posing::{JointBasis, dvec3, euler_xyz, euler_yxz, joint_world, quat, vec3};
use super::preparation::Preparation;
use super::suspension::TankSuspension;
use super::theme::{
    SHADOW_BIAS, SHADOW_MAP_SIZE, SHADOW_NORMAL_BIAS, SHADOW_RECEIVER_FLOOR, Theme, ThemeLook,
    theme_look,
};
use super::view_settings::{BAR_HEIGHT, FEEDBACK, FIRST_PERSON, PLAYER_BAR_HEIGHT, RETICLE_HEIGHT};
use super::{CosmeticRandom, PRESENTATION_EFFECTS};
use crate::color::hex_to_linear;
use crate::effects::Effects;
use crate::effects::spawn_pad_decks::SpawnPadDecks;
use crate::gpu::{
    Environment, Fog, InstanceId, Lifetime, ModelId, PrepareProgress, Renderer, SunShadow,
    WaterSettings,
};

mod inspect;

pub use inspect::{
    CoverInspection, FragmentInspection, PickupInspection, ReticleInspection, TankInspection,
    ViewInspection,
};

/// Hit shake: render-only recoil when a tank takes damage.
const HIT_SHAKE: [f64; 4] = [0.12, 0.09, 0.035, 0.045];
/// A dead tank's model hides; the live model rides this far below the body.
const TANK_MODEL_DROP: f64 = 0.4;
/// Barrel travel at full recoil.
const RECOIL_TRAVEL: f64 = 0.2;
/// Track tread scroll: metres of scroll per metre driven, and its wrap length.
const TRACK_SCROLL: f64 = 0.4;
const TRACK_PERIOD: f64 = 0.25;
/// Pickup gems bob and spin above their pads.
const GEM_HEIGHT: f64 = 1.2;
const GEM_BOB: f64 = 0.18;
/// Pickup glow proportions around the collecting tank.
const GLOW_SCALE: Vec3 = Vec3::new(1.65, 1.25, 1.9);
const GLOW_HEIGHT: f32 = 1.1;
/// Felling boughs (`tree-debris.ts`).
const MAX_BRANCHES: usize = 32;
const BRANCH_LIFETIME: f64 = 6.0;
const BRANCH_GRAVITY: f64 = 9.8;
const BRANCH_REST: f32 = 0.03;
const BRANCH_SINK: f64 = 0.6;
/// Debris sinks by its height plus this margin while it fades.
const SINK_MARGIN: f32 = 0.03;
/// Harbor water plane edge and heights come from `WaterSettings`.
const CUSTOM_FLOOR_Y: f64 = 0.008;
const CUSTOM_OUTER_FLOOR_Y: f64 = -0.002;

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn linear(hex: u32) -> [f32; 3] {
    hex_to_linear(hex)
}

const WRECK_PARTS: [WreckPart; 5] = [
    WreckPart::Intact,
    WreckPart::Hull,
    WreckPart::Turret,
    WreckPart::TurretBarrel,
    WreckPart::Barrel,
];

const PICKUP_KINDS: [PickupKind; 9] = [
    PickupKind::Spread,
    PickupKind::Rocket,
    PickupKind::Ricochet,
    PickupKind::Piercing,
    PickupKind::Rapid,
    PickupKind::Shield,
    PickupKind::Speed,
    PickupKind::Repair,
    PickupKind::Laser,
];

const DEBRIS_SHAPES: [FragmentShape; 10] = [
    FragmentShape::Armor,
    FragmentShape::Wheel,
    FragmentShape::Track,
    FragmentShape::Shard,
    FragmentShape::Wood,
    FragmentShape::Panel,
    FragmentShape::Beam,
    FragmentShape::Log,
    FragmentShape::DrumShell,
    FragmentShape::DrumLid,
];

/// Height of `bounds` under `world`, like `Box3.setFromObject` after a pose.
fn world_height(bounds: &Aabb, world: &Mat4) -> f32 {
    let mut low = f32::INFINITY;
    let mut high = f32::NEG_INFINITY;
    for i in 0..8 {
        let corner = Vec3::new(
            if i & 1 == 0 {
                bounds.min.x
            } else {
                bounds.max.x
            } as f32,
            if i & 2 == 0 {
                bounds.min.y
            } else {
                bounds.max.y
            } as f32,
            if i & 4 == 0 {
                bounds.min.z
            } else {
                bounds.max.z
            } as f32,
        );
        let y = world.transform_point3(corner).y;
        low = low.min(y);
        high = high.max(y);
    }
    high - low
}

fn lowest(bounds: &Aabb, world: &Mat4) -> f32 {
    (0..8)
        .map(|i| {
            let corner = Vec3::new(
                if i & 1 == 0 {
                    bounds.min.x
                } else {
                    bounds.max.x
                } as f32,
                if i & 2 == 0 {
                    bounds.min.y
                } else {
                    bounds.max.y
                } as f32,
                if i & 4 == 0 {
                    bounds.min.z
                } else {
                    bounds.max.z
                } as f32,
            );
            world.transform_point3(corner).y
        })
        .fold(f32::INFINITY, f32::min)
}

// ------------------------------------------------------------------ shared models

struct TankModel {
    model: ModelId,
    joints: usize,
    hull: JointBasis,
    turret: JointBasis,
    barrel: JointBasis,
    track: JointBasis,
    muzzle_height: f64,
}

struct BarModel {
    model: ModelId,
    fills: [JointBasis; 3],
    ranks: [usize; 3],
    shield: JointBasis,
    shield_fill: JointBasis,
    spawn: JointBasis,
    spawn_fill: JointBasis,
}

struct PickupModel {
    base: ModelId,
    gem: ModelId,
    ring: usize,
    ring_dim: usize,
    refill: usize,
}

struct MineModel {
    model: ModelId,
    light: usize,
}

struct BoundedModel {
    model: ModelId,
    bounds: Aabb,
}

struct ReticleModel {
    instance: InstanceId,
    ready: usize,
    reloading: usize,
    confirmed: usize,
}

/// Models shared by every round: built on first use and kept.
#[derive(Default)]
struct Library {
    tanks: HashMap<(VehicleKind, Team), TankModel>,
    bars: HashMap<Team, BarModel>,
    wrecks: HashMap<(VehicleKind, Team, WreckPart), BoundedModel>,
    pickups: HashMap<PickupKind, PickupModel>,
    mines: HashMap<Team, MineModel>,
    debris: HashMap<FragmentShape, BoundedModel>,
    effect_ring: Option<ModelId>,
    effect_glow: Option<ModelId>,
}

fn joint_index(renderer: &Renderer, model: ModelId, name: &str) -> usize {
    renderer
        .model_node(model, name)
        .unwrap_or_else(|| panic!("model joint {name}"))
}

fn basis(renderer: &Renderer, model: ModelId, source: &Node, name: &str) -> JointBasis {
    JointBasis::new(renderer.model_nodes(model), source, name)
        .unwrap_or_else(|| panic!("model joint {name}"))
}

impl Library {
    fn tank(&mut self, renderer: &mut Renderer, kind: VehicleKind, team: Team) -> &TankModel {
        self.tanks.entry((kind, team)).or_insert_with(|| {
            let source = tank_model(kind, team);
            let model = renderer.add_model(&source, Lifetime::Shared);
            TankModel {
                model,
                joints: renderer.model_nodes(model).len(),
                hull: basis(renderer, model, &source, part::HULL),
                turret: basis(renderer, model, &source, part::TURRET),
                barrel: basis(renderer, model, &source, part::BARREL),
                track: basis(renderer, model, &source, part::TRACK_GROUP),
                muzzle_height: tank_visual_muzzle(kind).y,
            }
        })
    }

    fn bar(&mut self, renderer: &mut Renderer, team: Team) -> &BarModel {
        self.bars.entry(team).or_insert_with(|| {
            let source = own::tank_bar(team);
            let model = renderer.add_model(&source, Lifetime::Shared);
            BarModel {
                model,
                fills: joint::BAR_FILLS.map(|name| basis(renderer, model, &source, name)),
                ranks: joint::BAR_RANKS.map(|name| joint_index(renderer, model, name)),
                shield: basis(renderer, model, &source, joint::BAR_SHIELD),
                shield_fill: basis(renderer, model, &source, joint::BAR_SHIELD_FILL),
                spawn: basis(renderer, model, &source, joint::BAR_SPAWN),
                spawn_fill: basis(renderer, model, &source, joint::BAR_SPAWN_FILL),
            }
        })
    }

    fn wreck(
        &mut self,
        renderer: &mut Renderer,
        kind: VehicleKind,
        team: Team,
        part: WreckPart,
    ) -> &BoundedModel {
        self.wrecks.entry((kind, team, part)).or_insert_with(|| {
            let mut source = (*wreck_model(kind, team, part)).clone();
            age_materials(&mut source);
            BoundedModel {
                model: renderer.add_model(&source, Lifetime::Shared),
                bounds: node_bounds(&source, DMat4::IDENTITY),
            }
        })
    }

    fn pickup(&mut self, renderer: &mut Renderer, kind: PickupKind) -> &PickupModel {
        self.pickups.entry(kind).or_insert_with(|| {
            let base = renderer.add_model(&own::pickup_base(kind), Lifetime::Shared);
            PickupModel {
                gem: renderer.add_model(&pickup_cube(kind), Lifetime::Shared),
                ring: joint_index(renderer, base, joint::PICKUP_RING),
                ring_dim: joint_index(renderer, base, joint::PICKUP_RING_DIM),
                refill: joint_index(renderer, base, joint::PICKUP_REFILL),
                base,
            }
        })
    }

    fn mine(&mut self, renderer: &mut Renderer, team: Team) -> &MineModel {
        self.mines.entry(team).or_insert_with(|| {
            let model = renderer.add_model(&own::mine(team), Lifetime::Shared);
            MineModel {
                light: joint_index(renderer, model, joint::MINE_LIGHT),
                model,
            }
        })
    }

    fn debris(&mut self, renderer: &mut Renderer, shape: FragmentShape) -> &BoundedModel {
        self.debris.entry(shape).or_insert_with(|| {
            let source = own::debris_piece(shape)
                .unwrap_or_else(|| model_catalog::surface_debris_piece(shape));
            BoundedModel {
                model: renderer.add_model(&source, Lifetime::Shared),
                bounds: node_bounds(&source, DMat4::IDENTITY),
            }
        })
    }

    fn effect_models(&mut self, renderer: &mut Renderer) -> (ModelId, ModelId) {
        let ring = *self.effect_ring.get_or_insert_with(|| {
            renderer.add_model(&own::pickup_effect_ring(), Lifetime::Shared)
        });
        let glow = *self.effect_glow.get_or_insert_with(|| {
            renderer.add_model(&own::pickup_effect_glow(), Lifetime::Shared)
        });
        (ring, glow)
    }
}

// ------------------------------------------------------------------ entity views

struct TankView {
    kind: VehicleKind,
    team: Team,
    instance: InstanceId,
    bar: InstanceId,
    suspension: Option<TankSuspension>,
    /// Joint overrides this frame (for the first-person eye).
    pose: Vec<Option<Mat4>>,
    world: Mat4,
}

struct BranchView {
    joint: usize,
    drop_stage: u32,
    /// Its index among the source crown's children.
    crown_child: usize,
    shown: bool,
}

struct TreeView {
    crown: usize,
    cut: usize,
    branches: Vec<BranchView>,
    branch_stage: u32,
}

struct CoverView {
    instance: InstanceId,
    /// The cover's joint when it lives in the round's combined static-cover
    /// model (hidden instead of removed); `None` for its own instance.
    joint: Option<usize>,
    key: String,
    stage: u32,
    hits: usize,
    movable: bool,
    scale: Vec3,
    world: Mat4,
    tree: Option<TreeView>,
}

struct CoverModelEntry {
    model: ModelId,
    users: u32,
    /// A cover inside the combined static-cover model, which outlives it.
    combined: bool,
    /// Where the model was built; other covers sharing it are offset from here.
    origin: DVec3,
    source: Arc<Node>,
    tree: Option<TreeParts>,
}

enum FragmentLook {
    Piece,
    Wreck(VehicleKind, Team, WreckPart),
    Owned(ModelId, Aabb),
}

struct FragmentView {
    instance: InstanceId,
    look: FragmentLook,
    sink: Option<f32>,
}

struct PickupView {
    kind: PickupKind,
    base: InstanceId,
    gem: InstanceId,
    spin: f64,
}

struct MineView {
    instance: InstanceId,
    light: usize,
}

struct PickupEffect {
    ring: InstanceId,
    glow: InstanceId,
    age: f64,
    tank: Option<u32>,
    x: f32,
    z: f32,
}

struct FallingBranch {
    model: ModelId,
    instance: InstanceId,
    bounds: Aabb,
    position: Vec3,
    rotation: Quat,
    scale: Vec3,
    velocity: Vec3,
    spin: Vec3,
    life: f64,
    landed: bool,
    resting_y: f32,
}

struct SceneryView {
    /// The live core scenery that `update(time)` animates.
    scenery: sloppy_core::models::Scenery,
    statics: InstanceId,
    /// Animated parts and their instances.
    movers: Vec<(InstanceId, SceneryMover)>,
    water: Option<WaterSettings>,
    /// The village chimney smoke model and its instance.
    smoke: Option<(ModelId, InstanceId)>,
}

/// The team flags (`flags_model`: instanced poles and cloth) and the one arena
/// breeze that ripples them.
struct Flags {
    instance: InstanceId,
    wind_from: (f64, f64),
    wind_to: (f64, f64),
    wind_start: f64,
    wind_duration: f64,
}

/// Round-start preparation (`prepare()`): pipelines, textures, the GPU running the
/// warm-up, then first frames.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PrepareStatus {
    pub compiled: u32,
    pub remaining: u32,
    pub textures_pending: u32,
    /// Only the GPU is working: compiling pipelines in the background, or running
    /// the submitted warm-up. Poll again after a short timer rather than a task.
    pub gpu_pending: bool,
    /// Everything is compiled and warm; [`Presentation::finish_prepare`] may draw.
    pub ready: bool,
}

/// Counts for diagnostics and Stats for nerds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PresentationStats {
    pub tanks: u32,
    pub covers: u32,
    pub cover_models: u32,
    pub fragments: u32,
    pub pickups: u32,
    pub mines: u32,
    pub pickup_effects: u32,
    pub branches: u32,
}

pub struct Presentation {
    pub renderer: Renderer,
    pub effects: Effects,
    pub rig: CameraRig,
    /// Presentation clock in seconds (effects, water, flags, bobbing).
    pub time: f64,
    pub crosshair_visible: bool,
    pub crosshair: Mat4,
    theme: Option<Theme>,
    library: Library,
    reticle: ReticleModel,
    player_ring: InstanceId,
    pulse: InstanceId,
    scenery: HashMap<&'static str, SceneryView>,
    pads: HashMap<u64, InstanceId>,
    floors: HashMap<String, InstanceId>,
    flags: Flags,
    tanks: HashMap<u32, TankView>,
    covers: HashMap<u32, CoverView>,
    cover_models: HashMap<String, CoverModelEntry>,
    fragments: HashMap<u32, FragmentView>,
    pickups: HashMap<u32, PickupView>,
    mines: HashMap<u32, MineView>,
    /// The spawn pads mines may lie on, for their drawn height.
    pad_decks: SpawnPadDecks,
    pickup_effects: VecDeque<PickupEffect>,
    branches: VecDeque<FallingBranch>,
    hit_until: HashMap<u32, f64>,
    hit_confirm_until: f64,
    spawn_cue: f64,
    player_was_alive: bool,
    samples: Vec<InstanceId>,
    sample_models: Vec<ModelId>,
    random: CosmeticRandom,
    /// Canvas-drawn and baked textures the scenery and cover sample.
    textures: GeneratedTextures,
    scratch: Vec<u32>,
    /// Ids present this frame, for dropping views of departed entities.
    live: std::collections::HashSet<u32>,
    preparation: Preparation,
}

impl Presentation {
    pub fn new(mut renderer: Renderer, seed: u64) -> Self {
        for effect in PRESENTATION_EFFECTS {
            renderer.register_effect(effect);
        }
        let effects = Effects::new(&mut renderer);
        let hud = |renderer: &mut Renderer, node: &Node| {
            let model = renderer.add_model(node, Lifetime::Shared);
            let instance = renderer
                .add_instance(model, Mat4::IDENTITY, Lifetime::Shared)
                .expect("model just added");
            // World-space HUD never appears in water reflections.
            renderer.set_reflected(instance, false);
            renderer.set_visible(instance, false);
            (model, instance)
        };
        let (reticle_model, reticle) = hud(&mut renderer, &own::reticle());
        let reticle = ReticleModel {
            instance: reticle,
            ready: joint_index(&renderer, reticle_model, joint::RETICLE_READY),
            reloading: joint_index(&renderer, reticle_model, joint::RETICLE_RELOADING),
            confirmed: joint_index(&renderer, reticle_model, joint::RETICLE_CONFIRMED),
        };
        let (_, player_ring) = hud(&mut renderer, &own::player_ring());
        renderer.set_reflected(player_ring, true);
        let (_, pulse) = hud(&mut renderer, &own::spawn_pulse());
        renderer.set_reflected(pulse, true);
        let mut random = CosmeticRandom::new(seed);
        let flags = Flags::new(&mut renderer, &mut random);
        let mut presentation = Self {
            renderer,
            effects,
            rig: CameraRig::default(),
            time: 0.0,
            crosshair_visible: false,
            crosshair: Mat4::IDENTITY,
            theme: None,
            library: Library::default(),
            reticle,
            player_ring,
            pulse,
            scenery: HashMap::new(),
            pads: HashMap::new(),
            floors: HashMap::new(),
            flags,
            tanks: HashMap::new(),
            covers: HashMap::new(),
            cover_models: HashMap::new(),
            fragments: HashMap::new(),
            pickups: HashMap::new(),
            mines: HashMap::new(),
            pad_decks: SpawnPadDecks::default(),
            pickup_effects: VecDeque::new(),
            branches: VecDeque::new(),
            hit_until: HashMap::new(),
            hit_confirm_until: 0.0,
            spawn_cue: 0.0,
            player_was_alive: false,
            samples: Vec::new(),
            sample_models: Vec::new(),
            random,
            textures: GeneratedTextures::default(),
            scratch: Vec::new(),
            live: std::collections::HashSet::new(),
            preparation: Preparation::default(),
        };
        presentation
            .flags
            .update(&mut presentation.renderer, 0.0, &mut presentation.random);
        presentation
    }

    /// Canvas drawing-buffer size in device pixels; the camera aspect follows it.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(width, height);
        let (width, height) = self.renderer.size();
        self.rig.set_aspect(width as f32 / height.max(1) as f32);
    }

    pub fn wreck_view(&self) -> WreckView {
        self.rig.wreck_view
    }

    pub fn stats(&self) -> PresentationStats {
        PresentationStats {
            tanks: self.tanks.len() as u32,
            covers: self.covers.len() as u32,
            cover_models: self.cover_models.len() as u32,
            fragments: self.fragments.len() as u32,
            pickups: self.pickups.len() as u32,
            mines: self.mines.len() as u32,
            pickup_effects: self.pickup_effects.len() as u32,
            branches: self.branches.len() as u32,
        }
    }

    /// Whether a tank's model is shown (diagnostics).
    pub fn tank_shown(&self, id: u32) -> Option<bool> {
        self.tanks.get(&id).map(|view| view.world.w_axis.w != 0.0)
    }

    // -------------------------------------------------------------- reset

    /// Build a theme's cached scenery. It needs no physics world, so startup can
    /// do this while the rest loads; `reset` reuses it.
    pub fn build_scenery(&mut self, theme_name: &str) {
        let theme = Theme::from_name(theme_name);
        let key = theme.name();
        if self.scenery.contains_key(key) {
            return;
        }
        let Some(model) = model_catalog::scenery(theme) else {
            return;
        };
        self.textures
            .request_scenery(&mut self.renderer, &model.root);
        let statics = self.renderer.add_scenery(&model.root, Lifetime::Shared);
        self.renderer.set_visible(statics, false);
        let movers = model
            .movers
            .into_iter()
            .map(|mover| {
                self.textures
                    .request_scenery(&mut self.renderer, &mover.root);
                let model_id = self.renderer.add_model(&mover.root, Lifetime::Shared);
                let (world, _) = mover.pose(&model.scenery);
                let instance = self
                    .renderer
                    .add_instance(model_id, world.as_mat4(), Lifetime::Shared)
                    .expect("model just added");
                self.renderer.set_visible(instance, false);
                (instance, mover)
            })
            .collect();
        let water = model.water.map(|water| match water {
            SceneryWater::Harbor(mesh) => WaterSettings::harbor(Arc::new(mesh)),
            SceneryWater::Creek(mesh) => WaterSettings::creek(Arc::new(mesh)),
        });
        let smoke = model.smoke.map(|smoke| {
            let model_id = self.renderer.add_model(&smoke, Lifetime::Shared);
            let instance = self
                .renderer
                .add_instance(model_id, Mat4::IDENTITY, Lifetime::Shared)
                .expect("model just added");
            // Wisps are screen-space sprites of the sky, not reflected scenery.
            self.renderer.set_reflected(instance, false);
            self.renderer.set_visible(instance, false);
            (model_id, instance)
        });
        self.scenery.insert(
            key,
            SceneryView {
                scenery: model.scenery,
                statics,
                movers,
                water,
                smoke,
            },
        );
    }

    fn custom_pads(&mut self, scale: f64) -> InstanceId {
        *self.pads.entry(scale.to_bits()).or_insert_with(|| {
            self.renderer
                .add_scenery(&custom_spawn_pads(scale), Lifetime::Shared)
        })
    }

    fn custom_floor(
        &mut self,
        kind: sloppy_core::sim::maps::GroundKind,
        extent: f64,
        y: f64,
    ) -> InstanceId {
        let key = format!("{kind:?}:{extent}:{y}");
        *self.floors.entry(key).or_insert_with(|| {
            self.renderer
                .add_scenery(&custom_floor(kind, Some(extent), y), Lifetime::Shared)
        })
    }

    fn apply_theme(&mut self, state: &RenderState) {
        let theme = Theme::from_name(&state.map_theme);
        let custom = theme == Theme::Custom;
        let pads = custom.then(|| self.custom_pads(state.map_scale));
        for &id in self.pads.values() {
            self.renderer.set_visible(id, Some(id) == pads);
        }
        let outer = state.map_outer_floor.map(|kind| {
            let extent = state.map_outer_floor_extent.unwrap_or(ARENA * 2.0);
            self.custom_floor(kind, extent, CUSTOM_OUTER_FLOOR_Y)
        });
        let floor = state
            .map_floor
            .map(|kind| self.custom_floor(kind, ARENA * 2.0 * state.map_scale, CUSTOM_FLOOR_Y));
        for &id in self.floors.values() {
            self.renderer
                .set_visible(id, Some(id) == floor || Some(id) == outer);
        }
        self.build_scenery(&state.map_theme);
        for (&key, view) in &mut self.scenery {
            let shown = key == theme.name();
            self.renderer.set_visible(view.statics, shown);
            for (instance, mover) in &view.movers {
                let (_, visible) = mover.pose(&view.scenery);
                self.renderer.set_visible(*instance, shown && visible);
            }
            if let Some((model, instance)) = view.smoke {
                self.renderer.set_visible(instance, shown);
                if shown
                    && let Some(smoke) =
                        model_catalog::refill_smoke(&mut view.scenery, &state.covers)
                {
                    let wisps = model_catalog::smoke_instances(&smoke);
                    self.renderer.set_part_instances(model, 0, &wisps);
                }
            }
        }
        if self.theme != Some(theme) {
            let water = self
                .scenery
                .get(theme.name())
                .and_then(|view| view.water.clone());
            self.renderer.set_water(water);
            self.theme = Some(theme);
        }
        let look = theme_look(theme);
        self.renderer.set_environment(environment(&look));
        self.renderer.set_sun_shadow(SunShadow {
            enabled: true,
            map_size: SHADOW_MAP_SIZE,
            camera: look.shadow,
            bias: SHADOW_BIAS,
            normal_bias: SHADOW_NORMAL_BIAS,
            radius: 1.0,
            receiver_floor: SHADOW_RECEIVER_FLOOR,
        });
    }

    /// Rebuild every entity view for a new round (`reset()`).
    pub fn reset(&mut self, state: &RenderState) {
        self.rig.snap_seat = true;
        self.apply_theme(state);
        self.renderer.reset_round();
        self.tanks.clear();
        self.covers.clear();
        self.cover_models.clear();
        self.fragments.clear();
        self.pickups.clear();
        self.mines.clear();
        self.pickup_effects.clear();
        self.branches.clear();
        self.samples.clear();
        self.sample_models.clear();
        self.hit_until.clear();
        self.hit_confirm_until = 0.0;
        self.player_was_alive = false;
        self.effects.reset(&mut self.renderer, state);
        self.add_covers(&state.covers);
        for tank in &state.tanks {
            self.add_tank(tank);
        }
        for pickup in &state.pickups {
            self.add_pickup(pickup.id, pickup.kind, pickup.x, pickup.z);
        }
        self.update_camera(state, 1.0, 0.0, false);
    }

    // -------------------------------------------------------------- prepare

    /// Register one model per first-use look this round can show (every wreck
    /// part, damaged cargo and timber stages, timber beams, falling crowns and
    /// boughs, mines, pickup glows, debris pieces), so `prepare_step` compiles
    /// their pipelines before combat instead of mid-fight.
    pub fn begin_prepare(&mut self, state: &RenderState) {
        self.preparation = Preparation::default();
        for kind in VehicleKind::ALL {
            for team in [Team::Blue, Team::Red] {
                self.library.tank(&mut self.renderer, kind, team);
                self.library.bar(&mut self.renderer, team);
                self.library.mine(&mut self.renderer, team);
                for part in WRECK_PARTS {
                    self.library.wreck(&mut self.renderer, kind, team, part);
                }
            }
        }
        for kind in PICKUP_KINDS {
            self.library.pickup(&mut self.renderer, kind);
        }
        for shape in DEBRIS_SHAPES {
            self.library.debris(&mut self.renderer, shape);
        }
        self.library.effect_models(&mut self.renderer);
        let mut seen = std::collections::HashSet::new();
        for cover in &state.covers {
            for health in [0.6, 0.3, 0.1] {
                let hp = cover.max_hp * health;
                let stage = cover_damage_stage(cover.kind, hp, cover.max_hp);
                if stage > 0 && seen.insert(format!("{:?}/{stage}", cover.kind)) {
                    let damaged = RenderCover {
                        hp,
                        ..cover.clone()
                    };
                    let model = model_catalog::cover_model(&damaged, stage);
                    self.sample_models
                        .push(self.renderer.add_model(&model.root, Lifetime::Round));
                }
            }
            if cover.kind == CoverKind::Timber && seen.insert("timber-parts".into()) {
                let wall = TimberWall {
                    x: cover.x,
                    z: cover.z,
                    w: cover.w,
                    h: cover.h,
                    d: cover.d,
                    color: cover.color,
                    hits: &cover.timber_hits,
                    join: cover.timber_join,
                };
                for part in timber_parts(&wall, 0) {
                    let node = model_catalog::timber_part_model(&part);
                    self.sample_models
                        .push(self.renderer.add_model(&node, Lifetime::Round));
                }
            }
            // Falling crowns and boughs reuse the live tree's materials, whose
            // faded variants every movable model registers up front.
        }
        self.effects.warm_up_samples(&mut self.renderer);
    }

    /// Create up to `budget` pipelines whose background compile has finished and queue
    /// the rest; yield to the page between calls. Once nothing remains and textures
    /// have loaded, draw every prepared variant offscreen, release the samples and wait
    /// (over later calls) until the GPU has run that warm-up.
    pub fn prepare_step(&mut self, budget: u32) -> Result<PrepareStatus, String> {
        // One band of any soil bake per step keeps the loading screen responsive.
        self.textures.step(&mut self.renderer, SOIL_ROWS_PER_STEP);
        let PrepareProgress {
            compiled,
            remaining,
            compiling,
        } = if self.preparation == Preparation::Compiling {
            self.renderer.prepare_step(budget)
        } else {
            PrepareProgress::default()
        };
        // A soil bake in progress still owes the renderer its texture.
        let textures_pending =
            self.renderer.textures_pending() as u32 + u32::from(self.textures.baking());
        if self.preparation.warm_up_due(remaining, textures_pending) {
            self.renderer.warm_up()?;
            for model in self.sample_models.drain(..) {
                self.renderer.remove_model(model);
            }
            for instance in self.samples.drain(..) {
                self.renderer.remove_instance(instance);
            }
            self.renderer.await_gpu();
        }
        self.preparation.gpu_finished(self.renderer.gpu_idle());
        // Nothing to do here but wait for the GPU or the page's bake worker: the page
        // polls on a timer. A band baked here needs the next step at once.
        let waiting = compiling > 0 || self.textures.baking_elsewhere();
        Ok(PrepareStatus {
            compiled,
            remaining,
            textures_pending,
            gpu_pending: self.preparation.awaiting_gpu()
                || (compiled == 0 && waiting && !self.textures.busy()),
            ready: self.preparation.ready(),
        })
    }

    /// A generated texture the page should bake off the main thread with the
    /// engine's `bake_texture`, handed out once (see `generated.rs`).
    pub fn claim_texture_bake(&mut self) -> Option<&'static str> {
        self.textures.claim_bake()
    }

    /// The pixels of a claimed bake; `false` when the key was no longer awaited.
    pub fn supply_texture(&mut self, key: &str, rgba: &js_sys::Uint8Array) -> Result<bool, String> {
        self.textures.supply(&mut self.renderer, key, rgba)
    }

    /// Bake a claimed texture here after all (the page's worker failed).
    pub fn release_texture_bake(&mut self, key: &str) {
        self.textures.release_bake(key);
    }

    /// Draw the actual first frames once [`PrepareStatus::ready`], so combat starts
    /// without a compile stall.
    pub fn finish_prepare(&mut self, state: &RenderState) -> Result<(), String> {
        for _ in 0..2 {
            self.render(state, 1.0, 0.0, false)?;
        }
        Ok(())
    }

    // -------------------------------------------------------------- views

    fn add_tank(&mut self, tank: &RenderTank) {
        let (model, joints) = {
            let tank_model = self.library.tank(&mut self.renderer, tank.kind, tank.team);
            (tank_model.model, tank_model.joints)
        };
        let bar_model = self.library.bar(&mut self.renderer, tank.team).model;
        let instance = self
            .renderer
            .add_instance(model, Mat4::IDENTITY, Lifetime::Round)
            .expect("tank model");
        let bar = self
            .renderer
            .add_instance(bar_model, Mat4::IDENTITY, Lifetime::Round)
            .expect("bar model");
        self.renderer.set_reflected(bar, false);
        if let Some(old) = self.tanks.insert(
            tank.id,
            TankView {
                kind: tank.kind,
                team: tank.team,
                instance,
                bar,
                suspension: None,
                pose: vec![None; joints],
                world: Mat4::IDENTITY,
            },
        ) {
            self.renderer.remove_instance(old.instance);
            self.renderer.remove_instance(old.bar);
        }
    }

    /// The cover as its model is built: movable cover at its original footprint.
    fn built_cover(cover: &RenderCover) -> std::borrow::Cow<'_, RenderCover> {
        match cover.motion {
            Some(motion) => std::borrow::Cow::Owned(RenderCover {
                x: motion.origin_x,
                z: motion.origin_z,
                w: motion.w,
                d: motion.d,
                ..cover.clone()
            }),
            None => std::borrow::Cow::Borrowed(cover),
        }
    }

    /// The shared model of a cover's look, building it on first use. Returns its
    /// key and the cover's world placement.
    fn cover_entry(&mut self, cover: &RenderCover, stage: u32) -> (String, Mat4) {
        let movable = cover.motion.is_some();
        let built = Self::built_cover(cover);
        let mut key = cover_key(&built, stage);
        if movable {
            key += "/movable";
        }
        if let Some(entry) = self.cover_models.get_mut(&key) {
            entry.users += 1;
        } else {
            let CoverModel { mut root, tree, .. } = model_catalog::cover_model(&built, stage);
            if movable {
                // The body's centre is half the cover's height up; drop the parts.
                let drop = cover.h / (2.0 * root.scale.y);
                for child in &mut root.children {
                    child.position.y -= drop;
                }
            }
            self.textures.request_scenery(&mut self.renderer, &root);
            let model = self.renderer.add_model(&root, Lifetime::Round);
            self.cover_models.insert(
                key.clone(),
                CoverModelEntry {
                    model,
                    users: 1,
                    combined: false,
                    origin: DVec3::new(built.x, 0.0, built.z),
                    source: Arc::new(root),
                    tree,
                },
            );
        }
        // Covers sharing a look share its model; each stands at its own footprint.
        let entry = &self.cover_models[&key];
        let offset = DVec3::new(built.x, 0.0, built.z) - entry.origin;
        let world = (DMat4::from_translation(offset) * entry.source.local_matrix()).as_mat4();
        (key, world)
    }

    fn release_cover_model(&mut self, key: &str) {
        if let Some(entry) = self.cover_models.get_mut(key) {
            entry.users -= 1;
            if entry.users == 0 {
                let entry = self.cover_models.remove(key).expect("present");
                if !entry.combined {
                    self.renderer.remove_model(entry.model);
                }
            }
        }
    }

    fn add_cover(&mut self, cover: &RenderCover) {
        let stage = cover_damage_stage(cover.kind, cover.hp, cover.max_hp);
        let (key, world) = self.cover_entry(cover, stage);
        let entry = &self.cover_models[&key];
        let model = entry.model;
        let scale = entry.source.scale.as_vec3();
        let nodes = self.renderer.model_nodes(model);
        let tree = entry
            .tree
            .as_ref()
            .map(|parts| tree_view(nodes, parts, 0..nodes.len()));
        let instance = self
            .renderer
            .add_instance(model, world, Lifetime::Round)
            .expect("cover model");
        self.replace_cover(
            cover.id,
            CoverView {
                instance,
                joint: None,
                key,
                stage,
                hits: cover.timber_hits.len(),
                movable: cover.motion.is_some(),
                scale,
                world,
                tree,
            },
        );
    }

    /// Install a cover's view, retiring the look it replaces.
    fn replace_cover(&mut self, id: u32, view: CoverView) {
        if let Some(old) = self.covers.insert(id, view) {
            match old.joint {
                Some(joint) => self.renderer.set_node_visible(old.instance, joint, false),
                None => self.renderer.remove_instance(old.instance),
            }
            self.release_cover_model(&old.key);
        }
    }

    /// The round's covers. Static covers with a look of their own join one
    /// combined model (a joint each), so their shadows merge into a couple of
    /// draws; covers sharing a look stay instanced, movable ones get their own
    /// instance. A combined cover that changes its look moves out to its own.
    fn add_covers(&mut self, covers: &[RenderCover]) {
        let stage = |cover: &RenderCover| cover_damage_stage(cover.kind, cover.hp, cover.max_hp);
        let mut looks: HashMap<String, usize> = HashMap::new();
        for cover in covers.iter().filter(|cover| cover.motion.is_none()) {
            *looks.entry(cover_key(cover, stage(cover))).or_default() += 1;
        }
        let (combined, own): (Vec<&RenderCover>, Vec<&RenderCover>) =
            covers.iter().partition(|cover| {
                cover.motion.is_none() && looks[&cover_key(cover, stage(cover))] == 1
            });
        if combined.len() > 1 {
            self.add_combined_covers(&combined);
        } else {
            for cover in &combined {
                self.add_cover(cover);
            }
        }
        for cover in own {
            self.add_cover(cover);
        }
    }

    fn add_combined_covers(&mut self, covers: &[&RenderCover]) {
        let mut root = Node::group("static-covers");
        let mut built = Vec::with_capacity(covers.len());
        for cover in covers {
            let stage = cover_damage_stage(cover.kind, cover.hp, cover.max_hp);
            let CoverModel {
                root: model, tree, ..
            } = model_catalog::cover_model(cover, stage);
            self.textures.request_scenery(&mut self.renderer, &model);
            let mut joint = Node::group(combined_joint(cover.id));
            joint.children.push(model.clone());
            root.children.push(joint);
            built.push((*cover, stage, Arc::new(model), tree));
        }
        let model = self.renderer.add_model(&root, Lifetime::Round);
        let instance = self
            .renderer
            .add_instance(model, Mat4::IDENTITY, Lifetime::Round)
            .expect("combined cover model");
        for (cover, stage, source, tree) in built {
            let nodes = self.renderer.model_nodes(model);
            let name = combined_joint(cover.id);
            let joint = joint_index(&self.renderer, model, &name);
            let end = nodes[joint + 1..]
                .iter()
                .position(|node| node.name.starts_with(COMBINED_JOINT))
                .map_or(nodes.len(), |offset| joint + 1 + offset);
            let tree_view = tree
                .as_ref()
                .map(|parts| tree_view(nodes, parts, joint..end));
            let key = format!("combined/{}", cover.id);
            self.cover_models.insert(
                key.clone(),
                CoverModelEntry {
                    model,
                    users: 1,
                    combined: true,
                    origin: DVec3::new(cover.x, 0.0, cover.z),
                    source,
                    tree,
                },
            );
            self.replace_cover(
                cover.id,
                CoverView {
                    instance,
                    joint: Some(joint),
                    key,
                    stage,
                    hits: cover.timber_hits.len(),
                    movable: false,
                    scale: Vec3::ONE,
                    world: Mat4::IDENTITY,
                    tree: tree_view,
                },
            );
        }
    }

    fn add_pickup(&mut self, id: u32, kind: PickupKind, x: f64, z: f64) {
        let (base_model, gem_model) = {
            let model = self.library.pickup(&mut self.renderer, kind);
            (model.base, model.gem)
        };
        let place = Mat4::from_translation(Vec3::new(x as f32, 0.0, z as f32));
        let base = self
            .renderer
            .add_instance(base_model, place, Lifetime::Round)
            .expect("pickup model");
        let gem = self
            .renderer
            .add_instance(gem_model, place, Lifetime::Round)
            .expect("gem model");
        self.pickups.insert(
            id,
            PickupView {
                kind,
                base,
                gem,
                spin: 0.0,
            },
        );
    }

    // -------------------------------------------------------------- events

    /// One simulation event; `player_hit` marks hurt/death events the viewer caused.
    pub fn event(&mut self, event: &SimEvent, player_hit: bool) {
        // Contact telemetry is for future material-specific sounds and effects.
        if matches!(
            event.kind,
            SimEventType::DebrisImpact | SimEventType::Notice
        ) {
            return;
        }
        self.effects.event(event);
        if player_hit {
            self.hit_confirm_until = self.time + FEEDBACK.hit_confirmation_seconds;
        }
        if matches!(event.kind, SimEventType::Death | SimEventType::Respawn)
            && let Some(id) = event.id
        {
            self.hit_until.remove(&id);
        }
        if event.kind == SimEventType::Hurt {
            let Some(id) = event.id.filter(|_| event.size.unwrap_or(0.0) > 0.0) else {
                return;
            };
            self.hit_until
                .insert(id, self.time + FEEDBACK.recoil_seconds);
        }
        if event.kind == SimEventType::Respawn {
            return;
        }
        if matches!(event.kind, SimEventType::Pickup | SimEventType::Promotion) {
            self.add_pickup_effect(event);
        }
    }

    /// Screen angle of damage for the HUD's direction indicator.
    pub fn damage_angle(&self, event: &SimEvent) -> Option<f64> {
        let origin = event.damage_source?.origin;
        self.rig
            .damage_angle((event.x, event.z), (origin.x, origin.z))
    }

    fn add_pickup_effect(&mut self, event: &SimEvent) {
        if self.pickup_effects.len() >= FEEDBACK.max_pickup_effects
            && let Some(oldest) = self.pickup_effects.pop_front()
        {
            self.renderer.remove_instance(oldest.ring);
            self.renderer.remove_instance(oldest.glow);
        }
        let (ring_model, glow_model) = self.library.effect_models(&mut self.renderer);
        let tint = linear(event.color.unwrap_or(0xffffff));
        let (x, z) = (event.x as f32, event.z as f32);
        let ring = self
            .renderer
            .add_instance(
                ring_model,
                Mat4::from_translation(Vec3::new(x, 0.0, z)),
                Lifetime::Round,
            )
            .expect("effect ring");
        let glow = self
            .renderer
            .add_instance(glow_model, Mat4::IDENTITY, Lifetime::Round)
            .expect("effect glow");
        for id in [ring, glow] {
            self.renderer.set_tint(id, tint);
        }
        self.renderer.set_visible(glow, false);
        self.pickup_effects.push_back(PickupEffect {
            ring,
            glow,
            age: 0.0,
            tank: event.id,
            x,
            z,
        });
    }

    // -------------------------------------------------------------- frame

    /// Synchronize entity visuals, then draw. `alpha` blends the previous and
    /// current physics poses; `dt` is the frame time.
    pub fn render(
        &mut self,
        state: &RenderState,
        alpha: f64,
        dt: f64,
        overview: bool,
    ) -> Result<(), String> {
        self.time += dt;
        if self.textures.busy() {
            self.textures.step(&mut self.renderer, SOIL_ROWS_PER_STEP);
        }
        self.flags
            .update(&mut self.renderer, self.time, &mut self.random);
        self.update_scenery();
        self.update_pickup_effects(state, alpha, dt);
        self.update_camera(state, alpha, dt, overview);
        self.update_player_indicators(state, alpha, dt, overview);
        self.update_tanks(state, alpha, dt);
        self.place_first_person_eye(state);
        self.update_branches(dt);
        self.update_covers(state);
        self.update_pickups(state, dt);
        self.update_fragments(state);
        self.update_mines(state);
        self.effects.update(
            &mut self.renderer,
            state,
            alpha as f32,
            dt as f32,
            self.time,
        );
        // A destroyed player in first person watches from above without aiming,
        // and neither reticle fits the view while the camera flies between them.
        let rig = &self.rig;
        self.crosshair_visible = state.match_state.phase == MatchPhase::Playing
            && (rig.in_first_person || (!rig.first_person.enabled && rig.seat_blend == 0.0));
        self.renderer
            .set_visible(self.reticle.instance, self.crosshair_visible);
        self.renderer
            .set_transform(self.reticle.instance, self.crosshair);
        self.renderer.set_camera(self.rig.camera);
        self.renderer.render(self.time as f32)
    }

    fn update_scenery(&mut self) {
        let Some(theme) = self.theme else { return };
        if let Some(view) = self.scenery.get_mut(theme.name()) {
            view.scenery.update(self.time);
            for (instance, mover) in &view.movers {
                let (world, visible) = mover.pose(&view.scenery);
                self.renderer.set_transform(*instance, world.as_mat4());
                self.renderer.set_visible(*instance, visible);
            }
        }
    }

    fn viewer_pose(state: &RenderState) -> ViewerPose {
        match state.viewer() {
            Some(tank) => ViewerPose {
                kind: tank.kind,
                alive: tank.alive,
                previous: DVec3::new(tank.previous.x, 0.0, tank.previous.z),
                position: dvec3(tank.position),
                aim: tank.aim,
            },
            None => ViewerPose {
                kind: VehicleKind::Balanced,
                alive: false,
                previous: DVec3::ZERO,
                position: DVec3::ZERO,
                aim: 0.0,
            },
        }
    }

    fn update_camera(&mut self, state: &RenderState, alpha: f64, dt: f64, overview: bool) {
        let has_model = self.tanks.contains_key(&state.viewer_id);
        self.rig
            .update(&Self::viewer_pose(state), alpha, dt, overview, has_model);
    }

    fn update_player_indicators(
        &mut self,
        state: &RenderState,
        alpha: f64,
        dt: f64,
        overview: bool,
    ) {
        let viewer = Self::viewer_pose(state);
        let cooldown = state.viewer().map_or(0.0, |tank| tank.cooldown);
        let confirmed = self.hit_confirm_until > self.time;
        let ready = cooldown <= 0.0;
        let instance = self.reticle.instance;
        for (joint, shown) in [
            (self.reticle.confirmed, confirmed),
            (self.reticle.ready, !confirmed && ready),
            (self.reticle.reloading, !confirmed && !ready),
        ] {
            self.renderer.set_node_visible(instance, joint, shown);
        }
        let in_first_person = self.rig.in_first_person;
        let scale = if confirmed { 1.2 } else { 1.0 }
            * if in_first_person {
                FIRST_PERSON.reticle_scale
            } else {
                1.0
            };
        let scale = Vec3::splat(scale as f32);
        if !in_first_person {
            let aim = self.rig.aim_point;
            self.crosshair = Mat4::from_scale_rotation_translation(
                scale,
                Quat::IDENTITY,
                Vec3::new(aim.x, RETICLE_HEIGHT as f32, aim.z),
            );
        } else {
            // Placed after the turret is posed; keep the scale for now.
            self.crosshair = Mat4::from_scale(scale);
        }
        if viewer.alive && !self.player_was_alive {
            self.spawn_cue = FEEDBACK.spawn_cue_seconds;
        }
        self.player_was_alive = viewer.alive;
        self.spawn_cue = (self.spawn_cue - dt).max(0.0);
        let ring_visible = viewer.alive && !overview && !in_first_person;
        let position = if viewer.alive {
            viewer.position
        } else {
            viewer.previous
        };
        let at = Vec3::new(
            lerp(viewer.previous.x, position.x, alpha) as f32,
            0.0,
            lerp(viewer.previous.z, position.z, alpha) as f32,
        );
        let vehicle_scale = vehicle(viewer.kind).scale as f32;
        self.renderer.set_visible(self.player_ring, ring_visible);
        // The ring grows with the vehicle but keeps its heights above the spawn pads.
        self.renderer.set_transform(
            self.player_ring,
            Mat4::from_scale_rotation_translation(
                Vec3::new(vehicle_scale, 1.0, vehicle_scale),
                Quat::IDENTITY,
                at,
            ),
        );
        let pulse_visible = ring_visible && self.spawn_cue > 0.0;
        self.renderer.set_visible(self.pulse, pulse_visible);
        if pulse_visible {
            let (size, opacity) = spawn_pulse(self.spawn_cue);
            self.renderer.set_transform(
                self.pulse,
                Mat4::from_scale_rotation_translation(
                    Vec3::splat(size as f32),
                    Quat::IDENTITY,
                    Vec3::new(at.x, own::SPAWN_PULSE_HEIGHT as f32, at.z),
                ),
            );
            self.renderer.set_opacity(self.pulse, opacity as f32);
        }
    }

    fn update_tanks(&mut self, state: &RenderState, alpha: f64, dt: f64) {
        let camera_rotation = self.rig.rotation;
        let seat_blend = self.rig.seat_blend;
        let seat_wanted = self.rig.seat_wanted;
        let look_yaw = self.rig.first_person.yaw;
        let playing = state.match_state.phase == MatchPhase::Playing;
        for tank in &state.tanks {
            // Reinforcements arrive after reset; build their visuals on first sight.
            let rebuild = self
                .tanks
                .get(&tank.id)
                .is_none_or(|view| view.kind != tank.kind || view.team != tank.team);
            if rebuild {
                self.add_tank(tank);
            }
            let (model_joints, bar_joints) = {
                let model = &self.library.tanks[&(tank.kind, tank.team)];
                let bar = &self.library.bars[&tank.team];
                (
                    (model.hull, model.turret, model.barrel, model.track),
                    (
                        bar.fills,
                        bar.ranks,
                        bar.shield,
                        bar.shield_fill,
                        bar.spawn,
                        bar.spawn_fill,
                    ),
                )
            };
            let view = self.tanks.get_mut(&tank.id).expect("tank view");
            let renderer = &mut self.renderer;
            let viewer = tank.id == state.viewer_id;
            let seated = viewer && seat_wanted;
            renderer.set_visible(view.instance, tank.alive);
            // The camera flies through the player's bar on the way into the turret.
            renderer.set_visible(view.bar, tank.alive && !(viewer && seat_blend > 0.0));
            let (fills, ranks, shield, shield_fill, spawn, spawn_fill) = bar_joints;
            let meters =
                protection_meters(tank.alive, tank.shield, tank.shield_points, tank.protection);
            renderer.set_node_visible(view.bar, shield.index, meters.shield_visible);
            renderer.set_node_transform(
                view.bar,
                shield.index,
                Some(shield.with_position(Vec3::new(
                    shield.position.x,
                    meters.shield_y as f32,
                    shield.position.z,
                ))),
            );
            renderer.set_node_transform(
                view.bar,
                shield_fill.index,
                Some(shield_fill.with_scale(Vec3::new(meters.shield_fill as f32, 1.0, 1.0))),
            );
            renderer.set_node_visible(view.bar, spawn.index, meters.spawn_visible);
            renderer.set_node_transform(
                view.bar,
                spawn.index,
                Some(spawn.with_position(Vec3::new(
                    spawn.position.x,
                    meters.spawn_y as f32,
                    spawn.position.z,
                ))),
            );
            renderer.set_node_transform(
                view.bar,
                spawn_fill.index,
                Some(spawn_fill.with_scale(Vec3::new(meters.spawn_fill as f32, 1.0, 1.0))),
            );
            if !tank.alive {
                self.hit_until.remove(&tank.id);
                view.suspension = None;
                view.world = Mat4::ZERO;
                continue;
            }
            let position = tank.position;
            let mut x = lerp(tank.previous.x, position.x, alpha);
            let y = position.y - TANK_MODEL_DROP;
            let mut z = lerp(tank.previous.z, position.z, alpha);
            let hit_remaining =
                (self.hit_until.get(&tank.id).copied().unwrap_or(0.0) - self.time).max(0.0);
            let hit_fade = hit_remaining / FEEDBACK.recoil_seconds;
            let hit_age = FEEDBACK.recoil_seconds - hit_remaining;
            // Render-only recoil: physics, steering and the camera keep the true pose.
            x += (hit_age * 70.0).cos() * HIT_SHAKE[0] * hit_fade;
            z += (hit_age * 55.0).sin() * HIT_SHAKE[1] * hit_fade;
            let tilt_x = (hit_age * 60.0).sin() * HIT_SHAKE[2] * hit_fade;
            let tilt_z = (hit_age * 65.0).cos() * HIT_SHAKE[3] * hit_fade;
            if hit_remaining == 0.0 {
                self.hit_until.remove(&tank.id);
            }
            let velocity = tank.velocity;
            let suspension = view.suspension.get_or_insert_with(TankSuspension::default);
            suspension.update(
                velocity.x,
                velocity.z,
                tank.heading,
                state.elapsed,
                if playing { dt } else { 0.0 },
            );
            let scale = vehicle(tank.kind).scale as f32;
            let world = Mat4::from_scale_rotation_translation(
                Vec3::splat(scale),
                euler_xyz(tilt_x as f32, 0.0, tilt_z as f32),
                Vec3::new(x as f32, y as f32, z as f32),
            );
            view.world = world;
            renderer.set_transform(view.instance, world);
            // The turret rides the hull's tilted ring, then turns to its own aim;
            // the barrel inherits the tilt and keeps its recoil.
            let (hull, turret, barrel, track) = model_joints;
            let hull_rotation = euler_yxz(
                suspension.pitch.angle as f32,
                tank.heading as f32,
                suspension.roll.angle as f32,
            );
            let aim = if seated { look_yaw } else { tank.aim };
            let turret_rotation =
                hull_rotation * Quat::from_rotation_y((aim - tank.heading) as f32);
            let hull_pose = hull.with_rotation(hull_rotation);
            let turret_pose = turret.with_rotation(turret_rotation);
            let barrel_pose = barrel.with_position(Vec3::new(
                barrel.position.x,
                barrel.position.y,
                -(tank.recoil * RECOIL_TRAVEL) as f32,
            ));
            let scroll = if tank.kind == VehicleKind::Humvee {
                0.0
            } else {
                (self.time * velocity.x.hypot(velocity.z) * TRACK_SCROLL) % TRACK_PERIOD
            };
            let track_pose =
                track.with_position(Vec3::new(track.position.x, track.position.y, scroll as f32));
            for (joint, pose) in [
                (hull.index, hull_pose),
                (turret.index, turret_pose),
                (barrel.index, barrel_pose),
                (track.index, track_pose),
            ] {
                view.pose[joint] = Some(pose);
                renderer.set_node_transform(view.instance, joint, Some(pose));
            }
            let bar_world = Mat4::from_rotation_translation(
                camera_rotation,
                Vec3::new(
                    x as f32,
                    if viewer {
                        PLAYER_BAR_HEIGHT
                    } else {
                        BAR_HEIGHT
                    } as f32,
                    z as f32,
                ),
            );
            renderer.set_transform(view.bar, bar_world);
            let health = health_bar_state(tank.hp, tank.max_hp, tank.team);
            let tone = match health.tone {
                HealthColor::Team => 0,
                HealthColor::Warning => 1,
                HealthColor::Critical => 2,
            };
            for (index, fill) in fills.iter().enumerate() {
                let shown = index == tone && health.ratio > 0.0;
                renderer.set_node_visible(view.bar, fill.index, shown);
                if shown {
                    renderer.set_node_transform(
                        view.bar,
                        fill.index,
                        Some(fill.with_scale(Vec3::new(health.ratio as f32, 1.0, 1.0))),
                    );
                }
            }
            let rank = rank_index(tank.xp);
            for (i, joint) in ranks.iter().enumerate() {
                renderer.set_node_visible(view.bar, *joint, i < rank);
            }
        }
        // Tanks that left the roster (never in single player, but in rooms).
        self.scratch.clear();
        self.live.clear();
        self.live.extend(state.tanks.iter().map(|tank| tank.id));
        self.scratch
            .extend(self.tanks.keys().filter(|id| !self.live.contains(*id)));
        for id in self.scratch.drain(..) {
            if let Some(view) = self.tanks.remove(&id) {
                self.renderer.remove_instance(view.instance);
                self.renderer.remove_instance(view.bar);
            }
        }
    }

    /// Seat the first-person camera in the posed turret, so it rides the hull's
    /// suspension, and float the reticle along the view.
    fn place_first_person_eye(&mut self, state: &RenderState) {
        let Some(viewer) = state.viewer() else { return };
        let Some(view) = self.tanks.get(&state.viewer_id) else {
            return;
        };
        if self.rig.seat_blend == 0.0 || view.world == Mat4::ZERO {
            return;
        }
        let model = &self.library.tanks[&(view.kind, view.team)];
        let turret = joint_world(
            self.renderer.model_nodes(model.model),
            &view.pose,
            view.world,
            model.turret.index,
        );
        self.rig.place_eye(&turret, viewer.kind);
        if self.rig.in_first_person {
            let muzzle = f64::from(view.world.w_axis.y) + model.muzzle_height;
            let scale = self.crosshair.x_axis.length();
            self.crosshair =
                self.rig.first_person_reticle(muzzle) * Mat4::from_scale(Vec3::splat(scale));
        }
    }

    fn update_pickup_effects(&mut self, state: &RenderState, alpha: f64, dt: f64) {
        let mut index = 0;
        while index < self.pickup_effects.len() {
            let effect = &mut self.pickup_effects[index];
            effect.age += dt;
            let progress = effect.age / FEEDBACK.pickup_seconds;
            if progress >= 1.0 {
                let effect = self.pickup_effects.remove(index).expect("in range");
                self.renderer.remove_instance(effect.ring);
                self.renderer.remove_instance(effect.glow);
                continue;
            }
            index += 1;
            let fade = (1.0 - progress).powi(2);
            self.renderer.set_transform(
                effect.ring,
                Mat4::from_scale_rotation_translation(
                    Vec3::splat((1.0 + progress * 3.0) as f32),
                    Quat::IDENTITY,
                    Vec3::new(effect.x, 0.0, effect.z),
                ),
            );
            self.renderer.set_opacity(effect.ring, (0.85 * fade) as f32);
            let tank = effect
                .tank
                .and_then(|id| state.tanks.iter().find(|tank| tank.id == id && tank.alive));
            self.renderer.set_visible(effect.glow, tank.is_some());
            if let Some(tank) = tank {
                let position = tank.position;
                let scale = vehicle(tank.kind).scale * (1.0 + progress * 0.15);
                self.renderer.set_transform(
                    effect.glow,
                    Mat4::from_scale_rotation_translation(
                        GLOW_SCALE * scale as f32,
                        Quat::IDENTITY,
                        Vec3::new(
                            lerp(tank.previous.x, position.x, alpha) as f32,
                            GLOW_HEIGHT,
                            lerp(tank.previous.z, position.z, alpha) as f32,
                        ),
                    ),
                );
                self.renderer.set_opacity(effect.glow, (0.2 * fade) as f32);
            }
        }
    }

    fn update_covers(&mut self, state: &RenderState) {
        let mut shed = Vec::new();
        for cover in &state.covers {
            let stump = cover.kind == CoverKind::Tree && !cover.alive;
            let stage = cover_damage_stage(cover.kind, cover.hp, cover.max_hp);
            let rebuild = self.covers.get(&cover.id).is_none_or(|view| {
                cover.alive
                    && (view.stage != stage
                        || (cover.kind == CoverKind::Timber
                            && view.hits != cover.timber_hits.len()))
            });
            if rebuild {
                self.add_cover(cover);
            }
            let view = self.covers.get_mut(&cover.id).expect("cover view");
            // Destruction removes a movable cover's body; its last pose stays.
            if view.movable && cover.alive {
                view.world = Mat4::from_scale_rotation_translation(
                    view.scale,
                    quat(cover.rotation),
                    vec3(cover.position),
                );
                self.renderer.set_transform(view.instance, view.world);
            }
            if let Some(tree) = &mut view.tree {
                if cover.alive {
                    let stage = tree_branch_stage(cover.hp / cover.max_hp);
                    if stage != tree.branch_stage {
                        tree.branch_stage = stage;
                        for branch in &mut tree.branches {
                            let visible = branch.drop_stage > stage;
                            if branch.shown && !visible {
                                shed.push((cover.id, branch.joint, branch.crown_child));
                            }
                            branch.shown = visible;
                            self.renderer
                                .set_node_visible(view.instance, branch.joint, visible);
                        }
                    }
                }
                self.renderer
                    .set_node_visible(view.instance, tree.crown, !stump);
                self.renderer
                    .set_node_visible(view.instance, tree.cut, stump);
            }
            let shown = cover.alive || stump;
            match view.joint {
                Some(joint) => self.renderer.set_node_visible(view.instance, joint, shown),
                None => self.renderer.set_visible(view.instance, shown),
            }
        }
        for (cover, joint, crown_child) in shed {
            self.shed_branch(cover, joint, crown_child);
        }
    }

    /// Drop a bough from a damaged tree: a detached copy falls, tumbles, lands and
    /// fades (`TreeDebris.shed`).
    fn shed_branch(&mut self, cover: u32, joint: usize, crown_child: usize) {
        let Some(view) = self.covers.get(&cover) else {
            return;
        };
        let Some(entry) = self.cover_models.get(&view.key) else {
            return;
        };
        let Some(source) = entry
            .tree
            .as_ref()
            .and_then(|tree| entry.source.find(tree.crown))
            .and_then(|crown| crown.children.get(crown_child))
        else {
            return;
        };
        let nodes = self.renderer.model_nodes(entry.model);
        let world = joint_world(nodes, &[], view.world, joint);
        let crown = view.tree.as_ref().map_or(view.world, |tree| {
            joint_world(nodes, &[], view.world, tree.crown)
        });
        let mut branch = source.clone();
        branch.position = DVec3::ZERO;
        branch.rotation = glam::DQuat::IDENTITY;
        branch.scale = DVec3::ONE;
        branch.visible = true;
        let bounds = node_bounds(&branch, DMat4::IDENTITY);
        if self.branches.len() == MAX_BRANCHES
            && let Some(oldest) = self.branches.pop_front()
        {
            self.renderer.remove_model(oldest.model);
        }
        let model = self.renderer.add_model(&branch, Lifetime::Round);
        let instance = self
            .renderer
            .add_instance(model, world, Lifetime::Round)
            .expect("branch model");
        let (scale, rotation, position) = world.to_scale_rotation_translation();
        let mut outward = position - crown.w_axis.truncate();
        outward.y = 0.0;
        let outward = outward.normalize_or_zero();
        let random = &mut self.random;
        let speed = 1.2 + random.next_f64() as f32;
        let spin = Vec3::new(
            random.next_f64() as f32 - 0.5,
            random.next_f64() as f32 - 0.5,
            random.next_f64() as f32 - 0.5,
        );
        self.branches.push_back(FallingBranch {
            model,
            instance,
            bounds,
            position,
            rotation,
            scale,
            velocity: Vec3::new(outward.x * speed, -0.5, outward.z * speed),
            spin,
            life: BRANCH_LIFETIME,
            landed: false,
            resting_y: 0.0,
        });
    }

    fn update_branches(&mut self, dt: f64) {
        let dt32 = dt as f32;
        let mut index = 0;
        while index < self.branches.len() {
            let branch = &mut self.branches[index];
            branch.life -= dt;
            if branch.life <= 0.0 {
                let branch = self.branches.remove(index).expect("in range");
                self.renderer.remove_model(branch.model);
                continue;
            }
            index += 1;
            if !branch.landed {
                branch.velocity.y -= (BRANCH_GRAVITY * dt) as f32;
                branch.position += branch.velocity * dt32;
                let turn = branch.spin * dt32 * 3.0;
                branch.rotation = (branch.rotation
                    * Quat::from_rotation_x(turn.x)
                    * Quat::from_rotation_y(turn.y)
                    * Quat::from_rotation_z(turn.z))
                .normalize();
                let world = Mat4::from_scale_rotation_translation(
                    branch.scale,
                    branch.rotation,
                    branch.position,
                );
                let bottom = lowest(&branch.bounds, &world);
                if bottom <= BRANCH_REST {
                    branch.position.y += BRANCH_REST - bottom;
                    branch.resting_y = branch.position.y;
                    branch.landed = true;
                }
            }
            let cleanup = debris_cleanup_progress(branch.life);
            if branch.landed {
                branch.position.y = branch.resting_y - (cleanup * BRANCH_SINK) as f32;
            }
            self.renderer.set_transform(
                branch.instance,
                Mat4::from_scale_rotation_translation(
                    branch.scale,
                    branch.rotation,
                    branch.position,
                ),
            );
            self.renderer
                .set_opacity(branch.instance, (1.0 - cleanup) as f32);
        }
    }

    fn update_pickups(&mut self, state: &RenderState, dt: f64) {
        // Pickups hover at eye height; first person sees through them.
        let opacity = if self.rig.in_first_person {
            FIRST_PERSON.pickup_opacity
        } else {
            1.0
        } as f32;
        // Pickups that left the arena (rules and fixtures may replace them).
        self.scratch.clear();
        self.live.clear();
        self.live
            .extend(state.pickups.iter().map(|pickup| pickup.id));
        self.scratch
            .extend(self.pickups.keys().filter(|id| !self.live.contains(*id)));
        for id in self.scratch.drain(..) {
            if let Some(view) = self.pickups.remove(&id) {
                self.renderer.remove_instance(view.base);
                self.renderer.remove_instance(view.gem);
            }
        }
        for pickup in &state.pickups {
            if !self.pickups.contains_key(&pickup.id) {
                self.add_pickup(pickup.id, pickup.kind, pickup.x, pickup.z);
            }
            let view = self.pickups.get_mut(&pickup.id).expect("pickup view");
            let model = &self.library.pickups[&view.kind];
            let renderer = &mut self.renderer;
            renderer.set_visible(view.gem, pickup.available);
            renderer.set_opacity(view.gem, opacity);
            renderer.set_node_visible(view.base, model.ring, pickup.available);
            renderer.set_node_visible(view.base, model.ring_dim, !pickup.available);
            renderer.set_node_visible(view.base, model.refill, !pickup.available);
            let fallback = if pickup.kind == PickupKind::Laser {
                LASER_DEFENSE.initial_delay
            } else {
                AMMO_RESPAWN_SECONDS
            };
            let duration = if pickup.cooldown_duration != 0.0 {
                pickup.cooldown_duration
            } else {
                fallback
            };
            let progress = (1.0 - pickup.cooldown / duration).clamp(0.0, 1.0);
            let segments = (progress * f64::from(own::REFILL_SEGMENTS)).floor() as f32;
            renderer.set_instance_data(view.base, [segments, 0.0, 0.0, 0.0]);
            view.spin += dt;
            let height = GEM_HEIGHT + (self.time * 2.0 + f64::from(pickup.id)).sin() * GEM_BOB;
            renderer.set_transform(
                view.gem,
                Mat4::from_rotation_translation(
                    Quat::from_rotation_y((PI / 4.0 + view.spin) as f32),
                    Vec3::new(pickup.x as f32, height as f32, pickup.z as f32),
                ),
            );
        }
    }

    fn update_fragments(&mut self, state: &RenderState) {
        self.scratch.clear();
        self.live.clear();
        self.live.extend(state.fragments.iter().map(|f| f.id));
        self.scratch
            .extend(self.fragments.keys().filter(|id| !self.live.contains(*id)));
        for id in self.scratch.drain(..) {
            if let Some(view) = self.fragments.remove(&id) {
                self.renderer.remove_instance(view.instance);
                if let FragmentLook::Owned(model, _) = view.look {
                    self.renderer.remove_model(model);
                }
            }
        }
        let mut pieces: HashMap<FragmentShape, usize> = HashMap::new();
        for fragment in &state.fragments {
            if !self.fragments.contains_key(&fragment.id) {
                match self.fragment_view(fragment) {
                    Some(view) => {
                        self.fragments.insert(fragment.id, view);
                    }
                    None => continue,
                }
            }
            let cleanup = debris_cleanup_progress(fragment.life) as f32;
            let view = self.fragments.get_mut(&fragment.id).expect("fragment view");
            let rotation = quat(fragment.rotation);
            let mut position = vec3(fragment.position);
            match view.look {
                FragmentLook::Piece => {
                    let shape = fragment.shape.unwrap_or(FragmentShape::Shard);
                    let count = pieces.entry(shape).or_default();
                    *count += 1;
                    // The piece pool is sized for the largest debris budget.
                    self.renderer
                        .set_visible(view.instance, *count <= FRAGMENT_CAPACITY);
                    let size = fragment.size as f32;
                    let scale = fragment
                        .dimensions
                        .map_or(Vec3::splat(size), |d| vec3(d) * size);
                    let bounds = &self.library.debris[&shape].bounds;
                    // Project the piece's bounds onto world Y at its resting
                    // rotation: a flat panel descends by its thickness.
                    let matrix = Mat4::from_scale_rotation_translation(scale, rotation, position);
                    let extent = (bounds.max - bounds.min).as_vec3();
                    let height = matrix.x_axis.y.abs() * extent.x
                        + matrix.y_axis.y.abs() * extent.y
                        + matrix.z_axis.y.abs() * extent.z;
                    position.y -= cleanup * (height + SINK_MARGIN);
                    self.renderer.set_transform(
                        view.instance,
                        Mat4::from_scale_rotation_translation(scale, rotation, position),
                    );
                    self.renderer
                        .set_tint(view.instance, linear(fragment.color));
                }
                FragmentLook::Wreck(..) | FragmentLook::Owned(..) => {
                    let (scale, bounds) = match &view.look {
                        FragmentLook::Wreck(kind, team, part) => (
                            vehicle(*kind).scale as f32,
                            self.library.wrecks[&(*kind, *team, *part)].bounds,
                        ),
                        FragmentLook::Owned(_, bounds) => (1.0, *bounds),
                        FragmentLook::Piece => unreachable!("handled above"),
                    };
                    let scale = Vec3::splat(scale);
                    if cleanup > 0.0 {
                        let world =
                            Mat4::from_scale_rotation_translation(scale, rotation, position);
                        let sink = *view
                            .sink
                            .get_or_insert_with(|| world_height(&bounds, &world) + SINK_MARGIN);
                        position.y -= cleanup * sink;
                    }
                    self.renderer.set_transform(
                        view.instance,
                        Mat4::from_scale_rotation_translation(scale, rotation, position),
                    );
                    if let FragmentLook::Wreck(..) = view.look {
                        // Burnt paint and team glow darken over the first seconds
                        // (`wreck-aging.ts`), fed per instance to the aging effect.
                        let since = state.elapsed - fragment.created_at.unwrap_or(state.elapsed);
                        let brightness = wreck_brightness(since) as f32;
                        self.renderer
                            .set_instance_data(view.instance, [brightness, 1.0, 0.0, 0.0]);
                    }
                }
            }
            self.renderer.set_opacity(view.instance, 1.0 - cleanup);
        }
    }

    /// A physical debris view: an instanced piece, a wreck part, a timber member
    /// or a falling crown.
    fn fragment_view(&mut self, fragment: &RenderFragment) -> Option<FragmentView> {
        let (model, look) = if let Some(part) = &fragment.timber_part {
            let node = model_catalog::timber_part_model(part);
            let bounds = node_bounds(&node, DMat4::IDENTITY);
            let model = self.renderer.add_model(&node, Lifetime::Round);
            (model, FragmentLook::Owned(model, bounds))
        } else if let Some(tree) = fragment.tree_cover_id {
            let view = self.covers.get(&tree)?;
            let entry = self.cover_models.get(&view.key)?;
            let shed: Vec<usize> = view.tree.as_ref().map_or_else(Vec::new, |tree| {
                tree.branches
                    .iter()
                    .filter(|branch| !branch.shown)
                    .map(|branch| branch.crown_child)
                    .collect()
            });
            let crown = crown_fragment(
                &entry.source,
                entry.tree.as_ref()?.crown,
                fragment.tree_center_y.unwrap_or(0.0),
                &shed,
            )?;
            let bounds = node_bounds(&crown, DMat4::IDENTITY);
            let model = self.renderer.add_model(&crown, Lifetime::Round);
            (model, FragmentLook::Owned(model, bounds))
        } else if let Some(kind) = fragment.wreck {
            let team = fragment.team.unwrap_or(Team::Blue);
            let part = fragment.part.unwrap_or(WreckPart::Hull);
            let model = self
                .library
                .wreck(&mut self.renderer, kind, team, part)
                .model;
            (model, FragmentLook::Wreck(kind, team, part))
        } else {
            let shape = fragment.shape.unwrap_or(FragmentShape::Shard);
            let model = self.library.debris(&mut self.renderer, shape).model;
            (model, FragmentLook::Piece)
        };
        let instance = self
            .renderer
            .add_instance(model, Mat4::IDENTITY, Lifetime::Round)?;
        Some(FragmentView {
            instance,
            look,
            sink: None,
        })
    }

    fn update_mines(&mut self, state: &RenderState) {
        self.scratch.clear();
        self.live.clear();
        self.live.extend(state.mines.iter().map(|mine| mine.id));
        self.scratch
            .extend(self.mines.keys().filter(|id| !self.live.contains(*id)));
        for id in self.scratch.drain(..) {
            if let Some(view) = self.mines.remove(&id) {
                self.renderer.remove_instance(view.instance);
            }
        }
        let blink = (self.time * 10.0).sin() > 0.0;
        self.pad_decks.sync(state);
        for mine in &state.mines {
            if !self.mines.contains_key(&mine.id) {
                let (model, light) = {
                    let model = self.library.mine(&mut self.renderer, mine.team);
                    (model.model, model.light)
                };
                let instance = self
                    .renderer
                    .add_instance(
                        model,
                        Mat4::from_translation(Vec3::new(
                            mine.x as f32,
                            own::mine_lift(&self.pad_decks, mine.x, mine.z) as f32,
                            mine.z as f32,
                        )),
                        Lifetime::Round,
                    )
                    .expect("mine model");
                self.mines.insert(mine.id, MineView { instance, light });
            }
            let view = &self.mines[&mine.id];
            self.renderer
                .set_node_visible(view.instance, view.light, mine.arm > 0.0 || blink);
        }
    }
}

/// Joint names of covers in the combined static-cover model.
const COMBINED_JOINT: &str = "cover:";

fn combined_joint(id: u32) -> String {
    format!("{COMBINED_JOINT}{id}")
}

/// A tree's damage joints among `nodes[range]` (its model, or its cover joint's
/// span in the combined model; joints of one tree are contiguous, crown order).
fn tree_view(
    nodes: &[crate::model::ModelNode],
    parts: &TreeParts,
    range: std::ops::Range<usize>,
) -> TreeView {
    let find = |name: &str| {
        range
            .clone()
            .find(|&index| nodes[index].name == name)
            .unwrap_or_else(|| panic!("tree joint {name}"))
    };
    let joints = range
        .clone()
        .filter(|&index| TreeParts::is_branch(&nodes[index].name));
    TreeView {
        crown: find(parts.crown),
        cut: find(parts.cut_surface),
        branches: parts
            .branches
            .iter()
            .zip(joints)
            .map(|(branch, joint)| BranchView {
                joint,
                drop_stage: branch.drop_stage,
                crown_child: branch.crown_child,
                shown: true,
            })
            .collect(),
        branch_stage: 0,
    }
}

/// Give every material of a wreck the aging effect, which darkens base and
/// emissive color by the brightness each instance carries.
fn age_materials(node: &mut Node) {
    if let Some(drawable) = &mut node.drawable {
        drawable.material = Arc::new(aged_wreck_material(&drawable.material, 0.0));
    }
    for child in &mut node.children {
        age_materials(child);
    }
}

/// The falling crown of a felled tree: its trunk-and-crown subtree, lowered so
/// the fragment body's centre sits `center_y` up the trunk.
fn crown_fragment(source: &Node, crown: &str, center_y: f64, shed: &[usize]) -> Option<Node> {
    let mut node = source.find(crown)?.clone();
    // Boughs already shed stay gone from the falling crown.
    for &index in shed {
        if let Some(bough) = node.children.get_mut(index) {
            bough.visible = false;
        }
    }
    node.name = String::new();
    node.visible = true;
    node.position = DVec3::new(0.0, -center_y, 0.0);
    let mut root = Node::group("crown-fragment");
    root.children.push(node);
    Some(root)
}

fn environment(look: &ThemeLook) -> Environment {
    Environment {
        background: look.sky,
        fog: Some(Fog {
            color: look.sky,
            near: look.fog_near,
            far: look.fog_far,
        }),
        sky_color: look.fill_color,
        ground_color: look.fill_ground,
        hemisphere_intensity: look.fill_intensity,
        sun_color: look.sun_color,
        sun_intensity: look.sun_intensity,
        sun_position: look.sun_position,
        sun_target: Vec3::ZERO,
        exposure: look.exposure,
        reflections: look.reflections,
    }
}

impl Flags {
    fn new(renderer: &mut Renderer, random: &mut CosmeticRandom) -> Self {
        let flags = flags_model();
        debug_assert!(flags.find(FLAG_CLOTH_NODE).is_some());
        let model = renderer.add_model(&flags, Lifetime::Shared);
        let instance = renderer
            .add_instance(model, Mat4::IDENTITY, Lifetime::Shared)
            .expect("flag model");
        Self {
            instance,
            wind_from: random_wind(random),
            wind_to: random_wind(random),
            wind_start: 0.0,
            wind_duration: 3.0 + random.next_f64() * 4.0,
        }
    }

    /// One breeze for the whole arena, easing toward a new random target every
    /// 3–7 seconds.
    fn update(&mut self, renderer: &mut Renderer, time: f64, random: &mut CosmeticRandom) {
        while time >= self.wind_start + self.wind_duration {
            self.wind_start += self.wind_duration;
            self.wind_from = self.wind_to;
            self.wind_to = random_wind(random);
            self.wind_duration = 3.0 + random.next_f64() * 4.0;
        }
        let progress = ((time - self.wind_start) / self.wind_duration).max(0.0);
        let blend = progress * progress * (3.0 - 2.0 * progress);
        let gust = lerp(self.wind_from.0, self.wind_to.0, blend) as f32;
        let direction = lerp(self.wind_from.1, self.wind_to.1, blend);
        let (sin, cos) = direction.sin_cos();
        renderer.set_instance_data(self.instance, [gust, sin as f32, cos as f32, 0.0]);
    }
}

fn random_wind(random: &mut CosmeticRandom) -> (f64, f64) {
    (
        0.15 + random.next_f64() * 0.85,
        (random.next_f64() - 0.5) * 1.3,
    )
}
