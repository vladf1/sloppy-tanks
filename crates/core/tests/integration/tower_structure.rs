//! Watchtowers built of parts: a shell between the legs and under the braces flies
//! through untouched, one at a leg strikes that leg alone, a tower missing a leg tips
//! over toward it and comes down in sections (nothing rising, nothing left hanging),
//! its ruins clear away to its footings, and clients and late joiners see it all.

use crate::{net_support, support};

use std::collections::BTreeMap;

use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::data::{STEP, weapon};
use sloppy_core::sim::house_structure::PartSet;
use sloppy_core::sim::math::Vec2;
use sloppy_core::sim::tower_layout::TOWER_BASE;
use sloppy_core::sim::tower_physics::tower_part_box;
use sloppy_core::sim::tower_structure::{
    CORNERS, DECKS, FOOTINGS, LADDER, LOWER_LEGS, ROOF, TowerPartKind, tower_structure,
};
use sloppy_core::sim::{CoverKind, SimEventType, Simulation, Team, Weapon};
use support::{clear_arena, idle, place_tank};

/// A watchtower alone at (24.75, 0.75), a navigation cell's middle, the human parked
/// far off.
fn lone_tower() -> (Simulation, usize) {
    let mut s = Simulation::with_seed(731.0);
    s.start();
    let human = s.human_index().expect("a human tank");
    clear_arena(&mut s, &[human]);
    place_tank(&mut s, 0, -40.0, -40.0, None);
    s.tanks[0].protection = 999.0;
    let tower = s.add_cover(&CoverDef::tower(24.75, 0.75));
    s.nav.rebuild(&s.covers, None);
    s.world.step();
    (s, tower)
}

fn gone(s: &Simulation, tower: usize) -> PartSet {
    s.covers[tower].tower.as_ref().expect("a watchtower").gone
}

fn damage(s: &Simulation, tower: usize) -> Vec<f64> {
    s.covers[tower]
        .tower
        .as_ref()
        .expect("a watchtower")
        .damage
        .clone()
}

/// A shell of `weapon_kind` at height `y` from `from` toward `to`.
fn fire(s: &mut Simulation, from: Vec2, to: Vec2, y: f64, weapon_kind: Weapon) -> u32 {
    let (dx, dz) = (to.x - from.x, to.z - from.z);
    let length = dx.hypot(dz);
    let stats = weapon(weapon_kind);
    let id = s.allocate_id();
    s.shots.push(sloppy_core::sim::Shot {
        id,
        x: from.x,
        z: from.z,
        y: Some(y),
        vx: dx / length * stats.speed,
        vz: dz / length * stats.speed,
        team: Team::Blue,
        owner: s.tanks[0].id,
        weapon: weapon_kind,
        damage: stats.damage,
        piercing: u32::from(weapon_kind == Weapon::Piercing),
        life: 3.0,
        ..Default::default()
    });
    id
}

fn run(s: &mut Simulation, seconds: f64) {
    for _ in 0..(seconds / STEP).round() as usize {
        idle(s);
    }
}

/// The ground point under leg `corner` of `tower`.
fn leg_at(s: &Simulation, tower: usize, corner: usize) -> Vec2 {
    let (sx, sz) = CORNERS[corner];
    let c = &s.covers[tower];
    Vec2::new(c.x + sx * TOWER_BASE.offset, c.z + sz * TOWER_BASE.post_z)
}

/// The tower's falling or fallen bodies.
fn bodies(s: &Simulation, tower: usize) -> Vec<(u32, PartSet)> {
    let id = s.covers[tower].id;
    s.fragments
        .iter()
        .filter(|f| f.tower_id == Some(id))
        .map(|f| (f.id, f.tower_parts.expect("a tower body's parts")))
        .collect()
}

