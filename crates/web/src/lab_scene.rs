//! The render lab's scene description, shared with `tools/render-lab.ts`: the page
//! builds the Three.js reference scene and this Rust scene from the same JSON, so
//! calibration compares renderers rather than two hand-copied scenes.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::{DQuat, DVec3, EulerRot};
use serde::Deserialize;
use sloppy_core::geometry::Mesh;
use sloppy_core::scene::{
    Blending, Color, Effect, Instance, Material, Node, Shading, Side, TextureRef, TextureSource,
    Wrap,
};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureSpec {
    pub path: Option<String>,
    pub generated: Option<String>,
    #[serde(default = "default_wrap")]
    pub wrap: String,
    #[serde(default = "one2")]
    pub repeat: [f32; 2],
    #[serde(default)]
    pub offset: [f32; 2],
    #[serde(default = "yes")]
    pub srgb: bool,
    #[serde(default = "one_u8")]
    pub anisotropy: u8,
    #[serde(default = "yes")]
    pub mipmaps: bool,
}

fn default_wrap() -> String {
    "repeat".into()
}
fn one2() -> [f32; 2] {
    [1.0, 1.0]
}
fn yes() -> bool {
    true
}
fn one() -> f32 {
    1.0
}
fn one_u8() -> u8 {
    1
}

#[derive(Clone, Debug, Deserialize)]
pub struct EffectSpec {
    pub name: String,
    pub params: Vec<f32>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialSpec {
    #[serde(default)]
    pub shading: Option<String>,
    pub color: u32,
    #[serde(default = "one")]
    pub roughness: f32,
    #[serde(default)]
    pub metalness: f32,
    #[serde(default)]
    pub emissive: u32,
    #[serde(default = "one")]
    pub emissive_intensity: f32,
    pub map: Option<TextureSpec>,
    pub bump_map: Option<TextureSpec>,
    #[serde(default = "one")]
    pub bump_scale: f32,
    #[serde(default)]
    pub vertex_colors: bool,
    #[serde(default)]
    pub flat_shading: bool,
    #[serde(default)]
    pub transparent: bool,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default)]
    pub alpha_test: f32,
    #[serde(default)]
    pub alpha_to_coverage: bool,
    #[serde(default)]
    pub side: Option<String>,
    #[serde(default)]
    pub blending: Option<String>,
    #[serde(default = "yes")]
    pub depth_write: bool,
    #[serde(default = "yes")]
    pub depth_test: bool,
    #[serde(default = "yes")]
    pub fog: bool,
    pub effect: Option<EffectSpec>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GeometrySpec {
    #[serde(rename_all = "camelCase")]
    Box { width: f32, height: f32, depth: f32 },
    #[serde(rename_all = "camelCase")]
    Sphere {
        radius: f32,
        width_segments: u32,
        height_segments: u32,
    },
    #[serde(rename_all = "camelCase")]
    Plane {
        width: f32,
        height: f32,
        #[serde(default = "one_u32")]
        width_segments: u32,
        #[serde(default = "one_u32")]
        height_segments: u32,
    },
    /// `createArenaFloor("dry-grass")`: a flat XZ plane with tint patches and UVs
    /// every eight metres.
    Ground { extent: f32 },
}

