//! Ports of Three.js r185 PolyhedronGeometry and its Icosahedron, Octahedron and
//! Tetrahedron presets: subdivided faces projected onto a sphere, non-indexed, with
//! spherical UVs corrected at the poles and the seam. Detail 0 has flat normals;
//! higher detail uses the smooth (radial) normals.

use std::f64::consts::PI;

use glam::DVec3;

use super::math::normalize;
use super::mesh::Mesh;

pub fn polyhedron_geometry(vertices: &[f64], indices: &[u32], radius: f64, detail: u32) -> Mesh {
    let mut buffer: Vec<DVec3> = Vec::new();
    let vertex = |index: u32| {
        let at = index as usize * 3;
        DVec3::new(vertices[at], vertices[at + 1], vertices[at + 2])
    };
    for face in indices.as_chunks::<3>().0 {
        subdivide_face(
            &mut buffer,
            vertex(face[0]),
            vertex(face[1]),
            vertex(face[2]),
            detail,
        );
    }
    for v in &mut buffer {
        *v = normalize(*v) * radius;
    }
    let mut uvs: Vec<[f64; 2]> = buffer
        .iter()
        .map(|&v| {
            [
                azimuth(v) / 2.0 / PI + 0.5,
                1.0 - (inclination(v) / PI + 0.5),
            ]
        })
        .collect();
    correct_uvs(&buffer, &mut uvs);
    correct_seam(&mut uvs);
    let mut mesh = Mesh {
        positions: buffer
            .iter()
            .map(|v| [v.x as f32, v.y as f32, v.z as f32])
            .collect(),
        normals: buffer
            .iter()
            .map(|v| [v.x as f32, v.y as f32, v.z as f32])
            .collect(),
        uvs: uvs.iter().map(|uv| [uv[0] as f32, uv[1] as f32]).collect(),
        ..Mesh::default()
    };
    if detail == 0 {
        mesh.compute_vertex_normals();
    } else {
        mesh.normalize_normals();
    }
    mesh
}

/// `Vector3.lerp`: `a + (b - a) * alpha`, per component.
fn lerp(a: DVec3, b: DVec3, alpha: f64) -> DVec3 {
    DVec3::new(
        a.x + (b.x - a.x) * alpha,
        a.y + (b.y - a.y) * alpha,
        a.z + (b.z - a.z) * alpha,
    )
}

fn subdivide_face(buffer: &mut Vec<DVec3>, a: DVec3, b: DVec3, c: DVec3, detail: u32) {
    let cols = detail as usize + 1;
    let mut grid: Vec<Vec<DVec3>> = Vec::with_capacity(cols + 1);
    for i in 0..=cols {
        let aj = lerp(a, c, i as f64 / cols as f64);
        let bj = lerp(b, c, i as f64 / cols as f64);
        let rows = cols - i;
        let row = (0..=rows)
            .map(|j| {
                if j == 0 && i == cols {
                    aj
                } else {
                    lerp(aj, bj, j as f64 / rows as f64)
                }
            })
            .collect();
        grid.push(row);
    }
    for i in 0..cols {
        for j in 0..2 * (cols - i) - 1 {
            let k = j / 2;
            if j % 2 == 0 {
                buffer.extend([grid[i][k + 1], grid[i + 1][k], grid[i][k]]);
            } else {
                buffer.extend([grid[i][k + 1], grid[i + 1][k + 1], grid[i + 1][k]]);
            }
        }
    }
}

fn azimuth(v: DVec3) -> f64 {
    v.z.atan2(-v.x)
}

fn inclination(v: DVec3) -> f64 {
    (-v.y).atan2((v.x * v.x + v.z * v.z).sqrt())
}

/// Pole vertices take the face centroid's azimuth; faces straddling the back
/// meridian (azimuth < 0) move their u = 1 corners to 0.
fn correct_uvs(buffer: &[DVec3], uvs: &mut [[f64; 2]]) {
    for (face, corners) in buffer.as_chunks::<3>().0.iter().enumerate() {
        // `divideScalar(3)` multiplies by the reciprocal.
        let centroid = (corners[0] + corners[1] + corners[2]) * (1.0 / 3.0);
        let face_azimuth = azimuth(centroid);
        for (k, corner) in corners.iter().enumerate() {
            let uv = &mut uvs[face * 3 + k];
            let u = uv[0];
            if face_azimuth < 0.0 && u == 1.0 {
                uv[0] = u - 1.0;
            }
            if corner.x == 0.0 && corner.z == 0.0 {
                uv[0] = face_azimuth / 2.0 / PI + 0.5;
            }
        }
    }
}

/// Faces spanning the u seam wrap their small u values past 1. Three steps through
/// the uv array six floats (three u/v pairs) at a time.
fn correct_seam(uvs: &mut [[f64; 2]]) {
    for face in uvs.as_chunks_mut::<3>().0 {
        let (x0, x1, x2) = (face[0][0], face[1][0], face[2][0]);
        let max = x0.max(x1).max(x2);
        let min = x0.min(x1).min(x2);
        if max > 0.9 && min < 0.1 {
            for corner in face.iter_mut() {
                if corner[0] < 0.2 {
                    corner[0] += 1.0;
                }
            }
        }
    }
}

/// `THREE.IcosahedronGeometry(radius, detail)`.
pub fn icosahedron_geometry(radius: f64, detail: u32) -> Mesh {
    let t = (1.0 + 5f64.sqrt()) / 2.0;
    #[rustfmt::skip]
    let vertices = [
        -1.0, t, 0.0, 1.0, t, 0.0, -1.0, -t, 0.0, 1.0, -t, 0.0,
        0.0, -1.0, t, 0.0, 1.0, t, 0.0, -1.0, -t, 0.0, 1.0, -t,
        t, 0.0, -1.0, t, 0.0, 1.0, -t, 0.0, -1.0, -t, 0.0, 1.0,
    ];
    #[rustfmt::skip]
    let indices = [
        0, 11, 5, 0, 5, 1, 0, 1, 7, 0, 7, 10, 0, 10, 11,
        1, 5, 9, 5, 11, 4, 11, 10, 2, 10, 7, 6, 7, 1, 8,
        3, 9, 4, 3, 4, 2, 3, 2, 6, 3, 6, 8, 3, 8, 9,
        4, 9, 5, 2, 4, 11, 6, 2, 10, 8, 6, 7, 9, 8, 1,
    ];
    polyhedron_geometry(&vertices, &indices, radius, detail)
}

/// `THREE.OctahedronGeometry(radius, detail)`.
pub fn octahedron_geometry(radius: f64, detail: u32) -> Mesh {
    #[rustfmt::skip]
    let vertices = [
        1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 1.0, 0.0,
        0.0, -1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -1.0,
    ];
    #[rustfmt::skip]
    let indices = [0, 2, 4, 0, 4, 3, 0, 3, 5, 0, 5, 2, 1, 2, 5, 1, 5, 3, 1, 3, 4, 1, 4, 2];
    polyhedron_geometry(&vertices, &indices, radius, detail)
}

/// `THREE.TetrahedronGeometry(radius, detail)`.
pub fn tetrahedron_geometry(radius: f64, detail: u32) -> Mesh {
    #[rustfmt::skip]
    let vertices = [1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0];
    let indices = [2, 1, 0, 0, 3, 2, 1, 3, 0, 2, 3, 1];
    polyhedron_geometry(&vertices, &indices, radius, detail)
}
