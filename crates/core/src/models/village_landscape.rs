//! Port of `village-landscape.ts`: the Pine Valley terrain around the arena, its
//! creek, riverbank rocks, the background forest and the snowy mountain ridge.
//!
//! `creek_distance` and `valley_height` are also the ground queries of the meadow
//! and the footbridge approach's stepping stones.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use glam::DVec3;

use crate::geometry::{CatmullRomCurve3, Mesh, icosahedron_geometry, plane_geometry_segments};
use crate::scene::{Effect, Material, Node};

use super::batching::batch;
use super::effects_scenery::MEADOW_SWAY;
use super::ground_surfaces::ground_uvs;
use super::model_primitives::{adopt_children, put, shadow_receiver};
use super::tree_models::{TreeDetail, TreeShape, tree_model};
use super::village_ground::farmed;
use super::water_surface::{WaterKind, water_surface};
use crate::geometry::math::{hex_to_linear, lerp, lerp_color, smoothstep};
use crate::sim::math::Random;
use crate::sim::tree_proportions::tree_proportions;

/// Height of the creek's mirror plane.
pub const CREEK_HEIGHT: f64 = -2.65;
/// Samples along the creek (`getPoints(160)`).
const CREEK_DIVISIONS: u32 = 160;

/// Segments per culling chunk of the creek polyline.
const CREEK_CHUNK: usize = 8;

struct Creek {
    curve: CatmullRomCurve3,
    points: Vec<DVec3>,
    /// Per segment: start x, start z, dx, dz, 1 / squared length.
    segments: Vec<[f64; 5]>,
    /// Per chunk of [`CREEK_CHUNK`] segments: min x, min z, max x, max z.
    chunks: Vec<[f64; 4]>,
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
        let chunks = points
            .windows(2)
            .collect::<Vec<_>>()
            .chunks(CREEK_CHUNK)
            .map(|chunk| {
                chunk.iter().flat_map(|pair| pair.iter()).fold(
                    [
                        f64::INFINITY,
                        f64::INFINITY,
                        f64::NEG_INFINITY,
                        f64::NEG_INFINITY,
                    ],
                    |[x0, z0, x1, z1], p| [x0.min(p.x), z0.min(p.z), x1.max(p.x), z1.max(p.z)],
                )
            })
            .collect();
        Creek {
            curve,
            points,
            segments,
            chunks,
        }
    })
}

/// A point on the creek's centreline at `t` in [0, 1] and the unit tangent there.
pub fn creek_at(t: f64) -> (DVec3, DVec3) {
    let creek = creek();
    (creek.curve.point(t), creek.curve.tangent(t))
}

