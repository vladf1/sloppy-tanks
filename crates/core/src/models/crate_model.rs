//! Cargo crates built from sawn boards like the timber walls: sheathing planks
//! with dark seams between framing battens and a diagonal brace on each wall, a
//! boarded lid inside a frame, a floor on skids and two steel straps, around a
//! dark hold.
//!
//! Each recorded shell hit breaches the face it struck: the planks, battens and
//! brace inside the breach break away, leaving stepped, grain-split ends with
//! splinters bent into the hole, and the boards beside it crack along the grain.
//! Below 35% health a strap snaps and curls, a lid board springs up where it held,
//! and the most damaged wall's brace gives way. A broken crate's sections
//! ([`crate_piece_model`]) are drawn from the same boards with torn edges.

use std::f64::consts::{FRAC_PI_2, PI};
use std::sync::Arc;

use glam::{DMat3, DQuat, DVec2, DVec3};

use super::batching::batch;
use super::model_primitives::shadowed;
use super::timber_model::{timber_member, timber_member_face, wood as timber_wood};
use crate::geometry::math::scale_hex_color;
use crate::geometry::{Mesh, Shape, box_geometry, shape_geometry};
use crate::scene::Node;
use crate::sim::crate_layout::{
    CRATE_BASE, CRATE_BATTEN, CRATE_BATTEN_WIDTH, CRATE_BOARD_PITCH, CRATE_LID, CRATE_PLANK,
    CRATE_SKID_WIDTH, CratePiece,
};
use crate::sim::math::{Random, clamp, js_round, to_int32};
use crate::sim::timber_layout::TimberHit;

/// `Pick<Cover, "x" | "z" | "w" | "d" | "h" | "color">`: crates also seed their
/// cosmetic randomness from their position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrateShape {
    pub x: f64,
    pub z: f64,
    pub w: f64,
    pub d: f64,
    pub h: f64,
    pub color: u32,
}

/// Dark gap between neighbouring boards, where the interior shows.
const SEAM: f64 = 0.012;
const FLOOR_THICKNESS: f64 = 0.04;
const BRACE_WIDTH: f64 = 0.13;
const STRAP_WIDTH: f64 = 0.07;
const STRAP_THICKNESS: f64 = 0.012;
/// Straps run over the lid this far either side of the centre, as a share of width.
const STRAP_SPREAD: f64 = 0.3;

const BATTEN_COLOR: u32 = 0xd2b07c;
const SKID_COLOR: u32 = 0x6e5139;
const STRAP_COLOR: u32 = 0x3b4044;
/// Fresh wood where boards tore, and the darker fibres under it.
/// The inside of the walls, out of the sun.
const HOLD_COLOR: u32 = 0x2b1f14;
const RAW_WOOD: u32 = 0xe4c899;
const TORN_WOOD: u32 = 0xc9a26e;
const CRACK_LIP_COLOR: u32 = 0xb98a55;
const CRACK_COLOR: u32 = 0x2b1d12;
/// Splinters, torn fibres, cracks, the hold and the straps sample this one texel
/// of the timber atlas's first plank row, so they share the boards' material and a
/// whole crate batches into a single draw.
const SCRAP_TEXEL: [f32; 2] = [0.3, 0.875];

/// Decals sit this far over the board they mark, enough to win the depth test.
const DECAL_LIFT: f64 = 0.003;
/// Splinters on the ground rest this far up, over the floors and the harbor paving.
const LITTER_LIFT: f64 = 0.035;

/// One crate face (or a loose section's own frame): the plane's origin, the
/// direction its boards run (`u`), up the face (`v`) and out of it (`n`).
#[derive(Clone, Copy, Debug)]
struct Face {
    origin: DVec3,
    u: DVec3,
    v: DVec3,
    n: DVec3,
    /// Half its length along `u`.
    half: f64,
}

impl Face {
    fn at(&self, u: f64, v: f64, out: f64) -> DVec3 {
        self.origin + self.u * u + self.v * v + self.n * out
    }

    /// The direction a board whose grain runs at `angle` from `u` lies along.
    fn along(&self, angle: f64) -> DVec3 {
        self.u * angle.cos() + self.v * angle.sin()
    }

    /// Rotation of a board (grain along its x, face out along its z) lying at
    /// `angle` from `u`.
    fn turn(&self, angle: f64) -> DQuat {
        let along = self.along(angle);
        DQuat::from_mat3(&DMat3::from_cols(along, self.n.cross(along), self.n))
    }
}

/// A board lying on a face: its centre in face coordinates, grain angle from `u`,
/// size, and how far its centre stands out of the face (sheathing is negative).
#[derive(Clone, Copy, Debug)]
struct Board {
    u: f64,
    v: f64,
    angle: f64,
    length: f64,
    width: f64,
    thickness: f64,
    out: f64,
    color: u32,
    seed: i32,
}

/// A shell's hole in a face: centre, half-extent along the boards and across them.
#[derive(Clone, Copy, Debug)]
struct Breach {
    u: f64,
    v: f64,
    along: f64,
    across: f64,
    seed: i32,
}

/// Part of a board that springs out of its face: everything on `side` (+1 or -1
/// along the board) of `at` turns about the board's width axis through `at` until
/// its far end stands `angle` radians out of the face.
#[derive(Clone, Copy, Debug)]
struct Bend {
    at: f64,
    side: f64,
    angle: f64,
}

/// What breaks a board: the face's breaches, extra torn spans and cracks along it
/// (board coordinates: a span, or a centre, half-length and seed) and an optional
/// bend. `skin` marks a board over the hold whose outer face alone can show, so
/// its unbroken lengths are drawn as that face; `coarse` a loose, tumbling one,
/// whose breaks carry only their stepped strip and a splinter or two.
#[derive(Default)]
struct Damage<'a> {
    breaches: &'a [Breach],
    gaps: Vec<(f64, f64)>,
    cracks: Vec<(f64, f64, i32)>,
    bend: Option<Bend>,
    skin: bool,
    coarse: bool,
}

fn hash(seed: i32, salt: i32) -> i32 {
    seed ^ salt.wrapping_add(1).wrapping_mul(0x45d9f3b)
}

fn random(seed: i32, salt: i32) -> Random {
    Random::new(f64::from(hash(seed, salt)))
}

/// Shades a crate's boards vary between. Each paint is a material the shared cache
/// keeps for good, so the shades are few rather than drawn continuously.
const BOARD_TONES: [f64; 4] = [0.87, 0.93, 0.99, 1.05];

