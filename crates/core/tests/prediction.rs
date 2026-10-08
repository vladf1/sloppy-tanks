//! Own-hull prediction against the full host simulation. The host runs first and records
//! what a client would receive; then one predictor replays it the way the client does,
//! restarting from the host's hull at every snapshot (through its wire form and the
//! client's scene mirror) and driving the same commands ahead of it.
//!
//! A replay that continues from the predictor's own state is bit-exact. A restart lands
//! on a tick the predictor has already passed, so its contacts warm-start from a later
//! tick than the host's did: free driving and turning stay within 2 mm, and the moment
//! of hitting or turning against a wall within 3 cm.

mod support;

use std::collections::BTreeMap;

use sloppy_core::net::multiplayer_simulation::{MultiplayerOptions, create_multiplayer_simulation};
use sloppy_core::net::prediction::{HullState, TankPredictor};
use sloppy_core::net::scene_codec::{MirrorScene, Scene};
use sloppy_core::net::schema::parse_record;
use sloppy_core::net::wire::WireReader;
use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::math::{Vec2, angle_delta, distance};
use sloppy_core::sim::types::{
    CoverKind, PlayerAssignment, SimEventType, Team, VehicleCommand, VehicleKind,
};
use sloppy_core::sim::{RenderState, Simulation};
use support::{clear_arena, place_tank, tank_index};

/// Snapshot batches carry every third tick.
const SNAPSHOT_TICKS: usize = 3;
/// Ticks the client predicts past a snapshot at 100 ms and 200 ms round trips, about
/// the round trip plus one batch.
const HORIZON_100_MS: usize = 9;
const HORIZON_200_MS: usize = 15;
/// Prediction errors below this are invisible; above it a window counts as diverged.
const VISIBLE_ERROR: f64 = 0.01;

/// One tick of the host as the viewer's client learns it.
struct HostTick {
    input: (f64, f64),
    hull: Option<HullState>,
    heading: f64,
    /// The scene after this tick, kept at snapshot ticks.
    scene: Option<RenderState>,
    /// A blast near the viewer, or a hit on it, this tick.
    blast: bool,
    nearest_tank: f64,
}

fn player(id: &str, team: Team, slot: usize, kind: VehicleKind) -> PlayerAssignment {
    PlayerAssignment {
        player_id: id.into(),
        name: id.into(),
        team,
        slot,
        kind,
    }
}

/// A started room of `players`; `humans_only` leaves out the fill bots.
fn room(map: MapId, players: &[PlayerAssignment], humans_only: bool) -> Simulation {
    let mut sim = create_multiplayer_simulation(
        4242.0,
        players,
        MultiplayerOptions {
            map_mode: Some(map),
            humans_only: Some(humans_only),
            ..MultiplayerOptions::default()
        },
    )
    .unwrap();
    sim.start();
    sim
}

fn human_id(sim: &Simulation, name: &str) -> u32 {
    sim.tanks
        .iter()
        .find(|tank| tank.player_id.as_deref() == Some(name))
        .unwrap()
        .id
}

/// The scene through the wire and the client's mirror.
fn client_scene(sim: &Simulation, viewer: u32) -> RenderState {
    let mut bytes = Vec::new();
    Scene::capture(sim).write(&mut bytes);
    MirrorScene::read(&mut WireReader::new(&bytes))
        .unwrap()
        .render(viewer)
        .unwrap()
}

/// The viewer's hull through its wire form, which must keep every bit.
fn client_hull(sim: &Simulation, viewer: u32, tick: u64) -> Option<HullState> {
    let hull = HullState::capture(sim, tank_index(sim, viewer), tick)?;
    let mut text = String::new();
    hull.write(&mut text);
    let read = HullState::read(&parse_record(&text).unwrap()).unwrap();
    assert_eq!(read, hull, "the wire form keeps every bit");
    Some(read)
}

type Script = dyn Fn(u64) -> (f64, f64);

