//! The browser presentation (`presentation.ts` `Presentation`): owns the
//! renderer and runtime effects, keeps one renderer instance per simulation
//! entity, and poses them from each frame's `RenderState`.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use glam::{BVec3, DMat4, DVec3, Mat4, Quat, Vec3};
use sloppy_core::geometry::math::smoothstep;
use sloppy_core::geometry::{Aabb, node_bounds};
use sloppy_core::models::{
    FLAG_CLOTH_NODE, HouseCoverBuild, PALLET_HEIGHT, SUPPLY_SCALE, TreeShape, cover_damage_stage,
    crate_piece_model, create_arena_floor, create_spawn_pads, flags_model, house_pieces, part,
    pickup_cube, pickup_signal, pickup_tile_offset, release_tank_models, tank_model,
    timber_debris_model, timber_part_model, tower_piece_model, tree_branch_stage, tree_foliage,
    tree_part, wreck_brightness, wreck_model,
};
use sloppy_core::scene::Node;
use sloppy_core::sim::ammunition::AMMO_RESPAWN_SECONDS;
use sloppy_core::sim::crate_layout::{CratePiece, crate_section_looks};
use sloppy_core::sim::data::{ARENA, LASER_DEFENSE, pickup, vehicle};
use sloppy_core::sim::debris_cleanup::debris_cleanup_progress;
use sloppy_core::sim::house_layout::HousePiece;
use sloppy_core::sim::render_state::{RenderCover, RenderFragment, RenderTank};
use sloppy_core::sim::simulation::WreckView;
use sloppy_core::sim::simulation_rules::FRAGMENT_CAPACITY;
use sloppy_core::sim::tank_dimensions::{tank_hull, tank_visual_muzzle};
use sloppy_core::sim::timber_layout::{TimberPart, TimberWall, timber_parts};
use sloppy_core::sim::tower_layout::TowerPiece;
use sloppy_core::sim::tree_proportions::tree_proportions;
use sloppy_core::sim::veterancy::rank_index;
use sloppy_core::sim::{
    CoverKind, FragmentShape, MatchPhase, PickupKind, RenderState, SimEvent, SimEventType, Team,
    VehicleKind, Weapon, WreckPart,
};

use super::PRESENTATION_EFFECTS;
use super::camera_rig::{CameraRig, ViewerPose};
use super::generated::{GeneratedTextures, SOIL_ROWS_PER_STEP};
use super::house_prep::{
    HousePreparation, PREPARE_BUDGET_MS, PREPARE_STEPS, Prepared, release_spent_houses,
};
use super::house_rest::SettledShards;
use super::hud::{
    BAR_FADE_RATE, BarTrail, approach, bar_scale, health_bar_state, protection_meters, spawn_pulse,
};
use super::model_catalog::{self, CoverModel, SceneryMover, SceneryWater, TreeParts, cover_key};
use super::models::{self as own, joint};
use super::posing::{JointBasis, dvec3, euler_yxz, joint_world, lerp, quat, vec3};
use super::preparation::Preparation;
use super::suspension::{MotionInput, TankMotion, engine_shudder};
use super::theme::{
    SHADOW_BIAS, SHADOW_FILTER_RADIUS, SHADOW_MAP_SIZE, SHADOW_NORMAL_BIAS, SHADOW_RECEIVER_FLOOR,
    Theme, ThemeLook, theme_look,
};
use super::view_settings::{
    BAR_HEIGHT, CAMERA, CAMERA_FEEL, FEEDBACK, FIRST_PERSON, OVERHEAD_RETICLE_SCALE,
    PLAYER_BAR_HEIGHT, RETICLE_HEIGHT, TANK_FEEL,
};
use super::wreck_library::{SAMPLE_WRECK, age_materials, library_wrecks};
use crate::camera::PerspectiveCamera;
use crate::color::hex_to_linear;
use crate::effects::leaves::Crown as LeafCrown;
use crate::effects::pickups::{supply_turn, supply_wear};
use crate::effects::spawn_pad_decks::SpawnPadDecks;
use crate::effects::{CosmeticRandom, Effects};
use crate::gpu::{
    Environment, Fog, InstanceId, Lifetime, ModelId, PendingModel, PrepareProgress, Renderer,
    SunShadow, WaterSettings,
};

mod inspect;

/// A dead tank's model hides; the live model rides this far below the body.
const TANK_MODEL_DROP: f64 = 0.4;
/// A tank takes a supply by driving over it: the load, already pressed down under
/// the hull's edge, bursts open under it. Its lid board pops off and two splinters
/// of it fly out from under the hull's side (`LOOSE_*`), land, hop once and lie
/// there until they fade; and `STOWED` pieces of its own stock (cans, a tube,
/// shells, a tool box) are tossed up from beside the hull, one `STOW_STAGGER`
/// seconds after the other, rising first and then over in an arc `STOW_ARC` metres
/// above the straight line, onto the tank's rear deck over `STOW_FLIGHT` seconds,
/// where they settle with a little bump and ride along until they fade
/// (`STOW_HOLD`, `PIECE_FADE`). The crushed grenade vents one puff of its smoke and
/// the hull dips as it rides over the drop (`COLLECT_ROCK`, rad/s). A pallet under
/// a hull is crushed by up to `PALLET_CRUSH` of its height and springs back once
/// the hull has passed. At most `MAX_SUPPLY_PIECES` pieces are out at once.
const COLLECT_ROCK: f64 = 0.3;
const PALLET_CRUSH: f64 = 0.7;
const STOWED: usize = 2;
const MAX_SUPPLY_PIECES: usize = 15;
const STOW_DELAY: f64 = 0.03;
const STOW_STAGGER: f64 = 0.09;
const STOW_ARC: f64 = 1.25;
const STOW_FLIGHT: f64 = 0.42;
const STOW_HOLD: f64 = 1.5;
/// How hard a stowed piece bumps as it lands (share of its height) and over how
/// long it settles.
const STOW_BUMP: f64 = 0.16;
const STOW_SETTLE: f64 = 0.18;
/// Where pieces stow: in the back of the turret, where crews strap their kit and
/// a bustle rack carries it (`STOW_BACK` of the way from the turret's pivot to
/// its rear, `STOW_ACROSS` of its width either side of its middle), riding round
/// with it, so the overhead camera sees them whichever way the hull faces; on a
/// vehicle with no turret to speak of, on its rear deck `DECK_END` metres in from
/// the hull's end, `STOW_ACROSS` of the hull's width either side. Either way each
/// rests on the highest surface under it.
const STOW_BACK: f64 = 0.62;
const STOW_ACROSS: f64 = 0.2;
const DECK_END: f64 = 0.45;
/// How far outside the hull's footprint pieces leave from, their height there, and
/// the stowed pieces' size against the load's.
const PIECE_MARGIN: f64 = 0.3;
const PIECE_RISE: f64 = 0.35;
const STOW_SCALE: f64 = 0.8;
/// The broken load's lid board and splinters: their launch speed out from the hull
/// and up, how far each splinter's bearing strays from the board's, their size
/// against the board, gravity, what the hop keeps of the landing speed (across,
/// up), how long they lie and how long any piece takes to fade.
const LOOSE_SPEED: [f64; 2] = [2.4, 4.6];
const SPLINTER_SPEED: [f64; 2] = [3.1, 3.8];
const SPLINTER_BEARING: f64 = 0.65;
const SPLINTER_SCALE: [f32; 3] = [0.75, 0.9, 0.22];
const LOOSE_GRAVITY: f64 = 9.8;
const LOOSE_HOP: [f64; 2] = [0.35, 0.22];
const LOOSE_HOLD: f64 = 2.2;
const PIECE_FADE: f64 = 0.4;
/// A tank driving onto a stocked crate presses it down under its hull before it is
/// close enough to take it: from when the crate's centre is `PRESS_REACH` outside
/// the hull's footprint until it is `PRESS_DEPTH` inside it, down to `PRESS_FLOOR`
/// of its height, bulging out by `PRESS_BULGE`.
const PRESS_REACH: f64 = 0.85;
const PRESS_DEPTH: f64 = 0.1;
const PRESS_FLOOR: f64 = 0.3;
const PRESS_BULGE: f64 = 0.12;
/// Tanks further than this from a crate (origin to its centre) cannot reach it.
const TANK_PRESS_RANGE: f32 = 5.5;
/// A restocked supply is dropped onto its fresh pallet from `RESTOCK_DROP` metres,
/// falling for `RESTOCK_FALL` seconds and squashing by `RESTOCK_SQUASH` as it lands.
const RESTOCK_DROP: f64 = 2.4;
const RESTOCK_FALL: f64 = 0.36;
const RESTOCK_SQUASH: f64 = 0.16;
/// An emptied pallet keeps one of its load's lid boards, band and code up, lying
/// on its deck (where, its turn and its size against the board thrown off), so a
/// player can read what will come back there.
const MARKER_AT: [f32; 2] = [0.1, 0.06];
const MARKER_TURN: f32 = 0.2;
const MARKER_SCALE: f32 = 1.55;
/// Felling boughs (`tree-debris.ts`). Their leaves catch the air: linear drag per
/// second slows the fall and the tumble. Landed, they lie until the debris cleanup
/// sinks them out of sight.
const MAX_BRANCHES: usize = 32;
const BRANCH_LIFETIME: f64 = 9.0;
const BRANCH_GRAVITY: f64 = 9.8;
const BRANCH_DRAG: f32 = 1.6;
const BRANCH_TUMBLE: f32 = 1.8;
const BRANCH_REST: f32 = 0.03;
/// A shell hit shudders a tree's crown about the top of its stump: peak lean
/// (rad), decay time (s), swing rate (rad/s), and when it is still again (s).
const TREE_SHUDDER: [f64; 4] = [0.04, 0.5, 9.0, 2.5];
/// How far from a tree's trunk a shell impact still counts as hitting it.
const TREE_HIT_REACH: f64 = 2.5;
/// A timber impact this close to a loose member's surface scars it, up to this
/// many scars per member (each one rebuilds the member's small model).
const DEBRIS_HIT_REACH: f32 = 0.15;
const MAX_DEBRIS_MARKS: usize = 8;
const DEBRIS_MARK_SIZE: f64 = 0.8;
/// Debris sinks by its height plus this margin while it fades.
const SINK_MARGIN: f32 = 0.03;
/// Milliseconds of a frame that marked covers may spend rebuilding their looks
/// before the rest wait for the next frame (one is always rebuilt).
const COVER_REBUILD_MS: f64 = 3.0;
/// An extra level's yard floor sits just above the ground plane, its outer apron
/// just below.
const CUSTOM_FLOOR_Y: f64 = 0.008;
const CUSTOM_OUTER_FLOOR_Y: f64 = -0.002;

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

thread_local! {
    // Looked up once: the house preparation reads the clock after each step.
    static PERFORMANCE: Option<web_sys::Performance> =
        web_sys::window().and_then(|window| window.performance());
}

/// The page clock (`performance.now()`), in milliseconds.
fn now_ms() -> f64 {
    PERFORMANCE.with(|performance| performance.as_ref().map_or(0.0, |p| p.now()))
}

/// World heights of the eight corners of `bounds` under `world`.
fn corner_heights(bounds: &Aabb, world: &Mat4) -> impl Iterator<Item = f32> {
    (0..8).map(move |i| {
        let corner = DVec3::select(
            BVec3::new(i & 1 != 0, i & 2 != 0, i & 4 != 0),
            bounds.max,
            bounds.min,
        );
        world.transform_point3(corner.as_vec3()).y
    })
}

/// Height of `bounds` under `world`, like `Box3.setFromObject` after a pose.
fn world_height(bounds: &Aabb, world: &Mat4) -> f32 {
    let (low, high) = corner_heights(bounds, world)
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(low, high), y| {
            (low.min(y), high.max(y))
        });
    high - low
}

fn lowest(bounds: &Aabb, world: &Mat4) -> f32 {
    corner_heights(bounds, world).fold(f32::INFINITY, f32::min)
}

// ------------------------------------------------------------------ shared models

struct TankModel {
    model: ModelId,
    joints: usize,
    /// The turret's alternative liveries (none on the Humvee); a tank shows the one
    /// its id picks.
    liveries: [Option<usize>; part::LIVERIES.len()],
    hull: JointBasis,
    turret: JointBasis,
    barrel: JointBasis,
    track: JointBasis,
    muzzle_height: f64,
    /// Where a collected supply's pieces land: two spots in the frame of the
    /// turret (or, without one, the hull) as `stow_on_turret` says.
    stow: [Vec3; 2],
    stow_on_turret: bool,
}

struct BarModel {
    model: ModelId,
    fills: [JointBasis; 3],
    trail: JointBasis,
    ranks: [usize; 3],
    shield: usize,
    shield_fill: JointBasis,
    spawn: JointBasis,
    spawn_fill: JointBasis,
}

/// A pickup kind's supply item, stocked on its pallet.
struct PickupModel {
    gem: ModelId,
}

