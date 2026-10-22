//! Port of `quarry-surfaces.ts`: the layered sandstone material, ledge rocks,
//! merged rubble and the feathered sand footings around rock cover. The shading
//! is the renderer's ([`effects_scenery::SANDSTONE`], [`effects_scenery::SAND_DRIFT`]).

use std::f64::consts::PI;
use std::sync::{Arc, OnceLock};

use glam::{DVec3, Vec3};

use crate::geometry::math::{compose, quat_from_euler, transform_point};
use crate::geometry::{Mesh, icosahedron_geometry, octahedron_geometry, widen};
use crate::scene::{Effect, Material, Node, TextureRef, Wrap};

use super::effects_scenery::{GRIT, SAND_DRIFT, SANDSTONE, SOIL};
use super::model_primitives::{Cache, shadowed};
use super::quarry_terrain::{quarry_grit_texture, quarry_soil_texture};
use crate::sim::math::Random;
use crate::sim::quarry_rock_shape::quarry_rock_shape;

pub const SANDSTONE_TEXTURE: &str = "textures/quarry/sandstone.webp";
/// Normals crease above this angle, keeping ledges sharp and shoulders round.
const ROCK_CREASE: f64 = PI / 5.0;

/// `sandstoneMaterial()`: one shared material for every rock, bench and rubble
/// mesh. `map` is the sandstone photo the effect samples triplanar in world space.
pub fn sandstone_material() -> Arc<Material> {
    static MATERIAL: OnceLock<Arc<Material>> = OnceLock::new();
    MATERIAL
        .get_or_init(|| {
            Arc::new(Material {
                map: Some(TextureRef {
                    wrap: Wrap::Mirror,
                    anisotropy: 4,
                    ..TextureRef::file(SANDSTONE_TEXTURE)
                }),
                vertex_colors: true,
                effect: Effect::Custom {
                    name: SANDSTONE,
                    params: Vec::new(),
                },
                extra_textures: vec![(SOIL, quarry_soil_texture()), (GRIT, quarry_grit_texture())],
                ..Material::standard(0xffffff, 0.0, 0.95)
            })
        })
        .clone()
}

static ROCKS: Cache<([u64; 3], u32), Mesh> = Cache::new();

/// Collision ring levels of `quarry_rock_shape`, toe to cap rim, and its sides.
const SHAPE_LEVELS: [f64; 7] = [0.0, 0.07, 0.28, 0.44, 0.72, 0.88, 1.0];
const SHAPE_SIDES: usize = 16;
/// Target thickness of one massive sandstone bed, in metres.
const BED_THICKNESS: f64 = 1.25;
/// Target width of one column of the eroded face, in metres.
const COLUMN_WIDTH: f64 = 1.0;
/// The face beds climb to this fraction of the collision levels; the rounded
/// shoulder above leans back to the cap.
const FACE_TOP: f64 = 0.88;
/// Rocks narrower than this, in metres, are boulders: one or two beds, no kit.
const BOULDER_SPAN: f64 = 3.0;
/// Added to `uv.y` (wear) on a sawn, drilled face, for the shader's drill
/// traces. A vertex attribute would keep the rock from merging with its footing.
const SAWN: f64 = 3.0;
/// `uv.y` of the sand skirt the shader shades as the floor around it.
const SKIRT: f64 = -3.0;

/// How a bed weathers: hard caprock juts out as a ledge over an undercut, soft
/// beds recess between them.
#[derive(Clone, Copy, PartialEq)]
pub enum Bed {
    Hard,
    Medium,
    Soft,
}

/// Rows of one bed, bottom to top: (fraction of the bed, outward offset in
/// units of the rock's relief, wear). Wear is the shader's chipped-edge (+1)
/// or cavity (-1) term, carried in `uv.y`.
pub fn bed_rows(bed: Bed) -> [(f64, f64, f64); 4] {
    match bed {
        Bed::Hard => [
            (0.0, -0.55, -1.0),
            (0.22, 0.85, 0.0),
            (0.78, 1.0, 0.25),
            (1.0, 0.72, 1.0),
        ],
        Bed::Medium => [
            (0.0, -0.25, -0.6),
            (0.2, 0.15, 0.0),
            (0.8, 0.25, 0.1),
            (1.0, 0.1, 0.6),
        ],
        Bed::Soft => [
            (0.0, -0.65, -0.7),
            (0.25, -0.5, -0.35),
            (0.75, -0.45, -0.3),
            (1.0, -0.55, -0.2),
        ],
    }
}

