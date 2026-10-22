//! Harbor Havoc's far shore, so the world ends in a coast rather than grey: green
//! headlands with wind turbines, a city across the bay, a neighbouring container
//! terminal, a tank farm, a breakwater with its lighthouse, ships riding at anchor
//! in the roads and a blue mountain range behind it all, every one outside the
//! playfield. Variation comes from a fixed hash of the angle, never the
//! simulation's random stream.
//!
//! The coast is split into quarter sectors, each merged into one mesh drawn with the
//! apron effect's far-shore mode ([`ApronMode::Shore`]): it lights the vertex
//! colors itself and hazes them by their distance from the eye, so the layers stay
//! apart instead of drowning in the scene fog, and red warning lights on the
//! cranes, masts and turbines shine through. A view draws only the sectors in
//! front of it. Nothing here casts or receives shadows.

use std::f64::consts::{FRAC_PI_2, PI, TAU};
use std::sync::Arc;

use crate::geometry::math::{hex_to_linear, scale_hex_color};
use crate::geometry::{Mesh, merge_geometries};
use crate::scene::{Material, Node};

use super::batching::batch;
use super::effects_scenery::ApronMode;
use super::harbor_surfaces::apron_material;
use super::model_primitives::{adopt_children, beam, box_part, cylinder_part, paint, put, rotated};

/// The four quarter sectors being filled, indexed by [`sector_of`].
type Sectors = [Node; 4];

/// The quarter sector an angle falls in: [-π, -π/2) north-west, [-π/2, 0)
/// north-east, [0, π/2) south-east, [π/2, π) south-west.
fn sector_of(theta: f64) -> usize {
    (((theta + PI) / FRAC_PI_2) as usize).min(3)
}

/// Place an assembly in the sector of `theta`, its nested groups baked flat so the
/// sector batches into a few draws.
fn place(sectors: &mut Sectors, theta: f64, assembly: Node) {
    fn flatten(parent: &mut Node, group: Node) {
        let mut moved = Node::group("");
        adopt_children(&mut moved, group);
        for child in moved.children {
            if child.drawable.is_none() {
                flatten(parent, child);
            } else {
                parent.children.push(child);
            }
        }
    }
    let target = &mut sectors[sector_of(theta)];
    if assembly.drawable.is_some() {
        target.children.push(assembly);
    } else {
        flatten(target, assembly);
    }
}

/// A part at a world position, in the sector of `theta`.
fn place_at(sectors: &mut Sectors, theta: f64, part: Node, x: f64, y: f64, z: f64) {
    let mut part = part;
    part.position = glam::DVec3::new(x, y, z);
    place(sectors, theta, part);
}

/// Ring segments around the bay.
const SEGMENTS: usize = 240;
/// Mean radius of the shoreline from the arena centre, metres.
const SHORE_RADIUS: f64 = 232.0;
/// Paint of a warning light: the far-shore effect makes this exact red glow.
const WARNING_LIGHT: u32 = 0xff0000;
/// The mountain range behind the coast: radius of its foot and its depth, metres.
const RANGE_RADIUS: f64 = 430.0;
const RANGE_DEPTH: f64 = 150.0;
/// The open sea to the north (−z), as an angle range with no land.
const OPEN_SEA: (f64, f64) = (-2.25, -0.95);
/// Water level the shore rises from (`harbor_water::HARBOR_WATER_HEIGHT`).
const SEA_LEVEL: f64 = -2.2;

/// A smooth pseudo-random value in 0..1 for an angle, periodic over the circle.
fn wave(theta: f64, seed: f64) -> f64 {
    let s = (theta * 3.0 + seed).sin() * 0.45
        + (theta * 7.0 + seed * 2.3).sin() * 0.3
        + (theta * 13.0 + seed * 4.1).sin() * 0.25;
    s * 0.5 + 0.5
}

/// A fixed hash of an index in 0..1.
fn hash(i: u32, seed: u32) -> f64 {
    let mut x = i.wrapping_mul(0x9e37_79b9) ^ seed.wrapping_mul(0x85eb_ca6b);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297a_2d39);
    x ^= x >> 15;
    f64::from(x) / f64::from(u32::MAX)
}

fn in_open_sea(theta: f64) -> bool {
    theta > OPEN_SEA.0 && theta < OPEN_SEA.1
}

/// How much of the headland stands at this angle: 0 at the open sea's edges,
/// rising over a quarter radian.
fn land_amount(theta: f64) -> f64 {
    let edge = (theta - OPEN_SEA.1).min(OPEN_SEA.0 + TAU - theta);
    let edge = if theta < OPEN_SEA.0 {
        (OPEN_SEA.0 - theta).min(theta + TAU - OPEN_SEA.1)
    } else {
        edge
    };
    (edge / 0.3).clamp(0.0, 1.0)
}

/// Ridge height of the hills behind the shore at an angle: low where the city and
/// the terminal stand (east), high headlands west.
fn ridge_height(theta: f64) -> f64 {
    let west = (-theta.cos()).max(0.0);
    let base = 7.0 + 26.0 * west + 12.0 * wave(theta, 1.7);
    base * land_amount(theta)
}

fn shore_radius(theta: f64) -> f64 {
    SHORE_RADIUS + 22.0 * wave(theta, 4.2) - 8.0
}

