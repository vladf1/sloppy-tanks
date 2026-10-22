//! A collapsing cottage's shards, cut from the parts it was built of along the
//! cracks [`HouseDesign::pieces`](crate::sim::house_layout::HouseDesign::pieces)
//! fractures it with: each wall and roof slope's meshes are clipped to a shard's
//! region, and every broken edge is closed with torn timber (the wall's thickness
//! from siding to lining, or the roof's from shingles to deck) bristling with
//! splintered boards, a snapped stud or a rafter end, so a fallen panel reads as
//! a heavy broken slab rather than a sheet of card.

use std::sync::Arc;

use glam::{DMat4, DVec2, DVec3};

use super::building_kit::Surface;
use super::tank_kit::{Kit, pose, shift};
use crate::geometry::{Mesh, narrow, widen};
use crate::sim::house_fracture::{Crack, PartPlane, ShardRegion, clip_vertices};
use crate::sim::house_layout::{CHIMNEY, PieceShape, RAFTER_DEPTH, Roof};
use crate::sim::math::Random;

/// How deep a wall's torn edge runs, from the siding back to the lining.
const WALL_EDGE: f64 = 0.2;
/// How far above and below the deck top a roof's torn edge runs: the shingles,
/// and the deck with a lath under it.
const SHINGLE_EDGE: f64 = 0.05;
const DECK_EDGE: f64 = 0.06;
/// Splinters along a torn edge: one about every this many metres.
const SPLINTER_SPACING: f64 = 0.38;

#[derive(Clone, Copy)]
struct Vertex {
    p: DVec3,
    n: DVec3,
    uv: DVec2,
}

fn mix(a: &Vertex, b: &Vertex, k: f64) -> Vertex {
    Vertex {
        p: a.p.lerp(b.p, k),
        n: a.n.lerp(b.n, k).normalize_or_zero(),
        uv: a.uv.lerp(b.uv, k),
    }
}

/// `mesh`'s share in `region` of its part's `plane`: triangles inside it and clear
/// of its cracks are kept whole, the rest are clipped cell by cell. None when
/// nothing of it is left.
pub(super) fn clip_mesh(mesh: &Mesh, plane: &PartPlane, region: &ShardRegion) -> Option<Mesh> {
    let vertex = |i: usize| Vertex {
        p: widen(mesh.positions[i]),
        n: widen(mesh.normals[i]),
        uv: DVec2::new(f64::from(mesh.uvs[i][0]), f64::from(mesh.uvs[i][1])),
    };
    let count = mesh.triangle_count();
    let corner = |t: usize, k: usize| match &mesh.indices {
        Some(indices) => indices[t * 3 + k] as usize,
        None => t * 3 + k,
    };
    let project = |v: &Vertex| plane.project(v.p);
    let mut out = Mesh::default();
    let mut emit = |polygon: &[Vertex]| {
        for i in 1..polygon.len().saturating_sub(1) {
            for v in [polygon[0], polygon[i], polygon[i + 1]] {
                out.positions.push(narrow(v.p));
                out.normals.push(narrow(v.n));
                out.uvs.push([v.uv.x as f32, v.uv.y as f32]);
            }
        }
    };
    for t in 0..count {
        let triangle = [0, 1, 2].map(|k| vertex(corner(t, k)));
        let points = triangle.map(|v| project(&v));
        let whole = points.iter().all(|q| region.contains(*q)) && !region.straddles(&points);
        if whole {
            emit(&triangle);
            continue;
        }
        for cell in &region.cells {
            let mut polygon = triangle.to_vec();
            for side in cell {
                polygon = clip_vertices(&polygon, side, project, mix);
                if polygon.len() < 3 {
                    break;
                }
            }
            if polygon.len() >= 3 {
                emit(&polygon);
            }
        }
    }
    (!out.positions.is_empty()).then_some(out)
}

