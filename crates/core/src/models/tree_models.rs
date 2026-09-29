//! Ports of `tree-models.ts` and the proportions half of `tree-proportions.ts`:
//! six tree families (pine, spruce, fir, oak, birch, aspen) from textured limbs,
//! needle-spray cards and leaf lobes, seeded by position.
//!
//! A full-detail tree (a cover) is an unnamed group at the tree's x/z holding
//! [`tree_part::STUMP`] (batched bark and roots, then the hidden
//! [`tree_part::CUT_SURFACE`]) and [`tree_part::CROWN`]. The crown holds the
//! shedding boughs first (named by the damage stage that drops them, see
//! [`tree_part::BRANCH_STAGE_1`]), each centred on its own bounds, then the batched
//! trunk and foliage. A background tree is one unbatched group (itself named
//! `trunk-and-crown`, as in the TypeScript) without stump or boughs.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::{DMat4, DVec3};

use super::batching::{batch, paint_mesh};
use super::model_primitives::{Cache, shadowed};
use super::prop_support::{
    Random, multiply_hex, quat_from_euler_yxz, quat_from_unit_vectors, rotate_z,
};
use crate::geometry::math::{js_round, normalize, quat_from_euler, to_int32};
use crate::geometry::{CylinderGeometry, Mesh, icosahedron_geometry, node_bounds};
use crate::scene::{Material, Node, Side, TextureRef};

/// Node names that presentation looks up on a full-detail tree.
pub mod tree_part {
    /// The trunk, boughs and foliage (TS `userData.crown`): hidden when felled.
    pub const CROWN: &str = "trunk-and-crown";
    /// The rooted stump group that stays after the tree is felled.
    pub const STUMP: &str = "rooted-stump";
    /// The exposed end grain on the stump (TS `userData.cutSurface`): shown when felled.
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
    /// Distant scenery: fewer tiers and lobes, no stump or bark limbs.
    Background,
}

/// `treeProportions(c)`: the deterministic proportions shared by the standing tree,
/// its fallen trunk and its stump, and the stream the model keeps drawing from.
///
/// Shared with `sim::tree_proportions`; de-duplicate at integration.
#[derive(Clone, Debug)]
pub struct TreeProportions {
    pub seed: u32,
    pub rng: Random,
    pub family: usize,
    pub twist: f64,
    pub height: f64,
    pub radius: f64,
    pub stump_height: f64,
    /// Blocks the solid flared trunk, not the thin roots along the ground.
    pub stump_radius: f64,
}

pub fn tree_proportions(c: &TreeShape) -> TreeProportions {
    let seed = (to_int32(js_round(c.x * 100.0) * 73_856_093.0)
        ^ to_int32(js_round(c.z * 100.0) * 19_349_663.0)) as u32;
    let mut rng = Random::new(f64::from(seed));
    let family = (rng.next() * 6.0).floor() as usize;
    let twist = rng.range(0.0, PI * 2.0);
    let height = c.h * rng.range(0.9, 1.07);
    let radius = c.w.min(c.d)
        * match family {
            3 => 0.14,
            4 | 5 => 0.1,
            _ => 0.12,
        };
    let stump_height = radius * rng.range(1.5, 1.9);
    TreeProportions {
        seed,
        rng,
        family,
        twist,
        height,
        radius,
        stump_height,
        stump_radius: radius * 1.25,
    }
}

/// A built tree and the traits the TypeScript kept in `userData`.
#[derive(Clone, Debug)]
pub struct TreeModel {
    pub node: Node,
    /// Index into [`TREE_FAMILIES`].
    pub family: usize,
    pub seed: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TreeSurface {
    Bark,
    Birch,
    Rings,
    Leaves,
    ConiferSpray,
}

impl TreeSurface {
    fn texture(self) -> &'static str {
        match self {
            TreeSurface::Bark => "textures/trees/bark.webp",
            TreeSurface::Birch => "textures/trees/birch.webp",
            TreeSurface::Rings => "textures/trees/rings.webp",
            TreeSurface::Leaves => "textures/trees/leaves.webp",
            TreeSurface::ConiferSpray => "textures/trees/conifer-spray.webp",
        }
    }
}

