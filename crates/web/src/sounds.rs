//! Which sounds a drained event batch plays, how loud and from which side
//! (`src/game/audio.ts` before the shells only played them). Both shells start the
//! planned sounds as they come; the page's Howler and the desktop's kira keep only the
//! voices and the master volume.
//!
//! Rate limits keep a burst of bot shots, blasts or breaking cover from stacking into
//! noise. They run on the frame clock the shell passes to `frame`, whether or not the
//! shell is playing sound yet.

use serde::{Serialize, Serializer};
use sloppy_core::sim::{CoverKind, SimEventType, Vec2, Weapon};

use crate::events::PendingEvent;

/// Milliseconds a throttled kind waits after it last played.
const HIT_INTERVAL_MS: f64 = 80.0;
const BOT_SHOT_INTERVAL_MS: f64 = 35.0;
const EXPLOSION_INTERVAL_MS: f64 = 70.0;
const DESTRUCTION_INTERVAL_MS: f64 = 80.0;
const LASER_INTERVAL_MS: f64 = 50.0;
/// Events farther than this from the listener stay silent.
const AUDIBLE_DISTANCE: f64 = 38.0;
/// The distance at which the fade would reach zero; `MIN_DISTANCE_GAIN` holds it above.
const FADE_DISTANCE: f64 = 40.0;
const MIN_DISTANCE_GAIN: f64 = 0.05;
/// Sideways distance at which a sound is fully in one speaker.
const STEREO_DISTANCE: f64 = 25.0;
/// The player's own hit tick: quiet and centred.
const HIT_GAIN: f64 = 0.14;
/// Each kind's gain at the listener, before the distance fade.
const EXPLOSION_GAIN: f64 = 0.45;
const DESTRUCTION_GAIN: f64 = 0.24;
const SHOT_GAIN: f64 = 0.25;
const LASER_GAIN: f64 = 0.12;
const OTHER_GAIN: f64 = 0.18;

/// The 13 battle sounds, each one file `audio/<name>.mp3` of the shells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
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
    pub const ALL: [Sound; 13] = [
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

    /// The file name, which is also how the page's JSON names the sound.
    pub const fn name(self) -> &'static str {
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
}

impl Serialize for Sound {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

/// One sound to start: which, how loud (linear, before the master volume) and where
/// (-1 left to 1 right).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct PlannedSound {
    pub sound: Sound,
    pub gain: f64,
    pub pan: f64,
}

/// The frame clock and when each throttled kind last played, in milliseconds (`None`
/// before it first plays).
#[derive(Clone, Debug, Default)]
pub struct SoundPlanner {
    now: f64,
    hit: Option<f64>,
    shot: Option<f64>,
    explosion: Option<f64>,
    destruction: Option<f64>,
    laser: Option<f64>,
}

impl SoundPlanner {
    /// The `now` the shell passed to `frame`: the next drain hears its events then.
    pub fn set_clock(&mut self, now: f64) {
        self.now = now;
    }

    /// The sounds `events` play, in order, heard from `listener` whose screen right is
    /// `right` (first person turns it with the view).
    pub fn plan(
        &mut self,
        events: &[PendingEvent],
        listener: Vec2,
        right: Vec2,
    ) -> Vec<PlannedSound> {
        let mut sounds = Vec::new();
        for event in events {
            self.event(event, listener, right, &mut sounds);
        }
        sounds
    }

