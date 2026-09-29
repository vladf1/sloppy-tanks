//! CPU triangle meshes. Generators port the Three.js r185 geometries the game used,
//! keeping their vertex order, UV layout and winding: seeded jitter and planar UV code
//! index vertices directly, and hull measurements must match the previous models.
//!
//! Like Three.js, generators compute in f64 (JavaScript numbers) and store f32
//! (Float32Array); mesh operations read the stored f32 values back. `math` holds the
//! Three-exact helpers (normalisation, `Math.sign`, Euler rotation, matrix inverse,
//! colour conversion) that keep results bit-compatible.
//!
//! | Three.js                                | Rust                                         |
//! | --------------------------------------- | -------------------------------------------- |
//! | `BoxGeometry`                           | [`box_geometry`], [`BoxGeometry`]            |
//! | `RoundedBoxGeometry` (addon)            | [`rounded_box_geometry`]                     |
//! | `PlaneGeometry`                         | [`plane_geometry`], [`plane_geometry_segments`] |
//! | `CircleGeometry`                        | [`circle_geometry`], [`circle_geometry_arc`] |
//! | `RingGeometry`                          | [`ring_geometry`], [`RingGeometry`]          |
//! | `CylinderGeometry` / `ConeGeometry`     | [`cylinder_geometry`], [`CylinderGeometry`], [`cone_geometry`] |
//! | `SphereGeometry`                        | [`sphere_geometry`], [`SphereGeometry`]      |
//! | `TorusGeometry`                         | [`torus_geometry`], [`TorusGeometry`]        |
//! | `Polyhedron`/`Icosahedron`/`Octahedron`/`TetrahedronGeometry` | [`polyhedron_geometry`], [`icosahedron_geometry`], [`octahedron_geometry`], [`tetrahedron_geometry`] |
//! | `LatheGeometry`                         | [`lathe_geometry`]                           |
//! | `Shape`, `Path`, 2D curves              | [`Shape`], [`Path`], [`Curve2`]              |
//! | `ShapeGeometry`                         | [`shape_geometry`]                           |
//! | `ExtrudeGeometry`                       | [`extrude_geometry`], [`ExtrudeOptions`]     |
//! | `ShapeUtils.triangulateShape` / earcut  | [`triangulate_shape`], [`earcut`]            |
//! | `CatmullRomCurve3`                      | [`CatmullRomCurve3`]                         |
//! | `BufferGeometry` transforms, `toNonIndexed`, `computeVertexNormals`, `center`, `computeBoundingBox` | [`Mesh`] methods |
//! | `BufferGeometryUtils.mergeGeometries` / `toCreasedNormals` | [`merge_geometries`], [`Mesh::to_creased_normals`] |
//! | `Box3.setFromObject(object)`            | [`node_bounds`], [`Aabb`]                    |

mod bounds;
mod curve3;
mod extrude;
mod lathe;
pub mod math;
mod mesh;
mod polyhedron;
mod primitives;
mod shape;
mod triangulate;

pub use bounds::{Aabb, node_bounds};
pub use curve3::{CatmullRomCurve3, CatmullRomKind};
pub use extrude::{ExtrudeOptions, extrude_geometry, shape_geometry};
pub use lathe::lathe_geometry;
pub use mesh::{Attribute, Mesh, merge_geometries, narrow, widen};
pub use polyhedron::{
    icosahedron_geometry, octahedron_geometry, polyhedron_geometry, tetrahedron_geometry,
};
pub use primitives::{
    BoxGeometry, CylinderGeometry, RingGeometry, SphereGeometry, TorusGeometry, box_geometry,
    circle_geometry, circle_geometry_arc, cone_geometry, cylinder_geometry, plane_geometry,
    plane_geometry_segments, ring_geometry, rounded_box_geometry, sphere_geometry, torus_geometry,
};
pub use shape::{Curve2, Path, Shape};
pub use triangulate::{earcut, is_clockwise, shape_area, triangulate_shape};

#[cfg(test)]
mod reference_tests;
