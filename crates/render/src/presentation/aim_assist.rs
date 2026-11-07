//! Touch aim assist: the turret's aim while the fire stick (the shell's `touch.rs`)
//! has it, read from the displayed scene like the rest of presentation. A thumb cannot
//! place a turret the way a mouse does, so the stick asks for one of two aims:
//!
//! - [`AimMode::Track`]: the turret follows the best target, the nearest enemy on
//!   screen that a shell can reach, and holds it until another is clearly nearer, so a
//!   tap fires where the reticle already shows.
//! - [`AimMode::Manual`]: the stick's direction on screen, pulled toward an enemy that
//!   lies close to that line and snapped onto it within a tank's width, so a drag picks
//!   any target and still lands on it.
//!
//! Both lead a moving target only partly, so a tank that keeps changing course still
//! dodges. The assist only chooses the command's aim angle; the simulation has no
//! idea which control produced it.

use glam::{DQuat, DVec3, Vec2, Vec3};
use sloppy_core::sim::RenderState;
use sloppy_core::sim::data::weapon;
use sloppy_core::sim::math::angle_delta;
use sloppy_core::sim::render_state::{RenderCover, RenderTank};
use sloppy_core::sim::tank_dimensions::tank_hull;

use super::view_settings::AIM_PLANE_HEIGHT;
use crate::camera::PerspectiveCamera;

/// The share of a target's travel during the shell's flight that the aim leads by: a
/// good bot leads by 0.65 (`ai.rs`), and a full lead would make every straight run a hit.
const LEAD: f64 = 0.5;
/// A nearer enemy takes over the tracked target only when the tracked one is this many
/// times further away, so the reticle does not flick between two tanks at similar range.
const KEEP_TARGET: f64 = 1.4;
/// The drag's pull: an enemy whose lane passes within `PULL_REACH` metres of the stick's
/// line pulls the aim toward it, fully within `SNAP_REACH` (about a hull's half width
/// plus a shell's slack), and no further off the line than `PULL_ANGLE` radians, so a
/// tank right beside the player does not swallow every direction.
const SNAP_REACH: f64 = 1.6;
const PULL_REACH: f64 = 4.5;
const PULL_ANGLE: f64 = 0.45;
/// Where a drag that pulls toward no enemy puts the reticle along its line, metres.
const MANUAL_RANGE: f64 = 16.0;
/// Covers lower than the sight line (`Simulation::visible`) let a shell over them; the
/// lane check also ignores a graze this close to a cover's side, metres.
const SIGHT_HEIGHT: f64 = 1.0;
const GRAZE: f64 = 0.15;
/// How far past the screen's edge, in NDC, an enemy still counts as on screen: a tank
/// half out of view is still a target.
const SCREEN_MARGIN: f32 = 0.05;
/// The step along the stick's direction in NDC that turns it into a ground direction.
const STICK_PROBE: f32 = 0.05;

/// What aims the turret this frame (the input frame's `AIM_MODE`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AimMode {
    /// The mouse, or a finger on the arena: the turret aims at the pointer.
    #[default]
    Pointer,
    /// The fire stick at rest or tapped: the tracked target.
    Track,
    /// The fire stick dragged: its direction, pulled toward an enemy near its line.
    Manual,
}

impl AimMode {
    pub fn from_code(code: f32) -> Self {
        match code.round() as i32 {
            1 => Self::Track,
            2 => Self::Manual,
            _ => Self::Pointer,
        }
    }

    pub fn code(self) -> f32 {
        match self {
            Self::Pointer => 0.0,
            Self::Track => 1.0,
            Self::Manual => 2.0,
        }
    }
}

/// The assisted aim: the turret's angle, and where the reticle goes, if anywhere.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Assisted {
    pub angle: f64,
    /// The reticle's ground point: on the target, or along a drag's line. `None` hides
    /// it (tracking with no target in reach).
    pub reticle: Option<(f64, f64)>,
    /// The enemy the aim is on, if any.
    pub target: Option<u32>,
}

/// The tracked target, kept between frames.
#[derive(Clone, Debug, Default)]
pub struct AimAssist {
    target: Option<u32>,
    /// This frame's [`candidates`], kept so touch aim does not allocate every frame.
    candidates: Vec<Candidate>,
}

/// An enemy the assist may aim at: its id, where the shell meets it (led), and its
/// distance from the viewer.
#[derive(Clone, Copy, Debug)]
struct Candidate {
    id: u32,
    aim: (f64, f64),
    distance: f64,
}

