//! Axis-aligned bounds (Three.js `Box3`) of meshes and model trees.

use glam::{DMat4, DVec3};

use super::math::transform_point;
use super::mesh::{Mesh, widen};
use crate::scene::Node;

/// An axis-aligned box. `Aabb::EMPTY` has inverted infinite bounds, like a fresh
/// `Box3`, so any point or box expands it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: DVec3,
    pub max: DVec3,
}

impl Aabb {
    pub const EMPTY: Self = Self {
        min: DVec3::INFINITY,
        max: DVec3::NEG_INFINITY,
    };

    pub fn from_points(points: impl IntoIterator<Item = DVec3>) -> Self {
        let mut bounds = Self::EMPTY;
        for point in points {
            bounds.expand_by_point(point);
        }
        bounds
    }

    pub fn is_empty(&self) -> bool {
        self.max.x < self.min.x || self.max.y < self.min.y || self.max.z < self.min.z
    }

    pub fn expand_by_point(&mut self, point: DVec3) {
        self.min = self.min.min(point);
        self.max = self.max.max(point);
    }

    pub fn union(&mut self, other: &Aabb) {
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
    }

    /// `Box3.applyMatrix4`: the box enclosing the eight transformed corners.
    pub fn transformed(&self, matrix: &DMat4) -> Aabb {
        if self.is_empty() {
            return *self;
        }
        let (lo, hi) = (self.min, self.max);
        Aabb::from_points(
            [
                DVec3::new(lo.x, lo.y, lo.z),
                DVec3::new(lo.x, lo.y, hi.z),
                DVec3::new(lo.x, hi.y, lo.z),
                DVec3::new(lo.x, hi.y, hi.z),
                DVec3::new(hi.x, lo.y, lo.z),
                DVec3::new(hi.x, lo.y, hi.z),
                DVec3::new(hi.x, hi.y, lo.z),
                DVec3::new(hi.x, hi.y, hi.z),
            ]
            .map(|corner| transform_point(matrix, corner)),
        )
    }

    /// `Box3.getCenter`; zero for an empty box.
    pub fn center(&self) -> DVec3 {
        if self.is_empty() {
            DVec3::ZERO
        } else {
            (self.min + self.max) * 0.5
        }
    }

    /// `Box3.getSize`; zero for an empty box.
    pub fn size(&self) -> DVec3 {
        if self.is_empty() {
            DVec3::ZERO
        } else {
            self.max - self.min
        }
    }
}

impl Mesh {
    /// `BufferGeometry.computeBoundingBox`: bounds of the stored f32 positions.
    pub fn bounding_box(&self) -> Aabb {
        Aabb::from_points(self.positions.iter().map(|&p| widen(p)))
    }
}

/// `new Box3().setFromObject(node)` with Three's default (non-precise) mode: every
/// drawable contributes its mesh bounding box transformed by its world matrix, so
/// rotated parts contribute their rotated box corners rather than their vertices.
/// An instanced drawable contributes the union of its f32 instance boxes first.
/// Visibility is ignored, as in Three. `parent_world` is the world matrix of the
/// node's parent (identity for a root).
pub fn node_bounds(node: &Node, parent_world: DMat4) -> Aabb {
    let mut bounds = Aabb::EMPTY;
    node.traverse(parent_world, &mut |part, world| {
        if let Some(drawable) = &part.drawable {
            let geometry = drawable.mesh.bounding_box();
            let local = match &drawable.instances {
                None => geometry,
                Some(instances) => {
                    let mut union = Aabb::EMPTY;
                    for instance in instances {
                        // InstancedMesh stores its matrices in a Float32Array.
                        let matrix = DMat4::from_cols_array(
                            &instance.matrix.to_cols_array().map(|v| f64::from(v as f32)),
                        );
                        union.union(&geometry.transformed(&matrix));
                    }
                    union
                }
            };
            bounds.union(&local.transformed(&world));
        }
    });
    bounds
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::geometry::box_geometry;
    use crate::scene::{Material, Node};

    #[test]
    fn rotated_box_bounds_use_transformed_corners() {
        let mut node = Node::mesh(
            Arc::new(box_geometry(2.0, 2.0, 2.0)),
            Arc::new(Material::default()),
        );
        node.rotation = glam::DQuat::from_rotation_y(std::f64::consts::FRAC_PI_4);
        let bounds = node_bounds(&node, DMat4::IDENTITY);
        assert!((bounds.max.x - 2f64.sqrt()).abs() < 1e-7);
        assert_eq!(bounds.center(), DVec3::ZERO);
    }
}
