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
use super::model_primitives::{Cache, shadowed};
use crate::geometry::math::{
    multiply_hex, normalize, quat_from_euler, quat_from_euler_yxz, quat_from_unit_vectors,
    quat_rotate_z, smoothstep, transform_point,
};
use crate::geometry::{CylinderGeometry, Mesh, node_bounds};
use crate::scene::{Color, Effect, Material, Node, Side, TextureRef};
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
    Background,
}

/// A built tree and the traits the TypeScript kept in `userData`.
#[derive(Clone, Debug)]
pub struct TreeModel {
    pub node: Node,
    /// Index into [`TREE_FAMILIES`].
    pub family: u32,
    pub seed: u32,
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

/// Shared tree materials by surface, color, and whether foliage sways.
static SURFACES: Cache<(TreeSurface, u32, bool), Material> = Cache::new();
static GEOMETRY: Cache<TreeGeometry, Mesh> = Cache::new();

fn surface(kind: TreeSurface, color: u32) -> Arc<Material> {
    tree_surface(kind, color, false)
}

/// `surface(kind, color)`: shared textured tree materials. Foliage cards are
/// alpha-tested, drawn from both sides with alpha to coverage and shaded by their
/// vertex colors; `sway` gives them the [`FOLIAGE`] effect.
fn tree_surface(kind: TreeSurface, color: u32, sway: bool) -> Arc<Material> {
    SURFACES.get_or_insert((kind, color, sway), || {
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
            vertex_colors: foliage,
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
    let delta = to - from;
    let kind = if root {
        TreeGeometry::Root
    } else if radius < 0.15 {
        TreeGeometry::Branch
    } else {
        TreeGeometry::Stem
    };
    let node = add_mesh(
        parent,
        geometry(kind),
        material,
        DVec3::new(
            (from.x + to.x) / 2.0,
            (from.y + to.y) / 2.0,
            (from.z + to.z) / 2.0,
        ),
        DVec3::new(radius, delta.length(), radius),
    );
    node.rotation = quat_from_unit_vectors(DVec3::Y, normalize(delta));
    node
}

/// Leaf tint per family; it multiplies the foliage texture.
const LEAF_COLORS: [u32; 6] = [0x9eb783, 0x7ea197, 0xa3bd8e, 0xb3c08c, 0xc6d08e, 0xd2d08a];
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
        (0.56 + 0.44 * smoothstep(depth, 0.15, 1.0)) * (0.82 + 0.18 * smoothstep(height, -0.9, 0.5))
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
fn foliage_mesh(cards: &[&Card], shape: &CrownShape) -> Mesh {
    let mut positions = Vec::with_capacity(cards.len() * 18);
    let mut normals = Vec::with_capacity(cards.len() * 18);
    let mut uvs = Vec::with_capacity(cards.len() * 12);
    let mut colors = Vec::with_capacity(cards.len() * 6);
    for card in cards {
        let [u0, v0, u1, v1] = card.cell;
        let corner_uvs = [[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
        let [a, b, _, d] = card.corners;
        let face = (b - a).cross(d - a).normalize_or(DVec3::Y);
        for index in [0, 1, 2, 0, 2, 3] {
            let p = card.corners[index];
            let (outward, depth, height) = shape.locate(p);
            let face = if face.dot(outward) < 0.0 { -face } else { face };
            let bulge = (p - card.clump).normalize_or(outward);
            let normal = (outward * 0.62 + bulge * 0.26 + face * 0.12).normalize();
            let light = CrownShape::light(depth, height);
            positions.extend_from_slice(&[p.x, p.y, p.z]);
            normals.extend_from_slice(&[normal.x, normal.y, normal.z]);
            uvs.extend_from_slice(&corner_uvs[index]);
            colors.push(card.tint.map(|channel| (channel * light) as f32));
        }
    }
    let mut mesh = Mesh::from_f64(&positions, &normals, &uvs, None);
    mesh.colors = colors;
    mesh
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
        if self.detail == TreeDetail::Background || stage == 0 {
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
        for card in 0..3u32 {
            let angle = (f64::from(card) * PI) / 3.0;
            let corner = |x: f64, z: f64| {
                transform_point(&transform, DVec3::new(x * angle.cos(), x * angle.sin(), z))
            };
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
    fn grow_foliage(&mut self, material: &Arc<Material>, shape: &CrownShape) {
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
            let mut mesh = foliage_mesh(&mine, shape);
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
pub fn tree_model(c: &TreeShape, detail: TreeDetail) -> TreeModel {
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
    let bark = if pale {
        surface(TreeSurface::Birch, BIRCH_COLORS[family as usize - 4])
    } else {
        surface(TreeSurface::Bark, BARK_COLORS[family as usize])
    };
    let foliage = tree_surface(
        if conifer {
            TreeSurface::ConiferSpray
        } else {
            TreeSurface::LeafSprigs
        },
        LEAF_COLORS[family as usize],
        full,
    );
    let rings = surface(TreeSurface::Rings, RINGS_COLOR);
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
    let shape = if conifer {
        add_conifer_crown(
            &mut crown, c, &mut rng, &bark, family, twist, height, radius, lean_x, lean_z,
        )
    } else {
        add_broadleaf_crown(
            &mut crown, c, &mut rng, &bark, family, twist, height, radius, lean_x, lean_z,
        )
    };
    crown.grow_foliage(&foliage, &shape);
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
                    let mut mesh = (*drawable.mesh).clone();
                    mesh.translate(-center.x, -center.y, -center.z);
                    drawable.mesh = Arc::new(mesh);
                }
            }
            bough.position = center;
        }
        batch(&mut crown);
        group.children.push(crown);
    } else {
        // The background crown is the tree group itself.
        crown.position = group.position;
        group = crown;
    }
    TreeModel {
        node: group,
        family,
        seed,
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
    let tiers = if full { 8 } else { 7 };
    let arms = 6;
    // Pines carry a looser, higher crown; spruce and fir retain their lower boughs.
    let base = match family {
        0 => 0.36,
        1 => 0.17,
        _ => 0.23,
    };
    let width = c.w * (if family == 2 { 0.44 } else { 0.5 });
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
        let size = c.w * (if oak { 0.3 } else { 0.27 }) * rng.range(0.84, 1.09);
        // Overlapping clumps of sprigs give the crown an irregular, branching silhouette.
        let clumps = if full { 4 } else { 3 };
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
            let cards = if full { 9 } else { 5 };
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

/// `setTreeDestroyed(tree, destroyed)`: a felled tree hides its crown and shows the
/// cut end grain on its stump. Background trees have neither and are unchanged.
pub fn set_tree_destroyed(tree: &mut Node, destroyed: bool) {
    if tree.find(tree_part::CUT_SURFACE).is_none() {
        return;
    }
    if let Some(crown) = tree.find_mut(tree_part::CROWN) {
        crown.visible = !destroyed;
    }
    if let Some(cut) = tree.find_mut(tree_part::CUT_SURFACE) {
        cut.visible = destroyed;
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

/// `setTreeDamage(tree, healthRatio, onDrop)`: show only the boughs that survive
/// `stage`, returning the crown child indices of boughs that were visible and are
/// now hidden (TS `onDrop`), in crown order. `previous_stage` is the stage last
/// applied (TS `userData.branchDamageStage`, initially 0); nothing changes when
/// the stage is the same.
pub fn set_tree_damage(
    tree: &mut Node,
    previous_stage: u32,
    health_ratio: f64,
) -> (u32, Vec<usize>) {
    let stage = tree_branch_stage(health_ratio);
    let mut dropped = Vec::new();
    if stage == previous_stage {
        return (stage, dropped);
    }
    if let Some(crown) = tree.find_mut(tree_part::CROWN) {
        for (index, bough) in crown.children.iter_mut().enumerate() {
            let Some(drop_stage) = branch_drop_stage(bough) else {
                continue;
            };
            let visible = drop_stage > stage;
            if bough.visible && !visible {
                dropped.push(index);
            }
            bough.visible = visible;
        }
    }
    (stage, dropped)
}

static TRUNK_FRAGMENT: Cache<(), Node> = Cache::new();

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
            surface(TreeSurface::Bark, 0xffffff),
            surface(TreeSurface::Rings, 0xffffff),
            surface(TreeSurface::Rings, 0xffffff),
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