/// The parts of the segment `a`–`b` (plane points) outside every opening
/// (`[s0, t0, s1, t1]` rectangles), where a wall's torn edge shows.
fn outside_openings(a: DVec2, b: DVec2, openings: &[[f64; 4]]) -> Vec<(DVec2, DVec2)> {
    let mut spans = vec![(0.0, 1.0)];
    let d = b - a;
    for o in openings {
        // Liang-Barsky: the segment's span inside the rectangle.
        let (mut enter, mut leave) = (0.0_f64, 1.0_f64);
        for (p, q) in [
            (-d.x, a.x - o[0]),
            (d.x, o[2] - a.x),
            (-d.y, a.y - o[1]),
            (d.y, o[3] - a.y),
        ] {
            if p.abs() < 1e-12 {
                if q < 0.0 {
                    enter = 1.0;
                    leave = 0.0;
                }
            } else if p < 0.0 {
                enter = enter.max(q / p);
            } else {
                leave = leave.min(q / p);
            }
        }
        if enter >= leave {
            continue;
        }
        spans = spans
            .into_iter()
            .flat_map(|(from, to): (f64, f64)| {
                let mut kept = Vec::new();
                if enter > from {
                    kept.push((from, enter.min(to)));
                }
                if leave < to {
                    kept.push((leave.max(from), to));
                }
                kept.into_iter().filter(|(f, t)| t - f > 1e-3)
            })
            .collect();
    }
    spans
        .into_iter()
        .map(|(from, to)| (a + d * from, a + d * to))
        .collect()
}

/// The unit plane direction out of the shard across its edge `a`–`b`.
fn outward(region: &ShardRegion, a: DVec2, b: DVec2) -> DVec2 {
    let d = (b - a).normalize_or_zero();
    let n = DVec2::new(d.y, -d.x);
    let mid = (a + b) / 2.0;
    if region.contains(mid + n * 0.02) {
        -n
    } else {
        n
    }
}

/// Torn timber along a wall shard's broken edges: the cut through siding, sheathing
/// and lining between its openings, splintered clapboards sticking out of it and a
/// snapped stud. `openings` are the wall's openings and shell holes.
pub(super) fn wall_breaks(
    kit: &mut Kit<Surface>,
    shape: &PieceShape,
    openings: &[[f64; 4]],
    rng: &mut Random,
) {
    let plane = &shape.plane;
    let out = plane.s.cross(plane.t);
    let at = |q: DVec2, depth: f64| plane.origin + plane.s * q.x + plane.t * q.y - out * depth;
    for (a, b) in shape.region.broken_edges(&shape.outline) {
        let n = outward(&shape.region, a, b);
        let facing = plane.s * n.x + plane.t * n.y;
        for (a, b) in outside_openings(a, b, openings) {
            kit.quad(
                Surface::Splinters,
                [at(a, 0.0), at(b, 0.0), at(b, WALL_EDGE), at(a, WALL_EDGE)],
                facing,
            );
            let length = (b - a).length();
            let count = (length / SPLINTER_SPACING).round().max(1.0) as u32;
            for i in 0..count {
                let q = a + (b - a) * ((f64::from(i) + rng.range(0.25, 0.75)) / f64::from(count));
                let reach = rng.range(0.06, 0.3);
                // A clapboard torn off long, bent a little out of the face.
                let along = plane.s * n.x + plane.t * n.y;
                let up = plane.s * -n.y + plane.t * n.x;
                let center = at(q + n * reach / 2.0, 0.015) + out * rng.range(0.0, 0.04);
                let frame = DMat4::from_cols(
                    along.extend(0.0),
                    up.extend(0.0),
                    out.extend(0.0),
                    center.extend(1.0),
                ) * pose(
                    DVec3::ZERO,
                    DVec3::new(
                        rng.range(-0.3, 0.3),
                        rng.range(-0.25, 0.25),
                        rng.range(-0.2, 0.2),
                    ),
                );
                kit.block(
                    if rng.next() < 0.75 {
                        Surface::Siding
                    } else {
                        Surface::Splinters
                    },
                    DVec3::new(reach, rng.range(0.07, 0.13), 0.025),
                    frame,
                );
            }
            if length > 0.5 {
                // A stud snapped across the crack, standing out of the torn edge.
                let q = a + (b - a) * rng.range(0.3, 0.7);
                let reach = rng.range(0.18, 0.42);
                let along = plane.s * n.x + plane.t * n.y;
                let up = plane.s * -n.y + plane.t * n.x;
                let center = at(q + n * (reach / 2.0 - 0.05), WALL_EDGE + 0.03);
                let frame = DMat4::from_cols(
                    along.extend(0.0),
                    up.extend(0.0),
                    out.extend(0.0),
                    center.extend(1.0),
                ) * pose(DVec3::ZERO, DVec3::new(0.0, 0.0, rng.range(-0.35, 0.35)));
                kit.block(Surface::Wood, DVec3::new(reach, 0.09, 0.06), frame);
            }
        }
    }
}

