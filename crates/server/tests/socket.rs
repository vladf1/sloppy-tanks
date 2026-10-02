//! Transport regressions use always-ready in-memory input, so busy readers and stalled
//! writers exercise queue bounds without timing races or a network load test.

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use sloppy_server::session::RoomSocket;
use sloppy_server::socket::{self, Ending, MAX_BUFFERED_BYTES};
use sloppy_server::websocket::{Codec, Role};

const PING_COUNT: usize = 21_000;
const MAX_READ_BYTES: usize = 8192;
const MAX_CLOSE_FRAME_BYTES: usize = 127;

#[derive(Default)]
struct Progress {
    read_bytes: usize,
    first_write_after: Option<usize>,
    largest_write: usize,
}

struct MemoryIo {
    input: Vec<u8>,
    progress: Arc<Mutex<Progress>>,
    stalled: bool,
}

impl AsyncRead for MemoryIo {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let mut progress = self.progress.lock().unwrap();
        let offset = progress.read_bytes;
        if offset == self.input.len() {
            return if !self.stalled && progress.first_write_after.is_some() {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            };
        }
        let count = (self.input.len() - offset)
            .min(buffer.remaining())
            .min(MAX_READ_BYTES);
        buffer.put_slice(&self.input[offset..offset + count]);
        progress.read_bytes += count;
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for MemoryIo {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let mut progress = self.progress.lock().unwrap();
        let read = progress.read_bytes;
        progress.first_write_after.get_or_insert(read);
        progress.largest_write = progress.largest_write.max(bytes.len());
        if self.stalled {
            Poll::Pending
        } else {
            Poll::Ready(Ok(bytes.len()))
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

fn ping_burst(stalled: bool) -> (MemoryIo, Arc<Mutex<Progress>>) {
    // A legal, masked control frame, with an all-zero mask and maximum-sized payload.
    let mut ping = vec![0x89, 0x80 | 125, 0, 0, 0, 0];
    ping.extend([b'p'; 125]);
    let progress = Arc::new(Mutex::new(Progress::default()));
    (
        MemoryIo {
            input: ping.repeat(PING_COUNT),
            progress: progress.clone(),
            stalled,
        },
        progress,
    )
}

#[tokio::test(start_paused = true)]
async fn busy_reader_does_not_starve_pong_writes() {
    let (io, progress) = ping_burst(false);
    let (_handle, output) = socket::socket_pair();
    let (_stop, terminate) = tokio::sync::watch::channel(false);
    let ending = socket::run(
        io,
        Codec::new(Role::Server, None, 8192),
        output,
        None,
        terminate,
    )
    .await;
    assert_eq!(ending, Ending::Dropped);
    let progress = progress.lock().unwrap();
    assert!(
        progress.first_write_after.unwrap() <= MAX_READ_BYTES,
        "first write waited for {} incoming bytes",
        progress.first_write_after.unwrap()
    );
    assert!(progress.largest_write <= MAX_READ_BYTES);
}

#[tokio::test(start_paused = true)]
async fn queued_close_is_serviced_before_a_busy_reader() {
    let (io, progress) = ping_burst(false);
    let (handle, output) = socket::socket_pair();
    handle.close(1000, "test");
    let (_stop, terminate) = tokio::sync::watch::channel(false);
    socket::run(
        io,
        Codec::new(Role::Server, None, 8192),
        output,
        None,
        terminate,
    )
    .await;
    let progress = progress.lock().unwrap();
    assert_eq!(progress.first_write_after, Some(0));
    assert!(progress.largest_write <= MAX_CLOSE_FRAME_BYTES);
}

#[tokio::test(start_paused = true)]
async fn pongs_to_a_stalled_writer_are_bounded_and_close() {
    let (io, progress) = ping_burst(true);
    let (_handle, output) = socket::socket_pair();
    let (_stop, terminate) = tokio::sync::watch::channel(false);
    let ending = tokio::time::timeout(
        socket::CLOSE_TIMEOUT + Duration::from_secs(1),
        socket::run(
            io,
            Codec::new(Role::Server, None, 8192),
            output,
            None,
            terminate,
        ),
    )
    .await;
    let progress = progress.lock().unwrap();
    assert!(
        progress.largest_write <= MAX_BUFFERED_BYTES + MAX_CLOSE_FRAME_BYTES,
        "pending output reached {} bytes",
        progress.largest_write
    );
    assert_eq!(
        ending.unwrap(),
        Ending::Dropped,
        "slow-reader close must time out"
    );
}
