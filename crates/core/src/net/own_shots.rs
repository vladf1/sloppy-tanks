//! The viewer's own shots, drawn the moment they are fired.
//!
//! The own-hull prediction runs the gun's reload like the host and says when held fire
//! shoots, at the tick the input asks the host to start on ([`super::hull_prediction`]).
//! Each such shot draws its muzzle flash, recoil and sound in that frame and launches the
//! shells the host will launch (three for spread) from the predicted muzzle. When the
//! host's launch of the same shell arrives it is recognised by its team, its launch tick
//! and where and which way it starts (the wire names no owner); the shell then follows
//! the host's path under its predicted id, so it neither jumps back nor restarts its
//! tracer, and the host's own muzzle flash for it is dropped. A predicted shell the host
//! never launched (the client misjudged the reload, the ammo or a death) is dropped once
//! the host's ticks pass it. Hits, damage and kills stay the host's.
//!
//! Own shells are drawn on a clock of their own: at the predicted present when fired, so
//! they leave the barrel at once, easing into the delayed display over their first
//! [`BLEND_VIEW_DELAYS`] view delays of flight, so they reach distant tanks and walls on
//! the timeline those are drawn on. Until the host's path is known that far, a shell ends
//! at the first cover the predicted world puts in its way, bouncing as the host's would
//! with bounces left, and shows the host's impact effect there itself; the host's own
//! effect for it is dropped when it arrives. The host's effects of own shells the
//! prediction did not foresee play on the shell's clock.

use std::collections::VecDeque;

use super::replication::TimedEvent;
use super::shot_paths::{PathEntry, ShotLaunch, ShotPath};
use crate::sim::combat_rules::COMBAT;
use crate::sim::data::{STEP, TEAM_COLORS, weapon};
use crate::sim::math::{Point3, Vec2, angle_delta, distance};
use crate::sim::projectiles::RocketThrust;
use crate::sim::render_state::RenderShot;
use crate::sim::simulation_rules::SIMULATION_RULES;
use crate::sim::tank_dimensions::{tank_muzzle, tank_visual_muzzle};
use crate::sim::types::{CoverKind, SimEvent, SimEventType, Team, VehicleKind, Weapon};
use crate::sim::weapons::MUZZLE_FRAME_OFFSET;

/// Multiplayer always fires at the standard shell speed.
const BULLET_SPEED: f64 = 1.0;
/// Render ids of predicted shells start here, above any id the host issues.
const FIRST_SHELL_ID: u32 = 0xf000_0000;
/// The host's launch of a predicted shell starts within this many ticks of it, this close
/// to its start (metres) and heading this close to its way (radians, well under the spread
/// between pellets).
const MATCH_TICKS: f64 = 4.0;
const MATCH_DISTANCE: f64 = 1.5;
const MATCH_HEADING: f64 = 0.08;
/// A predicted shot the host has not fired this many ticks past it never happened.
const UNCONFIRMED_TICKS: u64 = 12;
/// Own shells reach the delayed display after this many view delays of flight: they are
/// drawn at four fifths of their speed meanwhile.
const BLEND_VIEW_DELAYS: f64 = 4.0;
/// A host impact effect within this many ticks and metres of where an own shell's path
/// ended or bounced belongs to that shell.
const IMPACT_TICKS: f64 = 1.5;
const IMPACT_DISTANCE: f64 = 1.0;
/// Bounds on the shells and fired ticks kept.
const MAX_SHELLS: usize = 64;
const MAX_FIRED: usize = 32;
/// Legs a forecast follows: a ricochet shell's three bounces and the flight after them.
const MAX_FORECAST_LEGS: usize = 4;
/// Steps of the search for when an accelerating rocket has flown a distance.
const FLIGHT_SEARCH_STEPS: usize = 24;
/// Where a predicted shell was off the host's when its launch arrived (a late input
/// starts the host's a tick or two later, about 36 cm a tick), the drawn shell glides onto
/// the host's path, the difference decaying at this rate per second of the shell's clock.
const HANDOVER_RATE: f64 = 25.0;
/// A handover difference below this share of the first is gone.
const HANDOVER_DONE: f64 = 0.01;

/// Where a shell starting at a point and heading along a planar unit direction first
/// meets cover within a distance, as the host's shell query sees the cover.
pub trait CoverCast {
    fn cast_cover(&self, origin: Point3, direction: Vec2, distance: f64) -> Option<CoverHit>;
}

