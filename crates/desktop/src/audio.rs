//! Battle sounds (`src/game/audio.ts`): engine events played from the embedded MP3s.
//! Filled in by the audio port; silent until then.

use crate::engine_api::EventBatch;

pub struct Audio;

impl Audio {
    /// The output device, or `None` where the machine has none.
    pub fn new() -> Option<Self> {
        Some(Self)
    }

    /// Master volume, 0-1.
    pub fn set_volume(&mut self, _volume: f64) {}

    /// Play an event batch's sounds.
    pub fn play(&mut self, _batch: &EventBatch) {}
}
