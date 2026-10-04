//! Per-second readings, 10-second `/stats` samples, minute summaries and room lifecycle
//! lines. Observation only: nothing here feeds back into rooms.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;

use crate::process_stats;
use crate::room_list::RoomPhase;
use crate::session::{MessageCounts, RoomActivity, RoomSample};
use crate::tcp_path::{TcpReading, percent};

/// How often the monitor takes a live reading for the dashboard.
pub const READING_INTERVAL_MS: u64 = 1000;
/// Readings per `/stats` sample (ten seconds).
pub const READINGS_PER_SAMPLE: u32 = 10;
/// Samples between summary log lines while rooms are active (one minute).
const SAMPLES_PER_SUMMARY: u32 = 6;
/// Live readings kept for the dashboard charts (five minutes).
pub const HISTORY_READINGS: usize = 300;
/// Lifecycle events kept for the dashboard log.
pub const RECENT_EVENTS: usize = 100;
/// The runtime lag probe's timer interval; `/stats` delay figures include it, as Node's
/// event-loop delay histogram did, and the dashboard's lag figures subtract it.
pub const LOOP_DELAY_RESOLUTION_MS: f64 = 10.0;

const MB: f64 = 1024.0 * 1024.0;

fn round(value: f64, digits: i32) -> f64 {
    let scale = 10f64.powi(digits);
    (value * scale).round() / scale
}
fn kilobytes(bytes: u64, seconds: f64) -> f64 {
    round(bytes as f64 / 1024.0 / seconds, 1)
}
fn megabytes(bytes: u64) -> f64 {
    round(bytes as f64 / MB, 1)
}
/// Running traffic totals keep 1 KB resolution, so the dashboard can show a quiet
/// server's first kilobytes instead of "0 MB".
fn total_megabytes(bytes: u64) -> f64 {
    round(bytes as f64 / MB, 3)
}
fn cpu_percent(micros: u64, seconds: f64) -> f64 {
    round(micros as f64 / 1e4 / seconds, 1)
}
fn per_second(counts: &MessageCounts, seconds: f64) -> BTreeMap<&'static str, f64> {
    counts
        .iter()
        .map(|(kind, count)| (*kind, round(*count as f64 / seconds, 1)))
        .collect()
}
fn add_counts(total: &mut MessageCounts, counts: &MessageCounts) {
    for (kind, count) in counts {
        *total.entry(kind).or_default() += count;
    }
}

/// `90s` style durations for log lines: seconds below a minute, then `Nm Ss`.
pub fn duration(ms: u64) -> String {
    let seconds = (ms + 500) / 1000;
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m {}s", seconds / 60, seconds % 60)
    }
}

/// ` | rtt 85 ms, 31 of 7812 segments resent`: the connection's TCP figures, if measured.
fn tcp_note(tcp: &Option<TcpReading>) -> String {
    tcp.map_or_else(String::new, |reading| {
        format!(
            " | rtt {} ms, {} of {} segments resent",
            reading.rtt_ms().round(),
            reading.retransmitted_segments,
            reading.data_segments_sent
        )
    })
}

fn describe(event: &RoomActivity) -> String {
    match event {
        RoomActivity::Created => "created".into(),
        RoomActivity::Joined { players } => format!("player joined ({players} connected)"),
        RoomActivity::Left { players, code, tcp } => match code {
            Some(code) => format!(
                "player disconnected (code {code}) ({players} connected){}",
                tcp_note(tcp)
            ),
            None => format!("player disconnected ({players} connected){}", tcp_note(tcp)),
        },
        RoomActivity::Closed {
            players,
            code: 1000,
            tcp,
            ..
        } => format!("player left ({players} connected){}", tcp_note(tcp)),
        RoomActivity::Closed {
            players,
            code,
            reason,
            tcp,
        } => {
            format!(
                "server closed a socket: {code} {reason} ({players} connected){}",
                tcp_note(tcp)
            )
        }
    }
}

/// `2026-09-28T12:34:56.789Z` for Unix milliseconds, like `Date.prototype.toISOString`.
pub fn iso_time(unix_ms: u64) -> String {
    let days = (unix_ms / 86_400_000) as i64;
    let ms_of_day = unix_ms % 86_400_000;
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        ms_of_day / 3_600_000,
        ms_of_day / 60_000 % 60,
        ms_of_day / 1000 % 60,
        ms_of_day % 1000
    )
}

/// One room's state with its load as rates.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomLoad {
    pub room: String,
    pub map_mode: String,
    pub phase: RoomPhase,
    pub players: u32,
    pub seats: u32,
    pub sockets: u32,
    pub time_left: u32,
    pub scores: [u32; 2],
    pub age_seconds: u64,
    pub tick: u64,
    pub debt_ms: f64,
    pub tick_avg_ms: f64,
    pub tick_max_ms: f64,
    #[serde(rename = "sentKBps")]
    pub sent_kbps: f64,
    #[serde(rename = "receivedKBps")]
    pub received_kbps: f64,
    /// The joined players' lowest TCP round trips (zero where unmeasured).
    pub rtt_p50_ms: f64,
    pub rtt_max_ms: f64,
    /// Share of the data segments sent to the joined players since they connected that
    /// were retransmissions.
    pub retransmit_percent: f64,
    /// Input lapses in this match so far.
    pub input_lapses: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub rooms_created: u64,
    pub joins: u64,
    #[serde(rename = "sentMB")]
    pub sent_mb: f64,
    #[serde(rename = "receivedMB")]
    pub received_mb: f64,
    #[serde(rename = "wireSentMB")]
    pub wire_sent_mb: f64,
    #[serde(rename = "wireReceivedMB")]
    pub wire_received_mb: f64,
    pub data_segments_sent: u64,
    pub retransmitted_segments: u64,
    pub input_lapses: u64,
}

