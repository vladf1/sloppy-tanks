//! Seeded randomness and small planar helpers, with the JavaScript number semantics that the
//! seeded match contract depends on (the previous engine ran on JS doubles).

use serde::{Deserialize, Serialize};

/// A point on the playable X/Z plane, in metres.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f64,
    pub z: f64,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, z: 0.0 };

    pub const fn new(x: f64, z: f64) -> Self {
        Self { x, z }
    }
}

/// A world position or vector; Y is up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Point3 {
    pub const ZERO: Point3 = Point3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub const fn planar(self) -> Vec2 {
        Vec2 {
            x: self.x,
            z: self.z,
        }
    }

    /// Component-wise interpolation toward `other`.
    pub fn lerp(self, other: Point3, t: f64) -> Point3 {
        Point3::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
            self.z + (other.z - self.z) * t,
        )
    }
}

impl From<Point3> for Vec2 {
    fn from(point: Point3) -> Self {
        point.planar()
    }
}

/// A rotation quaternion as the physics engine reports it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Quat4 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

impl Default for Quat4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Quat4 {
    pub const IDENTITY: Quat4 = Quat4 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };

    /// A rotation about the vertical axis, as hulls and live tanks use.
    pub fn yaw(angle: f64) -> Self {
        Self {
            x: 0.0,
            y: (angle / 2.0).sin(),
            z: 0.0,
            w: (angle / 2.0).cos(),
        }
    }

    /// Euler angles applied in XYZ order (Three.js `Quaternion.setFromEuler` default).
    pub fn from_euler_xyz(x: f64, y: f64, z: f64) -> Self {
        let (c1, c2, c3) = ((x / 2.0).cos(), (y / 2.0).cos(), (z / 2.0).cos());
        let (s1, s2, s3) = ((x / 2.0).sin(), (y / 2.0).sin(), (z / 2.0).sin());
        Self {
            x: s1 * c2 * c3 + c1 * s2 * s3,
            y: c1 * s2 * c3 - s1 * c2 * s3,
            z: c1 * c2 * s3 + s1 * s2 * c3,
            w: c1 * c2 * c3 - s1 * s2 * s3,
        }
    }

    /// `self * other` (apply `other` first), as Three.js `Quaternion.multiply`.
    pub fn multiply(self, other: Quat4) -> Self {
        let (ax, ay, az, aw) = (self.x, self.y, self.z, self.w);
        let (bx, by, bz, bw) = (other.x, other.y, other.z, other.w);
        Self {
            x: ax * bw + aw * bx + ay * bz - az * by,
            y: ay * bw + aw * by + az * bx - ax * bz,
            z: az * bw + aw * bz + ax * by - ay * bx,
            w: aw * bw - ax * bx - ay * by - az * bz,
        }
    }

    /// Rotate a vector, as Three.js `Vector3.applyQuaternion`.
    pub fn rotate(self, v: Point3) -> Point3 {
        let tx = 2.0 * (self.y * v.z - self.z * v.y);
        let ty = 2.0 * (self.z * v.x - self.x * v.z);
        let tz = 2.0 * (self.x * v.y - self.y * v.x);
        Point3 {
            x: v.x + self.w * tx + self.y * tz - self.z * ty,
            y: v.y + self.w * ty + self.z * tx - self.x * tz,
            z: v.z + self.w * tz + self.x * ty - self.y * tx,
        }
    }
}

/// Mulberry32: keep these bit operations and draw order stable for seeded matches.
///
/// `state` is a double like the JS original: it grows by 0x6d2b79f5 on every draw without
/// wrapping, and the 32-bit conversions apply only inside the bit operations. Past 2^53 the
/// addition rounds exactly as JS does, so long-lived streams keep matching.
#[derive(Clone, Debug, PartialEq)]
pub struct Random {
    pub state: f64,
}

const MULBERRY_INCREMENT: f64 = 1_831_565_813.0; // 0x6d2b79f5

impl Random {
    pub const fn new(state: f64) -> Self {
        Self { state }
    }

    /// The next draw in [0, 1).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> f64 {
        self.state += MULBERRY_INCREMENT;
        let state = self.state;
        // t = Math.imul(t ^ (t >>> 15), t | 1)
        let t =
            (to_int32(state) ^ (to_uint32(state) >> 15) as i32).wrapping_mul(to_int32(state) | 1);
        // t ^= t + Math.imul(t ^ (t >>> 7), t | 61): the sum is an exact double.
        let mixed = (t ^ ((t as u32) >> 7) as i32).wrapping_mul(t | 61);
        let t = t ^ to_int32(t as f64 + mixed as f64);
        // ((t ^ (t >>> 14)) >>> 0) / 4294967296
        ((t ^ ((t as u32) >> 14) as i32) as u32) as f64 / 4_294_967_296.0
    }

    pub fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.next()
    }
}

