//! End-to-end checks over real localhost sockets (`tests/node-server.test.ts`), with the
//! lobby-only host and a test host that sends large and incompressible messages.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::BytesMut;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use sloppy_server::config::BuildInfo;
use sloppy_server::dashboard::MAX_DASHBOARD_VIEWERS;
use sloppy_server::host::{ConnectionId, HostOptions, HostOutput, RoomHost};
use sloppy_server::lobby_host::LobbyHost;
use sloppy_server::protocol::{CONTENT_VERSION, PROTOCOL_VERSION};
use sloppy_server::room_list::RoomListing;
use sloppy_server::server::{MultiplayerServer, ServerOptions};
use sloppy_server::websocket::{Codec, Event, Role, extension};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};

const ORIGIN: &str = "http://127.0.0.1:5173";
const WAIT: Duration = Duration::from_secs(5);

type Lines = Arc<Mutex<Vec<String>>>;

fn options(lines: &Lines) -> ServerOptions {
    let mut options = ServerOptions::new(vec![ORIGIN.to_string()], true);
    // The rate-limit test opens sockets faster than their closes are counted.
    options.max_sockets_per_ip = 1000;
    let sink = lines.clone();
    options.log = Arc::new(move |line| sink.lock().unwrap().push(line.to_string()));
    options
}

async fn start() -> (MultiplayerServer, String, Lines) {
    let lines = Lines::default();
    let server = MultiplayerServer::listen(options(&lines), LobbyHost::new, "127.0.0.1:0")
        .await
        .unwrap();
    let base = server.local_addr().to_string();
    (server, base, lines)
}

async fn eventually(what: &str, mut check: impl FnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + WAIT;
    while !check() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// A join as the browser writes it, with `type` first.
fn join(name: &str) -> String {
    format!(
        r#"{{"type":"join","version":{PROTOCOL_VERSION},"contentVersion":"{CONTENT_VERSION}","name":"{name}","kind":"balanced"}}"#
    )
}

struct HttpResponse {
    status: u16,
    headers: HashMap<String, String>,
    body: String,
}

/// One HTTP/1.1 request on a fresh connection, read to its end.
async fn http(base: &str, request_line: &str, headers: &[(&str, &str)]) -> HttpResponse {
    let mut stream = TcpStream::connect(base).await.unwrap();
    let mut request = format!("{request_line} HTTP/1.1\r\nHost: {base}\r\nConnection: close\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut raw = Vec::new();
    tokio::time::timeout(WAIT, stream.read_to_end(&mut raw))
        .await
        .unwrap()
        .unwrap();
    parse_response(&String::from_utf8(raw).unwrap())
}

fn parse_response(text: &str) -> HttpResponse {
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((text, ""));
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .unwrap()
        .split(' ')
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect();
    HttpResponse {
        status,
        headers,
        body: body.to_string(),
    }
}

async fn get(base: &str, path: &str, headers: &[(&str, &str)]) -> HttpResponse {
    http(base, &format!("GET {path}"), headers).await
}

async fn rooms(base: &str, query: &str) -> Vec<Value> {
    let response = get(base, &format!("/rooms{query}"), &[("Origin", ORIGIN)]).await;
    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_str(&response.body).unwrap();
    body["rooms"].as_array().unwrap().clone()
}

/// A browser-like client on the crate's own codec, offering permessage-deflate.
struct Client {
    stream: TcpStream,
    codec: Codec,
    buffer: BytesMut,
    extensions: String,
    messages: Vec<Value>,
}

enum Opened {
    Open(Box<Client>),
    Refused(HttpResponse),
}

impl Opened {
    fn client(self) -> Client {
        match self {
            Opened::Open(client) => *client,
            Opened::Refused(response) => {
                panic!("refused with {}: {}", response.status, response.body)
            }
        }
    }
    fn status(&self) -> u16 {
        match self {
            Opened::Open(_) => 101,
            Opened::Refused(response) => response.status,
        }
    }
}

const DEFLATE_OFFER: &str = "permessage-deflate; client_max_window_bits";

async fn open(base: &str, room: &str, headers: &[(&str, &str)]) -> Opened {
    let mut stream = TcpStream::connect(base).await.unwrap();
    let mut request = format!(
        "GET /room/{room} HTTP/1.1\r\nHost: {base}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n"
    );
    let mut headers: Vec<(&str, &str)> = headers.to_vec();
    if !headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("origin"))
    {
        headers.push(("Origin", ORIGIN));
    }
    if !headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("sec-websocket-extensions"))
    {
        headers.push(("Sec-WebSocket-Extensions", DEFLATE_OFFER));
    }
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut buffer = BytesMut::new();
    let head_end = loop {
        if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break end + 4;
        }
        let read = tokio::time::timeout(WAIT, stream.read_buf(&mut buffer))
            .await
            .unwrap()
            .unwrap();
        assert!(read > 0, "connection closed during the handshake");
    };
    let head = buffer.split_to(head_end);
    let response = parse_response(std::str::from_utf8(&head).unwrap());
    if response.status != 101 {
        let length: usize = response
            .headers
            .get("content-length")
            .map_or(0, |value| value.parse().unwrap());
        while buffer.len() < length {
            stream.read_buf(&mut buffer).await.unwrap();
        }
        return Opened::Refused(HttpResponse {
            body: String::from_utf8(buffer.to_vec()).unwrap(),
            ..response
        });
    }
    let extensions = response
        .headers
        .get("sec-websocket-extensions")
        .cloned()
        .unwrap_or_default();
    let params = extension::negotiate([extensions.as_str()]).unwrap();
    Opened::Open(Box::new(Client {
        stream,
        codec: Codec::new(Role::Client, params.as_ref(), 64 << 20),
        buffer,
        extensions,
        messages: Vec::new(),
    }))
}

