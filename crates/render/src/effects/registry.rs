//! Custom material effects: named WGSL snippets that replace the TSL node hooks
//! the Three.js materials used (`positionNode`, `vertexNode`, `colorNode`,
//! `opacityNode`, `normalNode`, ...).
//!
//! A snippet may define any of these functions; missing ones become no-ops:
//!
//! ```wgsl
//! // Local space, before the instance transform (positionNode / normalLocal).
//! fn effect_vertex(v: ptr<function, EffectVertex>) { ... }
//! // World space, after it (billboards, world-aligned sway).
//! fn effect_world(w: ptr<function, EffectWorld>, v: EffectVertex) { ... }
//! // Clip space, after the view projection (screen-sized sprites, vertexNode).
//! fn effect_clip(clip: ptr<function, vec4f>, w: EffectWorld, v: EffectVertex) { ... }
//! // Before alpha test, lighting and fog (colorNode, opacityNode, normalNode,
//! // roughness/metalness/emissive nodes). The shadow pass runs it too.
//! fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) { ... }
//! ```
//!
//! Snippets read `material.params` (the `Effect::Custom` params, 16 floats as
//! four vec4s), `frame.camera_position.w` (time in seconds), the frame's camera
//! vectors and per-instance `instance_data`; `extra0`/`extra1` start at zero and
//! carry what a vertex hook writes to the fragment. Shader variants are assembled
//! per material and cached by variant key, so an effect costs one pipeline per
//! distinct material setup.

use sloppy_core::models::{effects_props, effects_scenery};

/// One registered effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectDefinition {
    /// The `Effect::Custom` name that selects it.
    pub name: &'static str,
    /// WGSL defining any of `effect_vertex`, `effect_world`, `effect_surface`.
    pub wgsl: &'static str,
    /// The shadow pass dithers the caster away with the instance opacity, like
    /// the renderer's faded instances (`debris-fade.ts`).
    pub shadow_fade: bool,
    /// The vertex motion is a small cosmetic sway that leaves the cut-out alpha
    /// alone, so the caster may still merge into its model's still shadow.
    pub still_shadow: bool,
}

/// An effect with the default flags.
pub(crate) const fn effect(name: &'static str, wgsl: &'static str) -> EffectDefinition {
    EffectDefinition {
        name,
        wgsl,
        shadow_fade: false,
        still_shadow: false,
    }
}

impl EffectDefinition {
    pub fn has_vertex(&self) -> bool {
        self.wgsl.contains("fn effect_vertex(")
    }
    pub fn has_world(&self) -> bool {
        self.wgsl.contains("fn effect_world(")
    }
    pub fn has_clip(&self) -> bool {
        self.wgsl.contains("fn effect_clip(")
    }
    pub fn has_surface(&self) -> bool {
        self.wgsl.contains("fn effect_surface(")
    }
}

/// Cloth ripple for flags and banners (vertex hook). The mesh lies in its local
/// XY plane, pinned at `uv.x = 0`. params[0] = (amplitude m, wavelength m,
/// speed rad/s, unused).
pub const WAVE: EffectDefinition = effect("wave", include_str!("../shaders/effects/wave.wgsl"));

/// A glowing band that scrolls up the surface and thins its opacity (surface
/// hook). params[0] = (glow rgb linear, speed rad/s); params[1] = (bands per
/// metre × 2π, minimum opacity, unused, unused).
pub const PULSE: EffectDefinition = effect("pulse", include_str!("../shaders/effects/pulse.wgsl"));

/// Explosion smoke and fire (`explosion-effects.ts` puffs): a camera-facing quad
/// sized by the instance's X/Y scale, shaded as a soft disc. Instance tint is the
/// linear color and opacity. No params.
pub const PUFF: EffectDefinition = effect("puff", include_str!("../shaders/effects/puff.wgsl"));

/// A blast's scorched ground ring. `instance_data` = (age s, phase rad). No params.
pub const BLAST_RING: EffectDefinition = effect(
    "blast-ring",
    include_str!("../shaders/effects/blast_ring.wgsl"),
);

/// Fading tread marks (`tracks.ts`). `instance_data` = (birth s, strength);
/// params[0].x is the trail clock, which the track pool writes every frame.
pub const TRACK_MARK: EffectDefinition = effect(
    "track-mark",
    include_str!("../shaders/effects/track_mark.wgsl"),
);

/// Soft dust billboards (`effect-materials.ts` `dustMaterial`): track dust and
/// quarry wisps. Instance tint is the linear color and opacity. No params.
pub const DUST: EffectDefinition = effect("dust", include_str!("../shaders/effects/dust.wgsl"));