fn board_tone(color: u32, rng: &mut Random) -> u32 {
    let tone = BOARD_TONES[(rng.next() * BOARD_TONES.len() as f64) as usize];
    scale_hex_color(color, tone)
}

/// A crate's cosmetic seed from its position, never the combat RNG.
fn crate_seed(c: &CrateShape) -> i32 {
    to_int32(js_round(c.x * 100.0)).wrapping_mul(73_856_093)
        ^ to_int32(js_round(c.z * 100.0)).wrapping_mul(19_349_663)
}

/// A scrap of `mesh` painted `color` over the boards' wood (see [`SCRAP_TEXEL`]).
fn scrap(mut mesh: Mesh, color: u32) -> Node {
    mesh.uvs.fill(SCRAP_TEXEL);
    shadowed(Arc::new(mesh), timber_wood(color))
}

/// A splinter: a thin three-sided spike from its base at the origin to its tip
/// along +x, in four triangles.
fn spike(length: f64, width: f64, thickness: f64) -> Mesh {
    let base = [
        DVec3::new(0.0, -width / 2.0, -thickness / 2.0),
        DVec3::new(0.0, width / 2.0, -thickness / 2.0),
        DVec3::new(0.0, 0.0, thickness / 2.0),
    ];
    let tip = DVec3::new(length, 0.0, 0.0);
    let center = (base[0] + base[1] + base[2] + tip) / 4.0;
    let (mut positions, mut normals) = (Vec::with_capacity(36), Vec::with_capacity(36));
    for [a, b, c] in [
        [base[0], base[1], base[2]],
        [base[0], base[1], tip],
        [base[1], base[2], tip],
        [base[2], base[0], tip],
    ] {
        let mut normal = (b - a).cross(c - a).normalize();
        // Wind every face outwards.
        let (b, c) = if normal.dot((a + b + c) / 3.0 - center) < 0.0 {
            normal = -normal;
            (c, b)
        } else {
            (b, c)
        };
        for p in [a, b, c] {
            positions.extend_from_slice(&[p.x, p.y, p.z]);
            normals.extend_from_slice(&[normal.x, normal.y, normal.z]);
        }
    }
    let uvs = vec![0.0; positions.len() / 3 * 2];
    Mesh::from_f64(&positions, &normals, &uvs, None)
}

/// A flat chip lying on the ground: its upper face only, in two triangles.
fn flake(length: f64, width: f64) -> Mesh {
    let (x, z) = (length / 2.0, width / 2.0);
    let positions = [-x, 0.0, -z, x, 0.0, -z, x, 0.0, z, -x, 0.0, z];
    let normals = [0.0, 1.0, 0.0].repeat(4);
    Mesh::from_f64(
        &positions,
        &normals,
        &[0.0; 8],
        Some(vec![0, 2, 1, 0, 3, 2]),
    )
}

