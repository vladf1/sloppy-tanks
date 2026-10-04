//! Socket policy and the 50 ms timer around one [`RoomHost`].
//!
//! A session is plain synchronous state: it never touches the network or a runtime
//! timer. The server's room task feeds it socket events and calls
//! [`RoomSession::on_timer`] at [`RoomSession::deadline`]; tests drive it with fake
//! sockets and a manual clock.

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use crate::host::{ConnectionId, HostAction, HostFactory, HostOptions, HostOutput, RoomHost};
use crate::protocol::{CONTENT_VERSION, MAX_CLIENT_MESSAGE_BYTES};
use crate::random;
use crate::room_list::{RoomListing, RoomPhase};
use crate::tcp_path::TcpReading;

/// Open sockets per room, joined or not.
pub const MAX_PENDING_CONNECTIONS: usize = 16;
/// A socket that has not been welcomed into a seat by then is closed.
pub const JOIN_TIMEOUT_MS: u64 = 5000;
/// Active rooms refresh their directory listing this often.
pub const DIRECTORY_HEARTBEAT_MS: u64 = 20_000;
pub const MAX_SOCKET_MESSAGES_PER_SECOND: u32 = 65;
/// The room timer's cadence (`HOST_INTERVAL_MS` in `fixed-step-clock.ts`): snapshots
/// go out at 20 Hz while the simulation steps at 60 Hz inside each callback.
pub const HOST_INTERVAL_MS: u64 = 50;

/// Message types counted by name; anything else counts as `other`, so clients cannot
/// grow the table.
const MESSAGE_TYPES: [&str; 19] = [
    // Server to client.
    "welcome",
    "lobby",
    "control",
    "full",
    "snapshot",
    "pong",
    "error",
    "room-reset",
    // Client to server.
    "join",
    "input",
    "ping",
    "choose",
    "settings",
    "start",
    "end",
    "suspend",
    "resume",
    "resync",
    "leave",
];
/// Characters searched for the type; server messages put the room identity before it.
const TYPE_SEARCH_CHARS: usize = 160;
const TYPE_KEY: &str = "\"type\":\"";
const WELCOME_PREFIX: &str = "{\"type\":\"welcome\"";

/// Messages per type. Sorted, so JSON output is stable.
pub type MessageCounts = BTreeMap<&'static str, u64>;

/// A message's type without parsing it: the first `"type":"name"` within the first 160
/// characters. Both sides write `type` before any nested object (server messages lead
/// with the room epoch and round), so the first match is the message's own.
pub fn message_type(text: &str) -> &'static str {
    let end = text
        .char_indices()
        .nth(TYPE_SEARCH_CHARS)
        .map_or(text.len(), |(index, _)| index);
    let head = &text[..end];
    let mut from = 0;
    while let Some(found) = head[from..].find(TYPE_KEY) {
        let start = from + found + TYPE_KEY.len();
        let rest = &head[start..];
        let name_len = rest
            .bytes()
            .take_while(|byte| byte.is_ascii_lowercase() || *byte == b'-')
            .count();
        if (1..=16).contains(&name_len) && rest.as_bytes().get(name_len) == Some(&b'"') {
            let name = &rest[..name_len];
            return MESSAGE_TYPES
                .iter()
                .find(|known| **known == name)
                .copied()
                .unwrap_or("other");
        }
        from = from + found + 1;
    }
    "other"
}

fn count(counts: &mut MessageCounts, text: &str) {
    *counts.entry(message_type(text)).or_default() += 1;
}

/// The socket could not take the message; the session closes it with 1011.
#[derive(Debug)]
pub struct SendFailed;

/// The part of a WebSocket that a room needs.
pub trait RoomSocket {
    /// Queues a text message. Sending to a socket that is already closing is ignored.
    fn send(&self, text: String) -> Result<(), SendFailed>;
    /// Starts the closing handshake. Later calls are ignored.
    fn close(&self, code: u16, reason: &str);
    /// The connection's TCP round trip and retransmissions so far, where the transport
    /// measures them.
    fn tcp(&self) -> Option<TcpReading> {
        None
    }
}

