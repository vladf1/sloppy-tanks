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
//!
//! Hooks may instead be shared ones, behind a uniform branch on the material's
//! effect: shared surface hooks (`EffectDefinition::shared_surface`) ride in every
//! lit surface variant without a surface hook of its own, so those materials draw
//! with a plain lit fragment stage, and shared vertex hooks (`shared_vertex`) in one
//! vertex stage for every effect that has them (plain variants keep their own, which
//! the branches would slow). WebKit compiles each distinct stage at first use on a
//! cold start, about 0.3 s for a lit fragment stage and 0.05 s for a vertex stage,
//! and a debris glow, a pallet's wear, a crown's normals or a billboard's corners
//! each cost one for a few lines of their own. Only the particles' few-vertex
//! billboards share vertex hooks: the branches' material read cost every vertex of
//! a tree crown, a flag or a pallet (0.005 ms of an iPad-size village frame).

use sloppy_core::models::{effects_props, effects_scenery};

/// One registered effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectDefinition {
    /// The `Effect::Custom` name that selects it.
    pub name: &'static str,
    /// WGSL defining any of `effect_vertex`, `effect_world`, `effect_surface`.
    pub wgsl: &'static str,
    /// WGSL appended after `wgsl` that defines this definition's hooks from
    /// functions its `wgsl` shares with other definitions (the kinds of [`MODES`]),
    /// so the binary holds the shared source once; usually empty.
    pub entry: &'static str,
    /// WGSL defining any of `effect_vertex`, `effect_world` and `effect_clip` (and
    /// only what they use, named for the effect) for the effects with shared vertex
    /// hooks to share one vertex stage, in place of any in `wgsl`
    /// (`shader::shader_source`); usually empty.
    pub shared_vertex: &'static str,
    /// WGSL defining `effect_surface` (and only what it uses, named for the effect)
    /// for lit materials to share, in place of one in `wgsl`; usually empty.
    pub shared_surface: &'static str,
    /// The shadow pass dithers the caster away with the instance opacity, like
    /// the renderer's faded instances (`debris-fade.ts`).
    pub shadow_fade: bool,
    /// The vertex motion is a small cosmetic sway that leaves the cut-out alpha
    /// alone, so the caster may still merge into its model's still shadow.
    pub still_shadow: bool,
    /// The hooks it defines, its own (`wgsl` and `entry`) and shared, found once
    /// when the registry takes the definition: every model part registered asks, and
    /// searching a large snippet each time added up.
    hooks: Option<[Hooks; 2]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Hooks {
    vertex: bool,
    world: bool,
    clip: bool,
    surface: bool,
}

impl Hooks {
    fn of(wgsl: &str, entry: &str) -> Self {
        let defines = |hook: &str| wgsl.contains(hook) || entry.contains(hook);
        Self {
            vertex: defines("fn effect_vertex("),
            world: defines("fn effect_world("),
            clip: defines("fn effect_clip("),
            surface: defines("fn effect_surface("),
        }
    }
}

/// An effect with the default flags.
pub(crate) const fn effect(name: &'static str, wgsl: &'static str) -> EffectDefinition {
    EffectDefinition {
        name,
        wgsl,
        entry: "",
        shared_vertex: "",
        shared_surface: "",
        shadow_fade: false,
        still_shadow: false,
        hooks: None,
    }
}

impl EffectDefinition {
    /// The definition with `wgsl` as its shared vertex hooks.
    pub(crate) const fn with_shared_vertex(self, wgsl: &'static str) -> Self {
        Self {
            shared_vertex: wgsl,
            ..self
        }
    }

    /// The definition with `wgsl` as its shared surface hook.
    pub(crate) const fn with_shared_surface(self, wgsl: &'static str) -> Self {
        Self {
            shared_surface: wgsl,
            ..self
        }
    }

    /// The definition with its hooks looked up.
    fn with_hooks(self) -> Self {
        Self {
            hooks: Some(self.find_hooks()),
            ..self
        }
    }

