//! The real room host behind the server (`tests/node-server.test.ts` and
//! `tests/room-session.test.ts` parts that need a match): a created room starts its battle,
//! streams baselines and snapshots, acknowledges input, lists itself as playing, and a
//! dropped player rejoins the same seat.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use sloppy_core::net::wire_view::WireView;
use sloppy_server::match_room::MatchRoom;
use sloppy_server::protocol::{CONTENT_VERSION, PROTOCOL_VERSION};
use sloppy_server::server::{MultiplayerServer, ServerOptions};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

const ORIGIN: &str = "http://127.0.0.1:5173";
const WAIT: Duration = Duration::from_secs(10);

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn start() -> (MultiplayerServer, String) {
    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let mut options = ServerOptions::new(vec![ORIGIN.to_string()], true);
    let sink = lines.clone();
    options.log = Arc::new(move |line| sink.lock().unwrap().push(line.to_string()));
    let server = MultiplayerServer::listen(options, MatchRoom::new, "127.0.0.1:0")
        .await
        .unwrap();
    let base = server.local_addr().to_string();
    (server, base)
}

async fn connect(base: &str, room: &str) -> Socket {
    let mut request = format!("ws://{base}/room/{room}")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("Origin", HeaderValue::from_static(ORIGIN));
    let (socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    socket
}

struct Player {
    socket: Socket,
    /// Text messages, and binary state as the former JSON.
    messages: Vec<Value>,
    view: WireView,
}

impl Player {
    async fn send(&mut self, value: Value) {
        self.socket
            .send(Message::Text(value.to_string().into()))
            .await
            .unwrap();
    }

    /// Reads until `check` accepts a received message, returning it.
    async fn until(&mut self, what: &str, check: impl Fn(&Value) -> bool) -> Value {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            if let Some(found) = self.messages.iter().rev().find(|message| check(message)) {
                return found.clone();
            }
            let next = tokio::time::timeout_at(deadline, self.socket.next())
                .await
                .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
            match next {
                Some(Ok(Message::Text(text))) => {
                    self.messages.push(serde_json::from_str(&text).unwrap());
                }
                Some(Ok(Message::Binary(bytes))) => {
                    self.messages.push(self.view.binary(&bytes).unwrap());
                }
                Some(Ok(Message::Close(frame))) => panic!("closed before {what}: {frame:?}"),
                Some(Ok(_)) => {}
                other => panic!("socket ended before {what}: {other:?}"),
            }
        }
    }

    async fn next(&mut self, kind: &str) -> Value {
        self.until(kind, |message| message["type"] == kind).await
    }
}

fn join(name: &str, extra: Value) -> Value {
    let mut join = json!({
        "type": "join",
        "version": PROTOCOL_VERSION,
        "contentVersion": CONTENT_VERSION,
        "name": name,
        "kind": "balanced",
    });
    for (key, value) in extra.as_object().unwrap() {
        join[key] = value.clone();
    }
    join
}

async fn rooms(base: &str) -> Vec<Value> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = TcpStream::connect(base).await.unwrap();
    let request = format!(
        "GET /rooms HTTP/1.1\r\nHost: {base}\r\nOrigin: {ORIGIN}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).await.unwrap();
    let body = raw.split_once("\r\n\r\n").unwrap().1;
    let value: Value = serde_json::from_str(body).unwrap();
    value["rooms"].as_array().unwrap().clone()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_created_room_plays_streams_acks_input_and_keeps_a_dropped_seat() {
    let (server, base) = start().await;
    let mut alice = Player {
        socket: connect(&base, "MATCHRM2").await,
        messages: Vec::new(),
        view: WireView::default(),
    };
    alice
        .send(join(
            "alice",
            json!({ "create": { "mapMode": "village", "difficulty": "easy", "humansOnly": false } }),
        ))
        .await;
    let welcome = alice.next("welcome").await;
    let epoch = welcome["roomEpoch"].as_str().unwrap().to_string();
    let control = alice
        .until("the round's control", |message| {
            message["type"] == "control" && message["roundId"] == 1
        })
        .await;
    let full = alice
        .until("the round's baseline", |message| {
            message["type"] == "full" && message["roundId"] == 1
        })
        .await;
    assert_eq!(full["roomEpoch"], epoch.as_str());
    assert_eq!(
        full["state"]["entities"]["tanks"].as_array().unwrap().len(),
        12
    );
    alice.next("snapshot").await;

    let mut bob = Player {
        socket: connect(&base, "MATCHRM2").await,
        messages: Vec::new(),
        view: WireView::default(),
    };
    bob.send(join("bob", json!({ "existingRoom": true }))).await;
    let bob_welcome = bob.next("welcome").await;
    let bob_control = bob.next("control").await;
    bob.next("full").await;
    assert_eq!(bob_welcome["hostId"], welcome["playerId"]);

    // Drive for a second at the browser's 20 Hz and wait for the ack.
    for seq in 1..=20 {
        let tick = alice
            .messages
            .iter()
            .rev()
            .find(|message| message["type"] == "snapshot")
            .and_then(|message| message["snapshots"].as_array()?.last()?["tick"].as_u64())
            .unwrap_or(0);
        alice
            .send(json!({
                "type": "input", "roundId": 1, "controlEpoch": control["controlEpoch"],
                "moveX": 0, "moveZ": 1, "seq": seq, "observedTick": tick,
                "aim": { "angle": 0.5 }, "fire": true,
            }))
            .await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        alice
            .send(json!({ "type": "ping", "roundId": 1, "t": seq * 50, "observedTick": tick }))
            .await;
    }
    let acked = alice
        .until("an acknowledged input", |message| {
            message["type"] == "snapshot" && message["ack"].as_u64().is_some_and(|ack| ack > 0)
        })
        .await;
    assert!(
        acked["snapshots"]
            .as_array()
            .is_some_and(|frames| !frames.is_empty())
    );
    alice.next("pong").await;

    let listed = rooms(&base).await;
    let room = listed
        .iter()
        .find(|room| room["room"] == "MATCHRM2")
        .expect("the room is listed");
    assert_eq!(room["phase"], "playing");
    assert_eq!(room["players"], 2);

    // Bob's network drops; his seat waits for him.
    let token = bob_welcome["token"].clone();
    drop(bob);
    alice
        .until("bob shown as disconnected", |message| {
            message["type"] == "lobby"
                && message["players"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|player| player["name"] == "bob" && player["connected"] == false)
        })
        .await;
    let mut back = Player {
        socket: connect(&base, "MATCHRM2").await,
        messages: Vec::new(),
        view: WireView::default(),
    };
    back.send(join(
        "bob",
        json!({ "token": token, "roomEpoch": epoch, "existingRoom": true }),
    ))
    .await;
    let again = back.next("welcome").await;
    assert_eq!(
        again["playerId"], bob_welcome["playerId"],
        "the seat was kept"
    );
    assert_eq!(again["reset"], false);
    let control = back.next("control").await;
    assert_eq!(control["tankId"], bob_control["tankId"]);
    assert_eq!(control["driver"], "human");
    back.next("full").await;
    back.next("snapshot").await;
    server.close().await;
}
