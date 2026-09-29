//! The one adapter between presentation and the cover, tree, prop and scenery
//! model builders.
//!
//! **PLACEHOLDERS — wire at merge.** The functions marked `PLACEHOLDER` return
//! simple boxes, cylinders and cones so presentation, batching, damage stages,
//! felling and debris run end to end before the real models exist. At merge the
//! lead replaces their bodies with the model crates' builders, keeping these
//! signatures and the conventions documented on each type:
//!
//! - [`cover_model`] ← `models::cover_model(cover, stage)` / `models::tree_model(cover)`
//! - [`timber_part_model`] ← `models::timber_part_model(part)` (`timber-model.ts`)
//! - [`surface_debris_piece`] ← `sidingBox` / `trunkFragment` pieces (`house-surfaces.ts`, `tree-models.ts`)
//! - [`scenery`] ← `models::scenery::build(theme)` (village, harbor, quarry scenery,
//!   the harbor/creek water surfaces and scenery animation such as the mill wheel
//!   or bobbing boats)
//!
//! Everything else here ([`cover_damage_stage`], [`CoverModel`] conventions) is
//! real logic ported from `cover-model.ts`.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::{DMat4, DVec3};
use sloppy_core::geometry::{Mesh, circle_geometry, cone_geometry, plane_geometry};
use sloppy_core::models::{DEFAULT_BOX_RADIUS, box_part, cylinder_part, paint, put};
use sloppy_core::scene::{Material, Node};
use sloppy_core::sim::data::ARENA;
use sloppy_core::sim::maps::GroundKind;
use sloppy_core::sim::render_state::RenderCover;
use sloppy_core::sim::timber_layout::{TimberPart, timber_damage_stage};
use sloppy_core::sim::{CoverKind, FragmentShape};

use super::models::{arena_floor, spawn_pads, water_plane};
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
/// - Named nodes are joints presentation may hide (see [`TreeParts`]); leave
///   other parts unnamed so they batch.
pub struct CoverModel {
    pub root: Node,
    /// Equal for covers that look identical, so they share one prepared model
    /// and draw instanced. Include every input the builder reads (kind, size,
    /// color, seed, damage stage, timber hits and joins).
    pub key: String,
    pub tree: Option<TreeParts>,
}

/// The parts of a tree that damage and felling animate (`tree-models.ts`
/// `userData.crown`, `cutSurface`, `branches`).
pub struct TreeParts {
    /// Joint name of the trunk-and-crown group: hidden when felled; its subtree is
    /// also the falling crown fragment, placed `-tree_center_y` below the body.
    pub crown: String,
    /// Joint name of the stump's cut face, shown only once felled.
    pub cut_surface: String,
    /// Boughs shed with damage, each visible while `drop_stage` exceeds the
    /// branch damage stage (0 healthy, 1 hurt, 2 at 35% health).
    pub branches: Vec<TreeBranch>,
}

pub struct TreeBranch {
    pub name: String,
    pub drop_stage: u32,
}

/// `coverDamageStage`: cargo and timber rebuild with dents and splinters as they
/// take damage; other covers keep one look.
pub fn cover_damage_stage(kind: CoverKind, hp: f64, max_hp: f64) -> u32 {
    match kind {
        CoverKind::Cargo => {
            if hp >= max_hp {
                0
            } else if hp > max_hp * 0.35 {
                1
            } else {
                2
            }
        }
        CoverKind::Timber => timber_damage_stage(hp, max_hp),
        _ => 0,
    }
}

/// `setTreeDamage` stages: shed two boughs after the first damage, then two more
/// at 35% health.
pub fn tree_branch_stage(health_ratio: f64) -> u32 {
    if health_ratio >= 1.0 {
        0
    } else if health_ratio > 0.35 {
        1
    } else {
        2
    }
}

