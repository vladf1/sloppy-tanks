//! Clocks and display timing: the host's fixed-step clock and the dev transport delay
//! (`tests/network-clock.test.ts`), input cadence (`input-cadence.test.ts`), the adaptive
//! playout clock (`playout-clock.test.ts`), pose/event timelines
//! (`network-timeline.test.ts`), and projectile paths drawn against the host's flight.

mod net_support;
mod support;

use std::f64::consts::PI;

use net_support::{Sweep, WIRE_SLACK, baseline, fire_from, set_linvel, set_translation};
use sloppy_core::net::fixed_step_clock::FixedStepClock;
use sloppy_core::net::input_cadence::InputCadence;
use sloppy_core::net::multiplayer_simulation::{MultiplayerOptions, create_multiplayer_simulation};
use sloppy_core::net::network_timeline::NetworkTimeline;
use sloppy_core::net::player_controls::{Action, Aim, ControlInput};
use sloppy_core::net::playout_clock::PlayoutClock;
use sloppy_core::net::render_timeline::RenderTimeline;
use sloppy_core::net::replication::{StateMirror, StateStream, TimedEvent};
use sloppy_core::net::scene_codec::Scene;
use sloppy_core::net::shot_paths::{
    LivePaths, PATH_TOLERANCE, PathEntry, ShotLaunch, ShotPath, ShotPathRecorder,
};
use sloppy_core::net::transport_delay::DelayedChannel;
use sloppy_core::sim::Simulation;
use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::data::{INTERCEPTION_RADIUS, LASER_DEFENSE, MINE_RADIUS, weapon};
use sloppy_core::sim::hitboxes::SHELL_HIT_RADIUS;
use sloppy_core::sim::math::{Point3, Quat4, Vec2};
use sloppy_core::sim::render_state::{RenderFragment, RenderShot, RenderState};
use sloppy_core::sim::timber_layout::{
    TimberFace, TimberHit, TimberMark, TimberPart, TimberPartKind,
};
use sloppy_core::sim::types::{
    CoverKind, Mine, Shot, SimEvent, SimEventType, Team, VehicleCommand, Weapon,
};

// ---- Host clock and transport delay ---------------------------------------------------

#[test]
fn fifty_ms_host_batches_preserve_exactly_sixty_simulation_ticks_per_second() {
    let mut clock = FixedStepClock::new(0.0);
    let mut ticks = Vec::new();
    let mut now = 50.0;
    while now <= 10000.0 {
        assert_eq!(clock.advance(now, |tick| ticks.push(tick)), Ok(true));
        now += 50.0;
    }
    assert_eq!(ticks.len(), 600);
    assert_eq!(clock.tick, 600);
    assert!(clock.debt_ms < 1e-6);
}

#[test]
fn catch_up_is_bounded_and_retains_debt_instead_of_skipping_physics() {
    let mut clock = FixedStepClock::new(1000.0);
    assert_eq!(clock.advance(1200.0, |_| {}), Ok(true));
    assert_eq!(clock.tick, 6);
    assert!((clock.debt_ms - 100.0).abs() < 1e-6);
    clock.advance(1250.0, |_| {}).unwrap();
    assert_eq!(clock.tick, 12);
    assert!((clock.debt_ms - 50.0).abs() < 1e-6);
    clock.advance(1300.0, |_| {}).unwrap();
    assert_eq!(clock.tick, 18);
    assert!(clock.debt_ms < 1e-6);
}

#[test]
fn overload_fails_before_executing_an_unbounded_batch_and_backwards_clocks_add_no_time() {
    let mut clock = FixedStepClock::new(100.0);
    clock.advance(90.0, |_| panic!("No tick is due")).unwrap();
    clock.advance(150.0, |_| {}).unwrap();
    assert_eq!(clock.tick, 3);
    assert_eq!(
        clock.advance(500.0, |_| panic!("Overload must terminate")),
        Ok(false)
    );
    assert_eq!(clock.tick, 3);
    assert_eq!(
        clock.advance(f64::NAN, |_| {}),
        Err("Invalid host clock".into())
    );
}

#[test]
fn transport_jitter_preserves_reliable_message_order_and_due_time() {
    let mut channel = DelayedChannel::default();
    channel.send("old", 0.0, 100.0).unwrap();
    channel.send("new", 20.0, 10.0).unwrap();
    assert!(channel.receive(30.0).is_empty());
    assert!(channel.receive(99.0).is_empty());
    assert_eq!(channel.receive(100.0), vec!["old", "new"]);
    assert_eq!(channel.len(), 0);
}

#[test]
fn delay_buffers_are_bounded_and_reset_discards_old_life_actions() {
    let mut channel = DelayedChannel::with_capacity(2);
    channel.send("mine", 0.0, 50.0).unwrap();
    channel.send("fire", 0.0, 50.0).unwrap();
    assert!(
        channel
            .send("overflow", 0.0, 1.0)
            .unwrap_err()
            .contains("capacity")
    );
    channel.clear();
    assert!(channel.receive(1000.0).is_empty());
    assert!(
        channel
            .send("invalid", 0.0, f64::NAN)
            .unwrap_err()
            .contains("Invalid")
    );
}

// ---- Input cadence --------------------------------------------------------------------

fn sample() -> ControlInput {
    ControlInput {
        control_epoch: 1,
        seq: 0,
        observed_tick: 0,
        tick: None,
        move_x: 0.0,
        move_z: 0.0,
        aim: Aim::Point { x: 10.0, z: 20.0 },
        fire: false,
        actions: Vec::new(),
    }
}

#[test]
fn idle_input_sends_once_per_second_while_held_movement_and_fire_keep_their_lease_renewed() {
    for (input, expected) in [
        (sample(), 6),
        (
            ControlInput {
                move_x: 1.0,
                ..sample()
            },
            120,
        ),
        (
            ControlInput {
                fire: true,
                ..sample()
            },
            120,
        ),
    ] {
        let mut cadence = InputCadence::default();
        let mut sent = 0;
        let mut now = 0.0;
        while now < 6000.0 {
            if cadence.due(&input, now) {
                cadence.sent(&input, now);
                sent += 1;
            }
            now += 10.0;
        }
        assert_eq!(sent, expected);
    }
}