    fn find_hooks(&self) -> [Hooks; 2] {
        [
            Hooks::of(self.wgsl, self.entry),
            Hooks::of(self.shared_vertex, self.shared_surface),
        ]
    }

    /// Its own hooks and its shared ones.
    fn hooks(&self) -> [Hooks; 2] {
        self.hooks.unwrap_or_else(|| self.find_hooks())
    }

    // Whether it has each hook, its own or a shared one.
    pub fn has_vertex(&self) -> bool {
        self.hooks().iter().any(|hooks| hooks.vertex)
    }
    pub fn has_world(&self) -> bool {
        self.hooks().iter().any(|hooks| hooks.world)
    }
    pub fn has_clip(&self) -> bool {
        self.hooks().iter().any(|hooks| hooks.clip)
    }
    pub fn has_surface(&self) -> bool {
        self.hooks().iter().any(|hooks| hooks.surface)
    }

    /// Whether it defines vertex-stage hooks of its own, in `wgsl` or `entry`.
    pub fn has_own_vertex_hooks(&self) -> bool {
        let [own, _] = self.hooks();
        own.vertex || own.world || own.clip
    }
    /// Whether its surface hook is its own, in `wgsl` or `entry`.
    pub fn has_own_surface(&self) -> bool {
        self.hooks()[0].surface
    }
    pub fn has_shared_vertex(&self) -> bool {
        !self.shared_vertex.is_empty()
    }
    pub fn has_shared_surface(&self) -> bool {
        !self.shared_surface.is_empty()
    }
    /// Whether its materials name it to the shared hooks (`MaterialUniform::features`).
    pub fn shares_hooks(&self) -> bool {
        self.has_shared_vertex() || self.has_shared_surface()
    }
}

/// Cloth ripple for flags and banners (vertex hook). The mesh lies in its local
/// XY plane, pinned at `uv.x = 0`. params[0] = (amplitude m, wavelength m,
/// speed rad/s, unused).
pub const WAVE: EffectDefinition = effect("wave", wgsl!("shaders/effects/wave.wgsl"));

/// A glowing band that scrolls up the surface and thins its opacity (surface
/// hook). params[0] = (glow rgb linear, speed rad/s); params[1] = (bands per
/// metre × 2π, minimum opacity, unused, unused).
pub const PULSE: EffectDefinition = effect("pulse", wgsl!("shaders/effects/pulse.wgsl"));

/// Smoke, fire and dust puffs: a camera-facing quad sized by the instance's X/Y
/// scale, shaded as a sun-lit billow with a boiling noise rim that glows by its
/// heat. Instance tint is the linear smoke color and opacity; `instance_data` =
/// (seed, heat, erosion, softness). No params.
pub const PUFF: EffectDefinition = effect("puff", wgsl!("shaders/effects/puff.wgsl"))
    .with_shared_vertex(wgsl!("shaders/effects/puff_vertex.wgsl"));

/// Additive sparks, tracers, glows and flame tongues. `instance_data` =
/// (direction xyz, kind). No params.
pub const STREAK: EffectDefinition = effect("streak", wgsl!("shaders/effects/streak.wgsl"))
    .with_shared_vertex(wgsl!("shaders/effects/streak_vertex.wgsl"));

/// A blast's ground decals: dust ring, shockwave and scorch. `instance_data` =
/// (age s, phase rad, kind, strength). No params.
pub const BLAST_RING: EffectDefinition =
    effect("blast-ring", wgsl!("shaders/effects/blast_ring.wgsl"));

/// Fading tread marks (`tracks.ts`). `instance_data` = (birth s, strength);
/// params[0].x is the trail clock, which the track pool writes every frame.
pub const TRACK_MARK: EffectDefinition =
    effect("track-mark", wgsl!("shaders/effects/track_mark.wgsl"));

/// Lit debris chips with a per-instance glow for hot clods. `instance_data` =
/// (glow rgb linear, unused). No params.
pub const DEBRIS_CHIP: EffectDefinition = EffectDefinition {
    shared_surface: wgsl!("shaders/effects/debris_chip.wgsl"),
    ..effect("debris-chip", "")
};