#[test]
fn shells_between_the_legs_and_under_the_braces_fly_through_untouched() {
    let (mut s, tower) = lone_tower();
    let c = s.covers[tower].clone();
    let rng = s.rng.state;
    // Every tank's shell height, along z between the footings and along x between the
    // front and back legs.
    for y in [0.96, 1.02, 1.09, 1.4] {
        for (from, to) in [
            (Vec2::new(c.x, c.z + 25.0), Vec2::new(c.x, c.z - 25.0)),
            (
                Vec2::new(c.x - 25.0, c.z + 0.3),
                Vec2::new(c.x + 25.0, c.z + 0.3),
            ),
            (
                Vec2::new(c.x + 1.0, c.z - 25.0),
                Vec2::new(c.x + 1.0, c.z + 25.0),
            ),
        ] {
            let id = fire(&mut s, from, to, y, Weapon::Standard);
            let mut far = false;
            for _ in 0..120 {
                idle(&mut s);
                if let Some(shot) = s.shots.iter().find(|shot| shot.id == id)
                    && (shot.x - from.x).hypot(shot.z - from.z) > 35.0
                {
                    far = true;
                }
            }
            assert!(
                far,
                "a shell at {y} m from {from:?} flew on through the tower"
            );
        }
    }
    assert!(gone(&s, tower).is_empty());
    assert!(
        damage(&s, tower).iter().all(|&d| d == 0.0),
        "nothing was struck"
    );
    assert!(s.covers[tower].alive);
    assert_eq!(s.rng.state, rng, "a tower's parts draw nothing");
}

#[test]
fn the_footings_are_low_enough_that_a_shell_meets_the_leg() {
    let parts = &tower_structure().parts;
    for leg in LOWER_LEGS {
        let part = &parts[leg];
        assert!(part.min.y < 0.75, "the leg stands bare from its shoe");
        assert!(part.max.y > 1.4);
    }
    for footing in FOOTINGS {
        assert!(
            parts[footing].max.y < 0.9,
            "a footing stays under the shells"
        );
    }
}

#[test]
fn a_shell_at_a_leg_strikes_that_leg_alone_and_the_second_breaks_it() {
    let (mut s, tower) = lone_tower();
    // The front-east leg, from the east along the front legs' line.
    let corner = 3;
    let leg = LOWER_LEGS[corner];
    let at = leg_at(&s, tower, corner);
    let (from, to) = (Vec2::new(at.x + 20.0, at.z), Vec2::new(at.x - 20.0, at.z));
    fire(&mut s, from, to, 1.0, Weapon::Standard);
    run(&mut s, 1.5);
    let struck = damage(&s, tower);
    assert!(struck[leg] > 0.0, "the leg took the shell");
    assert!(gone(&s, tower).is_empty(), "one shell does not break a leg");
    let braces: Vec<usize> = (0..struck.len())
        .filter(|&i| tower_structure().parts[i].kind == TowerPartKind::Brace && struck[i] > 0.0)
        .collect();
    for &brace in &braces {
        assert!(
            struck[brace] < struck[leg],
            "only the burst's edge reaches a brace"
        );
    }
    s.events.clear();
    fire(&mut s, from, to, 1.0, Weapon::Standard);
    for _ in 0..90 {
        idle(&mut s);
        if gone(&s, tower).contains(leg) {
            break;
        }
    }
    assert!(
        gone(&s, tower).contains(leg),
        "the second shell breaks the leg"
    );
    // The leg shattered; whatever else went came down whole, not broken.
    let falling = bodies(&s, tower)
        .iter()
        .fold(PartSet::EMPTY, |all, (_, parts)| all.union(parts));
    let shattered: Vec<usize> = gone(&s, tower)
        .iter()
        .filter(|&part| !falling.contains(part))
        .collect();
    assert_eq!(shattered, [leg], "only the leg broke");
    assert!(
        s.events
            .iter()
            .any(|e| e.kind == SimEventType::Shatter && e.cover_kind == Some(CoverKind::Tower)),
        "its splinters fly"
    );
}

#[test]
fn an_armour_piercing_round_breaks_the_leg_it_goes_through_and_flies_on() {
    let (mut s, tower) = lone_tower();
    // Diagonally through the front-east leg alone.
    let at = leg_at(&s, tower, 3);
    let id = fire(
        &mut s,
        Vec2::new(at.x + 15.0, at.z + 15.0),
        at,
        1.0,
        Weapon::Piercing,
    );
    let mut out = false;
    for _ in 0..90 {
        idle(&mut s);
        if let Some(shot) = s.shots.iter().find(|shot| shot.id == id)
            && shot.x < at.x - 3.0
        {
            out = true;
        }
    }
    assert!(out, "the round flew on through");
    assert!(gone(&s, tower).contains(LOWER_LEGS[3]));
    for leg in &LOWER_LEGS[..3] {
        assert!(damage(&s, tower)[*leg] == 0.0, "leg {leg} untouched");
    }
}

/// How far (m) a section knocked over its edge as it lands may lift its centre (a deck
/// half coming down on its rim swings up toward standing before it falls flat).
const ROLL: f64 = 0.5;

