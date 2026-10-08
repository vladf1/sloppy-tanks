//! Wire compatibility with the TypeScript host: the same scripted room
//! (`fixtures/net-golden-script.json`) must produce the same message sequence on every
//! connection, identical lobby/control/welcome/pong/error/reset messages, and baselines
//! and snapshots with the same JSON paths and value types. `fixtures/net-golden.json` was
//! recorded from the TypeScript host (baseline `35afd91`) by `fixtures/net-golden.ts`, which
//! left with that engine (see Git history). Protocol 2 has since replaced the `shots`
//! records and frame `traces` with projectile `paths` (`shot_paths`), edited in by hand.
//! Protocol 3's binary state messages are compared through their JSON view, and their
//! fixed headers are pinned by `fixtures/net-golden-binary.json`, which the traffic bots'
//! header reader also reads; rerun with `SLOPPY_UPDATE_FIXTURES=1` after a deliberate
//! format change. Protocol 4's additions (each tank's `drive`, the snapshot's `ackTick`,
//! `ackArrival` and `hull`) were added to the JSON signatures by hand.

mod net_support;

use std::collections::{BTreeMap, BTreeSet};

use net_support::assert_same;
use serde_json::{Map, Value, json};
use sloppy_core::net::match_host::{HostEvent, MatchHost, MatchHostOptions};
use sloppy_core::net::protocol::{Message, PROTOCOL_VERSION, SNAPSHOT_MESSAGE};
use sloppy_core::net::replication::{BinaryMessage, read_binary_message};
use sloppy_core::net::wire_view::WireView;

const CONTENT: &str = "golden-content";
const EXACT: [&str; 6] = ["welcome", "lobby", "control", "pong", "error", "room-reset"];

#[derive(Default)]
struct Recording {
    /// Per connection: message types (closes as `close:<code>`) and exact messages.
    entries: BTreeMap<String, Vec<(String, Option<Value>)>>,
    full: BTreeSet<String>,
    snapshot: BTreeSet<String>,
    welcomed: Vec<String>,
    closed: BTreeSet<String>,
    latest: BTreeMap<(String, String), Value>,
}

fn signature(value: &Value, path: &str, out: &mut BTreeSet<String>) {
    let kind = match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    out.insert(format!("{path}:{kind}"));
    match value {
        Value::Array(items) => {
            for item in items {
                signature(item, &format!("{path}[]"), out);
            }
        }
        Value::Object(fields) => {
            let named = match fields.get("type") {
                Some(Value::String(kind)) if path.ends_with(".event") => format!("({kind})"),
                _ => String::new(),
            };
            for (key, item) in fields {
                let key = if !key.is_empty() && key.bytes().all(|b| b.is_ascii_digit()) {
                    "*"
                } else {
                    key
                };
                signature(item, &format!("{path}{named}.{key}"), out);
            }
        }
        _ => {}
    }
}

struct Runner {
    host: MatchHost,
    names: Vec<String>,
    now: u64,
    record: Recording,
    seqs: BTreeMap<String, u64>,
    /// Each connection's binary state as the former JSON.
    views: BTreeMap<String, WireView>,
    binary: Vec<Value>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The length of a binary state message's fixed header: the type byte and its varints
/// (`roundId tick ack firstSeq count` for a snapshot, `roundId tick seq eventCursor` and
/// the room epoch for a baseline). Frame contents follow the physics, which differs in
/// the last bits between platforms, so the fixture pins only the header.
fn header_len(bytes: &[u8]) -> usize {
    let mut at = 1;
    let varint = |at: &mut usize| {
        let mut value = 0u64;
        let mut shift = 0;
        loop {
            let byte = bytes[*at];
            *at += 1;
            value |= u64::from(byte & 0x7f) << shift;
            shift += 7;
            if byte & 0x80 == 0 {
                return value;
            }
        }
    };
    let snapshot = bytes[0] == SNAPSHOT_MESSAGE;
    for _ in 0..if snapshot { 5 } else { 4 } {
        varint(&mut at);
    }
    if !snapshot {
        at += varint(&mut at) as usize;
    }
    at
}

impl Runner {
    fn id(&mut self, name: &str) -> u64 {
        match self.names.iter().position(|known| known == name) {
            Some(index) => index as u64 + 1,
            None => {
                self.names.push(name.to_string());
                self.names.len() as u64
            }
        }
    }

