//! Three.js r185 math with its exact operation order. Generators and measurements
//! call these instead of glam's equivalents where the two can round differently
//! (normalisation of a zero vector, `Math.sign`, Euler conversion, matrix inverse),
//! so meshes and hull bounds match the previous models to the last f32 bit.

use glam::{DMat3, DMat4, DQuat, DVec3};

/// JavaScript `Math.sign`: zero (of either sign) stays zero, unlike `f64::signum`.
pub fn js_sign(value: f64) -> f64 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        value
    }
}

// JavaScript number conversions shared with the simulation, which owns them.
pub use crate::sim::math::{js_round, to_int32};

/// `Vector3.normalize()`: multiply by the reciprocal length; a zero vector stays zero.
pub fn normalize(v: DVec3) -> DVec3 {
    let length = v.length();
    v * (1.0 / if length == 0.0 { 1.0 } else { length })
}

/// `Vector3.angleTo`.
pub fn angle_to(a: DVec3, b: DVec3) -> f64 {
    let denominator = (a.length_squared() * b.length_squared()).sqrt();
    if denominator == 0.0 {
        return std::f64::consts::FRAC_PI_2;
    }
    let theta = a.dot(b) / denominator;
    theta.clamp(-1.0, 1.0).acos()
}

/// `Vector3.applyMatrix4` for affine and projective matrices.
pub fn transform_point(m: &DMat4, p: DVec3) -> DVec3 {
    let e = m.to_cols_array();
    let w = 1.0 / (e[3] * p.x + e[7] * p.y + e[11] * p.z + e[15]);
    DVec3::new(
        (e[0] * p.x + e[4] * p.y + e[8] * p.z + e[12]) * w,
        (e[1] * p.x + e[5] * p.y + e[9] * p.z + e[13]) * w,
        (e[2] * p.x + e[6] * p.y + e[10] * p.z + e[14]) * w,
    )
}

/// `Matrix3.getNormalMatrix(m)`: the inverse transpose of the upper 3x3, using
/// Three's cofactor inverse (a singular matrix becomes all zeros).
pub fn normal_matrix(m: &DMat4) -> DMat3 {
    let me = m.to_cols_array();
    let (n11, n21, n31) = (me[0], me[1], me[2]);
    let (n12, n22, n32) = (me[4], me[5], me[6]);
    let (n13, n23, n33) = (me[8], me[9], me[10]);
    let t11 = n33 * n22 - n32 * n23;
    let t12 = n32 * n13 - n33 * n12;
    let t13 = n23 * n12 - n22 * n13;
    let det = n11 * t11 + n21 * t12 + n31 * t13;
    if det == 0.0 {
        return DMat3::ZERO;
    }
    let det_inv = 1.0 / det;
    // Column-major inverse, as Three stores it before transposing.
    let inverse = [
        t11 * det_inv,
        (n31 * n23 - n33 * n21) * det_inv,
        (n32 * n21 - n31 * n22) * det_inv,
        t12 * det_inv,
        (n33 * n11 - n31 * n13) * det_inv,
        (n31 * n12 - n32 * n11) * det_inv,
        t13 * det_inv,
        (n21 * n13 - n23 * n11) * det_inv,
        (n22 * n11 - n21 * n12) * det_inv,
    ];
    DMat3::from_cols_array(&inverse).transpose()
}

/// `Vector3.applyNormalMatrix`: multiply by the normal matrix, then normalise.
pub fn transform_normal(normal_matrix: &DMat3, n: DVec3) -> DVec3 {
    let e = normal_matrix.to_cols_array();
    normalize(DVec3::new(
        e[0] * n.x + e[3] * n.y + e[6] * n.z,
        e[1] * n.x + e[4] * n.y + e[7] * n.z,
        e[2] * n.x + e[5] * n.y + e[8] * n.z,
    ))
}

/// `Matrix4.makeRotationX`.
pub fn rotation_x(angle: f64) -> DMat4 {
    let (s, c) = (angle.sin(), angle.cos());
    DMat4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, 0.0, c, s, 0.0, 0.0, -s, c, 0.0, 0.0, 0.0, 0.0, 1.0,
    ])
}

/// `Matrix4.makeRotationY`.
pub fn rotation_y(angle: f64) -> DMat4 {
    let (s, c) = (angle.sin(), angle.cos());
    DMat4::from_cols_array(&[
        c, 0.0, -s, 0.0, 0.0, 1.0, 0.0, 0.0, s, 0.0, c, 0.0, 0.0, 0.0, 0.0, 1.0,
    ])
}

