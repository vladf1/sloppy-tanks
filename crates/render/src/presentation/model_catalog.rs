//! The adapter between presentation and the cover, tree and scenery model
//! builders in `sloppy_core::models`.
//!
//! - [`cover_model`] ← `models::cover_model(cover, Full, stage)`, batched like
//!   `physicalCoverModel` in `presentation.ts`; trees list their shedding boughs
//!   ([`TreeParts`]).
//! - [`scenery`] ← `models::build_scenery(theme)`, split into what bakes static,
//!   the parts [`Scenery::update`] animates (movers), the water surface the
//!   renderer draws with its planar reflection, and the chimney smoke that
//!   `VillageScenery::set_covers` refills each round.

use std::collections::HashMap;

use glam::{DMat4, DVec3};
use sloppy_core::geometry::Mesh;
use sloppy_core::models::{
    self as core_models, CHIMNEY_SMOKE_NODE, CoverShape, Scenery, SmokeCover, TreeDetail,
    WATERWHEEL, batch, branch_drop_stage, build_scenery, cover_damage_stage, tree_part,
};
use sloppy_core::scene::{Effect, Node};
use sloppy_core::sim::CoverKind;
use sloppy_core::sim::quarry_barrier_shapes::dragon_tooth_variant;
use sloppy_core::sim::quarry_rock_shape::quarry_rock_variant;
use sloppy_core::sim::render_state::RenderCover;

use super::theme::Theme;

/// A cover's drawable model.
///
/// Conventions (as `coverModel` in `cover-model.ts`):
/// - `root` is placed like the TypeScript group: its transform puts the model at
///   the cover's footprint (`x, 0, z`) with any rotation/scale the kind uses;
///   geometry stands on y = 0 in the root's frame.
/// - Presentation poses movable covers (drums, teeth, hedgehogs) by their body:
///   it drops the root's children by `h / (2 * root.scale.y)` and replaces the
///   root translation/rotation with the body pose, keeping the root scale.
/// - Named nodes are joints presentation may hide (see [`TreeParts`]).
pub struct CoverModel {
    pub root: Node,
    pub tree: Option<TreeParts>,
}

/// The parts of a tree that damage and felling animate (`tree-models.ts`
/// `userData.crown`, `cutSurface`, `branches`). The crown is the trunk-and-crown
/// joint [`tree_part::CROWN`]: hidden when felled, its subtree is also the falling
/// crown fragment, placed `-tree_center_y` below the body. The stump's cut face,
/// [`tree_part::CUT_SURFACE`], shows only once felled.
pub struct TreeParts {
    /// Boughs shed with damage, in crown order, each visible while `drop_stage`
    /// exceeds the branch damage stage (0 healthy, 1 hurt, 2 at 35% health).
    pub branches: Vec<TreeBranch>,
}

pub struct TreeBranch {
    /// Its index among the crown's children (bough names repeat per stage).
    pub crown_child: usize,
    pub drop_stage: u32,
}

impl TreeParts {
    /// Whether a model joint is a shedding bough (joints of one tree appear in
    /// crown order).
    pub fn is_branch(name: &str) -> bool {
        name == tree_part::BRANCH_STAGE_1 || name == tree_part::BRANCH_STAGE_2
    }
}

/// What distinguishes covers that look alike: every builder input except where
/// the model stands. Builders seeded by position contribute their seed or
/// variant, so identical drums, containers, hedgehogs and walls share one model.
/// The order a round's covers are added in, by index: first the static covers with a
/// look of their own, which join one combined model (a joint each) so their shadows
/// merge into a couple of draws, then the rest, each its own instance (covers sharing a
/// look stay instanced, movable ones move). A lone static cover with a look of its own
/// is not worth a combined model: it leads the rest instead.
pub fn cover_order(covers: &[RenderCover]) -> (Vec<usize>, Vec<usize>) {
    let stage = |cover: &RenderCover| cover_damage_stage(cover.kind, cover.hp, cover.max_hp);
    let mut looks: HashMap<String, usize> = HashMap::new();
    for cover in covers.iter().filter(|cover| cover.motion.is_none()) {
        *looks.entry(cover_key(cover, stage(cover))).or_default() += 1;
    }
    let (mut combined, mut own): (Vec<usize>, Vec<usize>) = (0..covers.len()).partition(|&index| {
        let cover = &covers[index];
        cover.motion.is_none() && looks[&cover_key(cover, stage(cover))] == 1
    });
    if combined.len() < 2 {
        combined.append(&mut own);
        own = std::mem::take(&mut combined);
    }
    (combined, own)
}

