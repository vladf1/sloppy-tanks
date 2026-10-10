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

use glam::{DMat4, DVec3};
use sloppy_core::geometry::Mesh;
use sloppy_core::models::{
    self as core_models, CHIMNEY_SMOKE_NODE, CoverShape, Scenery, SmokeCover, TreeDetail,
    WATERWHEEL, batch, branch_drop_stage, build_scenery, tree_part,
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
    /// village refills its chimney smoke with `set_covers` on reset.
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
            // `HarborScenery::update`: the fleet's ships bob and roll, each crane's
            // load (its last child) sways, and the beacon group blinks.
            let fleet_index = 1;
            let fleet = &root.children[fleet_index];
            let mut paths = Vec::new();
            for (index, child) in fleet.children.iter().enumerate() {
                if index < SHIP_COUNT {
                    paths.push(vec![fleet_index, index]);
                } else {
                    paths.push(vec![fleet_index, index, child.children.len() - 1]);
                }
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
    let map_theme = match theme {
        Theme::Village => core_models::MapTheme::Village,
        Theme::Harbor => core_models::MapTheme::Harbor,
        Theme::Quarry => core_models::MapTheme::Quarry,
        Theme::Custom => return None,
    };
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
    let covers: Vec<SmokeCover> = covers
        .iter()
        .map(|cover| SmokeCover {
            kind: cover.kind.as_str(),
            destructible: cover.destructible,
            x: cover.x,
            z: cover.z,
            w: cover.w,
            h: cover.h,
            d: cover.d,
        })
        .collect();
    village.set_covers(&covers);
    village.root.find(CHIMNEY_SMOKE_NODE).cloned()
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

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::models::{cover_damage_stage, tree_branch_stage};
    use sloppy_core::sim::render_state::RenderCover;

    #[test]
    fn damage_stages_follow_cover_model_ts() {
        assert_eq!(cover_damage_stage(CoverKind::Cargo, 100.0, 100.0), 0);
        assert_eq!(cover_damage_stage(CoverKind::Cargo, 50.0, 100.0), 1);
        assert_eq!(cover_damage_stage(CoverKind::Cargo, 30.0, 100.0), 2);
        assert_eq!(cover_damage_stage(CoverKind::Timber, 100.0, 100.0), 0);
        assert_eq!(cover_damage_stage(CoverKind::Timber, 60.0, 100.0), 1);
        assert_eq!(cover_damage_stage(CoverKind::Timber, 30.0, 100.0), 2);
        assert_eq!(cover_damage_stage(CoverKind::Timber, 10.0, 100.0), 3);
        assert_eq!(cover_damage_stage(CoverKind::House, 1.0, 100.0), 0);
        assert_eq!(tree_branch_stage(1.0), 0);
        assert_eq!(tree_branch_stage(0.5), 1);
        assert_eq!(tree_branch_stage(0.35), 2);
    }

    #[test]
    fn scenery_splits_movers_water_and_smoke() {
        let village = scenery(Theme::Village).expect("village");
        assert_eq!(village.movers.len(), 1);
        assert!(matches!(village.water, Some(SceneryWater::Creek(_))));
        assert!(village.smoke.is_some());
        assert!(village.root.find(WATERWHEEL).is_none());
        let harbor = scenery(Theme::Harbor).expect("harbor");
        assert!(matches!(harbor.water, Some(SceneryWater::Harbor(_))));
        // Three ships, four crane loads and the beacons.
        assert_eq!(harbor.movers.len(), 8);
        let quarry = scenery(Theme::Quarry).expect("quarry");
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
            let mut model = scenery(theme).expect("scenery");
            // Ships bob and crane loads sway: sample the animation.
            for step in 0..40 {
                model.scenery.update(f64::from(step) * 0.7);
                let lowest = lowest_receiver(model.scenery.root(), DMat4::IDENTITY);
                assert!(lowest > floor, "{theme:?}: a receiver at {lowest:.2} m");
            }
        }
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
