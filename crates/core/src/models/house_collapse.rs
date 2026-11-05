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
use crate::sim::house_fracture::{PartPlane, ShardRegion, Side, clip_vertices_into};
use crate::sim::house_layout::{CHIMNEY, PieceShape, RAFTER_DEPTH, Roof};
use crate::sim::math::Random;

/// How deep a wall's torn edge runs, from the siding back to the lining.
const WALL_EDGE: f64 = 0.2;
/// A roof's torn edge: the top of its covering over the deck (no higher than its
/// sheet), and how far below the deck top it runs, through the deck and a lath.
const COVER_TOP: f64 = 0.004;
/// How far down the torn covering's colour runs into a roof's torn edge.
const COVER_LIP: f64 = 0.05;
const DECK_EDGE: f64 = 0.06;
/// Splinters along a torn edge: one about every this many metres.
const SPLINTER_SPACING: f64 = 0.38;
/// How far under a roof's top a splinter's tip lies.
const SPLINTER_SINK: f64 = 0.07;
/// Metres past a cell's side where a triangle is skipped unclipped: far beyond
/// `clip_vertices`' tolerance, so clipping it would have left nothing either.
const CLEARLY_OUTSIDE: f64 = 1e-6;
/// Metres from a part's plane origin past any of its shards' cells.
const PLANE_REACH: f64 = 1000.0;

/// A vertex being clipped, with its point in the part's plane (`PartPlane::project`
/// of `p`), worked out once rather than at every side a polygon is clipped against.
#[derive(Clone, Copy)]
struct Vertex {
    p: DVec3,
    n: DVec3,
    /// Where it is drawn and its normal there: its twin's in the settled mesh.
    shown: DVec3,
    shown_n: DVec3,
    uv: DVec2,
    /// Its vertex colour, when the mesh has them.
    color: DVec3,
    q: DVec2,
}

fn mix(plane: &PartPlane, a: &Vertex, b: &Vertex, k: f64) -> Vertex {
    let p = a.p.lerp(b.p, k);
    Vertex {
        p,
        n: a.n.lerp(b.n, k).normalize_or_zero(),
        shown: a.shown.lerp(b.shown, k),
        shown_n: a.shown_n.lerp(b.shown_n, k).normalize_or_zero(),
        uv: a.uv.lerp(b.uv, k),
        color: a.color.lerp(b.color, k),
        q: plane.project(p),
    }
}

/// A shard's region of its part's plane, ready to cut the part's meshes to: each
/// cell with its bounding box in the plane, and the bounds of them all. The boxes
/// depend only on the region, so the part's meshes, and its shingles, share one set:
/// working them out again for each mesh cost most of a cottage's collapse.
pub(super) struct ShardClip<'a> {
    plane: &'a PartPlane,
    region: &'a ShardRegion,
    cells: Vec<(&'a [Side], [DVec2; 2])>,
    min: DVec2,
    max: DVec2,
}

impl<'a> ShardClip<'a> {
    pub(super) fn new(plane: &'a PartPlane, region: &'a ShardRegion) -> Self {
        let cells: Vec<(&[Side], [DVec2; 2])> = region
            .cells
            .iter()
            .map(|cell| (&cell[..], cell_bounds(cell)))
            .collect();
        let [min, max] = cells.iter().fold(
            [DVec2::INFINITY, DVec2::NEG_INFINITY],
            |[low, high], (_, [min, max])| [low.min(*min), high.max(*max)],
        );
        Self {
            plane,
            region,
            cells,
            min,
            max,
        }
    }
}

