//! Snapshot traffic of a seeded room, for before/after comparisons of the wire format.
//! `cargo run -p sloppy-server --release --example room_traffic -- [label] [seconds]`
//!
//! Each room (a 4-player village with bots, the Stress Grid and the Scrap Yard) runs a
//! `MatchHost` on a manual 50 ms clock with four scripted players that drive, sweep their
//! turrets and fire like the traffic bots, from a fixed seed. Every message goes through a
//! server-role WebSocket codec per connection (permessage-deflate with context takeover,
//! as the server negotiates with browsers), so the report has raw and on-the-wire bytes per
//! message type, the host's interval time, the codec's encode time and each client's
//! decode time (parse, apply and project one batch). Every projected frame is hashed into
//! `<label>-<room>-frames.txt`, so two builds can prove they replicate identical state.
//! With `SLOPPY_VERIFY=1` every client's mirror is also compared with the host's own
//! capture after every frame and batch; `SLOPPY_DUMP=1` writes every message sent to
//! `<label>-<room>-messages.jsonl`. Results land in the ignored
//! `artifacts/performance/net-binary/`. Manual evidence, not a CI gate.

use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use bytes::BytesMut;
use serde_json::{Value, json};
use sloppy_core::net::match_host::{HostEvent, MatchHost, MatchHostOptions};
use sloppy_core::net::protocol::{CONTENT_VERSION, Message, PROTOCOL_VERSION};
use sloppy_core::net::replication::{BinaryMessage, StateMirror, read_binary_message};
use sloppy_core::net::scene_codec::Scene;
use sloppy_core::net::schema::parse_record;
use sloppy_core::sim::math::Random;
use sloppy_server::websocket::{Codec, DeflateParams, Role};

const ROOMS: [(&str, &str); 3] = [
    ("standard", "village"),
    ("stress-grid", "stress-test"),
    ("scrap-yard", "superstress"),
];
const PLAYERS: usize = 4;
const INTERVAL_MS: u64 = 50;
const SEED: u32 = 4242;
const INPUT_SEED: f64 = 77.0;
/// Messages the room sends before the measured window (joins, lobby, first baseline).
const WARMUP_INTERVALS: u64 = 40;

struct Player {
    name: String,
    codec: Codec,
    mirror: StateMirror,
    room_epoch: String,
    round_id: u64,
    tank_id: Option<u32>,
    control_epoch: u64,
    seq: u64,
    aim: f64,
    turret_speed: f64,
    move_x: f64,
    move_z: f64,
    fire: bool,
    maneuver_until: u64,
    last_ping: u64,
    decode_ms: Vec<f64>,
    frames: Vec<String>,
    truth: Truth,
    mismatches: Vec<String>,
}

type Truth = std::sync::Arc<std::sync::Mutex<BTreeMap<u64, Scene>>>;

#[derive(Default)]
struct Traffic {
    raw: BTreeMap<String, u64>,
    wire: BTreeMap<String, u64>,
    count: BTreeMap<String, u64>,
    encode_ms: Vec<f64>,
}

fn summary(samples: &[f64]) -> Value {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len().max(1);
    let at = |fraction: f64| sorted.get(((fraction * n as f64) as usize).min(n - 1));
    let round = |value: f64| (value * 10_000.0).round() / 10_000.0;
    json!({
        "n": sorted.len(),
        "mean": round(sorted.iter().sum::<f64>() / n as f64),
        "p50": at(0.5).map(|v| round(*v)),
        "p95": at(0.95).map(|v| round(*v)),
        "max": sorted.last().map(|v| round(*v)),
        "total": round(sorted.iter().sum::<f64>()),
    })
}

fn message_type(message: &Message) -> String {
    match message {
        Message::Text(text) => parse_record(text)
            .ok()
            .and_then(|message| message.get("type")?.as_str().map(str::to_string))
            .unwrap_or_else(|| "other".into()),
        Message::Binary(bytes) => Message::binary_type(bytes).to_string(),
    }
}

impl Player {
    fn hash_frame(&mut self) {
        let Some(tank) = self.tank_id else { return };
        if let Ok(state) = self.mirror.render(tank) {
            let mut hasher = DefaultHasher::new();
            serde_json::to_string(&state)
                .expect("JSON")
                .hash(&mut hasher);
            self.frames.push(format!(
                "{} {} {:016x}",
                self.name,
                self.mirror.tick,
                hasher.finish()
            ));
        }
    }

