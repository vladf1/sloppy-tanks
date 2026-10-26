//! "Dodger first": enemy shells drawn toward the viewer's predicted present as they close
//! in, struck against the predicted hull ahead of the host, and eased back to the display
//! once they have passed.

use sloppy_core::net::network_timeline::{Dodger, NetworkTimeline, TRAIL_TICKS};
use sloppy_core::net::own_shots::{CoverCast, CoverHit, NoCover};
use sloppy_core::net::replication::TimedEvent;
use sloppy_core::net::shot_paths::{LivePaths, PathEntry, ShotLaunch, ShotPath};
use sloppy_core::sim::data::{STEP, weapon};
use sloppy_core::sim::hitboxes::SHELL_HIT_RADIUS;
use sloppy_core::sim::math::{Point3, Vec2};
use sloppy_core::sim::render_state::{RenderShot, RenderState};
use sloppy_core::sim::shooter_rewind::MAX_REWIND_TICKS;
use sloppy_core::sim::tank_dimensions::tank_hull;
use sloppy_core::sim::types::{SimEvent, SimEventType, Team, VehicleKind, Weapon};

const STEP_MS: f64 = STEP * 1000.0;
/// The viewer's predicted present runs this many ticks ahead of the host's newest (about
/// a 120 ms round trip with the input margin).
const PRESENT_LEAD: f64 = 8.0;
const VIEWER: u32 = 1;
const SHELL: u32 = 50;
/// Frames drawn per host tick.
const FRAMES_PER_TICK: f64 = 4.0;

/// One drawn frame.
struct Frame {
    present: f64,
    display: f64,
    shots: Vec<RenderShot>,
    events: Vec<SimEvent>,
}

/// The viewer, at the origin facing +z.
fn dodger(present: f64) -> Dodger {
    Dodger {
        present,
        position: Vec2::new(0.0, 0.0),
        heading: 0.0,
        kind: VehicleKind::Balanced,
        team: Team::Blue,
        tank_id: VIEWER,
        trail: [None; TRAIL_TICKS],
    }
}

/// The front face of the viewer's hit box, as the host's shells meet it.
fn hull_front() -> f64 {
    let hull = tank_hull(VehicleKind::Balanced);
    hull.center.z + hull.size.z / 2.0 + SHELL_HIT_RADIUS
}

/// A standard enemy shell fired on `tick` from `x`, `z` straight down -z.
fn enemy_shell(tick: f64, x: f64, z: f64) -> ShotPath {
    ShotPath {
        id: SHELL,
        tick,
        x,
        z,
        vx: 0.0,
        vz: -weapon(Weapon::Standard).speed,
        launch: ShotLaunch {
            team: Team::Red,
            weapon: Weapon::Standard,
            y: Some(1.0),
            visual_y: None,
            thrust: None,
            rewind: 0,
        },
    }
}

/// The tick `path` reaches `z`.
fn reaches(path: &ShotPath, z: f64) -> f64 {
    path.tick + (path.z - z) / -path.vz / STEP
}

fn timed(tick: f64, kind: SimEventType, x: f64, z: f64, id: Option<u32>) -> TimedEvent {
    let mut event = SimEvent::at(kind, x, z);
    event.id = id;
    TimedEvent {
        event_id: 0,
        tick,
        event,
    }
}

/// Plays the host's ticks up to `last` as they fall due with no transport delay, `host`
/// giving each tick's events and path entries, and draws four frames a tick with the
/// viewer's present `PRESENT_LEAD` ticks ahead, as a dodger or not.
fn play(
    last: u64,
    dodger_first: bool,
    cover: &impl CoverCast,
    host: impl Fn(u64) -> (Vec<TimedEvent>, Vec<PathEntry>),
) -> (Vec<Frame>, NetworkTimeline) {
    play_as(dodger, last, dodger_first, cover, host)
}

