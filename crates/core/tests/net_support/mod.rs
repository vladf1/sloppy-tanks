//! A scripted room for the multiplayer tests: named connections, an in-memory clock, and
//! every message the host sends, parsed. Include with `mod net_support;`.
#![allow(dead_code)]

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};
use sloppy_core::net::match_host::{HostEvent, MatchHost, MatchHostOptions};
use sloppy_core::net::protocol::{CONTENT_VERSION, PROTOCOL_VERSION};
use sloppy_core::sim::Simulation;
use sloppy_core::sim::physics::vector;

pub struct Harness {
    pub host: MatchHost,
    pub now: u64,
    /// Parsed messages per connection name, in arrival order.
    pub messages: BTreeMap<String, Vec<Value>>,
    /// Raw texts per connection name.
    pub texts: BTreeMap<String, Vec<String>>,
    /// Connection names the host closed, with each close.
    pub closed: Vec<String>,
    pub close_codes: Vec<(String, u16, String)>,
    names: Vec<String>,
}

/// `createdMs` backdates the room so lifetime rules apply without simulating hours; the
/// test clock starts at zero either way.
pub fn harness_at(created_ms: i64, epoch: &str, seed: u32) -> Harness {
    // The host clock is unsigned, so shift everything by an offset that keeps a backdated
    // creation time non-negative.
    let offset = if created_ms < 0 {
        (-created_ms) as u64
    } else {
        0
    };
    let mut token = 0;
    let host = MatchHost::new(MatchHostOptions {
        room_epoch: epoch.to_string(),
        now_ms: (created_ms + offset as i64) as u64,
        token: Box::new(move || {
            token += 1;
            format!("credential-{token:020}")
        }),
        seed: Some(seed),
        content_version: None,
    });
    Harness {
        host,
        now: offset,
        messages: BTreeMap::new(),
        texts: BTreeMap::new(),
        closed: Vec::new(),
        close_codes: Vec::new(),
        names: Vec::new(),
    }
}

pub fn harness() -> Harness {
    harness_at(0, "test-room", 4242)
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

impl Harness {
    pub fn id(&mut self, name: &str) -> u64 {
        match self.names.iter().position(|known| known == name) {
            Some(index) => index as u64 + 1,
            None => {
                self.names.push(name.to_string());
                self.names.len() as u64
            }
        }
    }

    pub fn name(&self, id: u64) -> String {
        self.names[id as usize - 1].clone()
    }

    pub fn drain(&mut self) {
        for event in self.host.take_events() {
            match event {
                HostEvent::Send { connection, text } => {
                    let name = self.name(connection);
                    let value: Value = serde_json::from_str(&text).expect("host sends JSON");
                    self.messages.entry(name.clone()).or_default().push(value);
                    self.texts.entry(name).or_default().push(text);
                }
                HostEvent::Close {
                    connection,
                    code,
                    reason,
                } => {
                    let name = self.name(connection);
                    self.closed.push(name.clone());
                    self.close_codes.push((name, code, reason));
                }
                HostEvent::Changed => {}
            }
        }
    }

    pub fn send_text(&mut self, name: &str, text: &str) {
        let id = self.id(name);
        self.host.receive(id, text, self.now);
        self.drain();
    }

    pub fn send(&mut self, name: &str, message: Value) {
        self.send_text(name, &message.to_string());
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
    pub fn advance_by(&mut self, ms: u64) {
        self.now += ms;
        let names: Vec<String> = self.messages.keys().cloned().collect();
        for name in names {
            let t = self.now;
            let tick = self.host.tick();
            self.action(&name, "ping", json!({ "t": t, "observedTick": tick }));
        }
        self.host.advance(self.now);
        self.drain();
    }

    pub fn advance(&mut self) {
        self.advance_by(50);
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

    pub fn player_id(&self, name: &str) -> String {
        self.latest(name, "welcome")["playerId"]
            .as_str()
            .unwrap()
            .to_string()
    }

    /// Index of the tank a player's seat drives.
    pub fn tank_of(&mut self, name: &str) -> usize {
        let player = self.player_id(name);
        let sim = self.sim();
        sim.tanks
            .iter()
            .position(|tank| tank.player_id.as_deref() == Some(player.as_str()))
            .unwrap_or_else(|| panic!("{name} has a tank"))
    }

    /// The indices of tanks, for `clear_arena`.
    pub fn all_tanks(&mut self) -> Vec<usize> {
        (0..self.sim().tanks.len()).collect()
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

pub fn object(value: &Value) -> &Map<String, Value> {
    value.as_object().expect("an object")
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