/// Lay `board` on `face`, broken by `damage`: the spans breaches and gaps remove,
/// stepped and splintered ends where it broke, cracks along the grain beside
/// breaches, and its far part bent out of the face.
fn lay_board(group: &mut Node, face: &Face, board: &Board, damage: &Damage) {
    let half = board.length / 2.0;
    let dir = DVec2::new(board.angle.cos(), board.angle.sin());
    let perp = DVec2::new(-dir.y, dir.x);
    let mut removed: Vec<(f64, f64)> = damage.gaps.clone();
    let mut cracks = damage.cracks.clone();
    for breach in damage.breaches {
        let offset = DVec2::new(breach.u - board.u, breach.v - board.v);
        let (t, s) = (offset.dot(dir), offset.dot(perp));
        // The ellipse's reach along the board and across it.
        let reach =
            |d: DVec2| 1.0 / ((d.x / breach.along).powi(2) + (d.y / breach.across).powi(2)).sqrt();
        let (along, across) = (reach(dir), reach(perp));
        let near = (s.abs() - board.width / 2.0).max(0.0);
        let mut rng = random(breach.seed, board.seed);
        if near < across {
            let gap = along * (1.0 - (near / across).powi(2)).sqrt();
            if gap > 0.035 {
                removed.push((
                    t - gap * rng.range(0.7, 1.15),
                    t + gap * rng.range(0.7, 1.15),
                ));
                continue;
            }
        }
        if near < across * 1.9 {
            cracks.push((
                t,
                along * rng.range(0.9, 1.6),
                hash(breach.seed, board.seed),
            ));
        }
    }
    removed.sort_by(|a, b| a.0.total_cmp(&b.0));
    // Keep what the removed spans leave; scraps too short to hold a nail fall out.
    let mut kept = Vec::new();
    let mut start = -half;
    for &(from, to) in &removed {
        if from > start {
            kept.push((start, from.min(half)));
        }
        start = start.max(to);
    }
    if start < half {
        kept.push((start, half));
    }
    kept.retain(|&(a, b)| b - a > 0.08);
    // A bend splits the board where it springs; that end breaks too.
    if let Some(bend) = damage.bend {
        let mut split = Vec::with_capacity(kept.len() + 1);
        for (a, b) in kept {
            if a < bend.at - 0.04 && b > bend.at + 0.04 {
                split.push((a, bend.at - 0.012));
                split.push((bend.at + 0.012, b));
            } else {
                split.push((a, b));
            }
        }
        kept = split;
    }
    let center = face.at(board.u, board.v, board.out);
    let along = face.along(board.angle);
    let across = face.n.cross(along);
    let rotation = face.turn(board.angle);
    for (index, &(a, b)) in kept.iter().enumerate() {
        let mut rng = random(board.seed, index as i32 + 11);
        // The bent part turns about the board's width axis through the bend.
        let transform = damage
            .bend
            .filter(|bend| ((a + b) / 2.0 > bend.at) == (bend.side > 0.0))
            .map(|bend| {
                let pivot = center + along * bend.at;
                (
                    pivot,
                    DQuat::from_axis_angle(across, -bend.side * bend.angle),
                )
            });
        let place = |group: &mut Node, mut node: Node, t: f64, s: f64, z: f64, local: DQuat| {
            let mut position = center + along * t + across * s + face.n * z;
            let mut turn = rotation * local;
            if let Some((pivot, bent)) = transform {
                position = pivot + bent * (position - pivot);
                turn = bent * turn;
            }
            node.position = position;
            node.rotation = turn;
            group.children.push(node);
        };
        let broken = [a > -half + 1e-6, b < half - 1e-6];
        // A broken end splits along the grain: one strip of the board reaches past
        // the rest, so the break is stepped rather than sawn.
        let mut ends = [a, b];
        let mut strips = Vec::new();
        for (end, sign) in [(0usize, 1.0), (1, -1.0)] {
            if !broken[end] {
                continue;
            }
            let step = rng.range(0.05, 0.16).min((b - a) * 0.3);
            let share = rng.range(0.35, 0.6);
            let edge = if rng.next() < 0.5 { -1.0 } else { 1.0 };
            let tip = ends[end];
            ends[end] = tip + sign * step;
            strips.push((tip, ends[end], share, edge));
        }
        let length = ends[1] - ends[0];
        let whole = damage.skin && strips.is_empty() && transform.is_none();
        let member = if whole {
            timber_member_face
        } else {
            timber_member
        };
        place(
            group,
            member(
                DVec3::new(length, board.width, board.thickness),
                hash(board.seed, index as i32),
                board.color,
            ),
            (ends[0] + ends[1]) / 2.0,
            0.0,
            0.0,
            DQuat::IDENTITY,
        );
        for (strip, &(tip, base, share, edge)) in strips.iter().enumerate() {
            let width = board.width * share;
            place(
                group,
                timber_member(
                    DVec3::new((base - tip).abs(), width, board.thickness),
                    hash(board.seed, index as i32 + 50),
                    board.color,
                ),
                (tip + base) / 2.0,
                edge * (board.width - width) / 2.0,
                0.0,
                DQuat::IDENTITY,
            );
            // Splinters: from the strip's tip and the main board's end, mostly bent
            // into the hole, the odd one peeled outwards.
            let outward = if tip < base { -1.0 } else { 1.0 };
            let count = if damage.coarse {
                1 + (rng.next() * 2.0) as usize
            } else {
                2 + (rng.next() * 3.0) as usize
            };
            for i in 0..count {
                let on_strip = i % 2 == 0;
                let (from, low, high) = if on_strip {
                    (
                        tip,
                        edge * (board.width / 2.0 - width),
                        edge * board.width / 2.0,
                    )
                } else {
                    (
                        base,
                        -edge * board.width / 2.0,
                        edge * (board.width / 2.0 - width),
                    )
                };
                let s = rng.range(low.min(high) + 0.008, low.max(high) - 0.008);
                let length = rng.range(0.06, 0.2);
                let bend = if rng.next() < 0.25 {
                    -rng.range(0.1, 0.4)
                } else {
                    rng.range(0.2, 0.95)
                };
                let yaw = rng.range(-0.3, 0.3);
                let local = DQuat::from_rotation_z(if outward > 0.0 { yaw } else { PI + yaw })
                    * DQuat::from_rotation_y(bend);
                let color = if rng.next() < 0.6 {
                    RAW_WOOD
                } else {
                    TORN_WOOD
                };
                let splinter = spike(length, rng.range(0.015, 0.04), rng.range(0.01, 0.018));
                place(group, scrap(splinter, color), from, s, 0.0, local);
            }
            if damage.coarse {
                continue;
            }
            // Torn fibres on the face just behind the break.
            let depth = rng.range(0.03, 0.09);
            let points: Vec<DVec2> = (0..=6)
                .map(|i| {
                    let across = -board.width / 2.0 + board.width * f64::from(i) / 6.0;
                    DVec2::new(base - outward * depth * rng.range(0.3, 1.0), across * 0.96)
                })
                .chain([
                    DVec2::new(base + outward * 0.002, board.width * 0.48),
                    DVec2::new(base + outward * 0.002, -board.width * 0.48),
                ])
                .collect();
            let fibres = scrap(shape_geometry(&[Shape::from_points(&points)]), TORN_WOOD);
            place(
                group,
                fibres,
                0.0,
                0.0,
                board.thickness / 2.0 + DECAL_LIFT,
                DQuat::IDENTITY,
            );
            // Wood splits on along the grain from a break.
            if rng.next() < 0.65 {
                let reach = rng.range(0.12, 0.35).min(length * 0.7);
                let s = rng.range(-0.3, 0.3) * board.width;
                let seed = hash(board.seed, 200 + 2 * index as i32 + strip as i32);
                for (node, lift) in crack_decals(base - outward * reach / 2.0, s, reach / 2.0, seed)
                {
                    place(
                        group,
                        node,
                        0.0,
                        0.0,
                        board.thickness / 2.0 + lift,
                        DQuat::IDENTITY,
                    );
                }
            }
        }
        for &(t, reach, seed) in &cracks {
            if t > ends[0] + 0.05 && t < ends[1] - 0.05 {
                let reach = reach.min((t - ends[0]).min(ends[1] - t));
                let s = random(seed, 3).range(-0.25, 0.25) * board.width;
                for (node, lift) in crack_decals(t, s, reach, seed) {
                    place(
                        group,
                        node,
                        0.0,
                        0.0,
                        board.thickness / 2.0 + lift,
                        DQuat::IDENTITY,
                    );
                }
            }
        }
    }
}

/// A crack along the grain centred at `t`, `s` across the board: a light lip
/// around a dark split, tapering at both ends, in board coordinates, each with
/// how far over the board it lies.
fn crack_decals(t: f64, s: f64, reach: f64, seed: i32) -> Vec<(Node, f64)> {
    let mut rng = random(seed, 7);
    let count = 4;
    let path: Vec<DVec2> = (0..=count)
        .map(|i| {
            let x = t - reach + 2.0 * reach * f64::from(i) / f64::from(count);
            DVec2::new(x, s + rng.range(-0.012, 0.012))
        })
        .collect();
    [
        (0.009, CRACK_LIP_COLOR, DECAL_LIFT),
        (0.004, CRACK_COLOR, DECAL_LIFT + 0.002),
    ]
    .into_iter()
    .map(|(thickness, color, lift)| {
        let mut outline = Vec::with_capacity(path.len() * 2);
        for side in [1.0, -1.0] {
            for j in 0..path.len() {
                let i = if side > 0.0 { j } else { path.len() - 1 - j };
                let taper = (PI * i as f64 / (path.len() - 1) as f64).sin();
                outline.push(DVec2::new(path[i].x, path[i].y + side * thickness * taper));
            }
        }
        (
            scrap(shape_geometry(&[Shape::from_points(&outline)]), color),
            lift,
        )
    })
    .collect()
}

