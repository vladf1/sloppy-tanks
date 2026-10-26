//! How a room feels to the viewer, read where the client draws it: how long a click takes
//! to show its own muzzle flash, how far behind the viewer's predicted tank the other
//! tanks are drawn, and how far from the server's hit a target the viewer hit was drawn
//! when the viewer's own present reached that hit. Stats for nerds shows them and the
//! latency matrix (`scripts/multiplayer-latency-matrix.mjs`) records them; they never
//! change what is drawn or sent.

use std::collections::VecDeque;

use super::fixed_step_clock::SIMULATION_STEP_MS;
use crate::sim::math::{Vec2, distance};
use crate::sim::render_state::RenderState;

/// Recent samples kept for each reading's percentiles.
const MAX_SAMPLES: usize = 600;
/// A press still waiting for its shot this long got none (death, no ammo, a round end).
const MAX_FIRE_WAIT_MS: f64 = 1500.0;
/// The shot answering a press starts no earlier than this many ticks before the predicted
/// present at the press: the input arrival margin plus jitter. Earlier shots were already
/// on their way.
const FIRE_TICK_SLACK: f64 = 6.0;
/// Own shots reported but not yet displayed; a display buffer holds a handful.
const MAX_SHOT_ARRIVALS: usize = 16;
/// Drawn frames are kept this many ticks behind the newest: more than the longest a hit
/// report takes to arrive after the viewer's present passed it.
const FRAME_HISTORY_TICKS: f64 = 120.0;
const MAX_FRAMES: usize = 512;
/// A hit is looked up in the first frame whose present reached it, if that frame came
/// within this many ticks after it (frames are about one tick apart).
const MAX_HIT_FRAME_TICKS: f64 = 4.0;

/// One reading's samples: how many, their mean and largest, and percentiles of the
/// recent ones.
#[derive(Clone, Debug, Default)]
pub struct Samples {
    pub count: u64,
    total: f64,
    pub largest: f64,
    recent: VecDeque<f64>,
}

impl Samples {
    pub fn add(&mut self, value: f64) {
        self.count += 1;
        self.total += value;
        self.largest = self.largest.max(value);
        self.recent.push_back(value);
        self.recent.retain_back(MAX_SAMPLES);
    }

    pub fn mean(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.total / self.count as f64
        }
    }

    /// The `fraction` percentile (0 to 1) of the recent samples.
    pub fn percentile(&self, fraction: f64) -> f64 {
        if self.recent.is_empty() {
            return 0.0;
        }
        let mut sorted: Vec<f64> = self.recent.iter().copied().collect();
        sorted.sort_by(f64::total_cmp);
        sorted[((sorted.len() - 1) as f64 * fraction).round() as usize]
    }
}

#[derive(Clone, Copy, Debug)]
struct Press {
    at_ms: f64,
    /// The predicted present when the button went down.
    tick: f64,
}

/// One drawn frame: the viewer's predicted present and where each other live tank was
/// drawn.
#[derive(Clone, Debug, Default)]
struct DrawnFrame {
    present: f64,
    tanks: Vec<(u32, Vec2)>,
}

#[derive(Clone, Debug, Default)]
pub struct FeelReadings {
    /// From the fire button going down to the frame that draws its muzzle flash, ms.
    pub fire_to_flash: Samples,
    /// The part of `fire_to_flash` until the batch reporting the shot arrived, ms.
    pub fire_to_report: Samples,
    /// How far behind the viewer's predicted present the other tanks are drawn, ms, one
    /// sample per drawn frame.
    pub view_delay: Samples,
    /// Per hit the viewer scored on another tank: how far that tank was drawn, when the
    /// viewer's own present reached the hit, from where the server hit it, metres.
    pub hit_offset: Samples,
    /// Presses that got no shot within [`MAX_FIRE_WAIT_MS`].
    pub unanswered_presses: u64,
    press: Option<Press>,
    firing: bool,
    shot_reports: VecDeque<(f64, f64)>,
    frames: VecDeque<DrawnFrame>,
    spare: Vec<DrawnFrame>,
}