    /// The player's hit tick first, then the event's own sound if it is near enough and
    /// its kind has not played too recently.
    fn event(
        &mut self,
        pending: &PendingEvent,
        listener: Vec2,
        right: Vec2,
        sounds: &mut Vec<PlannedSound>,
    ) {
        let now = self.now;
        let event = &pending.event;
        // One quiet, centred tick even when several spread pellets land together.
        if pending.player_hit && ready(self.hit, now, HIT_INTERVAL_MS) {
            self.hit = Some(now);
            sounds.push(PlannedSound {
                sound: Sound::Hit,
                gain: HIT_GAIN,
                pan: 0.0,
            });
        }
        let distance = (event.x - listener.x).hypot(event.z - listener.z);
        if distance > AUDIBLE_DISTANCE {
            return;
        }
        // The sound, its kind's gain, and for a throttled kind when it last played and
        // how long it waits. A silent event (a drum's shell, another tank's promotion)
        // spends no interval.
        let (sound, kind_gain, throttle) = match event.kind {
            SimEventType::Shot => (
                shot_sound(event.weapon),
                SHOT_GAIN,
                Some((&mut self.shot, BOT_SHOT_INTERVAL_MS)),
            ),
            SimEventType::Explosion | SimEventType::Death => (
                Sound::Explosion,
                EXPLOSION_GAIN,
                Some((&mut self.explosion, EXPLOSION_INTERVAL_MS)),
            ),
            SimEventType::Laser => (
                Sound::Laser,
                LASER_GAIN,
                Some((&mut self.laser, LASER_INTERVAL_MS)),
            ),
            SimEventType::Shatter | SimEventType::Destroy => {
                let Some(sound) = destruction_sound(event.kind, event.cover_kind) else {
                    return;
                };
                let throttle = (&mut self.destruction, DESTRUCTION_INTERVAL_MS);
                (sound, DESTRUCTION_GAIN, Some(throttle))
            }
            SimEventType::Pickup => (Sound::Pickup, OTHER_GAIN, None),
            SimEventType::Promotion if pending.own => (Sound::Promotion, OTHER_GAIN, None),
            SimEventType::Impact | SimEventType::Ricochet => (Sound::Impact, OTHER_GAIN, None),
            _ => return,
        };
        if let Some((last, interval)) = throttle {
            // The player's own shots always play, and restart the bots' interval.
            let own_shot = pending.own && event.kind == SimEventType::Shot;
            if !own_shot && !ready(*last, now, interval) {
                return;
            }
            *last = Some(now);
        }
        sounds.push(PlannedSound {
            sound,
            gain: kind_gain * distance_gain(distance),
            pan: stereo_pan(Vec2::new(event.x, event.z), listener, right),
        });
    }
}

/// Whether a kind that last played at `last` may play again at `now`.
fn ready(last: Option<f64>, now: f64, interval: f64) -> bool {
    last.is_none_or(|last| now - last >= interval)
}

/// A weapon's firing sound; a shot that names none fired a standard shell.
fn shot_sound(weapon: Option<Weapon>) -> Sound {
    match weapon.unwrap_or_default() {
        Weapon::Standard => Sound::Shot,
        Weapon::Spread => Sound::ShotSpread,
        Weapon::Rocket | Weapon::Tow => Sound::ShotRocket,
        Weapon::Ricochet => Sound::ShotRicochet,
        Weapon::Piercing => Sound::ShotPiercing,
    }
}

/// Drums emit a separate blast event, so their shell plays nothing of its own. A shell
/// bursting a loose crate or watchtower piece (`shatter`) breaks wood.
fn destruction_sound(kind: SimEventType, cover_kind: Option<CoverKind>) -> Option<Sound> {
    match (kind, cover_kind) {
        (SimEventType::Shatter, _) => Some(Sound::WoodBreak),
        (SimEventType::Destroy, Some(CoverKind::Drum)) => None,
        (SimEventType::Destroy, Some(CoverKind::Tree | CoverKind::Timber | CoverKind::Cargo)) => {
            Some(Sound::WoodBreak)
        }
        (SimEventType::Destroy, _) => Some(Sound::RubbleBreak),
        _ => None,
    }
}

/// Distance fade, never quite silent within the audible range.
fn distance_gain(distance: f64) -> f64 {
    (1.0 - distance / FADE_DISTANCE).max(MIN_DISTANCE_GAIN)
}

