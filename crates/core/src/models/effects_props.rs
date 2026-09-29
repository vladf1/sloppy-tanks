//! Custom shading introduced by the cover, tree and prop models: the stable
//! [`Effect::Custom`] names, their parameters and exact semantics (translated from
//! the TSL node materials), for the renderer's WGSL.
//!
//! Behaviour that `Material` already expresses needs no effect: foliage cards use
//! `alpha_test` (0.35) with `alpha_to_coverage` and double sides; per-instance tint
//! is `Instance::color`; the fading copies of shed boughs are plain transparent
//! materials whose `opacity` presentation animates ([`super::fading_material`]).
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
/// - per instance: `phase = z * 0.12 + x * 0.04` from the instance translation
///   (see [`super::flag_phase`]); the instances are pure translations.
/// - per vertex: the plane's `uv` (1.4 x 0.9 m, 16 x 6 segments).
///
/// Vertex stage, all f32:
/// ```text
/// u = uv.x; v = 1 - uv.y                     // u = 0 at the pole, v = 0 at the top
/// point(u, v):
///   ripple   = sin(u*9 - t*5 + phase + v*1.8)
///   flutter  = sin(u*19 - t*8 + phase) * 0.035 * u*u
///   reach    = u * (gust*0.3 + 1.05)
///   sideways = u * (gust*0.09 + 0.1) * ripple + flutter
///   return vec3(reach*along_x + sideways*along_z,
///               -0.9*v - u*u*(0.45 - gust*0.22) + u*0.045*ripple,
///               reach*along_z - sideways*along_x)
/// p = point(u, v)
/// up = point(u, v - 1/6); down = point(u, v + 1/6)
/// left = point(u - 1/16, v); right = point(u + 1/16, v)
/// north_east = point(u + 1/16, v - 1/6); south_west = point(u - 1/16, v + 1/6)
/// n = 0                                     // the grid's area-weighted normals
/// if u < 1 && v < 1: n += cross(down - p, right - p)
/// if u < 1 && v > 0: n += cross(p - up, north_east - up) + cross(right - p, north_east - p)
/// if u > 0 && v > 0: n += cross(p - left, up - left)
/// if u > 0 && v < 1: n += cross(south_west - left, p - left) + cross(down - south_west, p - south_west)
/// local normal = normalize(n)
/// model position = instance_translation + p  // the undeformed plane is replaced
/// ```
/// The shadow pass uses the same displaced position. The deformed cloth stays
/// within [`super::FLAG_CLOTH_BOUNDS_RADIUS`] of its instance origin.
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

/// Batched debris fade (`addDebrisFade` in `debris-fade.ts`). Params: none; the
/// material is transparent without depth writes ([`debris_fade_material`]).
///
/// Per instance: `opacity` in 0..1 (TS instanced attribute `debrisOpacity`),
/// written by presentation as fragments sink and fade.
///
/// Color pass: alpha = material alpha * instance `opacity`.
/// Shadow pass: discard the fragment unless `opacity > hash(100 * dot(pw, pw))`,
/// where `pw` is the fragment's world position and `hash` is Three's PCG hash:
/// ```text
/// state  = u32(seed) * 747796405u + 2891336453u        // wrapping, seed truncated
/// word   = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u
/// result = (word >> 22u) ^ word
/// hash   = f32(result) * (1.0 / 4294967296.0)
/// ```
/// This stable spatial mask thins the shadow with the fade without a translucent
/// shadow pass.
pub const DEBRIS_FADE: &str = "debris-fade";

/// Pickup surfaces (`pickup-visuals.ts`). Params: none.
///
/// Frame uniform `pickup_opacity` (TS `setPickupOpacity`: 1 in the overhead view,
/// the first-person pickup opacity otherwise), shared by every pickup material.
/// Alpha = material opacity * `pickup_opacity` (the materials are transparent).
/// The pictogram faces glow through their `emissive_map` (the atlas, as in
/// Three); everything else is standard shading.
pub const PICKUP_SURFACE: &str = "pickup-surface";

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

/// `addDebrisFade`'s material: a transparent copy of a fragment's surface that
/// keeps its maps, roughness and tint and fades per instance.
pub fn debris_fade_material(source: &Material) -> Material {
    Material {
        transparent: true,
        depth_write: false,
        effect: Effect::Custom {
            name: DEBRIS_FADE,
            params: Vec::new(),
        },
        ..source.clone()
    }
}