static SURFACES: Cache<(TreeSurface, u32), Material> = Cache::new();
static GEOMETRY: Cache<TreeGeometry, Mesh> = Cache::new();

/// `surface(kind, color)`: shared textured tree materials. Needle sprays are
/// alpha-tested cards drawn from both sides with alpha to coverage.
fn surface(kind: TreeSurface, color: u32) -> Arc<Material> {
    SURFACES.get_or_insert((kind, color), || {
        let map = TextureRef {
            anisotropy: 4,
            ..TextureRef::file(kind.texture())
        };
        let spray = kind == TreeSurface::ConiferSpray;
        Material {
            map: Some(map.clone()),
            color: crate::scene::Color(color),
            roughness: 1.0,
            metalness: 0.0,
            bump_map: (!spray).then_some(map),
            bump_scale: if kind == TreeSurface::Bark {
                0.055
            } else {
                0.018
            },
            alpha_test: if spray { 0.35 } else { 0.0 },
            alpha_to_coverage: spray,
            side: if spray { Side::Double } else { Side::Front },
            ..Material::default()
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TreeGeometry {
    Stem,
    ConiferStem,
    Branch,
    Root,
    SmallCrown,
    Spray,
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
        ..CylinderGeometry::default()
    }
    .build()
}

fn geometry(kind: TreeGeometry) -> Arc<Mesh> {
    GEOMETRY.get_or_insert(kind, || match kind {
        TreeGeometry::Stem => open_cylinder(0.6, 8),
        TreeGeometry::ConiferStem => open_cylinder(0.025, 7),
        TreeGeometry::Branch => open_cylinder(0.6, 5),
        TreeGeometry::Root => open_cylinder(0.08, 5),
        TreeGeometry::SmallCrown => icosahedron_geometry(1.0, 0),
        TreeGeometry::Spray => spray_geometry(),
        TreeGeometry::StumpBark(variant) => stump_geometry(variant).0,
        TreeGeometry::StumpCut(variant) => stump_geometry(variant).1,
    })
}

/// Three intersecting needle cards keep volume from the overhead camera and at the
/// horizon; a complete spray is six triangles.
fn spray_geometry() -> Mesh {
    let mut vertices = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    for card in 0..3u32 {
        let angle = (f64::from(card) * PI) / 3.0;
        for [x, z] in [[-0.5, 0.0], [0.5, 0.0], [-0.5, 1.0], [0.5, 1.0]] {
            vertices.extend_from_slice(&[x * angle.cos(), x * angle.sin(), z]);
            uvs.extend_from_slice(&[x + 0.5, z]);
        }
        let n = card * 4;
        indices.extend_from_slice(&[n, n + 2, n + 1, n + 1, n + 2, n + 3]);
    }
    let mut mesh = Mesh::from_f64(&vertices, &[], &uvs, Some(indices));
    mesh.compute_vertex_normals();
    mesh
}

/// `stumpGeometry(variant)`: irregular flared bark and the matching cut surface.
fn stump_geometry(variant: u32) -> (Mesh, Mesh) {
    let mut rng = Random::new(f64::from(variant) * 17597.0 + 79.0);
    let sides = 10u32;
    let radii: Vec<f64> = (0..sides).map(|_| rng.range(0.9, 1.1)).collect();
    let tops: Vec<f64> = (0..sides).map(|_| rng.range(0.91, 1.09)).collect();
    let angle = |j: u32| (f64::from(j) / f64::from(sides)) * PI * 2.0;
    let mut vertices = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    for row in 0..3u32 {
        for i in 0..=sides {
            let j = i % sides;
            let a = angle(j);
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
    let mut cap_vertices = Vec::new();
    let mut cap_uvs = Vec::new();
    for i in 0..sides as i32 {
        for j in [-1, i, (i + 1) % sides as i32] {
            let (x, z, y) = if j < 0 {
                (0.0, 0.0, 0.91)
            } else {
                let a = angle(j as u32);
                let r = radii[j as usize];
                (a.sin() * r, a.cos() * r, tops[j as usize])
            };
            cap_vertices.extend_from_slice(&[x, y, z]);
            cap_uvs.extend_from_slice(&[0.5 + x / 2.4, 0.5 + z / 2.4]);
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

const LEAF_COLORS: [u32; 6] = [0x9eb783, 0x80a69a, 0xa3bd8e, 0x5c8c35, 0x80a64c, 0x9aae43];
const LEAF_SHADES: [u32; 3] = [0xb1c7a4, 0xd9e2c0, 0xffffff];
const BARK_COLOR: u32 = 0xd0b598;
const BIRCH_COLORS: [u32; 2] = [0xe5ddc5, 0xc4c6a0];
const RINGS_COLOR: u32 = 0xd9b77f;
/// Golden-angle step between successive whorls and lobes.
const GOLDEN_ANGLE: f64 = 2.39996;

/// Where new boughs go: straight into the crown, or into a shedding group.
struct Crown {
    node: Node,
    detail: TreeDetail,
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
    } = tree_proportions(c);
    let full = detail == TreeDetail::Full;
    let conifer = family < 3;
    let pale = family == 4 || family == 5;
    let bark = if pale {
        surface(TreeSurface::Birch, BIRCH_COLORS[family - 4])
    } else {
        surface(TreeSurface::Bark, BARK_COLOR)
    };
    let leaves = LEAF_SHADES.map(|tint| {
        surface(
            if conifer {
                TreeSurface::ConiferSpray
            } else {
                TreeSurface::Leaves
            },
            multiply_hex(LEAF_COLORS[family], tint),
        )
    });
    let mut group = Node {
        position: DVec3::new(c.x, 0.0, c.z),
        ..Node::default()
    };
    let mut crown = Crown {
        node: Node::group(tree_part::CROWN),
        detail,
    };
    if full {
        let mut stump = Node::group(tree_part::STUMP);
        let variant = seed % 24;
        let scale = DVec3::new(radius, stump_height, radius);
        add_mesh(
            &mut stump,
            geometry(TreeGeometry::StumpBark(variant)),
            bark.clone(),
            DVec3::ZERO,
            scale,
        )
        .set_rotation_euler(0.0, twist, 0.0);
        let root_count = 5 + seed % 3;
        for i in 0..root_count {
            let angle =
                twist + (f64::from(i) * PI * 2.0) / f64::from(root_count) + rng.range(-0.18, 0.18);
            let reach = radius * rng.range(2.0, 3.0);
            let from = DVec3::new(
                angle.sin() * radius * 0.5,
                stump_height * 0.5,
                angle.cos() * radius * 0.5,
            );
            let to = DVec3::new(angle.sin() * reach, 0.035, angle.cos() * reach);
            limb(
                &mut stump,
                bark.clone(),
                from,
                to,
                radius * rng.range(0.3, 0.5),
                true,
            );
        }
        batch(&mut stump);
        let cut = add_mesh(
            &mut stump,
            geometry(TreeGeometry::StumpCut(variant)),
            surface(TreeSurface::Rings, RINGS_COLOR),
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
    let trunk = limb(
        &mut crown.node,
        bark.clone(),
        DVec3::new(0.0, if full { stump_height * 0.88 } else { 0.0 }, 0.0),
        DVec3::new(lean_x, height * (if conifer { 0.98 } else { 0.78 }), lean_z),
        radius,
        false,
    );
    if conifer {
        if let Some(drawable) = &mut trunk.drawable {
            drawable.mesh = geometry(TreeGeometry::ConiferStem);
        }
        add_conifer_crown(
            &mut crown, c, &mut rng, &bark, &leaves, family, twist, height, radius, lean_x, lean_z,
        );
    } else {
        add_broadleaf_crown(
            &mut crown, c, &mut rng, &bark, &leaves, family, twist, height, radius, lean_x, lean_z,
        );
    }
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
    leaves: &[Arc<Material>; 3],
    family: usize,
    twist: f64,
    height: f64,
    radius: f64,
    lean_x: f64,
    lean_z: f64,
) {
    let full = crown.detail == TreeDetail::Full;
    let tiers = if full { 7 } else { 6 };
    let arms = if full { 6 } else { 5 };
    // Pines carry a looser, higher crown; spruce and fir retain their lower boughs.
    let base = match family {
        0 => 0.36,
        1 => 0.17,
        _ => 0.23,
    };
    for i in 0..tiers {
        let t = f64::from(i) / f64::from(tiers - 1);
        let y = height * (base + t * (0.87 - base));
        let span = c.w * (if family == 2 { 0.44 } else { 0.5 }) * (1.0 - t * 0.76);
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
                let shoot = add_mesh(
                    &mut crown.node,
                    geometry(TreeGeometry::Spray),
                    leaves[i as usize % 3].clone(),
                    DVec3::new(start.x, start.y - height * 0.06, start.z),
                    DVec3::new(span * 0.85, span * 0.85, height * (0.27 - t * 0.1)),
                );
                shoot.rotation = rotate_z(quat_from_euler(-PI / 2.0, 0.0, 0.0), angle);
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
                let end = DVec3::new(
                    start.x + angle.sin() * reach * 0.92,
                    start.y + rise,
                    start.z + angle.cos() * reach * 0.92,
                );
                limb(
                    crown.parent(bough),
                    bark.clone(),
                    start,
                    end,
                    radius * (0.17 - t * 0.1),
                    false,
                );
            }
            let spray = add_mesh(
                crown.parent(bough),
                geometry(TreeGeometry::Spray),
                leaves[(i + j) as usize % 3].clone(),
                start,
                DVec3::new(
                    reach * (if family == 0 { 0.95 } else { 0.85 }),
                    height * (0.25 - t * 0.12),
                    reach * 1.1,
                ),
            );
            spray.rotation = quat_from_euler_yxz(-rise.atan2(reach), angle, rng.range(-0.2, 0.2));
        }
    }
    let leader = add_mesh(
        &mut crown.node,
        geometry(TreeGeometry::Spray),
        leaves[2].clone(),
        DVec3::new(lean_x * 0.9, height * 0.82, lean_z * 0.9),
        DVec3::new(c.w * 0.2, c.w * 0.2, height * 0.2),
    );
    leader.rotation = rotate_z(quat_from_euler(-PI / 2.0, 0.0, 0.0), twist);
}

#[allow(clippy::too_many_arguments)]
fn add_broadleaf_crown(
    crown: &mut Crown,
    c: &TreeShape,
    rng: &mut Random,
    bark: &Arc<Material>,
    leaves: &[Arc<Material>; 3],
    family: usize,
    twist: f64,
    height: f64,
    radius: f64,
    lean_x: f64,
    lean_z: f64,
) {
    let full = crown.detail == TreeDetail::Full;
    let count = if full { 7 } else { 6 };
    for i in 0..count {
        let a = twist + f64::from(i) * GOLDEN_ANGLE;
        let t = f64::from(i) / f64::from(count - 1);
        let spread = (1.0 - t * 0.65) * (if family == 3 { 0.27 } else { 0.2 });
        let center = DVec3::new(
            lean_x + a.sin() * c.w * spread,
            height * (0.48 + t * 0.37),
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
        let size = c.w * (if family == 3 { 0.3 } else { 0.25 }) * rng.range(0.84, 1.09);
        // Smaller overlapping lobes give the crown an irregular, branching silhouette.
        let lobes = if full { 4 } else { 1 };
        for j in 0..lobes {
            let angle = a + f64::from(j) * GOLDEN_ANGLE;
            let reach = if j == 0 { 0.0 } else { size * 0.55 };
            let tip = center
                + DVec3::new(
                    angle.sin() * reach,
                    rng.range(-0.25, 0.35) * size,
                    angle.cos() * reach,
                );
            // Damage sheds small outer twigs, never an entire section of the crown.
            let bough = if j == lobes - 1 && i < 4 {
                crown.branch_parent(if i < 2 { 1 } else { 2 })
            } else {
                None
            };
            if full && bough.is_some() {
                limb(
                    crown.parent(bough),
                    bark.clone(),
                    center,
                    tip,
                    radius * 0.075,
                    false,
                );
            }
            let scale = DVec3::new(
                size * rng.range(0.48, 0.66),
                size * rng.range(0.55, 0.85),
                size * rng.range(0.45, 0.65),
            );
            let lobe = add_mesh(
                crown.parent(bough),
                geometry(TreeGeometry::SmallCrown),
                leaves[(i + j) as usize % 3].clone(),
                tip,
                scale,
            );
            let rx = rng.range(-0.5, 0.5);
            let rz = rng.range(-0.4, 0.4);
            lobe.set_rotation_euler(rx, angle, rz);
        }
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
