//! Regions of a cottage part's plane that its model is cut to: each wall and roof
//! slope of the model is a plane part, and a structural part of the cottage is the
//! region of it its cells cover.
//!
//! A part's plane has coordinates `(s, t)`: along a wall and up it, or along the
//! ridge and down a slope from it. A region is a union of convex cells, each the
//! intersection of half-planes. Clipping a polygon to those cells keeps track of which
//! of its edges are broken edges, where the model shows torn timber.

use glam::{DVec2, DVec3};

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
    /// A boundary with a neighbouring cell that carries on, not a real edge.
    Band,
    /// Where a neighbour is gone: torn timber.
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

/// A region of a part's plane, as convex cells.
#[derive(Clone, Debug, PartialEq)]
pub struct ShardRegion {
    pub cells: Vec<Vec<Side>>,
}

impl ShardRegion {
    /// Whether a polygon inside the region still lies across one of its broken
    /// lines, so it must be clipped.
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

    /// `polygon` (convex, edges marked) clipped to each cell: the region's pieces of
    /// it. Most cells leave nothing of it, so the clipping runs in two reused buffers
    /// and only a piece that is kept gets its own.
    fn clip(&self, polygon: &[(DVec2, EdgeKind)]) -> Vec<Vec<(DVec2, EdgeKind)>> {
        let mut pieces = Vec::new();
        let mut piece = Vec::with_capacity(polygon.len() + 8);
        let mut clipped = Vec::with_capacity(polygon.len() + 8);
        'cells: for cell in &self.cells {
            piece.clear();
            piece.extend_from_slice(polygon);
            for side in cell {
                clip_marked_into(&piece, side, &mut clipped);
                std::mem::swap(&mut piece, &mut clipped);
                if piece.len() < 3 {
                    continue 'cells;
                }
            }
            pieces.push(piece.clone());
        }
        pieces
    }

    /// The broken edges of the region's part of `outline` (convex).
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

/// Sutherland-Hodgman against one side into `out` (replacing what it held), each
/// vertex carrying the kind of the edge that leaves it; an edge along the clip line
/// takes the side's kind.
fn clip_marked_into(polygon: &[(DVec2, EdgeKind)], side: &Side, out: &mut Vec<(DVec2, EdgeKind)>) {
    out.clear();
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
}

/// Clip a convex polygon of interpolated vertices to a half-plane into `out`,
/// replacing what it held: `project` gives each vertex's plane point and `mix` blends
/// two vertices.
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
    fn neighbouring_regions_share_their_broken_line_and_cover_the_part_once() {
        let (a, b) = (DVec2::new(0.1, 0.0), DVec2::new(0.5, 3.0));
        let side = |keep: DVec2| vec![Side::through(a, b, keep, EdgeKind::Broken)];
        let left = ShardRegion {
            cells: vec![side(DVec2::new(-2.0, 1.5))],
        };
        let right = ShardRegion {
            cells: vec![side(DVec2::new(2.0, 1.5))],
        };
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
        assert!((rise - 3.0).abs() < 1e-9, "the line spans the part");
    }

    #[test]
    fn a_floor_line_cuts_a_region_from_below() {
        let floor = Side::through(
            DVec2::new(-2.0, 1.0),
            DVec2::new(2.0, 1.4),
            DVec2::new(0.0, 3.0),
            EdgeKind::Broken,
        );
        let region = ShardRegion {
            cells: vec![vec![floor]],
        };
        assert!(!region.contains(DVec2::new(0.0, 0.5)));
        assert!(region.contains(DVec2::new(0.0, 2.0)));
        assert_eq!(region.broken_edges(&square()).len(), 1);
    }
}
