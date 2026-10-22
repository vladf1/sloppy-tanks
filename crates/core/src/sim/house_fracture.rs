//! The cracks a cottage is pre-fractured along, shared by its collapse colliders and
//! the shard models drawn on them: each wall and roof slope is a plane part cut by a
//! few jagged cracks into shards, so a fallen wall comes down as irregular panels
//! and a roof as broken slabs rather than as whole sheets.
//!
//! A part's plane has coordinates `(s, t)`: along a wall and up it, or along the
//! ridge and down a slope from it. A crack runs across the part as `s` piecewise
//! linear in `t` through knots every crack of the part shares, so the region
//! between two neighbouring cracks is a strip of convex cells, one per band
//! between knots. Clipping a polygon to those cells keeps track of which of its
//! edges are broken (crack or floor) edges, where the model shows torn timber.

use glam::{DVec2, DVec3};

/// Far enough past any part that a crack's end bands cover it.
const FAR: f64 = 50.0;
/// Points closer than this to a clip line count as on it.
const EPSILON: f64 = 1e-9;

/// A part's plane: `s` and `t` measured from `origin` along two build-frame axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartPlane {
    pub origin: DVec3,
    pub s: DVec3,
    pub t: DVec3,
}

impl PartPlane {
    pub fn project(&self, p: DVec3) -> DVec2 {
        let d = p - self.origin;
        DVec2::new(d.dot(self.s), d.dot(self.t))
    }
}

/// What an edge of a clipped polygon lies on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeKind {
    /// The part's own outline.
    Outline,
    /// A band boundary inside a shard, not a real edge.
    Band,
    /// A crack or a broken floor line: torn timber.
    Broken,
}

/// The half-plane `normal · q >= offset`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Side {
    pub normal: DVec2,
    pub offset: f64,
    pub kind: EdgeKind,
}

impl Side {
    /// The side of the line through `a` and `b` that holds `keep`.
    pub fn through(a: DVec2, b: DVec2, keep: DVec2, kind: EdgeKind) -> Self {
        let d = b - a;
        let mut normal = DVec2::new(-d.y, d.x).normalize_or_zero();
        if normal.dot(keep - a) < 0.0 {
            normal = -normal;
        }
        Self {
            normal,
            offset: normal.dot(a),
            kind,
        }
    }

    pub fn distance(&self, q: DVec2) -> f64 {
        self.normal.dot(q) - self.offset
    }
}

/// A crack across a part: `s` through `points` (`(s, t)`, `t` increasing), held
/// straight past the first and last.
#[derive(Clone, Debug, PartialEq)]
pub struct Crack {
    pub points: Vec<DVec2>,
}

impl Crack {
    /// A crack at `s` wandering by up to `wander` at each interior knot `t`.
    pub fn new(s: f64, knots: &[f64], mut wander: impl FnMut() -> f64) -> Self {
        let inner: Vec<DVec2> = knots.iter().map(|&t| DVec2::new(s + wander(), t)).collect();
        let mut points = Vec::with_capacity(inner.len() + 2);
        points.push(DVec2::new(inner[0].x, -FAR));
        points.extend(&inner);
        points.push(DVec2::new(inner[inner.len() - 1].x, FAR));
        Self { points }
    }

    /// Where the crack crosses height `t`.
    pub fn at(&self, t: f64) -> f64 {
        for pair in self.points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if t <= b.y {
                let k = ((t - a.y) / (b.y - a.y)).clamp(0.0, 1.0);
                return a.x + (b.x - a.x) * k;
            }
        }
        self.points[self.points.len() - 1].x
    }
}

/// One shard's region: between its cracks (none at a part's ends) and above an
/// optional floor line, as convex cells.
#[derive(Clone, Debug, PartialEq)]
pub struct ShardRegion {
    pub cells: Vec<Vec<Side>>,
}

