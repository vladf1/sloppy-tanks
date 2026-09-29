//! Bytes through room connections after compression, including frame and handshake
//! bytes (Node read the room sockets' TCP counters).

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::monitor::WireBytes;

/// Running totals over every room connection since the server started.
#[derive(Default)]
pub struct WireTotals {
    sent: AtomicU64,
    received: AtomicU64,
}

impl WireTotals {
    pub fn total(&self) -> WireBytes {
        WireBytes {
            sent: self.sent.load(Ordering::Relaxed),
            received: self.received.load(Ordering::Relaxed),
        }
    }
}

/// One TCP connection's bytes. Plain HTTP requests (`/rooms`, `/health`) stay out of
/// the totals; a connection joins them, with its handshake, once it becomes a room socket.
pub struct ConnectionBytes {
    sent: AtomicU64,
    received: AtomicU64,
    tracked: AtomicBool,
    totals: Arc<WireTotals>,
}

impl ConnectionBytes {
    pub fn new(totals: Arc<WireTotals>) -> Arc<Self> {
        Arc::new(Self {
            sent: AtomicU64::new(0),
            received: AtomicU64::new(0),
            tracked: AtomicBool::new(false),
            totals,
        })
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
}

impl<T> CountingIo<T> {
    pub fn new(inner: T, bytes: Arc<ConnectionBytes>) -> Self {
        Self { inner, bytes }
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