impl FeelReadings {
    /// Forgets the frames and shots of a superseded display (a baseline or a new round);
    /// the samples stay for the page session.
    pub fn restart(&mut self) {
        self.press = None;
        self.firing = false;
        self.shot_reports.clear();
        self.spare.extend(self.frames.drain(..));
    }

    /// One drawn frame. `present` is the viewer's predicted tick while prediction runs,
    /// `display` the tick the other tanks are drawn at, `firing` whether the viewer holds
    /// fire with live controls and `ready` whether the gun will have reloaded by the time
    /// a shot sent now reaches the server.
    pub fn frame(
        &mut self,
        now_ms: f64,
        present: Option<f64>,
        display: f64,
        firing: bool,
        ready: bool,
        state: &RenderState,
    ) {
        if self
            .press
            .is_some_and(|press| now_ms - press.at_ms > MAX_FIRE_WAIT_MS)
        {
            self.press = None;
            self.unanswered_presses += 1;
        }
        let pressed = firing && !self.firing;
        self.firing = firing;
        let Some(present) = present else {
            return;
        };
        if pressed && ready {
            if self.press.is_some() {
                self.unanswered_presses += 1;
            }
            self.press = Some(Press {
                at_ms: now_ms,
                tick: present,
            });
        }
        self.view_delay
            .add((present - display).max(0.0) * SIMULATION_STEP_MS);
        while self
            .frames
            .front()
            .is_some_and(|frame| frame.present < present - FRAME_HISTORY_TICKS)
            || self.frames.len() >= MAX_FRAMES
        {
            let frame = self.frames.pop_front().expect("checked above");
            self.spare.push(frame);
        }
        let mut frame = self.spare.pop().unwrap_or_default();
        frame.present = present;
        frame.tanks.clear();
        frame.tanks.extend(
            state
                .tanks
                .iter()
                .filter(|tank| tank.alive && tank.id != state.viewer_id)
                .map(|tank| (tank.id, Vec2::new(tank.position.x, tank.position.z))),
        );
        self.frames.push_back(frame);
    }

    /// A batch arrived reporting the viewer's own shot at `tick`.
    pub fn own_shot_reported(&mut self, tick: f64, now_ms: f64) {
        self.shot_reports.push_back((tick, now_ms));
        self.shot_reports.retain_back(MAX_SHOT_ARRIVALS);
    }

    /// The display, now at tick `display`, reached one of the viewer's own shots: its
    /// muzzle flash draws this frame. Reloads keep own shots far more than a frame apart,
    /// so the newest report the display has passed is this shot's.
    pub fn own_shot_displayed(&mut self, display: f64, now_ms: f64) {
        let mut report = None;
        while let Some(passed) = self.shot_reports.pop_front_if(|(tick, _)| *tick <= display) {
            report = Some(passed);
        }
        let Some(press) = self.press else {
            return;
        };
        let tick = report.map_or(display, |(tick, _)| tick);
        if tick < press.tick - FIRE_TICK_SLACK {
            return;
        }
        self.press = None;
        self.fire_to_flash.add(now_ms - press.at_ms);
        if let Some((_, reported_ms)) = report {
            self.fire_to_report.add(reported_ms - press.at_ms);
        }
    }

    /// The viewer's shot or blast hurt or killed tank `victim` at `tick`, where the server
    /// had it at `x`, `z`.
    pub fn own_hit(&mut self, tick: f64, victim: u32, x: f64, z: f64) {
        let Some(frame) = self
            .frames
            .iter()
            .find(|frame| frame.present >= tick)
            .filter(|frame| frame.present - tick <= MAX_HIT_FRAME_TICKS)
        else {
            return;
        };
        if let Some((_, drawn)) = frame.tanks.iter().find(|(id, _)| *id == victim) {
            self.hit_offset.add(distance(*drawn, Vec2::new(x, z)));
        }
    }
}
