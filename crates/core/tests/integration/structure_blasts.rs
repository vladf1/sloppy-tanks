//! Blasts from outside on structures: a fuel drum wrecks the members it goes off beside and
//! only scratches what stands a few metres off, a trio's chain brings a Scrap Yard
//! watchtower down, timber beams are knocked out whole, a cottage wall's bays blow in
//! with the roof left on what still holds it, and a tank blowing up against a structure
//! breaks what it is up against.

use crate::house_structure::{VILLAGE_DESIGNS, sheets_down_off_a_hole};
use crate::support;

use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::house_structure::PartKind;
use sloppy_core::sim::math::Vec2;
use sloppy_core::sim::structure::{PartSet, Role};
use sloppy_core::sim::tank_destruction::tank_burnout;
use sloppy_core::sim::tank_dimensions::tank_hull;
use sloppy_core::sim::timber_layout::{BEAM_COUNT, TIMBER_HEALTH, TimberPartKind};
use sloppy_core::sim::timber_structure::TimberRole;
use sloppy_core::sim::tower_layout::TOWER_BASE;
use sloppy_core::sim::tower_structure::{CORNERS, LOWER_LEGS, ROOF};
use sloppy_core::sim::{CoverKind, SimEventType, Simulation};
use support::{clear_arena, place_tank, run};

/// `def` alone in a cleared arena, the human parked far off.
fn alone(def: CoverDef) -> (Simulation, usize) {
    let mut s = Simulation::with_seed(731.0);
    s.start();
    let human = s.human_index().expect("a human tank");
    clear_arena(&mut s, &[human]);
    place_tank(&mut s, 0, -40.0, -40.0, None);
    s.tanks[0].protection = 999.0;
    let cover = s.add_cover(&def);
    s.nav.rebuild(&s.covers, None);
    s.world.step();
    (s, cover)
}

/// [`alone`] with an enemy tank too, one whose death is a blast rather than a burnout.
fn alone_with_victim(def: CoverDef) -> (Simulation, usize, usize) {
    let mut s = Simulation::with_seed(731.0);
    s.start();
    let human = s.human_index().expect("a human tank");
    let victim = (0..s.tanks.len())
        .find(|&i| {
            s.tanks[i].team != s.tanks[human].team
                && !tank_burnout(s.seed, s.tanks[i].id, s.tanks[i].life + 1)
        })
        .expect("an enemy that blows up");
    clear_arena(&mut s, &[human, victim]);
    place_tank(&mut s, 0, -40.0, -40.0, None);
    s.tanks[0].protection = 999.0;
    let cover = s.add_cover(&def);
    s.nav.rebuild(&s.covers, None);
    s.world.step();
    (s, cover, 1)
}

fn lone_wall() -> CoverDef {
    CoverDef::new(
        CoverKind::Timber,
        0.0,
        10.0,
        3.7,
        0.9,
        2.8,
        TIMBER_HEALTH,
        0xb47a49,
    )
}

fn gone(s: &Simulation, cover: usize) -> PartSet {
    s.covers[cover]
        .structure
        .as_ref()
        .expect("a structure")
        .gone
}

/// A drum at `at` set off by the human.
fn blow_drum(s: &mut Simulation, at: Vec2) -> usize {
    let drum = s.add_cover(&CoverDef::drum(at.x, at.z));
    s.world.step();
    let (id, team) = (s.human().id, s.human().team);
    s.damage_cover(drum, 9999.0, id, team, None, None);
    drum
}

/// The human kills tank `victim` where it stands.
fn kill(s: &mut Simulation, victim: usize) {
    let (id, team) = (s.human().id, s.human().team);
    s.tanks[victim].protection = 0.0;
    s.tanks[victim].shield = 0.0;
    s.damage_tank(victim, 9999.0, id, team, None, None);
    assert!(!s.tanks[victim].alive);
}

fn beams(s: &Simulation, wall: usize) -> Vec<usize> {
    let layout = &s.covers[wall].structure.as_ref().unwrap().layout;
    (0..layout.parts.len())
        .filter(|&part| {
            matches!(
                layout.parts[part].role,
                Role::Timber(TimberRole {
                    kind: TimberPartKind::Beam,
                    ..
                })
            )
        })
        .collect()
}

/// The ground point under leg `corner` of `tower`, and the way out from the tower there.
fn leg(s: &Simulation, tower: usize, corner: usize) -> (Vec2, Vec2) {
    let (sx, sz) = CORNERS[corner];
    let c = &s.covers[tower];
    let at = Vec2::new(c.x + sx * TOWER_BASE.offset, c.z + sz * TOWER_BASE.post_z);
    (at, Vec2::new(sx, 0.0))
}

/// Where tower `tower`'s roof has got to.
fn roof(s: &Simulation, tower: usize) -> Option<Vec2> {
    s.fragments
        .iter()
        .find(|f| {
            f.of_structure(tower)
                && f.structure_parts()
                    .is_some_and(|parts| parts.contains(ROOF))
        })
        .map(|f| {
            let at = s.world.bodies[f.body].center_of_mass();
            Vec2::new(f64::from(at.x), f64::from(at.z))
        })
}

