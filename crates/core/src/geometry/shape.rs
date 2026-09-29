//! 2D outlines: ports of Three.js r185 `Path`, `Shape`, `CurvePath.getPoints` and the
//! 2D curves a path is built from (line, quadratic and cubic Bézier, ellipse/arc).
//! Shape and extrude generators sample outlines through `Path::points`.

use glam::DVec2;

/// One segment of a path.
#[derive(Clone, Debug, PartialEq)]
pub enum Curve2 {
    Line {
        from: DVec2,
        to: DVec2,
    },
    QuadraticBezier {
        start: DVec2,
        control: DVec2,
        end: DVec2,
    },
    CubicBezier {
        start: DVec2,
        control1: DVec2,
        control2: DVec2,
        end: DVec2,
    },
    /// `EllipseCurve`: angles in radians, counter-clockwise unless `clockwise`.
    Ellipse {
        center: DVec2,
        x_radius: f64,
        y_radius: f64,
        start_angle: f64,
        end_angle: f64,
        clockwise: bool,
        rotation: f64,
    },
}

fn quadratic_bezier(t: f64, p0: f64, p1: f64, p2: f64) -> f64 {
    let k = 1.0 - t;
    k * k * p0 + 2.0 * (1.0 - t) * t * p1 + t * t * p2
}

fn cubic_bezier(t: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    let k = 1.0 - t;
    k * k * k * p0 + 3.0 * k * k * t * p1 + 3.0 * (1.0 - t) * t * t * p2 + t * t * t * p3
}

impl Curve2 {
    /// `Curve.getPoint(t)` for `t` in 0..=1.
    pub fn point(&self, t: f64) -> DVec2 {
        match *self {
            Curve2::Line { from, to } => {
                if t == 1.0 {
                    to
                } else {
                    DVec2::new((to.x - from.x) * t + from.x, (to.y - from.y) * t + from.y)
                }
            }
            Curve2::QuadraticBezier {
                start,
                control,
                end,
            } => DVec2::new(
                quadratic_bezier(t, start.x, control.x, end.x),
                quadratic_bezier(t, start.y, control.y, end.y),
            ),
            Curve2::CubicBezier {
                start,
                control1,
                control2,
                end,
            } => DVec2::new(
                cubic_bezier(t, start.x, control1.x, control2.x, end.x),
                cubic_bezier(t, start.y, control1.y, control2.y, end.y),
            ),
            Curve2::Ellipse {
                center,
                x_radius,
                y_radius,
                start_angle,
                end_angle,
                clockwise,
                rotation,
            } => ellipse_point(
                t,
                center,
                [x_radius, y_radius],
                [start_angle, end_angle],
                clockwise,
                rotation,
            ),
        }
    }

    /// `Curve.getPoints(divisions)`: `divisions + 1` evenly parameterised samples.
    pub fn points(&self, divisions: u32) -> Vec<DVec2> {
        (0..=divisions)
            .map(|d| self.point(f64::from(d) / f64::from(divisions)))
            .collect()
    }
}

fn ellipse_point(
    t: f64,
    center: DVec2,
    [x_radius, y_radius]: [f64; 2],
    [start_angle, end_angle]: [f64; 2],
    clockwise: bool,
    rotation: f64,
) -> DVec2 {
    let two_pi = std::f64::consts::PI * 2.0;
    let mut delta_angle = end_angle - start_angle;
    let same_points = delta_angle.abs() < f64::EPSILON;
    while delta_angle < 0.0 {
        delta_angle += two_pi;
    }
    while delta_angle > two_pi {
        delta_angle -= two_pi;
    }
    if delta_angle < f64::EPSILON {
        delta_angle = if same_points { 0.0 } else { two_pi };
    }
    if clockwise && !same_points {
        delta_angle = if delta_angle == two_pi {
            -two_pi
        } else {
            delta_angle - two_pi
        };
    }
    let angle = start_angle + t * delta_angle;
    let mut x = center.x + x_radius * angle.cos();
    let mut y = center.y + y_radius * angle.sin();
    if rotation != 0.0 {
        let (cos, sin) = (rotation.cos(), rotation.sin());
        let (tx, ty) = (x - center.x, y - center.y);
        x = tx * cos - ty * sin + center.x;
        y = tx * sin + ty * cos + center.y;
    }
    DVec2::new(x, y)
}