/// The quarried-sandstone kits the outcrops are cut from. Every kit keeps to its
/// collision shape; they differ in how the beds part and the blocks break.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kit {
    /// Massive beds with one face sawn flat and drilled when the block was cut.
    Sawn,
    /// Blocks parted by deep joints, standing at different heights.
    Split,
    /// A hard caprock overhanging softer, undercut beds.
    Capped,
    /// Beds dipping across the block, one corner broken away onto the sand.
    Tilted,
}

fn rock_kit(variant: u32) -> Kit {
    match (variant / 3) % 4 {
        0 => Kit::Sawn,
        1 => Kit::Split,
        2 => Kit::Capped,
        _ => Kit::Tilted,
    }
}

/// How one bed of an outcrop's face weathers.
#[derive(Clone, Copy, PartialEq)]
enum Course {
    /// Thick, massive sandstone: near-vertical, a parting notch at its base and a
    /// rounded lip.
    Massive,
    /// Soft, thin-bedded stone recessed under the beds above.
    Recessed,
    /// Hard caprock jutting out over an undercut.
    Caprock,
}

/// Rows of one course, bottom to top: (fraction of the bed, outward offset in
/// relief units, wear).
fn course_rows(course: Course) -> [(f64, f64, f64); 4] {
    match course {
        Course::Massive => [
            (0.0, -0.7, -0.9),
            (0.07, 0.1, -0.1),
            (0.84, 0.3, 0.15),
            (1.0, 0.0, 0.85),
        ],
        Course::Recessed => [
            (0.0, -0.6, -0.6),
            (0.3, -0.85, -0.5),
            (0.75, -0.8, -0.45),
            (1.0, -0.6, -0.3),
        ],
        Course::Caprock => [
            (0.0, -0.6, -1.0),
            (0.1, 1.05, 0.0),
            (0.86, 1.2, 0.2),
            (1.0, 0.8, 1.0),
        ],
    }
}

/// `sandstoneRock(w, h, d, variant)`: a quarried sandstone outcrop around the
/// collision shape (`quarry_rock_shape`), from one of four kits (sawn and
/// drilled, split by joints, caprocked, tilted). A few massive beds part along
/// thin notches; vertical joints break them into blocks that stand proud, sit
/// back and top out at their own heights; corners are chipped and the fallen
/// pieces lie at the toe with the scree, on a sand skirt shaded like the floor.
/// `uv.x` names each bed for the shader's strata palette and `uv.y` carries the
/// chipped-edge/cavity wear, plus `SAWN` on sawn faces.
pub fn sandstone_rock(w: f64, h: f64, d: f64, variant: u32) -> Node {
    let key = ([w, h, d].map(f64::to_bits), variant);
    let mesh = ROCKS.get_or_insert(key, || quarried_rock(w, h, d, variant));
    shadowed(mesh, sandstone_material())
}