impl Client {
    async fn send(&mut self, text: &str) {
        let mut frame = BytesMut::new();
        self.codec.encode_text(text, &mut frame);
        self.stream.write_all(&frame).await.unwrap();
    }

    async fn send_raw(&mut self, bytes: &[u8]) {
        self.stream.write_all(bytes).await.unwrap();
    }

    /// The next event with whether its first frame was compressed; `None` at EOF.
    async fn event(&mut self) -> Option<(Event, bool)> {
        loop {
            let compressed = self.buffer.first().is_some_and(|byte| byte & 0x40 != 0);
            if let Some(event) = self.codec.decode(&mut self.buffer).unwrap() {
                if let Event::Text(text) = &event {
                    self.messages.push(serde_json::from_str(text).unwrap());
                }
                return Some((event, compressed));
            }
            let read = tokio::time::timeout(WAIT, self.stream.read_buf(&mut self.buffer))
                .await
                .expect("server went quiet");
            if read.map_or(true, |read| read == 0) {
                return None;
            }
        }
    }

    /// Reads until a message of this type arrives.
    async fn next(&mut self, kind: &str) -> Value {
        loop {
            if let Some(message) = self.messages.iter().find(|message| message["type"] == kind) {
                return message.clone();
            }
            match self.event().await {
                Some((Event::Close(code, reason), _)) => {
                    panic!("closed with {code:?} {reason} before {kind}")
                }
                Some(_) => {}
                None => panic!("connection ended before {kind}"),
            }
        }
    }

    async fn answer_close(&mut self, code: Option<u16>, reason: &str) {
        let mut frame = BytesMut::new();
        self.codec.encode_close(code, reason, &mut frame);
        let _ = self.stream.write_all(&frame).await;
    }

