//! How a room feels to the viewer, read where the client draws it: how long a click takes
//! to show its own muzzle flash, how far behind the viewer's predicted tank the other
//! tanks are drawn, and how far from the server's hit a target the viewer hit was drawn
//! when the viewer's own present reached that hit. Stats for nerds shows them and the
//! latency matrix (`scripts/multiplayer-latency-matrix.mjs`) records them; they never
//! change what is drawn or sent.

use std::collections::VecDeque;

use super::fixed_step_clock::SIMULATION_STEP_MS;
use super::network_timeline::hull_gap;
use crate::sim::math::{Vec2, distance};
use crate::sim::render_state::RenderState;
use crate::sim::shooter_rewind::{MAX_REWIND_TICKS, target_ticks};
use crate::sim::types::{Team, VehicleKind};

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
/// A shell that ended within this distance (metres) of a hull's hit box, where the host
/// had the hull over the shell's rewind, struck it: wire rounding and a hull's sweep within
/// a tick. Farther off, it hurt the hull with a rocket's blast elsewhere.
const STRIKE_REACH: f64 = 0.3;
/// Received ticks of hulls kept: the longest rewind, and the tick a hit sweeps through.
const HULL_HISTORY_TICKS: usize = MAX_REWIND_TICKS as usize + 2;

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

/// An enemy shell that ended near the viewer about when a hit on it arrived.
#[derive(Clone, Copy, Debug)]
pub struct ShellEnd {
    pub shell: u32,
    /// Where it was fired, and the ticks it rewinds the tanks it meets.
    pub origin: Vec2,
    pub rewind: u32,
    /// Where its path ended.
    pub at: Vec2,
}

/// Each live tank's hull in one received tick: id, kind, position and heading.
#[derive(Clone, Debug, Default)]
struct HostHulls {
    tick: f64,
    hulls: Vec<(u32, VehicleKind, Vec2, f64)>,
}

/// One drawn frame: the viewer's predicted present, where its hull and each other live
/// tank were drawn, and where the enemy shells were (their impacts, in the frame that
/// draws their end).
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
    /// metres. Zero when the server judges shells where the shooter saw the target. A
    /// blast that only reached the tank (a rocket's beside it, a drum's, a mine's) is none.
    pub hit_offset: Samples,
    /// Per enemy shell that hit the viewer: how close it was drawn to the viewer's hull (its
    /// hit box) around the moment the viewer's present reached the hit, metres: the hull as
    /// drawn then, or for a player's shell the hull as drawn its shooter's rewind earlier,
    /// where the host judged it. A shell is where its impact shows in the frame that draws
    /// its strike; one drawn on the display clock is still a view delay of flight away. A
    /// rocket's blast beside the hull is no shell reaching it.
    pub incoming_offset: Samples,
    /// Per player's shell judged against the viewer's past hull (shooter first): how far it
    /// was drawn from the viewer's hull as drawn when it struck, metres. It strikes where
    /// the shooter saw the viewer, so this is how far the viewer had moved since.
    pub incoming_behind: Samples,
    /// Enemy shells that hit the viewer without being drawn by then: fired so close that
    /// their whole flight was shorter than the view delay.
    pub unseen_incoming: u64,
    /// Presses that got no shot within [`MAX_FIRE_WAIT_MS`].
    pub unanswered_presses: u64,
    press: Option<Press>,
    firing: bool,
    frames: VecDeque<DrawnFrame>,
    spare: Vec<DrawnFrame>,
    /// The hulls of the newest received ticks, oldest first.
    host: VecDeque<HostHulls>,
}

impl FeelReadings {
    /// Forgets the frames and shots of a superseded display (a baseline or a new round);
    /// the samples stay for the page session.
    pub fn restart(&mut self) {
        self.press = None;
        self.firing = false;
        self.spare.extend(self.frames.drain(..));
        self.host.clear();
    }

    /// A received tick: where the host had each live hull, which tells a shell's strike on
    /// a hull from a blast beside it.
    pub fn received(&mut self, tick: f64, state: &RenderState) {
        let mut entry = if self.host.len() >= HULL_HISTORY_TICKS {
            self.host.pop_front().expect("checked above")
        } else {
            HostHulls::default()
        };
        entry.tick = tick;
        entry.hulls.clear();
        entry.hulls.extend(
            state
                .tanks
                .iter()
                .filter(|tank| tank.alive)
                .map(|tank| (tank.id, tank.kind, tank.position.planar(), tank.heading)),
        );
        self.host.push_back(entry);
    }