/// What every supply drop shares: the bare pallet and its grenade in its scuffed
/// ground, each kind's piece a collection sends aboard (by `PickupKind::ALL`'s
/// order) and the board its broken load throws off.
struct PadModels {
    base: ModelId,
    parcels: Vec<BoundedModel>,
    board: BoundedModel,
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
    pickup_pad: Option<PadModels>,
    /// Every wreck is built and the vehicles' source geometry released.
    complete: bool,
}

/// Whether solid cover hides the line from `eye` to `target`: seen from the turret,
/// a bar behind a rock, container, house or wall would otherwise draw through it.
/// Footprints are the covers' axis-aligned nav boxes, rising from the ground to
/// their height; open or small cover (trees, towers on legs, drums) never hides one.
/// Cover meshes bulge a little past their nav footprints.
const SIGHT_MARGIN: f64 = 0.3;

fn sight_blocked(eye: Vec3, target: Vec3, covers: &[RenderCover]) -> bool {
    let (ex, ey, ez) = (f64::from(eye.x), f64::from(eye.y), f64::from(eye.z));
    let (dx, dy, dz) = (
        f64::from(target.x) - ex,
        f64::from(target.y) - ey,
        f64::from(target.z) - ez,
    );
    covers
        .iter()
        .filter(|cover| {
            cover.alive
                && matches!(
                    cover.kind,
                    CoverKind::Rock
                        | CoverKind::Container
                        | CoverKind::Cargo
                        | CoverKind::House
                        | CoverKind::Timber
                        | CoverKind::Concrete
                )
        })
        .any(|cover| {
            let (mut enter, mut leave) = (0.0_f64, 1.0_f64);
            for (origin, delta, centre, half) in [
                (ex, dx, cover.x, cover.w / 2.0 + SIGHT_MARGIN),
                (ez, dz, cover.z, cover.d / 2.0 + SIGHT_MARGIN),
            ] {
                if delta.abs() < 1e-9 {
                    if (origin - centre).abs() > half {
                        return false;
                    }
                    continue;
                }
                let (a, b) = (
                    (centre - half - origin) / delta,
                    (centre + half - origin) / delta,
                );
                enter = enter.max(a.min(b));
                leave = leave.min(a.max(b));
                if enter > leave {
                    return false;
                }
            }
            // The sight line's lowest point over the footprint.
            let lowest = ey + dy * if dy < 0.0 { leave } else { enter };
            lowest < cover.h
        })
}

/// A stowed piece `age` seconds after it left `from`: rising first and then over in
/// its arc onto the stowage spot (`deck`, the hull's turn), tumbling a little as it
/// flies and squaring up to land at `turn` against the hull, then settling with a
/// bump and riding there.
fn stowed_pose(
    from: Vec3,
    (deck, hull): (Vec3, Quat),
    half: f32,
    turn: f32,
    age: f64,
    scale: Vec3,
) -> Mat4 {
    let t = (age / STOW_FLIGHT).min(1.0);
    let to = deck + hull * Vec3::Y * half;
    // Across slowly at first, so it is up over the hull's side before it crosses.
    let run = smoothstep(t, 0.0, 1.0) as f32;
    let lift = (STOW_ARC * 4.0 * t * (1.0 - t)) as f32;
    let mut position = from.lerp(to, run);
    position.y = from.y + (to.y - from.y) * t as f32 + lift;
    let tumble = (1.0 - t) as f32;
    let rotation = hull
        * Quat::from_rotation_y(turn + 0.9 * tumble)
        * Quat::from_rotation_x(1.5 * tumble)
        * Quat::from_rotation_z(-0.6 * tumble);
    let settled = age - STOW_FLIGHT;
    let squash = if settled > 0.0 && settled < STOW_SETTLE * 3.0 {
        (STOW_BUMP
            * (-settled * 3.0 / STOW_SETTLE).exp()
            * (settled * std::f64::consts::TAU / STOW_SETTLE).cos()) as f32
    } else {
        0.0
    };
    // Squashed about its foot, so it stays on the deck.
    let size = scale * Vec3::new(1.0 + squash * 0.5, 1.0 - squash, 1.0 + squash * 0.5);
    Mat4::from_scale_rotation_translation(size, rotation, position - hull * Vec3::Y * half * squash)
}

/// A loose piece `age` seconds after it left `from` at `velocity`: flying and
/// flipping over by `flips` half turns, landing flat on the ground at its `half`
/// thickness, one low hop, then lying still.
fn loose_pose(
    from: Vec3,
    velocity: Vec3,
    scale: Vec3,
    half: f32,
    spin: f32,
    flips: f32,
    age: f64,
) -> Mat4 {
    let gravity = LOOSE_GRAVITY;
    let rise = f64::from(velocity.y);
    let drop = f64::from(from.y - half).max(0.0);
    let land = (rise + (rise * rise + 2.0 * gravity * drop).sqrt()) / gravity;
    let hop_rise = (rise - gravity * land).abs() * LOOSE_HOP[1];
    let hop = 2.0 * hop_rise / gravity;
    let across = Vec3::new(velocity.x, 0.0, velocity.z);
    let landed = from + across * land as f32;
    let position = if age < land {
        from + velocity * age as f32 - Vec3::Y * (0.5 * gravity * age * age) as f32
    } else {
        let a = age.min(land + hop) - land;
        let height = (hop_rise * a - 0.5 * gravity * a * a).max(0.0);
        landed.with_y(half) + across * (LOOSE_HOP[0] * a) as f32 + Vec3::Y * height as f32
    };
    let flight = (age / (land + hop)).min(1.0) as f32;
    let flip = std::f32::consts::PI * flips * (age / land).min(1.0) as f32;
    let heading = velocity.x.atan2(velocity.z);
    let rotation = Quat::from_rotation_y(heading + spin * flight) * Quat::from_rotation_x(flip);
    Mat4::from_scale_rotation_translation(scale, rotation, position)
}

fn joint_index(renderer: &Renderer, model: ModelId, name: &str) -> usize {
    renderer
        .model_node(model, name)
        .unwrap_or_else(|| panic!("model joint {name}"))
}

/// The top of whatever `node` (in `parent`'s frame, without its gun) has over
/// each of `spots` (x, z in the model's frame), found by dropping a ray onto its
/// triangles; `None` where nothing lies under a spot.
fn drop_onto<const N: usize>(node: &Node, parent: DMat4, spots: [[f64; 2]; N]) -> [Option<f64>; N] {
    let mut tops = [None::<f64>; N];
    let world = node.world_matrix(parent);
    let mut visit = |node: &Node, world: DMat4| {
        let Some(drawable) = &node.drawable else {
            return;
        };
        let mesh = &drawable.mesh;
        let point = |index: usize| {
            let [x, y, z] = mesh.positions[index];
            world.transform_point3(DVec3::new(f64::from(x), f64::from(y), f64::from(z)))
        };
        let count = mesh.positions.len();
        let triangles: Vec<[usize; 3]> = match &mesh.indices {
            Some(indices) => indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|t| t.map(|i| i as usize))
                .collect(),
            None => (0..count / 3)
                .map(|t| [3 * t, 3 * t + 1, 3 * t + 2])
                .collect(),
        };
        for [a, b, c] in triangles {
            let (a, b, c) = (point(a), point(b), point(c));
            let det = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
            if det.abs() < 1e-9 {
                continue;
            }
            for ([x, z], top) in spots.iter().zip(&mut tops) {
                // Barycentric weights of the spot in the triangle's plan.
                let wa = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / det;
                let wb = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / det;
                let wc = 1.0 - wa - wb;
                if wa >= 0.0 && wb >= 0.0 && wc >= 0.0 {
                    let y = wa * a.y + wb * b.y + wc * c.y;
                    *top = Some(top.map_or(y, |top: f64| top.max(y)));
                }
            }
        }
    };
    if node.drawable.is_some() {
        visit(node, world);
    }
    for child in node.children.iter().filter(|c| c.name != part::BARREL) {
        child.traverse(world, &mut visit);
    }
    tops
}

/// A vehicle's two stowage spots (see `STOW_BACK`) in the own frame of its turret,
/// or of its hull when the turret has nothing under them, and which.
fn stowage(source: &Node, kind: VehicleKind) -> ([Vec3; 2], bool) {
    let dimensions = tank_hull(kind);
    let back = -(tank_visual_muzzle(kind).z - dimensions.center.z).signum();
    if let Some((turret, parent)) = source.find_with_parent_world(part::TURRET, DMat4::IDENTITY) {
        let world = turret.world_matrix(parent);
        let pivot = world.w_axis.truncate();
        // The turret's reach behind its pivot and its width, without its gun.
        let (mut rear, mut low_x, mut high_x) = (0.0_f64, f64::MAX, f64::MIN);
        for child in turret.children.iter().filter(|c| c.name != part::BARREL) {
            child.traverse(world, &mut |node, world| {
                if let Some(drawable) = &node.drawable {
                    for &[x, y, z] in &drawable.mesh.positions {
                        let p = world.transform_point3(DVec3::new(
                            f64::from(x),
                            f64::from(y),
                            f64::from(z),
                        ));
                        rear = rear.max((p.z - pivot.z) * back);
                        low_x = low_x.min(p.x);
                        high_x = high_x.max(p.x);
                    }
                }
            });
        }
        let z = pivot.z + back * rear * STOW_BACK;
        let across = (high_x - low_x).max(0.0) * STOW_ACROSS;
        let spots = [[pivot.x - across, z], [pivot.x + across, z]];
        if let [Some(left), Some(right)] = drop_onto(turret, parent, spots) {
            let own = world.inverse();
            let spots = [(spots[0], left), (spots[1], right)]
                .map(|([x, z], y)| own.transform_point3(DVec3::new(x, y, z)).as_vec3());
            return (spots, true);
        }
    }
    let Some((hull, parent)) = source.find_with_parent_world(part::HULL, DMat4::IDENTITY) else {
        return ([Vec3::ZERO; 2], false);
    };
    let z = dimensions.center.z + back * (dimensions.size.z / 2.0 - DECK_END);
    let across = dimensions.size.x * STOW_ACROSS;
    let spots = [
        [dimensions.center.x - across, z],
        [dimensions.center.x + across, z],
    ];
    let top = dimensions.center.y + dimensions.size.y / 2.0;
    let tops = drop_onto(hull, parent, spots);
    let own = hull.world_matrix(parent).inverse();
    let spots = [0, 1].map(|i| {
        let [x, z] = spots[i];
        own.transform_point3(DVec3::new(x, tops[i].unwrap_or(top), z))
            .as_vec3()
    });
    (spots, false)
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
            let (stow, stow_on_turret) = stowage(&source, kind);
            TankModel {
                model,
                joints: renderer.model_nodes(model).len(),
                liveries: part::LIVERIES.map(|name| renderer.model_node(model, name)),
                hull: basis(renderer, model, &source, part::HULL),
                turret: basis(renderer, model, &source, part::TURRET),
                barrel: basis(renderer, model, &source, part::BARREL),
                track: basis(renderer, model, &source, part::TRACK_GROUP),
                muzzle_height: tank_visual_muzzle(kind).y,
                stow,
                stow_on_turret,
            }
        })
    }

    fn bar(&mut self, renderer: &mut Renderer, team: Team) -> &BarModel {
        self.bars.entry(team).or_insert_with(|| {
            let source = own::tank_bar(team);
            // Bars fade out behind cover.
            let model = renderer.add_fading_model(&source, Lifetime::Shared);
            BarModel {
                model,
                fills: joint::BAR_FILLS.map(|name| basis(renderer, model, &source, name)),
                trail: basis(renderer, model, &source, joint::BAR_TRAIL),
                ranks: joint::BAR_RANKS.map(|name| joint_index(renderer, model, name)),
                shield: joint_index(renderer, model, joint::BAR_SHIELD),
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
            // Wreckage fades as it is cleared away.
            BoundedModel {
                model: renderer.add_fading_model(&source, Lifetime::Shared),
                bounds: node_bounds(&source, DMat4::IDENTITY),
            }
        })
    }

    fn pickup(&mut self, renderer: &mut Renderer, kind: PickupKind) -> &PickupModel {
        // First person sees through the supplies.
        self.pickups.entry(kind).or_insert_with(|| PickupModel {
            gem: renderer.add_fading_model(&pickup_cube(kind), Lifetime::Shared),
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
            let source = own::debris_piece(shape);
            // Debris fades as it is cleared away.
            BoundedModel {
                model: renderer.add_fading_model(&source, Lifetime::Shared),
                bounds: node_bounds(&source, DMat4::IDENTITY),
            }
        })
    }

    /// The pallet every supply drop shares, the pieces a collection throws; instance
    /// data paints them by kind.
    fn pickup_pad(&mut self, renderer: &mut Renderer) -> &PadModels {
        self.pickup_pad.get_or_insert_with(|| {
            // The thrown pieces fade once they land.
            let mut bounded = |source: Node| BoundedModel {
                model: renderer.add_fading_model(&source, Lifetime::Shared),
                bounds: node_bounds(&source, DMat4::IDENTITY),
            };
            PadModels {
                parcels: PickupKind::ALL
                    .into_iter()
                    .map(|kind| bounded(own::pickup_parcel_model(kind)))
                    .collect(),
                board: bounded(own::pickup_board_model()),
                base: renderer.add_model(&own::pickup_base(), Lifetime::Shared),
            }
        })
    }
}

