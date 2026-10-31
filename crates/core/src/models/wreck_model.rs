//! Port of `wreck-model.ts`: the burnt-out assemblies a destroyed vehicle leaves.
//!
//! Extract the part from a live model, center it, flatten every mesh into one
//! group with its transform baked, and batch once. Presentation registers each of
//! the chassis × teams × assemblies once per page and draws instances of it, so the
//! wreck is built on request rather than cached: a cache kept every wreck's
//! vertices on the CPU for good after the GPU had them (about 20 MB of the heap).
//!
//! The batched paint is then scorched: soot blooms over most of the armor, heavier
//! on upward faces where the fire licked, with rust-brown blistered patches and
//! only a trace of the team paint left. Presentation darkens the whole wreck further
//! as it ages.

use std::sync::{Arc, OnceLock};

use glam::{DMat4, DQuat, DVec2, DVec3};

use super::batching::batch;
use super::humvee_model::wreck_openings as humvee_wreck_openings;
use super::tank_model::{WreckPose, tank_model_variant, wreck_openings, wreck_pose};
use super::tank_surfaces::{
    is_camouflaged, is_signal_panel, scorched_paint_material, sooted_glass, unpaint,
};
use super::{Team, VehicleKind, part};
use crate::geometry::Mesh;
use crate::geometry::math::{compose, decompose, hex_to_linear};
use crate::geometry::node_bounds;
use crate::scene::Node;
use crate::sim::data::vehicle;
use crate::sim::tank_dimensions::tank_hull;

pub use crate::sim::types::WreckPart;

/// Intact wrecks are centred on this fixed point rather than their bounds, so
/// they rest where the live vehicle stood.
const INTACT_CENTER: DVec3 = DVec3::new(0.0, 0.55, 0.0);

/// An opening a burning wreck's fire comes out of, in the wreck piece's own
/// centred frame before the vehicle scale (the frame [`wreck_model`] builds in).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WreckVent {
    /// The middle of the opening's top.
    pub center: DVec3,
    /// Its half size across (x) and along (z) the vehicle.
    pub half_size: DVec2,
    /// How deep its floor lies below its top: zero for grilles and hatches on a
    /// flat deck, the well's depth for the open turret ring.
    pub depth: f64,
    /// The main blaze, which feeds the smoke column; the others lick out beside it.
    pub main: bool,
}

/// Where a wreck piece burns, main blaze first. A hull whose turret flew off roars
/// out of the open turret ring and licks out of its engine deck; a burnt-out tank
/// burns in its engine bay and out of the hatches the blast blew open; a Humvee
/// out of its engine bay and its gunner's ring. Thrown turrets and guns do not burn.
pub fn wreck_vents(kind: VehicleKind, wreck_part: WreckPart) -> &'static [WreckVent] {
    static VENTS: OnceLock<[[Vec<WreckVent>; 2]; 4]> = OnceLock::new();
    let vents = VENTS.get_or_init(|| {
        VehicleKind::ALL
            .map(|kind| [WreckPart::Intact, WreckPart::Hull].map(|part| vents(kind, part)))
    });
    match wreck_part {
        WreckPart::Intact => &vents[kind.index()][0],
        WreckPart::Hull => &vents[kind.index()][1],
        WreckPart::Turret | WreckPart::TurretBarrel | WreckPart::Barrel => &[],
    }
}

fn vents(kind: VehicleKind, wreck_part: WreckPart) -> Vec<WreckVent> {
    let flat = |(top, half_size)| (top, half_size, 0.0);
    let openings: Vec<(DVec3, DVec2, f64)> = match wreck_openings(kind) {
        Some(tank) if wreck_part == WreckPart::Hull => vec![tank.ring, flat(tank.engine)],
        Some(tank) => [tank.engine]
            .into_iter()
            .chain(tank.hatches)
            .map(flat)
            .collect(),
        None => humvee_wreck_openings().map(flat).to_vec(),
    };
    let center = wreck_center(kind, wreck_part);
    openings
        .into_iter()
        .enumerate()
        .map(|(i, (top, half_size, depth))| WreckVent {
            center: top - center,
            half_size,
            depth,
            main: i == 0,
        })
        .collect()
}

/// The point a wreck piece is centred on (`build_wreck`): a whole wreck's fixed
/// rest point, a hull's bounds measured once with the live vehicle's (the open
/// turret ring stays inside them), divided out of the vehicle scale.
fn wreck_center(kind: VehicleKind, wreck_part: WreckPart) -> DVec3 {
    match wreck_part {
        WreckPart::Intact => INTACT_CENTER,
        _ => tank_hull(kind).center / vehicle(kind).scale,
    }
}