/// The tower with leg `corner` shot out, as the falling frame lands and the ruin lies.
#[test]
fn a_tower_missing_a_leg_tips_toward_it_and_comes_down_in_sections() {
    for corner in 0..4 {
        let (mut s, tower) = lone_tower();
        let bodies_before = s.world.bodies.len();
        let rng = s.rng.state;
        let c = s.covers[tower].clone();
        s.break_tower_parts(
            tower,
            PartSet::from_indices([LOWER_LEGS[corner]]),
            PartSet::EMPTY,
            None,
            None,
        );
        assert!(
            !s.covers[tower].alive,
            "corner {corner}: its deck is coming down"
        );
        let falling = bodies(&s, tower);
        assert_eq!(
            falling.len(),
            2,
            "corner {corner}: the frame falls, the ladder off it"
        );
        assert!(falling[0].1.contains(ROOF) && falling[0].1.contains(DECKS[0]));
        assert_eq!(falling[1].1, PartSet::from_indices([LADDER]));
        for footing in FOOTINGS {
            assert!(!gone(&s, tower).contains(footing), "the footings stay");
        }
        // The falling frame's centre only ever drops, and a section's lifts no more than
        // rolling over its edge as it settles lifts it.
        let frame = s
            .fragments
            .iter()
            .find(|f| f.tower_id == Some(c.id))
            .unwrap()
            .id;
        let mut start: BTreeMap<u32, f64> = BTreeMap::new();
        let mut roof_end = None;
        let roof_start = {
            let body = s.fragments.iter().find(|f| f.id == frame).unwrap().body;
            s.world.bodies[body].center_of_mass()
        };
        for _ in 0..(6.0 / STEP) as usize {
            idle(&mut s);
            for fragment in s.fragments.iter().filter(|f| f.tower_id == Some(c.id)) {
                let y = f64::from(s.world.bodies[fragment.body].center_of_mass().y);
                let first = *start.entry(fragment.id).or_insert(y);
                let allowed = if fragment.id == frame { 0.05 } else { ROLL };
                assert!(
                    y <= first + allowed,
                    "corner {corner}: a body rose {y} from {first} at {}: {:?}",
                    s.elapsed,
                    fragment.tower_parts
                );
                if fragment
                    .tower_parts
                    .is_some_and(|parts| parts.contains(ROOF))
                {
                    roof_end = Some(s.world.bodies[fragment.body].center_of_mass());
                }
            }
        }
        let roof_end = roof_end.expect("the roof lies somewhere");
        // Toward the missing leg: over the line between its two neighbours, which on a
        // tower wider than it is deep throws it mostly front or back.
        let (sx, sz) = CORNERS[corner];
        let moved = Vec2::new(
            f64::from(roof_end.x - roof_start.x),
            f64::from(roof_end.z - roof_start.z),
        );
        let (ox, oz) = (sx * TOWER_BASE.offset, sz * TOWER_BASE.post_z);
        let toward = (moved.x * ox + moved.z * oz) / ox.hypot(oz);
        assert!(
            toward > 2.0 && moved.x * sx > -0.5 && moved.z * sz > 2.0,
            "corner {corner}: the top fell toward the missing leg: {moved:?}"
        );
        assert!(roof_end.y < 3.0, "corner {corner}: the roof is down");
        assert!(
            bodies(&s, tower).len() > 4,
            "corner {corner}: it broke into sections where it landed"
        );
        // Everything of it on the ground lies low; the footings stand.
        for fragment in s.fragments.iter().filter(|f| f.tower_id == Some(c.id)) {
            let y = f64::from(s.world.bodies[fragment.body].center_of_mass().y);
            assert!(y < 3.0, "corner {corner}: a section left up at {y}");
        }
        // The ruin clears away to the footings, every body with it.
        run(&mut s, 45.0);
        assert!(
            bodies(&s, tower).is_empty(),
            "corner {corner}: the ruin cleared"
        );
        assert_eq!(
            s.world.bodies.len(),
            bodies_before,
            "corner {corner}: bodies"
        );
        assert!(s.nav.is_blocked(Vec2::new(c.x + TOWER_BASE.offset, c.z)));
        assert!(!s.nav.is_blocked(Vec2::new(c.x, c.z)));
        assert_eq!(
            s.rng.state, rng,
            "corner {corner}: a collapse draws nothing"
        );
    }
}

