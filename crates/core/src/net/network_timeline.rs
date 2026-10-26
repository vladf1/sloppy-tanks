//! The client's display timeline (`src/net/interpolation.ts`): remote poses, events and
//! projectile paths share one delayed playout clock; only the local hull targets the
//! present. Own-hull prediction draws over that hull while it has one (see
//! `RenderTimeline`).
//!
//! "Dodger first": enemy shells are drawn toward the viewer's present as they close in
//! on it ([`Dodger`]). A shell's presence grows from 0 where it was fired to 1 at the
//! viewer's hull, by how much of that distance it has covered where it is drawn, and it
//! is drawn that share of the view delay ahead of the display; it reaches the hull on the
//! viewer's own clock, so whether it meets the predicted hull shows as the host will
//! decide it. Once it has passed the viewer it eases back to the display clock at four
//! fifths of real time, never running backwards; a bounce starts its approach anew. Ahead of the host's newest tick a shell
//! stops at cover the predicted world puts in its way, and at the viewer's hull, showing
//! its impact there; the host's own effect for it is then dropped. The host's effects at
//! a shell's bounces and end play when the shell is drawn there, and hits on the viewer
//! as they arrive. Deaths, destruction and everything else wait for the display.

use std::collections::VecDeque;

use super::fixed_step_clock::SIMULATION_STEP_MS;
use super::own_shots::{CoverCast, Forecast, forecast};
use super::playout_clock::PlayoutClock;
use super::render_timeline::{MAX_EXTRAPOLATION_SECONDS, RenderTimeline};
use super::replication::TimedEvent;
use super::shot_paths::{LivePaths, PathEntry, ShotPath};
use crate::sim::combat_rules::COMBAT;
use crate::sim::data::weapon;
use crate::sim::hitboxes::SHELL_HIT_RADIUS;
use crate::sim::math::{Vec2, distance};
use crate::sim::render_state::RenderState;
use crate::sim::tank_dimensions::tank_hull;
use crate::sim::types::{SimEvent, SimEventType, Team, VehicleKind, Weapon};

const MAX_HISTORY_ITEMS: usize = 4096;
/// How far the fallback local hull may aim past the newest received tick, and at most
/// how much of the one-way delay it adds on top.
const MAX_LOCAL_LEAD_MS: f64 = 150.0;
/// Paths are kept this many ticks past their end, for late display reads.
const PATH_RETENTION_TICKS: f64 = 12.0;
/// A shell is past the viewer once it is this much (metres) farther than its nearest
/// approach.
const RECEDE_DISTANCE: f64 = 1.0;
/// A shell that passed the viewer eases back to the display clock at this share of real
/// time.
const LEAVE_PACE: f64 = 0.8;
/// A host effect within this many ticks and metres of a shell's bounce, end or strike is
/// that one's.
const STRIKE_TICKS: f64 = 2.0;
const STRIKE_DISTANCE: f64 = 1.5;
/// A strike on the viewer's hull the host has not reported this many ticks after it never
/// happened (the predicted hull was elsewhere on the host): the shell flies on.
const STRIKE_CONFIRM_TICKS: f64 = 3.0;
/// Most local strikes remembered for dropping the host's effects.
const MAX_STRIKES: usize = 32;

/// The viewer's hull in the predicted present, which incoming shells are drawn toward.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dodger {
    /// The predicted present, fractional tick.
    pub present: f64,
    pub position: Vec2,
    pub heading: f64,
    pub kind: VehicleKind,
    pub team: Team,
    pub tank_id: u32,
}

impl Dodger {
    fn gap(&self, point: Vec2) -> f64 {
        hull_gap(self.kind, self.position, self.heading, point)
    }
}

/// How far a shell at `point` is from the hit box (the hull widened by a shell's radius)
/// of a `kind` hull at `position` facing `heading`, metres; 0 inside it.
pub(super) fn hull_gap(kind: VehicleKind, position: Vec2, heading: f64, point: Vec2) -> f64 {
    let hull = tank_hull(kind);
    let (dx, dz) = (point.x - position.x, point.z - position.z);
    let (sin, cos) = heading.sin_cos();
    let (across, along) = (dx * cos - dz * sin, dx * sin + dz * cos);
    let outside = |offset: f64, half: f64| (offset.abs() - half - SHELL_HIT_RADIUS).max(0.0);
    outside(across - hull.center.x, hull.size.x / 2.0)
        .hypot(outside(along - hull.center.z, hull.size.z / 2.0))
}