/// Soft dust billboards (`effect-materials.ts` `dustMaterial`): track dust and
/// quarry wisps. Instance tint is the linear color and opacity. No params.
pub const DUST: EffectDefinition = effect("dust", wgsl!("shaders/effects/dust.wgsl"))
    .with_shared_vertex(wgsl!("shaders/effects/dust_vertex.wgsl"));

// ------------------------------------------------------------------ model effects
// The `Effect::Custom` names of `sloppy_core::models::effects_props` and
// `effects_scenery`, with the semantics documented there.

/// Burnt wreck darkening of base and emissive color (`wreck-aging.ts`).
pub const WRECK_AGING: EffectDefinition = EffectDefinition {
    shared_surface: wgsl!("shaders/effects/wreck_aging.wgsl"),
    ..effect(effects_props::WRECK_AGING, "")
};

/// The Pine Village meadow's functions, which its three kinds and the ground's bake
/// (`bakes::VILLAGE_GROUND_BAKE`) share.
pub(crate) const MEADOW_WGSL: &str = wgsl!("shaders/effects/meadow_sway.wgsl");

/// Meadow tufts swaying in the wind (`village-vegetation.ts`), the meadow effect's
/// kind 0 ([`MODES`]).
pub const MEADOW_SWAY: EffectDefinition = EffectDefinition {
    entry:
        "fn effect_world(w: ptr<function, EffectWorld>, v: EffectVertex) { vg_tuft_world(w, v); }
fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) { vg_tuft_surface(s, f); }
",
    ..effect(effects_scenery::MEADOW_SWAY, MEADOW_WGSL)
};

/// The village ground (`ground_surfaces.rs`), the meadow effect's kind 1, drawn from
/// its bake (`bakes::VILLAGE_GROUND_BAKE`).
pub const VILLAGE_GROUND: EffectDefinition = EffectDefinition {
    entry:
        "fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) { village_ground(s, f); }
",
    ..effect("meadow-sway:ground", MEADOW_WGSL)
};

/// The village's mountain ranges (`village_landscape.rs`), the meadow effect's kind 2.
pub const VILLAGE_RANGES: EffectDefinition = EffectDefinition {
    entry:
        "fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) { village_ranges(s, f); }
",
    ..effect("meadow-sway:ranges", MEADOW_WGSL)
};

/// Effects whose one name serves materials of different kinds, told apart by
/// `params[0]`: each kind is a definition of its own, so a pipeline holds only the
/// code its kind runs (the meadow tufts and ranges compiled the whole village ground
/// beside their own code). Per name, the definition taken when `params[0]` exceeds
/// each bound, highest first; below them all the name's own definition.
const MODES: [(&str, &[(f32, &str)]); 1] = [(
    effects_scenery::MEADOW_SWAY,
    &[(1.5, VILLAGE_RANGES.name), (0.5, VILLAGE_GROUND.name)],
)];

/// Tree crowns swaying in the wind, shaded as rounded masses.
pub const FOLIAGE: EffectDefinition = EffectDefinition {
    still_shadow: true,
    shared_surface: wgsl!("shaders/effects/foliage_surface.wgsl"),
    ..effect(
        effects_scenery::FOLIAGE,
        wgsl!("shaders/effects/foliage.wgsl"),
    )
};

/// Screen-sized chimney wisps (`village-atmosphere.ts`).
pub const CHIMNEY_SMOKE: EffectDefinition = effect(
    effects_scenery::CHIMNEY_SMOKE,
    wgsl!("shaders/effects/chimney_smoke.wgsl"),
)
.with_shared_vertex(wgsl!("shaders/effects/chimney_smoke_vertex.wgsl"));

/// Baked quarry soil with world-space grit (`quarry-terrain.ts`).
pub const QUARRY_SOIL: EffectDefinition = effect(
    effects_scenery::QUARRY_SOIL,
    wgsl!(
        "shaders/effects/quarry_common.wgsl",
        "shaders/effects/quarry_soil.wgsl",
    ),
);

