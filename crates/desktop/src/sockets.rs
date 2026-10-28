//! The room's WebSockets (the `WebSocket` objects of `src/net/client.ts`): each socket
//! runs on its own thread, a small tokio runtime that waits on the room's sends and
//! the server's frames at once, so neither waits behind the other. Its events come
//! back to the main thread over a channel, which the room drains every frame and poll.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use sloppy_core::net::client::MAX_BUFFERED_BYTES;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};

/// Connecting and the opening handshake give up after this long.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// A close this side started waits this long for the server's close frame.
const CLOSE_WAIT: Duration = Duration::from_secs(2);
/// Quitting waits this long for the sockets to send what they hold and close.
const QUIT_WAIT: Duration = Duration::from_millis(500);
/// Close codes the browser reports: normal, no status, abnormal (no close frame).
const NORMAL_CLOSE: u16 = 1000;
const NO_STATUS: u16 = 1005;
const ABNORMAL_CLOSE: u16 = 1006;

/// What a socket reports, with the room's id for it.
#[derive(Debug, PartialEq, Eq)]
pub enum SocketEvent {
    Opened(u32),
    Text(u32, String),
    Binary(u32, Vec<u8>),
    /// The socket is gone, with its close code; the last event of a socket.
    Closed(u32, u16),
}

enum Command {
    Send(String),
    Close,
}

struct Socket {
    commands: UnboundedSender<Command>,
    /// Bytes handed to the thread and not yet written (`bufferedAmount`).
    buffered: Arc<AtomicUsize>,
    thread: JoinHandle<()>,
}

/// The room's open sockets and their events.
pub struct Sockets {
    sockets: HashMap<u32, Socket>,
    sender: Sender<SocketEvent>,
    events: Receiver<SocketEvent>,
    /// Closed sockets whose threads may still be sending their close.
    closing: Vec<JoinHandle<()>>,
}

impl Default for Sockets {
    fn default() -> Self {
        let (sender, events) = channel();
        Self {
            sockets: HashMap::new(),
            sender,
            events,
            closing: Vec::new(),
        }
    }
}

