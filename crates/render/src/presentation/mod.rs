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
pub use view::Presentation;

use crate::effects::{EffectDefinition, effect};

/// Flag cloth rippling in the arena breeze (`flags.ts`).
pub const FLAG_CLOTH: EffectDefinition = effect(
    models::FLAG_CLOTH_EFFECT,
    include_str!("shaders/flag_cloth.wgsl"),
);

/// The pickup refill arc growing back segment by segment.
pub const PICKUP_REFILL: EffectDefinition = effect(
    models::PICKUP_REFILL_EFFECT,
    include_str!("shaders/pickup_refill.wgsl"),
);

/// Effects presentation registers before building its models.
pub const PRESENTATION_EFFECTS: [EffectDefinition; 2] = [FLAG_CLOTH, PICKUP_REFILL];