/// `mesh`'s share in a shard (`clip`), drawn as `shown`: triangles inside it and clear
/// of its cracks are kept whole, the rest are clipped cell by cell. None when nothing
/// of it is left. `mesh` is a part as it was built, where the shard's cells are, and
/// `shown` the same triangles moved as the frame gave: a clipped corner lies where it
/// lay on the settled triangle, so the shards together are exactly the cottage that
/// stood.
pub(super) fn clip_settled(mesh: &Mesh, shown: &Mesh, clip: &ShardClip) -> Option<Mesh> {
    let ShardClip {
        plane,
        region,
        cells,
        min: region_min,
        max: region_max,
    } = clip;
    let (region_min, region_max) = (*region_min, *region_max);
    let coloured = mesh.colors.len() == mesh.positions.len();
    let vertex = |i: usize, q: DVec2| Vertex {
        p: widen(mesh.positions[i]),
        n: widen(mesh.normals[i]),
        shown: widen(shown.positions[i]),
        shown_n: widen(shown.normals[i]),
        uv: DVec2::new(f64::from(mesh.uvs[i][0]), f64::from(mesh.uvs[i][1])),
        color: if coloured {
            widen(mesh.colors[i])
        } else {
            DVec3::ZERO
        },
        q,
    };
    let count = mesh.triangle_count();
    let corner = |t: usize, k: usize| match &mesh.indices {
        Some(indices) => indices[t * 3 + k] as usize,
        None => t * 3 + k,
    };
    let project = |v: &Vertex| v.q;
    let mix = |a: &Vertex, b: &Vertex, k: f64| mix(plane, a, b, k);
    let mut out = Mesh::default();
    let mut emit = |polygon: &[Vertex]| {
        for i in 1..polygon.len().saturating_sub(1) {
            for v in [polygon[0], polygon[i], polygon[i + 1]] {
                out.positions.push(narrow(v.shown));
                out.normals.push(narrow(v.shown_n));
                out.uvs.push([v.uv.x as f32, v.uv.y as f32]);
                if coloured {
                    out.colors.push(narrow(v.color));
                }
            }
        }
    };
    let (mut polygon, mut clipped) = (Vec::new(), Vec::new());
    for t in 0..count {
        let corners = [0, 1, 2].map(|k| corner(t, k));
        let points = corners.map(|i| plane.project(widen(mesh.positions[i])));
        let low = points[0].min(points[1]).min(points[2]) - CLEARLY_OUTSIDE;
        let high = points[0].max(points[1]).max(points[2]) + CLEARLY_OUTSIDE;
        // A triangle clear of a cell's bounds, or wholly beyond one of its sides, leaves
        // nothing in it. Most of a part's triangles lie outside most shards, and
        // clipping each through every side made a cottage's collapse cost a frozen half
        // second.
        if low.cmpgt(region_max).any() || high.cmplt(region_min).any() {
            continue;
        }
        let reaches = |(cell, [min, max]): &&(&[Side], [DVec2; 2])| {
            low.cmple(*max).all()
                && high.cmpge(*min).all()
                && !cell
                    .iter()
                    .any(|side| points.iter().all(|q| side.distance(*q) < -CLEARLY_OUTSIDE))
        };
        if !cells.iter().any(|cell| reaches(&cell)) {
            continue;
        }
        let triangle = [0, 1, 2].map(|k| vertex(corners[k], points[k]));
        let whole = points.iter().all(|q| region.contains(*q)) && !region.straddles(&points);
        if whole {
            emit(&triangle);
            continue;
        }
        for &(cell, _) in cells.iter().filter(reaches) {
            polygon.clear();
            polygon.extend_from_slice(&triangle);
            for side in cell.iter() {
                clip_vertices_into(&polygon, side, project, mix, &mut clipped);
                std::mem::swap(&mut polygon, &mut clipped);
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

/// The corners of `cell`'s bounding box in its part's plane, clipped from a square
/// reaching past every part; inverted (empty) when nothing is inside.
fn cell_bounds(cell: &[Side]) -> [DVec2; 2] {
    let mut polygon = Vec::with_capacity(4 + cell.len());
    polygon.extend(
        [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
            .map(|(s, t)| DVec2::new(s, t) * PLANE_REACH),
    );
    let mut clipped = Vec::with_capacity(polygon.capacity());
    for side in cell {
        clip_vertices_into(
            &polygon,
            side,
            |q| *q,
            |a, b, k| a.lerp(*b, k),
            &mut clipped,
        );
        std::mem::swap(&mut polygon, &mut clipped);
    }
    polygon
        .iter()
        .fold([DVec2::INFINITY, DVec2::NEG_INFINITY], |[min, max], q| {
            [min.min(*q), max.max(*q)]
        })
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

/// Whether something reaching `reach` from `q` (a wall's plane) could show in one of
/// its `openings`.
fn near_opening(q: DVec2, reach: f64, openings: &[[f64; 4]]) -> bool {
    let r = reach + 0.1;
    openings
        .iter()
        .any(|o| q.x > o[0] - r && q.x < o[2] + r && q.y > o[1] - r && q.y < o[3] + r)
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
            // The cut through the wall: charred and shadowed timber, a dark hairline
            // while the cottage still stands in its shards.
            kit.quad(
                Surface::Dark,
                [at(a, 0.0), at(b, 0.0), at(b, WALL_EDGE), at(a, WALL_EDGE)],
                facing,
            );
            let length = (b - a).length();
            let count = (length / SPLINTER_SPACING).round().max(1.0) as u32;
            for i in 0..count {
                let q = a + (b - a) * ((f64::from(i) + rng.range(0.25, 0.75)) / f64::from(count));
                let reach = rng.range(0.06, 0.3);
                // A board torn off long inside the wall's thickness, between the
                // siding and the lining: hidden while the cottage stands whole, so
                // its facade shows the breaches and not every crack it will part
                // along, and bristling from the torn edge once the shard falls.
                let along = plane.s * n.x + plane.t * n.y;
                let up = plane.s * -n.y + plane.t * n.x;
                let center = at(q + n * reach / 2.0, rng.range(0.05, 0.15));
                let frame = DMat4::from_cols(
                    along.extend(0.0),
                    up.extend(0.0),
                    out.extend(0.0),
                    center.extend(1.0),
                ) * pose(
                    DVec3::ZERO,
                    DVec3::new(
                        rng.range(-0.08, 0.08),
                        rng.range(-0.08, 0.08),
                        rng.range(-0.35, 0.35),
                    ),
                );
                let coat = if rng.next() < 0.75 {
                    Surface::Siding
                } else {
                    Surface::Splinters
                };
                let size = DVec3::new(reach, rng.range(0.07, 0.13), 0.025);
                // None by an opening, whose reveal would show it while the cottage
                // still stands.
                if !near_opening(q, reach, openings) {
                    kit.block(coat, size, frame);
                }
            }
            if length > 0.5 {
                // A stud snapped across the crack, standing out of the torn edge
                // inside the wall's thickness, so the standing shards hide it (and no
                // window shows it) until they part.
                let q = a + (b - a) * rng.range(0.3, 0.7);
                let reach = rng.range(0.12, 0.2);
                let along = plane.s * n.x + plane.t * n.y;
                let up = plane.s * -n.y + plane.t * n.x;
                let center = at(q + n * (reach / 2.0 - 0.05), WALL_EDGE * 0.5);
                let frame = DMat4::from_cols(
                    along.extend(0.0),
                    up.extend(0.0),
                    out.extend(0.0),
                    center.extend(1.0),
                ) * pose(DVec3::ZERO, DVec3::new(0.0, 0.0, rng.range(-0.35, 0.35)));
                if !near_opening(q, reach, openings) {
                    kit.block(Surface::Wood, DVec3::new(reach, 0.09, 0.06), frame);
                }
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
        // The torn covering is its own colour, flush with it and on down into the
        // deck, and only below that dark, so a crack that has barely parted shows no
        // black seam across the roof: the held slabs settle apart by millimetres
        // before the roof gives.
        kit.quad(
            Surface::Roof,
            [
                at(a, COVER_TOP),
                at(b, COVER_TOP),
                at(b, -COVER_LIP),
                at(a, -COVER_LIP),
            ],
            facing,
        );
        kit.quad(
            Surface::Dark,
            [
                at(a, -COVER_LIP),
                at(b, -COVER_LIP),
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
            // Inside the deck, under the shingles, so the standing roof hides them
            // until its slabs part, deep enough that a sagging fold over a breach,
            // which bends the deck but not a straight splinter, still covers them.
            let tip = at(q + n * reach, -SPLINTER_SINK);
            let root = at(q, rng.range(-roof.thick * 0.8, -0.03));
            let droop = DVec3::Y * rng.range(-0.06, 0.0);
            // Mostly broken shingles; the odd split board under them.
            let coat = if rng.next() < 0.85 {
                Surface::Roof
            } else {
                Surface::Wood
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

/// How much a collapse piece's paint varies across it (darkest, range): patches
/// of dust, soot and damp over every finish, so a broken panel reads as a worn,
/// filthy board rather than a clean painted card.
const MOTTLE: (f32, f32) = (0.9, 0.12);
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
    // A corner's hash mixes one product per axis, so the eight corners share six.
    let axis = |at: f64, scale: i64| {
        [
            ((at + 0.0) as i64).wrapping_mul(scale),
            ((at + 1.0) as i64).wrapping_mul(scale),
        ]
    };
    let (hx, hy, hz) = (
        axis(cell.x, 73_856_093),
        axis(cell.y, 19_349_663),
        axis(cell.z, 83_492_791),
    );
    let corner = |dx: usize, dy: usize, dz: usize| {
        let mut h = hx[dx] ^ hy[dy] ^ hz[dz];
        h = (h ^ (h >> 13)).wrapping_mul(0x5bd1_e995);
        (h ^ (h >> 15)).rem_euclid(1000) as f64 / 999.0
    };
    let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let x00 = lerp(corner(0, 0, 0), corner(1, 0, 0), s.x);
    let x10 = lerp(corner(0, 1, 0), corner(1, 1, 0), s.x);
    let x01 = lerp(corner(0, 0, 1), corner(1, 0, 1), s.x);
    let x11 = lerp(corner(0, 1, 1), corner(1, 1, 1), s.x);
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
    shown: &[(C, Arc<Mesh>)],
    clip: &ShardClip,
) -> Vec<(C, Arc<Mesh>)> {
    meshes
        .iter()
        .zip(shown)
        .filter_map(|((coat, mesh), (_, shown))| {
            clip_settled(mesh, shown, clip).map(|mesh| (*coat, Arc::new(mesh)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::house_fracture::{Crack, clip_vertices};

    /// A wall-sized grid of `columns` × `rows` quads in the plane z = 0.
    fn grid(columns: u32, rows: u32) -> Mesh {
        let mut mesh = Mesh::default();
        let mut indices = Vec::new();
        for row in 0..=rows {
            for column in 0..=columns {
                let (s, t) = (column as f32 / columns as f32, row as f32 / rows as f32);
                mesh.positions.push([s * 4.0 - 2.0, t * 3.0, 0.0]);
                mesh.normals.push([0.0, 0.0, 1.0]);
                mesh.uvs.push([s, t]);
            }
        }
        for row in 0..rows {
            for column in 0..columns {
                let at = row * (columns + 1) + column;
                let above = at + columns + 1;
                indices.extend([at, at + 1, above + 1, at, above + 1, above]);
            }
        }
        mesh.indices = Some(indices);
        mesh
    }

    /// Every triangle clipped through every cell's sides, as shards were cut before
    /// triangles that cannot reach a cell were passed over.
    fn plain_clip(mesh: &Mesh, plane: &PartPlane, region: &ShardRegion) -> Mesh {
        let vertex = |i: u32| Vertex {
            p: widen(mesh.positions[i as usize]),
            n: widen(mesh.normals[i as usize]),
            shown: widen(mesh.positions[i as usize]),
            shown_n: widen(mesh.normals[i as usize]),
            uv: DVec2::new(
                f64::from(mesh.uvs[i as usize][0]),
                f64::from(mesh.uvs[i as usize][1]),
            ),
            color: DVec3::ZERO,
            q: DVec2::ZERO,
        };
        let project = |v: &Vertex| plane.project(v.p);
        let mix = |a: &Vertex, b: &Vertex, k: f64| mix(plane, a, b, k);
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
        for corners in mesh.indices.as_ref().unwrap().chunks(3) {
            let triangle = [0, 1, 2].map(|k| vertex(corners[k]));
            let points = triangle.map(|v| project(&v));
            if points.iter().all(|q| region.contains(*q)) && !region.straddles(&points) {
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
        out
    }

    #[test]
    fn shards_cut_only_the_triangles_they_reach_as_a_full_clip_would() {
        let mesh = grid(40, 30);
        let plane = PartPlane {
            origin: DVec3::ZERO,
            s: DVec3::X,
            t: DVec3::Y,
        };
        let mut wander = [0.21, -0.17, 0.09, -0.05].into_iter().cycle();
        let knots = [0.4, 0.9, 1.5, 2.2, 2.7];
        let cracks = [-0.7, 0.2, 1.1].map(|s| Crack::new(s, &knots, || wander.next().unwrap()));
        let regions = [
            ShardRegion::between(None, Some(&cracks[0]), None),
            ShardRegion::between(Some(&cracks[0]), Some(&cracks[1]), None),
            ShardRegion::between(Some(&cracks[1]), Some(&cracks[2]), None),
            ShardRegion::between(Some(&cracks[2]), None, None),
        ];
        let mut triangles = 0;
        for region in &regions {
            let cut = clip_settled(&mesh, &mesh, &ShardClip::new(&plane, region))
                .expect("every shard keeps some wall");
            let plain = plain_clip(&mesh, &plane, region);
            assert_eq!(cut.positions, plain.positions);
            assert_eq!(cut.normals, plain.normals);
            assert_eq!(cut.uvs, plain.uvs);
            triangles += cut.positions.len() / 3;
        }
        // Together the shards hold the whole wall, cut along the cracks.
        assert!(triangles > mesh.triangle_count());
    }
}