/// `Matrix4.makeRotationZ`.
pub fn rotation_z(angle: f64) -> DMat4 {
    let (s, c) = (angle.sin(), angle.cos());
    DMat4::from_cols_array(&[
        c, s, 0.0, 0.0, -s, c, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ])
}

/// `Quaternion.setFromEuler` for Three's default `XYZ` order: the rotation of an
/// Object3D whose `rotation` was set to `(x, y, z)`.
pub fn quat_from_euler(x: f64, y: f64, z: f64) -> DQuat {
    let (c1, c2, c3) = ((x / 2.0).cos(), (y / 2.0).cos(), (z / 2.0).cos());
    let (s1, s2, s3) = ((x / 2.0).sin(), (y / 2.0).sin(), (z / 2.0).sin());
    DQuat::from_xyzw(
        s1 * c2 * c3 + c1 * s2 * s3,
        c1 * s2 * c3 - s1 * c2 * s3,
        c1 * c2 * s3 + s1 * s2 * c3,
        c1 * c2 * c3 - s1 * s2 * s3,
    )
}

/// `Quaternion.setFromEuler` for Euler order `YXZ`.
pub fn quat_from_euler_yxz(x: f64, y: f64, z: f64) -> DQuat {
    let (c1, c2, c3) = ((x / 2.0).cos(), (y / 2.0).cos(), (z / 2.0).cos());
    let (s1, s2, s3) = ((x / 2.0).sin(), (y / 2.0).sin(), (z / 2.0).sin());
    DQuat::from_xyzw(
        s1 * c2 * c3 + c1 * s2 * s3,
        c1 * s2 * c3 - s1 * c2 * s3,
        c1 * c2 * s3 - s1 * s2 * c3,
        c1 * c2 * c3 + s1 * s2 * s3,
    )
}

/// `Quaternion.normalize`: a zero quaternion becomes the identity.
pub fn quat_normalize(q: DQuat) -> DQuat {
    let length = (q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w).sqrt();
    if length == 0.0 {
        return DQuat::IDENTITY;
    }
    let inverse = 1.0 / length;
    DQuat::from_xyzw(q.x * inverse, q.y * inverse, q.z * inverse, q.w * inverse)
}

