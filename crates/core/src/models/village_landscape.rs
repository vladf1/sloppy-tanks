//! Port of `village-landscape.ts`: the Pine Valley terrain around the arena, its
//! creek, riverbank rocks, the background forest and the snowy mountain ridge.
//!
//! `creek_distance` and `valley_height` are also the ground queries of the meadow
//! and the footbridge approach's stepping stones.

use std::sync::{Arc, OnceLock};

use glam::DVec3;

use crate::geometry::{CatmullRomCurve3, Mesh, icosahedron_geometry, plane_geometry_segments};
use crate::scene::{Effect, Material, Node};

use super::batching::batch;
use super::effects_scenery::MEADOW_SWAY;
use super::ground_surfaces::ground_uvs;
use super::model_primitives::{adopt_children, put, shadow_receiver};
use super::tree_models::{TreeDetail, TreeShape, tree_model};
use super::water_surface::{WaterKind, water_surface};
use crate::geometry::math::{hex_to_linear, lerp, lerp_color, smoothstep};
use crate::sim::math::Random;

/// Height of the creek's mirror plane.
pub const CREEK_HEIGHT: f64 = -2.65;
/// Samples along the creek (`getPoints(160)`).
const CREEK_DIVISIONS: u32 = 160;

struct Creek {
    curve: CatmullRomCurve3,
    points: Vec<DVec3>,
    /// Per segment: start x, start z, dx, dz, 1 / squared length.
    segments: Vec<[f64; 5]>,
}

fn creek() -> &'static Creek {
    static CREEK: OnceLock<Creek> = OnceLock::new();
    CREEK.get_or_init(|| {
        let curve = CatmullRomCurve3::new(
            [
                [150.0, -107.0],
                [85.0, -86.0],
                [15.0, -82.0],
                [-57.0, -85.0],
                [-81.0, -64.0],
                [-81.0, -12.0],
                [-86.0, 48.0],
                [-110.0, 145.0],
            ]
            .map(|[x, z]| DVec3::new(x, CREEK_HEIGHT, z))
            .to_vec(),
        );
        let points = curve.points(CREEK_DIVISIONS);
        // Terrain, rocks and vegetation query this polyline tens of thousands of
        // times while the arena loads: keep segment data flat, compare squares.
        let segments = points
            .windows(2)
            .map(|pair| {
                let (a, b) = (pair[0], pair[1]);
                let (dx, dz) = (b.x - a.x, b.z - a.z);
                [a.x, a.z, dx, dz, 1.0 / (dx * dx + dz * dz)]
            })
            .collect();
        Creek {
            curve,
            points,
            segments,
        }
    })
}

/// `creekDistance(x, z)`: horizontal distance to the creek's centreline.
pub fn creek_distance(x: f64, z: f64) -> f64 {
    let mut nearest = f64::INFINITY;
    for &[sx, sz, dx, dz, inverse] in &creek().segments {
        let ax = x - sx;
        let az = z - sz;
        let t = 1.0f64.min(0.0f64.max((ax * dx + az * dz) * inverse));
        let ex = ax - dx * t;
        let ez = az - dz * t;
        nearest = nearest.min(ex * ex + ez * ez);
    }
    nearest.sqrt()
}

/// `valleyHeight(x, z)`: the valley floor, rising into hills beyond the arena and
/// dropping into the creek bed.
pub fn valley_height(x: f64, z: f64) -> f64 {
    valley_height_at(x, z, creek_distance(x, z))
}

/// `valleyHeight(x, z, river)` with the creek distance already known.
pub fn valley_height_at(x: f64, z: f64, river: f64) -> f64 {
    let edge = x.abs().max(z.abs());
    let hill = smoothstep(edge, 66.0, 145.0)
        * (4.0 + 3.0 * (x * 0.047 + z * 0.028).sin() + 2.0 * (z * 0.069 - x * 0.021).sin());
    let bank = smoothstep(river, 4.9, 9.0);
    lerp(-4.2, -0.85 + hill, bank)
}

