//! How a room feels to the viewer, read where the client draws it: how long a click takes
//! to show its own muzzle flash, how far behind the viewer's predicted tank the other
//! tanks are drawn, and how far from the server's hit a target the viewer hit was drawn
//! when the viewer's own present reached that hit. Stats for nerds shows them and the
//! latency matrix (`scripts/multiplayer-latency-matrix.mjs`) records them; they never
//! change what is drawn or sent.

use std::collections::VecDeque;

use super::fixed_step_clock::SIMULATION_STEP_MS;
use super::network_timeline::hull_gap;
use crate::sim::math::Vec2;
use crate::sim::render_state::RenderState;
use crate::sim::types::VehicleKind;

/// Recent samples kept for each reading's percentiles.
const MAX_SAMPLES: usize = 600;
/// A press still waiting for its shot this long got none (death, no ammo, a round end).
const MAX_FIRE_WAIT_MS: f64 = 1500.0;
/// The shot answering a press starts no earlier than this many ticks before the predicted
/// present at the press: the input arrival margin plus jitter. Earlier shots were already
/// on their way.
const FIRE_TICK_SLACK: f64 = 6.0;
/// Drawn frames are kept this many ticks behind the newest: more than the longest a hit
/// report takes to arrive after the viewer's present passed it.
const FRAME_HISTORY_TICKS: f64 = 120.0;
const MAX_FRAMES: usize = 512;
/// A hit is looked up in the first frame whose present reached it, if that frame came
/// within this many ticks after it (frames are about one tick apart).
const MAX_HIT_FRAME_TICKS: f64 = 4.0;
/// An enemy shell's hit on the viewer is measured over the frames whose present lies
/// within this many ticks before it and one tick after.
const INCOMING_WINDOW_TICKS: f64 = 6.0;

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
    flashed: bool,
    reported: bool,
}

impl Press {
    /// Whether an own shot on `tick` answers this press rather than an earlier one.
    fn answered_by(&self, tick: f64) -> bool {
        tick >= self.tick - FIRE_TICK_SLACK
    }
}

/// One drawn frame: the viewer's predicted present, where its hull and each other live
/// tank were drawn, and where the enemy shells were.
#[derive(Clone, Debug, Default)]
struct DrawnFrame {
    present: f64,
    /// The viewer's hull: kind, position and heading.
    viewer: Option<(VehicleKind, Vec2, f64)>,
    /// Each other live tank: id, kind, position and heading.
    tanks: Vec<(u32, VehicleKind, Vec2, f64)>,
    shells: Vec<(u32, Vec2)>,
}

#[derive(Clone, Debug, Default)]
pub struct FeelReadings {
    /// From the fire button going down to the frame that draws its muzzle flash, ms.
    pub fire_to_flash: Samples,
    /// From the fire button going down to the batch reporting the host's shot, ms.
    pub fire_to_report: Samples,
    /// How far behind the viewer's predicted present the other tanks are drawn, ms, one
    /// sample per drawn frame.
    pub view_delay: Samples,
    /// Per shell hit the viewer scored on another tank: how far the server's impact was
    /// from that tank's hit box as drawn when the viewer's own present reached the hit,
    /// metres. Zero when the server judges shells where the shooter saw the target.
    pub hit_offset: Samples,
    /// Per enemy shell that hit the viewer: how close it was drawn to the viewer's hull (its
    /// hit box) around the moment the viewer's present reached the hit, metres. A shell
    /// drawn on the display clock is still a view delay of flight away.
    pub incoming_offset: Samples,
    /// Enemy shells that hit the viewer without being drawn by then: fired so close that
    /// their whole flight was shorter than the view delay.
    pub unseen_incoming: u64,
    /// Presses that got no shot within [`MAX_FIRE_WAIT_MS`].
    pub unanswered_presses: u64,
    press: Option<Press>,
    firing: bool,
    frames: VecDeque<DrawnFrame>,
    spare: Vec<DrawnFrame>,
}

