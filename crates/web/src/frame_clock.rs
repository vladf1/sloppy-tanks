//! The page's frame clock: animation-frame timestamps become capped frame deltas
//! ([`FrameTimes`]), fixed simulation steps and the interpolation fraction
//! ([`StepClock`]), and the GPU's backlog decides which frames run and when
//! ([`FramePacing`]). Plain arithmetic, so it is tested natively.

use sloppy_core::sim::data::STEP;

/// Frame deltas are capped so a stalled tab never fast-forwards the match.
pub const MAX_FRAME_DELTA_SECONDS: f64 = 0.1;
const MILLISECONDS_PER_SECOND: f64 = 1000.0;
/// Catch-up after a stall is bounded so one slow frame cannot spiral.
pub const MAX_CATCH_UP_STEPS: u32 = 5;
/// A frame runs at least this long after the last one whatever the backlog reads, so a
/// completion the browser never reports cannot freeze the game.
const MAX_WAIT_MS: f64 = 500.0;
/// A paced frame's time trails its timestamp by at most one 60 Hz frame: the most a
/// frame waits for an animation frame after the GPU frees it.
const MAX_LAG_MS: f64 = 1000.0 / 60.0;
/// Each running frame moves the smoothed interval this share of the way to its own.
const INTERVAL_SMOOTHING: f64 = 0.1;
/// Longer gaps between running frames are stalls, not pace.
const PACED_INTERVAL_LIMIT_MS: f64 = MAX_FRAME_DELTA_SECONDS * MILLISECONDS_PER_SECOND;
/// Each frame moves the averages Stats for nerds shows this share of the way to its own
/// times.
const AVERAGE_SMOOTHING: f64 = 0.05;

/// The animation-frame clock and the timings of the last frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameTimes {
    /// The latest frame timestamp, in milliseconds.
    pub last_ms: Option<f64>,
    pub frame_ms: f64,
    pub sim_ms: f64,
    pub render_ms: f64,
    /// Exponential average of frame time for the fps readout.
    pub average_ms: f64,
    /// Exponential averages of the simulation and render times, for Stats for nerds;
    /// they start at the first times measured.
    pub average_sim_ms: f64,
    pub average_render_ms: f64,
}

impl FrameTimes {
    /// Move the clock to the frame at `now` and return its capped delta in seconds. A
    /// queued timestamp can precede a slow rebuild; time never runs backwards.
    pub fn advance(&mut self, now: f64) -> f64 {
        self.average_frame_work();
        let last = self.last_ms.unwrap_or(now);
        let raw = ((now - last) / MILLISECONDS_PER_SECOND).max(0.0);
        self.last_ms = Some(last.max(now));
        // A clock started infinitely long ago (autoplay's round start) catches up
        // in full without counting as a frame time.
        if !raw.is_finite() {
            self.frame_ms = 0.0;
            return MAX_FRAME_DELTA_SECONDS;
        }
        self.frame_ms = raw * MILLISECONDS_PER_SECOND;
        if raw > 0.0 {
            self.average_ms += (self.frame_ms - self.average_ms) * AVERAGE_SMOOTHING;
        }
        raw.min(MAX_FRAME_DELTA_SECONDS)
    }

    /// The game types time a frame's simulation and drawing as it runs, so the previous
    /// frame's times are final when the next one starts: they join the averages then.
    fn average_frame_work(&mut self) {
        if self.last_ms.is_none() {
            return;
        }
        if self.average_sim_ms == 0.0 && self.average_render_ms == 0.0 {
            self.average_sim_ms = self.sim_ms;
            self.average_render_ms = self.render_ms;
            return;
        }
        self.average_sim_ms += (self.sim_ms - self.average_sim_ms) * AVERAGE_SMOOTHING;
        self.average_render_ms += (self.render_ms - self.average_render_ms) * AVERAGE_SMOOTHING;
    }

    pub fn fps(&self) -> f64 {
        if self.average_ms > 0.0 {
            MILLISECONDS_PER_SECOND / self.average_ms
        } else {
            0.0
        }
    }
}

