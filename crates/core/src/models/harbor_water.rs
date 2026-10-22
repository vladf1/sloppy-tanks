//! Port of `harbor-water.ts`: one water surface under every harbor berth, so a
//! single reflection target covers them all, reaching past the fog so the open
//! sea meets the sky without an edge. Reused across rounds.
//!
//! Near the port the surface is cut into tiles and leaves out those under the
//! apron: the reflection is culled to the cells that visible water polygons can
//! sample, and a plane running on under the quay would make every low view
//! across the apron reflect the whole scene for water it never shows.

use crate::geometry::Mesh;
use crate::scene::Node;

use super::water_surface::{WaterKind, water_surface};

/// Height of the harbor water surface below the apron.
pub const HARBOR_WATER_HEIGHT: f64 = -2.2;
const HARBOR_WATER_SIZE: f64 = 640.0;
/// Edge of the tiles near the port and how far from the centre they reach,
/// metres; beyond, four broad quads run out to the open sea.
const WATER_TILE: f64 = 20.0;
const TILED_REACH: f64 = 180.0;
/// Half the square under the apron with no water: inside the quay wall's face,
/// so the surface still meets the wall.
const APRON_HOLE: f64 = 60.0;

/// The water surface in its local XY plane (`water_surface` turns it onto the
/// horizontal, local y becoming world -z), facing +z.
fn harbor_water_mesh() -> Mesh {
    let (mut positions, mut normals, mut uvs, mut indices) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut quad = |x0: f64, z0: f64, x1: f64, z1: f64| {
        let base = (positions.len() / 3) as u32;
        for (x, z) in [(x0, z1), (x1, z1), (x1, z0), (x0, z0)] {
            positions.extend([x, -z, 0.0]);
            normals.extend([0.0, 0.0, 1.0]);
            uvs.extend([x / HARBOR_WATER_SIZE + 0.5, -z / HARBOR_WATER_SIZE + 0.5]);
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    };
    let tiles = (2.0 * TILED_REACH / WATER_TILE) as i32;
    for i in 0..tiles {
        for j in 0..tiles {
            let x0 = -TILED_REACH + f64::from(i) * WATER_TILE;
            let z0 = -TILED_REACH + f64::from(j) * WATER_TILE;
            let (x1, z1) = (x0 + WATER_TILE, z0 + WATER_TILE);
            let under_apron =
                x0 >= -APRON_HOLE && x1 <= APRON_HOLE && z0 >= -APRON_HOLE && z1 <= APRON_HOLE;
            if !under_apron {
                quad(x0, z0, x1, z1);
            }
        }
    }
    let (reach, edge) = (TILED_REACH, HARBOR_WATER_SIZE / 2.0);
    quad(-edge, -edge, -reach, edge);
    quad(reach, -edge, edge, edge);
    quad(-reach, -edge, reach, -reach);
    quad(-reach, reach, reach, edge);
    Mesh::from_f64(&positions, &normals, &uvs, Some(indices))
}

/// `new HarborWater().mesh`.
pub fn harbor_water() -> Node {
    water_surface(harbor_water_mesh(), WaterKind::Harbor, HARBOR_WATER_HEIGHT)
}