/// `Quaternion.setFromUnitVectors(from, to)` for normalised vectors.
pub fn quat_from_unit_vectors(from: DVec3, to: DVec3) -> DQuat {
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

/// `Quaternion.multiplyQuaternions(a, b)`.
pub fn quat_multiply(a: DQuat, b: DQuat) -> DQuat {
    DQuat::from_xyzw(
        a.x * b.w + a.w * b.x + a.y * b.z - a.z * b.y,
        a.y * b.w + a.w * b.y + a.z * b.x - a.x * b.z,
        a.z * b.w + a.w * b.z + a.x * b.y - a.y * b.x,
        a.w * b.w - a.x * b.x - a.y * b.y - a.z * b.z,
    )
}

/// `Object3D.rotateZ(angle)`: a local rotation after the current one.
pub fn quat_rotate_z(q: DQuat, angle: f64) -> DQuat {
    let half = angle / 2.0;
    quat_multiply(q, DQuat::from_xyzw(0.0, 0.0, half.sin(), half.cos()))
}

/// `Vector3.applyQuaternion`.
pub fn apply_quaternion(v: DVec3, q: DQuat) -> DVec3 {
    let tx = 2.0 * (q.y * v.z - q.z * v.y);
    let ty = 2.0 * (q.z * v.x - q.x * v.z);
    let tz = 2.0 * (q.x * v.y - q.y * v.x);
    DVec3::new(
        v.x + q.w * tx + q.y * tz - q.z * ty,
        v.y + q.w * ty + q.z * tx - q.x * tz,
        v.z + q.w * tz + q.x * ty - q.y * tx,
    )
}

/// `Vector3.applyEuler(new Euler(x, y, z))` in the default `XYZ` order.
pub fn apply_euler(v: DVec3, x: f64, y: f64, z: f64) -> DVec3 {
    apply_quaternion(v, quat_from_euler(x, y, z))
}

/// `Matrix4.compose(position, quaternion, scale)`.
pub fn compose(position: DVec3, rotation: DQuat, scale: DVec3) -> DMat4 {
    let (x, y, z, w) = (rotation.x, rotation.y, rotation.z, rotation.w);
    let (x2, y2, z2) = (x + x, y + y, z + z);
    let (xx, xy, xz) = (x * x2, x * y2, x * z2);
    let (yy, yz, zz) = (y * y2, y * z2, z * z2);
    let (wx, wy, wz) = (w * x2, w * y2, w * z2);
    let (sx, sy, sz) = (scale.x, scale.y, scale.z);
    DMat4::from_cols_array(&[
        (1.0 - (yy + zz)) * sx,
        (xy + wz) * sx,
        (xz - wy) * sx,
        0.0,
        (xy - wz) * sy,
        (1.0 - (xx + zz)) * sy,
        (yz + wx) * sy,
        0.0,
        (xz + wy) * sz,
        (yz - wx) * sz,
        (1.0 - (xx + yy)) * sz,
        0.0,
        position.x,
        position.y,
        position.z,
        1.0,
    ])
}

/// `Matrix4.decompose`: position, rotation and scale of an affine matrix. Like
/// Three, a sheared matrix loses its shear, and a mirrored one flips its x scale.
pub fn decompose(m: &DMat4) -> (DVec3, DQuat, DVec3) {
    let te = m.to_cols_array();
    let position = DVec3::new(te[12], te[13], te[14]);
    let det = determinant_affine(&te);
    if det == 0.0 {
        return (position, DQuat::IDENTITY, DVec3::ONE);
    }
    let mut sx = DVec3::new(te[0], te[1], te[2]).length();
    let sy = DVec3::new(te[4], te[5], te[6]).length();
    let sz = DVec3::new(te[8], te[9], te[10]).length();
    if det < 0.0 {
        sx = -sx;
    }
    let mut r = te;
    let (inv_sx, inv_sy, inv_sz) = (1.0 / sx, 1.0 / sy, 1.0 / sz);
    for i in 0..3 {
        r[i] *= inv_sx;
        r[4 + i] *= inv_sy;
        r[8 + i] *= inv_sz;
    }
    (
        position,
        quat_from_rotation_matrix(&r),
        DVec3::new(sx, sy, sz),
    )
}

fn determinant_affine(te: &[f64; 16]) -> f64 {
    let (n11, n12, n13) = (te[0], te[4], te[8]);
    let (n21, n22, n23) = (te[1], te[5], te[9]);
    let (n31, n32, n33) = (te[2], te[6], te[10]);
    n11 * (n22 * n33 - n23 * n32) - n12 * (n21 * n33 - n23 * n31) + n13 * (n21 * n32 - n22 * n31)
}

/// `Quaternion.setFromRotationMatrix` on column-major elements.
fn quat_from_rotation_matrix(te: &[f64; 16]) -> DQuat {
    let (m11, m12, m13) = (te[0], te[4], te[8]);
    let (m21, m22, m23) = (te[1], te[5], te[9]);
    let (m31, m32, m33) = (te[2], te[6], te[10]);
    let trace = m11 + m22 + m33;
    if trace > 0.0 {
        let s = 0.5 / (trace + 1.0).sqrt();
        DQuat::from_xyzw((m32 - m23) * s, (m13 - m31) * s, (m21 - m12) * s, 0.25 / s)
    } else if m11 > m22 && m11 > m33 {
        let s = 2.0 * (1.0 + m11 - m22 - m33).sqrt();
        DQuat::from_xyzw(0.25 * s, (m12 + m21) / s, (m13 + m31) / s, (m32 - m23) / s)
    } else if m22 > m33 {
        let s = 2.0 * (1.0 + m22 - m11 - m33).sqrt();
        DQuat::from_xyzw((m12 + m21) / s, 0.25 * s, (m23 + m32) / s, (m13 - m31) / s)
    } else {
        let s = 2.0 * (1.0 + m33 - m11 - m22).sqrt();
        DQuat::from_xyzw((m13 + m31) / s, (m23 + m32) / s, 0.25 * s, (m21 - m12) / s)
    }
}

/// Three.js `SRGBToLinear` for one channel in 0..=1.
pub fn srgb_to_linear(c: f64) -> f64 {
    if c < 0.04045 {
        c * 0.077_399_380_8
    } else {
        (c * 0.947_867_298_6 + 0.052_132_701_4).powf(2.4)
    }
}

/// Three.js `LinearToSRGB` for one channel.
pub fn linear_to_srgb(c: f64) -> f64 {
    if c < 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(0.41666) - 0.055
    }
}

