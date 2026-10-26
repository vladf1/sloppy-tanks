//! The client's prediction of its own hull: when to step it, which input drives each
//! tick, and how the drawn hull absorbs corrections.
//!
//! Local time maps to host ticks through an offset learned from acknowledgements: each
//! input's arrival tick against when it was sent. The predicted present runs a little past
//! the arrival of an input sent now, so input asks the host for the tick it was predicted
//! at and arrives in time to start there. Each snapshot restarts the predictor from the
//! host's hull and replays the ticks the host has not reached; the difference from the
//! previous prediction becomes a display offset that decays within about 100 ms.
//!
//! The prediction also runs the viewer's gun as the host does, so held fire shoots on the
//! ticks the host's will ([`FiredShot`], drawn by [`super::own_shots`]): its reload counts
//! down every tick and is rebuilt from the host's own shots at each snapshot.

use std::collections::VecDeque;

use super::fixed_step_clock::SIMULATION_STEP_MS;
use super::own_shots::{CoverCast, CoverHit};
use super::player_controls::Ack;
use super::prediction::{HullContacts, HullState, PredictedPose, TankPredictor, solid_debris};
use crate::sim::data::STEP;
use crate::sim::math::{Point3, Vec2, angle_delta, distance};
use crate::sim::render_state::RenderState;

/// Ticks the predicted present runs past the expected arrival of an input sent now, over
/// the arrivals' 90th percentile. A press goes out in the frame it is dated
/// (`input_cadence.rs`), so one tick is enough: two only added to how far behind the own
/// hull every other tank is drawn.
const ARRIVAL_MARGIN_TICKS: f64 = 1.0;
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
/// A host hull this close to what was predicted for its tick confirms the prediction,
/// which then runs on without a replay; the difference stays, at most this, until a
/// later snapshot disagrees. Free driving differs by up to 2 mm after a restart
/// (`tests/integration/prediction.rs`).
const CONFIRM_POSITION: f64 = 0.005;
const CONFIRM_VELOCITY: f64 = 0.05;
const CONFIRM_HEADING: f64 = 0.002;
/// Another tank or movable cover this close to the host's hull makes a snapshot replay
/// even when it confirms the prediction: the replay places them as replicated, which a
/// held prediction does not.
const MOVER_REACH: f64 = 8.0;
/// Corrections at least this large count toward their cause's tally.
const NOTICEABLE_CORRECTION: f64 = 0.02;
/// A solid piece of debris whose centre is this much closer than half its longest side
/// plus a hull's half length counts as touching the hull, for the statistics.
const DEBRIS_REACH: f64 = 2.5;

/// One frame's local controls as the prediction drives them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PredictedInput {
    /// The move input, rounded as the host reads it.
    pub drive: Vec2,
    pub fire: bool,
}

/// A shot the predicted gun fired: the host tick it fires on and the hull's position when
/// it fires, after the tick before.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FiredShot {
    pub tick: u64,
    pub position: Point3,
}