/// Triplanar layered sandstone (`quarry-surfaces.ts`).
pub const SANDSTONE: EffectDefinition = effect(
    effects_scenery::SANDSTONE,
    wgsl!(
        "shaders/effects/quarry_common.wgsl",
        "shaders/effects/sandstone.wgsl",
    ),
);

/// Sand drifted around rock cover (`quarry-surfaces.ts`).
pub const SAND_DRIFT: EffectDefinition = effect(
    effects_scenery::SAND_DRIFT,
    wgsl!(
        "shaders/effects/quarry_common.wgsl",
        "shaders/effects/sand_drift.wgsl",
    ),
);

/// The harbor apron's functions, which its three kinds share (and its wear bake,
/// `bakes::HARBOR_APRON_WEAR`).
pub(crate) const HARBOR_APRON_WGSL: &str = wgsl!("shaders/effects/harbor_apron.wgsl");

/// Harbor quay concrete and asphalt in world space (`harbor_surfaces`), over the
/// baked wear, the paint on it, and its quay walls, hulls, spawn pads, cover
/// containers and parapet: every apron material, so they share one shader (WebKit
/// compiles each distinct shader on a cold start, and a lit one's lighting costs
/// about as much as the apron's own code).
pub const HARBOR_APRON: EffectDefinition = EffectDefinition {
    entry: "fn effect_surface(s: ptr<function, Surface>, f: EffectFragment) {
    let mode = material.params[0].x;
    if mode < 2.5 {
        apron_ground(s, f);
    } else if mode < 4.5 {
        apron_paint(s, f);
    } else {
        apron_structure(s, f);
    }
}
",
    ..effect(effects_scenery::HARBOR_APRON, HARBOR_APRON_WGSL)
};

/// Harbor Havoc's far shore, lit and hazed in the effect (`harbor_backdrop`).
pub const HARBOR_SHORE: EffectDefinition = effect(
    effects_scenery::HARBOR_SHORE,
    wgsl!("shaders/effects/harbor_shore.wgsl"),
);

/// Worn, dirt-splashed vehicle paint with a team rim (`tank_surfaces.rs`).
pub const VEHICLE_PAINT: EffectDefinition = effect(
    sloppy_core::models::VEHICLE_PAINT_EFFECT,
    wgsl!("shaders/effects/vehicle_paint.wgsl"),
);

/// Effects available before any registration: the lab samples, the game's
/// runtime effect looks and the model effects. The planar-reflecting water
/// (`effects_scenery::WATER`) is the renderer's own water pass instead.
pub const BUILTIN_EFFECTS: [EffectDefinition; 20] = [
    WAVE,
    PULSE,
    PUFF,
    STREAK,
    BLAST_RING,
    TRACK_MARK,
    DUST,
    WRECK_AGING,
    MEADOW_SWAY,
    VILLAGE_GROUND,
    VILLAGE_RANGES,
    FOLIAGE,
    CHIMNEY_SMOKE,
    QUARRY_SOIL,
    SANDSTONE,
    SAND_DRIFT,
    HARBOR_APRON,
    VEHICLE_PAINT,
    DEBRIS_CHIP,
    HARBOR_SHORE,
];

/// The renderer's effect table. Index 0 means no effect; effect `i` has id `i + 1`.
#[derive(Clone, Debug)]
pub struct EffectRegistry {
    effects: Vec<EffectDefinition>,
}

impl Default for EffectRegistry {
    fn default() -> Self {
        Self {
            effects: BUILTIN_EFFECTS.map(EffectDefinition::with_hooks).to_vec(),
        }
    }
}

