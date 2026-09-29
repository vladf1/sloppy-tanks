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
//! // Before alpha test, lighting and fog (colorNode, opacityNode, normalNode,
//! // roughness/metalness/emissive nodes). The shadow pass runs it too.
//! fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) { ... }
//! ```
//!
//! Snippets read `material.params` (the `Effect::Custom` params, 16 floats as
//! four vec4s), `frame.camera_position.w` (time in seconds), the frame's camera
//! vectors, per-instance `instance_data`, and up to two named mesh attributes as
//! `extra0`/`extra1`. Shader variants are assembled per material and cached by
//! variant key, so an effect costs one pipeline per distinct material setup.

/// One registered effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectDefinition {
    /// The `Effect::Custom` name that selects it.
    pub name: &'static str,
    /// WGSL defining any of `effect_vertex`, `effect_world`, `effect_surface`.
    pub wgsl: &'static str,
    /// Mesh attributes bound to `extra0` and `extra1`, in order (at most two).
    pub attributes: &'static [&'static str],
}

impl EffectDefinition {
    pub fn has_vertex(&self) -> bool {
        self.wgsl.contains("fn effect_vertex(")
    }
    pub fn has_world(&self) -> bool {
        self.wgsl.contains("fn effect_world(")
    }
    pub fn has_surface(&self) -> bool {
        self.wgsl.contains("fn effect_surface(")
    }
}

/// Cloth ripple for flags and banners (vertex hook). The mesh lies in its local
/// XY plane, pinned at `uv.x = 0`. params[0] = (amplitude m, wavelength m,
/// speed rad/s, unused).
pub const WAVE: EffectDefinition = EffectDefinition {
    name: "wave",
    wgsl: include_str!("../shaders/effects/wave.wgsl"),
    attributes: &[],
};

/// A glowing band that scrolls up the surface and thins its opacity (surface
/// hook). params[0] = (glow rgb linear, speed rad/s); params[1] = (bands per
/// metre × 2π, minimum opacity, unused, unused).
pub const PULSE: EffectDefinition = EffectDefinition {
    name: "pulse",
    wgsl: include_str!("../shaders/effects/pulse.wgsl"),
    attributes: &[],
};

/// Explosion smoke and fire (`explosion-effects.ts` puffs): a camera-facing quad
/// sized by the instance's X/Y scale, shaded as a soft disc. Instance tint is the
/// linear color and opacity. No params.
pub const PUFF: EffectDefinition = EffectDefinition {
    name: "puff",
    wgsl: include_str!("../shaders/effects/puff.wgsl"),
    attributes: &[],
};

/// A blast's scorched ground ring. `instance_data` = (age s, phase rad). No params.
pub const BLAST_RING: EffectDefinition = EffectDefinition {
    name: "blast-ring",
    wgsl: include_str!("../shaders/effects/blast_ring.wgsl"),
    attributes: &[],
};

/// Fading tread marks (`tracks.ts`). `instance_data` = (birth s, strength);
/// params[0].x is the trail clock, which the track pool writes every frame.
pub const TRACK_MARK: EffectDefinition = EffectDefinition {
    name: "track-mark",
    wgsl: include_str!("../shaders/effects/track_mark.wgsl"),
    attributes: &[],
};

/// Soft dust billboards (`effect-materials.ts` `dustMaterial`): track dust and
/// quarry wisps. Instance tint is the linear color and opacity. No params.
pub const DUST: EffectDefinition = EffectDefinition {
    name: "dust",
    wgsl: include_str!("../shaders/effects/dust.wgsl"),
    attributes: &[],
};

/// Effects available before any registration: the lab samples and the game's
/// runtime effect looks.
pub const BUILTIN_EFFECTS: [EffectDefinition; 6] =
    [WAVE, PULSE, PUFF, BLAST_RING, TRACK_MARK, DUST];

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
        assert!(
            effect.attributes.len() <= 2,
            "effects read at most two attributes"
        );
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

    pub fn attributes(&self, id: u16) -> &'static [&'static str] {
        self.get(id).map_or(&[], |effect| effect.attributes)
    }

    pub fn iter(&self) -> impl Iterator<Item = (u16, &EffectDefinition)> {
        self.effects
            .iter()
            .enumerate()
            .map(|(index, effect)| (index as u16 + 1, effect))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shader::{Pass, ShaderKey, shader_source};
    use sloppy_core::scene::Side;

    fn validate(label: &str, code: &str) {
        let module = naga::front::wgsl::parse_str(code)
            .unwrap_or_else(|error| panic!("{label}: {}", error.emit_to_string(code)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap_or_else(|error| panic!("{label}: {error:?}"));
    }

    /// Every builtin effect compiles in the variants its materials use: basic and
    /// lit, with and without vertex colors, double-sided, and the shadow pass.
    #[test]
    fn every_builtin_effect_is_valid_wgsl() {
        let registry = EffectRegistry::default();
        for (id, effect) in registry.iter() {
            for (lit, vertex_colors, side) in [
                (false, false, Side::Front),
                (true, true, Side::Front),
                (false, false, Side::Double),
            ] {
                let key = ShaderKey {
                    pass: Pass::Main,
                    lit,
                    vertex_colors,
                    fog: true,
                    side,
                    effect: id,
                    ..ShaderKey::default()
                };
                validate(effect.name, &shader_source(&key, &registry));
            }
            let shadow = ShaderKey {
                pass: Pass::Shadow,
                effect: id,
                ..ShaderKey::default()
            };
            validate(effect.name, &shader_source(&shadow, &registry));
        }
        assert_eq!(registry.iter().count(), BUILTIN_EFFECTS.len());
    }
}