impl AimAssist {
    /// Forget the tracked target (a new round, a respawn, leaving touch play).
    pub fn clear(&mut self) {
        self.target = None;
    }

    /// The aim for `mode` from the viewer of `state`, seen through the overhead
    /// `camera`; `stick` is the fire stick's direction on screen (x right, y down).
    /// `None` for [`AimMode::Pointer`] or without a live viewer.
    pub fn aim(
        &mut self,
        state: &RenderState,
        camera: &PerspectiveCamera,
        mode: AimMode,
        stick: (f64, f64),
    ) -> Option<Assisted> {
        let viewer = state.viewer().filter(|viewer| viewer.alive)?;
        // The mouse aims without the assist: skip the scan of every enemy's lane.
        if mode == AimMode::Pointer {
            return None;
        }
        self.candidates.clear();
        self.candidates.extend(candidates(state, viewer, camera));
        let from = (viewer.position.x, viewer.position.z);
        match mode {
            AimMode::Pointer => None,
            AimMode::Track => {
                let target = self.track(&self.candidates);
                self.target = target.map(|candidate| candidate.id);
                Some(match target {
                    Some(target) => Assisted {
                        angle: bearing(from, target.aim),
                        reticle: Some(target.aim),
                        target: Some(target.id),
                    },
                    None => Assisted {
                        angle: viewer.aim,
                        reticle: None,
                        target: None,
                    },
                })
            }
            AimMode::Manual => {
                let line = stick_bearing(camera, from, stick).unwrap_or(viewer.aim);
                let pulled = pull(line, from, &self.candidates);
                // A drag onto an enemy makes it the one a later tap fires at.
                if let Some((target, _)) = pulled {
                    self.target = Some(target.id);
                }
                let (angle, range, target) = match pulled {
                    Some((target, share)) => (
                        line + angle_delta(line, bearing(from, target.aim)) * share,
                        target.distance,
                        Some(target.id),
                    ),
                    None => (line, MANUAL_RANGE, None),
                };
                Some(Assisted {
                    angle,
                    reticle: Some((from.0 + angle.sin() * range, from.1 + angle.cos() * range)),
                    target,
                })
            }
        }
    }

    /// The nearest candidate, unless the one already tracked is not much further.
    fn track(&self, candidates: &[Candidate]) -> Option<Candidate> {
        let nearest = candidates
            .iter()
            .min_by(|a, b| a.distance.total_cmp(&b.distance))
            .copied()?;
        let kept = self
            .target
            .and_then(|id| candidates.iter().find(|candidate| candidate.id == id))
            .filter(|kept| kept.distance <= nearest.distance * KEEP_TARGET);
        Some(kept.copied().unwrap_or(nearest))
    }
}

/// The enemies a shell can reach from `viewer`: alive, past their spawn protection, on
/// screen and with no cover in the lane.
fn candidates<'a>(
    state: &'a RenderState,
    viewer: &'a RenderTank,
    camera: &'a PerspectiveCamera,
) -> impl Iterator<Item = Candidate> + 'a {
    let from = (viewer.position.x, viewer.position.z);
    let shell_speed = weapon(viewer.selected_ammo).speed;
    state
        .tanks
        .iter()
        .filter(move |tank| {
            tank.alive && tank.id != viewer.id && tank.team != viewer.team && tank.protection <= 0.0
        })
        .filter(move |tank| on_screen(camera, (tank.position.x, tank.position.z)))
        .filter_map(move |tank| {
            let at = (tank.position.x, tank.position.z);
            let distance = (at.0 - from.0).hypot(at.1 - from.1);
            let flight = distance / shell_speed * LEAD;
            let aim = (
                at.0 + tank.velocity.x * flight,
                at.1 + tank.velocity.z * flight,
            );
            let hulls_clear = state
                .tanks
                .iter()
                .filter(|other| other.alive && other.id != viewer.id && other.id != tank.id)
                .all(|other| !hull_in_lane(other, from, at));
            (hulls_clear && lane_clear(&state.covers, from, at)).then_some(Candidate {
                id: tank.id,
                aim,
                distance,
            })
        })
}

