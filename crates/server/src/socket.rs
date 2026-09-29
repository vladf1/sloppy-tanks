//! One upgraded room connection: the socket handle a room sends through, and the task
//! that moves frames between the network and the room.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use bytes::{Buf, BytesMut};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

use crate::host::ConnectionId;
use crate::room_task::RoomCommand;
use crate::session::{RoomSocket, SendFailed};
use crate::websocket::{Codec, Event};

/// Queued output after which a reader is too slow to follow 20 Hz snapshots; the socket
/// is closed with 4002. Counted like `ws`'s `bufferedAmount`: compressed frames not yet
/// taken by the kernel plus messages not yet compressed.
pub const MAX_BUFFERED_BYTES: usize = 2_000_000;
/// Time a closing handshake may take before the connection is cut (`ws`'s closeTimeout).
pub const CLOSE_TIMEOUT: Duration = Duration::from_secs(30);
const READ_BUFFER_BYTES: usize = 8 * 1024;

enum Outbound {
    Text(String),
    Close(u16, String),
}

#[derive(Default)]
struct SocketState {
    queued: AtomicUsize,
    closing: AtomicBool,
}

/// What a room holds for a socket. Sending never blocks the room: messages queue for the
/// socket's task, which compresses and writes them.
#[derive(Clone)]
pub struct SocketHandle {
    sender: mpsc::UnboundedSender<Outbound>,
    state: Arc<SocketState>,
}

impl RoomSocket for SocketHandle {
    fn send(&self, text: String) -> Result<(), SendFailed> {
        if self.state.closing.load(Ordering::Relaxed) {
            return Ok(());
        }
        if self.state.queued.load(Ordering::Relaxed) > MAX_BUFFERED_BYTES {
            self.close(4002, "Slow reader");
            return Ok(());
        }
        self.state.queued.fetch_add(text.len(), Ordering::Relaxed);
        // A finished connection task has already reported its close to the room.
        let _ = self.sender.send(Outbound::Text(text));
        Ok(())
    }

    fn close(&self, code: u16, reason: &str) {
        if self.state.closing.swap(true, Ordering::Relaxed) {
            return;
        }
        let _ = self.sender.send(Outbound::Close(code, reason.to_string()));
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
    /// The connection dropped without a close frame, or the close timed out (1006).
    Dropped,
    /// The peer broke the protocol; the connection was cut at once, as `ws` does.
    Failed,
    /// The server stopped.
    Terminated,
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
    // Adds what `encode` appends to the queued-output figure the room checks.
    let append = |pending: &mut BytesMut, encode: &mut dyn FnMut(&mut BytesMut)| {
        let before = pending.len();
        encode(pending);
        state
            .queued
            .fetch_add(pending.len() - before, Ordering::Relaxed);
    };
    let ending = loop {
        if close_sent && peer_close.is_some() && pending.is_empty() {
            break Ending::Closed(peer_close.unwrap_or(1005));
        }
        let deadline = close_deadline;
        tokio::select! {
            biased;
            () = stopped(&mut terminate) => break Ending::Terminated,
            () = sleep_until_some(deadline), if deadline.is_some() => break Ending::Dropped,
            read = reader.read_buf(&mut input), if peer_close.is_none() => {
                match read {
                    Ok(0) | Err(_) => break Ending::Dropped,
                    Ok(_) => {}
                }
                let mut failed = false;
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
                            if !close_sent {
                                append(&mut pending, &mut |out| codec.encode_pong(&payload, out));
                            }
                        }
                        Ok(Some(Event::Pong)) => {}
                        Ok(Some(Event::Close(code, reason))) => {
                            peer_close = Some(code.unwrap_or(1005));
                            if !close_sent {
                                // Echo the peer's close, as ws does, after any queued output.
                                state.closing.store(true, Ordering::Relaxed);
                                append(&mut pending, &mut |out| codec.encode_close(code, &reason, out));
                                close_sent = true;
                            }
                            close_deadline.get_or_insert_with(|| Instant::now() + CLOSE_TIMEOUT);
                            break;
                        }
                        Err(_) => {
                            failed = true;
                            break;
                        }
                    }
                }
                if failed {
                    break Ending::Failed;
                }
            }
            command = receiver.recv(), if outbound_open && !close_sent => match command {
                Some(Outbound::Text(text)) => {
                    append(&mut pending, &mut |out| codec.encode_text(&text, out));
                    state.queued.fetch_sub(text.len(), Ordering::Relaxed);
                }
                Some(Outbound::Close(code, reason)) => {
                    append(&mut pending, &mut |out| codec.encode_close(Some(code), &reason, out));
                    close_sent = true;
                    close_deadline.get_or_insert_with(|| Instant::now() + CLOSE_TIMEOUT);
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
    let _ = terminate.wait_for(|stop| *stop).await.map(|_| ());
}

async fn sleep_until_some(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}
