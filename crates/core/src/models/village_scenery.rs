//! Port of `village-scenery.ts`: Pine Village's retained scenery (terrain, yard,
//! valley, meadow, chimney smoke, landmarks and the footbridge approach).

use std::sync::Arc;

use crate::geometry::box_geometry;
use crate::scene::{Color, Effect, Material, Node, TextureRef};

use super::batching::batch;
use super::effects_scenery::VILLAGE_SIGN_TEXTURE;
use super::model_primitives::{box_part, put, rotated};
use super::scenery::create_terrain;
use super::village_atmosphere::{
    CHIMNEY_SMOKE_NODE, SmokeCover, set_chimney_smoke, village_atmosphere,
};
use super::village_landmarks::{WATERWHEEL, village_landmarks};
use super::village_landscape::{valley_height, village_landscape};
use super::village_vegetation::village_vegetation;

/// Pine Village scenery, retained across matches (`VillageScenery`).
pub struct VillageScenery {
    /// `pine-village-scenery`: board, grass floor, roads, yard details, valley
    /// landscape, meadow, chimney smoke, landmarks, then the footbridge approach.
    pub root: Node,
    /// The waterwheel's child-index path from `root`, found once: the scenery has
    /// thousands of nodes to search by name, and the wheel turns every frame.
    wheel: Option<Vec<usize>>,
}

impl Default for VillageScenery {
    fn default() -> Self {
        Self::new()
    }
}

impl VillageScenery {
    pub fn new() -> Self {
        let mut root = Node::group("pine-village-scenery");
        let grass = create_terrain(&mut root);
        root.children.push(village_landscape(grass));
        root.children.push(village_vegetation());
        root.children.push(village_atmosphere());
        root.children.push(village_landmarks());
        root.children.push(approach_details());
        settle_paint(&mut root);
        let wheel = node_path(&root, WATERWHEEL);
        Self { root, wheel }
    }

    /// `update(time)`: turn the waterwheel. The creek, meadow sway and smoke read
    /// the same clock in their shaders.
    pub fn update(&mut self, time: f64) {
        if let Some(path) = &self.wheel {
            let wheel = path
                .iter()
                .fold(&mut self.root, |node, &index| &mut node.children[index]);
            wheel.set_rotation_euler(0.0, 0.0, -time * 0.16);
        }
    }

    /// `setCovers(covers)`: chimney smoke rises from this round's houses.
    pub fn set_covers(&mut self, covers: &[SmokeCover]) {
        if let Some(smoke) = self.root.find_mut(CHIMNEY_SMOKE_NODE) {
            set_chimney_smoke(smoke, covers);
        }
    }
}

/// The child-index path from `root` to the first node named `name` (depth first, as
/// `Node::find_mut` searches).
fn node_path(root: &Node, name: &str) -> Option<Vec<usize>> {
    if root.name == name {
        return Some(Vec::new());
    }
    root.children.iter().enumerate().find_map(|(index, child)| {
        node_path(child, name).map(|mut path| {
            path.insert(0, index);
            path
        })
    })
}

/// The finish every matte painted surface of the village shares.
const MATTE_ROUGHNESS: f32 = 0.8;
const MATTE_METALNESS: f32 = 0.05;

/// Give every plain matte paint (untextured, opaque, neither metal nor glowing)
/// one finish. Static scenery draws once per material and 60 m cell, and painted
/// parts merge only when everything but their colour matches: a dozen near-equal
/// roughness values cost a dozen draws in every cell for no visible difference.
fn settle_paint(root: &mut Node) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if let Some(drawable) = &mut node.drawable {
            let m = &drawable.material;
            let matte = m.effect == Effect::None
                && m.map.is_none()
                && m.bump_map.is_none()
                && !m.transparent
                && m.alpha_test == 0.0
                && m.emissive == Color(0)
                && m.metalness <= 0.2
                && m.roughness >= 0.55;
            if matte && (m.roughness, m.metalness) != (MATTE_ROUGHNESS, MATTE_METALNESS) {
                drawable.material = Arc::new(Material {
                    roughness: MATTE_ROUGHNESS,
                    metalness: MATTE_METALNESS,
                    ..(**m).clone()
                });
            }
        }
        stack.extend(node.children.iter_mut());
    }
}

/// Where the village sign's gate stands behind the north wall, and the height of
/// its posts' tops (m).
const SIGN_GATE_Z: f64 = -66.5;
const SIGN_GATE_TOP: f64 = 3.6;

/// Stepping stones and a timber sign link the village road to its old footbridge.
fn approach_details() -> Node {
    let mut details = Node::group("");
    for i in 0..6 {
        let i = f64::from(i);
        let z = -63.0 - i * 1.35;
        let stone = rotated(
            box_part(1.1, 0.15, 0.85, 0xa4aa8c, 0.08),
            0.0,
            i.sin() * 0.3,
            0.0,
        );
        put(
            &mut details,
            stone,
            (i * 0.6).sin() * 0.4,
            valley_height(0.0, z) + 0.12,
            z,
        );
    }
    // The gate stands well back from the boundary wall and tall enough that its
    // sign clears the wall from the overhead camera.
    let ground = valley_height(0.0, SIGN_GATE_Z);
    let post = SIGN_GATE_TOP - ground;
    for x in [-4.4, 4.4] {
        put(
            &mut details,
            box_part(0.26, post, 0.26, 0x715534, 0.0),
            x,
            ground + post / 2.0,
            SIGN_GATE_Z,
        );
    }
    put(
        &mut details,
        box_part(9.2, 0.22, 0.34, 0x80613d, 0.0),
        0.0,
        SIGN_GATE_TOP - 0.15,
        SIGN_GATE_Z,
    );
    // Small hanging sign is a landmark, not an in-game overlay.
    let sign = Node::mesh(
        Arc::new(box_geometry(5.2, 1.3, 0.16)),
        Arc::new(Material {
            map: Some(TextureRef::generated(VILLAGE_SIGN_TEXTURE)),
            ..Material::standard(0xffffff, 0.0, 1.0)
        }),
    );
    put(
        &mut details,
        sign,
        0.0,
        SIGN_GATE_TOP - 0.85,
        SIGN_GATE_Z + 0.1,
    );
    batch(&mut details);
    details
}