impl FeelReadings {
    /// Forgets the frames and shots of a superseded display (a baseline or a new round);
    /// the samples stay for the page session.
    pub fn restart(&mut self) {
        self.press = None;
        self.firing = false;
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
        if let Some(press) = self
            .press
            .filter(|press| now_ms - press.at_ms > MAX_FIRE_WAIT_MS)
        {
            self.press = None;
            if !press.flashed {
                self.unanswered_presses += 1;
            }
        }
        let pressed = firing && !self.firing;
        self.firing = firing;
        let Some(present) = present else {
            return;
        };
        if pressed && ready {
            if self.press.is_some_and(|press| !press.flashed) {
                self.unanswered_presses += 1;
            }
            self.press = Some(Press {
                at_ms: now_ms,
                tick: present,
                flashed: false,
                reported: false,
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
        let viewer = state.viewer();
        frame.viewer = viewer.map(|viewer| (viewer.kind, viewer.position.planar(), viewer.heading));
        frame.tanks.clear();
        frame.tanks.extend(
            state
                .tanks
                .iter()
                .filter(|tank| tank.alive && tank.id != state.viewer_id)
                .map(|tank| (tank.id, tank.kind, tank.position.planar(), tank.heading)),
        );
        frame.shells.clear();
        frame.shells.extend(
            state
                .shots
                .iter()
                .filter(|shot| viewer.is_some_and(|viewer| shot.team != viewer.team))
                .map(|shot| (shot.id, Vec2::new(shot.x, shot.z))),
        );
        self.frames.push_back(frame);
    }

    /// A batch arrived reporting the host's shot of the viewer's gun at `tick`.
    pub fn own_shot_reported(&mut self, tick: f64, now_ms: f64) {
        let Some(press) = self.press.as_mut().filter(|press| press.answered_by(tick)) else {
            return;
        };
        if !press.reported {
            press.reported = true;
            self.fire_to_report.add(now_ms - press.at_ms);
        }
        if press.flashed {
            self.press = None;
        }
    }

    /// The muzzle flash of an own shot on `tick` draws this frame: the predicted shot's at
    /// once, or an unpredicted one's when the display reaches it.
    pub fn own_shot_flashed(&mut self, tick: f64, now_ms: f64) {
        let Some(press) = self.press.as_mut().filter(|press| press.answered_by(tick)) else {
            return;
        };
        if !press.flashed {
            press.flashed = true;
            self.fire_to_flash.add(now_ms - press.at_ms);
        }
        if press.reported {
            self.press = None;
        }
    }

    /// The enemy shell `shell` hit the viewer at `tick`.
    pub fn incoming_hit(&mut self, tick: f64, shell: u32) {
        // A shell drawn in the present is gone once it strikes the hull, so the window
        // runs up to the hit and slightly past it.
        let nearest = self
            .frames
            .iter()
            .filter(|frame| {
                frame.present >= tick - INCOMING_WINDOW_TICKS && frame.present <= tick + 1.0
            })
            .filter_map(|frame| {
                let (kind, position, heading) = frame.viewer?;
                let (_, at) = frame.shells.iter().find(|(id, _)| *id == shell)?;
                Some(hull_gap(kind, position, heading, *at))
            })
            .fold(f64::INFINITY, f64::min);
        if nearest.is_finite() {
            self.incoming_offset.add(nearest);
        } else {
            self.unseen_incoming += 1;
        }
    }

    /// The viewer's shell hurt or killed tank `victim` at `tick`, striking it at `impact`.
    pub fn own_hit(&mut self, tick: f64, victim: u32, impact: Vec2) {
        let Some(frame) = self
            .frames
            .iter()
            .find(|frame| frame.present >= tick)
            .filter(|frame| frame.present - tick <= MAX_HIT_FRAME_TICKS)
        else {
            return;
        };
        if let Some(&(_, kind, position, heading)) =
            frame.tanks.iter().find(|(id, ..)| *id == victim)
        {
            self.hit_offset
                .add(hull_gap(kind, position, heading, impact));
        }
    }
}
