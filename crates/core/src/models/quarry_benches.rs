//! Port of `quarry-benches.ts`: the blasted quarry faces, talus heaps along their
//! toes, the crushed-stone stockpile, scree collapse spots and the stacked sentinel
//! butte. Scenery only: the innermost face is beyond the arena and machinery apron.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::DVec3;

use crate::geometry::Mesh;
use crate::scene::Node;

use super::batching::batch;
use super::model_primitives::shadowed;
use super::quarry_surfaces::{sandstone_material, sandstone_rock};
use crate::sim::math::Random;

/// Fractions of the face height, toe to crest; (0.3, 0.34) and (0.62, 0.66) are
/// the narrow ledges left between blast lifts.
const FACE_ROWS: [f64; 11] = [0.0, 0.07, 0.22, 0.3, 0.34, 0.5, 0.62, 0.66, 0.8, 0.93, 1.0];
const LEDGES: [usize; 3] = [3, 5, 6];

/// `quarryBench(length, height, depth, seed)`: a continuous blasted face with a
/// safety berm along its crest and a shelf behind. Blocky relief, jittered lifts
/// and flat-shaded facets keep the long walls from reading as extruded bands.
pub fn quarry_bench(length: f64, height: f64, depth: f64, seed: f64) -> Node {
    let mut rng = Random::new(seed);
    let segments = (length / 2.2).ceil() as usize;
    // Each wall reads differently: overall lean plus long swells along its length.
    let lean = 1.0 + 0.1 * (seed * 2.3).sin() + 0.06 * (seed * 5.1).sin();
    let rows = FACE_ROWS.len() + 3;
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();
    for i in 0..=segments {
        let edge = i == 0 || i == segments;
        let x = (i as f64 / segments as f64 - 0.5) * length
            + if edge { 0.0 } else { rng.range(-0.55, 0.55) };
        let fracture = (x * 0.13 + seed).sin() * 0.5 + rng.range(-0.3, 0.3);
        let crown = 0.74
            + 0.2 * (x * 0.05 + seed * 1.7).sin()
            + 0.1 * (x * 0.13 + seed).sin()
            + rng.range(-0.08, 0.08);
        let top = height * crown * lean;
        let shelf_inset = -0.18 + 0.1 * (x * 0.07 + seed * 0.9).sin();
        // Blast blocks stand proud or break back as coherent columns.
        let block = (x * 0.83 + seed * 3.1).sin() * 0.35 + rng.range(-0.2, 0.2);
        let crest = 2.8 + fracture;
        for ring in 0..rows {
            let (y, z);
            if let Some(&t) = FACE_ROWS.get(ring) {
                let inner = ring > 0 && ring < FACE_ROWS.len() - 1;
                y = t * top
                    + if inner {
                        rng.range(-0.035, 0.035) * top
                    } else {
                        0.0
                    };
                z = t * 2.8
                    + fracture
                    + if LEDGES.contains(&ring) {
                        shelf_inset
                    } else {
                        0.0
                    }
                    + if inner {
                        block * (t * PI).sin() + rng.range(-0.22, 0.22)
                    } else {
                        0.0
                    };
            } else if ring == FACE_ROWS.len() {
                // Windrowed safety berm pushed up just behind the crest.
                y = top + 0.5 + rng.range(-0.12, 0.18);
                z = crest + 1.4 + rng.range(-0.25, 0.25);
            } else if ring == FACE_ROWS.len() + 1 {
                y = top + 0.04;
                z = crest + 2.8;
            } else {
                y = top;
                z = depth;
            }
            positions.extend([x, y, z]);
            let flat = ring >= FACE_ROWS.len() - 1;
            uvs.extend([x / 5.0, if flat { z / 5.0 } else { y / 5.0 }]);
            // Warm sedimentary banding runs with height; ledge undersides sit in shade.
            let band = 1.0 + 0.05 * (y * 1.15 + seed * 2.0).sin();
            let shade = (if ring == 2 || ring == 4 || ring == 7 {
                0.74
            } else {
                0.95
            }) + rng.range(-0.05, 0.07);
            colors.push([
                (shade * band) as f32,
                (shade * (1.0 + (band - 1.0) * 0.6)) as f32,
                (shade * (1.0 + (band - 1.0) * 0.2)) as f32,
            ]);
            if i < segments && ring < rows - 1 {
                let a = (i * rows + ring) as u32;
                let b = a + rows as u32;
                indices.extend([a, a + 1, b, b, a + 1, b + 1]);
            }
        }
    }
    let mut geometry = Mesh::from_f64(&positions, &[], &uvs, Some(indices));
    geometry.colors = colors;
    // Broken flat normals make the vertical blast fractures read in grazing light.
    let mut fractured = geometry.to_non_indexed();
    fractured.compute_vertex_normals();
    shadowed(Arc::new(fractured), sandstone_material())
}