fn command((move_x, move_z): (f64, f64)) -> VehicleCommand {
    let mut command = VehicleCommand::idle();
    command.move_x = move_x;
    command.move_z = move_z;
    command
}

/// Steps the host with `commands` and records the tick as the viewer's client sees it.
fn record(
    sim: &mut Simulation,
    viewer: u32,
    tick: u64,
    commands: &BTreeMap<u32, VehicleCommand>,
) -> HostTick {
    sim.step_with(commands);
    let index = tank_index(sim, viewer);
    let tank = &sim.tanks[index];
    let at = sim.tank_position(tank).planar();
    let blast = sim.events.drain(..).any(|event| match event.kind {
        SimEventType::Explosion => distance(Vec2::new(event.x, event.z), at) < 8.0,
        SimEventType::Hurt | SimEventType::Death => event.id == Some(viewer),
        _ => false,
    });
    let tank = &sim.tanks[index];
    let nearest_tank = sim
        .tanks
        .iter()
        .filter(|other| other.id != viewer && other.alive)
        .map(|other| distance(sim.tank_position(other).planar(), at))
        .fold(f64::INFINITY, f64::min);
    HostTick {
        input: (commands[&viewer].move_x, commands[&viewer].move_z),
        hull: client_hull(sim, viewer, tick),
        heading: tank.heading,
        scene: (tick as usize)
            .is_multiple_of(SNAPSHOT_TICKS)
            .then(|| client_scene(sim, viewer)),
        blast,
        nearest_tank,
    }
}

/// Runs the host for `ticks` with the viewer following `script`.
fn host_run(sim: &mut Simulation, viewer: u32, script: &Script, ticks: u64) -> Vec<HostTick> {
    let mut run = vec![HostTick {
        input: (0.0, 0.0),
        hull: client_hull(sim, viewer, 0),
        heading: sim.tanks[tank_index(sim, viewer)].heading,
        scene: Some(client_scene(sim, viewer)),
        blast: false,
        nearest_tank: f64::INFINITY,
    }];
    for tick in 1..=ticks {
        let commands = BTreeMap::from([(viewer, command(script(tick)))]);
        run.push(record(sim, viewer, tick, &commands));
    }
    run
}

/// The worst divergence of one prediction window, and what the host saw meanwhile.
#[derive(Clone, Copy, Debug)]
struct Window {
    distance: f64,
    heading: f64,
    blast: bool,
    /// Closest any other tank came to the viewer.
    nearest_tank: f64,
}

/// Replays `run` like a client: one predictor, restarted at every snapshot and driven
/// `horizon` ticks ahead. Windows that cross a death or respawn are left out.
fn client_replays(run: &[HostTick], horizon: usize) -> Vec<Window> {
    let mut predictor = TankPredictor::new();
    let mut windows = Vec::new();
    for start in (0..run.len() - horizon).step_by(SNAPSHOT_TICKS) {
        let (Some(scene), Some(hull)) = (&run[start].scene, &run[start].hull) else {
            continue;
        };
        let ahead = &run[start + 1..=start + horizon];
        if ahead
            .iter()
            .any(|tick| tick.hull.is_none_or(|next| next.life != hull.life))
        {
            continue;
        }
        predictor.sync_scene(scene);
        predictor.reset(hull);
        let mut window = Window {
            distance: 0.0,
            heading: 0.0,
            blast: false,
            nearest_tank: f64::INFINITY,
        };
        for tick in ahead {
            predictor.step(tick.input.0, tick.input.1);
            let pose = predictor.pose().unwrap();
            let host = tick.hull.unwrap().position.planar();
            window.distance = window.distance.max(distance(pose.position.planar(), host));
            window.heading = window
                .heading
                .max(angle_delta(pose.heading, tick.heading).abs());
            window.blast |= tick.blast;
            window.nearest_tank = window.nearest_tank.min(tick.nearest_tank);
        }
        windows.push(window);
    }
    windows
}