/// PLACEHOLDER for `models::cover_model` / `models::tree_model`.
pub fn cover_model(cover: &RenderCover, stage: u32) -> CoverModel {
    let (w, h, d) = (cover.w, cover.h, cover.d);
    let mut root = Node::group("cover");
    root.position = DVec3::new(cover.x, 0.0, cover.z);
    let key = format!(
        "{:?}/{w}/{h}/{d}/{:x}/{stage}/{}",
        cover.kind,
        cover.color,
        cover.timber_hits.len()
    );
    let mut tree = None;
    match cover.kind {
        CoverKind::Tree => {
            let trunk_height = h.max(4.0);
            let mut crown = Node::group("tree-crown");
            put(&mut crown, cylinder_part(0.35, trunk_height * 0.6, 0x7a5a3a, 8), 0.0, trunk_height * 0.3, 0.0);
            let mut foliage = Node::mesh(
                Arc::new(cone_geometry(w.max(1.5) * 0.9, trunk_height * 0.8, 9)),
                paint(0x3f7d3a),
            );
            if let Some(drawable) = &mut foliage.drawable {
                drawable.cast_shadow = true;
                drawable.receive_shadow = true;
            }
            put(&mut crown, foliage, 0.0, trunk_height * 0.75, 0.0);
            let mut branches = Vec::new();
            for i in 0..4 {
                let name = format!("tree-branch-{i}");
                let angle = f64::from(i) * PI / 2.0 + 0.4;
                let mut bough = Node::group(name.clone());
                let mut leaves = box_part(1.1, 0.5, 1.1, 0x4c8f3f, DEFAULT_BOX_RADIUS);
                leaves.position = DVec3::new(0.6, 0.0, 0.0);
                bough.children.push(leaves);
                bough.set_rotation_euler(0.0, angle, 0.0);
                bough.position = DVec3::new(0.0, trunk_height * 0.45, 0.0);
                crown.children.push(bough);
                branches.push(TreeBranch {
                    name,
                    drop_stage: if i < 2 { 1 } else { 2 },
                });
            }
            root.children.push(crown);
            let mut stump_cut = Node::mesh(Arc::new(circle_geometry(0.36, 12)), paint(0xc9a67a));
            stump_cut.set_rotation_euler(-PI / 2.0, 0.0, 0.0);
            stump_cut.name = "tree-cut".into();
            stump_cut.position = DVec3::new(0.0, 0.5, 0.0);
            stump_cut.visible = false;
            root.children.push(stump_cut);
            put(&mut root, cylinder_part(0.4, 0.5, 0x6d4f33, 8), 0.0, 0.25, 0.0);
            tree = Some(TreeParts {
                crown: "tree-crown".into(),
                cut_surface: "tree-cut".into(),
                branches,
            });
        }
        CoverKind::Drum => {
            put(&mut root, cylinder_part(w / 2.0, h, cover.color, 14), 0.0, h / 2.0, 0.0);
        }
        CoverKind::House => {
            put(&mut root, box_part(w, h * 0.68, d, cover.color, 0.0), 0.0, h * 0.34, 0.0);
            let mut roof = box_part(w + 0.4, h * 0.32, d + 0.4, 0xb23b2e, 0.0);
            roof.position = DVec3::new(0.0, h * 0.84, 0.0);
            root.children.push(roof);
        }
        _ => {
            put(&mut root, box_part(w, h, d, cover.color, 0.0), 0.0, h / 2.0, 0.0);
        }
    }
    CoverModel { root, key, tree }
}

/// PLACEHOLDER for `timberPartModel(part)`: a group whose frame is the part's
/// centre (the fragment body pose places it).
pub fn timber_part_model(part: &TimberPart) -> Node {
    let mut root = Node::group("timber-part");
    root.children
        .push(box_part(part.w, part.h, part.d, part.color, 0.0));
    root
}