/// Stereo position from how far the event lies along screen right.
fn stereo_pan(event: Vec2, listener: Vec2, right: Vec2) -> f64 {
    let across = (event.x - listener.x) * right.x + (event.z - listener.z) * right.z;
    (across / STEREO_DISTANCE).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sloppy_core::sim::SimEvent;

    const ORIGIN: Vec2 = Vec2 { x: 0.0, z: 0.0 };
    const RIGHT: Vec2 = Vec2 { x: 1.0, z: 0.0 };
    const SILENT: [Sound; 0] = [];

    fn event(kind: SimEventType, x: f64, z: f64) -> PendingEvent {
        PendingEvent {
            event: SimEvent::at(kind, x, z),
            player_hit: false,
            own: false,
            damage_angle: None,
            feedback: Default::default(),
        }
    }

    fn destroy(cover_kind: CoverKind) -> PendingEvent {
        let mut destroy = event(SimEventType::Destroy, 0.0, 0.0);
        destroy.event.cover_kind = Some(cover_kind);
        destroy
    }

    /// The sounds `events` play at `now` (ms) from the origin, facing north.
    fn plan_at(planner: &mut SoundPlanner, now: f64, events: &[PendingEvent]) -> Vec<Sound> {
        planner.set_clock(now);
        planner
            .plan(events, ORIGIN, RIGHT)
            .iter()
            .map(|planned| planned.sound)
            .collect()
    }

    /// The one sound `event` plays on a fresh planner.
    fn only(event: &PendingEvent) -> Option<PlannedSound> {
        let sounds = SoundPlanner::default().plan(std::slice::from_ref(event), ORIGIN, RIGHT);
        assert!(sounds.len() <= 1, "{sounds:?}");
        sounds.first().copied()
    }

    #[test]
    fn sounds_serialize_as_their_file_names() {
        assert_eq!(
            serde_json::to_value(PlannedSound {
                sound: Sound::ShotRocket,
                gain: 0.25,
                pan: -0.5,
            })
            .unwrap(),
            json!({"sound": "shot-rocket", "gain": 0.25, "pan": -0.5})
        );
        for sound in Sound::ALL {
            assert_eq!(serde_json::to_value(sound).unwrap(), json!(sound.name()));
        }
    }

    #[test]
    fn gain_fades_with_distance_to_a_floor() {
        assert_eq!(distance_gain(0.0), 1.0);
        assert!((distance_gain(20.0) - 0.5).abs() < 1e-12);
        assert_eq!(distance_gain(39.0), MIN_DISTANCE_GAIN);
        let blast = only(&event(SimEventType::Explosion, 0.0, 20.0)).unwrap();
        assert!((blast.gain - EXPLOSION_GAIN * 0.5).abs() < 1e-12);
    }

    #[test]
    fn stereo_pans_along_the_listeners_right_which_first_person_turns() {
        assert!((stereo_pan(Vec2::new(12.5, 3.0), ORIGIN, RIGHT) - 0.5).abs() < 1e-12);
        assert_eq!(stereo_pan(Vec2::new(-30.0, 0.0), ORIGIN, RIGHT), -1.0);
        // First person turned a quarter: screen right is world -Z.
        let turned = Vec2::new(0.0, -1.0);
        assert_eq!(stereo_pan(Vec2::new(0.0, 50.0), ORIGIN, turned), -1.0);
        // An explosion 30 m along +Z: centred facing north, fully right facing +X.
        let blast = [event(SimEventType::Explosion, 0.0, 30.0)];
        let pans = [RIGHT, Vec2::new(0.0, 1.0)]
            .map(|right| SoundPlanner::default().plan(&blast, ORIGIN, right)[0].pan);
        assert_eq!(pans, [0.0, 1.0]);
    }

    #[test]
    fn cover_destruction_uses_material_sounds_while_drums_defer_to_their_blast() {
        for wood in [CoverKind::Tree, CoverKind::Timber, CoverKind::Cargo] {
            assert_eq!(
                destruction_sound(SimEventType::Destroy, Some(wood)),
                Some(Sound::WoodBreak)
            );
        }
        for rubble in [
            CoverKind::Tower,
            CoverKind::Rock,
            CoverKind::Concrete,
            CoverKind::House,
        ] {
            assert_eq!(
                destruction_sound(SimEventType::Destroy, Some(rubble)),
                Some(Sound::RubbleBreak)
            );
        }
        assert_eq!(
            destruction_sound(SimEventType::Destroy, None),
            Some(Sound::RubbleBreak)
        );
        assert_eq!(
            destruction_sound(SimEventType::Destroy, Some(CoverKind::Drum)),
            None
        );
        assert_eq!(destruction_sound(SimEventType::Death, None), None);
        assert_eq!(
            destruction_sound(SimEventType::Impact, Some(CoverKind::Tree)),
            None
        );
        // A shell bursting a loose crate or watchtower board.
        for kind in [CoverKind::Cargo, CoverKind::Tower] {
            assert_eq!(
                destruction_sound(SimEventType::Shatter, Some(kind)),
                Some(Sound::WoodBreak)
            );
        }
    }

    #[test]
    fn events_choose_their_sound_and_kind_gain() {
        let mut shot = event(SimEventType::Shot, 0.0, 0.0);
        let voice = only(&shot).unwrap();
        assert_eq!((voice.sound, voice.gain), (Sound::Shot, SHOT_GAIN));
        for (weapon, sound) in [
            (Weapon::Standard, Sound::Shot),
            (Weapon::Spread, Sound::ShotSpread),
            (Weapon::Rocket, Sound::ShotRocket),
            (Weapon::Tow, Sound::ShotRocket),
            (Weapon::Ricochet, Sound::ShotRicochet),
            (Weapon::Piercing, Sound::ShotPiercing),
        ] {
            shot.event.weapon = Some(weapon);
            assert_eq!(only(&shot).map(|voice| voice.sound), Some(sound));
        }
        let voice = only(&event(SimEventType::Death, 0.0, 0.0)).unwrap();
        assert_eq!(
            (voice.sound, voice.gain),
            (Sound::Explosion, EXPLOSION_GAIN)
        );
        let voice = only(&destroy(CoverKind::House)).unwrap();
        assert_eq!(
            (voice.sound, voice.gain),
            (Sound::RubbleBreak, DESTRUCTION_GAIN)
        );
        let voice = only(&event(SimEventType::Ricochet, 0.0, 0.0)).unwrap();
        assert_eq!((voice.sound, voice.gain), (Sound::Impact, OTHER_GAIN));
        let voice = only(&event(SimEventType::Pickup, 0.0, 0.0)).unwrap();
        assert_eq!((voice.sound, voice.gain), (Sound::Pickup, OTHER_GAIN));
        let voice = only(&event(SimEventType::Laser, 0.0, 0.0)).unwrap();
        assert_eq!((voice.sound, voice.gain), (Sound::Laser, LASER_GAIN));
        // Only the viewer's own promotion chimes.
        let mut promotion = event(SimEventType::Promotion, 0.0, 0.0);
        assert_eq!(only(&promotion), None);
        promotion.own = true;
        assert_eq!(
            only(&promotion).map(|voice| voice.sound),
            Some(Sound::Promotion)
        );
        for silent in [
            SimEventType::Respawn,
            SimEventType::Hurt,
            SimEventType::Notice,
            SimEventType::DebrisImpact,
        ] {
            assert_eq!(only(&event(silent, 0.0, 0.0)), None, "{silent:?}");
        }
    }

    #[test]
    fn far_events_are_silent_but_the_hit_tick_still_plays_centred() {
        let mut hit = event(SimEventType::Impact, 39.0, 0.0);
        hit.player_hit = true;
        let sounds = SoundPlanner::default().plan(&[hit], ORIGIN, RIGHT);
        assert_eq!(
            sounds,
            [PlannedSound {
                sound: Sound::Hit,
                gain: HIT_GAIN,
                pan: 0.0
            }]
        );
        assert!(only(&event(SimEventType::Impact, 0.0, 38.0)).is_some());
    }

    #[test]
    fn rate_limits_throttle_bots_but_not_the_players_shots() {
        let mut planner = SoundPlanner::default();
        let bot = event(SimEventType::Shot, 0.0, 0.0);
        let mut own = event(SimEventType::Shot, 0.0, 0.0);
        own.own = true;
        assert_eq!(
            plan_at(&mut planner, 1000.0, std::slice::from_ref(&bot)),
            [Sound::Shot]
        );
        assert_eq!(
            plan_at(&mut planner, 1020.0, std::slice::from_ref(&bot)),
            SILENT
        );
        assert_eq!(
            plan_at(&mut planner, 1020.0, std::slice::from_ref(&own)),
            [Sound::Shot]
        );
        // The player's shot also restarts the bot interval.
        assert_eq!(
            plan_at(&mut planner, 1040.0, std::slice::from_ref(&bot)),
            SILENT
        );
        assert_eq!(
            plan_at(&mut planner, 1055.0, std::slice::from_ref(&bot)),
            [Sound::Shot]
        );

        let blast = [event(SimEventType::Explosion, 0.0, 0.0)];
        assert_eq!(plan_at(&mut planner, 1000.0, &blast), [Sound::Explosion]);
        assert_eq!(plan_at(&mut planner, 1069.0, &blast), SILENT);
        assert_eq!(plan_at(&mut planner, 1070.0, &blast), [Sound::Explosion]);

        // Several pellets landing together tick once.
        let mut hit = event(SimEventType::Hurt, 0.0, 0.0);
        hit.player_hit = true;
        let pellets = [hit.clone(), hit.clone(), hit];
        let mut planner = SoundPlanner::default();
        let ticks = [0.0, 40.0, 80.0, 100.0].map(|now| plan_at(&mut planner, now, &pellets));
        assert_eq!(ticks, [vec![Sound::Hit], vec![], vec![Sound::Hit], vec![]]);
    }

    #[test]
    fn scenery_throttling_does_not_swallow_a_vehicle_explosion() {
        let mut planner = SoundPlanner::default();
        let tree_then_death = [
            destroy(CoverKind::Tree),
            event(SimEventType::Death, 0.0, 0.0),
        ];
        assert_eq!(
            plan_at(&mut planner, 0.0, &tree_then_death),
            [Sound::WoodBreak, Sound::Explosion]
        );
        // A drum's shell is silent and spends no destruction interval; its blast plays.
        let mut planner = SoundPlanner::default();
        let mut drum_blast = event(SimEventType::Explosion, 0.0, 0.0);
        drum_blast.event.cover_kind = Some(CoverKind::Drum);
        assert_eq!(
            plan_at(
                &mut planner,
                0.0,
                &[
                    destroy(CoverKind::Drum),
                    drum_blast,
                    destroy(CoverKind::House)
                ]
            ),
            [Sound::Explosion, Sound::RubbleBreak]
        );
    }

    #[test]
    fn limits_hold_across_a_batch_heard_at_one_frame() {
        let mut planner = SoundPlanner::default();
        let mut events = vec![
            event(SimEventType::Shot, 0.0, 0.0),
            event(SimEventType::Shot, 1.0, 0.0),
            event(SimEventType::Laser, 0.0, 0.0),
            event(SimEventType::Laser, 0.0, 1.0),
            destroy(CoverKind::Rock),
            event(SimEventType::Shatter, 0.0, 0.0),
            event(SimEventType::Impact, 0.0, 0.0),
            event(SimEventType::Impact, 0.0, 0.0),
        ];
        events[1].own = true;
        assert_eq!(
            plan_at(&mut planner, 5.0, &events),
            [
                Sound::Shot,
                Sound::Shot,
                Sound::Laser,
                Sound::RubbleBreak,
                Sound::Impact,
                Sound::Impact
            ]
        );
        // The next frame, 17 ms on, still holds lasers and cover; shots wait 35 ms.
        assert_eq!(plan_at(&mut planner, 22.0, &events[..6]), [Sound::Shot]);
        assert_eq!(
            plan_at(&mut planner, 60.0, &events[..6]),
            [Sound::Shot, Sound::Shot, Sound::Laser]
        );
    }
}