// ------------------------------------------------------------------ model effects
// The `Effect::Custom` names of `sloppy_core::models::effects_props` and
// `effects_scenery`, with the semantics documented there.

/// Burnt wreck darkening of base and emissive color (`wreck-aging.ts`).
pub const WRECK_AGING: EffectDefinition = effect(
    effects_props::WRECK_AGING,
    include_str!("../shaders/effects/wreck_aging.wgsl"),
);

/// Meadow tufts swaying in the wind (`village-vegetation.ts`).
pub const MEADOW_SWAY: EffectDefinition = effect(
    effects_scenery::MEADOW_SWAY,
    include_str!("../shaders/effects/meadow_sway.wgsl"),
);

/// Tree crowns swaying in the wind, shaded as rounded masses.
pub const FOLIAGE: EffectDefinition = EffectDefinition {
    still_shadow: true,
    ..effect(
        effects_scenery::FOLIAGE,
        include_str!("../shaders/effects/foliage.wgsl"),
    )
};

/// Screen-sized chimney wisps (`village-atmosphere.ts`).
pub const CHIMNEY_SMOKE: EffectDefinition = effect(
    effects_scenery::CHIMNEY_SMOKE,
    include_str!("../shaders/effects/chimney_smoke.wgsl"),
);

/// Baked quarry soil with world-space grit (`quarry-terrain.ts`).
pub const QUARRY_SOIL: EffectDefinition = effect(
    effects_scenery::QUARRY_SOIL,
    concat!(
        include_str!("../shaders/effects/quarry_common.wgsl"),
        include_str!("../shaders/effects/quarry_soil.wgsl"),
    ),
);

/// Triplanar layered sandstone (`quarry-surfaces.ts`).
pub const SANDSTONE: EffectDefinition = effect(
    effects_scenery::SANDSTONE,
    concat!(
        include_str!("../shaders/effects/quarry_common.wgsl"),
        include_str!("../shaders/effects/sandstone.wgsl"),
    ),
);

/// Sand drifted around rock cover (`quarry-surfaces.ts`).
pub const SAND_DRIFT: EffectDefinition = effect(
    effects_scenery::SAND_DRIFT,
    concat!(
        include_str!("../shaders/effects/quarry_common.wgsl"),
        include_str!("../shaders/effects/sand_drift.wgsl"),
    ),
);

/// Effects available before any registration: the lab samples, the game's
/// runtime effect looks and the model effects. The planar-reflecting water
/// (`effects_scenery::WATER`) is the renderer's own water pass instead.
pub const BUILTIN_EFFECTS: [EffectDefinition; 13] = [
    WAVE,
    PULSE,
    PUFF,
    BLAST_RING,
    TRACK_MARK,
    DUST,
    WRECK_AGING,
    MEADOW_SWAY,
    FOLIAGE,
    CHIMNEY_SMOKE,
    QUARRY_SOIL,
    SANDSTONE,
    SAND_DRIFT,
];

/// The renderer's effect table. Index 0 means no effect; effect `i` has id `i + 1`.
#[derive(Clone, Debug)]
pub struct EffectRegistry {
    effects: Vec<EffectDefinition>,
}

impl Default for EffectRegistry {
    fn default() -> Self {
        Self {
            effects: BUILTIN_EFFECTS.to_vec(),
        }
    }
}

impl EffectRegistry {
    /// Register or replace an effect by name; returns its id.
    pub fn register(&mut self, effect: EffectDefinition) -> u16 {
        if let Some(index) = self.effects.iter().position(|e| e.name == effect.name) {
            self.effects[index] = effect;
            return index as u16 + 1;
        }
        self.effects.push(effect);
        self.effects.len() as u16
    }

    /// The id for an effect name, `None` when unknown.
    pub fn id(&self, name: &str) -> Option<u16> {
        self.effects
            .iter()
            .position(|e| e.name == name)
            .map(|index| index as u16 + 1)
    }

    pub fn get(&self, id: u16) -> Option<&EffectDefinition> {
        id.checked_sub(1)
            .and_then(|index| self.effects.get(index as usize))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::PRESENTATION_EFFECTS;
    use crate::shader::webgl_check::validate_effect;

    /// Every builtin and presentation effect compiles in the variants its materials use.
    #[test]
    fn every_builtin_effect_is_valid_wgsl() {
        let mut registry = EffectRegistry::default();
        for effect in PRESENTATION_EFFECTS {
            registry.register(effect);
        }
        for id in 1..=(BUILTIN_EFFECTS.len() + PRESENTATION_EFFECTS.len()) as u16 {
            validate_effect(&registry, id);
        }
    }
}
