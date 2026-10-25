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

use std::sync::Arc;

use glam::{DMat4, DQuat, DVec3};

use super::batching::batch;
use super::tank_model::{WreckPose, tank_model_variant, wreck_pose};
use super::tank_surfaces::{
    is_camouflaged, is_signal_panel, scorched_paint_material, sooted_glass, unpaint,
};
use super::{Team, VehicleKind, part};
use crate::geometry::Mesh;
use crate::geometry::math::{compose, decompose, hex_to_linear};
use crate::geometry::node_bounds;
use crate::scene::Node;

pub use crate::sim::types::WreckPart;

/// Intact wrecks are centred on this fixed point rather than their bounds, so
/// they rest where the live vehicle stood.
const INTACT_CENTER: DVec3 = DVec3::new(0.0, 0.55, 0.0);

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
    for (i, color) in mesh.colors.iter_mut().enumerate() {
        let p = DVec3::from(mesh.positions[i].map(f64::from)) * SCORCH_FREQUENCY;
        let up = normals.get(i).map_or(0.0, |n| f64::from(n[1]).max(0.0));
        let left = PAINT_LEFT * (1.0 - up) * smooth01((value_noise(p) - 0.35) / 0.3);
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
    for (i, color) in mesh.colors.iter_mut().enumerate() {
        let p = DVec3::from(mesh.positions[i].map(f64::from)) * SCORCH_FREQUENCY;
        let up = normals.get(i).map_or(0.0, |n| f64::from(n[1]).max(0.0));
        let blotch = value_noise(p);
        let rusty = smooth01((value_noise(p * 1.7 + DVec3::splat(19.3)) - 0.58) / 0.12);
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

/// Smooth 3D value noise in [0, 1] on a unit lattice.
fn value_noise(p: DVec3) -> f64 {
    let cell = p.floor();
    let f = p - cell;
    let s = f * f * (DVec3::splat(3.0) - 2.0 * f);
    let corner = |dx: f64, dy: f64, dz: f64| {
        let q = cell + DVec3::new(dx, dy, dz);
        let mut h = (q.x as i64).wrapping_mul(73_856_093)
            ^ (q.y as i64).wrapping_mul(19_349_663)
            ^ (q.z as i64).wrapping_mul(83_492_791);
        h = (h ^ (h >> 13)).wrapping_mul(0x5bd1_e995);
        (h ^ (h >> 15)).rem_euclid(1000) as f64 / 999.0
    };
    let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let x00 = lerp(corner(0.0, 0.0, 0.0), corner(1.0, 0.0, 0.0), s.x);
    let x10 = lerp(corner(0.0, 1.0, 0.0), corner(1.0, 1.0, 0.0), s.x);
    let x01 = lerp(corner(0.0, 0.0, 1.0), corner(1.0, 0.0, 1.0), s.x);
    let x11 = lerp(corner(0.0, 1.0, 1.0), corner(1.0, 1.0, 1.0), s.x);
    lerp(lerp(x00, x10, s.y), lerp(x01, x11, s.y), s.z)
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
