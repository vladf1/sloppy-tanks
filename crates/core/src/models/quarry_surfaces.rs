//! Port of `quarry-surfaces.ts`: the layered sandstone material, ledge rocks,
//! merged rubble and the feathered sand footings around rock cover. The shading
//! is the renderer's ([`effects_scenery::SANDSTONE`], [`effects_scenery::SAND_DRIFT`]).

use std::f64::consts::PI;
use std::sync::{Arc, OnceLock};

use glam::{DVec3, Vec3};

use crate::geometry::math::{compose, js_sign, quat_from_euler, transform_point};
use crate::geometry::{
    Attribute, Mesh, VERTEX_ALPHA, icosahedron_geometry, octahedron_geometry, widen,
};
use crate::scene::{Effect, Material, Node, TextureRef, Wrap};

use super::effects_scenery::{GRIT, SAND_DRIFT, SANDSTONE, SOIL};
use super::model_primitives::{Cache, shadow_receiver, shadowed};
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
static FOOTINGS: Cache<([u64; 2], u32), Mesh> = Cache::new();

/// Collision ring levels of `quarry_rock_shape`, toe to cap rim, and its sides.
const SHAPE_LEVELS: [f64; 7] = [0.0, 0.07, 0.28, 0.44, 0.72, 0.88, 1.0];
const SHAPE_SIDES: usize = 16;
/// Target thickness of one sediment bed, in metres.
const BED_THICKNESS: f64 = 0.42;
/// Target width of one column of the eroded face, in metres.
const COLUMN_WIDTH: f64 = 0.8;

/// The face beds climb to this fraction of the collision levels; the shoulder
/// above steps back in terraces.
const FACE_TOP: f64 = 0.86;
/// Rocks narrower than this, in metres, are boulders: no terraces.
const BOULDER_SPAN: f64 = 3.0;

/// One ring of the eroded face: its outline at level `reach`, its height at level
/// `rise` (a tread steps back without rising), the bed it ends, its outward offset
/// in units of relief, and wear.
struct Row {
    reach: f64,
    rise: f64,
    bed: usize,
    offset: f64,
    wear: f64,
    tread: bool,
}

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

/// `sandstoneRock(w, h, d, variant)`: a weathered sandstone outcrop around the
/// collision shape (`quarry_rock_shape`). Its beds alternate between jutting
/// caprock ledges and recessed soft layers a few centimetres either side of the
/// collider, broken by vertical joints and fallen blocks, under a domed, eroded
/// cap. `uv.x` names each bed for the shader's strata palette and `uv.y` carries
/// the chipped-edge/cavity wear; the shader ignores real UVs.
pub fn sandstone_rock(w: f64, h: f64, d: f64, variant: u32) -> Node {
    let key = ([w, h, d].map(f64::to_bits), variant);
    let mesh = ROCKS.get_or_insert(key, || eroded_rock(w, h, d, variant));
    shadowed(mesh, sandstone_material())
}

