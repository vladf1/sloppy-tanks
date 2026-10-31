//! Battle sounds: the sounds `sloppy-web` plans for each event batch
//! ([`sloppy_web::sounds`]), played from the game's MP3s (`assets.rs`). Natively kira
//! decodes and mixes them; a page hands both to the browser's Web Audio.

use std::time::Duration;

use sloppy_web::sounds::Sound;

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
pub use native::Audio;
#[cfg(target_arch = "wasm32")]
pub use web::Audio;

/// How long a stolen voice takes to fade out, short enough not to be heard as a fade.
const STEAL_FADE: Duration = Duration::from_millis(15);

/// The page's Howler pools (`pool` in `src/game/audio.ts`) as a cap on each sound's
/// voices: a new one past the cap stops that sound's oldest.
fn pool(sound: Sound) -> usize {
    match sound {
        Sound::Shot
        | Sound::ShotSpread
        | Sound::ShotRocket
        | Sound::ShotRicochet
        | Sound::ShotPiercing => 16,
        Sound::Promotion => 2,
        _ => 12,
    }
}
