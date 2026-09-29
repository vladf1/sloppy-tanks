//! Port of `cottage-details.ts`: window boxes with soil, shrubs and flowers.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::DVec3;

use super::model_primitives::{Cache, box_part, material, put};
use crate::geometry::math::js_round;
use crate::geometry::{Mesh, octahedron_geometry, plane_geometry};
use crate::scene::Node;

const FLOWERS: [u32; 3] = [0xddb1ac, 0xf2d893, 0xc5b4d0];
const PLANTER: u32 = 0x947047;
const SOIL: u32 = 0x493f2d;
const LEAVES: u32 = 0x3d703e;

static GEOMETRY: Cache<u8, Mesh> = Cache::new();

fn soil() -> Arc<Mesh> {
    GEOMETRY.get_or_insert(0, || {
        let mut mesh = plane_geometry(1.2, 0.28);
        mesh.rotate_x(-PI / 2.0);
        mesh
    })
}

/// The shrub and blossom octahedra (two geometries in the TypeScript, identical).
fn octahedron() -> Arc<Mesh> {
    GEOMETRY.get_or_insert(1, || octahedron_geometry(1.0, 0))
}

/// `cottageDetails(group, c)`: attached garden details that disappear with the
/// cottage, rather than leaving floating planters.
pub fn cottage_details(group: &mut Node, w: f64, d: f64, h: f64, x: f64, z: f64) {
    let wall = h * 0.68;
    let color = FLOWERS[(js_round(x + z).abs() % FLOWERS.len() as f64) as usize];
    for side in [-1.0, 1.0] {
        for x in [-w * 0.29, w * 0.29] {
            let z = side * (d / 2.0 + 0.28);
            let y = wall * 0.59 - 0.79;
            put(group, box_part(1.34, 0.23, 0.38, PLANTER, 0.0), x, y, z);
            put(
                group,
                Node::mesh(soil(), material(SOIL, 0.0, 1.0)),
                x,
                y + 0.13,
                z,
            );
            for offset in [-0.34, 0.34] {
                let mut bush = Node::mesh(octahedron(), material(LEAVES, 0.0, 1.0));
                bush.scale = DVec3::new(0.35, 0.17, 0.19);
                put(group, bush, x + offset, y + 0.22, z);
                let mut flower = Node::mesh(octahedron(), material(color, 0.0, 0.9));
                flower.scale = DVec3::new(0.1, 0.065, 0.1);
                put(group, flower, x + offset * 0.8, y + 0.37, z + side * 0.05);
            }
        }
    }
}