/// How a shell in flight is drawn: where it was fired, how far ahead of the display, what
/// the predicted world says ahead of the host's ticks, and the host's effects along it.
#[derive(Clone, Debug)]
struct Shell {
    id: u32,
    team: Team,
    /// Where its current leg started (fired or bounced), and that leg's start tick.
    fired: Vec2,
    leg: Option<f64>,
    /// The largest share of its flight to the viewer it has covered where drawn.
    presence: f64,
    /// Its nearest approach to the viewer so far where drawn, metres.
    closest: f64,
    /// Whether it has passed the viewer.
    receding: bool,
    /// Ticks ahead of the display it is drawn at.
    lead: f64,
    /// Where its newest open path meets cover in the predicted world.
    forecast: Forecast,
    /// The tick it struck the viewer's hull ahead of the host's.
    struck: Option<f64>,
    /// It struck the hull where the host flew past: it strikes it no more.
    missed: bool,
    /// The host's effects at its bounces and end, shown when it is drawn there.
    events: Vec<TimedEvent>,
}

impl Shell {
    fn new(path: &ShotPath, forecast: Forecast) -> Self {
        Self {
            id: path.id,
            team: path.launch.team,
            fired: Vec2::new(path.x, path.z),
            leg: None,
            presence: 0.0,
            closest: f64::INFINITY,
            receding: false,
            lead: 0.0,
            forecast,
            struck: None,
            missed: false,
            events: Vec::new(),
        }
    }

