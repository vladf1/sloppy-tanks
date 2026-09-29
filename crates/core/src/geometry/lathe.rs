//! Port of Three.js r185 LatheGeometry: a profile in (radius, y) revolved about y.

use std::f64::consts::PI;

use glam::{DVec2, DVec3};

use super::math::normalize;
use super::mesh::Mesh;

/// `THREE.LatheGeometry(points, segments, phiStart, phiLength)`. Normals come from
/// the profile: perpendicular to the first and last segments at the ends, and the
/// normalised sum of the two adjacent segment perpendiculars in between.
pub fn lathe_geometry(points: &[DVec2], segments: u32, phi_start: f64, phi_length: f64) -> Mesh {
    let phi_length = phi_length.clamp(0.0, PI * 2.0);
    let inverse_segments = 1.0 / f64::from(segments);
    let last = points.len() - 1;
    let mut init_normals: Vec<DVec3> = Vec::with_capacity(points.len());
    let mut previous = DVec3::ZERO;
    for j in 0..points.len() {
        if j == 0 {
            let d = points[1] - points[0];
            let normal = DVec3::new(d.y * 1.0, -d.x, d.y * 0.0);
            previous = normal;
            init_normals.push(normalize(normal));
        } else if j == last {
            init_normals.push(previous);
        } else {
            let d = points[j + 1] - points[j];
            let current = DVec3::new(d.y * 1.0, -d.x, d.y * 0.0);
            init_normals.push(normalize(current + previous));
            previous = current;
        }
    }
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    for i in 0..=segments {
        let phi = phi_start + f64::from(i) * inverse_segments * phi_length;
        let (sin, cos) = (phi.sin(), phi.cos());
        for (j, point) in points.iter().enumerate() {
            positions.extend_from_slice(&[point.x * sin, point.y, point.x * cos]);
            uvs.extend_from_slice(&[f64::from(i) / f64::from(segments), j as f64 / last as f64]);
            let n = init_normals[j];
            normals.extend_from_slice(&[n.x * sin, n.y, n.x * cos]);
        }
    }
    let count = points.len() as u32;
    let mut indices = Vec::new();
    for i in 0..segments {
        for j in 0..count - 1 {
            let base = j + i * count;
            let (a, b, c, d) = (base, base + count, base + count + 1, base + 1);
            indices.extend_from_slice(&[a, b, d, c, d, b]);
        }
    }
    Mesh::from_f64(&positions, &normals, &uvs, Some(indices))
}
