//! Temporary stand-ins for code the scenery needs from modules other agents port:
//! the seeded `Random` (sim), arena constants and layouts (sim/maps) and tree models
//! (props). Everything here is marked `PENDING` and is replaced at integration; the
//! signatures are the ones the scenery calls.
//!
//! `Random`, `ARENA`, `spawn_positions` and `quarry_layout` are exact ports (the
//! scenery reference tests depend on them); `tree_model` is a placeholder.

use std::sync::{Arc, OnceLock};

use glam::DVec3;

use crate::geometry::math::to_int32;
use crate::geometry::{Mesh, cone_geometry};
use crate::scene::{Material, Node};

// PENDING: replaced by the simulation's `Random` (src/game/math.ts) at integration.
/// Mulberry32 with the double-valued state of the TypeScript: `state` grows by the
/// increment as a JavaScript number, so long streams (the soil bake draws millions)
/// keep the same rounding once the state exceeds 2^53.
#[derive(Clone, Debug)]
pub struct Random {
    pub state: f64,
}

impl Random {
    pub fn new(seed: f64) -> Self {
        Self { state: seed }
    }

    #[allow(clippy::should_implement_trait)] // Mirrors the TypeScript `Random.next`.
    pub fn next(&mut self) -> f64 {
        self.state += 1_831_565_813.0;
        let mut t = to_int32(self.state) as u32;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }

    pub fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.next()
    }
}

// PENDING: replaced by the simulation's `ARENA` (src/game/data.ts) at integration.
/// Half the standard arena's side, in metres.
pub const ARENA: f64 = 60.0;

// PENDING: replaced by the simulation's `spawnPositions` (src/game/arena.ts).
/// A team's five spawn points (x, z) for an arena `scale`.
pub fn spawn_positions(team: u8, scale: f64) -> [(f64, f64); 5] {
    [-46.0, -23.0, 0.0, 23.0, 46.0].map(|z: f64| {
        (
            (if team == 0 { -53.0 } else { 53.0 }) * scale,
            (if team == 0 { z } else { -z }) * scale,
        )
    })
}

/// The cover fields the scenery reads from a map layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutCover {
    pub kind: &'static str,
    pub x: f64,
    pub z: f64,
    pub w: f64,
    pub d: f64,
    pub h: f64,
}

// PENDING: replaced by the simulation's `DRAGON_TOOTH_SCALE` (quarry-barrier-shapes.ts).
const DRAGON_TOOTH_SCALE: f64 = 0.9;

// PENDING: replaced by the simulation's `quarryLayout` (src/game/quarry-layout.ts).
/// Dusty Dig's cover layout in its authored order (the sand drift splats follow it).
pub fn quarry_layout() -> Vec<LayoutCover> {
    let mut covers = Vec::new();
    let mut add = |kind, x, z, w, d, h| {
        covers.push(LayoutCover {
            kind,
            x,
            z,
            w,
            d,
            h,
        })
    };
    for side in [-1.0, 1.0] {
        add(
            "boundary",
            side * (ARENA + 0.5),
            0.0,
            1.0,
            ARENA * 2.0 + 2.0,
            1.2,
        );
        add(
            "boundary",
            0.0,
            side * (ARENA + 0.5),
            ARENA * 2.0 + 2.0,
            1.0,
            1.2,
        );
        add("rock", side * 25.0, side * 12.0, 14.0, 8.0, 4.6);
        add("rock", side * 25.0, -side * 12.0, 14.0, 8.0, 3.8);
        add("rock", side * 4.0, side * 28.0, 16.0, 9.0, 4.8);
        add("rock", side * 40.5, side * 37.0, 10.0, 12.0, 4.2);
        add("rock", side * 24.5, side * 37.0, 10.0, 12.0, 3.6);
        add("rock", -side * 10.0, side * 47.0, 13.0, 7.0, 3.4);
        for x in [31.2, 33.8] {
            for z in [35.4, 38.6] {
                add("cargo", side * x, side * z, 2.4, 2.8, 2.1);
            }
        }
        add("cargo", side * 9.0, -side * 7.0, 3.0, 3.0, 2.6);
        add("drum", side * 15.0, side * 27.0, 1.2, 1.2, 1.7);
        for [x, z, width, height] in [
            [41.7, -15.3, 1.9, 1.9],
            [42.25, -12.45, 2.0, 2.05],
            [41.85, -9.35, 1.9, 1.8],
            [42.5, -6.7, 1.9, 1.9],
            [45.25, -13.85, 2.0, 2.05],
            [44.7, -10.8, 1.9, 1.8],
            [45.4, -7.9, 1.9, 1.9],
            [45.05, -4.75, 2.0, 2.05],
        ] {
            add(
                "teeth",
                side * x,
                side * z,
                width * DRAGON_TOOTH_SCALE,
                width * DRAGON_TOOTH_SCALE,
                height * DRAGON_TOOTH_SCALE,
            );
        }
        for i in 0..4 {
            add(
                "hedgehog",
                side * (15.8 + f64::from(i) * 2.4),
                side * 22.0,
                2.32,
                2.56,
                2.16,
            );
        }
    }
    covers
}

/// The placement fields of `treeModel`'s cover argument (`Pick<Cover, "x" | "z" |
/// "w" | "d" | "h">`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeDef {
    pub x: f64,
    pub z: f64,
    pub w: f64,
    pub d: f64,
    pub h: f64,
}

/// `treeModel(c, detail)`'s level of detail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeDetail {
    Full,
    Background,
}

// PENDING: replaced by the props agent's tree_models at integration.
/// `treeModel(c, detail)`: a group positioned at `(x, 0, z)` whose direct children
/// are mesh parts (the background forest re-parents them into one batch). This
/// placeholder is one cone per tree and does not consume any shared RNG, like the
/// real model (which seeds its own stream from the position).
pub fn tree_model(def: &TreeDef, _detail: TreeDetail) -> Node {
    static CONE: OnceLock<(Arc<Mesh>, Arc<Material>)> = OnceLock::new();
    let (mesh, material) = CONE.get_or_init(|| {
        let mut cone = cone_geometry(0.5, 1.0, 8);
        cone.translate(0.0, 0.5, 0.0);
        (
            Arc::new(cone),
            Arc::new(Material::standard(0x5c8c35, 0.0, 0.9)),
        )
    });
    let mut group = Node::group("");
    group.position = DVec3::new(def.x, 0.0, def.z);
    let mut crown = Node::mesh(mesh.clone(), material.clone());
    crown.scale = DVec3::new(def.w, def.h, def.d);
    if let Some(drawable) = &mut crown.drawable {
        drawable.cast_shadow = true;
        drawable.receive_shadow = true;
    }
    group.children.push(crown);
    group
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_matches_typescript_mulberry32() {
        // new Random(8274): first draws printed by src/game/math.ts.
        let mut rng = Random::new(8274.0);
        let draws: Vec<f64> = (0..3).map(|_| rng.next()).collect();
        assert_eq!(
            draws,
            [0.4227251335978508, 0.626320224488154, 0.7331137533765286]
        );
        // Past 2^53 the double state rounds like the JavaScript number.
        let mut long = Random::new(7391.0);
        for _ in 0..9_000_000 {
            long.next();
        }
        assert_eq!(long.state, 16_484_092_312_925_152.0);
        assert_eq!(long.next(), 0.20891576213762164);
    }
}