/// Where a shell meets cover: how far along, the face's planar normal and the cover.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoverHit {
    pub distance: f64,
    pub normal: Vec2,
    pub struck: Option<Struck>,
}

/// The cover a shell struck, which colours its impact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Struck {
    pub kind: CoverKind,
    pub color: u32,
    pub height: f64,
}

/// A shot the prediction fired, with what the client knows of the viewer's gun.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Firing {
    /// The host tick it fires on.
    pub tick: u64,
    /// The hull's position when it fires, after the tick before.
    pub position: Point3,
    pub aim: f64,
    pub kind: VehicleKind,
    pub weapon: Weapon,
    pub team: Team,
    pub tank_id: u32,
}

/// One leg of a shell's flight, drawn from its start until `until`.
#[derive(Clone, Copy, Debug)]
struct Leg {
    path: ShotPath,
    until: f64,
}

/// Where an open leg meets cover (fractional tick), the legs a bouncing shell flies after
/// it, and the impact effect of each strike.
#[derive(Clone, Debug, Default)]
struct Forecast {
    stop: Option<f64>,
    legs: Vec<Leg>,
    strikes: Vec<TimedEvent>,
}

#[derive(Clone, Debug)]
struct OwnShell {
    render_id: u32,
    fired_tick: u64,
    /// The predicted launch leg until the host's arrives, then the host's legs.
    legs: Vec<Leg>,
    forecast: Forecast,
    /// The host's shell id once its launch arrived.
    server_id: Option<u32>,
    bounces: u32,
    ended: bool,
    /// Host impact effects for this shell, played when its clock reaches their tick.
    events: Vec<TimedEvent>,
    /// Predicted strikes already shown, whose host effects are dropped.
    shown: Vec<TimedEvent>,
    /// The predicted launch leg the drawn shell is gliding off, and the tick of its clock
    /// the glide started, once drawn.
    handover: Option<(ShotPath, Option<f64>)>,
}

impl OwnShell {
    fn launch_tick(&self) -> f64 {
        self.legs[0].path.tick
    }

    /// The shell at fractional tick `tick` of its clock, with the host's newest tick
    /// `newest`; `None` before launch or after it ended.
    fn at(&self, tick: f64, newest: f64) -> Option<RenderShot> {
        if tick >= self.launch_tick() + COMBAT.projectile_lifetime / STEP {
            return None;
        }
        let leg = self.legs.iter().rev().find(|leg| leg.path.tick <= tick)?;
        if tick >= leg.until {
            return None;
        }
        // An open leg past the host's newest tick flies only until the predicted cover,
        // unless the host already flew it past that point.
        if leg.until.is_infinite()
            && tick > newest
            && let Some(stop) = self.forecast.stop.filter(|stop| *stop > newest)
            && tick >= stop
        {
            let next = self
                .forecast
                .legs
                .iter()
                .rev()
                .find(|next| next.path.tick <= tick)?;
            return (tick < next.until).then(|| shell(next.path.at(tick), self.render_id));
        }
        Some(shell(leg.path.at(tick), self.render_id))
    }

    /// Where the shell's newest leg ends or bounces: its last leg's start (a bounce) and,
    /// once ended, its end.
    fn turns(&self) -> impl Iterator<Item = (f64, Vec2)> + '_ {
        let ends = self
            .legs
            .iter()
            .filter(|leg| leg.until.is_finite())
            .map(|leg| {
                let at = leg.path.at(leg.until);
                (leg.until, Vec2::new(at.x, at.z))
            });
        let starts = self
            .legs
            .iter()
            .skip(1)
            .map(|leg| (leg.path.tick, Vec2::new(leg.path.x, leg.path.z)));
        ends.chain(starts)
    }
}

fn shell(mut drawn: RenderShot, render_id: u32) -> RenderShot {
    drawn.id = render_id;
    drawn
}

/// Fractional ticks a shell on `path` takes to fly `range` metres from its start.
fn flight_ticks(path: &ShotPath, range: f64) -> f64 {
    let speed = path.vx.hypot(path.vz);
    if path.launch.thrust.is_none() {
        return if speed > 0.0 {
            range / speed / STEP
        } else {
            f64::INFINITY
        };
    }
    let flown = |ticks: f64| {
        let at = path.at(path.tick + ticks);
        (at.x - path.x).hypot(at.z - path.z)
    };
    let (mut low, mut high) = (0.0, COMBAT.projectile_lifetime / STEP);
    for _ in 0..FLIGHT_SEARCH_STEPS {
        let middle = (low + high) / 2.0;
        if flown(middle) < range {
            low = middle;
        } else {
            high = middle;
        }
    }
    high
}