/// `quarryTalusPoint(x, t, seed)`: talus heaped along a wall toe, in the wall's
/// local frame: x along the face, z toward the face (the rear buries itself 1.2 m
/// inside), y up from its base. Reach and height swell and pinch along the wall,
/// near the angle of repose.
pub fn quarry_talus_point(x: f64, t: f64, seed: f64) -> DVec3 {
    let swell = 0.5 + 0.3 * (x * 0.071 + seed).sin() + 0.2 * (x * 0.23 + seed * 1.9).sin();
    let reach = 1.6 + 3.2 * swell;
    let z = -reach + t * (reach + 1.2);
    // Concave toe steepening toward the face, with lumpy slump.
    let lump = (x * 1.3 + t * 5.1 + seed).sin() * (x * 0.47 - t * 3.3).sin() * 0.16;
    let y = reach * 0.68 * t.powf(1.45) + lump * (t * PI).sin() - 0.2 * (1.0 - t);
    DVec3::new(x, y, z)
}

/// `quarryTalusGeometry(x0, x1, seed)`: a continuous talus strip spanning x0..x1.
pub fn quarry_talus_geometry(x0: f64, x1: f64, seed: f64) -> Mesh {
    let across = ((x1 - x0) / 1.1).ceil() as u32;
    let rows = 6u32;
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for row in 0..=rows {
        for col in 0..=across {
            let x = x0 + ((x1 - x0) * f64::from(col)) / f64::from(across);
            let p = quarry_talus_point(x, f64::from(row) / f64::from(rows), seed);
            positions.extend([p.x, p.y, p.z]);
            if row < rows && col < across {
                let a = row * (across + 1) + col;
                let b = a + across + 1;
                indices.extend([a, b, a + 1, b, b + 1, a + 1]);
            }
        }
    }
    let mut geometry = Mesh::from_f64(&positions, &[], &[], Some(indices));
    geometry.compute_vertex_normals();
    geometry
}

/// A talus strip along one of the lowest cuts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TalusStrip {
    /// Toe of the lowest cut; the strip shares that wall's orientation.
    pub x: f64,
    pub z: f64,
    pub rot_y: f64,
    /// Span along the wall, in its local x.
    pub x0: f64,
    pub x1: f64,
    pub seed: f64,
}

/// `quarryTalusStrips()`: talus along the four lowest cuts, matching their toes.
/// The east strip stops short of the haul ramp; corners tuck into the flank walls.
pub fn quarry_talus_strips() -> [TalusStrip; 4] {
    let strip = |x, z, rot_y, x0, x1, seed| TalusStrip {
        x,
        z,
        rot_y,
        x0,
        x1,
        seed,
    };
    [
        strip(0.0, -77.0, PI, -80.0, 80.0, 5.0),
        strip(0.0, 78.0, 0.0, -80.0, 80.0, 9.0),
        strip(-80.0, 0.0, -PI / 2.0, -77.0, 78.0, 13.0),
        strip(78.0, 0.0, PI / 2.0, -22.0, 77.0, 17.0),
    ]
}

/// The crushed-stone stockpile under the screening conveyor's head drum.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StockpileSpot {
    pub x: f64,
    pub z: f64,
    pub radius: f64,
    pub height: f64,
    pub seed: f64,
}

pub fn quarry_stockpile_spot() -> StockpileSpot {
    StockpileSpot {
        x: -41.0,
        z: -71.5,
        radius: 6.4,
        height: 4.4,
        seed: 3.0,
    }
}

/// `quarryStockpileReach(spot, angle)`: radius of the pile's toe at a bearing; the
/// long axis follows the conveyor.
pub fn quarry_stockpile_reach(spot: &StockpileSpot, angle: f64) -> f64 {
    let c = angle.cos();
    spot.radius
        * (1.0
            + 0.14 * (c * c)
            + 0.07 * (angle * 3.0 + spot.seed).sin()
            + 0.04 * (angle * 7.0).sin())
}