    fn drain(&mut self) {
        for event in self.host.take_events() {
            match event {
                HostEvent::Send {
                    connection,
                    message,
                } => {
                    let name = self.names[connection as usize - 1].clone();
                    if let Message::Binary(bytes) = &message {
                        let header = match read_binary_message(bytes).unwrap() {
                            BinaryMessage::Full(full) => ("full", full.round_id, full.tick),
                            BinaryMessage::Snapshot(batch) => {
                                ("snapshot", batch.round_id, batch.tick)
                            }
                        };
                        self.binary.push(json!({
                            "connection": name,
                            "type": header.0,
                            "roundId": header.1,
                            "tick": header.2,
                            "header": hex(&bytes[..header_len(bytes)]),
                        }));
                    }
                    let message: Value = match &message {
                        Message::Text(text) => serde_json::from_str(text).unwrap(),
                        Message::Binary(bytes) => self
                            .views
                            .entry(name.clone())
                            .or_default()
                            .binary(bytes)
                            .unwrap(),
                    };
                    let kind = message["type"].as_str().unwrap().to_string();
                    let exact = EXACT.contains(&kind.as_str()).then(|| message.clone());
                    match kind.as_str() {
                        "full" => signature(&message, "full", &mut self.record.full),
                        "snapshot" => signature(&message, "snapshot", &mut self.record.snapshot),
                        "welcome" if !self.record.welcomed.contains(&name) => {
                            self.record.welcomed.push(name.clone())
                        }
                        _ => {}
                    }
                    self.record
                        .latest
                        .insert((name.clone(), kind.clone()), message);
                    self.record
                        .entries
                        .entry(name)
                        .or_default()
                        .push((kind, exact));
                }
                HostEvent::Close {
                    connection, code, ..
                } => {
                    let name = self.names[connection as usize - 1].clone();
                    self.record
                        .entries
                        .entry(name.clone())
                        .or_default()
                        .push((format!("close:{code}"), None));
                    self.record.closed.insert(name);
                }
                HostEvent::Changed => {}
            }
        }
    }

    fn receive(&mut self, name: &str, message: Value) {
        let id = self.id(name);
        self.host.receive(id, &message.to_string(), self.now);
        self.drain();
    }

    fn run(&mut self, step: &Map<String, Value>) {
        let conn = step
            .get("conn")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let extra = step.get("extra").cloned().unwrap_or(json!({}));
        let merge = |mut base: Value| {
            for (key, value) in extra.as_object().unwrap() {
                base[key] = value.clone();
            }
            base
        };
        match step["op"].as_str().unwrap() {
            "join" => {
                let mut join = merge(json!({
                    "type": "join",
                    "version": PROTOCOL_VERSION,
                    "contentVersion": CONTENT,
                    "name": conn,
                    "kind": "balanced",
                }));
                if let Some(owner) = step.get("tokenOf").and_then(Value::as_str) {
                    let welcome = &self.record.latest[&(owner.to_string(), "welcome".into())];
                    join["token"] = welcome["token"].clone();
                    join["roomEpoch"] = welcome["roomEpoch"].clone();
                }
                self.record.closed.remove(&conn);
                self.receive(&conn, join);
            }
            "action" => {
                let message = merge(json!({ "type": step["type"], "roundId": self.host.round_id }));
                self.receive(&conn, message);
            }
            "input" => {
                let seq = self.seqs.entry(conn.clone()).or_default();
                *seq += 1;
                let seq = *seq;
                let control = &self.record.latest[&(conn.clone(), "control".into())];
                let mut input = json!({
                    "type": "input",
                    "roundId": self.host.round_id,
                    "controlEpoch": control["controlEpoch"],
                    "seq": seq,
                    "observedTick": self.host.tick(),
                    "moveX": step["moveX"],
                    "moveZ": step["moveZ"],
                    "aim": step["aim"],
                });
                if step.get("fire") == Some(&Value::Bool(true)) {
                    input["fire"] = json!(true);
                }
                if let Some(actions) = step.get("actions") {
                    input["actions"] = actions.clone();
                }
                self.receive(&conn, input);
            }
            "advance" => {
                for _ in 0..step["count"].as_u64().unwrap() {
                    self.now += 50;
                    let open: Vec<String> = self
                        .record
                        .welcomed
                        .iter()
                        .filter(|name| !self.record.closed.contains(*name))
                        .cloned()
                        .collect();
                    for name in open {
                        let ping = json!({
                            "type": "ping",
                            "roundId": self.host.round_id,
                            "t": self.now,
                            "observedTick": self.host.tick(),
                        });
                        self.receive(&name, ping);
                    }
                    self.host.advance(self.now);
                    self.drain();
                }
            }
            "disconnect" => {
                let id = self.id(&conn);
                self.host.disconnect(id, self.now);
                self.drain();
                self.record.closed.insert(conn);
            }
            "dispose" => {
                self.host.dispose(step["reason"].as_str().unwrap());
                self.drain();
            }
            "raw" => {
                let id = self.id(&conn);
                self.host
                    .receive(id, step["text"].as_str().unwrap(), self.now);
                self.drain();
            }
            other => panic!("unknown step {other}"),
        }
    }
}

/// Paths whose presence depends on what the physics happened to do in the few seconds
/// scripted (which shells hit what), not on the wire format.
fn physics_dependent(path: &str) -> bool {
    path.contains(".events[].event(")
        || path.contains(".paths[]")
        || path.contains(".updates.fragments")
        || path.contains(".removed.")
        || path.contains(".updates.covers.*.")
        || path.contains(".updates.mines")
        || path.contains(".updates.pickups")
        || path.contains(".entities.fragments[]")
        || path.contains(".entities.mines[]")
}

#[test]
fn the_rust_host_speaks_the_typescript_wire_format_for_a_scripted_room() {
    let script: Value =
        serde_json::from_str(include_str!("fixtures/net-golden-script.json")).unwrap();
    let golden: Value = serde_json::from_str(include_str!("fixtures/net-golden.json")).unwrap();
    let mut token = 0;
    let mut runner = Runner {
        host: MatchHost::new(MatchHostOptions {
            room_epoch: script["roomEpoch"].as_str().unwrap().to_string(),
            now_ms: 0,
            token: Box::new(move || {
                token += 1;
                format!("credential-{token:020}")
            }),
            seed: Some(script["seed"].as_u64().unwrap() as u32),
            content_version: Some(CONTENT.to_string()),
        }),
        names: Vec::new(),
        now: 0,
        record: Recording::default(),
        seqs: BTreeMap::new(),
        views: BTreeMap::new(),
        binary: Vec::new(),
    };
    for step in script["steps"].as_array().unwrap() {
        runner.run(step.as_object().unwrap());
    }

    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/net-golden-binary.json"
    );
    let binary = json!({ "protocol": PROTOCOL_VERSION, "messages": runner.binary });
    if std::env::var_os("SLOPPY_UPDATE_FIXTURES").is_some() {
        let text = serde_json::to_string_pretty(&binary).unwrap() + "\n";
        std::fs::write(path, text).unwrap();
    }
    let pinned: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(binary, pinned, "binary messages differ from {path}");

