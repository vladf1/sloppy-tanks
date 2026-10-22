//! Port of `concrete-surfaces.ts`: the weathered concrete shared by every
//! perimeter wall, footing and plinth, with UVs repeating every four metres.

use std::sync::{Arc, OnceLock};

use crate::geometry::{Mesh, rounded_box_geometry};
use crate::scene::{Material, Node, Side, TextureRef};

use super::model_primitives::{Cache, box_part, put, shadowed};

/// World metres per concrete tile.
const CONCRETE_TILE: f64 = 4.0;
const CONCRETE_RADIUS: f64 = 0.06;
pub const CONCRETE_TEXTURE: &str = "textures/walls/weathered-concrete.webp";

static GEOMETRIES: Cache<[u64; 3], Mesh> = Cache::new();

/// The shared concrete material: the photo is both albedo and bump. Walls cast
/// shadows from their sun-facing side (`shadowSide = FrontSide`): the default
/// back-face depth let the bias light a sliver of ground along each wall's
/// shaded foot.
pub fn concrete_material() -> Arc<Material> {
    static MATERIAL: OnceLock<Arc<Material>> = OnceLock::new();
    MATERIAL
        .get_or_init(|| {
            let texture = TextureRef {
                anisotropy: 4,
                ..TextureRef::file(CONCRETE_TEXTURE)
            };
            Arc::new(Material {
                map: Some(texture.clone()),
                bump_map: Some(texture),
                bump_scale: 0.035,
                shadow_side: Some(Side::Front),
                ..Material::standard(0xffffff, 0.0, 0.95)
            })
        })
        .clone()
}

/// `concreteWall(w, h, d)`: a rounded box with a world-sized projection that
/// covers long faces, narrow ends and top surfaces.
pub fn concrete_wall(w: f64, h: f64, d: f64) -> Node {
    let mesh = GEOMETRIES.get_or_insert([w, h, d].map(f64::to_bits), || {
        let mut geometry = rounded_box_geometry(w, h, d, 1, CONCRETE_RADIUS);
        for i in 0..geometry.positions.len() {
            let [x, y, z] = geometry.positions[i].map(f64::from);
            let [nx, ny, nz] = geometry.normals[i].map(|n| f64::from(n).abs());
            let u = if nx > ny && nx > nz { z } else { x };
            let v = if ny >= nx && ny >= nz { z } else { y };
            geometry.uvs[i] = [(u / CONCRETE_TILE) as f32, (v / CONCRETE_TILE) as f32];
        }
        geometry
    });
    shadowed(mesh, concrete_material())
}

static BARRIERS: Cache<[u64; 3], Mesh> = Cache::new();

/// The precast barrier's cross-section as (fraction of its half thickness,
/// fraction of its height): a plinth at full thickness, the safety-shape slope
/// and a narrower stem up to a chamfered top. Never wider than the cover's box.
const BARRIER_PROFILE: [(f64, f64); 5] = [
    (1.0, 0.0),
    (1.0, 0.22),
    (0.8, 0.48),
    (0.77, 0.96),
    (0.68, 1.0),
];
/// Height of the reflectors along a barrier's faces, as a fraction of its
/// height, and their paint.
const BARRIER_REFLECTOR_AT: f64 = 0.7;
const BARRIER_REFLECTOR: u32 = 0x3d86b8;

/// A precast concrete barrier filling the `w` × `d` footprint and `h` tall, its
/// base at y 0: the weathered concrete in a safety-shape profile along its long
/// side, flat-faced so the low sun draws its slope, with reflectors every 1.1 m
/// along both faces.
pub fn concrete_barrier(w: f64, h: f64, d: f64) -> Node {
    let along_x = w >= d;
    let (length, thickness) = if along_x { (w, d) } else { (d, w) };
    let mesh = BARRIERS.get_or_insert([length, h, thickness].map(f64::to_bits), || {
        barrier_mesh(length, h, thickness)
    });
    let mut barrier = Node::group("");
    let mut body = shadowed(mesh, concrete_material());
    if !along_x {
        body.set_rotation_euler(0.0, std::f64::consts::FRAC_PI_2, 0.0);
    }
    barrier.children.push(body);
    let half = thickness / 2.0;
    let face = barrier_half_width(BARRIER_REFLECTOR_AT) * half + 0.015;
    let marks = (length / 1.1).floor() as u32;
    for i in 0..marks {
        let along = -length / 2.0 + 0.6 + f64::from(i) * 1.1;
        for side in [-1.0, 1.0] {
            let (x, z) = if along_x {
                (along, side * face)
            } else {
                (side * face, along)
            };
            let (mw, md) = if along_x { (0.34, 0.03) } else { (0.03, 0.34) };
            put(
                &mut barrier,
                box_part(mw, 0.12, md, BARRIER_REFLECTOR, 0.0),
                x,
                h * BARRIER_REFLECTOR_AT,
                z,
            );
        }
    }
    barrier
}

/// The profile's half thickness (a fraction) at a fraction of the height.
fn barrier_half_width(at: f64) -> f64 {
    BARRIER_PROFILE
        .windows(2)
        .find(|pair| at <= pair[1].1)
        .map_or(BARRIER_PROFILE[BARRIER_PROFILE.len() - 1].0, |pair| {
            let t = (at - pair[0].1) / (pair[1].1 - pair[0].1).max(1e-9);
            pair[0].0 + (pair[1].0 - pair[0].0) * t
        })
}

/// The barrier body along x, flat-shaded, with the concrete's world-sized UVs.
fn barrier_mesh(length: f64, h: f64, thickness: f64) -> Mesh {
    let half = thickness / 2.0;
    let end = length / 2.0;
    // The closed outline round the section: up one face, over the top, down the
    // other.
    let mut outline: Vec<(f64, f64)> = BARRIER_PROFILE
        .iter()
        .map(|&(w, y)| (w * half, y * h))
        .collect();
    outline.extend(
        BARRIER_PROFILE
            .iter()
            .rev()
            .map(|&(w, y)| (-w * half, y * h)),
    );
    let mut positions = Vec::new();
    let mut triangle = |a: [f64; 3], b: [f64; 3], c: [f64; 3]| {
        positions.extend(a);
        positions.extend(b);
        positions.extend(c);
    };
    for i in 0..outline.len() {
        let (z0, y0) = outline[i];
        let (z1, y1) = outline[(i + 1) % outline.len()];
        triangle([-end, y0, z0], [end, y0, z0], [end, y1, z1]);
        triangle([-end, y0, z0], [end, y1, z1], [-end, y1, z1]);
    }
    // End caps: the section is convex, so a fan from its first corner.
    for i in 1..outline.len() - 1 {
        let ((za, ya), (zb, yb), (zc, yc)) = (outline[0], outline[i], outline[i + 1]);
        triangle([end, ya, za], [end, yc, zc], [end, yb, zb]);
        triangle([-end, ya, za], [-end, yb, zb], [-end, yc, zc]);
    }
    let mut mesh = Mesh::from_f64(&positions, &[], &[], None);
    mesh.compute_vertex_normals();
    mesh.uvs = mesh
        .positions
        .iter()
        .zip(&mesh.normals)
        .map(|(p, n)| {
            let [x, y, z] = p.map(f64::from);
            let [nx, ny, nz] = n.map(|v| f64::from(v).abs());
            let u = if nx > ny && nx > nz { z } else { x };
            let v = if ny >= nx && ny >= nz { z } else { y };
            [(u / CONCRETE_TILE) as f32, (v / CONCRETE_TILE) as f32]
        })
        .collect();
    mesh
}