fn worst(windows: &[Window]) -> (f64, f64) {
    windows
        .iter()
        .fold((0.0, 0.0), |(distance, heading), window| {
            (distance.max(window.distance), heading.max(window.heading))
        })
}

fn percentile(values: &mut [f64], fraction: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * fraction).round() as usize]
}

/// Open floor at the centre: the viewer alone, every cover removed.
fn open_field(kind: VehicleKind) -> (Simulation, u32) {
    let mut sim = room(
        MapId::Village,
        &[player("alice", Team::Blue, 0, kind)],
        true,
    );
    let viewer = human_id(&sim, "alice");
    let index = tank_index(&sim, viewer);
    clear_arena(&mut sim, &[index]);
    place_tank(&mut sim, 0, 0.0, 0.0, Some(0.0));
    sim.world.step();
    (sim, viewer)
}

/// Forward, a held turn, a stop, reverse, a diagonal and turns in place, switching every
/// 20 to 45 ticks so snapshots land mid-acceleration, mid-turn and at rest.
fn driving(tick: u64) -> (f64, f64) {
    const PATTERN: [(f64, f64, u64); 8] = [
        (0.0, 1.0, 40),
        (1.0, 0.0, 31),
        (0.0, 0.0, 20),
        (0.0, -1.0, 35),
        (-0.7071, 0.7071, 44),
        (0.0, 0.0, 25),
        (-1.0, 0.0, 29),
        (0.0, 1.0, 25),
    ];
    let period: u64 = PATTERN.iter().map(|(_, _, ticks)| ticks).sum();
    let mut at = tick % period;
    for (move_x, move_z, ticks) in PATTERN {
        if at < ticks {
            return (move_x, move_z);
        }
        at -= ticks;
    }
    unreachable!()
}

/// A wandering route that keeps meeting houses, trees and walls.
fn wandering(tick: u64) -> (f64, f64) {
    let angle = (tick / 50) as f64 * 2.3;
    (angle.sin(), angle.cos())
}

#[test]
fn open_field_driving_and_turning_replay_exactly() {
    for kind in VehicleKind::PLAYABLE {
        let (mut sim, viewer) = open_field(kind);
        let run = host_run(&mut sim, viewer, &driving, 900);
        let (distance, heading) = worst(&client_replays(&run, HORIZON_200_MS));
        eprintln!("open field {kind:?}: {distance} m, {heading} rad");
        assert!(distance < 2e-3, "{kind:?} diverged {distance} m");
        assert_eq!(heading, 0.0, "{kind:?}");
    }
}

/// Grinding into a wall head on, then sliding along it at an angle and backing off.
#[test]
fn driving_into_and_along_a_wall_replays_within_three_centimetres() {
    let (mut sim, viewer) = open_field(VehicleKind::Balanced);
    sim.add_cover(&CoverDef {
        kind: CoverKind::Concrete,
        x: 0.0,
        z: 8.0,
        w: 12.0,
        d: 1.0,
        h: 2.0,
        hp: f64::INFINITY,
        color: 0x888888,
        timber_join: None,
        timber_bays: None,
        debris_seed: None,
    });
    sim.world.step();
    let script = |tick: u64| match tick {
        0..120 => (0.0, 1.0),
        120..240 => (0.5, 0.866),
        240..300 => (-0.3, 1.0),
        _ => (0.0, -1.0),
    };
    let run = host_run(&mut sim, viewer, &script, 360);
    let (distance, heading) = worst(&client_replays(&run, HORIZON_200_MS));
    eprintln!("wall: {distance} m, {heading} rad");
    assert!(distance < 0.03, "{distance}");
    assert!(heading < 1e-9, "{heading}");
}

