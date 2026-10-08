//! Snapshot bytes one room client receives, and the projectile share of them.
//! `cargo run -p sloppy-server --release --example snapshot_bandwidth -- [output.json]`
//! plays each room with one idle watcher while the bots fight, through `MatchHost` and the
//! server's permessage-deflate (level 2, context takeover). Every snapshot message counts
//! raw and deflated; the projectile share is the difference to the same stream with its
//! projectile keys (`traces`, `paths` and the `shots` entity kind) stripped, both
//! re-serialized and compressed on their own deflate contexts. `advance` times the host's
//! whole 50 ms interval: steps, capture, diff and JSON. Manual evidence, not a CI gate.

use std::path::PathBuf;
use std::time::Instant;

use serde_json::{Value, json};
use sloppy_core::net::match_host::{HostEvent, MatchHost, MatchHostOptions};
use sloppy_core::net::protocol::{CONTENT_VERSION, PROTOCOL_VERSION};
use sloppy_server::websocket::codec::Role;
use sloppy_server::websocket::deflate::Deflate;
use sloppy_server::websocket::extension::DeflateParams;

const INTERVAL_MS: u64 = 50;
const WARMUP_SECONDS: u64 = 20;
const MEASURED_SECONDS: u64 = 60;
const SEEDS: [u32; 3] = [4242, 7, 1999];
const ROOMS: [(&str, &str); 3] = [
    ("village", "village"),
    ("stress-grid", "stress-test"),
    ("scrap-yard", "superstress"),
];
const WATCHER: u64 = 1;

/// The frame without its projectile replication.
fn strip_projectiles(message: &mut Value) {
    let Some(frames) = message.get_mut("snapshots").and_then(Value::as_array_mut) else {
        return;
    };
    for frame in frames {
        let Some(frame) = frame.as_object_mut() else {
            continue;
        };
        frame.remove("traces");
        frame.remove("paths");
        for section in ["updates", "removed"] {
            if let Some(kinds) = frame.get_mut(section).and_then(Value::as_object_mut) {
                kinds.remove("shots");
                if kinds.is_empty() {
                    frame.remove(section);
                }
            }
        }
    }
}

#[derive(Default)]
struct Totals {
    messages: u64,
    raw: u64,
    deflated: u64,
    /// Re-serialized stream with and without projectiles, raw and deflated.
    whole_raw: u64,
    whole_deflated: u64,
    stripped_raw: u64,
    stripped_deflated: u64,
    advance_ms: Vec<f64>,
}

fn send(host: &mut MatchHost, now: u64, message: Value) {
    host.receive(WATCHER, &message.to_string(), now);
}

fn deflater() -> Deflate {
    Deflate::new(&DeflateParams::default(), Role::Server)
}

