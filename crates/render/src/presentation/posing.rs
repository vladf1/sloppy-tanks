//! Posing prepared model joints the way the TypeScript posed Three objects:
//! replace a named node's own position, rotation or scale while keeping any
//! unnamed groups between it and its parent joint.

use glam::{DQuat, DVec3, Mat4, Quat, Vec3};
use sloppy_core::scene::Node;
use sloppy_core::sim::math::{Point3, Quat4};

use crate::model::ModelNode;

/// A joint and the authored transform its pose starts from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointBasis {
    pub index: usize,
    /// Unnamed groups between the parent joint and this node.
    prefix: Mat4,
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl JointBasis {
    /// The joint `name` of a prepared model whose source tree is `source`.
    pub fn new(nodes: &[ModelNode], source: &Node, name: &str) -> Option<Self> {
        let index = nodes.iter().position(|node| node.name == name)?;
        let node = source.find(name)?;
        let local = Mat4::from_scale_rotation_translation(
            node.scale.as_vec3(),
            node.rotation.as_quat(),
            node.position.as_vec3(),
        );
        Some(Self {
            index,
            prefix: nodes[index].rest * local.inverse(),
            position: node.position.as_vec3(),
            rotation: node.rotation.as_quat(),
            scale: node.scale.as_vec3(),
        })
    }

    /// The joint's local transform with some of its own fields replaced.
    pub fn pose(&self, position: Vec3, rotation: Quat, scale: Vec3) -> Mat4 {
        self.prefix * Mat4::from_scale_rotation_translation(scale, rotation, position)
    }

    pub fn with_position(&self, position: Vec3) -> Mat4 {
        self.pose(position, self.rotation, self.scale)
    }

    pub fn with_rotation(&self, rotation: Quat) -> Mat4 {
        self.pose(self.position, rotation, self.scale)
    }

    pub fn with_scale(&self, scale: Vec3) -> Mat4 {
        self.pose(self.position, self.rotation, scale)
    }
}

/// World transform of joint `index`, given the root's world matrix and the
/// instance's joint overrides (`None` keeps the rest pose).
pub fn joint_world(
    nodes: &[ModelNode],
    overrides: &[Option<Mat4>],
    root: Mat4,
    index: usize,
) -> Mat4 {
    if index == 0 {
        return root;
    }
    let node = &nodes[index];
    let local = overrides.get(index).copied().flatten().unwrap_or(node.rest);
    let parent = node
        .parent
        .map_or(root, |parent| joint_world(nodes, overrides, root, parent));
    parent * local
}

pub fn vec3(point: Point3) -> Vec3 {
    Vec3::new(point.x as f32, point.y as f32, point.z as f32)
}

pub fn quat(rotation: Quat4) -> Quat {
    DQuat::from_xyzw(rotation.x, rotation.y, rotation.z, rotation.w)
        .as_quat()
        .normalize()
}

pub fn dvec3(point: Point3) -> DVec3 {
    DVec3::new(point.x, point.y, point.z)
}

/// The blend every interpolated pose and camera uses (core's `math::lerp` rounds
/// differently).
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Three's `Euler(x, y, z)` in its default XYZ order.
pub fn euler_xyz(x: f32, y: f32, z: f32) -> Quat {
    Quat::from_rotation_x(x) * Quat::from_rotation_y(y) * Quat::from_rotation_z(z)
}

/// Three's `Euler(x, y, z, "YXZ")`.
pub fn euler_yxz(x: f32, y: f32, z: f32) -> Quat {
    Quat::from_rotation_y(y) * Quat::from_rotation_x(x) * Quat::from_rotation_z(z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::MaterialInterner;
    use crate::model::prepare_model;
    use sloppy_core::models::{part, tank_model};
    use sloppy_core::sim::VehicleKind;

    #[test]
    fn rest_pose_round_trips_through_the_basis() {
        let source = tank_model(VehicleKind::Balanced, sloppy_core::sim::types::Team::Red);
        let mut interner = MaterialInterner::default();
        let prepared = prepare_model(&source, &mut interner, &|_| &[]);
        for name in [part::HULL, part::TURRET, part::BARREL, part::TRACK_GROUP] {
            let basis = JointBasis::new(&prepared.nodes, &source, name).expect(name);
            let rest = prepared.nodes[basis.index].rest;
            let posed = basis.pose(basis.position, basis.rotation, basis.scale);
            assert!(posed.abs_diff_eq(rest, 1e-5), "{name}");
        }
        let turret = JointBasis::new(&prepared.nodes, &source, part::TURRET).unwrap();
        let root = Mat4::from_translation(Vec3::new(3.0, 0.0, 0.0));
        let world = joint_world(&prepared.nodes, &[], root, turret.index);
        assert!((world.w_axis.x - 3.0 - turret.position.x).abs() < 1e-5);
    }
}