    /// What the browser's `NetworkClient` does with a message, minus display timing.
    fn receive(&mut self, message: &Message) {
        let text = match message {
            Message::Text(text) => text,
            Message::Binary(bytes) => return self.receive_binary(bytes),
        };
        let Ok(message) = parse_record(text) else {
            panic!("unreadable message");
        };
        match message.get("type").and_then(Value::as_str) {
            Some("welcome") => {
                self.room_epoch = message["roomEpoch"].as_str().unwrap().to_string();
            }
            Some("lobby") => self.round_id = message["roundId"].as_u64().unwrap(),
            Some("control") => {
                self.tank_id = message["tankId"].as_u64().map(|id| id as u32);
                self.control_epoch = message["controlEpoch"].as_u64().unwrap();
            }
            _ => {}
        }
    }

    fn receive_binary(&mut self, bytes: &[u8]) {
        let start = Instant::now();
        match read_binary_message(bytes).expect("readable state") {
            BinaryMessage::Full(baseline) => {
                self.mirror
                    .apply_full(&baseline, &self.room_epoch, self.round_id)
                    .expect("valid baseline");
                self.hash_frame();
            }
            BinaryMessage::Snapshot(mut batch) => {
                let mut rendered = Vec::new();
                for _ in 0..batch.count {
                    assert!(
                        self.mirror.apply_snapshot(&mut batch).is_some(),
                        "contiguous frames"
                    );
                    if let Some(scene) = self.truth.lock().unwrap().get(&self.mirror.tick)
                        && self.mirror.state.as_ref().unwrap().to_scene() != *scene
                    {
                        self.mismatches
                            .push(format!("{} frame tick {}", self.name, self.mirror.tick));
                    }
                    if let Some(tank) = self.tank_id {
                        rendered
                            .push((self.mirror.tick, self.mirror.render(tank).expect("viewer")));
                    }
                }
                self.decode_ms.push(start.elapsed().as_secs_f64() * 1000.0);
                for (tick, state) in rendered {
                    let mut hasher = DefaultHasher::new();
                    serde_json::to_string(&state)
                        .expect("JSON")
                        .hash(&mut hasher);
                    self.frames
                        .push(format!("{} {tick} {:016x}", self.name, hasher.finish()));
                }
            }
        }
    }

    /// Bot-like driving: random maneuvers, a sweeping turret, input at 20 Hz while
    /// active and once a second when idle, and a ping every second.
    fn update(&mut self, random: &mut Random, now: u64, out: &mut Vec<String>) {
        if now >= self.maneuver_until {
            self.maneuver_until = now + 400 + (random.next() * 2600.0) as u64;
            let heading = random.next() * std::f64::consts::TAU;
            let moving = random.next() >= 0.15;
            self.move_x = if moving { heading.sin() } else { 0.0 };
            self.move_z = if moving { heading.cos() } else { 0.0 };
            self.fire = random.next() < 0.6;
            self.turret_speed = (random.next() * 2.0 - 1.0) * 2.5;
        }
        self.aim = (self.aim + self.turret_speed * 0.05)
            .sin()
            .atan2((self.aim + self.turret_speed * 0.05).cos());
        if now >= self.last_ping + 1000 {
            self.last_ping = now;
            out.push(
                json!({"type": "ping", "roundId": self.round_id, "t": now, "observedTick": self.mirror.tick})
                    .to_string(),
            );
        }
        if self.tank_id.is_some() {
            let round = |value: f64, scale: f64| (value * scale).round() / scale;
            self.seq += 1;
            let mut input = json!({
                "type": "input",
                "roundId": self.round_id,
                "controlEpoch": self.control_epoch,
                "moveX": round(self.move_x, 100.0),
                "moveZ": round(self.move_z, 100.0),
                "seq": self.seq,
                "observedTick": self.mirror.tick,
                "aim": {"angle": round(self.aim, 10_000.0)},
            });
            if self.fire {
                input["fire"] = json!(true);
            }
            out.push(input.to_string());
        }
    }
}

