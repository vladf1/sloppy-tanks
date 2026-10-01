//! Port of `timber-model.ts`: a timber wall member's mesh with its chips, cracks
//! and straps. The members and where hits land are gameplay data
//! ([`crate::sim::timber_layout`]): the simulation turns the same members into
//! physical debris.
//!
//! Members are sawn boards: their long faces sample a plank row of the timber
//! atlas at true scale with the grain along the member, their ends an end-grain
//! cell, and the member's paint only tints the wood.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::{DVec2, DVec3};

use super::batching::batch;
use super::model_primitives::{Cache, box_part, material, put, shadowed};
use crate::geometry::math::{hex_to_linear, linear_to_hex, quat_from_euler};
use crate::geometry::{Mesh, Shape, shape_geometry};
use crate::scene::{Color, Material, Node, TextureRef};
use crate::sim::math::{Random, clamp};
use crate::sim::timber_layout::{TimberFace, TimberMark, TimberPart, TimberPartKind};

const STRAP_COLOR: u32 = 0x49423a;
const CHIP_COLOR: u32 = 0xc59b65;
const CHIP_CORE_COLOR: u32 = 0x805334;
const CRACK_LIP_COLOR: u32 = 0xb98a55;
const CRACK_COLOR: u32 = 0x503421;

/// The timber atlas (`scripts/generate-wood-texture.ts`): [`GRAIN_ROWS`] rows of
/// flat-sawn plank faces, each tileable along the grain, over a row of
/// [`END_CELLS`] end-grain cells.
pub const TIMBER_TEXTURE: &str = "textures/wood/timber.webp";
const GRAIN_ROWS: usize = 3;
const END_CELLS: usize = 4;
/// Metres of board the atlas width spans along the grain, and one row across it.
const GRAIN_SPAN: f64 = 2.4;
const ROW_SPAN: f64 = 0.6;
/// The share of a row or cell left clear at its edges, so mipmaps stay inside it.
const ATLAS_INSET: f64 = 0.02;
/// The average sRGB colour of the atlas's plank faces. A member's tint is its
/// paint divided by it, so the textured wood averages the paint it replaces:
/// posts stay darker than beams, and beams keep their alternating tones.
const PLANK_AVERAGE: u32 = 0xcaa378;

static WOOD: Cache<u32, Material> = Cache::new();

/// The textured wood of a member painted `color`; batching bakes the tint into
/// vertex colours, so every member shares one material and draw setup.
fn wood(color: u32) -> Arc<Material> {
    WOOD.get_or_insert(color, || {
        let map = TextureRef {
            anisotropy: 8,
            ..TextureRef::file(TIMBER_TEXTURE)
        };
        let (paint, plank) = (hex_to_linear(color), hex_to_linear(PLANK_AVERAGE));
        let tint = [0, 1, 2].map(|i| (paint[i] / plank[i]).min(1.0));
        Material {
            map: Some(map.clone()),
            bump_map: Some(map),
            bump_scale: 0.03,
            color: Color(linear_to_hex(tint)),
            roughness: 0.82,
            metalness: 0.0,
            ..Material::default()
        }
    })
}