#[test]
fn a_drum_beside_a_tower_leg_breaks_it_and_the_tower_tips_toward_the_drum() {
    for (corner, &leg_part) in LOWER_LEGS.iter().enumerate() {
        let (mut s, tower) = alone(CoverDef::tower(24.75, 0.75));
        let c = s.covers[tower].clone();
        let (foot, out) = leg(&s, tower, corner);
        // Its side a hand's breadth off the leg, out from the tower.
        let drum = Vec2::new(foot.x + out.x * 0.95, foot.z);
        blow_drum(&mut s, drum);
        // The drum draws as it always has, the tower nothing.
        let (mut bare, _) = alone(lone_wall());
        blow_drum(&mut bare, drum);
        assert_eq!(
            s.rng.state, bare.rng.state,
            "corner {corner}: the tower's break draws nothing"
        );
        assert!(
            gone(&s, tower).contains(leg_part),
            "corner {corner}: the leg by the drum broke"
        );
        assert!(
            !s.covers[tower].alive,
            "corner {corner}: the tower comes down"
        );
        let mut top = None;
        for _ in 0..360 {
            support::idle(&mut s);
            top = roof(&s, tower).or(top);
        }
        let top = top.expect("the roof lies somewhere");
        // Toward the drum's side, front or back, as a tower wider than it is deep tips.
        let moved = Vec2::new(top.x - c.x, top.z - c.z);
        let toward = Vec2::new(drum.x - c.x, drum.z - c.z);
        assert!(
            moved.x * toward.x + moved.z * toward.z > 0.0 && moved.z * toward.z.signum() > 1.5,
            "corner {corner}: the top fell toward the drum: {moved:?}"
        );
    }
}

#[test]
fn a_drum_five_metres_off_leaves_a_towers_legs_standing() {
    let (mut s, tower) = alone(CoverDef::tower(24.75, 0.75));
    let (foot, out) = leg(&s, tower, 3);
    blow_drum(&mut s, Vec2::new(foot.x + out.x * 5.0, foot.z));
    let state = s.covers[tower].structure.as_ref().unwrap();
    assert!(state.gone.is_empty(), "nothing broke: {:?}", state.gone);
    assert!(s.covers[tower].alive);
    assert!(
        state.damage[LOWER_LEGS[3]] > 0.0,
        "the near leg is scratched"
    );
    assert!(
        state.damage[LOWER_LEGS[3]] < 15.0,
        "{}",
        state.damage[LOWER_LEGS[3]]
    );
}

#[test]
fn a_drum_chain_by_a_scrap_yard_tower_brings_it_down_each_drum_once() {
    use sloppy_core::sim::SimulationSetup;
    use sloppy_core::sim::level_rules::single_player_rules;
    use sloppy_core::sim::superstress_level::superstress_level;
    let mut sim = Simulation::new(
        731.0,
        single_player_rules(superstress_level()).merged(SimulationSetup {
            round: Some(3),
            ..SimulationSetup::default()
        }),
    );
    for tank in 0..sim.tanks.len() {
        support::set_translation(&mut sim, tank, -30.0, 0.0);
    }
    sim.start();
    let towers: Vec<usize> = (0..sim.covers.len())
        .filter(|&i| sim.covers[i].kind == CoverKind::Tower)
        .collect();
    assert_eq!(towers.len(), 2);
    for tower in towers {
        let t = sim.covers[tower].clone();
        let near = |sim: &Simulation, side: f64| {
            (0..sim.covers.len())
                .filter(|&i| {
                    let c = &sim.covers[i];
                    c.kind == CoverKind::Drum
                        && c.alive
                        && (c.x - t.x) * side > 0.0
                        && (c.x - t.x).hypot(c.z - t.z) < 7.0
                })
                .collect::<Vec<usize>>()
        };
        for side in [1.0, -1.0] {
            let trio = near(&sim, side);
            assert_eq!(trio.len(), 3, "a trio stands by the tower's {side} legs");
            let ids: Vec<u32> = trio.iter().map(|&i| sim.covers[i].id).collect();
            let destroyed = sim.combat_record.cover_destroyed;
            let (id, team) = (sim.human().id, sim.human().team);
            sim.events.clear();
            sim.damage_cover(trio[0], 9999.0, id, team, None, None);
            for &drum in &trio {
                assert!(!sim.covers[drum].alive, "the chain set off every drum");
            }
            for id in ids {
                assert_eq!(
                    sim.events
                        .iter()
                        .filter(|e| e.kind == SimEventType::Destroy && e.id == Some(id))
                        .count(),
                    1,
                    "each drum goes off once"
                );
            }
            if side > 0.0 {
                assert!(!sim.covers[tower].alive, "the chain felled the tower");
                assert!(
                    sim.combat_record.cover_destroyed >= destroyed + 4,
                    "the drums and the tower count for the one who set them off"
                );
            }
        }
    }
}