/// [`play`] with the viewer's hull from `viewer` at each present.
fn play_as(
    viewer: impl Fn(f64) -> Dodger,
    last: u64,
    dodger_first: bool,
    cover: &impl CoverCast,
    host: impl Fn(u64) -> (Vec<TimedEvent>, Vec<PathEntry>),
) -> (Vec<Frame>, NetworkTimeline) {
    let mut timeline = NetworkTimeline::default();
    timeline.reset(&RenderState::default(), 0, 0.0, &LivePaths::default());
    let mut shown = RenderState::default();
    let mut frames = Vec::new();
    let mut newest = 0;
    let mut now = 0.0;
    while newest < last {
        now += STEP_MS / FRAMES_PER_TICK;
        while (newest + 1) as f64 * STEP_MS <= now + 1e-9 {
            newest += 1;
            let (events, paths) = host(newest);
            timeline
                .push(RenderState::default(), newest, events, paths, cover)
                .unwrap();
            timeline.arrive(now);
        }
        let present = now / STEP_MS + PRESENT_LEAD;
        let viewer = viewer(present);
        let events = timeline.read(
            now,
            100.0,
            STEP / FRAMES_PER_TICK,
            &mut shown,
            dodger_first.then_some(&viewer),
        );
        frames.push(Frame {
            present,
            display: timeline.display_tick(),
            shots: shown.shots.clone(),
            events,
        });
    }
    (frames, timeline)
}

fn kinds(frames: &[Frame], kind: SimEventType) -> Vec<usize> {
    frames
        .iter()
        .enumerate()
        .filter(|(_, frame)| frame.events.iter().any(|event| event.kind == kind))
        .map(|(index, _)| index)
        .collect()
}

/// The host hits the viewer with a shell fired from 20 m ahead.
fn host_hits(path: ShotPath) -> impl Fn(u64) -> (Vec<TimedEvent>, Vec<PathEntry>) {
    let front = hull_front();
    let hit = reaches(&path, front);
    move |tick| {
        if tick == path.tick as u64 {
            (Vec::new(), vec![PathEntry::Launch(path)])
        } else if tick == hit.ceil() as u64 {
            (
                vec![
                    timed(hit, SimEventType::Impact, 0.0, front, None),
                    timed(hit, SimEventType::Hurt, 0.0, 0.0, Some(VIEWER)),
                ],
                vec![PathEntry::End {
                    id: SHELL,
                    tick: hit,
                }],
            )
        } else {
            (Vec::new(), Vec::new())
        }
    }
}

#[test]
fn an_incoming_shell_meets_the_hull_as_the_present_reaches_the_hit() {
    let path = enemy_shell(10.0, 0.0, 20.0);
    let hit = reaches(&path, hull_front());
    let last = hit.ceil() as u64 + 20;

    let (frames, _) = play(last, true, &NoCover, host_hits(path));
    // The shell leaves the enemy's muzzle on the display clock...
    let first = frames.iter().find(|frame| !frame.shots.is_empty()).unwrap();
    assert!(
        (first.shots[0].z - path.at(first.display).z).abs() < 0.05,
        "it appears where the display has it: {} against {}",
        first.shots[0].z,
        path.at(first.display).z
    );
    // ...and strikes the predicted hull, ahead of the host, as the present reaches the hit.
    let impacts = kinds(&frames, SimEventType::Impact);
    assert_eq!(
        impacts.len(),
        1,
        "one impact: the host's is not shown again"
    );
    let strike = &frames[impacts[0]];
    assert!(
        (strike.present - hit).abs() <= 0.5,
        "struck at present {} for the host's hit at {hit}",
        strike.present
    );
    assert!(
        frames[impacts[0]..]
            .iter()
            .all(|frame| frame.shots.is_empty()),
        "gone once it struck"
    );
    // The hit itself shows as soon as it arrives, before the display reaches it.
    let hurt = &frames[kinds(&frames, SimEventType::Hurt)[0]];
    assert!(hurt.display < hit, "hurt at display {}", hurt.display);
    // Each frame it moved on, never back.
    let mut previous = f64::INFINITY;
    for frame in &frames {
        if let Some(shot) = frame.shots.first() {
            assert!(shot.z <= previous + 1e-9, "ran backwards to {}", shot.z);
            previous = shot.z;
        }
    }

    // Drawn on the display clock alone, the shell is still a view delay away when the
    // present reaches the hit.
    let (plain, _) = play(last, false, &NoCover, host_hits(path));
    let at_hit = plain.iter().find(|frame| frame.present >= hit).unwrap();
    let gap = at_hit.shots[0].z - hull_front();
    assert!(gap > 2.5, "{gap} m short of the hull");
    let impact = &plain[kinds(&plain, SimEventType::Impact)[0]];
    assert!(impact.display >= hit && impact.present > hit + PRESENT_LEAD);
}

