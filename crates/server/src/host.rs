//! The contract between the server's socket layer and a room's authority.
//!
//! [`crate::session::RoomSession`] owns sockets, limits and the 50 ms timer; a
//! [`RoomHost`] owns everything about the match: seats, tokens, the protocol, the
//! simulation and the room's own lifetime. The TypeScript server calls `MatchHost`
//! (`src/net/match-host.ts`) through exactly these operations;
//! [`crate::match_room::MatchRoom`] adapts core's port of it to this trait.

use sloppy_core::net::match_host::{ConnectionId, HostEvent, MatchHostOptions};
use sloppy_core::net::room_list::RoomListing;

use crate::protocol::Message;

/// Events a host produced during one call, applied in order after the call returns.
///
/// Deferring them (instead of the TypeScript's synchronous callbacks) means the host
/// is never re-entered while it runs: a `Close` it asks for reaches
/// [`RoomHost::disconnect`] only after the current call has finished.
#[derive(Debug, Default)]
pub struct HostOutput {
    events: Vec<HostEvent>,
}

impl HostOutput {
    /// Sends one text message to a connection. Unknown or closed connections are ignored.
    pub fn send(&mut self, connection: ConnectionId, text: impl Into<String>) {
        self.events.push(HostEvent::Send {
            connection,
            message: Message::Text(text.into()),
        });
    }

    /// Closes a connection with a WebSocket close code and reason. The session then
    /// forgets the socket and calls [`RoomHost::disconnect`] for it.
    pub fn close(&mut self, connection: ConnectionId, code: u16, reason: impl Into<String>) {
        self.events.push(HostEvent::Close {
            connection,
            code,
            reason: reason.into(),
        });
    }

    pub fn extend(&mut self, events: Vec<HostEvent>) {
        self.events.extend(events);
    }

    pub fn take(&mut self) -> Vec<HostEvent> {
        std::mem::take(&mut self.events)
    }
}

/// One room's authority, driven by [`crate::session::RoomSession`].
///
/// # Threading
///
/// Each room runs on its own Tokio task on a multi-threaded runtime, so a host must be
/// `Send` (a Rapier world is). It is only ever used from one task at a time and is never
/// shared, so it needs neither `Sync` nor internal locking. Calls run synchronously on a
/// runtime worker thread: a host must not block on I/O, and a busy match should keep
/// [`advance`](RoomHost::advance) well inside the 50 ms interval (a slow tick delays only
/// that room's next tick on a multi-core host, and everything on a one-core host).
///
/// # Time
///
/// `now_ms` is the session's monotonic millisecond clock (whole milliseconds, like
/// `Date.now()`), starting from [`MatchHostOptions::now_ms`]. It never goes backwards.
///
/// # Failure
///
/// A host reports bad client input by sending an `error` message and/or a `Close`
/// action, as `MatchHost` does. A panic inside any call is caught by the session, logged
/// as `Room simulation failed`, and ends the room with the reason `simulation-error`
/// (the server profile keeps unwinding for this).
///
/// # Lifetime rules the host owns
///
/// The session never expires a room itself. Like `MatchHost.advance`, the host disposes
/// itself with reason `expired` when it has had no connections for
/// [`EMPTY_GRACE_MS`](crate::protocol::EMPTY_GRACE_MS), when an idle lobby or results
/// screen passes [`ROOM_IDLE_MS`](crate::protocol::ROOM_IDLE_MS), or when the room passes
/// [`MAX_ROOM_MS`](crate::protocol::MAX_ROOM_MS) (plus
/// [`MAX_BATTLE_OVERRUN_MS`](crate::protocol::MAX_BATTLE_OVERRUN_MS) while a battle is
/// under way); it releases a disconnected seat after `EMPTY_GRACE_MS`, and disposes with
/// `empty` when the last seat explicitly leaves. The session stops the room on its next
/// tick after [`is_disposed`](RoomHost::is_disposed) turns true.
pub trait RoomHost: Send + 'static {
    /// A text message from a connection, already checked by the session for size
    /// (4096 UTF-8 bytes), rate (65 per second per socket) and UTF-8 validity. The first
    /// message of a connection should be a `join`; the host answers it with a `welcome`
    /// message whose text starts with `{"type":"welcome"` (the session marks the socket
    /// joined, exempting it from the join timeout, when it sends such a message).
    ///
    /// A disposed host must answer with a fatal `expired` error and a close.
    fn receive(&mut self, connection: ConnectionId, text: &str, now_ms: u64, out: &mut HostOutput);

    /// The connection is gone (closed by the client, closed by the server, or failed).
    /// Its seat, if any, stays reserved for the reconnect grace. Must ignore connections
    /// it does not know, because the session also calls it for sockets that never joined
    /// and for connections the host itself closed.
    fn disconnect(&mut self, connection: ConnectionId, now_ms: u64, out: &mut HostOutput);

    /// The 50 ms timer: expire the room or seats, drop connections that stopped
    /// consuming state, step the simulation by the elapsed fixed steps, and send each
    /// connection its snapshot batch.
    fn advance(&mut self, now_ms: u64, out: &mut HostOutput);

    /// Ends the match now: send every connection `room-reset` with `reason` and close it
    /// with code 1012, free the simulation, and report [`HostEvent::Changed`]. Calling
    /// it again does nothing.
    fn dispose(&mut self, reason: &str, out: &mut HostOutput);

    fn is_disposed(&self) -> bool;

    /// Why the room ended (`empty`, `expired`, `overload`, `server-restart`, ...), for the
    /// room's final log line; `None` until disposed.
    fn dispose_reason(&self) -> Option<&str>;

    /// The room's public listing. It must stay valid after disposal, with `players: 0`,
    /// so the directory drops the room.
    fn directory_entry(&self, room: &str) -> RoomListing;

    /// Connections that hold a seat (joined and not disconnected): the "players" count
    /// in room lifecycle log lines.
    fn connections(&self) -> u32;

    /// Simulation ticks run in the current round, for monitoring.
    fn tick(&self) -> u64 {
        0
    }

    /// Elapsed time the fixed-step clock still owes the simulation, in milliseconds. A
    /// value that keeps rising means the room is falling behind real time.
    fn debt_ms(&self) -> f64 {
        0.0
    }

    /// Held movement or fire that ran out before the player's next input arrived, over
    /// this match so far, for monitoring.
    fn input_lapses(&self) -> u64 {
        0
    }
}

/// Creates a fresh host whenever a room code gets its first socket, or a socket arrives
/// after the previous match in that room ended.
pub trait HostFactory: Send + Sync + 'static {
    type Host: RoomHost;
    fn create(&self, options: MatchHostOptions) -> Self::Host;
}

impl<F, H> HostFactory for F
where
    F: Fn(MatchHostOptions) -> H + Send + Sync + 'static,
    H: RoomHost,
{
    type Host = H;
    fn create(&self, options: MatchHostOptions) -> H {
        self(options)
    }
}
