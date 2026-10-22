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
use super::building_kit::{textured, tint_over};
use super::model_primitives::{box_part, cylinder_part, material, put, shadowed};
use crate::geometry::math::{compose, quat_from_euler, smoothstep};
use crate::geometry::{Mesh, Shape, shape_geometry};
use crate::scene::{Material, Node, TextureSource};
use crate::sim::math::{Random, clamp};
use crate::sim::timber_layout::{TimberFace, TimberMark, TimberPart, TimberPartKind};

const STRAP_COLOR: u32 = 0x3d3934;
const BOLT_COLOR: u32 = 0x5b554c;
/// Height of a post's iron band and the radius of its bolt heads.
const STRAP_HEIGHT: f64 = 0.085;
const BOLT_RADIUS: f64 = 0.032;
/// How much paler a member's worn chamfers are than its faces.
const EDGE_WEAR: f64 = 1.32;
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
const PLANK_AVERAGE: u32 = 0xbd9b74;
/// The largest edge chamfer on a member, in metres.
const MEMBER_BEVEL: f64 = 0.03;

/// The textured wood of a member painted `color`; batching bakes the tint into
/// vertex colours, so every member shares one material and draw setup.
pub(super) fn wood(color: u32) -> Arc<Material> {
    textured(TIMBER_TEXTURE, tint_over(color, PLANK_AVERAGE), 0.03, 0.82)
}

/// How a member maps the timber atlas: which plank row and end-grain cell it
/// samples, where along the board it starts and whether the row is mirrored.
struct MemberUvs {
    size: DVec3,
    grain: usize,
    row_center: f64,
    start: f64,
    cell: usize,
    mirror: f64,
}

impl MemberUvs {
    fn new(size: DVec3, seed: i32) -> Self {
        let mut rng = Random::new(f64::from(seed));
        let row = (rng.next() * GRAIN_ROWS as f64).floor() as usize;
        let start = rng.next();
        let cell = (rng.next() * END_CELLS as f64).floor() as usize;
        let mirror = if rng.next() < 0.5 { -1.0 } else { 1.0 };
        let rows = (GRAIN_ROWS + 1) as f64;
        Self {
            size,
            grain: if size.x >= size.y { 0 } else { 1 },
            // The image's top row is at v = 1 (flipped like Three's textures).
            row_center: 1.0 - (row as f64 + 0.5) / rows,
            start,
            cell,
            mirror,
        }
    }

    /// The atlas coordinate of `p` on a face whose normal runs along `axis`: end
    /// grain stretched over the ends, a plank row at true scale on the long faces.
    fn at(&self, p: DVec3, axis: usize) -> [f64; 2] {
        let rows = (GRAIN_ROWS + 1) as f64;
        let size = self.size;
        if axis == self.grain {
            let other = |i: usize| (p[i] / size[i] + 0.5) * (1.0 - 2.0 * ATLAS_INSET) + ATLAS_INSET;
            let (u, v) = if self.grain == 0 {
                (other(2), other(1))
            } else {
                (other(0), other(2))
            };
            [(self.cell as f64 + u) / END_CELLS as f64, v / rows]
        } else {
            let across = 3 - axis - self.grain;
            let fit = (ROW_SPAN * (1.0 - 2.0 * ATLAS_INSET) / size[across]).min(1.0);
            [
                self.start + p[self.grain] / GRAIN_SPAN,
                self.row_center + self.mirror * p[across] / ROW_SPAN * fit / rows,
            ]
        }
    }

    /// The long face whose mapping a chamfer between faces `i` and `j` borrows.
    fn chamfer_axis(&self, i: usize, j: usize) -> usize {
        if i != self.grain { i } else { j }
    }
}

/// The chamfer a member of `size` gets along every edge: a few centimetres on
/// beams and posts, less on thin boards, so edges catch the light like planed
/// timber.
fn member_bevel(size: DVec3) -> f64 {
    (size.min_element() * 0.16).min(MEMBER_BEVEL)
}

