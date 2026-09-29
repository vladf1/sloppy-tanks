//! Port of `model-primitives.ts`: shared materials, box and cylinder parts.
//!
//! Cached geometry and materials are shared across rounds; callers own only
//! transforms. Caches are keyed like the TypeScript (by value), so equal requests
//! return the same `Arc` and the renderer can batch parts that share them.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::geometry::{Mesh, box_geometry, cylinder_geometry, rounded_box_geometry};
use crate::scene::{Color, Material, Node};

pub use crate::sim::data::TEAM_COLORS;

/// Default metalness and roughness of `material()`.
pub const DEFAULT_METALNESS: f64 = 0.05;
pub const DEFAULT_ROUGHNESS: f64 = 0.65;
/// Default corner radius of `box()`.
pub const DEFAULT_BOX_RADIUS: f64 = 0.06;
/// Metalness of `cylinder()` parts.
pub const CYLINDER_METALNESS: f64 = 0.2;
/// Team paint glows faintly and skips tone mapping so it stays saturated.
const TEAM_EMISSIVE_INTENSITY: f32 = 0.04;

/// A process-wide cache of shared values keyed by their construction parameters.
pub(crate) struct Cache<K, V> {
    entries: OnceLock<Mutex<HashMap<K, Arc<V>>>>,
}

impl<K: std::hash::Hash + Eq, V> Cache<K, V> {
    pub(crate) const fn new() -> Self {
        Self {
            entries: OnceLock::new(),
        }
    }

    pub(crate) fn get_or_insert(&self, key: K, build: impl FnOnce() -> V) -> Arc<V> {
        let entries = self.entries.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some(value) = entries.lock().expect("cache lock").get(&key) {
            return value.clone();
        }
        // Build outside the lock: builders may use other caches (or this one).
        let value = Arc::new(build());
        entries
            .lock()
            .expect("cache lock")
            .entry(key)
            .or_insert(value)
            .clone()
    }
}

static MATERIALS: Cache<(u32, u64, u64), Material> = Cache::new();
static BOXES: Cache<[u64; 4], Mesh> = Cache::new();
static CYLINDERS: Cache<(u64, u64, u32), Mesh> = Cache::new();

/// `material(color, metalness, roughness)`: a shared MeshStandardMaterial. Team
/// colors also glow with their own color.
pub fn material(color: u32, metalness: f64, roughness: f64) -> Arc<Material> {
    MATERIALS.get_or_insert((color, metalness.to_bits(), roughness.to_bits()), || {
        let mut material = Material::standard(color, metalness as f32, roughness as f32);
        if TEAM_COLORS.contains(&color) {
            material.emissive = Color(color);
            material.emissive_intensity = TEAM_EMISSIVE_INTENSITY;
            material.tone_mapped = false;
        }
        material
    })
}

/// `material(color)` with the default metalness and roughness.
pub fn paint(color: u32) -> Arc<Material> {
    material(color, DEFAULT_METALNESS, DEFAULT_ROUGHNESS)
}

/// A mesh part that casts and receives shadows.
pub fn shadowed(mesh: Arc<Mesh>, material: Arc<Material>) -> Node {
    let mut node = Node::mesh(mesh, material);
    if let Some(drawable) = &mut node.drawable {
        drawable.cast_shadow = true;
        drawable.receive_shadow = true;
    }
    node
}

/// A mesh part that only receives shadows.
pub fn shadow_receiver(mesh: Arc<Mesh>, material: Arc<Material>) -> Node {
    let mut node = Node::mesh(mesh, material);
    if let Some(drawable) = &mut node.drawable {
        drawable.receive_shadow = true;
    }
    node
}

/// `box(w, h, d, color, r)`: a rounded box (one segment) when `radius > 0`,
/// otherwise a plain box; geometry is shared per size and radius.
pub fn box_part(width: f64, height: f64, depth: f64, color: u32, radius: f64) -> Node {
    let key = [width, height, depth, radius].map(f64::to_bits);
    let mesh = BOXES.get_or_insert(key, || {
        if radius > 0.0 {
            rounded_box_geometry(width, height, depth, 1, radius)
        } else {
            box_geometry(width, height, depth)
        }
    });
    shadowed(mesh, paint(color))
}

/// `cylinder(radius, height, color, sides)`: a y-axis cylinder with slightly
/// metallic paint; geometry is shared per radius, height and side count.
pub fn cylinder_part(radius: f64, height: f64, color: u32, sides: u32) -> Node {
    let mesh = CYLINDERS.get_or_insert((radius.to_bits(), height.to_bits(), sides), || {
        cylinder_geometry(radius, radius, height, sides)
    });
    shadowed(mesh, material(color, CYLINDER_METALNESS, DEFAULT_ROUGHNESS))
}

/// `put(parent, obj, x, y, z)`: position a part and append it to its parent.
pub fn put(parent: &mut Node, mut child: Node, x: f64, y: f64, z: f64) {
    child.position = glam::DVec3::new(x, y, z);
    parent.children.push(child);
}

/// A part rotated by Euler angles (Three's XYZ order), for `put`.
pub fn rotated(mut node: Node, x: f64, y: f64, z: f64) -> Node {
    node.set_rotation_euler(x, y, z);
    node
}
