//! Six tree families (pine, spruce, fir, oak, birch, aspen), seeded by position:
//! textured limbs and alpha-tested foliage cards (needle sprays on conifers, leaf
//! sprigs on broadleaves). The cards shade as one rounded crown: their normals lean
//! out of the crown and their vertex colors darken its interior and underside, so
//! a crown reads as a soft mass of leaves instead of the faceted lobes
//! `tree-models.ts` drew.
//!
//! A full-detail tree (a cover) is an unnamed group at the tree's x/z holding
//! [`tree_part::STUMP`] (batched bark and roots, then the hidden
//! [`tree_part::CUT_SURFACE`]) and [`tree_part::CROWN`]. The crown holds the
//! shedding boughs first (named by the damage stage that drops them, see
//! [`tree_part::BRANCH_STAGE_1`]), each centred on its own bounds, then the batched
//! trunk and foliage. A background tree is one unbatched group (itself named
//! `trunk-and-crown`, as in the TypeScript) without stump or boughs, and its
//! foliage does not sway: it bakes into world-space scenery.
//!
//! Proportions (family, height, stump) come from the simulation's
//! [`tree_proportions`], which also sizes the tree's collider and stump.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::{DMat4, DVec3};

use super::batching::{batch, paint_mesh};
use super::effects_scenery::FOLIAGE;
use super::model_primitives::{Cache, shadowed, span_between};
use crate::geometry::math::{
    hex_to_linear, lerp_color, multiply_hex, quat_from_euler, quat_from_euler_yxz, quat_rotate_z,
    smoothstep, transform_point,
};
use crate::geometry::{CylinderGeometry, Mesh, narrow, node_bounds};
use crate::scene::{Color, Effect, Material, Node, Side, TextureRef, TextureSource};
use crate::sim::math::Random;
use crate::sim::tree_proportions::{TreeProportions, tree_proportions};

/// Node names that presentation looks up on a full-detail tree.
pub mod tree_part {
    /// The trunk, boughs and foliage (TS `userData.crown`): hidden when felled.
    pub const CROWN: &str = "trunk-and-crown";
    /// The rooted stump group that stays after the tree is felled.
    pub const STUMP: &str = "rooted-stump";
    /// The splintered end grain on the stump (TS `userData.cutSurface`): shown when felled.
    pub const CUT_SURFACE: &str = "exposed-wood";
    /// A bough dropped at the first damage (TS `name = "shedding-branch"` with
    /// `userData.dropStage = 1`); the stage lives in the name here.
    pub const BRANCH_STAGE_1: &str = "shedding-branch-1";
    /// A bough dropped at 35% health (`dropStage = 2`).
    pub const BRANCH_STAGE_2: &str = "shedding-branch-2";
}

/// `TREE_FAMILIES`, indexed by [`TreeProportions::family`].
pub const TREE_FAMILIES: [&str; 6] = ["Pine", "Spruce", "Fir", "Oak", "Birch", "Aspen"];

/// Where a tree grows and its authored footprint and height (`Pick<Cover, ...>`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeShape {
    pub x: f64,
    pub z: f64,
    pub w: f64,
    pub d: f64,
    pub h: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeDetail {
    /// A cover tree: stump, boughs that shed, batched crown.
    Full,
    /// Distant scenery: fewer tiers, clumps and cards, no stump or bark limbs.
    /// Each tree's colour lives in its vertex colours, so a whole forest cell
    /// draws in four materials (bark, birch bark, needles, leaves).
    Background,
    /// Far woods on the ranges' feet and the valley's rim: a background tree
    /// with fewer whorls, clumps and cards, seen only across the valley.
    Distant,
}

impl TreeDetail {
    fn background(self) -> bool {
        self != TreeDetail::Full
    }
}

/// What a tree's crown sheds when it is hit or felled, for presentation's falling
/// leaves: needles or leaves, their average sRGB color, and the crown's height
/// range and horizontal radius above the tree's base.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeFoliage {
    pub conifer: bool,
    pub color: u32,
    pub bottom: f64,
    pub top: f64,
    pub radius: f64,
}