impl EffectRegistry {
    /// Register or replace an effect by name; returns its id.
    pub fn register(&mut self, effect: EffectDefinition) -> u16 {
        let effect = effect.with_hooks();
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

    /// The id for a material's `Effect::Custom { name, params }`: the definition of
    /// the kind `params[0]` selects for an effect with [`MODES`], else the name's.
    pub fn id_for(&self, name: &str, params: &[f32]) -> Option<u16> {
        let selector = params.first().copied().unwrap_or(0.0);
        let mode = MODES
            .iter()
            .find(|(effect, _)| *effect == name)
            .and_then(|(_, modes)| modes.iter().find(|(above, _)| selector > *above))
            .map(|&(_, mode)| mode);
        self.id(mode.unwrap_or(name))
    }

    /// Every id an effect name stands for: its own and its kinds' ([`MODES`]).
    pub fn ids_named(&self, name: &str) -> Vec<u16> {
        let modes = MODES
            .iter()
            .filter(|(effect, _)| *effect == name)
            .flat_map(|(_, modes)| modes.iter().map(|&(_, mode)| mode));
        std::iter::once(name)
            .chain(modes)
            .filter_map(|name| self.id(name))
            .collect()
    }

    /// The effects with shared hooks, by id.
    pub fn sharing_hooks(&self) -> impl Iterator<Item = (u16, &EffectDefinition)> {
        (1..=self.effects.len() as u16)
            .zip(&self.effects)
            .filter(|(_, effect)| effect.shares_hooks())
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

    /// The hooks a registered definition keeps are the ones its WGSL defines.
    #[test]
    fn registered_hooks_match_the_snippet() {
        let mut registry = EffectRegistry::default();
        for effect in PRESENTATION_EFFECTS {
            registry.register(effect);
        }
        for id in 1..=(BUILTIN_EFFECTS.len() + PRESENTATION_EFFECTS.len()) as u16 {
            let effect = registry.get(id).expect("registered");
            assert!(effect.hooks.is_some());
            let source = format!("{}\n{}", effect.wgsl, effect.entry);
            let shared = format!("{}\n{}", effect.shared_vertex, effect.shared_surface);
            let defines = |hook: &str| source.contains(hook) || shared.contains(hook);
            assert_eq!(effect.has_vertex(), defines("fn effect_vertex("));
            assert_eq!(effect.has_world(), defines("fn effect_world("));
            assert_eq!(effect.has_clip(), defines("fn effect_clip("));
            assert_eq!(effect.has_surface(), defines("fn effect_surface("));
            assert_eq!(
                effect.has_own_surface(),
                source.contains("fn effect_surface(")
            );
            // Shared hooks replace own ones, each snippet defining only its stage's.
            let vertex_hooks = ["fn effect_vertex(", "fn effect_world(", "fn effect_clip("];
            if effect.has_shared_vertex() {
                assert!(!effect.has_own_vertex_hooks(), "{}", effect.name);
                assert!(
                    vertex_hooks
                        .iter()
                        .any(|hook| effect.shared_vertex.contains(hook))
                );
                assert!(!effect.shared_vertex.contains("fn effect_surface("));
            }
            if effect.has_shared_surface() {
                assert!(!effect.has_own_surface(), "{}", effect.name);
                assert!(effect.shared_surface.contains("fn effect_surface("));
                for hook in vertex_hooks {
                    assert!(!effect.shared_surface.contains(hook), "{}", effect.name);
                }
            }
        }
    }

    /// Each kind of meadow material takes its own definition, and only the tufts sway.
    #[test]
    fn meadow_kinds_take_their_own_definitions() {
        let registry = EffectRegistry::default();
        let name = effects_scenery::MEADOW_SWAY;
        let kind = |selector: f32| {
            let id = registry
                .id_for(name, &[selector, 60.0])
                .expect("registered");
            registry.get(id).expect("registered")
        };
        assert_eq!(kind(0.0).name, MEADOW_SWAY.name);
        assert_eq!(kind(1.0).name, VILLAGE_GROUND.name);
        assert_eq!(kind(2.0).name, VILLAGE_RANGES.name);
        assert!(kind(0.0).has_world());
        assert!(!kind(1.0).has_world() && kind(1.0).has_surface());
        assert!(!kind(2.0).has_world() && kind(2.0).has_surface());
        assert_eq!(registry.ids_named(name).len(), 3);
        assert_eq!(registry.id_for("puff", &[2.0]), registry.id("puff"));
    }

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