/// `mountainRise(x, z)`: the northern range's height above the valley.
fn mountain_rise(x: f64, z: f64) -> f64 {
    if z > -112.0 {
        return 0.0;
    }
    let mut highest = 0.0f64;
    let mut total = 0.0;
    for [cx, cz, height, width] in [
        [-146.0, -164.0, 38.0, 31.0],
        [-94.0, -174.0, 51.0, 36.0],
        [-32.0, -161.0, 48.0, 32.0],
        [28.0, -181.0, 61.0, 41.0],
        [83.0, -162.0, 40.0, 30.0],
        [145.0, -179.0, 54.0, 39.0],
    ] {
        let dx = (x - cx) / width + (z * 0.09 + cx).sin() * 0.13;
        let dz = (z - cz) / (width * 0.8);
        let rise = height * (-1.4 * (dx * dx + dz * dz)).exp();
        highest = highest.max(rise);
        total += rise;
    }
    let crags = (x * 0.29 + z * 0.14).sin() * (z * 0.21 - x * 0.13).cos() * 2.4;
    0.0f64.max(highest * 0.8 + total * 0.2 + crags * smoothstep(highest, 2.0, 12.0))
        * smoothstep(-z, 112.0, 133.0)
}

/// Half size of the valley terrain square ([`village_landscape`]).
const VALLEY_HALF: f64 = 170.0;
/// Half size of the distant ranges around it, inside the camera's far plane.
const RANGES_HALF: f64 = 300.0;
/// Grid spacing of the distant ranges in metres.
const RANGES_CELL: f64 = 6.0;