fn run(label: &str, room: &str, map: &str, seconds: u64, output: &Path) -> Value {
    let mut token = 0;
    let mut host = MatchHost::new(MatchHostOptions {
        room_epoch: "traffic-room".into(),
        now_ms: 0,
        token: Box::new(move || {
            token += 1;
            format!("credential-{token:020}")
        }),
        seed: Some(SEED),
        content_version: None,
    });
    let truth = Truth::default();
    if std::env::var_os("SLOPPY_VERIFY").is_some() {
        let truth = truth.clone();
        host.tick_hook = Some(Box::new(move |simulation, tick| {
            truth
                .lock()
                .unwrap()
                .insert(tick, Scene::capture(simulation));
        }));
    }
    let mut players: Vec<Player> = (0..PLAYERS)
        .map(|index| Player {
            name: format!("p{index}"),
            codec: Codec::new(Role::Server, Some(&DeflateParams::default()), usize::MAX),
            mirror: StateMirror::default(),
            room_epoch: String::new(),
            round_id: 0,
            tank_id: None,
            control_epoch: 0,
            seq: 0,
            aim: 0.0,
            turret_speed: 0.0,
            move_x: 0.0,
            move_z: 0.0,
            fire: false,
            maneuver_until: 0,
            last_ping: 0,
            decode_ms: Vec::new(),
            frames: Vec::new(),
            truth: truth.clone(),
            mismatches: Vec::new(),
        })
        .collect();
    let mut random = Random::new(INPUT_SEED);
    let mut traffic = Traffic::default();
    let mut advance_ms = Vec::new();
    let mut now = 0;
    let mut measuring;
    let mut dump = std::env::var_os("SLOPPY_DUMP").map(|_| {
        std::io::BufWriter::new(
            std::fs::File::create(output.join(format!("{label}-{room}-messages.jsonl"))).unwrap(),
        )
    });
    let mut deliver =
        |host: &mut MatchHost, players: &mut [Player], traffic: &mut Traffic, measuring: bool| {
            for event in host.take_events() {
                if let HostEvent::Close {
                    connection,
                    code,
                    reason,
                } = &event
                {
                    panic!("the host closed connection {connection}: {code} {reason}");
                }
                if let HostEvent::Send {
                    connection,
                    message,
                } = event
                {
                    if let Some(dump) = dump.as_mut() {
                        let payload = match &message {
                            Message::Text(text) => json!(text),
                            Message::Binary(bytes) => json!(bytes),
                        };
                        writeln!(
                            dump,
                            "{}",
                            json!({"c": connection, "m": measuring, "t": payload})
                        )
                        .unwrap();
                    }
                    let player = &mut players[connection as usize - 1];
                    let mut framed = BytesMut::new();
                    let start = Instant::now();
                    match &message {
                        Message::Text(text) => player.codec.encode_text(text, &mut framed),
                        Message::Binary(bytes) => player.codec.encode_binary(bytes, &mut framed),
                    }
                    let encode = start.elapsed().as_secs_f64() * 1000.0;
                    if measuring {
                        let kind = message_type(&message);
                        *traffic.raw.entry(kind.clone()).or_default() += message.len() as u64;
                        *traffic.wire.entry(kind.clone()).or_default() += framed.len() as u64;
                        *traffic.count.entry(kind).or_default() += 1;
                        traffic.encode_ms.push(encode);
                    }
                    player.receive(&message);
                }
            }
        };
    for index in 0..PLAYERS {
        let mut join = json!({
            "type": "join",
            "version": PROTOCOL_VERSION,
            "contentVersion": CONTENT_VERSION,
            "name": format!("p{index}"),
            "kind": (["scout", "balanced", "heavy"][index % 3]),
            "team": index % 2,
        });
        if index == 0 {
            join["create"] = json!({
                "mapMode": map, "difficulty": "normal", "humansOnly": false, "roundMinutes": 20
            });
        }
        host.receive(index as u64 + 1, &join.to_string(), now);
        deliver(&mut host, &mut players, &mut traffic, false);
    }
    let verify = std::env::var_os("SLOPPY_VERIFY").is_some();
    let mut mismatches: Vec<String> = Vec::new();
    let intervals = WARMUP_INTERVALS + seconds * 1000 / INTERVAL_MS;
    for interval in 0..intervals {
        measuring = interval >= WARMUP_INTERVALS;
        now += INTERVAL_MS;
        for (index, player) in players.iter_mut().enumerate() {
            let mut out = Vec::new();
            player.update(&mut random, now, &mut out);
            for text in out {
                host.receive(index as u64 + 1, &text, now);
            }
        }
        deliver(&mut host, &mut players, &mut traffic, measuring);
        let start = Instant::now();
        host.advance(now);
        if measuring {
            advance_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        deliver(&mut host, &mut players, &mut traffic, measuring);
        if verify {
            let truth = Scene::capture(host.simulation.as_ref().unwrap());
            for player in &players {
                if let Some(state) = &player.mirror.state
                    && state.to_scene() != truth
                {
                    mismatches.push(format!("{} after tick {}", player.name, player.mirror.tick));
                }
            }
        }
    }
    if verify {
        let frames: Vec<String> = players.iter().flat_map(|p| p.mismatches.clone()).collect();
        println!(
            "{room}: {} mirror mismatches with the host after a batch, {} after a frame: {:?}",
            mismatches.len(),
            frames.len(),
            &frames[..frames.len().min(5)]
        );
    }
    let measured_seconds = (intervals - WARMUP_INTERVALS) as f64 * INTERVAL_MS as f64 / 1000.0;
    let mut frames = std::fs::File::create(output.join(format!("{label}-{room}-frames.txt")))
        .expect("frames file");
    for player in &players {
        for line in &player.frames {
            writeln!(frames, "{line}").expect("write");
        }
    }
    let decode: Vec<f64> = players.iter().flat_map(|p| p.decode_ms.clone()).collect();
    let per_client = |totals: &BTreeMap<String, u64>| {
        totals
            .iter()
            .map(|(kind, bytes)| {
                (
                    kind.clone(),
                    json!((*bytes as f64 / PLAYERS as f64 / measured_seconds).round()),
                )
            })
            .collect::<serde_json::Map<_, _>>()
    };
    let sum = |totals: &BTreeMap<String, u64>| totals.values().sum::<u64>() as f64;
    let simulation = host.simulation.as_ref().expect("a round");
    let report = json!({
        "room": room,
        "map": map,
        "tanks": simulation.tanks.len(),
        "covers": simulation.covers.len(),
        "fragments": simulation.fragments.len(),
        "seconds": measured_seconds,
        "rawBytesPerClientSecond": (sum(&traffic.raw) / PLAYERS as f64 / measured_seconds).round(),
        "wireBytesPerClientSecond": (sum(&traffic.wire) / PLAYERS as f64 / measured_seconds).round(),
        "rawByType": per_client(&traffic.raw),
        "wireByType": per_client(&traffic.wire),
        "messages": traffic.count,
        "advanceMs": summary(&advance_ms),
        "encodeMs": summary(&traffic.encode_ms),
        "encodeMsPerInterval": (traffic.encode_ms.iter().sum::<f64>() / advance_ms.len() as f64 * 10_000.0).round() / 10_000.0,
        "decodeMsPerBatch": summary(&decode),
        "frameHashes": players.iter().map(|p| p.frames.len()).sum::<usize>(),
    });
    println!(
        "{room}: raw {} B/s, wire {} B/s per client; advance {} ms, encode {} ms/interval, decode {} ms/batch",
        report["rawBytesPerClientSecond"],
        report["wireBytesPerClientSecond"],
        report["advanceMs"]["mean"],
        report["encodeMsPerInterval"],
        report["decodeMsPerBatch"]["mean"]
    );
    report
}

fn main() {
    let label = std::env::args().nth(1).unwrap_or_else(|| "run".into());
    let seconds = std::env::args()
        .nth(2)
        .and_then(|text| text.parse().ok())
        .unwrap_or(60);
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../artifacts/performance/net-binary");
    std::fs::create_dir_all(&output).expect("output directory");
    let mut results = serde_json::Map::new();
    for (room, map) in ROOMS {
        results.insert(room.to_string(), run(&label, room, map, seconds, &output));
    }
    let path = output.join(format!("{label}-traffic.json"));
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&Value::Object(results)).unwrap(),
    )
    .expect("write report");
    println!("Wrote {}", path.display());
}