fn eroded_rock(w: f64, h: f64, d: f64, variant: u32) -> Mesh {
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
    let perimeter: f64 = (0..SHAPE_SIDES)
        .map(|s| grid[SHAPE_SIDES + s].distance(grid[SHAPE_SIDES + (s + 1) % SHAPE_SIDES]))
        .sum();
    let split = ((perimeter / SHAPE_SIDES as f64 / COLUMN_WIDTH).round() as usize).clamp(1, 4);
    let columns = SHAPE_SIDES * split;
    // A point of the collision surface at a column and a level fraction.
    let surface = |column: usize, t: f64| -> DVec3 {
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
    let relief = (0.045 * h).min(0.05 * w.min(d)).clamp(0.02, 0.16);
    // Beds: uneven thicknesses, hard caprock never twice in a row. The face beds
    // climb to the shoulder, which steps back in terraces to the cap.
    // Boulders and loose blocks stay rounded: only outcrops step into terraces.
    let outcrop = w.max(d) >= BOULDER_SPAN;
    let face_top = if outcrop { FACE_TOP } else { 1.0 };
    let beds = ((h * face_top / BED_THICKNESS).round() as usize).clamp(1, 11);
    let terraces = if !outcrop {
        0
    } else if h > 1.5 {
        2
    } else {
        1
    };
    let weights: Vec<f64> = (0..beds).map(|_| rng.range(0.55, 1.45)).collect();
    let total: f64 = weights.iter().sum();
    let mut kinds = Vec::with_capacity(beds);
    for k in 0..beds {
        let roll = rng.next();
        let kind = if k + 1 == beds || (roll < 0.4 && kinds.last() != Some(&Bed::Hard)) {
            Bed::Hard
        } else if roll < 0.7 {
            Bed::Medium
        } else {
            Bed::Soft
        };
        kinds.push(kind);
    }
    let layer_base = f64::from(1 + (variant * 5) % 61);
    let mut rows: Vec<Row> = Vec::new();
    let mut level = 0.0;
    for k in 0..beds {
        let thickness = weights[k] / total * face_top;
        for (u, offset, wear) in bed_rows(kinds[k]) {
            // The buried toe keeps its collision footprint.
            let (offset, wear) = if k == 0 && u == 0.0 {
                (0.0, 0.0)
            } else {
                (offset, wear)
            };
            let t = level + u * thickness;
            rows.push(Row {
                reach: t,
                rise: t,
                bed: k,
                offset,
                wear,
                tread: false,
            });
        }
        level += thickness;
    }
    // Shoulder terraces: a flat tread back to the next level, then its riser.
    for step in 0..terraces {
        let from = level;
        let to = FACE_TOP + (1.0 - FACE_TOP) * (step + 1) as f64 / terraces as f64;
        let bed = beds + step;
        rows.push(Row {
            reach: to,
            rise: from,
            bed: bed - 1,
            offset: -0.4,
            wear: -0.8,
            tread: true,
        });
        rows.push(Row {
            reach: to,
            rise: to,
            bed,
            offset: 0.3,
            wear: 1.0,
            tread: false,
        });
        level = to;
    }
    let strata = beds + terraces;
    // Vertical joints split the beds into blocks; some blocks have fallen out.
    let joints: Vec<usize> = (0..(columns / 9).max(1))
        .map(|_| (rng.next() * columns as f64) as usize % columns)
        .collect();
    let mut hollow = vec![0.0f64; strata * columns];
    for k in 0..beds {
        for _ in 0..(columns / 12) {
            if rng.next() < 0.55 {
                let start = (rng.next() * columns as f64) as usize;
                let span = 1 + (rng.next() * 3.0) as usize;
                let depth = rng.range(0.7, 1.3);
                for c in start..start + span {
                    hollow[k * columns + c % columns] = depth;
                }
            }
        }
    }
    let chips: Vec<f64> = (0..rows.len() * columns)
        .map(|_| rng.range(-0.3, 0.3))
        .collect();
    let shades: Vec<f64> = (0..strata + 2).map(|_| rng.range(0.92, 1.06)).collect();
    // Each bed erodes back in its own lobes, so ledges wander in and out of step
    // with the beds above and below instead of stacking as contour lines.
    let lobes: Vec<(f64, f64, f64)> = (0..strata)
        .map(|_| {
            (
                rng.range(2.0, 5.0).floor(),
                rng.range(0.0, PI * 2.0),
                rng.range(0.5, 1.1),
            )
        })
        .collect();
    let mut points = Vec::with_capacity(rows.len() * columns);
    let mut wears = Vec::with_capacity(rows.len() * columns);
    for (r, row) in rows.iter().enumerate() {
        for c in 0..columns {
            let toe = r == 0;
            let hollowed = hollow[row.bed * columns + c];
            let (count, phase, depth) = lobes[row.bed];
            let angle = c as f64 / columns as f64 * PI * 2.0;
            let lobe = ((angle * count + phase).sin() * 0.5 - 0.5) * depth;
            let mut push = row.offset - hollowed + chips[r * columns + c] + lobe;
            let mut worn = row.wear - hollowed * 0.6;
            if joints.contains(&c) && !toe {
                push -= 1.25;
                worn = -1.0;
            }
            if toe {
                push = 0.0;
            }
            let reach = surface(c, row.reach);
            let y = surface(c, row.rise).y;
            points.push(DVec3::new(reach.x, y, reach.z) + outward[c] * push * relief);
            wears.push(worn.clamp(-1.0, 1.0));
        }
    }
    let mut positions: Vec<f64> = Vec::new();
    let mut uvs: Vec<f64> = Vec::new();
    let mut colors: Vec<[f32; 3]> = Vec::new();
    let mut corner = |p: DVec3, layer: f64, wear: f64, shade: f64| {
        positions.extend([p.x, p.y, p.z]);
        uvs.extend([layer, wear]);
        let dust = 0.88 + 0.12 * (p.y / h).clamp(0.0, 1.0);
        colors.push([
            (shade * dust * 1.01) as f32,
            (shade * dust) as f32,
            (shade * dust * 0.99) as f32,
        ]);
    };
    let mut band = |a: &[DVec3], wa: &[f64], b: &[DVec3], wb: &[f64], bed: usize| {
        let layer = layer_base + bed as f64;
        for c in 0..a.len() {
            let n = (c + 1) % a.len();
            for (p, w) in [
                (a[c], wa[c]),
                (b[c], wb[c]),
                (a[n], wa[n]),
                (a[n], wa[n]),
                (b[c], wb[c]),
                (b[n], wb[n]),
            ] {
                corner(p, layer, w, shades[bed]);
            }
        }
    };
    for r in 0..rows.len() - 1 {
        let (lower, upper) = (&rows[r], &rows[r + 1]);
        // A ledge between beds belongs to whichever bed juts further; a tread is
        // the top of the bed below it.
        let bed = if lower.bed == upper.bed || upper.tread || lower.offset > upper.offset {
            lower.bed
        } else {
            upper.bed
        };
        let (a, b) = (r * columns, (r + 1) * columns);
        band(
            &points[a..a + columns],
            &wears[a..a + columns],
            &points[b..b + columns],
            &wears[b..b + columns],
            bed,
        );
    }
    // The cap: two more low terraces stepping in toward the crown, each a flat
    // tread and a short riser.
    let mut ring: Vec<DVec3> = points[points.len() - columns..].to_vec();
    let mut ring_wear: Vec<f64> = wears[wears.len() - columns..].to_vec();
    let mut bed = strata - 1;
    let level_y = ring.iter().map(|p| p.y).sum::<f64>() / columns as f64;
    let steps: &[(f64, f64)] = if outcrop {
        &[(0.38, 0.05), (0.68, 0.04)]
    } else {
        &[]
    };
    for &(reach, rise) in steps {
        let lift = h * rise;
        // Smooth, low-frequency wander keeps the terrace outlines organic
        // without star-shaped creases, and the crown settles toward level.
        let phase = rng.range(0.0, PI * 2.0);
        let lobes = rng.range(2.0, 4.0).floor();
        let tread: Vec<DVec3> = ring
            .iter()
            .enumerate()
            .map(|(c, p)| {
                let angle = c as f64 / columns as f64 * PI * 2.0;
                let wander = (angle * lobes + phase).sin() * 0.04;
                let q = *p + (center - *p) * (reach + wander);
                DVec3::new(q.x, p.y, q.z)
            })
            .collect();
        let riser: Vec<DVec3> = tread
            .iter()
            .map(|p| {
                let settled = p.y + (level_y - p.y) * 0.9 + lift;
                DVec3::new(p.x, settled.max(p.y + lift * 0.4), p.z)
            })
            .collect();
        let cavity = vec![-0.6; columns];
        let lip = vec![0.8; columns];
        band(&ring, &ring_wear, &tread, &cavity, bed);
        bed += 1;
        band(&tread, &cavity, &riser, &lip, bed);
        ring = riser;
        ring_wear = lip;
    }
    let crown = ring.iter().map(|p| p.y).sum::<f64>() / columns as f64;
    let peak = if outcrop {
        DVec3::new(center.x, crown, center.z)
    } else {
        DVec3::new(center.x, crown.max(center.y) + h * 0.05, center.z)
    };
    let layer = layer_base + bed as f64;
    for c in 0..columns {
        let n = (c + 1) % columns;
        corner(peak, layer, 0.0, shades[bed]);
        corner(ring[n], layer, ring_wear[n], shades[bed]);
        corner(ring[c], layer, ring_wear[c], shades[bed]);
    }
    // The cap's vertices: after every face band, before any scree.
    let cap = (rows.len() - 1) * columns * 6..positions.len() / 3;
    let mut geometry = Mesh::from_f64(&positions, &[], &uvs, None);
    geometry.colors = colors;
    if outcrop {
        // Scree shed by the face lies around the toe in the same draw.
        let toe: Vec<DVec3> = (0..columns).map(|c| surface(c, 0.0)).collect();
        let scree = stone_mesh(&scree_stones(&toe, &outward, h, &mut rng));
        geometry.positions.extend(&scree.positions);
        geometry.uvs.extend(&scree.uvs);
        geometry.colors.extend(&scree.colors);
    }
    let mut geometry = geometry.to_creased_normals(ROCK_CREASE);
    if outcrop {
        // Shade the terraced cap as one weathered dome: its low risers still
        // step the silhouette, but its facets no longer crumple in the light.
        for normal in &mut geometry.normals[cap] {
            let bent = Vec3::from(*normal) * (1.0 - CAP_SOFTEN) + Vec3::Y * CAP_SOFTEN;
            *normal = bent.normalize_or_zero().to_array();
        }
    }
    geometry
}

/// How far the cap's normals lean toward straight up.
const CAP_SOFTEN: f32 = 0.7;

/// Stones per metre of an outcrop's toe.
const SCREE_DENSITY: f64 = 2.2;

/// Scree at an outcrop's toe: fallen blocks leaning on the face, then chips that
/// thin out and shrink across the sand apron. Ankle-high, so it never reads as
/// cover; the apron's drift half-buries the stones nearest the face.
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
                rng.next().powf(1.8) * 1.5 + 0.05
            };
            let fade = 1.0 - reach / 1.6;
            let size = if block {
                rng.range(0.4, 0.75).min(h * 0.25)
            } else {
                rng.range(0.09, 0.32) * (0.55 + fade * 0.6)
            };
            let p = at + out * (reach + size * 0.35);
            RubbleStone {
                x: p.x,
                y: 0.04 + 0.1 * fade,
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

/// `sandstone_rubble` for gravel a few pixels wide: 8-triangle chips that
/// receive shadows but cast none.
pub fn sandstone_pebbles(stones: &[RubbleStone]) -> Node {
    static CHIP: OnceLock<Mesh> = OnceLock::new();
    let chip = CHIP.get_or_init(|| octahedron_geometry(0.5, 0).to_non_indexed());
    shadow_receiver(Arc::new(stones_from(chip, stones)), sandstone_material())
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

/// `sandstoneFooting(w, d, variant)`: a low, feathered sand apron around a rock's
/// footprint; cosmetic sediment, never tall enough to imply cover. Its outer ring
/// fades out through its [`VERTEX_ALPHA`]. Receives shadows only.
pub fn sandstone_footing(w: f64, d: f64, variant: u32) -> Node {
    let key = ([w, d].map(f64::to_bits), variant);
    let mesh = FOOTINGS.get_or_insert(key, || {
        let shape = quarry_rock_shape(w, 1.0, d, variant).positions;
        let mut rng = Random::new(f64::from(variant) + 451.0);
        let sides = 16u32;
        let extensions: Vec<f64> = (0..sides).map(|_| rng.range(0.5, 1.5)).collect();
        let mut positions = Vec::with_capacity(48 * 3);
        let mut uvs = Vec::with_capacity(48 * 2);
        let mut colors = Vec::with_capacity(48);
        let mut alpha = Vec::with_capacity(48);
        let mut indices = Vec::new();
        for ring in 0..3u32 {
            for side in 0..sides {
                let x = f64::from(shape[side as usize * 3]);
                let z = f64::from(shape[side as usize * 3 + 2]);
                let extension = if ring == 0 {
                    -0.35
                } else {
                    extensions[side as usize] * if ring == 1 { 0.45 } else { 1.0 }
                };
                let px = x + js_sign(x) * extension;
                let pz = z + js_sign(z) * extension;
                let y = match ring {
                    0 => 0.18,
                    1 => 0.055,
                    _ => 0.018,
                };
                positions.extend([px, y, pz]);
                uvs.extend([px / 8.0, pz / 8.0]);
                let shade = if ring == 0 { 0.82 } else { 1.0 };
                colors.push([shade as f32; 3]);
                alpha.push(if ring == 2 { 0.0 } else { 1.0 });
                if ring < 2 {
                    let a = ring * sides + side;
                    let b = ring * sides + (side + 1) % sides;
                    indices.extend([a, b, a + sides, b, b + sides, a + sides]);
                }
            }
        }
        let mut geometry = Mesh::from_f64(&positions, &[], &uvs, Some(indices));
        geometry.colors = colors;
        geometry.set_attribute(Attribute::vertex(VERTEX_ALPHA, 1, alpha));
        geometry.compute_vertex_normals();
        geometry
    });
    shadow_receiver(mesh, sand_drift_material())
}
