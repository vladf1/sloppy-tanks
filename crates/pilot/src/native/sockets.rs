//! The pilots' WebSockets on the command line's runtime: each socket is a task that waits
//! on its pilot's sends and the server's frames at once, and reports what happens over
//! one channel the run loop reads (as the desktop app's `sockets.rs` does on threads).

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures_util::{Sink, SinkExt, Stream, StreamExt};
use sloppy_core::net::client::MAX_BUFFERED_BYTES;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_tungstenite::Connector;
use tokio_tungstenite::tungstenite::{Bytes, Message, Utf8Bytes};

/// Connecting and the opening handshake give up after this long.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// One write to the server gives up after this long, and the socket with it.
const SEND_TIMEOUT: Duration = Duration::from_secs(10);
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
    Text(usize, u32, Utf8Bytes),
    Binary(usize, u32, Bytes),
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
    commands: UnboundedReceiver<Command>,
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
    let (write, read) = socket.split();
    carry(pilot, id, write, read, commands, buffered, events).await
}

/// What ends a socket.
enum Ending {
    /// This side closes: a close command, or the run loop dropped the socket.
    Close,
    /// The server sent its close frame, with this code; a flush sends the reply.
    ServerClosed(u16),
    /// The connection failed, or a write stalled past [`SEND_TIMEOUT`].
    Lost,
}

/// Carries the pilot's sends to `write` and the server's frames from `read` until either
/// side closes. Sends go out one at a time, in order; while one is pending the socket
/// still reads frames and takes commands, and none waits longer than [`SEND_TIMEOUT`].
/// A close still sends what was queued before it (a pilot's `leave` releases its seat at
/// once, where a dropped socket holds it for the reconnect grace), but only until
/// [`CLOSE_WAIT`] has passed, so a write the server does not take cannot keep it open.
async fn carry<W, R, E>(
    pilot: usize,
    id: u32,
    mut write: W,
    mut read: R,
    mut commands: UnboundedReceiver<Command>,
    buffered: &AtomicUsize,
    events: &UnboundedSender<SocketEvent>,
) -> u16
where
    W: Sink<Message> + Unpin,
    R: Stream<Item = Result<Message, E>> + Unpin,
{
    let mut queued = VecDeque::new();
    // Once this side closes: when the sends queued before the close and the closing
    // handshake must be done.
    let mut closing: Option<Instant> = None;
    let ending = loop {
        let Some(text) = queued.pop_front() else {
            if closing.is_some() {
                break Ending::Close;
            }
            tokio::select! {
                command = commands.recv() => match command {
                    Some(Command::Send(text)) => queued.push_back(text),
                    Some(Command::Close) | None => break Ending::Close,
                },
                message = read.next() => {
                    if let Some(ending) = deliver(pilot, id, message, events) {
                        break ending;
                    }
                }
            }
            continue;
        };
        let length = text.len();
        let send = tokio::time::timeout(
            SEND_TIMEOUT,
            write.send(Message::Text(Utf8Bytes::from(text))),
        );
        tokio::pin!(send);
        let ending = loop {
            tokio::select! {
                sent = &mut send => {
                    buffered.fetch_sub(length, Ordering::Relaxed);
                    break match sent {
                        Ok(Ok(())) => None,
                        Ok(Err(_)) | Err(_) => Some(Ending::Lost),
                    };
                }
                () = tokio::time::sleep_until(closing.unwrap_or_else(Instant::now)),
                    if closing.is_some() => break Some(Ending::Close),
                command = commands.recv(), if closing.is_none() => match command {
                    Some(Command::Send(text)) => queued.push_back(text),
                    Some(Command::Close) | None => closing = Some(Instant::now() + CLOSE_WAIT),
                },
                message = read.next() => {
                    if let Some(ending) = deliver(pilot, id, message, events) {
                        break Some(ending);
                    }
                }
            }
        };
        if let Some(ending) = ending {
            break ending;
        }
    };
    match ending {
        Ending::Close => {
            // Bounded with the sends before it: a sink still stalled on an interrupted
            // write gives up with the close frame.
            let deadline = closing.unwrap_or_else(|| Instant::now() + CLOSE_WAIT);
            let _ = tokio::time::timeout_at(deadline, async {
                if write.send(Message::Close(None)).await.is_err() {
                    return;
                }
                while let Some(Ok(message)) = read.next().await {
                    if matches!(message, Message::Close(_)) {
                        break;
                    }
                }
            })
            .await;
            NORMAL_CLOSE
        }
        Ending::ServerClosed(code) => {
            let _ = tokio::time::timeout(CLOSE_WAIT, write.flush()).await;
            code
        }
        Ending::Lost => ABNORMAL_CLOSE,
    }
}

