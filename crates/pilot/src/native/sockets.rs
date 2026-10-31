//! The pilots' WebSockets on the command line's runtime: each socket is a task that waits
//! on its pilot's sends and the server's frames at once, and reports what happens over
//! one channel the run loop reads (as the desktop app's `sockets.rs` does on threads).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use sloppy_core::net::client::MAX_BUFFERED_BYTES;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::task::JoinHandle;
use tokio_tungstenite::Connector;
use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};

/// Connecting and the opening handshake give up after this long.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// A close this side started waits this long for the server's close frame.
const CLOSE_WAIT: Duration = Duration::from_secs(2);
/// Close codes a browser reports: normal, no status, abnormal (no close frame).
const NORMAL_CLOSE: u16 = 1000;
const NO_STATUS: u16 = 1005;
const ABNORMAL_CLOSE: u16 = 1006;

/// What a socket reports, with its pilot and the pilot's id for it.
#[derive(Debug)]
pub enum SocketEvent {
    Opened(usize, u32),
    Text(usize, u32, String),
    Binary(usize, u32, Vec<u8>),
    /// The socket is gone, with its close code; the last event of a socket.
    Closed(usize, u32, u16),
}

enum Command {
    Send(String),
    Close,
}

struct Socket {
    commands: UnboundedSender<Command>,
    /// Bytes handed to the task and not yet written (`bufferedAmount`).
    buffered: Arc<AtomicUsize>,
}

pub struct Sockets {
    sockets: HashMap<(usize, u32), Socket>,
    events: UnboundedSender<SocketEvent>,
    tls: Arc<rustls::ClientConfig>,
    /// Every socket's task, so leaving can wait for the last sends and closes.
    tasks: Vec<JoinHandle<()>>,
}

impl Sockets {
    pub fn new(tls: Arc<rustls::ClientConfig>) -> (Self, UnboundedReceiver<SocketEvent>) {
        let (events, receiver) = unbounded_channel();
        let sockets = Self {
            sockets: HashMap::new(),
            events,
            tls,
            tasks: Vec::new(),
        };
        (sockets, receiver)
    }

    /// Connects to `url` on a new task; its events carry `pilot` and `socket`.
    pub fn open(&mut self, pilot: usize, socket: u32, url: String) {
        let (commands, receiver) = unbounded_channel();
        let buffered = Arc::new(AtomicUsize::new(0));
        let events = self.events.clone();
        let pending = buffered.clone();
        let tls = self.tls.clone();
        self.tasks.retain(|task| !task.is_finished());
        self.tasks.push(tokio::spawn(async move {
            let code = run(pilot, socket, &url, tls, receiver, &pending, &events).await;
            let _ = events.send(SocketEvent::Closed(pilot, socket, code));
        }));
        self.sockets
            .insert((pilot, socket), Socket { commands, buffered });
    }

    /// Sends `text`, or closes the socket when it is too far behind (the pilot reconnects).
    pub fn send(&mut self, pilot: usize, socket: u32, text: String) {
        let Some(open) = self.sockets.get(&(pilot, socket)) else {
            return;
        };
        if open.buffered.load(Ordering::Relaxed) > MAX_BUFFERED_BYTES {
            self.close(pilot, socket);
            return;
        }
        open.buffered.fetch_add(text.len(), Ordering::Relaxed);
        let _ = open.commands.send(Command::Send(text));
    }

    /// Closes the socket after the sends queued before this.
    pub fn close(&mut self, pilot: usize, socket: u32) {
        if let Some(open) = self.sockets.remove(&(pilot, socket)) {
            let _ = open.commands.send(Command::Close);
        }
    }

    /// Forgets a socket that reported its close.
    pub fn closed(&mut self, pilot: usize, socket: u32) {
        self.sockets.remove(&(pilot, socket));
    }

    /// Closes every socket and waits up to `wait` for them to send what they hold (the
    /// pilots' `leave` among it) and close.
    pub async fn close_all(&mut self, wait: Duration) {
        let open: Vec<(usize, u32)> = self.sockets.keys().copied().collect();
        for (pilot, socket) in open {
            self.close(pilot, socket);
        }
        let tasks = std::mem::take(&mut self.tasks);
        let _ = tokio::time::timeout(wait, async {
            for task in tasks {
                let _ = task.await;
            }
        })
        .await;
    }
}

/// A socket's life: connect, then carry the pilot's sends and the server's frames until
/// either side closes. Returns the close code: the server's when it closed first.
async fn run(
    pilot: usize,
    id: u32,
    url: &str,
    tls: Arc<rustls::ClientConfig>,
    mut commands: UnboundedReceiver<Command>,
    buffered: &AtomicUsize,
    events: &UnboundedSender<SocketEvent>,
) -> u16 {
    let connector = url.starts_with("wss:").then_some(Connector::Rustls(tls));
    let connect = tokio_tungstenite::connect_async_tls_with_config(url, None, true, connector);
    let Ok(Ok((socket, _))) = tokio::time::timeout(CONNECT_TIMEOUT, connect).await else {
        return ABNORMAL_CLOSE;
    };
    if events.send(SocketEvent::Opened(pilot, id)).is_err() {
        return NORMAL_CLOSE;
    }
    let (mut write, mut read) = socket.split();
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(Command::Send(text)) => {
                    let length = text.len();
                    let sent = write.send(Message::Text(Utf8Bytes::from(text))).await;
                    buffered.fetch_sub(length, Ordering::Relaxed);
                    if sent.is_err() {
                        return ABNORMAL_CLOSE;
                    }
                }
                Some(Command::Close) | None => {
                    let _ = write.send(Message::Close(None)).await;
                    let _ = tokio::time::timeout(CLOSE_WAIT, async {
                        while let Some(Ok(message)) = read.next().await {
                            if matches!(message, Message::Close(_)) {
                                break;
                            }
                        }
                    })
                    .await;
                    return NORMAL_CLOSE;
                }
            },
            message = read.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    let _ = events.send(SocketEvent::Text(pilot, id, text.as_str().to_owned()));
                }
                Some(Ok(Message::Binary(bytes))) => {
                    let _ = events.send(SocketEvent::Binary(pilot, id, bytes.to_vec()));
                }
                Some(Ok(Message::Close(frame))) => {
                    let code = frame.map_or(NO_STATUS, |frame| frame.code.into());
                    let _ = write.flush().await;
                    return code;
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => return ABNORMAL_CLOSE,
            },
        }
    }
}