/// Torn decking along a roof slab's broken edges: the cut through shingles and
/// deck, broken shingles and boards hanging out of it and a rafter end.
pub(super) fn roof_breaks(
    kit: &mut Kit<Surface>,
    shape: &PieceShape,
    roof: &Roof,
    rng: &mut Random,
) {
    let plane = &shape.plane;
    let at = |q: DVec2, lift: f64| {
        let p = plane.origin + plane.s * q.x + plane.t * q.y;
        DVec3::new(p.x, roof.top(p.x, p.z) + lift, p.z)
    };
    for (a, b) in shape.region.broken_edges(&shape.outline) {
        let n = outward(&shape.region, a, b);
        let facing = plane.s * n.x + plane.t * n.y;
        let depth = roof.thick + DECK_EDGE;
        kit.quad(
            Surface::Splinters,
            [
                at(a, SHINGLE_EDGE),
                at(b, SHINGLE_EDGE),
                at(b, -depth),
                at(a, -depth),
            ],
            facing,
        );
        let length = (b - a).length();
        let count = (length / SPLINTER_SPACING).round().max(1.0) as u32;
        for i in 0..count {
            let q = a + (b - a) * ((f64::from(i) + rng.range(0.25, 0.75)) / f64::from(count));
            let reach = rng.range(0.08, 0.32);
            let tip = at(q + n * reach, 0.0);
            let root = at(q, rng.range(-roof.thick, SHINGLE_EDGE * 0.5));
            let droop = DVec3::Y * rng.range(-0.12, 0.02);
            let coat = if rng.next() < 0.6 {
                Surface::Roof
            } else {
                Surface::Splinters
            };
            kit.block(
                coat,
                DVec3::new(rng.range(0.1, 0.22), 0.025, (tip + droop - root).length()),
                aim(root, tip + droop),
            );
        }
        if length > 0.6 {
            // A rafter snapped where the deck tore, its end jutting out below.
            let q = a + (b - a) * rng.range(0.3, 0.7);
            let reach = rng.range(0.2, 0.5);
            let root = at(q - n * 0.2, -roof.thick - RAFTER_DEPTH / 2.0);
            let tip = at(q + n * reach, -roof.thick - RAFTER_DEPTH / 2.0)
                - DVec3::Y * rng.range(0.0, 0.15);
            kit.block(
                Surface::Wood,
                DVec3::new(0.06, RAFTER_DEPTH, (tip - root).length()),
                aim(root, tip),
            );
        }
    }
}

/// Broken brickwork where the chimney snapped (`snap` high over the flue at
/// `flue`): the cut course across the stack, on the stump's top or under the
/// top's foot, with bricks knocked half out of it and a ragged course standing
/// proud on two sides, so the break reads as masonry rather than a sawn post.
pub(super) fn chimney_breaks(
    kit: &mut Kit<Surface>,
    shape: &PieceShape,
    flue: DVec3,
    snap: f64,
    rng: &mut Random,
) {
    const BRICK: DVec3 = DVec3::new(0.22, 0.075, 0.11);
    let half = CHIMNEY / 2.0;
    // The stump's break faces up, the top's down.
    let face = if shape.piece.shard == 0 { 1.0 } else { -1.0 };
    let at = |x: f64, z: f64, lift: f64| DVec3::new(flue.x + x, snap + face * lift, flue.z + z);
    kit.quad(
        Surface::Brick,
        [
            at(-half, -half, 0.0),
            at(half, -half, 0.0),
            at(half, half, 0.0),
            at(-half, half, 0.0),
        ],
        DVec3::Y * face,
    );
    // A course still standing along two sides, broken off at odd heights.
    for side in [-1.0, 1.0] {
        let x = side * (half - BRICK.z / 2.0);
        for k in 0..3 {
            if rng.next() < 0.3 {
                continue;
            }
            let z = -half + BRICK.x / 2.0 + f64::from(k) * (CHIMNEY - BRICK.x) / 2.0;
            let rise = rng.range(0.5, 1.0) * BRICK.y;
            kit.block(
                Surface::Brick,
                DVec3::new(BRICK.z, rise, BRICK.x),
                shift(at(x, z, rise / 2.0)),
            );
        }
    }
    // Bricks knocked half out of the cut.
    for _ in 0..3 {
        let (x, z) = (rng.range(-0.15, 0.15), rng.range(-0.15, 0.15));
        kit.block(
            Surface::Brick,
            BRICK,
            pose(
                at(x, z, BRICK.y * 0.4),
                DVec3::new(
                    rng.range(-0.5, 0.5),
                    rng.range(0.0, std::f64::consts::PI),
                    rng.range(-0.5, 0.5),
                ),
            ),
        );
    }
}