/// The foliage a tree at `c` grows (see [`TreeFoliage`]).
pub fn tree_foliage(c: &TreeShape) -> TreeFoliage {
    let TreeProportions { family, height, .. } = tree_proportions(c.x, c.z, c.w, c.d, c.h);
    let conifer = family < 3;
    TreeFoliage {
        conifer,
        color: foliage_color(family),
        bottom: height * if conifer { 0.25 } else { 0.42 },
        top: height * 0.92,
        radius: c.w.min(c.d) * 0.42,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TreeSurface {
    Bark,
    Birch,
    Rings,
    /// Broadleaf sprig cards (`leaf-sprigs.webp`, see [`OAK_CELLS`]).
    LeafSprigs,
    ConiferSpray,
}

impl TreeSurface {
    fn texture(self) -> &'static str {
        match self {
            TreeSurface::Bark => "textures/trees/bark.webp",
            TreeSurface::Birch => "textures/trees/birch.webp",
            TreeSurface::Rings => "textures/trees/rings.webp",
            TreeSurface::LeafSprigs => "textures/trees/leaf-sprigs.webp",
            TreeSurface::ConiferSpray => "textures/trees/conifer-spray.webp",
        }
    }

    fn foliage(self) -> bool {
        matches!(self, TreeSurface::LeafSprigs | TreeSurface::ConiferSpray)
    }
}

/// Foliage below this texture alpha is cut out.
const FOLIAGE_CUTOFF: f32 = 0.35;

/// Every tree surface side by side in one texture (`generate-tree-atlas.ts`), so a
/// tree's crown, each of its boughs and a whole cell of background trees draw in
/// one material: limbs, foliage and cut wood share its alpha-tested, double-sided
/// surface (bark is opaque in it). Width and height in texels.
const TREE_ATLAS: &str = "textures/trees/atlas.webp";
const TREE_ATLAS_SIZE: [f64; 2] = [1792.0, 512.0];
/// Texels kept clear of a column's edges, so filtering never reaches the next one.
const TREE_ATLAS_INSET: f64 = 1.5;
/// Bark relief over the whole atlas: the foliage takes a little of it too.
const TREE_ATLAS_BUMP: f32 = 0.035;

impl TreeSurface {
    /// Its column of [`TREE_ATLAS`]: left edge and width in texels, and the
    /// fraction of the column's height one tile of the surface fills (the 256 px
    /// surfaces stack twice in their 512 px column).
    fn atlas_column(self) -> (f64, f64, f64) {
        match self {
            TreeSurface::Bark => (0.0, 256.0, 0.5),
            TreeSurface::Birch => (256.0, 256.0, 0.5),
            TreeSurface::Rings => (512.0, 256.0, 0.5),
            TreeSurface::LeafSprigs => (768.0, 512.0, 1.0),
            TreeSurface::ConiferSpray => (1280.0, 512.0, 1.0),
        }
    }

    /// The surface whose own texture a material maps, if any.
    fn of(material: &Material) -> Option<Self> {
        let path = match &material.map.as_ref()?.source {
            TextureSource::File(path) => *path,
            TextureSource::Generated(_) => return None,
        };
        [
            TreeSurface::Bark,
            TreeSurface::Birch,
            TreeSurface::Rings,
            TreeSurface::LeafSprigs,
            TreeSurface::ConiferSpray,
        ]
        .into_iter()
        .find(|kind| kind.texture() == path)
    }
}

/// The atlas material, swaying like foliage or (background trees) still.
static ATLAS_SURFACES: Cache<bool, Material> = Cache::new("tree_models::ATLAS_SURFACES");

fn atlas_surface(sway: bool) -> Arc<Material> {
    ATLAS_SURFACES.get_or_insert(sway, || {
        let map = TextureRef {
            anisotropy: 4,
            ..TextureRef::file(TREE_ATLAS)
        };
        Material {
            map: Some(map.clone()),
            roughness: 1.0,
            metalness: 0.0,
            bump_map: Some(map),
            bump_scale: TREE_ATLAS_BUMP,
            alpha_test: FOLIAGE_CUTOFF,
            alpha_to_coverage: true,
            side: Side::Double,
            vertex_colors: true,
            effect: if sway {
                Effect::Custom {
                    name: FOLIAGE,
                    params: Vec::new(),
                }
            } else {
                Effect::None
            },
            ..Material::default()
        }
    })
}

/// Move every tree surface under `node` onto the atlas: UVs into the surface's
/// column, the material's tint into the vertex colours.
fn into_atlas(node: &mut Node, sway: bool) {
    let atlas = atlas_surface(sway);
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        if let Some(drawable) = &mut node.drawable
            && let Some(kind) = TreeSurface::of(&drawable.material)
        {
            let (left, width, tile) = kind.atlas_column();
            let [r, g, b] = hex_to_linear(drawable.material.color.0).map(|c| c as f32);
            // Batched and tinted meshes are the tree's own: convert them in place.
            let mesh = Arc::make_mut(&mut drawable.mesh);
            for uv in &mut mesh.uvs {
                let u = f64::from(uv[0]).clamp(0.0, 1.0);
                let texel = left + TREE_ATLAS_INSET + u * (width - 2.0 * TREE_ATLAS_INSET);
                uv[0] = (texel / TREE_ATLAS_SIZE[0]) as f32;
                uv[1] = (f64::from(uv[1]) * tile) as f32;
            }
            if drawable.material.vertex_colors && mesh.colors.len() == mesh.positions.len() {
                for color in &mut mesh.colors {
                    *color = [color[0] * r, color[1] * g, color[2] * b];
                }
            } else {
                mesh.colors = vec![[r, g, b]; mesh.positions.len()];
            }
            drawable.material = atlas.clone();
        }
        stack.extend(node.children.iter_mut());
    }
}

/// Shared tree materials by surface, color, whether foliage sways and whether
/// vertex colours carry the tint (background trees).
static SURFACES: Cache<(TreeSurface, u32, bool, bool), Material> =
    Cache::new("tree_models::SURFACES");
static GEOMETRY: Cache<TreeGeometry, Mesh> = Cache::new("tree_models::GEOMETRY");

/// `surface(kind, color)`: shared textured tree materials. Foliage cards are
/// alpha-tested, drawn from both sides with alpha to coverage and shaded by their
/// vertex colors; `sway` gives them the [`FOLIAGE`] effect.
fn surface(kind: TreeSurface, color: u32, sway: bool) -> Arc<Material> {
    surface_with(kind, color, sway, false)
}

/// A background tree's white material: its tint, and every tree's own variation,
/// are in the mesh's vertex colours, so trees of every family merge per cell.
fn tinted_surface(kind: TreeSurface) -> Arc<Material> {
    surface_with(kind, 0xffffff, false, true)
}