/// The effect the host shows where a shell on `path` strikes cover at `tick`, ending
/// there or bouncing off (`resolve_contact`'s world contact).
fn strike(path: &ShotPath, tick: f64, struck: Option<Struck>, bounces: bool) -> TimedEvent {
    let at = path.at(tick);
    let speed = at.vx.hypot(at.vz);
    let launch = &path.launch;
    let event = if launch.weapon == Weapon::Rocket {
        let mut blast = SimEvent::at(SimEventType::Explosion, at.x, at.z);
        blast.size = Some(COMBAT.rocket_blast_radius);
        blast
    } else if bounces {
        let mut ricochet = SimEvent::at(SimEventType::Ricochet, at.x, at.z);
        ricochet.weapon = Some(launch.weapon);
        ricochet.team = Some(launch.team);
        ricochet.size = Some(0.6);
        ricochet
    } else {
        let chipped = struck.filter(|struck| {
            matches!(
                struck.kind,
                CoverKind::Tree | CoverKind::Timber | CoverKind::Cargo
            )
        });
        let mut impact = SimEvent::at(SimEventType::Impact, at.x, at.z);
        impact.size = Some(0.6);
        impact.color = Some(chipped.map_or(weapon(launch.weapon).color, |struck| struck.color));
        impact.cover_kind = chipped.map(|struck| struck.kind);
        impact.height = chipped.map(|struck| struck.height);
        // A crate's splinters burst from the struck face, back along the shell.
        if chipped.is_some_and(|struck| struck.kind == CoverKind::Cargo) && speed > 0.0 {
            let y = launch.y.unwrap_or(1.0);
            impact.height = Some(y);
            impact.from = Some(Point3::new(at.x - at.vx / speed, y, at.z - at.vz / speed));
        }
        impact
    };
    TimedEvent {
        event_id: 0,
        tick,
        event,
    }
}

/// Where `path`, an open leg, meets cover in the predicted world and how it flies on with
/// `bounces` left: off the cover's face like the host's ricochet, or not at all.
fn forecast(path: &ShotPath, bounces: u32, cover: &impl CoverCast) -> Forecast {
    let mut result = Forecast::default();
    let mut current = *path;
    let mut bounces_left = bounces;
    while result.legs.len() < MAX_FORECAST_LEGS {
        let speed = current.vx.hypot(current.vz);
        if speed <= 0.0 {
            break;
        }
        let direction = Vec2::new(current.vx / speed, current.vz / speed);
        let top_speed = current
            .launch
            .thrust
            .map_or(speed, |thrust| thrust.top_speed.max(speed));
        let origin = Point3::new(current.x, current.launch.y.unwrap_or(1.0), current.z);
        let Some(hit) = cover.cast_cover(origin, direction, top_speed * COMBAT.projectile_lifetime)
        else {
            break;
        };
        let normal = hit.normal;
        let hit_tick = current.tick + flight_ticks(&current, hit.distance);
        match result.legs.last_mut() {
            Some(last) => last.until = hit_tick,
            None => result.stop = Some(hit_tick),
        }
        result
            .strikes
            .push(strike(&current, hit_tick, hit.struck, bounces_left > 0));
        if bounces_left == 0 {
            break;
        }
        bounces_left -= 1;
        let hit = current.at(hit_tick);
        let along = hit.vx * normal.x + hit.vz * normal.z;
        current = ShotPath {
            tick: hit_tick,
            x: hit.x + normal.x * COMBAT.bounce_clearance,
            z: hit.z + normal.z * COMBAT.bounce_clearance,
            vx: hit.vx - 2.0 * along * normal.x,
            vz: hit.vz - 2.0 * along * normal.z,
            ..current
        };
        result.legs.push(Leg {
            path: current,
            until: f64::INFINITY,
        });
    }
    result
}

/// How own-shot prediction went, for the statistics.
#[derive(Clone, Debug, Default)]
pub struct OwnShotStats {
    /// Shots the prediction fired.
    pub predicted: u64,
    /// Of those, shots the host fired too, and predicted shells it never launched.
    pub confirmed: u64,
    pub unconfirmed_shells: u64,
    /// Own shots the host fired that the prediction did not.
    pub unpredicted: u64,
    /// Per matched shell: how far the predicted shell was from the host's six ticks
    /// after launch, metres.
    pub error: super::feel_readings::Samples,
}

