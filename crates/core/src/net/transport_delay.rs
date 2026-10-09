//! Development-only transport delay (`src/net/transport-delay.ts`): an ordered half-RTT
//! delay with jitter and occasional head-of-line stalls, applied to both directions of
//! the client's connection. Enabled by the `latency`, `jitter` and `stall` page parameters.

use std::collections::VecDeque;

/// Share of messages that model a lost TCP segment when `stall` is set.
const STALL_CHANCE: f64 = 0.02;
const CHANNEL_CAPACITY: usize = 128;
/// Page parameters that turn the delay on.
pub const TRANSPORT_DELAY_PARAMS: [&str; 3] = ["latency", "jitter", "stall"];

/// Ordered transport delay with bounded storage. Jitter never reorders reliable messages.
#[derive(Clone, Debug)]
pub struct DelayedChannel<T> {
    queue: VecDeque<(f64, T)>,
    capacity: usize,
}

impl<T> Default for DelayedChannel<T> {
    fn default() -> Self {
        Self::with_capacity(CHANNEL_CAPACITY)
    }
}

impl<T> DelayedChannel<T> {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            capacity,
        }
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Queues `value` for delivery `delay_ms` after `now_ms`, never before the message
    /// ahead of it.
    pub fn send(&mut self, value: T, now_ms: f64, delay_ms: f64) -> Result<(), String> {
        if self.queue.len() >= self.capacity {
            return Err("Delayed channel capacity exceeded".into());
        }
        if !now_ms.is_finite() || !delay_ms.is_finite() || delay_ms < 0.0 {
            return Err("Invalid transport delay".into());
        }
        let after = self.queue.back().map_or(f64::NEG_INFINITY, |(at, _)| *at);
        self.queue
            .push_back(((now_ms + delay_ms).max(after), value));
        Ok(())
    }

    /// Everything due by `now_ms`, in order.
    pub fn receive(&mut self, now_ms: f64) -> Vec<T> {
        let mut due = Vec::new();
        while self.queue.front().is_some_and(|(at, _)| *at <= now_ms) {
            due.push(self.queue.pop_front().expect("checked").1);
        }
        due
    }

    pub fn clear(&mut self) {
        self.queue.clear();
    }
}

/// The delay settings from the page parameters.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DelaySettings {
    /// Half the simulated round trip, in milliseconds (0..=150).
    pub half_ms: f64,
    /// Extra random delay per message (0..=30 ms).
    pub jitter_ms: f64,
    /// How long an occasional stalled message is held (0..=500 ms).
    pub stall_ms: f64,
}

fn parameter(value: Option<&str>) -> f64 {
    // `Number(value) || 0`: missing, empty or unparsable reads as zero.
    value
        .and_then(|text| text.trim().parse::<f64>().ok())
        .filter(|number| number.is_finite())
        .unwrap_or(0.0)
}

impl DelaySettings {
    /// Reads `latency`, `jitter` and `stall` (each `None` when absent), clamped like the
    /// TypeScript.
    pub fn from_params(latency: Option<&str>, jitter: Option<&str>, stall: Option<&str>) -> Self {
        Self {
            half_ms: parameter(latency).clamp(0.0, 300.0) / 2.0,
            jitter_ms: parameter(jitter).clamp(0.0, 30.0),
            stall_ms: parameter(stall).clamp(0.0, 500.0),
        }
    }
}

/// Both directions of a delayed connection. `random` supplies uniform numbers in [0, 1).
pub struct TransportDelay {
    pub settings: DelaySettings,
    pub outbound: DelayedChannel<String>,
    pub inbound: DelayedChannel<super::protocol::Message>,
    random: Box<dyn FnMut() -> f64 + Send>,
}

impl TransportDelay {
    pub fn new(settings: DelaySettings, random: Box<dyn FnMut() -> f64 + Send>) -> Self {
        Self {
            settings,
            outbound: DelayedChannel::default(),
            inbound: DelayedChannel::default(),
            random,
        }
    }

    fn delay(&mut self) -> f64 {
        let stalled = if self.settings.stall_ms > 0.0 && (self.random)() < STALL_CHANCE {
            self.settings.stall_ms
        } else {
            0.0
        };
        self.settings.half_ms + (self.random)() * self.settings.jitter_ms + stalled
    }

    pub fn send(&mut self, text: String, now_ms: f64) -> Result<(), String> {
        let delay = self.delay();
        self.outbound.send(text, now_ms, delay)
    }

    pub fn receive(
        &mut self,
        message: super::protocol::Message,
        now_ms: f64,
    ) -> Result<(), String> {
        let delay = self.delay();
        self.inbound.send(message, now_ms, delay)
    }

    /// No per-message timers survive a reconnect.
    pub fn clear(&mut self) {
        self.outbound.clear();
        self.inbound.clear();
    }
}
