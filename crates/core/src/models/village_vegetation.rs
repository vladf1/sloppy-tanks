//! Port of `village-vegetation.ts`: low meadow tufts, reeds and flowers. Detail stays
//! below shell height; the shared tuft instances sway entirely on the GPU
//! ([`effects_scenery::MEADOW_SWAY`]).

use std::f64::consts::PI;
use std::sync::Arc;

use glam::DVec3;

use crate::geometry::math::{compose, hex_to_linear, quat_from_euler};
use crate::geometry::{Attribute, Mesh, octahedron_geometry};
use crate::scene::{Effect, Instance, Material, Node, Side};

use super::effects_scenery::{MEADOW_SWAY, WIND_ORIGIN};
use super::village_landscape::{creek_distance, valley_height_at};
use crate::sim::math::Random;

/// Instance capacities of the tuft and flower meshes.
pub const MEADOW_TUFTS: usize = 3600;
pub const MEADOW_FLOWERS: usize = 1500;
/// Candidate placements tried before the tuft capacity fills.
const MEADOW_CANDIDATES: usize = 4200;
/// The TypeScript's full-turn approximation for tuft yaw (kept for identical output).
#[allow(clippy::approx_constant)]
const TUFT_TURN: f64 = 6.28;
const TUFT_COLORS: [u32; 4] = [0x598245, 0x7b9d51, 0x9eaf6b, 0x477650];
const FLOWER_COLORS: [u32; 4] = [0xe8ddad, 0xbda6ca, 0xe5bb68, 0xf2e6d0];

/// Four crossed blades, each a single triangle, flat-normalled.
fn tuft_geometry() -> Mesh {
    let mut vertices = Vec::with_capacity(36);
    for i in 0..4 {
        let a = f64::from(i) * PI * 0.5;
        let (x, z) = (a.cos(), a.sin());
        vertices.extend([
            -z * 0.11,
            0.0,
            x * 0.11,
            z * 0.11,
            0.0,
            -x * 0.11,
            x * 0.25,
            0.7 + f64::from(i % 2) * 0.3,
            z * 0.25,
        ]);
    }
    let mut geometry = Mesh::from_f64(&vertices, &[], &[], None);
    geometry.compute_vertex_normals();
    geometry
}

fn instance_color(hex: u32) -> Option<[f32; 3]> {
    Some(hex_to_linear(hex).map(|c| c as f32))
}

/// `new VillageVegetation().group` (`village-meadow`): the instanced tufts (with
/// the per-instance [`WIND_ORIGIN`] attribute) and flowers.
pub fn village_vegetation() -> Node {
    let mut group = Node::group("village-meadow");
    let mut rng = Random::new(91387.0);
    let mut tufts = Vec::with_capacity(MEADOW_TUFTS);
    let mut origins = Vec::with_capacity(MEADOW_TUFTS * 2);
    let mut flowers = Vec::with_capacity(MEADOW_FLOWERS);
    let mut i = 0;
    while i < MEADOW_CANDIDATES && tufts.len() < MEADOW_TUFTS {
        let early = i < 1600;
        let x = if early {
            rng.range(-45.0, 45.0)
        } else {
            rng.range(-120.0, 120.0)
        };
        let z = if early {
            rng.range(-56.0, 56.0)
        } else {
            rng.range(-125.0, 110.0)
        };
        let candidate = i;
        i += 1;
        let inside = x.abs() < 59.0 && z.abs() < 59.0;
        if inside
            && (x.abs() < 10.5 || x.abs() > 46.0 || z.abs() < 7.5 || (z.abs() - 38.0).abs() < 5.5)
        {
            continue;
        }
        let river = creek_distance(x, z);
        if !inside && (river < 7.0 || (x.abs() < 65.0 && z.abs() < 65.0)) {
            continue;
        }
        if (x * 0.24).sin() * (z * 0.18).cos() + rng.next() < 0.1 {
            continue;
        }
        let height = if inside {
            0.0
        } else {
            valley_height_at(x, z, river)
        };
        let reed = !inside && river < 11.0;
        let size = if reed {
            rng.range(0.6, 1.05)
        } else {
            rng.range(0.24, 0.48)
        };
        let mut position = DVec3::new(x, height + 0.025, z);
        let rotation = quat_from_euler(0.0, rng.next() * TUFT_TURN, 0.0);
        tufts.push(Instance {
            matrix: compose(position, rotation, DVec3::new(size * 0.8, size, size * 0.8)),
            color: instance_color(TUFT_COLORS[candidate % 4]),
        });
        origins.extend([x as f32, z as f32]);
        if !reed && candidate % 3 == 0 && flowers.len() < MEADOW_FLOWERS {
            position.y += size * 0.85;
            let bloom = rng.range(0.07, 0.12);
            flowers.push(Instance {
                matrix: compose(position, rotation, DVec3::splat(bloom)),
                color: instance_color(FLOWER_COLORS[candidate % 4]),
            });
        }
    }
    let mut tuft_mesh = tuft_geometry();
    tuft_mesh.set_attribute(Attribute {
        name: WIND_ORIGIN,
        item_size: 2,
        data: origins,
    });
    let grass = Material {
        side: Side::Double,
        effect: Effect::Custom {
            name: MEADOW_SWAY,
            params: Vec::new(),
        },
        ..Material::standard(0xffffff, 0.0, 1.0)
    };
    let mut tuft_node = Node::mesh(Arc::new(tuft_mesh), Arc::new(grass));
    let mut flower_node = Node::mesh(
        Arc::new(octahedron_geometry(1.0, 0)),
        Arc::new(Material::standard(0xffffff, 0.0, 0.9)),
    );
    for (node, instances) in [(&mut tuft_node, tufts), (&mut flower_node, flowers)] {
        let drawable = node.drawable.as_mut().expect("meadow mesh");
        drawable.receive_shadow = true;
        drawable.instances = Some(instances);
    }
    group.children.push(tuft_node);
    group.children.push(flower_node);
    group
}