    /// How near `point` came to tank `id`'s hit box where the host had it over the `rewind`
    /// ticks before `tick` and the tick it swept through, metres.
    fn host_gap(&self, id: u32, point: Vec2, tick: f64, rewind: u32) -> Option<f64> {
        self.host
            .iter()
            .filter(|received| {
                received.tick <= tick && received.tick >= tick - f64::from(rewind) - 1.0
            })
            .filter_map(|received| {
                let &(_, kind, position, heading) =
                    received.hulls.iter().find(|hull| hull.0 == id)?;
                Some(hull_gap(kind, position, heading, point))
            })
            .min_by(f64::total_cmp)
    }

    /// Of `points` near tank `id` about when it was hurt at `tick`, the one that struck its
    /// hull where the host had it.
    fn strike<T: Copy>(
        &self,
        id: u32,
        tick: f64,
        points: &[T],
        place: impl Fn(&T) -> (Vec2, u32),
    ) -> Option<T> {
        points
            .iter()
            .filter_map(|candidate| {
                let (point, rewind) = place(candidate);
                Some((*candidate, self.host_gap(id, point, tick, rewind)?))
            })
            .filter(|(_, gap)| *gap <= STRIKE_REACH)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(candidate, _)| candidate)
    }

    /// One drawn frame. `present` is the viewer's predicted tick while prediction runs,
    /// `display` the tick the other tanks are drawn at, `firing` whether the viewer holds
    /// fire with live controls and `ready` whether the gun will have reloaded by the time
    /// a shot sent now reaches the server. `ended` holds the shells whose impact the frame
    /// draws instead of them (id, team, where).
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        now_ms: f64,
        present: Option<f64>,
        display: f64,
        firing: bool,
        ready: bool,
        state: &RenderState,
        ended: &[(u32, Team, Vec2)],
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
        let enemy = |team: Team| viewer.is_some_and(|viewer| team != viewer.team);
        frame.shells.extend(
            state
                .shots
                .iter()
                .filter(|shot| enemy(shot.team))
                .map(|shot| (shot.id, Vec2::new(shot.x, shot.z))),
        );
        frame.shells.extend(
            ended
                .iter()
                .filter(|(_, team, _)| enemy(*team))
                .map(|&(id, _, at)| (id, at)),
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

    /// An enemy shell hurt the viewer `viewer` at `tick`; `ends` are the shells that ended
    /// near it then. A rocket's blast beside the hull is no shell drawn reaching it.
    pub fn incoming_hit(&mut self, tick: f64, viewer: u32, ends: &[ShellEnd]) {
        let Some(ShellEnd {
            shell,
            origin,
            rewind,
            ..
        }) = self.strike(viewer, tick, ends, |end| (end.at, end.rewind))
        else {
            return;
        };
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
                let back = target_ticks(rewind, distance(origin, position));
                let (judged, judged_heading) = self
                    .viewer_at(frame.present - f64::from(back))
                    .unwrap_or((position, heading));
                Some((
                    hull_gap(kind, judged, judged_heading, *at),
                    hull_gap(kind, position, heading, *at),
                    back,
                ))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        match nearest {
            Some((judged, now, back)) => {
                self.incoming_offset.add(judged);
                if back > 0 {
                    self.incoming_behind.add(now);
                }
            }
            None => self.unseen_incoming += 1,
        }
    }

    /// Where the viewer's hull was drawn in the frame nearest the predicted tick `present`,
    /// if one came within a tick of it.
    fn viewer_at(&self, present: f64) -> Option<(Vec2, f64)> {
        self.frames
            .iter()
            .filter(|frame| (frame.present - present).abs() <= 1.0)
            .min_by(|a, b| {
                (a.present - present)
                    .abs()
                    .total_cmp(&(b.present - present).abs())
            })
            .and_then(|frame| frame.viewer)
            .map(|(_, position, heading)| (position, heading))
    }

    /// The viewer's shell hurt or killed tank `victim` at `tick`; `impacts` are the shell
    /// impacts near it then. A rocket's blast beside the hull struck nothing there.
    pub fn own_hit(&mut self, tick: f64, victim: u32, impacts: &[Vec2]) {
        let Some(impact) = self.strike(victim, tick, impacts, |&at| (at, MAX_REWIND_TICKS)) else {
            return;
        };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::hitboxes::SHELL_HIT_RADIUS;
    use crate::sim::math::Point3;
    use crate::sim::render_state::{RenderShot, RenderTank};
    use crate::sim::tank_dimensions::tank_hull;

    const VIEWER: u32 = 1;
    const ENEMY: u32 = 2;
    const SHELL: u32 = 50;
    const ROCKET: u32 = 51;
    /// Ticks the other tanks are drawn behind the present.
    const VIEW_TICKS: f64 = 8.0;

    fn tank(id: u32, team: Team, at: Vec2) -> RenderTank {
        RenderTank {
            id,
            kind: VehicleKind::Balanced,
            team,
            alive: true,
            position: Point3::new(at.x, 0.65, at.z),
            ..RenderTank::default()
        }
    }

    /// The viewer at `viewer` and an enemy at `enemy`, both facing +z, and enemy shells.
    fn scene(viewer: Vec2, enemy: Vec2, shells: &[(u32, Vec2)]) -> RenderState {
        RenderState {
            viewer_id: VIEWER,
            tanks: vec![
                tank(VIEWER, Team::Blue, viewer),
                tank(ENEMY, Team::Red, enemy),
            ],
            shots: shells
                .iter()
                .map(|&(id, at)| RenderShot {
                    id,
                    x: at.x,
                    z: at.z,
                    team: Team::Red,
                    ..RenderShot::default()
                })
                .collect(),
            ..RenderState::default()
        }
    }

    /// How far ahead of a hull's centre its hit box's front face lies.
    fn front() -> f64 {
        let hull = tank_hull(VehicleKind::Balanced);
        hull.center.z + hull.size.z / 2.0 + SHELL_HIT_RADIUS
    }

    /// A point three metres off the side of a hull at `at`, where a rocket struck a wall.
    fn beside(at: Vec2) -> Vec2 {
        Vec2::new(
            at.x + tank_hull(VehicleKind::Balanced).size.x / 2.0 + 3.0,
            at.z,
        )
    }

    #[test]
    fn the_viewers_hit_is_measured_at_a_shell_impact_on_the_hull_the_host_judged() {
        let mut feel = FeelReadings::default();
        // The enemy drives along +x at 0.2 m a tick; the host had it where the viewer's
        // shell struck, ten ticks back for a player's shell.
        let enemy = |tick: f64| Vec2::new(0.2 * tick - 20.0, 10.0);
        for tick in 80..=100 {
            let tick = f64::from(tick);
            feel.received(tick, &scene(Vec2::ZERO, enemy(tick), &[]));
            let drawn = scene(Vec2::ZERO, enemy(tick - VIEW_TICKS), &[]);
            feel.frame(
                0.0,
                Some(tick),
                tick - VIEW_TICKS,
                false,
                false,
                &drawn,
                &[],
            );
        }
        // A rocket's blast at the wall beside it hurt it, but struck nothing there.
        feel.own_hit(95.0, ENEMY, &[beside(enemy(95.0))]);
        assert_eq!(feel.hit_offset.count, 0, "{feel:?}");
        // A shell that struck its hull where the shooter saw it, beside another impact.
        feel.own_hit(96.0, ENEMY, &[beside(enemy(96.0)), enemy(96.0 - 10.0)]);
        assert_eq!(feel.hit_offset.count, 1, "{feel:?}");
        assert!(feel.hit_offset.largest < 0.5, "{feel:?}");
    }

    #[test]
    fn an_incoming_shell_is_measured_where_its_strike_is_drawn_and_a_blast_beside_is_none() {
        let mut feel = FeelReadings::default();
        let enemy = Vec2::new(0.0, 40.0);
        let hit = 100.0;
        let strike = Vec2::new(0.0, front());
        // Closing in fast, the shell is drawn two metres short of the hull in the last frame
        // before its strike; the strike's frame draws its impact on the hull instead.
        for tick in 90..=100 {
            let tick = f64::from(tick);
            let shells = [(SHELL, Vec2::new(0.0, front() + 2.0 * (hit - tick)))];
            let drawn = if tick < hit { &shells[..] } else { &[] };
            let ended = if tick < hit {
                Vec::new()
            } else {
                vec![(SHELL, Team::Red, strike)]
            };
            let state = scene(Vec2::ZERO, enemy, drawn);
            feel.received(tick, &state);
            feel.frame(
                0.0,
                Some(tick),
                tick - VIEW_TICKS,
                false,
                false,
                &state,
                &ended,
            );
        }
        let shell = ShellEnd {
            shell: SHELL,
            origin: enemy,
            rewind: 0,
            at: strike,
        };
        let rocket = ShellEnd {
            shell: ROCKET,
            at: beside(Vec2::ZERO),
            ..shell
        };
        // A rocket's blast at the wall beside the hull is no shell drawn reaching it.
        feel.incoming_hit(hit, VIEWER, &[rocket]);
        assert_eq!(feel.incoming_offset.count, 0, "{feel:?}");
        assert_eq!(feel.unseen_incoming, 0, "{feel:?}");
        // The shell that struck is the one whose end met the hull, nearest or not.
        feel.incoming_hit(hit, VIEWER, &[rocket, shell]);
        assert_eq!(feel.incoming_offset.count, 1, "{feel:?}");
        assert!(feel.incoming_offset.largest < 1e-9, "{feel:?}");
    }
}