// ------------------------------------------------------------------ entity views

struct TankView {
    kind: VehicleKind,
    team: Team,
    instance: InstanceId,
    bar: InstanceId,
    /// Whether the bar would show without cover in the way: the tank is alive and the
    /// view is not seated in it.
    bar_shown: bool,
    /// The bar's centre above the drawn hull.
    bar_anchor: Vec3,
    /// The bar's opacity, easing out behind cover and back in (0..1).
    bar_fade: f64,
    /// The pale chunk of hull the last hit took; `None` until the tank is drawn alive.
    trail: Option<BarTrail>,
    /// Drawn-pose motion (suspension, hit shove and flash, treads); `None` while
    /// destroyed, so a respawn starts level.
    motion: Option<TankMotion>,
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
    /// The crown joint's authored transform, and the height it shudders about.
    crown_rest: Mat4,
    pivot: f32,
    /// What the crown sheds when hit or felled.
    leaves: LeafCrown,
    /// The horizontal axis and start time of the shudder from the last hit.
    shudder: Option<(Vec3, f64)>,
}

struct CoverView {
    instance: InstanceId,
    /// The cover's joint when it lives in the round's combined static-cover
    /// model (hidden instead of removed); `None` for its own instance.
    joint: Option<usize>,
    key: String,
    stage: u32,
    hits: usize,
    /// Rebuilt towers reuse rubble identities but choose a new look on collapse.
    debris_seed: Option<f64>,
    movable: bool,
    scale: Vec3,
    world: Mat4,
    tree: Option<TreeView>,
}

/// A hit cover's new look being built over a few frames while its last one shows.
struct CoverLook {
    cover: u32,
    /// The look being built (`cover_key`).
    key: String,
    phase: LookPhase,
}

/// Where a cover's new look has got to. Each phase is a step of its own: a model is
/// uploaded in a separate step from building it (a cottage's also batched and
/// prepared), so a slow device spreads the work over frames instead of taking it in one.
enum LookPhase {
    /// A cottage's model, built a step at a time (its kits in progress are large).
    Building(Box<HouseCoverBuild>),
    /// A cottage's model, built, to batch and prepare for upload.
    Built(Node),
    /// A crate's or timber wall's look, its model still to build.
    Unbuilt,
    /// Ready to upload and show (prepared for upload ahead for a cottage).
    Ready(CoverModel, Option<PendingModel>),
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
    /// A [`SharedDebris`] model, which every fragment of its look shares this round.
    Shared(Aabb),
}

/// Looks each crate section comes in, so a crate's matching sections differ.
const CRATE_SECTION_LOOKS: u32 = 3;

/// A crate section's look: the section, its size in whole millimetres (as
/// replication rounds it), the crate's paint and which of [`CRATE_SECTION_LOOKS`]
/// it wears.
type SectionKey = (CratePiece, [i64; 3], u32, u32);

fn section_key(piece: CratePiece, size: DVec3, color: u32, look: u32) -> SectionKey {
    let millimetres = (size * 1000.0).round().as_i64vec3().to_array();
    (piece, millimetres, color, look)
}

/// A debris look that depends only on what broke, so one model serves every fragment
/// wearing it this round and is built ahead, a look a frame, before anything breaks:
/// a crate section, or a piece of a toppled watchtower in its paint. Building a
/// collapse's pieces on the frame it fell cost a weak device tens of milliseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SharedDebris {
    Section(SectionKey),
    Tower(TowerPiece, u32),
}

struct FragmentView {
    instance: InstanceId,
    look: FragmentLook,
    sink: Option<f32>,
    /// It fades while it sinks; a felled crown only sinks, so its alpha-tested
    /// foliage never turns into a blended sheet.
    fades: bool,
    /// A loose timber member and the scars shells have left on it since it fell.
    timber: Option<TimberPart>,
    /// The body's last pose, to find where a shell struck the piece.
    pose: (Vec3, Quat),
}

struct PickupView {
    base: InstanceId,
    gem: InstanceId,
    /// The lid board left on the emptied pallet, and where it lies on the deck.
    marker: InstanceId,
    marker_rest: Mat4,
    /// Where the pallet and the item on it stand at rest.
    ground: Mat4,
    rest: Mat4,
    /// Whether the views show the supply stocked (None: not yet set), when it was
    /// last restocked (for the drop onto its pallet) and whether the item is posed
    /// away from rest (dropping or pressed under a hull).
    shown: Option<bool>,
    restocked: f64,
    posed: bool,
    /// How far a hull has crushed the pallet (0..1), as last posed.
    crush: f32,
    /// How far the supply has restocked (0..1, 1 while stocked).
    progress: f32,
}

struct MineView {
    instance: InstanceId,
    light: usize,
}

/// A piece of a collected supply: one of its stock tossed aboard the tank that
/// took it, or a board of its broken load thrown loose.
struct SupplyPiece {
    instance: InstanceId,
    model: ModelId,
    /// When it left the load (view time) and where from.
    start: f64,
    from: Vec3,
    flight: PieceFlight,
    /// Whether it is out (else it waits, hidden, to be thrown again).
    active: bool,
}

#[derive(Clone, Copy)]
enum PieceFlight {
    /// Tossed onto stowage spot `slot` of tank `tank`: its half height and its
    /// turn against the turret or hull once stowed (rad).
    Stow {
        tank: u32,
        slot: usize,
        half: f32,
        turn: f32,
    },
    /// Thrown loose: its launch velocity, its scale, the half thickness it rests
    /// on, its spin about the vertical over the flight (rad) and the half turns it
    /// flips over before it lands.
    Loose {
        velocity: Vec3,
        scale: Vec3,
        half: f32,
        spin: f32,
        flips: f32,
    },
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
    /// How far it sinks to leave sight once landed.
    sink: f32,
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
    /// Library wrecks the page has yet to build ([`Presentation::prebuild_step`]).
    pub library_pending: u32,
}

pub struct Presentation {
    pub renderer: Renderer,
    pub effects: Effects,
    pub rig: CameraRig,
    /// Presentation clock in seconds (effects, water, flags, bobbing).
    pub time: f64,
    pub crosshair_visible: bool,
    pub crosshair: Mat4,
    /// The reticle's resting size: it keeps its size on screen as the overhead
    /// camera closes in, and confirmed hits pulse it larger.
    reticle_size: f64,
    /// Phones aim overhead by touching the arena and draw no overhead reticle; the
    /// first-person gun sight stays.
    pub hide_reticle: bool,
    /// The view's height in CSS pixels (0 until the page says), which keeps the
    /// overhead bars readable on a phone's short screen.
    pub css_height: f64,
    /// Bars step aside while the battle waits behind a menu (pause, settings, the
    /// report), so none floats crisp over the dialog's blur.
    bars_hidden: bool,
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
    /// Hit cottages', crates' and timber walls' new looks still being built or
    /// uploaded (one per cover at most).
    cover_looks: Vec<CoverLook>,
    /// Whether this frame already rebuilt a cover's look or built a debris model:
    /// work that can wait (debris looks built ahead, a cottage's collapse cut ahead)
    /// then waits for a quieter frame instead of piling onto this one.
    busy: bool,
    fragments: HashMap<u32, FragmentView>,
    /// Debris models built this round, shared by every fragment that looks the same:
    /// crate walls split at a few places, so sizes repeat between crates, and every
    /// watchtower of a paint breaks into the same pieces.
    shared_debris: HashMap<SharedDebris, BoundedModel>,
    /// Debris looks this round's covers can break into, still to build; one is built
    /// a frame, so a crate or tower breaking builds none.
    debris_warmups: Vec<SharedDebris>,
    /// The pieces of each collapsing cottage still to be given a view: the whole
    /// set is taken from the preparation, or built, when its first piece appears.
    house_pieces: HashMap<u32, Vec<Prepared<(ModelId, Aabb)>>>,
    /// The collapse pieces of the most damaged standing cottage, cut and uploaded
    /// a little each frame before it falls.
    house_prep: HousePreparation<(ModelId, Aabb)>,
    /// Collapsed cottages' resting shards, merged into one model each.
    settled: SettledShards<(ModelId, InstanceId)>,
    /// Whether every cottage chimney that smokes still stands, as the smoke was
    /// last filled.
    chimneys: Vec<(u32, bool)>,
    pickups: HashMap<u32, PickupView>,
    /// Pieces of collected supplies, out or waiting to be thrown (bounded by
    /// `MAX_SUPPLY_PIECES`, reused).
    supply_pieces: Vec<SupplyPiece>,
    mines: HashMap<u32, MineView>,
    /// The spawn pads mines may lie on, for their drawn height.
    pad_decks: SpawnPadDecks,
    branches: VecDeque<FallingBranch>,
    /// The followed tank, for events that shake the camera.
    viewer_id: Option<u32>,
    hit_confirm_until: f64,
    spawn_cue: f64,
    player_was_alive: bool,
    sample_models: Vec<ModelId>,
    random: CosmeticRandom,
    /// Canvas-drawn and baked textures the scenery and cover sample.
    textures: GeneratedTextures,
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
        // The spawn pulse fades as it spreads.
        let hud = |renderer: &mut Renderer, node: &Node| {
            let model = renderer.add_fading_model(node, Lifetime::Shared);
            let instance = renderer
                .add_instance(model, Mat4::IDENTITY, Lifetime::Shared)
                .expect("model just added");
            renderer.set_visible(instance, false);
            (model, instance)
        };
        let (reticle_model, reticle) = hud(&mut renderer, &own::reticle());
        // The reticle never appears in water reflections; the player ring and spawn
        // pulse on the ground do.
        renderer.set_reflected(reticle, false);
        let reticle = ReticleModel {
            instance: reticle,
            ready: joint_index(&renderer, reticle_model, joint::RETICLE_READY),
            reloading: joint_index(&renderer, reticle_model, joint::RETICLE_RELOADING),
            confirmed: joint_index(&renderer, reticle_model, joint::RETICLE_CONFIRMED),
        };
        let (_, player_ring) = hud(&mut renderer, &own::player_ring());
        let (_, pulse) = hud(&mut renderer, &own::spawn_pulse());
        let mut random = CosmeticRandom::seeded(seed);
        let flags = Flags::new(&mut renderer, &mut random);
        let mut presentation = Self {
            renderer,
            effects,
            rig: CameraRig::default(),
            time: 0.0,
            crosshair_visible: false,
            crosshair: Mat4::IDENTITY,
            reticle_size: 1.0,
            hide_reticle: false,
            css_height: 0.0,
            bars_hidden: false,
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
            cover_looks: Vec::new(),
            busy: false,
            fragments: HashMap::new(),
            shared_debris: HashMap::new(),
            debris_warmups: Vec::new(),
            house_pieces: HashMap::new(),
            house_prep: HousePreparation::default(),
            settled: SettledShards::default(),
            chimneys: Vec::new(),
            pickups: HashMap::new(),
            supply_pieces: Vec::with_capacity(MAX_SUPPLY_PIECES),
            mines: HashMap::new(),
            pad_decks: SpawnPadDecks::default(),
            branches: VecDeque::new(),
            viewer_id: None,
            hit_confirm_until: 0.0,
            spawn_cue: 0.0,
            player_was_alive: false,
            sample_models: Vec::new(),
            random,
            textures: GeneratedTextures::default(),
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

    /// Face every tank bar to `camera` (whose orientation is `rotation`), keep it a
    /// sane size on screen however close the lens comes, and fade it out behind solid
    /// cover over `dt` seconds. The drawn camera places them each frame; a fixed debug
    /// pose redraws them for itself.
    pub fn place_bars(
        &mut self,
        covers: &[RenderCover],
        camera: &PerspectiveCamera,
        rotation: Quat,
        dt: f64,
    ) {
        let eye = camera.position;
        let fov_y = f64::from(camera.fov_y_degrees).to_radians();
        for view in self.tanks.values_mut() {
            if self.bars_hidden {
                self.renderer.set_visible(view.bar, false);
                continue;
            }
            let shown = view.bar_shown && !sight_blocked(eye, view.bar_anchor, covers);
            let fade = if view.bar_shown {
                approach(view.bar_fade, f64::from(u8::from(shown)), BAR_FADE_RATE, dt)
            } else {
                0.0
            };
            // An infinite step is a fixed pose's redraw: it shows its own occlusion but
            // leaves the follow camera's fade where it was.
            if dt.is_finite() {
                view.bar_fade = fade;
            }
            self.renderer.set_visible(view.bar, fade > 0.02);
            self.renderer.set_opacity(view.bar, fade as f32);
            if fade <= 0.02 {
                continue;
            }
            let size = bar_scale(
                f64::from(view.bar_anchor.distance(eye)),
                fov_y,
                own::BAR_RIM_WIDTH,
                self.css_height,
            ) as f32;
            self.renderer.set_transform(
                view.bar,
                Mat4::from_scale_rotation_translation(Vec3::splat(size), rotation, view.bar_anchor),
            );
        }
    }

    /// Where the viewer's tank sits on screen (NDC, y up), so the page's damage ring
    /// wraps it; `None` in first person or when it is off screen.
    pub fn viewer_ndc(&self) -> Option<(f32, f32)> {
        if self.rig.seat_blend > 0.0 {
            return None;
        }
        let follow = self.rig.follow;
        let point = self
            .rig
            .camera
            .project(Vec3::new(follow.x as f32, 1.0, follow.z as f32));
        (point.z > 0.0 && point.z < 1.0 && point.x.abs() < 1.0 && point.y.abs() < 1.0)
            .then_some((point.x, point.y))
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
            pickup_effects: self.effects.systems.pickups.len() as u32,
            branches: self.branches.len() as u32,
            library_pending: library_wrecks()
                .filter(|wreck| !self.library.wrecks.contains_key(wreck))
                .count() as u32,
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
                .add_scenery(&create_spawn_pads(scale), Lifetime::Shared)
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
                .add_scenery(&create_arena_floor(kind, extent, y), Lifetime::Shared)
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
            if shown {
                self.chimneys = model_catalog::standing_chimneys(&state.covers);
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
        self.renderer.set_atmosphere(look.atmosphere);
        self.renderer.set_sun_shadow(SunShadow {
            enabled: true,
            map_size: SHADOW_MAP_SIZE,
            camera: look.shadow,
            bias: SHADOW_BIAS,
            normal_bias: SHADOW_NORMAL_BIAS,
            radius: SHADOW_FILTER_RADIUS,
            receiver_floor: SHADOW_RECEIVER_FLOOR,
        });
    }

    /// Rebuild every entity view for a new round (`reset()`).
    pub fn reset(&mut self, state: &RenderState) {
        self.rig.snap_seat = true;
        if let Some(viewer) = state.viewer() {
            self.rig.first_person.yaw = viewer.aim;
        }
        // The old round's views go before its renderer resources: cover models hold
        // their source meshes, which `reset_round` can then free at once, before this
        // round and a new theme upload into the mesh pages they leave.
        self.tanks.clear();
        self.covers.clear();
        self.cover_models.clear();
        self.cover_looks.clear();
        self.fragments.clear();
        self.shared_debris.clear();
        self.house_pieces.clear();
        // Its models are the round's, which `reset_round` frees.
        self.house_prep = HousePreparation::default();
        // Round models go with the round.
        self.settled.clear();
        self.pickups.clear();
        self.supply_pieces.clear();
        self.mines.clear();
        self.branches.clear();
        self.sample_models.clear();
        self.renderer.reset_round();
        self.apply_theme(state);
        self.rig.settle();
        self.hit_confirm_until = 0.0;
        self.player_was_alive = false;
        self.effects.reset(&mut self.renderer);
        self.add_covers(&state.covers);
        self.queue_debris(&state.covers);
        for tank in &state.tanks {
            self.add_tank(tank);
        }
        for pickup in &state.pickups {
            self.add_pickup(pickup.id, pickup.kind, pickup.x, pickup.z);
        }
        self.update_camera(state, 1.0, 0.0, false);
    }

    // -------------------------------------------------------------- prepare

    /// Register one model per first-use look this round can show (the wrecks' looks,
    /// damaged cargo and timber stages, timber beams, mines, pickup glows, debris
    /// pieces), so `prepare_step` compiles their pipelines before combat instead of
    /// mid-fight. Of the wrecks only the sample is built now (`wreck_library`); the page
    /// builds the rest once the arena is ready ([`Self::prebuild_step`]).
    pub fn begin_prepare(&mut self, state: &RenderState) {
        self.preparation = Preparation::default();
        for kind in VehicleKind::ALL {
            for team in [Team::Blue, Team::Red] {
                self.library.tank(&mut self.renderer, kind, team);
                self.library.bar(&mut self.renderer, team);
                self.library.mine(&mut self.renderer, team);
            }
        }
        let (kind, team, part) = SAMPLE_WRECK;
        self.library.wreck(&mut self.renderer, kind, team, part);
        for kind in PickupKind::ALL {
            self.library.pickup(&mut self.renderer, kind);
        }
        for shape in DEBRIS_SHAPES {
            self.library.debris(&mut self.renderer, shape);
        }
        self.library.pickup_pad(&mut self.renderer);
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
                    let node = timber_part_model(&part);
                    self.sample_models
                        .push(self.renderer.add_model(&node, Lifetime::Round));
                }
            }
            // Falling crowns and boughs reuse the live tree's materials, whose
            // faded variants every movable model registers up front.
        }
        // The arena's whole look is registered: its pipelines compile from here on.
        self.renderer.request_pipelines();
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