/// Seat and socket changes a runtime may log; they never affect room behaviour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoomActivity {
    Created,
    Joined {
        players: u32,
    },
    /// The client's transport closed; `code` is the close code it sent, 1005 without
    /// one, or 1006 when the connection dropped. `tcp` is the connection's final reading.
    Left {
        players: u32,
        code: Option<u16>,
        tcp: Option<TcpReading>,
    },
    /// The server closed the socket.
    Closed {
        players: u32,
        code: u16,
        reason: String,
        tcp: Option<TcpReading>,
    },
}

/// Where a session reports to the rest of the server.
pub trait RoomEvents: Send {
    /// A listing for the public room directory; forced on lobby changes, otherwise a
    /// heartbeat.
    fn listing(&mut self, entry: RoomListing);
    fn activity(&mut self, _event: RoomActivity) {}
    /// The match was disposed and every socket released; the runtime may forget this room.
    fn ended(&mut self, _reason: &str, _age_ms: u64) {}
}

/// Monotonic milliseconds. Only differences between readings matter.
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// A frame's payload as the room sees it.
pub enum Incoming<'a> {
    Text(&'a str),
    /// Binary messages are not part of the protocol; the socket is closed with 1008.
    Binary,
}

/// One room's state and load since the previous sample. Byte counts are UTF-8 lengths
/// before compression (the TypeScript counted UTF-16 units; both equal bytes for the
/// ASCII JSON the game sends).
#[derive(Clone, Debug, PartialEq)]
pub struct RoomSample {
    pub room: String,
    pub map_mode: String,
    pub phase: RoomPhase,
    pub players: u32,
    pub seats: u32,
    pub sockets: u32,
    pub time_left: u32,
    pub scores: [u32; 2],
    pub age_seconds: u64,
    pub tick: u64,
    pub debt_ms: f64,
    /// Timer callbacks since the previous sample, which weight `tick_avg_ms` over longer
    /// windows.
    pub ticks: u32,
    pub tick_avg_ms: f64,
    pub tick_max_ms: f64,
    pub sent_bytes: u64,
    pub received_bytes: u64,
    /// Messages sent and accepted per message type.
    pub sent_messages: MessageCounts,
    pub received_messages: MessageCounts,
    /// The lowest TCP round trip of each joined socket the transport measures, in
    /// milliseconds.
    pub rtt_ms: Vec<f64>,
    /// Data segments sent to those sockets since each connected, and how many were
    /// retransmissions.
    pub data_segments_sent: u64,
    pub retransmitted_segments: u64,
    /// Input lapses since the previous sample, and over the whole match.
    pub input_lapses: u64,
    pub match_input_lapses: u64,
}

struct SocketEntry<S> {
    socket: S,
    opened_ms: u64,
    joined: bool,
    window_ms: u64,
    messages: u32,
}

/// Connection ids are unique across the process, so a log or a host never confuses a
/// reconnecting socket with the one it replaced.
static NEXT_CONNECTION: AtomicU64 = AtomicU64::new(1);

#[derive(Default)]
struct Load {
    sent_bytes: u64,
    received_bytes: u64,
    ticks: u32,
    tick_total_ms: f64,
    tick_max_ms: f64,
    sent_messages: MessageCounts,
    received_messages: MessageCounts,
}

/// Socket limits, join timeout, directory publishing and the fixed-deadline timer for one
/// room code. Hosts come and go: a socket arriving after a match ended starts a new one.
pub struct RoomSession<F: HostFactory, S: RoomSocket> {
    room: String,
    factory: Arc<F>,
    clock: Clock,
    events: Box<dyn RoomEvents>,
    host: Option<F::Host>,
    host_created_ms: u64,
    /// Set when the host panicked even while being disposed and had to be dropped.
    failure: Option<&'static str>,
    sockets: BTreeMap<ConnectionId, SocketEntry<S>>,
    deadline_ms: Option<u64>,
    next_tick_ms: u64,
    last_listed_ms: u64,
    ended: bool,
    load: Load,
    /// The host's input lapses at the previous sample.
    sampled_lapses: u64,
}