/// Profile of the coast from the waterline inland: (metres past the shoreline,
/// fraction of the ridge height, extra height, color). Surf breaks white on the
/// waterline and pale sand rises from it, so the coast reads as a line at a glance.
const PROFILE: [(f64, f64, f64, u32); PROFILE_ROWS] = [
    (-3.0, 0.0, -1.2, 0x4f5650),
    (0.0, 0.0, 0.35, 0xe4e8e2),
    (2.0, 0.0, 2.4, 0xc2b28a),
    (9.0, 0.12, 3.0, 0x6f8a4e),
    (19.0, 0.36, 2.6, 0x5a7c46),
    (30.0, 0.62, 2.0, 0x4f7040),
    (42.0, 0.86, 1.0, 0x48683d),
    (55.0, 1.0, 0.0, 0x3f5e3a),
    (80.0, 0.8, 0.0, 0x4a6744),
    (110.0, 0.55, 0.0, 0x58704f),
];
const PROFILE_ROWS: usize = 10;
/// The rows of [`PROFILE`] that are shoreline (rock foot, surf and sand, or the
/// quay's wall and coping): they run true, and keep hard creases.
const SHORELINE_ROWS: usize = 3;
/// The rows whose ground can break into bare crags under a high ridge.
const CRAG_ROWS: std::ops::RangeInclusive<usize> = 5..=7;
/// Normals are smoothed across the coast's facets that meet at less than this
/// angle, radians: rolling hills, with the quay walls and crags kept sharp.
const COAST_CREASE: f64 = 0.7;
/// Bare rock showing through on the steepest ground.
const ROCK: u32 = 0x7d786c;

/// The shore ring, split into its four sectors: smooth-shaded, vertex-colored
/// hills (creased at the quay walls and crags) flagged for the countryside.
fn shore_meshes() -> [Option<Node>; 4] {
    ring_sectors(
        PROFILE_ROWS,
        |step, row| {
            let theta = step_angle(step);
            let radius = shore_radius(theta);
            let ridge = ridge_height(theta);
            let (offset, height, color) = profile_row(theta, row);
            let jitter = hash(step as u32 * 16 + row as u32, 11) - 0.5;
            // The waterline rows stay in step so the surf and beach (or the quay
            // wall) read as bands; the quay's rows run true; the hills behind it
            // keep their roughness.
            let rough = if row <= SHORELINE_ROWS + 2 {
                1.0 - quay_amount(theta)
            } else {
                1.0
            };
            let r = radius + offset + jitter * rough * if row < SHORELINE_ROWS { 0.6 } else { 4.0 };
            let y = SEA_LEVEL + height + jitter * rough * ridge * 0.06;
            let shade = 0.84 + hash(step as u32 * 16 + row as u32, 23) * 0.26;
            let craggy = CRAG_ROWS.contains(&row) && ridge > 22.0 && hash(step as u32, 29) > 0.55;
            let color = if craggy && quay_amount(theta) == 0.0 {
                ROCK
            } else {
                color
            };
            (
                [r * theta.cos(), y, r * theta.sin()],
                scale_hex_color(color, shade),
            )
        },
        |theta, next| in_open_sea(theta) && in_open_sea(next),
        TERRAIN_FLAG,
    )
}

/// The angle of a ring step.
fn step_angle(step: usize) -> f64 {
    -PI + TAU * step as f64 / SEGMENTS as f64
}

/// A ring of `rows` vertices per step all round the bay, its normals smoothed
/// across the whole ring (so no seam shows between sectors) and creased at sharp
/// edges, then split into the quarter sectors. `vertex` gives a step and row's
/// position and paint, `skip` the columns between two angles left out, and every
/// vertex takes `flag` as its uv.y.
fn ring_sectors(
    rows: usize,
    vertex: impl Fn(usize, usize) -> ([f64; 3], u32),
    skip: impl Fn(f64, f64) -> bool,
    flag: f32,
) -> [Option<Node>; 4] {
    let mut positions = Vec::new();
    let mut colors = Vec::new();
    for step in 0..=SEGMENTS {
        for row in 0..rows {
            // The last step closes the ring on the first.
            let (position, color) = vertex(step % SEGMENTS, row);
            positions.extend(position);
            let [cr, cg, cb] = hex_to_linear(color);
            colors.push([cr as f32, cg as f32, cb as f32]);
        }
    }
    let mut indices = Vec::new();
    // The first triangle of each sector, in the de-indexed mesh.
    let mut sector_starts = [usize::MAX; 4];
    for column in 0..SEGMENTS {
        let (theta, next) = (step_angle(column), step_angle(column + 1));
        if skip(theta, next) {
            continue;
        }
        let sector = sector_of(theta);
        if sector_starts[sector] == usize::MAX {
            sector_starts[sector] = indices.len() / 3;
        }
        for row in 0..rows - 1 {
            let a = (column * rows + row) as u32;
            let b = a + rows as u32;
            indices.extend([a, b, a + 1, b, b + 1, a + 1]);
        }
    }
    let triangles = indices.len() / 3;
    let mut mesh = Mesh::from_f64(&positions, &[], &[], Some(indices));
    mesh.colors = colors;
    let ring = mesh.to_creased_normals(COAST_CREASE);
    std::array::from_fn(|sector| {
        let start = sector_starts[sector];
        if start == usize::MAX {
            return None;
        }
        let end = sector_starts[sector + 1..]
            .iter()
            .copied()
            .find(|&next| next != usize::MAX)
            .unwrap_or(triangles);
        let corners = start * 3..end * 3;
        let part = Mesh {
            positions: ring.positions[corners.clone()].to_vec(),
            normals: ring.normals[corners.clone()].to_vec(),
            colors: ring.colors[corners.clone()].to_vec(),
            uvs: vec![[0.0, flag]; corners.len()],
            ..Mesh::default()
        };
        Some(Node::mesh(
            Arc::new(part),
            Arc::new(Material {
                vertex_colors: true,
                ..Material::standard(0xffffff, 0.0, 1.0)
            }),
        ))
    })
}

