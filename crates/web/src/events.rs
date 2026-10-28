//! Audio and HUD events shared by the single-player and room page boundaries.

use serde::Serialize;
use sloppy_core::sim::{SimEvent, Vec2};

use crate::sounds::{PlannedSound, SoundPlanner};

/// A simulation cue with the viewer-relative feedback the page needs.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingEvent {
    #[serde(flatten)]
    pub event: SimEvent,
    /// The viewer's shot hurt or destroyed an enemy (the hit tick).
    pub player_hit: bool,
    /// The event happened to the viewer's tank.
    pub own: bool,
    /// The clockwise screen angle (radians) of damage the viewer took.
    pub damage_angle: Option<f64>,
}

/// The events waiting since the last drain, with where the viewer listens from.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventBatch {
    pub listener: Vec2,
    /// World X/Z of screen right, which pans the sounds.
    pub listener_right: Vec2,
    pub events: Vec<PendingEvent>,
    /// The sounds the events play from the listener, in the order to start them.
    pub sounds: Vec<PlannedSound>,
}

/// The waiting events as a batch heard from `listener`; the queue starts again empty.
pub fn take_events(
    events: &mut Vec<PendingEvent>,
    listener: (f64, f64),
    listener_right: (f64, f64),
    sounds: &mut SoundPlanner,
) -> EventBatch {
    let listener = Vec2::new(listener.0, listener.1);
    let listener_right = Vec2::new(listener_right.0, listener_right.1);
    let events = std::mem::take(events);
    EventBatch {
        sounds: sounds.plan(&events, listener, listener_right),
        listener,
        listener_right,
        events,
    }
}

/// Write directly to JSON without copying events into intermediate object maps. The
/// queue keeps its allocation, and the returned text owns everything the page reads.
pub fn drain_events(
    events: &mut Vec<PendingEvent>,
    listener: (f64, f64),
    listener_right: (f64, f64),
    sounds: &mut SoundPlanner,
) -> String {
    let batch = take_events(events, listener, listener_right, sounds);
    let text = serde_json::to_string(&batch).expect("page events serialize");
    *events = batch.events;
    events.clear();
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use sloppy_core::sim::SimEventType;

    /// The drained batch as the page parses it, heard by a fresh sound planner.
    fn drain(
        events: &mut Vec<PendingEvent>,
        listener: (f64, f64),
        listener_right: (f64, f64),
    ) -> Value {
        let mut sounds = SoundPlanner::default();
        let text = drain_events(events, listener, listener_right, &mut sounds);
        serde_json::from_str(&text).unwrap()
    }

    #[test]
    fn an_empty_batch_preserves_listener_coordinates_and_queue_capacity() {
        let mut events = Vec::with_capacity(8);
        let capacity = events.capacity();
        let value = drain(&mut events, (1.25, -2.5), (0.5, -0.75));
        assert_eq!(
            value,
            json!({
                "listener": {"x": 1.25, "z": -2.5},
                "listenerRight": {"x": 0.5, "z": -0.75},
                "events": [],
                "sounds": [],
            })
        );
        assert_eq!(events.capacity(), capacity);
    }

    #[test]
    fn optional_fields_and_escaped_text_keep_the_page_schema() {
        let event = json!({
            "type": "death", "x": 1.25, "z": -2.5,
            "id": 7, "owner": 2, "ownerLife": 3, "weapon": "rocket", "team": 1,
            "size": 1.5, "label": "Quoted \"hit\"\nbackslash\\tab\t火\u{0000}",
            "color": 16711680, "from": {"x": 3.0, "y": 1.0, "z": -4.0},
            "deathStyle": "burnout", "material": "wood", "force": 2.75,
            "damageSource": {"cause": "rocket", "origin": {"x": 5.0, "z": -6.0}},
            "coverKind": "timber", "height": 2.0,
        });
        let mut expected = event.clone();
        expected["playerHit"] = json!(true);
        expected["own"] = json!(false);
        expected["damageAngle"] = json!(-0.75);
        let mut events = vec![PendingEvent {
            event: serde_json::from_value(event).unwrap(),
            player_hit: true,
            own: false,
            damage_angle: Some(-0.75),
        }];
        let capacity = events.capacity();
        let value = drain(&mut events, (0.0, 0.0), (1.0, 0.0));
        // JSON objects are compared by parsed data; their key order is not a page API.
        assert_eq!(value["events"], json!([expected]));
        // The viewer's centred hit tick, then the blast 2.8 m away, a little to the right.
        let sounds = value["sounds"].as_array().unwrap();
        assert_eq!(sounds.len(), 2);
        assert_eq!(sounds[0], json!({"sound": "hit", "gain": 0.14, "pan": 0.0}));
        assert_eq!(
            (&sounds[1]["sound"], &sounds[1]["pan"]),
            (&json!("explosion"), &json!(0.05))
        );
        assert!(events.is_empty());
        assert_eq!(events.capacity(), capacity);
        let again = drain(&mut events, (0.0, 0.0), (1.0, 0.0));
        assert_eq!(again["events"], json!([]), "a cue is drained exactly once");
        assert_eq!(again["sounds"], json!([]));
    }

    #[test]
    fn all_event_kinds_keep_order_and_omit_absent_simulation_fields() {
        let kinds = [
            (SimEventType::DebrisImpact, "debris-impact"),
            (SimEventType::Notice, "notice"),
            (SimEventType::Shot, "shot"),
            (SimEventType::Impact, "impact"),
            (SimEventType::Explosion, "explosion"),
            (SimEventType::Destroy, "destroy"),
            (SimEventType::Death, "death"),
            (SimEventType::Pickup, "pickup"),
            (SimEventType::Respawn, "respawn"),
            (SimEventType::Hurt, "hurt"),
            (SimEventType::Ricochet, "ricochet"),
            (SimEventType::Laser, "laser"),
            (SimEventType::Promotion, "promotion"),
            (SimEventType::Shatter, "shatter"),
        ];
        let mut events: Vec<_> = kinds
            .iter()
            .map(|&(kind, _)| PendingEvent {
                event: SimEvent::at(kind, 0.0, -1.0),
                player_hit: false,
                own: true,
                damage_angle: None,
            })
            .collect();
        let value = drain(&mut events, (0.0, 0.0), (1.0, 0.0));
        let expected: Vec<_> = kinds
            .iter()
            .map(|&(_, kind)| {
                json!({
                    "type": kind, "x": 0.0, "z": -1.0,
                    "playerHit": false, "own": true, "damageAngle": null,
                })
            })
            .collect();
        assert_eq!(value["events"], json!(expected));
    }
}
