//! Material classification: which list a material draws in, which shader variant
//! it needs, and the painted variants that let differently colored parts share one
//! batch (the `vertexMaterial` rule from `batching.ts`).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use sloppy_core::scene::{Blending, Effect, Material, Shading, Side, TextureRef};

use crate::color::hex_to_linear;

/// Whether a material draws in Three's transparent list (sorted back to front).
pub fn is_transparent(material: &Material) -> bool {
    material.transparent
}

/// Three's `NodeBuilder.isOpaque()`: such materials write alpha 1.
pub fn forces_opaque_alpha(material: &Material) -> bool {
    !material.transparent && material.blending == Blending::Normal && !material.alpha_to_coverage
}

/// Materials whose paint can be baked into vertex colors: opaque standard surfaces
/// without per-pixel alpha, vertex colors or custom shading.
pub fn is_paintable(material: &Material) -> bool {
    material.shading == Shading::Standard
        && !material.transparent
        && material.opacity == 1.0
        && material.alpha_test == 0.0
        && !material.vertex_colors
        && material.effect == Effect::None
}

/// The shared white, vertex-colored stand-in for a paintable material.
pub fn painted(material: &Material) -> Material {
    Material {
        color: sloppy_core::scene::Color(0xffffff),
        vertex_colors: true,
        ..material.clone()
    }
}

/// The linear paint a part bakes into its vertices when its material is painted.
pub fn paint_color(material: &Material) -> [f32; 3] {
    hex_to_linear(material.color.0)
}

fn hash_texture(texture: &Option<TextureRef>, state: &mut impl Hasher) {
    match texture {
        None => 0u8.hash(state),
        Some(texture) => {
            1u8.hash(state);
            texture.source.hash(state);
            texture.wrap.hash(state);
            for value in texture.repeat.iter().chain(&texture.offset) {
                value.to_bits().hash(state);
            }
            texture.srgb.hash(state);
            texture.anisotropy.hash(state);
            texture.mipmaps.hash(state);
        }
    }
}

/// A hash of every field; equal materials hash equally.
pub fn material_hash(material: &Material) -> u64 {
    let mut state = std::collections::hash_map::DefaultHasher::new();
    let m = material;
    m.shading.hash(&mut state);
    m.color.hash(&mut state);
    for value in [
        m.roughness,
        m.metalness,
        m.emissive_intensity,
        m.bump_scale,
        m.opacity,
        m.alpha_test,
    ] {
        value.to_bits().hash(&mut state);
    }
    m.emissive.hash(&mut state);
    hash_texture(&m.map, &mut state);
    hash_texture(&m.bump_map, &mut state);
    (
        m.vertex_colors,
        m.flat_shading,
        m.transparent,
        m.alpha_to_coverage,
        m.side,
        m.blending,
        m.depth_test,
        m.depth_write,
        m.tone_mapped,
        m.fog,
    )
        .hash(&mut state);
    match &m.effect {
        Effect::None => 0u8.hash(&mut state),
        Effect::Custom { name, params } => {
            1u8.hash(&mut state);
            name.hash(&mut state);
            for value in params {
                value.to_bits().hash(&mut state);
            }
        }
    }
    state.finish()
}

/// Deduplicates materials by value so equal materials built separately (or the
/// painted stand-ins) share one GPU material and can batch together.
#[derive(Default)]
pub struct MaterialInterner {
    by_hash: HashMap<u64, Vec<Arc<Material>>>,
}

impl MaterialInterner {
    pub fn intern(&mut self, material: &Arc<Material>) -> Arc<Material> {
        let bucket = self.by_hash.entry(material_hash(material)).or_default();
        if let Some(existing) = bucket.iter().find(|m| m.as_ref() == material.as_ref()) {
            return existing.clone();
        }
        bucket.push(material.clone());
        material.clone()
    }

    pub fn intern_value(&mut self, material: Material) -> Arc<Material> {
        self.intern(&Arc::new(material))
    }

    pub fn len(&self) -> usize {
        self.by_hash.values().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Culling and depth-only faces per Three: a front-side material casts from its
/// back faces (`_shadowSide`), a back-side one from its front faces.
pub fn shadow_side(side: Side) -> Side {
    match side {
        Side::Front => Side::Back,
        Side::Back => Side::Front,
        Side::Double => Side::Double,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paintable_materials_share_a_painted_variant() {
        let red = Material::standard(0xff0000, 0.05, 0.65);
        let blue = Material::standard(0x0000ff, 0.05, 0.65);
        assert!(is_paintable(&red));
        let mut interner = MaterialInterner::default();
        let a = interner.intern_value(painted(&red));
        let b = interner.intern_value(painted(&blue));
        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!(paint_color(&red), [1.0, 0.0, 0.0]);
        let rough = interner.intern_value(painted(&Material::standard(0xff0000, 0.05, 0.9)));
        assert!(!Arc::ptr_eq(&a, &rough));
    }

    #[test]
    fn transparency_and_effects_are_not_painted() {
        let glass = Material {
            transparent: true,
            opacity: 0.5,
            ..Material::default()
        };
        assert!(!is_paintable(&glass));
        assert!(!forces_opaque_alpha(&glass));
        assert!(!is_paintable(&Material::basic(0xffffff)));
        let wavy = Material {
            effect: Effect::Custom {
                name: "wave",
                params: vec![1.0],
            },
            ..Material::default()
        };
        assert!(!is_paintable(&wavy));
        assert_eq!(shadow_side(Side::Front), Side::Back);
    }
}
