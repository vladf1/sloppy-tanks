//! `tests/room-session.test.ts`, driven with fake sockets, a manual clock and the real
//! host in its lobby, which builds no simulation.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::Value;
use sloppy_core::net::match_host::{ConnectionId, MatchHostOptions};
use sloppy_core::net::room_list::RoomListing;

use crate::host::{HostOutput, RoomHost};
use crate::match_room::MatchRoom;
use crate::protocol::{CONTENT_VERSION, Message, PROTOCOL_VERSION};
use crate::session::*;
use crate::tcp_path::TcpReading;

#[derive(Default)]
struct SocketLog {
    sent: Vec<Value>,
    closed: Option<(u16, String)>,
    tcp: Option<TcpReading>,
}

#[derive(Clone, Default)]
struct FakeSocket(Rc<RefCell<SocketLog>>);

impl FakeSocket {
    fn sent_type(&self, index: usize) -> String {
        self.0.borrow().sent[index]["type"]
            .as_str()
            .unwrap_or("")
            .to_string()
    }
    fn last(&self) -> Value {
        self.0.borrow().sent.last().cloned().unwrap_or(Value::Null)
    }
    fn closed(&self) -> Option<(u16, String)> {
        self.0.borrow().closed.clone()
    }
    fn closed_code(&self) -> Option<u16> {
        self.closed().map(|(code, _)| code)
    }
}

impl RoomSocket for FakeSocket {
    fn send(&self, message: Message) {
        let Message::Text(text) = message else {
            panic!("a lobby sends only text");
        };
        self.0
            .borrow_mut()
            .sent
            .push(serde_json::from_str(&text).expect("server messages are JSON"));
    }
    fn close(&self, code: u16, reason: &str) {
        self.0
            .borrow_mut()
            .closed
            .get_or_insert((code, reason.to_string()));
    }
    fn tcp(&self) -> Option<TcpReading> {
        self.0.borrow().tcp
    }
}

#[derive(Default)]
struct Recorded {
    listings: Vec<RoomListing>,
    activity: Vec<RoomActivity>,
    ended: Vec<(String, u64)>,
}

struct Recorder(Arc<Mutex<Recorded>>);

impl RoomEvents for Recorder {
    fn listing(&mut self, entry: RoomListing) {
        self.0.lock().unwrap().listings.push(entry);
    }
    fn activity(&mut self, event: RoomActivity) {
        self.0.lock().unwrap().activity.push(event);
    }
    fn ended(&mut self, reason: &str, age_ms: u64) {
        self.0
            .lock()
            .unwrap()
            .ended
            .push((reason.to_string(), age_ms));
    }
}

type LobbySession = RoomSession<fn(MatchHostOptions) -> MatchRoom, FakeSocket>;

struct Harness {
    room: LobbySession,
    now: Arc<AtomicU64>,
    events: Arc<Mutex<Recorded>>,
}

impl Harness {
    fn new() -> Self {
        let now = Arc::new(AtomicU64::new(1_000_000));
        let reader = now.clone();
        let events = Arc::new(Mutex::new(Recorded::default()));
        let factory: fn(MatchHostOptions) -> MatchRoom = MatchRoom::new;
        let room = RoomSession::new(
            "ABCDEFGH",
            Arc::new(factory),
            Arc::new(move || reader.load(Ordering::Relaxed)),
            Box::new(Recorder(events.clone())),
        );
        Self { room, now, events }
    }

    /// Advances the manual clock, running every timer callback that falls due.
    fn tick(&mut self, ms: u64) {
        let target = self.now.load(Ordering::Relaxed) + ms;
        while let Some(deadline) = self.room.deadline().filter(|deadline| *deadline <= target) {
            self.now.store(deadline, Ordering::Relaxed);
            self.room.on_timer();
        }
        self.now.store(target, Ordering::Relaxed);
    }

    fn open(&mut self) -> (FakeSocket, ConnectionId) {
        let socket = FakeSocket::default();
        let id = self.room.accept(socket.clone()).expect("room has space");
        (socket, id)
    }

    fn send(&mut self, id: ConnectionId, text: &str) {
        self.room.message(id, Incoming::Text(text));
    }
}

fn join(name: &str) -> String {
    serde_json::json!({
        "type": "join",
        "version": PROTOCOL_VERSION,
        "contentVersion": CONTENT_VERSION,
        "name": name,
        "kind": "balanced",
    })
    .to_string()
}

