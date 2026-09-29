//! Model trees and material descriptions: the contract between model builders and the
//! renderer. It mirrors the subset of Three.js Object3D/Mesh/InstancedMesh and
//! MeshStandardMaterial/MeshBasicMaterial that the game used, without GPU resources.
//!
//! Transforms stay in f64 like Three.js math, so bounds measured from a model (tank
//! hulls, muzzles) match the previous implementation; the renderer narrows to f32.
//! Shared geometry and materials are `Arc`s: model caches outlive round resets, and
//! the renderer batches parts that share a mesh and material.

use std::sync::Arc;

use glam::{DMat4, DQuat, DVec3};

use crate::geometry::Mesh;

/// A color as authored, in sRGB hex like `0x3d6b8f`. The renderer converts to linear.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Color(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Shading {
    /// Metallic-roughness PBR lit by the sun, sky and fog (MeshStandardMaterial).
    #[default]
    Standard,
    /// Unlit color (MeshBasicMaterial).
    Basic,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Side {
    #[default]
    Front,
    Back,
    Double,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Blending {
    #[default]
    Normal,
    Additive,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Wrap {
    #[default]
    Clamp,
    Repeat,
    Mirror,
}

/// Where a texture's pixels come from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TextureSource {
    /// A runtime asset under `public/`, for example `textures/trees/bark.webp`.
    File(&'static str),
    /// Pixels generated at runtime (the former CanvasTexture drawings), by stable key.
    Generated(&'static str),
}

/// A sampled texture and its sampler/transform settings (Three.js Texture fields).
#[derive(Clone, Debug, PartialEq)]
pub struct TextureRef {
    pub source: TextureSource,
    pub wrap: Wrap,
    pub repeat: [f32; 2],
    pub offset: [f32; 2],
    /// Color data (albedo) is sRGB; data textures such as normals are linear.
    pub srgb: bool,
    pub anisotropy: u8,
    pub mipmaps: bool,
}

impl TextureRef {
    pub fn file(path: &'static str) -> Self {
        Self {
            source: TextureSource::File(path),
            wrap: Wrap::Repeat,
            repeat: [1.0, 1.0],
            offset: [0.0, 0.0],
            srgb: true,
            anisotropy: 1,
            mipmaps: true,
        }
    }
}

/// Custom shading that Three.js expressed with TSL nodes. Model builders choose a
/// variant and its parameters; the renderer owns the matching WGSL. Variants are
/// added as those materials are ported; `None` is plain standard/basic shading.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Effect {
    #[default]
    None,
    /// Named effect with numeric parameters, for materials whose parameters are still
    /// settling during the port. Prefer a dedicated variant once it is stable.
    Custom {
        name: &'static str,
        params: Vec<f32>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub shading: Shading,
    pub color: Color,
    pub roughness: f32,
    pub metalness: f32,
    pub emissive: Color,
    pub emissive_intensity: f32,
    pub map: Option<TextureRef>,
    pub bump_map: Option<TextureRef>,
    pub bump_scale: f32,
    pub vertex_colors: bool,
    pub flat_shading: bool,
    pub transparent: bool,
    pub opacity: f32,
    pub alpha_test: f32,
    pub alpha_to_coverage: bool,
    pub side: Side,
    pub blending: Blending,
    pub depth_test: bool,
    pub depth_write: bool,
    pub tone_mapped: bool,
    pub fog: bool,
    pub effect: Effect,
}

impl Default for Material {
    /// Three.js MeshStandardMaterial defaults.
    fn default() -> Self {
        Self {
            shading: Shading::Standard,
            color: Color(0xffffff),
            roughness: 1.0,
            metalness: 0.0,
            emissive: Color(0x000000),
            emissive_intensity: 1.0,
            map: None,
            bump_map: None,
            bump_scale: 1.0,
            vertex_colors: false,
            flat_shading: false,
            transparent: false,
            opacity: 1.0,
            alpha_test: 0.0,
            alpha_to_coverage: false,
            side: Side::Front,
            blending: Blending::Normal,
            depth_test: true,
            depth_write: true,
            tone_mapped: true,
            fog: true,
            effect: Effect::None,
        }
    }
}

impl Material {
    /// Three.js MeshBasicMaterial defaults with a color.
    pub fn basic(color: u32) -> Self {
        Self {
            shading: Shading::Basic,
            color: Color(color),
            ..Self::default()
        }
    }

    /// Three.js MeshStandardMaterial with a color, metalness and roughness.
    pub fn standard(color: u32, metalness: f32, roughness: f32) -> Self {
        Self {
            color: Color(color),
            metalness,
            roughness,
            ..Self::default()
        }
    }
}

/// Per-instance transform and optional color of an InstancedMesh.
#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    pub matrix: DMat4,
    /// Linear RGB multiplier (Three.js instanceColor), when the mesh uses instance colors.
    pub color: Option<[f32; 3]>,
}

/// A drawable attached to a node: a Mesh, or an InstancedMesh when `instances` is set.
#[derive(Clone, Debug)]
pub struct Drawable {
    pub mesh: Arc<Mesh>,
    pub material: Arc<Material>,
    pub cast_shadow: bool,
    pub receive_shadow: bool,
    pub render_order: i32,
    pub frustum_culled: bool,
    pub instances: Option<Vec<Instance>>,
}

impl Drawable {
    pub fn new(mesh: Arc<Mesh>, material: Arc<Material>) -> Self {
        Self {
            mesh,
            material,
            cast_shadow: false,
            receive_shadow: false,
            render_order: 0,
            frustum_culled: true,
            instances: None,
        }
    }
}

/// An Object3D/Group/Mesh. Names mark the parts that simulation or animation looks up
/// (for example `hull`, `turret`, `barrel`, `muzzle`); most parts are unnamed.
#[derive(Clone, Debug)]
pub struct Node {
    pub name: String,
    pub position: DVec3,
    pub rotation: DQuat,
    pub scale: DVec3,
    pub visible: bool,
    pub drawable: Option<Drawable>,
    pub children: Vec<Node>,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            name: String::new(),
            position: DVec3::ZERO,
            rotation: DQuat::IDENTITY,
            scale: DVec3::ONE,
            visible: true,
            drawable: None,
            children: Vec::new(),
        }
    }
}

impl Node {
    pub fn group(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    pub fn mesh(mesh: Arc<Mesh>, material: Arc<Material>) -> Self {
        Self {
            drawable: Some(Drawable::new(mesh, material)),
            ..Self::default()
        }
    }

    /// The transform from this node's space to its parent's (Three.js `matrix`).
    pub fn local_matrix(&self) -> DMat4 {
        DMat4::from_scale_rotation_translation(self.scale, self.rotation, self.position)
    }

    /// Depth-first search by name.
    pub fn find(&self, name: &str) -> Option<&Node> {
        if self.name == name {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find(name))
    }

    pub fn find_mut(&mut self, name: &str) -> Option<&mut Node> {
        if self.name == name {
            return Some(self);
        }
        self.children
            .iter_mut()
            .find_map(|child| child.find_mut(name))
    }

    /// Visit every node with its world matrix relative to `parent`, parents first.
    pub fn traverse(&self, parent: DMat4, visit: &mut impl FnMut(&Node, DMat4)) {
        let world = parent * self.local_matrix();
        visit(self, world);
        for child in &self.children {
            child.traverse(world, visit);
        }
    }
}