fn quarried_rock(w: f64, h: f64, d: f64, variant: u32) -> Mesh {
    let shape = quarry_rock_shape(w, h, d, variant);
    let grid: Vec<DVec3> = shape
        .positions
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| widen(*p))
        .collect();
    let center = grid[grid.len() - 1];
    let mut rng = Random::new(812.0 + f64::from(variant) * 7.0 + 0.5);
    let outcrop = w.max(d) >= BOULDER_SPAN;
    let kit = rock_kit(variant);
    let perimeter: f64 = (0..SHAPE_SIDES)
        .map(|s| grid[SHAPE_SIDES + s].distance(grid[SHAPE_SIDES + (s + 1) % SHAPE_SIDES]))
        .sum();
    let split = ((perimeter / SHAPE_SIDES as f64 / COLUMN_WIDTH).round() as usize).clamp(1, 3);
    let columns = SHAPE_SIDES * split;
    // A point of the collision surface at a column and a level fraction.
    let surface = |column: usize, t: f64| -> DVec3 {
        let t = t.clamp(0.0, 1.0);
        let ring = SHAPE_LEVELS
            .windows(2)
            .position(|pair| t <= pair[1])
            .unwrap_or(SHAPE_LEVELS.len() - 2);
        let f = ((t - SHAPE_LEVELS[ring]) / (SHAPE_LEVELS[ring + 1] - SHAPE_LEVELS[ring]))
            .clamp(0.0, 1.0);
        let side = column / split;
        let along = (column % split) as f64 / split as f64;
        let point = |ring: usize| {
            let a = grid[ring * SHAPE_SIDES + side];
            let b = grid[ring * SHAPE_SIDES + (side + 1) % SHAPE_SIDES];
            a + (b - a) * along
        };
        point(ring) + (point(ring + 1) - point(ring)) * f
    };
    // Horizontal outward normal of each column, from the outline at mid height.
    let outward: Vec<DVec3> = (0..columns)
        .map(|c| {
            let before = surface((c + columns - 1) % columns, 0.4);
            let after = surface((c + 1) % columns, 0.4);
            let tangent = after - before;
            let mut n = DVec3::new(tangent.z, 0.0, -tangent.x).normalize_or_zero();
            let here = surface(c, 0.4);
            if n.dot(DVec3::new(here.x, 0.0, here.z)) < 0.0 {
                n = -n;
            }
            n
        })
        .collect();
    let relief = (0.065 * h.min(w.min(d))).clamp(0.03, 0.3);

    // Courses: a few massive beds with thin partings; the kits vary the stack.
    let face_top = if outcrop { FACE_TOP } else { 1.0 };
    let beds = ((h * face_top / BED_THICKNESS).round() as usize).clamp(1, 4);
    let mut courses: Vec<(f64, Course)> = (0..beds)
        .map(|_| (rng.range(0.6, 1.5), Course::Massive))
        .collect();
    if outcrop {
        match kit {
            Kit::Capped => {
                // Soft beds undercut beneath a thick caprock.
                courses = vec![
                    (rng.range(0.9, 1.2), Course::Recessed),
                    (rng.range(0.25, 0.4), Course::Recessed),
                    (rng.range(0.8, 1.1), Course::Massive),
                    (rng.range(0.7, 0.9), Course::Caprock),
                ];
            }
            Kit::Split | Kit::Tilted | Kit::Sawn => {
                // A thin recessive parting somewhere in the middle of the stack.
                if beds >= 2 {
                    let at = 1 + (rng.next() * (beds - 1) as f64) as usize;
                    courses.insert(at, (rng.range(0.18, 0.3), Course::Recessed));
                }
            }
        }
    }
    let total: f64 = courses.iter().map(|(weight, _)| weight).sum();
    let mut boundaries = vec![0.0];
    for (weight, _) in &courses {
        boundaries.push(boundaries.last().unwrap() + weight / total * face_top);
    }
    // Tilted beds: the partings dip across the block.
    let dip = if outcrop && kit == Kit::Tilted {
        let angle = rng.range(0.0, PI * 2.0);
        DVec3::new(angle.cos(), 0.0, angle.sin()) * rng.range(0.06, 0.1)
    } else {
        DVec3::ZERO
    };
    let extent = w.max(d) / 2.0;

    // Blocks: runs of columns split by vertical joints; each block stands proud
    // or sits back as a unit and tops out at its own height.
    let mut block_of = vec![0usize; columns];
    let mut joints = vec![false; columns];
    let mut block_push = Vec::new();
    let mut block_top = Vec::new();
    let mut block_bed_push = Vec::new();
    let start = (rng.next() * columns as f64) as usize;
    let mut c = 0;
    while c < columns {
        let run = if outcrop {
            3 + (rng.next() * 4.0) as usize
        } else {
            columns
        };
        let block = block_push.len();
        for k in c..(c + run).min(columns) {
            block_of[(k + start) % columns] = block;
        }
        joints[(c + start) % columns] = outcrop;
        block_push.push(rng.range(-0.35, 0.35));
        let drop = match kit {
            Kit::Split => rng.range(-0.09, 0.0),
            _ => rng.range(-0.05, 0.0),
        };
        block_top.push(if outcrop { drop * h } else { 0.0 });
        block_bed_push.push(
            (0..courses.len())
                .map(|_| rng.range(-0.2, 0.2))
                .collect::<Vec<f64>>(),
        );
        c += run;
    }
    let deep_joint: Vec<bool> = (0..columns)
        .map(|c| joints[c] && kit == Kit::Split && rng.next() < 0.6)
        .collect();
    // The sawn face: columns facing one way, cut flat through every bed.
    let sawn_side = {
        let angle = (rng.next() * 4.0).floor() * PI / 2.0 + 0.3;
        DVec3::new(angle.cos(), 0.0, angle.sin())
    };
    let sawn: Vec<bool> = outward
        .iter()
        .map(|n| outcrop && kit == Kit::Sawn && n.dot(sawn_side) > 0.72)
        .collect();
    // Chipped corners: columns at the outline's corners lose a bite per bed.
    let mut chipped = vec![0.0f64; courses.len() * columns];
    if outcrop {
        for c in (0..columns).step_by(split * 2) {
            for k in 0..courses.len() {
                if !sawn[c] && rng.next() < 0.4 {
                    let bite = rng.range(1.2, 2.2);
                    chipped[k * columns + c] = bite;
                    if split > 1 && rng.next() < 0.5 {
                        chipped[k * columns + (c + 1) % columns] = bite * 0.5;
                    }
                }
            }
        }
    }
    // The tilted kit's broken corner: the upper beds of one corner gone.
    let broken = if outcrop && kit == Kit::Tilted {
        Some((rng.next() * 8.0).floor() as usize * split * 2)
    } else {
        None
    };
    let broken_by = |c: usize| -> f64 {
        broken.map_or(0.0, |corner| {
            let distance = (c as isize - corner as isize).rem_euclid(columns as isize);
            let distance = distance.min(columns as isize - distance) as f64;
            (1.0 - distance / (split as f64 * 1.6)).max(0.0)
        })
    };

    // Rows of the face, bottom to top.
    struct Row {
        course: usize,
        u: f64,
        offset: f64,
        wear: f64,
    }
    let mut rows = Vec::new();
    for (k, &(_, course)) in courses.iter().enumerate() {
        for (u, offset, wear) in course_rows(course) {
            // Rows shared with the bed below are only emitted once.
            if k > 0 && u == 0.0 {
                // The parting notch sits just above the previous bed's lip.
                rows.push(Row {
                    course: k,
                    u: 0.04,
                    offset,
                    wear,
                });
                continue;
            }
            rows.push(Row {
                course: k,
                u,
                offset,
                wear,
            });
        }
    }
    let chips: Vec<f64> = (0..rows.len() * columns)
        .map(|_| rng.range(-0.15, 0.15))
        .collect();
    let mut points = Vec::with_capacity(rows.len() * columns);
    let mut wears = Vec::with_capacity(rows.len() * columns);
    let mut cuts = Vec::with_capacity(rows.len() * columns);
    let last_course = courses.len() - 1;
    for (r, row) in rows.iter().enumerate() {
        for c in 0..columns {
            let toe = r == 0;
            let local = surface(c, 0.5);
            let tilt = dip.dot(DVec3::new(local.x, 0.0, local.z)) / extent;
            let level = |k: usize| {
                let base = boundaries[k];
                if k == 0 || k == courses.len() {
                    base
                } else {
                    (base + tilt * face_top).clamp(0.02, face_top - 0.02)
                }
            };
            let (from, to) = (level(row.course), level(row.course + 1));
            let t = from + (to - from) * row.u;
            let block = block_of[c];
            let mut push = row.offset
                + block_push[block]
                + block_bed_push[block][row.course]
                + chips[r * columns + c];
            let mut worn = row.wear + block_push[block].max(0.0) * 0.4;
            let bite = chipped[row.course * columns + c];
            if bite > 0.0 {
                push -= bite;
                worn = 1.0;
            }
            let lost = broken_by(c) * f64::from(u8::from(row.course + 2 > courses.len()));
            if lost > 0.0 {
                push -= lost * 3.2;
                worn = worn.max(lost);
            }
            if joints[c] {
                push -= if deep_joint[c] { 3.2 } else { 1.1 };
                worn = -1.0;
            }
            let cut = sawn[c] && !joints[c];
            if cut {
                // Cut straight through the beds: only the partings show, as seams.
                push = row.offset.min(0.0) * 0.25;
                worn = row.wear.min(0.0) * 0.4;
            }
            if toe {
                push = 0.0;
            }
            let mut p = surface(c, t);
            p += outward[c] * push * relief;
            // The top bed's blocks top out at their own heights.
            if row.course == last_course && row.u > 0.5 {
                p.y += block_top[block] * (row.u - 0.5) * 2.0;
            }
            points.push(p);
            wears.push(worn.clamp(-1.0, 1.0));
            cuts.push(cut);
        }
    }
    // The shoulder: a weathered lip leaning back to the cap, then the cap's
    // outer ring, each column at its block's height.
    let lip_row = rows.len();
    for (reach, rise, inset, wear) in [(0.95, 0.95, 0.0, 0.7), (1.0, 1.0, 0.3, 0.25)] {
        for c in 0..columns {
            let block = block_of[c];
            let mut p = surface(c, reach);
            p.y = surface(c, rise).y + block_top[block];
            p += (center - p) * DVec3::new(inset, 0.0, inset);
            let mut push = -0.2 + block_push[block] * 0.5;
            // Joints notch the lip only: notched cap rims crease the crown into a star.
            if joints[c] && inset == 0.0 {
                push -= if deep_joint[c] { 2.4 } else { 0.8 };
            }
            if broken_by(c) > 0.0 {
                p.y -= broken_by(c) * h * 0.08;
                push -= broken_by(c) * 2.4;
            }
            if sawn[c] && !joints[c] && inset == 0.0 {
                push = 0.0;
            }
            p += outward[c] * push * relief;
            points.push(p);
            wears.push(if joints[c] && inset == 0.0 {
                -0.6
            } else {
                wear
            });
            cuts.push(false);
        }
    }
    let ring_count = lip_row + 2;
    let crown_ring = &points[(ring_count - 1) * columns..];
    let crown = crown_ring.iter().map(|p| p.y).sum::<f64>() / columns as f64;
    let peak = if outcrop {
        DVec3::new(center.x, crown + h * 0.01, center.z)
    } else {
        DVec3::new(center.x, crown.max(center.y) + h * 0.05, center.z)
    };

    let shades: Vec<f64> = (0..courses.len() + 2)
        .map(|_| rng.range(0.93, 1.05))
        .collect();
    let layer_base = f64::from(1 + (variant * 5) % 61);
    let mut positions: Vec<f64> = Vec::new();
    let mut uvs: Vec<f64> = Vec::new();
    let mut colors: Vec<[f32; 3]> = Vec::new();
    let mut corner = |p: DVec3, layer: f64, wear: f64, shade: f64, cut: bool| {
        positions.extend([p.x, p.y, p.z]);
        uvs.extend([layer, if cut { wear + SAWN } else { wear }]);
        let dust = 0.9 + 0.1 * (p.y / h).clamp(0.0, 1.0);
        let s = shade * dust;
        colors.push([(s * 1.01) as f32, s as f32, (s * 0.99) as f32]);
    };
    let course_of_row = |r: usize| rows.get(r).map_or(last_course + 1, |row| row.course);
    for r in 0..ring_count - 1 {
        // A band belongs to the course of its upper row, except the lip band
        // (the parting notch belongs to the bed above it).
        let course = course_of_row(r + 1).min(courses.len() + 1);
        let layer = layer_base + course as f64;
        let shade = shades[course.min(shades.len() - 1)];
        let (a, b) = (r * columns, (r + 1) * columns);
        for c in 0..columns {
            let n = (c + 1) % columns;
            for v in [a + c, b + c, a + n, a + n, b + c, b + n] {
                corner(points[v], layer, wears[v], shade, cuts[v]);
            }
        }
    }
    // The cap's vertices: after every face band, before any scree.
    let cap_start = (ring_count - 1) * columns * 6;
    let cap_layer = layer_base + (courses.len() + 1) as f64;
    let ring = (ring_count - 1) * columns;
    // An inner ring halfway to the crown, swelling and sagging on its own, so the
    // top weathers into low hummocks and hollows instead of one smooth dome.
    let swell: Vec<f64> = (0..columns).map(|_| rng.range(-1.0, 1.0)).collect();
    let inner: Vec<(DVec3, f64)> = (0..columns)
        .map(|c| {
            let around =
                (swell[(c + columns - 1) % columns] + swell[c] * 2.0 + swell[(c + 1) % columns])
                    / 4.0;
            let outer = points[ring + c];
            let q = outer + (center - outer) * CAP_INNER;
            let lift = if outcrop { h * CAP_SWELL * around } else { 0.0 };
            (
                DVec3::new(q.x, (outer.y + peak.y) * 0.5 + lift, q.z),
                around * 0.5,
            )
        })
        .collect();
    let shade = shades[last_course];
    for c in 0..columns {
        let n = (c + 1) % columns;
        let (inner_c, wear_c) = inner[c];
        let (inner_n, wear_n) = inner[n];
        corner(inner_c, cap_layer, wear_c, shade, false);
        corner(points[ring + n], cap_layer, wears[ring + n], shade, false);
        corner(points[ring + c], cap_layer, wears[ring + c], shade, false);
        corner(inner_c, cap_layer, wear_c, shade, false);
        corner(inner_n, cap_layer, wear_n, shade, false);
        corner(points[ring + n], cap_layer, wears[ring + n], shade, false);
        corner(peak, cap_layer, 0.1, shade, false);
        corner(inner_n, cap_layer, wear_n, shade, false);
        corner(inner_c, cap_layer, wear_c, shade, false);
    }
    let cap = cap_start..cap_start + columns * 9;
    let mut geometry = Mesh::from_f64(&positions, &[], &uvs, None);
    geometry.colors = colors;
    if outcrop {
        // Scree and fallen blocks at the toe, then the sand skirt.
        let toe: Vec<DVec3> = (0..columns).map(|c| surface(c, 0.0)).collect();
        let mut stones = scree_stones(&toe, &outward, h, &mut rng);
        for c in 0..columns {
            let fell = chipped[c] > 0.0
                || chipped.get(columns + c).is_some_and(|&bite| bite > 0.0)
                || broken_by(c) > 0.5;
            if fell && rng.next() < 0.55 {
                let size = rng.range(0.55, 1.0).min(h * 0.24);
                let p = toe[c] + outward[c] * (size * 0.55 + rng.range(0.0, 0.4));
                stones.push(RubbleStone {
                    x: p.x,
                    y: size * 0.12,
                    z: p.z,
                    w: size,
                    h: size * rng.range(0.6, 0.85),
                    d: size * rng.range(0.7, 1.0),
                    rot_y: rng.range(-PI, PI),
                    shade: rng.range(0.85, 1.02),
                });
            }
        }
        let scree = scatter_mesh(&stones);
        geometry.positions.extend(&scree.positions);
        geometry.uvs.extend(&scree.uvs);
        geometry.colors.extend(&scree.colors);
        let skirt = sand_skirt(&toe, &outward, &mut rng);
        geometry.positions.extend(&skirt.positions);
        geometry.uvs.extend(&skirt.uvs);
        geometry.colors.extend(&skirt.colors);
    }
    let mut geometry = geometry.to_creased_normals(ROCK_CREASE);
    // Shade the cap as one weathered dome: its facets would crumple in the light.
    for normal in &mut geometry.normals[cap] {
        let bent = Vec3::from(*normal) * (1.0 - CAP_SOFTEN) + Vec3::Y * CAP_SOFTEN;
        *normal = bent.normalize_or_zero().to_array();
    }
    geometry
}

