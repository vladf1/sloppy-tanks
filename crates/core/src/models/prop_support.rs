//! Helpers shared by the cover, tree and prop models: the seeded Mulberry32 stream
//! (`Random` in `src/game/math.ts`) and the Three.js r185 quaternion and matrix
//! operations those models used, with Three's exact operation order.

use glam::{DMat4, DQuat, DVec3};

use crate::geometry::math::{compose, decompose, hex_to_linear, linear_to_hex, to_int32};
use crate::scene::Node;

/// Mulberry32 as in `src/game/math.ts`: the state is a JavaScript number, so it grows
/// as a double (never wraps) and each draw converts it with ToInt32. Model seeds and
/// draw order are part of the look: a changed draw moves every later chip and bough.
///
/// Shared with `sim::Random`; de-duplicate at integration.
#[derive(Clone, Debug)]
pub struct Random {
    pub state: f64,
}

impl Random {
    pub fn new(seed: f64) -> Self {
        Self { state: seed }
    }

    /// The next draw in 0..1 (the TypeScript name; not an iterator).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> f64 {
        self.state += f64::from(0x6d2b_79f5_u32);
        let mut t = to_int32(self.state);
        t = (t ^ ((t as u32) >> 15) as i32).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ ((t as u32) >> 7) as i32).wrapping_mul(t | 61));
        f64::from((t ^ ((t as u32) >> 14) as i32) as u32) / 4_294_967_296.0
    }

    pub fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.next()
    }
}

/// `Math.imul`.
pub(crate) fn imul(a: i32, b: i32) -> i32 {
    a.wrapping_mul(b)
}

/// V8's `Math.hypot(a, b)`: scaled by the larger magnitude with a compensated sum,
/// which can differ from libm's `hypot` in the last bit.
pub(crate) fn js_hypot(a: f64, b: f64) -> f64 {
    let values = [a.abs(), b.abs()];
    let max = values[0].max(values[1]);
    if max == f64::INFINITY {
        return f64::INFINITY;
    }
    if values.iter().any(|v| v.is_nan()) {
        return f64::NAN;
    }
    if max == 0.0 {
        return 0.0;
    }
    let (mut sum, mut compensation) = (0.0f64, 0.0f64);
    for value in values {
        let n = value / max;
        let summand = n * n - compensation;
        let preliminary = sum + summand;
        compensation = (preliminary - sum) - summand;
        sum = preliminary;
    }
    sum.sqrt() * max
}

/// `THREE.MathUtils.clamp`.
pub(crate) fn clamp(value: f64, min: f64, max: f64) -> f64 {
    min.max(max.min(value))
}

/// `new THREE.Color(a).multiply(new THREE.Color(b)).getHex()`.
pub(crate) fn multiply_hex(a: u32, b: u32) -> u32 {
    let (a, b) = (hex_to_linear(a), hex_to_linear(b));
    linear_to_hex([a[0] * b[0], a[1] * b[1], a[2] * b[2]])
}

/// `Quaternion.normalize`.
pub(crate) fn quat_normalize(q: DQuat) -> DQuat {
    let length = (q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w).sqrt();
    if length == 0.0 {
        return DQuat::IDENTITY;
    }
    let inverse = 1.0 / length;
    DQuat::from_xyzw(q.x * inverse, q.y * inverse, q.z * inverse, q.w * inverse)
}

/// `Quaternion.setFromUnitVectors(from, to)`.
pub(crate) fn quat_from_unit_vectors(from: DVec3, to: DVec3) -> DQuat {
    let r = from.x * to.x + from.y * to.y + from.z * to.z + 1.0;
    let q = if r < 1e-8 {
        if from.x.abs() > from.z.abs() {
            DQuat::from_xyzw(-from.y, from.x, 0.0, 0.0)
        } else {
            DQuat::from_xyzw(0.0, -from.z, from.y, 0.0)
        }
    } else {
        DQuat::from_xyzw(
            from.y * to.z - from.z * to.y,
            from.z * to.x - from.x * to.z,
            from.x * to.y - from.y * to.x,
            r,
        )
    };
    quat_normalize(q)
}

/// `Quaternion.setFromEuler` for Euler order `YXZ`.
pub(crate) fn quat_from_euler_yxz(x: f64, y: f64, z: f64) -> DQuat {
    let (c1, c2, c3) = ((x / 2.0).cos(), (y / 2.0).cos(), (z / 2.0).cos());
    let (s1, s2, s3) = ((x / 2.0).sin(), (y / 2.0).sin(), (z / 2.0).sin());
    DQuat::from_xyzw(
        s1 * c2 * c3 + c1 * s2 * s3,
        c1 * s2 * c3 - s1 * c2 * s3,
        c1 * c2 * s3 - s1 * s2 * c3,
        c1 * c2 * c3 + s1 * s2 * s3,
    )
}

/// `Quaternion.multiplyQuaternions(a, b)`.
pub(crate) fn quat_multiply(a: DQuat, b: DQuat) -> DQuat {
    DQuat::from_xyzw(
        a.x * b.w + a.w * b.x + a.y * b.z - a.z * b.y,
        a.y * b.w + a.w * b.y + a.z * b.x - a.x * b.z,
        a.z * b.w + a.w * b.z + a.x * b.y - a.y * b.x,
        a.w * b.w - a.x * b.x - a.y * b.y - a.z * b.z,
    )
}

/// `Object3D.rotateZ(angle)`: a local rotation after the current one.
pub(crate) fn rotate_z(q: DQuat, angle: f64) -> DQuat {
    let half = angle / 2.0;
    quat_multiply(q, DQuat::from_xyzw(0.0, 0.0, half.sin(), half.cos()))
}

/// `Vector3.applyEuler(new Euler(x, y, z))` (XYZ): rotate by the Euler's quaternion.
pub(crate) fn apply_euler(v: DVec3, x: f64, y: f64, z: f64) -> DVec3 {
    apply_quaternion(v, crate::geometry::math::quat_from_euler(x, y, z))
}

/// `Vector3.applyQuaternion`.
pub(crate) fn apply_quaternion(v: DVec3, q: DQuat) -> DVec3 {
    let (vx, vy, vz) = (v.x, v.y, v.z);
    let (qx, qy, qz, qw) = (q.x, q.y, q.z, q.w);
    let tx = 2.0 * (qy * vz - qz * vy);
    let ty = 2.0 * (qz * vx - qx * vz);
    let tz = 2.0 * (qx * vy - qy * vx);
    DVec3::new(
        vx + qw * tx + qy * tz - qz * ty,
        vy + qw * ty + qz * tx - qx * tz,
        vz + qw * tz + qx * ty - qy * tx,
    )
}

/// `object.applyMatrix4(matrix)` for an object with automatic matrix updates:
/// premultiply its composed matrix and decompose the product back into the
/// node's position, rotation and scale.
pub(crate) fn apply_matrix_to_node(node: &mut Node, matrix: &DMat4) {
    let product = *matrix * compose(node.position, node.rotation, node.scale);
    let (position, rotation, scale) = decompose(&product);
    node.position = position;
    node.rotation = rotation;
    node.scale = scale;
}