/// A smooth lattice noise in [0, 1] for the distant ranges' shapes and colours.
fn lattice_noise(x: f64, z: f64) -> f64 {
    let hash = |ix: f64, iz: f64| {
        let h = (ix * 127.1 + iz * 311.7).sin() * 43758.5453;
        h - h.floor()
    };
    let (ix, iz) = (x.floor(), z.floor());
    let (fx, fz) = (x - ix, z - iz);
    let (ux, uz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let a = lerp(hash(ix, iz), hash(ix + 1.0, iz), ux);
    let b = lerp(hash(ix, iz + 1.0), hash(ix + 1.0, iz + 1.0), ux);
    lerp(a, b, uz)
}

/// Ridged peaks of the distant ranges: sharp crests, broad valleys.
fn ridges(x: f64, z: f64) -> f64 {
    let mut total = 0.0;
    let mut amplitude = 0.6;
    let mut frequency = 0.011;
    for _ in 0..4 {
        let n = lattice_noise(x * frequency + 13.7, z * frequency - 4.1);
        total += (1.0 - (n * 2.0 - 1.0).abs()).powi(2) * amplitude;
        amplitude *= 0.45;
        frequency *= 2.1;
    }
    total
}

/// `farRise(x, z)`: how far the distant ranges stand above the valley's hills,
/// rising from nothing at the valley's edge to snowy peaks.
fn far_rise(x: f64, z: f64) -> f64 {
    let edge = x.abs().max(z.abs());
    let rise = smoothstep(edge, VALLEY_HALF - 8.0, VALLEY_HALF + 70.0);
    rise * (10.0 + 62.0 * ridges(x, z)) + rise * rise * 8.0 * lattice_noise(x * 0.05, z * 0.05)
}

/// Slope colours of the mountains: forest below, scree and rock above it, snow
/// on the peaks, each band broken by noise so no contour line shows.
fn range_color(x: f64, y: f64, z: f64) -> [f64; 3] {
    let n = lattice_noise(x * 0.09, z * 0.09);
    let forest = lerp_color(hex_to_linear(0x3f5f3b), hex_to_linear(0x56774a), n);
    let meadow = hex_to_linear(0x6f9150);
    let rock = lerp_color(hex_to_linear(0x7d8682), hex_to_linear(0x9a9a8e), n);
    let snow = hex_to_linear(0xeef1ea);
    let mut color = lerp_color(meadow, forest, smoothstep(y, 4.0, 12.0));
    let treeline = 30.0 + n * 10.0;
    color = lerp_color(color, rock, smoothstep(y, treeline, treeline + 8.0));
    let snowline = 50.0 + (x * 0.031 + z * 0.027).sin() * 6.0 + n * 6.0;
    lerp_color(color, snow, smoothstep(y, snowline, snowline + 5.0))
}

/// The distant ranges ringing the valley: a grid out to [`RANGES_HALF`] that
/// continues the valley's hills into forested slopes and snowy peaks. Where it
/// lies under the valley terrain or the northern ridge it sinks below them, so
/// the nearer surfaces always win.
fn far_ranges() -> Mesh {
    let cells = (RANGES_HALF * 2.0 / RANGES_CELL) as usize;
    let row = cells + 1;
    let under = |x: f64, z: f64| {
        (x.abs() < VALLEY_HALF && z.abs() < VALLEY_HALF)
            || (x.abs() < 180.0 && (-207.0..-112.0).contains(&z))
    };
    let mut positions = Vec::with_capacity(row * row * 3);
    let mut colors = Vec::with_capacity(row * row);
    for j in 0..row {
        for i in 0..row {
            let x = -RANGES_HALF + i as f64 * RANGES_CELL;
            let z = -RANGES_HALF + j as f64 * RANGES_CELL;
            let mut y = valley_height(x, z) + mountain_rise(x, z) + far_rise(x, z);
            colors.push(range_color(x, y, z).map(|c| c as f32));
            if under(x, z) {
                y -= 1.5;
            }
            positions.extend([x, y, z]);
        }
    }
    let inside = |x: f64, z: f64| under(x, z) && under(x + RANGES_CELL, z + RANGES_CELL);
    let mut indices = Vec::new();
    for j in 0..cells {
        for i in 0..cells {
            let x = -RANGES_HALF + i as f64 * RANGES_CELL;
            let z = -RANGES_HALF + j as f64 * RANGES_CELL;
            if inside(x, z) && under(x + RANGES_CELL, z) && under(x, z + RANGES_CELL) {
                continue;
            }
            let a = (j * row + i) as u32;
            let (b, c, d) = (a + 1, a + row as u32, a + row as u32 + 1);
            indices.extend([a, c, b, b, c, d]);
        }
    }
    let mut mesh = Mesh::from_f64(&positions, &[], &[], Some(indices));
    mesh.colors = colors;
    mesh.compute_vertex_normals();
    mesh
}

/// `streamGeometry()`: a ribbon along the creek in local XY (the water node turns
/// it onto its horizontal mirror plane), UV x across and y along the stream.
fn stream_geometry() -> Mesh {
    let creek = creek();
    let count = creek.points.len();
    let mut positions = Vec::with_capacity(count * 6);
    let mut uvs = Vec::with_capacity(count * 4);
    let mut indices = Vec::with_capacity((count - 1) * 6);
    for (i, p) in creek.points.iter().enumerate() {
        let tangent = creek.curve.tangent(i as f64 / (count - 1) as f64);
        let width = 6.5 + (i as f64 * 0.12).sin() * 0.55;
        for side in [-1.0, 1.0] {
            positions.extend([
                p.x - tangent.z * width * side,
                -p.z - tangent.x * width * side,
                0.0,
            ]);
            uvs.extend([(side + 1.0) / 2.0, i as f64 * 1.9]);
        }
        if i < count - 1 {
            let n = (i * 2) as u32;
            indices.extend([n, n + 1, n + 2, n + 1, n + 3, n + 2]);
        }
    }
    let mut geometry = Mesh::from_f64(&positions, &[], &uvs, Some(indices));
    geometry.compute_vertex_normals();
    geometry
}

/// `VillageLandscape`: the valley group (`pine-valley-landscape`). Children: the
/// terrain, the creek water, the mountain ridge, batched rocks, batched forest.
pub fn village_landscape(grass: Arc<Material>) -> Node {
    let mut group = Node::group("pine-valley-landscape");
    let mut geometry = plane_geometry_segments(340.0, 340.0, 112, 112);
    geometry.rotate_x(-std::f64::consts::FRAC_PI_2);
    // Vertex colours multiply the ground shader's grass: broad patches, and a
    // muddy, sparser bank along the creek.
    let bank = [0.92, 0.74, 0.55];
    let mut colors = Vec::with_capacity(geometry.positions.len());
    for p in &mut geometry.positions {
        let (x, z) = (f64::from(p[0]), f64::from(p[2]));
        let river = creek_distance(x, z);
        p[1] = valley_height_at(x, z, river) as f32;
        let patch = 0.5 + 0.25 * (x * 0.064 + z * 0.03).sin() + 0.25 * (z * 0.1 - x * 0.05).sin();
        let mut color = [0.8 + patch * 0.16, 0.86 + patch * 0.12, 0.84 + patch * 0.14];
        if river < 8.0 {
            color = lerp_color(color, bank, (8.0 - river) / 8.0);
        }
        colors.push(color.map(|c| c as f32));
    }
    geometry.colors = colors;
    geometry.compute_vertex_normals();
    ground_uvs(&mut geometry, 0.0, 0.0);
    group
        .children
        .push(shadow_receiver(Arc::new(geometry), grass));
    group.children.push(water_surface(
        stream_geometry(),
        WaterKind::Creek,
        CREEK_HEIGHT,
    ));
    backdrop(&mut group);
    group
}

/// Background trees placed by position (the fixed treeline beside the arena).
const TREELINE: [[f64; 2]; 15] = [
    [-59.0, -66.0],
    [-52.0, -67.0],
    [-19.0, -67.0],
    [17.0, -68.0],
    [23.0, -68.0],
    [45.0, -67.0],
    [54.0, -66.0],
    [61.0, -67.0],
    [-67.0, -48.0],
    [-67.0, -6.0],
    [-68.0, 10.0],
    [-70.0, 54.0],
    [67.0, -42.0],
    [68.0, 4.0],
    [67.0, 42.0],
];

fn plant_tree(forest: &mut Node, shape: TreeShape) {
    let mut tree = tree_model(&shape, TreeDetail::Background);
    tree.position.y = valley_height(shape.x, shape.z) + mountain_rise(shape.x, shape.z);
    adopt_children(forest, tree);
}

fn backdrop(group: &mut Node) {
    let mut rng = Random::new(8274.0);
    let mut forest = Node::group("");
    let mut rocks = Node::group("");
    let stone = Arc::new(icosahedron_geometry(1.0, 0));
    let stone_materials = [0x879087, 0xa4aa92, 0x717f76].map(|color| {
        Arc::new(Material {
            flat_shading: true,
            ..Material::standard(color, 0.0, 0.96)
        })
    });
    let creek = creek();
    for i in 0..140 {
        let t = rng.next();
        let p = creek.curve.point(t);
        let tangent = creek.curve.tangent(t);
        let side = if i % 2 == 1 { -1.0 } else { 1.0 };
        let offset = rng.range(5.5, 9.5) * side;
        let x = p.x - tangent.z * offset;
        let z = p.z + tangent.x * offset;
        let mut rock = Node::mesh(stone.clone(), stone_materials[i % 3].clone());
        let size = rng.range(0.3, 1.4);
        rock.scale = DVec3::new(size, size * 0.4, size * rng.range(0.7, 1.2));
        let pitch = rng.next();
        let yaw = rng.next() * 6.0;
        rock.set_rotation_euler(pitch, yaw, 0.0);
        put(&mut rocks, rock, x, valley_height(x, z) + size * 0.08, z);
    }
    // Most of the forest grows in groves, so the hills read as woods with
    // clearings rather than evenly sprinkled trees.
    let groves: Vec<(f64, f64)> = (0..46)
        .map(|_| (rng.range(-150.0, 150.0), rng.range(-145.0, 105.0)))
        .collect();
    for i in 0..420 {
        let (x, z) = if i % 10 < 7 {
            let (gx, gz) = groves[i % groves.len()];
            let angle = rng.range(0.0, std::f64::consts::TAU);
            let reach = rng.range(0.0, 1.0).sqrt() * 13.0;
            (gx + angle.cos() * reach, gz + angle.sin() * reach)
        } else {
            (rng.range(-148.0, 148.0), rng.range(-145.0, 105.0))
        };
        if (x.abs() < 69.0 && z > -99.0)
            || (z > 57.0 && x.abs() < 98.0)
            || creek_distance(x, z) < 11.0
        {
            continue;
        }
        if ((x + 35.0).abs() < 15.0 && (z + 69.0).abs() < 17.0)
            || (x.abs() < 12.0 && (z + 82.0).abs() < 18.0)
        {
            continue;
        }
        if mountain_rise(x, z) > 22.0 {
            continue;
        }
        let height = rng.range(6.5, 13.5);
        let span = rng.range(4.4, 6.8);
        plant_tree(
            &mut forest,
            TreeShape {
                x,
                z,
                w: span,
                d: span,
                h: height,
            },
        );
    }
    for [x, z] in TREELINE {
        let h = 8.0 + rng.next() * 2.0;
        plant_tree(
            &mut forest,
            TreeShape {
                x,
                z,
                w: 4.4,
                d: 4.4,
                h,
            },
        );
    }
    // A continuous craggy ridge blends into the foothills, with snow following the terrain.
    let mut ridge = plane_geometry_segments(360.0, 95.0, 90, 26);
    ridge.rotate_x(-std::f64::consts::FRAC_PI_2);
    ridge.translate(0.0, 0.0, -159.5);
    let mut shades = Vec::with_capacity(ridge.positions.len());
    for p in &mut ridge.positions {
        let (x, z) = (f64::from(p[0]), f64::from(p[2]));
        let y = valley_height(x, z) + mountain_rise(x, z);
        p[1] = y as f32;
        shades.push(range_color(x, y, z).map(|c| c as f32));
    }
    ridge.colors = shades;
    ridge.compute_vertex_normals();
    // The meadow effect's range mode adds conifer stands, strata, crags and
    // settled snow to the vertex-coloured bands.
    let ranges = Arc::new(Material {
        vertex_colors: true,
        effect: Effect::Custom {
            name: MEADOW_SWAY,
            params: vec![2.0],
        },
        ..Material::standard(0xffffff, 0.0, 1.0)
    });
    let mut mountains = Node::mesh(Arc::new(ridge), ranges.clone());
    mountains.name = "pine-mountain-ridge".into();
    group.children.push(mountains);
    group
        .children
        .push(Node::mesh(Arc::new(far_ranges()), ranges));
    // Forest on the southern hills, beyond the overview's foreground.
    for _ in 0..260 {
        let x = rng.range(-165.0, 165.0);
        let z = rng.range(74.0, 165.0);
        if creek_distance(x, z) < 11.0 || (x.abs() < 40.0 && z < 92.0) {
            continue;
        }
        let height = rng.range(7.0, 14.0);
        let span = rng.range(3.6, 6.0);
        plant_tree(
            &mut forest,
            TreeShape {
                x,
                z,
                w: span,
                d: span,
                h: height,
            },
        );
    }
    batch(&mut rocks);
    batch(&mut forest);
    group.children.push(rocks);
    group.children.push(forest);
}