/// Where the cap's inner ring sits between its rim and the crown, and how far it
/// swells or sags, as a fraction of the outcrop's height.
const CAP_INNER: f64 = 0.45;
const CAP_SWELL: f64 = 0.035;

/// How far the cap's normals lean toward straight up.
const CAP_SOFTEN: f32 = 0.55;

/// Stones per metre of an outcrop's toe.
const SCREE_DENSITY: f64 = 1.3;

/// Scree at an outcrop's toe: fallen blocks leaning on the face, then chips that
/// thin out and shrink across the sand apron. Ankle-high, so it never reads as
/// cover; the skirt half-buries the stones nearest the face.
fn scree_stones(toe: &[DVec3], outward: &[DVec3], h: f64, rng: &mut Random) -> Vec<RubbleStone> {
    let perimeter: f64 = (0..toe.len())
        .map(|c| toe[c].distance(toe[(c + 1) % toe.len()]))
        .sum();
    let count = (perimeter * SCREE_DENSITY) as usize;
    (0..count)
        .map(|i| {
            let c = (rng.next() * toe.len() as f64) as usize % toe.len();
            let n = (c + 1) % toe.len();
            let at = toe[c] + (toe[n] - toe[c]) * rng.next();
            let out = (outward[c] + outward[n]).normalize_or_zero();
            let block = i % 9 == 0;
            let reach = if block {
                rng.range(0.05, 0.35)
            } else {
                rng.next().powf(1.8) * 1.6 + 0.1
            };
            let fade = 1.0 - reach / 1.8;
            let size = if block {
                rng.range(0.4, 0.75).min(h * 0.25)
            } else {
                rng.range(0.08, 0.3) * (0.55 + fade * 0.6)
            };
            let p = at + out * (reach + size * 0.35);
            RubbleStone {
                x: p.x,
                y: 0.03 + 0.08 * fade,
                z: p.z,
                w: size,
                h: size * rng.range(0.4, 0.75),
                d: size * rng.range(0.6, 1.1),
                rot_y: rng.range(-PI, PI),
                shade: rng.range(0.72, 1.0),
            }
        })
        .collect()
}