    /// The tick it is drawn at for `dodger` with the display at `display`, `elapsed`
    /// display ticks after the last read; `leg_at` gives its path at a tick.
    fn clock(
        &mut self,
        display: f64,
        elapsed: f64,
        dodger: &Dodger,
        leg_at: impl Fn(f64) -> Option<DisplayedPath>,
    ) -> f64 {
        let ahead = (dodger.present - display).max(0.0);
        let leaving = (self.lead - (1.0 - LEAVE_PACE) * elapsed).max(0.0);
        // Presence is measured where the shell is drawn, so it meets the viewer just as
        // the viewer's present reaches it.
        let guess = display
            + if self.receding {
                leaving
            } else {
                ahead * self.presence
            };
        if let Some(leg) = leg_at(guess).filter(|leg| guess < leg.until) {
            if self.leg != Some(leg.path.tick) {
                // A bounce starts its approach anew from the clock it is drawn on.
                self.leg = Some(leg.path.tick);
                self.fired = Vec2::new(leg.path.x, leg.path.z);
                self.closest = f64::INFINITY;
                self.receding = false;
                if ahead > 0.0 {
                    self.presence = (self.lead / ahead).min(1.0);
                }
            }
            if !self.receding {
                let at = leg.path.at(guess);
                let gap = dodger.gap(Vec2::new(at.x, at.z));
                self.closest = self.closest.min(gap);
                self.receding = gap > self.closest + RECEDE_DISTANCE;
                let span = dodger.gap(self.fired);
                let presence = if span > 0.0 {
                    (1.0 - gap / span).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                if !self.receding {
                    self.presence = self.presence.max(presence);
                }
            }
        }
        self.lead = if self.receding {
            leaving
        } else {
            ahead * self.presence
        };
        display + self.lead
    }
}

/// A shell's path on the display clock: drawn from its start until `until`, the start of
/// the shell's next path or its end; unbounded while it is the current one.
#[derive(Clone, Copy, Debug)]
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
    shells: Vec<Shell>,
    /// Strikes of incoming shells shown ahead of the host, whose own effects are dropped.
    strikes: VecDeque<TimedEvent>,
    /// The display tick of the last read.
    last_display: f64,
    /// Strikes on the viewer's hull the host never confirmed, for the statistics.
    pub phantom_strikes: u64,
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
        self.events.clear();
        self.paths.clear();
        self.shells.clear();
        self.strikes.clear();
        self.paths
            .extend(shots.paths.iter().map(|&path| DisplayedPath {
                path,
                until: f64::INFINITY,
            }));
        self.shells.extend(
            shots
                .paths
                .iter()
                .map(|path| Shell::new(path, Forecast::default())),
        );
        self.last_display = tick as f64;
        self.poses.reset(at_tick(state.clone(), tick));
    }

    /// Records one message's arrival after its frames have been pushed.
    pub fn arrive(&mut self, now_ms: f64) {
        self.clock.arrive(self.newest_tick, now_ms);
    }

    /// Takes ownership of one received frame, forecasting where its new shell paths meet
    /// `cover`. Fails when history overflows; the client then resyncs.
    pub fn push(
        &mut self,
        state: RenderState,
        tick: u64,
        events: Vec<TimedEvent>,
        paths: Vec<PathEntry>,
        cover: &impl CoverCast,
    ) -> Result<(), String> {
        self.newest_tick = tick;
        self.poses.push(at_tick(state, tick));
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
            let shell = self.shells.iter_mut().find(|shell| shell.id == id);
            match (entry, shell) {
                (PathEntry::Launch(path), _) => self.shells.push(Shell::new(
                    &path,
                    forecast(&path, weapon(path.launch.weapon).bounces, cover),
                )),
                (PathEntry::Change(path), Some(shell)) => {
                    shell.forecast = forecast(&path, 0, cover)
                }
                (PathEntry::End { .. }, Some(shell)) => shell.forecast = Forecast::default(),
                _ => {}
            }
        }
        for timed in events {
            // The host's effects of strikes already shown are not shown again.
            if self.strikes.iter().any(|shown| same_strike(shown, &timed)) {
                continue;
            }
            let paths = &self.paths;
            let shell = matches!(
                timed.event.kind,
                SimEventType::Impact
                    | SimEventType::Ricochet
                    | SimEventType::Explosion
                    | SimEventType::Shatter
            )
            .then(|| {
                let at = Vec2::new(timed.event.x, timed.event.z);
                self.shells.iter_mut().find(|shell| {
                    turns(paths, shell.id).any(|(tick, point)| {
                        (tick - timed.tick).abs() <= STRIKE_TICKS
                            && distance(point, at) <= STRIKE_DISTANCE
                    })
                })
            })
            .flatten();
            match shell {
                Some(shell) => shell.events.push(timed),
                None => self.events.push_back(timed),
            }
        }
        if self.events.len() > MAX_HISTORY_ITEMS || self.paths.len() > MAX_HISTORY_ITEMS {
            return Err("Display history overflow; resync required".into());
        }
        Ok(())
    }

    /// The shell whose path ended within a couple of ticks of `tick` nearest `point`, within
    /// `reach` metres.
    pub fn shell_ending_near(&self, tick: f64, point: Vec2, reach: f64) -> Option<u32> {
        self.paths
            .iter()
            .filter(|displayed| (displayed.until - tick).abs() <= STRIKE_TICKS)
            .map(|displayed| {
                let at = displayed.path.at(displayed.until);
                (displayed.path.id, distance(Vec2::new(at.x, at.z), point))
            })
            .filter(|(_, gap)| *gap <= reach)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id)
    }

    /// Received simulation still ahead of the display; negative while remote poses
    /// extrapolate.
    pub fn margin_ms(&self) -> f64 {
        (self.newest_tick as f64 - self.display_tick()) * SIMULATION_STEP_MS
    }

    /// The display tick of the last read (fractional).
    pub fn display_tick(&self) -> f64 {
        self.clock.display_ms / SIMULATION_STEP_MS
    }

    /// Fills `output` with the scene to draw at `now_ms`, overwriting all of it in place,
    /// and returns the events whose tick the display just reached. With a `dodger`, shells
    /// coming at it and their effects near it are drawn toward its present.
    pub fn read(
        &mut self,
        now_ms: f64,
        rtt_ms: f64,
        dt: f64,
        output: &mut RenderState,
        dodger: Option<&Dodger>,
    ) -> Vec<SimEvent> {
        let display = self.clock.read(now_ms) / SIMULATION_STEP_MS;
        let newest_ms = self.newest_tick as f64 * SIMULATION_STEP_MS;
        // The fallback local hull extrapolates toward the server's present: newest path
        // time plus one way.
        let local_time = ((newest_ms + MAX_LOCAL_LEAD_MS).min(self.clock.path_server_ms(now_ms))
            + MAX_LOCAL_LEAD_MS.min(rtt_ms / 2.0))
            / 1000.0;
        self.poses.read(display / 60.0, local_time, dt, output);
        let mut events = Vec::new();
        while let Some(timed) = self.events.pop_front_if(|event| event.tick <= display) {
            events.push(timed.event);
        }
        if let Some(dodger) = dodger {
            // The viewer's present is past its hits already when they arrive.
            self.events.retain(|timed| {
                let hit = timed.event.kind == SimEventType::Hurt
                    && timed.event.id == Some(dodger.tank_id);
                if hit {
                    events.push(timed.event.clone());
                }
                !hit
            });
        }
        let elapsed = (display - self.last_display).max(0.0);
        self.last_display = display;
        // A shell whose end has not arrived flies on its path only as far as hulls
        // extrapolate past the newest frame, unless it is drawn ahead of the display.
        let newest = self.newest_tick as f64;
        let horizon = newest + MAX_EXTRAPOLATION_SECONDS * 1000.0 / SIMULATION_STEP_MS;
        output.shots.clear();
        let paths = &self.paths;
        let strikes = &mut self.strikes;
        let mut phantoms = 0;
        for shell in &mut self.shells {
            let id = shell.id;
            let leg_at = |tick: f64| {
                paths
                    .iter()
                    .rev()
                    .find(|displayed| displayed.path.id == id && displayed.path.tick <= tick)
                    .copied()
            };
            let incoming = dodger.filter(|dodger| dodger.team != shell.team);
            let tick = match incoming {
                Some(dodger) => shell.clock(display, elapsed, dodger, leg_at),
                None => display,
            };
            shell.events.retain(|timed| {
                let due = timed.tick <= tick;
                if due {
                    events.push(timed.event.clone());
                }
                !due
            });
            // A strike on the hull the host flew past instead never happened.
            if let Some(struck) = shell.struck
                && newest >= struck + STRIKE_CONFIRM_TICKS
                && leg_at(struck).is_some_and(|leg| leg.until > struck + STRIKE_CONFIRM_TICKS)
            {
                shell.struck = None;
                shell.missed = true;
                phantoms += 1;
            }
            let Some(leg) = leg_at(tick).filter(|leg| tick < leg.until) else {
                continue;
            };
            if shell.lead == 0.0 && tick >= horizon {
                continue;
            }
            let mut drawn = Some(leg.path.at(tick));
            if let Some(dodger) = incoming
                && leg.until.is_infinite()
                && tick > newest
            {
                // Ahead of the host: the cover the predicted world puts in its way, and
                // its strike's effect.
                for strike in &shell.forecast.strikes {
                    if strike.tick <= tick
                        && strike.tick > newest
                        && !strikes.iter().any(|shown| same_strike(shown, strike))
                    {
                        events.push(strike.event.clone());
                        strikes.push_back(strike.clone());
                    }
                }
                if shell
                    .forecast
                    .stop
                    .is_some_and(|stop| stop > newest && tick >= stop)
                {
                    drawn = shell
                        .forecast
                        .legs
                        .iter()
                        .rev()
                        .find(|next| next.path.tick <= tick && tick < next.until)
                        .map(|next| next.path.at(tick));
                }
                if shell.struck.is_some() {
                    drawn = None;
                } else if let Some(at) =
                    drawn.filter(|at| !shell.missed && dodger.gap(Vec2::new(at.x, at.z)) == 0.0)
                {
                    // It reaches the viewer's hull on the viewer's clock.
                    shell.struck = Some(tick);
                    for strike in hull_strike(&leg.path, tick, Vec2::new(at.x, at.z)) {
                        events.push(strike.event.clone());
                        strikes.push_back(strike);
                    }
                    drawn = None;
                }
            }
            if let Some(at) = drawn {
                output.shots.push(at);
            }
        }
        self.strikes.retain_back(MAX_STRIKES);
        self.phantom_strikes += phantoms;
        self.paths
            .retain(|displayed| displayed.until >= display - PATH_RETENTION_TICKS);
        let paths = &self.paths;
        self.shells
            .retain(|shell| paths.iter().any(|displayed| displayed.path.id == shell.id));
        events
    }
}

