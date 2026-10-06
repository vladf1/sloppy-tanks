//! Clocks and display timing: the host's fixed-step clock and the dev transport delay
//! (`tests/network-clock.test.ts`), input cadence (`input-cadence.test.ts`), the adaptive
//! playout clock (`playout-clock.test.ts`) and pose/event timelines
//! (`network-timeline.test.ts`).

mod net_support;

use net_support::{set_linvel, set_translation};
use sloppy_core::net::fixed_step_clock::FixedStepClock;
use sloppy_core::net::input_cadence::{InputCadence, InputSample};
use sloppy_core::net::multiplayer_simulation::{MultiplayerOptions, create_multiplayer_simulation};
use sloppy_core::net::network_timeline::NetworkTimeline;
use sloppy_core::net::player_controls::{Action, Aim};
use sloppy_core::net::playout_clock::PlayoutClock;
use sloppy_core::net::render_timeline::RenderTimeline;
use sloppy_core::net::replication::{ShotTrace, TimedEvent};
use sloppy_core::net::transport_delay::DelayedChannel;
use sloppy_core::sim::Simulation;
use sloppy_core::sim::math::{Point3, Quat4, Vec2};
use sloppy_core::sim::render_state::{RenderFragment, RenderShot, RenderState};
use sloppy_core::sim::timber_layout::{
    TimberFace, TimberHit, TimberMark, TimberPart, TimberPartKind,
};
use sloppy_core::sim::types::{SimEvent, SimEventType, Team, Weapon};

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

fn sample() -> InputSample {
    InputSample {
        control_epoch: 1,
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
            InputSample {
                move_x: 1.0,
                ..sample()
            },
            120,
        ),
        (
            InputSample {
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
        InputSample {
            aim: Aim::Point { x: 11.0, z: 20.0 },
            ..sample()
        },
        InputSample {
            actions: vec![Action::Mine],
            ..sample()
        },
        InputSample {
            actions: vec![Action::Ammo(Weapon::Rocket)],
            ..sample()
        },
        InputSample {
            control_epoch: 2,
            ..sample()
        },
        InputSample {
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
        InputSample {
            move_x: 1.0,
            ..sample()
        },
        InputSample {
            move_z: -1.0,
            ..sample()
        },
        InputSample {
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
fn sub_centimetre_camera_noise_does_not_flood_idle_traffic_but_cumulative_aim_changes_are_sent() {
    let mut cadence = InputCadence::default();
    cadence.sent(&sample(), 0.0);
    let aimed = |x: f64| InputSample {
        aim: Aim::Point { x, z: 20.0 },
        ..sample()
    };
    assert!(!cadence.due(&aimed(10.005), 100.0));
    assert!(cadence.due(&aimed(10.02), 150.0));
    let angle = |angle: f64| InputSample {
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
    assert_eq!(
        frames
            .iter()
            .filter(|frame| frame.display_ms > frame.newest_ms)
            .count(),
        0
    );
    assert!(
        frames
            .iter()
            .map(|frame| frame.buffer_ms)
            .fold(0.0, f64::max)
            < 110.0
    );
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
    let starved = frames
        .iter()
        .filter(|frame| frame.display_ms > frame.newest_ms)
        .count();
    assert!(
        starved > 0 && starved as f64 * FRAME_MS < 200.0,
        "only part of the stall shows"
    );
    let peak = frames
        .iter()
        .map(|frame| frame.buffer_ms)
        .fold(0.0, f64::max);
    assert!(peak > 100.0, "the stall grows the buffer");
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
    let settled: Vec<&Frame> = frames
        .iter()
        .filter(|frame| frame.now_ms > 4000.0)
        .collect();
    assert_eq!(
        settled
            .iter()
            .filter(|frame| frame.display_ms > frame.newest_ms)
            .count(),
        0,
        "motion no longer pauses once the stall pattern is learned"
    );
    assert!(
        settled
            .iter()
            .map(|frame| frame.buffer_ms)
            .fold(0.0, f64::max)
            <= 250.0
    );
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
    timeline.reset(&first, 0, 0.0);
    timeline
        .push(
            &dead,
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
            &respawn,
            4,
            vec![TimedEvent {
                event_id: 2,
                tick: 4.0,
                event: event(SimEventType::Respawn, Some(viewer), 30.0),
            }],
            Vec::new(),
        )
        .unwrap();
    timeline.push(&respawn, 6, Vec::new(), Vec::new()).unwrap();
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
    timeline.reset(&respawn, 6, 400.0);
    assert!(timeline.read(410.0, 0.0, 1.0 / 60.0, &mut state).is_empty());
}

#[test]
fn a_projectile_born_and_destroyed_between_snapshots_follows_its_segment_and_disappears_at_impact()
{
    let sim = empty_room();
    let state = sim.render_state(Some(sim.tanks[0].id));
    let shot = RenderShot {
        id: 999,
        x: 0.0,
        z: 0.0,
        y: Some(1.0),
        visual_y: None,
        vx: 120.0,
        vz: 0.0,
        weapon: Weapon::Standard,
        team: Team::Blue,
    };
    let mut timeline = NetworkTimeline::default();
    timeline.reset(&state, 0, 0.0);
    timeline
        .push(
            &state,
            6,
            vec![TimedEvent {
                event_id: 1,
                tick: 1.5,
                event: SimEvent::at(SimEventType::Impact, 1.0, 0.0),
            }],
            vec![ShotTrace {
                tick: 1.0,
                end_tick: 1.5,
                shot,
                end: Vec2::new(1.0, 0.0),
            }],
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
                "shot follows its segment"
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
    timeline.reset(&first, 0, 0.0);
    timeline.push(&moved, 3, Vec::new(), Vec::new()).unwrap();
    timeline.push(&later, 6, Vec::new(), Vec::new()).unwrap();
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