/// Sand banked against an outcrop's toe: a low skirt from the face out onto the
/// floor, sinking just under it at its rim. The shader shades it as the floor
/// (`uv.y` [`SKIRT`]), so it needs no blending and joins the rock's draw.
fn sand_skirt(toe: &[DVec3], outward: &[DVec3], rng: &mut Random) -> Mesh {
    let columns = toe.len();
    let reach: Vec<f64> = (0..columns).map(|_| rng.range(0.7, 1.6)).collect();
    // Smooth the reach so the drift swells and thins instead of zigzagging.
    let reach: Vec<f64> = (0..columns)
        .map(|c| {
            (reach[(c + columns - 1) % columns] + reach[c] * 2.0 + reach[(c + 1) % columns]) / 4.0
        })
        .collect();
    let ring = |c: usize, f: f64, y: f64| -> DVec3 {
        let p = toe[c] + outward[c] * (reach[c] * f - 0.12);
        DVec3::new(p.x, y, p.z)
    };
    let mut positions: Vec<f64> = Vec::new();
    for c in 0..columns {
        let n = (c + 1) % columns;
        for (f0, y0, f1, y1) in [(0.0, 0.2, 0.4, 0.07), (0.4, 0.07, 1.0, -0.012)] {
            for p in [
                ring(c, f0, y0),
                ring(n, f0, y0),
                ring(c, f1, y1),
                ring(c, f1, y1),
                ring(n, f0, y0),
                ring(n, f1, y1),
            ] {
                positions.extend([p.x, p.y, p.z]);
            }
        }
    }
    // Face up whichever way the outline winds.
    let corner =
        |i: usize| DVec3::new(positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]);
    if (corner(1) - corner(0)).cross(corner(2) - corner(0)).y < 0.0 {
        for triangle in positions.chunks_exact_mut(9) {
            let second: [f64; 3] = [triangle[3], triangle[4], triangle[5]];
            triangle.copy_within(6..9, 3);
            triangle[6..9].copy_from_slice(&second);
        }
    }
    let count = positions.len() / 3;
    let mut mesh = Mesh::from_f64(&positions, &[], &[], None);
    mesh.uvs = vec![[0.0, SKIRT as f32]; count];
    mesh.colors = vec![[1.0; 3]; count];
    mesh
}