/// `/stats`: the last ten seconds summed from one-second readings, plus totals.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStats {
    pub sampled_at: String,
    pub window_seconds: f64,
    pub uptime_seconds: u64,
    pub rooms: u32,
    pub players: u32,
    pub sockets: u32,
    #[serde(rename = "sentKBps")]
    pub sent_kbps: f64,
    #[serde(rename = "receivedKBps")]
    pub received_kbps: f64,
    #[serde(rename = "wireSentKBps")]
    pub wire_sent_kbps: f64,
    #[serde(rename = "wireReceivedKBps")]
    pub wire_received_kbps: f64,
    /// Room sockets' lowest TCP round trips at the sample.
    pub rtt_p50_ms: f64,
    pub rtt_max_ms: f64,
    /// Share of the data segments sent to room sockets in the window that were
    /// retransmissions.
    pub retransmit_percent: f64,
    /// Held movement or fire that ran out before the player's next input arrived, in
    /// the window.
    pub input_lapses: u64,
    pub cpu_percent: f64,
    #[serde(rename = "rssMB")]
    pub rss_mb: f64,
    /// Bytes the server currently has allocated (Node: JavaScript heap used).
    #[serde(rename = "heapUsedMB")]
    pub heap_used_mb: f64,
    /// Most bytes allocated at once since start (Node: JavaScript heap reserved).
    #[serde(rename = "heapTotalMB")]
    pub heap_total_mb: f64,
    /// Intervals of the runtime's 10 ms lag probe, interval included.
    pub loop_delay_p99_ms: f64,
    pub loop_delay_max_ms: f64,
    pub totals: Totals,
    pub room_list: Vec<RoomLoad>,
}

/// One second of process and room load, as kept in the chart history.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LivePoint {
    pub at_ms: u64,
    pub rooms: u32,
    pub players: u32,
    pub sockets: u32,
    pub cpu_percent: f64,
    /// Share of the second the runtime's worker threads spent running tasks rather than
    /// parked (the average over workers; Node: event-loop utilization).
    pub loop_busy_percent: f64,
    /// How late the runtime ran its 10 ms lag probe, excluding the interval itself.
    pub loop_lag_p50_ms: f64,
    pub loop_lag_p90_ms: f64,
    pub loop_lag_p99_ms: f64,
    pub loop_lag_max_ms: f64,
    /// Garbage-collection pauses; always zero in Rust, kept for the dashboard.
    pub gc_ms: f64,
    #[serde(rename = "rssMB")]
    pub rss_mb: f64,
    #[serde(rename = "heapUsedMB")]
    pub heap_used_mb: f64,
    #[serde(rename = "heapTotalMB")]
    pub heap_total_mb: f64,
    /// Room messages before WebSocket compression.
    #[serde(rename = "sentKBps")]
    pub sent_kbps: f64,
    #[serde(rename = "receivedKBps")]
    pub received_kbps: f64,
    /// Bytes through the sockets after compression, including frame and handshake bytes.
    #[serde(rename = "wireSentKBps")]
    pub wire_sent_kbps: f64,
    #[serde(rename = "wireReceivedKBps")]
    pub wire_received_kbps: f64,
    /// Room sockets' lowest TCP round trips.
    pub rtt_p50_ms: f64,
    pub rtt_max_ms: f64,
    /// Share of this second's data segments to room sockets that were retransmissions.
    pub retransmit_percent: f64,
    /// Input lapses in this second.
    pub input_lapses: u64,
    /// Messages per second by type.
    pub sent_messages: BTreeMap<&'static str, f64>,
    pub received_messages: BTreeMap<&'static str, f64>,
    /// Slowest room timer callback in this second.
    pub tick_max_ms: f64,
}

/// One second of load for the dashboard.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveReading {
    #[serde(flatten)]
    pub point: LivePoint,
    pub room_list: Vec<RoomLoad>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorEvent {
    /// Increases by one per event, so a reader can ask for what it has not seen.
    pub id: u64,
    pub at_ms: u64,
    pub room: String,
    pub message: String,
}

/// Bytes written to and read from room sockets since the server started.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireBytes {
    pub sent: u64,
    pub received: u64,
}

/// TCP data segments sent to room sockets since the server started, and how many were
/// retransmissions (Linux only; zero elsewhere).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TcpSegments {
    pub sent: u64,
    pub retransmitted: u64,
}

/// What the server gathers from its rooms and sockets for one reading.
#[derive(Clone, Debug, Default)]
pub struct MonitorInput {
    pub samples: Vec<RoomSample>,
    pub sockets: u32,
    pub wire: WireBytes,
    pub segments: TcpSegments,
}

/// Intervals measured by the runtime lag probe (see `server::run_lag_probe`), kept
/// separately for the one-second reading and the ten-second sample.
#[derive(Default)]
pub struct LagRecorder {
    windows: Mutex<(Vec<f64>, Vec<f64>)>,
}