#[test]
fn a_shell_that_passes_the_viewer_eases_back_onto_the_display_clock() {
    // Three metres to the side: it misses.
    let path = enemy_shell(10.0, 3.0, 20.0);
    let (frames, _) = play(160, true, &NoCover, |tick| {
        if tick == 10 {
            (Vec::new(), vec![PathEntry::Launch(path)])
        } else {
            (Vec::new(), Vec::new())
        }
    });
    assert!(kinds(&frames, SimEventType::Impact).is_empty(), "no strike");
    let ahead = |frame: &Frame| (path.at(frame.display).z - frame.shots[0].z) / -path.vz / STEP;
    let drawn: Vec<&Frame> = frames
        .iter()
        .filter(|frame| !frame.shots.is_empty())
        .collect();
    // Beside the viewer it is drawn in the present...
    let beside = drawn
        .iter()
        .find(|frame| frame.shots[0].z <= 0.0)
        .expect("it passes the viewer");
    assert!(
        ahead(beside) > 0.75 * (beside.present - beside.display),
        "beside the viewer it is drawn near the present: {} ticks ahead of {}",
        ahead(beside),
        beside.present - beside.display
    );
    // ...and later back on the display clock, never running backwards on the way.
    let last = drawn.last().unwrap();
    assert!(ahead(last).abs() < 1e-6, "{} ticks ahead", ahead(last));
    for pair in drawn.windows(2) {
        assert!(pair[1].shots[0].z <= pair[0].shots[0].z + 1e-9);
    }
}

#[test]
fn a_strike_the_host_flew_past_is_a_phantom_and_the_shell_flies_on() {
    // On the host the hull was elsewhere: the shell's path stays open past it.
    let path = enemy_shell(10.0, 0.0, 20.0);
    let hit = reaches(&path, hull_front());
    let (frames, timeline) = play(hit.ceil() as u64 + 30, true, &NoCover, |tick| {
        if tick == 10 {
            (Vec::new(), vec![PathEntry::Launch(path)])
        } else {
            (Vec::new(), Vec::new())
        }
    });
    assert_eq!(timeline.phantom_strikes, 1);
    assert_eq!(kinds(&frames, SimEventType::Impact).len(), 1, "struck once");
    let behind = frames
        .iter()
        .rev()
        .find(|frame| !frame.shots.is_empty())
        .expect("it flies on");
    assert!(behind.shots[0].z < -hull_front(), "{}", behind.shots[0].z);
}

/// A wall across the field at `z`, facing +z.
struct Wall {
    z: f64,
}

impl CoverCast for Wall {
    fn cast_cover(&self, origin: Point3, direction: Vec2, distance: f64) -> Option<CoverHit> {
        let along = (origin.z - self.z) / -direction.z;
        (direction.z < 0.0 && along >= 0.0 && along <= distance).then_some(CoverHit {
            distance: along,
            normal: Vec2::new(0.0, 1.0),
            struck: None,
        })
    }
}

