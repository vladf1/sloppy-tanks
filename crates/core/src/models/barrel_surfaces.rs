//! Port of `barrel-surfaces.ts`: the painted explosive drum.

use std::f64::consts::TAU;

use glam::DVec2;

use super::model_primitives::{Cache, shadowed};
use crate::geometry::Mesh;
use crate::scene::{Material, Node, TextureRef, Wrap};

/// The drum atlas (`scripts/generate-barrels.ts`): the painted wall on the left
/// three quarters, the lid on the right.
pub const PAINTED_DRUM_TEXTURE: &str = "textures/barrels/painted-drum.webp";

/// Drum radius and height (the cover's 0.6 m radius, 1.6 m tall), and the sides
/// of its lathe.
const RADIUS: f64 = 0.6;
const HEIGHT: f64 = 1.6;
const SIDES: u32 = 20;
/// The rolling hoops' height from the drum's centre and how far they stand out.
const HOOP_Y: f64 = 0.56;
const HOOP: f64 = 0.026;
/// How far the lid sits below the rolled chime.
pub(super) const LID_DROP: f64 = 0.035;
/// The wall's share of the atlas width.
const WALL_U: f64 = 0.75;

static DRUM_MESH: Cache<(), Mesh> = Cache::new("barrel_surfaces::DRUM_MESH");
static DRUM_MATERIAL: Cache<(), Material> = Cache::new("barrel_surfaces::DRUM_MATERIAL");

/// The drum's lathe profile from the bottom centre to the lid's centre, as
/// (radius, height from the drum's centre): rolled chimes top and bottom, two
/// swaged rolling hoops, and a lid recessed inside the top chime.
fn profile() -> Vec<DVec2> {
    let h = HEIGHT / 2.0;
    let hoop = |y: f64| {
        [
            DVec2::new(RADIUS, y - 0.05),
            DVec2::new(RADIUS + HOOP * 0.7, y - 0.025),
            DVec2::new(RADIUS + HOOP, y),
            DVec2::new(RADIUS + HOOP * 0.7, y + 0.025),
            DVec2::new(RADIUS, y + 0.05),
        ]
    };
    let mut points = vec![
        DVec2::new(0.0, -h + LID_DROP),
        DVec2::new(RADIUS - 0.04, -h + LID_DROP),
        DVec2::new(RADIUS - 0.035, -h),
        DVec2::new(RADIUS + 0.008, -h + 0.006),
        DVec2::new(RADIUS + 0.014, -h + 0.03),
        DVec2::new(RADIUS, -h + 0.055),
    ];
    points.extend(hoop(-HOOP_Y));
    points.extend(hoop(HOOP_Y));
    points.extend([
        DVec2::new(RADIUS, h - 0.055),
        DVec2::new(RADIUS + 0.014, h - 0.03),
        DVec2::new(RADIUS + 0.008, h - 0.006),
        DVec2::new(RADIUS - 0.035, h),
        DVec2::new(RADIUS - 0.04, h - LID_DROP),
        DVec2::new(0.0, h - LID_DROP),
    ]);
    points
}

/// The atlas coordinate of a lathe point: the wall wraps the left of the atlas
/// by angle and height; the ends map the lid disc on the right.
fn drum_uv(radius: f64, y: f64, angle: f64, end: bool) -> [f32; 2] {
    if end {
        let (s, c) = angle.sin_cos();
        let r = radius / RADIUS * 0.5;
        [
            (WALL_U + (0.5 + c * r) * (1.0 - WALL_U)) as f32,
            (0.25 + (0.5 + s * r) * 0.5) as f32,
        ]
    } else {
        [
            (angle / TAU * WALL_U) as f32,
            ((y + HEIGHT / 2.0) / HEIGHT) as f32,
        ]
    }
}

fn drum_mesh() -> Mesh {
    let points = profile();
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let d = b - a;
        // The profile runs up the outside, so its outward normal is (d.y, -d.x).
        let n = DVec2::new(d.y, -d.x).normalize();
        let end = d.x.abs() > d.y.abs() * 3.0 && (a.x < RADIUS - 0.03 || b.x < RADIUS - 0.03);
        for k in 0..SIDES {
            let angles = [k, k + 1].map(|s| f64::from(s) / f64::from(SIDES) * TAU);
            let vertex = |p: DVec2, angle: f64| {
                let (s, c) = angle.sin_cos();
                (
                    [p.x * c, p.y, p.x * s],
                    [n.x * c, n.y, n.x * s],
                    drum_uv(p.x, p.y, angle, end),
                )
            };
            let quad = [
                vertex(a, angles[0]),
                vertex(b, angles[0]),
                vertex(b, angles[1]),
                vertex(a, angles[1]),
            ];
            // Counter-clockwise seen from outside.
            for i in [0, 1, 2, 0, 2, 3] {
                let (p, normal, uv) = quad[i];
                positions.extend(p);
                normals.extend(normal);
                uvs.push(uv);
            }
        }
    }
    let mut mesh = Mesh::from_f64(&positions, &normals, &[], None);
    mesh.uvs = uvs;
    mesh
}

/// `explosiveBarrel()`: one shared lathed steel drum, centred on its middle,
/// whose wall and lid sample separate regions of the atlas.
pub fn explosive_barrel() -> Node {
    let mesh = DRUM_MESH.get_or_insert((), drum_mesh);
    let material = DRUM_MATERIAL.get_or_insert((), || Material {
        map: Some(TextureRef {
            wrap: Wrap::Clamp,
            anisotropy: 4,
            ..TextureRef::file(PAINTED_DRUM_TEXTURE)
        }),
        roughness: 0.62,
        metalness: 0.25,
        ..Material::default()
    });
    shadowed(mesh, material)
}