/// Reports one of the server's frames, or returns what it ends the socket with.
fn deliver<E>(
    pilot: usize,
    id: u32,
    message: Option<Result<Message, E>>,
    events: &UnboundedSender<SocketEvent>,
) -> Option<Ending> {
    match message {
        Some(Ok(Message::Text(text))) => {
            let _ = events.send(SocketEvent::Text(pilot, id, text));
        }
        Some(Ok(Message::Binary(bytes))) => {
            let _ = events.send(SocketEvent::Binary(pilot, id, bytes));
        }
        Some(Ok(Message::Close(frame))) => {
            let code = frame.map_or(NO_STATUS, |frame| frame.code.into());
            return Some(Ending::ServerClosed(code));
        }
        Some(Ok(_)) => {}
        Some(Err(_)) | None => return Some(Ending::Lost),
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    /// A sink whose peer never reads: every write waits forever.
    struct Stalled;

    impl Sink<Message> for Stalled {
        type Error = ();
        fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), ()>> {
            Poll::Pending
        }
        fn start_send(self: Pin<&mut Self>, _: Message) -> Result<(), ()> {
            Ok(())
        }
        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), ()>> {
            Poll::Pending
        }
        fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), ()>> {
            Poll::Pending
        }
    }

    /// A sink whose peer takes every write at once, keeping them in order.
    #[derive(Default)]
    struct Taken(Vec<Message>);

    impl Sink<Message> for Taken {
        type Error = ();
        fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), ()>> {
            Poll::Ready(Ok(()))
        }
        fn start_send(mut self: Pin<&mut Self>, message: Message) -> Result<(), ()> {
            self.0.push(message);
            Ok(())
        }
        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), ()>> {
            Poll::Ready(Ok(()))
        }
        fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), ()>> {
            Poll::Ready(Ok(()))
        }
    }

    /// Runs a socket on a stalled sink and a silent server with `commands` queued,
    /// returning its close code and how long it lasted on the paused clock.
    async fn stalled_socket(commands: Vec<Command>) -> (u16, Duration) {
        let (sender, receiver) = unbounded_channel();
        let (events, _events) = unbounded_channel();
        let buffered = AtomicUsize::new(0);
        for command in commands {
            if let Command::Send(text) = &command {
                buffered.fetch_add(text.len(), Ordering::Relaxed);
            }
            sender.send(command).unwrap();
        }
        let read = futures_util::stream::pending::<Result<Message, ()>>();
        let started = tokio::time::Instant::now();
        let code = carry(0, 1, Stalled, read, receiver, &buffered, &events).await;
        // Held to here, so only the commands sent can end the socket.
        drop(sender);
        (code, started.elapsed())
    }

    #[tokio::test(start_paused = true)]
    async fn a_close_interrupts_a_stalled_send() {
        let commands = vec![Command::Send("input".into()), Command::Close];
        let (code, elapsed) = stalled_socket(commands).await;
        assert_eq!(code, NORMAL_CLOSE);
        // One bound for the stalled send and the close frame together.
        assert_eq!(elapsed, CLOSE_WAIT);
    }

    #[tokio::test(start_paused = true)]
    async fn a_close_sends_what_was_queued_before_it() {
        let (sender, receiver) = unbounded_channel();
        let (events, _events) = unbounded_channel();
        let buffered = AtomicUsize::new(0);
        for command in [
            Command::Send("input".into()),
            Command::Send("leave".into()),
            Command::Close,
            Command::Send("after".into()),
        ] {
            sender.send(command).unwrap();
        }
        let mut written = Taken::default();
        let read = futures_util::stream::pending::<Result<Message, ()>>();
        let code = carry(0, 1, &mut written, read, receiver, &buffered, &events).await;
        drop(sender);
        assert_eq!(code, NORMAL_CLOSE);
        assert_eq!(
            written.0,
            [
                Message::Text("input".into()),
                Message::Text("leave".into()),
                Message::Close(None),
            ],
            "the leave goes out before the close frame, and nothing after it"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_stalled_send_gives_up_after_its_timeout() {
        let (code, elapsed) = stalled_socket(vec![Command::Send("input".into())]).await;
        assert_eq!(code, ABNORMAL_CLOSE);
        assert_eq!(elapsed, SEND_TIMEOUT);
    }
}