#[test]
fn a_drum_by_a_timber_wall_knocks_its_beams_out_whole_away_from_it() {
    let (mut s, wall) = alone(lone_wall());
    // Beside the wall's -z face, off its middle.
    blow_drum(&mut s, Vec2::new(0.4, 10.0 - 0.45 - 0.7));
    let down: Vec<usize> = beams(&s, wall)
        .into_iter()
        .filter(|&beam| gone(&s, wall).contains(beam))
        .collect();
    assert!(down.len() >= 2, "beams knocked out: {down:?}");
    let mut thrown = 0;
    for beam in down {
        // Whole: in a falling body, not splintered where it stood.
        let fragment = s
            .fragments
            .iter()
            .find(|f| {
                f.of_structure(wall)
                    && f.structure_parts()
                        .is_some_and(|parts| parts.contains(beam))
            })
            .unwrap_or_else(|| panic!("beam {beam} comes down whole"));
        if fragment.structure_parts() == Some(PartSet::from_indices([beam])) {
            let velocity = s.world.bodies[fragment.body].linvel();
            assert!(
                velocity.z > 1.0,
                "beam {beam} is thrown away from the drum: {velocity:?}"
            );
            thrown += 1;
        }
    }
    assert!(thrown >= 1, "the drum knocked a beam out on its own");
}

#[test]
fn a_drum_by_a_cottage_wall_blows_in_its_bays_and_the_roof_stays_on_what_holds_it() {
    // At the arena cottage's gable end, the lower bays by the drum go and the roof stays.
    let (x, z, w, d, h) = (24.0, 0.0, 5.0, 7.0, 4.9);
    let (mut s, house) = alone(CoverDef::house(x, z, w, d, h, 0xb87b4c));
    blow_drum(&mut s, Vec2::new(x + w / 2.0 + 0.7, z + 0.6));
    run(&mut s, 2.0);
    let layout = s.covers[house].structure.as_ref().unwrap().layout.clone();
    let down = gone(&s, house);
    let lower = down
        .iter()
        .filter(|&part| {
            layout.parts[part].house().kind == PartKind::Panel && layout.parts[part].min.y < 1.0
        })
        .count();
    assert!(lower >= 2, "the lower bays by the drum blew in: {down:?}");
    assert!(
        down.iter().all(|part| !matches!(
            layout.parts[part].house().kind,
            PartKind::Sheet | PartKind::Ridge
        )),
        "the roof stays on: {down:?}"
    );
    assert!(s.covers[house].alive);
    // By any wall of any village cottage, roof sheets come down only off what the drum
    // took from under them.
    for design in VILLAGE_DESIGNS {
        for (nx, nz) in [(0.0, 1.0), (0.0, -1.0), (1.0, 0.0), (-1.0, 0.0)] {
            let (x, z, w, d, h) = design;
            let (mut s, house) = alone(CoverDef::house(x, z, w, d, h, 0xb87b4c));
            let face = Vec2::new(x + nx * (w / 2.0 + 0.7), z + nz * (d / 2.0 + 0.7));
            blow_drum(&mut s, Vec2::new(face.x - nz * 0.6, face.z + nx * 0.6));
            run(&mut s, 1.0);
            let layout = s.covers[house].structure.as_ref().unwrap().layout.clone();
            let sheets = sheets_down_off_a_hole(&layout, &gone(&s, house));
            assert!(
                sheets.is_empty(),
                "{design:?} face ({nx}, {nz}): sheets {sheets:?} came down with their walls standing"
            );
        }
    }
}

#[test]
fn a_tank_blowing_up_against_a_structure_breaks_what_it_is_up_against() {
    let side_on = std::f64::consts::FRAC_PI_2;
    // Side on against a timber wall's -z face: a beam or more knocked out, not the wall.
    let (mut s, wall, victim) = alone_with_victim(lone_wall());
    let half = tank_hull(s.tanks[victim].kind).size.x / 2.0;
    place_tank(&mut s, victim, 0.3, 9.55 - half - 0.05, Some(side_on));
    s.world.step();
    kill(&mut s, victim);
    let down = beams(&s, wall)
        .into_iter()
        .filter(|&b| gone(&s, wall).contains(b))
        .count();
    assert!((1..BEAM_COUNT).contains(&down), "{down} beams knocked out");
    // Three metres off, nothing.
    let (mut s, wall, victim) = alone_with_victim(lone_wall());
    place_tank(&mut s, victim, 0.3, 9.55 - half - 3.0, Some(side_on));
    s.world.step();
    kill(&mut s, victim);
    assert!(gone(&s, wall).is_empty(), "{:?}", gone(&s, wall));
    // Against a watchtower's leg it breaks the leg; three metres off it leaves it.
    for (clearance, breaks) in [(0.05, true), (3.0, false)] {
        let (mut s, tower, victim) = alone_with_victim(CoverDef::tower(24.75, 0.75));
        let (foot, out) = leg(&s, tower, 3);
        place_tank(
            &mut s,
            victim,
            foot.x + out.x * (0.15 + half + clearance),
            foot.z,
            Some(0.0),
        );
        s.world.step();
        kill(&mut s, victim);
        assert_eq!(
            gone(&s, tower).contains(LOWER_LEGS[3]),
            breaks,
            "a tank {clearance} m off the leg"
        );
    }
}