impl<F: HostFactory, S: RoomSocket> RoomSession<F, S> {
    pub fn new(
        room: impl Into<String>,
        factory: Arc<F>,
        clock: Clock,
        events: Box<dyn RoomEvents>,
    ) -> Self {
        Self {
            room: room.into(),
            factory,
            clock,
            events,
            host: None,
            host_created_ms: 0,
            failure: None,
            sockets: BTreeMap::new(),
            deadline_ms: None,
            next_tick_ms: 0,
            last_listed_ms: 0,
            ended: false,
            load: Load::default(),
            sampled_lapses: 0,
        }
    }

    pub fn room(&self) -> &str {
        &self.room
    }

    pub fn connections(&self) -> usize {
        self.sockets.len()
    }

    pub fn is_full(&self) -> bool {
        self.sockets.len() >= MAX_PENDING_CONNECTIONS
    }

    /// When [`on_timer`](Self::on_timer) is due; `None` while no match is running.
    pub fn deadline(&self) -> Option<u64> {
        self.deadline_ms
    }

    /// True once after the room ended (every socket released); the runtime may then
    /// forget the session.
    pub fn take_ended(&mut self) -> bool {
        std::mem::take(&mut self.ended)
    }

    /// Registers an open socket; `None` means the caller should refuse it at the room limit.
    pub fn accept(&mut self, socket: S) -> Option<ConnectionId> {
        if self.is_full() {
            return None;
        }
        let now = (self.clock)();
        if !self.host_alive() {
            let content_version = CONTENT_VERSION.to_string();
            self.host = Some(self.factory.create(HostOptions {
                room_epoch: random::uuid_v4(),
                now_ms: now,
                seed: random::seed(),
                content_version,
                token: Box::new(random::token),
            }));
            self.failure = None;
            self.host_created_ms = now;
            self.sampled_lapses = 0;
            self.events.activity(RoomActivity::Created);
        }
        let id = ConnectionId(NEXT_CONNECTION.fetch_add(1, Ordering::Relaxed));
        self.sockets.insert(
            id,
            SocketEntry {
                socket,
                opened_ms: now,
                joined: false,
                window_ms: now,
                messages: 0,
            },
        );
        if self.deadline_ms.is_none() {
            self.schedule();
        }
        Some(id)
    }

    pub fn message(&mut self, id: ConnectionId, message: Incoming<'_>) {
        let now = (self.clock)();
        let Some(entry) = self.sockets.get_mut(&id) else {
            return;
        };
        if now.saturating_sub(entry.window_ms) >= 1000 {
            entry.window_ms = now;
            entry.messages = 0;
        }
        entry.messages += 1;
        let text = match message {
            Incoming::Text(text) if text.len() <= MAX_CLIENT_MESSAGE_BYTES => text,
            _ => {
                self.drop_socket(id, 1008, "Invalid message or rate");
                return;
            }
        };
        if entry.messages > MAX_SOCKET_MESSAGES_PER_SECOND {
            self.drop_socket(id, 1008, "Invalid message or rate");
            return;
        }
        self.load.received_bytes += text.len() as u64;
        count(&mut self.load.received_messages, text);
        self.with_host(|host, out| host.receive(id, text, now, out));
    }

    /// The transport closed; the seat stays reserved for the host's reconnect grace.
    pub fn closed(&mut self, id: ConnectionId, code: Option<u16>) {
        if let Some(entry) = self.forget(id)
            && entry.joined
        {
            let players = self.players();
            let tcp = entry.socket.tcp();
            self.events
                .activity(RoomActivity::Left { players, code, tcp });
        }
    }

