//! Bytes through room connections after compression, including frame and handshake
//! bytes (Node read the room sockets' TCP counters), and each room socket's TCP round
//! trip and retransmissions.

use std::io;
use std::os::fd::{AsRawFd, RawFd};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;

use crate::monitor::{TcpSegments, WireBytes};
use crate::tcp_path::{self, TcpReading};

/// How often a busy room socket reads its TCP figures: once per monitor reading.
const TCP_READING_INTERVAL: Duration = Duration::from_secs(1);

/// Running totals over every room connection since the server started.
#[derive(Default)]
pub struct WireTotals {
    sent: AtomicU64,
    received: AtomicU64,
    data_segments_sent: AtomicU64,
    retransmitted_segments: AtomicU64,
}

impl WireTotals {
    pub fn total(&self) -> WireBytes {
        WireBytes {
            sent: self.sent.load(Ordering::Relaxed),
            received: self.received.load(Ordering::Relaxed),
        }
    }

    pub fn segments(&self) -> TcpSegments {
        TcpSegments {
            sent: self.data_segments_sent.load(Ordering::Relaxed),
            retransmitted: self.retransmitted_segments.load(Ordering::Relaxed),
        }
    }
}

/// One TCP connection's bytes and, once it is a room socket, its latest TCP reading.
/// Plain HTTP requests (`/rooms`, `/health`) stay out of the totals; a connection joins
/// them, with its handshake, once it becomes a room socket.
pub struct ConnectionBytes {
    sent: AtomicU64,
    received: AtomicU64,
    tracked: AtomicBool,
    totals: Arc<WireTotals>,
    measured: AtomicBool,
    rtt_us: AtomicU32,
    data_segments_sent: AtomicU64,
    retransmitted_segments: AtomicU64,
}

impl ConnectionBytes {
    pub fn new(totals: Arc<WireTotals>) -> Arc<Self> {
        Arc::new(Self {
            sent: AtomicU64::new(0),
            received: AtomicU64::new(0),
            tracked: AtomicBool::new(false),
            totals,
            measured: AtomicBool::new(false),
            rtt_us: AtomicU32::new(0),
            data_segments_sent: AtomicU64::new(0),
            retransmitted_segments: AtomicU64::new(0),
        })
    }

    /// The connection's latest TCP reading; `None` until a room socket is first measured
    /// (or always, off Linux).
    pub fn tcp(&self) -> Option<TcpReading> {
        self.measured.load(Ordering::Relaxed).then(|| TcpReading {
            rtt_us: self.rtt_us.load(Ordering::Relaxed),
            data_segments_sent: self.data_segments_sent.load(Ordering::Relaxed),
            retransmitted_segments: self.retransmitted_segments.load(Ordering::Relaxed),
        })
    }

    /// Keeps `reading` as the latest and adds what it counted since the previous one to
    /// the totals. The connection's own I/O is the only writer.
    fn record(&self, reading: TcpReading) {
        let sent = self
            .data_segments_sent
            .swap(reading.data_segments_sent, Ordering::Relaxed);
        let retransmitted = self
            .retransmitted_segments
            .swap(reading.retransmitted_segments, Ordering::Relaxed);
        self.totals.data_segments_sent.fetch_add(
            reading.data_segments_sent.saturating_sub(sent),
            Ordering::Relaxed,
        );
        self.totals.retransmitted_segments.fetch_add(
            reading.retransmitted_segments.saturating_sub(retransmitted),
            Ordering::Relaxed,
        );
        self.rtt_us.store(reading.rtt_us, Ordering::Relaxed);
        self.measured.store(true, Ordering::Relaxed);
    }

    /// Counts this connection, including the bytes it already moved, in the totals.
    pub fn track(&self) {
        if self.tracked.swap(true, Ordering::Relaxed) {
            return;
        }
        self.totals
            .sent
            .fetch_add(self.sent.swap(0, Ordering::Relaxed), Ordering::Relaxed);
        self.totals
            .received
            .fetch_add(self.received.swap(0, Ordering::Relaxed), Ordering::Relaxed);
    }

    fn add(&self, own: &AtomicU64, total: &AtomicU64, bytes: usize) {
        let counter = if self.tracked.load(Ordering::Relaxed) {
            total
        } else {
            own
        };
        counter.fetch_add(bytes as u64, Ordering::Relaxed);
    }
}

/// A stream that counts what passes through it.
pub struct CountingIo<T> {
    inner: T,
    bytes: Arc<ConnectionBytes>,
    /// The TCP socket `inner` owns, so it stays open as long as this stream exists.
    socket: Option<RawFd>,
    next_reading: Instant,
}

impl<T> CountingIo<T> {
    pub fn new(inner: T, bytes: Arc<ConnectionBytes>) -> Self {
        Self {
            inner,
            bytes,
            socket: None,
            next_reading: Instant::now(),
        }
    }

    /// Reads the socket's TCP figures once it is a room socket, at most once per
    /// interval unless `force` is set.
    fn measure(&mut self, force: bool) {
        let Some(socket) = self.socket else {
            return;
        };
        let now = Instant::now();
        if !self.bytes.tracked.load(Ordering::Relaxed) || (!force && now < self.next_reading) {
            return;
        }
        self.next_reading = now + TCP_READING_INTERVAL;
        if let Some(reading) = tcp_path::read(socket) {
            self.bytes.record(reading);
        }
    }
}

impl CountingIo<TcpStream> {
    /// Counts a TCP connection's bytes and, once it is a room socket, reads its round
    /// trip and retransmissions while it writes and once more as it closes.
    pub fn tcp(stream: TcpStream, bytes: Arc<ConnectionBytes>) -> Self {
        let mut io = Self::new(stream, bytes);
        io.socket = Some(io.inner.as_raw_fd());
        io
    }
}

impl<T> Drop for CountingIo<T> {
    /// The final reading, while `inner` still holds the socket open.
    fn drop(&mut self) {
        self.measure(true);
    }
}

impl<T: AsyncRead + Unpin> AsyncRead for CountingIo<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buffer.filled().len();
        let result = Pin::new(&mut self.inner).poll_read(context, buffer);
        let read = buffer.filled().len() - before;
        if read > 0 {
            let bytes = &self.bytes;
            bytes.add(&bytes.received, &bytes.totals.received, read);
        }
        result
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for CountingIo<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(&mut self.inner).poll_write(context, data);
        if let Poll::Ready(Ok(written)) = result {
            let bytes = &self.bytes;
            bytes.add(&bytes.sent, &bytes.totals.sent, written);
            self.measure(false);
        }
        result
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffers: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(&mut self.inner).poll_write_vectored(context, buffers);
        if let Poll::Ready(Ok(written)) = result {
            let bytes = &self.bytes;
            bytes.add(&bytes.sent, &bytes.totals.sent, written);
            self.measure(false);
        }
        result
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(context)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(context)
    }
}
