//! The client's prediction of its own hull: when to step it, which input drives each
//! tick, and how the drawn hull absorbs corrections.
//!
//! Local time maps to host ticks through an offset learned from acknowledgements: each
//! input's arrival tick against when it was sent. The predicted present runs a little past
//! the arrival of an input sent now, so input asks the host for the tick it was predicted
//! at and arrives in time to start there. Each snapshot restarts the predictor from the
//! host's hull and replays the ticks the host has not reached; the difference from the
//! previous prediction becomes a display offset that decays within about 100 ms.

use std::collections::VecDeque;

use super::fixed_step_clock::SIMULATION_STEP_MS;
use super::player_controls::Ack;
use super::prediction::{HullContacts, HullState, PredictedPose, TankPredictor, solid_debris};
use crate::sim::math::{Point3, Vec2, angle_delta, distance};
use crate::sim::render_state::RenderState;

/// Ticks the predicted present runs past the expected arrival of an input sent now. It
/// covers tick quantization and the up to 25 ms a press may wait for its send slot.
const ARRIVAL_MARGIN_TICKS: f64 = 2.0;
/// Arrivals this late or earlier, among recent ones, still start on their requested tick.
const ARRIVAL_PERCENTILE: f64 = 0.9;
const MAX_ARRIVAL_SAMPLES: usize = 40;
/// The predicted clock speeds up or slows down by at most this fraction to follow its
/// target, so the hull never visibly hurries or stalls.
const MAX_CLOCK_SLEW: f64 = 0.1;
/// A target this far off (or a client behind the host) restarts the clock instead.
const CLOCK_SNAP_TICKS: f64 = 15.0;
/// Most ticks one frame may predict; a longer gap catches up over several frames.
const MAX_STEPS_PER_FRAME: u64 = 8;
const MAX_PENDING_TICKS: usize = 120;
const MAX_UNACKED_SENDS: usize = 120;
/// Corrections decay at this rate per second, about 95 % within 100 ms.
const CORRECTION_RATE: f64 = 30.0;
/// A correction this large is drawn at once rather than glided.
const SNAP_DISTANCE: f64 = 3.0;
/// Recent correction sizes kept for the statistics.
const CORRECTION_SAMPLES: usize = 600;
/// Corrections at least this large count toward their cause's tally.
const NOTICEABLE_CORRECTION: f64 = 0.02;
/// A solid piece of debris whose centre is this much closer than half its longest side
/// plus a hull's half length counts as touching the hull, for the statistics.
const DEBRIS_REACH: f64 = 2.5;

/// One snapshot's acknowledgement and the viewer's hull.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HostUpdate {
    pub ack: Ack,
    pub hull: Option<HullState>,
    /// The batch carried a blast near the viewer or a hit on it, which the host may have
    /// pushed the hull with.
    pub blast: bool,
}

/// What a correction is put down to, for the statistics: the first of these, in this
/// order, that the batch or the prediction since the previous snapshot saw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorrectionCause {
    /// A blast near the hull or a hit on it.
    Blast,
    /// Contact with another tank, whose steering the prediction only holds.
    Tank,
    /// Contact with movable cover (crates, drums), drawn from the delayed timeline.
    Crate,
    /// Solid debris (wrecks, timber, broken scenery), absent from the prediction.
    Debris,
    /// Contact with standing cover or a stump.
    Cover,
    /// An input change that reached the host after the tick it asked for.
    LateInput,
    Other,
}

impl CorrectionCause {
    pub const ALL: [CorrectionCause; 7] = [
        CorrectionCause::Blast,
        CorrectionCause::Tank,
        CorrectionCause::Crate,
        CorrectionCause::Debris,
        CorrectionCause::Cover,
        CorrectionCause::LateInput,
        CorrectionCause::Other,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            CorrectionCause::Blast => "blast",
            CorrectionCause::Tank => "tank",
            CorrectionCause::Crate => "crate",
            CorrectionCause::Debris => "debris",
            CorrectionCause::Cover => "cover",
            CorrectionCause::LateInput => "lateInput",
            CorrectionCause::Other => "other",
        }
    }

    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|cause| *cause == self)
            .expect("listed")
    }
}

/// Corrections put down to one cause.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CauseTally {
    /// Corrections of at least [`NOTICEABLE_CORRECTION`].
    pub count: u64,
    /// Their distance, metres.
    pub total: f64,
}