    /// The transport failed (a protocol error or an I/O error).
    pub fn failed(&mut self, id: ConnectionId) {
        self.drop_socket(id, 1011, "Socket failed");
    }

    /// Ends the match now, telling joined players why, and releases every socket.
    pub fn reset(&mut self, reason: &str) {
        self.with_host(|host, out| host.dispose(reason, out));
        self.stop();
    }

    /// Current state plus load since the previous call; `None` when no match is live.
    pub fn sample(&mut self) -> Option<RoomSample> {
        if !self.host_alive() {
            return None;
        }
        let now = (self.clock)();
        let host = self.host.as_ref()?;
        let entry = host.directory_entry(&self.room);
        let load = std::mem::take(&mut self.load);
        let tcp: Vec<TcpReading> = self
            .sockets
            .values()
            .filter(|socket| socket.joined)
            .filter_map(|socket| socket.socket.tcp())
            .collect();
        let match_input_lapses = host.input_lapses();
        let input_lapses = match_input_lapses.saturating_sub(self.sampled_lapses);
        self.sampled_lapses = match_input_lapses;
        Some(RoomSample {
            room: self.room.clone(),
            map_mode: entry.map_mode,
            phase: entry.phase,
            players: entry.players,
            seats: entry.reserved,
            sockets: self.sockets.len() as u32,
            time_left: entry.time,
            scores: entry.scores,
            age_seconds: (now.saturating_sub(self.host_created_ms) + 500) / 1000,
            tick: host.tick(),
            debt_ms: host.debt_ms(),
            ticks: load.ticks,
            tick_avg_ms: if load.ticks > 0 {
                load.tick_total_ms / f64::from(load.ticks)
            } else {
                0.0
            },
            tick_max_ms: load.tick_max_ms,
            sent_bytes: load.sent_bytes,
            received_bytes: load.received_bytes,
            sent_messages: load.sent_messages,
            received_messages: load.received_messages,
            rtt_ms: tcp.iter().map(TcpReading::rtt_ms).collect(),
            data_segments_sent: tcp.iter().map(|reading| reading.data_segments_sent).sum(),
            retransmitted_segments: tcp
                .iter()
                .map(|reading| reading.retransmitted_segments)
                .sum(),
            input_lapses,
            match_input_lapses,
        })
    }

