//! Port of `quarry-benches.ts`: the blasted quarry faces, talus heaps along their
//! toes, the crushed-stone stockpile, scree collapse spots and the stacked sentinel
//! butte. Scenery only: the innermost face is beyond the arena and machinery apron.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::DVec3;

use crate::geometry::Mesh;
use crate::scene::Node;

use super::batching::batch;
use super::model_primitives::{shadow_receiver, shadowed};
use super::quarry_surfaces::{
    Bed, RockSetting, bed_rows, mesh_triangles, sandstone_material, sandstone_rock,
    sandstone_rock_mesh, sandstone_rock_toe, surface_height,
};
use crate::sim::math::Random;

/// Normals crease above this angle: ledges stay sharp, a bed's face stays soft.
const BENCH_CREASE: f64 = PI / 4.0;

/// Bed thickness range of the quarry faces, in metres.
const BENCH_BED: (f64, f64) = (1.5, 2.6);
/// Width of one column of a bench face, in metres.
const BENCH_COLUMN: f64 = 1.35;
/// How far a bench bed juts out or recesses, in metres.
const BENCH_RELIEF: f64 = 0.42;
/// Columns per culled chunk of a face (about 35 m).
const BENCH_CHUNK: usize = 26;
/// Mean spacing of the buttresses and gullies along a face, in metres.
const GULLY_SPACING: f64 = 11.0;
/// Chance that a run of crest columns has broken away, and how far it drops,
/// in metres: the skyline steps down in blocks instead of running ruler-straight.
const CREST_BREAK: f64 = 0.4;
const CREST_DROP: (f64, f64) = (0.5, 1.9);
/// Passes of easing along the crest's safety berm.
const BERM_SMOOTHING: usize = 3;
/// How far a face's lowest row sinks below its base into the floor or shelf
/// it stands on, in metres: deep enough to stay under the lower bench's
/// shelf where that shelf's crest has broken low.
const FOOT_BURIAL: f64 = 2.0;