/// The candidate whose lane passes nearest the line at `line` radians from `from`, and
/// how far (0 to 1) it pulls the aim toward itself.
fn pull(line: f64, from: (f64, f64), candidates: &[Candidate]) -> Option<(Candidate, f64)> {
    candidates
        .iter()
        .filter_map(|candidate| {
            let off = angle_delta(line, bearing(from, candidate.aim));
            if off.abs() > PULL_ANGLE {
                return None;
            }
            let miss = candidate.distance * off.sin().abs();
            let share = ((PULL_REACH - miss) / (PULL_REACH - SNAP_REACH)).clamp(0.0, 1.0);
            (share > 0.0).then_some((*candidate, share, miss))
        })
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(candidate, share, _)| (candidate, share))
}

/// The game's bearing from `from` to `to`: 0 along +z, turning toward +x.
fn bearing(from: (f64, f64), to: (f64, f64)) -> f64 {
    (to.0 - from.0).atan2(to.1 - from.1)
}

fn ndc(camera: &PerspectiveCamera, (x, z): (f64, f64)) -> Vec3 {
    camera.project(Vec3::new(x as f32, AIM_PLANE_HEIGHT as f32, z as f32))
}

fn on_screen(camera: &PerspectiveCamera, at: (f64, f64)) -> bool {
    let point = ndc(camera, at);
    let reach = 1.0 + SCREEN_MARGIN;
    point.x.abs() <= reach && point.y.abs() <= reach && (0.0..=1.0).contains(&point.z)
}

/// The ground bearing the stick's screen direction points along from the tank at
/// `from`. A screen vector of (x, y) pixels is (x, -y × aspect) in NDC's proportions.
fn stick_bearing(camera: &PerspectiveCamera, from: (f64, f64), (x, y): (f64, f64)) -> Option<f64> {
    let direction = Vec2::new(x as f32, -(y as f32) * camera.aspect).try_normalize()?;
    let start = ndc(camera, from);
    let probe = Vec2::new(start.x, start.y) + direction * STICK_PROBE;
    let ground = camera.pick_ground(probe, AIM_PLANE_HEIGHT as f32)?;
    let to = (f64::from(ground.x), f64::from(ground.z));
    ((to.0 - from.0).hypot(to.1 - from.1) > 1e-6).then(|| bearing(from, to))
}

/// Whether a shell's lane from `from` to `to` passes every standing cover taller than
/// the sight line, each a box of its footprint turned by its rotation.
fn lane_clear(covers: &[RenderCover], from: (f64, f64), to: (f64, f64)) -> bool {
    covers.iter().all(|cover| {
        if !cover.alive || cover.position.y + cover.h / 2.0 < SIGHT_HEIGHT {
            return true;
        }
        let rotation = DQuat::from_xyzw(
            cover.rotation.x,
            cover.rotation.y,
            cover.rotation.z,
            cover.rotation.w,
        )
        .inverse();
        let local = |(x, z): (f64, f64)| {
            let point = rotation * DVec3::new(x - cover.position.x, 0.0, z - cover.position.z);
            (point.x, point.z)
        };
        let half = (
            (cover.w / 2.0 - GRAZE).max(0.0),
            (cover.d / 2.0 - GRAZE).max(0.0),
        );
        !segment_meets_box(local(from), local(to), half)
    })
}

/// Whether `tank`'s hull stands in a shell's lane from `from` to `to`: an ally stops a
/// shell as a wall does (`projectiles.rs`), so a target behind one is out of reach.
fn hull_in_lane(tank: &RenderTank, from: (f64, f64), to: (f64, f64)) -> bool {
    let hull = tank_hull(tank.kind);
    let (sin, cos) = tank.heading.sin_cos();
    let local = |(x, z): (f64, f64)| {
        let (dx, dz) = (x - tank.position.x, z - tank.position.z);
        (
            dx * cos - dz * sin - hull.center.x,
            dx * sin + dz * cos - hull.center.z,
        )
    };
    let half = (
        (hull.size.x / 2.0 - GRAZE).max(0.0),
        (hull.size.z / 2.0 - GRAZE).max(0.0),
    );
    segment_meets_box(local(from), local(to), half)
}