/// Where the shell `id` bounced and, once ended, where it ended.
fn turns(paths: &[DisplayedPath], id: u32) -> impl Iterator<Item = (f64, Vec2)> + '_ {
    let legs = paths
        .iter()
        .filter(move |displayed| displayed.path.id == id);
    let ends = legs
        .clone()
        .filter(|displayed| displayed.until.is_finite())
        .map(|displayed| {
            let at = displayed.path.at(displayed.until);
            (displayed.until, Vec2::new(at.x, at.z))
        });
    let starts = legs.skip(1).map(|displayed| {
        (
            displayed.path.tick,
            Vec2::new(displayed.path.x, displayed.path.z),
        )
    });
    ends.chain(starts)
}

/// Whether two strikes are the same one.
fn same_strike(a: &TimedEvent, b: &TimedEvent) -> bool {
    a.event.kind == b.event.kind
        && (a.tick - b.tick).abs() <= STRIKE_TICKS
        && distance(
            Vec2::new(a.event.x, a.event.z),
            Vec2::new(b.event.x, b.event.z),
        ) <= STRIKE_DISTANCE
}

/// The host's effects of a shell on `path` striking the viewer's hull at `point` on
/// `tick` (`resolve_contact`'s tank contact): an impact flash, after a rocket's blast.
fn hull_strike(path: &ShotPath, tick: f64, point: Vec2) -> Vec<TimedEvent> {
    let mut strikes = Vec::with_capacity(2);
    let mut timed = |event| {
        strikes.push(TimedEvent {
            event_id: 0,
            tick,
            event,
        })
    };
    if path.launch.weapon == Weapon::Rocket {
        let mut blast = SimEvent::at(SimEventType::Explosion, point.x, point.z);
        blast.size = Some(COMBAT.rocket_blast_radius);
        timed(blast);
    }
    let mut impact = SimEvent::at(SimEventType::Impact, point.x, point.z);
    impact.size = Some(0.6);
    impact.color = Some(weapon(path.launch.weapon).color);
    impact.height = Some(1.0);
    timed(impact);
    strikes
}