#[test]
fn aim_one_shot_actions_and_epochs_wake_idle_sends_and_releases_do_not_wait_a_second() {
    for input in [
        ControlInput {
            aim: Aim::Point { x: 11.0, z: 20.0 },
            ..sample()
        },
        ControlInput {
            control_epoch: 2,
            ..sample()
        },
        ControlInput {
            aim: Aim::Angle(0.0),
            ..sample()
        },
    ] {
        let mut cadence = InputCadence::default();
        cadence.sent(&sample(), 0.0);
        assert!(!cadence.due(&input, 49.0));
        assert!(cadence.due(&input, 50.0));
    }
    for held in [
        ControlInput {
            move_x: 1.0,
            ..sample()
        },
        ControlInput {
            move_z: -1.0,
            ..sample()
        },
        ControlInput {
            fire: true,
            ..sample()
        },
    ] {
        let mut cadence = InputCadence::default();
        cadence.sent(&held, 0.0);
        assert!(cadence.due(&sample(), 50.0));
        cadence.sent(&sample(), 50.0);
        assert!(!cadence.due(&sample(), 100.0));
    }
}

#[test]
fn presses_releases_and_one_shot_actions_skip_the_20_hz_slot_but_stay_25_ms_apart() {
    let held = |move_x: f64, move_z: f64, fire: bool| ControlInput {
        move_x,
        move_z,
        fire,
        ..sample()
    };
    for (previous, input) in [
        (sample(), held(1.0, 0.0, false)),
        (held(1.0, 0.0, false), sample()),
        (held(0.0, -1.0, false), held(0.0, 1.0, false)),
        (held(0.0, -1.0, false), held(0.7, -0.7, false)),
        (sample(), held(0.0, 0.0, true)),
        (held(0.0, 0.0, true), sample()),
        (
            sample(),
            ControlInput {
                actions: vec![Action::Mine],
                ..sample()
            },
        ),
        (
            sample(),
            ControlInput {
                actions: vec![Action::Ammo(Weapon::Rocket)],
                ..sample()
            },
        ),
    ] {
        let mut cadence = InputCadence::default();
        cadence.sent(&previous, 0.0);
        assert!(!cadence.due(&input, 24.0));
        assert!(cadence.due(&input, 25.0));
    }
    // A stick drifting within one direction, and aim, keep the 20 Hz cadence.
    for (previous, input) in [
        (held(0.4, 0.2, false), held(0.6, 0.3, false)),
        (
            held(1.0, 0.0, false),
            ControlInput {
                aim: Aim::Point { x: 12.0, z: 20.0 },
                ..held(1.0, 0.0, false)
            },
        ),
    ] {
        let mut cadence = InputCadence::default();
        cadence.sent(&previous, 0.0);
        assert!(!cadence.due(&input, 49.0));
        assert!(cadence.due(&input, 50.0));
    }
}

#[test]
fn sub_centimetre_camera_noise_does_not_flood_idle_traffic_but_cumulative_aim_changes_are_sent() {
    let mut cadence = InputCadence::default();
    cadence.sent(&sample(), 0.0);
    let aimed = |x: f64| ControlInput {
        aim: Aim::Point { x, z: 20.0 },
        ..sample()
    };
    assert!(!cadence.due(&aimed(10.005), 100.0));
    assert!(cadence.due(&aimed(10.02), 150.0));
    let angle = |angle: f64| ControlInput {
        aim: Aim::Angle(angle),
        ..sample()
    };
    cadence.sent(&angle(std::f64::consts::PI - 0.0001), 200.0);
    assert!(!cadence.due(&angle(-std::f64::consts::PI + 0.0001), 300.0));
    assert!(cadence.due(&angle(-std::f64::consts::PI + 0.002), 300.0));
}

// ---- Playout clock --------------------------------------------------------------------

const FRAME_MS: f64 = 1000.0 / 60.0;
const TICKS_PER_BATCH: u64 = 3;
const BATCH_MS: f64 = 50.0;

#[derive(Clone, Copy, Debug)]
struct Frame {
    now_ms: f64,
    display_ms: f64,
    newest_ms: f64,
    buffer_ms: f64,
}

/// Plays 20 Hz batches through a path whose delay per batch is `delay(batch)`, reading
/// at 60 Hz.
fn play(seconds: f64, mut delay: impl FnMut(u64) -> f64) -> Vec<Frame> {
    let mut clock = PlayoutClock::default();
    let mut arrivals = Vec::new();
    let mut release = f64::NEG_INFINITY;
    let mut batch = 1;
    while batch as f64 * BATCH_MS <= seconds * 1000.0 {
        // A reliable ordered stream never delivers a batch before the one ahead of it.
        release = release.max(batch as f64 * BATCH_MS + delay(batch));
        arrivals.push((release, batch * TICKS_PER_BATCH));
        batch += 1;
    }
    let start = delay(0);
    clock.reset(0, start);
    let mut next = 0;
    let mut newest_ms = 0.0;
    let mut frames = Vec::new();
    let mut now = start;
    while now <= seconds * 1000.0 {
        while next < arrivals.len() && arrivals[next].0 <= now {
            clock.arrive(arrivals[next].1, arrivals[next].0);
            newest_ms = arrivals[next].1 as f64 * 1000.0 / 60.0;
            next += 1;
        }
        frames.push(Frame {
            now_ms: now,
            display_ms: clock.read(now),
            newest_ms,
            buffer_ms: clock.buffer_ms,
        });
        now += FRAME_MS;
    }
    frames
}

fn assert_monotonic(frames: &[Frame]) {
    for pair in frames.windows(2) {
        assert!(
            pair[1].display_ms >= pair[0].display_ms,
            "display time never runs backwards"
        );
    }
}

/// Frames whose display ran past the newest received data.
fn starved(frames: &[Frame]) -> usize {
    frames
        .iter()
        .filter(|frame| frame.display_ms > frame.newest_ms)
        .count()
}

