//! Own-shot prediction against the host's firing: a predicted shot launches the shells
//! the host launches, from the same muzzle and with the same flash, and its shells end at
//! the cover the host's would.

use crate::support;

use std::collections::BTreeMap;

use sloppy_core::net::multiplayer_simulation::{MultiplayerOptions, create_multiplayer_simulation};
use sloppy_core::net::own_shots::{Firing, OwnShots, own_clock};
use sloppy_core::net::prediction::{HullState, TankPredictor};
use sloppy_core::net::scene_codec::{MirrorScene, Scene};
use sloppy_core::net::shot_paths::{PathEntry, ShotPath, ShotPathRecorder};
use sloppy_core::net::wire::WireReader;
use sloppy_core::sim::Simulation;
use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::data::STEP;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::math::Point3;
use sloppy_core::sim::render_state::RenderShot;
use sloppy_core::sim::types::{
    AmmoInventory, CoverKind, PlayerAssignment, SimEventType, Team, VehicleCommand, VehicleKind,
    Weapon,
};
use support::{clear_arena, place_tank, tank_index};

/// Alice alone on cleared village ground, at the centre facing +z.
fn alone(kind: VehicleKind) -> (Simulation, u32) {
    let mut sim = create_multiplayer_simulation(
        4242.0,
        &[PlayerAssignment {
            player_id: "alice".into(),
            name: "alice".into(),
            team: Team::Blue,
            slot: 0,
            kind,
        }],
        MultiplayerOptions {
            map_mode: Some(MapId::Village),
            humans_only: Some(true),
            ..MultiplayerOptions::default()
        },
    )
    .unwrap();
    sim.start();
    let viewer = sim.tanks[0].id;
    clear_arena(&mut sim, &[0]);
    place_tank(&mut sim, 0, 0.0, 0.0, Some(0.0));
    sim.world.step();
    sim.projectile_moves = Some(Vec::new());
    (sim, viewer)
}

/// A predictor holding the cover a client of `sim` would hold.
fn predictor(sim: &Simulation, viewer: u32) -> TankPredictor {
    let mut bytes = Vec::new();
    Scene::capture(sim).write(&mut bytes);
    let scene = MirrorScene::read(&mut WireReader::new(&bytes))
        .unwrap()
        .render(viewer)
        .unwrap();
    let mut predictor = TankPredictor::default();
    predictor.sync_scene(&scene);
    // The predicted world's queries see cover once it has stepped with the hull.
    predictor.reset(&HullState::capture(sim, tank_index(sim, viewer), 0).unwrap());
    predictor.step(0.0, 0.0);
    predictor
}

/// The host fires `weapon` at `aim` on tick 1: its launches and its muzzle flash, and the
/// hull position it fired from.
fn host_fires(
    sim: &mut Simulation,
    viewer: u32,
    weapon: Weapon,
    aim: f64,
) -> (Vec<ShotPath>, (f64, f64), Point3) {
    let index = tank_index(sim, viewer);
    let tank = &mut sim.tanks[index];
    tank.ammo = AmmoInventory {
        spread: 9.0,
        rocket: 9.0,
        ricochet: 9.0,
        piercing: 9.0,
    };
    tank.selected_ammo = weapon;
    let position = sim.body_translation(sim.tanks[index].body);
    let mut command = VehicleCommand::idle_aiming(aim);
    command.fire = true;
    sim.events.clear();
    sim.step_with(&BTreeMap::from([(viewer, command)]));
    let mut recorder = ShotPathRecorder::default();
    recorder.follow(sim, 1);
    let launches = recorder
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            PathEntry::Launch(path) => Some(*path),
            _ => None,
        })
        .collect();
    let flash = sim
        .events
        .iter()
        .find(|event| event.kind == SimEventType::Shot)
        .map(|event| (event.x, event.z))
        .expect("the host fired");
    (launches, flash, position)
}

/// The own shells drawn with no view delay at `tick`.
fn drawn_at(own: &mut OwnShots, tick: f64) -> Vec<RenderShot> {
    let mut shots = Vec::new();
    own.draw(tick, tick, -1.0, &[], &mut shots);
    shots
}

#[test]
fn a_predicted_shot_launches_the_hosts_shells_from_its_muzzle() {
    for (kind, weapon) in [
        (VehicleKind::Balanced, Weapon::Standard),
        (VehicleKind::Scout, Weapon::Spread),
        (VehicleKind::Heavy, Weapon::Rocket),
        (VehicleKind::Balanced, Weapon::Ricochet),
        (VehicleKind::Balanced, Weapon::Piercing),
    ] {
        let (mut sim, viewer) = alone(kind);
        let cover = predictor(&sim, viewer);
        let aim = 0.7;
        let (launches, host_flash, position) = host_fires(&mut sim, viewer, weapon, aim);
        let mut own = OwnShots::default();
        let flash = own.fire(
            Firing {
                tick: 1,
                position,
                aim,
                kind,
                weapon,
                team: Team::Blue,
                tank_id: viewer,
            },
            &cover,
        );
        assert!(
            (flash.x - host_flash.0).hypot(flash.z - host_flash.1) < 1e-9,
            "{weapon:?}: the flash shows at the host's muzzle"
        );
        for ticks in [0.5, 6.0, 40.0] {
            let tick = launches[0].tick + ticks;
            let drawn = drawn_at(&mut own, tick);
            assert_eq!(drawn.len(), launches.len(), "{weapon:?}: every shell");
            for launch in &launches {
                let host = launch.at(tick);
                let nearest = drawn
                    .iter()
                    .map(|shot| (shot.x - host.x).hypot(shot.z - host.z))
                    .fold(f64::INFINITY, f64::min);
                // The host's paths are rounded to the wire's millimetres.
                assert!(nearest < 0.005, "{weapon:?} at +{ticks}: {nearest} m off");
            }
        }
    }
}

