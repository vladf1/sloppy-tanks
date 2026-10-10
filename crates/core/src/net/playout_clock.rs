//! The client's adaptive playout clock (`src/net/playout-clock.ts`).

use std::collections::VecDeque;

use super::fixed_step_clock::{HOST_INTERVAL_MS, SIMULATION_STEP_MS};
use super::render_timeline::MAX_EXTRAPOLATION_SECONDS;

/// Covers tick quantization and host timer slop on top of one batch interval.
const BUFFER_MARGIN_MS: f64 = 20.0;
const MIN_BUFFER_MS: f64 = HOST_INTERVAL_MS + BUFFER_MARGIN_MS;
const MAX_BUFFER_MS: f64 = 250.0;
const LATENESS_WINDOW_MS: f64 = 3000.0;
const LATENESS_PERCENTILE: f64 = 0.95;
const BUFFER_SHRINK_MS_PER_SECOND: f64 = 20.0;
/// A faster arrival lowers the path estimate at once; a slower route is followed gradually.
const PATH_RISE_MS_PER_SECOND: f64 = 5.0;
const MAX_SLEW: f64 = 0.1;
/// Display error that produces the full slew rate.
const SLEW_RESPONSE_MS: f64 = 1000.0;
const SNAP_MS: f64 = 500.0;
const MAX_FRAME_MS: f64 = 250.0;
const UNDERRUN_AVERAGE_MS: f64 = 2000.0;
const MAX_UNDERRUN_MS: f64 = MAX_EXTRAPOLATION_SECONDS * 1000.0;

#[derive(Clone, Copy, Debug)]
struct Arrival {
    at_ms: f64,
    /// Local arrival time minus server simulation time.
    path_ms: f64,
}

/// Maps local time to the server simulation time being displayed. Arrival jitter is
/// absorbed by an adaptive buffer behind the fastest observed path, and the display
/// advances at a slewed real-time rate instead of restarting from each packet, so a late
/// packet never pauses motion until the buffer is exhausted.
#[derive(Clone, Debug)]
pub struct PlayoutClock {
    /// Server simulation time being displayed, in milliseconds.
    pub display_ms: f64,
    pub buffer_ms: f64,
    /// Fraction of recent frames that ran past the newest snapshot.
    pub underrun: f64,
    newest_ms: f64,
    path_ms: f64,
    target_buffer_ms: f64,
    arrivals: VecDeque<Arrival>,
    lateness: Vec<f64>,
    last_read_ms: Option<f64>,
}

impl Default for PlayoutClock {
    fn default() -> Self {
        Self {
            display_ms: 0.0,
            buffer_ms: MIN_BUFFER_MS,
            underrun: 0.0,
            newest_ms: 0.0,
            path_ms: 0.0,
            target_buffer_ms: MIN_BUFFER_MS,
            arrivals: VecDeque::new(),
            lateness: Vec::new(),
            last_read_ms: None,
        }
    }
}

impl PlayoutClock {
    pub fn reset(&mut self, tick: u64, now_ms: f64) {
        self.newest_ms = tick as f64 * SIMULATION_STEP_MS;
        self.path_ms = now_ms - self.newest_ms;
        self.arrivals.clear();
        self.arrivals.push_back(Arrival {
            at_ms: now_ms,
            path_ms: self.path_ms,
        });
        self.buffer_ms = MIN_BUFFER_MS;
        self.target_buffer_ms = MIN_BUFFER_MS;
        self.display_ms = self.newest_ms - self.buffer_ms;
        self.last_read_ms = None;
        self.underrun = 0.0;
    }

    /// Call once per received message with its newest tick. Earlier frames of the same
    /// batch were simulated sooner, not delivered later, so they must not count as late.
    pub fn arrive(&mut self, tick: u64, now_ms: f64) {
        let server_ms = tick as f64 * SIMULATION_STEP_MS;
        let sample = now_ms - server_ms;
        self.newest_ms = self.newest_ms.max(server_ms);
        let elapsed = self
            .arrivals
            .back()
            .map_or(0.0, |previous| (now_ms - previous.at_ms).max(0.0));
        self.path_ms = sample.min(self.path_ms + PATH_RISE_MS_PER_SECOND * elapsed / 1000.0);
        self.arrivals.push_back(Arrival {
            at_ms: now_ms,
            path_ms: sample,
        });
        let window_start = now_ms - LATENESS_WINDOW_MS;
        while self
            .arrivals
            .pop_front_if(|arrival| arrival.at_ms < window_start)
            .is_some()
        {}
        self.lateness.clear();
        self.lateness.extend(
            self.arrivals
                .iter()
                .map(|arrival| arrival.path_ms - self.path_ms),
        );
        self.lateness.sort_unstable_by(f64::total_cmp);
        let percentile = self.lateness
            [((self.lateness.len() - 1) as f64 * LATENESS_PERCENTILE).floor() as usize];
        self.target_buffer_ms = (MIN_BUFFER_MS + percentile).clamp(MIN_BUFFER_MS, MAX_BUFFER_MS);
        // A stall grows the buffer immediately; recovery gives the delay back slowly in read().
        self.buffer_ms = self.buffer_ms.max(self.target_buffer_ms);
    }

    /// Server time the newest arrival would carry if it had taken the fastest recent path.
    pub fn path_server_ms(&self, now_ms: f64) -> f64 {
        now_ms - self.path_ms
    }

    /// The server time to display at `now_ms`.
    pub fn read(&mut self, now_ms: f64) -> f64 {
        let dt = self
            .last_read_ms
            .map_or(0.0, |last| (now_ms - last).clamp(0.0, MAX_FRAME_MS));
        self.last_read_ms = Some(now_ms);
        if self.buffer_ms > self.target_buffer_ms {
            self.buffer_ms = self
                .target_buffer_ms
                .max(self.buffer_ms - BUFFER_SHRINK_MS_PER_SECOND * dt / 1000.0);
        }
        let target = now_ms - self.path_ms - self.buffer_ms;
        let error = target - (self.display_ms + dt);
        if error > SNAP_MS {
            self.display_ms = target;
        } else {
            let slew = (error / SLEW_RESPONSE_MS).clamp(-MAX_SLEW, MAX_SLEW);
            self.display_ms += dt * (1.0 + slew);
        }
        self.display_ms = self.display_ms.min(self.newest_ms + MAX_UNDERRUN_MS);
        let starved = if self.display_ms > self.newest_ms {
            1.0
        } else {
            0.0
        };
        self.underrun += (starved - self.underrun) * (1.0 - (-dt / UNDERRUN_AVERAGE_MS).exp());
        self.display_ms
    }
}