#[test]
fn message_types_are_read_without_trusting_client_names() {
    // Server messages lead with the room identity; nested objects come after the type.
    let lobby = r#"{"roomEpoch":"e","roundId":2,"type":"lobby","players":[{"type":"x"}]}"#;
    assert_eq!(message_type(lobby), "lobby");
    assert_eq!(
        message_type(r#"{"type":"snapshot","roundId":1}"#),
        "snapshot"
    );
    assert_eq!(message_type(r#"{"type":"made-up"}"#), "other");
    assert_eq!(
        message_type(r#"{"type":"Upper","x":{"type":"ping"}}"#),
        "ping",
        "later matches count"
    );
    assert_eq!(message_type("not json"), "other");
    let late = format!(r#"{{"pad":"{}","type":"ping"}}"#, "x".repeat(160));
    assert_eq!(
        message_type(&late),
        "other",
        "only the first 160 characters are searched"
    );
}

#[test]
fn counts_messages_by_type_and_starts_each_sample_from_zero() {
    let mut harness = Harness::new();
    let (_, id) = harness.open();
    harness.send(id, &join("player"));
    harness.send(id, r#"{"type":"made-up","roundId":0}"#);
    let sample = harness.room.sample().unwrap();
    assert_eq!(sample.received_messages, [("join", 1), ("other", 1)].into());
    assert_eq!(sample.sent_messages.get("welcome"), Some(&1));
    assert!(harness.room.sample().unwrap().received_messages.is_empty());
    harness.room.reset("test");
}

#[test]
fn samples_the_tcp_figures_of_joined_sockets_and_logs_each_on_leaving() {
    let mut harness = Harness::new();
    let reading = |rtt_us, sent, retransmitted| {
        Some(TcpReading {
            rtt_us: Some(rtt_us),
            data_segments_sent: sent,
            retransmitted_segments: retransmitted,
        })
    };
    let (near, near_id) = harness.open();
    let (far, far_id) = harness.open();
    let (fresh, fresh_id) = harness.open();
    let (waiting, _) = harness.open();
    near.0.borrow_mut().tcp = reading(20_000, 900, 1);
    far.0.borrow_mut().tcp = reading(150_000, 100, 4);
    // Read before the kernel timed a round trip: its segments count, its latency does not.
    fresh.0.borrow_mut().tcp = Some(TcpReading {
        rtt_us: None,
        data_segments_sent: 10,
        retransmitted_segments: 1,
    });
    waiting.0.borrow_mut().tcp = reading(999_000, 50, 50);
    harness.send(near_id, &join("near"));
    harness.send(far_id, &join("far"));
    harness.send(fresh_id, &join("fresh"));
    let sample = harness.room.sample().unwrap();
    assert_eq!(
        sample.rtt_ms,
        [20.0, 150.0],
        "only measured, seated players count"
    );
    assert_eq!(sample.data_segments_sent, 1010);
    assert_eq!(sample.retransmitted_segments, 6);
    assert_eq!((sample.input_lapses, sample.match_input_lapses), (0, 0));
    harness.room.closed(far_id, 1001);
    assert!(
        harness
            .events
            .lock()
            .unwrap()
            .activity
            .contains(&RoomActivity::Left {
                players: 2,
                code: 1001,
                tcp: reading(150_000, 100, 4),
            })
    );
    harness.room.reset("test");
}

#[test]
fn refuses_sockets_past_the_pending_connection_cap() {
    let mut harness = Harness::new();
    for _ in 0..MAX_PENDING_CONNECTIONS {
        assert!(harness.room.accept(FakeSocket::default()).is_some());
    }
    assert_eq!(harness.room.connections(), MAX_PENDING_CONNECTIONS);
    assert!(harness.room.accept(FakeSocket::default()).is_none());
    harness.room.reset("test");
}

#[test]
fn closes_sockets_that_send_binary_oversized_or_too_many_messages() {
    let mut harness = Harness::new();
    let (binary, binary_id) = harness.open();
    let (oversized, oversized_id) = harness.open();
    let (flooding, flooding_id) = harness.open();
    harness.room.message(binary_id, Incoming::Binary);
    harness.send(oversized_id, &"x".repeat(5000));
    harness.send(flooding_id, &join("flood"));
    for index in 0..MAX_SOCKET_MESSAGES_PER_SECOND {
        harness.send(
            flooding_id,
            &format!(r#"{{"type":"ping","roundId":0,"t":{index},"observedTick":0}}"#),
        );
    }
    for socket in [&binary, &oversized, &flooding] {
        assert_eq!(socket.closed_code(), Some(1008));
    }
    assert_eq!(harness.room.connections(), 0);
    harness.room.reset("test");
}

#[test]
fn a_new_second_restores_a_sockets_message_budget() {
    let mut harness = Harness::new();
    let (socket, id) = harness.open();
    harness.send(id, &join("steady"));
    for second in 0..3 {
        for index in 0..60 {
            harness.send(
                id,
                &format!(r#"{{"type":"ping","roundId":0,"t":{index},"observedTick":0}}"#),
            );
        }
        assert_eq!(socket.closed(), None, "second {second}");
        harness.tick(1000);
    }
    harness.room.reset("test");
}

#[test]
fn times_out_sockets_that_never_join_but_keeps_joined_ones() {
    let mut harness = Harness::new();
    let (idle, _) = harness.open();
    let (player, player_id) = harness.open();
    harness.send(player_id, &join("player"));
    assert_eq!(player.sent_type(0), "welcome");
    harness.tick(JOIN_TIMEOUT_MS + 50);
    assert_eq!(idle.closed(), Some((1008, "Join timed out".into())));
    assert_eq!(player.closed(), None);
    assert_eq!(harness.room.connections(), 1);
    let activity = harness.events.lock().unwrap().activity.clone();
    assert!(activity.contains(&RoomActivity::Joined { players: 1 }));
    assert!(activity.contains(&RoomActivity::Closed {
        players: 1,
        code: 1008,
        reason: "Join timed out".into(),
        tcp: None,
    }));
    harness.room.reset("test");
}

#[test]
fn lists_lobby_changes_at_once_and_otherwise_on_a_heartbeat() {
    let mut harness = Harness::new();
    let (_, id) = harness.open();
    harness.send(id, &join("player"));
    let joined = harness.events.lock().unwrap().listings.len();
    assert!(joined > 0);
    {
        let events = harness.events.lock().unwrap();
        let last = events.listings.last().unwrap();
        assert_eq!((last.players, last.room.as_str()), (1, "ABCDEFGH"));
    }
    harness.tick(DIRECTORY_HEARTBEAT_MS / 2);
    assert_eq!(harness.events.lock().unwrap().listings.len(), joined);
    harness.tick(DIRECTORY_HEARTBEAT_MS / 2 + 100);
    assert_eq!(harness.events.lock().unwrap().listings.len(), joined + 1);
    harness.room.reset("test");
}

#[test]
fn reset_tells_players_why_and_releases_every_socket() {
    let mut harness = Harness::new();
    let (player, player_id) = harness.open();
    let (pending, _) = harness.open();
    harness.send(player_id, &join("player"));
    harness.room.reset("server-restart");
    assert_eq!(player.last()["type"], "room-reset");
    assert_eq!(player.last()["reason"], "server-restart");
    assert_eq!(player.closed_code(), Some(1012));
    assert_eq!(pending.closed(), Some((1012, "Room closed".into())));
    let events = harness.events.lock().unwrap();
    assert_eq!(events.listings.last().unwrap().players, 0);
    assert_eq!(events.ended.len(), 1);
    assert_eq!(events.ended[0].0, "server-restart");
    drop(events);
    assert_eq!(harness.room.connections(), 0);
    assert!(harness.room.take_ended());
    assert!(!harness.room.take_ended(), "reported once");
}

#[test]
fn ends_after_the_last_player_leaves_and_can_host_a_fresh_match() {
    let mut harness = Harness::new();
    let (player, id) = harness.open();
    harness.send(id, &join("player"));
    harness.send(id, r#"{"type":"leave","roundId":0}"#);
    assert_eq!(player.closed(), Some((1000, "Left room".into())));
    harness.tick(100);
    // The room stops on its next timer callback, one interval after it opened.
    assert_eq!(
        harness.events.lock().unwrap().ended,
        [("empty".to_string(), HOST_INTERVAL_MS)]
    );
    assert!(
        harness.room.deadline().is_none(),
        "the timer stops with the match"
    );
    let (next, next_id) = harness.open();
    harness.send(next_id, &join("next"));
    assert_eq!(next.sent_type(0), "welcome");
    harness.room.reset("test");
}

#[test]
fn a_dropped_connection_keeps_its_seat_through_the_grace_then_the_room_expires() {
    let mut harness = Harness::new();
    let (_, id) = harness.open();
    harness.send(id, &join("player"));
    harness.room.closed(id, 1006);
    let left = RoomActivity::Left {
        players: 0,
        code: 1006,
        tcp: None,
    };
    assert!(harness.events.lock().unwrap().activity.contains(&left));
    assert_eq!(
        harness.room.sample().unwrap().seats,
        1,
        "the seat waits for a reconnect"
    );
    harness.tick(crate::protocol::EMPTY_GRACE_MS + 100);
    assert_eq!(harness.events.lock().unwrap().ended[0].0, "expired");
    assert!(harness.room.sample().is_none());
}

#[test]
fn keeps_fixed_deadlines_and_reanchors_after_a_late_callback() {
    let mut harness = Harness::new();
    let (_, id) = harness.open();
    harness.send(id, &join("player"));
    let first = harness.room.deadline().unwrap();
    assert_eq!(first, 1_000_000 + HOST_INTERVAL_MS);
    // A callback that runs 10 ms late keeps the next deadline on the 50 ms grid.
    harness.now.store(first + 10, Ordering::Relaxed);
    harness.room.on_timer();
    assert_eq!(harness.room.deadline(), Some(first + HOST_INTERVAL_MS));
    // One that misses a whole interval re-anchors a full interval out.
    let late = first + 3 * HOST_INTERVAL_MS + 7;
    harness.now.store(late, Ordering::Relaxed);
    harness.room.on_timer();
    assert_eq!(harness.room.deadline(), Some(late + HOST_INTERVAL_MS));
    assert_eq!(harness.room.sample().unwrap().ticks, 2);
    harness.room.reset("test");
}

/// A host that panics in `advance`, to show a room failure stays inside its room.
struct PanickingHost(MatchRoom, bool);

impl RoomHost for PanickingHost {
    fn receive(&mut self, connection: ConnectionId, text: &str, now_ms: u64, out: &mut HostOutput) {
        self.0.receive(connection, text, now_ms, out);
    }
    fn disconnect(&mut self, connection: ConnectionId, now_ms: u64, out: &mut HostOutput) {
        self.0.disconnect(connection, now_ms, out);
    }
    fn advance(&mut self, _now_ms: u64, _out: &mut HostOutput) {
        panic!("simulated physics failure");
    }
    fn dispose(&mut self, reason: &str, out: &mut HostOutput) {
        if self.1 {
            panic!("cannot even dispose");
        }
        self.0.dispose(reason, out);
    }
    fn is_disposed(&self) -> bool {
        self.0.is_disposed()
    }
    fn dispose_reason(&self) -> Option<&str> {
        self.0.dispose_reason()
    }
    fn directory_entry(&self, room: &str) -> RoomListing {
        self.0.directory_entry(room)
    }
    fn connections(&self) -> u32 {
        self.0.connections()
    }
}

#[test]
fn a_panicking_host_ends_its_room_with_simulation_error() {
    for broken_dispose in [false, true] {
        let now = Arc::new(AtomicU64::new(0));
        let reader = now.clone();
        let events = Arc::new(Mutex::new(Recorded::default()));
        let factory =
            move |options: MatchHostOptions| PanickingHost(MatchRoom::new(options), broken_dispose);
        let mut room: RoomSession<_, FakeSocket> = RoomSession::new(
            "PANICKED",
            Arc::new(factory),
            Arc::new(move || reader.load(Ordering::Relaxed)),
            Box::new(Recorder(events.clone())),
        );
        let player = FakeSocket::default();
        let id = room.accept(player.clone()).unwrap();
        room.message(id, Incoming::Text(&join("player")));
        now.store(room.deadline().unwrap(), Ordering::Relaxed);
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        room.on_timer();
        std::panic::set_hook(hook);
        assert_eq!(
            player.closed_code(),
            Some(1012),
            "broken dispose: {broken_dispose}"
        );
        assert_eq!(events.lock().unwrap().ended[0].0, "simulation-error");
        assert!(room.take_ended());
        assert!(room.deadline().is_none());
    }
}