impl LagRecorder {
    pub fn record(&self, interval_ms: f64) {
        let mut windows = self.windows.lock().expect("lag recorder");
        windows.0.push(interval_ms);
        windows.1.push(interval_ms);
    }
    fn take_reading(&self) -> Vec<f64> {
        std::mem::take(&mut self.windows.lock().expect("lag recorder").0)
    }
    fn take_sample(&self) -> Vec<f64> {
        std::mem::take(&mut self.windows.lock().expect("lag recorder").1)
    }
}

/// Nearest-rank percentile of `values`, which it sorts; zero without values.
fn percentile(values: &mut [f64], percent: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    let rank = ((percent / 100.0) * values.len() as f64).ceil() as usize;
    values[rank.clamp(1, values.len()) - 1]
}

/// Where log lines go (standard output, the systemd journal, in the binary).
pub type LogSink = Box<dyn Fn(&str) + Send>;
/// Unix milliseconds.
pub type WallClock = Arc<dyn Fn() -> u64 + Send + Sync>;
/// Total busy time across the runtime's worker threads, and how many there are.
pub type BusyProbe = Box<dyn Fn() -> (Duration, usize) + Send>;

#[derive(Default)]
struct RoomWindow {
    sent_bytes: u64,
    received_bytes: u64,
    ticks: u32,
    tick_total_ms: f64,
    tick_max_ms: f64,
}

#[derive(Default)]
struct Counts {
    rooms_created: u64,
    joins: u64,
    sent_bytes: u64,
    received_bytes: u64,
    wire_sent_bytes: u64,
    wire_received_bytes: u64,
    segments: TcpSegments,
    input_lapses: u64,
}

#[derive(Default)]
struct WindowCounts {
    sent: u64,
    received: u64,
    wire_sent: u64,
    wire_received: u64,
    segments: TcpSegments,
    input_lapses: u64,
}

impl TcpSegments {
    fn since(self, earlier: TcpSegments) -> TcpSegments {
        TcpSegments {
            sent: self.sent.saturating_sub(earlier.sent),
            retransmitted: self.retransmitted.saturating_sub(earlier.retransmitted),
        }
    }

    fn add(&mut self, more: TcpSegments) {
        self.sent += more.sent;
        self.retransmitted += more.retransmitted;
    }

    fn retransmit_percent(self) -> f64 {
        round(percent(self.retransmitted, self.sent), 2)
    }
}

/// Nearest-rank median and maximum of round trips, rounded; zeros without any.
fn rtt_figures(rtt_ms: &mut [f64]) -> (f64, f64) {
    (
        round(percentile(rtt_ms, 50.0), 1),
        round(percentile(rtt_ms, 100.0), 1),
    )
}

/// Reads room and process load every second for the dashboard, sums ten readings into
/// each `/stats` sample and six samples into a once-a-minute summary, and logs room
/// lifecycle lines.
pub struct ServerMonitor {
    pub latest: Option<ServerStats>,
    history: VecDeque<LivePoint>,
    events: VecDeque<MonitorEvent>,
    started_ms: u64,
    clock: WallClock,
    log: LogSink,
    lag: Arc<LagRecorder>,
    busy: BusyProbe,
    readings: u32,
    read_ms: u64,
    read_cpu: u64,
    read_busy: Duration,
    read_wire: WireBytes,
    read_segments: TcpSegments,
    sampled_ms: u64,
    sample_cpu: u64,
    /// Load per room and in total since the last sample, summed from readings.
    window: HashMap<String, RoomWindow>,
    window_counts: WindowCounts,
    samples_since_summary: u32,
    active_since_summary: bool,
    counts: Counts,
}

impl ServerMonitor {
    pub fn new(clock: WallClock, log: LogSink) -> Self {
        let now = clock();
        let cpu = process_stats::cpu_micros();
        Self {
            latest: None,
            history: VecDeque::new(),
            events: VecDeque::new(),
            started_ms: now,
            clock,
            log,
            lag: Arc::default(),
            busy: Box::new(|| (Duration::ZERO, 1)),
            readings: 0,
            read_ms: now,
            read_cpu: cpu,
            read_busy: Duration::ZERO,
            read_wire: WireBytes::default(),
            read_segments: TcpSegments::default(),
            sampled_ms: now,
            sample_cpu: cpu,
            window: HashMap::new(),
            window_counts: WindowCounts::default(),
            samples_since_summary: 0,
            active_since_summary: false,
            counts: Counts::default(),
        }
    }

    /// Attaches the runtime probes; without them lag and busy figures read zero.
    pub fn with_runtime(mut self, lag: Arc<LagRecorder>, busy: BusyProbe) -> Self {
        self.read_busy = busy().0;
        self.lag = lag;
        self.busy = busy;
        self
    }

    pub fn started_ms(&self) -> u64 {
        self.started_ms
    }

    /// Recent readings, oldest first.
    pub fn history(&self) -> &VecDeque<LivePoint> {
        &self.history
    }

    /// Recent lifecycle events, oldest first.
    pub fn events(&self) -> &VecDeque<MonitorEvent> {
        &self.events
    }

    /// The dashboard log always has at least one line, even on a server nobody has joined.
    pub fn start(&mut self) {
        self.add_event("", "server started".into());
    }

