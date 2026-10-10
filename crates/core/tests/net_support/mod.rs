//! A scripted room for the multiplayer tests: named connections, an in-memory clock, and
//! every message the host sends, parsed (binary state through each connection's
//! [`WireView`], as the former JSON); and the host's shell sweeps, to check what a client
//! draws against them. Include with `mod net_support;`.
#![allow(dead_code)]

use std::collections::BTreeMap;

use serde_json::{Value, json};
use sloppy_core::net::match_host::{HostEvent, MatchHost, MatchHostOptions};
use sloppy_core::net::player_controls::Ack;
use sloppy_core::net::protocol::{CONTENT_VERSION, FULL_MESSAGE, Message, PROTOCOL_VERSION};
use sloppy_core::net::replication::{
    Baseline, BinaryMessage, FrameExtras, SnapshotBatch, StateMirror, read_binary_message,
    write_snapshot_header,
};
use sloppy_core::net::shot_paths::PATH_TOLERANCE;
use sloppy_core::net::wire::put_varint;
use sloppy_core::net::wire_view::WireView;
use sloppy_core::sim::Simulation;
use sloppy_core::sim::data::STEP;
use sloppy_core::sim::math::Vec2;
use sloppy_core::sim::physics::vector;
use sloppy_core::sim::render_state::RenderShot;
use sloppy_core::sim::simulation::ProjectileMove;
use sloppy_core::sim::types::{AmmoInventory, Weapon};
use sloppy_core::sim::weapons::fire_weapon;

pub struct Harness {
    pub host: MatchHost,
    pub now: u64,
    /// Parsed messages per connection name, in arrival order.
    pub messages: BTreeMap<String, Vec<Value>>,
    /// Messages per connection name as sent.
    pub wire: BTreeMap<String, Vec<Message>>,
    views: BTreeMap<String, WireView>,
    /// Connection names the host closed, once per close.
    pub closed: Vec<String>,
    names: Vec<String>,
}

/// A host created at time 0 whose seat tokens count up from `credential-…001`.
pub fn test_host(room_epoch: &str, seed: u32, content_version: Option<&str>) -> MatchHost {
    let mut token = 0;
    MatchHost::new(MatchHostOptions {
        room_epoch: room_epoch.to_string(),
        now_ms: 0,
        token: Box::new(move || {
            token += 1;
            format!("credential-{token:020}")
        }),
        seed: Some(seed),
        content_version: content_version.map(str::to_string),
    })
}

/// A room created `age_ms` before the test clock starts, so lifetime rules apply without
/// simulating hours.
pub fn harness_aged(age_ms: u64) -> Harness {
    Harness {
        host: test_host("test-room", 4242, None),
        now: age_ms,
        messages: BTreeMap::new(),
        wire: BTreeMap::new(),
        views: BTreeMap::new(),
        closed: Vec::new(),
        names: Vec::new(),
    }
}

/// The wire rounds positions to millimetres and ticks to thousandths, which a 60 m/s
/// shell crosses in another millimetre: a drawn shell may be this much farther from its
/// simulated position than `PATH_TOLERANCE`.
pub const WIRE_SLACK: f64 = 0.003;
/// A height rounded to the wire's millimetres.
pub const HEIGHT_SLACK: f64 = 0.0005 + 1e-9;

/// Whether a drawn shell height is the simulated one, as the wire rounds it.
pub fn same_height(drawn: Option<f64>, simulated: Option<f64>) -> bool {
    match (drawn, simulated) {
        (Some(drawn), Some(simulated)) => (drawn - simulated).abs() <= HEIGHT_SLACK,
        (None, None) => true,
        _ => false,
    }
}

pub fn harness() -> Harness {
    harness_aged(0)
}

/// Merges `extra`'s fields over `base`.
pub fn merged(mut base: Value, extra: Value) -> Value {
    if let (Some(base), Value::Object(extra)) = (base.as_object_mut(), extra) {
        for (key, value) in extra {
            base.insert(key, value);
        }
    }
    base
}

/// The connection id of `name`, numbered from 1 in order of first use.
pub fn connection_id(names: &mut Vec<String>, name: &str) -> u64 {
    match names.iter().position(|known| known == name) {
        Some(index) => index as u64 + 1,
        None => {
            names.push(name.to_string());
            names.len() as u64
        }
    }
}

impl Harness {
    pub fn id(&mut self, name: &str) -> u64 {
        connection_id(&mut self.names, name)
    }

    pub fn name(&self, id: u64) -> String {
        self.names[id as usize - 1].clone()
    }