    let expected = golden["connections"].as_object().unwrap();
    let actual = &runner.record.entries;
    assert_eq!(
        actual.keys().collect::<Vec<_>>(),
        expected.keys().collect::<Vec<_>>(),
        "the same connections heard from the host"
    );
    let mut compared = 0;
    for (name, entries) in expected {
        let entries = entries.as_array().unwrap();
        let types: Vec<&str> = entries
            .iter()
            .map(|entry| entry["type"].as_str().unwrap())
            .collect();
        let ours: Vec<&str> = actual[name].iter().map(|(kind, _)| kind.as_str()).collect();
        assert_eq!(ours, types, "{name}: message sequence");
        for (index, entry) in entries.iter().enumerate() {
            if let Some(exact) = entry.get("exact") {
                let mine = actual[name][index].1.as_ref().unwrap();
                assert_same(
                    mine,
                    exact,
                    &format!("{name} message {index} ({})", types[index]),
                );
                compared += 1;
            }
        }
    }
    assert!(compared > 50, "compared {compared} exact messages");

    for (kind, ours) in [
        ("full", &runner.record.full),
        ("snapshot", &runner.record.snapshot),
    ] {
        let theirs: BTreeSet<String> = golden["signatures"][kind]
            .as_array()
            .unwrap()
            .iter()
            .map(|path| path.as_str().unwrap().to_string())
            .collect();
        let missing: Vec<&String> = theirs
            .difference(ours)
            .filter(|path| !physics_dependent(path))
            .collect();
        let extra: Vec<&String> = ours
            .difference(&theirs)
            .filter(|path| !physics_dependent(path))
            .collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "{kind} structure differs: missing {missing:#?}, extra {extra:#?}"
        );
        let physics: Vec<&String> = theirs.symmetric_difference(ours).collect();
        eprintln!(
            "{kind}: {} shared paths, {} physics-dependent differences {physics:?}",
            theirs.intersection(ours).count(),
            physics.len()
        );
        // The physics-independent core is large: every field of every tank, cover and
        // pickup, the match, the map and the frame envelope.
        assert!(theirs.intersection(ours).count() > 100);
    }
}
