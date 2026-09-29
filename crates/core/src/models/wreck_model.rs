//! Port of `wreck-model.ts`: the burnt-out assemblies a destroyed vehicle leaves.
//!
//! Extract the part from a live model, center it, flatten every mesh into one
//! group with its transform baked, and batch once. Bounded by chassis × teams ×
//! assemblies; geometry is shared across rounds, instances clone the node.

use std::sync::Arc;

use glam::{DMat4, DVec3};

use super::batching::batch;
use super::model_primitives::Cache;
use super::tank_model::tank_model_variant;
use super::{Team, VehicleKind, part};
use crate::geometry::math::{compose, decompose};
use crate::geometry::node_bounds;
use crate::scene::Node;

/// Which assembly a wreck piece is (TS `WreckPart`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WreckPart {
    /// Hull and turret together.
    Intact,
    /// The hull alone, with the turret ring cut open.
    Hull,
    /// The turret without its gun.
    Turret,
    /// The turret with its gun.
    TurretBarrel,
    /// The gun alone.
    Barrel,
}

/// Intact wrecks are centred on this fixed point rather than their bounds, so
/// they rest where the live vehicle stood.
const INTACT_CENTER: DVec3 = DVec3::new(0.0, 0.55, 0.0);

static WRECKS: Cache<(VehicleKind, Team, WreckPart), Node> = Cache::new();

/// `wreckModel(kind, team, part)`: a flat group of batched meshes in the part's
/// centred frame. The vehicle's overall scale is not applied (the parts leave the
/// scaled root), as in the TypeScript.
pub fn wreck_model(kind: VehicleKind, team: Team, wreck_part: WreckPart) -> Arc<Node> {
    WRECKS.get_or_insert((kind, team, wreck_part), || {
        build_wreck(kind, team, wreck_part)
    })
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
    let mut source =
        (*tank_model_variant(kind, team, false, wreck_part == WreckPart::Hull)).clone();
    let hull = take_child(&mut source, part::HULL);
    let mut turret = take_child(&mut source, part::TURRET);
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
    flat.children = flatten_meshes(&result);
    batch(&mut flat);
    flat
}

/// Every mesh in the tree, in traversal order, with its parent's world transform
/// applied (`mesh.applyMatrix4(mesh.parent.matrixWorld)`): the product is
/// decomposed back into position, rotation and scale, so a sheared product loses
/// its shear exactly as Three.js did.
fn flatten_meshes(root: &Node) -> Vec<Node> {
    let mut meshes = Vec::new();
    collect_meshes(root, DMat4::IDENTITY, &mut meshes);
    meshes
}

fn collect_meshes(node: &Node, parent_world: DMat4, out: &mut Vec<Node>) {
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