/// How far prediction had to move the hull, for the network statistics.
#[derive(Clone, Debug, Default)]
pub struct CorrectionStats {
    /// Snapshots that corrected a continuing prediction.
    pub count: u64,
    /// Sum of correction distances, metres.
    pub total: f64,
    /// Seconds of continuing prediction those corrections span.
    pub seconds: f64,
    pub largest: f64,
    /// Acknowledged input changes, and those the host started after the tick they asked
    /// for because they arrived too late.
    pub changes: u64,
    pub late_changes: u64,
    /// The latest correction sizes, metres.
    pub recent: VecDeque<f64>,
    /// Noticeable corrections by cause, in [`CorrectionCause::ALL`] order.
    pub causes: [CauseTally; CorrectionCause::ALL.len()],
}

impl CorrectionStats {
    fn record(&mut self, size: f64, cause: CorrectionCause) {
        self.count += 1;
        self.total += size;
        self.largest = self.largest.max(size);
        self.recent.push_back(size);
        self.recent.retain_back(CORRECTION_SAMPLES);
        if size >= NOTICEABLE_CORRECTION {
            let tally = &mut self.causes[cause.index()];
            tally.count += 1;
            tally.total += size;
        }
    }

    pub fn cause(&self, cause: CorrectionCause) -> CauseTally {
        self.causes[cause.index()]
    }

    /// The 95th percentile of recent corrections, metres.
    pub fn p95(&self) -> f64 {
        if self.recent.is_empty() {
            return 0.0;
        }
        let mut sorted: Vec<f64> = self.recent.iter().copied().collect();
        sorted.sort_by(f64::total_cmp);
        sorted[((sorted.len() - 1) as f64 * 0.95).round() as usize]
    }
}

#[derive(Clone, Copy, Debug)]
struct SentInput {
    seq: i64,
    sent_ms: f64,
    requested: Option<u64>,
}

/// The drawn own hull.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawnHull {
    pub life: u32,
    pub position: Point3,
    pub velocity: Point3,
    pub heading: f64,
}

#[derive(Default)]
pub struct HullPrediction {
    predictor: TankPredictor,
    life: Option<u32>,
    /// The newest predicted tick; `previous` and `current` are the hull after the tick
    /// before it and after it.
    tick: u64,
    previous: Option<PredictedPose>,
    current: Option<PredictedPose>,
    /// The move input of each predicted tick the host has not confirmed, oldest first.
    inputs: VecDeque<(u64, Vec2)>,
    /// The first tick the current local input drives, set once when that input appears.
    run_start: u64,
    /// The local input of the last frame.
    last_input: Option<Vec2>,
    /// `P(now) = now / step + offset`: the predicted tick at local time `now`.
    offset: Option<f64>,
    target_offset: f64,
    last_ms: Option<f64>,
    /// Sent inputs not yet acknowledged.
    sent: VecDeque<SentInput>,
    /// The current run's start has gone to the host.
    run_sent: bool,
    acked: i64,
    /// Recent arrival ticks less their send time in ticks.
    arrivals: VecDeque<f64>,
    correction: Vec2,
    heading_correction: f64,
    /// What the hull touched in the steps predicted since the previous snapshot.
    touched: HullContacts,
    pub stats: CorrectionStats,
}

impl HullPrediction {
    pub fn active(&self) -> bool {
        self.life.is_some() && self.current.is_some()
    }

    /// Stops predicting until the next live hull, keeping the learned clock. A new control
    /// epoch restarts the host's acknowledgements, so unacknowledged sends are dropped.
    pub fn clear(&mut self) {
        self.sent.clear();
        self.acked = 0;
        self.life = None;
        self.previous = None;
        self.current = None;
        self.inputs.clear();
        self.last_input = None;
        self.correction = Vec2::ZERO;
        self.heading_correction = 0.0;
        self.touched = HullContacts::default();
        self.predictor.clear_hull();
    }

    /// Forgets everything, for a new round or room.
    pub fn reset(&mut self) {
        let stats = std::mem::take(&mut self.stats);
        *self = Self {
            stats,
            ..Self::default()
        };
    }

    /// The predicted tick at `now_ms`, once the clock is set.
    pub fn present(&self, now_ms: f64) -> Option<f64> {
        self.offset
            .map(|offset| now_ms / SIMULATION_STEP_MS + offset)
    }

    /// How far the predicted present runs ahead of the newest host tick, in milliseconds.
    pub fn lead_ms(&self, now_ms: f64, host_tick: u64) -> f64 {
        self.present(now_ms).map_or(0.0, |present| {
            (present - host_tick as f64) * SIMULATION_STEP_MS
        })
    }

    /// The tick to ask the host to start the input sent now from: where the current local
    /// input began driving the prediction. Only the first send of a run asks; repeats of
    /// the same input change nothing the host drives, so they start on arrival.
    pub fn requested_tick(&self) -> Option<u64> {
        (self.active() && !self.run_sent).then_some(self.run_start)
    }

