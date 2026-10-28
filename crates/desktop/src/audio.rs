//! Battle sounds (`src/game/audio.ts`): engine events played from the embedded MP3s,
//! placed by distance and stereo side from the human's tank.

use std::io::Cursor;
use std::time::{Duration, Instant};

use kira::sound::PlaybackState;
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::track::MainTrackBuilder;
use kira::{AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Panning, Tween};

use crate::engine_api::{EngineEvent, EventBatch, Point};

const HIT_INTERVAL: Duration = Duration::from_millis(80);
const BOT_SHOT_INTERVAL: Duration = Duration::from_millis(35);
const EXPLOSION_INTERVAL: Duration = Duration::from_millis(70);
const DESTRUCTION_INTERVAL: Duration = Duration::from_millis(80);
const LASER_INTERVAL: Duration = Duration::from_millis(50);
/// Events farther than this from the listener stay silent.
const AUDIBLE_DISTANCE: f64 = 38.0;
/// The distance at which the fade would reach zero; `MIN_DISTANCE_GAIN` holds it above.
const FADE_DISTANCE: f64 = 40.0;
const MIN_DISTANCE_GAIN: f64 = 0.05;
/// Sideways distance at which a sound is fully in one speaker.
const STEREO_DISTANCE: f64 = 25.0;
/// The player's own hit tick: quiet and centred.
const HIT_GAIN: f64 = 0.14;
/// Room for every sound's whole pool at once, with the stopping voices a steal leaves fading.
const SOUND_CAPACITY: usize = 256;
/// How long a stolen voice takes to fade out, short enough not to be heard as a fade.
const STEAL_FADE: Duration = Duration::from_millis(15);

/// The 13 embedded sounds. The page's Howler pools (`pool`) become a cap on each sound's
/// voices: a new one past the cap stops that sound's oldest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sound {
    Shot,
    ShotSpread,
    ShotRocket,
    ShotRicochet,
    ShotPiercing,
    Explosion,
    WoodBreak,
    RubbleBreak,
    Impact,
    Pickup,
    Hit,
    Laser,
    Promotion,
}

impl Sound {
    const ALL: [Sound; 13] = [
        Sound::Shot,
        Sound::ShotSpread,
        Sound::ShotRocket,
        Sound::ShotRicochet,
        Sound::ShotPiercing,
        Sound::Explosion,
        Sound::WoodBreak,
        Sound::RubbleBreak,
        Sound::Impact,
        Sound::Pickup,
        Sound::Hit,
        Sound::Laser,
        Sound::Promotion,
    ];