    /// Reads until the server's close frame, answering it; the close code.
    async fn closed(&mut self) -> Option<u16> {
        loop {
            match self.event().await {
                Some((Event::Close(code, reason), _)) => {
                    self.answer_close(code, &reason).await;
                    return code;
                }
                Some(_) => {}
                None => return None,
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reports_health_with_the_content_version_at_the_root_too() {
    let (server, base, _) = start().await;
    let health = get(&base, "/health", &[]).await;
    assert_eq!(health.status, 200);
    assert_eq!(health.headers["content-type"], "application/json");
    let body: Value = serde_json::from_str(&health.body).unwrap();
    assert_eq!(body["contentVersion"], CONTENT_VERSION);
    assert_eq!(body["protocol"], PROTOCOL_VERSION);
    let mut keys: Vec<&String> = body.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["contentVersion", "protocol", "serverBuild"]);
    assert!(
        health.body.ends_with("\n}\n"),
        "pretty-printed with a trailing newline"
    );
    assert!(
        health.body.starts_with("{\n  \"protocol\": 1,"),
        "fields keep the TypeScript order"
    );
    let slashed = get(&base, "/health/", &[]).await;
    assert_eq!(slashed.status, 200);
    assert_eq!(slashed.body, health.body);
    let root = get(&base, "/", &[]).await;
    let dashboard = get(&base, "/dashboard", &[]).await;
    assert_eq!(root.status, 200);
    assert_eq!(
        root.body, dashboard.body,
        "the bare address shows the dashboard"
    );
    server.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reports_the_image_build_stamps_like_the_page() {
    let lines = Lines::default();
    let mut options = options(&lines);
    options.build = BuildInfo {
        release: Some("1.1.0.628".into()),
        commit: Some("27e68e8".into()),
        dirty: false,
        built_at: Some("2026-10-02T14:02:23.799Z".into()),
    };
    let server = MultiplayerServer::listen(options, LobbyHost::new, "127.0.0.1:0")
        .await
        .unwrap();
    let health = get(&server.local_addr().to_string(), "/health", &[]).await;
    let body: Value = serde_json::from_str(&health.body).unwrap();
    assert_eq!(body["version"], "1.1.0.628");
    assert_eq!(body["commit"], "27e68e8");
    assert_eq!(body["dirty"], false);
    assert_eq!(body["builtAt"], "2026-10-02T14:02:23.799Z");
    server.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rejects_foreign_origins_plain_http_rooms_and_invalid_codes() {
    let (server, base, _) = start().await;
    let foreign = get(&base, "/rooms", &[("Origin", "https://evil.example")]).await;
    assert_eq!(
        (foreign.status, foreign.body.as_str()),
        (403, "Origin not allowed")
    );
    let plain = get(&base, "/room/ABCDEFGH", &[("Origin", ORIGIN)]).await;
    assert_eq!(
        (plain.status, plain.body.as_str()),
        (426, "WebSocket required")
    );
    assert_eq!(
        get(&base, "/room/abc", &[("Origin", ORIGIN)]).await.status,
        404
    );
    let refused = open(&base, "ABCDEFGH", &[("Origin", "https://evil.example")]).await;
    assert_eq!(refused.status(), 403);
    let Opened::Refused(missing) = open(&base, "abc", &[]).await else {
        panic!("opened an invalid code")
    };
    assert_eq!((missing.status, missing.body.as_str()), (404, "Not Found"));
    assert_eq!(missing.headers["connection"], "close");
    let bad_version = http(
        &base,
        "GET /room/ABCDEFGH",
        &[
            ("Upgrade", "websocket"),
            ("Connection", "Upgrade"),
            ("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ=="),
            ("Sec-WebSocket-Version", "99"),
            ("Origin", ORIGIN),
        ],
    )
    .await;
    assert_eq!(bad_version.status, 400);
    assert_eq!(bad_version.headers["sec-websocket-version"], "13, 8");
    let bad_extension = open(
        &base,
        "ABCDEFGH",
        &[("Sec-WebSocket-Extensions", "permessage-deflate; bogus")],
    )
    .await;
    assert_eq!(bad_extension.status(), 400);
    let options = http(&base, "OPTIONS /rooms", &[("Origin", ORIGIN)]).await;
    assert_eq!(options.status, 405);
    assert_eq!(options.headers["access-control-allow-origin"], ORIGIN);
    assert_eq!(get(&base, "/nowhere", &[]).await.body, "Not found");
    server.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hosts_a_room_lists_it_and_forgets_it_after_the_last_leave() {
    let (server, base, lines) = start().await;
    let mut player = open(&base, "TESTROOM", &[]).await.client();
    assert_eq!(
        player.extensions, "permessage-deflate",
        "room traffic is compressed"
    );
    player.send(&join("player")).await;
    player.next("welcome").await;
    assert_eq!(server.room_codes(), ["TESTROOM"]);
    let listed = get(&base, "/rooms", &[("Origin", ORIGIN)]).await;
    assert_eq!(listed.headers["access-control-allow-origin"], ORIGIN);
    assert_eq!(listed.headers["cache-control"], "no-store");
    assert_eq!(listed.headers["vary"], "Origin");
    let rooms_now = rooms(&base, "").await;
    let codes: Vec<(&str, u64)> = rooms_now
        .iter()
        .map(|room| {
            (
                room["room"].as_str().unwrap(),
                room["players"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(codes, [("TESTROOM", 1)]);
    let stats = server.sample_now().await;
    let listed: Vec<(&str, u32, u32)> = stats
        .room_list
        .iter()
        .map(|room| (room.room.as_str(), room.players, room.sockets))
        .collect();
    assert_eq!(listed, [("TESTROOM", 1, 1)]);
    assert!(stats.room_list[0].sent_kbps > 0.0);
    player.send(r#"{"type":"leave","roundId":0}"#).await;
    assert_eq!(player.closed().await, Some(1000));
    eventually("the room to end", || server.room_codes().is_empty()).await;
    assert!(rooms(&base, "").await.is_empty());
    let lines: Vec<String> = lines
        .lock()
        .unwrap()
        .iter()
        .filter(|line| line.starts_with("room TESTROOM"))
        .cloned()
        .collect();
    assert_eq!(lines[0], "room TESTROOM created");
    assert_eq!(lines[1], "room TESTROOM player joined (1 connected)");
    let last = lines.last().unwrap();
    assert!(
        last.starts_with("room TESTROOM ended: empty after ") && last.ends_with('s'),
        "{last}"
    );
    server.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn extra_level_rooms_are_listed_only_when_asked_for() {
    let (server, base, _) = start().await;
    let mut player = open(&base, "YARDROOM", &[]).await.client();
    let mut create: Value = serde_json::from_str(&join("yard")).unwrap();
    create["create"] = serde_json::json!({ "mapMode": "superstress", "difficulty": "normal", "humansOnly": false, "roundMinutes": 5 });
    player.send(&create.to_string()).await;
    player.next("welcome").await;
    let codes = |rooms: Vec<Value>| -> Vec<String> {
        rooms
            .iter()
            .map(|room| room["room"].as_str().unwrap().to_string())
            .collect()
    };
    assert!(codes(rooms(&base, "").await).is_empty());
    assert_eq!(codes(rooms(&base, "?debug").await), ["YARDROOM"]);
    player.send(r#"{"type":"leave","roundId":1}"#).await;
    player.closed().await;
    eventually("the room to end", || server.room_codes().is_empty()).await;
    server.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn serves_stats_publicly_and_samples_early_only_for_direct_local_requests() {
    let (server, base, _) = start().await;
    let proxied = &[("X-Forwarded-For", "203.0.113.7")];
    let early = get(&base, "/stats", proxied).await;
    assert_eq!(early.status, 503, "no sample is due yet");
    let direct = get(&base, "/stats", &[]).await;
    assert_eq!(direct.status, 200);
    assert_eq!(direct.headers["cache-control"], "no-store");
    let stats: Value = serde_json::from_str(&direct.body).unwrap();
    assert!(stats["rssMB"].is_number());
    assert!(stats["roomList"].is_array());
    assert!(stats["loopDelayP99Ms"].is_number());
    assert!(stats["totals"]["wireSentMB"].is_number());
    let public = get(&base, "/stats", proxied).await;
    assert_eq!(
        public.status, 200,
        "the direct request's sample is now the latest"
    );
    assert_eq!(public.body, direct.body);
    server.close().await;
}

/// An open `/dashboard/stream`, read one Server-Sent Event at a time. HTTP/1.0 keeps
/// the body free of chunk framing.
struct Viewer {
    stream: TcpStream,
    status: u16,
    buffered: String,
}

impl Viewer {
    async fn open(base: &str, ip: &str) -> Viewer {
        let mut stream = TcpStream::connect(base).await.unwrap();
        let request = format!(
            "GET /dashboard/stream HTTP/1.0\r\nHost: {base}\r\nX-Forwarded-For: {ip}\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut viewer = Viewer {
            stream,
            status: 0,
            buffered: String::new(),
        };
        let head = viewer.until("\r\n\r\n").await;
        viewer.status = head.split(' ').nth(1).unwrap().parse().unwrap();
        viewer
    }

    async fn until(&mut self, separator: &str) -> String {
        loop {
            if let Some(end) = self.buffered.find(separator) {
                let text = self.buffered[..end].to_string();
                self.buffered.drain(..end + separator.len());
                return text;
            }
            let mut chunk = [0u8; 16 * 1024];
            let read = tokio::time::timeout(WAIT, self.stream.read(&mut chunk))
                .await
                .unwrap()
                .unwrap();
            assert!(read > 0, "dashboard stream ended");
            self.buffered
                .push_str(std::str::from_utf8(&chunk[..read]).unwrap());
        }
    }

    async fn next(&mut self) -> (String, Value, String) {
        let text = self.until("\n\n").await;
        let kind = text
            .lines()
            .find_map(|line| line.strip_prefix("event: "))
            .unwrap()
            .to_string();
        let data = text
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .unwrap();
        (kind, serde_json::from_str(data).unwrap(), text)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn streams_the_dashboard_without_revealing_room_codes() {
    let (server, base, _) = start().await;
    let page = get(&base, "/dashboard", &[]).await;
    assert_eq!(page.status, 200);
    assert!(page.headers["content-type"].starts_with("text/html"));
    assert!(page.headers["content-security-policy"].contains("frame-ancestors 'none'"));
    assert!(page.body.contains("<title>Sloppy Tanks server</title>"));

    let mut player = open(&base, "DASHROOM", &[]).await.client();
    player.send(&join("player")).await;
    player.next("welcome").await;
    let mut viewer = Viewer::open(&base, "198.51.100.20").await;
    assert_eq!(viewer.status, 200);
    let (kind, hello, hello_text) = viewer.next().await;
    assert_eq!(kind, "hello");
    assert_eq!(hello["server"]["contentVersion"], CONTENT_VERSION);
    assert!(hello["history"].is_array());
    server.read_now().await;
    let (kind, reading, reading_text) = viewer.next().await;
    assert_eq!(kind, "reading");
    let rooms: Vec<(&str, u64)> = reading["roomList"]
        .as_array()
        .unwrap()
        .iter()
        .map(|room| {
            (
                room["room"].as_str().unwrap(),
                room["players"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(rooms, [("DAS•••••", 1)]);
    for text in [&hello_text, &reading_text] {
        assert!(!text.contains("DASHROOM"));
    }
    assert!(
        format!("{hello_text}{reading_text}").contains("DAS•••••"),
        "events and rooms show the masked code"
    );
    assert!(
        reading["wireSentKBps"].as_f64().unwrap() > 0.0,
        "socket bytes are counted"
    );
    assert!(reading["receivedMessages"]["join"].as_f64().unwrap() > 0.0);
    assert!(reading["sentMessages"]["welcome"].as_f64().unwrap() > 0.0);
    assert!(reading["totals"]["joins"].as_u64().unwrap() >= 1);
    player.send(r#"{"type":"leave","roundId":0}"#).await;
    player.closed().await;
    server.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn caps_dashboard_viewers_and_frees_a_slot_when_one_leaves() {
    let (server, base, _) = start().await;
    let ip = "198.51.100.21";
    let mut viewers = Vec::new();
    for _ in 0..MAX_DASHBOARD_VIEWERS {
        let mut viewer = Viewer::open(&base, ip).await;
        assert_eq!(viewer.next().await.0, "hello");
        viewers.push(viewer);
    }
    assert_eq!(Viewer::open(&base, ip).await.status, 503);
    drop(viewers.pop());
    // The server notices the closed stream a moment after the client drops it.
    let mut reopened = Viewer::open(&base, ip).await;
    for _ in 0..100 {
        if reopened.status == 200 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
        reopened = Viewer::open(&base, ip).await;
    }
    assert_eq!(reopened.status, 200);
    server.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rate_limits_room_connections_per_forwarded_client_ip() {
    let (server, base, _) = start().await;
    let mut statuses = Vec::new();
    for _ in 0..61 {
        // Each socket closes straight away, so the room's 16-socket cap never applies.
        statuses.push(
            open(&base, "RATELIMT", &[("X-Forwarded-For", "203.0.113.9")])
                .await
                .status(),
        );
    }
    assert_eq!(statuses.iter().filter(|status| **status == 101).count(), 60);
    assert_eq!(statuses.last(), Some(&429));
    // Another address keeps its own budget.
    assert_eq!(
        open(&base, "RATELIMT", &[("X-Forwarded-For", "203.0.113.10")])
            .await
            .status(),
        101
    );
    server.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn caps_live_rooms_and_open_sockets_per_address() {
    let lines = Lines::default();
    let mut options = options(&lines);
    options.max_rooms = 1;
    options.max_sockets_per_ip = 2;
    let server = MultiplayerServer::listen(options, LobbyHost::new, "127.0.0.1:0")
        .await
        .unwrap();
    let base = server.local_addr().to_string();
    let from = |ip: &'static str| [("X-Forwarded-For", ip)];
    let first = open(&base, "CAPROOM2", &from("192.0.2.1")).await;
    assert_eq!(first.status(), 101);
    let Opened::Refused(full) = open(&base, "CAPROOM3", &from("192.0.2.2")).await else {
        panic!("a new room past the cap")
    };
    assert_eq!(
        (full.status, full.body.as_str()),
        (503, "Server is full; try again later")
    );
    let second = open(&base, "CAPROOM2", &from("192.0.2.1")).await;
    assert_eq!(
        second.status(),
        101,
        "joining an existing room is still allowed"
    );
    assert_eq!(
        open(&base, "CAPROOM2", &from("192.0.2.1")).await.status(),
        429,
        "third socket from one IP"
    );
    let mut second = second.client();
    let mut close = BytesMut::new();
    second.codec.encode_close(Some(1000), "", &mut close);
    second.send_raw(&close).await;
    assert!(matches!(
        second.event().await,
        Some((Event::Close(Some(1000), _), _))
    ));
    let mut again = open(&base, "CAPROOM2", &from("192.0.2.1")).await;
    for _ in 0..100 {
        if again.status() == 101 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
        again = open(&base, "CAPROOM2", &from("192.0.2.1")).await;
    }
    assert_eq!(
        again.status(),
        101,
        "a closed socket frees its address slot"
    );
    drop(first);
    server.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_room_refuses_its_seventeenth_socket() {
    let (server, base, _) = start().await;
    let mut clients = Vec::new();
    for _ in 0..16 {
        clients.push(open(&base, "BUSYROOM", &[]).await.client());
    }
    let refused = open(&base, "BUSYROOM", &[]).await;
    assert_eq!(refused.status(), 429);
    server.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_resets_live_rooms() {
    let (server, base, _) = start().await;
    let mut player = open(&base, "SHUTDOWN", &[("X-Forwarded-For", "198.51.100.4")])
        .await
        .client();
    player.send(&join("player")).await;
    player.next("welcome").await;
    let reader = tokio::spawn(async move {
        let code = player.closed().await;
        (code, player.next("room-reset").await)
    });
    let codes = server.room_codes();
    assert_eq!(codes, ["SHUTDOWN"]);
    server.close().await;
    let (code, reset) = reader.await.unwrap();
    assert_eq!(code, Some(1012));
    assert_eq!(reset["reason"], "server-restart");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_independent_client_without_deflate_gets_plain_frames() {
    let (server, base, _) = start().await;
    let mut request = format!("ws://{base}/room/PLAINWSS")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("Origin", HeaderValue::from_static(ORIGIN));
    let (mut socket, response) = tokio_tungstenite::connect_async(request).await.unwrap();
    assert!(response.headers().get("sec-websocket-extensions").is_none());
    socket
        .send(Message::text(join("tungstenite")))
        .await
        .unwrap();
    let welcome = socket.next().await.unwrap().unwrap();
    assert!(
        welcome
            .to_text()
            .unwrap()
            .starts_with(r#"{"type":"welcome""#)
    );
    socket
        .send(Message::Ping(b"hi".to_vec().into()))
        .await
        .unwrap();
    let mut ponged = false;
    socket
        .send(Message::text(
            r#"{"type":"ping","roundId":0,"t":5,"observedTick":0}"#,
        ))
        .await
        .unwrap();
    let mut pong = None;
    while pong.is_none() || !ponged {
        match socket.next().await.unwrap().unwrap() {
            Message::Pong(payload) => ponged = &payload[..] == b"hi",
            Message::Text(text) if text.contains(r#""type":"pong""#) => {
                pong = Some(text.to_string())
            }
            _ => {}
        }
    }
    assert_eq!(pong.unwrap(), r#"{"type":"pong","t":5,"tick":0}"#);
    socket.close(None).await.unwrap();
    while let Some(Ok(_)) = socket.next().await {}
    server.close().await;
}

/// Echoes messages and, on request, floods its connection with incompressible text.
struct EchoHost {
    options: HostOptions,
    clients: Vec<ConnectionId>,
    disposed: Option<String>,
}

fn noise(length: usize, seed: &mut u64) -> String {
    (0..length)
        .map(|_| {
            *seed ^= *seed << 13;
            *seed ^= *seed >> 7;
            *seed ^= *seed << 17;
            char::from(b'a' + (*seed % 26) as u8)
        })
        .collect()
}

impl RoomHost for EchoHost {
    fn receive(
        &mut self,
        connection: ConnectionId,
        text: &str,
        _now_ms: u64,
        out: &mut HostOutput,
    ) {
        if text.contains(r#""type":"join""#) {
            self.clients.push(connection);
            out.send(
                connection,
                format!(
                    r#"{{"type":"welcome","roomEpoch":"{}"}}"#,
                    self.options.room_epoch
                ),
            );
        } else if let Some(count) = text.strip_prefix("flood ") {
            let mut seed = 0x2545_f491_4f6c_dd1d;
            for _ in 0..count.parse::<usize>().unwrap() {
                out.send(
                    connection,
                    format!(
                        r#"{{"type":"snapshot","noise":"{}"}}"#,
                        noise(60_000, &mut seed)
                    ),
                );
            }
        } else {
            out.send(connection, text);
        }
    }
    fn disconnect(&mut self, connection: ConnectionId, _now_ms: u64, _out: &mut HostOutput) {
        self.clients.retain(|client| *client != connection);
    }
    fn advance(&mut self, _now_ms: u64, _out: &mut HostOutput) {}
    fn dispose(&mut self, reason: &str, out: &mut HostOutput) {
        for client in self.clients.drain(..) {
            out.close(client, 1012, reason);
        }
        self.disposed = Some(reason.to_string());
    }
    fn is_disposed(&self) -> bool {
        self.disposed.is_some()
    }
    fn dispose_reason(&self) -> Option<&str> {
        self.disposed.as_deref()
    }
    fn directory_entry(&self, room: &str) -> RoomListing {
        let lobby = LobbyHost::new(HostOptions {
            room_epoch: String::new(),
            now_ms: 0,
            seed: 0,
            content_version: String::new(),
            token: Box::new(String::new),
        });
        RoomListing {
            players: self.clients.len() as u32,
            ..lobby.directory_entry(room)
        }
    }
    fn connections(&self) -> u32 {
        self.clients.len() as u32
    }
    fn tick(&self) -> u64 {
        0
    }
    fn debt_ms(&self) -> f64 {
        0.0
    }
}

async fn start_echo() -> (MultiplayerServer, String, Lines) {
    let lines = Lines::default();
    let factory = |options: HostOptions| EchoHost {
        options,
        clients: Vec::new(),
        disposed: None,
    };
    let server = MultiplayerServer::listen(options(&lines), factory, "127.0.0.1:0")
        .await
        .unwrap();
    let base = server.local_addr().to_string();
    (server, base, lines)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compresses_large_messages_both_ways_with_context_takeover() {
    let (server, base, _) = start_echo().await;
    let mut client = open(&base, "ECHOROOM", &[]).await.client();
    client.send(&join("echo")).await;
    let (_, compressed) = client.event().await.unwrap();
    assert!(!compressed, "a short welcome goes out uncompressed");
    let snapshot = |index: usize| {
        format!(
            r#"{{"type":"input","roundId":0,"seq":{index},"pad":"{}"}}"#,
            "tank:12.5,-3.25,0.75;".repeat(150)
        )
    };
    for index in 0..3 {
        let text = snapshot(index);
        assert!(text.len() > 3000 && text.len() < 4096);
        client.send(&text).await;
        let (event, compressed) = client.event().await.unwrap();
        assert_eq!(event, Event::Text(text));
        assert!(compressed, "large messages are compressed");
    }
    let reading = server.read_now().await;
    assert!(
        reading.point.wire_sent_kbps * 4.0 < reading.point.sent_kbps,
        "the wire carries a fraction of the JSON: {} vs {} KB/s",
        reading.point.wire_sent_kbps,
        reading.point.sent_kbps
    );
    server.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closes_a_slow_reader_with_4002() {
    let (server, base, lines) = start_echo().await;
    let mut client = open(&base, "SLOWROOM", &[]).await.client();
    client.send(&join("slow")).await;
    client.event().await.unwrap();
    // 120 incompressible 60 KB messages, far more than the socket buffers hold.
    client.send("flood 120").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut received = 0;
    let code = loop {
        match client.event().await {
            Some((Event::Text(_), _)) => received += 1,
            Some((Event::Close(code, reason), _)) => {
                client.answer_close(code, &reason).await;
                break code;
            }
            Some(_) => {}
            None => break None,
        }
    };
    assert_eq!(code, Some(4002));
    assert!(
        received < 120,
        "the backlog was cut short after {received} messages"
    );
    eventually("the close to be logged", || {
        lines
            .lock()
            .unwrap()
            .iter()
            .any(|line| line == "room SLOWROOM player disconnected (code 4002) (0 connected)")
    })
    .await;
    server.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_protocol_violation_drops_the_socket_and_logs_1011() {
    let (server, base, lines) = start().await;
    let mut client = open(&base, "BADFRAME", &[]).await.client();
    client.send(&join("bad")).await;
    client.next("welcome").await;
    // An unmasked client frame.
    client.send_raw(&[0x81, 0x02, b'h', b'i']).await;
    loop {
        match client.event().await {
            None => break,
            Some((Event::Close(..), _)) => panic!("ws cuts the connection without a close frame"),
            Some(_) => {}
        }
    }
    eventually("the failure to be logged", || {
        lines.lock().unwrap().iter().any(|line| {
            line == "room BADFRAME server closed a socket: 1011 Socket failed (0 connected)"
        })
    })
    .await;
    // Oversized frames are refused from the header alone.
    let mut client = open(&base, "BADFRAME", &[]).await.client();
    let mut frame = BytesMut::new();
    client.codec.encode_text(&"x".repeat(9000), &mut frame);
    client.send_raw(&frame).await;
    assert!(client.event().await.is_none());
    server.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inflates_compressed_client_messages() {
    let (server, base, _) = start().await;
    let mut client = open(&base, "INFLATES", &[]).await.client();
    let mut padded: Value = serde_json::from_str(&join("padded")).unwrap();
    padded["pad"] = Value::String("abc".repeat(1000));
    client.send(&padded.to_string()).await;
    client.next("welcome").await;
    // Within the frame limit but over the protocol's 4096 bytes once inflated: 1008.
    padded["pad"] = Value::String("abc".repeat(2000));
    client.send(&padded.to_string()).await;
    assert_eq!(client.closed().await, Some(1008));
    server.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tungstenite_sees_closing_codes_from_the_room() {
    let (server, base, _) = start().await;
    let mut request = format!("ws://{base}/room/CLOSING2")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("Origin", HeaderValue::from_static(ORIGIN));
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    socket.send(Message::text("{not json")).await.unwrap();
    let mut close = None;
    while let Some(message) = socket.next().await {
        match message {
            Ok(Message::Close(frame)) => {
                close = frame.map(|frame| (u16::from(frame.code), frame.reason.to_string()))
            }
            Ok(_) => {}
            Err(WsError::ConnectionClosed) | Err(_) => break,
        }
    }
    assert_eq!(close, Some((1008, "invalid-message".into())));
    server.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn simultaneous_upgrades_share_one_address_reservation() {
    let lines = Lines::default();
    let mut options = options(&lines);
    options.max_sockets_per_ip = 1;
    options.max_rooms = 32;
    let server = MultiplayerServer::listen(options, LobbyHost::new, "127.0.0.1:0")
        .await
        .unwrap();
    let address = server.local_addr();
    let ready = Arc::new(tokio::sync::Barrier::new(33));
    let mut clients = Vec::new();
    for index in 0..32 {
        let ready = ready.clone();
        clients.push(tokio::spawn(async move {
            let mut stream = TcpStream::connect(address).await.unwrap();
            let room = format!(
                "AAAAAA{}{}",
                (b'A' + index / 26) as char,
                (b'A' + index % 26) as char
            );
            let request = format!(
                "GET /room/{room} HTTP/1.1\r\nHost: {address}\r\nOrigin: {ORIGIN}\r\n\
                 Upgrade: websocket\r\nConnection: Upgrade\r\n\
                 Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
                 Sec-WebSocket-Version: 13\r\n\r\n"
            );
            ready.wait().await;
            stream.write_all(request.as_bytes()).await.unwrap();
            let mut response = Vec::new();
            let mut buffer = [0; 1024];
            while !response.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                let count = tokio::time::timeout(WAIT, stream.read(&mut buffer))
                    .await
                    .unwrap()
                    .unwrap();
                assert!(count > 0, "connection closed before its handshake response");
                response.extend_from_slice(&buffer[..count]);
            }
            let status = parse_response(std::str::from_utf8(&response).unwrap()).status;
            (status, stream)
        }));
    }
    ready.wait().await;
    let mut sockets = Vec::new();
    let mut accepted = 0;
    for client in clients {
        let (status, stream) = client.await.unwrap();
        assert!(matches!(status, 101 | 429), "unexpected status {status}");
        accepted += usize::from(status == 101);
        // Every accepted connection stays open until all handshakes have completed.
        sockets.push(stream);
    }
    drop(sockets);
    server.close().await;
    assert_eq!(accepted, 1, "simultaneous connections exceeded the IP cap");
}

#[tokio::test]
async fn refused_handshake_releases_its_address_reservation() {
    let lines = Lines::default();
    let mut options = options(&lines);
    options.max_sockets_per_ip = 1;
    let server = MultiplayerServer::listen(options, LobbyHost::new, "127.0.0.1:0")
        .await
        .unwrap();
    let base = server.local_addr().to_string();
    let refused = open(
        &base,
        "RETRY222",
        &[("Sec-WebSocket-Extensions", "permessage-deflate; =broken")],
    )
    .await;
    assert_eq!(refused.status(), 400);
    let accepted = open(&base, "RETRY222", &[]).await;
    assert_eq!(accepted.status(), 101);
    drop(accepted);
    server.close().await;
}
