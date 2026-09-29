//! Port of `village-scenery.ts`: Pine Village's retained scenery (terrain, yard,
//! valley, meadow, chimney smoke, landmarks and the footbridge approach).

use std::sync::Arc;

use crate::geometry::box_geometry;
use crate::scene::{Material, Node, TextureRef, TextureSource, Wrap};

use super::batching::batch;
use super::effects_scenery::VILLAGE_SIGN_TEXTURE;
use super::model_primitives::{box_part, put, rotated};
use super::scenery::create_terrain;
use super::village_atmosphere::{SmokeCover, set_chimney_smoke, village_atmosphere};
use super::village_landmarks::{WATERWHEEL, village_landmarks};
use super::village_landscape::{valley_height, village_landscape};
use super::village_vegetation::village_vegetation;

/// Node name of the chimney smoke (see [`VillageScenery::set_covers`]).
pub const CHIMNEY_SMOKE_NODE: &str = "village-chimney-smoke";

/// Pine Village scenery, retained across matches (`VillageScenery`).
pub struct VillageScenery {
    /// `pine-village-scenery`: board, grass floor, roads, yard details, valley
    /// landscape, meadow, chimney smoke, landmarks, then the footbridge approach.
    pub root: Node,
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
        Self { root }
    }

    /// `update(time)`: turn the waterwheel. The creek, meadow sway and smoke read
    /// the same clock in their shaders.
    pub fn update(&mut self, time: f64) {
        if let Some(wheel) = self.root.find_mut(WATERWHEEL) {
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
    for x in [-4.4, 4.4] {
        put(
            &mut details,
            box_part(0.26, 3.2, 0.26, 0x715534, 0.0),
            x,
            0.75,
            -64.5,
        );
    }
    put(
        &mut details,
        box_part(9.2, 0.22, 0.34, 0x80613d, 0.0),
        0.0,
        2.35,
        -64.5,
    );
    // Small hanging sign is a landmark, not an in-game overlay.
    let map = TextureRef {
        source: TextureSource::Generated(VILLAGE_SIGN_TEXTURE),
        wrap: Wrap::Clamp,
        ..TextureRef::file("")
    };
    let sign = Node::mesh(
        Arc::new(box_geometry(5.2, 1.3, 0.16)),
        Arc::new(Material {
            map: Some(map),
            ..Material::standard(0xffffff, 0.0, 1.0)
        }),
    );
    put(&mut details, sign, 0.0, 1.55, -64.4);
    batch(&mut details);
    details
}
