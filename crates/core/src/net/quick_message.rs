//! The host's reader for its two frequent client messages, `input` and `ping`, which
//! arrive 20-60 times a second from every player: read straight from the text, borrowing
//! its strings, instead of through a [`serde_json::Value`] tree.
//!
//! It accepts only the plain form a client sends: an object whose keys are all ones these
//! messages use, each at most once, holding numbers, booleans, `null`, unescaped strings
//! and the `aim` and action objects. Anything else (an unknown or repeated key, an array
//! where an object belongs, a string with escapes, a value of another type, invalid JSON)
//! fails to deserialize, and the host reads that text through the general `Value` path
//! ([`read_input`](super::player_controls::read_input), [`schema`](super::schema)) as
//! before. What this reads therefore always agrees with that path: every field it reads
//! is one the `Value` path would read as the same number, and every message shape it
//! cannot match exactly is left to it.

use std::marker::PhantomData;

use serde::de::value::MapAccessDeserializer;
use serde::de::{Deserializer, MapAccess, Visitor};
use serde::Deserialize;

use super::player_controls::{
    Action, ControlInput, MAX_QUEUED_ACTIONS, aim_angle, aim_point, epoch, finite_number,
    player_ammo, requested_tick, safe_integer_number, view_ticks,
};

/// A JSON object only: a derived struct alone would also read an array positionally,
/// where the `Value` path expects an object.
struct Object<T>(T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Object<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
            type Value = Object<T>;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an object")
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                T::deserialize(MapAccessDeserializer::new(map)).map(Object)
            }
        }
        deserializer.deserialize_map(ObjectVisitor(PhantomData))
    }
}

/// A key of a nested object whose key count decides how the `Value` path reads it: it
/// may be missing, but `null` (which that count includes) is left to the `Value` path.
struct Present<T>(Option<T>);

impl<T> Default for Present<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<T> Present<T> {
    fn count(&self) -> usize {
        usize::from(self.0.is_some())
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Present<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(|value| Self(Some(value)))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AimFields {
    #[serde(default)]
    angle: Present<f64>,
    #[serde(default)]
    x: Present<f64>,
    #[serde(default)]
    z: Present<f64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionFields<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    #[serde(default, borrow)]
    weapon: Present<&'a str>,
}

/// Top-level keys read as `Option`: there a missing key and `null` read alike.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct MessageFields<'a> {
    #[serde(rename = "type", borrow)]
    kind: Option<&'a str>,
    round_id: Option<f64>,
    observed_tick: Option<f64>,
    t: Option<f64>,
    control_epoch: Option<f64>,
    seq: Option<f64>,
    tick: Option<f64>,
    move_x: Option<f64>,
    move_z: Option<f64>,
    aim: Option<Object<AimFields>>,
    fire: Option<bool>,
    view: Option<f64>,
    #[serde(borrow)]
    actions: Option<Vec<Object<ActionFields<'a>>>>,
}

/// An `input` or `ping` message in its plain form.
pub struct QuickMessage<'a> {
    fields: MessageFields<'a>,
}

impl<'a> QuickMessage<'a> {
    /// The message in `text` when it is a plain `input` or `ping`; `None` leaves it to
    /// the `Value` path.
    pub fn parse(text: &'a str) -> Option<Self> {
        let Object(fields) = serde_json::from_str::<Object<MessageFields>>(text).ok()?;
        matches!(fields.kind, Some("input" | "ping")).then_some(Self { fields })
    }

    pub fn is_ping(&self) -> bool {
        self.fields.kind == Some("ping")
    }

    /// `roundId` as a number.
    pub fn round_id(&self) -> Option<f64> {
        self.fields.round_id
    }

    /// `observedTick` as a number.
    pub fn observed_tick(&self) -> Option<f64> {
        self.fields.observed_tick
    }

    /// A ping's `t` as a number.
    pub fn t(&self) -> Option<f64> {
        self.fields.t
    }

    /// The input this message carries, exactly as
    /// [`read_input`](super::player_controls::read_input) reads the same text.
    pub fn input(&self) -> Option<ControlInput> {
        let fields = &self.fields;
        let Object(aim) = fields.aim.as_ref()?;
        let aim = match aim.angle.count() + aim.x.count() + aim.z.count() {
            1 => aim_angle(aim.angle.0)?,
            2 => aim_point(aim.x.0, aim.z.0)?,
            _ => return None,
        };
        let actions = match &fields.actions {
            None => Vec::new(),
            Some(items) if items.len() <= MAX_QUEUED_ACTIONS => items
                .iter()
                .map(|Object(action)| match (action.kind, action.weapon.0) {
                    ("mine", None) => Some(Action::Mine),
                    ("ammo", Some(weapon)) => player_ammo(weapon).map(Action::Ammo),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()?,
            Some(_) => return None,
        };
        let tick = match fields.tick {
            None => None,
            some => Some(requested_tick(some)?),
        };
        let view = match fields.view {
            None => 0,
            some => view_ticks(some)?,
        };
        Some(ControlInput {
            control_epoch: epoch(safe_integer_number(fields.control_epoch)?),
            seq: safe_integer_number(fields.seq)?,
            observed_tick: safe_integer_number(fields.observed_tick)?,
            tick,
            move_x: finite_number(fields.move_x, 1.0)?,
            move_z: finite_number(fields.move_z, 1.0)?,
            aim,
            fire: fields.fire.unwrap_or(false),
            actions,
            view,
        })
    }
}