/// The viewer's hull from the newest snapshot batch.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HostUpdate {
    pub hull: Option<HullState>,
    /// A batch since the previous update carried a blast near the viewer or a hit on it,
    /// which the host may have pushed the hull with.
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
    /// Snapshots that confirmed the prediction, which ran on without a replay.
    pub confirmed: u64,
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
    /// The input of each predicted tick the host has not confirmed, oldest first.
    inputs: VecDeque<(u64, PredictedInput)>,
    /// The predicted hull after each of those ticks.
    poses: VecDeque<(u64, PredictedPose)>,
    /// The first tick the current local input drives, set once when that input appears.
    run_start: u64,
    /// The local input of the last frame.
    last_input: Option<PredictedInput>,
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
    /// An acknowledgement since the previous restart showed an input started late.
    late_input: bool,
    /// The gun's reload after the newest predicted tick, seconds, and the interval it
    /// reloads to.
    gun_cooldown: f64,
    gun_interval: f64,
    /// The host's latest shot of this life: its tick and the interval it reloaded to.
    host_shot: Option<(u64, f64)>,
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
        self.poses.clear();
        self.last_input = None;
        self.correction = Vec2::ZERO;
        self.heading_correction = 0.0;
        self.touched = HullContacts::default();
        // A new life starts loaded.
        self.gun_cooldown = 0.0;
        self.host_shot = None;
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

    /// The host fired the viewer's gun on `tick`, reloading for `interval` seconds.
    pub fn host_fired(&mut self, tick: u64, interval: f64) {
        if self.host_shot.is_none_or(|(latest, _)| tick > latest) {
            self.host_shot = Some((tick, interval));
        }
    }

    /// One snapshot batch's acknowledgement: when inputs reach the host.
    pub fn acknowledge(&mut self, ack: &Ack) {
        let late_changes = self.stats.late_changes;
        self.learn_arrival(ack);
        self.late_input |= self.stats.late_changes > late_changes;
    }

    /// Restarts from the host's hull and replays the ticks it has not reached, unless the
    /// hull confirms the prediction. `scene` is the newest received scene and `now_ms`
    /// when it arrived.
    pub fn receive(&mut self, update: &HostUpdate, scene: &RenderState, now_ms: f64, rtt_ms: f64) {
        let late_input = std::mem::take(&mut self.late_input);
        let Some(hull) = update.hull else {
            self.clear();
            return;
        };
        let newest_input = self
            .inputs
            .back()
            .map_or(PredictedInput::default(), |(_, input)| *input);
        while self
            .inputs
            .pop_front_if(|(tick, _)| *tick <= hull.tick)
            .is_some()
        {}
        let predicted = self
            .poses
            .iter()
            .find(|(tick, _)| *tick == hull.tick)
            .map(|(_, pose)| *pose);
        while self
            .poses
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
        // A prediction the host confirms runs on: replaying it would reproduce it.
        if !restart
            && !update.blast
            && let Some(difference) = predicted.and_then(|pose| agreement(&pose, &hull))
            && !movers_near(scene, hull.position)
        {
            self.predictor.sync_covers(scene);
            self.touched = HullContacts::default();
            self.stats.confirmed += 1;
            self.stats.record(difference, CorrectionCause::Other);
            self.reload_gun(hull.tick, scene);
            return;
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
        self.poses.clear();
        for &(tick, input) in &self.inputs {
            self.predictor.step(input.drive.x, input.drive.z);
            self.touched.add(self.predictor.hull_contacts());
            previous = current;
            current = self.predictor.pose();
            if let Some(pose) = current {
                self.poses.push_back((tick, pose));
            }
        }
        self.previous = previous;
        self.current = current;
        self.life = Some(hull.life);
        self.reload_gun(hull.tick, scene);
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

    /// Rebuilds the gun's reload at the newest predicted tick: the host's at `host_tick`,
    /// from its latest shot when known (the replicated reload is rounded to 0.01 s, which
    /// could move a shot by a tick) or the scene's, then the predicted ticks' fire.
    fn reload_gun(&mut self, host_tick: u64, scene: &RenderState) {
        let mut cooldown = match self.host_shot {
            Some((tick, interval)) if tick <= host_tick => {
                let mut cooldown = interval;
                for _ in tick..host_tick {
                    if cooldown == 0.0 {
                        break;
                    }
                    cooldown = 0f64.max(cooldown - STEP);
                }
                cooldown
            }
            _ => scene.viewer().map_or(0.0, |viewer| viewer.cooldown),
        };
        for (_, input) in &self.inputs {
            cooldown = 0f64.max(cooldown - STEP);
            if input.fire && cooldown <= 0.0 {
                cooldown = self.gun_interval;
            }
        }
        self.gun_cooldown = cooldown;
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

    /// Advances the prediction to `now_ms`, driving each new tick with `input` and a gun
    /// that reloads for `interval` seconds. Returns the shots it fired.
    pub fn advance(&mut self, now_ms: f64, input: PredictedInput, interval: f64) -> Vec<FiredShot> {
        let mut fired = Vec::new();
        self.gun_interval = interval;
        let dt_ms = self.last_ms.map_or(0.0, |last| (now_ms - last).max(0.0));
        self.last_ms = Some(now_ms);
        let Some(mut offset) = self.offset else {
            return fired;
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
            return fired;
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
            // The host's order: the reload runs down, then held fire shoots from where the
            // hull stood after the tick before, then the drive and the world step.
            self.gun_cooldown = 0f64.max(self.gun_cooldown - STEP);
            if drive.fire
                && self.gun_cooldown <= 0.0
                && let Some(pose) = self.current
            {
                self.gun_cooldown = interval;
                fired.push(FiredShot {
                    tick: self.tick,
                    position: pose.position,
                });
            }
            self.predictor.step(drive.drive.x, drive.drive.z);
            self.touched.add(self.predictor.hull_contacts());
            self.inputs.push_back((self.tick, drive));
            self.inputs.retain_back(MAX_PENDING_TICKS);
            self.previous = self.current;
            self.current = self.predictor.pose();
            if let Some(pose) = self.current {
                self.poses.push_back((self.tick, pose));
                self.poses.retain_back(MAX_PENDING_TICKS);
            }
            steps += 1;
        }
        fired
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

/// How far `hull` is from the pose predicted for its tick, when it is close enough to
/// confirm it.
fn agreement(predicted: &PredictedPose, hull: &HullState) -> Option<f64> {
    let apart = |a: Point3, b: Point3| (a.x - b.x).hypot(a.y - b.y).hypot(a.z - b.z);
    let position = apart(predicted.position, hull.position);
    (position <= CONFIRM_POSITION
        && apart(predicted.velocity, hull.velocity) <= CONFIRM_VELOCITY
        && angle_delta(predicted.heading, hull.heading).abs() <= CONFIRM_HEADING
        && (predicted.speed - hull.speed).abs() < STEP)
        .then_some(position)
}

/// Whether another live tank or movable cover in `scene` is within [`MOVER_REACH`] of
/// the viewer's hull at `position`.
fn movers_near(scene: &RenderState, position: Point3) -> bool {
    let at = position.planar();
    scene
        .tanks
        .iter()
        .filter(|tank| tank.alive && tank.id != scene.viewer_id)
        .map(|tank| tank.position.planar())
        .chain(
            scene
                .covers
                .iter()
                .filter(|cover| cover.alive && cover.kind.movable())
                .map(|cover| cover.position.planar()),
        )
        .any(|other| distance(at, other) < MOVER_REACH)
}

impl CoverCast for HullPrediction {
    fn cast_cover(&self, origin: Point3, direction: Vec2, distance: f64) -> Option<CoverHit> {
        CoverCast::cast_cover(&self.predictor, origin, direction, distance)
    }
}