pub fn cover_key(cover: &RenderCover, stage: u32) -> String {
    let placement = match cover.kind {
        CoverKind::Drum
        | CoverKind::Hedgehog
        | CoverKind::Container
        | CoverKind::Concrete
        | CoverKind::Boundary
        | CoverKind::Tower => String::new(),
        CoverKind::Rock => format!("v{}", quarry_rock_variant(cover.x, cover.z)),
        CoverKind::Teeth => format!("v{}", dragon_tooth_variant(cover.x, cover.z)),
        CoverKind::Rubble if cover.debris_seed.is_some() => {
            format!("s{}", cover.debris_seed.unwrap_or(0.0))
        }
        _ => format!("@{}/{}", cover.x, cover.z),
    };
    let hits: Vec<String> = cover
        .timber_hits
        .iter()
        .map(|hit| format!("{hit:?}"))
        .collect();
    format!(
        "{:?}/{}/{}/{}/{:x}/{stage}/{placement}/{:?}/{}",
        cover.kind,
        cover.w,
        cover.h,
        cover.d,
        cover.color,
        cover.timber_join,
        hits.join(";"),
    )
}

/// `physicalCoverModel`'s model: the cover's full-detail model, batched.
pub fn cover_model(cover: &RenderCover, stage: u32) -> CoverModel {
    let shape = CoverShape::from(cover);
    let mut root = core_models::cover_model(&shape, TreeDetail::Full, stage);
    batch(&mut root);
    let tree = (cover.kind == CoverKind::Tree).then(|| tree_parts(&root));
    CoverModel { root, tree }
}

/// [`cover_model`] for a cottage whose core model was built ahead, in steps
/// (`models::HouseCoverBuild`): batched the same way.
pub fn built_cover_model(mut root: Node) -> CoverModel {
    batch(&mut root);
    CoverModel { root, tree: None }
}