/// A member's box with the grain along the longer of its width (beams) and height
/// (posts). Long faces map one plank row at true scale, starting at a seeded point
/// along the board; the two ends map one end-grain cell. `seed` picks the row, the
/// start, the cell and a mirror, so neighbouring members never repeat.
fn member_mesh(size: DVec3, seed: i32) -> Mesh {
    let mut rng = Random::new(f64::from(seed));
    let row = (rng.next() * GRAIN_ROWS as f64).floor() as usize;
    let start = rng.next();
    let cell = (rng.next() * END_CELLS as f64).floor() as usize;
    let mirror = if rng.next() < 0.5 { -1.0 } else { 1.0 };
    let grain = if size.x >= size.y { 0 } else { 1 };
    let half = size / 2.0;
    let rows = (GRAIN_ROWS + 1) as f64;
    // The image's top row is at v = 1 (flipped like Three's textures).
    let row_center = 1.0 - (row as f64 + 0.5) / rows;
    let mut positions = Vec::with_capacity(72);
    let mut normals = Vec::with_capacity(72);
    let mut uvs = Vec::with_capacity(48);
    let mut indices = Vec::with_capacity(36);
    for axis in 0..3 {
        let (a, b) = ((axis + 1) % 3, (axis + 2) % 3);
        for sign in [-1.0, 1.0] {
            let base = (positions.len() / 3) as u32;
            for (sa, sb) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let mut p = DVec3::ZERO;
                p[axis] = sign * half[axis];
                p[a] = sa * half[a];
                p[b] = sb * half[b];
                let uv = if axis == grain {
                    // End grain, stretched over the end face.
                    let other =
                        |i: usize| (p[i] / size[i] + 0.5) * (1.0 - 2.0 * ATLAS_INSET) + ATLAS_INSET;
                    let (u, v) = if grain == 0 {
                        (other(2), other(1))
                    } else {
                        (other(0), other(2))
                    };
                    [(cell as f64 + u) / END_CELLS as f64, v / rows]
                } else {
                    let across = if a == grain { b } else { a };
                    let fit = (ROW_SPAN * (1.0 - 2.0 * ATLAS_INSET) / size[across]).min(1.0);
                    [
                        start + p[grain] / GRAIN_SPAN,
                        row_center + mirror * p[across] / ROW_SPAN * fit / rows,
                    ]
                };
                positions.extend_from_slice(&[p.x, p.y, p.z]);
                let mut normal = [0.0; 3];
                normal[axis] = sign;
                normals.extend_from_slice(&normal);
                uvs.extend_from_slice(&uv);
            }
            // (a, b, axis) is right-handed, so the corners run counter-clockwise
            // seen from outside on the positive face.
            if sign > 0.0 {
                indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            } else {
                indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
            }
        }
    }
    Mesh::from_f64(&positions, &normals, &uvs, Some(indices))
}

/// `timberPartModel(p)`: a batched member with its chips and cracks, reused for
/// standing walls and detached pieces.
pub fn timber_part_model(p: &TimberPart) -> Node {
    let mut group = Node::default();
    put(
        &mut group,
        shadowed(
            Arc::new(member_mesh(DVec3::new(p.w, p.h, p.d), p.damage_seed)),
            wood(p.color),
        ),
        0.0,
        0.0,
        0.0,
    );
    for (mark_index, mark) in p.marks.iter().enumerate() {
        add_mark(&mut group, p, mark_index, mark);
    }
    if p.kind == TimberPartKind::Post {
        for fraction in [-0.28, 0.28] {
            let mut strap = box_part(p.w + 0.024, 0.085, p.d + 0.025, STRAP_COLOR, 0.0);
            let bent = p.damage == 3 && fraction > 0.0;
            if bent {
                strap.set_rotation_euler(0.0, 0.0, 0.13);
            }
            put(
                &mut group,
                strap,
                0.0,
                p.h * fraction,
                if bent { 0.025 } else { 0.0 },
            );
        }
    }
    batch(&mut group);
    group
}