#[test]
fn village_cover_replays_within_two_millimetres() {
    let mut sim = room(
        MapId::Village,
        &[player("alice", Team::Blue, 0, VehicleKind::Scout)],
        true,
    );
    let viewer = human_id(&sim, "alice");
    let run = host_run(&mut sim, viewer, &wandering, 3600);
    let (distance, heading) = worst(&client_replays(&run, HORIZON_200_MS));
    eprintln!("village: {distance} m, {heading} rad");
    assert!(distance < 2e-3, "{distance}");
    assert!(heading < 1e-9, "{heading}");
}

/// The viewer rams a parked tank and pushes it, it drives back, then pivots away. The
/// predictor holds the other tank's replicated input, so pushes replay closely until
/// that input changes.
#[test]
fn pushing_another_tank_replays_until_its_input_changes() {
    let mut sim = room(
        MapId::Village,
        &[
            player("alice", Team::Blue, 0, VehicleKind::Balanced),
            player("bob", Team::Red, 0, VehicleKind::Heavy),
        ],
        true,
    );
    let viewer = human_id(&sim, "alice");
    let bob = human_id(&sim, "bob");
    let (alice_index, bob_index) = (tank_index(&sim, viewer), tank_index(&sim, bob));
    clear_arena(&mut sim, &[alice_index, bob_index]);
    place_tank(&mut sim, 0, 0.0, 0.0, Some(0.0));
    place_tank(&mut sim, 1, 0.0, 8.0, Some(0.0));
    sim.world.step();
    // Bob is parked for two seconds, then drives at alice, then turns away.
    let bob_input = |tick: u64| match tick {
        0..120 => (0.0, 0.0),
        120..200 => (0.0, -1.0),
        _ => (1.0, 0.0),
    };
    let mut run = Vec::new();
    for tick in 0..300 {
        let commands = BTreeMap::from([
            (viewer, command((0.0, 1.0))),
            (bob, command(bob_input(tick))),
        ]);
        run.push(record(&mut sim, viewer, tick, &commands));
    }
    // Bob's input changes at ticks 120 and 200; no client can see them coming.
    let changes = [120, 200];
    for (horizon, label) in [(HORIZON_100_MS, "100 ms"), (HORIZON_200_MS, "200 ms")] {
        let windows = client_replays(&run, horizon);
        let mut steady = Vec::new();
        let mut surprised = Vec::new();
        for (index, window) in windows.iter().enumerate() {
            if window.nearest_tank >= 6.0 {
                continue;
            }
            let start = index * SNAPSHOT_TICKS;
            let span = start + 1..=start + horizon;
            if changes.iter().any(|tick| span.contains(tick)) {
                surprised.push(window.distance);
            } else {
                steady.push(window.distance);
            }
        }
        let steady_max = percentile(&mut steady, 1.0);
        let steady_p95 = percentile(&mut steady, 0.95);
        let surprised_max = percentile(&mut surprised, 1.0);
        eprintln!(
            "bump at {label}: {} windows in contact p95 {steady_p95:.4} m, max {steady_max:.4} m; \
             {} across bob's input changes max {surprised_max:.3} m",
            steady.len(),
            surprised.len()
        );
        // A heavy tank pivoting against the hull is the worst case: chaotic contact
        // amplifies the replicated pose's rounding.
        assert!(steady_p95 < 0.05, "{label}: {steady_p95}");
        assert!(steady_max < 0.3, "{label}: {steady_max}");
        assert!(surprised_max < 1.5, "{label}: {surprised_max}");
    }
}

