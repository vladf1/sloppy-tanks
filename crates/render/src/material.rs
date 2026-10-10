//! Material classification: the painted variants that let differently colored parts
//! share one batch (the `vertexMaterial` rule from `batching.ts`), interning by value,
//! and the faces a material casts its shadow from.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub use sloppy_core::models::{is_paintable, painted};
use sloppy_core::scene::{Effect, Material, Side, TextureRef};

fn hash_texture(texture: Option<&TextureRef>, state: &mut impl Hasher) {
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
            texture.flip_y.hash(state);
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
    hash_texture(m.map.as_ref(), &mut state);
    hash_texture(m.emissive_map.as_ref(), &mut state);
    hash_texture(m.bump_map.as_ref(), &mut state);
    for (name, texture) in &m.extra_textures {
        name.hash(&mut state);
        hash_texture(Some(texture), &mut state);
    }
    m.shadow_side.hash(&mut state);
    m.polygon_offset
        .map(|(factor, units)| (factor.to_bits(), units.to_bits()))
        .hash(&mut state);
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

    /// Forget materials nobody else holds any more (after a round reset).
    pub fn retain_used(&mut self) {
        self.by_hash.retain(|_, bucket| {
            bucket.retain(|material| Arc::strong_count(material) > 1);
            !bucket.is_empty()
        });
    }
}

/// Culling and depth-only faces per Three: an explicit `shadow_side` wins, otherwise
/// a front-side material casts from its back faces (`_shadowSide`), a back-side one
/// from its front faces.
pub fn shadow_side(material: &Material) -> Side {
    material.shadow_side.unwrap_or(match material.side {
        Side::Front => Side::Back,
        Side::Back => Side::Front,
        Side::Double => Side::Double,
    })
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
        let a = interner.intern(&Arc::new(painted(&red)));
        let b = interner.intern(&Arc::new(painted(&blue)));
        assert!(Arc::ptr_eq(&a, &b));
        let rough = interner.intern(&Arc::new(painted(&Material::standard(0xff0000, 0.05, 0.9))));
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
        assert!(!is_paintable(&Material::basic(0xffffff)));
        let wavy = Material {
            effect: Effect::Custom {
                name: "wave",
                params: vec![1.0],
            },
            ..Material::default()
        };
        assert!(!is_paintable(&wavy));
        assert_eq!(shadow_side(&Material::default()), Side::Back);
    }
}
