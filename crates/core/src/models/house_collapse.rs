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
use super::tank_kit::{Kit, pose};
use crate::geometry::{Mesh, narrow, widen};
use crate::sim::house_fracture::{PartPlane, ShardRegion, clip_vertices};
use crate::sim::house_layout::{PieceShape, RAFTER_DEPTH, Roof};
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