/// A full room of bots shooting at a wandering viewer: how often prediction misses, by
/// how much, and why.
#[test]
fn battle_divergence_comes_from_blasts_and_other_tanks() {
    let mut sim = room(
        MapId::Village,
        &[player("alice", Team::Blue, 0, VehicleKind::Balanced)],
        false,
    );
    let viewer = human_id(&sim, "alice");
    let run = host_run(&mut sim, viewer, &wandering, 3600);
    for (horizon, label) in [(HORIZON_100_MS, "100 ms"), (HORIZON_200_MS, "200 ms")] {
        let windows = client_replays(&run, horizon);
        let mut errors: Vec<f64> = windows.iter().map(|window| window.distance).collect();
        let p50 = percentile(&mut errors, 0.5);
        let p95 = percentile(&mut errors, 0.95);
        let max = percentile(&mut errors, 1.0);
        let diverged: Vec<&Window> = windows
            .iter()
            .filter(|window| window.distance > VISIBLE_ERROR)
            .collect();
        let blasts = diverged.iter().filter(|window| window.blast).count();
        let tanks = diverged
            .iter()
            .filter(|window| !window.blast && window.nearest_tank < 6.0)
            .count();
        let other = diverged.len() - blasts - tanks;
        eprintln!(
            "battle at {label}: {} windows, p50 {p50:.5} m, p95 {p95:.5} m, max {max:.3} m; \
             {} over {VISIBLE_ERROR} m: {blasts} blasts, {tanks} near tanks, {other} other",
            windows.len(),
            diverged.len()
        );
        // Everything the drive model covers replays exactly; misses need a cause.
        assert!(
            other <= diverged.len() / 4 + 1,
            "{other} unexplained misses"
        );
    }
}

/// What the client cannot foresee, sized: a rocket blast beside the hull, and timber the
/// hull is pushing against breaking under it. Each costs the windows that span it, and
/// the predicted world has no debris, so the hull then drives through the fresh timber
/// fragments the host's hull has to shove aside.
#[test]
fn unseen_blasts_and_breaking_cover_cost_their_windows() {
    let timber = CoverDef {
        kind: CoverKind::Timber,
        x: 0.0,
        z: 6.0,
        w: 8.0,
        d: 0.6,
        h: 2.0,
        hp: 40.0,
        color: 0x8b5a2b,
        timber_join: None,
        timber_bays: None,
        debris_seed: Some(1.0),
    };
    for (event, label, later_bound) in [(60, "rocket blast", 0.05), (150, "timber breaking", 0.3)] {
        let (mut sim, viewer) = open_field(VehicleKind::Balanced);
        sim.add_cover(&timber);
        sim.world.step();
        let mut run = Vec::new();
        for tick in 0..240 {
            if tick == event {
                if label == "rocket blast" {
                    // Beside the hull, from an enemy, so it pushes and hurts.
                    let at = support::tank_xz(&sim, tank_index(&sim, viewer));
                    sim.explode(
                        Vec2::new(at.x + 1.5, at.z),
                        3.0,
                        5.0,
                        999,
                        Team::Red,
                        None,
                        sloppy_core::sim::types::DamageCause::Rocket,
                    );
                } else {
                    let index = sim
                        .covers
                        .iter()
                        .position(|cover| cover.kind == CoverKind::Timber)
                        .unwrap();
                    sim.damage_cover(index, 1000.0, 999, Team::Red, None, None);
                }
            }
            let commands = BTreeMap::from([(viewer, command((0.0, 1.0)))]);
            run.push(record(&mut sim, viewer, tick, &commands));
        }
        for (horizon, name) in [(HORIZON_100_MS, "100 ms"), (HORIZON_200_MS, "200 ms")] {
            let windows = client_replays(&run, horizon);
            let mut spanning = Vec::new();
            let mut others = Vec::new();
            for (index, window) in windows.iter().enumerate() {
                let start = index * SNAPSHOT_TICKS;
                if (start + 1..=start + horizon).contains(&(event as usize + 1)) {
                    spanning.push(window.distance);
                } else {
                    others.push(window.distance);
                }
            }
            let spanning_max = percentile(&mut spanning, 1.0);
            let others_max = percentile(&mut others, 1.0);
            eprintln!(
                "{label} at {name}: {} windows spanning it, max {spanning_max:.3} m; others max {others_max:.4} m",
                spanning.len()
            );
            assert!(
                spanning_max > VISIBLE_ERROR,
                "{label} should be unpredictable"
            );
            assert!(others_max < later_bound, "{label} {name}: {others_max}");
        }
    }
}
