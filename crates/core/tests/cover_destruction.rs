//! Cover destruction (the former `tests/cover-destruction.test.ts`): collider identity and
//! sight lines, ricochet off surviving cover only, single destruction, and the navigation and
//! collision openings left by towers, village cover and harbor cargo.

mod support;

use sloppy_core::sim::arena::{CoverDef, spawn_positions};
use sloppy_core::sim::data::{STEP, weapon};
use sloppy_core::sim::debris_physics::DebrisMaterial;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::math::Vec2;
use sloppy_core::sim::physics::vector;
use sloppy_core::sim::projectiles::step_projectiles;
use sloppy_core::sim::{
    Cover, CoverKind, FragmentShape, Shot, SimEventType, Simulation, Team, Weapon,
};
use support::clear_arena;

/// The TS `coverDamageStage` from the cover model: cargo shows two damage stages.
fn cover_damage_stage(cover: &Cover) -> u32 {
    match cover.kind {
        CoverKind::Cargo if cover.hp >= cover.max_hp => 0,
        CoverKind::Cargo if cover.hp > cover.max_hp * 0.35 => 1,
        CoverKind::Cargo => 2,
        CoverKind::Timber => {
            sloppy_core::sim::timber_layout::timber_damage_stage(cover.hp, cover.max_hp)
        }
        _ => 0,
    }
}

fn at(cover: &Cover) -> Vec2 {
    Vec2::new(cover.x, cover.z)
}

/// A shell from x = -3 heading east into a 10 m concrete wall at the origin.
fn wall_shot(hp: f64, kind: Weapon, bounces: Option<u32>) -> (Simulation, usize) {
    let mut s = Simulation::with_seed(123.0);
    clear_arena(&mut s, &[]);
    s.start();
    let cover = s.add_cover(&CoverDef::new(
        CoverKind::Concrete,
        0.0,
        0.0,
        1.0,
        10.0,
        2.0,
        hp,
        0,
    ));
    s.world.step();
    s.shots.push(Shot {
        id: 999,
        x: -3.0,
        z: 0.0,
        vx: 180.0,
        vz: 0.0,
        owner: 999,
        team: Team::Blue,
        damage: weapon(kind).damage,
        bounces: bounces.unwrap_or(weapon(kind).bounces),
        life: 2.0,
        piercing: 0,
        weapon: kind,
        ..Shot::default()
    });
    step_projectiles(&mut s, STEP, false);
    (s, cover)
}

#[test]
fn cover_queries_ignore_tanks_and_debris_and_release_destroyed_collider_identities() {
    let mut s = Simulation::with_seed(123.0);
    let c = s.add_cover(&CoverDef::new(
        CoverKind::Concrete,
        0.0,
        50.0,
        1.0,
        4.0,
        2.0,
        10.0,
        0,
    ));
    let handle = s.covers[c].collider;
    let human = s.human_index().unwrap();
    let body = s.tanks[human].body;
    s.world.bodies[body].set_translation(vector(-2.0, 0.65, 50.0), true);
    s.fragment(2.0, 50.0, 0, 0.5, FragmentShape::Shard, 1.0);
    s.world.step();
    let a = Vec2::new(-5.0, 50.0);
    let b = Vec2::new(5.0, 50.0);
    assert!(!s.visible(a, b));
    assert_eq!(s.cover_by_collider.get(&handle), Some(&c));
    let (id, team) = (s.tanks[human].id, s.human_team);
    s.damage_cover(c, 10.0, id, team, None, None);
    assert!(!s.cover_by_collider.contains_key(&handle));
    assert!(s.visible(a, b));
    s.reset(None);
    assert_eq!(s.cover_by_collider.len(), s.covers.len());
    for (index, cover) in s.covers.iter().enumerate() {
        assert_eq!(s.cover_by_collider.get(&cover.collider), Some(&index));
    }
}

#[test]
fn only_ricochet_ammo_reflects_off_surviving_cover() {
    for kind in [Weapon::Standard, Weapon::Spread, Weapon::Ricochet] {
        let (s, cover) = wall_shot(200.0, kind, None);
        assert_eq!(s.covers[cover].hp, 200.0 - weapon(kind).damage, "{kind:?}");
        if kind == Weapon::Ricochet {
            assert_eq!(s.shots[0].bounces, 2);
            assert!(s.shots[0].vx < 0.0);
        } else {
            assert_eq!(s.shots.len(), 0, "{kind:?}");
            assert_eq!(
                s.events
                    .iter()
                    .filter(|e| e.kind == SimEventType::Ricochet)
                    .count(),
                0,
                "{kind:?}"
            );
        }
    }
}

#[test]
fn destroyed_cover_does_not_reflect_shells_and_breaks_exactly_once() {
    // A shell that could still bounce is absorbed by the cover it destroys.
    let (mut s, cover) = wall_shot(weapon(Weapon::Standard).damage, Weapon::Standard, Some(1));
    assert!(!s.covers[cover].alive);
    assert_eq!(s.shots.len(), 0);
    s.damage_cover(cover, 100.0, 0, Team::Blue, None, None);
    assert_eq!(s.destroyed, 1);
}