impl ShardRegion {
    /// The strip between `left` and `right` (each with the same knots), above `floor`.
    pub fn between(left: Option<&Crack>, right: Option<&Crack>, floor: Option<Side>) -> Self {
        let knots: Vec<f64> = left.or(right).map_or_else(
            || vec![-FAR, FAR],
            |c| c.points.iter().map(|p| p.y).collect(),
        );
        let mut cells = Vec::new();
        for band in 0..knots.len() - 1 {
            let (t0, t1) = (knots[band], knots[band + 1]);
            let mid = (t0 + t1) / 2.0;
            let inside_s = match (left, right) {
                (Some(l), Some(r)) => (l.at(mid) + r.at(mid)) / 2.0,
                (Some(l), None) => l.at(mid) + 1.0,
                (None, Some(r)) => r.at(mid) - 1.0,
                (None, None) => 0.0,
            };
            let keep = DVec2::new(inside_s, mid);
            let mut sides = vec![
                Side::through(
                    DVec2::new(0.0, t0),
                    DVec2::new(1.0, t0),
                    keep,
                    EdgeKind::Band,
                ),
                Side::through(
                    DVec2::new(0.0, t1),
                    DVec2::new(1.0, t1),
                    keep,
                    EdgeKind::Band,
                ),
            ];
            for crack in [left, right].into_iter().flatten() {
                sides.push(Side::through(
                    crack.points[band],
                    crack.points[band + 1],
                    keep,
                    EdgeKind::Broken,
                ));
            }
            sides.extend(floor);
            cells.push(sides);
        }
        Self { cells }
    }

    /// The region cut down to `other` (a union of convex cells too): every pair of
    /// cells, one from each, as one cell bounded by the sides of both.
    pub fn intersect(&self, other: &[Vec<Side>]) -> Self {
        let cells = self
            .cells
            .iter()
            .flat_map(|a| {
                other.iter().map(move |b| {
                    let mut cell = a.clone();
                    cell.extend(b.iter().copied());
                    cell
                })
            })
            .collect();
        Self { cells }
    }

    /// Whether nothing cuts the part: the shard is all of it.
    pub fn is_whole(&self) -> bool {
        self.cells
            .iter()
            .flatten()
            .all(|side| side.kind != EdgeKind::Broken)
    }

    /// Whether a polygon inside the region still lies across one of its broken
    /// lines (where a crack's jag pokes into it), so it must be clipped.
    pub fn straddles(&self, points: &[DVec2]) -> bool {
        self.cells.iter().any(|cell| {
            let in_band = cell
                .iter()
                .filter(|side| side.kind == EdgeKind::Band)
                .all(|side| points.iter().any(|q| side.distance(*q) > 0.0));
            in_band
                && cell
                    .iter()
                    .filter(|side| side.kind == EdgeKind::Broken)
                    .any(|side| {
                        let (low, high) = points.iter().fold(
                            (f64::INFINITY, f64::NEG_INFINITY),
                            |(low, high), q| {
                                let d = side.distance(*q);
                                (low.min(d), high.max(d))
                            },
                        );
                        low < -1e-7 && high > 1e-7
                    })
        })
    }

    pub fn contains(&self, q: DVec2) -> bool {
        self.cells
            .iter()
            .any(|cell| cell.iter().all(|side| side.distance(q) >= -1e-7))
    }

    /// `polygon` (convex, edges marked) clipped to each cell: the shard's pieces of it.
    pub fn clip(&self, polygon: &[(DVec2, EdgeKind)]) -> Vec<Vec<(DVec2, EdgeKind)>> {
        self.cells
            .iter()
            .filter_map(|cell| {
                let mut piece = polygon.to_vec();
                for side in cell {
                    piece = clip_marked(&piece, side);
                    if piece.len() < 3 {
                        return None;
                    }
                }
                Some(piece)
            })
            .collect()
    }

    /// Every point of `outline` (convex) the shard keeps, cell by cell.
    pub fn outline_points(&self, outline: &[DVec2]) -> Vec<DVec2> {
        self.clip(&marked(outline))
            .into_iter()
            .flatten()
            .map(|(p, _)| p)
            .collect()
    }

    /// The broken edges of the shard's part of `outline` (convex): crack and floor
    /// lines.
    pub fn broken_edges(&self, outline: &[DVec2]) -> Vec<(DVec2, DVec2)> {
        let mut edges = Vec::new();
        for piece in self.clip(&marked(outline)) {
            for i in 0..piece.len() {
                let (a, kind) = piece[i];
                let b = piece[(i + 1) % piece.len()].0;
                if kind == EdgeKind::Broken && (b - a).length_squared() > 1e-8 {
                    edges.push((a, b));
                }
            }
        }
        edges
    }
}

fn marked(outline: &[DVec2]) -> Vec<(DVec2, EdgeKind)> {
    outline.iter().map(|&p| (p, EdgeKind::Outline)).collect()
}