    /// Records an input that went out at `now_ms`.
    pub fn sent(&mut self, seq: i64, now_ms: f64) {
        let requested = self.requested_tick();
        self.sent.push_back(SentInput {
            seq,
            sent_ms: now_ms,
            requested,
        });
        if requested.is_some() {
            self.run_sent = true;
        }
        self.sent.retain_back(MAX_UNACKED_SENDS);
    }

    /// One snapshot: learn from its acknowledgement, then restart from the host's hull and
    /// replay the ticks it has not reached. `scene` is the newest received scene.
    pub fn receive(&mut self, update: &HostUpdate, scene: &RenderState, now_ms: f64, rtt_ms: f64) {
        let late_changes = self.stats.late_changes;
        self.learn_arrival(&update.ack);
        let late_input = self.stats.late_changes > late_changes;
        let Some(hull) = update.hull else {
            self.clear();
            return;
        };
        let newest_input = self.inputs.back().map_or(Vec2::ZERO, |(_, input)| *input);
        while self
            .inputs
            .pop_front_if(|(tick, _)| *tick <= hull.tick)
            .is_some()
        {}
        let continuing = self.life == Some(hull.life) && self.current.is_some();
        // The recorded inputs must cover every tick from the host's to the predicted one;
        // otherwise (first hull, respawn, a client behind the host) restart from the host's
        // tick. A continuing hull catches up at once with its newest input, so it glides.
        let covered = self.tick > hull.tick
            && self.inputs.len() as u64 == self.tick - hull.tick
            && self
                .inputs
                .front()
                .is_some_and(|(tick, _)| *tick == hull.tick + 1);
        let restart = !continuing || !covered;
        if self.offset.is_none() || self.tick <= hull.tick {
            // Until arrivals are measured, assume input reaches the host a round trip
            // after the newest tick was simulated.
            let estimate = hull.tick as f64 + rtt_ms.max(0.0) / SIMULATION_STEP_MS
                - now_ms / SIMULATION_STEP_MS
                + ARRIVAL_MARGIN_TICKS;
            if self.arrivals.is_empty() {
                self.target_offset = estimate;
            }
            self.offset = Some(self.target_offset.max(estimate));
        }
        if restart {
            let predicted = self.tick;
            self.tick = hull.tick;
            self.inputs.clear();
            if continuing {
                while self.tick < predicted && self.inputs.len() < MAX_PENDING_TICKS {
                    self.tick += 1;
                    self.inputs.push_back((self.tick, newest_input));
                }
            }
        }
        self.predictor.sync_scene(scene);
        self.predictor.reset(&hull);
        let old = if continuing { self.current } else { None };
        let mut previous = self.predictor.pose();
        let mut current = previous;
        for &(_, input) in &self.inputs {
            self.predictor.step(input.x, input.z);
            self.touched.add(self.predictor.hull_contacts());
            previous = current;
            current = self.predictor.pose();
        }
        self.previous = previous;
        self.current = current;
        self.life = Some(hull.life);
        let touched = std::mem::take(&mut self.touched);
        match (old, current) {
            (Some(old), Some(new)) => {
                let size = distance(old.position.planar(), new.position.planar());
                let cause = if update.blast {
                    CorrectionCause::Blast
                } else if touched.tank {
                    CorrectionCause::Tank
                } else if touched.movable_cover {
                    CorrectionCause::Crate
                } else if near_solid_debris(scene, &[hull.position, old.position, new.position]) {
                    CorrectionCause::Debris
                } else if touched.cover {
                    CorrectionCause::Cover
                } else if late_input {
                    CorrectionCause::LateInput
                } else {
                    CorrectionCause::Other
                };
                self.stats.record(size, cause);
                if size > SNAP_DISTANCE {
                    self.correction = Vec2::ZERO;
                    self.heading_correction = 0.0;
                } else {
                    self.correction.x += old.position.x - new.position.x;
                    self.correction.z += old.position.z - new.position.z;
                    self.heading_correction += angle_delta(new.heading, old.heading);
                }
            }
            _ => {
                self.correction = Vec2::ZERO;
                self.heading_correction = 0.0;
            }
        }
    }

