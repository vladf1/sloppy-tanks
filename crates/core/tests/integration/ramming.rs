//! Tanks run down thin cover (`sim::ramming`): a young tree or a street lamp's pole goes
//! over to a hull driving into it, stump and all, and the hull drives on; a thick tree
//! and a post that cannot break stop it as before, and a tank standing beside a sapling
//! leaves it alone.

use crate::support::{clear_arena, event_count, run, tank_xz};

use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::data::vehicle;
use sloppy_core::sim::math::Vec2;
use sloppy_core::sim::ramming::{RAM_POST_WIDTH, RAM_TRUNK_RADIUS};
use sloppy_core::sim::tree_proportions::tree_proportions;
use sloppy_core::sim::tree_structure::{TREE_BUTT, TREE_STUMP};
use sloppy_core::sim::{CoverKind, SimEventType, Simulation, Team, VehicleCommand, VehicleKind};

/// The human alone on an open floor at the origin, facing +z.
fn open_floor() -> Simulation {
    let mut s = Simulation::with_seed(123.0);
    s.start();
    let human = s.human_index().expect("a human tank");
    clear_arena(&mut s, &[human]);
    s.place_tank(0, 0.0, 0.0, Some(0.0));
    s
}

/// Drive the human straight along +z for `seconds`.
fn drive_north(s: &mut Simulation, seconds: f64) {
    let drive = VehicleCommand {
        move_z: 1.0,
        ..VehicleCommand::idle()
    };
    for _ in 0..(seconds * 60.0).round() as usize {
        s.step(drive, false);
    }
}

/// A tree of footprint `size` at `(x, z)`.
fn tree(s: &mut Simulation, x: f64, z: f64, size: f64) -> usize {
    let def = CoverDef::new(CoverKind::Tree, x, z, size, size, 4.5, 80.0, 0x169f65);
    let index = s.add_cover(&def);
    s.nav.rebuild(&s.covers, None);
    index
}

#[test]
fn a_hull_runs_a_thin_tree_down_and_drives_on() {
    let mut s = open_floor();
    let thin = tree(&mut s, 0.3, 8.0, 1.5);
    assert!(
        tree_proportions(0.3, 8.0, 1.5, 1.5, 4.5).radius <= RAM_TRUNK_RADIUS,
        "a parkway tree's trunk is thin"
    );
    let mut bursts = 0;
    let drive = VehicleCommand {
        move_z: 1.0,
        ..VehicleCommand::idle()
    };
    for _ in 0..180 {
        s.step(drive, false);
        bursts += event_count(&s, SimEventType::Destroy) + event_count(&s, SimEventType::Shatter);
        s.events.clear();
    }
    assert_eq!(
        bursts, 0,
        "a tree a hull pushed over goes down and lands without a burst"
    );
    let state = s.covers[thin].structure.as_ref().expect("a tree");
    assert!(!s.covers[thin].alive, "the tree is down");
    assert!(!state.standing(TREE_BUTT) && !state.standing(TREE_STUMP));
    assert!(
        state.colliders[TREE_STUMP].is_none(),
        "no stump is left in the way"
    );
    let reached = tank_xz(&s, 0).z;
    let open = vehicle(s.tanks[0].kind).speed * 3.0;
    assert!(
        reached > 8.0 + 2.0 && reached > open * 0.8,
        "the hull drove on past the tree: z {reached:.2} of {open:.2} on open floor"
    );
    assert!(
        !s.nav.is_blocked(Vec2::new(0.3, 8.0)),
        "the bots' routes pass where it stood"
    );
}

#[test]
fn a_thick_tree_stops_a_hull() {
    let mut s = open_floor();
    // Dead ahead: a hull that meets a stump off its middle slides round it.
    let thick = tree(&mut s, 0.0, 8.0, 2.6);
    assert!(tree_proportions(0.0, 8.0, 2.6, 2.6, 4.5).radius > RAM_TRUNK_RADIUS);
    drive_north(&mut s, 3.0);
    assert!(s.covers[thick].alive, "the tree stands");
    assert!(tank_xz(&s, 0).z < 8.0, "the hull stopped at its stump");
}