/// A shard's inner breaks: shards narrower than this (m across) stay in one
/// piece, and halves narrower than the second width break no further; how far
/// (rad) the two halves of a wall shard and of a roof slab fold against each
/// other along a break, how far its crack wanders off straight (m), and how far
/// (m) the halves slip along it.
const FOLD_WIDTH: f64 = 1.1;
const REFOLD_WIDTH: f64 = 1.5;
const WALL_FOLD: (f64, f64) = (0.2, 0.4);
const ROOF_FOLD: (f64, f64) = (0.2, 0.38);
const FOLD_WANDER: f64 = 0.18;
const FOLD_SLIP: f64 = 0.04;

/// A wall shard or broken roof slab cracked once more across itself and folded
/// along the crack, as a falling panel buckles where it is weakest: its two
/// halves (each the shard's shape cut to one side, so their torn edges and
/// splinters close the new crack too) with the fold that bends each about the
/// crack. Narrow shards, whole parts and the chimney come back unbroken. The
/// shard's collider stays its flat core: the fold only bends its look a little out
/// of that plane, so it beds into the pile rather than lying on it like a card.
pub(super) fn fold_shard(
    shape: &PieceShape,
    roof: &Roof,
    rng: &mut Random,
) -> Vec<(PieceShape, DMat4)> {
    let mut pieces = Vec::new();
    for (half, fold) in fold_once(shape, roof, rng, FOLD_WIDTH) {
        // A wide half breaks again inside itself, folding in its own frame
        // before the first fold carries it.
        for (quarter, inner) in fold_once(&half, roof, rng, REFOLD_WIDTH) {
            pieces.push((quarter, fold * inner));
        }
    }
    pieces
}

/// One inner break of [`fold_shard`]: the shape's two halves and their folds, or
/// the shape whole when it is narrower than `width`.
fn fold_once(
    shape: &PieceShape,
    roof: &Roof,
    rng: &mut Random,
    width: f64,
) -> Vec<(PieceShape, DMat4)> {
    let part = shape.piece.part;
    let wall = part.wall_index().is_some();
    let unbroken = vec![(shape.clone(), DMat4::IDENTITY)];
    if !(wall || part.is_roof()) || shape.region.is_whole() {
        return unbroken;
    }
    let points = shape.region.outline_points(&shape.outline);
    let Some(lo) = points.iter().copied().reduce(DVec2::min) else {
        return unbroken;
    };
    let hi = points.iter().copied().reduce(DVec2::max).unwrap_or(lo);
    let span = hi - lo;
    if span.x < width || span.y < 0.6 {
        return unbroken;
    }
    let s = lo.x + span.x * rng.range(0.38, 0.62);
    let knots = [0.25, 0.5, 0.75].map(|f| lo.y + span.y * f);
    let crack = Crack::new(s, &knots, || rng.range(-FOLD_WANDER, FOLD_WANDER));
    let [left, right] = [
        ShardRegion::between(None, Some(&crack), None),
        ShardRegion::between(Some(&crack), None, None),
    ]
    .map(|side| shape.region.intersect(&side.cells));
    // The hinge: the crack's line through the middle of the wall's or deck's
    // thickness.
    let plane = &shape.plane;
    let out = plane.s.cross(plane.t);
    let middle = |q: DVec2| {
        let p = plane.origin + plane.s * q.x + plane.t * q.y;
        if wall {
            p - out * WALL_EDGE / 2.0
        } else {
            DVec3::new(p.x, roof.top(p.x, p.z) - roof.thick / 2.0, p.z)
        }
    };
    let a = middle(DVec2::new(crack.at(lo.y), lo.y));
    let b = middle(DVec2::new(crack.at(hi.y), hi.y));
    let axis = (b - a).normalize_or(DVec3::Y);
    let pivot = (a + b) / 2.0;
    let (low, high) = if wall { WALL_FOLD } else { ROOF_FOLD };
    let sign = if rng.next() < 0.5 { 1.0 } else { -1.0 };
    let angle = rng.range(low, high) * sign;
    let slip = axis * rng.range(-FOLD_SLIP, FOLD_SLIP);
    let bend = |angle: f64| {
        DMat4::from_translation(pivot)
            * DMat4::from_axis_angle(axis, angle)
            * DMat4::from_translation(-pivot)
    };
    vec![
        (
            PieceShape {
                region: left,
                ..shape.clone()
            },
            bend(angle / 2.0),
        ),
        (
            PieceShape {
                region: right,
                ..shape.clone()
            },
            DMat4::from_translation(slip) * bend(-angle / 2.0),
        ),
    ]
}

