//! Ports of `timber-layout.ts` (the members of a timber wall and where its hits
//! land) and `timber-model.ts` (a member's mesh with its chips, cracks and straps).
//!
//! The layout is gameplay data too: the simulation turns the same members into
//! physical debris. Shared with `sim::timber_layout`; de-duplicate at integration.

use std::f64::consts::PI;

use glam::{DVec2, DVec3};

use super::batching::batch;
use super::model_primitives::{box_part, material, put};
use super::prop_support::{Random, clamp, imul};
use crate::geometry::math::{js_round, quat_from_euler, to_int32};
use crate::geometry::{Shape, shape_geometry};
use crate::scene::Node;

/// Hit points of a timber wall (`TIMBER_HEALTH`).
pub const TIMBER_HEALTH: f64 = 80.0;
const BEAM_COUNT: usize = 4;
const POST_COLOR: u32 = 0x805336;
const STRAP_COLOR: u32 = 0x49423a;
const CHIP_COLOR: u32 = 0xc59b65;
const CHIP_CORE_COLOR: u32 = 0x805334;
const CRACK_LIP_COLOR: u32 = 0xb98a55;
const CRACK_COLOR: u32 = 0x503421;

/// Which ends of a wall join a neighbour (`TimberJoin`). Ends use the increasing
/// world X/Z axis, independent of the model's yaw; `post` is a lone corner post.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TimberJoin {
    pub open_min: bool,
    pub open_max: bool,
    pub post: bool,
}

/// An impact on a wall in its local (unrotated) frame, with the mark size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimberHit {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub size: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimberFace {
    Front,
    Back,
    Left,
    Right,
}

/// A scar on one face of a member, in that face's coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimberMark {
    pub x: f64,
    pub y: f64,
    pub face: TimberFace,
    pub size: f64,
    pub seed: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimberPartKind {
    Beam,
    Post,
}

/// One beam or post of a timber wall (`TimberPart`), in the cover's frame.
#[derive(Clone, Debug, PartialEq)]
pub struct TimberPart {
    pub kind: TimberPartKind,
    pub index: usize,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
    pub h: f64,
    pub d: f64,
    pub yaw: f64,
    pub lean: f64,
    pub color: u32,
    pub damage: u32,
    pub damage_seed: i32,
    pub marks: Vec<TimberMark>,
}

/// The wall fields `timberParts` reads (`Pick<Cover, ...>`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TimberWall {
    pub x: f64,
    pub z: f64,
    pub w: f64,
    pub d: f64,
    pub h: f64,
    pub color: u32,
    pub hits: Vec<TimberHit>,
    pub join: Option<TimberJoin>,
}

/// `timberDamageStage(hp, maxHp)`: 0 intact, then 1–3 as the wall weakens.
pub fn timber_damage_stage(hp: f64, max_hp: f64) -> u32 {
    if hp >= max_hp {
        0
    } else if hp > max_hp * 0.5 {
        1
    } else if hp > max_hp * 0.2 {
        2
    } else {
        3
    }
}