#[test]
fn a_shot_out_brace_falls_alone_and_the_tower_stands() {
    let (mut s, tower) = lone_tower();
    let brace = (0..tower_structure().parts.len())
        .find(|&i| tower_structure().parts[i].kind == TowerPartKind::Brace)
        .unwrap();
    let girt = (0..tower_structure().parts.len())
        .find(|&i| tower_structure().parts[i].kind == TowerPartKind::Girt)
        .unwrap();
    s.break_tower_parts(
        tower,
        PartSet::from_indices([brace, girt]),
        PartSet::EMPTY,
        None,
        None,
    );
    assert!(s.covers[tower].alive);
    assert_eq!(gone(&s, tower), PartSet::from_indices([brace, girt]));
    assert!(bodies(&s, tower).is_empty(), "nothing else comes down");
    let (min, max) = tower_part_box(&s.covers[tower], brace).unwrap();
    assert!(max.y > min.y);
}

#[test]
fn a_rebuilt_tower_stands_whole_again_and_clears_its_ruin() {
    let (mut s, tower) = lone_tower();
    let colliders = s.world.colliders.len();
    let bodies_before = s.world.bodies.len();
    s.break_tower_parts(
        tower,
        PartSet::from_indices([LOWER_LEGS[0]]),
        PartSet::EMPTY,
        None,
        None,
    );
    run(&mut s, 3.0);
    assert!(!s.covers[tower].alive);
    s.restore_cover(tower);
    assert!(s.covers[tower].alive);
    assert!(gone(&s, tower).is_empty());
    assert!(
        bodies(&s, tower).is_empty(),
        "its fallen sections are cleared"
    );
    assert_eq!(s.world.colliders.len(), colliders);
    assert_eq!(s.world.bodies.len(), bodies_before);
    assert!(
        s.nav
            .is_blocked(Vec2::new(s.covers[tower].x, s.covers[tower].z))
    );
}

#[test]
fn a_scrap_yard_tower_falls_and_rises_again_in_place() {
    use sloppy_core::sim::SimulationSetup;
    use sloppy_core::sim::level_rules::single_player_rules;
    use sloppy_core::sim::superstress_level::{
        REBUILD_SECONDS, superstress_level, superstress_rules,
    };
    let mut sim = Simulation::new(
        731.0,
        single_player_rules(superstress_level()).merged(SimulationSetup {
            round: Some(3),
            ..SimulationSetup::default()
        }),
    );
    let tower = sim
        .covers
        .iter()
        .position(|c| c.kind == CoverKind::Tower)
        .unwrap();
    for tank in 0..sim.tanks.len() {
        support::set_translation(&mut sim, tank, -30.0, 0.0);
    }
    let (x, z) = (sim.covers[tower].x, sim.covers[tower].z);
    let covers = sim.covers.len();
    sim.start();
    let (id, team) = (sim.human().id, sim.human().team);
    for (cycle, &leg) in LOWER_LEGS[..2].iter().enumerate() {
        sim.break_tower_parts(
            tower,
            PartSet::from_indices([leg]),
            PartSet::EMPTY,
            None,
            Some((id, team)),
        );
        assert!(!sim.covers[tower].alive, "cycle {cycle}: it fell");
        for _ in 0..(2.0 / STEP) as usize {
            sim.step(sloppy_core::sim::VehicleCommand::idle(), false);
        }
        assert!(
            !bodies(&sim, tower).is_empty(),
            "cycle {cycle}: its sections lie about"
        );
        assert!(
            sim.nav.is_blocked(Vec2::new(x - TOWER_BASE.offset, z)),
            "its footings block"
        );
        superstress_rules(&mut sim);
        sim.elapsed += REBUILD_SECONDS;
        superstress_rules(&mut sim);
        assert!(sim.covers[tower].alive, "cycle {cycle}: the tower rebuilt");
        assert!(gone(&sim, tower).is_empty());
        assert!(
            bodies(&sim, tower).is_empty(),
            "cycle {cycle}: its ruin is cleared"
        );
        assert_eq!(sim.covers.len(), covers, "cycle {cycle}: no cover is added");
        assert!(
            sim.nav.is_blocked(Vec2::new(x, z)),
            "the tower blocks routes again"
        );
        assert_eq!(
            sim.cover_by_collider.get(&sim.covers[tower].collider),
            Some(&tower)
        );
    }
}