/// `quarryBench(length, height, depth, seed)`: a continuous blasted face with a
/// safety berm along its crest and a shelf behind. The face is built bed by bed
/// like the cover rocks (`sandstone_rock`): hard beds jut out as ledges over
/// undercuts, soft beds recess, vertical joints and fallen blocks break the
/// courses. `uv.x` is minus the bed's palette entry (the shader calms the
/// backdrop's colors), `uv.y` the wear. Built in chunks the renderer culls apart.
pub fn quarry_bench(length: f64, height: f64, depth: f64, seed: f64) -> Node {
    let mut rng = Random::new(seed);
    let segments = (length / BENCH_COLUMN).ceil() as usize;
    // Each wall reads differently: overall lean plus long swells along its length.
    let lean = 1.0 + 0.1 * (seed * 2.3).sin() + 0.06 * (seed * 5.1).sin();
    // Buttresses and gullies at irregular spacing: a smooth random profile along
    // the face, so no stretch of it repeats another.
    let knots: Vec<f64> = (0..=(length / GULLY_SPACING).ceil() as usize + 1)
        .map(|_| rng.range(-1.0, 1.0))
        .collect();
    let gully_at = |x: f64| {
        let t = ((x + length / 2.0) / GULLY_SPACING).clamp(0.0, (knots.len() - 2) as f64);
        let i = (t.floor() as usize).min(knots.len() - 2);
        let f = t - i as f64;
        let f = f * f * (3.0 - 2.0 * f);
        knots[i] + (knots[i + 1] - knots[i]) * f
    };
    struct Column {
        x: f64,
        top: f64,
        fracture: f64,
        block: f64,
        crest: f64,
        berm: f64,
        joint: bool,
    }
    let mut columns: Vec<Column> = (0..=segments)
        .map(|i| {
            let edge = i == 0 || i == segments;
            let x = (i as f64 / segments as f64 - 0.5) * length
                + if edge {
                    0.0
                } else {
                    rng.range(-0.4, 0.4) * BENCH_COLUMN
                };
            let fracture = (x * 0.13 + seed).sin() * 0.5 + rng.range(-0.3, 0.3);
            let crown = 0.74
                + 0.2 * (x * 0.05 + seed * 1.7).sin()
                + 0.1 * (x * 0.13 + seed).sin()
                + rng.range(-0.03, 0.03);
            // Blast blocks stand proud or break back as coherent columns, on
            // buttresses and gullies metres deep that model the face at range.
            let gully = (x * 0.11 + seed).sin() * 0.95
                + (x * 0.29 + seed * 1.3).sin() * 0.5
                + gully_at(x) * 0.45;
            let block = (x * 0.83 + seed * 3.1).sin() * 0.35 + rng.range(-0.2, 0.2) + gully;
            Column {
                x,
                top: height * crown * lean * (1.0 - 0.05 * gully.max(0.0)),
                fracture,
                block,
                crest: 2.8 + fracture,
                berm: rng.range(-0.12, 0.18),
                joint: !edge && rng.next() < 0.2,
            }
        })
        .collect();
    // Blast-broken crest: runs of columns stand lower as a block, the edges of
    // the break left square.
    let mut i = 1;
    while i < segments {
        let run = 2 + (rng.next() * 5.0) as usize;
        if rng.next() < CREST_BREAK {
            let drop = rng.range(CREST_DROP.0, CREST_DROP.1);
            for column in &mut columns[i..(i + run).min(segments)] {
                column.top = (column.top - drop).max(height * 0.45);
            }
        }
        i += run;
    }
    // The safety berm was pushed up by a dozer running along the crest: one
    // continuous windrow, its height and line easing from column to column
    // instead of a row of tent-shaped peaks.
    for _ in 0..BERM_SMOOTHING {
        let eased: Vec<(f64, f64)> = (0..columns.len())
            .map(|i| {
                let before = &columns[i.saturating_sub(1)];
                let after = &columns[(i + 1).min(columns.len() - 1)];
                let here = &columns[i];
                (
                    (before.berm + here.berm * 2.0 + after.berm) / 4.0,
                    (before.crest + here.crest * 2.0 + after.crest) / 4.0,
                )
            })
            .collect();
        for (column, (berm, crest)) in columns.iter_mut().zip(eased) {
            column.berm = berm;
            column.crest = crest;
        }
    }
    let peak = columns.iter().map(|c| c.top).fold(0.0, f64::max);
    // Bench-wide beds, gently undulating along the face.
    let mut beds: Vec<(f64, f64, Bed)> = Vec::new();
    let mut base = 0.0;
    let mut last = Bed::Soft;
    while base < peak {
        let thickness = rng.range(BENCH_BED.0, BENCH_BED.1);
        let roll = rng.next();
        let bed = if roll < 0.38 && last != Bed::Hard {
            Bed::Hard
        } else if roll < 0.68 {
            Bed::Medium
        } else {
            Bed::Soft
        };
        beds.push((base, thickness, bed));
        last = bed;
        base += thickness;
    }
    let layer_base = (seed * 3.7).round().abs() % 50.0 + 100.0;
    let shades: Vec<f64> = beds.iter().map(|_| rng.range(0.9, 1.06)).collect();
    let hollows: Vec<f64> = (0..beds.len() * (segments + 1))
        .map(|_| {
            if rng.next() < 0.07 {
                rng.range(0.6, 1.4)
            } else {
                0.0
            }
        })
        .collect();
    // Blast blocks: each bed breaks into runs of a few columns that stand proud
    // or sit back as a unit, so courses read as stacked blocks, not extrusions.
    let stride = segments + 1;
    let mut blocks = vec![0.0; beds.len() * stride];
    for k in 0..beds.len() {
        let mut i = 0;
        while i < stride {
            let run = 2 + (rng.next() * 4.0) as usize;
            let offset = rng.range(-0.6, 0.6);
            for b in &mut blocks[k * stride + i..k * stride + (i + run).min(stride)] {
                *b = offset;
            }
            i += run;
        }
    }
    // Vertical joints cut through a span of beds, not the whole face.
    let joint_beds: Vec<(usize, usize)> = columns
        .iter()
        .map(|_| {
            let low = (rng.next() * beds.len() as f64 * 0.6) as usize;
            (low, low + 2 + (rng.next() * beds.len() as f64) as usize)
        })
        .collect();
    // Rows of each column: position, bed, wear.
    let row_count = beds.len() * 4 + 3;
    let mut points = Vec::with_capacity((segments + 1) * row_count);
    let mut wears = Vec::with_capacity((segments + 1) * row_count);
    let mut row_beds = Vec::with_capacity(row_count);
    let mut row_tops = Vec::with_capacity(row_count);
    for (k, &(_, _, bed)) in beds.iter().enumerate() {
        for (u, _, _) in bed_rows(bed) {
            row_beds.push(k);
            row_tops.push(u == 1.0);
        }
    }
    for (i, column) in columns.iter().enumerate() {
        let undulate = 0.35 * (column.x * 0.045 + seed).sin() + 0.15 * (column.x * 0.17).sin();
        for (k, &(bottom, thickness, bed)) in beds.iter().enumerate() {
            let hollow = hollows[k * stride + i]
                + if i > 0 {
                    hollows[k * stride + i - 1] * 0.5
                } else {
                    0.0
                };
            let block = blocks[k * stride + i];
            let (joint_low, joint_high) = joint_beds[i];
            let jointed = column.joint && k >= joint_low && k < joint_high;
            for (u, offset, wear) in bed_rows(bed) {
                // The face's foot is buried in the floor below it: the beds'
                // undulation must never lift it and open a slot of sky under
                // the cliff.
                let raw = if k == 0 && u == 0.0 {
                    -FOOT_BURIAL
                } else {
                    bottom + u * thickness + undulate
                };
                let y = raw.clamp(-FOOT_BURIAL, column.top);
                let clamped = raw >= column.top;
                let t = y.max(0.0) / column.top;
                let (mut push, mut worn) = if clamped || raw <= 0.0 {
                    (0.0, 0.8 * f64::from(u8::from(clamped)))
                } else {
                    (
                        offset - hollow + block,
                        wear - hollow * 0.6 + block.max(0.0) * 0.5,
                    )
                };
                if jointed && !clamped {
                    push -= 1.1;
                    worn = -1.0;
                }
                let z =
                    t * 2.8 + column.fracture + column.block * (t * PI).sin() - push * BENCH_RELIEF;
                points.push(DVec3::new(column.x, y, z));
                wears.push(worn.clamp(-1.0, 1.0));
            }
        }
        // Windrowed safety berm pushed up just behind the crest, then the shelf.
        for (y, z, wear) in [
            (
                column.top + 0.35 + column.berm * 0.7,
                column.crest + 1.4,
                0.0,
            ),
            (column.top + 0.04, column.crest + 2.8, 0.0),
            (column.top, depth, 0.0),
        ] {
            points.push(DVec3::new(column.x, y, z));
            wears.push(wear);
        }
    }
    let shelf_bed = beds.len() - 1;
    for _ in 0..3 {
        row_beds.push(shelf_bed);
        row_tops.push(false);
    }
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut colors = Vec::new();
    for i in 0..segments {
        for r in 0..row_count - 1 {
            // A ledge belongs to the bed it tops; beds own their own faces.
            let bed = if row_tops[r] {
                row_beds[r]
            } else {
                row_beds[r + 1]
            };
            let layer = -(layer_base + bed as f64);
            let a = i * row_count + r;
            let b = a + row_count;
            for v in [a, a + 1, b, b, a + 1, b + 1] {
                let p = points[v];
                positions.extend([p.x, p.y, p.z]);
                uvs.extend([layer, wears[v]]);
                let band = 1.0 + 0.04 * (p.y * 1.15 + seed * 2.0).sin();
                let shade = shades[bed.min(shades.len() - 1)] * band;
                colors.push([(shade * 1.01) as f32, shade as f32, (shade * 0.99) as f32]);
            }
        }
    }
    let mut geometry = Mesh::from_f64(&positions, &[], &uvs, None);
    geometry.colors = colors;
    let geometry = geometry.to_creased_normals(BENCH_CREASE);
    // Split into chunks of whole columns so the renderer culls them per cell.
    let per_column = (row_count - 1) * 6;
    let mut group = Node::group("");
    for chunk in (0..segments).step_by(BENCH_CHUNK) {
        let from = chunk * per_column;
        let to = (chunk + BENCH_CHUNK).min(segments) * per_column;
        let part = Mesh {
            positions: geometry.positions[from..to].to_vec(),
            normals: geometry.normals[from..to].to_vec(),
            uvs: geometry.uvs[from..to].to_vec(),
            colors: geometry.colors[from..to].to_vec(),
            ..Mesh::default()
        };
        group
            .children
            .push(shadowed(Arc::new(part), sandstone_material()));
    }
    group
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

/// w, h, d, dx, dz per stacked slab, bottom to top. The bottom slab stands on the
/// apron; each slab above is seated on the one below ([`seat_height`]).
const BUTTE_SLABS: [[f64; 5]; 6] = [
    [15.0, 4.5, 11.0, 0.0, 0.0],
    [12.5, 3.9, 9.5, 0.9, -0.5],
    [10.0, 3.6, 8.0, -0.7, 0.6],
    [7.6, 3.4, 6.2, 0.5, -0.4],
    [5.0, 3.1, 4.4, -0.4, 0.3],
    [3.4, 2.5, 3.0, 0.3, -0.2],
];
/// How far each butte slab's toe sinks into the slab under it, in metres.
const BUTTE_SEAT: f64 = 0.08;
/// Points sampled along each side of a slab's toe when seating it.
const SEAT_SAMPLES: usize = 4;

/// `quarryButte(scale, rotY)` (`quarry-sentinel-butte`) at `quarryButteSpot()`:
/// stacked offset slabs with an eroded cap, merged into one batch so the landmark
/// costs one draw. A lone layered sentinel on the north apron, clear of the
/// boundary, the conveyor, the excavator swing and both northern scree collapses.
pub fn quarry_butte() -> Node {
    let mut group = Node::group("quarry-sentinel-butte");
    let mut below: Vec<[DVec3; 3]> = Vec::new();
    for (i, [w, h, d, dx, dz]) in BUTTE_SLABS.into_iter().enumerate() {
        let variant = 20 + i as u32;
        // Only the bottom slab stands on the ground and sheds scree onto it.
        let setting = if i == 0 {
            RockSetting::Ground
        } else {
            RockSetting::Stacked
        };
        let mut rock = sandstone_rock(w, h, d, variant, setting);
        rock.position = DVec3::new(dx, 0.0, dz);
        rock.set_rotation_euler(0.0, 0.15 + i as f64 * 0.22, 0.0);
        if i > 0 {
            rock.position.y = seat_height(&below, &rock, w, h, d, variant);
        }
        let matrix = rock.local_matrix();
        // The face without the bottom slab's scree, which the toe above never reaches.
        below = mesh_triangles(&sandstone_rock_mesh(w, h, d, variant, RockSetting::Stacked))
            .into_iter()
            .map(|corners| corners.map(|p| matrix.transform_point3(p)))
            .collect();
        group.children.push(rock);
    }
    batch(&mut group);
    group.position = DVec3::new(16.0, -1.8, -68.0);
    group
}

/// The height to place `slab` (already turned and moved across) so that its
/// undercut toe sinks [`BUTTE_SEAT`] into the rock `below` (world triangles) where
/// that rock is lowest under it: no gap shows between them. The few toe points
/// beyond the rock below overhang it, like a caprock ledge.
fn seat_height(below: &[[DVec3; 3]], slab: &Node, w: f64, h: f64, d: f64, variant: u32) -> f64 {
    let toe = sandstone_rock_toe(w, h, d, variant, RockSetting::Stacked);
    let matrix = slab.local_matrix();
    let mut lowest = f64::INFINITY;
    for (s, &a) in toe.iter().enumerate() {
        let b = toe[(s + 1) % toe.len()];
        for k in 0..SEAT_SAMPLES {
            let p = matrix.transform_point3(a + (b - a) * (k as f64 / SEAT_SAMPLES as f64));
            if let Some(top) = surface_height(below, p.x, p.z) {
                lowest = lowest.min(top - p.y);
            }
        }
    }
    lowest - BUTTE_SEAT
}

/// Sides of a distant mesa's outline.
const MESA_SIDES: usize = 26;
/// Bed bands per tier of a distant mesa.
const MESA_BEDS: usize = 3;

/// One distant mesa beyond the pit rim, in world metres.
struct Mesa {
    angle: f64,
    distance: f64,
    radius: f64,
    height: f64,
    base: f64,
}

/// The ring of mesas standing beyond the quarry rim: a far silhouette layer
/// above the benches that gives the backdrop depth. Low-polygon stepped tiers in
/// the backdrop's calm strata, receiving shadows only (they stand far outside the
/// shadow map). One mesh, one draw.
pub fn quarry_mesas() -> Node {
    let mut rng = Random::new(70313.0);
    let mut mesas = Vec::new();
    let mut angle = rng.range(0.0, 0.3);
    while angle < PI * 2.0 - 0.2 {
        mesas.push(Mesa {
            angle,
            distance: rng.range(192.0, 240.0),
            radius: rng.range(18.0, 38.0),
            height: rng.range(14.0, 30.0),
            base: rng.range(8.0, 14.0),
        });
        angle += rng.range(0.32, 0.55);
    }
    let mut positions: Vec<f64> = Vec::new();
    let mut uvs: Vec<f64> = Vec::new();
    for (m, mesa) in mesas.iter().enumerate() {
        let centre = DVec3::new(
            mesa.angle.cos() * mesa.distance,
            mesa.base,
            mesa.angle.sin() * mesa.distance,
        );
        let phase = rng.range(0.0, PI * 2.0);
        let stretch = rng.range(1.0, 1.8);
        let outline: Vec<DVec3> = (0..MESA_SIDES)
            .map(|s| {
                let a = s as f64 / MESA_SIDES as f64 * PI * 2.0;
                let r = mesa.radius
                    * (1.0 + 0.14 * (a * 3.0 + phase).sin() + 0.08 * (a * 5.0 - phase).sin())
                    * rng.range(0.92, 1.08);
                // Long axis tangent to the ring, like eroded rim remnants.
                let tangent = mesa.angle + PI / 2.0;
                let (along, across) = (a.cos() * r * stretch, a.sin() * r);
                DVec3::new(
                    along * tangent.cos() - across * tangent.sin(),
                    0.0,
                    along * tangent.sin() + across * tangent.cos(),
                )
            })
            .collect();
        // Rings bottom to top: outline scale, height, palette layer, wear and
        // the shift of the tier's centre. Each tier is a caprock cliff over a
        // concave talus slope, stepping back off-centre to the next.
        struct Ring {
            scale: f64,
            y: f64,
            layer: f64,
            wear: f64,
            shift: DVec3,
            /// How deep the cliff's gullies cut into this ring, as a fraction.
            gullies: f64,
            /// How far notches in the caprock lip drop this ring, in metres.
            notches: f64,
        }
        // Rain gullies notch the cliffs between buttresses, at the same bearings
        // from tier to tier so they run down the whole face.
        let gully_count = rng.range(5.0, 9.0).floor();
        let gully_phase = rng.range(0.0, PI * 2.0);
        // Caprock lips break back in notches between the gullies (a stream of
        // their own, so the ring's layout keeps its draws).
        let mut notch_rng = Random::new(70411.0 + m as f64 * 13.0);
        let notch_count = notch_rng.range(9.0, 14.0).floor();
        let notch_phase = notch_rng.range(0.0, PI * 2.0);
        let tiers = 1 + (rng.next() * 2.99) as usize;
        let layer_base = 300.0 + (m * 7) as f64;
        let mut shift = DVec3::ZERO;
        let mut scale = 1.0;
        let mut rings = vec![Ring {
            scale: 1.6,
            y: -10.0,
            layer: layer_base,
            wear: 0.0,
            shift,
            gullies: 0.0,
            notches: 0.0,
        }];
        let mut bottom = 0.0;
        for tier in 0..tiers {
            let top = if tier + 1 == tiers {
                mesa.height
            } else {
                bottom + (mesa.height - bottom) * rng.range(0.4, 0.6)
            };
            let span = top - bottom;
            let cliff = rng.range(0.4, 0.6);
            let layer = layer_base + (tier * (MESA_BEDS + 1)) as f64;
            // Talus: steepening up to the foot of the cliff.
            for (t, shrink) in [(0.0, 1.0), (0.22, 0.88), (1.0 - cliff, 0.8)] {
                rings.push(Ring {
                    scale: scale * shrink,
                    y: bottom + span * t,
                    layer,
                    wear: if t == 0.0 { -0.5 } else { 0.0 },
                    shift,
                    gullies: if t == 0.0 { 0.0 } else { 0.08 },
                    notches: 0.0,
                });
            }
            scale *= 0.8;
            for bed in 1..=MESA_BEDS {
                let hard = bed == MESA_BEDS;
                rings.push(Ring {
                    scale: scale * if hard { 1.02 } else { rng.range(0.97, 1.0) },
                    y: bottom + span * (1.0 - cliff + cliff * bed as f64 / MESA_BEDS as f64),
                    layer: layer + bed as f64,
                    wear: if hard { 0.9 } else { 0.0 },
                    shift,
                    gullies: if hard { 0.1 } else { 0.16 },
                    notches: if hard { span * cliff * 0.3 } else { 0.0 },
                });
            }
            // A tread back to the next tier, off-centre.
            if tier + 1 < tiers {
                scale *= rng.range(0.5, 0.75);
                let room = mesa.radius * scale * 0.35;
                shift += DVec3::new(rng.range(-room, room), 0.0, rng.range(-room, room));
                rings.push(Ring {
                    scale,
                    y: top,
                    layer: layer + MESA_BEDS as f64,
                    wear: -0.4,
                    shift,
                    gullies: 0.05,
                    notches: span * cliff * 0.12,
                });
            }
            bottom = top;
        }
        let ring_points: Vec<Vec<DVec3>> = rings
            .iter()
            .map(|ring| {
                outline
                    .iter()
                    .enumerate()
                    .map(|(s, p)| {
                        let a = s as f64 / MESA_SIDES as f64 * PI * 2.0;
                        let notch = (0.5 + 0.5 * (a * gully_count + gully_phase).sin()).powi(4);
                        let scale = ring.scale * (1.0 - ring.gullies * notch);
                        let broken = (0.5 + 0.5 * (a * notch_count + notch_phase).sin()).powi(6);
                        let y = ring.y - ring.notches * broken;
                        centre + ring.shift + DVec3::new(p.x * scale, y, p.z * scale)
                    })
                    .collect()
            })
            .collect();
        for r in 0..rings.len() - 1 {
            // A band takes the palette entry of its upper ring (the bed it tops).
            let layer = -rings[r + 1].layer;
            for s in 0..MESA_SIDES {
                let n = (s + 1) % MESA_SIDES;
                for (p, wear) in [
                    (ring_points[r][s], rings[r].wear),
                    (ring_points[r + 1][s], rings[r + 1].wear),
                    (ring_points[r][n], rings[r].wear),
                    (ring_points[r][n], rings[r].wear),
                    (ring_points[r + 1][s], rings[r + 1].wear),
                    (ring_points[r + 1][n], rings[r + 1].wear),
                ] {
                    positions.extend([p.x, p.y, p.z]);
                    uvs.extend([layer, wear]);
                }
            }
        }
        let top = ring_points.last().expect("mesa rings");
        let last = rings.last().expect("mesa rings");
        // A flat, barely domed caprock top, not a cone.
        let crown = centre + last.shift + DVec3::new(0.0, mesa.height + 0.2, 0.0);
        let layer = -last.layer;
        for s in 0..MESA_SIDES {
            let n = (s + 1) % MESA_SIDES;
            for p in [crown, top[n], top[s]] {
                positions.extend([p.x, p.y, p.z]);
                uvs.extend([layer, 0.0]);
            }
        }
    }
    let mut geometry = Mesh::from_f64(&positions, &[], &uvs, None);
    geometry.colors = vec![[0.97, 0.96, 0.95]; geometry.positions.len()];
    let geometry = geometry.to_creased_normals(BENCH_CREASE);
    shadow_receiver(Arc::new(geometry), sandstone_material())
}