/// `wreckModel(kind, team, part)`: a flat group of batched meshes in the part's
/// centred frame. The vehicle's overall scale is not applied (the parts leave the
/// scaled root), as in the TypeScript.
pub fn wreck_model(kind: VehicleKind, team: Team, wreck_part: WreckPart) -> Arc<Node> {
    Arc::new(build_wreck(kind, team, wreck_part))
}

fn take_child(parent: &mut Node, name: &str) -> Node {
    let index = parent
        .children
        .iter()
        .position(|child| child.name == name)
        .unwrap_or_else(|| panic!("vehicle models have a {name} part"));
    parent.children.remove(index)
}

fn build_wreck(kind: VehicleKind, team: Team, wreck_part: WreckPart) -> Node {
    let mut source = (*tank_model_variant(kind, team, wreck_part == WreckPart::Hull)).clone();
    let hull = take_child(&mut source, part::HULL);
    let mut turret = take_child(&mut source, part::TURRET);
    let pose = wreck_pose(kind);
    if let Some(pose) = &pose
        && matches!(wreck_part, WreckPart::Intact | WreckPart::TurretBarrel)
    {
        knock_askew(&mut turret, pose, wreck_part == WreckPart::Intact);
    }
    let mut result = Node::group("wreck");
    result.children = match wreck_part {
        WreckPart::Intact => vec![hull, turret],
        WreckPart::Hull => vec![hull],
        WreckPart::Barrel => vec![take_child(&mut turret, part::BARREL)],
        WreckPart::Turret => {
            take_child(&mut turret, part::BARREL);
            vec![turret]
        }
        WreckPart::TurretBarrel => vec![turret],
    };
    let center = if wreck_part == WreckPart::Intact {
        INTACT_CENTER
    } else {
        node_bounds(&result, DMat4::IDENTITY).center()
    };
    for child in &mut result.children {
        child.position -= center;
    }
    let mut flat = Node::group("wreck");
    collect_meshes(&result, DMat4::IDENTITY, &mut flat.children);
    // Soot dulls the windows: glass batches with the rest as plain paint.
    for drawable in flat
        .children
        .iter_mut()
        .filter_map(|child| child.drawable.as_mut())
    {
        if let Some(glass) = sooted_glass(&drawable.material) {
            drawable.material = glass;
        }
    }
    // Cloth recognition panels burn away with the vehicle.
    flat.children.retain(|child| {
        child
            .drawable
            .as_ref()
            .is_none_or(|drawable| !is_signal_panel(&drawable.material))
    });
    batch(&mut flat);
    for child in &mut flat.children {
        if let Some(drawable) = &mut child.drawable
            && drawable.material.vertex_colors
        {
            let mut mesh = (*drawable.mesh).clone();
            if is_camouflaged(&drawable.material) {
                unpaint(&mut mesh, &drawable.material);
                burn_paint(&mut mesh);
                drawable.material = scorched_paint_material();
            } else {
                scorch(&mut mesh);
            }
            drawable.mesh = Arc::new(mesh);
        }
    }
    flat
}

/// Turn a wreck's turret on its ring and droop its gun on the trunnions, rotating
/// the parts about those axes; with `hatches_blown`, embers glow in its hatches.
fn knock_askew(turret: &mut Node, pose: &WreckPose, hatches_blown: bool) {
    let about = |pivot: DVec3, rotation: DQuat| {
        DMat4::from_translation(pivot)
            * DMat4::from_quat(rotation)
            * DMat4::from_translation(-pivot)
    };
    let traverse = about(pose.ring, DQuat::from_rotation_y(pose.turret_yaw));
    let droop = about(pose.trunnion, DQuat::from_rotation_x(pose.gun_droop));
    if let Some(barrel) = turret
        .children
        .iter_mut()
        .find(|child| child.name == part::BARREL)
    {
        set_matrix(barrel, droop * barrel.local_matrix());
    }
    if hatches_blown {
        turret.children.push(pose.embers.clone());
    }
    let local = turret.local_matrix();
    set_matrix(turret, traverse * local);
}

fn set_matrix(node: &mut Node, matrix: DMat4) {
    let (position, rotation, scale) = decompose(&matrix);
    node.position = position;
    node.rotation = rotation;
    node.scale = scale;
}

/// The pale ash base the scorch tile darkens, and the team paint left on the
/// lower sides.
const ASH: u32 = 0xb3aba1;
const PAINT_LEFT: f64 = 0.35;