/// ECMAScript ToInt32: the double's integer value modulo 2^32, as a signed integer.
pub fn to_int32(value: f64) -> i32 {
    to_uint32(value) as i32
}

/// ECMAScript ToUint32 (`value >>> 0`).
pub fn to_uint32(value: f64) -> u32 {
    if !value.is_finite() {
        return 0;
    }
    // fmod is exact, so large doubles keep their low 32 bits as JS computes them.
    let wrapped = value.trunc() % 4_294_967_296.0;
    (wrapped as i64) as u32
}

/// `Math.round`: halves round toward positive infinity.
pub fn js_round(value: f64) -> f64 {
    let floor = value.floor();
    if value - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

/// `Math.min` for two values: NaN wins, unlike `f64::min`.
pub fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b {
        a
    } else {
        b
    }
}

/// `Math.max` for two values: NaN wins, unlike `f64::max`.
pub fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else {
        b
    }
}

/// `Math.max(low, Math.min(high, value))`.
pub fn clamp(value: f64, low: f64, high: f64) -> f64 {
    js_max(low, js_min(high, value))
}

/// Three-argument `Math.hypot`.
pub fn hypot3(x: f64, y: f64, z: f64) -> f64 {
    (x * x + y * y + z * z).sqrt()
}

pub fn distance(a: Vec2, b: Vec2) -> f64 {
    (a.x - b.x).hypot(a.z - b.z)
}

/// Signed shortest turn from `a` to `b`, in radians.
pub fn angle_delta(a: f64, b: f64) -> f64 {
    (b - a).sin().atan2((b - a).cos())
}

/// Highest score wins; equal scores retain the original candidate order.
pub fn best_by<T>(
    items: impl IntoIterator<Item = T>,
    mut score: impl FnMut(&T) -> f64,
) -> Option<T> {
    let mut best = None;
    let mut highest = f64::NEG_INFINITY;
    for item in items {
        let value = score(&item);
        if value > highest {
            best = Some(item);
            highest = value;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference draws from the TypeScript `Random`, computed with
    /// `node --import tsx -e 'import { Random } from "./src/game/math.ts"; ...'`.
    #[test]
    fn mulberry_matches_javascript() {
        let cases: [(f64, [f64; 5]); 6] = [
            (
                12345.0,
                [
                    0.9797282677609473,
                    0.3067522644996643,
                    0.484205421525985,
                    0.817934412509203,
                    0.5094283693470061,
                ],
            ),
            (
                79.0,
                [
                    0.031801843317225575,
                    0.9946586957667023,
                    0.7499384770635515,
                    0.39322641328908503,
                    0.9233031782787293,
                ],
            ),
            (
                0.0,
                [
                    0.26642920868471265,
                    0.0003297457005828619,
                    0.2232720274478197,
                    0.1462021479383111,
                    0.46732782293111086,
                ],
            ),
            (
                4294967295.0,
                [
                    0.8964226141106337,
                    0.189478256739676,
                    0.7156526781618595,
                    0.9440599093213677,
                    0.8452364315744489,
                ],
            ),
            (
                1e15,
                [
                    0.4975225117523223,
                    0.35765172680839896,
                    0.6440928732044995,
                    0.5371393547393382,
                    0.16874345601536334,
                ],
            ),
            (
                9007199254740000.0,
                [
                    0.12178368237800896,
                    0.21338037331588566,
                    0.2742189238779247,
                    0.04447795427404344,
                    0.03171212412416935,
                ],
            ),
        ];
        for (seed, draws) in cases {
            let mut random = Random::new(seed);
            for expected in draws {
                assert_eq!(random.next(), expected, "seed {seed}");
            }
        }
        // Past 2^53 the double state rounds on every increment, exactly like JS.
        let mut random = Random::new(1e15);
        for _ in 0..5_000_000 {
            random.next();
        }
        assert_eq!(random.state, 10157829064371780.0);
        assert_eq!(random.next(), 0.9079377842135727);
        assert_eq!(random.next(), 0.24066300364211202);
    }

    #[test]
    fn js_integer_conversions() {
        assert_eq!(to_int32(4_294_967_296.0 + 5.0), 5);
        assert_eq!(to_int32(-1.0), -1);
        assert_eq!(to_uint32(-1.0), u32::MAX);
        assert_eq!(to_int32(2_147_483_648.0), i32::MIN);
        assert_eq!(to_uint32(f64::NAN), 0);
        assert_eq!(to_int32(1e20), 1_661_992_960);
        assert_eq!(js_round(-2.5), -2.0);
        assert_eq!(js_round(2.5), 3.0);
        assert!(js_min(f64::NAN, 1.0).is_nan());
    }
}