/// `creekDistance(x, z)`: horizontal distance to the creek's centreline.
pub fn creek_distance(x: f64, z: f64) -> f64 {
    let creek = creek();
    let mut nearest = f64::INFINITY;
    // Chunks whose bounds lie farther than the nearest segment so far are skipped.
    for (chunk, &[x0, z0, x1, z1]) in creek.segments.chunks(CREEK_CHUNK).zip(&creek.chunks) {
        let bx = (x0 - x).max(x - x1).max(0.0);
        let bz = (z0 - z).max(z - z1).max(0.0);
        if bx * bx + bz * bz >= nearest {
            continue;
        }
        for &[sx, sz, dx, dz, inverse] in chunk {
            let ax = x - sx;
            let az = z - sz;
            let t = 1.0f64.min(0.0f64.max((ax * dx + az * dz) * inverse));
            let ex = ax - dx * t;
            let ez = az - dz * t;
            nearest = nearest.min(ex * ex + ez * ez);
        }
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
    lerp(
        -4.2,
        -0.85 + hill + knolls(x, z, edge, river) + glacis(x, z, edge),
        bank,
    )
}

/// How high the earth banks up against the outside of the perimeter wall, and
/// the edge distances (m from the centre) over which it falls to the valley.
const GLACIS_RISE: f64 = 0.72;
const GLACIS_REACH: [f64; 2] = [62.0, 75.0];

/// The valley floor banks up against the outside of the wall nearly to the
/// arena's level, so the wall stands dug into an earth bank instead of on a
/// raised table; level under the landmarks of [`LEVEL_FOOTINGS`].
fn glacis(x: f64, z: f64, edge: f64) -> f64 {
    let [foot, toe] = GLACIS_REACH;
    if edge >= toe {
        return 0.0;
    }
    (1.0 - smoothstep(edge, foot, toe)) * GLACIS_RISE * footing_level(x, z)
}

/// Landmarks set at fixed heights on the valley floor (x, z, radius in metres):
/// the watermill, both footbridges and the log camp keep level ground.
const LEVEL_FOOTINGS: [[f64; 3]; 4] = [
    [-35.0, -69.0, 14.0],
    [0.0, -82.0, 10.0],
    [-85.0, 38.0, 10.0],
    [-68.0, -24.0, 8.0],
];

/// How far the valley floor's rolls and banks reach at (x, z) under the
/// [`LEVEL_FOOTINGS`]: 0 on a footing, rising to 1 ten metres beyond it.
fn footing_level(x: f64, z: f64) -> f64 {
    LEVEL_FOOTINGS
        .iter()
        .map(|&[cx, cz, radius]| {
            let (dx, dz) = (x - cx, z - cz);
            let reach = radius + 10.0;
            // Clearly beyond the ramp the smoothstep is exactly 1, which leaves the
            // minimum alone: skip its hypot (a slow libm call in the page), keeping a
            // margin far wider than either's rounding.
            if dx * dx + dz * dz > reach * reach * (1.0 + 1e-9) {
                return 1.0;
            }
            smoothstep(dx.hypot(dz), radius, reach)
        })
        .fold(1.0, f64::min)
}

/// Knolls and swales rolling the valley floor beyond the wall, a metre or two
/// high: level at the wall's foot, along the creek and under the landmarks of
/// [`LEVEL_FOOTINGS`], so nothing set on the old floor floats or sinks.
fn knolls(x: f64, z: f64, edge: f64, river: f64) -> f64 {
    let reach = smoothstep(edge, 62.0, 76.0) * smoothstep(river, 9.0, 18.0);
    if reach <= 0.0 {
        return 0.0;
    }
    let level = footing_level(x, z);
    let rolls = (lattice_noise(x * 0.045 + 3.1, z * 0.045 - 7.7) - 0.5) * 3.8
        + (lattice_noise(x * 0.11 - 2.0, z * 0.11 + 5.0) - 0.5) * 1.1;
    reach * level * rolls
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
    // Ridged noise raises sharp aretes and notches the crests, so the peaks
    // stand as jagged rock rather than smooth cones; no finer than the ridge's
    // grid can carry, or the faces break into terraces and zigzag streaks.
    let aretes = (ridges(x * 1.05 + 40.0, z * 1.05) - 0.3) * highest * 0.42;
    0.0f64.max(highest * 0.8 + total * 0.2 + (crags + aretes) * smoothstep(highest, 2.0, 12.0))
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

/// Ridged peaks of the distant ranges: sharp crests, broad valleys. Three
/// octaves: a fourth, ten metres across, aliased on the ranges' 6 m grid into
/// terraced, streaky faces.
fn ridges(x: f64, z: f64) -> f64 {
    let mut total = 0.0;
    let mut amplitude = 0.66;
    let mut frequency = 0.011;
    for _ in 0..3 {
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
    let mut geometry = plane_geometry_segments(
        VALLEY_HALF * 2.0,
        VALLEY_HALF * 2.0,
        VALLEY_SEGMENTS,
        VALLEY_SEGMENTS,
    );
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

/// The northern ridge's grid: size, segments and centre.
const RIDGE_SIZE: [f64; 2] = [360.0, 95.0];
const RIDGE_SEGMENTS: [u32; 2] = [90, 26];
const RIDGE_Z: f64 = -159.5;
/// The valley terrain's grid.
const VALLEY_SEGMENTS: u32 = 112;

/// Height of a triangulated height grid at (x, z), exactly as its mesh draws it:
/// cells of `cell` metres from `origin`, split along the diagonal from their
/// (low x, high z) corner to their (high x, low z) corner like `PlaneGeometry`
/// and [`far_ranges`]. `corner(i, j)` is the height of the grid's corner `i`
/// cells along x and `j` along z.
fn grid_surface(
    x: f64,
    z: f64,
    origin: [f64; 2],
    cell: [f64; 2],
    mut corner: impl FnMut(f64, f64) -> f64,
) -> f64 {
    let (gx, gz) = ((x - origin[0]) / cell[0], (z - origin[1]) / cell[1]);
    let (ix, iz) = (gx.floor(), gz.floor());
    let (u, v) = (gx - ix, gz - iz);
    let (b, d) = (corner(ix, iz + 1.0), corner(ix + 1.0, iz));
    if u + v <= 1.0 {
        let a = corner(ix, iz);
        a + (d - a) * u + (b - a) * v
    } else {
        let c = corner(ix + 1.0, iz + 1.0);
        c + (b - c) * (1.0 - u) + (d - c) * (1.0 - v)
    }
}

/// One height grid of the backdrop, its corner heights kept once computed.
struct CornerGrid {
    origin: [f64; 2],
    cell: [f64; 2],
    /// Corners along x (cells + 1); rows run along z.
    columns: usize,
    heights: Vec<Option<f64>>,
    height: fn(f64, f64) -> f64,
}

impl CornerGrid {
    fn new(origin: [f64; 2], cell: [f64; 2], cells: [u32; 2], height: fn(f64, f64) -> f64) -> Self {
        let columns = cells[0] as usize + 1;
        Self {
            origin,
            cell,
            columns,
            heights: vec![None; columns * (cells[1] as usize + 1)],
            height,
        }
    }

    /// [`grid_surface`] of this grid: a corner's height is the same whenever it is
    /// asked for, so each is computed once.
    fn surface(&mut self, x: f64, z: f64) -> f64 {
        let (origin, cell, columns, height) = (self.origin, self.cell, self.columns, self.height);
        let heights = &mut self.heights;
        grid_surface(x, z, origin, cell, |i, j| {
            let at = |i: f64, j: f64| height(origin[0] + i * cell[0], origin[1] + j * cell[1]);
            let index = (i >= 0.0 && j >= 0.0 && i < columns as f64)
                .then(|| j as usize * columns + i as usize)
                .filter(|&index| index < heights.len());
            match index {
                Some(index) => *heights[index].get_or_insert_with(|| at(i, j)),
                None => at(i, j),
            }
        })
    }
}

fn ridge_height(x: f64, z: f64) -> f64 {
    valley_height(x, z) + mountain_rise(x, z)
}

fn far_height(x: f64, z: f64) -> f64 {
    valley_height(x, z) + mountain_rise(x, z) + far_rise(x, z)
}

/// The drawn ground under the backdrop: the valley terrain, the northern ridge
/// and the distant ranges as their meshes interpolate them. The forest asks for
/// it five times round every trunk, and neighbouring trees share cells, so the
/// grids keep the corner heights they have computed.
struct BackdropGround {
    valley: CornerGrid,
    ridge: CornerGrid,
    far: CornerGrid,
}

impl BackdropGround {
    fn new() -> Self {
        let valley_cell = VALLEY_HALF * 2.0 / f64::from(VALLEY_SEGMENTS);
        let [width, depth] = RIDGE_SIZE;
        let ranges_cells = (RANGES_HALF * 2.0 / RANGES_CELL) as u32;
        Self {
            valley: CornerGrid::new(
                [-VALLEY_HALF; 2],
                [valley_cell; 2],
                [VALLEY_SEGMENTS; 2],
                valley_height,
            ),
            ridge: CornerGrid::new(
                [-width / 2.0, RIDGE_Z - depth / 2.0],
                [
                    width / f64::from(RIDGE_SEGMENTS[0]),
                    depth / f64::from(RIDGE_SEGMENTS[1]),
                ],
                RIDGE_SEGMENTS,
                ridge_height,
            ),
            far: CornerGrid::new(
                [-RANGES_HALF; 2],
                [RANGES_CELL; 2],
                [ranges_cells; 2],
                far_height,
            ),
        }
    }

    /// The drawn ground at (x, z): the highest of the valley terrain, the
    /// northern ridge and the distant ranges where each lies, as their meshes
    /// interpolate it (a tree planted on the true height would hang off a crest
    /// the mesh cuts across).
    fn at(&mut self, x: f64, z: f64) -> f64 {
        let mut ground = f64::NEG_INFINITY;
        if x.abs() < VALLEY_HALF && z.abs() < VALLEY_HALF {
            ground = ground.max(self.valley.surface(x, z));
        }
        let [width, depth] = RIDGE_SIZE;
        if x.abs() < width / 2.0 && (z - RIDGE_Z).abs() < depth / 2.0 {
            ground = ground.max(self.ridge.surface(x, z));
        }
        if ground == f64::NEG_INFINITY || x.abs() > VALLEY_HALF - 8.0 || z.abs() > VALLEY_HALF - 8.0
        {
            let far = self.far.surface(x, z);
            ground = ground.max(far - if ground > f64::NEG_INFINITY { 1.5 } else { 0.0 });
        }
        ground
    }

    /// Where a trunk can stand on the backdrop: the lowest drawn ground around its
    /// base, a little deeper, so it never hangs over a slope; `None` on faces too
    /// steep for a tree.
    fn trunk_footing(&mut self, x: f64, z: f64) -> Option<f64> {
        let reach = 1.1;
        let around = [(reach, 0.0), (-reach, 0.0), (0.0, reach), (0.0, -reach)]
            .map(|(dx, dz)| self.at(x + dx, z + dz));
        let low = around.iter().copied().fold(self.at(x, z), f64::min);
        let high = around.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let slope = (high - low) / (reach * 2.0);
        // Trees keep to ground they could root in; on a slope the trunk sinks
        // deeper, so its downhill side never shows daylight under it.
        (slope < MAX_TREE_SLOPE).then_some(low - 0.3 - slope * 0.5)
    }
}

/// The steepest backdrop ground a tree grows on (rise over run).
const MAX_TREE_SLOPE: f64 = 0.62;
/// Trees farther than this from the arena's centre (on either axis) are only ever
/// seen across the valley and grow at [`TreeDetail::Distant`].
const DISTANT_TREES: f64 = 100.0;

/// A position near (x, z) whose tree grows into a family `want` accepts: the
/// family is seeded by position, so nudge it by a few centimetres at a time.
fn nudge_for_family(x: f64, z: f64, h: f64, want: impl Fn(u32) -> bool) -> (f64, f64) {
    for k in 0..24 {
        let (tx, tz) = (x + f64::from(k) * 0.07, z - f64::from(k) * 0.05);
        if want(tree_proportions(tx, tz, 4.0, 4.0, h).family) {
            return (tx, tz);
        }
    }
    (x, z)
}

/// The forest's batching sectors: the renderer culls each merged batch whole,
/// so the forest is batched by compass sector rather than whole (a single
/// forest batch would draw everywhere, even from the follow camera), in a near
/// ring along the wall, which the follow camera sees a sector or two of, and a
/// far ring beyond [`DISTANT_TREES`], each in four draws (bark, birch bark,
/// needles, leaves).
const FOREST_SECTORS: f64 = 8.0;

/// The background forest, one group of trees per batching cell.
struct Forest {
    cells: BTreeMap<(i64, i64), Node>,
    ground: BackdropGround,
}

impl Forest {
    fn new() -> Self {
        Self {
            cells: BTreeMap::new(),
            ground: BackdropGround::new(),
        }
    }

    fn plant(&mut self, shape: TreeShape) -> bool {
        let Some(footing) = self.ground.trunk_footing(shape.x, shape.z) else {
            return false;
        };
        let distant = shape.x.abs().max(shape.z.abs()) > DISTANT_TREES;
        let detail = if distant {
            TreeDetail::Distant
        } else {
            TreeDetail::Background
        };
        let mut tree = tree_model(&shape, detail);
        tree.position.y = footing;
        let turn = shape.z.atan2(shape.x) / std::f64::consts::TAU + 0.5;
        let key = (
            i64::from(distant),
            (turn * FOREST_SECTORS).floor() as i64 % FOREST_SECTORS as i64,
        );
        adopt_children(self.cells.entry(key).or_default(), tree);
        true
    }

    /// The batched cells under one group.
    fn finish(self) -> Node {
        let mut group = Node::group("");
        for (_, mut cell) in self.cells {
            batch(&mut cell);
            group.children.push(cell);
        }
        group
    }
}

/// Where the background forest leaves the ground open: the village and its yard,
/// the creek, the mill and the bridge approach, the fields, lanes and farmyard.
fn forest_clearing(x: f64, z: f64, treeline: bool) -> bool {
    (x.abs() < 66.0 && z.abs() < 66.0)
        // Near the wall only the authored treeline stands grow: their 60 m
        // scenery cells are in view from the follow camera.
        || (!treeline && x.abs() < 69.0 && z > -99.0 && z < 66.0)
        // The overview looks in over the southern wall.
        || (z > 57.0 && z < 92.0 && x.abs() < 98.0)
        || creek_distance(x, z) < 10.5
        || ((x + 35.0).abs() < 15.0 && (z + 69.0).abs() < 13.0)
        || (x.abs() < 9.0 && (z + 82.0).abs() < 16.0)
        // The field hut north of the creek.
        || ((x + 41.0).abs() < 4.0 && (z + 99.0).abs() < 4.0)
        || farmed(x, z)
        // The ranges' own canopy shading clothes the mountain faces.
        || mountain_rise(x, z) > 9.0
}

/// One grove: its trees stand thickest and tallest in the middle, younger and
/// smaller toward the edge, with saplings and shrubs round its skirt.
#[allow(clippy::too_many_arguments)]
fn grove(
    forest: &mut Forest,
    rng: &mut Random,
    centre: (f64, f64),
    radius: f64,
    trees: usize,
    age: f64,
    conifer_share: f64,
    treeline: bool,
) {
    let (cx, cz) = centre;
    for i in 0..trees {
        let angle = rng.range(0.0, std::f64::consts::TAU);
        let reach = rng.next().powf(0.7) * radius;
        let edge = reach / radius;
        let (x, z) = (cx + angle.cos() * reach, cz + angle.sin() * reach * 0.8);
        if forest_clearing(x, z, treeline) {
            continue;
        }
        let height = age * rng.range(0.78, 1.18) * (1.0 - edge * 0.35);
        let span = height * rng.range(0.4, 0.52);
        let conifer = rng.next() < conifer_share * (1.0 - edge * 0.3);
        let (x, z) = nudge_for_family(x, z, height, |family| {
            if conifer {
                family < 3
            } else {
                family >= 3 && (i % 3 == 0 || family != 4)
            }
        });
        forest.plant(TreeShape {
            x,
            z,
            w: span,
            d: span,
            h: height,
        });
    }
    // Saplings and shrubs where the grove meets the open ground.
    let skirt = (trees / 3).max(3);
    for _ in 0..skirt {
        let angle = rng.range(0.0, std::f64::consts::TAU);
        let reach = radius * rng.range(0.85, 1.25);
        let (x, z) = (cx + angle.cos() * reach, cz + angle.sin() * reach * 0.8);
        if forest_clearing(x, z, treeline) {
            continue;
        }
        let height = rng.range(1.8, 3.6);
        let span = height * rng.range(0.9, 1.3);
        let (x, z) = nudge_for_family(x, z, height, |family| {
            family == 1 || family == 3 || family == 5
        });
        forest.plant(TreeShape {
            x,
            z,
            w: span,
            d: span,
            h: height,
        });
    }
}

fn backdrop(group: &mut Node) {
    let mut rng = Random::new(8274.0);
    let mut forest = Forest::new();
    let mut rocks = Node::group("");
    let stone = Arc::new(icosahedron_geometry(1.0, 0));
    let stone_materials = [0x879087, 0xa4aa92, 0x717f76].map(|color| {
        Arc::new(Material {
            flat_shading: true,
            ..Material::standard(color, 0.0, 0.96)
        })
    });
    let creek = creek();
    // Banks of rounded stones, in clusters at the bends, a few boulders among them.
    for i in 0..220 {
        let t = rng.next();
        let p = creek.curve.point(t);
        let tangent = creek.curve.tangent(t);
        let side = if i % 2 == 1 { -1.0 } else { 1.0 };
        let offset = rng.range(5.0, 9.0) * side;
        let x = p.x - tangent.z * offset;
        let z = p.z + tangent.x * offset;
        let mut rock = Node::mesh(stone.clone(), stone_materials[i % 3].clone());
        let size = if i % 17 == 0 {
            rng.range(1.4, 2.2)
        } else {
            rng.range(0.25, 1.1)
        };
        rock.scale = DVec3::new(
            size,
            size * rng.range(0.38, 0.6),
            size * rng.range(0.7, 1.2),
        );
        let pitch = rng.next();
        let yaw = rng.next() * 6.0;
        rock.set_rotation_euler(pitch, yaw, 0.0);
        // On the drawn ground: the coarse terrain mesh cuts below `valley_height`
        // across the steep creek bank, where a stone set on the true height hovers.
        put(&mut rocks, rock, x, forest.ground.at(x, z) + size * 0.06, z);
    }
    // The forest grows in groves of every age, so the hills read as woods with
    // clearings and depth rather than ranks of one tree. Mountain groves are all
    // conifers; valley groves mix in oak, birch and aspen.
    let groves: Vec<(f64, f64, f64)> = (0..56)
        .map(|_| {
            (
                rng.range(-160.0, 160.0),
                rng.range(-150.0, 160.0),
                rng.range(0.0, 1.0),
            )
        })
        .collect();
    for &(x, z, pick) in &groves {
        let alpine = mountain_rise(x, z) > 3.0 || z < -108.0;
        let radius = 7.0 + pick * 9.0;
        let trees = (radius * radius * 0.09) as usize;
        let age = 7.0 + (1.0 - pick) * 6.0;
        grove(
            &mut forest,
            &mut rng,
            (x, z),
            radius,
            trees,
            age,
            if alpine { 1.0 } else { 0.7 },
            false,
        );
    }
    // The foothill wood: an uneven band of stands along the mountains' feet,
    // deep in some stretches and thin in others, broken by meadow, mostly
    // conifer with stands of aspen and birch, so the valley ends in a wood of
    // many ages rather than a rank of one tree.
    let mut x: f64 = -178.0;
    while x < 178.0 {
        let radius = rng.range(4.5, 11.0);
        let depth = rng.next();
        let z = -110.0 - depth * 20.0 + (x * 0.043).sin() * 4.0;
        let trees = (radius * radius * rng.range(0.07, 0.13)) as usize;
        let age = rng.range(7.0, 15.5);
        let conifers = if rng.next() < 0.24 { 0.3 } else { 0.95 };
        let centre = (x + rng.range(-3.0, 3.0), z);
        grove(
            &mut forest,
            &mut rng,
            centre,
            radius,
            trees,
            age,
            conifers,
            false,
        );
        x += rng.range(6.0, 15.0);
    }
    // Stands on the creek's far bank and between the fields beyond it.
    for [x, z, radius] in NORTH_BANK {
        let trees = (radius * radius * 0.13) as usize;
        let age = rng.range(8.0, 12.0);
        grove(&mut forest, &mut rng, (x, z), radius, trees, age, 0.8, true);
    }
    // The near treeline outside the wall: small stands and single trees of mixed
    // age with shrubs, leaving the lanes, the mill and the bridge open. Mostly
    // pines and spruces, so the village is ringed by the trees it is named for.
    for [x, z, radius, count] in TREELINE {
        let age = 7.0 + rng.next() * 4.0;
        grove(
            &mut forest,
            &mut rng,
            (x, z),
            radius,
            count as usize,
            age,
            0.85,
            true,
        );
    }
    // A continuous craggy ridge blends into the foothills, with snow following the terrain.
    let mut ridge = plane_geometry_segments(
        RIDGE_SIZE[0],
        RIDGE_SIZE[1],
        RIDGE_SEGMENTS[0],
        RIDGE_SEGMENTS[1],
    );
    ridge.rotate_x(-std::f64::consts::FRAC_PI_2);
    ridge.translate(0.0, 0.0, RIDGE_Z);
    let mut shades = Vec::with_capacity(ridge.positions.len());
    for p in &mut ridge.positions {
        let (x, z) = (f64::from(p[0]), f64::from(p[2]));
        let y = valley_height(x, z) + mountain_rise(x, z);
        p[1] = y as f32;
        shades.push(range_color(x, y, z).map(|c| c as f32));
    }
    ridge.colors = shades;
    ridge.compute_vertex_normals();
    // The meadow effect's range mode cuts the slopes into rock facets, forest
    // and crisp snowfields over the vertex-coloured bands.
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
    batch(&mut rocks);
    group.children.push(rocks);
    group.children.push(forest.finish());
}

/// Stands north of the creek: centre x, z and radius.
const NORTH_BANK: [[f64; 3]; 9] = [
    [-26.0, -92.0, 5.0],
    [-64.0, -97.0, 7.0],
    [-47.0, -106.0, 6.0],
    [-50.0, -92.0, 4.0],
    [9.0, -106.0, 5.5],
    [17.0, -112.0, 5.0],
    [66.0, -103.0, 7.0],
    [80.0, -88.0, 5.0],
    [-2.0, -112.0, 4.5],
];

/// Stands of the near treeline outside the wall: centre x, z, radius and trees.
const TREELINE: [[f64; 4]; 18] = [
    [-60.0, -70.0, 5.0, 5.0],
    [-12.0, -77.0, 3.5, 3.0],
    [11.0, -76.0, 3.5, 3.0],
    [-50.0, -74.0, 4.0, 3.0],
    [-19.0, -72.0, 3.5, 3.0],
    [18.0, -70.0, 4.5, 4.0],
    [33.0, -73.0, 3.0, 2.0],
    [50.0, -70.0, 6.0, 6.0],
    [64.0, -76.0, 4.0, 3.0],
    [-72.0, -48.0, 5.0, 4.0],
    [-71.0, -8.0, 4.0, 3.0],
    [-72.0, 14.0, 3.5, 3.0],
    [-74.0, 58.0, 5.0, 5.0],
    [74.0, -62.0, 4.0, 3.0],
    [72.0, 26.0, 3.5, 3.0],
    [75.0, 52.0, 5.0, 4.0],
    [-74.0, 30.0, 3.0, 2.0],
    [73.0, -30.0, 3.0, 2.0],
];