/// `new THREE.Color(hex)`: linear working-space channels of an sRGB hex color.
pub fn hex_to_linear(hex: u32) -> [f64; 3] {
    [
        srgb_to_linear(f64::from((hex >> 16) & 255) / 255.0),
        srgb_to_linear(f64::from((hex >> 8) & 255) / 255.0),
        srgb_to_linear(f64::from(hex & 255) / 255.0),
    ]
}

/// `Color.getHex()`: sRGB hex of linear working-space channels.
pub fn linear_to_hex(rgb: [f64; 3]) -> u32 {
    let channel = |c: f64| js_round((linear_to_srgb(c) * 255.0).clamp(0.0, 255.0)) as u32;
    channel(rgb[0]) * 65536 + channel(rgb[1]) * 256 + channel(rgb[2])
}

/// `new THREE.Color(hex).multiplyScalar(factor).getHex()`: darken or brighten in
/// linear space, as the models' shade colors were derived.
pub fn scale_hex_color(hex: u32, factor: f64) -> u32 {
    let [r, g, b] = hex_to_linear(hex);
    linear_to_hex([r * factor, g * factor, b * factor])
}

/// `new THREE.Color(a).multiply(new THREE.Color(b)).getHex()`.
pub fn multiply_hex(a: u32, b: u32) -> u32 {
    let (a, b) = (hex_to_linear(a), hex_to_linear(b));
    linear_to_hex([a[0] * b[0], a[1] * b[1], a[2] * b[2]])
}

/// `Color.lerp(target, alpha)` on linear channels.
pub fn lerp_color(color: [f64; 3], target: [f64; 3], alpha: f64) -> [f64; 3] {
    [
        color[0] + (target[0] - color[0]) * alpha,
        color[1] + (target[1] - color[1]) * alpha,
        color[2] + (target[2] - color[2]) * alpha,
    ]
}

/// V8's `Math.hypot`: scale by the largest magnitude and Kahan-sum the squares,
/// which can differ from libm's `hypot` in the last bit.
pub fn js_hypot(values: &[f64]) -> f64 {
    let max = values.iter().fold(0.0f64, |max, v| max.max(v.abs()));
    if max == f64::INFINITY {
        return f64::INFINITY;
    }
    if values.iter().any(|v| v.is_nan()) {
        return f64::NAN;
    }
    if max == 0.0 {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut compensation = 0.0;
    for value in values {
        let n = value.abs() / max;
        let summand = n * n - compensation;
        let preliminary = sum + summand;
        compensation = (preliminary - sum) - summand;
        sum = preliminary;
    }
    sum.sqrt() * max
}

/// `MathUtils.smoothstep(x, min, max)`.
pub fn smoothstep(x: f64, min: f64, max: f64) -> f64 {
    if x <= min {
        return 0.0;
    }
    if x >= max {
        return 1.0;
    }
    let x = (x - min) / (max - min);
    x * x * (3.0 - 2.0 * x)
}

/// `MathUtils.lerp(x, y, t)`.
pub fn lerp(x: f64, y: f64, t: f64) -> f64 {
    (1.0 - t) * x + t * y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_sign_keeps_zero() {
        assert_eq!(js_sign(0.0), 0.0);
        assert_eq!(js_sign(-3.0), -1.0);
        assert_eq!(js_sign(2.0), 1.0);
    }

    #[test]
    fn to_int32_truncates_and_wraps() {
        assert_eq!(to_int32(-1.7), -1);
        assert_eq!(to_int32(2_147_483_648.0), -2_147_483_648);
        assert_eq!(to_int32(12.9), 12);
    }

    #[test]
    fn euler_matches_matrix_rotation() {
        let q = quat_from_euler(0.3, 0.0, 0.0);
        let a = compose(DVec3::ZERO, q, DVec3::ONE);
        let b = rotation_x(0.3);
        assert!(a.abs_diff_eq(b, 1e-15));
    }

    #[test]
    fn decompose_round_trips_compose() {
        let q = quat_from_euler(0.2, -0.4, 1.1);
        let m = compose(DVec3::new(1.0, 2.0, 3.0), q, DVec3::new(2.0, 0.5, 1.5));
        let (p, r, s) = decompose(&m);
        assert!(compose(p, r, s).abs_diff_eq(m, 1e-12));
    }
}