/// A fired tick the host may still confirm.
#[derive(Clone, Copy, Debug)]
struct Fired {
    tick: u64,
    confirmed: bool,
}

#[derive(Clone, Debug, Default)]
pub struct OwnShots {
    shells: Vec<OwnShell>,
    fired: VecDeque<Fired>,
    next_id: u32,
    /// Gun recoil of the drawn own tank, 1 just after a shot and recovering to 0.
    pub recoil: f64,
    pub stats: OwnShotStats,
}

impl OwnShots {
    /// Forgets every shell, for a new baseline, round or room.
    pub fn reset(&mut self) {
        self.shells.clear();
        self.fired.clear();
        self.recoil = 0.0;
    }

    /// Launches a predicted shot: its shells from the predicted muzzle and the recoil,
    /// returning the muzzle flash to draw now (the host's `Shot` event).
    pub fn fire(&mut self, firing: Firing, cover: &impl CoverCast) -> SimEvent {
        let Firing {
            tick,
            position,
            aim,
            kind,
            weapon: fired,
            team,
            tank_id,
        } = firing;
        // As `fire_weapon`: a muzzle pulled back out of cover, rounds along the aim.
        let muzzle = tank_muzzle(kind);
        let visual_muzzle = tank_visual_muzzle(kind);
        let muzzle_height = position.y - MUZZLE_FRAME_OFFSET + muzzle.y;
        let visual_height = position.y - MUZZLE_FRAME_OFFSET + visual_muzzle.y;
        let direction = Vec2::new(aim.sin(), aim.cos());
        let mut spawn = muzzle.z;
        if let Some(hit) = cover.cast_cover(
            Point3::new(position.x, muzzle_height, position.z),
            direction,
            muzzle.z,
        ) {
            spawn = spawn.min(hit.distance);
        }
        if spawn < muzzle.z {
            spawn = 0f64.max(spawn - COMBAT.muzzle_clearance);
        }
        let offsets: &[f64] = if fired == Weapon::Spread {
            &[-COMBAT.spread_angle, 0.0, COMBAT.spread_angle]
        } else {
            &[0.0]
        };
        let stats = weapon(fired);
        let launch = ShotLaunch {
            team,
            weapon: fired,
            y: Some(muzzle_height),
            visual_y: Some(visual_height),
            thrust: (fired == Weapon::Rocket).then(|| RocketThrust::at_bullet_speed(BULLET_SPEED)),
        };
        // A rocket takes its first tick of thrust before its first sweep, so its path
        // starts at the speed after it (`step_projectiles`).
        let speed = stats.speed * BULLET_SPEED;
        let speed = launch.thrust.map_or(speed, |thrust| {
            thrust.top_speed.min(speed + thrust.acceleration * STEP)
        });
        for offset in offsets {
            let angle = aim + offset;
            let path = ShotPath {
                id: 0,
                tick: tick as f64 - 1.0,
                x: position.x + direction.x * spawn,
                z: position.z + direction.z * spawn,
                vx: angle.sin() * speed,
                vz: angle.cos() * speed,
                launch,
            };
            let render_id = FIRST_SHELL_ID + self.next_id % (u32::MAX - FIRST_SHELL_ID);
            self.next_id = self.next_id.wrapping_add(1);
            if self.shells.len() >= MAX_SHELLS {
                self.shells.remove(0);
            }
            self.shells.push(OwnShell {
                render_id,
                fired_tick: tick,
                legs: vec![Leg {
                    path,
                    until: f64::INFINITY,
                }],
                forecast: forecast(&path, stats.bounces, cover),
                server_id: None,
                bounces: stats.bounces,
                ended: false,
                events: Vec::new(),
                shown: Vec::new(),
                handover: None,
            });
        }
        self.fired.push_back(Fired {
            tick,
            confirmed: false,
        });
        self.fired.retain_back(MAX_FIRED);
        self.stats.predicted += 1;
        self.recoil = 1.0;
        let mut flash = SimEvent::at(
            SimEventType::Shot,
            position.x + direction.x * muzzle.z,
            position.z + direction.z * muzzle.z,
        );
        flash.weapon = Some(fired);
        flash.id = Some(tank_id);
        flash.team = Some(team);
        flash.size = Some(if fired == Weapon::Rocket { 1.5 } else { 1.0 });
        flash.color = Some(TEAM_COLORS[team.index()]);
        flash
    }