/// The largest playout buffer the frames held.
fn peak_buffer(frames: &[Frame]) -> f64 {
    frames
        .iter()
        .map(|frame| frame.buffer_ms)
        .fold(0.0, f64::max)
}

#[test]
fn a_steady_long_path_keeps_a_full_buffer_and_rtt_no_longer_eats_the_interpolation_delay() {
    for path in [0.0, 50.0, 100.0, 200.0] {
        let frames: Vec<Frame> = play(6.0, |_| path)
            .into_iter()
            .filter(|frame| frame.now_ms > 1500.0)
            .collect();
        assert_monotonic(&frames);
        for frame in &frames {
            assert!(
                frame.display_ms < frame.newest_ms,
                "display stays behind received data at {path} ms"
            );
        }
        for pair in frames.windows(2) {
            let advance = pair[1].display_ms - pair[0].display_ms;
            assert!(
                advance > FRAME_MS * 0.85 && advance < FRAME_MS * 1.15,
                "motion never pauses"
            );
        }
    }
}

#[test]
fn ordinary_arrival_jitter_is_absorbed_without_underrun() {
    let mut seed: u32 = 7;
    let frames: Vec<Frame> = play(10.0, |_| {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        80.0 + f64::from(seed) / 4_294_967_296.0 * 25.0
    })
    .into_iter()
    .filter(|frame| frame.now_ms > 3000.0)
    .collect();
    assert_monotonic(&frames);
    assert_eq!(starved(&frames), 0);
    assert!(peak_buffer(&frames) < 110.0);
}

#[test]
fn a_head_of_line_stall_extrapolates_briefly_grows_the_buffer_then_gives_the_delay_back() {
    // Batch 60 (3 s) is retransmitted 200 ms late; later batches queue behind it.
    let frames = play(20.0, |batch| if batch == 60 { 230.0 } else { 30.0 });
    assert_monotonic(&frames);
    for frame in &frames {
        assert!(
            frame.display_ms <= frame.newest_ms + 100.0,
            "extrapolation is bounded"
        );
    }
    let starved = starved(&frames);
    assert!(
        starved > 0 && starved as f64 * FRAME_MS < 200.0,
        "only part of the stall shows"
    );
    assert!(peak_buffer(&frames) > 100.0, "the stall grows the buffer");
    assert!(
        frames.last().unwrap().buffer_ms < 75.0,
        "the buffer shrinks once arrivals are steady again"
    );
}

#[test]
fn repeated_stalls_buy_enough_buffer_to_hide_the_next_one() {
    // Every second, one batch is retransmitted 150 ms late on a 40 ms path.
    let frames = play(12.0, |batch| if batch % 20 == 0 { 190.0 } else { 40.0 });
    assert_monotonic(&frames);
    let settled: Vec<Frame> = frames
        .into_iter()
        .filter(|frame| frame.now_ms > 4000.0)
        .collect();
    assert_eq!(
        starved(&settled),
        0,
        "motion no longer pauses once the stall pattern is learned"
    );
    assert!(peak_buffer(&settled) <= 250.0);
}

#[test]
fn reset_starts_behind_the_baseline_and_a_long_gap_snaps_forward() {
    let mut clock = PlayoutClock::default();
    clock.reset(600, 1000.0);
    assert!(clock.read(1000.0) < 10_000.0);
    clock.arrive(1200, 11_000.0);
    let display = clock.read(11_000.0);
    assert!(
        display > 19_800.0 && display <= 20_000.0,
        "a long gap is not slewed through"
    );
}

// ---- Render and network timelines ---------------------------------------------------

fn empty_room() -> Simulation {
    create_multiplayer_simulation(4242.0, &[], MultiplayerOptions::default()).unwrap()
}

fn with_viewer(
    source: &RenderState,
    edit: impl FnOnce(&mut sloppy_core::sim::render_state::RenderTank),
) -> RenderState {
    let mut state = source.clone();
    let viewer = state.viewer_id;
    edit(
        state
            .tanks
            .iter_mut()
            .find(|tank| tank.id == viewer)
            .unwrap(),
    );
    state
}

fn read(timeline: &mut RenderTimeline, time: f64, local: f64, dt: f64) -> RenderState {
    let mut output = RenderState::default();
    timeline.read(time, local, dt, &mut output);
    output
}

#[test]
fn local_hull_heading_advances_between_packets_through_the_short_arc_and_resets_on_a_new_life() {
    let sim = empty_room();
    let source = sim.render_state(Some(sim.tanks[0].id));
    let pose = |heading: f64, elapsed: f64, life: u32| {
        let mut state = with_viewer(&source, |viewer| {
            viewer.heading = heading;
            viewer.life = life;
        });
        state.elapsed = elapsed;
        state
    };
    let first = pose(3.1, 0.0, 0);
    let next = pose(-3.1, 0.05, 0);
    let mut timeline = RenderTimeline::default();
    timeline.reset(first);
    read(&mut timeline, 0.0, 0.0, 1.0 / 60.0);
    timeline.push(next);
    let a = read(&mut timeline, 0.05, 0.05, 1.0 / 60.0)
        .viewer()
        .unwrap()
        .heading;
    assert!(
        a > 3.1 && a < std::f64::consts::PI * 2.0 - 3.1,
        "new packet must not snap local heading"
    );
    let b = read(&mut timeline, 0.05, 0.0667, 1.0 / 60.0)
        .viewer()
        .unwrap()
        .heading;
    assert!(
        b > a,
        "heading keeps moving while waiting for the next packet"
    );
    timeline.push(pose(-1.0, 0.1, 1));
    assert_eq!(
        read(&mut timeline, 0.1, 0.1, 1.0 / 60.0)
            .viewer()
            .unwrap()
            .heading,
        -1.0
    );
}

fn event(kind: SimEventType, id: Option<u32>, x: f64) -> SimEvent {
    let mut event = SimEvent::at(kind, x, 0.0);
    event.id = id;
    event
}