/// Mark a part's faces for the far-shore effect by its uv.y (every other
/// backdrop part keeps its uvs within 0..1): [`FACADE_FLAG`] draws a grid of
/// windows over a tower's walls, [`STACK_FLAG`] the boxes of a container stack,
/// each tier and bay its own line's color, and [`TERRAIN_FLAG`] the countryside
/// (fields, woods and rock) over the green of the coast; the pines carry
/// [`TREE_FLAG`] in their own mesh.
fn flagged(mut part: Node, flag: f32) -> Node {
    if let Some(drawable) = &mut part.drawable {
        let mut mesh = (*drawable.mesh).clone();
        mesh.uvs = vec![[0.0, flag]; mesh.positions.len()];
        drawable.mesh = Arc::new(mesh);
    }
    part
}
fn facade(part: Node) -> Node {
    flagged(part, FACADE_FLAG)
}
const FACADE_FLAG: f32 = 10.0;
const STACK_FLAG: f32 = 20.0;
const TERRAIN_FLAG: f32 = 30.0;
/// The far pines, lit mostly by the sky so the woods read as dark masses.
const TREE_FLAG: f32 = 40.0;

/// Ridge height of the mountain range at an angle, metres above the sea.
fn range_height(theta: f64) -> f64 {
    let peaks = wave(theta, 9.1).powf(1.6) * 95.0 + wave(theta * 2.0, 3.3) * 30.0;
    (35.0 + peaks) * land_amount(theta)
}

/// The blue mountain range behind the coast, in its four sectors: a foot, the
/// ridge and the far slope, with darker gullies and paler crests.
fn range_meshes() -> [Option<Node>; 4] {
    const ROWS: [(f64, f64, u32); 4] = [
        (0.0, 0.0, 0x56656c),
        (0.45, 0.62, 0x4c5d68),
        (0.7, 1.0, 0x667a86),
        (1.0, 0.4, 0x5b6d79),
    ];
    ring_sectors(
        ROWS.len(),
        |step, row| {
            let theta = step_angle(step);
            let (depth, fraction, color) = ROWS[row];
            let jitter = hash(step as u32 * 4 + row as u32, 131) - 0.5;
            let r = RANGE_RADIUS + depth * RANGE_DEPTH + jitter * 10.0;
            let y = SEA_LEVEL - 4.0 + range_height(theta) * fraction * (1.0 + jitter * 0.25);
            let shade = 0.85 + hash(step as u32 * 4 + row as u32, 137) * 0.3;
            (
                [r * theta.cos(), y, r * theta.sin()],
                scale_hex_color(color, shade),
            )
        },
        |theta, next| land_amount(theta) <= 0.0 && land_amount(next) <= 0.0,
        0.0,
    )
}