/// Sutherland-Hodgman against one side, each vertex carrying the kind of the edge
/// that leaves it; an edge along the clip line takes the side's kind.
fn clip_marked(polygon: &[(DVec2, EdgeKind)], side: &Side) -> Vec<(DVec2, EdgeKind)> {
    let mut out = Vec::with_capacity(polygon.len() + 2);
    for i in 0..polygon.len() {
        let (a, kind) = polygon[i];
        let b = polygon[(i + 1) % polygon.len()].0;
        let (da, db) = (side.distance(a), side.distance(b));
        let a_in = da >= -EPSILON;
        let b_in = db >= -EPSILON;
        let cross = || a + (b - a) * (da / (da - db));
        match (a_in, b_in) {
            (true, true) => out.push((a, kind)),
            (true, false) => {
                out.push((a, kind));
                out.push((cross(), side.kind));
            }
            (false, true) => out.push((cross(), kind)),
            (false, false) => {}
        }
    }
    out
}

/// Clip a convex polygon of interpolated vertices to a half-plane: `project`
/// gives each vertex's plane point and `mix` blends two vertices.
pub fn clip_vertices<V: Copy>(
    polygon: &[V],
    side: &Side,
    project: impl Fn(&V) -> DVec2,
    mix: impl Fn(&V, &V, f64) -> V,
) -> Vec<V> {
    let mut out = Vec::with_capacity(polygon.len() + 2);
    clip_vertices_into(polygon, side, project, mix, &mut out);
    out
}

/// [`clip_vertices`] into `out`, replacing what it held, for loops that clip many
/// polygons without allocating each one.
pub fn clip_vertices_into<V: Copy>(
    polygon: &[V],
    side: &Side,
    project: impl Fn(&V) -> DVec2,
    mix: impl Fn(&V, &V, f64) -> V,
    out: &mut Vec<V>,
) {
    out.clear();
    for i in 0..polygon.len() {
        let (a, b) = (&polygon[i], &polygon[(i + 1) % polygon.len()]);
        let (da, db) = (side.distance(project(a)), side.distance(project(b)));
        let a_in = da >= -EPSILON;
        let b_in = db >= -EPSILON;
        if a_in {
            out.push(*a);
        }
        if a_in != b_in {
            out.push(mix(a, b, da / (da - db)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<DVec2> {
        vec![
            DVec2::new(-2.0, 0.0),
            DVec2::new(2.0, 0.0),
            DVec2::new(2.0, 3.0),
            DVec2::new(-2.0, 3.0),
        ]
    }

    #[test]
    fn neighbouring_shards_share_their_crack_and_cover_the_part_once() {
        let mut wander = [0.2, -0.15, 0.1].into_iter().cycle();
        let crack = Crack::new(0.3, &[0.8, 1.6, 2.4], || wander.next().unwrap());
        let left = ShardRegion::between(None, Some(&crack), None);
        let right = ShardRegion::between(Some(&crack), None, None);
        for (s, t) in [(-1.9, 0.1), (1.9, 2.9), (0.0, 1.0), (0.6, 2.0)] {
            let q = DVec2::new(s, t);
            assert_ne!(left.contains(q), right.contains(q), "{q}");
        }
        let area = |points: &[(DVec2, EdgeKind)]| {
            let mut sum = 0.0;
            for i in 0..points.len() {
                let (a, b) = (points[i].0, points[(i + 1) % points.len()].0);
                sum += a.perp_dot(b);
            }
            sum.abs() / 2.0
        };
        let total: f64 = [&left, &right]
            .iter()
            .flat_map(|r| r.clip(&marked(&square())))
            .map(|piece| area(&piece))
            .sum();
        assert!((total - 12.0).abs() < 1e-9, "{total}");
        let rise: f64 = left
            .broken_edges(&square())
            .iter()
            .map(|(a, b)| (b.y - a.y).abs())
            .sum();
        assert!((rise - 3.0).abs() < 1e-9, "the crack spans the part");
    }

    #[test]
    fn a_floor_line_cuts_a_shard_from_below() {
        let floor = Side::through(
            DVec2::new(-2.0, 1.0),
            DVec2::new(2.0, 1.4),
            DVec2::new(0.0, 3.0),
            EdgeKind::Broken,
        );
        let region = ShardRegion::between(None, None, Some(floor));
        assert!(!region.contains(DVec2::new(0.0, 0.5)));
        assert!(region.contains(DVec2::new(0.0, 2.0)));
        assert_eq!(region.broken_edges(&square()).len(), 1);
    }
}
