//! Port of `quarry-scree.ts`: collapsed sediment fans at the foot of the first
//! terrace and their angular rubble.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::DVec3;

use crate::geometry::Mesh;
use crate::scene::{Material, Node};

use super::model_primitives::{shadow_receiver, shadowed};
use super::quarry_benches::ScreeSpot;
use super::quarry_surfaces::{RubbleStone, sandstone_material, scatter_mesh};
use super::quarry_terrain::{plain_soil_colors, quarry_ground_drop, soil_uvs};
use crate::sim::math::Random;

/// `screePoint(spot, u, t)`: a fan of sediment with scalloped toes and sides buried
/// below the apron. The high back extends into the quarry cut so it cannot expose
/// a thin lip.
fn scree_point(spot: &ScreeSpot, u: f64, t: f64) -> DVec3 {
    let side = 0.0f64.max(1.0 - u * u);
    let toe = 0.4 + 1.3 * (0.5 + 0.5 * (u * 11.0 + spot.seed).sin()) + 1.5 * u * u;
    let crest = 0.87 + 0.13 * (u * 7.0 + spot.seed).sin();
    let relief = (u * 23.0 + t * 17.0 + spot.seed).sin() * 0.14 * (t * PI).sin();
    DVec3::new(
        (u * spot.length * (1.0 - t * 0.18)) / 2.0,
        side * (t.powf(1.2) * spot.height * crest + relief) - 0.16,
        toe + t * (spot.depth - toe),
    )
}

/// `quarryScreeGeometry(spot)`: a closed volume, including a buried underside.
pub fn quarry_scree_geometry(spot: &ScreeSpot) -> Mesh {
    let across = (spot.length / 0.8).ceil() as u32;
    let rows = 12u32;
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for row in 0..=rows {
        for col in 0..=across {
            let p = scree_point(
                spot,
                (f64::from(col) / f64::from(across)) * 2.0 - 1.0,
                f64::from(row) / f64::from(rows),
            );
            positions.extend([p.x, p.y, p.z]);
            if row < rows && col < across {
                let a = row * (across + 1) + col;
                let b = a + across + 1;
                indices.extend([a, b, a + 1, b, b + 1, a + 1]);
            }
        }
    }
    let rim: Vec<u32> = (0..across)
        .chain((0..rows).map(|i| i * (across + 1) + across))
        .chain((0..across).map(|i| rows * (across + 1) + across - i))
        .chain((0..rows).map(|i| (rows - i) * (across + 1)))
        .collect();
    let bottom = (positions.len() / 3) as u32;
    for &index in &rim {
        let at = index as usize * 3;
        positions.extend([positions[at], -0.5, positions[at + 2]]);
    }
    let center = (positions.len() / 3) as u32;
    positions.extend([0.0, -0.5, spot.depth / 2.0]);
    let count = rim.len() as u32;
    for i in 0..count {
        let next = (i + 1) % count;
        let (r, n) = (rim[i as usize], rim[next as usize]);
        indices.extend([r, n, bottom + next, r, bottom + next, bottom + i]);
        indices.extend([center, bottom + i, bottom + next]);
    }
    let mut geometry = Mesh::from_f64(&positions, &[], &[], Some(indices));
    geometry.compute_vertex_normals();
    geometry
}

/// Rubble stones per square metre of a scree fan.
const SCREE_STONES: f64 = 0.62;

/// `quarryScreeRubble(spot)`: angular fragments, sparse at the toe and coarser
/// toward the cut, merged into one mesh (position, normal, uv, color). All
/// randomness is local to scenery.
pub fn quarry_scree_rubble(spot: &ScreeSpot) -> Mesh {
    let mut rng = Random::new(spot.seed * 131.0 + 7.0);
    let count = (spot.length * spot.depth * SCREE_STONES).ceil() as usize;
    let stones: Vec<RubbleStone> = (0..count)
        .map(|i| {
            let u = rng.range(-0.97, 0.97);
            let t = rng.range(0.04, 0.98);
            let point = scree_point(spot, u, t);
            let size = rng.range(0.14, 0.48) * (0.65 + t * 1.1);
            let large = if i % 11 == 0 { 1.8 } else { 1.0 };
            RubbleStone {
                x: point.x,
                y: point.y.max(0.0) + size * 0.15,
                z: point.z,
                w: size * large,
                h: size * rng.range(0.45, 0.85),
                d: size * rng.range(0.65, 1.25) * large,
                rot_y: rng.range(-PI, PI),
                shade: rng.range(0.74, 1.08),
            }
        })
        .collect();
    scatter_mesh(&stones)
}

/// `quarryScree(spot, soil)`: the fan (soil-textured, sampling the ground bake at
/// world coordinates so its buried perimeter meets the apron) and its rubble.
pub fn quarry_scree(spot: &ScreeSpot, soil: &Arc<Material>) -> [Node; 2] {
    let mut mound = quarry_scree_geometry(spot);
    let mut rubble = quarry_scree_rubble(spot);
    let dip = quarry_ground_drop(spot.x, spot.z);
    for geometry in [&mut mound, &mut rubble] {
        geometry.rotate_y(spot.rot_y);
        geometry.translate(spot.x, 0.008 - dip, spot.z);
    }
    soil_uvs(&mut mound);
    plain_soil_colors(&mut mound);
    // Both share their batches with the rest of the apron: the fan receives
    // shadows like the floor, the rubble casts like the talus.
    [
        shadow_receiver(Arc::new(mound), soil.clone()),
        shadowed(Arc::new(rubble), sandstone_material()),
    ]
}