    pub fn drain(&mut self) {
        for event in self.host.take_events() {
            match event {
                HostEvent::Send {
                    connection,
                    message,
                } => {
                    let name = self.name(connection);
                    let value: Value = match &message {
                        Message::Text(text) => {
                            serde_json::from_str(text).expect("host sends JSON text")
                        }
                        Message::Binary(bytes) => self
                            .views
                            .entry(name.clone())
                            .or_default()
                            .binary(bytes)
                            .unwrap_or_else(|error| panic!("{name} cannot read state: {error}")),
                    };
                    self.messages.entry(name.clone()).or_default().push(value);
                    self.wire.entry(name).or_default().push(message);
                }
                HostEvent::Close { connection, .. } => self.closed.push(self.name(connection)),
                HostEvent::Changed => {}
            }
        }
    }

    pub fn send(&mut self, name: &str, message: Value) {
        let id = self.id(name);
        self.host.receive(id, &message.to_string(), self.now);
        self.drain();
    }

    pub fn join(&mut self, name: &str, extra: Value) {
        let base = json!({
            "type": "join",
            "version": PROTOCOL_VERSION,
            "contentVersion": CONTENT_VERSION,
            "name": name,
            "kind": "balanced",
        });
        self.send(name, merged(base, extra));
    }

    pub fn action(&mut self, name: &str, kind: &str, extra: Value) {
        let base = json!({ "type": kind, "roundId": self.host.round_id });
        self.send(name, merged(base, extra));
    }

    pub fn disconnect(&mut self, name: &str) {
        let id = self.id(name);
        self.host.disconnect(id, self.now);
        self.drain();
    }

    pub fn all(&self, name: &str) -> &[Value] {
        self.messages.get(name).map_or(&[], Vec::as_slice)
    }

    pub fn latest(&self, name: &str, kind: &str) -> Value {
        self.all(name)
            .iter()
            .rev()
            .find(|message| message["type"] == kind)
            .cloned()
            .unwrap_or_else(|| panic!("{name} received no {kind}"))
    }

    pub fn count(&self, name: &str, kind: &str) -> usize {
        self.all(name)
            .iter()
            .filter(|message| message["type"] == kind)
            .count()
    }

    /// Pings every connection that has heard from the host, then runs the 50 ms timer.
    pub fn advance(&mut self) {
        self.now += 50;
        let names: Vec<String> = self.messages.keys().cloned().collect();
        for name in names {
            let t = self.now;
            let tick = self.host.tick();
            self.action(&name, "ping", json!({ "t": t, "observedTick": tick }));
        }
        self.host.advance(self.now);
        self.drain();
    }

    /// Runs the timer without pings.
    pub fn tick_only(&mut self, ms: u64) {
        self.now += ms;
        self.host.advance(self.now);
        self.drain();
    }

    pub fn sim(&mut self) -> &mut Simulation {
        self.host.simulation.as_mut().expect("a round is built")
    }

    /// Index of the tank a player's seat drives.
    pub fn tank_of(&mut self, name: &str) -> usize {
        let player = self.latest(name, "welcome")["playerId"]
            .as_str()
            .unwrap()
            .to_string();
        let sim = self.sim();
        sim.tanks
            .iter()
            .position(|tank| tank.player_id.as_deref() == Some(player.as_str()))
            .unwrap_or_else(|| panic!("{name} has a tank"))
    }
}

pub fn set_translation(sim: &mut Simulation, index: usize, x: f64, y: f64, z: f64) {
    let body = sim.tanks[index].body;
    sim.world.bodies[body].set_translation(vector(x, y, z), true);
}

pub fn set_linvel(sim: &mut Simulation, index: usize, x: f64, y: f64, z: f64) {
    let body = sim.tanks[index].body;
    sim.world.bodies[body].set_linvel(vector(x, y, z), true);
}

/// JSON equality with numbers compared as doubles (`1` equals `1.0`), like `deepEqual` on
/// parsed JavaScript values.
pub fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same(x, y))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(key, value)| y.get(key).is_some_and(|other| same(value, other)))
        }
        _ => a == b,
    }
}

