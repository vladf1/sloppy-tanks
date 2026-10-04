//! The public, read-only `/dashboard` page and its Server-Sent Events stream of the
//! monitor's one-second readings.
//!
//! A room code is the key to join a room, so codes are masked here; full codes are in
//! `/stats`, `/rooms` and the journal. Nothing identifies players.

use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};

use bytes::Bytes;
use http_body::{Body, Frame};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::config::BuildInfo;
use crate::monitor::{
    HISTORY_READINGS, LiveReading, MonitorEvent, READING_INTERVAL_MS, ServerMonitor,
};
use crate::process_stats;
use crate::protocol::{CONTENT_VERSION, PROTOCOL_VERSION, SERVER_BUILD};

/// Open dashboard streams per process; each costs one small write per second.
pub const MAX_DASHBOARD_VIEWERS: usize = 10;
/// Unread output after which a viewer is dropped; one second's update is a few KB.
const MAX_VIEWER_BACKLOG_BYTES: usize = 256 * 1024;
/// Room code characters shown: enough to tell rooms apart, far too few to join one.
const SHOWN_CODE_CHARS: usize = 3;
const MB: u64 = 1024 * 1024;

/// The page, plain HTML and script. Its charts load uPlot from jsDelivr, pinned by version
/// and subresource integrity; without the library it still shows everything else.
pub const PAGE: &str = include_str!("dashboard.html");
pub const PAGE_HEADERS: [(&str, &str); 6] = [
    ("Content-Type", "text/html; charset=utf-8"),
    ("Cache-Control", "no-store"),
    (
        "Content-Security-Policy",
        "default-src 'none'; script-src 'unsafe-inline' https://cdn.jsdelivr.net; \
         style-src 'unsafe-inline' https://cdn.jsdelivr.net; \
         connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
    ),
    ("Referrer-Policy", "no-referrer"),
    ("X-Content-Type-Options", "nosniff"),
    ("X-Robots-Tag", "noindex"),
];

pub fn mask_room(code: &str) -> String {
    let shown: String = code.chars().take(SHOWN_CODE_CHARS).collect();
    let hidden = code.chars().count().saturating_sub(SHOWN_CODE_CHARS);
    shown + &"•".repeat(hidden)
}

fn public_event(event: &MonitorEvent) -> Value {
    json!({ "id": event.id, "atMs": event.at_ms, "room": mask_room(&event.room), "message": event.message })
}

fn frame(kind: &str, data: &Value) -> Bytes {
    Bytes::from(format!("event: {kind}\ndata: {data}\n\n"))
}

struct Viewer {
    sender: mpsc::UnboundedSender<Bytes>,
    backlog: Arc<AtomicUsize>,
}

/// Viewers and the latest public reading.
pub struct Dashboard {
    viewers: Vec<Viewer>,
    latest: Option<Value>,
    sent_event_id: u64,
    max_rooms: usize,
    build: BuildInfo,
    closed: bool,
}

impl Dashboard {
    pub fn new(max_rooms: usize, build: BuildInfo) -> Self {
        Self {
            viewers: Vec::new(),
            latest: None,
            sent_event_id: 0,
            max_rooms,
            build,
            closed: false,
        }
    }

    /// Open streams; a viewer whose connection ended no longer counts.
    pub fn viewers(&mut self) -> usize {
        self.viewers.retain(|viewer| !viewer.sender.is_closed());
        self.viewers.len()
    }

    /// Starts a stream with the recent history; `None` when every viewer slot is taken.
    pub fn stream(&mut self, monitor: &ServerMonitor) -> Option<EventStream> {
        if self.closed || self.viewers() >= MAX_DASHBOARD_VIEWERS {
            return None;
        }
        let (sender, receiver) = mpsc::unbounded_channel();
        let backlog = Arc::new(AtomicUsize::new(0));
        let (total_memory, _) = process_stats::host_memory();
        let mut hello = json!({
            "server": {
                "contentVersion": CONTENT_VERSION,
                "protocolVersion": PROTOCOL_VERSION,
                "serverBuild": SERVER_BUILD,
                "release": self.build.release,
                "commit": self.build.commit,
                // The crate version never changes; `release` is the version that does.
                "runtime": "Rust server",
                "environment": process_stats::environment(),
                "startedAtMs": monitor.started_ms(),
                "maxRooms": self.max_rooms,
                "cpus": process_stats::cpu_count(),
                "hostMemoryMB": (total_memory + MB / 2) / MB,
                "readingIntervalMs": READING_INTERVAL_MS,
                "historyReadings": HISTORY_READINGS,
            },
            "history": monitor.history(),
            "events": monitor.events().iter().map(public_event).collect::<Vec<_>>(),
        });
        if let Some(reading) = &self.latest {
            hello["reading"] = reading.clone();
        }
        let hello = frame("hello", &hello);
        backlog.fetch_add(hello.len(), Ordering::Relaxed);
        sender.send(hello).ok()?;
        self.viewers.push(Viewer {
            sender,
            backlog: backlog.clone(),
        });
        Some(EventStream { receiver, backlog })
    }

    /// Ends every stream; viewers' pages keep retrying until the server is back.
    pub fn close(&mut self) {
        self.closed = true;
        self.viewers.clear();
    }

    pub fn broadcast(&mut self, reading: &LiveReading, monitor: &ServerMonitor) {
        let events: Vec<Value> = monitor
            .events()
            .iter()
            .filter(|event| event.id > self.sent_event_id)
            .map(public_event)
            .collect();
        if let Some(last) = monitor.events().back() {
            self.sent_event_id = last.id;
        }
        let viewers = self.viewers();
        let mut public = serde_json::to_value(reading).expect("reading serializes");
        if let Some(rooms) = public["roomList"].as_array_mut() {
            for room in rooms {
                let masked = mask_room(room["room"].as_str().unwrap_or(""));
                room["room"] = Value::String(masked);
            }
        }
        let (total_memory, free_memory) = process_stats::host_memory();
        public["totals"] = serde_json::to_value(monitor.totals()).expect("totals serialize");
        public["viewers"] = json!(viewers);
        public["hostLoad"] = json!((process_stats::load_average() * 100.0).round() / 100.0);
        public["hostMemoryUsedMB"] =
            json!((total_memory.saturating_sub(free_memory) + MB / 2) / MB);
        let mut message = public.clone();
        message["events"] = Value::Array(events);
        self.latest = Some(public);
        if viewers == 0 {
            return;
        }
        let message = frame("reading", &message);
        self.viewers.retain(|viewer| {
            if viewer.backlog.load(Ordering::Relaxed) > MAX_VIEWER_BACKLOG_BYTES {
                return false;
            }
            viewer.backlog.fetch_add(message.len(), Ordering::Relaxed);
            viewer.sender.send(message.clone()).is_ok()
        });
    }
}

/// The body of one `/dashboard/stream` response.
pub struct EventStream {
    receiver: mpsc::UnboundedReceiver<Bytes>,
    backlog: Arc<AtomicUsize>,
}

impl Body for EventStream {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        match self.receiver.poll_recv(context) {
            Poll::Ready(Some(bytes)) => {
                self.backlog.fetch_sub(bytes.len(), Ordering::Relaxed);
                Poll::Ready(Some(Ok(Frame::data(bytes))))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_all_but_the_first_three_characters() {
        assert_eq!(mask_room("DASHROOM"), "DAS•••••");
        assert_eq!(mask_room(""), "");
        assert!(PAGE.contains("<title>Sloppy Tanks server</title>"));
    }
}