#[test]
fn coalesced_death_and_respawn_preserve_each_life_and_emit_effects_on_the_display_clock_once() {
    let sim = empty_room();
    let first = sim.render_state(Some(sim.tanks[0].id));
    let viewer = first.viewer_id;
    let dead = with_viewer(&first, |tank| {
        tank.alive = false;
        tank.hp = 0.0;
    });
    let respawn = with_viewer(&first, |tank| {
        tank.life = 1;
        tank.position = Point3::new(30.0, 0.65, 0.0);
    });
    let mut timeline = NetworkTimeline::default();
    timeline.reset(&first, 0, 0.0, &LivePaths::default());
    timeline
        .push(
            dead,
            2,
            vec![TimedEvent {
                event_id: 1,
                tick: 2.0,
                event: event(SimEventType::Death, Some(viewer), 0.0),
            }],
            Vec::new(),
        )
        .unwrap();
    timeline
        .push(
            respawn.clone(),
            4,
            vec![TimedEvent {
                event_id: 2,
                tick: 4.0,
                event: event(SimEventType::Respawn, Some(viewer), 30.0),
            }],
            Vec::new(),
        )
        .unwrap();
    timeline
        .push(respawn.clone(), 6, Vec::new(), Vec::new())
        .unwrap();
    timeline.arrive(50.0);
    let mut state = RenderState::default();
    let mut frames = Vec::new();
    for now in 50..400 {
        let events = timeline.read(f64::from(now), 0.0, 0.001, &mut state);
        let tank = state.viewer().unwrap();
        frames.push((
            tank.alive,
            tank.life,
            tank.position.x,
            events.iter().map(|event| event.kind).collect::<Vec<_>>(),
        ));
    }
    assert!(frames[0].0);
    let death = frames.iter().position(|frame| !frame.0).unwrap();
    let respawned = frames.iter().position(|frame| frame.1 == 1).unwrap();
    assert!(
        death > 0 && respawned > death,
        "each life is displayed in order"
    );
    assert_eq!(
        frames[death].3,
        vec![SimEventType::Death],
        "death effect lands with the wreck"
    );
    assert_eq!(frames[respawned].3, vec![SimEventType::Respawn]);
    assert_eq!(
        frames[respawned].2, 30.0,
        "no interpolation from the wreck to the respawn"
    );
    let all: Vec<SimEventType> = frames.iter().flat_map(|frame| frame.3.clone()).collect();
    assert_eq!(
        all,
        vec![SimEventType::Death, SimEventType::Respawn],
        "effects play once"
    );
    timeline.reset(&respawn, 6, 400.0, &LivePaths::default());
    assert!(timeline.read(410.0, 0.0, 1.0 / 60.0, &mut state).is_empty());
}

#[test]
fn a_projectile_born_and_destroyed_between_snapshots_follows_its_path_and_disappears_at_impact() {
    let sim = empty_room();
    let state = sim.render_state(Some(sim.tanks[0].id));
    let path = ShotPath {
        id: 999,
        tick: 1.0,
        x: 0.0,
        z: 0.0,
        vx: 120.0,
        vz: 0.0,
        launch: ShotLaunch {
            team: Team::Blue,
            weapon: Weapon::Standard,
            y: Some(1.0),
            visual_y: None,
            thrust: None,
        },
    };
    let mut timeline = NetworkTimeline::default();
    timeline.reset(&state, 0, 0.0, &LivePaths::default());
    timeline
        .push(
            state,
            6,
            vec![TimedEvent {
                event_id: 1,
                tick: 1.5,
                event: SimEvent::at(SimEventType::Impact, 1.0, 0.0),
            }],
            vec![
                PathEntry::Launch(path),
                PathEntry::End { id: 999, tick: 1.5 },
            ],
        )
        .unwrap();
    timeline.arrive(50.0);
    let mut flights = 0;
    let mut impacts = Vec::new();
    let mut shown = RenderState::default();
    let mut now = 50.0;
    while now < 300.0 {
        let events = timeline.read(now, 0.0, 1.0 / 60.0, &mut shown);
        let shots = shown.shots.clone();
        let tick = timeline.clock.display_ms * 60.0 / 1000.0;
        if (1.0..1.5).contains(&tick) {
            flights += 1;
            assert!(
                (shots[0].x - (tick - 1.0) * 2.0).abs() < 1e-8,
                "shot follows its path"
            );
            assert!(events.is_empty());
        } else {
            assert!(
                shots.is_empty(),
                "shot is visible only between launch and impact"
            );
        }
        if !events.is_empty() {
            assert!(tick >= 1.5);
            impacts.extend(events.iter().map(|event| event.kind));
        }
        now += 0.5;
    }
    assert!(flights > 0);
    assert_eq!(impacts, vec![SimEventType::Impact]);
}