/// The four walls and the lid: front (+z), back, right (+x), left, lid (+y).
fn crate_faces(c: &CrateShape) -> [Face; 5] {
    let wall = |origin: DVec3, u: DVec3, n: DVec3, half: f64| Face {
        origin,
        u,
        v: DVec3::Y,
        n,
        half,
    };
    [
        wall(
            DVec3::new(0.0, 0.0, c.d / 2.0),
            DVec3::X,
            DVec3::Z,
            c.w / 2.0,
        ),
        wall(
            DVec3::new(0.0, 0.0, -c.d / 2.0),
            -DVec3::X,
            -DVec3::Z,
            c.w / 2.0,
        ),
        wall(
            DVec3::new(c.w / 2.0, 0.0, 0.0),
            -DVec3::Z,
            DVec3::X,
            c.d / 2.0,
        ),
        wall(
            DVec3::new(-c.w / 2.0, 0.0, 0.0),
            DVec3::Z,
            -DVec3::X,
            c.d / 2.0,
        ),
        Face {
            origin: DVec3::new(0.0, c.h, 0.0),
            u: DVec3::X,
            v: -DVec3::Z,
            n: DVec3::Y,
            half: c.w / 2.0,
        },
    ]
}

const LID: usize = 4;

/// The face a hit relative to the crate struck: the lid if it landed on top,
/// otherwise the wall it is nearest relative to the crate's footprint.
fn struck_face(c: &CrateShape, hit: &TimberHit) -> usize {
    if hit.y > c.h - 0.08 && hit.x.abs() < c.w / 2.0 - 0.06 && hit.z.abs() < c.d / 2.0 - 0.06 {
        return LID;
    }
    let (across, deep) = (hit.x.abs() / (c.w / 2.0), hit.z.abs() / (c.d / 2.0));
    match (deep >= across, hit.z >= 0.0, hit.x >= 0.0) {
        (true, true, _) => 0,
        (true, false, _) => 1,
        (false, _, true) => 2,
        (false, _, false) => 3,
    }
}

/// Each face's breaches. A heavier hit (`size` grows with each one) tears a
/// larger hole, longer along the boards than across them.
fn crate_breaches(
    c: &CrateShape,
    faces: &[Face; 5],
    hits: &[TimberHit],
    seed: i32,
) -> [Vec<Breach>; 5] {
    let mut breaches: [Vec<Breach>; 5] = Default::default();
    let top = c.h - CRATE_LID;
    for (index, hit) in hits.iter().enumerate() {
        let face_index = struck_face(c, hit);
        let face = &faces[face_index];
        let relative = DVec3::new(hit.x, hit.y, hit.z) - face.origin;
        let (u, v) = (relative.dot(face.u), relative.dot(face.v));
        let seed = hash(seed, index as i32 + 100);
        let mut rng = Random::new(f64::from(seed));
        let radius = 0.17 + 0.11 * hit.size;
        let (v_low, v_high) = if face_index == LID {
            (-c.d / 2.0 + 0.12, c.d / 2.0 - 0.12)
        } else {
            (CRATE_BASE + 0.15, top - 0.1)
        };
        breaches[face_index].push(Breach {
            u: clamp(u, -face.half + 0.12, face.half - 0.12),
            v: clamp(v, v_low, v_high),
            along: radius * rng.range(1.15, 1.45),
            across: radius * rng.range(0.7, 0.9),
            seed,
        });
    }
    breaches
}