#[test]
fn broken_parts_and_falling_bodies_reach_room_clients_and_late_joiners() {
    use net_support::{apply_batch, baseline, batch};
    use sloppy_core::net::multiplayer_simulation::{
        MultiplayerOptions, create_multiplayer_simulation,
    };
    use sloppy_core::net::replication::{StateMirror, StateStream};
    use sloppy_core::net::scene_codec::{MirrorScene, Scene};
    use sloppy_core::sim::map_options::MapId;
    use sloppy_core::sim::types::{PlayerAssignment, VehicleKind};
    let players = [PlayerAssignment {
        player_id: "one".into(),
        name: "One".into(),
        team: Team::Blue,
        slot: 0,
        kind: VehicleKind::Balanced,
    }];
    let mut s = create_multiplayer_simulation(
        4242.0,
        &players,
        MultiplayerOptions {
            map_mode: Some(MapId::Village),
            ..MultiplayerOptions::default()
        },
    )
    .unwrap();
    s.start();
    let tower = (0..s.covers.len())
        .find(|&i| s.covers[i].kind == CoverKind::Tower)
        .expect("a watchtower");
    let id = s.covers[tower].id;
    let viewer = s.human().id;
    let mut stream = StateStream::new("room", 1);
    let mut mirror = StateMirror::default();
    let scene = Scene::capture(&s);
    mirror
        .apply_full(
            &baseline(&stream.full(0, 0, [], || scene.clone())),
            "room",
            1,
        )
        .unwrap();
    // A brace, then a leg: the frame falls, a few ticks of it falling, then its landing.
    let brace = (0..tower_structure().parts.len())
        .find(|&i| tower_structure().parts[i].kind == TowerPartKind::Brace)
        .unwrap();
    let mut tick = 0u64;
    for part in [brace, LOWER_LEGS[1]] {
        s.break_tower_parts(
            tower,
            PartSet::from_indices([part]),
            PartSet::EMPTY,
            None,
            None,
        );
        for _ in 0..90 {
            tick += 1;
            s.step_with(&BTreeMap::new());
            let mut next = Scene::capture(&s);
            let body = stream.snapshot(&mut next, tick, &[], &[]);
            apply_batch(&mut mirror, &batch(1, 0, stream.seq, &[(tick, body)]));
            let shown = mirror.render(viewer).unwrap();
            let cover = shown.covers.iter().find(|cover| cover.id == id).unwrap();
            assert_eq!(cover.broken, gone(&s, tower), "tick {tick}");
            assert_eq!(cover.alive, s.covers[tower].alive, "tick {tick}");
            for fragment in s.fragments.iter().filter(|f| f.tower_id == Some(id)) {
                let seen = shown
                    .fragments
                    .iter()
                    .find(|shown| shown.id == fragment.id)
                    .expect("each falling body reaches the client");
                assert_eq!(seen.tower_parts, fragment.tower_parts);
                assert_eq!(seen.tower_id, Some(id));
            }
        }
    }
    assert!(!bodies(&s, tower).is_empty(), "something fell");
    // A client joining now sees the tower as it stands and what lies about.
    let scene = Scene::capture(&s);
    let late = MirrorScene::from_scene(scene)
        .unwrap()
        .render(viewer)
        .unwrap();
    let cover = late.covers.iter().find(|cover| cover.id == id).unwrap();
    assert_eq!(cover.broken, gone(&s, tower));
    assert!(!cover.alive);
    let seen: Vec<_> = late
        .fragments
        .iter()
        .filter(|f| f.tower_id == Some(id))
        .map(|f| (f.id, f.tower_parts.unwrap()))
        .collect();
    assert_eq!(seen, bodies(&s, tower));
}

#[test]
fn a_pilot_or_client_world_stands_the_tower_as_the_scene_shows_it() {
    let (mut s, tower) = lone_tower();
    let (mut mirror, mirrored) = lone_tower();
    s.break_tower_parts(
        tower,
        PartSet::from_indices([LOWER_LEGS[2]]),
        PartSet::EMPTY,
        None,
        None,
    );
    let shown = gone(&s, tower);
    mirror.covers[mirrored].alive = s.covers[tower].alive;
    mirror.set_tower_gone(mirrored, shown);
    assert_eq!(gone(&mirror, mirrored), shown);
    let standing = mirror.covers[mirrored]
        .tower
        .as_ref()
        .unwrap()
        .colliders
        .iter()
        .filter(|c| c.is_some())
        .count();
    assert_eq!(standing, tower_structure().parts.len() - shown.len());
    assert!(
        !mirror.nav.is_blocked(Vec2::new(24.75, 0.75)),
        "the middle opens"
    );
    assert!(mirror.fragments.is_empty(), "nothing falls in a mirror");
}
