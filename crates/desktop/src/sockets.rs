//! The room's WebSockets (the `WebSocket` objects of `src/net/client.ts`): each socket
//! runs on its own thread with `tungstenite`, and its events come back to the main
//! thread over a channel, which the room drains every frame and poll.

use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::Duration;

use tungstenite::error::Error as SocketError;
use tungstenite::protocol::WebSocket;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, Utf8Bytes};

/// A socket this far behind on sends is closed and reconnected instead
/// (`MAX_BUFFERED_BYTES`, the page's `bufferedAmount` limit).
pub const MAX_BUFFERED_BYTES: usize = 16_384;
/// Connecting and the opening handshake give up after this long.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a socket thread waits for a frame before it looks for sends again: the
/// most a send waits behind a quiet socket.
const READ_WAIT: Duration = Duration::from_millis(2);
/// A close this side started waits this long for the server's close frame.
const CLOSE_WAIT: Duration = Duration::from_secs(2);
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
    commands: Sender<Command>,
    /// Bytes handed to the thread and not yet written (`bufferedAmount`).
    buffered: Arc<AtomicUsize>,
}

/// The room's open sockets and their events.
pub struct Sockets {
    sockets: HashMap<u32, Socket>,
    sender: Sender<SocketEvent>,
    events: Receiver<SocketEvent>,
}

impl Default for Sockets {
    fn default() -> Self {
        let (sender, events) = channel();
        Self {
            sockets: HashMap::new(),
            sender,
            events,
        }
    }
}

impl Sockets {
    /// Connect to `url` on a new thread; its events carry `id`.
    pub fn open(&mut self, id: u32, url: String) {
        let (commands, receiver) = channel();
        let buffered = Arc::new(AtomicUsize::new(0));
        let events = self.sender.clone();
        let pending = buffered.clone();
        let spawned = std::thread::Builder::new()
            .name(format!("room socket {id}"))
            .spawn(move || {
                let code = run(id, &url, &receiver, &pending, &events);
                let _ = events.send(SocketEvent::Closed(id, code));
            });
        if spawned.is_err() {
            let _ = self.sender.send(SocketEvent::Closed(id, ABNORMAL_CLOSE));
            return;
        }
        self.sockets.insert(id, Socket { commands, buffered });
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
}

impl Drop for Sockets {
    /// Each thread sends what it was handed, closes its socket and ends.
    fn drop(&mut self) {
        for (_, socket) in self.sockets.drain() {
            let _ = socket.commands.send(Command::Close);
        }
    }
}

/// Connect: TCP within [`CONNECT_TIMEOUT`], then TLS for `wss:` and the WebSocket
/// handshake. Returns the socket and a handle to its TCP stream for timeouts.
fn connect(url: &str) -> Option<(WebSocket<MaybeTlsStream<TcpStream>>, TcpStream)> {
    let request = tungstenite::client::IntoClientRequest::into_client_request(url).ok()?;
    let uri = request.uri();
    let host = uri.host()?.trim_start_matches('[').trim_end_matches(']');
    let port = uri
        .port_u16()
        .unwrap_or(if uri.scheme_str() == Some("wss") {
            443
        } else {
            80
        });
    let address = (host, port).to_socket_addrs().ok()?.next()?;
    let stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT).ok()?;
    stream.set_nodelay(true).ok()?;
    stream.set_read_timeout(Some(CONNECT_TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(CONNECT_TIMEOUT)).ok()?;
    let handle = stream.try_clone().ok()?;
    let (socket, _) = tungstenite::client_tls_with_config(request, stream, None, None).ok()?;
    Some((socket, handle))
}

/// A socket's life on its thread: connect, then alternate between the room's sends and
/// the server's frames until either side closes. Returns the close code.
fn run(
    id: u32,
    url: &str,
    commands: &Receiver<Command>,
    buffered: &AtomicUsize,
    events: &Sender<SocketEvent>,
) -> u16 {
    let Some((mut socket, stream)) = connect(url) else {
        return ABNORMAL_CLOSE;
    };
    if stream.set_read_timeout(Some(READ_WAIT)).is_err() {
        return ABNORMAL_CLOSE;
    }
    if events.send(SocketEvent::Opened(id)).is_err() {
        return NORMAL_CLOSE;
    }
    let mut closing: Option<std::time::Instant> = None;
    let mut code = NO_STATUS;
    loop {
        while closing.is_none() {
            match commands.try_recv() {
                Ok(Command::Send(text)) => {
                    let length = text.len();
                    let sent = socket.send(Message::Text(Utf8Bytes::from(text)));
                    buffered.fetch_sub(length, Ordering::Relaxed);
                    if sent.is_err() {
                        return ABNORMAL_CLOSE;
                    }
                }
                Ok(Command::Close) | Err(TryRecvError::Disconnected) => {
                    closing = Some(std::time::Instant::now());
                    code = NORMAL_CLOSE;
                    let _ = socket.close(None);
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                if closing.is_none() {
                    let _ = events.send(SocketEvent::Text(id, text.as_str().to_owned()));
                }
            }
            Ok(Message::Binary(bytes)) => {
                if closing.is_none() {
                    let _ = events.send(SocketEvent::Binary(id, bytes.to_vec()));
                }
            }
            Ok(Message::Close(frame)) => {
                if closing.is_none() {
                    code = frame.map_or(NO_STATUS, |frame| frame.code.into());
                }
            }
            Ok(_) => {}
            Err(SocketError::Io(error))
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
            {
                if let Some(started) = closing {
                    if started.elapsed() > CLOSE_WAIT {
                        return code;
                    }
                    let _ = socket.flush();
                }
            }
            Err(SocketError::ConnectionClosed | SocketError::AlreadyClosed) => return code,
            Err(_) => {
                return if closing.is_some() {
                    code
                } else {
                    ABNORMAL_CLOSE
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::time::Instant;

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