/// Whether the segment from `a` to `b` touches the box of half extents `half` about the
/// origin (the slab test).
fn segment_meets_box(a: (f64, f64), b: (f64, f64), half: (f64, f64)) -> bool {
    let mut enter: f64 = 0.0;
    let mut exit: f64 = 1.0;
    for (start, end, extent) in [(a.0, b.0, half.0), (a.1, b.1, half.1)] {
        let delta = end - start;
        if delta.abs() < 1e-12 {
            if start.abs() > extent {
                return false;
            }
            continue;
        }
        let (t0, t1) = ((-extent - start) / delta, (extent - start) / delta);
        enter = enter.max(t0.min(t1));
        exit = exit.min(t0.max(t1));
        if enter > exit {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::sim::math::{Point3, Quat4};
    use sloppy_core::sim::{CoverKind, Team};

    fn tank(id: u32, team: Team, x: f64, z: f64) -> RenderTank {
        RenderTank {
            id,
            team,
            alive: true,
            position: Point3 { x, y: 0.65, z },
            ..RenderTank::default()
        }
    }

    fn wall(x: f64, z: f64, w: f64, d: f64) -> RenderCover {
        RenderCover {
            kind: CoverKind::Concrete,
            alive: true,
            w,
            h: 2.0,
            d,
            position: Point3 { x, y: 1.0, z },
            rotation: Quat4::yaw(0.0),
            ..RenderCover::default()
        }
    }

    /// The viewer at the origin, its enemies on Red, under a camera high over it that
    /// sees the whole test field.
    fn scene(
        enemies: &[(u32, f64, f64)],
        covers: Vec<RenderCover>,
    ) -> (RenderState, PerspectiveCamera) {
        let mut tanks = vec![tank(1, Team::Blue, 0.0, 0.0)];
        tanks.extend(enemies.iter().map(|&(id, x, z)| tank(id, Team::Red, x, z)));
        let state = RenderState {
            viewer_id: 1,
            tanks,
            covers,
            ..RenderState::default()
        };
        let mut camera = PerspectiveCamera::new(50.0, 0.1, 500.0);
        camera.aspect = 1.5;
        // Overhead and a little behind (toward -z), as the game's camera sits.
        camera.look_at(Vec3::new(0.0, 80.0, -40.0), Vec3::ZERO);
        (state, camera)
    }

    #[test]
    fn tracking_aims_at_the_nearest_enemy_and_holds_it() {
        let (mut state, camera) = scene(&[(2, 0.0, 20.0), (3, 12.0, 0.0)], Vec::new());
        let mut assist = AimAssist::default();
        let aim = assist
            .aim(&state, &camera, AimMode::Track, (0.0, 0.0))
            .unwrap();
        assert_eq!(aim.target, Some(3));
        assert!((aim.angle - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        // Slightly further than the other now: still held.
        state.tanks[2].position.x = 22.0;
        let aim = assist
            .aim(&state, &camera, AimMode::Track, (0.0, 0.0))
            .unwrap();
        assert_eq!(aim.target, Some(3));
        // Clearly further: the nearer one takes over.
        state.tanks[2].position.x = 35.0;
        let aim = assist
            .aim(&state, &camera, AimMode::Track, (0.0, 0.0))
            .unwrap();
        assert_eq!(aim.target, Some(2));
    }

    #[test]
    fn tracking_skips_allies_wrecks_protected_tanks_and_walled_lanes() {
        let (mut state, camera) = scene(
            &[
                (2, 0.0, 8.0),
                (3, 9.0, 0.0),
                (4, -10.0, 0.0),
                (5, 0.0, -11.0),
            ],
            vec![wall(5.0, 0.0, 1.0, 4.0)],
        );
        state.tanks[1].team = Team::Blue;
        state.tanks[3].alive = false;
        state.tanks[4].protection = 2.0;
        let mut assist = AimAssist::default();
        let aim = assist
            .aim(&state, &camera, AimMode::Track, (0.0, 0.0))
            .unwrap();
        assert_eq!(
            aim.target, None,
            "the wall hides 3; the rest are not targets"
        );
        assert_eq!(aim.reticle, None);
        // A wall too low for the sight line hides nothing.
        state.covers[0].h = 0.6;
        state.covers[0].position.y = 0.3;
        let aim = assist
            .aim(&state, &camera, AimMode::Track, (0.0, 0.0))
            .unwrap();
        assert_eq!(aim.target, Some(3));
    }

    #[test]
    fn a_tank_in_the_lane_hides_the_enemy_behind_it() {
        // An ally halfway to the nearer enemy: the open lane to the further one wins.
        let (mut state, camera) = scene(&[(2, 0.0, 12.0), (3, 20.0, 0.0)], Vec::new());
        state.tanks.push(tank(4, Team::Blue, 0.0, 6.0));
        let mut assist = AimAssist::default();
        let aim = assist
            .aim(&state, &camera, AimMode::Track, (0.0, 0.0))
            .unwrap();
        assert_eq!(aim.target, Some(3));
        // Out of the lane, it hides nothing.
        state.tanks[3].position.x = 5.0;
        let aim = assist
            .aim(&state, &camera, AimMode::Track, (0.0, 0.0))
            .unwrap();
        assert_eq!(aim.target, Some(2));
    }

    #[test]
    fn tracking_leads_a_moving_target_partly() {
        let (mut state, camera) = scene(&[(2, 0.0, 20.0)], Vec::new());
        state.tanks[1].velocity = Point3 {
            x: 6.0,
            y: 0.0,
            z: 0.0,
        };
        let mut assist = AimAssist::default();
        let aim = assist
            .aim(&state, &camera, AimMode::Track, (0.0, 0.0))
            .unwrap();
        let flight = 20.0 / weapon(state.tanks[0].selected_ammo).speed;
        let (x, _) = aim.reticle.unwrap();
        assert!((x - 6.0 * flight * LEAD).abs() < 1e-9, "{x}");
    }

    #[test]
    fn a_drag_points_along_the_screen_and_snaps_onto_a_nearby_enemy() {
        let (state, camera) = scene(&[(2, 3.0, 25.0)], Vec::new());
        let mut assist = AimAssist::default();
        // Screen right is the world's -x for a camera looking toward +z.
        let right = assist
            .aim(&state, &camera, AimMode::Manual, (1.0, 0.0))
            .unwrap();
        assert!(
            (right.angle + std::f64::consts::FRAC_PI_2).abs() < 1e-3,
            "{}",
            right.angle
        );
        assert_eq!(right.target, None);
        // Screen up is the world's +z; the enemy 3 m off that line is pulled in fully.
        let up = assist
            .aim(&state, &camera, AimMode::Manual, (0.0, -1.0))
            .unwrap();
        assert_eq!(up.target, Some(2));
        assert!((up.angle - 3.0_f64.atan2(25.0)).abs() < 0.1);
        let share = (PULL_REACH - 25.0 * (3.0_f64.atan2(25.0)).sin()) / (PULL_REACH - SNAP_REACH);
        assert!(share < 1.0 && share > 0.0);
        // Within a hull's width it lands exactly on the enemy, and a later tap keeps it
        // over one a little nearer.
        let (state, camera) = scene(&[(2, 1.0, 25.0), (3, 20.0, 0.0)], Vec::new());
        let on = assist
            .aim(&state, &camera, AimMode::Manual, (0.0, -1.0))
            .unwrap();
        assert!((on.angle - 1.0_f64.atan2(25.0)).abs() < 1e-9);
        let tap = assist
            .aim(&state, &camera, AimMode::Track, (0.0, 0.0))
            .unwrap();
        assert_eq!(tap.target, Some(2));
    }

    #[test]
    fn a_turned_wall_blocks_by_its_rotated_footprint() {
        // A wall 6 m long along x, turned a quarter turn, lies along z: it blocks a lane
        // along x through its centre but not one passing 2 m beside it along z.
        let mut covers = vec![wall(5.0, 0.0, 6.0, 0.5)];
        covers[0].rotation = Quat4::yaw(std::f64::consts::FRAC_PI_2);
        assert!(!lane_clear(&covers, (0.0, 0.0), (10.0, 0.0)));
        assert!(lane_clear(&covers, (7.0, -10.0), (7.0, 10.0)));
        assert!(!lane_clear(&covers, (5.0, -10.0), (5.0, 10.0)));
    }

    #[test]
    fn pointer_mode_leaves_the_aim_to_the_pointer() {
        let (state, camera) = scene(&[(2, 0.0, 20.0)], Vec::new());
        let mut assist = AimAssist::default();
        assert_eq!(
            assist.aim(&state, &camera, AimMode::Pointer, (0.0, 0.0)),
            None
        );
        assert_eq!(assist.target, None);
        // The tracked target outlasts pointer frames between touches of the stick.
        assist.aim(&state, &camera, AimMode::Track, (0.0, 0.0));
        assert_eq!(
            assist.aim(&state, &camera, AimMode::Pointer, (0.0, 0.0)),
            None
        );
        assert_eq!(assist.target, Some(2));
    }
}
