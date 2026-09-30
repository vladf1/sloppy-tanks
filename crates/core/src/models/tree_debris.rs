//! The model half of `tree-debris.ts`: a detached, fade-ready copy of a shed bough.
//! The falling, landing and sinking state machine (at most `MAX_BRANCHES` boughs
//! for `LIFETIME` seconds) stays with presentation.

use std::sync::Arc;

use glam::DMat4;

use crate::geometry::math::decompose;
use crate::scene::{Material, Node};

/// A material copy that can fade: transparent without depth writes. Presentation
/// owns the copy and animates its `opacity` (TS `materials` per branch).
pub fn fading_material(source: &Material) -> Material {
    Material {
        transparent: true,
        depth_write: false,
        ..source.clone()
    }
}

/// `TreeDebris.branchModel(source)`: a visible copy of the bough placed at the
/// bough's world pose (`source_world`, from the tree's world matrix and the
/// bough's parents), with fade-ready copies of every material. Geometry is shared.
pub fn falling_branch_model(source: &Node, source_world: DMat4) -> Node {
    let mut model = source.clone();
    let (position, rotation, scale) = decompose(&source_world);
    model.position = position;
    model.rotation = rotation;
    model.scale = scale;
    model.visible = true;
    make_fading(&mut model);
    model
}

fn make_fading(node: &mut Node) {
    if let Some(drawable) = &mut node.drawable {
        drawable.material = Arc::new(fading_material(&drawable.material));
    }
    for child in &mut node.children {
        make_fading(child);
    }
}