/// How much a collapse piece's paint varies across it (darkest, range): patches
/// of dust, soot and damp over every finish, so a broken panel reads as a worn,
/// filthy board rather than a clean painted card.
const MOTTLE: (f32, f32) = (0.7, 0.36);
/// Size (m) of the patches.
const MOTTLE_SCALE: f64 = 0.8;

/// Mottle a batched collapse piece's vertex colours (positions in its own frame;
/// `seed` keeps neighbouring pieces' patches apart).
pub(super) fn mottle(mesh: &mut Mesh, seed: f64) {
    if mesh.colors.len() != mesh.positions.len() {
        return;
    }
    let offset = DVec3::new(seed * 7.31, seed * 3.17, seed * 5.53);
    for (color, p) in mesh.colors.iter_mut().zip(&mesh.positions) {
        let q = widen(*p) / MOTTLE_SCALE + offset;
        let patch = value_noise(q) * 0.7 + value_noise(q * 2.3 + 17.0) * 0.3;
        let shade = MOTTLE.0 + MOTTLE.1 * patch as f32;
        for channel in color.iter_mut() {
            *channel *= shade;
        }
    }
}

/// Smooth 3D value noise in [0, 1] on a unit lattice.
fn value_noise(p: DVec3) -> f64 {
    let cell = p.floor();
    let f = p - cell;
    let s = f * f * (DVec3::splat(3.0) - 2.0 * f);
    let corner = |dx: f64, dy: f64, dz: f64| {
        let q = cell + DVec3::new(dx, dy, dz);
        let mut h = (q.x as i64).wrapping_mul(73_856_093)
            ^ (q.y as i64).wrapping_mul(19_349_663)
            ^ (q.z as i64).wrapping_mul(83_492_791);
        h = (h ^ (h >> 13)).wrapping_mul(0x5bd1_e995);
        (h ^ (h >> 15)).rem_euclid(1000) as f64 / 999.0
    };
    let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let x00 = lerp(corner(0.0, 0.0, 0.0), corner(1.0, 0.0, 0.0), s.x);
    let x10 = lerp(corner(0.0, 1.0, 0.0), corner(1.0, 1.0, 0.0), s.x);
    let x01 = lerp(corner(0.0, 0.0, 1.0), corner(1.0, 0.0, 1.0), s.x);
    let x11 = lerp(corner(0.0, 1.0, 1.0), corner(1.0, 1.0, 1.0), s.x);
    lerp(lerp(x00, x10, s.y), lerp(x01, x11, s.y), s.z)
}

/// A placement whose local +z runs from `from` to `to`, centred between them.
fn aim(from: DVec3, to: DVec3) -> DMat4 {
    super::tank_kit::aim(from, to)
        * DMat4::from_translation(DVec3::Z * ((to - from).length() / 2.0))
}

/// Meshes cut to a shard, each with its finish.
pub(super) fn clip_meshes<C: Copy>(
    meshes: &[(C, Arc<Mesh>)],
    shape: &PieceShape,
) -> Vec<(C, Arc<Mesh>)> {
    meshes
        .iter()
        .filter_map(|(coat, mesh)| {
            clip_mesh(mesh, &shape.plane, &shape.region).map(|mesh| (*coat, Arc::new(mesh)))
        })
        .collect()
}