    pub fn activity(&mut self, room: &str, event: &RoomActivity) {
        match event {
            RoomActivity::Created => self.counts.rooms_created += 1,
            RoomActivity::Joined { .. } => self.counts.joins += 1,
            _ => {}
        }
        self.record(room, describe(event));
    }

    pub fn ended(&mut self, room: &str, reason: &str, age_ms: u64) {
        self.record(room, format!("ended: {reason} after {}", duration(age_ms)));
    }

    pub fn totals(&self) -> Totals {
        Totals {
            rooms_created: self.counts.rooms_created,
            joins: self.counts.joins,
            sent_mb: total_megabytes(self.counts.sent_bytes),
            received_mb: total_megabytes(self.counts.received_bytes),
            wire_sent_mb: total_megabytes(self.counts.wire_sent_bytes),
            wire_received_mb: total_megabytes(self.counts.wire_received_bytes),
            data_segments_sent: self.counts.segments.sent,
            retransmitted_segments: self.counts.segments.retransmitted,
            input_lapses: self.counts.input_lapses,
        }
    }

    /// The once-a-second timer: a reading, and every tenth time a full sample too.
    pub fn tick(&mut self, input: MonitorInput) -> LiveReading {
        self.readings += 1;
        if self.readings.is_multiple_of(READINGS_PER_SAMPLE) {
            self.sample(input).0
        } else {
            self.read(input)
        }
    }

    /// Takes the load since the previous reading.
    pub fn read(&mut self, input: MonitorInput) -> LiveReading {
        let now = (self.clock)();
        let seconds = (now.saturating_sub(self.read_ms) as f64 / 1000.0).max(0.001);
        let cpu = process_stats::cpu_micros();
        let (busy, workers) = (self.busy)();
        let wire_sent = input.wire.sent.saturating_sub(self.read_wire.sent);
        let wire_received = input.wire.received.saturating_sub(self.read_wire.received);
        let segments = input.segments.since(self.read_segments);
        let mut sent_messages = MessageCounts::new();
        let mut received_messages = MessageCounts::new();
        let (mut sent, mut received, mut tick_max_ms) = (0u64, 0u64, 0f64);
        let mut input_lapses = 0;
        let mut rtt_ms: Vec<f64> = Vec::new();
        for room in &input.samples {
            sent += room.sent_bytes;
            received += room.received_bytes;
            input_lapses += room.input_lapses;
            rtt_ms.extend(&room.rtt_ms);
            add_counts(&mut sent_messages, &room.sent_messages);
            add_counts(&mut received_messages, &room.received_messages);
            tick_max_ms = tick_max_ms.max(room.tick_max_ms);
            let window = self.window.entry(room.room.clone()).or_default();
            window.sent_bytes += room.sent_bytes;
            window.received_bytes += room.received_bytes;
            window.ticks += room.ticks;
            window.tick_total_ms += room.tick_avg_ms * f64::from(room.ticks);
            window.tick_max_ms = window.tick_max_ms.max(room.tick_max_ms);
        }
        self.window_counts.sent += sent;
        self.window_counts.received += received;
        self.window_counts.wire_sent += wire_sent;
        self.window_counts.wire_received += wire_received;
        self.window_counts.segments.add(segments);
        self.window_counts.input_lapses += input_lapses;
        self.counts.sent_bytes += sent;
        self.counts.received_bytes += received;
        self.counts.wire_sent_bytes += wire_sent;
        self.counts.wire_received_bytes += wire_received;
        self.counts.segments.add(segments);
        self.counts.input_lapses += input_lapses;
        let (rtt_p50_ms, rtt_max_ms) = rtt_figures(&mut rtt_ms);
        let mut lag = self.lag.take_reading();
        let lag_ms = |value: f64| round((value - LOOP_DELAY_RESOLUTION_MS).max(0.0), 1);
        let (p50, p90, p99, max) = if lag.is_empty() {
            (0.0, 0.0, 0.0, 0.0)
        } else {
            (
                lag_ms(percentile(&mut lag, 50.0)),
                lag_ms(percentile(&mut lag, 90.0)),
                lag_ms(percentile(&mut lag, 99.0)),
                lag_ms(percentile(&mut lag, 100.0)),
            )
        };
        let busy_share =
            busy.saturating_sub(self.read_busy).as_secs_f64() / (seconds * workers.max(1) as f64);
        let point = LivePoint {
            at_ms: now,
            rooms: input.samples.len() as u32,
            players: input.samples.iter().map(|room| room.players).sum(),
            sockets: input.sockets,
            cpu_percent: cpu_percent(cpu.saturating_sub(self.read_cpu), seconds),
            loop_busy_percent: round((busy_share * 100.0).clamp(0.0, 100.0), 1),
            loop_lag_p50_ms: p50,
            loop_lag_p90_ms: p90,
            loop_lag_p99_ms: p99,
            loop_lag_max_ms: max,
            gc_ms: 0.0,
            rss_mb: megabytes(process_stats::rss_bytes()),
            heap_used_mb: megabytes(process_stats::heap_used_bytes() as u64),
            heap_total_mb: megabytes(process_stats::heap_peak_bytes() as u64),
            sent_kbps: kilobytes(sent, seconds),
            received_kbps: kilobytes(received, seconds),
            wire_sent_kbps: kilobytes(wire_sent, seconds),
            wire_received_kbps: kilobytes(wire_received, seconds),
            rtt_p50_ms,
            rtt_max_ms,
            retransmit_percent: segments.retransmit_percent(),
            input_lapses,
            sent_messages: per_second(&sent_messages, seconds),
            received_messages: per_second(&received_messages, seconds),
            tick_max_ms: round(tick_max_ms, 2),
        };
        // Message counts stay server-wide; the dashboard charts them by type.
        let room_list = input
            .samples
            .into_iter()
            .map(|mut room| {
                let (rtt_p50_ms, rtt_max_ms) = rtt_figures(&mut room.rtt_ms);
                RoomLoad {
                    rtt_p50_ms,
                    rtt_max_ms,
                    retransmit_percent: TcpSegments {
                        sent: room.data_segments_sent,
                        retransmitted: room.retransmitted_segments,
                    }
                    .retransmit_percent(),
                    input_lapses: room.match_input_lapses,
                    debt_ms: round(room.debt_ms, 1),
                    tick_avg_ms: round(room.tick_avg_ms, 2),
                    tick_max_ms: round(room.tick_max_ms, 2),
                    sent_kbps: kilobytes(room.sent_bytes, seconds),
                    received_kbps: kilobytes(room.received_bytes, seconds),
                    room: room.room,
                    map_mode: room.map_mode,
                    phase: room.phase,
                    players: room.players,
                    seats: room.seats,
                    sockets: room.sockets,
                    time_left: room.time_left,
                    scores: room.scores,
                    age_seconds: room.age_seconds,
                    tick: room.tick,
                }
            })
            .collect();
        self.read_ms = now;
        self.read_wire = input.wire;
        self.read_segments = input.segments;
        self.read_cpu = cpu;
        self.read_busy = busy;
        self.history.push_back(point.clone());
        self.history.retain_back(HISTORY_READINGS);
        LiveReading { point, room_list }
    }