    /// Build one of the library's wrecks that preparation left for later; false once
    /// every one is built. Every vehicle and wreck is then uploaded and kept for the
    /// page, so their source geometry goes, leaving room in the Wasm heap for what a
    /// battle builds.
    pub fn prebuild_step(&mut self) -> bool {
        if self.library.complete {
            return false;
        }
        if let Some((kind, team, part)) =
            library_wrecks().find(|wreck| !self.library.wrecks.contains_key(wreck))
        {
            self.library.wreck(&mut self.renderer, kind, team, part);
            return true;
        }
        release_tank_models();
        self.library.complete = true;
        false
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
        let (model, joints, liveries) = {
            let tank_model = self.library.tank(&mut self.renderer, tank.kind, tank.team);
            (tank_model.model, tank_model.joints, tank_model.liveries)
        };
        let bar_model = self.library.bar(&mut self.renderer, tank.team).model;
        let instance = self
            .renderer
            .add_instance(model, Mat4::IDENTITY, Lifetime::Round)
            .expect("tank model");
        // Team-mates of one chassis wear different unit numbers and stowage; a
        // human player's tank wears the crew livery.
        let worn = if tank.human {
            part::CREW_LIVERY
        } else {
            tank.id as usize % part::CREW_LIVERY
        };
        for (index, joint) in liveries.into_iter().enumerate() {
            if let Some(joint) = joint {
                self.renderer
                    .set_node_visible(instance, joint, index == worn);
            }
        }
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
                bar_shown: false,
                bar_anchor: Vec3::ZERO,
                bar_fade: 0.0,
                trail: None,
                motion: None,
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

    /// The shared model of a cover's look, building it on first use (or taking
    /// `built_model`, built ahead). Returns its key and the cover's world placement.
    fn cover_entry(
        &mut self,
        cover: &RenderCover,
        stage: u32,
        built_model: Option<(CoverModel, Option<PendingModel>)>,
    ) -> (String, Mat4) {
        let movable = cover.motion.is_some();
        let built = Self::built_cover(cover);
        let key = Self::cover_model_key(cover, stage);
        if let Some(entry) = self.cover_models.get_mut(&key) {
            entry.users += 1;
        } else {
            let (CoverModel { mut root, tree }, pending) =
                built_model.unwrap_or_else(|| (model_catalog::cover_model(&built, stage), None));
            if movable {
                // The body's centre is half the cover's height up; drop the parts.
                let drop = cover.h / (2.0 * root.scale.y);
                for child in &mut root.children {
                    child.position.y -= drop;
                }
            }
            self.textures.request_scenery(&mut self.renderer, &root);
            // A model prepared ahead was prepared from this root; only cottages, which
            // never move, are.
            let model = match pending.filter(|_| !movable) {
                Some(pending) => self.renderer.add_pending_model(pending, Lifetime::Round),
                None => self.renderer.add_model(&root, Lifetime::Round),
            };
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

    /// The key a cover's look's model is kept under in `cover_models`: covers that look
    /// the same share it, movable ones apart (their parts are dropped to the body's
    /// centre).
    fn cover_model_key(cover: &RenderCover, stage: u32) -> String {
        let mut key = cover_key(&Self::built_cover(cover), stage);
        if cover.motion.is_some() {
            key += "/movable";
        }
        key
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
        self.add_cover_with(cover, None);
    }

    /// [`add_cover`](Self::add_cover) with its look's model when it was built ahead,
    /// and prepared for upload when that was done ahead too.
    fn add_cover_with(
        &mut self,
        cover: &RenderCover,
        built_model: Option<(CoverModel, Option<PendingModel>)>,
    ) {
        let stage = cover_damage_stage(cover.kind, cover.hp, cover.max_hp);
        let (key, world) = self.cover_entry(cover, stage, built_model);
        let entry = &self.cover_models[&key];
        let model = entry.model;
        let scale = entry.source.scale.as_vec3();
        let nodes = self.renderer.model_nodes(model);
        let tree = entry
            .tree
            .as_ref()
            .map(|parts| tree_view(nodes, parts, 0..nodes.len(), cover));
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
                debris_seed: cover.debris_seed,
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
    /// Queue every shared debris look this round's covers can break into: crate walls
    /// and lids split in a few patterns only, so each size of crate has a few dozen
    /// sections, and each paint of watchtower falls into its twelve pieces. Looks pop
    /// from the back, so the towers' pieces, the costliest to build, come first.
    fn queue_debris(&mut self, covers: &[RenderCover]) {
        self.debris_warmups.clear();
        let mut crates = Vec::new();
        for cover in covers.iter().filter(|cover| cover.kind == CoverKind::Cargo) {
            let shape = [cover.w, cover.d, cover.h].map(f64::to_bits);
            if crates.contains(&(shape, cover.color)) {
                continue;
            }
            crates.push((shape, cover.color));
            for (piece, size) in crate_section_looks(cover.w, cover.d, cover.h) {
                for look in 0..CRATE_SECTION_LOOKS {
                    let key = section_key(piece, DVec3::from_array(size), cover.color, look);
                    let key = SharedDebris::Section(key);
                    if !self.debris_warmups.contains(&key) {
                        self.debris_warmups.push(key);
                    }
                }
            }
        }
        for cover in covers.iter().filter(|cover| cover.kind == CoverKind::Tower) {
            for piece in TowerPiece::ALL.into_iter().rev() {
                let key = SharedDebris::Tower(piece, cover.color);
                if !self.debris_warmups.contains(&key) {
                    self.debris_warmups.push(key);
                }
            }
        }
    }

    /// Build one queued debris look, on a frame that rebuilt no cover and built no
    /// debris (the look is ahead of need, so it waits for a quieter frame).
    fn warm_debris(&mut self) {
        if self.busy {
            return;
        }
        if let Some(key) = self.debris_warmups.pop() {
            self.shared_debris(key);
        }
    }

    /// A shared debris look's model, built on first use. Debris fades as it is
    /// cleared away.
    fn shared_debris(&mut self, key: SharedDebris) -> &BoundedModel {
        let renderer = &mut self.renderer;
        self.shared_debris.entry(key).or_insert_with(|| {
            let node = match key {
                SharedDebris::Section((piece, millimetres, color, look)) => {
                    let size = DVec3::from_array(millimetres.map(|mm| mm as f64 / 1000.0));
                    crate_piece_model(piece, size, color, look)
                }
                SharedDebris::Tower(piece, color) => tower_piece_model(piece, color),
            };
            BoundedModel {
                bounds: node_bounds(&node, DMat4::IDENTITY),
                model: renderer.add_fading_model(&node, Lifetime::Round),
            }
        })
    }

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
            let CoverModel { root: model, tree } = model_catalog::cover_model(cover, stage);
            self.textures.request_scenery(&mut self.renderer, &model);
            let mut joint = Node::group(combined_joint(cover.id));
            joint.children.push(model.clone());
            root.children.push(joint);
            built.push((*cover, stage, Arc::new(model), tree));
        }
        // Each cover joint only ever shows or hides (a cover that changes its look
        // moves out to its own model), so their parts merge across covers.
        let model = self.renderer.add_combined_model(&root, Lifetime::Round);
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
                .map(|parts| tree_view(nodes, parts, joint..end, cover));
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
                    debris_seed: cover.debris_seed,
                    movable: false,
                    scale: Vec3::ONE,
                    world: Mat4::IDENTITY,
                    tree: tree_view,
                },
            );
        }
    }

    fn add_pickup(&mut self, id: u32, kind: PickupKind, x: f64, z: f64) {
        let gem_model = self.library.pickup(&mut self.renderer, kind).gem;
        let base_model = self.library.pickup_pad(&mut self.renderer).base;
        let turn = Quat::from_rotation_y(supply_turn(id) as f32);
        let at = Vec3::new(x as f32, 0.0, z as f32);
        let scale = Vec3::splat(SUPPLY_SCALE as f32);
        let ground = Mat4::from_scale_rotation_translation(scale, turn, at);
        let rest = Mat4::from_scale_rotation_translation(
            scale,
            turn,
            at + Vec3::Y * (PALLET_HEIGHT * SUPPLY_SCALE) as f32,
        );
        let renderer = &mut self.renderer;
        let base = renderer
            .add_instance(base_model, ground, Lifetime::Round)
            .expect("pallet model");
        let gem = renderer
            .add_instance(gem_model, rest, Lifetime::Round)
            .expect("supply model");
        let board = &self.library.pickup_pad.as_ref().expect("pickup pad").board;
        let half = board.bounds.size().y as f32 / 2.0;
        let marker_rest = Mat4::from_scale_rotation_translation(
            Vec3::splat(MARKER_SCALE),
            Quat::from_rotation_y(MARKER_TURN),
            Vec3::new(
                MARKER_AT[0],
                PALLET_HEIGHT as f32 + half * MARKER_SCALE,
                MARKER_AT[1],
            ),
        );
        let marker = renderer
            .add_instance(board.model, ground * marker_rest, Lifetime::Round)
            .expect("marker model");
        renderer.set_visible(marker, false);
        let [u, v] = pickup_tile_offset(kind);
        let wear = supply_wear(id);
        renderer.set_instance_data(gem, [u, v, 1.0, wear]);
        renderer.set_instance_data(marker, [u, v, 1.0, wear]);
        // The pallet's deck, stringer ends and smoke grenade wear the kind's paint.
        renderer.set_instance_data(base, [u, v, 1.0, wear]);
        self.pickups.insert(
            id,
            PickupView {
                base,
                gem,
                marker,
                marker_rest,
                ground,
                rest,
                shown: None,
                restocked: f64::NEG_INFINITY,
                posed: false,
                crush: 0.0,
                progress: 1.0,
            },
        );
    }

    // -------------------------------------------------------------- events

    /// One simulation event; `player_hit` marks hurt/death events the viewer caused.
    pub fn event(&mut self, event: &SimEvent, player_hit: bool) {
        // Debris contacts only raise landing dust (the effects' own filter).
        if event.kind == SimEventType::DebrisImpact {
            self.effects.systems.event(event);
            return;
        }
        if event.kind == SimEventType::Notice {
            return;
        }
        self.effects.systems.event(event);
        if event.kind == SimEventType::Pickup {
            self.collect_supply(event);
        }
        if event.cover_kind == Some(CoverKind::Tree) {
            self.tree_event(event);
        }
        if event.kind == SimEventType::Impact && event.cover_kind == Some(CoverKind::Timber) {
            self.scar_timber_debris(event);
        }
        if player_hit {
            self.hit_confirm_until = self.time + FEEDBACK.hit_confirmation_seconds;
        }
        self.feel_event(event);
    }

    /// Weight for the battle's knocks: a shot rocks its tank back, a hit shoves
    /// and flashes its victim, and blasts rock nearby tanks. The viewer's camera
    /// shakes only when it is hit or destroyed, never for its own shots, kills or
    /// other explosions. The action never pauses or slows for a kill or a hit.
    fn feel_event(&mut self, event: &SimEvent) {
        let viewer = event.id.is_some() && event.id == self.viewer_id;
        match event.kind {
            SimEventType::Shot => {
                let rocket = event.weapon == Some(Weapon::Rocket);
                if let Some(view) = event.id.and_then(|id| self.tanks.get_mut(&id)) {
                    view.motion
                        .get_or_insert_with(TankMotion::default)
                        .pending_fire = if rocket {
                        TANK_FEEL.rocket_rock
                    } else {
                        TANK_FEEL.fire_rock
                    };
                }
            }
            SimEventType::Hurt => {
                let damage = event.size.unwrap_or(0.0);
                let origin = event
                    .damage_source
                    .map(|source| (source.origin.x, source.origin.z));
                if let Some(view) = event.id.and_then(|id| self.tanks.get_mut(&id)) {
                    view.motion.get_or_insert_with(TankMotion::default).hit(
                        (event.x, event.z),
                        origin,
                        damage,
                    );
                }
                if viewer {
                    let (dx, dz) = origin.map_or((0.0, 0.0), |(x, z)| (event.x - x, event.z - z));
                    self.rig.hurt(dx, dz, damage);
                }
            }
            SimEventType::Explosion => {
                let radius = event.size.unwrap_or(2.0);
                self.rock_tanks(event.x, event.z, radius);
            }
            SimEventType::Death => {
                let radius = event.size.unwrap_or(3.0);
                self.rock_tanks(event.x, event.z, radius);
                if viewer {
                    self.rig.shake.add_trauma(CAMERA_FEEL.death_trauma);
                }
            }
            _ => {}
        }
    }

    /// Every drawn tank near a blast at `(x, z)` rocks on its springs.
    fn rock_tanks(&mut self, x: f64, z: f64, radius: f64) {
        for view in self.tanks.values_mut() {
            let Some(motion) = view.motion.as_mut() else {
                continue;
            };
            let at = view.world.w_axis;
            motion.blast((f64::from(at.x), f64::from(at.z)), (x, z), radius);
        }
    }

    /// A shell hit shudders a tree's crown and shakes leaves loose; a felled tree
    /// sheds leaves as it goes over. Impacts name no cover, so the hit tree is the
    /// nearest one within reach.
    fn tree_event(&mut self, event: &SimEvent) {
        let tree = match event.kind {
            SimEventType::Destroy => event
                .id
                .and_then(|id| self.covers.get_mut(&id))
                .and_then(|view| view.tree.as_mut()),
            SimEventType::Impact => self
                .covers
                .values_mut()
                .filter_map(|view| view.tree.as_mut())
                .map(|tree| {
                    let distance = (tree.leaves.x - event.x).hypot(tree.leaves.z - event.z);
                    (distance, tree)
                })
                .filter(|(distance, _)| *distance <= TREE_HIT_REACH)
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, tree)| tree),
            _ => None,
        };
        let Some(tree) = tree else {
            return;
        };
        self.effects.systems.shed_leaves(event, &tree.leaves);
        if event.kind == SimEventType::Impact {
            // The crown leans away from the shell first.
            let push = Vec3::new(
                (tree.leaves.x - event.x) as f32,
                0.0,
                (tree.leaves.z - event.z) as f32,
            )
            .normalize_or(Vec3::X);
            tree.shudder = Some((Vec3::Y.cross(push).normalize_or(Vec3::X), self.time));
        }
    }

    /// A shell that strikes a loose timber member leaves a scar on it, like the
    /// standing wall's. The simulation only marks standing walls (their members are
    /// gameplay data), so loose members are marked here from the impact, which every
    /// viewer receives: the struck member is the one whose surface is nearest.
    fn scar_timber_debris(&mut self, event: &SimEvent) {
        let Some(height) = event.height else {
            return;
        };
        let point = Vec3::new(event.x as f32, height as f32, event.z as f32);
        let struck = self
            .fragments
            .iter()
            .filter_map(|(&id, view)| {
                let part = view.timber.as_ref()?;
                let (position, rotation) = view.pose;
                let local = rotation.inverse() * (point - position);
                let half = Vec3::new(part.w as f32, part.h as f32, part.d as f32) / 2.0;
                let outside = (local.abs() - half).max(Vec3::ZERO).length();
                (outside <= DEBRIS_HIT_REACH && part.marks.len() < MAX_DEBRIS_MARKS)
                    .then_some((outside, id, local))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let Some((_, id, local)) = struck else {
            return;
        };
        let view = self.fragments.get_mut(&id).expect("struck fragment");
        let Some(part) = view.timber.as_mut() else {
            return;
        };
        let mark = part.loose_mark(
            f64::from(local.x),
            f64::from(local.y),
            f64::from(local.z),
            DEBRIS_MARK_SIZE,
        );
        part.marks.push(mark);
        let (model, look) = owned_look(&mut self.renderer, &timber_debris_model(part), true);
        let Some(instance) = self
            .renderer
            .add_instance(model, Mat4::IDENTITY, Lifetime::Round)
        else {
            self.renderer.remove_model(model);
            return;
        };
        self.renderer.remove_instance(view.instance);
        if let FragmentLook::Owned(old, _) = view.look {
            self.renderer.remove_model(old);
        }
        view.instance = instance;
        view.look = look;
        // The next fragment update poses the new instance.
        let (position, rotation) = view.pose;
        self.renderer.set_transform(
            instance,
            Mat4::from_rotation_translation(rotation, position),
        );
    }

    /// Screen angle of damage for the HUD's direction indicator.
    pub fn damage_angle(&self, event: &SimEvent) -> Option<f64> {
        let origin = event.damage_source?.origin;
        self.rig
            .damage_angle((event.x, event.z), (origin.x, origin.z))
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
        self.busy = false;
        self.textures.step(&mut self.renderer, SOIL_ROWS_PER_STEP);
        self.flags
            .update(&mut self.renderer, self.time, &mut self.random);
        self.update_scenery();
        self.update_camera(state, alpha, dt, overview);
        self.update_player_indicators(state, dt, overview);
        self.update_tanks(state, alpha, dt);
        self.place_first_person_eye(state);
        // Bars face and size for the final camera, seated in the turret or overhead.
        self.bars_hidden = state.match_state.phase != MatchPhase::Playing;
        let camera = self.rig.camera;
        self.place_bars(&state.covers, &camera, self.rig.rotation, dt);
        self.update_branches(dt);
        self.update_covers(state);
        self.update_pickups(state);
        self.update_supply_pieces();
        self.update_fragments(state);
        self.warm_debris();
        self.prepare_houses(&state.covers);
        self.update_mines(state);
        // The camera is final here; the effects order smoke by this frame's eye.
        self.renderer.set_camera(self.rig.camera);
        self.effects
            .update(&mut self.renderer, state, alpha, dt, self.time);
        // A destroyed player keeps the view without aiming, in either camera, and
        // neither reticle fits the view while the camera flies between them.
        let rig = &self.rig;
        let viewer_alive = state.viewer().is_some_and(|viewer| viewer.alive);
        self.crosshair_visible = state.match_state.phase == MatchPhase::Playing
            && viewer_alive
            && (rig.in_first_person
                || (!self.hide_reticle && !rig.first_person.enabled && rig.seat_blend == 0.0));
        self.renderer
            .set_visible(self.reticle.instance, self.crosshair_visible);
        self.renderer
            .set_transform(self.reticle.instance, self.crosshair);
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
                velocity: DVec3::new(tank.velocity.x, 0.0, tank.velocity.z),
                aim: tank.aim,
            },
            None => ViewerPose::default(),
        }
    }

    fn update_camera(&mut self, state: &RenderState, alpha: f64, dt: f64, overview: bool) {
        let has_model = self.tanks.contains_key(&state.viewer_id);
        self.viewer_id = Some(state.viewer_id);
        self.rig
            .update(&Self::viewer_pose(state), alpha, dt, overview, has_model);
    }

    fn update_player_indicators(&mut self, state: &RenderState, dt: f64, overview: bool) {
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
        self.reticle_size = if in_first_person {
            FIRST_PERSON.reticle_scale
        } else {
            (self.rig.view_zoom / CAMERA.default_zoom).clamp(0.5, 1.6) * OVERHEAD_RETICLE_SCALE
        };
        let scale = if confirmed { 1.2 } else { 1.0 } * self.reticle_size;
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
        // The camera rig has just interpolated the viewer's pose (the overview, where
        // it follows nothing, hides the ring).
        let follow = self.rig.follow;
        let at = Vec3::new(follow.x as f32, 0.0, follow.z as f32);
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
            let model = &self.library.tanks[&(tank.kind, tank.team)];
            let bar = &self.library.bars[&tank.team];
            let view = self.tanks.get_mut(&tank.id).expect("tank view");
            let renderer = &mut self.renderer;
            let viewer = tank.id == state.viewer_id;
            let seated = viewer && seat_wanted;
            renderer.set_visible(view.instance, tank.alive);
            let bar_height = if viewer {
                PLAYER_BAR_HEIGHT
            } else {
                BAR_HEIGHT
            } as f32;
            // The camera flies through the player's bar on the way into the turret.
            view.bar_shown = tank.alive && !(viewer && seat_blend > 0.0);
            let meters =
                protection_meters(tank.alive, tank.shield, tank.shield_points, tank.protection);
            renderer.set_node_visible(view.bar, bar.shield, meters.shield_visible);
            renderer.set_node_transform(
                view.bar,
                bar.shield_fill.index,
                Some(
                    bar.shield_fill
                        .with_scale(Vec3::new(meters.shield_fill as f32, 1.0, 1.0)),
                ),
            );
            renderer.set_node_visible(view.bar, bar.spawn.index, meters.spawn_visible);
            renderer.set_node_transform(
                view.bar,
                bar.spawn.index,
                Some(bar.spawn.with_position(Vec3::new(
                    bar.spawn.position.x,
                    meters.spawn_y as f32,
                    bar.spawn.position.z,
                ))),
            );
            renderer.set_node_transform(
                view.bar,
                bar.spawn_fill.index,
                Some(
                    bar.spawn_fill
                        .with_scale(Vec3::new(meters.spawn_fill as f32, 1.0, 1.0)),
                ),
            );
            if !tank.alive {
                view.motion = None;
                view.trail = None;
                view.world = Mat4::ZERO;
                continue;
            }
            let position = tank.position;
            let aim = if seated { look_yaw } else { tank.aim };
            let motion = view.motion.get_or_insert_with(TankMotion::default);
            motion.update(
                &MotionInput {
                    vx: tank.velocity.x,
                    vz: tank.velocity.z,
                    heading: tank.heading,
                    aim,
                    time: state.elapsed,
                },
                if playing { dt } else { 0.0 },
            );
            // Render-only weight: physics, steering and the camera keep the true pose.
            let x = lerp(tank.previous.x, position.x, alpha) + motion.shove.x;
            let y = position.y - TANK_MODEL_DROP;
            let z = lerp(tank.previous.z, position.z, alpha) + motion.shove.z;
            let suspension = motion.suspension;
            let scale = vehicle(tank.kind).scale as f32;
            let world = Mat4::from_scale_rotation_translation(
                Vec3::splat(scale),
                Quat::IDENTITY,
                Vec3::new(x as f32, y as f32, z as f32),
            );
            view.world = world;
            renderer.set_transform(view.instance, world);
            renderer.set_tint(view.instance, motion.tint());
            // The turret rides the hull's tilted ring, then turns to its own aim;
            // the barrel inherits the tilt and keeps its recoil.
            let (hull, turret, barrel, track) =
                (&model.hull, &model.turret, &model.barrel, &model.track);
            // The engine trembles through every hull, each tank to its own beat.
            let speed = tank.velocity.x.hypot(tank.velocity.z) / CAMERA_FEEL.full_speed;
            let (shudder_pitch, shudder_roll) =
                engine_shudder(state.elapsed, f64::from(tank.id) * 0.618, speed);
            let hull_rotation = euler_yxz(
                (suspension.pitch.angle + shudder_pitch) as f32,
                tank.heading as f32,
                (suspension.roll.angle + shudder_roll) as f32,
            );
            let turret_rotation =
                hull_rotation * Quat::from_rotation_y((aim - tank.heading) as f32);
            let hull_pose = hull.with_rotation(hull_rotation);
            let turret_pose = turret.with_rotation(turret_rotation);
            // The gun slams back, holds a beat and runs out again, its muzzle
            // climbing with the kick.
            let recoil = motion.gun_recoil(tank.recoil);
            let barrel_pose = barrel.pose(
                Vec3::new(
                    barrel.position.x,
                    barrel.position.y,
                    barrel.position.z - (recoil * TANK_FEEL.recoil_travel) as f32,
                ),
                barrel.rotation * Quat::from_rotation_x(-(recoil * TANK_FEEL.recoil_climb) as f32),
                barrel.scale,
            );
            let track_pose = if tank.kind == VehicleKind::Humvee {
                // The wheels stay planted while the body rocks on its springs: the
                // wheel group takes back the hull's pitch and roll.
                track.with_rotation(
                    track.rotation
                        * euler_yxz(
                            (suspension.pitch.angle + shudder_pitch) as f32,
                            0.0,
                            (suspension.roll.angle + shudder_roll) as f32,
                        )
                        .inverse(),
                )
            } else {
                // The belts scroll in their shader (instance data x).
                renderer
                    .set_instance_data(view.instance, [motion.belt_scroll() as f32, 0.0, 0.0, 0.0]);
                track.with_position(Vec3::new(
                    track.position.x,
                    track.position.y,
                    motion.tread_scroll() as f32,
                ))
            };
            for (joint, pose) in [
                (hull.index, hull_pose),
                (turret.index, turret_pose),
                (barrel.index, barrel_pose),
                (track.index, track_pose),
            ] {
                view.pose[joint] = Some(pose);
                renderer.set_node_transform(view.instance, joint, Some(pose));
            }
            view.bar_anchor = Vec3::new(x as f32, bar_height, z as f32);
            let health = health_bar_state(tank.hp, tank.max_hp);
            let trail = view
                .trail
                .get_or_insert_with(|| BarTrail::new(health.ratio, state.elapsed))
                .update(health.ratio, state.elapsed);
            let trailing = trail > health.ratio + 0.002;
            renderer.set_node_visible(view.bar, bar.trail.index, trailing);
            if trailing {
                renderer.set_node_transform(
                    view.bar,
                    bar.trail.index,
                    Some(bar.trail.with_scale(Vec3::new(trail as f32, 1.0, 1.0))),
                );
            }
            let tone = health.tone as usize;
            for (index, fill) in bar.fills.iter().enumerate() {
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
            for (i, joint) in bar.ranks.iter().enumerate() {
                renderer.set_node_visible(view.bar, *joint, i < rank);
            }
        }
        // Tanks that left the roster (never in single player, but in rooms).
        self.live.clear();
        self.live.extend(state.tanks.iter().map(|tank| tank.id));
        for (_, view) in self.tanks.extract_if(|id, _| !self.live.contains(id)) {
            self.renderer.remove_instance(view.instance);
            self.renderer.remove_instance(view.bar);
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

    /// Stop the smoke of a cottage whose chimney has toppled or fallen, and start it
    /// again when the cottage stands again.
    fn update_chimney_smoke(&mut self, state: &RenderState) {
        let chimneys = model_catalog::standing_chimneys(&state.covers);
        if chimneys == self.chimneys {
            return;
        }
        self.chimneys = chimneys;
        let Some(theme) = self.theme else {
            return;
        };
        if let Some(view) = self.scenery.get_mut(theme.name())
            && let Some((model, _)) = view.smoke
            && let Some(smoke) = model_catalog::refill_smoke(&mut view.scenery, &state.covers)
        {
            let wisps = model_catalog::smoke_instances(&smoke);
            self.renderer.set_part_instances(model, 0, &wisps);
        }
    }

    /// Build a hit cover's new look a step at a time while this frame's rebuild time
    /// (from `start`) lasts, installing it once done; a newer look starts over. A step
    /// takes up to a few milliseconds on a slow device, so one that would not fit in
    /// what is left of the time waits for the next frame (`longest` is the longest
    /// step this frame has taken): the time bounds the frame rather than when its last
    /// step starts.
    fn build_cover_look(&mut self, cover: &RenderCover, stage: u32, start: f64, longest: &mut f64) {
        let key = cover_key(cover, stage);
        let index = match self
            .cover_looks
            .iter()
            .position(|look| look.cover == cover.id)
        {
            Some(index) if self.cover_looks[index].key == key => index,
            found => {
                let phase = if cover.kind == CoverKind::House {
                    LookPhase::Building(Box::new(HouseCoverBuild::new(
                        cover.x,
                        cover.z,
                        cover.w,
                        cover.d,
                        cover.h,
                        stage,
                        &cover.timber_hits,
                    )))
                } else {
                    LookPhase::Unbuilt
                };
                let look = CoverLook {
                    cover: cover.id,
                    key,
                    phase,
                };
                match found {
                    Some(index) => {
                        self.cover_looks[index] = look;
                        index
                    }
                    None => {
                        self.cover_looks.push(look);
                        self.cover_looks.len() - 1
                    }
                }
            }
        };
        loop {
            let before = now_ms();
            let phase = &mut self.cover_looks[index].phase;
            let installed = match phase {
                LookPhase::Building(build) => {
                    if let Some(root) = build.step() {
                        *phase = LookPhase::Built(root);
                    }
                    false
                }
                LookPhase::Built(root) => {
                    let model = model_catalog::built_cover_model(std::mem::take(root));
                    let pending = self.renderer.prepare_pending_model(&model.root);
                    *phase = LookPhase::Ready(model, Some(pending));
                    false
                }
                // A look another cover already shows needs no model of its own.
                LookPhase::Unbuilt
                    if !self
                        .cover_models
                        .contains_key(&Self::cover_model_key(cover, stage)) =>
                {
                    let model = model_catalog::cover_model(&Self::built_cover(cover), stage);
                    *phase = LookPhase::Ready(model, None);
                    false
                }
                LookPhase::Unbuilt | LookPhase::Ready(..) => {
                    let look = self.cover_looks.swap_remove(index);
                    match look.phase {
                        LookPhase::Ready(model, pending) => {
                            self.add_cover_with(cover, Some((model, pending)));
                        }
                        _ => self.add_cover(cover),
                    }
                    true
                }
            };
            let after = now_ms();
            *longest = longest.max(after - before);
            if installed || after - start + *longest >= COVER_REBUILD_MS {
                return;
            }
        }
    }

    fn update_covers(&mut self, state: &RenderState) {
        self.update_chimney_smoke(state);
        let mut shed = Vec::new();
        let start = now_ms();
        let mut rebuilt = false;
        let mut longest = 0.0_f64;
        for cover in &state.covers {
            let stump = cover.kind == CoverKind::Tree && !cover.alive;
            let stage = cover_damage_stage(cover.kind, cover.hp, cover.max_hp);
            let shown = self.covers.get(&cover.id);
            let rebuild = shown.is_none_or(|view| {
                cover.alive
                    && (view.stage != stage
                        || view.debris_seed != cover.debris_seed
                        || (matches!(
                            cover.kind,
                            CoverKind::Timber | CoverKind::Cargo | CoverKind::House
                        ) && view.hits != cover.timber_hits.len()))
            });
            // A blast that marks several crates, walls or cottages at once rebuilds them
            // over a few frames on a slow device: one step a frame at least, more while
            // this frame's rebuild time lasts. A cover keeps its last look meanwhile;
            // one without a view is always built. A hit cottage, crate or timber wall
            // builds its new look in steps (a cottage's model in several), uploading
            // it in a step of its own.
            let shown = shown.is_some();
            let wait = shown && rebuilt && now_ms() - start + longest >= COVER_REBUILD_MS;
            if rebuild && !wait {
                if shown
                    && matches!(
                        cover.kind,
                        CoverKind::House | CoverKind::Cargo | CoverKind::Timber
                    )
                {
                    self.build_cover_look(cover, stage, start, &mut longest);
                } else {
                    let before = now_ms();
                    self.add_cover(cover);
                    longest = longest.max(now_ms() - before);
                }
                rebuilt = true;
                self.busy = true;
            } else if !rebuild && !self.cover_looks.is_empty() {
                self.cover_looks.retain(|look| look.cover != cover.id);
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
                if let Some((axis, start)) = tree.shudder {
                    let [lean, decay, rate, settled] = TREE_SHUDDER;
                    let age = self.time - start;
                    let pose = (age < settled && cover.alive).then(|| {
                        let angle = lean * (-age / decay).exp() * (age * rate).sin();
                        let pivot = Vec3::Y * tree.pivot;
                        tree.crown_rest
                            * Mat4::from_translation(pivot)
                            * Mat4::from_axis_angle(axis, angle as f32)
                            * Mat4::from_translation(-pivot)
                    });
                    if pose.is_none() {
                        tree.shudder = None;
                    }
                    self.renderer
                        .set_node_transform(view.instance, tree.crown, pose);
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
            .and_then(|_| entry.source.find(tree_part::CROWN))
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
            sink: 0.0,
        });
    }

    fn update_branches(&mut self, dt: f64) {
        let dt32 = dt as f32;
        let renderer = &mut self.renderer;
        self.branches.retain_mut(|branch| {
            branch.life -= dt;
            if branch.life <= 0.0 {
                renderer.remove_model(branch.model);
                return false;
            }
            if !branch.landed {
                branch.velocity.y -= (BRANCH_GRAVITY * dt) as f32;
                branch.velocity *= (-BRANCH_DRAG * dt32).exp();
                branch.position += branch.velocity * dt32;
                let turn = branch.spin * dt32 * BRANCH_TUMBLE;
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
                    branch.sink = world_height(&branch.bounds, &world) + SINK_MARGIN;
                }
            }
            // Sinking out of sight instead of fading keeps the alpha-tested leaves
            // opaque (a blended copy draws them as a ghostly sheet).
            let cleanup = debris_cleanup_progress(branch.life) as f32;
            if branch.landed {
                branch.position.y = branch.resting_y - cleanup * branch.sink;
            }
            renderer.set_transform(
                branch.instance,
                Mat4::from_scale_rotation_translation(
                    branch.scale,
                    branch.rotation,
                    branch.position,
                ),
            );
            true
        });
    }

    fn update_pickups(&mut self, state: &RenderState) {
        // Supplies stand at a turret's eye height; first person sees through them.
        let opacity = if self.rig.in_first_person {
            FIRST_PERSON.pickup_opacity
        } else {
            1.0
        } as f32;
        // Pickups that left the arena (rules and fixtures may replace them).
        self.live.clear();
        self.live
            .extend(state.pickups.iter().map(|pickup| pickup.id));
        for (_, view) in self.pickups.extract_if(|id, _| !self.live.contains(id)) {
            self.renderer.remove_instance(view.base);
            self.renderer.remove_instance(view.gem);
            self.renderer.remove_instance(view.marker);
        }
        for supply in &state.pickups {
            if !self.pickups.contains_key(&supply.id) {
                self.add_pickup(supply.id, supply.kind, supply.x, supply.z);
            }
            // A hull rolling onto the drop presses its load down and crushes the
            // pallet under it; both spring back once it has passed.
            let over = self.hull_press(supply.x, supply.z);
            let press = if supply.available { over } else { 0.0 };
            let crush = (over * PALLET_CRUSH) as f32;
            let view = self.pickups.get_mut(&supply.id).expect("pickup view");
            let renderer = &mut self.renderer;
            let stocked = supply.available;
            renderer.set_opacity(view.gem, opacity);
            if view.shown != Some(stocked) {
                // A supply seen coming back is dropped onto its fresh pallet.
                if stocked && view.shown == Some(false) {
                    view.restocked = state.elapsed;
                    self.effects
                        .systems
                        .pickups
                        .land(supply.x, supply.z, RESTOCK_FALL);
                }
                view.shown = Some(stocked);
                renderer.set_visible(view.gem, stocked);
                renderer.set_visible(view.marker, !stocked);
            }
            view.progress = if stocked {
                1.0
            } else {
                let fallback = if supply.kind == PickupKind::Laser {
                    LASER_DEFENSE.initial_delay
                } else {
                    AMMO_RESPAWN_SECONDS
                };
                let duration = if supply.cooldown_duration != 0.0 {
                    supply.cooldown_duration
                } else {
                    fallback
                };
                (1.0 - supply.cooldown / duration).clamp(0.0, 1.0) as f32
            };
            if (crush - view.crush).abs() > 0.01 || (crush == 0.0) != (view.crush == 0.0) {
                view.crush = crush;
                let flatten = Mat4::from_scale(Vec3::new(1.0, 1.0 - crush, 1.0));
                renderer.set_transform(view.base, view.ground * flatten);
                // The marker board rides down with the crushed deck.
                let lowered = Mat4::from_translation(Vec3::Y * -crush * PALLET_HEIGHT as f32);
                renderer.set_transform(view.marker, view.ground * lowered * view.marker_rest);
            }
            if !stocked {
                continue;
            }
            // Dropped in from above, landing with a squash; or pressed down under
            // a hull driving onto it.
            let since = state.elapsed - view.restocked;
            let fall = since / RESTOCK_FALL;
            let (drop, squash) = if fall < 1.0 {
                (RESTOCK_DROP * (1.0 - fall * fall), 0.0)
            } else {
                let landed = since - RESTOCK_FALL;
                let squash = RESTOCK_SQUASH
                    * (-landed * 12.0).exp()
                    * (landed * std::f64::consts::TAU * 2.5).cos();
                (0.0, if landed < 0.6 { squash } else { 0.0 })
            };
            let height = (1.0 - squash) * (1.0 - press * (1.0 - PRESS_FLOOR));
            let width = 1.0 + squash * 0.5 + press * PRESS_BULGE;
            // The load rides down with the crushed deck under it.
            let drop = drop - f64::from(crush) * PALLET_HEIGHT * SUPPLY_SCALE;
            if drop != 0.0 || height != 1.0 || width != 1.0 {
                let pose = Mat4::from_translation(Vec3::Y * drop as f32)
                    * view.rest
                    * Mat4::from_scale(Vec3::new(width as f32, height as f32, width as f32));
                renderer.set_transform(view.gem, pose);
                view.posed = true;
            } else if view.posed {
                renderer.set_transform(view.gem, view.rest);
                view.posed = false;
            }
        }
    }

    /// How far a hull driving onto the stocked crate at (x, z) presses it down
    /// (0..1): by how deep the crate's centre lies inside the footprint of the
    /// nearest live tank's hull, as drawn.
    fn hull_press(&self, x: f64, z: f64) -> f64 {
        self.hull_over(x, z).0
    }

    /// [`hull_press`](Self::hull_press) at (x, z), and the ground point under the
    /// centre of the hull pressing it hardest (the point itself when none does).
    fn hull_over(&self, x: f64, z: f64) -> (f64, Vec3) {
        let at = Vec3::new(x as f32, 0.0, z as f32);
        let mut press = 0.0_f64;
        let mut center = at;
        for view in self.tanks.values() {
            if view.world == Mat4::ZERO
                || view.world.w_axis.truncate().distance(at) > TANK_PRESS_RANGE
            {
                continue;
            }
            let hull = tank_hull(view.kind);
            let local = view.world.inverse().transform_point3(at);
            let outside = (f64::from(local.x) - hull.center.x).abs() - hull.size.x / 2.0;
            let outside =
                outside.max((f64::from(local.z) - hull.center.z).abs() - hull.size.z / 2.0);
            let this = 1.0 - smoothstep(outside, -PRESS_DEPTH, PRESS_REACH);
            if this > press {
                press = this;
                center = view.world.transform_point3(Vec3::new(
                    hull.center.x as f32,
                    0.0,
                    hull.center.z as f32,
                ));
                center.y = 0.0;
            }
        }
        (press, center)
    }

    /// A tank drove over a supply and took it: the load bursts open under the hull,
    /// its lid board and splinters fly out and two pieces of its stock are tossed
    /// onto the tank's rear deck, the crushed grenade vents a puff of its smoke and
    /// the hull dips as it rides over; the bare pallet stays behind. The event names
    /// the kind only by its colour, which is unique per kind, the pallet by where it
    /// stands and the tank by its id.
    fn collect_supply(&mut self, event: &SimEvent) {
        let Some(kind) = PickupKind::ALL
            .into_iter()
            .find(|&kind| Some(pickup(kind).color) == event.color)
        else {
            return;
        };
        let at = Vec3::new(event.x as f32, 0.0, event.z as f32);
        let hull = event
            .id
            .and_then(|id| self.tanks.get(&id))
            .filter(|view| view.world != Mat4::ZERO)
            .map(|view| view.world.w_axis.truncate());
        // The load bursts out from under the hull on the side away from it: the
        // crushed grenade vents there once and the ground's dust kicks up round it.
        let away = hull
            .map(|hull| Vec3::new(at.x - hull.x, 0.0, at.z - hull.z))
            .filter(|away| away.length() > 1e-3)
            .map_or(Vec3::X, Vec3::normalize);
        let burst = event.id.map_or(at, |id| self.beside_hull(id, at));
        self.effects.systems.pickups.vent(
            f64::from(burst.x),
            f64::from(burst.z),
            [away.x, away.z],
            pickup_signal(kind),
        );
        let from = burst.with_y((PALLET_HEIGHT * SUPPLY_SCALE + PIECE_RISE) as f32);
        self.throw_loose(kind, from, away);
        if let (Some(id), Some(hull)) = (event.id, hull) {
            self.stow_pieces(kind, from, id);
            // The hull dips as it rides over the drop.
            if let Some(motion) = self
                .tanks
                .get_mut(&id)
                .and_then(|view| view.motion.as_mut())
            {
                let (dx, dz) = (f64::from(hull.x) - event.x, f64::from(hull.z) - event.z);
                let length = dx.hypot(dz).max(1e-3);
                motion
                    .pending_hit
                    .get_or_insert((dx / length, dz / length, COLLECT_ROCK));
            }
        }
    }

    /// Where a load at `at` under or beside tank `id` bursts out: the point where
    /// the line from the hull's middle out over the load leaves its footprint (and
    /// a margin), or the load itself when it lies further out, so nothing thrown
    /// passes through the hull.
    fn beside_hull(&self, id: u32, at: Vec3) -> Vec3 {
        let Some(view) = self.tanks.get(&id).filter(|view| view.world != Mat4::ZERO) else {
            return at;
        };
        let hull = tank_hull(view.kind);
        let Some(model) = self.library.tanks.get(&(view.kind, view.team)) else {
            return at;
        };
        // The hull's frame on the ground: its heading without its rocking.
        let nodes = self.renderer.model_nodes(model.model);
        let frame = joint_world(nodes, &view.pose, view.world, model.hull.index);
        let (_, rotation, _) = frame.to_scale_rotation_translation();
        let (heading, _, _) = rotation.to_euler(glam::EulerRot::YXZ);
        let (scale, _, origin) = view.world.to_scale_rotation_translation();
        let ground = Mat4::from_scale_rotation_translation(
            scale,
            Quat::from_rotation_y(heading),
            Vec3::new(origin.x, 0.0, origin.z),
        );
        let inverse = ground.inverse();
        let center = Vec3::new(hull.center.x as f32, 0.0, hull.center.z as f32);
        let local = (inverse.transform_point3(at) - center) * Vec3::new(1.0, 0.0, 1.0);
        let half = Vec3::new(
            (hull.size.x / 2.0 + PIECE_MARGIN) as f32,
            0.0,
            (hull.size.z / 2.0 + PIECE_MARGIN) as f32,
        );
        let direction = local.try_normalize().unwrap_or(Vec3::X);
        let exit = (half.x / direction.x.abs().max(1e-4)).min(half.z / direction.z.abs().max(1e-4));
        let edge = ground.transform_point3(center + direction * exit.max(local.length()));
        Vec3::new(edge.x, 0.0, edge.z)
    }

    /// A free piece of the pool for an instance of `model`: an idle one of that
    /// model, a new one while the pool has room, an idle one of another model
    /// remade for this one, or else the oldest piece of this model still out.
    fn supply_piece(&mut self, model: ModelId) -> Option<usize> {
        let pieces = &mut self.supply_pieces;
        if let Some(slot) = pieces.iter().position(|p| !p.active && p.model == model) {
            return Some(slot);
        }
        let fresh = |renderer: &mut Renderer| {
            let instance = renderer.add_instance(model, Mat4::IDENTITY, Lifetime::Round)?;
            Some(SupplyPiece {
                instance,
                model,
                start: 0.0,
                from: Vec3::ZERO,
                flight: PieceFlight::Stow {
                    tank: 0,
                    slot: 0,
                    half: 0.0,
                    turn: 0.0,
                },
                active: false,
            })
        };
        if pieces.len() < MAX_SUPPLY_PIECES {
            pieces.push(fresh(&mut self.renderer)?);
            return Some(pieces.len() - 1);
        }
        if let Some(slot) = pieces.iter().position(|p| !p.active) {
            self.renderer.remove_instance(pieces[slot].instance);
            pieces[slot] = fresh(&mut self.renderer)?;
            return Some(slot);
        }
        pieces
            .iter()
            .enumerate()
            .filter(|(_, p)| p.model == model)
            .min_by(|a, b| a.1.start.total_cmp(&b.1.start))
            .map(|(slot, _)| slot)
    }

    /// Send a piece of `model` painted for `kind` off from `from` now plus `delay`.
    fn throw_piece(
        &mut self,
        model: ModelId,
        kind: PickupKind,
        from: Vec3,
        delay: f64,
        flight: PieceFlight,
    ) {
        let Some(slot) = self.supply_piece(model) else {
            return;
        };
        let [u, v] = pickup_tile_offset(kind);
        let piece = &mut self.supply_pieces[slot];
        piece.start = self.time + delay;
        piece.from = from;
        piece.flight = flight;
        piece.active = true;
        self.renderer
            .set_instance_data(piece.instance, [u, v, 1.0, 0.37 * slot as f32]);
        self.renderer.set_opacity(piece.instance, 1.0);
        self.renderer.set_visible(piece.instance, false);
    }

    /// The broken load's lid board, popped off out and up away from the hull, and
    /// two splinters of it thrown either side.
    fn throw_loose(&mut self, kind: PickupKind, from: Vec3, away: Vec3) {
        let board = &self.library.pickup_pad(&mut self.renderer).board;
        let (model, half) = (board.model, board.bounds.size().y as f32 / 2.0);
        let scale = (SUPPLY_SCALE * STOW_SCALE) as f32;
        let throws = [
            (0.0, LOOSE_SPEED, Vec3::splat(scale), 1.0_f32, 2.0_f32),
            (
                -SPLINTER_BEARING,
                SPLINTER_SPEED,
                Vec3::from(SPLINTER_SCALE) * scale,
                -2.6,
                1.0,
            ),
            (
                SPLINTER_BEARING * 0.8,
                SPLINTER_SPEED,
                Vec3::from(SPLINTER_SCALE) * scale,
                3.1,
                3.0,
            ),
        ];
        for (bearing, [out, up], size, spin, flips) in throws {
            let direction = Quat::from_rotation_y(bearing as f32) * away;
            let velocity = direction * out as f32 + Vec3::Y * up as f32;
            self.throw_piece(
                model,
                kind,
                from,
                0.0,
                PieceFlight::Loose {
                    velocity,
                    scale: size,
                    half: half * size.y,
                    spin,
                    flips,
                },
            );
        }
    }

    /// Toss `STOWED` pieces of `kind`'s stock from `from` onto tank `id`, one to
    /// each stowage spot.
    fn stow_pieces(&mut self, kind: PickupKind, from: Vec3, id: u32) {
        let parcel = &self.library.pickup_pad(&mut self.renderer).parcels[kind as usize];
        let (model, half) = (parcel.model, parcel.bounds.size().y as f32 / 2.0);
        let half = half * (SUPPLY_SCALE * STOW_SCALE) as f32;
        for index in 0..STOWED {
            let turn = [0.08, -0.12][index % 2];
            self.throw_piece(
                model,
                kind,
                from,
                STOW_DELAY + STOW_STAGGER * index as f64,
                PieceFlight::Stow {
                    tank: id,
                    slot: index,
                    half,
                    turn,
                },
            );
        }
    }

    /// Move every piece out one frame: stowed pieces arcing onto their tank's rear
    /// deck, wherever it has driven, and riding there; loose ones flying, landing,
    /// hopping once and lying; each fading at the end. A stowed piece whose tank is
    /// gone just drops from view.
    fn update_supply_pieces(&mut self) {
        let stow_scale = Vec3::splat((SUPPLY_SCALE * STOW_SCALE) as f32);
        for index in 0..self.supply_pieces.len() {
            let piece = &self.supply_pieces[index];
            if !piece.active {
                continue;
            }
            let age = self.time - piece.start;
            if age < 0.0 {
                continue;
            }
            let pose = match piece.flight {
                PieceFlight::Stow {
                    tank,
                    slot,
                    half,
                    turn,
                } => self
                    .stow_spot(tank, slot)
                    .map(|spot| stowed_pose(piece.from, spot, half, turn, age, stow_scale)),
                PieceFlight::Loose {
                    velocity,
                    scale,
                    half,
                    spin,
                    flips,
                } => Some(loose_pose(
                    piece.from, velocity, scale, half, spin, flips, age,
                )),
            };
            let span = match piece.flight {
                PieceFlight::Stow { .. } => STOW_FLIGHT + STOW_HOLD,
                PieceFlight::Loose { .. } => LOOSE_HOLD,
            };
            let instance = piece.instance;
            let Some(pose) = pose.filter(|_| age < span + PIECE_FADE) else {
                self.supply_pieces[index].active = false;
                self.renderer.set_visible(instance, false);
                continue;
            };
            let fade = 1.0 - smoothstep(age, span, span + PIECE_FADE);
            self.renderer.set_transform(instance, pose);
            self.renderer.set_opacity(instance, fade as f32);
            self.renderer.set_visible(instance, true);
        }
    }

    /// Stowage spot `slot` on tank `id` as drawn: where it is and the turn of the
    /// turret (or hull) it rides on, rocking and all.
    fn stow_spot(&self, id: u32, slot: usize) -> Option<(Vec3, Quat)> {
        let view = self
            .tanks
            .get(&id)
            .filter(|view| view.world != Mat4::ZERO)?;
        let model = self.library.tanks.get(&(view.kind, view.team))?;
        let nodes = self.renderer.model_nodes(model.model);
        let joint = if model.stow_on_turret {
            model.turret.index
        } else {
            model.hull.index
        };
        let frame = joint_world(nodes, &view.pose, view.world, joint);
        let (_, rotation, _) = frame.to_scale_rotation_translation();
        Some((frame.transform_point3(model.stow[slot]), rotation))
    }

    fn update_fragments(&mut self, state: &RenderState) {
        self.live.clear();
        self.live.extend(state.fragments.iter().map(|f| f.id));
        for (_, view) in self.fragments.extract_if(|id, _| !self.live.contains(id)) {
            self.renderer.remove_instance(view.instance);
            if let FragmentLook::Owned(model, _) = view.look {
                self.renderer.remove_model(model);
            }
        }
        for (model, instance) in self.settled.retain(&self.live) {
            self.renderer.remove_instance(instance);
            self.renderer.remove_model(model);
        }
        let renderer = &mut self.renderer;
        release_spent_houses(&mut self.house_pieces, &state.fragments, |(model, _)| {
            renderer.remove_model(model);
        });
        let standing = |house: u32| state.covers.iter().any(|c| c.id == house && c.alive);
        for (model, instance) in self.settled.rebuilt(standing) {
            self.renderer.remove_instance(instance);
            self.renderer.remove_model(model);
        }
        // Pieces drawn so far this frame, per shape.
        let mut pieces = [0_usize; DEBRIS_SHAPES.len()];
        for fragment in &state.fragments {
            if !self.fragments.contains_key(&fragment.id) {
                match self.fragment_view(fragment, &state.covers) {
                    Some(view) => {
                        // A debris model of its own (or a shared look not built ahead).
                        self.busy |= matches!(
                            view.look,
                            FragmentLook::Owned(..) | FragmentLook::Shared(..)
                        );
                        self.fragments.insert(fragment.id, view);
                    }
                    None => continue,
                }
            }
            let cleanup = debris_cleanup_progress(fragment.life) as f32;
            let view = self.fragments.get_mut(&fragment.id).expect("fragment view");
            let rotation = quat(fragment.rotation);
            let mut position = vec3(fragment.position);
            view.pose = (position, rotation);
            match view.look {
                FragmentLook::Piece => {
                    let shape = fragment.shape.unwrap_or(FragmentShape::Shard);
                    pieces[shape as usize] += 1;
                    let count = pieces[shape as usize];
                    // The piece pool is sized for the largest debris budget.
                    self.renderer
                        .set_visible(view.instance, count <= FRAGMENT_CAPACITY);
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
                        .set_tint(view.instance, hex_to_linear(fragment.color));
                }
                FragmentLook::Wreck(..) | FragmentLook::Owned(..) | FragmentLook::Shared(..) => {
                    let (scale, bounds) = match &view.look {
                        FragmentLook::Wreck(kind, team, part) => (
                            vehicle(*kind).scale as f32,
                            self.library.wrecks[&(*kind, *team, *part)].bounds,
                        ),
                        FragmentLook::Owned(_, bounds) | FragmentLook::Shared(bounds) => {
                            (1.0, *bounds)
                        }
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
                        // How upright the piece lies, so embers glow only in a
                        // hull resting on its tracks, never out of an upturned belly.
                        let upright = (rotation * Vec3::Y).y;
                        self.renderer
                            .set_instance_data(view.instance, [brightness, 1.0, upright, 0.0]);
                    }
                }
            }
            if view.fades {
                self.renderer.set_opacity(view.instance, 1.0 - cleanup);
            }
            if self.settled.tracks(fragment.id) {
                self.settled
                    .observe(fragment.id, view.pose, cleanup > 0.0, self.time);
            }
        }
        self.settle_houses();
    }

    /// Bake resting cottage shards into merged chunks, one a frame, and hide their
    /// own views; show again the ones whose chunk broke up. The merges are one-off
    /// models, merged straight into the upload layout and not kept once uploaded.
    fn settle_houses(&mut self) {
        let plan = self.settled.plan(self.time);
        for (model, instance) in plan.freed {
            self.renderer.remove_instance(instance);
            self.renderer.remove_model(model);
        }
        if let Some((chunk, parts)) = plan.build {
            let model = self.renderer.add_merged_model(parts, Lifetime::Round);
            if let Some(instance) =
                self.renderer
                    .add_instance(model, Mat4::IDENTITY, Lifetime::Round)
            {
                self.settled.built(chunk, (model, instance));
            }
        }
        if let Some((house, parts)) = self.settled.remains() {
            let model = self.renderer.add_merged_model(parts, Lifetime::Round);
            if let Some(instance) =
                self.renderer
                    .add_instance(model, Mat4::IDENTITY, Lifetime::Round)
            {
                self.settled.remains_built(house, (model, instance));
            }
        }
        for (&id, view) in &self.fragments {
            if self.settled.tracks(id) {
                self.renderer
                    .set_visible(view.instance, !self.settled.merged(id));
            }
        }
    }

    /// A physical debris view: an instanced piece, a wreck part, a timber member,
    /// a watchtower or cottage piece, a section of a crate or a falling crown.
    fn fragment_view(
        &mut self,
        fragment: &RenderFragment,
        covers: &[RenderCover],
    ) -> Option<FragmentView> {
        let (model, look) = if let Some(part) = &fragment.timber_part {
            owned_look(&mut self.renderer, &timber_debris_model(part), true)
        } else if let (Some(piece), Some(house)) = (fragment.house_piece, fragment.house_id) {
            let Prepared { node, model, .. } = self.house_piece(piece, house, covers)?;
            let look = match model {
                Some((model, bounds)) => (model, FragmentLook::Owned(model, bounds)),
                None => owned_look(&mut self.renderer, &node, true),
            };
            self.settled.track(fragment.id, house, node, self.time);
            look
        } else if let Some(piece) = fragment.tower_piece {
            let piece = self.shared_debris(SharedDebris::Tower(piece, fragment.color));
            (piece.model, FragmentLook::Shared(piece.bounds))
        } else if let (Some(piece), Some(size)) = (fragment.crate_piece, fragment.dimensions) {
            let size = DVec3::new(size.x, size.y, size.z);
            let look = fragment.id % CRATE_SECTION_LOOKS;
            let key = section_key(piece, size, fragment.color, look);
            let section = self.shared_debris(SharedDebris::Section(key));
            (section.model, FragmentLook::Shared(section.bounds))
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
            // Only a tree's model has the crown.
            let crown = crown_fragment(
                &entry.source,
                tree_part::CROWN,
                fragment.tree_center_y.unwrap_or(0.0),
                &shed,
            )?;
            owned_look(&mut self.renderer, &crown, false)
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
            fades: fragment.tree_cover_id.is_none(),
            timber: fragment.timber_part.clone(),
            pose: (vec3(fragment.position), quat(fragment.rotation)),
        })
    }

    /// A collapsing cottage's piece, taking the cottage's whole set when its first
    /// piece appears: made ahead (models and all) if it was the one being prepared,
    /// otherwise cut now.
    fn house_piece(
        &mut self,
        piece: HousePiece,
        house: u32,
        covers: &[RenderCover],
    ) -> Option<Prepared<(ModelId, Aabb)>> {
        let pieces = match self.house_pieces.entry(house) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let cover = covers.iter().find(|cover| cover.id == house)?;
                let pieces = self.house_prep.take(cover).unwrap_or_else(|| {
                    house_pieces(
                        cover.x,
                        cover.z,
                        cover.w,
                        cover.d,
                        cover.h,
                        &cover.timber_hits,
                    )
                    .into_iter()
                    .map(|(piece, node)| Prepared {
                        piece,
                        node,
                        model: None,
                    })
                    .collect()
                });
                entry.insert(pieces)
            }
        };
        let index = pieces.iter().position(|prepared| prepared.piece == piece)?;
        let prepared = pieces.swap_remove(index);
        if pieces.is_empty() {
            self.house_pieces.remove(&house);
        }
        Some(prepared)
    }

    /// Work on the next cottage collapse for this frame's share of time: the most
    /// damaged standing cottage's pieces, cut and uploaded ahead of its fall.
    fn prepare_houses(&mut self, covers: &[RenderCover]) {
        for (model, _) in self.house_prep.aim(covers, self.time) {
            self.renderer.remove_model(model);
        }
        // The cut is ahead of the collapse: a frame that rebuilt a look or built debris
        // leaves it for the next.
        if self.busy {
            return;
        }
        let (start, time) = (now_ms(), self.time);
        // A step takes up to a few milliseconds on a slow device, so one that would not
        // fit in what is left of the budget waits for the next frame (the first always
        // runs): the budget bounds the frame rather than when the last step starts.
        let mut longest_step = 0.0_f64;
        for _ in 0..PREPARE_STEPS {
            let renderer = &mut self.renderer;
            let before = now_ms();
            // Its pieces are debris, which fades as it is cleared away.
            let more = self.house_prep.step(time, |node| {
                let bounds = node_bounds(node, DMat4::IDENTITY);
                (renderer.add_fading_model(node, Lifetime::Round), bounds)
            });
            let after = now_ms();
            longest_step = longest_step.max(after - before);
            if !more || after - start + longest_step >= PREPARE_BUDGET_MS {
                break;
            }
        }
    }

    fn update_mines(&mut self, state: &RenderState) {
        self.live.clear();
        self.live.extend(state.mines.iter().map(|mine| mine.id));
        for (_, view) in self.mines.extract_if(|id, _| !self.live.contains(id)) {
            self.renderer.remove_instance(view.instance);
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
    cover: &RenderCover,
) -> TreeView {
    let shape = TreeShape {
        x: cover.x,
        z: cover.z,
        w: cover.w,
        d: cover.d,
        h: cover.h,
    };
    let find = |name: &str| {
        range
            .clone()
            .find(|&index| nodes[index].name == name)
            .unwrap_or_else(|| panic!("tree joint {name}"))
    };
    let crown = find(tree_part::CROWN);
    let joints = range
        .clone()
        .filter(|&index| TreeParts::is_branch(&nodes[index].name));
    TreeView {
        crown,
        cut: find(tree_part::CUT_SURFACE),
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
        crown_rest: nodes[crown].rest,
        pivot: tree_proportions(shape.x, shape.z, shape.w, shape.d, shape.h).stump_height as f32,
        leaves: LeafCrown {
            x: cover.x,
            z: cover.z,
            foliage: tree_foliage(&shape),
        },
        shudder: None,
    }
}

/// A debris look with a model of its own, built for this round: one that fades as it
/// is cleared away, unless `fades` is false (a felled crown only sinks).
fn owned_look(renderer: &mut Renderer, node: &Node, fades: bool) -> (ModelId, FragmentLook) {
    let bounds = node_bounds(node, DMat4::IDENTITY);
    let model = if fades {
        renderer.add_fading_model(node, Lifetime::Round)
    } else {
        renderer.add_model(node, Lifetime::Round)
    };
    (model, FragmentLook::Owned(model, bounds))
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
        exposure: 1.0,
        // Every theme reflects its sky at full strength (see `environment_radiance`).
        reflections: 1.0,
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
        let blend = smoothstep(progress, 0.0, 1.0);
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