/// One fragment of [`sandstone_rubble`], in world metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RubbleStone {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// Footprint and height, in metres.
    pub w: f64,
    pub h: f64,
    pub d: f64,
    pub rot_y: f64,
    /// Per-stone brightness; hue comes from the shared sandstone strata.
    pub shade: f64,
}

/// The strata palette entry (`uv.x`) of a loose stone: one bed of a rock.
pub fn rubble_layer(stone: usize) -> f32 {
    (1 + (stone * 7) % 37) as f32
}

/// `roughenStone(vertex, stone)`: push a unit stone's corner in or out by a
/// per-stone hash of that corner, so shared corners stay welded and no two stones
/// read as dice.
pub fn roughen_stone(vertex: DVec3, stone: f64) -> DVec3 {
    let hash = (stone * 12.9898 + vertex.x * 78.233 + vertex.y * 37.719 + vertex.z * 11.131).sin();
    let r = hash * 43758.5453 - (hash * 43758.5453).floor();
    vertex * (0.72 + r * 0.5)
}

/// `sandstoneRubble(stones)`: 20-triangle fragments merged into one sandstone mesh
/// (loose rubble and gravel where a full ledge rock would spend ten times the
/// triangles on stones a few pixels wide). Flat facets keep them angular.
pub fn sandstone_rubble(stones: &[RubbleStone]) -> Node {
    shadowed(Arc::new(stone_mesh(stones)), sandstone_material())
}

