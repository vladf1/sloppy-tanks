//! Battle sounds in a page: the browser's Web Audio decodes the MP3s and plays each
//! voice through its own gain and pan into the master gain, so no mixing runs in the
//! Wasm on the main thread, and no decoder ships in it.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AudioBuffer, AudioBufferSourceNode, AudioContext, AudioScheduledSourceNode, GainNode,
};

use sloppy_web::events::EventBatch;
use sloppy_web::sounds::{PlannedSound, Sound};

use super::{STEAL_FADE, pool};
use crate::assets::Asset;

/// A sound's decoded file.
enum Clip {
    Ready(AudioBuffer),
    /// The page is still downloading the file: looked for again at each play.
    Loading,
    /// The browser is decoding it.
    Decoding,
    /// Missing, or it failed to decode.
    Silent,
}

/// A playing voice: its source, its gain (which a steal fades) and when it ends, on
/// the context's clock.
struct Voice {
    source: AudioBufferSourceNode,
    gain: GainNode,
    ends: f64,
}

pub struct Audio {
    context: AudioContext,
    master: GainNode,
    /// In `Sound::ALL` order; a decode fills its slot when the browser has it.
    sounds: Vec<Rc<RefCell<Clip>>>,
    /// Each sound's voices still playing, oldest first.
    voices: Vec<Vec<Voice>>,
}

impl Audio {
    /// The page's audio, or `None` where the browser has none.
    pub fn new() -> Option<Self> {
        let context = AudioContext::new()
            .map_err(|error| report!("No sound: {error:?}"))
            .ok()?;
        let master = context.create_gain().ok()?;
        master
            .connect_with_audio_node(&context.destination())
            .ok()?;
        let sounds = Sound::ALL
            .iter()
            .map(|_| Rc::new(RefCell::new(Clip::Loading)))
            .collect();
        let mut audio = Self {
            context,
            master,
            sounds,
            voices: Sound::ALL.iter().map(|_| Vec::new()).collect(),
        };
        for sound in Sound::ALL {
            audio.decode(sound);
        }
        Some(audio)
    }

    /// Master volume, 0-1.
    pub fn set_volume(&mut self, volume: f64) {
        self.master.gain().set_value(volume.clamp(0.0, 1.0) as f32);
    }

    /// Start an event batch's planned sounds.
    pub fn play(&mut self, batch: &EventBatch) {
        for &planned in &batch.sounds {
            self.start(planned);
        }
    }

    fn start(&mut self, planned: PlannedSound) {
        let index = planned.sound as usize;
        if matches!(*self.sounds[index].borrow(), Clip::Loading) {
            self.decode(planned.sound);
        }
        let Clip::Ready(buffer) = &*self.sounds[index].borrow() else {
            return;
        };
        let now = self.context.current_time();
        let playing = &mut self.voices[index];
        playing.retain(|voice| voice.ends > now);
        while playing.len() >= pool(planned.sound) {
            let oldest = playing.remove(0);
            let fade = STEAL_FADE.as_secs_f64();
            let gain = oldest.gain.gain();
            let _ = gain.set_value_at_time(gain.value(), now);
            let _ = gain.linear_ramp_to_value_at_time(0.0, now + fade);
            let _ = AudioScheduledSourceNode::stop_with_when(&oldest.source, now + fade);
        }
        if let Some(voice) = voice(&self.context, &self.master, buffer, planned) {
            self.voices[index].push(voice);
        }
    }

    /// Hand `sound`'s file to the browser to decode, once the page has it.
    fn decode(&mut self, sound: Sound) {
        let path = format!("audio/{}.mp3", sound.name());
        let slot = Rc::clone(&self.sounds[sound as usize]);
        let bytes = match crate::assets::load(&path) {
            Asset::Ready(bytes) => bytes,
            Asset::Loading => return,
            Asset::Missing => {
                *slot.borrow_mut() = Clip::Silent;
                return;
            }
        };
        let file = js_sys::Uint8Array::from(&bytes[..]).buffer();
        let Ok(promise) = self.context.decode_audio_data(&file) else {
            *slot.borrow_mut() = Clip::Silent;
            return;
        };
        *slot.borrow_mut() = Clip::Decoding;
        wasm_bindgen_futures::spawn_local(async move {
            let clip = match JsFuture::from(promise).await {
                Ok(buffer) => buffer
                    .dyn_into::<AudioBuffer>()
                    .map_or(Clip::Silent, Clip::Ready),
                Err(error) => {
                    report!("Cannot decode {path}: {error:?}");
                    Clip::Silent
                }
            };
            *slot.borrow_mut() = clip;
        });
    }
}

/// `buffer` playing on `context` at `planned`'s gain and pan, into `master`.
fn voice(
    context: &AudioContext,
    master: &GainNode,
    buffer: &AudioBuffer,
    planned: PlannedSound,
) -> Option<Voice> {
    let source = context.create_buffer_source().ok()?;
    source.set_buffer(Some(buffer));
    let gain = context.create_gain().ok()?;
    gain.gain().set_value(planned.gain.max(0.0) as f32);
    let panner = context.create_stereo_panner().ok()?;
    panner.pan().set_value(planned.pan as f32);
    source
        .connect_with_audio_node(&gain)
        .and_then(|node| node.connect_with_audio_node(&panner))
        .and_then(|node| node.connect_with_audio_node(master))
        .ok()?;
    AudioScheduledSourceNode::start(&source).ok()?;
    let ends = context.current_time() + buffer.duration();
    Some(Voice { source, gain, ends })
}