/// Fixed simulation steps from frame deltas. The drawn pose lies `alpha` of the way
/// from the previous step's pose to the latest, so displayed time advances by exactly
/// each frame's delta whatever the display rate (one step behind the clock), until a
/// frame needs more than [`MAX_CATCH_UP_STEPS`].
#[derive(Clone, Copy, Debug, Default)]
pub struct StepClock {
    accumulator: f64,
}

impl StepClock {
    /// Add a frame's delta in seconds; returns how many steps to run now. Time beyond
    /// the catch-up bound is dropped.
    pub fn advance(&mut self, dt: f64) -> u32 {
        self.accumulator = (self.accumulator + dt).min(STEP * f64::from(MAX_CATCH_UP_STEPS));
        let mut steps = 0;
        while self.accumulator >= STEP && steps < MAX_CATCH_UP_STEPS {
            self.accumulator -= STEP;
            steps += 1;
        }
        steps
    }

    /// The interpolation fraction between the previous and the latest step.
    pub fn alpha(&self) -> f64 {
        self.accumulator / STEP
    }

    /// Start over at a step (a new round, pause or resume).
    pub fn reset(&mut self) {
        self.accumulator = 0.0;
    }
}

/// Which animation frames run, and the time each runs at, where the browser queues a
/// WebGPU canvas's frames without holding animation frames back: there a GPU-bound page
/// runs far ahead of its picture, and the controls answer that late. With a `limit`, a
/// frame that finds that many frames still on the GPU is skipped (its presses wait for
/// the next), so frames run when the GPU finishes one, which falls anywhere between two
/// animation frames. Their timestamps would then step the game by one or two display
/// frames while the pictures show at the GPU's even pace, so a frame that runs takes
/// the smoothed interval between running frames instead, held within a display frame
/// of its timestamp.
#[derive(Clone, Copy, Debug, Default)]
pub struct FramePacing {
    /// Frames the GPU may still be running when the next one runs; 0 runs every frame
    /// at its own timestamp.
    pub limit: u32,
    /// The timestamp of the last frame that ran and the time it ran at, in ms.
    last: Option<(f64, f64)>,
    /// The smoothed interval between frames that ran, in ms.
    interval: f64,
}

impl FramePacing {
    pub fn new(limit: u32) -> Self {
        Self {
            limit,
            ..Self::default()
        }
    }