fn surface_with(kind: TreeSurface, color: u32, sway: bool, tinted: bool) -> Arc<Material> {
    SURFACES.get_or_insert((kind, color, sway, tinted), || {
        let map = TextureRef {
            anisotropy: 4,
            ..TextureRef::file(kind.texture())
        };
        let foliage = kind.foliage();
        Material {
            map: Some(map.clone()),
            color: Color(color),
            roughness: 1.0,
            metalness: 0.0,
            bump_map: (!foliage).then_some(map),
            bump_scale: if kind == TreeSurface::Bark {
                0.055
            } else {
                0.018
            },
            alpha_test: if foliage { FOLIAGE_CUTOFF } else { 0.0 },
            alpha_to_coverage: foliage,
            side: if foliage { Side::Double } else { Side::Front },
            vertex_colors: foliage || tinted,
            effect: if sway {
                Effect::Custom {
                    name: FOLIAGE,
                    params: Vec::new(),
                }
            } else {
                Effect::None
            },
            ..Material::default()
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TreeGeometry {
    Stem,
    ConiferStem,
    /// A broadleaf trunk, tapering as it divides into the crown's limbs.
    BroadleafStem,
    Branch,
    Root,
    StumpBark(u32),
    StumpCut(u32),
}

fn open_cylinder(radius_top: f64, radial_segments: u32) -> Mesh {
    CylinderGeometry {
        radius_top,
        radius_bottom: 1.0,
        height: 1.0,
        radial_segments,
        height_segments: 1,
        open_ended: true,
    }
    .build()
}

fn geometry(kind: TreeGeometry) -> Arc<Mesh> {
    GEOMETRY.get_or_insert(kind, || match kind {
        TreeGeometry::Stem => open_cylinder(0.6, 8),
        TreeGeometry::ConiferStem => open_cylinder(0.025, 7),
        TreeGeometry::BroadleafStem => open_cylinder(0.35, 8),
        TreeGeometry::Branch => open_cylinder(0.6, 5),
        TreeGeometry::Root => open_cylinder(0.3, 6),
        TreeGeometry::StumpBark(variant) => stump_geometry(variant).0,
        TreeGeometry::StumpCut(variant) => stump_geometry(variant).1,
    })
}

/// `stumpGeometry(variant)`: irregular flared bark and the matching splintered
/// break, with long slivers standing up from the end grain where the trunk tore.
fn stump_geometry(variant: u32) -> (Mesh, Mesh) {
    let mut rng = Random::new(f64::from(variant) * 17597.0 + 79.0);
    let sides = 10u32;
    let radii: Vec<f64> = (0..sides).map(|_| rng.range(0.9, 1.1)).collect();
    let tops: Vec<f64> = (0..sides).map(|_| rng.range(0.91, 1.09)).collect();
    let angle = |j: f64| (j / f64::from(sides)) * PI * 2.0;
    let mut vertices = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    for row in 0..3u32 {
        for i in 0..=sides {
            let j = i % sides;
            let a = angle(f64::from(j));
            let radius = radii[j as usize] * [1.5, 1.12, 1.0][row as usize];
            let top = tops[j as usize];
            vertices.extend_from_slice(&[
                a.sin() * radius,
                if row == 2 { top } else { f64::from(row) * 0.42 },
                a.cos() * radius,
            ]);
            uvs.extend_from_slice(&[
                f64::from(i) / f64::from(sides),
                if row == 2 {
                    top * 0.55
                } else {
                    f64::from(row) * 0.23
                },
            ]);
            if row < 2 && i < sides {
                let n = row * (sides + 1) + i;
                indices.extend_from_slice(&[
                    n,
                    n + 1,
                    n + sides + 1,
                    n + 1,
                    n + sides + 2,
                    n + sides + 1,
                ]);
            }
        }
    }
    let mut bark = Mesh::from_f64(&vertices, &[], &uvs, Some(indices));
    bark.compute_vertex_normals();
    // The break: the bark rim, an inner ring of slivers (every third one long) and
    // a raised heart, faceted like torn wood.
    let rim = |j: u32| {
        let a = angle(f64::from(j % sides));
        let r = radii[(j % sides) as usize];
        DVec3::new(a.sin() * r, tops[(j % sides) as usize], a.cos() * r)
    };
    let inner: Vec<DVec3> = (0..sides)
        .map(|j| {
            let a = angle(f64::from(j) + 0.5);
            let r = rng.range(0.42, 0.62);
            let rise = if j % 3 == 0 {
                rng.range(0.45, 0.8)
            } else {
                rng.range(0.02, 0.22)
            };
            DVec3::new(a.sin() * r, 1.0 + rise, a.cos() * r)
        })
        .collect();
    let heart = DVec3::new(rng.range(-0.1, 0.1), 1.0 + rng.range(0.15, 0.35), 0.0);
    let mut cap_vertices = Vec::new();
    let mut cap_uvs = Vec::new();
    let mut corner = |p: DVec3| {
        cap_vertices.extend_from_slice(&[p.x, p.y, p.z]);
        cap_uvs.extend_from_slice(&[0.5 + p.x / 2.4, 0.5 + p.z / 2.4]);
    };
    for j in 0..sides {
        let (a, b) = (inner[j as usize], inner[((j + 1) % sides) as usize]);
        for p in [rim(j), rim(j + 1), a, a, rim(j + 1), b, heart, a, b] {
            corner(p);
        }
    }
    let mut cut = Mesh::from_f64(&cap_vertices, &[], &cap_uvs, None);
    cut.compute_vertex_normals();
    (bark, cut)
}

/// `mesh(parent, geo, mat, x, y, z, sx, sy, sz)`: a shadowed part, appended.
fn add_mesh(
    parent: &mut Node,
    mesh: Arc<Mesh>,
    material: Arc<Material>,
    position: DVec3,
    scale: DVec3,
) -> &mut Node {
    let mut node = shadowed(mesh, material);
    node.position = position;
    node.scale = scale;
    parent.children.push(node);
    parent.children.last_mut().expect("just pushed")
}

/// `limb(parent, mat, from, to, radius, root)`: a tapered open cylinder from `from`
/// to `to`.
fn limb(
    parent: &mut Node,
    material: Arc<Material>,
    from: DVec3,
    to: DVec3,
    radius: f64,
    root: bool,
) -> &mut Node {
    let kind = if root {
        TreeGeometry::Root
    } else if radius < 0.15 {
        TreeGeometry::Branch
    } else {
        TreeGeometry::Stem
    };
    let mut node = shadowed(geometry(kind), material);
    node.scale = DVec3::new(radius, (to - from).length(), radius);
    parent.children.push(span_between(node, from, to));
    parent.children.last_mut().expect("just pushed")
}

/// Leaf tint per family; it multiplies the foliage texture.
const LEAF_COLORS: [u32; 6] = [0xa9c68a, 0x8bb0a0, 0xafcb94, 0xb3c08c, 0xc6d08e, 0xd2d08a];
/// Bark tint per family: warm pine, grey-brown spruce, fir and oak; the pale
/// families use their own birch bark colors.
const BARK_COLORS: [u32; 4] = [0xb89c86, 0x9e9286, 0xa6998c, 0x9e9488];
const BIRCH_COLORS: [u32; 2] = [0xe5ddc5, 0xc4c6a0];
const RINGS_COLOR: u32 = 0xd9b77f;
/// The average color of each foliage texture's leaves, for colors derived from it.
const SPRIG_GREEN: u32 = 0x7d964b;
const SPRAY_GREEN: u32 = 0x8b9d6f;
/// Golden-angle step between successive whorls, lobes and cards.
const GOLDEN_ANGLE: f64 = 2.39996;
/// The cells of `leaf-sprigs.webp` as (u0, v0, u1, v1) with V up the image: two
/// lobed oak sprigs across its top half, two small ovate sprigs below. Each sprig's
/// twig enters at the bottom centre of its cell.
const OAK_CELLS: [[f64; 4]; 2] = [[0.0, 0.5, 0.5, 1.0], [0.5, 0.5, 1.0, 1.0]];
const OVATE_CELLS: [[f64; 4]; 2] = [[0.0, 0.0, 0.5, 0.5], [0.5, 0.0, 1.0, 0.5]];

/// A unit hash of a tree's seed, for its own colour: kept off the tree's random
/// stream, so its shape stays the one the simulation sized.
fn seed_unit(seed: u32, salt: u32) -> f64 {
    let mut h = seed.wrapping_mul(0x9e37_79b9) ^ salt.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^= h >> 15;
    f64::from(h) / 4_294_967_296.0
}

/// Late-summer colour turning in some broadleaves: aspen and birch gold, oak russet.
const TURNING_GOLD: u32 = 0xf0c85a;
const TURNING_RUSSET: u32 = 0xd8955a;

/// A background tree's linear foliage and bark tints: its family's colours, each
/// tree a little lighter or darker, warmer or cooler than its neighbours, spruce
/// bluer, and a few broadleaves already turning, so a wood reads as many trees.
fn background_tints(seed: u32, family: u32) -> ([f64; 3], [f64; 3]) {
    let leaf = hex_to_linear(LEAF_COLORS[family as usize]);
    let value = 0.8 + seed_unit(seed, 1) * 0.32;
    let warm = (seed_unit(seed, 2) - 0.45) * 0.16 + if family == 1 { -0.05 } else { 0.0 };
    let mut leaf = [
        leaf[0] * value * (1.0 + warm),
        leaf[1] * value,
        leaf[2] * value * (1.0 - warm * 1.5),
    ];
    let turning = seed_unit(seed, 3);
    if family >= 4 && turning < 0.16 {
        leaf = lerp_color(leaf, hex_to_linear(TURNING_GOLD), 0.4 + turning * 2.5);
    } else if family == 3 && turning < 0.07 {
        leaf = lerp_color(leaf, hex_to_linear(TURNING_RUSSET), 0.35 + turning * 3.0);
    }
    let bark = if family >= 4 {
        hex_to_linear(BIRCH_COLORS[family as usize - 4])
    } else {
        hex_to_linear(BARK_COLORS[family as usize])
    };
    let shade = 0.85 + seed_unit(seed, 4) * 0.25;
    (leaf, bark.map(|c| c * shade))
}

/// The average sRGB color of a family's foliage.
fn foliage_color(family: u32) -> u32 {
    let green = if family < 3 { SPRAY_GREEN } else { SPRIG_GREEN };
    multiply_hex(green, LEAF_COLORS[family as usize])
}

/// The rounded volume a crown's foliage shades as.
#[derive(Clone, Copy, Debug)]
enum CrownShape {
    /// A broadleaf crown: an ellipsoid.
    Round { center: DVec3, radii: DVec3 },
    /// A conifer: a cone around the leaning trunk (`lean` is the trunk's offset
    /// per metre of height) from `base` up to `top`, `radius` wide at its base.
    Cone {
        lean: DVec3,
        base: f64,
        top: f64,
        radius: f64,
    },
}

impl CrownShape {
    /// Where a point sits in the crown: the outward direction of the crown's
    /// surface there, and how deep inside it is (0 at the heart, 1 at the surface).
    fn locate(&self, p: DVec3) -> (DVec3, f64, f64) {
        match *self {
            CrownShape::Round { center, radii } => {
                let offset = (p - center) / radii;
                let outward = (offset / radii).normalize_or(DVec3::Y);
                (outward, offset.length(), offset.y)
            }
            CrownShape::Cone {
                lean,
                base,
                top,
                radius,
            } => {
                let span = top - base;
                let mut radial = p - lean * p.y;
                radial.y = 0.0;
                let local = radius * ((top - p.y) / span).clamp(0.08, 1.0);
                let outward =
                    (radial.normalize_or(DVec3::X) + DVec3::Y * (radius / span)).normalize();
                (
                    outward,
                    radial.length() / local,
                    (p.y - base) / span * 2.0 - 1.0,
                )
            }
        }
    }

    /// Skylight reaching a point of foliage: dim deep inside and underneath.
    fn light(depth: f64, height: f64) -> f64 {
        (0.64 + 0.36 * smoothstep(depth, 0.15, 1.0)) * (0.84 + 0.16 * smoothstep(height, -0.9, 0.5))
    }
}

/// One foliage card: corners counter-clockwise from the bottom left, its atlas
/// cell, its tint, and the clump its normals bulge from.
struct Card {
    corners: [DVec3; 4],
    cell: [f64; 4],
    tint: [f64; 3],
    clump: DVec3,
}

/// A card's color variation: a little lighter or darker, a little yellower or bluer.
fn card_tint(rng: &mut Random) -> [f64; 3] {
    let light = rng.range(0.82, 1.12);
    let warm = rng.range(-0.05, 0.07);
    [light * (1.0 + warm), light, light * (1.0 - warm * 1.6)]
}

/// The foliage mesh of `cards`, shaded as part of `shape`: normals lean out of the
/// crown and out of each card's clump, vertex colors carry the card's tint dimmed
/// by the skylight its position receives.
fn foliage_mesh(cards: &[&Card], shape: &CrownShape, tint: [f64; 3]) -> Mesh {
    let mut positions = Vec::with_capacity(cards.len() * 6);
    let mut normals = Vec::with_capacity(cards.len() * 6);
    let mut uvs = Vec::with_capacity(cards.len() * 6);
    let mut colors = Vec::with_capacity(cards.len() * 6);
    for card in cards {
        let [u0, v0, u1, v1] = card.cell;
        let corner_uvs = [[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
        let [a, b, _, d] = card.corners;
        let face = (b - a).cross(d - a).normalize_or(DVec3::Y);
        // Each corner shaded once: the card's two triangles share two of them.
        let shaded = card.corners.map(|p| {
            let (outward, depth, height) = shape.locate(p);
            let face = if face.dot(outward) < 0.0 { -face } else { face };
            let bulge = (p - card.clump).normalize_or(outward);
            let normal = (outward * 0.62 + bulge * 0.26 + face * 0.12).normalize();
            let light = CrownShape::light(depth, height);
            (
                narrow(normal),
                [0, 1, 2].map(|k| (card.tint[k] * tint[k] * light) as f32),
            )
        });
        for index in [0, 1, 2, 0, 2, 3] {
            let (normal, color) = shaded[index];
            positions.push(narrow(card.corners[index]));
            normals.push(normal);
            uvs.push(corner_uvs[index].map(|v| v as f32));
            colors.push(color);
        }
    }
    Mesh {
        positions,
        normals,
        uvs,
        colors,
        ..Mesh::default()
    }
}

/// Where new boughs and cards go: straight into the crown, or into a shedding group.
struct Crown {
    node: Node,
    detail: TreeDetail,
    /// Foliage cards with the crown child index of their bough (`None`: the crown).
    cards: Vec<(Option<usize>, Card)>,
}

impl Crown {
    /// `branchParent(stage)`: the crown for fixed foliage, or a new shedding bough
    /// group (full detail only). Returns the index of the bough among the crown's
    /// children, or `None` for the crown itself.
    fn branch_parent(&mut self, stage: u32) -> Option<usize> {
        if self.detail.background() || stage == 0 {
            return None;
        }
        let name = if stage == 1 {
            tree_part::BRANCH_STAGE_1
        } else {
            tree_part::BRANCH_STAGE_2
        };
        self.node.children.push(Node::group(name));
        Some(self.node.children.len() - 1)
    }

    fn parent(&mut self, bough: Option<usize>) -> &mut Node {
        match bough {
            Some(index) => &mut self.node.children[index],
            None => &mut self.node,
        }
    }

    fn card(&mut self, bough: Option<usize>, card: Card) {
        self.cards.push((bough, card));
    }

    /// Add the three crossed cards of a needle spray: its local Z runs from the
    /// branch along the spray, X across it, and `transform` places it.
    fn spray(&mut self, bough: Option<usize>, transform: DMat4, rng: &mut Random) {
        let clump = transform_point(&transform, DVec3::new(0.0, 0.0, 0.5));
        let tint = card_tint(rng);
        // Distant sprays cross two cards instead of three.
        let crossed = if self.detail == TreeDetail::Distant {
            2
        } else {
            3
        };
        for card in 0..crossed {
            let angle = (f64::from(card) * PI) / f64::from(crossed);
            let (cos, sin) = (angle.cos(), angle.sin());
            let corner =
                |x: f64, z: f64| transform_point(&transform, DVec3::new(x * cos, x * sin, z));
            self.card(
                bough,
                Card {
                    corners: [
                        corner(-0.5, 0.0),
                        corner(0.5, 0.0),
                        corner(0.5, 1.0),
                        corner(-0.5, 1.0),
                    ],
                    cell: [0.0, 0.0, 1.0, 1.0],
                    tint,
                    clump,
                },
            );
        }
    }

    /// Build each parent's foliage mesh from its cards, shaded as `shape`.
    fn grow_foliage(&mut self, material: &Arc<Material>, shape: &CrownShape, tint: [f64; 3]) {
        let cards = std::mem::take(&mut self.cards);
        let mut parents: Vec<Option<usize>> = Vec::new();
        for (parent, _) in &cards {
            if !parents.contains(parent) {
                parents.push(*parent);
            }
        }
        for parent in parents {
            let mine: Vec<&Card> = cards
                .iter()
                .filter(|(p, _)| *p == parent)
                .map(|(_, card)| card)
                .collect();
            let mut mesh = foliage_mesh(&mine, shape, tint);
            if parent.is_some() {
                // A shed bough lands any way up: half its shading bulges from its
                // own middle, so whichever side faces the sky is lit.
                let (low, high) = mine.iter().flat_map(|card| card.corners).fold(
                    (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)),
                    |(low, high), p| (low.min(p), high.max(p)),
                );
                let middle = ((low + high) / 2.0).as_vec3();
                for (normal, position) in mesh.normals.iter_mut().zip(&mesh.positions) {
                    let own = (glam::Vec3::from(*position) - middle).normalize_or_zero();
                    *normal = (glam::Vec3::from(*normal) + own)
                        .normalize_or(glam::Vec3::Y)
                        .to_array();
                }
            }
            self.parent(parent)
                .children
                .push(shadowed(Arc::new(mesh), material.clone()));
        }
    }
}

/// A sprig card in a clump at `tip` with ellipsoid `radii`: the `k`th of `count`
/// spread over the clump from `phase`, its twig toward the inside and its leaves
/// reaching out and up, `size` metres across.
#[allow(clippy::too_many_arguments)]
fn sprig_card(
    rng: &mut Random,
    tip: DVec3,
    radii: DVec3,
    k: u32,
    count: u32,
    phase: f64,
    size: f64,
    cells: &[[f64; 4]; 2],
) -> Card {
    let y = 1.0 - (f64::from(k) + 0.5) / f64::from(count) * 1.7;
    let ring = (1.0 - y * y).max(0.0).sqrt();
    let theta = phase + f64::from(k) * GOLDEN_ANGLE;
    let direction = DVec3::new(theta.cos() * ring, y, theta.sin() * ring);
    let center = tip + direction * radii * rng.range(0.3, 0.75);
    let outward = (direction + DVec3::Y * 0.3).normalize();
    let wander = DVec3::new(
        rng.range(-1.0, 1.0),
        rng.range(-1.0, 1.0),
        rng.range(-1.0, 1.0),
    );
    // Sprigs face out of the clump like shingles, so the crown shows leaves rather
    // than card edges, and grow up and outward from a twig inside it.
    let face = (outward + wander * 0.4).normalize_or(outward);
    let rise = outward * 0.6 + DVec3::Y + wander * 0.3;
    let up = (rise - face * rise.dot(face)).normalize_or(face.any_orthonormal_vector());
    let across = up.cross(face);
    let size = size * rng.range(0.88, 1.18);
    // The leafy mass sits a little below the card's middle.
    let bottom = center - up * size * 0.42;
    let half = across * size * 0.5;
    let mut cell = cells[usize::from(rng.next() < 0.5)];
    if rng.next() < 0.5 {
        cell.swap(0, 2);
    }
    Card {
        corners: [
            bottom - half,
            bottom + half,
            bottom + half + up * size,
            bottom - half + up * size,
        ],
        cell,
        tint: card_tint(rng),
        clump: tip,
    }
}

/// `treeModel(c, detail)`.
pub fn tree_model(c: &TreeShape, detail: TreeDetail) -> Node {
    let TreeProportions {
        seed,
        mut rng,
        family,
        twist,
        height,
        radius,
        stump_height,
        ..
    } = tree_proportions(c.x, c.z, c.w, c.d, c.h);
    let full = detail == TreeDetail::Full;
    let conifer = family < 3;
    let pale = family == 4 || family == 5;
    let bark_surface = if pale {
        TreeSurface::Birch
    } else {
        TreeSurface::Bark
    };
    let foliage_surface = if conifer {
        TreeSurface::ConiferSpray
    } else {
        TreeSurface::LeafSprigs
    };
    let (bark, foliage, tints) = if full {
        let bark_color = if pale {
            BIRCH_COLORS[family as usize - 4]
        } else {
            BARK_COLORS[family as usize]
        };
        (
            surface(bark_surface, bark_color, false),
            surface(foliage_surface, LEAF_COLORS[family as usize], true),
            None,
        )
    } else {
        (
            tinted_surface(bark_surface),
            tinted_surface(foliage_surface),
            Some(background_tints(seed, family)),
        )
    };
    let rings = surface(TreeSurface::Rings, RINGS_COLOR, false);
    let mut group = Node {
        position: DVec3::new(c.x, 0.0, c.z),
        ..Node::default()
    };
    let mut crown = Crown {
        node: Node::group(tree_part::CROWN),
        detail,
        cards: Vec::new(),
    };
    let variant = seed % 24;
    if full {
        let mut stump = Node::group(tree_part::STUMP);
        let scale = DVec3::new(radius, stump_height, radius);
        add_mesh(
            &mut stump,
            geometry(TreeGeometry::StumpBark(variant)),
            bark.clone(),
            DVec3::ZERO,
            scale,
        )
        .set_rotation_euler(0.0, twist, 0.0);
        // Buttress roots flare from the stump and dive into the ground.
        let root_count = 5 + seed % 3;
        for i in 0..root_count {
            let angle =
                twist + (f64::from(i) * PI * 2.0) / f64::from(root_count) + rng.range(-0.2, 0.2);
            let reach = radius * rng.range(1.5, 2.1);
            let from = DVec3::new(
                angle.sin() * radius * 0.55,
                stump_height * rng.range(0.45, 0.6),
                angle.cos() * radius * 0.55,
            );
            let to = DVec3::new(angle.sin() * reach, -0.08, angle.cos() * reach);
            limb(
                &mut stump,
                bark.clone(),
                from,
                to,
                radius * rng.range(0.38, 0.52),
                true,
            );
        }
        batch(&mut stump);
        let cut = add_mesh(
            &mut stump,
            geometry(TreeGeometry::StumpCut(variant)),
            rings.clone(),
            DVec3::ZERO,
            scale,
        );
        cut.set_rotation_euler(0.0, twist, 0.0);
        cut.name = tree_part::CUT_SURFACE.to_string();
        paint_mesh(cut);
        cut.visible = false;
        group.children.push(stump);
    }
    let lean_x = rng.range(-0.1, 0.1) * c.w;
    let lean_z = rng.range(-0.07, 0.07) * c.d;
    let trunk_base = if full { stump_height * 0.88 } else { 0.0 };
    let trunk = limb(
        &mut crown.node,
        bark.clone(),
        DVec3::new(0.0, trunk_base, 0.0),
        DVec3::new(lean_x, height * (if conifer { 0.98 } else { 0.78 }), lean_z),
        radius,
        false,
    );
    if let Some(drawable) = &mut trunk.drawable {
        drawable.mesh = geometry(if conifer {
            TreeGeometry::ConiferStem
        } else {
            TreeGeometry::BroadleafStem
        });
        if let Some((_, bark_tint)) = tints {
            // The tinted trunk is the tree's own: background trees merge per cell.
            let mut mesh = (*drawable.mesh).clone();
            let color = bark_tint.map(|c| c as f32);
            mesh.colors = vec![color; mesh.positions.len()];
            drawable.mesh = Arc::new(mesh);
        }
    }
    if full {
        // The torn underside of the trunk: hidden in the stump while the tree
        // stands, the splintered end of the log once it falls.
        let end = add_mesh(
            &mut crown.node,
            geometry(TreeGeometry::StumpCut((variant + 7) % 24)),
            rings,
            DVec3::new(0.0, trunk_base + stump_height * 0.55, 0.0),
            DVec3::new(radius * 0.85, stump_height * 0.55, radius * 0.85),
        );
        end.rotation = quat_from_euler(PI, twist, 0.0);
    }
    let shape = if conifer && detail == TreeDetail::Distant {
        distant_conifer(
            &mut crown.node,
            c,
            &mut rng,
            family,
            twist,
            height,
            (lean_x, lean_z),
            tints.map_or([1.0; 3], |(leaf, _)| leaf),
        )
    } else if conifer {
        add_conifer_crown(
            &mut crown, c, &mut rng, &bark, family, twist, height, radius, lean_x, lean_z,
        )
    } else {
        add_broadleaf_crown(
            &mut crown, c, &mut rng, &bark, family, twist, height, radius, lean_x, lean_z,
        )
    };
    crown.grow_foliage(&foliage, &shape, tints.map_or([1.0; 3], |(leaf, _)| leaf));
    let mut crown = crown.node;
    if full {
        let offset = group.position;
        let world = DMat4::from_translation(offset);
        for bough in crown
            .children
            .iter_mut()
            .filter(|child| child.drawable.is_none())
        {
            batch(bough);
            // Pivot each falling bough around its own center, not around the trunk.
            let center = node_bounds(bough, world).center() - offset;
            for child in &mut bough.children {
                if let Some(drawable) = &mut child.drawable {
                    Arc::make_mut(&mut drawable.mesh).translate(-center.x, -center.y, -center.z);
                }
            }
            bough.position = center;
        }
        batch(&mut crown);
        into_atlas(&mut crown, true);
        group.children.push(crown);
    } else {
        // The background crown is the tree group itself.
        crown.position = group.position;
        group = crown;
        into_atlas(&mut group, false);
    }
    group
}

/// A texel of [`TREE_ATLAS`]'s birch column with no marks within five texels,
/// read the same way up or flipped (its column UV), and its linear colour: a far
/// conifer's solid crown samples only it, so its vertex colours carry its green.
const PLAIN_ATLAS_UV: [f64; 2] = [0.4585, 0.0957];
const PLAIN_ATLAS_COLOR: [f64; 3] = [0.791, 0.753, 0.665];
/// The conifer spray's average linear colour, which a far conifer's solid crown
/// matches so it reads as the same wood as the card trees nearer by.
const NEEDLE_COLOR: [f64; 3] = [0.29, 0.37, 0.18];
/// Points round each whorl of a far conifer: alternately reaching boughs and the
/// notches between them, so its silhouette is branchy rather than a cone.
const DISTANT_WHORL_POINTS: u32 = 8;

/// A far conifer's crown as solid whorls (its trunk is drawn by the caller):
/// stacked, drooping skirts of boughs, lit at their tips and dark in their
/// notches. Seen only across the valley, it keeps a crisp, branchy silhouette
/// where a few crossed cards thinned into streaks, in a quarter of the triangles.
#[allow(clippy::too_many_arguments)]
fn distant_conifer(
    crown: &mut Node,
    c: &TreeShape,
    rng: &mut Random,
    family: u32,
    twist: f64,
    height: f64,
    lean: (f64, f64),
    tint: [f64; 3],
) -> CrownShape {
    // Pines carry a high, open crown of few whorls; spruce and fir skirt the
    // ground in many.
    let (tiers, base) = match family {
        0 => (3, 0.34),
        1 => (5, 0.07),
        _ => (4, 0.11),
    };
    let width = c.w * (if family == 2 { 0.44 } else { 0.5 }) * 1.18;
    let lean = DVec3::new(lean.0 / height, 0.0, lean.1 / height);
    let color = |light: f64| {
        [0, 1, 2].map(|k| (NEEDLE_COLOR[k] * tint[k] * light / PLAIN_ATLAS_COLOR[k]) as f32)
    };
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();
    let mut vertex = |p: DVec3, n: DVec3, light: f64| {
        positions.extend([p.x, p.y, p.z]);
        normals.extend([n.x, n.y, n.z]);
        uvs.extend(PLAIN_ATLAS_UV);
        colors.push(color(light));
        (positions.len() / 3 - 1) as u32
    };
    for i in 0..tiers {
        let t = f64::from(i) / f64::from(tiers);
        let bottom = height * (base + (0.86 - base) * t);
        let top = if i == tiers - 1 {
            height
        } else {
            bottom + (height - bottom) * 0.58
        };
        let reach = width * (1.0 - t * 0.78) * rng.range(0.9, 1.1);
        // Lower whorls lie in the crown's shade.
        let shade = 0.78 + 0.22 * t;
        let apex = vertex(lean * top + DVec3::Y * top, DVec3::Y, shade);
        let turn = twist + f64::from(i) * GOLDEN_ANGLE;
        let first = apex + 1;
        for k in 0..DISTANT_WHORL_POINTS {
            let bough = k % 2 == 0;
            let angle = turn
                + f64::from(k) * std::f64::consts::TAU / f64::from(DISTANT_WHORL_POINTS)
                + rng.range(-0.12, 0.12);
            let out = if bough {
                reach * rng.range(0.88, 1.12)
            } else {
                reach * rng.range(0.6, 0.72)
            };
            let radial = DVec3::new(angle.sin(), 0.0, angle.cos());
            // The boughs droop at their tips.
            let y = bottom - if bough { reach * 0.16 } else { -reach * 0.04 };
            let p = lean * y + DVec3::Y * y + radial * out;
            let normal = (radial * 0.72 + DVec3::Y * 0.6).normalize();
            vertex(p, normal, shade * if bough { 0.72 } else { 0.5 });
        }
        for k in 0..DISTANT_WHORL_POINTS {
            let next = (k + 1) % DISTANT_WHORL_POINTS;
            indices.extend([apex, first + k, first + next]);
        }
    }
    let mut mesh = Mesh::from_f64(&positions, &normals, &uvs, Some(indices));
    mesh.colors = colors;
    crown
        .children
        .push(shadowed(Arc::new(mesh), tinted_surface(TreeSurface::Birch)));
    CrownShape::Cone {
        lean,
        base: height * (base - 0.08),
        top: height * 1.02,
        radius: width * 1.05,
    }
}

#[allow(clippy::too_many_arguments)]
fn add_conifer_crown(
    crown: &mut Crown,
    c: &TreeShape,
    rng: &mut Random,
    bark: &Arc<Material>,
    family: u32,
    twist: f64,
    height: f64,
    radius: f64,
    lean_x: f64,
    lean_z: f64,
) -> CrownShape {
    let full = crown.detail == TreeDetail::Full;
    let tiers = match crown.detail {
        TreeDetail::Full => 8,
        TreeDetail::Background => 6,
        TreeDetail::Distant => 5,
    };
    let arms = 6;
    // Pines carry a looser, higher crown; spruce and fir retain their lower boughs.
    let base = match family {
        0 => 0.36,
        1 => 0.17,
        _ => 0.23,
    };
    // Background forests keep their boughs lower and fuller, so the hills read
    // as dense woods rather than rows of bare poles.
    let (base, fullness) = match crown.detail {
        TreeDetail::Full => (base, 1.0),
        TreeDetail::Background => (base * 0.6, 1.12),
        // Far stands show as full, dark cones down to the ground.
        TreeDetail::Distant => (base * 0.45, 1.22),
    };
    let width = c.w * (if family == 2 { 0.44 } else { 0.5 }) * fullness;
    for i in 0..tiers {
        let t = f64::from(i) / f64::from(tiers - 1);
        let y = height * (base + t * (0.87 - base));
        let span = width * (1.0 - t * 0.76);
        for j in 0..arms {
            let angle = twist
                + f64::from(i) * GOLDEN_ANGLE
                + (f64::from(j) * PI * 2.0) / f64::from(arms)
                + rng.range(-0.24, 0.24);
            let reach = span * rng.range(0.78, 1.16);
            let start = DVec3::new(
                lean_x * (base + t * (1.0 - base)),
                y + rng.range(-0.05, 0.05) * height,
                lean_z * (base + t * (1.0 - base)),
            );
            // An upright inner shoot fills the crown between whorls.
            if j == arms - 1 {
                let transform = DMat4::from_scale_rotation_translation(
                    DVec3::new(span * 0.85, span * 0.85, height * (0.27 - t * 0.1)),
                    quat_rotate_z(quat_from_euler(-PI / 2.0, 0.0, 0.0), angle),
                    DVec3::new(start.x, start.y - height * 0.06, start.z),
                );
                crown.spray(None, transform, rng);
                continue;
            }
            let stage = match (i, j) {
                (1, 0) | (2, 3) => 1,
                (0, 2) | (3, 1) => 2,
                _ => 0,
            };
            let bough = crown.branch_parent(stage);
            let rise = reach
                * match family {
                    0 => 0.24,
                    1 => -0.16,
                    _ => 0.06,
                };
            if full && i < tiers - 2 {
                // The limb stays inside its spray.
                let end = DVec3::new(
                    start.x + angle.sin() * reach * 0.66,
                    start.y + rise * 0.7,
                    start.z + angle.cos() * reach * 0.66,
                );
                limb(
                    crown.parent(bough),
                    bark.clone(),
                    start,
                    end,
                    radius * (0.13 - t * 0.08),
                    false,
                );
            }
            let transform = DMat4::from_scale_rotation_translation(
                DVec3::new(
                    reach * (if family == 0 { 0.95 } else { 0.85 }),
                    height * (0.25 - t * 0.12),
                    reach * 1.1,
                ),
                quat_from_euler_yxz(-rise.atan2(reach), angle, rng.range(-0.2, 0.2)),
                start,
            );
            crown.spray(bough, transform, rng);
        }
    }
    let leader = DMat4::from_scale_rotation_translation(
        DVec3::new(c.w * 0.2, c.w * 0.2, height * 0.2),
        quat_rotate_z(quat_from_euler(-PI / 2.0, 0.0, 0.0), twist),
        DVec3::new(lean_x * 0.9, height * 0.82, lean_z * 0.9),
    );
    crown.spray(None, leader, rng);
    CrownShape::Cone {
        lean: DVec3::new(lean_x / height, 0.0, lean_z / height),
        base: height * (base - 0.08),
        top: height * 1.02,
        radius: width * 1.05,
    }
}

#[allow(clippy::too_many_arguments)]
fn add_broadleaf_crown(
    crown: &mut Crown,
    c: &TreeShape,
    rng: &mut Random,
    bark: &Arc<Material>,
    family: u32,
    twist: f64,
    height: f64,
    radius: f64,
    lean_x: f64,
    lean_z: f64,
) -> CrownShape {
    let full = crown.detail == TreeDetail::Full;
    let count = 7;
    let oak = family == 3;
    // Oaks spread wide from a short bole; birch and aspen carry a taller oval crown
    // from a third of their height.
    let (spread_at, low_lobe, crown_rise) = if oak {
        (0.3, 0.42, 0.42)
    } else {
        (0.25, 0.38, 0.47)
    };
    // Background woods carry fuller crowns lower on the trunk.
    let (low_lobe, fullness) = if full {
        (low_lobe, 1.0)
    } else {
        (low_lobe - 0.12, 1.18)
    };
    let cells = if oak { &OAK_CELLS } else { &OVATE_CELLS };
    let (mut low, mut high) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
    for i in 0..count {
        let a = twist + f64::from(i) * GOLDEN_ANGLE;
        let t = f64::from(i) / f64::from(count - 1);
        let spread = (1.0 - t * 0.6) * spread_at;
        let center = DVec3::new(
            lean_x + a.sin() * c.w * spread,
            height * (low_lobe + t * crown_rise),
            lean_z + a.cos() * c.d * spread,
        );
        if full {
            let from = DVec3::new(lean_x * 0.5, center.y - height * 0.2, lean_z * 0.5);
            limb(
                &mut crown.node,
                bark.clone(),
                from,
                center,
                radius * (0.32 - t * 0.16),
                false,
            );
        }
        let size = c.w * (if oak { 0.3 } else { 0.27 }) * rng.range(0.84, 1.09) * fullness;
        // Overlapping clumps of sprigs give the crown an irregular, branching silhouette.
        let clumps = match crown.detail {
            TreeDetail::Full => 4,
            TreeDetail::Background => 3,
            TreeDetail::Distant => 2,
        };
        for j in 0..clumps {
            let angle = a + f64::from(j) * GOLDEN_ANGLE;
            let reach = if j == 0 { 0.0 } else { size * 0.55 };
            let tip = center
                + DVec3::new(
                    angle.sin() * reach,
                    rng.range(-0.25, 0.35) * size,
                    angle.cos() * reach,
                );
            // Damage sheds small outer twigs, never an entire section of the crown.
            let bough = if j == clumps - 1 && i < 4 {
                crown.branch_parent(if i < 2 { 1 } else { 2 })
            } else {
                None
            };
            if full && j > 0 {
                limb(
                    crown.parent(bough),
                    bark.clone(),
                    center,
                    tip,
                    radius * 0.075,
                    false,
                );
            }
            let radii = DVec3::new(
                size * rng.range(0.48, 0.66),
                size * rng.range(0.55, 0.85),
                size * rng.range(0.45, 0.65),
            );
            let cards = match crown.detail {
                TreeDetail::Full => 9,
                TreeDetail::Background => 5,
                TreeDetail::Distant => 5,
            };
            let phase = rng.range(0.0, PI * 2.0);
            let card_size = size * if oak { 1.35 } else { 1.28 };
            for k in 0..cards {
                let card = sprig_card(rng, tip, radii, k, cards, phase, card_size, cells);
                for corner in card.corners {
                    low = low.min(corner);
                    high = high.max(corner);
                }
                crown.card(bough, card);
            }
        }
    }
    CrownShape::Round {
        center: (low + high) / 2.0,
        radii: (high - low) / 2.0,
    }
}

/// The damage stage a tree's boughs show: two drop after the first damage, two
/// more at 35% health (`setTreeDamage`).
pub fn tree_branch_stage(health_ratio: f64) -> u32 {
    if health_ratio >= 1.0 {
        0
    } else if health_ratio > 0.35 {
        1
    } else {
        2
    }
}

/// The drop stage of a shedding bough node, from its name.
pub fn branch_drop_stage(node: &Node) -> Option<u32> {
    match node.name.as_str() {
        tree_part::BRANCH_STAGE_1 => Some(1),
        tree_part::BRANCH_STAGE_2 => Some(2),
        _ => None,
    }
}

static TRUNK_FRAGMENT: Cache<(), Node> = Cache::new("tree_models::TRUNK_FRAGMENT");

/// `trunkFragment()`: the instanced physical trunk section, a tapered cylinder with
/// bark on its side and end grain on its caps. Three.js drew it as one geometry with
/// three material groups; here it is a group of three meshes (side, top cap,
/// bottom cap) that share the vertex arrays and draw with the same instances.
pub fn trunk_fragment() -> Arc<Node> {
    TRUNK_FRAGMENT.get_or_insert((), || {
        let mesh = crate::geometry::cylinder_geometry(0.36, 0.5, 1.0, 8);
        let indices = mesh.indices.clone().expect("cylinders are indexed");
        let radial = 8usize;
        let torso = radial * 6;
        let cap = radial * 3;
        let ranges = [0..torso, torso..torso + cap, torso + cap..torso + 2 * cap];
        let materials = [
            surface(TreeSurface::Bark, 0xffffff, false),
            surface(TreeSurface::Rings, 0xffffff, false),
            surface(TreeSurface::Rings, 0xffffff, false),
        ];
        let mut group = Node::group("trunk-fragment");
        for (range, material) in ranges.into_iter().zip(materials) {
            let part = Mesh {
                indices: Some(indices[range].to_vec()),
                ..mesh.clone()
            };
            group.children.push(Node::mesh(Arc::new(part), material));
        }
        group
    })
}