#[test]
fn tower_collapse_opens_center_route_and_retains_side_rubble() {
    let mut s = Simulation::with_seed(123.0);
    let tower = s
        .covers
        .iter()
        .position(|c| c.kind == CoverKind::Tower)
        .unwrap();
    let place = at(&s.covers[tower]);
    let before = s.nav.blocked[s.nav.index(place)];
    let version = s.nav.version;
    let (id, team) = (s.human().id, s.human_team);
    s.damage_cover(tower, 999.0, id, team, None, None);
    assert_eq!(before, 1);
    assert_eq!(s.nav.blocked[s.nav.index(place)], 0);
    assert!(s.nav.version > version);
    assert_eq!(
        s.covers
            .iter()
            .filter(|c| c.kind == CoverKind::Rubble)
            .count(),
        2
    );
    let path = s.nav.find(
        Vec2::new(place.x, place.z - 6.0),
        Vec2::new(place.x, place.z + 6.0),
    );
    assert!(
        path.iter()
            .any(|p| (p.x - place.x).abs() < 1.0 && (p.z - place.z).abs() < 2.0)
    );
}

#[test]
fn destroyed_village_cover_opens_routes_except_rooted_stumps_while_all_spawns_reach_midfield() {
    let mut s = Simulation::with_seed(123.0);
    for kind in [CoverKind::House, CoverKind::Tree, CoverKind::Timber] {
        let c = s
            .covers
            .iter()
            .position(|c| {
                c.kind == kind
                    && c.destructible
                    && (kind != CoverKind::Timber || (c.x == -2.0 && c.z == 13.0))
            })
            .unwrap_or_else(|| panic!("{kind:?} exists"));
        assert!(s.covers[c].destructible);
        let place = at(&s.covers[c]);
        assert_eq!(s.nav.blocked[s.nav.index(place)], 1, "{kind:?}");
        let (id, team) = (s.human().id, s.human_team);
        s.damage_cover(c, 1000.0, id, team, None, None);
        assert!(!s.covers[c].alive);
        assert_eq!(
            s.nav.blocked[s.nav.index(place)],
            if kind == CoverKind::Tree { 1 } else { 0 },
            "{kind:?}"
        );
    }
    for team in [Team::Blue, Team::Red] {
        for p in spawn_positions(team, 1.0) {
            assert!(
                !s.nav.find(p, Vec2::ZERO).is_empty(),
                "{team:?} spawn {p:?}"
            );
        }
    }
}

#[test]
fn harbor_cargo_stays_solid_while_damaged_then_opens_collision_and_navigation() {
    let mut sim = Simulation::with_seed(417.0);
    sim.map_mode = MapId::Harbor;
    sim.reset(None);
    sim.start();
    let cargo = sim
        .covers
        .iter()
        .position(|c| c.kind == CoverKind::Cargo && c.x == 4.0)
        .unwrap();
    let container = sim
        .covers
        .iter()
        .position(|c| c.kind == CoverKind::Container)
        .unwrap();
    let handle = sim.covers[cargo].collider;
    let cargo_at = at(&sim.covers[cargo]);
    let (id, team) = (sim.human().id, sim.human_team);
    assert_eq!(cover_damage_stage(&sim.covers[cargo]), 0);
    assert_eq!(sim.nav.blocked[sim.nav.index(cargo_at)], 1);
    sim.damage_cover(cargo, 40.0, id, team, None, None);
    assert!(sim.covers[cargo].alive);
    assert_eq!(cover_damage_stage(&sim.covers[cargo]), 1);
    sim.damage_cover(cargo, 40.0, id, team, None, None);
    assert_eq!(cover_damage_stage(&sim.covers[cargo]), 2);
    assert!(sim.covers[cargo].alive);
    assert!(
        sim.cover_by_collider.contains_key(&handle),
        "damaged crates still stop shells"
    );
    assert_eq!(
        sim.nav.blocked[sim.nav.index(cargo_at)],
        1,
        "damage does not open the route early"
    );
    sim.damage_cover(cargo, 20.0, id, team, None, None);
    assert!(!sim.covers[cargo].alive);
    assert!(!sim.cover_by_collider.contains_key(&handle));
    assert_eq!(sim.nav.blocked[sim.nav.index(cargo_at)], 0);
    assert!(sim.fragments.iter().any(|f| {
        f.shape == Some(FragmentShape::Panel) && f.material == Some(DebrisMaterial::Wood)
    }));
    sim.damage_cover(container, 10000.0, id, team, None, None);
    assert!(sim.covers[container].alive);
    let container_at = at(&sim.covers[container]);
    assert_eq!(sim.nav.blocked[sim.nav.index(container_at)], 1);
}