#[test]
fn a_display_state_refilled_in_place_matches_a_fresh_read_as_names_debris_and_membership_change() {
    let sim = empty_room();
    let first = sim.render_state(Some(sim.tanks[0].id));
    assert!(first.tanks.len() > 2 && !first.covers.is_empty());
    let remote = first.tanks[1].id;
    let start_x = first.tanks[1].position.x;
    let beam = TimberPart {
        kind: TimberPartKind::Beam,
        index: 0,
        x: 0.0,
        y: 0.5,
        z: 0.0,
        w: 2.0,
        h: 0.2,
        d: 0.2,
        yaw: 0.0,
        lean: 0.0,
        color: 0x8b5a2b,
        damage: 1,
        damage_seed: 7,
        marks: vec![TimberMark {
            x: 0.1,
            y: 0.1,
            face: TimberFace::Front,
            size: 0.2,
            seed: 3,
        }],
    };
    let fragment = |id: u32, timber: bool| RenderFragment {
        id,
        life: 5.0,
        timber_part: timber.then(|| beam.clone()),
        position: Point3::new(f64::from(id), 1.0, 0.0),
        ..RenderFragment::default()
    };
    // Ticks 0 → 3: the remote hull moves 3 m and is renamed, the last tank leaves, a cover
    // is hit and timber debris appears; tick 6 brings the tank back and swaps the debris.
    let mut moved = first.clone();
    moved.tanks[1].position.x += 3.0;
    moved.tanks[1].name = "A much longer replacement name".into();
    moved.tanks.pop();
    moved.covers[0].timber_hits.push(TimberHit {
        x: 0.0,
        y: 1.0,
        z: 0.0,
        size: 0.3,
    });
    moved.fragments = vec![fragment(1, true), fragment(2, false)];
    let mut later = first.clone();
    later.fragments = vec![fragment(2, false), fragment(3, true)];
    let mut timeline = NetworkTimeline::default();
    timeline.reset(&first, 0, 0.0, &LivePaths::default());
    timeline.push(moved, 3, Vec::new(), Vec::new()).unwrap();
    timeline.push(later, 6, Vec::new(), Vec::new()).unwrap();
    timeline.arrive(50.0);
    let mut shown = RenderState::default();
    let mut interpolated = 0;
    for now in 50..400 {
        let now = f64::from(now);
        let mut fresh = RenderState::default();
        timeline.clone().read(now, 0.0, 1.0 / 60.0, &mut fresh);
        timeline.read(now, 0.0, 1.0 / 60.0, &mut shown);
        assert_eq!(
            shown, fresh,
            "the reused state at {now} ms keeps nothing stale"
        );
        let tick = timeline.display_tick();
        if tick > 0.5 && tick < 2.5 {
            interpolated += 1;
            let tank = shown.tanks.iter().find(|tank| tank.id == remote).unwrap();
            assert!(
                (tank.position.x - (start_x + tick)).abs() < 1e-9,
                "the remote hull moves 1 m per tick"
            );
            assert_eq!(tank.name, first.tanks[1].name);
        }
    }
    assert!(interpolated > 0);
    assert!(timeline.display_tick() > 6.0);
    assert_eq!(shown.tanks.len(), first.tanks.len());
    assert_eq!(shown.covers[0].timber_hits, first.covers[0].timber_hits);
    let debris: Vec<_> = shown
        .fragments
        .iter()
        .map(|fragment| (fragment.id, fragment.timber_part.is_some()))
        .collect();
    assert_eq!(debris, [(2, false), (3, true)]);
}