    /// The time in ms the animation frame at `now` runs at, or `None` to skip it.
    pub fn frame_time(&mut self, now: f64, frames_in_flight: u32) -> Option<f64> {
        if self.limit == 0 {
            return Some(now);
        }
        let Some((last_now, last_time)) = self.last else {
            self.last = Some((now, now));
            return Some(now);
        };
        let waited = now - last_now;
        if frames_in_flight >= self.limit && waited < MAX_WAIT_MS {
            return None;
        }
        let time = if !(0.0..=PACED_INTERVAL_LIMIT_MS).contains(&waited) {
            // A stall, a hidden tab or a clock that ran back: start over from here.
            self.interval = 0.0;
            now
        } else {
            if self.interval == 0.0 {
                self.interval = waited;
            }
            self.interval += (waited - self.interval) * INTERVAL_SMOOTHING;
            (last_time + self.interval).clamp(now - MAX_LAG_MS, now)
        };
        self.last = Some((now, time));
        Some(time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Displayed time (one step behind the clock) of a [`StepClock`] fed `deltas`, after
    /// each frame.
    fn displayed(deltas: impl IntoIterator<Item = f64>) -> Vec<(u32, f64)> {
        let mut clock = StepClock::default();
        let mut steps = 0;
        deltas
            .into_iter()
            .map(|dt| {
                let now = clock.advance(dt);
                steps += now;
                (now, (f64::from(steps) + clock.alpha() - 1.0) * STEP)
            })
            .collect()
    }

    /// Frame deltas from animation-frame timestamps `period` ms apart, rounded to whole
    /// milliseconds as WebKit reports them.
    fn webkit_deltas(period: f64, frames: usize) -> Vec<f64> {
        let mut times = FrameTimes::default();
        (0..=frames)
            .map(|frame| times.advance((frame as f64 * period).round()))
            .skip(1)
            .collect()
    }

    fn assert_tracks_clock(deltas: &[f64]) {
        let frames = displayed(deltas.iter().copied());
        let mut clock = -STEP;
        for ((_, shown), dt) in frames.iter().zip(deltas) {
            clock += dt;
            assert!((shown - clock).abs() < 1e-9, "shown {shown} at {clock}");
        }
    }

    #[test]
    fn displayed_time_follows_every_display_rate() {
        for hz in [24.0, 30.0, 45.0, 50.0, 60.0, 90.0, 120.0, 144.0] {
            assert_tracks_clock(&vec![1.0 / hz; 600]);
        }
    }

    #[test]
    fn whole_millisecond_timestamps_only_move_the_pose_by_their_rounding() {
        for period in [1000.0 / 120.0, 1000.0 / 60.0, 1000.0 / 30.0] {
            let deltas = webkit_deltas(period, 600);
            assert_tracks_clock(&deltas);
            assert!(deltas.iter().all(|dt| (dt * 1000.0 - period).abs() <= 1.0));
        }
    }

    #[test]
    fn steps_never_bunch_at_steady_rates() {
        let at = |hz: f64| displayed(vec![1.0 / hz; 600]);
        // A 120 Hz display alternates frames with and without a step; 60 Hz steps once a
        // frame (float drift may move a step by a frame, never more); 30 Hz twice.
        assert!(at(120.0).iter().all(|(steps, _)| *steps <= 1));
        assert!(at(60.0).iter().skip(1).all(|(steps, _)| *steps <= 2));
        assert!(
            at(30.0)
                .iter()
                .skip(1)
                .all(|(steps, _)| (1..=3).contains(steps))
        );
        let total: u32 = at(60.0).iter().map(|(steps, _)| steps).sum();
        assert!((599..=600).contains(&total));
    }

    #[test]
    fn a_stall_catches_up_a_bounded_number_of_steps() {
        let mut clock = StepClock::default();
        assert_eq!(clock.advance(MAX_FRAME_DELTA_SECONDS), MAX_CATCH_UP_STEPS);
        assert!(clock.alpha() < 1e-9);
        assert_eq!(clock.advance(STEP * 0.5), 0);
        assert!((clock.alpha() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn time_never_runs_backwards_and_the_first_frame_is_still() {
        let mut times = FrameTimes::default();
        assert_eq!(times.advance(1000.0), 0.0);
        assert!((times.advance(1016.0) - 0.016).abs() < 1e-12);
        assert_eq!(times.advance(990.0), 0.0);
        assert!((times.advance(1032.0) - 0.016).abs() < 1e-12);
        assert_eq!(times.advance(5000.0), MAX_FRAME_DELTA_SECONDS);
        times.last_ms = Some(f64::NEG_INFINITY);
        assert_eq!(times.advance(6000.0), MAX_FRAME_DELTA_SECONDS);
    }

    #[test]
    fn frame_work_averages_start_at_the_first_frame_and_follow_later_ones() {
        let mut times = FrameTimes::default();
        times.advance(0.0);
        times.sim_ms = 2.0;
        times.render_ms = 4.0;
        times.advance(16.0);
        assert_eq!((times.average_sim_ms, times.average_render_ms), (2.0, 4.0));
        for frame in 2..400 {
            times.sim_ms = 1.0;
            times.render_ms = 3.0;
            times.advance(f64::from(frame) * 16.0);
        }
        assert!((times.average_sim_ms - 1.0).abs() < 1e-6);
        assert!((times.average_render_ms - 3.0).abs() < 1e-6);
        assert!((times.fps() - 62.5).abs() < 1e-6);
    }

    /// The frames a page paced at `limit` runs over `ticks` animation frames `period` ms
    /// apart (`round`ed to whole ms as WebKit reports them) while each frame keeps the
    /// GPU busy `gpu` ms after the one before: `(timestamp, time it ran at)`.
    fn paced(limit: u32, period: f64, gpu: f64, ticks: usize, round: bool) -> Vec<(f64, f64)> {
        let mut pacing = FramePacing::new(limit);
        let mut done: Vec<f64> = Vec::new();
        let mut ran = Vec::new();
        for tick in 0..ticks {
            let now = tick as f64 * period;
            let now = if round { now.round() } else { now };
            let in_flight = done.iter().filter(|&&finish| finish > now).count() as u32;
            if let Some(time) = pacing.frame_time(now, in_flight) {
                let start = done.last().map_or(now, |&last| last.max(now));
                done.push(start + gpu);
                ran.push((now, time));
            }
        }
        ran
    }

    /// The largest departure of consecutive gaps from their mean.
    fn unevenness(times: impl Iterator<Item = f64>) -> f64 {
        let times: Vec<f64> = times.collect();
        let gaps: Vec<f64> = times.windows(2).map(|pair| pair[1] - pair[0]).collect();
        let mean = gaps.iter().sum::<f64>() / gaps.len() as f64;
        gaps.iter()
            .map(|gap| (gap - mean).abs())
            .fold(0.0, f64::max)
    }

    #[test]
    fn a_gpu_bound_page_runs_at_the_gpu_pace_on_an_even_clock() {
        for gpu in [22.0, 27.0, 40.0, 70.0] {
            let ran = paced(2, 1000.0 / 60.0, gpu, 1200, true);
            let steady = &ran[60..];
            // One frame runs per GPU frame, so the GPU never idles and never queues more.
            let rate = (steady.len() - 1) as f64 / (steady.last().unwrap().0 - steady[0].0);
            assert!((rate * gpu - 1.0).abs() < 0.02, "{gpu} ms: {rate}");
            // Timestamps step by whole display frames; the clock steps evenly.
            let stamps = unevenness(steady.iter().map(|&(now, _)| now));
            let clock = unevenness(steady.iter().map(|&(_, time)| time));
            assert!(stamps > 5.0, "{gpu} ms: timestamps {stamps}");
            assert!(clock < stamps / 3.0, "{gpu} ms: clock {clock} vs {stamps}");
            for &(now, time) in steady {
                assert!(
                    time <= now && time >= now - MAX_LAG_MS,
                    "{gpu} ms: {time} at {now}"
                );
            }
        }
    }

    #[test]
    fn a_page_the_gpu_keeps_up_with_runs_every_frame_on_time() {
        for round in [false, true] {
            let ran = paced(2, 1000.0 / 60.0, 9.0, 600, round);
            assert_eq!(ran.len(), 600);
            for &(now, time) in &ran[30..] {
                assert!(now - time < 1.0, "{time} at {now}");
            }
            // WebKit's whole-millisecond timestamps even out.
            assert!(unevenness(ran[30..].iter().map(|&(_, time)| time)) < 0.5);
        }
    }

    #[test]
    fn without_a_limit_every_frame_runs_at_its_timestamp() {
        let mut pacing = FramePacing::default();
        for (frame, in_flight) in (0..100).zip((0..10).cycle()) {
            let now = f64::from(frame) * 16.0;
            assert_eq!(pacing.frame_time(now, in_flight), Some(now));
        }
    }

    #[test]
    fn a_backlog_that_never_clears_still_runs_and_a_stall_starts_over() {
        let mut pacing = FramePacing::new(2);
        let mut last = None;
        for tick in 0..240 {
            let now = f64::from(tick) * 1000.0 / 60.0;
            if let Some(time) = pacing.frame_time(now, 5) {
                assert_eq!(time, now);
                if let Some(last) = last {
                    assert!(now - last <= MAX_WAIT_MS + 17.0, "{now} after {last}");
                }
                last = Some(now);
            }
        }
        assert!(last.unwrap() > 3000.0);
        let mut pacing = FramePacing::new(2);
        assert_eq!(pacing.frame_time(0.0, 0), Some(0.0));
        assert_eq!(pacing.frame_time(16.0, 0), Some(16.0));
        assert_eq!(pacing.frame_time(916.0, 0), Some(916.0));
    }
}
