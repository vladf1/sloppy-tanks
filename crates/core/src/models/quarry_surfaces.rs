//! Port of `quarry-surfaces.ts`: the layered sandstone material, ledge rocks,
//! merged rubble and the feathered sand footings around rock cover. The shading
//! is the renderer's ([`effects_scenery::SANDSTONE`], [`effects_scenery::SAND_DRIFT`]).

use std::f64::consts::PI;
use std::sync::{Arc, OnceLock};

use glam::DVec3;

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

/// Quarried rocks by size, variant and what they stand on.
static ROCKS: Cache<([u64; 3], u32, RockSetting), Mesh> = Cache::new();
/// Sand skirts of rocks by footprint and variant.
static SKIRTS: Cache<([u64; 2], u32), Mesh> = Cache::new();

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
/// How far, as a fraction of the face, a block's partings step up or down from
/// its neighbours'.
const BED_STEP: f64 = 0.045;
/// Rocks narrower than this, in metres, are boulders: one or two beds, no kit.
const BOULDER_SPAN: f64 = 3.0;
/// Added to `uv.y` (wear) on a sawn, drilled face, for the shader's drill
/// traces. A vertex attribute would keep the rock from merging with its footing.
pub(super) const SAWN: f64 = 3.0;
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
pub fn sandstone_rock(w: f64, h: f64, d: f64, variant: u32, setting: RockSetting) -> Node {
    shadowed(
        sandstone_rock_mesh(w, h, d, variant, setting),
        sandstone_material(),
    )
}

/// What a sandstone rock stands on, which decides where its scree rests.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RockSetting {
    /// Bare ground level with the rock's base.
    Ground,
    /// Its own sand skirt ([`sandstone_footing_height`]; cover rocks).
    Drift,
    /// Another rock: a slab with no scree or skirt, since stones shed around its
    /// toe would hang in the air, and an undercut base ([`STACKED_UNDERCUT`]) so
    /// that toe fits on the lower rock's cap while its upper beds jut out.
    Stacked,
}

/// The toe outline of a [`sandstone_rock`], in the rock's own space: where it
/// meets what it stands on.
pub fn sandstone_rock_toe(
    w: f64,
    h: f64,
    d: f64,
    variant: u32,
    setting: RockSetting,
) -> Vec<DVec3> {
    let mut grid = rock_grid(w, h, d, variant, setting);
    grid.truncate(SHAPE_SIDES);
    grid
}

/// The cached mesh of a [`sandstone_rock`].
pub fn sandstone_rock_mesh(
    w: f64,
    h: f64,
    d: f64,
    variant: u32,
    setting: RockSetting,
) -> Arc<Mesh> {
    let key = ([w, h, d].map(f64::to_bits), variant, setting);
    ROCKS.get_or_insert(key, || quarried_rock(w, h, d, variant, setting))
}

/// How far a stacked slab's lowest collision rings are drawn in toward its axis,
/// toe first: an undercut soft base under the jutting beds above.
const STACKED_UNDERCUT: [f64; 4] = [0.64, 0.68, 0.88, 0.97];

/// The collision shape's rings (`SHAPE_SIDES` points each, toe first) and its cap
/// centre last; a stacked slab's lowest rings are undercut.
fn rock_grid(w: f64, h: f64, d: f64, variant: u32, setting: RockSetting) -> Vec<DVec3> {
    let shape = quarry_rock_shape(w, h, d, variant);
    let mut grid: Vec<DVec3> = shape
        .positions
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| widen(*p))
        .collect();
    if setting == RockSetting::Stacked {
        for (ring, scale) in STACKED_UNDERCUT.into_iter().enumerate() {
            for p in &mut grid[ring * SHAPE_SIDES..(ring + 1) * SHAPE_SIDES] {
                p.x *= scale;
                p.z *= scale;
            }
        }
    }
    grid
}