/// `timberParts(c, stage)`: the same members are used by the standing wall and its
/// physical debris.
pub fn timber_parts(c: &TimberWall, stage: u32) -> Vec<TimberPart> {
    let along = c.w > c.d;
    let length = c.w.max(c.d);
    let depth = c.w.min(c.d);
    let yaw = if along { 0.0 } else { PI / 2.0 };
    let post_width = 0.32f64.min(length * 0.12);
    // Separate physical members must start clear of the posts, even loosened.
    let end_clearance = 0.04 + c.h * 0.02;
    let join = c.join.unwrap_or_default();
    let open_negative = if along { join.open_min } else { join.open_max };
    let open_positive = if along { join.open_max } else { join.open_min };
    let inset = 0.18 + post_width / 2.0 + end_clearance;
    let beam_min = -length / 2.0 + if open_negative { 0.025 } else { inset };
    let beam_max = length / 2.0 - if open_positive { 0.025 } else { inset };
    let beam_center = (beam_min + beam_max) / 2.0;
    let pitch = (c.h - 0.12) / BEAM_COUNT as f64;
    let colors = [c.color, 0x94613e, 0xa66f46];
    // Cosmetic randomness is stable per wall and never consumes the combat RNG.
    let seed = imul(to_int32(js_round(c.x * 100.0)), 73_856_093)
        ^ imul(to_int32(js_round(c.z * 100.0)), 19_349_663);
    let mut parts = Vec::new();
    let beams = if join.post { 0 } else { BEAM_COUNT };
    for index in 0..beams {
        let damage = if index == 1 {
            stage
        } else {
            stage.saturating_sub(if index == 2 { 1 } else { 2 })
        };
        parts.push(TimberPart {
            kind: TimberPartKind::Beam,
            index,
            x: if along { beam_center } else { 0.0 },
            y: 0.06 + pitch * (index as f64 + 0.5),
            z: if along { 0.0 } else { -beam_center },
            w: beam_max - beam_min,
            h: pitch - 0.025,
            d: 0.66f64.min(depth * 0.75),
            yaw,
            lean: if index == BEAM_COUNT - 1 {
                f64::from(stage) * 0.006
            } else {
                0.0
            },
            color: colors[index % colors.len()],
            damage,
            marks: Vec::new(),
            damage_seed: seed ^ imul(index as i32 + 1, 83_492_791),
        });
    }
    let sides: &[i32] = if join.post { &[0] } else { &[-1, 1] };
    for &side in sides {
        if (side < 0 && open_negative) || (side > 0 && open_positive) {
            continue;
        }
        let offset = f64::from(side) * (length / 2.0 - 0.18);
        parts.push(TimberPart {
            kind: TimberPartKind::Post,
            index: if side == 0 {
                0
            } else if side < 0 {
                BEAM_COUNT
            } else {
                BEAM_COUNT + 1
            },
            x: if along { offset } else { 0.0 },
            y: c.h / 2.0,
            z: if along { 0.0 } else { -offset },
            w: if join.post { length } else { post_width },
            h: c.h,
            d: depth,
            yaw,
            lean: if stage == 3 {
                f64::from(side) * 0.028
            } else {
                0.0
            },
            color: POST_COLOR,
            damage: stage,
            marks: Vec::new(),
            damage_seed: seed ^ imul(side + 7, 83_492_791),
        });
    }
    // Attach each hit to its nearest actual beam/post and only the struck face.
    for (hit_index, hit) in c.hits.iter().enumerate() {
        let mut nearest: Option<(usize, f64, f64, f64, f64)> = None;
        for (i, part) in parts.iter().enumerate() {
            let dx = hit.x - part.x;
            let dz = hit.z - part.z;
            let x = dx * yaw.cos() - dz * yaw.sin();
            let z = dx * yaw.sin() + dz * yaw.cos();
            let y = hit.y - part.y;
            let distance = (x.abs() - part.w / 2.0).max(0.0).powi(2)
                + (y.abs() - part.h / 2.0).max(0.0).powi(2)
                + (z.abs() - part.d / 2.0).max(0.0).powi(2);
            // A stable sort by distance keeps the first of equally near members.
            if nearest.is_none_or(|(_, _, _, _, best)| distance < best) {
                nearest = Some((i, x, y, z, distance));
            }
        }
        let Some((i, x, y, z, _)) = nearest else {
            continue;
        };
        let part = &mut parts[i];
        let end = x.abs() > part.w / 2.0 - 0.015;
        part.marks.push(TimberMark {
            x: if end { z } else { x },
            y,
            face: match (end, x < 0.0, z < 0.0) {
                (true, true, _) => TimberFace::Left,
                (true, false, _) => TimberFace::Right,
                (false, _, true) => TimberFace::Back,
                (false, _, false) => TimberFace::Front,
            },
            size: hit.size,
            seed: part.damage_seed ^ imul(hit_index as i32 + 1, 0x45d_9f3b),
        });
    }
    // Marks attach in the original piece coordinates, then move with a loosened beam.
    let shift = if stage >= 2 && !open_negative && !open_positive {
        0.02 * f64::from(stage - 1)
    } else {
        0.0
    };
    if !join.post {
        let top = &mut parts[BEAM_COUNT - 1];
        top.x += if along { shift } else { 0.0 };
        top.z -= if along { 0.0 } else { shift };
    }
    parts
}

/// `timberPartModel(p)`: a batched member with its chips and cracks, reused for
/// standing walls and detached pieces.
pub fn timber_part_model(p: &TimberPart) -> Node {
    let mut group = Node::default();
    put(
        &mut group,
        box_part(p.w, p.h, p.d, p.color, 0.0),
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
        // A scar clipped flat against an edge triangulates to nothing. Three merged
        // its empty attributes harmlessly; `merge_geometries` would reject the batch.
        if mesh.triangle_count() == 0 {
            return;
        }
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
