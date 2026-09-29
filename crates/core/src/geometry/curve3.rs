//! Port of Three.js r185 `CatmullRomCurve3` with the `Curve` sampling methods the
//! game used (`getPoint`, `getPoints`, `getTangent`) and the arc-length ones.

use glam::DVec3;

use super::math::normalize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CatmullRomKind {
    /// Three's default: knot spacing by the fourth root of the squared distance.
    #[default]
    Centripetal,
    Chordal,
    /// Uniform Catmull-Rom with the curve's `tension`.
    Uniform,
}

/// `THREE.CatmullRomCurve3(points, closed, curveType, tension)`.
#[derive(Clone, Debug, PartialEq)]
pub struct CatmullRomCurve3 {
    pub points: Vec<DVec3>,
    pub closed: bool,
    pub kind: CatmullRomKind,
    pub tension: f64,
}

/// `CubicPoly`: coefficients of one axis of a spline segment.
struct CubicPoly([f64; 4]);

impl CubicPoly {
    fn new(x0: f64, x1: f64, t0: f64, t1: f64) -> Self {
        Self([
            x0,
            t0,
            -3.0 * x0 + 3.0 * x1 - 2.0 * t0 - t1,
            2.0 * x0 - 2.0 * x1 + t0 + t1,
        ])
    }

    fn uniform(x0: f64, x1: f64, x2: f64, x3: f64, tension: f64) -> Self {
        Self::new(x1, x2, tension * (x2 - x0), tension * (x3 - x1))
    }

    fn nonuniform(x: [f64; 4], dt0: f64, dt1: f64, dt2: f64) -> Self {
        let [x0, x1, x2, x3] = x;
        let mut t1 = (x1 - x0) / dt0 - (x2 - x0) / (dt0 + dt1) + (x2 - x1) / dt1;
        let mut t2 = (x2 - x1) / dt1 - (x3 - x1) / (dt1 + dt2) + (x3 - x2) / dt2;
        t1 *= dt1;
        t2 *= dt1;
        Self::new(x1, x2, t1, t2)
    }

    fn calc(&self, t: f64) -> f64 {
        let [c0, c1, c2, c3] = self.0;
        let t2 = t * t;
        let t3 = t2 * t;
        c0 + c1 * t + c2 * t2 + c3 * t3
    }
}

impl CatmullRomCurve3 {
    /// A centripetal, open curve (Three's defaults).
    pub fn new(points: Vec<DVec3>) -> Self {
        Self {
            points,
            closed: false,
            kind: CatmullRomKind::Centripetal,
            tension: 0.5,
        }
    }

    /// `getPoint(t)` for `t` in 0..=1.
    pub fn point(&self, t: f64) -> DVec3 {
        let points = &self.points;
        let l = points.len();
        let p = (l as f64 - if self.closed { 0.0 } else { 1.0 }) * t;
        let mut int_point = p.floor();
        let mut weight = p - int_point;
        if self.closed {
            if int_point <= 0.0 {
                int_point += ((int_point.abs() / l as f64).floor() + 1.0) * l as f64;
            }
        } else if weight == 0.0 && int_point == (l - 1) as f64 {
            int_point = (l - 2) as f64;
            weight = 1.0;
        }
        let int_point = int_point as i64;
        let at = |i: i64| points[i.rem_euclid(l as i64) as usize];
        let p0 = if self.closed || int_point > 0 {
            at(int_point - 1)
        } else {
            (points[0] - points[1]) + points[0]
        };
        let p1 = at(int_point);
        let p2 = at(int_point + 1);
        let p3 = if self.closed || int_point + 2 < l as i64 {
            at(int_point + 2)
        } else {
            (points[l - 1] - points[l - 2]) + points[l - 1]
        };
        let axes: [CubicPoly; 3] = match self.kind {
            CatmullRomKind::Centripetal | CatmullRomKind::Chordal => {
                let pow = if self.kind == CatmullRomKind::Chordal {
                    0.5
                } else {
                    0.25
                };
                let mut dt0 = p0.distance_squared(p1).powf(pow);
                let mut dt1 = p1.distance_squared(p2).powf(pow);
                let mut dt2 = p2.distance_squared(p3).powf(pow);
                if dt1 < 1e-4 {
                    dt1 = 1.0;
                }
                if dt0 < 1e-4 {
                    dt0 = dt1;
                }
                if dt2 < 1e-4 {
                    dt2 = dt1;
                }
                [0, 1, 2].map(|axis| {
                    CubicPoly::nonuniform([p0[axis], p1[axis], p2[axis], p3[axis]], dt0, dt1, dt2)
                })
            }
            CatmullRomKind::Uniform => [0, 1, 2].map(|axis| {
                CubicPoly::uniform(p0[axis], p1[axis], p2[axis], p3[axis], self.tension)
            }),
        };
        DVec3::new(
            axes[0].calc(weight),
            axes[1].calc(weight),
            axes[2].calc(weight),
        )
    }

    /// `getPoints(divisions)`: `divisions + 1` evenly parameterised samples.
    pub fn points(&self, divisions: u32) -> Vec<DVec3> {
        (0..=divisions)
            .map(|d| self.point(f64::from(d) / f64::from(divisions)))
            .collect()
    }

    /// `getTangent(t)`: normalised finite difference over ±0.0001, clamped to 0..=1.
    pub fn tangent(&self, t: f64) -> DVec3 {
        let delta = 0.0001;
        let t1 = (t - delta).max(0.0);
        let t2 = (t + delta).min(1.0);
        normalize(self.point(t2) - self.point(t1))
    }

    /// `getLengths(divisions)`: cumulative chord lengths of `divisions` samples
    /// (Three's default is 200).
    pub fn lengths(&self, divisions: u32) -> Vec<f64> {
        let mut lengths = Vec::with_capacity(divisions as usize + 1);
        let mut last = self.point(0.0);
        let mut sum = 0.0;
        lengths.push(0.0);
        for p in 1..=divisions {
            let current = self.point(f64::from(p) / f64::from(divisions));
            sum += current.distance(last);
            lengths.push(sum);
            last = current;
        }
        lengths
    }

    /// `getLength()` with the default 200 arc-length divisions.
    pub fn length(&self) -> f64 {
        *self.lengths(ARC_LENGTH_DIVISIONS).last().unwrap_or(&0.0)
    }

    /// `getUtoTmapping(u)`: the parameter at arc-length fraction `u`.
    pub fn u_to_t(&self, u: f64) -> f64 {
        let arc_lengths = self.lengths(ARC_LENGTH_DIVISIONS);
        let il = arc_lengths.len();
        let target = u * arc_lengths[il - 1];
        let (mut low, mut high) = (0i64, il as i64 - 1);
        while low <= high {
            let i = low + (high - low) / 2;
            let comparison = arc_lengths[i as usize] - target;
            if comparison < 0.0 {
                low = i + 1;
            } else if comparison > 0.0 {
                high = i - 1;
            } else {
                high = i;
                break;
            }
        }
        let i = high.max(0) as usize;
        if arc_lengths[i] == target {
            return i as f64 / (il - 1) as f64;
        }
        let before = arc_lengths[i];
        let after = arc_lengths[i + 1];
        let fraction = (target - before) / (after - before);
        (i as f64 + fraction) / (il - 1) as f64
    }

    /// `getPointAt(u)`: the point at arc-length fraction `u`.
    pub fn point_at(&self, u: f64) -> DVec3 {
        self.point(self.u_to_t(u))
    }
}

/// `Curve.arcLengthDivisions` default.
const ARC_LENGTH_DIVISIONS: u32 = 200;