    /// Takes a reading and returns it with the `/stats` figures since the previous sample.
    pub fn sample(&mut self, input: MonitorInput) -> (LiveReading, ServerStats) {
        let reading = self.read(input);
        let at_ms = reading.point.at_ms;
        let seconds = (at_ms.saturating_sub(self.sampled_ms) as f64 / 1000.0).max(0.001);
        let mut delays = self.lag.take_sample();
        let stats = ServerStats {
            sampled_at: iso_time(at_ms),
            window_seconds: round(seconds, 1),
            uptime_seconds: (at_ms.saturating_sub(self.started_ms) + 500) / 1000,
            rooms: reading.point.rooms,
            players: reading.point.players,
            sockets: reading.point.sockets,
            sent_kbps: kilobytes(self.window_counts.sent, seconds),
            received_kbps: kilobytes(self.window_counts.received, seconds),
            wire_sent_kbps: kilobytes(self.window_counts.wire_sent, seconds),
            wire_received_kbps: kilobytes(self.window_counts.wire_received, seconds),
            rtt_p50_ms: reading.point.rtt_p50_ms,
            rtt_max_ms: reading.point.rtt_max_ms,
            retransmit_percent: self.window_counts.segments.retransmit_percent(),
            input_lapses: self.window_counts.input_lapses,
            cpu_percent: cpu_percent(self.read_cpu.saturating_sub(self.sample_cpu), seconds),
            rss_mb: reading.point.rss_mb,
            heap_used_mb: reading.point.heap_used_mb,
            heap_total_mb: reading.point.heap_total_mb,
            loop_delay_p99_ms: round(percentile(&mut delays, 99.0), 1),
            loop_delay_max_ms: round(percentile(&mut delays, 100.0), 1),
            totals: self.totals(),
            room_list: reading
                .room_list
                .iter()
                .map(|room| {
                    let window = self.window.get(&room.room);
                    let (ticks, total, max, sent, received) =
                        window.map_or((0, 0.0, 0.0, 0, 0), |window| {
                            (
                                window.ticks,
                                window.tick_total_ms,
                                window.tick_max_ms,
                                window.sent_bytes,
                                window.received_bytes,
                            )
                        });
                    RoomLoad {
                        tick_avg_ms: round(
                            if ticks > 0 {
                                total / f64::from(ticks)
                            } else {
                                0.0
                            },
                            2,
                        ),
                        tick_max_ms: round(max, 2),
                        sent_kbps: kilobytes(sent, seconds),
                        received_kbps: kilobytes(received, seconds),
                        ..room.clone()
                    }
                })
                .collect(),
        };
        self.sampled_ms = at_ms;
        self.sample_cpu = self.read_cpu;
        self.window.clear();
        self.window_counts = WindowCounts::default();
        self.latest = Some(stats.clone());
        if stats.rooms > 0 {
            self.active_since_summary = true;
        }
        self.samples_since_summary += 1;
        if self.samples_since_summary >= SAMPLES_PER_SUMMARY {
            self.samples_since_summary = 0;
            // Quiet servers log one final idle summary, then stay silent until players return.
            if self.active_since_summary {
                self.summarize(&stats);
            }
            self.active_since_summary = stats.rooms > 0;
        }
        (reading, stats)
    }

