//! Port of `tank-surfaces.ts`: the worn armor texture on painted vehicle parts.
//!
//! The TypeScript cloned each painted material once and gave it the wear image as
//! both color map and bump map, only after the image loaded. Here the material
//! always names the texture; the renderer loads it and may fall back to plain
//! paint when the image is unavailable, as the TypeScript did.

use std::sync::{Arc, Mutex};

use crate::scene::{Material, Node, Shading, TextureRef};

/// The shared wear image under `public/`.
pub const ARMOR_WEAR_TEXTURE: &str = "textures/tanks/armor-wear.webp";
const ARMOR_WEAR_ANISOTROPY: u8 = 4;
/// Height of the wear bump relief.
const ARMOR_WEAR_BUMP_SCALE: f32 = 0.025;

/// The wear texture as the TypeScript configured it: sRGB, repeating, 4x anisotropy.
pub fn armor_wear_texture() -> TextureRef {
    TextureRef {
        anisotropy: ARMOR_WEAR_ANISOTROPY,
        ..TextureRef::file(ARMOR_WEAR_TEXTURE)
    }
}

/// Painted clones by source material. Sources come from the shared material cache
/// and live for the whole process, so their addresses identify them.
static PAINTED: Mutex<Vec<(Arc<Material>, Arc<Material>)>> = Mutex::new(Vec::new());

fn painted(source: &Arc<Material>) -> Arc<Material> {
    let mut painted = PAINTED.lock().expect("painted material cache");
    if let Some((_, clone)) = painted.iter().find(|(s, _)| Arc::ptr_eq(s, source)) {
        return clone.clone();
    }
    let texture = armor_wear_texture();
    let clone = Arc::new(Material {
        map: Some(texture.clone()),
        bump_map: Some(texture),
        bump_scale: ARMOR_WEAR_BUMP_SCALE,
        ..(**source).clone()
    });
    painted.push((source.clone(), clone.clone()));
    clone
}

/// `applyTankSurface(root, colors)`: every standard-shaded part whose paint is one
/// of `colors` switches to the shared worn clone of its material.
pub fn apply_tank_surface(root: &mut Node, colors: &[u32]) {
    if let Some(drawable) = &mut root.drawable
        && drawable.material.shading == Shading::Standard
        && colors.contains(&drawable.material.color.0)
    {
        drawable.material = painted(&drawable.material);
    }
    for child in &mut root.children {
        apply_tank_surface(child, colors);
    }
}