/// A point on the land `inland` metres past the shoreline at `theta`, at ground
/// height.
fn on_land(theta: f64, inland: f64) -> (f64, f64, f64) {
    let r = shore_radius(theta) + inland;
    let rows: [(f64, f64); PROFILE_ROWS] = std::array::from_fn(|row| {
        let (offset, height, _) = profile_row(theta, row);
        (offset, height)
    });
    let height =
        rows.windows(2)
            .find(|pair| inland <= pair[1].0)
            .map_or(rows[PROFILE_ROWS - 1].1, |pair| {
                let (a, b) = (pair[0], pair[1]);
                let t = ((inland - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
                a.1 + (b.1 - a.1) * t
            });
    (r * theta.cos(), SEA_LEVEL + height, r * theta.sin())
}

/// The city's and the terminal's waterfront, east across the bay, as an angle
/// range: a concrete quay with a sheer wall instead of a beach.
const WATERFRONT: (f64, f64) = (-0.72, 1.16);

/// How much of the waterfront's quay stands at this angle: 1 along it, easing to
/// the natural shore over a tenth of a radian at each end.
fn quay_amount(theta: f64) -> f64 {
    let inside = (theta - WATERFRONT.0).min(WATERFRONT.1 - theta);
    let t = (inside / 0.1).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Height of the waterfront's deck above the sea, metres.
const QUAY_DECK: f64 = 2.8;

/// The waterfront's profile, row for row with [`PROFILE`]: the wall's foot under
/// water, its dark wet face and fender line, a pale coping, the deck the cranes and
/// stacks stand on, then the hills behind. (offset, ridge fraction, extra height,
/// color) as in [`PROFILE`].
const QUAY_PROFILE: [(f64, f64, f64, u32); PROFILE_ROWS] = [
    (-3.0, 0.0, -1.2, 0x3f4547),
    (-2.8, 0.0, QUAY_DECK - 0.5, 0x2c3133),
    (-2.6, 0.0, QUAY_DECK, 0xd2d0c6),
    (6.0, 0.0, QUAY_DECK, 0xa9a89f),
    (26.0, 0.0, QUAY_DECK, 0xa3a299),
    (46.0, 0.0, QUAY_DECK, 0x9d9c94),
    (54.0, 0.5, QUAY_DECK * 0.5, 0x48683d),
    (62.0, 1.0, 0.0, 0x3f5e3a),
    (85.0, 0.8, 0.0, 0x4a6744),
    (110.0, 0.55, 0.0, 0x58704f),
];

/// Row `row` of the coast's profile at an angle: metres past the shoreline, height
/// above the sea and color, the natural shore blended into the waterfront's quay.
fn profile_row(theta: f64, row: usize) -> (f64, f64, u32) {
    let ridge = ridge_height(theta);
    let natural = PROFILE[row];
    let quay = QUAY_PROFILE[row];
    let q = quay_amount(theta);
    let land = land_amount(theta).max(0.2);
    let height = |(_, fraction, lift, _): (f64, f64, f64, u32)| (ridge * fraction + lift) * land;
    let offset = natural.0 + (quay.0 - natural.0) * q;
    // Knolls and saddles roll along the hills, the same for every structure
    // standing on them as for the ground's own mesh.
    let knolls = if row > SHORELINE_ROWS {
        (wave(theta * 4.0 + row as f64 * 0.7, 2.9) - 0.5) * ridge * 0.35 * land
    } else {
        0.0
    };
    let y = height(natural) + (height(quay) - height(natural)) * q + knolls * (1.0 - q);
    (offset, y, if q > 0.5 { quay.3 } else { natural.3 })
}

const CITY_PAINT: [u32; 7] = [
    0xdfe3e4, 0xb8c6cf, 0x8fa4b4, 0xe8ddc9, 0x9db8c4, 0x70869a, 0xc9b9a3,
];

/// Towers across the bay to the east, banded with floors of glazing.
fn city(sectors: &mut Sectors) {
    for i in 0..44u32 {
        let theta = -0.62 + 1.1 * hash(i, 3);
        let inland = 10.0 + 50.0 * hash(i, 5);
        let (x, y, z) = on_land(theta, inland);
        let centre = 1.0 - (theta - (-0.05)).abs() / 0.6;
        let height = 6.0 + hash(i, 7).powi(2) * 30.0 * (0.4 + 0.6 * centre.max(0.0));
        let (w, d) = (5.0 + 5.0 * hash(i, 9), 5.0 + 5.0 * hash(i, 13));
        let color = CITY_PAINT[(hash(i, 17) * CITY_PAINT.len() as f64) as usize % CITY_PAINT.len()];
        let yaw = -theta + hash(i, 19) * 0.3;
        let mut tower = Node::group("");
        // Founded below the slope so no tower floats on the facets.
        // Its floors of windows are the far-shore effect's, drawn on the walls.
        put(
            &mut tower,
            facade(box_part(w, height + FOOTING, d, color, 0.0)),
            0.0,
            (height - FOOTING) / 2.0,
            0.0,
        );
        put(
            &mut tower,
            box_part(w * 0.4, 2.4, d * 0.4, 0x9aa0a0, 0.0),
            w * 0.15,
            height + 1.2,
            0.0,
        );
        // The tall ones carry a mast with an aircraft warning light.
        if height > 22.0 {
            put(
                &mut tower,
                box_part(0.35, 6.0, 0.35, 0xc9cccb, 0.0),
                -w * 0.2,
                height + 3.0,
                0.0,
            );
            put(
                &mut tower,
                box_part(0.9, 0.9, 0.9, WARNING_LIGHT, 0.0),
                -w * 0.2,
                height + 6.2,
                0.0,
            );
        }
        place_at(sectors, theta, rotated(tower, 0.0, yaw, 0.0), x, y - 0.5, z);
    }
    // A low waterfront of sheds and offices in front of the towers.
    for i in 0..26u32 {
        let theta = -0.7 + 1.2 * hash(i, 23);
        let (x, y, z) = on_land(theta, 3.0 + 5.0 * hash(i, 29));
        let (w, h) = (10.0 + 14.0 * hash(i, 31), 3.5 + 4.0 * hash(i, 37));
        let color = CITY_PAINT[(hash(i, 41) * CITY_PAINT.len() as f64) as usize % CITY_PAINT.len()];
        let shed = rotated(
            box_part(w, h + FOOTING, 7.0, scale_hex_color(color, 0.85), 0.0),
            0.0,
            -theta + FRAC_PI_2,
            0.0,
        );
        place_at(sectors, theta, shed, x, y + (h - FOOTING) / 2.0, z);
    }
}

/// How far below its ground point a backdrop structure is founded, metres.
const FOOTING: f64 = 4.0;

const BOX_PAINT: [u32; 6] = [0xc96f3b, 0x3d8a8c, 0x5f7fa6, 0xb7a26e, 0x9b4f45, 0xe0dccd];

/// A neighbouring container terminal to the south-east: stacked boxes and three
/// ship-to-shore cranes with their booms raised.
fn terminal(sectors: &mut Sectors) {
    for i in 0..60u32 {
        let theta = 0.55 + 0.5 * hash(i, 31);
        let inland = 6.0 + 26.0 * hash(i, 37);
        let (x, y, z) = on_land(theta, inland);
        let levels = 1 + (hash(i, 41) * 4.0) as u32;
        let color = BOX_PAINT[(hash(i, 43) * 6.0) as usize % 6];
        let stack = rotated(
            flagged(
                box_part(12.0, 2.6 * f64::from(levels), 2.5, color, 0.0),
                STACK_FLAG,
            ),
            0.0,
            -theta,
            0.0,
        );
        place_at(sectors, theta, stack, x, y + 1.3 * f64::from(levels), z);
    }
    // Ship-to-shore cranes in a row along the berth, of two generations and
    // liveries, some working with their booms down over a ship and some idle with
    // them raised: the terminal's silhouette.
    for i in 0..6u32 {
        let theta = 0.56 + 0.105 * f64::from(i) + 0.03 * hash(i, 47);
        let (x, y, z) = on_land(theta, 3.0);
        let (leg, trim) = CRANE_LIVERIES[(i as usize * 2 + 1) % CRANE_LIVERIES.len()];
        let raise = if hash(i, 53) > 0.45 {
            1.0 + 0.35 * hash(i, 59)
        } else {
            0.0
        };
        let tall = 1.0 + 0.25 * f64::from(i % 2);
        let mut crane = terminal_crane(leg, trim, raise, tall);
        crane = rotated(crane, 0.0, FRAC_PI_2 - theta, 0.0);
        crane.scale = glam::DVec3::splat(0.72);
        place_at(sectors, theta, crane, x, y, z);
    }
    // A feeder ship alongside the working cranes.
    let theta = 0.72;
    let (x, _, z) = on_land(theta, -16.0);
    let ship = rotated(anchored_ship(0, 70.0), 0.0, -theta - FRAC_PI_2, 0.0);
    place_at(sectors, theta, ship, x, SEA_LEVEL - 1.5, z);
}

/// Crane paint: legs and boom, and the machinery house trim.
const CRANE_LIVERIES: [(u32, u32); 3] = [
    (0xa8342c, 0xe5e2da),
    (0x2f5f8a, 0xe5e2da),
    (0xd9a63a, 0x3b4b54),
];

/// A terminal's ship-to-shore crane in its own frame, the boom reaching toward
/// -z: four legs, the portal, an A-frame and a boom that is `raise` radians up
/// from level, with warning lights at the apex and the boom tip.
fn terminal_crane(leg: u32, trim: u32, raise: f64, tall: f64) -> Node {
    let mut crane = Node::group("");
    let top = 24.0 * tall;
    for (lx, lz) in [(-5.0, -6.0), (5.0, -6.0), (-5.0, 6.0), (5.0, 6.0)] {
        put(
            &mut crane,
            box_part(1.0, top + FOOTING, 1.0, leg, 0.0),
            lx,
            (top - FOOTING) / 2.0,
            lz,
        );
    }
    for lz in [-6.0, 6.0] {
        put(
            &mut crane,
            box_part(11.0, 1.2, 1.0, leg, 0.0),
            0.0,
            top - 0.5,
            lz,
        );
        put(&mut crane, box_part(11.0, 0.7, 0.8, leg, 0.0), 0.0, 6.0, lz);
        beam(&mut crane, [-5.0, 6.0, lz], [5.0, top - 1.0, lz], 0.5, leg);
    }
    let apex = [0.0, top + 12.0, 1.0];
    let hinge = [0.0, top + 0.5, -6.0];
    let reach = 40.0;
    let tip = [
        0.0,
        hinge[1] + reach * raise.sin(),
        hinge[2] - reach * raise.cos(),
    ];
    for lx in [-4.0, 4.0] {
        beam(
            &mut crane,
            [lx, top, -6.0],
            [lx * 0.3, apex[1], apex[2]],
            0.8,
            leg,
        );
        beam(
            &mut crane,
            [lx, top, 6.0],
            [lx * 0.3, apex[1], apex[2]],
            0.8,
            leg,
        );
        let side = [lx * 0.3, 0.0, 0.0];
        beam(
            &mut crane,
            [side[0], hinge[1], hinge[2]],
            [side[0], tip[1], tip[2]],
            1.0,
            leg,
        );
        // Stays from the apex to the boom's middle and tip.
        let middle = [
            side[0],
            (hinge[1] + tip[1]) / 2.0,
            (hinge[2] + tip[2]) / 2.0,
        ];
        beam(&mut crane, [lx * 0.3, apex[1], apex[2]], middle, 0.3, trim);
        beam(
            &mut crane,
            [lx * 0.3, apex[1], apex[2]],
            [side[0], tip[1], tip[2]],
            0.3,
            trim,
        );
        // Backreach behind the portal, carrying the machinery house.
        beam(
            &mut crane,
            [side[0], top + 0.5, 6.0],
            [side[0], top + 0.5, 14.0],
            0.9,
            leg,
        );
    }
    put(
        &mut crane,
        box_part(12.0, 1.4, 14.0, leg, 0.0),
        0.0,
        top + 0.6,
        0.0,
    );
    put(
        &mut crane,
        box_part(6.0, 3.4, 6.0, trim, 0.0),
        0.0,
        top + 2.9,
        10.5,
    );
    put(
        &mut crane,
        box_part(1.2, 1.2, 1.2, WARNING_LIGHT, 0.0),
        0.0,
        apex[1] + 0.9,
        apex[2],
    );
    put(
        &mut crane,
        box_part(1.2, 1.2, 1.2, WARNING_LIGHT, 0.0),
        0.0,
        tip[1] + 0.9,
        tip[2],
    );
    if raise == 0.0 {
        // Working: the trolley out over the ship with a box on the spreader.
        put(
            &mut crane,
            box_part(3.0, 1.6, 3.0, trim, 0.0),
            0.0,
            top - 0.6,
            -24.0,
        );
        put(
            &mut crane,
            box_part(0.2, 9.0, 0.2, 0x2b3236, 0.0),
            0.0,
            top - 5.5,
            -24.0,
        );
        put(
            &mut crane,
            box_part(2.6, 2.6, 6.1, BOX_PAINT[(tall * 7.0) as usize % 6], 0.0),
            0.0,
            top - 11.0,
            -24.0,
        );
    }
    crane
}

/// A ship riding at anchor or lying alongside, `length` metres long, in its own
/// frame with the bow toward +x: a hull with a boot-top, the bridge aft, and either
/// container stacks or a tanker's pipe deck and manifold.
fn anchored_ship(variant: u32, length: f64) -> Node {
    let mut ship = Node::group("");
    let beam_width = length * 0.15;
    let hull = [0x2e3e4c, 0x7a2f2a, 0x31503f, 0x2b2f33][variant as usize % 4];
    put(
        &mut ship,
        box_part(length, 3.0, beam_width, 0x8a2f25, 0.0),
        0.0,
        0.5,
        0.0,
    );
    put(
        &mut ship,
        box_part(length, 5.0, beam_width, hull, 0.0),
        0.0,
        4.5,
        0.0,
    );
    // The bow, raked forward.
    let bow = rotated(
        box_part(beam_width * 0.75, 5.5, beam_width * 0.75, hull, 0.0),
        0.0,
        PI / 4.0,
        0.0,
    );
    put(&mut ship, bow, length / 2.0, 4.3, 0.0);
    put(
        &mut ship,
        box_part(9.0, 9.0, beam_width * 0.9, 0xeeeae0, 0.0),
        -length * 0.38,
        11.5,
        0.0,
    );
    put(
        &mut ship,
        box_part(9.4, 1.4, beam_width * 1.15, 0x2c3a44, 0.0),
        -length * 0.38,
        15.0,
        0.0,
    );
    put(
        &mut ship,
        box_part(2.6, 5.0, 3.2, 0xc8473b, 0.0),
        -length * 0.44,
        17.0,
        0.0,
    );
    put(
        &mut ship,
        box_part(0.4, 9.0, 0.4, 0xd9d6cc, 0.0),
        -length * 0.36,
        20.0,
        0.0,
    );
    put(
        &mut ship,
        box_part(0.9, 0.9, 0.9, WARNING_LIGHT, 0.0),
        -length * 0.36,
        24.8,
        0.0,
    );
    if variant % 3 == 2 {
        // Tanker: a pipe deck, the manifold and its derrick.
        put(
            &mut ship,
            box_part(length * 0.7, 0.9, 2.0, 0x6d7477, 0.0),
            length * 0.08,
            7.5,
            0.0,
        );
        put(
            &mut ship,
            box_part(3.0, 2.4, beam_width * 0.8, 0x6d7477, 0.0),
            0.0,
            8.2,
            0.0,
        );
        beam(&mut ship, [2.0, 7.0, 0.0], [6.0, 15.0, 2.0], 0.5, 0xd9d6cc);
    } else {
        let bays = ((length * 0.62) / 7.0) as u32;
        for bay in 0..bays {
            let tiers = 1 + (hash(bay + variant * 31, 151) * 3.0) as u32;
            for row in 0..3u32 {
                let color = BOX_PAINT[(hash(bay * 7 + row + variant * 13, 157) * 6.0) as usize % 6];
                put(
                    &mut ship,
                    box_part(6.0, 2.6 * f64::from(tiers), beam_width * 0.28, color, 0.0),
                    -length * 0.28 + 7.0 * f64::from(bay),
                    7.0 + 1.3 * f64::from(tiers),
                    (f64::from(row) - 1.0) * beam_width * 0.3,
                );
            }
        }
    }
    ship
}

/// Ships riding at anchor in the roads beyond the breakwater: (angle, distance
/// from the arena centre, length, heading, variant).
const ROADS: [(f64, f64, f64, f64, u32); 4] = [
    (-2.02, 262.0, 120.0, 0.4, 0),
    (-1.68, 214.0, 90.0, 2.6, 2),
    (-1.42, 300.0, 140.0, 0.9, 1),
    (-1.12, 236.0, 85.0, 3.6, 3),
];

fn roads(sectors: &mut Sectors) {
    for &(theta, distance, length, heading, variant) in &ROADS {
        let ship = rotated(anchored_ship(variant, length), 0.0, heading, 0.0);
        place_at(
            sectors,
            theta,
            ship,
            distance * theta.cos(),
            SEA_LEVEL - 1.5,
            distance * theta.sin(),
        );
    }
}

/// Fuel tanks and a pair of banded stacks to the south.
fn tank_farm(sectors: &mut Sectors) {
    for i in 0..12u32 {
        let theta = 1.35 + 0.55 * hash(i, 51);
        let inland = 14.0 + 30.0 * hash(i, 53);
        let (x, y, z) = on_land(theta, inland);
        let radius = 2.5 + 2.0 * hash(i, 57);
        let height = 4.0 + 3.0 * hash(i, 59);
        // Each tank stands in its bund: a low concrete wall round a gravel floor.
        let bund = cylinder_part(radius + 2.2, 1.0 + FOOTING, 0x9d988a, 12);
        place_at(sectors, theta, bund, x, y + (1.0 - FOOTING) / 2.0, z);
        let tank = cylinder_part(radius, height + FOOTING, 0xe6e4dc, 16);
        place_at(
            sectors,
            theta,
            tank,
            x,
            y + (height - FOOTING) / 2.0 + 0.6,
            z,
        );
        let roof = cylinder_part(radius + 0.2, 0.8, 0x9fa6a6, 16);
        place_at(sectors, theta, roof, x, y + height + 0.6, z);
        let band = cylinder_part(radius + 0.05, 0.6, 0x2f6f8a, 16);
        place_at(sectors, theta, band, x, y + height * 0.75, z);
    }
    for (i, theta) in [1.62, 1.71].into_iter().enumerate() {
        let (x, y, z) = on_land(theta, 22.0 + 6.0 * i as f64);
        for band in 0..6 {
            let color = if band % 2 == 0 { 0xe9e5dc } else { 0xc0463c };
            let stack = cylinder_part(1.3 - band as f64 * 0.06, 5.0, color, 12);
            place_at(sectors, theta, stack, x, y + 2.5 + band as f64 * 5.0, z);
        }
        place_at(
            sectors,
            theta,
            cylinder_part(1.6, FOOTING, 0x8d8a80, 12),
            x,
            y - FOOTING / 2.0 + 0.2,
            z,
        );
        place_at(
            sectors,
            theta,
            box_part(1.0, 1.0, 1.0, WARNING_LIGHT, 0.0),
            x,
            y + 30.5,
            z,
        );
    }
}

/// Whether an angle is kept clear of woods for the city, the
/// terminal or the tank farm.
fn built_up(theta: f64) -> bool {
    (-0.75..0.52).contains(&theta) || (0.52..1.15).contains(&theta) || (1.3..1.95).contains(&theta)
}

/// A far pine: an open five-sided spire of unit radius and height standing on
/// its base. Seen only from across the bay, it needs no base cap or more sides.
fn pine_mesh() -> Arc<Mesh> {
    static PINE: std::sync::OnceLock<Arc<Mesh>> = std::sync::OnceLock::new();
    PINE.get_or_init(|| {
        let mut positions = vec![0.0, 1.0, 0.0];
        for i in 0..5 {
            let a = TAU * f64::from(i) / 5.0;
            positions.extend([a.cos(), 0.0, a.sin()]);
        }
        let indices = (0..5u32)
            .flat_map(|i| [0, 1 + (i + 1) % 5, 1 + i])
            .collect();
        let mut mesh = Mesh::from_f64(&positions, &[], &[], Some(indices));
        soft_foliage_normals(&mut mesh);
        mesh.uvs = vec![[0.0, TREE_FLAG]; mesh.positions.len()];
        Arc::new(mesh)
    })
    .clone()
}

/// Normals pointing out from a point under a tree's middle, as stylised foliage
/// is shaded: the crown darkens softly away from the light instead of splitting
/// into a lit and a shaded facet that read as a paper cut-out across the bay.
fn soft_foliage_normals(mesh: &mut Mesh) {
    mesh.normals = mesh
        .positions
        .iter()
        .map(|&[x, y, z]| {
            let v = glam::Vec3::new(x, y + 0.6, z).normalize();
            v.to_array()
        })
        .collect();
    mesh.uvs = vec![[0.0, 0.0]; mesh.positions.len()];
}

/// The far-shore effect's `apron_hash(cell).x` (`harbor_apron.wgsl`), so trees can
/// stand where the effect paints its woods.
fn shader_hash(x: f64, y: f64) -> f64 {
    let fract = |v: f64| v - v.floor();
    let mut p = [
        fract(x * 0.1031),
        fract(y * 0.1030),
        fract(x * 0.0973),
        fract(y * 0.1099),
    ];
    let d = p[0] * (p[3] + 33.33)
        + p[1] * (p[2] + 33.33)
        + p[2] * (p[0] + 33.33)
        + p[3] * (p[1] + 33.33);
    p = p.map(|v| v + d);
    fract((p[0] + p[1]) * p[2])
}

/// The effect's `apron_noise`: smoothed value noise over [`shader_hash`].
fn shader_noise(x: f64, y: f64) -> f64 {
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (x - ix, y - iy);
    let (ux, uy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let a = shader_hash(ix, iy);
    let b = shader_hash(ix + 1.0, iy);
    let c = shader_hash(ix, iy + 1.0);
    let d = shader_hash(ix + 1.0, iy + 1.0);
    (a + (b - a) * ux) + ((c + (d - c) * ux) - (a + (b - a) * ux)) * uy
}

/// How far into the effect's painted woods a ground point lies: above 0.5 is
/// under its canopy (`woods` and `woods_edge` in the far-shore mode, on gentle
/// ground).
fn woods_depth(x: f64, z: f64) -> f64 {
    shader_noise(x / 34.0 + 11.0, z / 34.0 + 11.0) * 0.65
        + shader_noise(x / 11.0 + 3.0, z / 11.0 + 3.0) * 0.35
        - 0.03
}

/// Trees in the woods the far-shore effect paints over the headlands: pines,
/// tallest deep in the woods, as dark as the canopy painted round them, so their
/// ragged tops rise out of the painted canopy along the ridges and the woods read
/// as dark masses rather than single trees dotted over the grass.
fn woods(sectors: &mut Sectors) {
    for i in 0..TREE_SITES {
        let theta = -PI + TAU * hash(i, 201);
        if in_open_sea(theta)
            || built_up(theta)
            || land_amount(theta) < 0.6
            || (1.1..2.05).contains(&theta)
        {
            continue;
        }
        let inland = 6.0 + 104.0 * hash(i, 203).powf(0.8);
        let (x, y, z) = on_land(theta, inland);
        let depth = woods_depth(x, z);
        if depth < WOODS_EDGE {
            continue;
        }
        let deep = ((depth - WOODS_EDGE) / 0.12).min(1.0);
        let height = (3.4 + 3.4 * hash(i, 207)) * (0.85 + 0.35 * deep);
        let width = height * (0.3 + 0.1 * hash(i, 223));
        let color = [0x1a2e1d, 0x1f3621, 0x223b22, 0x182a1b][(hash(i, 211) * 4.0) as usize % 4];
        let mut part = Node::mesh(pine_mesh(), paint(color));
        part.scale = glam::DVec3::new(width, height, width);
        place_at(sectors, theta, part, x, y - 0.5, z);
    }
}
/// Points tried for a tree around the bay; only those in the painted woods grow one.
const TREE_SITES: u32 = 3800;
/// The [`woods_depth`] where trees start: just inside the painted woods' edge.
const WOODS_EDGE: f64 = 0.5;

/// The western headland's wind farm: (angle round the bay, metres inland, scale).
/// A pair close together on the near ridge and two more spread further back, each
/// its own size, so they read as a wind farm on the hills rather than a row of
/// clones evenly spaced along the skyline.
const TURBINES: [(f64, f64, f64); 4] = [
    (2.4, 50.0, 0.72),
    (2.53, 66.0, 0.6),
    (3.0, 78.0, 0.52),
    (3.42, 60.0, 0.64),
];

/// Wind turbines along the western headland ridge.
fn turbines(sectors: &mut Sectors) {
    for (i, &(theta, inland, scale)) in TURBINES.iter().enumerate() {
        let i = i as u32;
        let theta = if theta > PI { theta - TAU } else { theta };
        if in_open_sea(theta) || land_amount(theta) < 0.9 {
            continue;
        }
        let (x, y, z) = on_land(theta, inland);
        let mut turbine = Node::group("");
        put(
            &mut turbine,
            cylinder_part(0.7, 32.0 + FOOTING, 0xf1f1ec, 10),
            0.0,
            (32.0 - FOOTING) / 2.0,
            0.0,
        );
        put(
            &mut turbine,
            cylinder_part(2.2, 0.8, 0x9d988a, 10),
            0.0,
            0.1,
            0.0,
        );
        put(
            &mut turbine,
            box_part(0.7, 0.7, 0.7, WARNING_LIGHT, 0.0),
            0.0,
            33.6,
            -0.4,
        );
        put(
            &mut turbine,
            box_part(3.4, 1.8, 1.8, 0xf1f1ec, 0.0),
            0.0,
            32.4,
            0.5,
        );
        let spin = hash(i, 67) * TAU;
        for blade in 0..3 {
            let angle = spin + f64::from(blade) * TAU / 3.0;
            let mut arm = Node::group("");
            put(
                &mut arm,
                box_part(0.55, 15.0, 0.2, 0xf4f4f0, 0.0),
                0.0,
                7.5,
                0.0,
            );
            let arm = rotated(arm, 0.0, 0.0, angle);
            put(&mut turbine, arm, 0.0, 32.4, 1.5);
        }
        let mut turbine = rotated(turbine, 0.0, -theta - FRAC_PI_2, 0.0);
        turbine.scale = glam::DVec3::splat(scale);
        place_at(sectors, theta, turbine, x, y, z);
    }
}

/// A rubble breakwater across the northern sea with a lighthouse at its head.
fn breakwater(sectors: &mut Sectors) {
    let z = -158.0;
    let mut group = Node::group("");
    let group = &mut group;
    put(
        group,
        box_part(150.0, 4.0, 9.0, 0x8c8a80, 0.0),
        -55.0,
        SEA_LEVEL + 1.0,
        z,
    );
    for i in 0..40u32 {
        let x = -128.0 + f64::from(i) * 3.7;
        let size = 2.2 + hash(i, 71) * 1.8;
        let rock = rotated(
            box_part(
                size,
                size * 0.8,
                size,
                scale_hex_color(0x8b877b, 0.85 + hash(i, 73) * 0.3),
                0.0,
            ),
            hash(i, 79) * 0.6,
            hash(i, 83) * TAU,
            hash(i, 89) * 0.6,
        );
        put(group, rock, x, SEA_LEVEL + 1.2, z + 4.2 + hash(i, 97) * 1.2);
    }
    let (lx, lz) = (22.0, z);
    put(
        group,
        cylinder_part(5.0, 3.0, 0x8c8a80, 16),
        lx,
        SEA_LEVEL + 2.0,
        lz,
    );
    for band in 0..4 {
        let color = if band % 2 == 0 { 0xf2efe6 } else { 0xc8433a };
        put(
            group,
            cylinder_part(2.3 - f64::from(band) * 0.2, 4.5, color, 16),
            lx,
            SEA_LEVEL + 5.75 + f64::from(band) * 4.5,
            lz,
        );
    }
    put(
        group,
        cylinder_part(1.6, 2.4, 0xffe7a8, 12),
        lx,
        SEA_LEVEL + 23.0,
        lz,
    );
    put(
        group,
        cylinder_part(1.9, 0.7, 0x2f3b40, 12),
        lx,
        SEA_LEVEL + 24.5,
        lz,
    );
    let group = std::mem::replace(group, Node::group(""));
    place(sectors, -FRAC_PI_2 + 0.1, group);
}

/// Merge a sector into one far-shore mesh (its paint already vertex colors),
/// kept out of the shadow pass.
fn finish(mut sector: Node) -> Node {
    batch(&mut sector);
    let meshes: Vec<Arc<Mesh>> = sector
        .children
        .drain(..)
        .filter_map(|child| child.drawable.map(|drawable| drawable.mesh))
        .collect();
    let merged = merge_geometries(&meshes.iter().map(|mesh| &**mesh).collect::<Vec<_>>())
        .expect("backdrop parts share one vertex layout");
    let mut node = Node::mesh(Arc::new(merged), apron_material(ApronMode::Shore, 0xffffff));
    if let Some(drawable) = &mut node.drawable {
        drawable.cast_shadow = false;
        drawable.receive_shadow = false;
    }
    node
}

/// The far shore around the harbor, as four batched sectors.
pub fn harbor_backdrop() -> Node {
    let mut root = Node::group("harbor-backdrop");
    let mut sectors: Sectors = std::array::from_fn(|_| Node::group(""));
    for (sector, (shore, range)) in sectors
        .iter_mut()
        .zip(shore_meshes().into_iter().zip(range_meshes()))
    {
        sector.children.extend(shore);
        sector.children.extend(range);
    }
    city(&mut sectors);
    terminal(&mut sectors);
    tank_farm(&mut sectors);
    turbines(&mut sectors);
    woods(&mut sectors);
    breakwater(&mut sectors);
    roads(&mut sectors);
    for sector in sectors {
        root.children.push(finish(sector));
    }
    root
}
