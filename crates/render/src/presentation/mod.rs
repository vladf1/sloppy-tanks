//! Presentation: turns `RenderState` into renderer instances each frame. Port of
//! `presentation.ts` and its helpers (view settings, cameras and first person,
//! suspension, world-space HUD, pickups, mines, debris, wrecks, felling trees,
//! flags, theme lighting and water, warm-up).
//!
//! It never moves simulation state: poses are interpolated copies, and
//! render-only motion (hit shake, suspension, recoil, felling boughs) lives here.
//! Pure calculations compile natively and carry the unit tests; [`Presentation`]
//! drives the browser renderer.
//!
//! Draw-call budget: vehicles, bars, pickups, mines and identical covers are
//! instances of models shared by kind and team, so the renderer batches every
//! instance of a part into one instanced draw; themed scenery is baked static.

pub mod camera_rig;
pub mod first_person;
pub mod generated;
pub mod hud;
pub mod input;
pub mod model_catalog;
pub mod models;
pub mod posing;
pub mod preparation;
pub mod suspension;
pub mod theme;
pub mod view_settings;

#[cfg(target_arch = "wasm32")]
mod view;

#[cfg(target_arch = "wasm32")]
pub use view::{
    CoverInspection, FragmentInspection, PickupInspection, PrepareStatus, Presentation,
    PresentationStats, ReticleInspection, TankInspection, ViewInspection,
};

use crate::effects::EffectDefinition;

/// Flag cloth rippling in the arena breeze (`flags.ts`).
pub const FLAG_CLOTH: EffectDefinition = EffectDefinition {
    name: models::FLAG_CLOTH_EFFECT,
    wgsl: include_str!("shaders/flag_cloth.wgsl"),
    attributes: &[],
    shadow_fade: false,
};

/// The pickup refill arc growing back segment by segment.
pub const PICKUP_REFILL: EffectDefinition = EffectDefinition {
    name: models::PICKUP_REFILL_EFFECT,
    wgsl: include_str!("shaders/pickup_refill.wgsl"),
    attributes: &[],
    shadow_fade: false,
};

/// Effects presentation registers before building its models.
pub const PRESENTATION_EFFECTS: [EffectDefinition; 2] = [FLAG_CLOTH, PICKUP_REFILL];

/// Cosmetic randomness (wind gusts, falling boughs). Never gameplay: the seeded
/// simulation stream is untouched.
#[derive(Clone, Copy, Debug)]
pub struct CosmeticRandom(u64);

impl CosmeticRandom {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    /// A value in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        // xorshift64*
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        (x.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::EffectRegistry;
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

    #[test]
    fn presentation_effects_are_valid_wgsl() {
        let mut effects = EffectRegistry::default();
        for effect in PRESENTATION_EFFECTS {
            let id = effects.register(effect);
            for pass in [Pass::Main, Pass::Shadow] {
                for (map, lit) in [(false, true), (true, true), (false, false)] {
                    let key = ShaderKey {
                        pass,
                        lit,
                        map,
                        fog: true,
                        side: Side::Double,
                        effect: id,
                        ..ShaderKey::default()
                    };
                    validate(effect.name, &shader_source(&key, &effects));
                }
            }
        }
    }

    #[test]
    fn cosmetic_random_stays_in_range() {
        let mut random = CosmeticRandom::new(7);
        for _ in 0..1000 {
            let value = random.next_f64();
            assert!((0.0..1.0).contains(&value));
        }
    }
}
