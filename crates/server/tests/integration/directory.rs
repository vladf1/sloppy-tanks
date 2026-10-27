//! Announcing to the server directory against a fake directory on localhost, and
//! `/servers`, the same listing served by the server itself.

use std::convert::Infallible;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::support::{Lines, ORIGIN, get, http, options};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Empty};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use serde_json::Value;
use sloppy_server::directory::{DirectoryOptions, DirectoryUrl};
use sloppy_server::match_room::MatchRoom;
use sloppy_server::protocol::{CONTENT_VERSION, PROTOCOL_VERSION};
use sloppy_server::server::MultiplayerServer;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

const WAIT: Duration = Duration::from_secs(5);
const CHECK: Duration = Duration::from_millis(20);
const INTERVAL: Duration = Duration::from_millis(100);

/// One request the fake directory received.
struct Received {
    at: Instant,
    method: String,
    target: String,
    content_type: String,
    body: Value,
}

/// A directory on a free localhost port that records every request and answers `status`.
#[derive(Clone, Default)]
struct FakeDirectory {
    received: Arc<Mutex<Vec<Received>>>,
    status: Arc<AtomicU16>,
}

impl FakeDirectory {
    async fn start(status: u16) -> (Self, String) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "http://{}/servers?environment=test",
            listener.local_addr().unwrap()
        );
        let directory = Self::default();
        directory.status.store(status, Ordering::Relaxed);
        let serving = directory.clone();
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let directory = serving.clone();
                let service = service_fn(move |request| directory.clone().record(request));
                tokio::spawn(http1::Builder::new().serve_connection(TokioIo::new(stream), service));
            }
        });
        (directory, url)
    }

    async fn record(
        self,
        request: Request<Incoming>,
    ) -> Result<Response<Empty<Bytes>>, Infallible> {
        let method = request.method().to_string();
        let target = request.uri().to_string();
        let content_type = request
            .headers()
            .get("content-type")
            .map_or("", |value| value.to_str().unwrap())
            .to_string();
        let body = request.into_body().collect().await.unwrap().to_bytes();
        self.received.lock().unwrap().push(Received {
            at: Instant::now(),
            method,
            target,
            content_type,
            body: serde_json::from_slice(&body).unwrap(),
        });
        let status = self.status.load(Ordering::Relaxed);
        Ok(Response::builder()
            .status(status)
            .body(Empty::new())
            .unwrap())
    }

    fn count(&self) -> usize {
        self.received.lock().unwrap().len()
    }

    /// Waits for a request after the first `after` whose body passes `check`; its index.
    async fn wait_for(&self, after: usize, what: &str, check: impl Fn(&Value) -> bool) -> usize {
        let deadline = Instant::now() + WAIT;
        loop {
            let found = self
                .received
                .lock()
                .unwrap()
                .iter()
                .skip(after)
                .position(|received| check(&received.body));
            if let Some(index) = found {
                return after + index;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
}

async fn start(
    url: &str,
    check: Duration,
    interval: Duration,
) -> (MultiplayerServer, String, Lines) {
    let lines = Lines::default();
    let mut options = options(&lines);
    options.directory = Some(DirectoryOptions {
        check,
        interval,
        timeout: Duration::from_secs(2),
        ..DirectoryOptions::new(DirectoryUrl::parse(url).unwrap())
    });
    let server = MultiplayerServer::listen(options, MatchRoom::new, "127.0.0.1:0")
        .await
        .unwrap();
    let base = server.local_addr().to_string();
    (server, base, lines)
}

fn room_codes(listing: &Value) -> Vec<&str> {
    listing["rooms"]
        .as_array()
        .unwrap()
        .iter()
        .map(|room| room["room"].as_str().unwrap())
        .collect()
}

fn lines_with(lines: &Lines, text: &str) -> Vec<String> {
    lines
        .lock()
        .unwrap()
        .iter()
        .filter(|line| line.contains(text))
        .cloned()
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn announces_room_changes_at_once_and_an_unchanged_listing_after_the_interval() {
    let (directory, url) = FakeDirectory::start(204).await;
    let (server, base, lines) = start(&url, CHECK, INTERVAL).await;
    let first = directory
        .wait_for(0, "the first announcement", |_| true)
        .await;
    {
        let received = directory.received.lock().unwrap();
        let first = &received[first];
        assert_eq!(first.method, "PUT");
        assert_eq!(first.target, "/servers?environment=test");
        assert_eq!(first.content_type, "application/json");
        assert_eq!(first.body["protocol"], PROTOCOL_VERSION);
        assert_eq!(first.body["contentVersion"], CONTENT_VERSION);
        assert!(first.body["serverBuild"].is_string());
        assert!(first.body["startedAt"].as_str().unwrap().ends_with('Z'));
        assert_eq!(first.body["rooms"], Value::Array(Vec::new()));
    }

    let mut socket = join(&base, "LISTROOM").await;
    let opened = directory
        .wait_for(first + 1, "the room's announcement", |body| {
            room_codes(body) == ["LISTROOM"]
        })
        .await;
    assert_eq!(
        directory.received.lock().unwrap()[opened].body["rooms"][0]["players"],
        1
    );

    // The server is its own directory too, with the listing it announces. A `/stats`
    // sample adds the load figures.
    server.sample_now().await;
    let servers = get(&base, "/servers", &[("Origin", ORIGIN)]).await;
    assert_eq!(servers.status, 200);
    assert_eq!(servers.headers["access-control-allow-origin"], ORIGIN);
    assert_eq!(servers.headers["cache-control"], "no-store");
    let body: Value = serde_json::from_str(&servers.body).unwrap();
    let listed = body["servers"].as_array().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["url"], format!("ws://{base}"));
    assert_eq!(listed[0]["protocol"], PROTOCOL_VERSION);
    assert_eq!(listed[0]["contentVersion"], CONTENT_VERSION);
    assert_eq!(room_codes(&listed[0]), ["LISTROOM"]);
    assert!(listed[0]["startedAt"].is_string());
    assert!(listed[0]["cpuPercent"].as_f64().unwrap() >= 0.0);
    assert!(listed[0]["cpus"].as_u64().unwrap() >= 1);
    assert!(listed[0]["rssMB"].as_f64().unwrap() > 0.0);
    assert!(listed[0]["hostMemoryMB"].as_f64().unwrap() > 0.0);
    let proxied = get(&base, "/servers", &[("X-Forwarded-Proto", "https")]).await;
    let body: Value = serde_json::from_str(&proxied.body).unwrap();
    assert_eq!(body["servers"][0]["url"], format!("wss://{base}"));
    assert_eq!(http(&base, "POST /servers", &[]).await.status, 405);

    socket
        .send(Message::text(r#"{"type":"leave","roundId":0}"#))
        .await
        .unwrap();
    while socket.next().await.is_some_and(|message| message.is_ok()) {}
    let closed = directory
        .wait_for(opened + 1, "the empty room's announcement", |body| {
            room_codes(body).is_empty()
        })
        .await;
    // Two keepalives of the unchanged rooms; load figures alone are no news.
    directory
        .wait_for(closed + 2, "a second keepalive", |_| true)
        .await;
    for pair in directory.received.lock().unwrap().windows(2) {
        if pair[0].body["rooms"] == pair[1].body["rooms"] {
            let gap = pair[1].at - pair[0].at;
            assert!(
                gap >= INTERVAL,
                "an unchanged listing went out after {gap:?}"
            );
        }
    }
    assert_eq!(
        lines_with(&lines, "server directory").len(),
        1,
        "only the first success is logged"
    );
    server.close().await;
}

async fn join(
    base: &str,
    code: &str,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://{base}/room/{code}"))
        .await
        .unwrap();
    let join = format!(
        r#"{{"type":"join","version":{PROTOCOL_VERSION},"contentVersion":"{CONTENT_VERSION}","name":"lister","kind":"balanced"}}"#
    );
    socket.send(Message::text(join)).await.unwrap();
    socket
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_room_opening_or_closing_is_announced_without_waiting_for_a_check() {
    let (directory, url) = FakeDirectory::start(204).await;
    // Only the first check would ever fire within the test.
    let hour = Duration::from_secs(3600);
    let (server, base, _) = start(&url, hour, hour).await;
    let first = directory
        .wait_for(0, "the first announcement", |_| true)
        .await;
    let mut socket = join(&base, "FASTROOM").await;
    let opened = directory
        .wait_for(first + 1, "the opened room", |body| {
            room_codes(body) == ["FASTROOM"]
        })
        .await;
    socket
        .send(Message::text(r#"{"type":"leave","roundId":0}"#))
        .await
        .unwrap();
    while socket.next().await.is_some_and(|message| message.is_ok()) {}
    directory
        .wait_for(opened + 1, "the closed room", |body| {
            room_codes(body).is_empty()
        })
        .await;
    server.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn logs_only_when_announcing_starts_failing_or_works_again() {
    let (directory, url) = FakeDirectory::start(503).await;
    let (server, _, lines) = start(&url, CHECK, Duration::from_millis(40)).await;
    // A failed announcement is retried at every check.
    directory.wait_for(3, "retries", |_| true).await;
    let failures = lines_with(&lines, "cannot announce to the server directory");
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(
        failures[0].ends_with(": HTTP 503 Service Unavailable"),
        "{}",
        failures[0]
    );
    directory.status.store(204, Ordering::Relaxed);
    let recovered = directory.count();
    directory
        .wait_for(recovered + 3, "keepalives after recovering", |_| true)
        .await;
    assert_eq!(
        lines_with(&lines, "announcing to the server directory").len(),
        1
    );
    assert_eq!(
        lines_with(&lines, "cannot announce to the server directory").len(),
        1
    );
    server.close().await;
}
