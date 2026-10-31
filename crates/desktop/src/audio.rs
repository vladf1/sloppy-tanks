//! Battle sounds: the sounds `sloppy-web` plans for each event batch
//! ([`sloppy_web::sounds`]), played from the game's MP3s (`assets.rs`).

use std::io::Cursor;
use std::time::Duration;

use kira::sound::PlaybackState;
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::track::MainTrackBuilder;
use kira::{AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Panning, Tween};

use sloppy_web::events::EventBatch;
use sloppy_web::sounds::{PlannedSound, Sound};

use crate::assets::Asset;

/// Room for every sound's whole pool at once, with the stopping voices a steal leaves fading.
const SOUND_CAPACITY: usize = 256;
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

/// Linear gain as kira's decibels; zero is silence.
fn decibels(gain: f64) -> Decibels {
    if gain <= 0.0 {
        Decibels::SILENCE
    } else {
        Decibels(((20.0 * gain.log10()) as f32).max(Decibels::SILENCE.0))
    }
}

/// A sound's decoded file.
enum Clip {
    Ready(Box<StaticSoundData>),
    /// A page is still downloading the file: looked for again at each play.
    Loading,
    /// Missing, or it failed to decode.
    Silent,
}

pub struct Audio {
    manager: AudioManager<DefaultBackend>,
    /// Decoded once, in `Sound::ALL` order.
    sounds: Vec<Clip>,
    /// Each sound's voices still playing, oldest first.
    voices: Vec<Vec<StaticSoundHandle>>,
}

impl Audio {
    /// The output device, or `None` where the machine has none.
    pub fn new() -> Option<Self> {
        let settings = AudioManagerSettings {
            main_track_builder: MainTrackBuilder::new().sound_capacity(SOUND_CAPACITY),
            ..Default::default()
        };
        let manager = AudioManager::<DefaultBackend>::new(settings)
            .map_err(|error| report!("No sound: {error}"))
            .ok()?;
        Some(Self {
            manager,
            sounds: Sound::ALL.iter().map(|&sound| decode(sound)).collect(),
            voices: Sound::ALL.iter().map(|_| Vec::new()).collect(),
        })
    }

    /// Master volume, 0-1.
    pub fn set_volume(&mut self, volume: f64) {
        self.manager
            .main_track()
            .set_volume(decibels(volume.clamp(0.0, 1.0)), Tween::default());
    }

    /// Start an event batch's planned sounds.
    pub fn play(&mut self, batch: &EventBatch) {
        for &planned in &batch.sounds {
            self.start(planned);
        }
    }

    fn start(&mut self, planned: PlannedSound) {
        let index = planned.sound as usize;
        if matches!(self.sounds[index], Clip::Loading) {
            self.sounds[index] = decode(planned.sound);
        }
        let Clip::Ready(data) = &self.sounds[index] else {
            return;
        };
        let data = data
            .volume(decibels(planned.gain))
            .panning(Panning(planned.pan as f32));
        let playing = &mut self.voices[index];
        playing.retain(|handle| handle.state() != PlaybackState::Stopped);
        while playing.len() >= pool(planned.sound) {
            let mut oldest = playing.remove(0);
            oldest.stop(Tween {
                duration: STEAL_FADE,
                ..Default::default()
            });
        }
        if let Ok(handle) = self.manager.play(data) {
            playing.push(handle);
        }
    }
}

fn decode(sound: Sound) -> Clip {
    let path = format!("audio/{}.mp3", sound.name());
    let bytes = match crate::assets::load(&path) {
        Asset::Ready(bytes) => bytes,
        Asset::Loading => return Clip::Loading,
        Asset::Missing => return Clip::Silent,
    };
    StaticSoundData::from_cursor(Cursor::new(bytes))
        .map_err(|error| report!("Cannot decode {path}: {error}"))
        .map_or(Clip::Silent, |data| Clip::Ready(Box::new(data)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sound_is_embedded_and_decodes() {
        for sound in Sound::ALL {
            assert!(matches!(decode(sound), Clip::Ready(_)), "{}", sound.name());
            assert_eq!(Sound::ALL[sound as usize], sound);
        }
    }

    #[test]
    fn gain_converts_to_decibels_with_zero_silent() {
        assert_eq!(decibels(1.0), Decibels(0.0));
        assert!((decibels(0.5).0 + 6.0206).abs() < 1e-3);
        assert_eq!(decibels(0.0), Decibels::SILENCE);
    }
}
