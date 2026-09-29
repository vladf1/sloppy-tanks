//! CPU triangle meshes. Generators port the Three.js r185 geometries the game used,
//! keeping their vertex order, UV layout and winding: seeded jitter and planar UV code
//! index vertices directly, and hull measurements must match the previous models.

mod mesh;

pub use mesh::{Attribute, Mesh};