#[test]
fn moving_debris_rotations_interpolate_through_the_short_quaternion_arc() {
    let sim = empty_room();
    let source = sim.render_state(Some(sim.tanks[0].id));
    let mut first = source.clone();
    first.elapsed = 0.0;
    first.covers[0].rotation = Quat4::IDENTITY;
    let mut last = source.clone();
    last.elapsed = 0.05;
    last.covers[0].rotation = Quat4 {
        x: 0.0,
        y: 1.0,
        z: 0.0,
        w: 0.0,
    };
    let mut timeline = RenderTimeline::default();
    timeline.reset(first);
    timeline.push(last);
    let rotation = read(&mut timeline, 0.025, 0.05, 1.0 / 60.0).covers[0].rotation;
    assert!((rotation.y - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-10);
    assert!((rotation.w - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-10);
}

#[test]
fn delayed_display_keeps_a_remote_tank_alive_until_the_display_clock_reaches_its_death() {
    let mut simulation = Simulation::with_seed(4242.0);
    simulation.start();
    let first = simulation.render_state(None);
    let victim = simulation
        .tanks
        .iter()
        .position(|tank| !tank.human)
        .unwrap();
    let victim_id = simulation.tanks[victim].id;
    simulation.tanks[victim].protection = 0.0;
    let human = simulation.human().clone();
    simulation.damage_tank(victim, 10000.0, human.id, human.team, None, None);
    simulation.elapsed = 0.05;
    let mut timeline = RenderTimeline::default();
    timeline.reset(first);
    timeline.push(simulation.render_state(None));
    let alive = |state: &RenderState| {
        state
            .tanks
            .iter()
            .find(|tank| tank.id == victim_id)
            .unwrap()
            .alive
    };
    assert!(alive(&read(&mut timeline, 0.025, 0.05, 1.0 / 60.0)));
    assert!(!alive(&read(&mut timeline, 0.05, 0.05, 1.0 / 60.0)));
}

#[test]
fn local_extrapolation_is_bounded_and_resets_across_tank_lives() {
    let mut simulation = Simulation::with_seed(4242.0);
    let human = simulation.human_index().unwrap();
    set_linvel(&mut simulation, human, 10.0, 0.0, 0.0);
    let first = simulation.render_state(None);
    let mut timeline = RenderTimeline::default();
    timeline.reset(first.clone());
    let ahead = read(&mut timeline, 0.0, 5.0, 1.0 / 60.0)
        .viewer()
        .unwrap()
        .position
        .x
        - first.viewer().unwrap().position.x;
    assert!(
        (ahead - 1.0).abs() < 1e-6,
        "0.1 s of extrapolation at 10 m/s"
    );
    simulation.tanks[human].life += 1;
    set_translation(&mut simulation, human, 0.0, 0.65, 0.0);
    set_linvel(&mut simulation, human, 0.0, 0.0, 0.0);
    simulation.elapsed = 0.05;
    timeline.push(simulation.render_state(None));
    assert_eq!(
        read(&mut timeline, 0.05, 0.05, 1.0 / 60.0)
            .viewer()
            .unwrap()
            .position
            .x,
        0.0
    );
}

// ---- Projectile paths: what a client draws against what the host simulated -------------

/// A recorded room: the host's sweeps and events, and what its client drew.
#[derive(Default)]
struct Replay {
    sweeps: Vec<Sweep>,
    /// Every shell path entry the client received, in order.
    entries: Vec<PathEntry>,
    /// Every event the client received, at its tick.
    events: Vec<TimedEvent>,
    /// Display reads: the display tick and the shells drawn at it.
    reads: Vec<(f64, Vec<RenderShot>)>,
    ticks: u64,
}

/// Two tanks on a cleared arena, neither driven by a bot: the shooter at the origin
/// aiming along +z and a target 30 m ahead.
fn range() -> Simulation {
    let mut sim = Simulation::with_seed(123.0);
    support::clear_arena(&mut sim, &[0, 1]);
    for (index, z) in [(0, 0.0), (1, 30.0)] {
        sim.tanks[index].human = true;
        sim.tanks[index].protection = 0.0;
        sim.tanks[index].aim = 0.0;
        support::place_tank(&mut sim, index, 0.0, z, Some(0.0));
    }
    sim.world.step();
    sim.start();
    sim
}

/// An indestructible rock centred at (`x`, `z`): the quarry boulder's octagonal outline
/// stretched to `w` by `d`, so its long faces span only the middle of its length and two
/// rocks laid end to end leave a gap between their chamfered ends. Walls of rocks overlap.
fn rock(sim: &mut Simulation, x: f64, z: f64, w: f64, d: f64) {
    sim.add_cover(&CoverDef::new(
        CoverKind::Rock,
        x,
        z,
        w,
        d,
        3.0,
        1e6,
        0x777777,
    ));
}

/// Fires the shooter's `weapon` at `aim` radians.
fn fire(sim: &mut Simulation, weapon: Weapon, aim: f64) {
    fire_from(sim, 0, weapon, aim);
}

/// Plays `ticks` of `sim` as a room does: the host follows every projectile sweep with its
/// path recorder and sends a frame every third tick, which goes through the binary wire and
/// the client's mirror into its display timeline, read every 4 ms as frames arrive every
/// 50 ms. `before` runs ahead of each tick, to fire.
fn replay(mut sim: Simulation, ticks: u64, mut before: impl FnMut(&mut Simulation, u64)) -> Replay {
    let viewer = sim.tanks[0].id;
    sim.projectile_moves = Some(Vec::new());
    let mut recorder = ShotPathRecorder::default();
    let mut stream = StateStream::new("room", 1);
    let mut mirror = StateMirror::default();
    let full = stream.full(0, 0, recorder.paths(), || Scene::capture(&sim));
    mirror.apply_full(&baseline(&full), "room", 1).unwrap();
    let mut timeline = NetworkTimeline::default();
    timeline.reset(&mirror.render(viewer).unwrap(), 0, 0.0, &mirror.shots);
    let mut replay = Replay {
        ticks,
        ..Replay::default()
    };
    let mut events = Vec::new();
    let mut event_id = 0;
    let mut scene = Scene::default();
    let mut now = 0.0;
    for tick in 1..=ticks {
        before(&mut sim, tick);
        sim.step(VehicleCommand::idle(), false);
        let moves = sim.projectile_moves.as_ref().unwrap();
        replay.sweeps.extend(Sweep::record(moves, tick));
        recorder.follow(&mut sim, tick);
        for event in sim.events.drain(..) {
            event_id += 1;
            events.push(TimedEvent {
                event_id,
                tick: tick as f64,
                event,
            });
        }
        if tick % 3 != 0 {
            continue;
        }
        scene.capture_from(&sim);
        let frame = stream.snapshot(&mut scene, tick, &events, recorder.entries());
        events.clear();
        recorder.clear_entries();
        let extras = net_support::apply_batch(
            &mut mirror,
            &net_support::batch(1, 0, stream.seq, &[(tick, frame)]),
        )
        .pop()
        .expect("a valid frame");
        replay.entries.extend(extras.paths.iter().copied());
        replay.events.extend(extras.events.iter().cloned());
        timeline
            .push(
                mirror.render(viewer).unwrap(),
                tick,
                extras.events,
                extras.paths,
            )
            .unwrap();
        timeline.arrive(now);
        let mut shown = RenderState::default();
        for _ in 0..12 {
            timeline.read(now, 0.0, 0.004, &mut shown);
            replay
                .reads
                .push((timeline.display_tick(), shown.shots.clone()));
            now += 50.0 / 12.0;
        }
    }
    replay
}

/// Checks the replay's display reads against its sweeps, every read drawing each flying
/// shell. Returns how many drawn shells were compared and the largest distance seen.
fn assert_drawn_where_simulated(replay: &Replay) -> (usize, f64) {
    net_support::assert_drawn_where_simulated(
        &replay.sweeps,
        &replay.reads,
        replay.ticks as f64,
        f64::NEG_INFINITY,
    )
}

/// Each shell's path ends where its last sweep did, and an impact effect there arrives
/// within a tick: the explosion lines up with the shell's last drawn position.
fn assert_impacts_meet_the_drawn_shell(replay: &Replay) -> usize {
    let impacts = events_of(replay, SimEventType::Impact);
    let mut met = 0;
    for end in drawn_ends(replay) {
        let (id, tick, drawn) = (end.id, end.tick, end.at);
        let last = replay.sweeps.iter().rfind(|sweep| sweep.id == id).unwrap();
        assert!(
            (drawn.x - last.to.x).hypot(drawn.z - last.to.z) <= PATH_TOLERANCE + WIRE_SLACK,
            "shell {id} vanishes where it stopped"
        );
        let impact = impacts.iter().find(|impact| {
            let (x, z) = (impact.event.x, impact.event.z);
            (tick..tick + 1.0).contains(&impact.tick) && (x - last.to.x).hypot(z - last.to.z) < 1e-3
        });
        if let Some(impact) = impact {
            let (x, z) = (impact.event.x, impact.event.z);
            assert!(
                (x - drawn.x).hypot(z - drawn.z) <= PATH_TOLERANCE + WIRE_SLACK,
                "shell {id}'s impact shows where it was last drawn"
            );
            met += 1;
        }
    }
    met
}

/// A shell's path end as the client drew it: the end tick and the last drawn position.
struct DrawnEnd {
    id: u32,
    tick: f64,
    at: Vec2,
}

fn drawn_ends(replay: &Replay) -> Vec<DrawnEnd> {
    let mut paths: Vec<ShotPath> = Vec::new();
    let mut ends = Vec::new();
    for entry in &replay.entries {
        match *entry {
            PathEntry::Launch(path) | PathEntry::Change(path) => {
                paths.retain(|known| known.id != path.id);
                paths.push(path);
            }
            PathEntry::End { id, tick } => {
                let drawn = paths.iter().find(|path| path.id == id).unwrap().at(tick);
                ends.push(DrawnEnd {
                    id,
                    tick,
                    at: Vec2::new(drawn.x, drawn.z),
                });
            }
        }
    }
    ends
}

/// The shell the host destroyed during tick `tick` within `reach` of (`x`, `z`): its path
/// ends in that tick, where the shell was last drawn, and no display read draws it after.
fn assert_destroyed_at(replay: &Replay, tick: f64, x: f64, z: f64, reach: f64) -> u32 {
    let end = drawn_ends(replay)
        .into_iter()
        .find(|end| {
            tick - 1.0 < end.tick
                && end.tick <= tick + 1e-3
                && (end.at.x - x).hypot(end.at.z - z) <= reach + PATH_TOLERANCE + WIRE_SLACK
        })
        .unwrap_or_else(|| panic!("no shell path ends at ({x}, {z}) in tick {tick}"));
    for (read, shots) in &replay.reads {
        if *read > end.tick + 1e-3 {
            assert!(
                shots.iter().all(|shot| shot.id != end.id),
                "shell {} is drawn at tick {read} after it ended at {}",
                end.id,
                end.tick
            );
        }
    }
    end.id
}

/// Each bounce's ricochet effect is where the drawn shell turns: a new path for the shell
/// starts there during the bounce's tick. Returns how many bounces were checked.
fn assert_bounces_turn_the_drawn_shell(replay: &Replay) -> usize {
    let bounces = events_of(replay, SimEventType::Ricochet);
    for bounce in &bounces {
        let (tick, x, z) = (bounce.tick, bounce.event.x, bounce.event.z);
        assert!(
            replay.entries.iter().any(|entry| match entry {
                PathEntry::Change(path) =>
                    tick - 1.0 <= path.tick
                        && path.tick <= tick + 1e-3
                        && (path.x - x).hypot(path.z - z) <= PATH_TOLERANCE + WIRE_SLACK,
                _ => false,
            }),
            "no drawn shell turns at the bounce at ({x}, {z}) in tick {tick}"
        );
    }
    bounces.len()
}

/// Received events of one kind.
fn events_of(replay: &Replay, kind: SimEventType) -> Vec<&TimedEvent> {
    replay
        .events
        .iter()
        .filter(|timed| timed.event.kind == kind)
        .collect()
}

fn launches(replay: &Replay) -> usize {
    replay
        .entries
        .iter()
        .filter(|entry| matches!(entry, PathEntry::Launch(_)))
        .count()
}

fn changes(replay: &Replay) -> usize {
    replay
        .entries
        .iter()
        .filter(|entry| matches!(entry, PathEntry::Change(_)))
        .count()
}

#[test]
fn standard_shells_are_launched_once_and_drawn_on_their_simulated_flight_until_impact() {
    let mut sim = range();
    rock(&mut sim, 12.0, 20.0, 4.0, 2.0);
    let replay = replay(sim, 150, |sim, tick| match tick {
        1 => fire(sim, Weapon::Standard, 0.0),
        30 => fire(sim, Weapon::Standard, 0.54),
        60 => fire(sim, Weapon::Standard, 0.9),
        _ => {}
    });
    let (compared, worst) = assert_drawn_where_simulated(&replay);
    assert!(compared > 100, "compared {compared} drawn shells");
    assert!(
        worst < 0.01,
        "straight flight is exact on the wire: {worst} m"
    );
    assert_eq!(launches(&replay), 3);
    assert_eq!(changes(&replay), 0, "straight flight needs no new paths");
    assert!(
        assert_impacts_meet_the_drawn_shell(&replay) >= 2,
        "the tank and the rock are hit"
    );
}

#[test]
fn ricochet_shells_start_a_path_at_each_bounce_and_stay_on_the_simulated_flight() {
    let mut sim = range();
    // A corridor along +z whose walls the shell bounces between until its bounces run out
    // and the next wall stops it; the target waits beside it. Each wall is two overlapping
    // rocks: one long rock's chamfered ends would let the first bounce miss.
    support::place_tank(&mut sim, 1, 20.0, 30.0, Some(0.0));
    for z in [14.0, 30.0] {
        rock(&mut sim, -4.0, z, 1.0, 20.0);
        rock(&mut sim, 4.0, z, 1.0, 20.0);
    }
    let replay = replay(sim, 150, |sim, tick| {
        if tick == 1 {
            fire(sim, Weapon::Ricochet, 0.6);
        }
    });
    let bounces = replay
        .sweeps
        .windows(2)
        .filter(|pair| {
            let [a, b] = pair else { unreachable!() };
            a.id == b.id && {
                let (ax, az) = (a.to.x - a.from.x, a.to.z - a.from.z);
                let (bx, bz) = (b.to.x - b.from.x, b.to.z - b.from.z);
                ax * bx < 0.0 && az * bz >= 0.0
            }
        })
        .count();
    assert_eq!(bounces, 3, "the shell bounced {bounces} times");
    assert!(changes(&replay) >= 3, "each bounce starts a path");
    let (compared, worst) = assert_drawn_where_simulated(&replay);
    assert!(compared > 50, "compared {compared} drawn shells");
    assert!(worst <= PATH_TOLERANCE + WIRE_SLACK);
    let turns = assert_bounces_turn_the_drawn_shell(&replay);
    assert_eq!(turns, 3, "every bounce turns the drawn shell");
    assert_eq!(
        assert_impacts_meet_the_drawn_shell(&replay),
        1,
        "the shell's last wall hit meets its drawn shell"
    );
}

#[test]
fn accelerating_rockets_and_a_steering_missile_follow_their_simulated_flight() {
    let mut sim = range();
    // The target moves off the launch line, so the missile has to turn.
    let target = sim.tanks[1].clone();
    support::place_tank(&mut sim, 1, 18.0, 30.0, Some(0.0));
    let replay = replay(sim, 150, |sim, tick| {
        if tick == 1 {
            fire(sim, Weapon::Rocket, -0.2);
        }
        if tick == 2 {
            let id = sim.next_id;
            sim.next_id += 1;
            sim.shots.push(Shot {
                id,
                x: -8.0,
                z: 4.0,
                y: Some(1.0),
                vx: 0.0,
                vz: weapon(Weapon::Tow).speed,
                team: target.team.opponent(),
                owner: 999,
                weapon: Weapon::Tow,
                damage: weapon(Weapon::Tow).damage,
                life: 4.0,
                target_id: Some(target.id),
                target_life: Some(target.life),
                ..Shot::default()
            });
        }
    });
    let (compared, worst) = assert_drawn_where_simulated(&replay);
    assert!(compared > 100, "compared {compared} drawn shells");
    assert!(worst <= PATH_TOLERANCE + WIRE_SLACK);
    let rocket = replay.sweeps[0].id;
    let rocket_paths = replay
        .entries
        .iter()
        .filter(|entry| entry.id() == rocket && !matches!(entry, PathEntry::End { .. }))
        .count();
    assert_eq!(rocket_paths, 1, "a rocket's speed-up is part of its path");
    let missile_paths = replay
        .entries
        .iter()
        .filter(|entry| entry.id() != rocket && !matches!(entry, PathEntry::End { .. }))
        .count();
    let missile_ticks = replay
        .sweeps
        .iter()
        .filter(|sweep| sweep.id != rocket)
        .count();
    assert!(
        missile_paths > 1 && missile_paths * 2 < missile_ticks,
        "a turning missile sends {missile_paths} paths for {missile_ticks} sweeps"
    );
    assert!(assert_impacts_meet_the_drawn_shell(&replay) >= 1);
}

#[test]
fn spread_pellets_each_follow_their_own_simulated_flight() {
    let mut sim = range();
    rock(&mut sim, 0.0, 12.0, 6.0, 1.0);
    let replay = replay(sim, 90, |sim, tick| {
        if tick == 1 {
            fire(sim, Weapon::Spread, 0.0);
        }
        if tick == 40 {
            fire(sim, Weapon::Spread, 1.2);
        }
    });
    assert_eq!(launches(&replay), 6);
    assert_eq!(changes(&replay), 0);
    let (compared, worst) = assert_drawn_where_simulated(&replay);
    assert!(compared > 100, "compared {compared} drawn shells");
    assert!(worst < 0.01);
    assert!(assert_impacts_meet_the_drawn_shell(&replay) >= 3);
}

#[test]
fn shells_that_intercept_each_other_stop_drawing_where_they_met() {
    let sim = range();
    assert_ne!(sim.tanks[0].team, sim.tanks[1].team);
    let replay = replay(sim, 60, |sim, tick| {
        if tick == 1 {
            fire_from(sim, 0, Weapon::Standard, 0.0);
            fire_from(sim, 1, Weapon::Standard, PI);
        }
    });
    let flashes = events_of(&replay, SimEventType::Explosion);
    assert_eq!(flashes.len(), 1, "the shells met once");
    let flash = &flashes[0];
    let ends = drawn_ends(&replay);
    assert_eq!(ends.len(), 2, "both shells ended");
    // The flash is at the midpoint of the two shells as they were last drawn.
    let reach = INTERCEPTION_RADIUS / 2.0;
    let a = assert_destroyed_at(&replay, flash.tick, flash.event.x, flash.event.z, reach);
    let b = ends.iter().find(|end| end.id != a).unwrap();
    assert_destroyed_at(&replay, flash.tick, b.at.x, b.at.z, 0.0);
    let middle = Vec2::new(
        (ends[0].at.x + ends[1].at.x) / 2.0,
        (ends[0].at.z + ends[1].at.z) / 2.0,
    );
    assert!(
        (middle.x - flash.event.x).hypot(middle.z - flash.event.z) <= PATH_TOLERANCE + WIRE_SLACK,
        "the interception flashes between the drawn shells"
    );
    let (compared, worst) = assert_drawn_where_simulated(&replay);
    assert!(compared > 20, "compared {compared} drawn shells");
    assert!(worst < 0.01);
    assert!(
        events_of(&replay, SimEventType::Impact).is_empty(),
        "neither tank is hit"
    );
}

#[test]
fn a_shell_zapped_by_a_laser_defense_stops_drawing_where_the_beam_met_it() {
    let mut sim = range();
    sim.tanks[1].laser = LASER_DEFENSE.duration;
    let replay = replay(sim, 150, |sim, tick| {
        if tick % 30 == 1 && tick < 100 {
            fire(sim, Weapon::Standard, 0.0);
        }
    });
    let zaps = events_of(&replay, SimEventType::Laser);
    assert!(!zaps.is_empty(), "the defense zapped a shell");
    for zap in &zaps {
        assert_destroyed_at(&replay, zap.tick, zap.event.x, zap.event.z, 0.0);
        assert!(
            zap.event.z < 30.0 - 2.0,
            "the beam met the shell short of the hull"
        );
    }
    assert_eq!(drawn_ends(&replay).len(), 4, "every shell ended");
    let (compared, worst) = assert_drawn_where_simulated(&replay);
    assert!(compared > 50, "compared {compared} drawn shells");
    assert!(worst < 0.01);
}

#[test]
fn a_shell_that_sets_off_a_mine_stops_drawing_at_the_mine() {
    let mut sim = range();
    let id = sim.next_id;
    sim.next_id += 1;
    let (owner, team) = (sim.tanks[1].id, sim.tanks[1].team);
    sim.mines.push(Mine {
        id,
        owner,
        owner_life: None,
        damage: None,
        team,
        x: 0.0,
        z: 15.0,
        arm: 0.0,
        life: 25.0,
    });
    let replay = replay(sim, 60, |sim, tick| {
        if tick == 1 {
            fire(sim, Weapon::Standard, 0.0);
        }
    });
    let blasts = events_of(&replay, SimEventType::Explosion);
    assert_eq!(blasts.len(), 1, "the mine went off once");
    let blast = &blasts[0];
    assert!((blast.event.x - 0.0).hypot(blast.event.z - 15.0) < 1e-6);
    assert_destroyed_at(
        &replay,
        blast.tick,
        0.0,
        15.0,
        MINE_RADIUS + SHELL_HIT_RADIUS,
    );
    assert_eq!(drawn_ends(&replay).len(), 1);
    let (compared, worst) = assert_drawn_where_simulated(&replay);
    assert!(compared > 10, "compared {compared} drawn shells");
    assert!(worst < 0.01);
}