#[test]
fn a_predicted_shell_stops_at_cover_and_a_ricochet_bounces_off_it() {
    for (weapon, bounces) in [(Weapon::Standard, false), (Weapon::Ricochet, true)] {
        let (mut sim, viewer) = alone(VehicleKind::Balanced);
        // An indestructible concrete block 8 m ahead.
        sim.add_cover(&CoverDef {
            kind: CoverKind::Concrete,
            x: 0.0,
            z: 8.0,
            w: 6.0,
            d: 1.0,
            h: 2.0,
            hp: f64::INFINITY,
            color: 0x888888,
            timber_join: None,
            timber_bays: None,
            debris_seed: None,
        });
        sim.world.step();
        let cover = predictor(&sim, viewer);
        let (launches, _, position) = host_fires(&mut sim, viewer, weapon, 0.0);
        let mut own = OwnShots::default();
        own.fire(
            Firing {
                tick: 1,
                position,
                aim: 0.0,
                kind: VehicleKind::Balanced,
                weapon,
                team: Team::Blue,
                tank_id: viewer,
            },
            &cover,
        );
        let speed = launches[0].vx.hypot(launches[0].vz);
        // The block's face is about 7.5 m out: the shell meets it after this many ticks.
        let at_face = launches[0].tick + (7.5 - launches[0].z) / speed / STEP;
        let before = drawn_at(&mut own, at_face - 3.0);
        assert_eq!(before.len(), 1, "{weapon:?} flies up to the cover");
        let after = drawn_at(&mut own, at_face + 6.0);
        if bounces {
            assert_eq!(after.len(), 1, "a ricochet flies on");
            assert!(
                after[0].vz < 0.0 && after[0].z < 7.5,
                "back off the face: {after:?}"
            );
        } else {
            assert!(after.is_empty(), "{weapon:?} ends at the cover: {after:?}");
        }
    }
}

#[test]
fn own_shells_leave_in_the_present_and_ease_into_the_display_at_a_steady_pace() {
    // A 200 ms view delay: 12 ticks between the predicted present and the display.
    let delay = 12.0;
    let launch = 1000.0;
    let mut previous = own_clock(launch, launch - delay, launch);
    assert_eq!(previous, launch, "a shell leaves the barrel in the present");
    let mut present = launch;
    while present < launch + 10.0 * delay {
        present += 0.25;
        let tick = own_clock(present, present - delay, launch);
        let pace = (tick - previous) / 0.25;
        assert!(
            (pace - 0.8).abs() < 1e-9 || (pace - 1.0).abs() < 1e-9,
            "four fifths of real time while easing, then real time: {pace}"
        );
        previous = tick;
    }
    assert_eq!(previous, present - delay, "it ends on the display tick");
}

#[test]
fn a_shell_the_host_launched_a_tick_late_glides_onto_its_path() {
    let (sim, viewer) = alone(VehicleKind::Balanced);
    let cover = predictor(&sim, viewer);
    let mut own = OwnShots::default();
    let position = sim.body_translation(sim.tanks[0].body);
    own.fire(
        Firing {
            tick: 100,
            position,
            aim: 0.0,
            kind: VehicleKind::Balanced,
            weapon: Weapon::Standard,
            team: Team::Blue,
            tank_id: viewer,
        },
        &cover,
    );
    let start = drawn_at(&mut own, 99.0)[0];
    let mut previous = start.z;
    let mut step = |own: &mut OwnShots, tick: f64| {
        let shot = drawn_at(own, tick)[0];
        let moved = shot.z - previous;
        assert!(
            moved > 0.0 && moved <= shot.vz * STEP * 0.5 * 1.001,
            "at {tick}: moved {moved} m"
        );
        previous = shot.z;
        shot
    };
    for half in 1..10 {
        step(&mut own, 99.0 + f64::from(half) * 0.5);
    }
    // The host's launch: the same shell, a tick later.
    let host = ShotPath {
        id: 77,
        tick: 100.0,
        x: start.x,
        z: start.z,
        vx: start.vx,
        vz: start.vz,
        launch: sloppy_core::net::shot_paths::ShotLaunch {
            team: Team::Blue,
            weapon: Weapon::Standard,
            y: start.y,
            visual_y: start.visual_y,
            thrust: None,
            rewind: 0,
        },
    };
    own.claim(
        &mut vec![PathEntry::Launch(host)],
        &mut Vec::new(),
        Team::Blue,
        viewer,
        &cover,
    );
    let mut last = start;
    for half in 10..80 {
        last = step(&mut own, 99.0 + f64::from(half) * 0.5);
    }
    let on_path = host.at(99.0 + 79.0 * 0.5);
    assert!(
        (last.z - on_path.z).abs() < 0.005,
        "it ends on the host's path: {} against {}",
        last.z,
        on_path.z
    );
}