    fn learn_arrival(&mut self, ack: &Ack) {
        if ack.input_seq <= self.acked || ack.arrival_tick == 0 {
            return;
        }
        self.acked = ack.input_seq;
        while let Some(sent) = self.sent.pop_front_if(|sent| sent.seq <= ack.input_seq) {
            if sent.seq != ack.input_seq {
                continue;
            }
            if let Some(requested) = sent.requested {
                self.stats.changes += 1;
                if ack.applied_tick > requested {
                    self.stats.late_changes += 1;
                }
            }
            self.arrivals
                .push_back(ack.arrival_tick as f64 - sent.sent_ms / SIMULATION_STEP_MS);
            self.arrivals.retain_back(MAX_ARRIVAL_SAMPLES);
            let mut sorted: Vec<f64> = self.arrivals.iter().copied().collect();
            sorted.sort_by(f64::total_cmp);
            let late = sorted[((sorted.len() - 1) as f64 * ARRIVAL_PERCENTILE).round() as usize];
            self.target_offset = late + ARRIVAL_MARGIN_TICKS;
        }
    }

    /// Advances the prediction to `now_ms`, driving each new tick with `input`.
    pub fn advance(&mut self, now_ms: f64, input: Vec2) {
        let dt_ms = self.last_ms.map_or(0.0, |last| (now_ms - last).max(0.0));
        self.last_ms = Some(now_ms);
        let Some(mut offset) = self.offset else {
            return;
        };
        // Follow the target at a bounded rate; a large error restarts the clock.
        let error = self.target_offset - offset;
        if error.abs() > CLOCK_SNAP_TICKS {
            offset = self.target_offset;
        } else {
            let limit = MAX_CLOCK_SLEW * dt_ms / SIMULATION_STEP_MS;
            offset += error.clamp(-limit, limit);
        }
        self.offset = Some(offset);
        let decay = (-CORRECTION_RATE * dt_ms / 1000.0).exp();
        self.correction.x *= decay;
        self.correction.z *= decay;
        self.heading_correction *= decay;
        if !self.active() {
            return;
        }
        self.stats.seconds += dt_ms / 1000.0;
        let present = now_ms / SIMULATION_STEP_MS + offset;
        let target = present.ceil().max(0.0) as u64;
        // Date each change once: a frame that steps no tick must not push it later than
        // the tick already sent to the host. A change first seen after a stalled frame
        // cannot reach the host in time for the ticks the stall skipped; those keep the
        // previous input there and here.
        // The skipped ticks keep the newest recorded input, or after a restart emptied
        // the record, the input of the frame before the change.
        let earlier = self
            .inputs
            .back()
            .map(|(_, input)| *input)
            .or(self.last_input)
            .unwrap_or(input);
        if self.last_input != Some(input) {
            self.last_input = Some(input);
            self.run_start = (self.tick + 1).max(target);
            self.run_sent = false;
        }
        let mut steps = 0;
        while self.tick < target && steps < MAX_STEPS_PER_FRAME {
            self.tick += 1;
            let drive = if self.tick >= self.run_start {
                input
            } else {
                earlier
            };
            self.predictor.step(drive.x, drive.z);
            self.touched.add(self.predictor.hull_contacts());
            self.inputs.push_back((self.tick, drive));
            self.inputs.retain_back(MAX_PENDING_TICKS);
            self.previous = self.current;
            self.current = self.predictor.pose();
            steps += 1;
        }
    }

    /// The predicted present (fractional tick) while a live hull is predicted.
    pub fn predicted_present(&self, now_ms: f64) -> Option<f64> {
        self.present(now_ms).filter(|_| self.active())
    }

    /// The hull to draw at `now_ms`: between the last two predicted ticks, with what is
    /// left of the latest correction.
    pub fn drawn(&self, now_ms: f64) -> Option<DrawnHull> {
        let (life, current) = (self.life?, self.current?);
        let previous = self.previous.unwrap_or(current);
        let alpha = self.present(now_ms).map_or(1.0, |present| {
            (present - (self.tick as f64 - 1.0)).clamp(0.0, 1.0)
        });
        let mut position = previous.position.lerp(current.position, alpha);
        position.x += self.correction.x;
        position.z += self.correction.z;
        let heading = previous.heading
            + angle_delta(previous.heading, current.heading) * alpha
            + self.heading_correction;
        Some(DrawnHull {
            life,
            position,
            velocity: previous.velocity.lerp(current.velocity, alpha),
            heading,
        })
    }
}

/// Whether solid debris in `scene` lies within a hull's reach of any of `points`.
fn near_solid_debris(scene: &RenderState, points: &[Point3]) -> bool {
    scene
        .fragments
        .iter()
        .filter(|fragment| solid_debris(fragment))
        .any(|fragment| {
            let extent = fragment
                .dimensions
                .map_or(fragment.size, |size| size.x.max(size.y).max(size.z));
            let reach = DEBRIS_REACH + extent / 2.0;
            points
                .iter()
                .any(|point| distance(point.planar(), fragment.position.planar()) < reach)
        })
}
