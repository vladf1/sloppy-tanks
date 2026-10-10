//! Port of Three.js r185 `CatmullRomCurve3` with the `Curve` sampling methods the
//! game used (`getPoint`, `getPoints`, `getTangent`).

use glam::DVec3;

use super::math::normalize;

/// `THREE.CatmullRomCurve3(points)` with Three's defaults: open and centripetal
/// (knot spacing by the fourth root of the squared distance).
#[derive(Clone, Debug, PartialEq)]
pub struct CatmullRomCurve3 {
    pub points: Vec<DVec3>,
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
    pub fn new(points: Vec<DVec3>) -> Self {
        Self { points }
    }

    /// `getPoint(t)` for `t` in 0..=1.
    pub fn point(&self, t: f64) -> DVec3 {
        let points = &self.points;
        let l = points.len();
        let p = (l as f64 - 1.0) * t;
        let mut int_point = p.floor();
        let mut weight = p - int_point;
        if weight == 0.0 && int_point == (l - 1) as f64 {
            int_point = (l - 2) as f64;
            weight = 1.0;
        }
        let int_point = int_point as usize;
        let p0 = if int_point > 0 {
            points[int_point - 1]
        } else {
            (points[0] - points[1]) + points[0]
        };
        let p1 = points[int_point];
        let p2 = points[int_point + 1];
        let p3 = if int_point + 2 < l {
            points[int_point + 2]
        } else {
            (points[l - 1] - points[l - 2]) + points[l - 1]
        };
        let mut dt0 = p0.distance_squared(p1).powf(0.25);
        let mut dt1 = p1.distance_squared(p2).powf(0.25);
        let mut dt2 = p2.distance_squared(p3).powf(0.25);
        if dt1 < 1e-4 {
            dt1 = 1.0;
        }
        if dt0 < 1e-4 {
            dt0 = dt1;
        }
        if dt2 < 1e-4 {
            dt2 = dt1;
        }
        let axes = [0, 1, 2].map(|axis| {
            CubicPoly::nonuniform([p0[axis], p1[axis], p2[axis], p3[axis]], dt0, dt1, dt2)
        });
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
}