fn quarried_rock(w: f64, h: f64, d: f64, variant: u32, setting: RockSetting) -> Mesh {
    let grid = rock_grid(w, h, d, variant, setting);
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
    // Outcrops weather deeper than boulders: ledges jut and undercuts bite.
    let relief = if outcrop {
        (0.08 * h.min(w.min(d))).clamp(0.03, 0.36)
    } else {
        (0.065 * h.min(w.min(d))).clamp(0.03, 0.3)
    };

    // Courses: a few massive beds with thin partings; the kits vary the stack.
    let face_top = if outcrop { FACE_TOP } else { 1.0 };
    let beds = ((h * face_top / BED_THICKNESS).round() as usize).clamp(1, 4);
    let mut courses: Vec<(f64, Course)> = (0..beds)
        .map(|_| (rng.range(0.45, 1.75), Course::Massive))
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
                // Some outcrops keep a harder bed on top, jutting over the
                // softer stone it shelters.
                if kit != Kit::Sawn && beds >= 2 && rng.next() < 0.6 {
                    let last = courses.len() - 1;
                    courses[last].1 = Course::Caprock;
                    courses[last].0 *= 0.7;
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
    let origin = DVec3::new(center.x, 0.0, center.z);

    // Joint planes: a systematic set of near-parallel joints cuts across the long
    // axis and splits the caprock into slabs. Each joint is snapped to the outline
    // columns it leaves by, so slabs, faces and the notches down them share
    // vertices and no gap opens where they meet.
    let top_ring: Vec<DVec3> = (0..columns).map(|c| surface(c, FACE_TOP)).collect();
    let mut joint_ends: Vec<(usize, usize)> = Vec::new();
    if outcrop {
        let long_axis = if w >= d { DVec3::X } else { DVec3::Z };
        let long = w.max(d);
        let count = if kit == Kit::Split {
            ((long / 4.0).round() as usize).clamp(2, 3)
        } else {
            ((long / 5.0).round() as usize).clamp(1, 3)
        };
        let set_angle = rng.range(-0.25, 0.25);
        for i in 0..count {
            let spacing = long / (count + 1) as f64;
            let s = -long / 2.0 + spacing * (i + 1) as f64 + rng.range(-0.3, 0.3) * spacing;
            let angle = set_angle + rng.range(-0.3, 0.3);
            let (sin, cos) = angle.sin_cos();
            let normal = DVec3::new(
                long_axis.x * cos - long_axis.z * sin,
                0.0,
                long_axis.x * sin + long_axis.z * cos,
            );
            let side = |c: usize| (top_ring[c] - origin).dot(normal) - s;
            let mut ends = Vec::new();
            for c in 0..columns {
                let n = (c + 1) % columns;
                let (a, b) = (side(c), side(n));
                if (a < 0.0) != (b < 0.0) {
                    ends.push(if a.abs() <= b.abs() { c } else { n });
                }
            }
            let clear = |e: usize| {
                joint_ends.iter().all(|&(a, b)| {
                    [a, b].iter().all(|&o| {
                        let gap = (e as isize - o as isize).rem_euclid(columns as isize);
                        gap.min(columns as isize - gap) >= 2
                    })
                })
            };
            if let [a, b] = ends[..]
                && a != b
                && clear(a)
                && clear(b)
            {
                joint_ends.push((a.min(b), a.max(b)));
            }
        }
    }
    let joint_column = |c: usize| joint_ends.iter().any(|&(a, b)| a == c || b == c);

    // Slabs: each outline segment (column c to c + 1) belongs to the slab on its
    // side of every joint.
    let joint_lines: Vec<(DVec3, DVec3)> = joint_ends
        .iter()
        .map(|&(a, b)| {
            let along = top_ring[b] - top_ring[a];
            (
                top_ring[a],
                DVec3::new(-along.z, 0.0, along.x).normalize_or_zero(),
            )
        })
        .collect();
    let slab_of_point = |p: DVec3| -> usize {
        joint_lines
            .iter()
            .enumerate()
            .map(|(i, &(at, normal))| usize::from((p - at).dot(normal) > 0.0) << i)
            .sum()
    };
    let segment_slab: Vec<usize> = (0..columns)
        .map(|c| slab_of_point((top_ring[c] + top_ring[(c + 1) % columns]) * 0.5))
        .collect();
    let slab_ids: Vec<usize> = {
        let mut ids = segment_slab.clone();
        ids.sort_unstable();
        ids.dedup();
        ids
    };
    let slab_count = slab_ids.iter().max().map_or(1, |&m| m + 1);

    // Each slab's top is a plane: its own height, tipped a few degrees, so the
    // slabs catch the light differently and step up or down at every joint.
    let mut slab_base = vec![0.0f64; slab_count];
    let mut slab_tilt = vec![DVec3::ZERO; slab_count];
    let mut slab_anchor = vec![origin; slab_count];
    let plane = |base: &[f64], tilt: &[DVec3], anchor: &[DVec3], slab: usize, p: DVec3| {
        base[slab] + tilt[slab].dot(DVec3::new(p.x, 0.0, p.z) - anchor[slab])
    };
    {
        let tilt_limit = if outcrop { 0.06 } else { 0.03 };
        for &slab in &slab_ids {
            let members: Vec<usize> = (0..columns).filter(|&c| segment_slab[c] == slab).collect();
            let crown =
                members.iter().map(|&c| surface(c, 1.0).y).sum::<f64>() / members.len() as f64;
            let anchor = members.iter().map(|&c| top_ring[c]).sum::<DVec3>() / members.len() as f64;
            slab_anchor[slab] = DVec3::new(anchor.x, 0.0, anchor.z);
            slab_tilt[slab] = DVec3::new(
                rng.range(-tilt_limit, tilt_limit),
                0.0,
                rng.range(-tilt_limit, tilt_limit),
            );
            slab_base[slab] = crown * rng.range(0.93, 0.985);
        }
        // Step every joint by at least `step` at both of its ends.
        let step = h * if kit == Kit::Split { 0.09 } else { 0.06 };
        // A slab lowered for one joint can close another: settle a few times.
        for &(a, b) in joint_ends.iter().cycle().take(joint_ends.len() * 3) {
            let left = segment_slab[(a + columns - 1) % columns];
            let right = segment_slab[a];
            let (low, high) = if slab_base[left] <= slab_base[right] {
                (left, right)
            } else {
                (right, left)
            };
            let gap = [top_ring[a], top_ring[b]]
                .iter()
                .map(|&e| {
                    plane(&slab_base, &slab_tilt, &slab_anchor, high, e)
                        - plane(&slab_base, &slab_tilt, &slab_anchor, low, e)
                })
                .fold(f64::INFINITY, f64::min);
            if gap < step {
                slab_base[low] -= step - gap;
            }
        }
    }
    let slab_height = |slab: usize, p: DVec3| plane(&slab_base, &slab_tilt, &slab_anchor, slab, p);

    // Blocks down the faces: runs of columns split by vertical joints, broken at
    // every joint plane and between them; each stands proud or sits back as a
    // unit with one flat face.
    let mut cuts_at: Vec<usize> = joint_ends.iter().flat_map(|&(a, b)| [a, b]).collect();
    if cuts_at.is_empty() {
        cuts_at.push((rng.next() * columns as f64) as usize % columns);
    }
    cuts_at.sort_unstable();
    let mut block_of = vec![0usize; columns];
    let mut joints = vec![false; columns];
    let mut deep_joint = vec![false; columns];
    let mut block_push = Vec::new();
    let mut block_bed_push = Vec::new();
    // Each block's partings sit a little higher or lower than its neighbours':
    // the beds step at the joints instead of running round the rock as one line.
    let mut block_bed_shift: Vec<f64> = Vec::new();
    let mut block_starts = Vec::new();
    for (i, &from) in cuts_at.iter().enumerate() {
        let to = cuts_at[(i + 1) % cuts_at.len()];
        let mut length = (to + columns - from) % columns;
        if length == 0 {
            length = columns;
        }
        let mut at = 0;
        while at < length {
            let mut run = if outcrop {
                4 + (rng.next() * 5.0) as usize
            } else {
                length
            };
            if length - at - run.min(length - at) < 2 {
                run = length - at;
            }
            let start = (from + at) % columns;
            let block = block_push.len();
            for k in 0..run {
                block_of[(start + k) % columns] = block;
            }
            block_starts.push((start, run));
            joints[start] = outcrop;
            deep_joint[start] =
                outcrop && (joint_column(start) || (kit == Kit::Split && rng.next() < 0.5));
            block_push.push(rng.range(-0.45, 0.45));
            block_bed_push.push(
                (0..courses.len())
                    .map(|_| rng.range(-0.2, 0.2))
                    .collect::<Vec<f64>>(),
            );
            // One shift for all of a block's partings, so no thin bed inverts.
            block_bed_shift.push(if outcrop {
                rng.range(-BED_STEP, BED_STEP)
            } else {
                0.0
            });
            at += run;
        }
    }
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
    // The arris round the top: a narrow chamfer, bitten deep where a corner of
    // the caprock has spalled off.
    let arris = if outcrop {
        (0.035 * w.min(d)).clamp(0.05, 0.2)
    } else {
        (0.16 * w.min(d)).clamp(0.05, 0.4)
    };
    let mut spall = vec![0.0f64; columns];
    for c in 0..columns {
        if outcrop && !sawn[c] && rng.next() < 0.22 {
            let bite = rng.range(2.0, 3.6);
            spall[c] = spall[c].max(bite);
            spall[(c + 1) % columns] = spall[(c + 1) % columns].max(bite * rng.range(0.3, 0.7));
            spall[(c + columns - 1) % columns] =
                spall[(c + columns - 1) % columns].max(bite * rng.range(0.2, 0.6));
        }
    }
    // Each slab weathered its own arris: some edges crisp, some worn round.
    let slab_arris: Vec<f64> = (0..slab_count)
        .map(|_| if outcrop { rng.range(0.45, 1.9) } else { 1.0 })
        .collect();
    let chamfer: Vec<(f64, f64)> = (0..columns)
        .map(|c| {
            let bite = 1.0 + spall[c];
            let width = arris * rng.range(0.75, 1.25) * bite * slab_arris[segment_slab[c]];
            let drop = arris * rng.range(0.7, 1.15) * bite.powf(0.8);
            (width, if outcrop { drop } else { drop * 1.6 })
        })
        .collect();
    // Where each column's face tops out: under the lower slab at a joint, so the
    // joint opens as a notch down the face.
    let column_top: Vec<f64> = (0..columns)
        .map(|c| {
            let before = slab_height(segment_slab[(c + columns - 1) % columns], top_ring[c]);
            let after = slab_height(segment_slab[c], top_ring[c]);
            before.min(after) - chamfer[c].1
        })
        .collect();

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
            let block = block_of[c];
            let level = |k: usize| {
                let base = boundaries[k];
                if k == 0 || k == courses.len() {
                    base
                } else {
                    (base + tilt * face_top + block_bed_shift[block]).clamp(0.02, face_top - 0.02)
                }
            };
            let (from, to) = (level(row.course), level(row.course + 1));
            let t = from + (to - from) * row.u;
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
                push -= if deep_joint[c] { JOINT_NOTCH } else { 1.0 };
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
            points.push(p);
            wears.push(worn.clamp(-1.0, 1.0));
            cuts.push(cut);
        }
    }
    // The top bed climbs or settles to meet its slab, column by column.
    let top_row = rows.len() - 1;
    let course_start = rows
        .iter()
        .position(|row| row.course == last_course)
        .unwrap_or(0)
        .max(1)
        - 1;
    for c in 0..columns {
        let y0 = points[course_start * columns + c].y;
        let y1 = points[top_row * columns + c].y;
        let target = column_top[c].max(y0 + (y1 - y0) * 0.35);
        let scale = if y1 - y0 > 1e-6 {
            (target - y0) / (y1 - y0)
        } else {
            1.0
        };
        for r in course_start + 1..=top_row {
            let p = &mut points[r * columns + c];
            p.y = y0 + (p.y - y0) * scale;
        }
    }
    // Each block's face is one flat plane per row: struck faces, not a rounded loaf.
    for &(start, run) in &block_starts {
        if run < 3 || !outcrop {
            continue;
        }
        let normal = (0..=run)
            .map(|k| outward[(start + k) % columns])
            .sum::<DVec3>()
            .normalize_or_zero();
        for r in 1..rows.len() {
            let inner: Vec<usize> = (1..run)
                .map(|k| (start + k) % columns)
                .filter(|&c| !sawn[c])
                .collect();
            if inner.len() < 2 {
                continue;
            }
            let mean = inner
                .iter()
                .map(|&c| points[r * columns + c].dot(normal))
                .sum::<f64>()
                / inner.len() as f64;
            for &c in &inner {
                let p = &mut points[r * columns + c];
                let off = mean - p.dot(normal);
                *p += DVec3::new(normal.x, 0.0, normal.z) * off * BLOCK_FLATNESS;
            }
        }
    }

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
    for r in 0..rows.len() - 1 {
        let course = rows[r + 1].course;
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
    // The caprock: one plane per slab, inside the chamfer round the top of the
    // face, stepping down at each joint.
    let cap_layer = layer_base + (courses.len() + 1) as f64;
    let shade = shades[last_course];
    let rim = |c: usize, slab: usize| {
        let p = points[top_row * columns + c] - outward[c] * chamfer[c].0;
        DVec3::new(p.x, slab_height(slab, p), p.z)
    };
    let rim_wear = |c: usize| if spall[c] > 0.0 { 1.0 } else { 0.45 };
    let face_top = top_row * columns;
    for c in 0..columns {
        let n = (c + 1) % columns;
        let slab = segment_slab[c];
        // The chamfer: face top to the slab's rim.
        corner(
            points[face_top + c],
            cap_layer,
            wears[face_top + c].max(0.3),
            shade,
            false,
        );
        corner(rim(c, slab), cap_layer, rim_wear(c), shade, false);
        corner(
            points[face_top + n],
            cap_layer,
            wears[face_top + n].max(0.3),
            shade,
            false,
        );
        corner(
            points[face_top + n],
            cap_layer,
            wears[face_top + n].max(0.3),
            shade,
            false,
        );
        corner(rim(c, slab), cap_layer, rim_wear(c), shade, false);
        corner(rim(n, slab), cap_layer, rim_wear(n), shade, false);
        // Where two slabs meet at a column, close the notch between their rims.
        let before = segment_slab[(c + columns - 1) % columns];
        if before != slab {
            let (high, low) = if slab_height(before, top_ring[c]) > slab_height(slab, top_ring[c]) {
                (before, slab)
            } else {
                (slab, before)
            };
            let mut tri = [points[face_top + c], rim(c, high), rim(c, low)];
            let toward = slab_anchor[low] - slab_anchor[high];
            if (tri[1] - tri[0]).cross(tri[2] - tri[0]).dot(toward) < 0.0 {
                tri.swap(1, 2);
            }
            for (k, p) in tri.into_iter().enumerate() {
                corner(p, cap_layer, if k == 2 { -0.8 } else { 0.4 }, shade, false);
            }
        }
    }
    for &slab in &slab_ids {
        // The slab's outline in ring order: its own arcs of the rim, closed by the
        // joints it shares with its neighbours.
        let outline: Vec<usize> = (0..columns)
            .filter(|&c| {
                segment_slab[c] == slab || segment_slab[(c + columns - 1) % columns] == slab
            })
            .collect();
        let rims: Vec<DVec3> = outline.iter().map(|&c| rim(c, slab)).collect();
        let middle = rims.iter().sum::<DVec3>() / rims.len() as f64;
        let swell = if outcrop {
            h * rng.range(-0.012, 0.018)
        } else {
            h * 0.07
        };
        let crown = DVec3::new(middle.x, slab_height(slab, middle) + swell, middle.z);
        // Rims along a joint sit under the step of a higher neighbour: in its shade.
        let wear_of = |c: usize| {
            if joint_column(c) {
                let other = if segment_slab[c] == slab {
                    segment_slab[(c + columns - 1) % columns]
                } else {
                    segment_slab[c]
                };
                if slab_height(other, top_ring[c]) > slab_height(slab, top_ring[c]) {
                    -0.8
                } else {
                    0.2
                }
            } else {
                rim_wear(c)
            }
        };
        for i in 0..rims.len() {
            let j = (i + 1) % rims.len();
            let mut tri = [crown, rims[j], rims[i]];
            let mut wear = [0.05, wear_of(outline[j]), wear_of(outline[i])];
            if (tri[1] - tri[0]).cross(tri[2] - tri[0]).y < 0.0 {
                tri.swap(1, 2);
                wear.swap(1, 2);
            }
            for (p, w) in tri.into_iter().zip(wear) {
                corner(p, cap_layer, w, shade, false);
            }
        }
    }
    // The step at each joint: a low wall from the higher slab's edge down to the
    // lower one, fresh along its top and shaded at its foot.
    for &(a, b) in &joint_ends {
        let one = segment_slab[(a + columns - 1) % columns];
        let two = segment_slab[a];
        let (high, low) = if slab_height(one, top_ring[a]) > slab_height(two, top_ring[a]) {
            (one, two)
        } else {
            (two, one)
        };
        let quad = [rim(a, high), rim(b, high), rim(b, low), rim(a, low)];
        let toward = slab_anchor[low] - slab_anchor[high];
        let facing = (quad[1] - quad[0]).cross(quad[3] - quad[0]).dot(toward);
        let order: [usize; 6] = if facing >= 0.0 {
            [0, 1, 3, 3, 1, 2]
        } else {
            [0, 3, 1, 1, 3, 2]
        };
        for k in order {
            let wear = if k < 2 { 0.25 } else { -1.0 };
            corner(quad[k], cap_layer, wear, shade * 0.97, false);
        }
    }
    let mut geometry = Mesh::from_f64(&positions, &[], &uvs, None);
    geometry.colors = colors;
    if outcrop && setting != RockSetting::Stacked {
        // The sand skirt, then scree and fallen blocks resting on it.
        let skirt = skirt_mesh(w, d, variant);
        let banked = mesh_triangles(&skirt);
        let rest = |x: f64, z: f64| surface_height(&banked, x, z).unwrap_or(0.0).max(0.0);
        let toe: Vec<DVec3> = (0..columns).map(|c| surface(c, 0.0)).collect();
        let mut stones = scree_stones(&toe, &outward, h, &rest, &mut rng);
        for c in 0..columns {
            let fell = chipped[c] > 0.0
                || chipped.get(columns + c).is_some_and(|&bite| bite > 0.0)
                || broken_by(c) > 0.5
                || spall[c] > 2.5;
            if fell && rng.next() < 0.55 {
                let size = rng.range(0.55, 1.0).min(h * 0.24);
                let p = toe[c] + outward[c] * (size * 0.55 + rng.range(0.0, 0.4));
                let height = size * rng.range(0.6, 0.85);
                stones.push(RubbleStone {
                    x: p.x,
                    y: rest(p.x, p.z) + height * STONE_SEAT,
                    z: p.z,
                    w: size,
                    h: height,
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
        geometry.positions.extend(&skirt.positions);
        geometry.uvs.extend(&skirt.uvs);
        geometry.colors.extend(&skirt.colors);
    }
    geometry.to_creased_normals(ROCK_CREASE)
}

/// How deep a joint plane notches the faces it leaves by, in units of relief: a
/// narrow crack, not a pinch between lumps.
const JOINT_NOTCH: f64 = 1.7;
/// How far each block's face is drawn onto one plane (1 = flat).
const BLOCK_FLATNESS: f64 = 1.0;

/// Stones per metre of an outcrop's toe.
const SCREE_DENSITY: f64 = 1.3;

/// Scree at an outcrop's toe: fallen blocks leaning on the face, then chips that
/// thin out and shrink across the sand apron. Ankle-high, so it never reads as
/// cover; the skirt half-buries the stones nearest the face. The shed stones
/// rest on what `rest` (x, z) reads as the surface there.
fn scree_stones(
    toe: &[DVec3],
    outward: &[DVec3],
    h: f64,
    rest: &impl Fn(f64, f64) -> f64,
    rng: &mut Random,
) -> Vec<RubbleStone> {
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
            let height = size * rng.range(0.4, 0.75);
            RubbleStone {
                x: p.x,
                y: rest(p.x, p.z) + height * STONE_SEAT,
                z: p.z,
                w: size,
                h: height,
                d: size * rng.range(0.6, 1.1),
                rot_y: rng.range(-PI, PI),
                shade: rng.range(0.72, 1.0),
            }
        })
        .collect()
}

/// Sand banked against an outcrop's toe: a low skirt from the face out onto the
/// floor, sinking just under it at its rim. The shader shades it as the floor
/// (`uv.y` [`SKIRT`]), so it needs no blending and joins the rock's draw. It
/// follows the collision toe, which depends on the footprint alone.
fn skirt_mesh(w: f64, d: f64, variant: u32) -> Arc<Mesh> {
    SKIRTS.get_or_insert(([w, d].map(f64::to_bits), variant), || {
        let grid = rock_grid(w, 1.0, d, variant, RockSetting::Ground);
        // Two columns per side of the outline, so the drift swells smoothly.
        let toe: Vec<DVec3> = (0..SHAPE_SIDES * 2)
            .map(|c| {
                let (a, b) = (grid[c / 2], grid[(c / 2 + 1) % SHAPE_SIDES]);
                a + (b - a) * (c % 2) as f64 * 0.5
            })
            .collect();
        let outward: Vec<DVec3> = (0..toe.len())
            .map(|c| {
                let tangent = toe[(c + 1) % toe.len()] - toe[(c + toe.len() - 1) % toe.len()];
                let n = DVec3::new(tangent.z, 0.0, -tangent.x).normalize_or_zero();
                if n.dot(DVec3::new(toe[c].x, 0.0, toe[c].z)) < 0.0 {
                    -n
                } else {
                    n
                }
            })
            .collect();
        let mut rng = Random::new(1907.0 + f64::from(variant) * 3.0);
        sand_skirt(&toe, &outward, &mut rng)
    })
}

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
        for triangle in positions.as_chunks_mut::<9>().0 {
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

/// A [`RubbleStone`]'s centre stands this many of its heights above the surface it
/// rests on. Its squashed underside reaches 0.126 to 0.21 heights below the centre
/// ([`stones_from`]), so every stone touches or sinks slightly into that surface.
pub const STONE_SEAT: f64 = 0.12;

/// The sand skirt of a cover rock (in the rock's space) at x/z, or the ground's
/// level (0) beyond it: where loose stones around a cover rock rest.
pub fn sandstone_footing_height(w: f64, d: f64, variant: u32, x: f64, z: f64) -> f64 {
    surface_height(&mesh_triangles(&skirt_mesh(w, d, variant)), x, z)
        .unwrap_or(0.0)
        .max(0.0)
}

/// Every triangle of `mesh`, indexed or not, for [`surface_height`].
pub fn mesh_triangles(mesh: &Mesh) -> Vec<[DVec3; 3]> {
    let corner = |i: u32| widen(mesh.positions[i as usize]);
    match &mesh.indices {
        Some(indices) => indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| t.map(corner))
            .collect(),
        None => mesh
            .positions
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| t.map(widen))
            .collect(),
    }
}

/// The highest point of `triangles` straight above or below x/z, if any.
pub fn surface_height(triangles: &[[DVec3; 3]], x: f64, z: f64) -> Option<f64> {
    triangles
        .iter()
        .filter_map(|&[a, b, c]| {
            let det = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
            if det.abs() < 1e-12 {
                return None;
            }
            let u = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / det;
            let v = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / det;
            (u >= 0.0 && v >= 0.0 && u + v <= 1.0)
                .then_some(u * a.y + v * b.y + (1.0 - u - v) * c.y)
        })
        .reduce(f64::max)
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