/// One impact scar: a two-tone chip and a forked crack, clipped to the face.
fn add_mark(group: &mut Node, p: &TimberPart, mark_index: usize, mark: &TimberMark) {
    let mut rng = Random::new(f64::from(mark.seed));
    let end = matches!(mark.face, TimberFace::Left | TimberFace::Right);
    let sign = if matches!(mark.face, TimberFace::Left | TimberFace::Back) {
        -1.0
    } else {
        1.0
    };
    let span = if end { p.d } else { p.w };
    let x = clamp(mark.x, -span / 2.0 + 0.01, span / 2.0 - 0.01);
    // Keep the scar close to impact while avoiding a half-mark clipped along a seam.
    let margin = (p.h * 0.3).min(0.13 * mark.size);
    let y = clamp(mark.y, -p.h / 2.0 + margin, p.h / 2.0 - margin);
    let mut plane = |points: &[[f64; 2]], color: u32, layer: f64| {
        // Clip scars at the piece edges; no cracks floating beyond the wood.
        let outline: Vec<DVec2> = points
            .iter()
            .map(|point| {
                DVec2::new(
                    (if end { -sign } else { sign })
                        * clamp(x + point[0], -span / 2.0 + 0.005, span / 2.0 - 0.005),
                    clamp(y + point[1], -p.h / 2.0 + 0.005, p.h / 2.0 - 0.005),
                )
            })
            .collect();
        let mut mesh = shape_geometry(&[Shape::from_points(&outline)], 12);
        mesh.rotate_y(if end {
            (sign * PI) / 2.0
        } else if sign < 0.0 {
            PI
        } else {
            0.0
        });
        let offset = 0.003 + mark_index as f64 * 0.001 + layer * 0.001;
        put(
            group,
            Node::mesh(std::sync::Arc::new(mesh), material(color, 0.05, 0.65)),
            if end {
                sign * (p.w / 2.0 + offset)
            } else {
                0.0
            },
            0.0,
            if end {
                0.0
            } else {
                sign * (p.d / 2.0 + offset)
            },
        );
    };
    let width = 0.19 * mark.size * rng.range(0.85, 1.15);
    let height = 0.075 * mark.size * rng.range(0.85, 1.15);
    let chip: Vec<[f64; 2]> = (0..10)
        .map(|i| {
            let angle = (f64::from(i) * PI) / 5.0;
            let radius = rng.range(0.65, 1.0);
            [angle.cos() * width * radius, angle.sin() * height * radius]
        })
        .collect();
    plane(&chip, CHIP_COLOR, 0.0);
    let core: Vec<[f64; 2]> = chip
        .iter()
        .map(|&[u, v]| [u * 0.65 + 0.02, v * 0.65 + 0.012])
        .collect();
    plane(&core, CHIP_CORE_COLOR, 1.0);
    // Every impact owns its random path and fixed size, independent of later stages.
    let reach = 0.38 * mark.size * rng.range(0.85, 1.15);
    let slope = rng.range(-0.12, 0.12);
    let count = 5 + (rng.next() * 3.0).floor() as usize;
    let points: Vec<[f64; 2]> = (0..=count)
        .map(|i| {
            let u = -reach + (2.0 * reach * i as f64) / count as f64;
            [u, u * slope + rng.range(-0.045, 0.045)]
        })
        .collect();
    let mut split = |path: &[[f64; 2]], thickness: f64| {
        for lip in [true, false] {
            let mut outline = Vec::with_capacity(path.len() * 2);
            for side in [1.0, -1.0] {
                for j in 0..path.len() {
                    let i = if side > 0.0 { j } else { path.len() - 1 - j };
                    let taper = ((PI * i as f64) / (path.len() - 1) as f64).sin();
                    outline.push([
                        path[i][0],
                        path[i][1] + side * (thickness + if lip { 0.004 } else { 0.0 }) * taper
                            - if lip { 0.006 } else { 0.0 },
                    ]);
                }
            }
            plane(
                &outline,
                if lip { CRACK_LIP_COLOR } else { CRACK_COLOR },
                if lip { 2.0 } else { 3.0 },
            );
        }
    };
    split(&points, 0.012 * mark.size);
    let root = points[2 + (rng.next() * (count - 2) as f64).floor() as usize];
    let direction = if rng.next() < 0.5 { -1.0 } else { 1.0 };
    let branch = [
        root,
        [root[0] + reach * 0.2, root[1] + direction * 0.08],
        [
            root[0] + reach * rng.range(0.35, 0.6),
            root[1] + direction * 0.16 * mark.size,
        ],
    ];
    split(&branch, 0.008 * mark.size);
}

/// `addTimberParts(group, parts)`: every member's batched meshes, placed in the
/// cover's frame. Beams are lengthened to hide their debris clearance inside the
/// posts while the wall stands.
pub fn add_timber_parts(group: &mut Node, parts: &[TimberPart]) {
    let wall_height = parts
        .iter()
        .map(|part| part.h)
        .fold(f64::NEG_INFINITY, f64::max);
    let end_overlap = 0.04 + wall_height * 0.02 + 0.01;
    for part in parts {
        let assembly = if part.kind == TimberPartKind::Beam {
            timber_part_model(&TimberPart {
                w: part.w + end_overlap * 2.0,
                ..part.clone()
            })
        } else {
            timber_part_model(part)
        };
        // Batched meshes have identity transforms, so each takes the assembly's.
        let rotation = quat_from_euler(0.0, part.yaw, part.lean);
        for mut child in assembly.children {
            child.position = DVec3::new(part.x, part.y, part.z);
            child.rotation = rotation;
            group.children.push(child);
        }
    }
}
