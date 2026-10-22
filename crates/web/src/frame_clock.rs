//! The page's frame clock: animation-frame timestamps become capped frame deltas
//! ([`FrameTimes`]), fixed simulation steps and the interpolation fraction
//! ([`StepClock`]), and the GPU's backlog decides which frames draw
//! ([`DrawPacing`]). Plain arithmetic, so it is tested natively.

use sloppy_core::sim::data::STEP;

/// Frame deltas are capped so a stalled tab never fast-forwards the match.
pub const MAX_FRAME_DELTA_SECONDS: f64 = 0.1;
const MILLISECONDS_PER_SECOND: f64 = 1000.0;
/// Catch-up after a stall is bounded so one slow frame cannot spiral.
pub const MAX_CATCH_UP_STEPS: u32 = 5;
/// A frame draws at least this long after the last one whatever the backlog reads, so
/// a completion the browser never reports cannot freeze the picture.
const MAX_UNDRAWN_SECONDS: f64 = 0.5;

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
}

impl FrameTimes {
    /// Move the clock to the frame at `now` and return its capped delta in seconds. A
    /// queued timestamp can precede a slow rebuild; time never runs backwards.
    pub fn advance(&mut self, now: f64) -> f64 {
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
            self.average_ms += (self.frame_ms - self.average_ms) * 0.05;
        }
        raw.min(MAX_FRAME_DELTA_SECONDS)
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

/// Which frames draw. A browser that queues a WebGPU canvas's frames without holding
/// animation frames back lets a GPU-bound page run far ahead of its picture, and the
/// controls answer that late. With a `limit`, a frame that finds that many frames still
/// on the GPU skips drawing (the simulation still steps), and the next one that draws
/// advances presentation by the skipped time too, so effects keep real time.
#[derive(Clone, Copy, Debug, Default)]
pub struct DrawPacing {
    undrawn: f64,
    /// Frames the GPU may still be running when the next one draws; 0 draws every frame.
    pub limit: u32,
}

impl DrawPacing {
    pub fn new(limit: u32) -> Self {
        Self {
            undrawn: 0.0,
            limit,
        }
    }

    /// The presentation delta to draw this frame with, or `None` to skip drawing it.
    pub fn draw_delta(&mut self, frames_in_flight: u32, dt: f64) -> Option<f64> {
        self.undrawn += dt;
        if self.limit > 0 && frames_in_flight >= self.limit && self.undrawn < MAX_UNDRAWN_SECONDS {
            return None;
        }
        Some(std::mem::take(&mut self.undrawn).min(MAX_FRAME_DELTA_SECONDS))
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
    fn frames_wait_for_the_gpu_and_draw_the_skipped_time() {
        let mut pacing = DrawPacing::new(2);
        assert_eq!(pacing.draw_delta(0, 0.016), Some(0.016));
        assert_eq!(pacing.draw_delta(1, 0.016), Some(0.016));
        assert_eq!(pacing.draw_delta(2, 0.016), None);
        assert_eq!(pacing.draw_delta(3, 0.016), None);
        let drawn = pacing.draw_delta(1, 0.016).unwrap();
        assert!((drawn - 0.048).abs() < 1e-12);
        assert_eq!(pacing.draw_delta(1, 0.016), Some(0.016));
    }

    #[test]
    fn without_a_limit_every_frame_draws() {
        let mut pacing = DrawPacing::default();
        assert!((0..100).all(|frames| pacing.draw_delta(frames, 0.016) == Some(0.016)));
    }

    #[test]
    fn a_backlog_that_never_clears_still_draws() {
        let mut pacing = DrawPacing::new(2);
        let mut last = 0;
        for frame in 1..=240 {
            if let Some(dt) = pacing.draw_delta(2, 1.0 / 60.0) {
                // At least every half second, never advancing more than a capped delta.
                assert!(frame - last <= 31, "frame {frame} after {last}");
                assert_eq!(dt, MAX_FRAME_DELTA_SECONDS);
                last = frame;
            }
        }
        assert!(last > 200);
    }
}
