//! One Tokio task per room: it owns the [`RoomSession`] (and so the host), receives
//! socket events and runs the session's timer on the 50 ms cadence.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sloppy_core::net::match_host::ConnectionId;
use sloppy_core::net::room_list::RoomListing;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

use crate::host::HostFactory;
use crate::monitor::ServerMonitor;
use crate::room_catalog::RoomCatalog;
use crate::session::{Clock, Incoming, RoomActivity, RoomEvents, RoomSample, RoomSession};
use crate::socket::{SocketHandle, sleep_until_some};

/// Commands queued for one room. The queue is bounded, so a flooding client slows its
/// own socket's reads instead of growing server memory.
pub const ROOM_INBOX: usize = 1024;

pub enum RoomCommand {
    Open {
        socket: SocketHandle,
        reply: oneshot::Sender<Admission>,
    },
    Text {
        id: ConnectionId,
        text: String,
    },
    Binary {
        id: ConnectionId,
    },
    /// The transport closed with this code (1005 without one, 1006 when it dropped).
    Closed {
        id: ConnectionId,
        code: u16,
    },
    Failed {
        id: ConnectionId,
    },
    /// Ends the match (`server-restart` on shutdown); answered once the room is gone.
    Reset {
        reason: String,
        done: oneshot::Sender<()>,
    },
    Sample {
        reply: oneshot::Sender<Option<RoomSample>>,
    },
}

pub enum Admission {
    Accepted(ConnectionId),
    /// The room is at its socket limit.
    Full,
    /// The room ended before it could take the socket; try the room code again.
    Ended,
}

/// The registry's view of a running room.
#[derive(Clone)]
pub struct RoomHandle {
    /// Distinguishes a room from a later one with the same code; also the creation order.
    pub instance: u64,
    pub sender: mpsc::Sender<RoomCommand>,
    /// Open sockets, for refusing upgrades to a full room before the handshake.
    pub connections: Arc<AtomicUsize>,
}

pub type Registry = Mutex<HashMap<String, RoomHandle>>;

/// Where a room's listings and lifecycle events go.
pub struct ServerRoomEvents {
    pub room: String,
    pub catalog: Arc<Mutex<RoomCatalog>>,
    pub monitor: Arc<Mutex<ServerMonitor>>,
    pub clock: Clock,
}

impl RoomEvents for ServerRoomEvents {
    fn listing(&mut self, entry: RoomListing) {
        let now = (self.clock)();
        self.catalog.lock().expect("catalog").update(entry, now);
    }

    fn activity(&mut self, event: RoomActivity) {
        self.monitor
            .lock()
            .expect("monitor")
            .activity(&self.room, &event);
    }

    fn ended(&mut self, reason: &str, age_ms: u64) {
        self.monitor
            .lock()
            .expect("monitor")
            .ended(&self.room, reason, age_ms);
    }
}

/// Starts a room task; the caller inserts the handle into `registry`.
pub fn spawn_room<F: HostFactory>(
    code: String,
    instance: u64,
    factory: Arc<F>,
    clock: Clock,
    epoch: Instant,
    events: ServerRoomEvents,
    registry: Arc<Registry>,
) -> RoomHandle {
    let (sender, inbox) = mpsc::channel(ROOM_INBOX);
    let connections = Arc::new(AtomicUsize::new(0));
    let session = RoomSession::new(code.clone(), factory, clock, Box::new(events));
    tokio::spawn(run(
        code,
        instance,
        session,
        inbox,
        registry,
        connections.clone(),
        epoch,
    ));
    RoomHandle {
        instance,
        sender,
        connections,
    }
}

async fn run<F: HostFactory>(
    code: String,
    instance: u64,
    mut session: RoomSession<F, SocketHandle>,
    mut inbox: mpsc::Receiver<RoomCommand>,
    registry: Arc<Registry>,
    connections: Arc<AtomicUsize>,
    epoch: Instant,
) {
    let mut resets = Vec::new();
    loop {
        let deadline = session
            .deadline()
            .map(|ms| epoch + Duration::from_millis(ms));
        tokio::select! {
            // The timer goes first so a burst of messages cannot starve the simulation.
            biased;
            () = sleep_until_some(deadline) => session.on_timer(),
            command = inbox.recv() => match command {
                Some(command) => handle(&mut session, command, &mut resets),
                // Every sender is gone, which the registry prevents while the room lives.
                None => break,
            },
        }
        connections.store(session.connections(), Ordering::Relaxed);
        if session.take_ended() {
            break;
        }
        for done in resets.drain(..) {
            let _: Result<(), ()> = done.send(());
        }
    }
    // Leave the registry and close the inbox under its lock, so no socket can queue an
    // Open after this point; the ones already queued go back to try the code again.
    {
        let mut rooms = registry.lock().expect("room registry");
        if rooms
            .get(&code)
            .is_some_and(|room| room.instance == instance)
        {
            rooms.remove(&code);
        }
        inbox.close();
    }
    while let Ok(command) = inbox.try_recv() {
        match command {
            RoomCommand::Open { reply, .. } => {
                let _ = reply.send(Admission::Ended);
            }
            RoomCommand::Sample { reply } => {
                let _ = reply.send(None);
            }
            RoomCommand::Reset { done, .. } => resets.push(done),
            _ => {}
        }
    }
    for done in resets {
        let _ = done.send(());
    }
}

fn handle<F: HostFactory>(
    session: &mut RoomSession<F, SocketHandle>,
    command: RoomCommand,
    resets: &mut Vec<oneshot::Sender<()>>,
) {
    match command {
        RoomCommand::Open { socket, reply } => {
            let admission = match session.accept(socket) {
                Some(id) => Admission::Accepted(id),
                None => Admission::Full,
            };
            let _ = reply.send(admission);
        }
        RoomCommand::Text { id, text } => session.message(id, Incoming::Text(&text)),
        RoomCommand::Binary { id } => session.message(id, Incoming::Binary),
        RoomCommand::Closed { id, code } => session.closed(id, code),
        RoomCommand::Failed { id } => session.failed(id),
        RoomCommand::Reset { reason, done } => {
            session.reset(&reason);
            resets.push(done);
        }
        RoomCommand::Sample { reply } => {
            let _ = reply.send(session.sample());
        }
    }
}