    /// The 50 ms timer callback: join timeouts, one host advance, the listing heartbeat.
    pub fn on_timer(&mut self) {
        self.deadline_ms = None;
        let now = (self.clock)();
        let expired: Vec<ConnectionId> = self
            .sockets
            .iter()
            .filter(|(_, entry)| {
                !entry.joined && now.saturating_sub(entry.opened_ms) >= JOIN_TIMEOUT_MS
            })
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            self.drop_socket(id, 1008, "Join timed out");
        }
        let started = Instant::now();
        self.with_host(|host, out| host.advance(now, out));
        self.publish_listing(false);
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        self.load.ticks += 1;
        self.load.tick_total_ms += elapsed;
        self.load.tick_max_ms = self.load.tick_max_ms.max(elapsed);
        if self.host_alive() {
            self.schedule();
        } else {
            self.stop();
        }
    }

    fn host_alive(&self) -> bool {
        self.host.as_ref().is_some_and(|host| !host.is_disposed())
    }

    fn players(&self) -> u32 {
        self.host.as_ref().map_or(0, RoomHost::connections)
    }

    /// Runs one host call, then applies what it asked for. A panic ends the match.
    fn with_host(&mut self, call: impl FnOnce(&mut F::Host, &mut HostOutput)) {
        let Some(host) = self.host.as_mut() else {
            return;
        };
        let mut out = HostOutput::default();
        let result = catch_unwind(AssertUnwindSafe(|| call(host, &mut out)));
        self.apply(out.take());
        if result.is_err() {
            self.host_failed();
        }
    }

    fn host_failed(&mut self) {
        eprintln!("Room {} simulation failed", self.room);
        let Some(host) = self.host.as_mut() else {
            return;
        };
        let mut out = HostOutput::default();
        let disposed = catch_unwind(AssertUnwindSafe(|| {
            host.dispose("simulation-error", &mut out)
        }));
        self.apply(out.take());
        if disposed.is_err() {
            // The host cannot even end cleanly: drop it and release its sockets here.
            self.host = None;
            self.failure = Some("simulation-error");
            self.stop();
        }
    }

    fn apply(&mut self, actions: Vec<HostAction>) {
        for action in actions {
            match action {
                HostAction::Send { connection, text } => {
                    let Some(entry) = self.sockets.get_mut(&connection) else {
                        continue;
                    };
                    let bytes = text.len() as u64;
                    let kind = message_type(&text);
                    let welcome = text.starts_with(WELCOME_PREFIX);
                    if entry.socket.send(text).is_err() {
                        self.drop_socket(connection, 1011, "Send failed");
                        continue;
                    }
                    self.load.sent_bytes += bytes;
                    *self.load.sent_messages.entry(kind).or_default() += 1;
                    if welcome {
                        entry.joined = true;
                        let players = self.players();
                        self.events.activity(RoomActivity::Joined { players });
                    }
                }
                HostAction::Close {
                    connection,
                    code,
                    reason,
                } => self.drop_socket(connection, code, &reason),
                HostAction::Changed => self.publish_listing(true),
            }
        }
    }

    fn publish_listing(&mut self, force: bool) {
        let Some(host) = self.host.as_ref() else {
            return;
        };
        let now = (self.clock)();
        if !force && now.saturating_sub(self.last_listed_ms) < DIRECTORY_HEARTBEAT_MS {
            return;
        }
        self.last_listed_ms = now;
        let entry = host.directory_entry(&self.room);
        self.events.listing(entry);
    }

    fn forget(&mut self, id: ConnectionId) -> Option<SocketEntry<S>> {
        let entry = self.sockets.remove(&id)?;
        let now = (self.clock)();
        self.with_host(|host, out| host.disconnect(id, now, out));
        Some(entry)
    }

    fn drop_socket(&mut self, id: ConnectionId, code: u16, reason: &str) {
        let Some(entry) = self.forget(id) else {
            return;
        };
        let players = self.players();
        self.events.activity(RoomActivity::Closed {
            players,
            code,
            reason: reason.to_string(),
            tcp: entry.socket.tcp(),
        });
        entry.socket.close(code, reason);
    }

    fn stop(&mut self) {
        self.deadline_ms = None;
        // Sockets that never joined have no seat to reset; close them rather than leak them.
        let remaining: Vec<ConnectionId> = self.sockets.keys().copied().collect();
        for id in remaining {
            self.drop_socket(id, 1012, "Room closed");
        }
        let reason = self
            .host
            .as_ref()
            .and_then(RoomHost::dispose_reason)
            .or(self.failure)
            .unwrap_or("closed")
            .to_string();
        let age = (self.clock)().saturating_sub(self.host_created_ms);
        self.events.ended(&reason, age);
        self.ended = true;
    }

    fn schedule(&mut self) {
        // Fixed deadlines keep snapshot batches at 20 Hz; chaining a full interval after
        // each callback would add simulation time and timer slop to every gap clients
        // must buffer.
        let now = (self.clock)();
        // A missed deadline re-anchors one full interval out: an immediate callback would
        // only resend the snapshot just broadcast. The host's fixed-step clock still owes
        // the late ticks.
        self.next_tick_ms += HOST_INTERVAL_MS;
        if self.next_tick_ms <= now || self.next_tick_ms > now + HOST_INTERVAL_MS {
            self.next_tick_ms = now + HOST_INTERVAL_MS;
        }
        self.deadline_ms = Some(self.next_tick_ms);
    }
}