fn one_u32() -> u32 {
    1
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectSpec {
    pub name: String,
    pub geometry: GeometrySpec,
    pub material: MaterialSpec,
    pub position: [f64; 3],
    #[serde(default)]
    pub rotation: [f64; 3],
    #[serde(default = "one3")]
    pub scale: [f64; 3],
    #[serde(default)]
    pub cast_shadow: bool,
    #[serde(default)]
    pub receive_shadow: bool,
    #[serde(default)]
    pub render_order: i32,
    /// Bake into static scenery instead of a movable instance.
    #[serde(rename = "static", default)]
    pub is_static: bool,
    /// InstancedMesh translations relative to the object.
    pub instances: Option<Vec<[f64; 3]>>,
    /// Child parts; a named child is a movable joint, an unnamed one is rigid.
    #[serde(default)]
    pub children: Vec<ObjectSpec>,
    /// More instances of the same model, as translations of `position`.
    #[serde(default)]
    pub copies: Vec<[f64; 3]>,
}

fn one3() -> [f64; 3] {
    [1.0; 3]
}

#[derive(Clone, Debug, Deserialize)]
pub struct FogSpec {
    pub color: u32,
    pub near: f32,
    pub far: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct HemisphereSpec {
    pub sky: u32,
    pub ground: u32,
    pub intensity: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SunSpec {
    pub color: u32,
    pub intensity: f32,
    pub position: [f32; 3],
    pub target: [f32; 3],
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShadowSpec {
    pub map_size: u32,
    pub half: f32,
    pub near: f32,
    pub depth: f32,
    pub bias: f32,
    pub normal_bias: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PointLightSpec {
    pub color: u32,
    pub intensity: f32,
    pub distance: f32,
    pub decay: f32,
    pub position: [f32; 3],
}

#[derive(Clone, Debug, Deserialize)]
pub struct CameraSpec {
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    pub position: [f32; 3],
    pub target: [f32; 3],
}

#[derive(Clone, Debug, Deserialize)]
pub struct WaterSpec {
    pub width: f32,
    pub depth: f32,
    pub height: f32,
    pub center: [f32; 2],
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneSpec {
    pub background: u32,
    pub fog: Option<FogSpec>,
    pub hemisphere: HemisphereSpec,
    pub sun: SunSpec,
    pub shadow: ShadowSpec,
    pub point_light: Option<PointLightSpec>,
    pub exposure: f32,
    pub camera: CameraSpec,
    pub water: Option<WaterSpec>,
    pub objects: Vec<ObjectSpec>,
}

fn leak(text: &str) -> &'static str {
    // Lab scenes are loaded a bounded number of times; the core contract uses
    // static texture keys and effect names.
    Box::leak(text.to_owned().into_boxed_str())
}

pub fn texture(spec: &TextureSpec) -> TextureRef {
    let source = match (&spec.path, &spec.generated) {
        (Some(path), _) => TextureSource::File(leak(path)),
        (None, Some(name)) => TextureSource::Generated(leak(name)),
        (None, None) => TextureSource::Generated("missing"),
    };
    TextureRef {
        source,
        wrap: match spec.wrap.as_str() {
            "clamp" => Wrap::Clamp,
            "mirror" => Wrap::Mirror,
            _ => Wrap::Repeat,
        },
        repeat: spec.repeat,
        offset: spec.offset,
        srgb: spec.srgb,
        anisotropy: spec.anisotropy,
        mipmaps: spec.mipmaps,
    }
}

pub fn material(spec: &MaterialSpec) -> Material {
    Material {
        shading: if spec.shading.as_deref() == Some("basic") {
            Shading::Basic
        } else {
            Shading::Standard
        },
        color: Color(spec.color),
        roughness: spec.roughness,
        metalness: spec.metalness,
        emissive: Color(spec.emissive),
        emissive_intensity: spec.emissive_intensity,
        map: spec.map.as_ref().map(texture),
        bump_map: spec.bump_map.as_ref().map(texture),
        bump_scale: spec.bump_scale,
        vertex_colors: spec.vertex_colors,
        flat_shading: spec.flat_shading,
        transparent: spec.transparent,
        opacity: spec.opacity,
        alpha_test: spec.alpha_test,
        alpha_to_coverage: spec.alpha_to_coverage,
        side: match spec.side.as_deref() {
            Some("back") => Side::Back,
            Some("double") => Side::Double,
            _ => Side::Front,
        },
        shadow_side: None,
        blending: if spec.blending.as_deref() == Some("additive") {
            Blending::Additive
        } else {
            Blending::Normal
        },
        depth_test: spec.depth_test,
        depth_write: spec.depth_write,
        tone_mapped: true,
        fog: spec.fog,
        effect: spec
            .effect
            .as_ref()
            .map_or(Effect::None, |effect| Effect::Custom {
                name: leak(&effect.name),
                params: effect.params.clone(),
            }),
    }
}

/// One BoxGeometry face: (u, v, w axes, u and v directions, width, height, depth).
type BoxPlane = (usize, usize, usize, f32, f32, f32, f32, f32);

/// Three.js `BoxGeometry(width, height, depth)` with one segment per side: same
/// vertex order, normals and UVs.
pub fn box_mesh(width: f32, height: f32, depth: f32) -> Mesh {
    let mut mesh = Mesh::default();
    let mut indices = Vec::new();
    let planes: [BoxPlane; 6] = [
        (2, 1, 0, -1.0, -1.0, depth, height, width),
        (2, 1, 0, 1.0, -1.0, depth, height, -width),
        (0, 2, 1, 1.0, 1.0, width, depth, height),
        (0, 2, 1, 1.0, -1.0, width, depth, -height),
        (0, 1, 2, 1.0, -1.0, width, height, depth),
        (0, 1, 2, -1.0, -1.0, width, height, -depth),
    ];
    for (u, v, w, udir, vdir, plane_width, plane_height, plane_depth) in planes {
        let base = mesh.positions.len() as u32;
        for iy in 0..2 {
            let y = iy as f32 * plane_height - plane_height / 2.0;
            for ix in 0..2 {
                let x = ix as f32 * plane_width - plane_width / 2.0;
                let mut position = [0.0; 3];
                position[u] = x * udir;
                position[v] = y * vdir;
                position[w] = plane_depth / 2.0;
                let mut normal = [0.0; 3];
                normal[w] = if plane_depth > 0.0 { 1.0 } else { -1.0 };
                mesh.positions.push(position);
                mesh.normals.push(normal);
                mesh.uvs.push([ix as f32, 1.0 - iy as f32]);
            }
        }
        let (a, b, c, d) = (base, base + 2, base + 3, base + 1);
        indices.extend([a, b, d, b, c, d]);
    }
    mesh.indices = Some(indices);
    mesh
}

/// Three.js `PlaneGeometry` in the XY plane facing +Z.
pub fn plane_mesh(width: f32, height: f32, width_segments: u32, height_segments: u32) -> Mesh {
    let mut mesh = Mesh::default();
    let columns = width_segments + 1;
    for iy in 0..=height_segments {
        let y = iy as f32 * height / height_segments as f32 - height / 2.0;
        for ix in 0..=width_segments {
            let x = ix as f32 * width / width_segments as f32 - width / 2.0;
            mesh.positions.push([x, -y, 0.0]);
            mesh.normals.push([0.0, 0.0, 1.0]);
            mesh.uvs.push([
                ix as f32 / width_segments as f32,
                1.0 - iy as f32 / height_segments as f32,
            ]);
        }
    }
    let mut indices = Vec::new();
    for iy in 0..height_segments {
        for ix in 0..width_segments {
            let a = ix + columns * iy;
            let b = ix + columns * (iy + 1);
            let c = ix + 1 + columns * (iy + 1);
            let d = ix + 1 + columns * iy;
            indices.extend([a, b, d, b, c, d]);
        }
    }
    mesh.indices = Some(indices);
    mesh
}

/// Three.js `SphereGeometry(radius, widthSegments, heightSegments)`.
pub fn sphere_mesh(radius: f32, width_segments: u32, height_segments: u32) -> Mesh {
    let mut mesh = Mesh::default();
    let mut grid = Vec::new();
    for iy in 0..=height_segments {
        let v = iy as f64 / height_segments as f64;
        let u_offset = if iy == 0 {
            0.5 / width_segments as f64
        } else if iy == height_segments {
            -0.5 / width_segments as f64
        } else {
            0.0
        };
        let mut row = Vec::new();
        for ix in 0..=width_segments {
            let u = ix as f64 / width_segments as f64;
            let r = radius as f64;
            let x = -r * (u * 2.0 * PI).cos() * (v * PI).sin();
            let y = r * (v * PI).cos();
            let z = r * (u * 2.0 * PI).sin() * (v * PI).sin();
            let n = DVec3::new(x, y, z).normalize_or_zero();
            row.push(mesh.positions.len() as u32);
            mesh.positions.push([x as f32, y as f32, z as f32]);
            mesh.normals.push([n.x as f32, n.y as f32, n.z as f32]);
            mesh.uvs.push([(u + u_offset) as f32, (1.0 - v) as f32]);
        }
        grid.push(row);
    }
    let mut indices = Vec::new();
    for iy in 0..height_segments as usize {
        for ix in 0..width_segments as usize {
            let a = grid[iy][ix + 1];
            let b = grid[iy][ix];
            let c = grid[iy + 1][ix];
            let d = grid[iy + 1][ix + 1];
            if iy != 0 {
                indices.extend([a, b, d]);
            }
            if iy != height_segments as usize - 1 {
                indices.extend([b, c, d]);
            }
        }
    }
    mesh.indices = Some(indices);
    mesh
}

/// `createArenaFloor("dry-grass")` geometry: the plane rotated flat, UVs every
/// eight metres and the patchy vertex tint.
pub fn ground_mesh(extent: f32) -> Mesh {
    let segments = ((extent / 2.5).round() as u32).max(1);
    let mut mesh = plane_mesh(extent, extent, segments, segments);
    for ((position, normal), uv) in mesh
        .positions
        .iter_mut()
        .zip(&mut mesh.normals)
        .zip(&mut mesh.uvs)
    {
        // rotateX(-π/2): (x, y, z) → (x, z, -y)
        *position = [position[0], position[2], -position[1]];
        *normal = [0.0, 1.0, 0.0];
        *uv = [position[0] / 8.0, position[2] / 8.0];
    }
    mesh.colors = mesh
        .positions
        .iter()
        .map(|p| {
            let (x, z) = (p[0] as f64, p[2] as f64);
            let patch =
                0.5 + 0.25 * (x * 0.18 + z * 0.09).sin() + 0.25 * (z * 0.22 - x * 0.1).sin();
            [
                (0.68 + patch * 0.28) as f32,
                (0.83 + patch * 0.14) as f32,
                (0.42 + patch * 0.36) as f32,
            ]
        })
        .collect();
    mesh
}

/// The water surface in world XZ at y = 0 (a rotated PlaneGeometry).
pub fn water_mesh(spec: &WaterSpec) -> Mesh {
    let mut mesh = plane_mesh(spec.width, spec.depth, 1, 1);
    for (position, normal) in mesh.positions.iter_mut().zip(&mut mesh.normals) {
        *position = [
            position[0] + spec.center[0],
            0.0,
            -position[1] + spec.center[1],
        ];
        *normal = [0.0, 1.0, 0.0];
    }
    mesh
}

pub fn geometry(spec: &GeometrySpec) -> Mesh {
    match *spec {
        GeometrySpec::Box {
            width,
            height,
            depth,
        } => box_mesh(width, height, depth),
        GeometrySpec::Sphere {
            radius,
            width_segments,
            height_segments,
        } => sphere_mesh(radius, width_segments, height_segments),
        GeometrySpec::Plane {
            width,
            height,
            width_segments,
            height_segments,
        } => plane_mesh(width, height, width_segments, height_segments),
        GeometrySpec::Ground { extent } => ground_mesh(extent),
    }
}

/// The object as a model tree: a named root holding the drawable node.
pub fn object_node(spec: &ObjectSpec) -> Node {
    let mesh = Arc::new(geometry(&spec.geometry));
    let material = Arc::new(material(&spec.material));
    let mut node = Node::mesh(mesh, material);
    node.name = spec.name.clone();
    node.position = DVec3::from(spec.position);
    node.rotation = DQuat::from_euler(
        EulerRot::XYZ,
        spec.rotation[0],
        spec.rotation[1],
        spec.rotation[2],
    );
    node.scale = DVec3::from(spec.scale);
    let drawable = node.drawable.as_mut().expect("mesh node");
    drawable.cast_shadow = spec.cast_shadow;
    drawable.receive_shadow = spec.receive_shadow;
    drawable.render_order = spec.render_order;
    drawable.instances = spec.instances.as_ref().map(|offsets| {
        offsets
            .iter()
            .map(|offset| Instance {
                matrix: glam::DMat4::from_translation(DVec3::from(*offset)),
                color: None,
            })
            .collect()
    });
    node.children = spec.children.iter().map(object_node).collect();
    node
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_match_three_counts() {
        let cube = box_mesh(2.0, 2.0, 2.0);
        assert_eq!(cube.positions.len(), 24);
        assert_eq!(cube.triangle_count(), 12);
        // BoxGeometry's first vertex: +X face, top-left corner.
        assert_eq!(cube.positions[0], [1.0, 1.0, 1.0]);
        assert_eq!(cube.uvs[0], [0.0, 1.0]);
        let sphere = sphere_mesh(1.0, 32, 16);
        assert_eq!(sphere.positions.len(), 33 * 17);
        assert_eq!(sphere.triangle_count(), 32 * 16 * 2 - 64);
        let ground = ground_mesh(40.0);
        assert_eq!(ground.positions.len(), 17 * 17);
        assert_eq!(ground.positions[0], [-20.0, 0.0, -20.0]);
        assert_eq!(ground.uvs[0], [-2.5, -2.5]);
    }

    #[test]
    fn scene_spec_parses() {
        let json = r#"{
            "background": 11193282, "fog": {"color": 11193282, "near": 20, "far": 80},
            "hemisphere": {"sky": 12440053, "ground": 7702939, "intensity": 1.65},
            "sun": {"color": 16766363, "intensity": 2.8, "position": [-45, 68, 25], "target": [0, 0, 0]},
            "shadow": {"mapSize": 2048, "half": 70, "near": 0.5, "depth": 219.5, "bias": -0.0002, "normalBias": 0.05},
            "exposure": 1,
            "camera": {"fov": 43, "near": 0.1, "far": 320, "position": [0, 9, 30], "target": [0, 2, 0]},
            "objects": [{"name": "box", "geometry": {"type": "box", "width": 1, "height": 1, "depth": 1},
                "material": {"color": 255, "roughness": 0.5, "effect": {"name": "pulse", "params": [1, 2]}},
                "position": [0, 0.5, 0], "castShadow": true, "static": true}]
        }"#;
        let scene: SceneSpec = serde_json::from_str(json).unwrap();
        assert!(scene.objects[0].is_static);
        let node = object_node(&scene.objects[0]);
        assert!(matches!(
            node.drawable.unwrap().material.effect,
            Effect::Custom { name: "pulse", .. }
        ));
    }
}
