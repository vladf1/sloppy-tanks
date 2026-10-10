//! One upgraded room connection: the socket handle a room sends through, and the task
//! that moves frames between the network and the room.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use bytes::{Buf, BytesMut};
use sloppy_core::net::match_host::ConnectionId;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

use crate::protocol::Message;
use crate::room_task::RoomCommand;
use crate::session::RoomSocket;
use crate::tcp_path::TcpReading;
use crate::websocket::{Codec, Event};
use crate::wire::ConnectionBytes;

/// Queued output after which a reader is too slow to follow 20 Hz snapshots; the socket
/// is closed with 4002. Counted like `ws`'s `bufferedAmount`: compressed frames not yet
/// taken by the kernel plus messages not yet compressed. A final close frame may add
/// at most one control frame beyond this budget.
pub const MAX_BUFFERED_BYTES: usize = 2_000_000;
/// Time a closing handshake may take before the connection is cut (`ws`'s closeTimeout).
pub const CLOSE_TIMEOUT: Duration = Duration::from_secs(30);
const READ_BUFFER_BYTES: usize = 8 * 1024;

enum Outbound {
    Message(Message),
    Close(u16, String),
}

#[derive(Default)]
struct SocketState {
    queued: AtomicUsize,
    closing: AtomicBool,
}

impl SocketState {
    fn reserve(&self, bytes: usize) -> bool {
        self.queued
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |queued| {
                queued
                    .checked_add(bytes)
                    .filter(|total| *total <= MAX_BUFFERED_BYTES)
            })
            .is_ok()
    }
}

/// What a room holds for a socket. Sending never blocks the room: messages queue for the
/// socket's task, which compresses and writes them.
#[derive(Clone)]
pub struct SocketHandle {
    sender: mpsc::UnboundedSender<Outbound>,
    state: Arc<SocketState>,
    connection: Option<Arc<ConnectionBytes>>,
}

impl SocketHandle {
    /// Lets the room read the TCP figures `connection` records (see [`RoomSocket::tcp`]).
    pub fn measured_by(self, connection: Arc<ConnectionBytes>) -> Self {
        Self {
            connection: Some(connection),
            ..self
        }
    }
}

impl RoomSocket for SocketHandle {
    fn send(&self, message: Message) {
        if self.state.closing.load(Ordering::Relaxed) {
            return;
        }
        let bytes = message.len();
        if !self.state.reserve(bytes) {
            self.close(4002, "Slow reader");
            return;
        }
        // A finished connection task has already reported its close to the room.
        if self.sender.send(Outbound::Message(message)).is_err() {
            self.state.queued.fetch_sub(bytes, Ordering::Relaxed);
        }
    }

    fn close(&self, code: u16, reason: &str) {
        if self.state.closing.swap(true, Ordering::Relaxed) {
            return;
        }
        let _ = self.sender.send(Outbound::Close(code, reason.to_string()));
    }

    fn tcp(&self) -> Option<TcpReading> {
        self.connection.as_ref()?.tcp()
    }
}

/// The connection's end of a [`SocketHandle`].
pub struct SocketOutput {
    receiver: mpsc::UnboundedReceiver<Outbound>,
    state: Arc<SocketState>,
}

pub fn socket_pair() -> (SocketHandle, SocketOutput) {
    let (sender, receiver) = mpsc::unbounded_channel();
    let state = Arc::new(SocketState::default());
    (
        SocketHandle {
            sender,
            state: state.clone(),
            connection: None,
        },
        SocketOutput { receiver, state },
    )
}

/// Where a connection's messages go once a room admitted it.
pub struct RoomLink {
    pub id: ConnectionId,
    pub room: mpsc::Sender<RoomCommand>,
}

/// How a connection ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Ending {
    /// The peer sent a close frame with this code (1005 without one).
    Closed(u16),
    /// The connection dropped without a close frame, the close timed out, or the server
    /// stopped (1006).
    Dropped,
    /// The peer broke the protocol; the connection was cut at once, as `ws` does.
    Failed,
}

impl Ending {
    /// The close code `ws` reports for this ending.
    pub fn code(&self) -> u16 {
        match self {
            Ending::Closed(code) => *code,
            _ => 1006,
        }
    }
}