/// `THREE.Path`: a sequence of curves drawn from a current point. Builder methods
/// return `&mut Self` for chaining, like the JavaScript.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    pub curves: Vec<Curve2>,
    pub current_point: DVec2,
    pub auto_close: bool,
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
        self.curves.push(Curve2::Line {
            from: self.current_point,
            to,
        });
        self.current_point = to;
        self
    }

    pub fn quadratic_curve_to(&mut self, cpx: f64, cpy: f64, x: f64, y: f64) -> &mut Self {
        let end = DVec2::new(x, y);
        self.curves.push(Curve2::QuadraticBezier {
            start: self.current_point,
            control: DVec2::new(cpx, cpy),
            end,
        });
        self.current_point = end;
        self
    }

    pub fn bezier_curve_to(&mut self, cp1: DVec2, cp2: DVec2, x: f64, y: f64) -> &mut Self {
        let end = DVec2::new(x, y);
        self.curves.push(Curve2::CubicBezier {
            start: self.current_point,
            control1: cp1,
            control2: cp2,
            end,
        });
        self.current_point = end;
        self
    }

    /// `arc`: like `absarc` with the center relative to the current point.
    pub fn arc(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
        start_angle: f64,
        end_angle: f64,
        clockwise: bool,
    ) -> &mut Self {
        let origin = self.current_point;
        self.absarc(
            x + origin.x,
            y + origin.y,
            radius,
            start_angle,
            end_angle,
            clockwise,
        )
    }

    pub fn absarc(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
        start_angle: f64,
        end_angle: f64,
        clockwise: bool,
    ) -> &mut Self {
        self.absellipse(
            DVec2::new(x, y),
            [radius, radius],
            [start_angle, end_angle],
            clockwise,
            0.0,
        )
    }

    /// `absellipse`: when the path already has curves and the ellipse does not start
    /// at the current point, a connecting line is added first.
    pub fn absellipse(
        &mut self,
        center: DVec2,
        [x_radius, y_radius]: [f64; 2],
        [start_angle, end_angle]: [f64; 2],
        clockwise: bool,
        rotation: f64,
    ) -> &mut Self {
        let curve = Curve2::Ellipse {
            center,
            x_radius,
            y_radius,
            start_angle,
            end_angle,
            clockwise,
            rotation,
        };
        if !self.curves.is_empty() {
            let first = curve.point(0.0);
            if first != self.current_point {
                self.line_to(first.x, first.y);
            }
        }
        self.current_point = curve.point(1.0);
        self.curves.push(curve);
        self
    }

    /// `closePath`: a line back to the start when the path does not end there.
    pub fn close_path(&mut self) -> &mut Self {
        if let (Some(first), Some(last)) = (self.curves.first(), self.curves.last()) {
            let (start, end) = (first.point(0.0), last.point(1.0));
            if start != end {
                self.curves.push(Curve2::Line {
                    from: end,
                    to: start,
                });
            }
        }
        self
    }

    /// `CurvePath.getPoints(divisions)`: lines contribute their ends, ellipses
    /// `2 * divisions` segments and Béziers `divisions`; consecutive duplicates
    /// are dropped.
    pub fn points(&self, divisions: u32) -> Vec<DVec2> {
        let mut points: Vec<DVec2> = Vec::new();
        for curve in &self.curves {
            let resolution = match curve {
                Curve2::Ellipse { .. } => divisions * 2,
                Curve2::Line { .. } => 1,
                _ => divisions,
            };
            for point in curve.points(resolution) {
                if points.last() == Some(&point) {
                    continue;
                }
                points.push(point);
            }
        }
        if self.auto_close && points.len() > 1 && points.last() != points.first() {
            points.push(points[0]);
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

    /// `Shape.extractPoints(divisions)`: the sampled outline and holes.
    pub fn extract_points(&self, divisions: u32) -> (Vec<DVec2>, Vec<Vec<DVec2>>) {
        (
            self.outline.points(divisions),
            self.holes
                .iter()
                .map(|hole| hole.points(divisions))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_samples_twice_the_divisions() {
        let mut path = Path::new();
        path.move_to(-1.0, -1.0).line_to(1.0, -1.0).absarc(
            1.0,
            0.0,
            1.0,
            -std::f64::consts::FRAC_PI_2,
            std::f64::consts::FRAC_PI_2,
            false,
        );
        let points = path.points(6);
        // Two line ends, then 13 arc samples whose first repeats the line end.
        assert_eq!(points.len(), 14);
        assert!((points[13] - DVec2::new(1.0, 1.0)).length() < 1e-12);
    }
}
