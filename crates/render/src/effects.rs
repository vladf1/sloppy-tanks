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
    wgsl: include_str!("shaders/effects/wave.wgsl"),
    attributes: &[],
};

/// A glowing band that scrolls up the surface and thins its opacity (surface
/// hook). params[0] = (glow rgb linear, speed rad/s); params[1] = (bands per
/// metre × 2π, minimum opacity, unused, unused).
pub const PULSE: EffectDefinition = EffectDefinition {
    name: "pulse",
    wgsl: include_str!("shaders/effects/pulse.wgsl"),
    attributes: &[],
};

/// Effects available before any registration. Later waves add the game's own.
pub const BUILTIN_EFFECTS: [EffectDefinition; 2] = [WAVE, PULSE];

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
        assert!(effect.attributes.len() <= 2, "effects read at most two attributes");
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