/// Vertex colors of a wreck's burnt paint: ash on upward faces, a trace of the
/// darkened team paint in blotches on the sides.
fn burn_paint(mesh: &mut Mesh) {
    let ash = hex_to_linear(ASH);
    let normals = mesh.normals.clone();
    let mut blotches = ValueNoise::new();
    for (i, color) in mesh.colors.iter_mut().enumerate() {
        let p = DVec3::from(mesh.positions[i].map(f64::from)) * SCORCH_FREQUENCY;
        let up = normals.get(i).map_or(0.0, |n| f64::from(n[1]).max(0.0));
        let left = PAINT_LEFT * (1.0 - up) * smooth01((blotches.at(p) - 0.35) / 0.3);
        for c in 0..3 {
            let paint = f64::from(color[c]) * 0.7;
            color[c] = (ash[c] + (paint - ash[c]) * left) as f32;
        }
    }
}

/// Soot and blistered rust (sRGB) of a burnt-out hull.
const SOOT: u32 = 0x1d1916;
const RUST: u32 = 0x6b3b22;
/// Soot coverage: the least any vertex gets, how much the blotches add and the
/// extra on upward faces.
const SOOT_MIN: f64 = 0.3;
const SOOT_BLOTCH: f64 = 0.42;
const SOOT_TOP: f64 = 0.12;
/// Blotch scale (per metre) and how much of a rust patch shows.
const SCORCH_FREQUENCY: f64 = 2.3;
const RUST_STRENGTH: f64 = 0.55;

/// Burn the vertex colors of a batched wreck mesh.
fn scorch(mesh: &mut Mesh) {
    let soot = hex_to_linear(SOOT);
    let rust = hex_to_linear(RUST);
    let normals = mesh.normals.clone();
    let (mut blotches, mut rust_patches) = (ValueNoise::new(), ValueNoise::new());
    for (i, color) in mesh.colors.iter_mut().enumerate() {
        let p = DVec3::from(mesh.positions[i].map(f64::from)) * SCORCH_FREQUENCY;
        let up = normals.get(i).map_or(0.0, |n| f64::from(n[1]).max(0.0));
        let blotch = blotches.at(p);
        let rusty = smooth01((rust_patches.at(p * 1.7 + DVec3::splat(19.3)) - 0.58) / 0.12);
        let burnt = (SOOT_MIN + SOOT_BLOTCH * blotch + SOOT_TOP * up).min(0.95);
        for c in 0..3 {
            let paint = f64::from(color[c]);
            let rusted = paint + (rust[c] - paint) * rusty * RUST_STRENGTH;
            color[c] = (rusted + (soot[c] - rusted) * burnt) as f32;
        }
    }
}

fn smooth01(x: f64) -> f64 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Smooth 3D value noise in [0, 1] on a unit lattice. It keeps the corner values
/// of the last cell it sampled: a batched wreck lists its vertices triangle by
/// triangle, part by part, so runs of them fall in one cell (the lattice is about
/// 0.4 m) and hashing its eight corners for every vertex was most of a wreck's
/// build.
struct ValueNoise {
    cell: DVec3,
    /// The cell's corners, x fastest, then y, then z.
    corners: [f64; 8],
}

impl ValueNoise {
    fn new() -> Self {
        Self {
            cell: DVec3::NAN,
            corners: [0.0; 8],
        }
    }

    fn at(&mut self, p: DVec3) -> f64 {
        let cell = p.floor();
        let f = p - cell;
        let s = f * f * (DVec3::splat(3.0) - 2.0 * f);
        // NaN never equals itself, so the first sample (and any NaN cell) hashes.
        if cell != self.cell {
            self.cell = cell;
            self.corners = [0, 1, 2, 3, 4, 5, 6, 7].map(|k| {
                lattice_value(
                    cell + DVec3::new(f64::from(k & 1), f64::from(k >> 1 & 1), f64::from(k >> 2)),
                )
            });
        }
        let c = self.corners;
        let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
        let x00 = lerp(c[0], c[1], s.x);
        let x10 = lerp(c[2], c[3], s.x);
        let x01 = lerp(c[4], c[5], s.x);
        let x11 = lerp(c[6], c[7], s.x);
        lerp(lerp(x00, x10, s.y), lerp(x01, x11, s.y), s.z)
    }
}

/// The noise's value at a lattice corner.
fn lattice_value(q: DVec3) -> f64 {
    let mut h = (q.x as i64).wrapping_mul(73_856_093)
        ^ (q.y as i64).wrapping_mul(19_349_663)
        ^ (q.z as i64).wrapping_mul(83_492_791);
    h = (h ^ (h >> 13)).wrapping_mul(0x5bd1_e995);
    (h ^ (h >> 15)).rem_euclid(1000) as f64 / 999.0
}