/// Moves frames until the connection ends. Room messages are forwarded to `room` (when
/// admitted); the room's output arrives through `output`.
pub async fn run<IO>(
    io: IO,
    mut codec: Codec,
    output: SocketOutput,
    room: Option<&RoomLink>,
    mut terminate: watch::Receiver<bool>,
) -> Ending
where
    IO: AsyncRead + AsyncWrite + Send,
{
    let SocketOutput {
        mut receiver,
        state,
    } = output;
    let (mut reader, mut writer) = tokio::io::split(io);
    let mut input = BytesMut::with_capacity(READ_BUFFER_BYTES);
    let mut pending = BytesMut::new();
    let mut close_sent = false;
    let mut peer_close: Option<u16> = None;
    let mut close_deadline: Option<Instant> = None;
    let mut outbound_open = true;
    let mut close_requested: Option<(Option<u16>, String)> = None;
    // Messages already reserved their uncompressed bytes at admission; control replies reserve
    // their full frame here. Adjust atomically against messages the room may be queuing.
    let append =
        |pending: &mut BytesMut, reserved: usize, encode: &mut dyn FnMut(&mut BytesMut)| {
            let before = pending.len();
            encode(pending);
            let encoded = pending.len() - before;
            if encoded > reserved && !state.reserve(encoded - reserved) {
                pending.truncate(before);
                state.queued.fetch_sub(reserved, Ordering::Relaxed);
                return false;
            }
            if reserved > encoded {
                state
                    .queued
                    .fetch_sub(reserved - encoded, Ordering::Relaxed);
            }
            true
        };
    let ending = 'connection: loop {
        if let Some((code, reason)) = close_requested.take() {
            state.closing.store(true, Ordering::Relaxed);
            // Keep already encoded output intact (it can end in a partially written
            // frame), and allow only this final control frame beyond the queue budget.
            let before = pending.len();
            codec.encode_close(code, &reason, &mut pending);
            state
                .queued
                .fetch_add(pending.len() - before, Ordering::Relaxed);
            close_sent = true;
            close_deadline.get_or_insert_with(|| Instant::now() + CLOSE_TIMEOUT);
        }
        if let Some(code) = peer_close
            && close_sent
            && pending.is_empty()
        {
            break Ending::Closed(code);
        }
        let deadline = close_deadline;
        tokio::select! {
            biased;
            () = stopped(&mut terminate) => break Ending::Dropped,
            () = sleep_until_some(deadline) => break Ending::Dropped,
            command = receiver.recv(), if outbound_open && !close_sent => match command {
                Some(Outbound::Message(message)) => {
                    let encode = &mut |out: &mut BytesMut| match &message {
                        Message::Text(text) => codec.encode_text(text, out),
                        Message::Binary(bytes) => codec.encode_binary(bytes, out),
                    };
                    if !append(&mut pending, message.len(), encode) {
                        close_requested = Some((Some(4002), "Slow reader".into()));
                    }
                }
                Some(Outbound::Close(code, reason)) => {
                    close_requested = Some((Some(code), reason));
                }
                None => outbound_open = false,
            },
            written = writer.write(&pending), if !pending.is_empty() => match written {
                Ok(0) | Err(_) => break Ending::Dropped,
                Ok(count) => {
                    pending.advance(count);
                    state.queued.fetch_sub(count, Ordering::Relaxed);
                }
            },
            // Service room output, closes and ready writes before taking another batch
            // from a busy reader. A stalled writer may still receive the peer's close.
            read = reader.read_buf(&mut input), if peer_close.is_none() => {
                match read {
                    Ok(0) | Err(_) => break Ending::Dropped,
                    Ok(_) => {}
                }
                loop {
                    match codec.decode(&mut input) {
                        Ok(None) => break,
                        Ok(Some(Event::Text(text))) => {
                            if let (Some(link), false) = (room, close_sent) {
                                let _ = link.room.send(RoomCommand::Text { id: link.id, text }).await;
                            }
                        }
                        Ok(Some(Event::Binary(_))) => {
                            if let (Some(link), false) = (room, close_sent) {
                                let _ = link.room.send(RoomCommand::Binary { id: link.id }).await;
                            }
                        }
                        Ok(Some(Event::Ping(payload))) => {
                            if !close_sent
                                && !append(&mut pending, 0, &mut |out| codec.encode_pong(&payload, out))
                            {
                                close_requested = Some((Some(4002), "Slow reader".into()));
                                break;
                            }
                        }
                        Ok(Some(Event::Pong)) => {}
                        Ok(Some(Event::Close(code, reason))) => {
                            peer_close = Some(code.unwrap_or(1005));
                            if !close_sent {
                                // Echo the peer's close, as ws does, after any queued output.
                                close_requested = Some((code, reason));
                            }
                            close_deadline.get_or_insert_with(|| Instant::now() + CLOSE_TIMEOUT);
                            break;
                        }
                        Err(_) => break 'connection Ending::Failed,
                    }
                }
            }
        }
    };
    state.closing.store(true, Ordering::Relaxed);
    if matches!(ending, Ending::Closed(_)) {
        let _ = tokio::time::timeout(Duration::from_secs(1), writer.shutdown()).await;
    }
    ending
}

/// Resolves once the server asks every connection to stop.
pub async fn stopped(terminate: &mut watch::Receiver<bool>) {
    // A dropped sender means the server is gone, which also stops the connection.
    let _ = terminate.wait_for(|stop| *stop).await;
}

/// Resolves at `deadline`, or never without one.
pub(crate) async fn sleep_until_some(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}
