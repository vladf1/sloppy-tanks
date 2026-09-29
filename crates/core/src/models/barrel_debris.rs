//! Port of `barrel-debris.ts`: torn steel scraps of an exploded drum.

use glam::DVec3;

use crate::geometry::{BoxGeometry, CylinderGeometry, Mesh};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BarrelScrap {
    /// A curled wall scrap.
    Shell,
    /// The buckled, ragged lid.
    Lid,
}

/// `barrelScrapGeometry(kind)`: closed, low-poly torn steel with unit bounds, so the
/// instanced dimensions match the simple colliders.
pub fn barrel_scrap_geometry(kind: BarrelScrap) -> Mesh {
    let mut mesh = match kind {
        BarrelScrap::Shell => BoxGeometry {
            width: 1.0,
            height: 1.0,
            depth: 0.12,
            width_segments: 4,
            height_segments: 3,
            depth_segments: 1,
        }
        .build(),
        BarrelScrap::Lid => CylinderGeometry {
            radius_top: 0.5,
            radius_bottom: 0.5,
            height: 0.12,
            radial_segments: 10,
            height_segments: 1,
            ..CylinderGeometry::default()
        }
        .build(),
    };
    for p in &mut mesh.positions {
        let [x, y, z] = p.map(f64::from);
        let bent = match kind {
            BarrelScrap::Shell => [
                x * (0.83 + 0.17 * (y * 19.0).cos()),
                y + 0.07 * (x * 23.0).sin(),
                z + 0.7 * x * x + 0.12 * (y * 8.0 + x * 5.0).sin(),
            ],
            BarrelScrap::Lid => {
                let angle = z.atan2(x);
                let radius = 0.83 + 0.17 * (angle * 5.0).cos();
                [x * radius, y + 0.45 * x.abs() - 0.18 * z, z * radius]
            }
        };
        *p = bent.map(|v| v as f32);
    }
    let size: DVec3 = mesh.bounding_box().size();
    mesh.center();
    mesh.scale(1.0 / size.x, 1.0 / size.y, 1.0 / size.z);
    mesh.compute_vertex_normals();
    mesh
}