/// PLACEHOLDER for the textured debris pieces: `sidingBox(1.5, 0.18, 0.45)` for
/// wood, `sidingBox(1, 1, 1)` for panels and beams, `trunkFragment()` for logs.
/// Unit-colored (white) so the instance tint paints each piece.
pub fn surface_debris_piece(shape: FragmentShape) -> Node {
    let piece = match shape {
        FragmentShape::Wood => box_part(1.5, 0.18, 0.45, 0xffffff, 0.0),
        FragmentShape::Log => cylinder_part(0.43, 1.0, 0xffffff, 8),
        _ => box_part(1.0, 1.0, 1.0, 0xffffff, 0.0),
    };
    let mut root = Node::group("debris");
    root.children.push(piece);
    root
}

/// Theme scenery: static geometry baked once, optional movers, optional water.
pub struct SceneryModel {
    /// Baked into static batches (world transforms as authored).
    pub root: Node,
    /// Animated scenery parts; each frame presentation places `root` at
    /// `world * motion(time)`.
    pub movers: Vec<SceneryMover>,
    pub water: Option<SceneryWater>,
}

pub struct SceneryMover {
    pub root: Node,
    pub world: DMat4,
    pub motion: fn(f64) -> DMat4,
}

/// The planar-reflection water surface in world XZ at y = 0.
pub enum SceneryWater {
    /// `HarborWater`: the basin around the pier.
    Harbor(Mesh),
    /// `VillageLandscape`'s creek.
    Creek(Mesh),
}

/// PLACEHOLDER for `models::scenery::build(theme)`: a plain ground board per
/// theme (the village's is the real `createTerrain` board and grass), and the
/// harbor's water basin.
pub fn scenery(theme: Theme) -> Option<SceneryModel> {
    let (board, outer) = match theme {
        Theme::Village => (0x947c4d, 0x6f9a4a),
        Theme::Harbor => (0x8d918f, 0x6f7a7d),
        Theme::Quarry => (0xc7a878, 0xb99a6c),
        Theme::Custom => return None,
    };
    let mut root = Node::group(format!("{} scenery", theme.name()));
    let size = ARENA * 2.0 + 6.0;
    put(&mut root, box_part(size, 1.2, size, board, 0.4), 0.0, -0.8, 0.0);
    match theme {
        Theme::Village => {
            put(&mut root, arena_floor(GroundKind::DryGrass, ARENA * 2.0), 0.0, 0.008, 0.0);
        }
        _ => {
            let mut floor = Node::mesh(
                Arc::new(flat_plane(ARENA * 2.0)),
                Arc::new(Material::standard(board, 0.0, 1.0)),
            );
            if let Some(drawable) = &mut floor.drawable {
                drawable.receive_shadow = true;
            }
            put(&mut root, floor, 0.0, 0.008, 0.0);
        }
    }
    // Every themed scenery builds its own spawn pads (`createSpawnPads`).
    root.children.push(spawn_pads(1.0));
    if theme != Theme::Harbor {
        let mut surround = Node::mesh(
            Arc::new(flat_plane(360.0)),
            Arc::new(Material::standard(outer, 0.0, 1.0)),
        );
        if let Some(drawable) = &mut surround.drawable {
            drawable.receive_shadow = true;
        }
        put(&mut root, surround, 0.0, -1.4, 0.0);
    }
    let water = (theme == Theme::Harbor).then(|| SceneryWater::Harbor(water_plane(340.0)));
    Some(SceneryModel {
        root,
        movers: Vec::new(),
        water,
    })
}

fn flat_plane(size: f64) -> Mesh {
    let mut plane = plane_geometry(size, size);
    plane.rotate_x(-PI / 2.0);
    plane
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_stages_follow_cover_model_ts() {
        assert_eq!(cover_damage_stage(CoverKind::Cargo, 100.0, 100.0), 0);
        assert_eq!(cover_damage_stage(CoverKind::Cargo, 50.0, 100.0), 1);
        assert_eq!(cover_damage_stage(CoverKind::Cargo, 30.0, 100.0), 2);
        assert_eq!(cover_damage_stage(CoverKind::House, 1.0, 100.0), 0);
        assert_eq!(tree_branch_stage(1.0), 0);
        assert_eq!(tree_branch_stage(0.5), 1);
        assert_eq!(tree_branch_stage(0.35), 2);
    }
}
