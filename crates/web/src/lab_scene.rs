//! The render lab's scene description, the JSON `tools/render-lab.ts` defines and loads
//! (`RenderLab::load_scene`). The lab compares the drawn scene with reference frames
//! captured from the same scene in the former Three.js renderer.

use std::f64::consts::FRAC_PI_2;
use std::sync::Arc;

use glam::{DQuat, DVec3, EulerRot};
use serde::Deserialize;
use sloppy_core::geometry::{
    Mesh, box_geometry, plane_geometry, plane_geometry_segments, sphere_geometry,
};
use sloppy_core::models::create_arena_floor;
use sloppy_core::scene::{
    Blending, Color, Effect, Instance, Material, Node, Shading, Side, TextureRef, TextureSource,
    Wrap,
};
use sloppy_core::sim::maps::GroundKind;

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
    #[serde(default = "yes")]
    pub flip_y: bool,
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
    pub emissive_map: Option<TextureSpec>,
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
    pub side: Option<String>,
    pub blending: Option<String>,
    #[serde(default = "yes")]
    pub depth_write: bool,
    #[serde(default = "yes")]
    pub depth_test: bool,
    #[serde(default = "yes")]
    pub fog: bool,
    /// Three's polygon offset as `[factor, units]`.
    pub polygon_offset: Option<[f32; 2]>,
    pub effect: Option<EffectSpec>,
    /// Secondary textures for the effect, bound in order.
    #[serde(default)]
    pub extra_textures: Vec<NamedTextureSpec>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedTextureSpec {
    pub name: String,
    pub texture: TextureSpec,
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
    /// False draws without the sun's shadow map (the tank previews' lighting).
    #[serde(default = "yes")]
    pub enabled: bool,
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
    /// Sky reflection strength; absent (0) in the Three.js calibration scenes.
    #[serde(default)]
    pub reflections: f32,
    pub camera: CameraSpec,
    pub water: Option<WaterSpec>,
    pub objects: Vec<ObjectSpec>,
}

pub(crate) fn leak(text: &str) -> &'static str {
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
        flip_y: spec.flip_y,
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
        emissive_map: spec.emissive_map.as_ref().map(texture),
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
        polygon_offset: spec.polygon_offset.map(|[factor, units]| (factor, units)),
        tone_mapped: true,
        fog: spec.fog,
        effect: spec
            .effect
            .as_ref()
            .map_or(Effect::None, |effect| Effect::Custom {
                name: leak(&effect.name),
                params: effect.params.clone(),
            }),
        extra_textures: spec
            .extra_textures
            .iter()
            .map(|extra| (leak(&extra.name), texture(&extra.texture)))
            .collect(),
    }
}

/// The water surface in world XZ at y = 0 (a rotated PlaneGeometry).
pub fn water_mesh(spec: &WaterSpec) -> Mesh {
    let mut mesh = plane_geometry(spec.width.into(), spec.depth.into());
    mesh.rotate_x(-FRAC_PI_2)
        .translate(spec.center[0].into(), 0.0, spec.center[1].into());
    mesh
}

/// Three.js `BoxGeometry`, `SphereGeometry` and `PlaneGeometry`, and
/// `createArenaFloor("dry-grass")` for `Ground`.
pub fn geometry(spec: &GeometrySpec) -> Arc<Mesh> {
    match *spec {
        GeometrySpec::Box {
            width,
            height,
            depth,
        } => Arc::new(box_geometry(width.into(), height.into(), depth.into())),
        GeometrySpec::Sphere {
            radius,
            width_segments,
            height_segments,
        } => Arc::new(sphere_geometry(
            radius.into(),
            width_segments,
            height_segments,
        )),
        GeometrySpec::Plane {
            width,
            height,
            width_segments,
            height_segments,
        } => Arc::new(plane_geometry_segments(
            width.into(),
            height.into(),
            width_segments,
            height_segments,
        )),
        GeometrySpec::Ground { extent } => {
            create_arena_floor(GroundKind::DryGrass, extent.into())
                .drawable
                .expect("floor mesh")
                .mesh
        }
    }
}

/// The object as a model tree: a named root holding the drawable node.
pub fn object_node(spec: &ObjectSpec) -> Node {
    let mesh = geometry(&spec.geometry);
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
        let cube = geometry(&GeometrySpec::Box {
            width: 2.0,
            height: 2.0,
            depth: 2.0,
        });
        assert_eq!(cube.positions.len(), 24);
        assert_eq!(cube.triangle_count(), 12);
        // BoxGeometry's first vertex: +X face, top-left corner.
        assert_eq!(cube.positions[0], [1.0, 1.0, 1.0]);
        assert_eq!(cube.uvs[0], [0.0, 1.0]);
        let sphere = geometry(&GeometrySpec::Sphere {
            radius: 1.0,
            width_segments: 32,
            height_segments: 16,
        });
        assert_eq!(sphere.positions.len(), 33 * 17);
        assert_eq!(sphere.triangle_count(), 32 * 16 * 2 - 64);
        let ground = geometry(&GeometrySpec::Ground { extent: 40.0 });
        assert_eq!(ground.positions.len(), 17 * 17);
        // Rotated flat like Three's rotateX(-π/2), which leaves y a rounding error off 0.
        let [x, y, z] = ground.positions[0];
        assert_eq!([x, z], [-20.0, -20.0]);
        assert!(y.abs() < 1e-6);
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