fn tree_parts(root: &Node) -> TreeParts {
    let branches = root
        .find(tree_part::CROWN)
        .map(|crown| {
            crown
                .children
                .iter()
                .enumerate()
                .filter_map(|(crown_child, bough)| {
                    branch_drop_stage(bough).map(|drop_stage| TreeBranch {
                        crown_child,
                        drop_stage,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    TreeParts { branches }
}

/// A themed scenery split for drawing.
pub struct SceneryModel {
    /// The live core scenery: [`Scenery::update`] animates it each frame and the
    /// village refills its chimney smoke with `set_covers` on reset. Its drawables
    /// are released ([`Scenery::release_drawables`]): `root` and `movers` draw it.
    pub scenery: Scenery,
    /// Everything that never moves, baked into static batches (world transforms
    /// as authored).
    pub root: Node,
    /// Subtrees [`Scenery::update`] moves or blinks.
    pub movers: Vec<SceneryMover>,
    pub water: Option<SceneryWater>,
    /// The village chimney smoke: an instanced wisp quad (see
    /// [`smoke_instances`]).
    pub smoke: Option<Node>,
}

/// An animated scenery subtree, found by its child-index path from the root.
pub struct SceneryMover {
    /// The subtree, with its own transform left to the instance.
    pub root: Node,
    path: Vec<usize>,
    /// World transform of its (static) parent.
    parent_world: DMat4,
}

impl SceneryMover {
    /// The subtree's world transform and visibility in the animated scenery.
    pub fn pose(&self, scenery: &Scenery) -> (DMat4, bool) {
        let mut node = scenery.root();
        let mut visible = node.visible;
        for &index in &self.path {
            node = &node.children[index];
            visible &= node.visible;
        }
        (self.parent_world * node.local_matrix(), visible)
    }
}

/// The planar-reflection water surface in world XZ at y = 0, drawn at the
/// material's mirror height.
pub enum SceneryWater {
    /// `HarborWater`: the basin around the pier.
    Harbor(Mesh),
    /// `VillageLandscape`'s creek.
    Creek(Mesh),
}

/// A node's child-index path and its parent's world transform.
fn find_path(root: &Node, matches: &dyn Fn(&Node) -> bool) -> Option<(Vec<usize>, DMat4)> {
    fn visit(
        node: &Node,
        world: DMat4,
        matches: &dyn Fn(&Node) -> bool,
        path: &mut Vec<usize>,
    ) -> Option<DMat4> {
        for (index, child) in node.children.iter().enumerate() {
            path.push(index);
            if matches(child) {
                return Some(world);
            }
            if let Some(found) = visit(child, world * child.local_matrix(), matches, path) {
                return Some(found);
            }
            path.pop();
        }
        None
    }
    let mut path = Vec::new();
    let world = visit(root, root.local_matrix(), matches, &mut path)?;
    Some((path, world))
}

fn node_at<'a>(root: &'a Node, path: &[usize]) -> &'a Node {
    path.iter().fold(root, |node, &index| &node.children[index])
}

/// Replace the node at `path` with an empty group (keeping sibling indices).
fn take_at(root: &mut Node, path: &[usize]) -> Node {
    let mut node = root;
    for &index in path {
        node = &mut node.children[index];
    }
    std::mem::replace(node, Node::group(""))
}

/// The paths of the subtrees `Scenery::update` animates.
fn animated_paths(scenery: &Scenery) -> Vec<Vec<usize>> {
    let root = scenery.root();
    match scenery {
        Scenery::Village(_) => find_path(root, &|node| node.name == WATERWHEEL)
            .map(|(path, _)| vec![path])
            .unwrap_or_default(),
        Scenery::Harbor(_) => {
            // `HarborScenery::update`: each crane's load (its last child) sways,
            // each tug (its lane's last child) runs its loop, and the beacon group
            // blinks. The moored ships lie still and bake with the static scenery.
            let fleet_index = 1;
            let fleet = &root.children[fleet_index];
            let mut paths = Vec::new();
            for (index, child) in fleet.children.iter().enumerate().skip(SHIP_COUNT) {
                paths.push(vec![fleet_index, index, child.children.len() - 1]);
            }
            paths.push(vec![root.children.len() - 1]);
            paths
        }
        Scenery::Quarry(_) => Vec::new(),
    }
}

/// Moored ships in `HarborFleet` (its first children; the cranes follow).
const SHIP_COUNT: usize = 3;

fn is_water(node: &Node) -> bool {
    node.drawable.as_ref().is_some_and(|drawable| {
        matches!(&drawable.material.effect, Effect::Custom { name, .. }
            if *name == core_models::effects_scenery::WATER)
    })
}

/// `buildScenery(theme)`: the theme's scenery, split for drawing. `None` for
/// extra levels, which show plain pads and floors instead.
pub fn scenery(theme: Theme) -> Option<SceneryModel> {
    let map_theme = theme.map_theme()?;
    let mut scenery = build_scenery(map_theme);
    scenery.update(0.0);
    let source = scenery.root();
    let mut root = source.clone();
    let mut movers = Vec::new();
    for path in animated_paths(&scenery) {
        let parent_world = path[..path.len() - 1]
            .iter()
            .fold((source, source.local_matrix()), |(node, world), &index| {
                let child = &node.children[index];
                (child, world * child.local_matrix())
            })
            .1;
        let mut subtree = take_at(&mut root, &path);
        subtree.position = DVec3::ZERO;
        subtree.rotation = glam::DQuat::IDENTITY;
        subtree.scale = DVec3::ONE;
        subtree.visible = true;
        movers.push(SceneryMover {
            root: subtree,
            path,
            parent_world,
        });
    }
    let water = find_path(source, &is_water).map(|(path, parent_world)| {
        let node = node_at(source, &path);
        take_at(&mut root, &path);
        let drawable = node.drawable.as_ref().expect("water mesh");
        let height = match &drawable.material.effect {
            Effect::Custom { params, .. } => f64::from(params[0]),
            Effect::None => 0.0,
        };
        let mut mesh = (*drawable.mesh).clone();
        mesh.apply_matrix4(&(parent_world * node.local_matrix()));
        mesh.translate(0.0, -height, 0.0);
        if theme == Theme::Harbor {
            SceneryWater::Harbor(mesh)
        } else {
            SceneryWater::Creek(mesh)
        }
    });
    let smoke = find_path(source, &|node| node.name == CHIMNEY_SMOKE_NODE)
        .map(|(path, _)| take_at(&mut root, &path));
    // `root` and the movers now hold every mesh until they are registered, after
    // which only the GPU keeps them: the retained scenery animates transforms alone.
    scenery.release_drawables();
    Some(SceneryModel {
        scenery,
        root,
        movers,
        water,
        smoke,
    })
}

/// Fill the village chimney smoke for this round's houses and return the
/// refreshed smoke node (`VillageAtmosphere.setCovers`).
pub fn refill_smoke(scenery: &mut Scenery, covers: &[RenderCover]) -> Option<Node> {
    let Scenery::Village(village) = scenery else {
        return None;
    };
    // Every cottage smokes while its chimney stands; a toppled or fallen one keeps
    // its wisps (so the others' never reshuffle) far underground.
    let covers: Vec<SmokeCover> = covers
        .iter()
        .map(|cover| {
            let house = cover.kind == CoverKind::House;
            SmokeCover {
                kind: cover.kind.as_str(),
                destructible: cover.destructible && !house,
                x: cover.x,
                z: cover.z,
                w: cover.w,
                h: if house && !chimney_stands(cover) {
                    FALLEN_CHIMNEY_DEPTH
                } else {
                    cover.h
                },
                d: cover.d,
            }
        })
        .collect();
    village.set_covers(&covers);
    village.root.find(CHIMNEY_SMOKE_NODE).cloned()
}

/// Where a fallen chimney's smoke is sent, out of sight below the ground.
const FALLEN_CHIMNEY_DEPTH: f64 = -200.0;

/// Whether a cottage's chimney is up: it topples when the cottage is wrecked.
fn chimney_stands(cover: &RenderCover) -> bool {
    cover.alive && cover_damage_stage(cover.kind, cover.hp, cover.max_hp) < 2
}

/// Every cottage and whether its chimney stands, to notice when the smoke changes.
pub fn standing_chimneys(covers: &[RenderCover]) -> Vec<(u32, bool)> {
    covers
        .iter()
        .filter(|cover| cover.kind == CoverKind::House)
        .map(|cover| (cover.id, chimney_stands(cover)))
        .collect()
}

/// The wisp instances of a smoke node: identity placements with each wisp's
/// origin and phase as effect data.
pub fn smoke_instances(smoke: &Node) -> Vec<crate::model::InstanceData> {
    let Some(drawable) = &smoke.drawable else {
        return Vec::new();
    };
    let count = drawable.instances.as_ref().map_or(0, Vec::len);
    (0..count)
        .map(|index| crate::model::InstanceData {
            matrix: glam::Mat4::IDENTITY,
            color: [1.0; 3],
            data: crate::model::instance_attribute_data(&drawable.mesh, index),
        })
        .collect()
}

/// Each standard theme's scenery, built once for the tests that read it.
#[cfg(test)]
pub(crate) mod shared_scenery {
    use super::{SceneryModel, Theme, scenery};
    use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

    const THEMES: [Theme; 3] = [Theme::Village, Theme::Harbor, Theme::Quarry];

    /// `theme`'s scenery at time 0. A test that animates it holds the lock and
    /// returns it to time 0.
    pub(crate) fn shared_scenery(theme: Theme) -> MutexGuard<'static, SceneryModel> {
        static MODELS: [OnceLock<Mutex<SceneryModel>>; 3] = [const { OnceLock::new() }; 3];
        let index = THEMES
            .iter()
            .position(|&standard| standard == theme)
            .expect("a standard theme");
        MODELS[index]
            .get_or_init(|| Mutex::new(scenery(theme).expect("standard scenery")))
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::shared_scenery::shared_scenery;
    use super::*;
    use sloppy_core::sim::render_state::RenderCover;

    #[test]
    fn scenery_splits_movers_water_and_smoke() {
        let village = shared_scenery(Theme::Village);
        assert_eq!(village.movers.len(), 1);
        assert!(matches!(village.water, Some(SceneryWater::Creek(_))));
        assert!(village.smoke.is_some());
        assert!(village.root.find(WATERWHEEL).is_none());
        drop(village);
        let harbor = shared_scenery(Theme::Harbor);
        assert!(matches!(harbor.water, Some(SceneryWater::Harbor(_))));
        // Four crane loads, two tugs and the beacons; the moored ships are static.
        assert_eq!(harbor.movers.len(), 7);
        drop(harbor);
        let quarry = shared_scenery(Theme::Quarry);
        assert!(quarry.movers.is_empty() && quarry.water.is_none());
        assert!(scenery(Theme::Custom).is_none());
    }

    /// The renderer drops shadow casters by where their shadow can fall above
    /// `SHADOW_RECEIVER_FLOOR`; a receiving surface below it would lose shadows.
    #[test]
    fn shadow_receivers_stay_above_the_receiver_floor() {
        fn lowest_receiver(node: &Node, parent: DMat4) -> f64 {
            let world = parent * node.local_matrix();
            let own = node
                .drawable
                .as_ref()
                .filter(|drawable| drawable.receive_shadow)
                .map_or(f64::INFINITY, |drawable| {
                    let placements = match &drawable.instances {
                        Some(list) => list.iter().map(|item| world * item.matrix).collect(),
                        None => vec![world],
                    };
                    placements
                        .iter()
                        .flat_map(|matrix| {
                            drawable.mesh.positions.iter().map(|&[x, y, z]| {
                                matrix
                                    .transform_point3(DVec3::new(x.into(), y.into(), z.into()))
                                    .y
                            })
                        })
                        .fold(f64::INFINITY, f64::min)
                });
            node.children
                .iter()
                .map(|child| lowest_receiver(child, world))
                .fold(own, f64::min)
        }
        let floor = f64::from(super::super::theme::SHADOW_RECEIVER_FLOOR);
        for theme in [Theme::Village, Theme::Harbor, Theme::Quarry] {
            let mut model = shared_scenery(theme);
            let still = lowest_receiver(&model.root, DMat4::IDENTITY);
            assert!(still > floor, "{theme:?}: a receiver at {still:.2} m");
            // Ships bob and crane loads sway: sample the animation of what moves.
            for step in 0..40 {
                model.scenery.update(f64::from(step) * 0.7);
                for mover in &model.movers {
                    let (world, _) = mover.pose(&model.scenery);
                    let lowest = lowest_receiver(&mover.root, world);
                    assert!(lowest > floor, "{theme:?}: a receiver at {lowest:.2} m");
                }
            }
            model.scenery.update(0.0);
        }
    }

    /// The retained scenery only animates transforms and refills the chimney smoke,
    /// so it keeps no other drawable: `root` and the movers carry the meshes.
    #[test]
    fn retained_scenery_keeps_only_the_chimney_smoke() {
        fn drawables(node: &Node) -> usize {
            if node.name == CHIMNEY_SMOKE_NODE {
                return 0;
            }
            usize::from(node.drawable.is_some())
                + node.children.iter().map(drawables).sum::<usize>()
        }
        for theme in [Theme::Village, Theme::Harbor, Theme::Quarry] {
            let model = shared_scenery(theme);
            assert_eq!(drawables(model.scenery.root()), 0, "{theme:?}");
            assert!(drawables(&model.root) > 0, "{theme:?}");
        }
        let village = shared_scenery(Theme::Village);
        let smoke = village.scenery.root().find(CHIMNEY_SMOKE_NODE);
        assert!(smoke.is_some_and(|smoke| smoke.drawable.is_some()));
    }

    #[test]
    fn covers_with_looks_of_their_own_combine_first() {
        let drum = |x: f64| RenderCover {
            kind: CoverKind::Drum,
            x,
            w: 1.2,
            h: 1.6,
            d: 1.2,
            ..RenderCover::default()
        };
        let house = |x: f64| RenderCover {
            kind: CoverKind::House,
            ..drum(x)
        };
        let movable = |x: f64| RenderCover {
            motion: Some(Default::default()),
            ..house(x)
        };
        let covers = [drum(0.0), house(1.0), movable(2.0), drum(3.0), house(4.0)];
        assert_eq!(cover_order(&covers), (vec![1, 4], vec![0, 2, 3]));
        // One cover of its own look leads the rest, in order.
        let covers = [drum(0.0), house(1.0), drum(3.0)];
        assert_eq!(cover_order(&covers), (vec![], vec![1, 0, 2]));
    }

    #[test]
    fn identical_covers_share_keys() {
        let drum = |x: f64| RenderCover {
            kind: CoverKind::Drum,
            x,
            z: 3.0,
            w: 1.2,
            h: 1.6,
            d: 1.2,
            ..RenderCover::default()
        };
        assert_eq!(cover_key(&drum(0.0), 0), cover_key(&drum(5.0), 0));
        let house = |x: f64| RenderCover {
            kind: CoverKind::House,
            ..drum(x)
        };
        assert_ne!(cover_key(&house(0.0), 0), cover_key(&house(5.0), 0));
    }
}