/// Stones narrower than this, in metres, are 8-triangle chips in a scatter; wider
/// ones keep the 20-triangle rubble shape.
const CHIP_SPAN: f64 = 0.42;

/// Loose scree, talus and spoil: chips for the small stones and rubble shapes for
/// the big ones, merged into one sandstone mesh that casts and receives shadows,
/// so it joins the scenery batch of its cell.
pub fn sandstone_scatter(stones: &[RubbleStone]) -> Node {
    shadowed(Arc::new(scatter_mesh(stones)), sandstone_material())
}

/// The merged mesh of [`sandstone_scatter`].
pub fn scatter_mesh(stones: &[RubbleStone]) -> Mesh {
    let (small, big): (Vec<RubbleStone>, Vec<RubbleStone>) =
        stones.iter().partition(|stone| stone.w < CHIP_SPAN);
    let mut mesh = chip_mesh(&small);
    let rubble = stone_mesh(&big);
    mesh.positions.extend(rubble.positions);
    mesh.normals.extend(rubble.normals);
    mesh.uvs.extend(rubble.uvs);
    mesh.colors.extend(rubble.colors);
    mesh
}

/// The merged 8-triangle chips of [`scatter_mesh`] and outcrop scree.
fn chip_mesh(stones: &[RubbleStone]) -> Mesh {
    static CHIP: OnceLock<Mesh> = OnceLock::new();
    stones_from(
        CHIP.get_or_init(|| octahedron_geometry(0.5, 0).to_non_indexed()),
        stones,
    )
}

/// The merged, flat-faceted stones of [`sandstone_rubble`].
fn stone_mesh(stones: &[RubbleStone]) -> Mesh {
    static PEBBLE: OnceLock<Mesh> = OnceLock::new();
    stones_from(
        PEBBLE.get_or_init(|| icosahedron_geometry(0.5, 0).to_non_indexed()),
        stones,
    )
}