#[test]
fn an_incoming_shell_stops_at_cover_ahead_of_the_host_and_its_impact_shows_once() {
    let wall = Wall { z: 5.0 };
    let path = enemy_shell(10.0, 0.0, 20.0);
    let stop = reaches(&path, wall.z);
    let (frames, _) = play(stop.ceil() as u64 + 20, true, &wall, |tick| {
        if tick == 10 {
            (Vec::new(), vec![PathEntry::Launch(path)])
        } else if tick == stop.ceil() as u64 {
            (
                vec![timed(stop, SimEventType::Impact, 0.0, wall.z, None)],
                vec![PathEntry::End {
                    id: SHELL,
                    tick: stop,
                }],
            )
        } else {
            (Vec::new(), Vec::new())
        }
    });
    let impacts = kinds(&frames, SimEventType::Impact);
    assert_eq!(impacts.len(), 1, "the host's impact is not shown again");
    let shown = &frames[impacts[0]];
    assert!(shown.display < stop, "shown ahead of the display");
    assert!(shown.shots.is_empty(), "the shell ends with its impact");
    assert!(
        !frames[impacts[0] - 1].shots.is_empty()
            && frames[impacts[0] - 1].shots[0].z >= wall.z - 0.01,
        "it flew up to the wall, not through it"
    );
}

#[test]
fn a_ricochet_that_bounces_back_at_the_viewer_closes_in_again() {
    // Fired away from the viewer, it bounces off a wall 20 m out and comes back.
    let speed = weapon(Weapon::Ricochet).speed;
    let away = ShotPath {
        vz: speed,
        launch: ShotLaunch {
            weapon: Weapon::Ricochet,
            ..enemy_shell(10.0, 0.0, 10.0).launch
        },
        ..enemy_shell(10.0, 0.0, 10.0)
    };
    let bounce = away.tick + 10.0 / speed / STEP;
    let back = ShotPath {
        tick: bounce,
        z: 20.0,
        vz: -speed,
        ..away
    };
    let hit = reaches(&back, hull_front());
    let (frames, _) = play(hit.ceil() as u64 + 20, true, &NoCover, |tick| {
        if tick == 10 {
            (Vec::new(), vec![PathEntry::Launch(away)])
        } else if tick == bounce.ceil() as u64 {
            (Vec::new(), vec![PathEntry::Change(back)])
        } else if tick == hit.ceil() as u64 {
            (
                vec![timed(hit, SimEventType::Impact, 0.0, hull_front(), None)],
                vec![PathEntry::End {
                    id: SHELL,
                    tick: hit,
                }],
            )
        } else {
            (Vec::new(), Vec::new())
        }
    });
    let impacts = kinds(&frames, SimEventType::Impact);
    assert_eq!(impacts.len(), 1);
    let strike = &frames[impacts[0]];
    assert!(
        (strike.present - hit).abs() <= 0.5,
        "struck at present {} for the host's hit at {hit}",
        strike.present
    );
}

#[test]
fn a_players_shell_strikes_the_hull_where_its_shooter_saw_it() {
    // The viewer slid 3 m along -x over the last ticks: a player's shell, judged against
    // where its shooter saw the hull 12 ticks ago, strikes it there.
    let moved = |present: f64| Dodger {
        trail: std::array::from_fn(|back| Some((Vec2::new(0.25 * back as f64, 0.0), 0.0))),
        ..dodger(present)
    };
    let fire = |rewind: u32| {
        let mut path = enemy_shell(10.0, 3.0, 20.0);
        path.launch.rewind = rewind;
        let hit = reaches(&path, hull_front());
        let quiet = move |tick: u64| {
            if tick == 10 {
                (Vec::new(), vec![PathEntry::Launch(path)])
            } else {
                (Vec::new(), Vec::new())
            }
        };
        let (frames, _) = play_as(moved, hit.ceil() as u64 + 2, true, &NoCover, quiet);
        (frames, hit)
    };
    let (frames, hit) = fire(MAX_REWIND_TICKS);
    let impacts = kinds(&frames, SimEventType::Impact);
    assert_eq!(impacts.len(), 1, "struck where the hull was");
    assert!((frames[impacts[0]].present - hit).abs() <= 1.0);
    // A bot's shell is judged against the hull now, 3 m away: it flies past.
    let (frames, _) = fire(0);
    assert!(kinds(&frames, SimEventType::Impact).is_empty());
}