    fn record(&mut self, room: &str, message: String) {
        self.active_since_summary = true;
        (self.log)(&format!("room {room} {message}"));
        self.add_event(room, message);
    }

    fn add_event(&mut self, room: &str, message: String) {
        let id = self.events.back().map_or(0, |event| event.id) + 1;
        let at_ms = (self.clock)();
        self.events.push_back(MonitorEvent {
            id,
            at_ms,
            room: room.to_string(),
            message,
        });
        self.events.retain_back(RECENT_EVENTS);
    }

    fn summarize(&self, stats: &ServerStats) {
        (self.log)(&format!(
            "stats: {} rooms, {} players, {} sockets | out {} KB/s, in {} KB/s | \
             rtt p50 {} ms, max {} ms, resent {}%, input lapses {} | cpu {}% | \
             rss {} MB, heap {}/{} MB | loop delay p99 {} ms, max {} ms",
            stats.rooms,
            stats.players,
            stats.sockets,
            stats.sent_kbps,
            stats.received_kbps,
            stats.rtt_p50_ms,
            stats.rtt_max_ms,
            stats.retransmit_percent,
            stats.input_lapses,
            stats.cpu_percent,
            stats.rss_mb,
            stats.heap_used_mb,
            stats.heap_total_mb,
            stats.loop_delay_p99_ms,
            stats.loop_delay_max_ms
        ));
        for room in &stats.room_list {
            (self.log)(&format!(
                "  {} {} {} {}/{} players {} left {}-{} | tick avg {} ms, max {} ms, debt {} ms | out {} KB/s | \
                 rtt p50 {} ms, max {} ms, resent {}%, input lapses {}",
                room.room,
                room.map_mode,
                room.phase.as_str(),
                room.players,
                room.seats,
                duration(u64::from(room.time_left) * 1000),
                room.scores[0],
                room.scores[1],
                room.tick_avg_ms,
                room.tick_max_ms,
                room.debt_ms,
                room.sent_kbps,
                room.rtt_p50_ms,
                room.rtt_max_ms,
                room.retransmit_percent,
                room.input_lapses
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn room() -> RoomSample {
        RoomSample {
            room: "ABCDEFGH".into(),
            map_mode: "harbor".into(),
            phase: RoomPhase::Playing,
            players: 3,
            seats: 4,
            sockets: 3,
            time_left: 125,
            scores: [4, 7],
            age_seconds: 60,
            tick: 3600,
            debt_ms: 0.0,
            ticks: 20,
            tick_avg_ms: 2.5,
            tick_max_ms: 9.0,
            sent_bytes: 1024 * 100,
            received_bytes: 1024,
            sent_messages: [("snapshot", 20), ("lobby", 1)].into(),
            received_messages: [("input", 30)].into(),
            rtt_ms: vec![120.0, 40.0, 85.4],
            data_segments_sent: 1000,
            retransmitted_segments: 5,
            input_lapses: 1,
            match_input_lapses: 4,
        }
    }

    fn input(samples: Vec<RoomSample>, sockets: u32) -> MonitorInput {
        MonitorInput {
            samples,
            sockets,
            ..MonitorInput::default()
        }
    }

    fn manual_clock(start: u64) -> (Arc<AtomicU64>, WallClock) {
        let now = Arc::new(AtomicU64::new(start));
        let reader = now.clone();
        (now, Arc::new(move || reader.load(Ordering::Relaxed)))
    }

    fn logger() -> (Arc<Mutex<Vec<String>>>, LogSink) {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let sink = lines.clone();
        (
            lines,
            Box::new(move |line| sink.lock().unwrap().push(line.to_string())),
        )
    }

    #[test]
    fn reads_every_second_and_sums_ten_readings_into_each_sample() {
        let (now, clock) = manual_clock(1_000_000);
        let mut monitor = ServerMonitor::new(clock, Box::new(|_| {}));
        // Alternate a light and a heavy second so the sample must weight and take the worst.
        let second = |index: u32| RoomSample {
            tick_avg_ms: if index % 2 == 1 { 4.0 } else { 2.0 },
            tick_max_ms: if index % 2 == 1 { 12.0 } else { 3.0 },
            ..room()
        };
        let mut heard = Vec::new();
        for index in 0..9 {
            now.fetch_add(1000, Ordering::Relaxed);
            let reading = monitor.tick(input(vec![second(index)], 3));
            assert_eq!(
                reading.room_list[0].tick_max_ms,
                if index % 2 == 1 { 12.0 } else { 3.0 }
            );
            heard.push(reading);
        }
        now.fetch_add(1000, Ordering::Relaxed);
        heard.push(monitor.tick(input(vec![second(9)], 3)));
        let stats = monitor
            .latest
            .clone()
            .expect("the tenth reading is a sample");
        assert_eq!(stats.room_list[0].tick_avg_ms, 3.0);
        assert_eq!(stats.room_list[0].tick_max_ms, 12.0);
        assert_eq!(
            stats.totals.sent_mb, 0.977,
            "ten readings of 100 KB, to the nearest KB"
        );
        assert_eq!(stats.window_seconds, 10.0);
        assert_eq!(heard.len(), 10, "a sample is also a reading");
        assert_eq!(monitor.history().len(), 10);
        let point = serde_json::to_value(&monitor.history()[0]).unwrap();
        assert!(point.get("roomList").is_none(), "history keeps totals only");

        now.fetch_add(1000, Ordering::Relaxed);
        assert_eq!(
            monitor.sample(input(vec![second(10)], 3)).1.room_list[0].tick_max_ms,
            3.0,
            "each sample starts a new window"
        );
        for _ in 0..HISTORY_READINGS {
            monitor.read(input(vec![], 0));
        }
        assert_eq!(monitor.history().len(), HISTORY_READINGS);
    }

    #[test]
    fn adds_message_types_across_rooms_and_turns_wire_counters_into_rates() {
        let (now, clock) = manual_clock(1_000_000);
        let mut monitor = ServerMonitor::new(clock, Box::new(|_| {}));
        let other = RoomSample {
            room: "IJKLMNOP".into(),
            sent_messages: [("snapshot", 20), ("pong", 1)].into(),
            received_messages: MessageCounts::new(),
            ..room()
        };
        now.fetch_add(1000, Ordering::Relaxed);
        let reading = monitor.read(MonitorInput {
            samples: vec![room(), other.clone()],
            sockets: 6,
            wire: WireBytes {
                sent: 20 * 1024,
                received: 2 * 1024,
            },
            segments: TcpSegments::default(),
        });
        let expected: BTreeMap<&str, f64> =
            [("snapshot", 40.0), ("lobby", 1.0), ("pong", 1.0)].into();
        assert_eq!(reading.point.sent_messages, expected);
        assert_eq!(reading.point.received_messages, [("input", 30.0)].into());
        assert_eq!(reading.point.wire_sent_kbps, 20.0);
        assert_eq!(reading.point.wire_received_kbps, 2.0);
        let json = serde_json::to_value(&reading).unwrap();
        assert!(
            json["roomList"][0].get("sentMessages").is_none(),
            "counts are server-wide only"
        );
        now.fetch_add(1000, Ordering::Relaxed);
        let next = monitor.read(MonitorInput {
            samples: vec![],
            sockets: 0,
            wire: WireBytes {
                sent: 30 * 1024,
                received: 2 * 1024,
            },
            segments: TcpSegments::default(),
        });
        assert_eq!(
            next.point.wire_sent_kbps, 10.0,
            "readings take the difference of running totals"
        );
    }

    #[test]
    fn reports_round_trips_retransmits_and_input_lapses_per_room_and_server() {
        let (now, clock) = manual_clock(1_000_000);
        let mut monitor = ServerMonitor::new(clock, Box::new(|_| {}));
        let quiet = RoomSample {
            room: "IJKLMNOP".into(),
            rtt_ms: vec![30.0],
            data_segments_sent: 0,
            retransmitted_segments: 0,
            input_lapses: 0,
            match_input_lapses: 0,
            ..room()
        };
        let mut segments = TcpSegments::default();
        for second in 1..=10 {
            now.fetch_add(1000, Ordering::Relaxed);
            segments.sent += 400;
            segments.retransmitted += if second == 1 { 2 } else { 0 };
            let reading = monitor.tick(MonitorInput {
                samples: vec![room(), quiet.clone()],
                sockets: 4,
                wire: WireBytes::default(),
                segments,
            });
            assert_eq!(
                reading.point.input_lapses, 1,
                "lapses since the last reading"
            );
            if second == 1 {
                assert_eq!(reading.point.retransmit_percent, 0.5);
                assert_eq!(reading.point.rtt_p50_ms, 40.0);
                assert_eq!(reading.point.rtt_max_ms, 120.0);
                let busy = &reading.room_list[0];
                assert_eq!((busy.rtt_p50_ms, busy.rtt_max_ms), (85.4, 120.0));
                assert_eq!(busy.retransmit_percent, 0.5, "since each player connected");
                assert_eq!(busy.input_lapses, 4, "the match so far");
                assert_eq!(reading.room_list[1].retransmit_percent, 0.0);
            } else {
                assert_eq!(reading.point.retransmit_percent, 0.0);
            }
        }
        let stats = monitor.latest.clone().expect("ten readings make a sample");
        assert_eq!(
            stats.retransmit_percent, 0.05,
            "2 of 4000 segments in the window"
        );
        assert_eq!(stats.input_lapses, 10);
        assert_eq!(stats.rtt_max_ms, 120.0);
        assert_eq!(stats.totals.data_segments_sent, 4000);
        assert_eq!(stats.totals.retransmitted_segments, 2);
        assert_eq!(stats.totals.input_lapses, 10);
        let json = serde_json::to_value(&stats).unwrap();
        assert_eq!(json["roomList"][0]["rttP50Ms"], 85.4);
        assert_eq!(json["retransmitPercent"], 0.05);
    }

    #[test]
    fn keeps_a_bounded_numbered_list_of_recent_events() {
        let (_, clock) = manual_clock(0);
        let mut monitor = ServerMonitor::new(clock, Box::new(|_| {}));
        for _ in 0..RECENT_EVENTS + 5 {
            monitor.activity("ABCDEFGH", &RoomActivity::Joined { players: 1 });
        }
        assert_eq!(monitor.events().len(), RECENT_EVENTS);
        assert_eq!(monitor.events()[0].id, 6);
        assert_eq!(
            monitor.events().back().unwrap().id,
            RECENT_EVENTS as u64 + 5
        );
        assert_eq!(monitor.events()[0].room, "ABCDEFGH");
        assert_eq!(monitor.events()[0].message, "player joined (1 connected)");
        assert_eq!(monitor.totals().joins, RECENT_EVENTS as u64 + 5);
    }

    #[test]
    fn starts_its_event_list_with_a_startup_line_without_logging_it() {
        let (_, clock) = manual_clock(0);
        let (lines, log) = logger();
        let mut monitor = ServerMonitor::new(clock, log);
        monitor.start();
        let events: Vec<(u64, &str, &str)> = monitor
            .events()
            .iter()
            .map(|event| (event.id, event.room.as_str(), event.message.as_str()))
            .collect();
        assert_eq!(events, [(1, "", "server started")]);
        assert!(
            lines.lock().unwrap().is_empty(),
            "main already logs the listener"
        );
    }

    #[test]
    fn summarizes_each_minute_while_active_then_logs_one_idle_summary() {
        let (now, clock) = manual_clock(0);
        let (lines, log) = logger();
        let mut monitor = ServerMonitor::new(clock, log);
        let mut sample = |rooms: Vec<RoomSample>| {
            now.fetch_add(10_000, Ordering::Relaxed);
            let sockets = rooms.len() as u32 * 3;
            monitor.sample(input(rooms, sockets)).1
        };
        for _ in 0..5 {
            sample(vec![room()]);
        }
        assert!(lines.lock().unwrap().is_empty());
        let stats = sample(vec![room()]);
        assert_eq!(stats.players, 3);
        assert_eq!(stats.room_list[0].room, "ABCDEFGH");
        {
            let lines = lines.lock().unwrap();
            assert_eq!(lines.len(), 2);
            assert!(
                lines[0].starts_with("stats: 1 rooms, 3 players, 3 sockets | out "),
                "{}",
                lines[0]
            );
            assert!(
                lines[1].contains("ABCDEFGH harbor playing 3/4 players 2m 5s left 4-7"),
                "{}",
                lines[1]
            );
        }
        for _ in 0..6 {
            sample(vec![]);
        }
        assert_eq!(lines.lock().unwrap().len(), 3);
        assert!(lines.lock().unwrap()[2].starts_with("stats: 0 rooms, 0 players"));
        for _ in 0..12 {
            sample(vec![]);
        }
        assert_eq!(lines.lock().unwrap().len(), 3);
    }

    #[test]
    fn logs_room_lifecycle_in_plain_lines() {
        let (_, clock) = manual_clock(0);
        let (lines, log) = logger();
        let mut monitor = ServerMonitor::new(clock, log);
        let room = "ABCDEFGH";
        monitor.activity(room, &RoomActivity::Created);
        monitor.activity(room, &RoomActivity::Joined { players: 1 });
        monitor.activity(
            room,
            &RoomActivity::Left {
                players: 0,
                code: Some(1006),
                tcp: Some(TcpReading {
                    rtt_us: 85_400,
                    data_segments_sent: 800,
                    retransmitted_segments: 6,
                }),
            },
        );
        monitor.activity(
            room,
            &RoomActivity::Closed {
                players: 0,
                code: 1000,
                reason: "Left room".into(),
                tcp: None,
            },
        );
        monitor.activity(
            room,
            &RoomActivity::Closed {
                players: 0,
                code: 1008,
                reason: "Join timed out".into(),
                tcp: None,
            },
        );
        monitor.ended(room, "expired", 1_800_000);
        assert_eq!(
            *lines.lock().unwrap(),
            [
                "room ABCDEFGH created",
                "room ABCDEFGH player joined (1 connected)",
                "room ABCDEFGH player disconnected (code 1006) (0 connected) | rtt 85 ms, 6 of 800 segments resent",
                "room ABCDEFGH player left (0 connected)",
                "room ABCDEFGH server closed a socket: 1008 Join timed out (0 connected)",
                "room ABCDEFGH ended: expired after 30m 0s",
            ]
        );
        assert_eq!(monitor.sample(input(vec![], 0)).1.totals.rooms_created, 1);
    }

    #[test]
    fn lag_figures_subtract_the_probe_interval_only_on_the_dashboard() {
        let (now, clock) = manual_clock(0);
        let lag = Arc::new(LagRecorder::default());
        let mut monitor = ServerMonitor::new(clock, Box::new(|_| {}))
            .with_runtime(lag.clone(), Box::new(|| (Duration::ZERO, 1)));
        for interval in [10.0, 10.5, 11.0, 30.0] {
            lag.record(interval);
        }
        now.fetch_add(1000, Ordering::Relaxed);
        let (reading, stats) = monitor.sample(input(vec![], 0));
        assert_eq!(reading.point.loop_lag_p50_ms, 0.5);
        assert_eq!(reading.point.loop_lag_max_ms, 20.0);
        assert_eq!(stats.loop_delay_max_ms, 30.0);
        assert_eq!(stats.loop_delay_p99_ms, 30.0);
    }

    #[test]
    fn formats_times_like_to_iso_string() {
        assert_eq!(iso_time(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_time(1_790_000_000_123), "2026-09-21T14:13:20.123Z");
        assert_eq!(iso_time(951_782_400_000), "2000-02-29T00:00:00.000Z");
        assert_eq!(duration(59_400), "59s");
        assert_eq!(duration(125_000), "2m 5s");
    }
}
