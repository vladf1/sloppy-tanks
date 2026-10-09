//! The client's display timeline (`src/net/interpolation.ts`): remote poses, events and
//! projectile paths share one delayed playout clock; only the local hull targets the
//! present. Own-hull prediction draws over that hull while it has one (see
//! `RenderTimeline`).

use std::collections::VecDeque;

use super::fixed_step_clock::SIMULATION_STEP_MS;
use super::playout_clock::PlayoutClock;
use super::render_timeline::{MAX_EXTRAPOLATION_SECONDS, RenderTimeline};
use super::replication::TimedEvent;
use super::shot_paths::{LivePaths, PathEntry, ShotPath};
use crate::sim::render_state::RenderState;
use crate::sim::types::SimEvent;

const MAX_HISTORY_ITEMS: usize = 4096;
/// How far the fallback local hull may aim past the newest received tick, and at most
/// how much of the one-way delay it adds on top.
const MAX_LOCAL_LEAD_MS: f64 = 150.0;
/// Paths are kept this many ticks past their end, for late display reads.
const PATH_RETENTION_TICKS: f64 = 12.0;

/// A shell's path on the display clock: drawn from its start until `until`, the start of
/// the shell's next path or its end; unbounded while it is the current one.
#[derive(Clone, Debug)]
struct DisplayedPath {
    path: ShotPath,
    until: f64,
}

#[derive(Clone, Debug, Default)]
pub struct NetworkTimeline {
    pub clock: PlayoutClock,
    poses: RenderTimeline,
    events: VecDeque<TimedEvent>,
    paths: Vec<DisplayedPath>,
    newest_tick: u64,
    display_tick: f64,
}

fn at_tick(mut state: RenderState, tick: u64) -> RenderState {
    state.elapsed = tick as f64 / 60.0;
    state
}

impl NetworkTimeline {
    /// Starts over from a baseline received at `now_ms`, with its shells in flight.
    pub fn reset(&mut self, state: &RenderState, tick: u64, now_ms: f64, shots: &LivePaths) {
        self.newest_tick = tick;
        self.clock.reset(tick, now_ms);
        self.display_tick = self.clock.display_ms / SIMULATION_STEP_MS;
        self.events.clear();
        self.paths.clear();
        self.paths
            .extend(shots.paths.iter().map(|&path| DisplayedPath {
                path,
                until: f64::INFINITY,
            }));
        self.poses.reset(at_tick(state.clone(), tick));
    }

    /// Records one message's arrival after its frames have been pushed.
    pub fn arrive(&mut self, now_ms: f64) {
        self.clock.arrive(self.newest_tick, now_ms);
    }

    /// Takes ownership of one received frame. Fails when history overflows; the client then resyncs.
    pub fn push(
        &mut self,
        state: RenderState,
        tick: u64,
        events: Vec<TimedEvent>,
        paths: Vec<PathEntry>,
    ) -> Result<(), String> {
        self.newest_tick = tick;
        self.poses.push(at_tick(state, tick));
        self.events.extend(events);
        for entry in paths {
            let (id, from) = match entry {
                PathEntry::Launch(path) | PathEntry::Change(path) => (path.id, path.tick),
                PathEntry::End { id, tick } => (id, tick),
            };
            if let Some(current) = self
                .paths
                .iter_mut()
                .rev()
                .find(|displayed| displayed.path.id == id && displayed.until == f64::INFINITY)
            {
                current.until = from;
            }
            if let PathEntry::Launch(path) | PathEntry::Change(path) = entry {
                self.paths.push(DisplayedPath {
                    path,
                    until: f64::INFINITY,
                });
            }
        }
        if self.events.len() > MAX_HISTORY_ITEMS || self.paths.len() > MAX_HISTORY_ITEMS {
            return Err("Display history overflow; resync required".into());
        }
        Ok(())
    }

    /// Received simulation still ahead of the display; negative while remote poses
    /// extrapolate.
    pub fn margin_ms(&self) -> f64 {
        (self.newest_tick as f64 - self.display_tick) * SIMULATION_STEP_MS
    }

    /// The display tick of the last read (fractional).
    pub fn display_tick(&self) -> f64 {
        self.display_tick
    }

    /// Fills `output` with the scene to draw at `now_ms`, overwriting all of it in place,
    /// and returns the events whose tick the display just reached.
    pub fn read(
        &mut self,
        now_ms: f64,
        rtt_ms: f64,
        dt: f64,
        output: &mut RenderState,
    ) -> Vec<SimEvent> {
        self.display_tick = self.clock.read(now_ms) / SIMULATION_STEP_MS;
        let newest_ms = self.newest_tick as f64 * SIMULATION_STEP_MS;
        // The fallback local hull extrapolates toward the server's present: newest path
        // time plus one way.
        let local_time = ((newest_ms + MAX_LOCAL_LEAD_MS).min(self.clock.path_server_ms(now_ms))
            + MAX_LOCAL_LEAD_MS.min(rtt_ms / 2.0))
            / 1000.0;
        self.poses
            .read(self.display_tick / 60.0, local_time, dt, output);
        let mut events = Vec::new();
        while self
            .events
            .front()
            .is_some_and(|event| event.tick <= self.display_tick)
        {
            events.push(self.events.pop_front().expect("checked").event);
        }
        // A shell whose end has not arrived flies on its path only as far as hulls
        // extrapolate past the newest frame.
        let display = self.display_tick;
        let horizon =
            self.newest_tick as f64 + MAX_EXTRAPOLATION_SECONDS * 1000.0 / SIMULATION_STEP_MS;
        output.shots.clear();
        for displayed in &self.paths {
            if displayed.path.tick <= display && display < displayed.until.min(horizon) {
                output.shots.push(displayed.path.at(display));
            }
        }
        self.paths
            .retain(|displayed| displayed.until >= display - PATH_RETENTION_TICKS);
        events
    }
}
