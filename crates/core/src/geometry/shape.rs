//! 2D outlines: ports of Three.js r185 `Path`, `Shape` and `CurvePath.getPoints` for
//! the straight segments the game's outlines are made of. Shape and extrude
//! generators sample outlines through `Path::points`.

use glam::DVec2;

/// One straight segment of a path (Three's `LineCurve`).
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub from: DVec2,
    pub to: DVec2,
}

impl Line {
    /// `Curve.getPoint(t)` for `t` in 0..=1.
    fn point(&self, t: f64) -> DVec2 {
        let Line { from, to } = *self;
        if t == 1.0 {
            to
        } else {
            DVec2::new((to.x - from.x) * t + from.x, (to.y - from.y) * t + from.y)
        }
    }
}

/// `THREE.Path`: a sequence of lines drawn from a current point. Builder methods
/// return `&mut Self` for chaining, like the JavaScript.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    pub curves: Vec<Line>,
    pub current_point: DVec2,
}

impl Path {
    pub fn new() -> Self {
        Self::default()
    }

    /// `new Path(points)` / `setFromPoints`: a polyline through the points.
    pub fn from_points(points: &[DVec2]) -> Self {
        let mut path = Self::new();
        if let Some((first, rest)) = points.split_first() {
            path.move_to(first.x, first.y);
            for point in rest {
                path.line_to(point.x, point.y);
            }
        }
        path
    }

    pub fn move_to(&mut self, x: f64, y: f64) -> &mut Self {
        self.current_point = DVec2::new(x, y);
        self
    }

    pub fn line_to(&mut self, x: f64, y: f64) -> &mut Self {
        let to = DVec2::new(x, y);
        self.curves.push(Line {
            from: self.current_point,
            to,
        });
        self.current_point = to;
        self
    }

    /// `closePath`: a line back to the start when the path does not end there.
    pub fn close_path(&mut self) -> &mut Self {
        if let (Some(first), Some(last)) = (self.curves.first(), self.curves.last()) {
            let (start, end) = (first.point(0.0), last.point(1.0));
            if start != end {
                self.curves.push(Line {
                    from: end,
                    to: start,
                });
            }
        }
        self
    }

    /// `CurvePath.getPoints`: every line contributes its ends; consecutive
    /// duplicates are dropped.
    pub fn points(&self) -> Vec<DVec2> {
        let mut points: Vec<DVec2> = Vec::new();
        for line in &self.curves {
            for point in [line.point(0.0), line.point(1.0)] {
                if points.last() != Some(&point) {
                    points.push(point);
                }
            }
        }
        points
    }
}

/// `THREE.Shape`: an outline path with optional hole paths.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shape {
    pub outline: Path,
    pub holes: Vec<Path>,
}

impl Shape {
    pub fn new(outline: Path) -> Self {
        Self {
            outline,
            holes: Vec::new(),
        }
    }

    /// `new Shape(points)`.
    pub fn from_points(points: &[DVec2]) -> Self {
        Self::new(Path::from_points(points))
    }

    /// `Shape.extractPoints`: the sampled outline and holes.
    pub fn extract_points(&self) -> (Vec<DVec2>, Vec<Vec<DVec2>>) {
        (
            self.outline.points(),
            self.holes.iter().map(Path::points).collect(),
        )
    }
}