/// `quarryStockpileGeometry(spot)`: a conical stockpile at the angle of repose,
/// slumped and lumpy, its base buried below the apron; centred on its apex axis.
pub fn quarry_stockpile_geometry(spot: &StockpileSpot) -> Mesh {
    let rings = 9u32;
    let segments = 36u32;
    let mut positions = vec![0.0, spot.height, 0.0];
    let mut indices = Vec::new();
    for ring in 1..=rings {
        let t = f64::from(ring) / f64::from(rings);
        for side in 0..segments {
            let angle = (f64::from(side) / f64::from(segments)) * PI * 2.0;
            let reach = quarry_stockpile_reach(spot, angle) * t;
            // A rounded crest where the stream lands, then a straight repose slope.
            let lump = (angle * 11.0 + t * 9.0 + spot.seed).sin() * 0.09 * t;
            let y = if ring == rings {
                -0.25
            } else {
                spot.height * (1.0 - t.powf(1.08)) + lump
            };
            positions.extend([angle.cos() * reach, y, angle.sin() * reach]);
            let a = 1 + (ring - 1) * segments + side;
            let b = 1 + (ring - 1) * segments + (side + 1) % segments;
            if ring == 1 {
                indices.extend([0, b, a]);
            } else {
                indices.extend([a - segments, b - segments, a, b - segments, b, a]);
            }
        }
    }
    let mut geometry = Mesh::from_f64(&positions, &[], &[], Some(indices));
    geometry.compute_vertex_normals();
    geometry
}

/// A localized collapse interrupting the first terrace.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreeSpot {
    pub x: f64,
    pub z: f64,
    pub rot_y: f64,
    pub length: f64,
    pub height: f64,
    pub depth: f64,
    pub seed: f64,
}

/// `quarryScreeSpots()`: every footprint stays on the machinery apron, clear of
/// the playable boundary and the haul loop.
pub fn quarry_scree_spots() -> [ScreeSpot; 6] {
    let spot = |x, z, rot_y, length, height, depth, seed| ScreeSpot {
        x,
        z,
        rot_y,
        length,
        height,
        depth,
        seed,
    };
    [
        spot(-32.0, 70.0, 0.0, 26.0, 3.6, 12.0, 11.0),
        spot(16.0, 70.0, 0.0, 20.0, 3.1, 12.0, 23.0),
        spot(-6.0, -70.0, PI, 24.0, 3.4, 11.0, 37.0),
        spot(36.0, -70.0, PI, 18.0, 3.0, 11.0, 49.0),
        spot(-70.0, 6.0, -PI / 2.0, 22.0, 3.3, 14.0, 61.0),
        spot(70.0, -12.0, PI / 2.0, 20.0, 3.2, 12.0, 73.0),
    ]
}

/// Where the sentinel butte stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButteSpot {
    pub x: f64,
    pub z: f64,
    pub base_y: f64,
    pub scale: f64,
    pub rot_y: f64,
}

/// `quarryButteSpot()`: a lone layered sentinel on the north apron, clear of the
/// boundary, the conveyor, the excavator swing and both northern scree collapses.
pub fn quarry_butte_spot() -> ButteSpot {
    ButteSpot {
        x: 16.0,
        z: -68.0,
        base_y: -1.8,
        scale: 1.0,
        rot_y: 0.15,
    }
}

/// w, h, d, dx, dy, dz per stacked slab.
const BUTTE_SLABS: [[f64; 6]; 6] = [
    [15.0, 3.2, 11.0, 0.0, 1.6, 0.0],
    [12.5, 2.8, 9.5, 0.9, 4.6, -0.5],
    [10.0, 2.6, 8.0, -0.7, 7.3, 0.6],
    [7.6, 2.4, 6.2, 0.5, 9.9, -0.4],
    [5.0, 2.2, 4.4, -0.4, 12.2, 0.3],
    [3.4, 1.8, 3.0, 0.3, 14.0, -0.2],
];

/// `quarryButte(scale, rotY)` (`quarry-sentinel-butte`): stacked offset slabs with an
/// eroded cap, merged into one batch so the landmark costs one draw.
pub fn quarry_butte(scale: f64, rot_y: f64) -> Node {
    let mut group = Node::group("quarry-sentinel-butte");
    for (i, [w, h, d, dx, dy, dz]) in BUTTE_SLABS.into_iter().enumerate() {
        let mut rock = sandstone_rock(w * scale, h * scale, d * scale, 20 + i as u32);
        rock.position = DVec3::new(dx * scale, dy * scale, dz * scale);
        rock.set_rotation_euler(0.0, rot_y + i as f64 * 0.22, 0.0);
        group.children.push(rock);
    }
    batch(&mut group);
    group
}