/// `cargoStack(group, c, damageStage)`'s replacement: a framed, strapped crate of
/// boards on skids, breached where `hits` struck. Stage 2 (35% health or less)
/// adds a snapped strap, a sprung lid board and a failed brace.
pub fn cargo_crate(group: &mut Node, c: CrateShape, hits: &[TimberHit], damage_stage: u32) {
    let seed = crate_seed(&c);
    let mut rng = random(seed, 1);
    let faces = crate_faces(&c);
    let breaches = crate_breaches(&c, &faces, hits, seed);
    let top = c.h - CRATE_LID;
    let worn = damage_stage >= 2;
    // The wall that took the most hits gives way first.
    let weakest = (0..4)
        .max_by_key(|&i| (breaches[i].len(), usize::from(i == 0)))
        .unwrap_or(0);
    let snapped = if rng.next() < 0.5 { -1.0 } else { 1.0 };
    let tone = |rng: &mut Random| board_tone(c.color, rng);

    // Skids under the crate.
    let skids: &[f64] = if c.w >= 2.6 {
        &[-0.36, 0.0, 0.36]
    } else {
        &[-0.34, 0.34]
    };
    for (i, &share) in skids.iter().enumerate() {
        let mut skid = timber_member(
            DVec3::new(c.d, CRATE_BASE, CRATE_SKID_WIDTH),
            hash(seed, i as i32 + 400),
            SKID_COLOR,
        );
        skid.rotation = DQuat::from_rotation_y(FRAC_PI_2);
        skid.position = DVec3::new(c.w * share, CRATE_BASE / 2.0, 0.0);
        group.children.push(skid);
    }
    // The floor; through a breach the walls' and lid's own inner faces show,
    // shaded by the lid.
    let mut floor = timber_member(
        DVec3::new(
            c.w - 2.0 * CRATE_PLANK,
            FLOOR_THICKNESS,
            c.d - 2.0 * CRATE_PLANK,
        ),
        hash(seed, 450),
        scale_hex_color(c.color, 0.7),
    );
    floor.position = DVec3::new(0.0, CRATE_BASE + FLOOR_THICKNESS / 2.0, 0.0);
    group.children.push(floor);
    // A liner turned inside out just within the walls: only its far side shows,
    // so seams and breaches look into the crate's dark hold, not out of the other
    // side.
    let mut liner = box_geometry(
        c.w - 2.0 * CRATE_PLANK - 0.006,
        top - CRATE_BASE - FLOOR_THICKNESS,
        c.d - 2.0 * CRATE_PLANK - 0.006,
    );
    if let Some(indices) = &mut liner.indices {
        for triangle in indices.chunks_mut(3) {
            triangle.swap(1, 2);
        }
    }
    for normal in &mut liner.normals {
        *normal = normal.map(|n| -n);
    }
    let mut hold = scrap(liner, HOLD_COLOR);
    hold.position = DVec3::new(0.0, (CRATE_BASE + FLOOR_THICKNESS + top) / 2.0, 0.0);
    group.children.push(hold);

    // Walls: sheathing planks, then the frame and brace over them.
    for (index, face) in faces[..4].iter().enumerate() {
        let damage = |bend: Option<Bend>| Damage {
            breaches: &breaches[index],
            bend,
            ..Damage::default()
        };
        // Long walls cover the ends' plank edges and frames at the corners.
        let long = index < 2;
        let plank_half = if long {
            face.half
        } else {
            face.half - CRATE_PLANK
        };
        let rows = ((top - CRATE_BASE) / CRATE_BOARD_PITCH)
            .round()
            .clamp(3.0, 10.0);
        let pitch = (top - CRATE_BASE) / rows;
        for row in 0..rows as usize {
            let board = Board {
                u: 0.0,
                v: CRATE_BASE + pitch * (row as f64 + 0.5),
                angle: 0.0,
                length: plank_half * 2.0,
                width: pitch - SEAM,
                thickness: CRATE_PLANK,
                out: -CRATE_PLANK / 2.0,
                color: tone(&mut rng),
                seed: hash(seed, (index * 40 + row) as i32),
            };
            let skin = Damage {
                skin: true,
                ..damage(None)
            };
            lay_board(group, face, &board, &skin);
        }
        let reach = if long {
            face.half + CRATE_BATTEN
        } else {
            face.half
        };
        let frame = |u: f64, v: f64, angle: f64, length: f64, salt: usize| Board {
            u,
            v,
            angle,
            length,
            width: CRATE_BATTEN_WIDTH,
            thickness: CRATE_BATTEN,
            out: CRATE_BATTEN / 2.0,
            color: BATTEN_COLOR,
            seed: hash(seed, (index * 40 + 20 + salt) as i32),
        };
        let bottom = frame(
            0.0,
            CRATE_BASE + CRATE_BATTEN_WIDTH / 2.0,
            0.0,
            reach * 2.0,
            0,
        );
        // A worn crate's weakest wall loses the nails at one end of its bottom batten.
        let popped = (worn && index == weakest).then_some(Bend {
            at: reach * snapped,
            side: -snapped,
            angle: 0.045,
        });
        lay_board(group, face, &bottom, &damage(popped));
        let top_batten = frame(0.0, c.h - CRATE_BATTEN_WIDTH / 2.0, 0.0, reach * 2.0, 1);
        lay_board(group, face, &top_batten, &damage(None));
        let post = c.h - CRATE_BASE - 2.0 * CRATE_BATTEN_WIDTH;
        for (salt, side) in [(2, -1.0), (3, 1.0)] {
            let edge = frame(
                side * (reach - CRATE_BATTEN_WIDTH / 2.0),
                (CRATE_BASE + c.h) / 2.0,
                FRAC_PI_2,
                post,
                salt,
            );
            lay_board(group, face, &edge, &damage(None));
        }
        // The brace rises corner to corner inside the frame, tucked under it.
        let (run, rise) = (2.0 * (reach - CRATE_BATTEN_WIDTH), post);
        let lean = if index % 2 == 0 { 1.0 } else { -1.0 };
        let brace = Board {
            width: BRACE_WIDTH,
            thickness: CRATE_BATTEN - 0.008,
            out: (CRATE_BATTEN - 0.008) / 2.0,
            ..frame(
                0.0,
                (CRATE_BASE + c.h) / 2.0,
                lean * rise.atan2(run),
                run.hypot(rise) + CRATE_BATTEN_WIDTH * 0.6,
                4,
            )
        };
        if worn && index == weakest {
            // It snaps near the middle and its lower half swings out of the frame.
            let at = rng.range(-0.15, 0.15) * brace.length;
            let mut failed = damage(Some(Bend {
                at,
                side: -lean,
                angle: 0.16,
            }));
            failed.gaps.push((at - 0.03, at + 0.03));
            lay_board(group, face, &brace, &failed);
        } else {
            lay_board(group, face, &brace, &damage(None));
        }
    }

    // Splinters blown out of each breach lie on the ground at the wall's foot.
    for (wall, face) in faces[..4].iter().enumerate() {
        for breach in &breaches[wall] {
            let mut rng = random(breach.seed, 600);
            let count = 5 + (rng.next() * 5.0) as usize;
            for _ in 0..count {
                let (length, width, thickness) = (
                    rng.range(0.07, 0.38),
                    rng.range(0.025, 0.08),
                    rng.range(0.012, 0.02),
                );
                let color = match (rng.next() * 3.0) as usize {
                    0 => RAW_WOOD,
                    1 => TORN_WOOD,
                    _ => tone(&mut rng),
                };
                let mut chip = scrap(flake(length, width), color);
                chip.position = face.at(
                    clamp(
                        breach.u + rng.range(-0.7, 0.7),
                        -face.half - 0.4,
                        face.half + 0.4,
                    ),
                    thickness + LITTER_LIFT,
                    rng.range(CRATE_BATTEN + 0.08, 1.1),
                );
                chip.rotation = DQuat::from_rotation_y(rng.range(0.0, PI))
                    * DQuat::from_rotation_z(rng.range(-0.08, 0.08));
                group.children.push(chip);
            }
        }
    }

    // The lid: boards across the top inside a frame along its long edges.
    let lid = &faces[LID];
    let rows = (c.d / CRATE_BOARD_PITCH).round().clamp(4.0, 12.0);
    let pitch = c.d / rows;
    let strap_x = c.w * STRAP_SPREAD;
    // The snapped strap no longer holds its end of the lid; one or two boards
    // inside the frame spring up there, pivoting at the strap that still holds.
    let sprung = 1 + (rng.next() * (rows - 2.0).max(1.0)) as usize;
    // Hits on the walls shake the lid: its boards along a struck wall split along
    // the grain, and a worn crate loses a piece of one by its weakest wall, a dark
    // slot that shows from above. In lid coordinates x runs along the boards and
    // y towards -z.
    let mut shaken: Vec<Damage> = (0..rows as usize).map(|_| Damage::default()).collect();
    let edge_row =
        |v: f64| (((v + c.d / 2.0) / pitch).floor() as usize).clamp(1, rows as usize - 2);
    for (wall, struck) in breaches[..4].iter().enumerate() {
        for (index, breach) in struck.iter().enumerate() {
            let mut rng = random(breach.seed, 500);
            let (row, t) = match wall {
                0 => (1, breach.u),
                1 => (rows as usize - 2, -breach.u),
                2 => (edge_row(breach.u), c.w / 2.0 - rng.range(0.2, 0.45)),
                _ => (edge_row(-breach.u), -c.w / 2.0 + rng.range(0.2, 0.45)),
            };
            shaken[row]
                .cracks
                .push((t, rng.range(0.22, 0.45), hash(breach.seed, 501)));
            if worn && wall == weakest && index == 0 {
                let open = rng.range(0.22, 0.4);
                shaken[row].gaps.push(match wall {
                    2 => (c.w / 2.0 - open * 2.2, c.w),
                    3 => (-c.w, -c.w / 2.0 + open * 2.2),
                    _ => (t - open, t + open * rng.range(0.8, 1.3)),
                });
            }
        }
    }
    for (row, shock) in shaken.into_iter().enumerate() {
        let v = -c.d / 2.0 + pitch * (row as f64 + 0.5);
        let board = Board {
            u: 0.0,
            v,
            angle: 0.0,
            length: c.w,
            width: pitch - SEAM,
            thickness: CRATE_LID,
            out: -CRATE_LID / 2.0,
            color: tone(&mut rng),
            seed: hash(seed, 300 + row as i32),
        };
        let inside = v.abs() < c.d / 2.0 - CRATE_BATTEN_WIDTH - pitch / 2.0;
        let bend = (worn && inside && (row == sprung || row == sprung + 1)).then(|| Bend {
            at: -snapped * (strap_x - STRAP_WIDTH),
            side: snapped,
            angle: if row == sprung { 0.11 } else { 0.05 },
        });
        lay_board(
            group,
            lid,
            &board,
            &Damage {
                breaches: &breaches[LID],
                bend,
                skin: true,
                ..shock
            },
        );
    }
    for (salt, side) in [(0, -1.0), (1, 1.0)] {
        let board = Board {
            u: 0.0,
            v: side * (c.d / 2.0 - CRATE_BATTEN_WIDTH / 2.0),
            angle: 0.0,
            length: c.w,
            width: CRATE_BATTEN_WIDTH,
            thickness: CRATE_BATTEN,
            out: CRATE_BATTEN / 2.0,
            color: BATTEN_COLOR,
            seed: hash(seed, 350 + salt),
        };
        lay_board(
            group,
            lid,
            &board,
            &Damage {
                breaches: &breaches[LID],
                ..Damage::default()
            },
        );
    }

    // Two steel straps over the lid and down the long walls, bridging between the
    // battens like tensioned bands. A worn crate's strap on one side has snapped
    // and its ends curl up off the lid.
    let band = |size: DVec3| scrap(box_geometry(size.x, size.y, size.z), STRAP_COLOR);
    let over = c.h + CRATE_BATTEN + STRAP_THICKNESS / 2.0;
    let wrap = c.d / 2.0 + CRATE_BATTEN + STRAP_THICKNESS / 2.0;
    let rise = over + STRAP_THICKNESS / 2.0 - CRATE_BASE;
    for side in [-1.0, 1.0] {
        let x = side * strap_x;
        let broken = worn && side == snapped;
        for wall in [-1.0, 1.0] {
            let mut down = band(DVec3::new(STRAP_WIDTH, rise, STRAP_THICKNESS));
            down.position = DVec3::new(x, CRATE_BASE + rise / 2.0, wall * wrap);
            if broken {
                // Slack now, it sags away from the wall from where it is still held.
                let sag = DQuat::from_rotation_x(wall * 0.05);
                let foot = DVec3::new(x, CRATE_BASE, wall * wrap);
                down.position = foot + sag * (down.position - foot);
                down.rotation = sag;
            }
            group.children.push(down);
        }
        let span = 2.0 * wrap + STRAP_THICKNESS;
        if !broken {
            let mut across = band(DVec3::new(STRAP_WIDTH, STRAP_THICKNESS, span));
            across.position = DVec3::new(x, over, 0.0);
            group.children.push(across);
            let mut buckle = band(DVec3::new(STRAP_WIDTH + 0.025, 0.014, 0.1));
            buckle.position = DVec3::new(x, over + STRAP_THICKNESS, rng.range(-0.2, 0.2) * c.d);
            group.children.push(buckle);
            continue;
        }
        let cut = rng.range(-0.25, 0.25) * c.d;
        for wall in [-1.0, 1.0] {
            // Each end still lies over the lid from its wall, then springs up and
            // curls where it parted.
            let length = (wall * wrap - cut).abs() - 0.03;
            let curled = rng.range(0.22, 0.38).min(length * 0.5);
            let flat = length - curled;
            let edge = wall * (wrap + STRAP_THICKNESS / 2.0);
            let mut lying = band(DVec3::new(STRAP_WIDTH, STRAP_THICKNESS, flat));
            lying.position = DVec3::new(x, over, edge - wall * flat / 2.0);
            group.children.push(lying);
            let mut tip = band(DVec3::new(STRAP_WIDTH, STRAP_THICKNESS, curled));
            let bend = DVec3::new(x, over, edge - wall * flat);
            let curl = DQuat::from_rotation_x(wall * rng.range(0.6, 1.1));
            tip.position = bend + curl * DVec3::new(0.0, 0.0, -wall * curled / 2.0);
            tip.rotation = curl;
            group.children.push(tip);
        }
    }
}