/// Asserts [`same`], printing the first differing path.
pub fn assert_same(a: &Value, b: &Value, context: &str) {
    fn first_difference(a: &Value, b: &Value, path: String) -> Option<String> {
        match (a, b) {
            (Value::Array(x), Value::Array(y)) if x.len() == y.len() => x
                .iter()
                .zip(y)
                .enumerate()
                .find_map(|(i, (x, y))| first_difference(x, y, format!("{path}[{i}]"))),
            (Value::Object(x), Value::Object(y)) => {
                for (key, value) in x {
                    match y.get(key) {
                        Some(other) => {
                            if let Some(found) =
                                first_difference(value, other, format!("{path}.{key}"))
                            {
                                return Some(found);
                            }
                        }
                        None => return Some(format!("{path}.{key}: {value} vs missing")),
                    }
                }
                y.keys()
                    .find(|key| !x.contains_key(*key))
                    .map(|key| format!("{path}.{key}: missing vs {}", y[key]))
            }
            _ if same(a, b) => None,
            _ => Some(format!("{path}: {a} vs {b}")),
        }
    }
    if let Some(difference) = first_difference(a, b, String::new()) {
        panic!("{context}: {difference}");
    }
}

impl Harness {
    /// The binary messages of one type (`FULL_MESSAGE` or `SNAPSHOT_MESSAGE`) a
    /// connection received, in order.
    pub fn binary(&self, name: &str, kind: u8) -> Vec<Vec<u8>> {
        self.wire
            .get(name)
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .filter_map(|message| match message {
                Message::Binary(bytes) if bytes.first() == Some(&kind) => Some(bytes.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn latest_binary(&self, name: &str, kind: u8) -> Vec<u8> {
        self.binary(name, kind)
            .pop()
            .unwrap_or_else(|| panic!("{name} received no binary message {kind}"))
    }

    /// A mirror started from the connection's latest baseline.
    pub fn mirror_from_latest_full(&self, name: &str) -> StateMirror {
        let bytes = self.latest_binary(name, FULL_MESSAGE);
        let baseline = baseline(&bytes);
        let mut mirror = StateMirror::default();
        mirror
            .apply_full(&baseline, baseline.room_epoch, baseline.round_id)
            .expect("valid baseline");
        mirror
    }
}

/// A binary baseline message, read.
pub fn baseline(bytes: &[u8]) -> Baseline<'_> {
    match read_binary_message(bytes).unwrap() {
        BinaryMessage::Full(baseline) => baseline,
        BinaryMessage::Snapshot(_) => panic!("not a baseline"),
    }
}

/// A binary snapshot batch message, read.
pub fn snapshot_batch(bytes: &[u8]) -> SnapshotBatch<'_> {
    match read_binary_message(bytes).unwrap() {
        BinaryMessage::Snapshot(batch) => batch,
        BinaryMessage::Full(_) => panic!("not a snapshot batch"),
    }
}

/// Applies the first frame of a binary snapshot batch, which the mirror may reject.
pub fn apply_one(mirror: &mut StateMirror, bytes: &[u8]) -> Option<FrameExtras> {
    mirror.apply_snapshot(&mut snapshot_batch(bytes))
}

/// Applies every frame of a binary snapshot batch, asserting each applies.
pub fn apply_batch(mirror: &mut StateMirror, bytes: &[u8]) -> Vec<FrameExtras> {
    let mut batch = snapshot_batch(bytes);
    (0..batch.count)
        .map(|_| mirror.apply_snapshot(&mut batch).expect("frame applies"))
        .collect()
}

/// The first frame sequence number of a binary snapshot batch.
pub fn first_seq(bytes: &[u8]) -> u64 {
    snapshot_batch(bytes).first_seq
}

/// The last frame sequence number of a binary snapshot batch.
pub fn last_seq(bytes: &[u8]) -> u64 {
    let batch = snapshot_batch(bytes);
    batch.first_seq + batch.count - 1
}

/// A batch message of frames the host would send: `(tick, body)` from
/// [`StateStream::snapshot`](sloppy_core::net::replication::StateStream::snapshot), the
/// first with sequence number `first_seq`.
pub fn batch(round_id: u64, ack: u64, first_seq: u64, frames: &[(u64, Vec<u8>)]) -> Vec<u8> {
    let last = frames.last().map_or(0, |(tick, _)| *tick);
    let mut out = Vec::new();
    let ack = Ack {
        input_seq: ack as i64,
        ..Ack::default()
    };
    write_snapshot_header(
        &mut out,
        round_id,
        last,
        &ack,
        first_seq,
        frames.len(),
        None,
    );
    for (tick, body) in frames {
        put_varint(&mut out, last - tick);
        out.extend_from_slice(body);
    }
    out
}

impl Harness {
    /// The scene a connection's view mirrors, as the former JSON.
    pub fn mirrored(&self, name: &str) -> Value {
        self.views[name]
            .mirror
            .state
            .as_ref()
            .unwrap_or_else(|| panic!("{name} holds no baseline"))
            .to_value()
    }
}

/// One sweep the host simulated: shell `id` flew straight from `from` at tick `start` to
/// `to` at tick `end` (fractional ticks), at combat height `y` and render height `visual_y`.
pub struct Sweep {
    pub id: u32,
    pub start: f64,
    pub end: f64,
    pub from: Vec2,
    pub to: Vec2,
    pub y: Option<f64>,
    pub visual_y: Option<f64>,
}

impl Sweep {
    /// The sweeps tick `tick` recorded in the simulation's `projectile_moves`.
    pub fn record(moves: &[ProjectileMove], tick: u64) -> impl Iterator<Item = Sweep> {
        moves.iter().map(move |sweep| {
            let shot = &sweep.shot;
            let start = tick as f64 - 1.0 + sweep.offset / STEP;
            Sweep {
                id: shot.id,
                start,
                end: start + sweep.seconds / STEP,
                from: Vec2::new(
                    shot.x - shot.vx * sweep.seconds,
                    shot.z - shot.vz * sweep.seconds,
                ),
                to: Vec2::new(shot.x, shot.z),
                y: shot.y,
                visual_y: shot.visual_y,
            }
        })
    }

    /// Where the host had the shell at `tick`, inside this sweep.
    pub fn at(&self, tick: f64) -> Vec2 {
        let span = self.end - self.start;
        let alpha = if span > 0.0 {
            ((tick - self.start) / span).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Vec2::new(
            self.from.x + (self.to.x - self.from.x) * alpha,
            self.from.z + (self.to.z - self.from.z) * alpha,
        )
    }
}

/// Sweeps of shell `id` that contain display tick `tick`, with slack for the wire's
/// thousandth-of-a-tick rounding.
pub fn sweeps_at(sweeps: &[Sweep], id: u32, tick: f64) -> impl Iterator<Item = &Sweep> {
    sweeps.iter().filter(move |sweep| {
        sweep.id == id && sweep.start - 1e-3 <= tick && tick <= sweep.end + 1e-3
    })
}

/// Fires tank `index`'s `weapon` at `aim` radians.
pub fn fire_from(sim: &mut Simulation, index: usize, weapon: Weapon, aim: f64) {
    let tank = &mut sim.tanks[index];
    tank.ammo = AmmoInventory {
        spread: 99.0,
        rocket: 99.0,
        ricochet: 99.0,
        piercing: 99.0,
    };
    tank.selected_ammo = weapon;
    tank.aim = aim;
    tank.cooldown = 0.0;
    fire_weapon(sim, index);
}

/// Checks every display read at least three ticks before `newest` against the host's
/// sweeps: each drawn shell was flying there and is within `PATH_TOLERANCE` of its
/// simulated position at its simulated combat and render heights, and from display tick
/// `drawn_from` on each flying shell is drawn. Returns how many drawn shells were compared
/// and the largest distance seen.
pub fn assert_drawn_where_simulated(
    sweeps: &[Sweep],
    reads: &[(f64, Vec<RenderShot>)],
    newest: f64,
    drawn_from: f64,
) -> (usize, f64) {
    let mut compared = 0;
    let mut worst: f64 = 0.0;
    for (tick, shots) in reads {
        let tick = *tick;
        if tick > newest - 3.0 {
            continue;
        }
        for shot in shots {
            let distance = sweeps_at(sweeps, shot.id, tick)
                .map(|sweep| {
                    assert!(
                        same_height(shot.y, sweep.y) && same_height(shot.visual_y, sweep.visual_y),
                        "shell {} is drawn at height {:?}/{:?}, flies at {:?}/{:?}",
                        shot.id,
                        shot.y,
                        shot.visual_y,
                        sweep.y,
                        sweep.visual_y
                    );
                    let at = sweep.at(tick);
                    (at.x - shot.x).hypot(at.z - shot.z)
                })
                .reduce(f64::min)
                .unwrap_or_else(|| {
                    panic!("shell {} is drawn at tick {tick} but not flying", shot.id)
                });
            assert!(
                distance <= PATH_TOLERANCE + WIRE_SLACK,
                "shell {} is drawn {distance} m from where the host flies it at tick {tick}",
                shot.id
            );
            worst = worst.max(distance);
            compared += 1;
        }
        if tick < drawn_from {
            continue;
        }
        // A shell flying well inside one sweep is drawn.
        for sweep in sweeps {
            if sweep.start + 0.01 < tick && tick < sweep.end - 0.01 {
                assert!(
                    shots.iter().any(|shot| shot.id == sweep.id),
                    "shell {} flies at tick {tick} but is not drawn",
                    sweep.id
                );
            }
        }
    }
    (compared, worst)
}