    /// Takes the host's entries for own shells out of one received frame: launches that
    /// match predicted shells, their changes and ends, the muzzle flashes of predicted
    /// shots and the impact effects of own shells. `team` and `tank_id` are the viewer's.
    pub fn claim(
        &mut self,
        paths: &mut Vec<PathEntry>,
        events: &mut Vec<TimedEvent>,
        team: Team,
        tank_id: u32,
        cover: &impl CoverCast,
    ) {
        paths.retain(|entry| !self.take_path(entry, team, cover));
        events.retain(|timed| !self.take_event(timed, tank_id));
    }

    /// Whether `entry` belongs to an own shell, which then follows it.
    fn take_path(&mut self, entry: &PathEntry, team: Team, cover: &impl CoverCast) -> bool {
        match *entry {
            PathEntry::Launch(path) => {
                if path.launch.team != team {
                    return false;
                }
                let heading = path.vx.atan2(path.vz);
                let Some(shell) = self
                    .shells
                    .iter_mut()
                    .filter(|shell| shell.server_id.is_none())
                    .filter_map(|shell| {
                        let predicted = shell.legs[0].path;
                        let off_heading =
                            angle_delta(predicted.vx.atan2(predicted.vz), heading).abs();
                        ((predicted.tick - path.tick).abs() <= MATCH_TICKS
                            && distance(
                                Vec2::new(predicted.x, predicted.z),
                                Vec2::new(path.x, path.z),
                            ) <= MATCH_DISTANCE
                            && off_heading <= MATCH_HEADING)
                            .then_some((shell, off_heading))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(shell, _)| shell)
                else {
                    return false;
                };
                let check = path.tick + 6.0;
                let (predicted, host) = (shell.legs[0].path.at(check), path.at(check));
                self.stats
                    .error
                    .add((predicted.x - host.x).hypot(predicted.z - host.z));
                shell.server_id = Some(path.id);
                shell.handover = Some((shell.legs[0].path, None));
                shell.legs = vec![Leg {
                    path,
                    until: f64::INFINITY,
                }];
                shell.forecast = forecast(&path, shell.bounces, cover);
                true
            }
            PathEntry::Change(path) => {
                let Some(shell) = self.shell_mut(path.id) else {
                    return false;
                };
                if let Some(last) = shell.legs.last_mut() {
                    last.until = path.tick;
                }
                shell.legs.push(Leg {
                    path,
                    until: f64::INFINITY,
                });
                shell.bounces = shell.bounces.saturating_sub(1);
                shell.forecast = forecast(&path, shell.bounces, cover);
                true
            }
            PathEntry::End { id, tick } => {
                let Some(shell) = self.shell_mut(id) else {
                    return false;
                };
                if let Some(last) = shell.legs.last_mut() {
                    last.until = tick;
                }
                shell.ended = true;
                shell.forecast = Forecast::default();
                true
            }
        }
    }

    fn shell_mut(&mut self, server_id: u32) -> Option<&mut OwnShell> {
        self.shells
            .iter_mut()
            .find(|shell| shell.server_id == Some(server_id))
    }

    /// Whether `timed` is the host's muzzle flash of a predicted shot or an impact of an
    /// own shell, which then plays it on its clock.
    fn take_event(&mut self, timed: &TimedEvent, tank_id: u32) -> bool {
        let event = &timed.event;
        match event.kind {
            SimEventType::Shot if event.id == Some(tank_id) => {
                let fired = self.fired.iter_mut().find(|fired| {
                    !fired.confirmed && (fired.tick as f64 - timed.tick).abs() <= MATCH_TICKS
                });
                match fired {
                    Some(fired) => {
                        fired.confirmed = true;
                        self.stats.confirmed += 1;
                        true
                    }
                    None => {
                        self.stats.unpredicted += 1;
                        false
                    }
                }
            }
            SimEventType::Impact
            | SimEventType::Explosion
            | SimEventType::Ricochet
            | SimEventType::Shatter => {
                let at = Vec2::new(event.x, event.z);
                let near = |other: &TimedEvent| {
                    other.event.kind == event.kind
                        && (other.tick - timed.tick).abs() <= IMPACT_TICKS
                        && distance(Vec2::new(other.event.x, other.event.z), at) <= IMPACT_DISTANCE
                };
                // Already shown where the prediction foresaw it.
                if self.shells.iter().any(|shell| shell.shown.iter().any(near)) {
                    return true;
                }
                let Some(shell) = self.shells.iter_mut().find(|shell| {
                    shell.server_id.is_some()
                        && shell.turns().any(|(tick, point)| {
                            (tick - timed.tick).abs() <= IMPACT_TICKS
                                && distance(point, at) <= IMPACT_DISTANCE
                        })
                }) else {
                    return false;
                };
                shell.events.push(timed.clone());
                true
            }
            _ => false,
        }
    }

    /// Draws the own shells into `shots` and returns the impact effects due, for the
    /// predicted present `present`, the display tick `display` and the host's newest tick
    /// `newest` (all fractional ticks). Drops shells that ended and predicted ones the
    /// host never launched.
    pub fn draw(
        &mut self,
        present: f64,
        display: f64,
        newest: f64,
        shots: &mut Vec<RenderShot>,
    ) -> Vec<SimEvent> {
        let mut due = Vec::new();
        let unconfirmed = &mut self.stats.unconfirmed_shells;
        self.shells.retain_mut(|shell| {
            let tick = own_clock(present, display, shell.launch_tick());
            if let Some(mut drawn) = shell.at(tick, newest) {
                // Off the predicted path onto the host's, while still on its first leg.
                if let Some((predicted, from)) = shell.handover.as_mut() {
                    let from = *from.get_or_insert(tick);
                    let left = (-(tick - from) * STEP * HANDOVER_RATE).exp();
                    if left < HANDOVER_DONE || shell.legs.len() > 1 {
                        shell.handover = None;
                    } else {
                        let (was, host) = (predicted.at(tick), shell.legs[0].path.at(tick));
                        drawn.x += (was.x - host.x) * left;
                        drawn.z += (was.z - host.z) * left;
                    }
                }
                shots.push(drawn);
            }
            // A strike the host has not reported yet shows when the shell reaches it.
            for strike in &shell.forecast.strikes {
                let foreseen = strike.tick <= tick
                    && strike.tick > newest
                    && !shell.shown.iter().any(|shown| {
                        (shown.tick - strike.tick).abs() <= IMPACT_TICKS
                            && distance(
                                Vec2::new(shown.event.x, shown.event.z),
                                Vec2::new(strike.event.x, strike.event.z),
                            ) <= IMPACT_DISTANCE
                    });
                if foreseen {
                    due.push(strike.event.clone());
                    shell.shown.push(strike.clone());
                }
            }
            shell.events.retain(|timed| {
                if timed.tick <= tick {
                    due.push(timed.event.clone());
                    false
                } else {
                    true
                }
            });
            let end = shell.legs.last().map_or(0.0, |leg| leg.until);
            // A finished shell stays a moment for the host's effects of strikes it showed.
            let finished = shell.ended
                && tick >= end
                && shell.events.is_empty()
                && shell
                    .shown
                    .iter()
                    .all(|shown| newest > shown.tick + IMPACT_TICKS);
            let expired = tick >= shell.launch_tick() + COMBAT.projectile_lifetime / STEP
                && (shell.ended || shell.server_id.is_none());
            let never_fired = shell.server_id.is_none()
                && newest >= (shell.fired_tick + UNCONFIRMED_TICKS) as f64;
            if never_fired {
                *unconfirmed += 1;
            }
            !(finished || expired || never_fired)
        });
        due
    }

    /// One frame of recoil recovery, as the host's tick runs it down.
    pub fn recover(&mut self, seconds: f64) {
        self.recoil = 0f64.max(self.recoil - seconds * SIMULATION_RULES.recoil_recovery_per_second);
    }
}

/// The tick an own shell launched at `launch` is drawn at: the predicted present when
/// fired, easing into the display tick over [`BLEND_VIEW_DELAYS`] view delays of flight at
/// a constant four fifths of real time.
pub fn own_clock(present: f64, display: f64, launch: f64) -> f64 {
    let delay = (present - display).max(0.0);
    let blend = BLEND_VIEW_DELAYS * delay;
    if blend + delay <= 0.0 || present - launch >= blend + delay {
        return present - delay;
    }
    (present * blend + delay * launch) / (blend + delay)
}