/// The model of a broken crate's section, centred on its body: `size` is along
/// its boards, up its face and through it (see [`CratePiece`]), `color` the
/// crate's paint and `look` picks how it tore. Batched like the watchtower's
/// pieces; the renderer shares one model between sections that look alike.
pub fn crate_piece_model(piece: CratePiece, size: DVec3, color: u32, look: u32) -> Node {
    let seed = to_int32(f64::from(look) + 1.0).wrapping_mul(0x2c1b_3c6d);
    let mut rng = random(seed, 0);
    let mut group = Node::default();
    let (length, height) = (size.x, size.y);
    let half = length / 2.0;
    // The section's own frame: boards along x, outside towards +z. Sheathing lies
    // behind z = 0 and the frame in front of it, as on the standing crate.
    let face = Face {
        origin: DVec3::new(0.0, 0.0, size.z / 2.0 - CRATE_BATTEN),
        u: DVec3::X,
        v: DVec3::Y,
        n: DVec3::Z,
        half,
    };
    let tone = |rng: &mut Random| board_tone(color, rng);
    let loose = || Damage {
        coarse: true,
        ..Damage::default()
    };
    match piece {
        CratePiece::Wall
        | CratePiece::WallLeft
        | CratePiece::WallMiddle
        | CratePiece::WallRight => {
            // Torn sides are ragged; at a clean side the boards met a corner.
            let (low, high) = piece.torn();
            let ragged = |rng: &mut Random, deepest: f64| {
                let mut gaps = Vec::new();
                for (torn, side) in [(low, -1.0), (high, 1.0)] {
                    if torn {
                        let depth = rng.range(0.03, deepest).min(length * 0.35);
                        gaps.push(if side > 0.0 {
                            (half - depth, half + 1.0)
                        } else {
                            (-half - 1.0, -half + depth)
                        });
                    }
                }
                gaps
            };
            let rows = (height / CRATE_BOARD_PITCH).round().max(3.0);
            let pitch = height / rows;
            // A board or two knocked clean out somewhere; a wall that came away
            // whole always shows the blast.
            let knocked = if low || high { 1 } else { 2 };
            let missing: Vec<usize> = (0..knocked)
                .map(|_| (rng.next() * rows * 1.6) as usize)
                .collect();
            for row in 0..rows as usize {
                let board = Board {
                    u: 0.0,
                    v: -height / 2.0 + pitch * (row as f64 + 0.5),
                    angle: 0.0,
                    length,
                    width: pitch - SEAM,
                    thickness: CRATE_PLANK,
                    out: -CRATE_PLANK / 2.0,
                    color: tone(&mut rng),
                    seed: hash(seed, row as i32),
                };
                let mut damage = Damage {
                    gaps: ragged(&mut rng, 0.42),
                    ..loose()
                };
                if missing.contains(&row) {
                    let at = rng.range(-0.3, 0.3) * length;
                    damage
                        .gaps
                        .push((at - rng.range(0.12, 0.3), at + rng.range(0.1, 0.3)));
                }
                lay_board(&mut group, &face, &board, &damage);
            }
            let frame = |u: f64, v: f64, angle: f64, length: f64, salt: i32| Board {
                u,
                v,
                angle,
                length,
                width: CRATE_BATTEN_WIDTH,
                thickness: CRATE_BATTEN,
                out: CRATE_BATTEN / 2.0,
                color: BATTEN_COLOR,
                seed: hash(seed, 40 + salt),
            };
            for (salt, side) in [(0, -1.0), (1, 1.0)] {
                let batten = frame(
                    0.0,
                    side * (height / 2.0 - CRATE_BATTEN_WIDTH / 2.0),
                    0.0,
                    length,
                    salt,
                );
                let damage = Damage {
                    gaps: ragged(&mut rng, 0.6),
                    ..loose()
                };
                lay_board(&mut group, &face, &batten, &damage);
            }
            let post = height - 2.0 * CRATE_BATTEN_WIDTH;
            for (torn, side, salt) in [(low, -1.0, 2), (high, 1.0, 3)] {
                if !torn {
                    let corner = frame(
                        side * (half - CRATE_BATTEN_WIDTH / 2.0),
                        0.0,
                        FRAC_PI_2,
                        post,
                        salt,
                    );
                    lay_board(&mut group, &face, &corner, &loose());
                }
            }
            // The brace: corner to corner on a whole wall, a stub climbing from the
            // corner on a wall's end, a broken length across a middle.
            let lean = if rng.next() < 0.5 { -1.0 } else { 1.0 };
            let (start, run, kept) = match (low, high) {
                (false, false) => {
                    let run = length - 2.0 * CRATE_BATTEN_WIDTH;
                    (
                        -lean * (half - CRATE_BATTEN_WIDTH),
                        lean * run,
                        run.hypot(post),
                    )
                }
                (false, true) => {
                    let run = 2.0 * length;
                    let kept = run.hypot(post) * rng.range(0.32, 0.45);
                    (-half + CRATE_BATTEN_WIDTH, run, kept)
                }
                (true, false) => {
                    let run = 2.0 * length;
                    let kept = run.hypot(post) * rng.range(0.32, 0.45);
                    (half - CRATE_BATTEN_WIDTH, -run, kept)
                }
                (true, true) => (-half, 3.0 * length, length * 1.05),
            };
            let angle = post.atan2(run) * if low && high { lean } else { 1.0 };
            let direction = DVec2::new(angle.cos(), angle.sin());
            let from = if low && high {
                DVec2::new(-half, -direction.y * length / 2.0)
            } else {
                DVec2::new(start, -post / 2.0)
            };
            let middle = from + direction * (kept / 2.0);
            let brace = Board {
                u: middle.x,
                v: middle.y,
                angle,
                length: kept + CRATE_BATTEN_WIDTH * 0.3,
                width: BRACE_WIDTH,
                thickness: CRATE_BATTEN - 0.008,
                out: (CRATE_BATTEN - 0.008) / 2.0,
                color: BATTEN_COLOR,
                seed: hash(seed, 45),
            };
            let end = kept / 2.0 + CRATE_BATTEN_WIDTH * 0.15;
            let mut gaps = Vec::new();
            if high || low {
                gaps.push((end - 0.02, end + 1.0));
            }
            if low && high {
                gaps.push((-end - 1.0, -end + 0.02));
            }
            lay_board(&mut group, &face, &brace, &Damage { gaps, ..loose() });
        }
        CratePiece::Lid | CratePiece::LidFront | CratePiece::LidMiddle | CratePiece::LidBack => {
            // Boards along x. Along each torn edge (-y, +y) the outermost boards
            // broke part way along; each clean edge keeps its frame batten.
            let (low, high) = piece.torn();
            let lid_face = Face {
                origin: DVec3::new(0.0, 0.0, size.z / 2.0 - CRATE_BATTEN),
                ..face
            };
            let rows = (height / CRATE_BOARD_PITCH).round().max(2.0);
            let pitch = height / rows;
            for row in 0..rows as usize {
                let v = -height / 2.0 + pitch * (row as f64 + 0.5);
                let board = Board {
                    u: 0.0,
                    v,
                    angle: 0.0,
                    length,
                    width: pitch - SEAM,
                    thickness: CRATE_LID,
                    out: -CRATE_LID / 2.0,
                    color: tone(&mut rng),
                    seed: hash(seed, row as i32),
                };
                let near = |side: f64| side * v > height / 2.0 - pitch * 1.5;
                let mut damage = loose();
                if (low && near(-1.0)) || (high && near(1.0)) {
                    let at = rng.range(-0.35, 0.35) * length;
                    damage.gaps.push(if rng.next() < 0.5 {
                        (at, half + 1.0)
                    } else {
                        (-half - 1.0, at)
                    });
                }
                lay_board(&mut group, &lid_face, &board, &damage);
            }
            for (torn, side) in [(low, -1.0), (high, 1.0)] {
                if torn {
                    continue;
                }
                let frame = Board {
                    u: 0.0,
                    v: side * (height / 2.0 - CRATE_BATTEN_WIDTH / 2.0),
                    angle: 0.0,
                    length,
                    width: CRATE_BATTEN_WIDTH,
                    thickness: CRATE_BATTEN,
                    out: CRATE_BATTEN / 2.0,
                    color: BATTEN_COLOR,
                    seed: hash(seed, 60 + side as i32),
                };
                lay_board(&mut group, &lid_face, &frame, &loose());
            }
            // The straps' ends: anchored at a clean edge, lying over the lid and
            // curling up where they tore. A middle piece lost its straps.
            let anchor = match (low, high) {
                (true, true) => None,
                (true, false) => Some(1.0),
                _ => Some(-1.0),
            };
            if let Some(edge_side) = anchor {
                let toward = -edge_side;
                for side in [-1.0, 1.0] {
                    let reach = height * rng.range(0.55, 0.95);
                    let curled = rng.range(0.15, 0.3).min(reach * 0.5);
                    let flat = reach - curled;
                    let x = side * length * STRAP_SPREAD;
                    let edge = edge_side * height / 2.0;
                    let rest = size.z / 2.0 + STRAP_THICKNESS / 2.0;
                    let band = |long: f64| {
                        scrap(
                            box_geometry(STRAP_WIDTH, long, STRAP_THICKNESS),
                            STRAP_COLOR,
                        )
                    };
                    let mut lying = band(flat);
                    lying.position = DVec3::new(x, edge + toward * flat / 2.0, rest);
                    group.children.push(lying);
                    let mut tip = band(curled);
                    let bend = DVec3::new(x, edge + toward * flat, rest);
                    let curl = DQuat::from_rotation_x(toward * rng.range(0.5, 1.0));
                    tip.position = bend + curl * DVec3::new(0.0, toward * curled / 2.0, 0.0);
                    tip.rotation = curl;
                    group.children.push(tip);
                }
            }
        }
        CratePiece::Skid => {
            let face = Face {
                origin: DVec3::ZERO,
                ..face
            };
            let board = Board {
                u: 0.0,
                v: 0.0,
                angle: 0.0,
                length,
                width: height,
                thickness: size.z,
                out: 0.0,
                color: SKID_COLOR,
                seed,
            };
            let at = half - rng.range(0.0, 0.25);
            lay_board(
                &mut group,
                &face,
                &board,
                &Damage {
                    gaps: vec![(at, half + 1.0)],
                    ..loose()
                },
            );
        }
        CratePiece::Plank => {
            let face = Face {
                origin: DVec3::ZERO,
                ..face
            };
            let board = Board {
                u: 0.0,
                v: 0.0,
                angle: 0.0,
                length,
                width: height,
                thickness: size.z,
                out: 0.0,
                color: tone(&mut rng),
                seed,
            };
            let (left, right) = (rng.range(0.0, 0.2), rng.range(0.0, 0.2));
            lay_board(
                &mut group,
                &face,
                &board,
                &Damage {
                    gaps: vec![(-half - 1.0, -half + left), (half - right, half + 1.0)],
                    ..loose()
                },
            );
        }
    }
    batch(&mut group);
    group
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::node_bounds;
    use crate::sim::crate_layout::crate_section_looks;

    /// The nearest front-facing wood a ray from `origin` along `dir` meets, as the
    /// distance along it (Möller–Trumbore over the batched triangles).
    fn first_board(node: &Node, origin: DVec3, dir: DVec3) -> Option<f64> {
        let mut group = node.clone();
        batch(&mut group);
        group
            .children
            .iter()
            .filter_map(|child| child.drawable.as_ref())
            .filter(|drawable| drawable.material.map.is_some())
            .flat_map(|drawable| drawable.mesh.positions.chunks(3))
            .filter_map(|triangle| {
                let [a, b, c] = [0, 1, 2].map(|i| {
                    let p = triangle[i];
                    DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))
                });
                let (e1, e2) = (b - a, c - a);
                let p = dir.cross(e2);
                // Back faces are culled, as the GPU does (the hold is inside out).
                let det = e1.dot(p);
                if det < 1e-12 {
                    return None;
                }
                let t0 = origin - a;
                let u = t0.dot(p) / det;
                let q = t0.cross(e1);
                let v = dir.dot(q) / det;
                let t = e2.dot(q) / det;
                (u >= 0.0 && v >= 0.0 && u + v <= 1.0 && t > 0.0).then_some(t)
            })
            .min_by(f64::total_cmp)
    }

    #[test]
    fn a_hit_opens_a_hole_in_the_face_it_struck() {
        let c = CrateShape {
            x: 4.0,
            z: -20.0,
            w: 3.0,
            d: 3.0,
            h: 2.6,
            color: 0xb88b53,
        };
        let hit = TimberHit {
            x: 0.3,
            y: 1.1,
            z: c.d / 2.0,
            size: 1.0,
        };
        // Looking straight into the front at the hit, and into the back opposite it.
        let front = DVec3::new(hit.x, hit.y, c.d);
        let back = DVec3::new(hit.x, hit.y, -c.d);
        let mut intact = Node::default();
        cargo_crate(&mut intact, c, &[], 0);
        let wall = first_board(&intact, front, -DVec3::Z).expect("the front is boarded");
        assert!((wall - c.d / 2.0).abs() < 1e-3, "the sheathing: {wall}");
        let mut struck = Node::default();
        cargo_crate(&mut struck, c, &[hit], 1);
        let through = first_board(&struck, front, -DVec3::Z).expect("the back wall");
        assert!(
            through > c.d,
            "the shell broke through the front: {through}"
        );
        let far = first_board(&struck, back, DVec3::Z).expect("the back is boarded");
        assert!(
            (far - c.d / 2.0).abs() < 1e-3,
            "the back wall is whole: {far}"
        );
    }

    #[test]
    fn every_section_look_fits_the_body_it_rides() {
        for (w, d, h) in [(3.1, 3.0, 2.6), (2.4, 2.8, 2.1), (2.0, 2.0, 2.0)] {
            for (piece, size) in crate_section_looks(w, d, h) {
                for look in 0..3 {
                    let size = DVec3::from_array(size);
                    let node = crate_piece_model(piece, size, 0xb47a49, look);
                    let bounds = node_bounds(&node, glam::DMat4::IDENTITY);
                    // Splinters and a curled strap end may poke a little past the box.
                    let slack = DVec3::splat(0.25);
                    assert!(
                        bounds.min.cmpge(-size / 2.0 - slack).all()
                            && bounds.max.cmple(size / 2.0 + slack).all(),
                        "{piece:?} {size:?}: {bounds:?}"
                    );
                    assert!(
                        bounds.max.x - bounds.min.x > size.x * 0.6,
                        "{piece:?} keeps its length"
                    );
                }
            }
        }
    }
}