#[test]
fn a_hull_runs_a_lamp_post_down_but_not_a_post_that_cannot_break() {
    let width = RAM_POST_WIDTH * 0.75;
    let mut s = open_floor();
    let lamp = s.add_cover(&CoverDef::new(
        CoverKind::Concrete,
        -0.2,
        8.0,
        width,
        width,
        3.5,
        40.0,
        0x2a3a2e,
    ));
    let mut knocks = 0;
    let mut bursts = 0;
    let drive = VehicleCommand {
        move_z: 1.0,
        ..VehicleCommand::idle()
    };
    for _ in 0..180 {
        s.step(drive, false);
        knocks += event_count(&s, SimEventType::Impact);
        bursts += event_count(&s, SimEventType::Destroy) + event_count(&s, SimEventType::Shatter);
        s.events.clear();
    }
    assert!(!s.covers[lamp].alive, "the lamp is down");
    assert!(tank_xz(&s, 0).z > 10.0, "the hull drove on");
    assert_eq!(
        (knocks, bursts),
        (1, 0),
        "it goes over with a knock, not a burst"
    );
    // Its shaft lies on the ground, whole.
    let shaft = s
        .fragments
        .iter()
        .max_by(|a, b| {
            let height = |f: &sloppy_core::sim::Fragment| f.dimensions.map_or(0.0, |d| d.y);
            height(a).total_cmp(&height(b))
        })
        .expect("the lamp's pieces");
    assert_eq!(
        s.fragments.len(),
        2,
        "a street lamp goes down as its shaft and its globe"
    );
    let body = &s.world.bodies[shaft.body];
    let up = body.rotation() * rapier3d::prelude::Vector::Y;
    assert!(
        body.translation().y < 0.5 && up.y.abs() < 0.3,
        "the shaft lies flat: y {:.2}, its axis {up:?}",
        body.translation().y
    );

    let mut s = open_floor();
    let post = s.add_cover(&CoverDef::new(
        CoverKind::Concrete,
        0.0,
        8.0,
        width,
        width,
        3.5,
        f64::INFINITY,
        0,
    ));
    drive_north(&mut s, 3.0);
    assert!(s.covers[post].alive);
    assert!(
        tank_xz(&s, 0).z < 8.0,
        "a post that cannot break stops the hull"
    );
}

#[test]
fn a_heavier_hull_loses_less_speed_to_a_tree() {
    let after = |kind: VehicleKind| {
        let mut s = Simulation::with_seed(123.0);
        s.start();
        clear_arena(&mut s, &[]);
        s.add_tank(Team::Blue, true, kind, 0);
        s.place_tank(0, 0.0, 0.0, Some(0.0));
        let z = vehicle(kind).speed * 1.0;
        tree(&mut s, 0.0, z, 1.5);
        let mut kept: f64 = 1.0;
        let drive = VehicleCommand {
            move_z: 1.0,
            ..VehicleCommand::idle()
        };
        for _ in 0..90 {
            let before = s.world.bodies[s.tanks[0].body].linvel().z;
            let standing = s.covers[0].alive;
            s.step(drive, false);
            if standing && !s.covers[0].alive {
                // The tick it went over: the speed the pass left, against the speed before.
                kept = f64::from(s.world.bodies[s.tanks[0].body].linvel().z / before);
            }
        }
        assert!(!s.covers[0].alive, "{kind:?} runs the tree down");
        kept
    };
    let (light, heavy) = (after(VehicleKind::Humvee), after(VehicleKind::Heavy));
    assert!(
        light < heavy && heavy < 1.0,
        "the light hull keeps {light:.2} of its speed, the heavy one {heavy:.2}"
    );
}

#[test]
fn a_tank_standing_by_a_sapling_leaves_it() {
    let mut s = open_floor();
    let thin = tree(&mut s, 0.0, 2.2, 1.5);
    run(&mut s, 2.0);
    assert!(s.covers[thin].alive, "an idle tank knocks nothing down");
}