impl Sockets {
    /// Connect to `url` on a new thread; its events carry `id`.
    pub fn open(&mut self, id: u32, url: String) {
        let (commands, receiver) = unbounded_channel();
        let buffered = Arc::new(AtomicUsize::new(0));
        let events = self.sender.clone();
        let pending = buffered.clone();
        let spawned = std::thread::Builder::new()
            .name(format!("room socket {id}"))
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                let code = match runtime {
                    Ok(runtime) => runtime.block_on(run(id, &url, receiver, &pending, &events)),
                    Err(_) => ABNORMAL_CLOSE,
                };
                let _ = events.send(SocketEvent::Closed(id, code));
            });
        match spawned {
            Ok(thread) => {
                self.sockets.insert(
                    id,
                    Socket {
                        commands,
                        buffered,
                        thread,
                    },
                );
            }
            Err(_) => {
                let _ = self.sender.send(SocketEvent::Closed(id, ABNORMAL_CLOSE));
            }
        }
    }

    /// Send `text`, or close the socket when it is too far behind (the room reconnects).
    pub fn send(&mut self, id: u32, text: String) {
        let Some(socket) = self.sockets.get(&id) else {
            return;
        };
        if socket.buffered.load(Ordering::Relaxed) > MAX_BUFFERED_BYTES {
            self.close(id);
            return;
        }
        socket.buffered.fetch_add(text.len(), Ordering::Relaxed);
        let _ = socket.commands.send(Command::Send(text));
    }

    /// Close the socket after the sends queued before this.
    pub fn close(&mut self, id: u32) {
        if let Some(socket) = self.sockets.remove(&id) {
            let _ = socket.commands.send(Command::Close);
            self.closing.retain(|thread| !thread.is_finished());
            self.closing.push(socket.thread);
        }
    }

    /// The next event of any socket, without waiting.
    pub fn next_event(&mut self) -> Option<SocketEvent> {
        let event = self.events.try_recv().ok()?;
        if let SocketEvent::Closed(id, _) = event {
            self.sockets.remove(&id);
        }
        Some(event)
    }

    /// Close every socket and wait a moment for them to send what they hold, the
    /// room's leave among it, before the app quits.
    pub fn close_all_and_wait(&mut self) {
        let ids: Vec<u32> = self.sockets.keys().copied().collect();
        for id in ids {
            self.close(id);
        }
        let deadline = Instant::now() + QUIT_WAIT;
        while Instant::now() < deadline && self.closing.iter().any(|thread| !thread.is_finished()) {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Sockets {
    /// Each thread sends what it was handed, closes its socket and ends.
    fn drop(&mut self) {
        for (_, socket) in self.sockets.drain() {
            let _ = socket.commands.send(Command::Close);
        }
    }
}

/// A socket's life on its thread: connect (every address the name resolves to, like
/// a browser), then wait on the room's sends and the server's frames together until
/// either side closes. Returns the close code: the server's when it closed first.
async fn run(
    id: u32,
    url: &str,
    mut commands: UnboundedReceiver<Command>,
    buffered: &AtomicUsize,
    events: &Sender<SocketEvent>,
) -> u16 {
    let connect = tokio_tungstenite::connect_async_tls_with_config(url, None, true, None);
    let Ok(Ok((socket, _))) = tokio::time::timeout(CONNECT_TIMEOUT, connect).await else {
        return ABNORMAL_CLOSE;
    };
    if events.send(SocketEvent::Opened(id)).is_err() {
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
                // The room closed it (or is gone): close, and wait for the server's
                // close frame a while.
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
                    let _ = events.send(SocketEvent::Text(id, text.as_str().to_owned()));
                }
                Some(Ok(Message::Binary(bytes))) => {
                    let _ = events.send(SocketEvent::Binary(id, bytes.to_vec()));
                }
                // The server closed first: its code stands, whatever is still queued
                // here (tungstenite answers the close frame itself).
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use tokio_tungstenite::tungstenite;

    /// Events until `stop` matches or two seconds pass.
    fn events_until(
        sockets: &mut Sockets,
        stop: impl Fn(&SocketEvent) -> bool,
    ) -> Vec<SocketEvent> {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut seen = Vec::new();
        while Instant::now() < deadline {
            match sockets.next_event() {
                Some(event) => {
                    let done = stop(&event);
                    seen.push(event);
                    if done {
                        break;
                    }
                }
                None => std::thread::yield_now(),
            }
        }
        seen
    }

    #[test]
    fn a_socket_echoes_text_and_binary_and_reports_its_close() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut server = tungstenite::accept(stream).unwrap();
            let Message::Text(text) = server.read().unwrap() else {
                panic!("text first");
            };
            server.send(Message::Text(text)).unwrap();
            server.send(Message::Binary(vec![1, 2, 3].into())).unwrap();
            server
                .close(Some(tungstenite::protocol::CloseFrame {
                    code: 4001.into(),
                    reason: "bye".into(),
                }))
                .unwrap();
            while server.read().is_ok() {}
        });
        let mut sockets = Sockets::default();
        sockets.open(7, format!("ws://{address}/room/ABCDEFGH"));
        let opened = events_until(&mut sockets, |event| *event == SocketEvent::Opened(7));
        assert_eq!(opened.last(), Some(&SocketEvent::Opened(7)));
        sockets.send(7, "hello".into());
        let events = events_until(&mut sockets, |event| {
            matches!(event, SocketEvent::Closed(..))
        });
        assert_eq!(
            events,
            vec![
                SocketEvent::Text(7, "hello".into()),
                SocketEvent::Binary(7, vec![1, 2, 3]),
                SocketEvent::Closed(7, 4001),
            ]
        );
    }

    #[test]
    fn the_servers_close_code_survives_sends_queued_behind_it() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut server = tungstenite::accept(stream).unwrap();
            let _ = server.read();
            server
                .close(Some(tungstenite::protocol::CloseFrame {
                    code: 4001.into(),
                    reason: "another tab".into(),
                }))
                .unwrap();
            while server.read().is_ok() {}
        });
        let mut sockets = Sockets::default();
        sockets.open(3, format!("ws://{address}/room/ABCDEFGH"));
        events_until(&mut sockets, |event| *event == SocketEvent::Opened(3));
        // Input keeps going out at frame rate while the server's close arrives.
        for frame in 0..200 {
            sockets.send(3, format!("input {frame}"));
            std::thread::sleep(Duration::from_millis(1));
        }
        let events = events_until(&mut sockets, |event| {
            matches!(event, SocketEvent::Closed(..))
        });
        assert_eq!(events.last(), Some(&SocketEvent::Closed(3, 4001)));
    }

    #[test]
    fn an_unreachable_server_closes_abnormally() {
        // A port nothing listens on: bind one, then free it.
        let address = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let mut sockets = Sockets::default();
        sockets.open(1, format!("ws://{address}/room/ABCDEFGH"));
        let events = events_until(&mut sockets, |event| {
            matches!(event, SocketEvent::Closed(..))
        });
        assert_eq!(events, vec![SocketEvent::Closed(1, ABNORMAL_CLOSE)]);
    }
}
