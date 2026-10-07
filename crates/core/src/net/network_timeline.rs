//! The client's display timeline (`src/net/interpolation.ts`): remote poses, events and
//! projectile traces share one delayed playout clock; only the local hull targets the
//! present.

use std::collections::VecDeque;

use super::fixed_step_clock::SIMULATION_STEP_MS;
use super::playout_clock::PlayoutClock;
use super::render_timeline::RenderTimeline;
use super::replication::{ShotTrace, TimedEvent};
use crate::sim::render_state::{RenderShot, RenderState};
use crate::sim::types::SimEvent;

const MAX_HISTORY_ITEMS: usize = 4096;
const MAX_LOCAL_LEAD_MS: f64 = 150.0;
/// Traces are kept this many ticks past their end, for late display reads.
const TRACE_RETENTION_TICKS: f64 = 12.0;

#[derive(Clone, Debug, Default)]
pub struct NetworkTimeline {
    pub clock: PlayoutClock,
    poses: RenderTimeline,
    events: VecDeque<TimedEvent>,
    traces: Vec<ShotTrace>,
    newest_tick: u64,
    display_tick: f64,
}

fn at_tick(mut state: RenderState, tick: u64) -> RenderState {
    state.elapsed = tick as f64 / 60.0;
    state
}

impl NetworkTimeline {
    /// Starts over from a baseline received at `now_ms`.
    pub fn reset(&mut self, state: &RenderState, tick: u64, now_ms: f64) {
        self.newest_tick = tick;
        self.clock.reset(tick, now_ms);
        self.display_tick = self.clock.display_ms / SIMULATION_STEP_MS;
        self.events.clear();
        self.traces.clear();
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
        traces: Vec<ShotTrace>,
    ) -> Result<(), String> {
        self.newest_tick = tick;
        self.poses.push(at_tick(state, tick));
        self.events.extend(events);
        self.traces.extend(traces);
        if self.events.len() > MAX_HISTORY_ITEMS || self.traces.len() > MAX_HISTORY_ITEMS {
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
        // The local hull extrapolates toward the server's present: newest path time plus one way.
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
        let display = self.display_tick;
        let caught_up = display >= self.newest_tick as f64;
        let traced = |id: u32| self.traces.iter().any(|trace| trace.shot.id == id);
        let shots = &mut output.shots;
        shots.retain(|shot| !traced(shot.id) || caught_up);
        for trace in &self.traces {
            if trace.tick <= display && display < trace.end_tick {
                let alpha = (display - trace.tick) / (trace.end_tick - trace.tick);
                let shot = RenderShot {
                    x: trace.shot.x + (trace.end.x - trace.shot.x) * alpha,
                    z: trace.shot.z + (trace.end.z - trace.shot.z) * alpha,
                    ..trace.shot
                };
                match shots.iter_mut().find(|existing| existing.id == shot.id) {
                    Some(existing) => *existing = shot,
                    None => shots.push(shot),
                }
            }
        }
        self.traces
            .retain(|trace| trace.end_tick >= display - TRACE_RETENTION_TICKS);
        events
    }
}
