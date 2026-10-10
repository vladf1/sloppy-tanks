//! Custom shading introduced by the cover, tree and prop models: the stable
//! [`Effect::Custom`] names and their parameters and inputs (the TSL node
//! materials' replacements), implemented in the renderer's WGSL.
//!
//! Behaviour that `Material` already expresses needs no effect: foliage cards use
//! `alpha_test` (0.35) with `alpha_to_coverage` and double sides; per-instance tint
//! is `Instance::color`.
//!
//! Rock cover draws with the scenery's sandstone and sand-drift effects
//! ([`super::effects_scenery::SANDSTONE`], [`super::effects_scenery::SAND_DRIFT`]).

use crate::scene::{Effect, Material};

/// Waving flag cloth (`Flags.clothPosition` in `flags.ts`). Params: none.
///
/// Inputs:
/// - frame uniforms `t` (seconds, the presentation clock passed to
///   `Flags.update(time)`) and `breeze = (gust, along_x, along_z)` where
///   `along_x = sin(direction)`, `along_z = cos(direction)`; presentation eases
///   `gust` in 0.15..1 and `direction` in ±0.65 rad toward new random targets
///   every 3–7 s with smoothstep `p*p*(3-2p)`.
/// - per instance: `phase = z * 0.12 + x * 0.04` from the instance translation;
///   the instances are pure translations.
/// - per vertex: the plane's `uv` (1.4 x 0.9 m, 16 x 6 segments).
///
/// The vertex stage replaces the undeformed plane with the rippling cloth and its
/// grid normals (the renderer's `presentation/shaders/flag_cloth.wgsl`); the shadow
/// pass uses the same displaced position. The deformed cloth stays within 2 m of
/// its instance origin.
pub const FLAG_CLOTH: &str = "flag-cloth";

/// Burnt wreck darkening (`ageWreckMaterial` in `wreck-aging.ts`). Params:
/// `[brightness]`, from [`wreck_brightness`] of the seconds since the vehicle died
/// (0.8 at death, falling linearly to 0.2 after 2.5 s).
///
/// Fragment: the material's base color (after vertex colors and map) and its
/// emissive color are both multiplied by `brightness`; everything else is
/// standard shading. The TypeScript kept one material clone per wreck and rewrote
/// its colors every frame; presentation can do the same with per-wreck materials
/// ([`aged_wreck_material`]) or feed `brightness` per instance for instanced wrecks.
pub const WRECK_AGING: &str = "wreck-aging";

/// Seconds after death over which a wreck darkens fully.
const WRECK_AGING_SECONDS: f64 = 2.5;

/// The brightness of a wreck's paint `seconds_since_death` after it died.
pub fn wreck_brightness(seconds_since_death: f64) -> f64 {
    0.8 - 0.6 * (seconds_since_death / WRECK_AGING_SECONDS).clamp(0.0, 1.0)
}

/// A per-wreck copy of `base` darkened for `seconds_since_death`.
pub fn aged_wreck_material(base: &Material, seconds_since_death: f64) -> Material {
    Material {
        effect: Effect::Custom {
            name: WRECK_AGING,
            params: vec![wreck_brightness(seconds_since_death) as f32],
        },
        ..base.clone()
    }
}