/// Every mesh in the tree, in traversal order, with its parent's world transform
/// applied (`mesh.applyMatrix4(mesh.parent.matrixWorld)`): the product is
/// decomposed back into position, rotation and scale, so a sheared product loses
/// its shear exactly as Three.js did.
fn collect_meshes(node: &Node, parent_world: DMat4, out: &mut Vec<Node>) {
    // Hidden alternatives (the liveries a wreck does not wear) and the live
    // turret's deck occlusion stay out.
    if !node.visible || node.name == part::TURRET_OCCLUSION {
        return;
    }
    let world = parent_world * compose(node.position, node.rotation, node.scale);
    if node.drawable.is_some() {
        let (position, rotation, scale) = decompose(&world);
        out.push(Node {
            position,
            rotation,
            scale,
            children: Vec::new(),
            ..node.clone()
        });
    }
    for child in &node.children {
        collect_meshes(child, world, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3Swizzles;

    use crate::geometry::Aabb;
    use crate::models::tank_surfaces::ember_material;

    /// The bounds of a wreck's glowing embers: the open turret ring's floor, or
    /// the blown hatches. Batching bakes their paint into vertex colours but keeps
    /// their glow.
    fn embers(kind: VehicleKind, wreck_part: WreckPart) -> Aabb {
        let ember = ember_material();
        let mut glow = Node::group("embers");
        glow.children = wreck_model(kind, Team::Blue, wreck_part)
            .children
            .iter()
            .filter(|child| {
                child.drawable.as_ref().is_some_and(|drawable| {
                    drawable.material.emissive == ember.emissive
                        && drawable.material.emissive_intensity == ember.emissive_intensity
                })
            })
            .cloned()
            .collect();
        node_bounds(&glow, DMat4::IDENTITY)
    }

    #[test]
    fn wrecks_burn_out_of_their_openings() {
        for kind in VehicleKind::ALL {
            for part in [WreckPart::Intact, WreckPart::Hull] {
                let vents = wreck_vents(kind, part);
                assert_eq!(vents.iter().filter(|vent| vent.main).count(), 1);
                assert!(vents[0].main, "{kind:?} {part:?}: the main blaze first");
                let bounds = node_bounds(&wreck_model(kind, Team::Blue, part), DMat4::IDENTITY);
                for vent in vents {
                    // Over the piece and up on it (an intact wreck's turret stands
                    // above its engine deck), neither floating above it nor buried
                    // in its belly.
                    let (low, high) = (bounds.min - 0.01, bounds.max + 0.01);
                    let c = vent.center;
                    assert!(
                        c.x - vent.half_size.x >= low.x
                            && c.x + vent.half_size.x <= high.x
                            && c.z - vent.half_size.y >= low.z
                            && c.z + vent.half_size.y <= high.z
                            && c.y > bounds.min.y + 0.4 * bounds.size().y
                            && c.y <= high.y,
                        "{kind:?} {part:?} {vent:?} {bounds:?}"
                    );
                }
            }
            for part in [
                WreckPart::Turret,
                WreckPart::TurretBarrel,
                WreckPart::Barrel,
            ] {
                assert!(wreck_vents(kind, part).is_empty());
            }
            if kind == VehicleKind::Humvee {
                continue;
            }
            // The turret ring's fire stands in its smouldering well, and a burnout's
            // hatch fires in the hatches whose embers glow.
            let ring = embers(kind, WreckPart::Hull);
            let vent = wreck_vents(kind, WreckPart::Hull)[0];
            assert!(
                (ring.center() - vent.center).xz().length() < 0.01,
                "{kind:?} {ring:?} {vent:?}"
            );
            assert!(
                (ring.size().x / 2.0 - vent.half_size.x).abs() < 0.01,
                "{kind:?} {ring:?} {vent:?}"
            );
            // Its flames rise from the well's floor.
            assert!(
                (ring.max.y - (vent.center.y - vent.depth)).abs() < 0.01,
                "{kind:?} {ring:?} {vent:?}"
            );
            let hatches = embers(kind, WreckPart::Intact);
            let [_, first, second] = wreck_vents(kind, WreckPart::Intact) else {
                panic!("{kind:?}: engine deck and two hatches");
            };
            let middle = (first.center + second.center) / 2.0;
            assert!((hatches.center() - middle).xz().length() < 0.03, "{kind:?}");
        }
    }
}