    fn name(self) -> &'static str {
        match self {
            Sound::Shot => "shot",
            Sound::ShotSpread => "shot-spread",
            Sound::ShotRocket => "shot-rocket",
            Sound::ShotRicochet => "shot-ricochet",
            Sound::ShotPiercing => "shot-piercing",
            Sound::Explosion => "explosion",
            Sound::WoodBreak => "wood-break",
            Sound::RubbleBreak => "rubble-break",
            Sound::Impact => "impact",
            Sound::Pickup => "pickup",
            Sound::Hit => "hit",
            Sound::Laser => "laser",
            Sound::Promotion => "promotion",
        }
    }

    fn pool(self) -> usize {
        match self {
            Sound::Shot
            | Sound::ShotSpread
            | Sound::ShotRocket
            | Sound::ShotRicochet
            | Sound::ShotPiercing => 16,
            Sound::Promotion => 2,
            _ => 12,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// A weapon's firing sound; `None` for a weapon this build does not know.
fn shot_sound(weapon: Option<&str>) -> Option<Sound> {
    match weapon.unwrap_or("standard") {
        "standard" => Some(Sound::Shot),
        "spread" => Some(Sound::ShotSpread),
        "rocket" | "tow" => Some(Sound::ShotRocket),
        "ricochet" => Some(Sound::ShotRicochet),
        "piercing" => Some(Sound::ShotPiercing),
        _ => None,
    }
}

/// Drums emit a separate blast event, so their shell plays nothing of its own. A shell
/// bursting a loose crate or watchtower piece (`shatter`) breaks wood.
fn destruction_sound(kind: &str, cover_kind: Option<&str>) -> Option<Sound> {
    match (kind, cover_kind) {
        ("shatter", _) => Some(Sound::WoodBreak),
        ("destroy", Some("drum")) => None,
        ("destroy", Some("tree" | "timber" | "cargo")) => Some(Sound::WoodBreak),
        ("destroy", _) => Some(Sound::RubbleBreak),
        _ => None,
    }
}

/// Distance fade, never quite silent within the audible range.
fn distance_gain(distance: f64) -> f64 {
    (1.0 - distance / FADE_DISTANCE).max(MIN_DISTANCE_GAIN)
}

/// Stereo position from how far the event lies along screen right.
fn stereo_pan(event: Point, listener: Point, right: Point) -> f64 {
    let across = (event.x - listener.x) * right.x + (event.z - listener.z) * right.z;
    (across / STEREO_DISTANCE).clamp(-1.0, 1.0)
}

/// Linear gain as kira's decibels; zero is silence.
fn decibels(gain: f64) -> Decibels {
    if gain <= 0.0 {
        Decibels::SILENCE
    } else {
        Decibels(((20.0 * gain.log10()) as f32).max(Decibels::SILENCE.0))
    }
}

/// When each throttled kind last played.
#[derive(Default)]
struct RateLimits {
    hit: Option<Instant>,
    shot: Option<Instant>,
    explosion: Option<Instant>,
    destruction: Option<Instant>,
    laser: Option<Instant>,
}

fn ready(last: Option<Instant>, now: Instant, interval: Duration) -> bool {
    last.is_none_or(|last| now.saturating_duration_since(last) >= interval)
}

/// One sound to start: which, how loud (linear) and where (-1 left to 1 right).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Voice {
    sound: Sound,
    gain: f64,
    pan: f64,
}

/// The sounds one event plays at `now`, updating the rate limits as `audio.ts` does:
/// the player's hit tick first, then the event's own sound if it is near enough and its
/// kind has not played too recently.
fn plan_event(
    event: &EngineEvent,
    listener: Point,
    right: Point,
    now: Instant,
    limits: &mut RateLimits,
) -> [Option<Voice>; 2] {
    let mut voices = [None, None];
    // One quiet, centred tick even when several spread pellets land together.
    if event.player_hit && ready(limits.hit, now, HIT_INTERVAL) {
        limits.hit = Some(now);
        voices[0] = Some(Voice {
            sound: Sound::Hit,
            gain: HIT_GAIN,
            pan: 0.0,
        });
    }
    let distance = (event.x - listener.x).hypot(event.z - listener.z);
    if distance > AUDIBLE_DISTANCE {
        return voices;
    }
    let kind = event.kind.as_str();
    let shot = kind == "shot";
    let explosion = kind == "explosion" || kind == "death";
    let laser = kind == "laser";
    let destruction = destruction_sound(kind, event.cover_kind.as_deref());
    if (shot && !event.own && !ready(limits.shot, now, BOT_SHOT_INTERVAL))
        || (explosion && !ready(limits.explosion, now, EXPLOSION_INTERVAL))
        || (destruction.is_some() && !ready(limits.destruction, now, DESTRUCTION_INTERVAL))
        || (laser && !ready(limits.laser, now, LASER_INTERVAL))
    {
        return voices;
    }
    let sound = if destruction.is_some() {
        destruction
    } else if shot {
        shot_sound(event.weapon.as_deref())
    } else if explosion {
        Some(Sound::Explosion)
    } else {
        match kind {
            "pickup" => Some(Sound::Pickup),
            "promotion" if event.own => Some(Sound::Promotion),
            "laser" => Some(Sound::Laser),
            "impact" | "ricochet" => Some(Sound::Impact),
            _ => None,
        }
    };
    let Some(sound) = sound else {
        return voices;
    };
    if shot {
        limits.shot = Some(now);
    }
    if explosion {
        limits.explosion = Some(now);
    }
    if laser {
        limits.laser = Some(now);
    }
    if destruction.is_some() {
        limits.destruction = Some(now);
    }
    let kind_gain = if explosion {
        0.45
    } else if destruction.is_some() {
        0.24
    } else if shot {
        0.25
    } else if laser {
        0.12
    } else {
        0.18
    };
    voices[1] = Some(Voice {
        sound,
        gain: kind_gain * distance_gain(distance),
        pan: stereo_pan(
            Point {
                x: event.x,
                z: event.z,
            },
            listener,
            right,
        ),
    });
    voices
}

pub struct Audio {
    manager: AudioManager<DefaultBackend>,
    /// Decoded once, by `Sound::index`; `None` where a file failed to decode.
    sounds: Vec<Option<StaticSoundData>>,
    /// Each sound's voices still playing, oldest first.
    voices: Vec<Vec<StaticSoundHandle>>,
    limits: RateLimits,
}

impl Audio {
    /// The output device, or `None` where the machine has none.
    pub fn new() -> Option<Self> {
        let settings = AudioManagerSettings {
            main_track_builder: MainTrackBuilder::new().sound_capacity(SOUND_CAPACITY),
            ..Default::default()
        };
        let manager = AudioManager::<DefaultBackend>::new(settings)
            .map_err(|error| eprintln!("No sound: {error}"))
            .ok()?;
        Some(Self {
            manager,
            sounds: Sound::ALL.iter().map(|&sound| decode(sound)).collect(),
            voices: Sound::ALL.iter().map(|_| Vec::new()).collect(),
            limits: RateLimits::default(),
        })
    }

    /// Master volume, 0-1.
    pub fn set_volume(&mut self, volume: f64) {
        self.manager
            .main_track()
            .set_volume(decibels(volume.clamp(0.0, 1.0)), Tween::default());
    }

    /// Play an event batch's sounds from the human's position.
    pub fn play(&mut self, batch: &EventBatch) {
        let now = Instant::now();
        for event in &batch.events {
            let voices = plan_event(
                event,
                batch.listener,
                batch.listener_right,
                now,
                &mut self.limits,
            );
            for voice in voices.into_iter().flatten() {
                self.start(voice);
            }
        }
    }

    fn start(&mut self, voice: Voice) {
        let Some(data) = &self.sounds[voice.sound.index()] else {
            return;
        };
        let data = data
            .volume(decibels(voice.gain))
            .panning(Panning(voice.pan as f32));
        let playing = &mut self.voices[voice.sound.index()];
        playing.retain(|handle| handle.state() != PlaybackState::Stopped);
        while playing.len() >= voice.sound.pool() {
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

fn decode(sound: Sound) -> Option<StaticSoundData> {
    let path = format!("audio/{}.mp3", sound.name());
    let bytes = crate::assets::file(&path)?;
    StaticSoundData::from_cursor(Cursor::new(bytes))
        .map_err(|error| eprintln!("Cannot decode {path}: {error}"))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: Point = Point { x: 0.0, z: 0.0 };
    const RIGHT: Point = Point { x: 1.0, z: 0.0 };

    fn event(kind: &str, x: f64, z: f64) -> EngineEvent {
        EngineEvent {
            kind: kind.into(),
            x,
            z,
            id: None,
            owner: None,
            team: None,
            weapon: None,
            label: None,
            cover_kind: None,
            damage_source: None,
            player_hit: false,
            own: false,
            damage_angle: None,
        }
    }

    fn plan(event: &EngineEvent, now: Instant, limits: &mut RateLimits) -> Option<Voice> {
        plan_event(event, ORIGIN, RIGHT, now, limits)[1]
    }

    #[test]
    fn every_sound_is_embedded_and_decodes() {
        for sound in Sound::ALL {
            assert!(decode(sound).is_some(), "{}", sound.name());
            assert_eq!(Sound::ALL[sound.index()], sound);
        }
    }

    #[test]
    fn gain_fades_with_distance_to_a_floor() {
        assert_eq!(distance_gain(0.0), 1.0);
        assert!((distance_gain(20.0) - 0.5).abs() < 1e-12);
        assert_eq!(distance_gain(39.0), MIN_DISTANCE_GAIN);
        assert_eq!(decibels(1.0), Decibels(0.0));
        assert!((decibels(0.5).0 + 6.0206).abs() < 1e-3);
        assert_eq!(decibels(0.0), Decibels::SILENCE);
    }

    #[test]
    fn pan_follows_screen_right_and_clamps() {
        assert!((stereo_pan(Point { x: 12.5, z: 3.0 }, ORIGIN, RIGHT) - 0.5).abs() < 1e-12);
        assert_eq!(stereo_pan(Point { x: -30.0, z: 0.0 }, ORIGIN, RIGHT), -1.0);
        // First person turned a quarter: screen right is world -Z.
        let turned = Point { x: 0.0, z: -1.0 };
        assert_eq!(stereo_pan(Point { x: 0.0, z: 50.0 }, ORIGIN, turned), -1.0);
    }

    #[test]
    fn destruction_breaks_wood_or_rubble_and_drums_stay_silent() {
        assert_eq!(destruction_sound("shatter", None), Some(Sound::WoodBreak));
        for wood in ["tree", "timber", "cargo"] {
            assert_eq!(
                destruction_sound("destroy", Some(wood)),
                Some(Sound::WoodBreak)
            );
        }
        assert_eq!(
            destruction_sound("destroy", Some("house")),
            Some(Sound::RubbleBreak)
        );
        assert_eq!(destruction_sound("destroy", None), Some(Sound::RubbleBreak));
        assert_eq!(destruction_sound("destroy", Some("drum")), None);
        assert_eq!(destruction_sound("impact", Some("tree")), None);
    }

    #[test]
    fn events_choose_their_sound_and_kind_gain() {
        let now = Instant::now();
        let mut shot = event("shot", 0.0, 0.0);
        shot.weapon = Some("tow".into());
        let voice = plan(&shot, now, &mut RateLimits::default()).unwrap();
        assert_eq!((voice.sound, voice.gain), (Sound::ShotRocket, 0.25));
        let voice = plan(&event("death", 0.0, 0.0), now, &mut RateLimits::default()).unwrap();
        assert_eq!((voice.sound, voice.gain), (Sound::Explosion, 0.45));
        let voice = plan(
            &event("ricochet", 0.0, 0.0),
            now,
            &mut RateLimits::default(),
        )
        .unwrap();
        assert_eq!((voice.sound, voice.gain), (Sound::Impact, 0.18));
        let voice = plan(&event("laser", 0.0, 0.0), now, &mut RateLimits::default()).unwrap();
        assert_eq!((voice.sound, voice.gain), (Sound::Laser, 0.12));
        let mut promotion = event("promotion", 0.0, 0.0);
        assert_eq!(plan(&promotion, now, &mut RateLimits::default()), None);
        promotion.own = true;
        assert_eq!(
            plan(&promotion, now, &mut RateLimits::default()).map(|voice| voice.sound),
            Some(Sound::Promotion)
        );
        assert_eq!(
            plan(&event("spawn", 0.0, 0.0), now, &mut RateLimits::default()),
            None
        );
    }

    #[test]
    fn far_events_are_silent_but_the_hit_tick_still_plays_centred() {
        let mut hit = event("impact", 39.0, 0.0);
        hit.player_hit = true;
        let voices = plan_event(
            &hit,
            ORIGIN,
            RIGHT,
            Instant::now(),
            &mut RateLimits::default(),
        );
        assert_eq!(
            voices,
            [
                Some(Voice {
                    sound: Sound::Hit,
                    gain: HIT_GAIN,
                    pan: 0.0
                }),
                None
            ]
        );
    }

    #[test]
    fn rate_limits_throttle_bots_but_not_the_players_shots() {
        let start = Instant::now();
        let mut limits = RateLimits::default();
        let bot = event("shot", 0.0, 0.0);
        assert!(plan(&bot, start, &mut limits).is_some());
        let soon = start + Duration::from_millis(20);
        assert!(plan(&bot, soon, &mut limits).is_none());
        let mut own = event("shot", 0.0, 0.0);
        own.own = true;
        assert!(plan(&own, soon, &mut limits).is_some());
        // The player's shot also restarts the bot interval.
        assert!(plan(&bot, start + Duration::from_millis(40), &mut limits).is_none());
        assert!(plan(&bot, start + Duration::from_millis(55), &mut limits).is_some());

        let blast = event("explosion", 0.0, 0.0);
        assert!(plan(&blast, start, &mut limits).is_some());
        assert!(plan(&blast, start + Duration::from_millis(69), &mut limits).is_none());
        assert!(plan(&blast, start + Duration::from_millis(70), &mut limits).is_some());

        let mut hit = event("impact", 0.0, 0.0);
        hit.player_hit = true;
        let mut limits = RateLimits::default();
        let ticks = [0, 40, 80, 100].map(|ms| {
            plan_event(
                &hit,
                ORIGIN,
                RIGHT,
                start + Duration::from_millis(ms),
                &mut limits,
            )[0]
            .is_some()
        });
        assert_eq!(ticks, [true, false, true, false]);
    }

    #[test]
    fn a_silent_event_does_not_spend_its_kinds_interval() {
        let start = Instant::now();
        let mut limits = RateLimits::default();
        let mut unknown = event("shot", 0.0, 0.0);
        unknown.weapon = Some("unknown".into());
        assert!(plan(&unknown, start, &mut limits).is_none());
        assert!(plan(&event("shot", 0.0, 0.0), start, &mut limits).is_some());
    }
}