/// Appends a flat polygon (a triangle or a quad), wound to face `normal`.
fn polygon(
    positions: &mut Vec<f64>,
    normals: &mut Vec<f64>,
    uvs: &mut Vec<f64>,
    indices: &mut Vec<u32>,
    corners: &[(DVec3, [f64; 2])],
    normal: DVec3,
) {
    let base = (positions.len() / 3) as u32;
    for (p, uv) in corners {
        positions.extend_from_slice(&[p.x, p.y, p.z]);
        normals.extend_from_slice(&[normal.x, normal.y, normal.z]);
        uvs.extend_from_slice(uv);
    }
    let facing = (corners[1].0 - corners[0].0)
        .cross(corners[2].0 - corners[0].0)
        .dot(normal)
        > 0.0;
    for k in 1..corners.len() as u32 - 1 {
        if facing {
            indices.extend_from_slice(&[base, base + k, base + k + 1]);
        } else {
            indices.extend_from_slice(&[base, base + k + 1, base + k]);
        }
    }
}

/// A member's box with the grain along the longer of its width (beams) and height
/// (posts), its edges chamfered by `bevel`. Long faces map one plank row at true
/// scale, starting at a seeded point along the board; the two ends map one
/// end-grain cell. `seed` picks the row, the start, the cell and a mirror, so
/// neighbouring members never repeat. The six faces come first (x-, x+, y-, y+,
/// z-, z+, four corners each), then the chamfers and corner facets.
fn member_mesh(size: DVec3, seed: i32, bevel: f64) -> Mesh {
    let map = MemberUvs::new(size, seed);
    let half = size / 2.0;
    let inner = half - DVec3::splat(bevel);
    let mut positions = Vec::with_capacity(240);
    let mut normals = Vec::with_capacity(240);
    let mut uvs = Vec::with_capacity(160);
    let mut indices = Vec::with_capacity(132);
    let unit = |axis: usize| {
        let mut v = DVec3::ZERO;
        v[axis] = 1.0;
        v
    };
    for axis in 0..3 {
        let (a, b) = ((axis + 1) % 3, (axis + 2) % 3);
        for sign in [-1.0, 1.0] {
            let corners: Vec<(DVec3, [f64; 2])> =
                [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                    .iter()
                    .map(|&(sa, sb)| {
                        let mut p = DVec3::ZERO;
                        p[axis] = sign * half[axis];
                        p[a] = sa * inner[a];
                        p[b] = sb * inner[b];
                        (p, map.at(p, axis))
                    })
                    .collect();
            polygon(
                &mut positions,
                &mut normals,
                &mut uvs,
                &mut indices,
                &corners,
                unit(axis) * sign,
            );
        }
    }
    if bevel <= 0.0 {
        return Mesh::from_f64(&positions, &normals, &uvs, Some(indices));
    }
    // Edge chamfers between faces i and j, running along the third axis k.
    for (i, j) in [(0, 1), (0, 2), (1, 2)] {
        let k = 3 - i - j;
        let face = map.chamfer_axis(i, j);
        for si in [-1.0, 1.0] {
            for sj in [-1.0, 1.0] {
                let corner = |on_i: bool, sk: f64| {
                    let mut p = DVec3::ZERO;
                    p[i] = si * if on_i { half[i] } else { inner[i] };
                    p[j] = sj * if on_i { inner[j] } else { half[j] };
                    p[k] = sk * inner[k];
                    (p, map.at(p, face))
                };
                let corners = [
                    corner(true, -1.0),
                    corner(true, 1.0),
                    corner(false, 1.0),
                    corner(false, -1.0),
                ];
                let normal = (unit(i) * si + unit(j) * sj).normalize();
                polygon(
                    &mut positions,
                    &mut normals,
                    &mut uvs,
                    &mut indices,
                    &corners,
                    normal,
                );
            }
        }
    }
    // Corner facets.
    let face = map.chamfer_axis(0, 1);
    for sx in [-1.0, 1.0] {
        for sy in [-1.0, 1.0] {
            for sz in [-1.0, 1.0] {
                let s = DVec3::new(sx, sy, sz);
                let corners: Vec<(DVec3, [f64; 2])> = (0..3)
                    .map(|axis| {
                        let mut p = inner * s;
                        p[axis] = s[axis] * half[axis];
                        (p, map.at(p, face))
                    })
                    .collect();
                polygon(
                    &mut positions,
                    &mut normals,
                    &mut uvs,
                    &mut indices,
                    &corners,
                    s.normalize(),
                );
            }
        }
    }
    Mesh::from_f64(&positions, &normals, &uvs, Some(indices))
}

/// A sawn member of `size` (grain along the longer of x and y) painted `color`,
/// for framing outside the timber walls: the watchtower's posts, braces and deck.
pub(super) fn timber_member(size: DVec3, seed: i32, color: u32) -> Node {
    shadowed(
        Arc::new(member_mesh(size, seed, member_bevel(size))),
        wood(color),
    )
}

/// Only the +z face of [`timber_member`] (2 triangles instead of 12), for boards
/// whose other faces nothing can see.
pub(super) fn timber_member_face(size: DVec3, seed: i32, color: u32) -> Node {
    let mut mesh = member_mesh(size, seed, 0.0);
    // Faces run x-, x+, y-, y+, z-, z+, four corners and two triangles each.
    let keep = 20..24;
    mesh.positions.drain(..keep.start);
    mesh.normals.drain(..keep.start);
    mesh.uvs.drain(..keep.start);
    mesh.indices = Some(vec![0, 1, 2, 0, 2, 3]);
    shadowed(Arc::new(mesh), wood(color))
}

/// `timberPartModel(p)`: a batched member with its chips and cracks, reused for
/// standing walls and detached pieces. Posts carry two iron bands, each fixed by
/// a hex-headed through bolt.
pub fn timber_part_model(p: &TimberPart) -> Node {
    let mut group = Node::default();
    group.children.push(timber_member(
        DVec3::new(p.w, p.h, p.d),
        p.damage_seed,
        p.color,
    ));
    for (mark_index, mark) in p.marks.iter().enumerate() {
        add_mark(&mut group, p, mark_index, mark);
    }
    if p.kind == TimberPartKind::Post {
        for fraction in [-0.28, 0.28] {
            let bent = p.damage == 3 && fraction > 0.0;
            let mut band = Node::group("");
            band.children.push(box_part(
                p.w + 0.024,
                STRAP_HEIGHT,
                p.d + 0.025,
                STRAP_COLOR,
                0.0,
            ));
            for side in [-1.0, 1.0] {
                let mut head = cylinder_part(BOLT_RADIUS, 0.035, BOLT_COLOR, 6);
                head.set_rotation_euler(PI / 2.0, 0.0, 0.0);
                put(&mut band, head, 0.0, 0.0, side * (p.d / 2.0 + 0.0125));
            }
            if bent {
                band.set_rotation_euler(0.0, 0.0, 0.13);
            }
            band.position = DVec3::new(0.0, p.h * fraction, if bent { 0.025 } else { 0.0 });
            // Flatten the band into the member's frame so it batches with it.
            let transform = compose(band.position, band.rotation, band.scale);
            for mut part in band.children {
                let local = compose(part.position, part.rotation, part.scale);
                let (scale, rotation, position) =
                    (transform * local).to_scale_rotation_translation();
                part.position = position;
                part.rotation = rotation;
                part.scale = scale;
                group.children.push(part);
            }
        }
    }
    batch(&mut group);
    weather_wood(&mut group, p.y, p.damage_seed);
    group
}

/// A member torn out of a collapsing wall: the standing member's model with
/// both ends snapped into a fringe of pale splinters where it tore off its posts
/// and neighbours. The splinters are the member's own textured wood, so they
/// batch into its draw.
pub fn timber_debris_model(p: &TimberPart) -> Node {
    let mut rng = Random::new(f64::from(p.damage_seed) * 2.71 + 5.0);
    let mut group = timber_part_model(p);
    // The member's long axis: x for boards, y for posts.
    let along = if p.h > p.w && p.h > p.d { 1 } else { 0 };
    let size = [p.w, p.h, p.d];
    let (across_a, across_b) = if along == 1 { (0, 2) } else { (1, 2) };
    for end in [-1.0, 1.0] {
        let mut positions = Vec::new();
        let mut uvs = Vec::new();
        for _ in 0..SPLINTERS_PER_END {
            let half_a = size[across_a] * rng.range(0.1, 0.2);
            let half_b = size[across_b] * rng.range(0.12, 0.22);
            let ca = rng.range(-0.5, 0.5) * size[across_a] * 0.7;
            let cb = rng.range(-0.5, 0.5) * size[across_b] * 0.7;
            // Long, ragged slivers, so a snapped end reads as torn wood from the
            // overhead camera rather than a sawn cut.
            let reach = rng.range(0.12, 0.38) + size[along] * 0.03;
            let lean = [rng.range(-0.4, 0.4), rng.range(-0.4, 0.4)];
            // Base corners sunk just inside the end face, and the tip out past it.
            let face = end * (size[along] / 2.0 - 0.01);
            let point = |a: f64, b: f64, out: f64| {
                let mut v = [0.0; 3];
                v[along] = face + end * out;
                v[across_a] = a;
                v[across_b] = b;
                v
            };
            let base = [
                point(ca - half_a, cb - half_b, 0.0),
                point(ca + half_a, cb - half_b, 0.0),
                point(ca + half_a, cb + half_b, 0.0),
                point(ca - half_a, cb + half_b, 0.0),
            ];
            let tip = point(
                ca + lean[0] * half_a * 2.0,
                cb + lean[1] * half_b * 2.0,
                reach,
            );
            // Both windings, so every sliver shows whichever way it faces.
            for i in 0..4 {
                let (a, b) = (base[i], base[(i + 1) % 4]);
                positions.extend(a.into_iter().chain(b).chain(tip));
                positions.extend(b.into_iter().chain(a).chain(tip));
                // A sliver of plank face, grain running out to the tip.
                for _ in 0..2 {
                    uvs.extend([0.1, 0.3, 0.14, 0.3, 0.12, 0.42]);
                }
            }
        }
        let mut mesh = Mesh::from_f64(&positions, &[], &uvs, None);
        mesh.compute_vertex_normals();
        group
            .children
            .push(Node::mesh(Arc::new(mesh), wood(SPLINTER_COLOR)));
    }
    batch(&mut group);
    group
}

/// Splinters jutting from each snapped end of a fallen member, and their fresh,
/// pale wood.
const SPLINTERS_PER_END: usize = 8;
const SPLINTER_COLOR: u32 = 0xd2ad78;

/// Weathers a batched member's wood in its vertex colours: worn, paler chamfers
/// catch the light along every edge, and the wood darkens toward the ground
/// where rain splashes it, greening with moss at the very foot. Each member
/// (`seed`) has its own age, so neighbouring boards differ in tone and some
/// have gone silver. `centre` is the member's height above the ground.
fn weather_wood(group: &mut Node, centre: f64, seed: i32) {
    let mut rng = Random::new(f64::from(seed) * 1.37 + 11.0);
    let tone = rng.range(0.84, 1.1);
    let silvered = if rng.next() < 0.3 {
        rng.range(0.3, 0.6)
    } else {
        0.0
    };
    for child in &mut group.children {
        let Some(drawable) = &mut child.drawable else {
            continue;
        };
        let wood_map = drawable
            .material
            .map
            .as_ref()
            .is_some_and(|map| map.source == TextureSource::File(TIMBER_TEXTURE));
        if !wood_map {
            continue;
        }
        let mesh = Arc::make_mut(&mut drawable.mesh);
        for ((color, position), normal) in mesh
            .colors
            .iter_mut()
            .zip(&mesh.positions)
            .zip(&mesh.normals)
        {
            let edge = normal.iter().filter(|n| n.abs() > 0.05).count() > 1;
            let height = centre + f64::from(position[1]);
            let splash = 0.6 + 0.4 * smoothstep(height, 0.0, 0.8);
            let moss = 1.0 - smoothstep(height, 0.0, 0.3);
            let wear = if edge { EDGE_WEAR } else { 1.0 };
            let grey = (f64::from(color[0]) + f64::from(color[1]) + f64::from(color[2])) / 3.0;
            for (k, c) in color.iter_mut().enumerate() {
                let aged = f64::from(*c) * (1.0 - silvered) + grey * 0.92 * silvered;
                let green = [0.86, 1.04, 0.8][k];
                *c = (aged * tone * splash * wear * (1.0 + (green - 1.0) * moss * 0.6)) as f32;
            }
        }
    }
}

/// One impact scar: a two-tone chip and a forked crack, clipped to the face.
/// Front and back faces carry `x` along the width and `y` up the height, ends `x`
/// across the depth, and the top and bottom (loose boards lying on the ground)
/// `x` along the width and `y` across the depth.
fn add_mark(group: &mut Node, p: &TimberPart, mark_index: usize, mark: &TimberMark) {
    let mut rng = Random::new(f64::from(mark.seed));
    let end = matches!(mark.face, TimberFace::Left | TimberFace::Right);
    let cap = matches!(mark.face, TimberFace::Top | TimberFace::Bottom);
    let sign = if matches!(
        mark.face,
        TimberFace::Left | TimberFace::Back | TimberFace::Bottom
    ) {
        -1.0
    } else {
        1.0
    };
    // Scars stay on the flat of the face, inside the edge chamfers.
    let bevel = member_bevel(DVec3::new(p.w, p.h, p.d));
    let span = if end { p.d } else { p.w } - bevel * 2.0;
    let rise = if cap { p.d } else { p.h } - bevel * 2.0;
    let x = clamp(mark.x, -span / 2.0 + 0.01, span / 2.0 - 0.01);
    // Keep the scar close to impact while avoiding a half-mark clipped along a seam.
    let margin = (rise * 0.3).min(0.13 * mark.size);
    let y = clamp(mark.y, -rise / 2.0 + margin, rise / 2.0 - margin);
    let mut plane = |points: &[[f64; 2]], color: u32, layer: f64| {
        // Clip scars at the piece edges; no cracks floating beyond the wood.
        let outline: Vec<DVec2> = points
            .iter()
            .map(|point| {
                let u = clamp(x + point[0], -span / 2.0 + 0.005, span / 2.0 - 0.005);
                let v = clamp(y + point[1], -rise / 2.0 + 0.005, rise / 2.0 - 0.005);
                if cap {
                    // Turning the plane onto the top maps its v to -z, onto the bottom to +z.
                    DVec2::new(u, -sign * v)
                } else {
                    DVec2::new((if end { -sign } else { sign }) * u, v)
                }
            })
            .collect();
        let mut mesh = shape_geometry(&[Shape::from_points(&outline)]);
        if cap {
            mesh.rotate_x(-sign * PI / 2.0);
        } else {
            mesh.rotate_y(if end {
                (sign * PI) / 2.0
            } else if sign < 0.0 {
                PI
            } else {
                0.0
            });
        }
        let offset = 0.003 + mark_index as f64 * 0.001 + layer * 0.001;
        let lift = |half: f64| sign * (half + offset);
        let (px, py, pz) = if end {
            (lift(p.w / 2.0), 0.0, 0.0)
        } else if cap {
            (0.0, lift(p.h / 2.0), 0.0)
        } else {
            (0.0, 0.0, lift(p.d / 2.0))
        };
        put(
            group,
            Node::mesh(Arc::new(mesh), material(color, 0.05, 0.65)),
            px,
            py,
            pz,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::timber_layout::{TimberWall, timber_parts};

    fn drawables(node: &Node) -> usize {
        let mut count = 0;
        node.traverse(glam::DMat4::IDENTITY, &mut |child, _| {
            count += usize::from(child.drawable.is_some());
        });
        count
    }

    #[test]
    fn snapped_members_draw_their_splinters_with_their_wood() {
        let wall = TimberWall {
            x: 0.0,
            z: 0.0,
            w: 6.0,
            h: 2.2,
            d: 0.5,
            color: 0x8a5a36,
            hits: &[],
            join: None,
        };
        let parts = timber_parts(&wall, 0);
        assert!(!parts.is_empty());
        for part in &parts {
            assert_eq!(
                drawables(&timber_debris_model(part)),
                drawables(&timber_part_model(part)),
                "a torn {:?} costs no extra draw",
                part.kind
            );
        }
    }
}