fn run(map_mode: &str, seed: u32) -> Totals {
    let mut token = 0;
    let mut host = MatchHost::new(MatchHostOptions {
        room_epoch: "bandwidth".into(),
        now_ms: 0,
        token: Box::new(move || {
            token += 1;
            format!("credential-{token:020}")
        }),
        seed: Some(seed),
        content_version: None,
    });
    let mut now = 0;
    send(
        &mut host,
        now,
        json!({
            "type": "join", "version": PROTOCOL_VERSION, "contentVersion": CONTENT_VERSION,
            "name": "watcher", "kind": "balanced", "team": 0,
        }),
    );
    let round = host.round_id;
    send(
        &mut host,
        now,
        json!({ "type": "settings", "roundId": round, "mapMode": map_mode,
                "difficulty": "normal", "humansOnly": false, "roundMinutes": 5 }),
    );
    send(&mut host, now, json!({ "type": "start", "roundId": round }));
    host.take_events();
    let (mut wire, mut whole, mut stripped) = (deflater(), deflater(), deflater());
    let mut totals = Totals::default();
    let warmup = WARMUP_SECONDS * 1000 / INTERVAL_MS;
    let measured = MEASURED_SECONDS * 1000 / INTERVAL_MS;
    for interval in 0..warmup + measured {
        now += INTERVAL_MS;
        let ping = json!({
            "type": "ping", "roundId": host.round_id, "t": now, "observedTick": host.tick(),
        });
        send(&mut host, now, ping);
        let start = Instant::now();
        host.advance(now);
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        for event in host.take_events() {
            let HostEvent::Send { text, .. } = event else {
                continue;
            };
            if !text.starts_with("{\"type\":\"snapshot\"") {
                continue;
            }
            // Every context sees the whole stream, warm-up included, like a live socket.
            let deflated = wire.compress(text.as_bytes()).len();
            let mut value: Value = serde_json::from_str(&text).expect("host sends JSON");
            let whole_text = value.to_string();
            strip_projectiles(&mut value);
            let stripped_text = value.to_string();
            let whole_deflated = whole.compress(whole_text.as_bytes()).len();
            let stripped_deflated = stripped.compress(stripped_text.as_bytes()).len();
            if interval < warmup {
                continue;
            }
            totals.messages += 1;
            totals.raw += text.len() as u64;
            totals.deflated += deflated as u64;
            totals.whole_raw += whole_text.len() as u64;
            totals.whole_deflated += whole_deflated as u64;
            totals.stripped_raw += stripped_text.len() as u64;
            totals.stripped_deflated += stripped_deflated as u64;
        }
        if interval >= warmup {
            totals.advance_ms.push(elapsed);
        }
    }
    totals
}

fn main() {
    let output = std::env::args().nth(1).map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../artifacts/performance/snapshot-bandwidth.json")
        },
        PathBuf::from,
    );
    let mut results = serde_json::Map::new();
    for (name, map_mode) in ROOMS {
        let mut runs = Vec::new();
        for seed in SEEDS {
            let mut totals = run(map_mode, seed);
            let per_second = |bytes: u64| bytes as f64 / MEASURED_SECONDS as f64;
            let raw_share = 1.0 - totals.stripped_raw as f64 / totals.whole_raw as f64;
            let deflated_share =
                1.0 - totals.stripped_deflated as f64 / totals.whole_deflated as f64;
            totals.advance_ms.sort_by(f64::total_cmp);
            let n = totals.advance_ms.len();
            let mean = totals.advance_ms.iter().sum::<f64>() / n as f64;
            let p95 = totals.advance_ms[(n * 95 / 100).min(n - 1)];
            println!(
                "{name} seed {seed}: {} msgs, raw {:.0} B/s ({:.1}% projectiles), deflated {:.0} B/s ({:.1}% projectiles), advance mean {mean:.3} ms p95 {p95:.3} ms",
                totals.messages,
                per_second(totals.raw),
                raw_share * 100.0,
                per_second(totals.deflated),
                deflated_share * 100.0,
            );
            runs.push(json!({
                "seed": seed,
                "messages": totals.messages,
                "rawBytesPerSecond": per_second(totals.raw),
                "deflatedBytesPerSecond": per_second(totals.deflated),
                "projectileRawShare": raw_share,
                "projectileDeflatedShare": deflated_share,
                "projectileRawBytesPerSecond": raw_share * per_second(totals.raw),
                "projectileDeflatedBytesPerSecond": deflated_share * per_second(totals.deflated),
                "advanceMs": { "n": n, "mean": mean, "p95": p95, "max": totals.advance_ms[n - 1] },
            }));
        }
        results.insert(name.to_string(), Value::Array(runs));
    }
    if let Some(directory) = output.parent() {
        std::fs::create_dir_all(directory).expect("create the output directory");
    }
    let report = json!({
        "warmupSeconds": WARMUP_SECONDS,
        "measuredSeconds": MEASURED_SECONDS,
        "seeds": SEEDS,
        "results": results,
    });
    std::fs::write(
        &output,
        serde_json::to_string_pretty(&report).expect("JSON"),
    )
    .expect("write the report");
    println!("Wrote {}", output.display());
}