/// One copy of the unit `pebble` per stone, roughened, placed and shaded.
fn stones_from(pebble: &Mesh, stones: &[RubbleStone]) -> Mesh {
    let per_stone = pebble.positions.len();
    let mut positions = Vec::with_capacity(stones.len() * per_stone);
    let mut uvs = Vec::with_capacity(stones.len() * per_stone);
    let mut colors = Vec::with_capacity(stones.len() * per_stone);
    for (s, stone) in stones.iter().enumerate() {
        // A pebble-specific tilt: a few stones sit on edge, most lie flat.
        let jitter = ((s * 7919) % 13) as f64;
        let rotation = quat_from_euler(
            (jitter - 6.0) * 0.05,
            stone.rot_y,
            (((jitter * 5.0) % 13.0) - 6.0) * 0.04,
        );
        let matrix = compose(
            DVec3::new(stone.x, stone.y, stone.z),
            rotation,
            DVec3::new(stone.w, stone.h, stone.d),
        );
        for (v, corner) in pebble.positions.iter().enumerate() {
            let mut vertex = roughen_stone(widen(*corner), s as f64);
            // Squash the lower half so stones rest on, and sink into, the ground.
            if vertex.y < 0.0 {
                vertex.y *= 0.35;
            }
            let vertex = transform_point(&matrix, vertex);
            positions.push([vertex.x as f32, vertex.y as f32, vertex.z as f32]);
            // Each stone is a chip of one bed, freshly broken along its edges.
            uvs.push([rubble_layer(s), 0.35]);
            // Facet-level variation plus darker undersides where stones meet soil.
            let facet = 0.94 + (((v / 3) * 37 + s * 11) % 9) as f64 * 0.015;
            let under = if vertex.y < stone.y { 0.82 } else { 1.0 };
            let shade = stone.shade * facet * under;
            colors.push([shade as f32, shade as f32, (shade * 0.98) as f32]);
        }
    }
    let mut geometry = Mesh {
        positions,
        uvs,
        colors,
        ..Mesh::default()
    };
    geometry.compute_vertex_normals();
    geometry
}

/// The drift material of rock footings: soil-coloured sand with world-space grit,
/// blended over the ground ([`SAND_DRIFT`]).
pub fn sand_drift_material() -> Arc<Material> {
    static MATERIAL: OnceLock<Arc<Material>> = OnceLock::new();
    MATERIAL
        .get_or_init(|| {
            Arc::new(Material {
                map: Some(quarry_soil_texture()),
                vertex_colors: true,
                transparent: true,
                depth_write: false,
                // Keeps the drift above the coplanar floor.
                polygon_offset: Some((-1.0, -1.0)),
                effect: Effect::Custom {
                    name: SAND_DRIFT,
                    params: Vec::new(),
                },
                extra_textures: vec![(GRIT, quarry_grit_texture())],
                ..Material::standard(0xffffff, 0.0, 1.0)
            })
        })
        .clone()
}

/// `sandstoneFooting(w, d, variant)`: the sand around a rock's footprint. It is
/// now the rock's own skirt ([`sandstone_rock`]), shaded as the floor in the
/// rock's draw, so the footing is an empty group: one blended draw fewer per
/// rock.
pub fn sandstone_footing(_w: f64, _d: f64, _variant: u32) -> Node {
    Node::group("")
}

#[cfg(test)]
mod tmp_rock_probe {
    #[test]
    fn probe() {
        for [x, z, w, d, h] in [
            [25.0, 12.0, 14.0, 8.0, 4.6],
            [25.0, -12.0, 14.0, 8.0, 3.8],
            [4.0, 28.0, 16.0, 9.0, 4.8],
            [40.5, 37.0, 10.0, 12.0, 4.2],
            [24.5, 37.0, 10.0, 12.0, 3.6],
            [-10.0, 47.0, 13.0, 7.0, 3.4],
        ] {
            for side in [-1.0, 1.0] {
                let v = crate::sim::quarry_rock_shape::quarry_rock_variant(side * x, side * z);
                let m = super::quarried_rock(w, h, d, v);
                let bad = m
                    .positions
                    .iter()
                    .filter(|p| p.iter().any(|c| !c.is_finite()))
                    .count();
                let badn = m
                    .normals
                    .iter()
                    .filter(|p| p.iter().any(|c| !c.is_finite()))
                    .count();
                let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                for p in &m.positions {
                    for i in 0..3 {
                        lo[i] = lo[i].min(p[i]);
                        hi[i] = hi[i].max(p[i]);
                    }
                }
                println!(
                    "v{v} kit {:?} tris {} nan {bad} nan_n {badn} lo {lo:?} hi {hi:?}",
                    super::rock_kit(v),
                    m.positions.len() / 3
                );
            }
        }
    }
}
