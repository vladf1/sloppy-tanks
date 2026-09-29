//! Physical destruction (the former `tests/destruction-physics.test.ts`): rooted tree stumps,
//! blast and shell impulses on wrecks, concrete, steel and timber, scenery breakup into a few
//! material pieces, the shared debris budget and collision groups, pushing debris with tanks,
//! stacking, and deterministic replay.

mod support;

use std::f64::consts::{FRAC_1_SQRT_2, FRAC_PI_2, PI};

use rapier3d::prelude::{ColliderBuilder, QueryFilter, Ray, RigidBodyBuilder, RigidBodyHandle};
use sloppy_core::sim::arena::CoverDef;
use sloppy_core::sim::data::{STEP, group};
use sloppy_core::sim::debris_cleanup::DEBRIS_CLEANUP_SECONDS;
use sloppy_core::sim::debris_physics::{
    DebrisMaterial, blast_debris, hit_movable_cover, hit_projectile_debris,
};
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::math::{Point3, Quat4, Random, Vec2};
use sloppy_core::sim::navigation::Navigation;
use sloppy_core::sim::physics::{interaction_groups, query_filter, to_rotation, vector};
use sloppy_core::sim::projectiles::step_projectiles;
use sloppy_core::sim::simulation::packed_groups;
use sloppy_core::sim::tank_destruction::tank_burnout;
use sloppy_core::sim::timber_layout::TimberPartKind;
use sloppy_core::sim::tree_proportions::tree_proportions;
use sloppy_core::sim::{
    CoverKind, DamageCause, Fragment, FragmentShape, Shot, SimEventType, Simulation, Team,
    VehicleCommand, VehicleKind, Weapon, WreckPart,
};
use support::clear_arena;

fn arena() -> Simulation {
    let mut s = Simulation::with_seed(731.0);
    clear_arena(&mut s, &[]);
    s.start();
    s
}

/// Whether two packed interaction groups (membership << 16 | filter) collide.
fn collides(a: u32, b: u32) -> bool {
    ((a >> 16) & b & 0xffff) != 0 && ((b >> 16) & a & 0xffff) != 0
}

fn idle(s: &mut Simulation) {
    s.step(VehicleCommand::idle(), false);
}

/// `for (let i = 0; i < seconds / STEP; i++) s.step()`, including its floating-point count.
fn tick(s: &mut Simulation, seconds: f64) {
    let mut i = 0.0;
    while i < seconds / STEP {
        idle(s);
        i += 1.0;
    }
}

/// Drives a fresh scout east from `from_x` into whatever lies ahead; it must stay grounded
/// and unharmed.
fn shove_with_scout(s: &mut Simulation, from_x: f64, steps: usize) -> usize {
    let tank = s.add_tank(Team::Blue, true, VehicleKind::Scout, 0);
    s.tanks[tank].heading = PI / 2.0;
    let body = s.tanks[tank].body;
    park(s, body, from_x, 0.65, 0.0);
    let hp = s.tanks[tank].hp;
    for _ in 0..steps {
        s.step(
            VehicleCommand {
                move_x: 1.0,
                ..VehicleCommand::idle()
            },
            false,
        );
    }
    assert!(
        s.body_translation(body).y < 0.8,
        "the tank does not climb what it pushes"
    );
    assert_eq!(s.tanks[tank].hp, hp, "pushing never damages the tank");
    tank
}

fn cover(s: &mut Simulation, kind: CoverKind, x: f64, z: f64) -> usize {
    let h = if kind == CoverKind::Tree { 6.0 } else { 2.0 };
    let hp = if kind == CoverKind::Teeth || kind == CoverKind::Hedgehog {
        f64::INFINITY
    } else {
        40.0
    };
    let c = s.add_cover(&CoverDef::new(kind, x, z, 2.0, 2.0, h, hp, 0x92734e));
    s.nav.rebuild(&s.covers, None);
    s.world.step();
    c
}

fn round(s: &mut Simulation, kind: Weapon, y: f64) -> Shot {
    let id = s.next_id;
    s.next_id += 1;
    Shot {
        id,
        owner: 999,
        team: Team::Blue,
        x: -4.0,
        z: 0.0,
        y: Some(y),
        vx: 25.0,
        vz: 0.0,
        damage: 40.0,
        bounces: 0,
        life: 2.0,
        weapon: kind,
        piercing: 0,
        ..Shot::default()
    }
}

fn shot(s: &mut Simulation, kind: Weapon, y: f64) {
    let shell = round(s, kind, y);
    s.shots.push(shell);
    step_projectiles(s, 0.2, false);
}

/// Destroys a balanced tank at the origin as a breakup (never a burnout) and returns the id
/// of its turret fragment.
fn wreck(s: &mut Simulation) -> u32 {
    let tank = s.add_tank(Team::Red, false, VehicleKind::Balanced, 0);
    // These tests require separated hull/turret pieces, regardless of map-assigned IDs.
    while tank_burnout(s.seed, s.tanks[tank].id, s.tanks[tank].life + 1) {
        s.tanks[tank].life += 1;
    }
    s.tanks[tank].protection = 0.0;
    let body = s.tanks[tank].body;
    s.world.bodies[body].set_translation(vector(0.0, 0.65, 0.0), true);
    s.damage_tank(tank, 1000.0, 999, Team::Blue, None, None);
    s.tanks.clear();
    s.fragments
        .iter()
        .find(|f| matches!(f.part, Some(WreckPart::Turret | WreckPart::TurretBarrel)))
        .expect("breakup has a turret")
        .id
}

fn park(s: &mut Simulation, body: RigidBodyHandle, x: f64, y: f64, z: f64) {
    let body = &mut s.world.bodies[body];
    body.set_translation(vector(x, y, z), true);
    body.set_linvel(vector(0.0, 0.0, 0.0), true);
    body.set_angvel(vector(0.0, 0.0, 0.0), true);
    body.sleep();
}

fn level(s: &mut Simulation, body: RigidBodyHandle) {
    s.world.bodies[body].set_rotation(to_rotation(Quat4::IDENTITY), true);
}

fn find(s: &Simulation, id: u32) -> Option<&Fragment> {
    s.fragments.iter().find(|f| f.id == id)
}

fn fragment_body(s: &Simulation, id: u32) -> RigidBodyHandle {
    find(s, id).expect("fragment exists").body
}

fn fragment_where(s: &Simulation, test: impl Fn(&Fragment) -> bool) -> u32 {
    s.fragments
        .iter()
        .find(|f| test(f))
        .expect("matching fragment")
        .id
}

/// Park every other fragment out of the way at (30, 1).
fn park_others(s: &mut Simulation, keep: &[u32]) {
    let others: Vec<_> = s
        .fragments
        .iter()
        .filter(|f| !keep.contains(&f.id))
        .map(|f| f.body)
        .collect();
    for body in others {
        park(s, body, 30.0, 1.0, 0.0);
    }
}

fn first_collider_groups(s: &Simulation, body: RigidBodyHandle) -> u32 {
    let collider = s.world.bodies[body].colliders()[0];
    packed_groups(s.world.colliders[collider].collision_groups())
}

fn linvel(s: &Simulation, body: RigidBodyHandle) -> Point3 {
    s.body_linvel(body)
}

fn angvel(s: &Simulation, body: RigidBodyHandle) -> Point3 {
    let w = s.world.bodies[body].angvel();
    Point3::new(w.x as f64, w.y as f64, w.z as f64)
}

fn length(v: Point3) -> f64 {
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
}

fn sleeping(s: &Simulation, body: RigidBodyHandle) -> bool {
    s.world.bodies[body].is_sleeping()
}

fn cover_at(s: &Simulation, cover: usize) -> Vec2 {
    Vec2::new(s.covers[cover].x, s.covers[cover].z)
}

fn cast_ray(s: &Simulation, origin: Point3, max: f64, filter: QueryFilter) -> bool {
    let ray = Ray::new(vector(origin.x, origin.y, origin.z), vector(1.0, 0.0, 0.0));
    s.world.cast_ray(&ray, max as f32, true, filter).is_some()
}

/// The TS `propagateModifiedBodyPositionsToColliders`: refresh collider poses (and the query
/// structure) after teleporting bodies, without advancing the simulation.
fn refresh_queries(s: &mut Simulation) {
    s.world.detect_collisions(&(), &());
}

fn remove_all_fragments(s: &mut Simulation) {
    let bodies: Vec<_> = s.fragments.iter().map(|f| f.body).collect();
    for body in bodies {
        s.remove_body(body);
    }
    s.fragments.clear();
}

#[test]
fn rooted_stumps_block_every_chassis_after_debris_cleanup_and_leave_the_crown_space_open() {
    // Every chassis and approach heading appears once.
    for (kind, heading) in [
        (VehicleKind::Scout, 0.0),
        (VehicleKind::Balanced, FRAC_PI_2),
        (VehicleKind::Heavy, PI),
        (VehicleKind::Heavy, -FRAC_PI_2),
    ] {
        let mut s = arena();
        let tree = cover(&mut s, CoverKind::Tree, 0.0, 0.0);
        s.damage_cover(tree, 1000.0, 999, Team::Blue, None, None);
        // Isolate the permanent stump from the temporary falling log.
        remove_all_fragments(&mut s);
        let tank = s.add_tank(Team::Blue, true, kind, 0);
        let direction = Vec2::new(heading.sin(), heading.cos());
        s.tanks[tank].heading = heading;
        let body = s.tanks[tank].body;
        s.world.bodies[body].set_rotation(to_rotation(Quat4::yaw(heading)), true);
        s.world.bodies[body]
            .set_translation(vector(-5.0 * direction.x, 0.65, -5.0 * direction.z), true);
        s.tanks[tank].previous = Vec2::new(-5.0 * direction.x, -5.0 * direction.z);
        s.world.step();
        for _ in 0..180 {
            s.step(
                VehicleCommand {
                    move_x: direction.x,
                    move_z: direction.z,
                    ..VehicleCommand::idle()
                },
                false,
            );
        }
        let position = s.body_translation(body);
        assert!(
            position.x * direction.x + position.z * direction.z < -0.7,
            "{kind:?} must stop before the stump: {position:?}"
        );
        assert!(
            position.y < 0.8,
            "the stump must not lift the tank over its footprint"
        );
        let tree_body = s.covers[tree].body;
        assert!(s.world.bodies.contains(tree_body));
        assert!(s.world.bodies[tree_body].is_fixed());
        assert_eq!(
            s.nav.blocked[s.nav.index(cover_at(&s, tree))],
            1,
            "bots must route around the stump"
        );
        assert!(!s.nav.clear_line(Vec2::new(-4.0, 0.0), Vec2::new(4.0, 0.0)));
        assert!(
            !cast_ray(
                &s,
                Point3::new(-4.0, 1.0, 0.0),
                8.0,
                query_filter(group::COVER_QUERY)
            ),
            "shells must fly above the stump"
        );
        assert!(
            cast_ray(
                &s,
                Point3::new(-4.0, 0.65, 0.0),
                8.0,
                query_filter(group::STEERING_QUERY).exclude_rigid_body(body)
            ),
            "local steering must detect the stump"
        );
        let t = &s.covers[tree];
        let radius = s.world.colliders[t.collider]
            .shape()
            .as_cylinder()
            .expect("stump cylinder")
            .radius;
        let expected = tree_proportions(t.x, t.z, t.w, t.d, t.h).stump_radius;
        assert!((radius as f64 - expected).abs() < 1e-6);
        let destroyed = s.destroyed;
        s.damage_cover(tree, 1000.0, 999, Team::Blue, None, None);
        assert_eq!(s.destroyed, destroyed, "a stump cannot be destroyed twice");
    }
}

#[test]
fn destroyed_trees_leave_a_narrow_stump_rather_than_the_original_canopy_sized_obstacle() {
    let mut s = arena();
    let tree = cover(&mut s, CoverKind::Tree, 0.0, 0.0);
    assert_eq!(s.nav.blocked[s.nav.index(Vec2::new(2.0, 0.0))], 1);
    s.damage_cover(tree, 1000.0, 999, Team::Blue, None, None);
    remove_all_fragments(&mut s);
    assert_eq!(s.nav.blocked[s.nav.index(Vec2::new(2.0, 0.0))], 0);
    let tank = s.add_tank(Team::Blue, true, VehicleKind::Heavy, 0);
    s.tanks[tank].heading = 0.0;
    let body = s.tanks[tank].body;
    park(&mut s, body, 2.0, 0.65, -5.0);
    for _ in 0..120 {
        s.step(
            VehicleCommand {
                move_z: 1.0,
                ..VehicleCommand::idle()
            },
            false,
        );
    }
    assert!(
        s.body_translation(body).z > 2.0,
        "a tank can pass beside the solid stump"
    );
    s.reset(None);
    assert!(
        s.covers
            .iter()
            .filter(|c| c.kind == CoverKind::Tree)
            .all(|c| {
                c.alive
                    && packed_groups(s.world.colliders[c.collider].collision_groups())
                        == group::COVER
            })
    );
}

#[test]
fn blasts_wake_and_tumble_a_wreck_while_edge_distant_and_airborne_debris_obey_falloff_without_rng_draws()
 {
    let mut s = arena();
    let f = wreck(&mut s);
    let body = fragment_body(&s, f);
    let state = s.rng.state;
    park(&mut s, body, 1.0, 0.5, 0.0);
    blast_debris(&mut s, Vec2::ZERO, 5.0, 60.0);
    let near = linvel(&s, body).y;
    assert!(
        near > 3.0 && near < 8.0,
        "wrecks lift without the old weightless launch: {near}"
    );
    assert!(length(angvel(&s, body)) > 0.1);
    assert!(!sleeping(&s, body));
    park(&mut s, body, 4.8, 0.5, 0.0);
    blast_debris(&mut s, Vec2::ZERO, 5.0, 60.0);
    assert!(linvel(&s, body).y < near * 0.02);
    for (x, y) in [(6.0, 0.5), (0.0, 8.0)] {
        park(&mut s, body, x, y, 0.0);
        blast_debris(&mut s, Vec2::ZERO, 5.0, 60.0);
        assert_eq!(linvel(&s, body).y, 0.0);
        assert!(sleeping(&s, body));
    }
    assert_eq!(s.rng.state, state);
}

#[test]
fn a_turret_lands_then_a_drum_chain_naturally_launches_that_same_body_again() {
    let mut s = arena();
    let f = wreck(&mut s);
    let body = fragment_body(&s, f);
    tick(&mut s, 4.0);
    assert!(s.body_translation(body).y < 1.5);
    let p = s.body_translation(body);
    let drum = cover(&mut s, CoverKind::Drum, p.x + 1.4, p.z);
    cover(&mut s, CoverKind::Drum, p.x + 3.5, p.z);
    s.damage_cover(drum, 100.0, 999, Team::Blue, None, None);
    assert!(linvel(&s, body).y > 4.0, "{}", linvel(&s, body).y);
    assert_eq!(fragment_body(&s, f), body);
    assert!(
        s.events
            .iter()
            .filter(|e| e.kind == SimEventType::Explosion)
            .count()
            >= 2
    );
    tick(&mut s, 0.3);
    assert!(s.body_translation(body).y > p.y + 0.5);
}

#[test]
fn real_projectile_hits_shove_concrete_cumulatively_while_rockets_and_nearby_blasts_are_stronger() {
    let mut speeds = Vec::new();
    for mode in ["standard", "rocket", "blast"] {
        let mut s = arena();
        let c = cover(&mut s, CoverKind::Teeth, 0.0, 0.0);
        match mode {
            "blast" => s.explode(
                Vec2::new(-1.0, 0.0),
                5.0,
                80.0,
                999,
                Team::Blue,
                None,
                DamageCause::Explosion,
            ),
            "rocket" => shot(&mut s, Weapon::Rocket, 1.0),
            _ => shot(&mut s, Weapon::Standard, 1.0),
        }
        let body = s.covers[c].body;
        let v = linvel(&s, body);
        speeds.push(length(v));
        assert!(v.x > 0.5, "{mode}: {v:?}");
        assert!(s.covers[c].alive);
        assert_eq!(s.covers[c].hp, f64::INFINITY);
        let collider = &s.world.colliders[s.covers[c].collider];
        assert!(collider.friction() >= 0.6);
        assert!(collider.restitution() < 0.05);
        if mode == "standard" {
            shot(&mut s, Weapon::Standard, 1.0);
            assert!(linvel(&s, body).x > v.x * 1.8);
            assert!(angvel(&s, body).z.abs() > 0.1);
        }
    }
    assert!(speeds[1] > speeds[0] * 2.0, "{speeds:?}");
    assert!(speeds[2] > speeds[0] * 2.0, "{speeds:?}");
}

#[test]
fn repeated_impacts_displace_concrete_update_old_and_new_navigation_footprints_and_eventually_sleep()
 {
    let mut s = arena();
    let c = cover(&mut s, CoverKind::Teeth, 0.0, 0.0);
    let initial_version = s.nav.version;
    s.explode(
        Vec2::new(-1.0, 0.0),
        6.0,
        100.0,
        999,
        Team::Blue,
        None,
        DamageCause::Explosion,
    );
    tick(&mut s, 8.0);
    // Heavy cover now needs repeated blasts to clear its old navigation footprint.
    let at = cover_at(&s, c);
    s.explode(
        Vec2::new(at.x - 1.0, at.z),
        6.0,
        100.0,
        999,
        Team::Blue,
        None,
        DamageCause::Explosion,
    );
    tick(&mut s, 8.0);
    let x = s.covers[c].x;
    assert!(x > 1.0 && x < 6.0, "heavy concrete displacement {x}");
    assert_eq!(s.nav.blocked[s.nav.index(Vec2::ZERO)], 0);
    assert_eq!(s.nav.blocked[s.nav.index(cover_at(&s, c))], 1);
    assert!(s.nav.version > initial_version);
    assert!(
        s.nav.version - initial_version <= 64,
        "no per-frame rebuilds"
    );
    let body = s.covers[c].body;
    assert!(sleeping(&s, body), "settled heavy concrete should sleep");
    assert!(
        s.body_translation(body).y > 0.0,
        "concrete collides with ground"
    );
    let version = s.nav.version;
    tick(&mut s, 2.0);
    assert_eq!(s.nav.version, version);
    let at = cover_at(&s, c);
    let path = s
        .nav
        .find(Vec2::new(at.x - 8.0, at.z), Vec2::new(at.x + 8.0, at.z));
    assert!(!path.is_empty());
    assert!(path.iter().all(|&p| s.nav.blocked[s.nav.index(p)] == 0));
}

#[test]
fn authored_scenery_emits_a_few_material_specific_pieces_with_matching_dimensions_and_contact_telemetry()
 {
    use FragmentShape::{Beam, DrumLid, DrumShell, Log, Panel};
    let cases: [(CoverKind, &[FragmentShape], usize); 5] = [
        (CoverKind::Cargo, &[Panel, Panel, Panel, Beam], 33),
        (CoverKind::Timber, &[Beam, Beam, Beam, Beam, Beam, Beam], 77),
        (CoverKind::Tree, &[Log, Beam], 99),
        (
            CoverKind::Drum,
            &[DrumShell, DrumShell, DrumShell, DrumLid],
            33,
        ),
        (CoverKind::Tower, &[Panel, Beam, Panel, Beam], 112),
    ];
    for (kind, shapes, old_draws) in cases {
        let mut s = arena();
        let c = cover(&mut s, kind, 0.0, 0.0);
        let mut expected_rng = Random::new(s.rng.state);
        for _ in 0..old_draws {
            expected_rng.next();
        }
        s.damage_cover(c, 1000.0, 999, Team::Blue, None, None);
        assert_eq!(
            s.rng.state, expected_rng.state,
            "{kind:?}: destruction preserves the legacy combat RNG stream"
        );
        let actual: Vec<_> = s.fragments.iter().map(|f| f.shape.unwrap()).collect();
        assert_eq!(actual, shapes, "{kind:?}");
        for f in &s.fragments {
            assert_eq!(f.source_kind, Some(kind));
            assert!(f.dimensions.is_some());
            assert!(s.world.bodies[f.body].is_dynamic());
            assert_eq!(
                first_collider_groups(&s, f.body),
                if kind == CoverKind::Timber {
                    group::TIMBER_DEBRIS
                } else {
                    group::PUSHABLE_DEBRIS
                }
            );
            assert_eq!(
                s.world.bodies[f.body].is_ccd_enabled(),
                kind == CoverKind::Timber
            );
        }
        if kind == CoverKind::Tree {
            let trunk = s.fragments.iter().find(|f| f.shape == Some(Log)).unwrap();
            assert_eq!(
                trunk.tree_cover_id,
                Some(s.covers[c].id),
                "the falling model retains its source crown"
            );
            assert!((s.body_translation(trunk.body).y - trunk.tree_center_y.unwrap()).abs() < 1e-5);
            assert_eq!(
                linvel(&s, trunk.body).y,
                0.0,
                "a severed tree falls rather than launching upward"
            );
            assert_eq!(
                s.world.bodies[trunk.body].colliders().len(),
                2,
                "both trunk and crown contact the ground"
            );
        }
        tick(&mut s, 4.0);
        assert!(
            s.events.iter().any(|e| {
                e.kind == SimEventType::DebrisImpact
                    && e.material.is_some()
                    && e.force.unwrap_or(0.0) > 0.0
            }),
            "{kind:?}: debris impact telemetry"
        );
        tick(&mut s, 15.0);
        assert_eq!(s.fragments.len(), 0, "{kind:?}");
    }
}

#[test]
fn physical_pieces_stay_within_the_shared_body_budget_stay_out_of_cover_queries_and_reset_cleanly()
{
    let mut s = arena();
    let initial = s.world.bodies.len();
    for i in 0..35 {
        let c = cover(
            &mut s,
            CoverKind::Cargo,
            (i % 7) as f64 * 5.0 - 15.0,
            (i / 7) as f64 * 5.0 - 10.0,
        );
        s.damage_cover(c, 100.0, 999, Team::Blue, None, None);
    }
    assert_eq!(s.fragments.len(), s.max_fragments);
    assert_eq!(s.world.bodies.len(), initial + s.max_fragments);
    assert!(!collides(group::FRAGMENT, group::TANK));
    assert!(!collides(group::FRAGMENT, group::FRAGMENT));
    assert!(collides(group::FRAGMENT, group::GROUND));
    assert!(collides(group::FRAGMENT, group::MOVABLE_COVER));
    s.world.step();
    assert!(!cast_ray(
        &s,
        Point3::new(-20.0, 1.0, 0.0),
        40.0,
        query_filter(group::COVER_QUERY)
    ));
    tick(&mut s, 19.0);
    assert_eq!(s.fragments.len(), 0);
    assert_eq!(s.world.bodies.len(), initial);
    s.map_mode = MapId::Quarry;
    s.reset(None);
    let bodies = s.world.bodies.len();
    let colliders = s.world.colliders.len();
    s.start();
    let first = cover_at(&s, s.movable_covers[0]);
    s.explode(
        first,
        6.0,
        100.0,
        999,
        Team::Blue,
        None,
        DamageCause::Explosion,
    );
    tick(&mut s, 1.0);
    s.reset(None);
    assert_eq!(s.world.bodies.len(), bodies);
    assert_eq!(s.world.colliders.len(), colliders);
    assert_eq!(s.fragments.len(), 0);
    let movable: Vec<_> = s.movable_covers.iter().map(|&i| &s.covers[i]).collect();
    assert_eq!(
        movable.iter().filter(|c| c.kind != CoverKind::Drum).count(),
        24
    );
    assert_eq!(
        movable.iter().filter(|c| c.kind == CoverKind::Drum).count(),
        2
    );
    assert!(movable.iter().all(|c| s.world.bodies.contains(c.body)));
}

#[test]
fn destroying_finite_movable_cover_removes_its_body_without_poisoning_later_simulation_steps() {
    let mut s = arena();
    let movable = cover(&mut s, CoverKind::Teeth, 0.0, 0.0);
    s.covers[movable].hp = 40.0;
    s.covers[movable].max_hp = 40.0;
    s.covers[movable].destructible = true;
    let body = s.covers[movable].body;

    s.damage_cover(movable, 40.0, 999, Team::Blue, None, None);

    assert!(!s.covers[movable].alive);
    assert!(!s.world.bodies.contains(body));
    idle(&mut s);
}

#[test]
fn physical_destruction_and_blast_replay_remain_deterministic_for_a_fixed_seed() {
    let run = || {
        let mut s = arena();
        cover(&mut s, CoverKind::Teeth, 4.0, 0.0);
        let cargo = cover(&mut s, CoverKind::Cargo, 0.0, 0.0);
        s.damage_cover(cargo, 100.0, 999, Team::Blue, None, None);
        for i in 0..180 {
            if i % 60 == 0 {
                s.explode(
                    Vec2::new(1.0, 0.0),
                    6.0,
                    80.0,
                    999,
                    Team::Blue,
                    None,
                    DamageCause::Explosion,
                );
            }
            idle(&mut s);
        }
        s.fragments
            .iter()
            .map(|f| f.body)
            .chain(s.movable_covers.iter().map(|&i| s.covers[i].body))
            .map(|body| {
                (
                    s.body_translation(body),
                    s.body_rotation(body),
                    s.body_linvel(body),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(run(), run());
}

#[test]
fn bots_route_around_a_displaced_tooth_and_cross_its_former_position_without_repeated_recovery() {
    let mut s = arena();
    let c = cover(&mut s, CoverKind::Teeth, 0.0, 0.0);
    s.explode(
        Vec2::new(-1.0, 0.0),
        6.0,
        100.0,
        999,
        Team::Blue,
        None,
        DamageCause::Explosion,
    );
    tick(&mut s, 8.0);
    let human = s.add_tank(Team::Blue, true, VehicleKind::Balanced, 0);
    let human_body = s.tanks[human].body;
    s.world.bodies[human_body].set_translation(vector(40.0, 0.65, 40.0), true);
    let bot = s.add_tank(Team::Blue, false, VehicleKind::Balanced, 1);
    let at = cover_at(&s, c);
    let from = Vec2::new(at.x - 8.0, at.z);
    let goal = Vec2::new(at.x + 8.0, at.z);
    let bot_body = s.tanks[bot].body;
    s.world.bodies[bot_body].set_translation(vector(from.x, 0.65, from.z), true);
    let path = s.nav.find(from, goal);
    let version = s.nav.version;
    let tank = &mut s.tanks[bot];
    tank.previous = from;
    tank.brain.last = from;
    tank.brain.decision = 999.0;
    tank.brain.goal = goal;
    tank.brain.path = path;
    tank.brain.nav_version = version;
    tick(&mut s, 12.0);
    let p = s.body_translation(bot_body);
    assert!(
        (p.x - goal.x).hypot(p.z - goal.z) < 1.0,
        "bot stopped at {}/{}",
        p.x,
        p.z
    );
    assert!(
        s.tanks[bot].brain.recoveries <= 2,
        "bot does not keep driving into the moved barrier"
    );
}

#[test]
fn steel_hedgehogs_keep_open_compound_geometry_and_move_settle_and_update_navigation_after_blasts()
{
    let mut s = arena();
    let c = cover(&mut s, CoverKind::Hedgehog, 0.0, 0.0);
    let body = s.covers[c].body;
    assert_eq!(s.world.bodies[body].colliders().len(), 9);
    assert!((s.world.bodies[body].mass() as f64 - 6.0).abs() < 0.001);
    for collider in s.world.bodies[body].colliders() {
        assert_eq!(s.cover_by_collider.get(collider), Some(&c));
    }
    s.explode(
        Vec2::new(-1.0, 0.0),
        5.0,
        80.0,
        999,
        Team::Blue,
        None,
        DamageCause::Explosion,
    );
    assert!(linvel(&s, body).x > 3.0);
    assert!(length(angvel(&s, body)) > 1.0);
    tick(&mut s, 10.0);
    let at = cover_at(&s, c);
    s.explode(
        Vec2::new(at.x - 1.0, at.z),
        5.0,
        80.0,
        999,
        Team::Blue,
        None,
        DamageCause::Explosion,
    );
    tick(&mut s, 10.0);
    let x = s.covers[c].x;
    assert!(x > 1.0 && x < 6.0, "heavy steel displacement {x}");
    assert!(s.body_translation(body).y > 0.0);
    assert!(sleeping(&s, body));
    assert_eq!(s.nav.blocked[s.nav.index(cover_at(&s, c))], 1);
    let mut expected_nav = Navigation::new();
    expected_nav.rebuild(&s.covers, None);
    assert_eq!(
        s.nav.blocked, expected_nav.blocked,
        "navigation follows the displaced steel footprint"
    );
    assert!(s.events.iter().any(|e| {
        e.kind == SimEventType::DebrisImpact && e.material == Some(DebrisMaterial::Metal)
    }));
}

#[test]
fn a_scout_can_steadily_push_every_concrete_profile_with_throttled_navigation_and_no_damage() {
    // x = 0..3 selects all four authored profiles, using the tallest quarry tooth.
    for x in [0.0, 1.0, 2.0, 3.0] {
        let mut s = arena();
        let c = s.add_cover(&CoverDef::new(
            CoverKind::Teeth,
            x,
            0.0,
            1.845,
            1.845,
            1.845,
            f64::INFINITY,
            0xaaaaaa,
        ));
        s.nav.rebuild(&s.covers, None);
        let version = s.nav.version;
        let tank = shove_with_scout(&mut s, x - 2.8, 240);
        let moved = s.covers[c].x - x;
        assert!(moved > 1.0, "profile {x} moved only {moved}");
        let tank_body = s.tanks[tank].body;
        assert!(s.body_translation(tank_body).x > x - 1.8);
        assert_eq!(s.covers[c].hp, f64::INFINITY);
        assert!(s.nav.version > version && s.nav.version - version <= 16);
        assert_eq!(s.nav.blocked[s.nav.index(cover_at(&s, c))], 1);
        s.remove_body(tank_body);
        s.tanks.clear();
        tick(&mut s, 8.0);
        assert!(
            sleeping(&s, s.covers[c].body),
            "concrete settles after pushing stops"
        );
    }
}

#[test]
fn tanks_physically_shove_landed_hulls_and_turrets_without_damage_and_wreck_cleanup_still_removes_bodies()
 {
    for part in [WreckPart::Hull, WreckPart::Turret] {
        let mut s = arena();
        let turret = wreck(&mut s);
        let f = if part == WreckPart::Hull {
            fragment_where(&s, |f| f.part == Some(WreckPart::Hull))
        } else {
            turret
        };
        park_others(&mut s, &[f]);
        let body = fragment_body(&s, f);
        level(&mut s, body);
        park(&mut s, body, 0.0, 0.5, 0.0);
        tick(&mut s, 0.5);
        let tank = shove_with_scout(&mut s, -4.0, 120);
        assert!(
            s.body_translation(body).x > 2.0,
            "{part:?} must move through tank contact"
        );
        let tank_body = s.tanks[tank].body;
        assert!(
            s.body_translation(tank_body).x > 0.0,
            "wreck does not trap the tank"
        );
        tick(&mut s, 19.0);
        assert_eq!(s.fragments.len(), 0);
        assert!(!s.world.bodies.contains(body));
        assert_eq!(s.world.bodies.len(), 2, "only tank and ground remain");
        wreck(&mut s);
        s.reset(None);
        assert_eq!(s.fragments.len(), 0);
        let bodies = s.world.bodies.len();
        s.reset(None);
        assert_eq!(
            s.world.bodies.len(),
            bodies,
            "reset does not retain wreck bodies"
        );
    }
}

#[test]
fn only_large_wrecks_accept_tank_contact_and_projectile_hits_while_steering_still_excludes_wrecks()
{
    assert!(collides(group::WRECK, group::TANK));
    assert!(collides(group::WRECK, group::DEBRIS_QUERY));
    assert!(collides(group::WRECK, group::WRECK));
    for other in [group::FRAGMENT, group::COVER_QUERY, group::STEERING_QUERY] {
        assert!(!collides(group::WRECK, other));
    }
    assert!(!collides(group::FRAGMENT, group::TANK));
    assert!(collides(group::WRECK, group::GROUND));
    assert!(collides(group::WRECK, group::MOVABLE_COVER));
    let mut s = arena();
    wreck(&mut s);
    let bodies: Vec<_> = s.fragments.iter().map(|f| f.body).collect();
    for body in bodies {
        assert_eq!(first_collider_groups(&s, body), group::WRECK);
        park(&mut s, body, 0.0, 1.0, 0.0);
    }
    s.world.step();
    for groups in [group::COVER_QUERY, group::STEERING_QUERY] {
        assert!(!cast_ray(
            &s,
            Point3::new(-5.0, 1.0, 0.0),
            10.0,
            query_filter(groups)
        ));
    }
}

#[test]
fn shells_shove_indestructible_wrecks_rockets_detonate_on_them_and_high_rounds_clear_them() {
    for part in [WreckPart::Hull, WreckPart::Turret] {
        let mut s = arena();
        let turret = wreck(&mut s);
        let f = if part == WreckPart::Hull {
            fragment_where(&s, |fragment| fragment.part == Some(WreckPart::Hull))
        } else {
            turret
        };
        park_others(&mut s, &[f]);
        let body = fragment_body(&s, f);
        level(&mut s, body);
        park(&mut s, body, 0.0, 0.65, 0.0);
        s.world.step();
        s.events.clear();
        shot(&mut s, Weapon::Standard, 1.0);
        assert_eq!(s.shots.len(), 0, "{part:?} absorbs the shell");
        assert!(linvel(&s, body).x > 0.0, "{part:?} moves from the impact");
        assert!(
            !s.events
                .iter()
                .any(|event| event.kind == SimEventType::Explosion)
        );

        park(&mut s, body, 0.0, 0.65, 0.0);
        refresh_queries(&mut s);
        s.events.clear();
        shot(&mut s, Weapon::Rocket, 1.0);
        assert_eq!(s.shots.len(), 0, "rocket impacts the {part:?}");
        assert!(
            s.events
                .iter()
                .any(|event| event.kind == SimEventType::Explosion)
        );
        assert!(find(&s, f).is_some(), "{part:?} survives the rocket blast");
        assert!(s.world.bodies.contains(body));

        park(&mut s, body, 0.0, 0.25, 0.0);
        refresh_queries(&mut s);
        s.events.clear();
        shot(&mut s, Weapon::Standard, 1.0);
        assert_eq!(s.shots.len(), 1, "shell passes above half-sunken {part:?}");
        assert!(s.shots[0].x > 0.0);
    }
}

#[test]
fn shells_wake_and_shove_timber_at_flight_height_and_rockets_detonate_on_it() {
    for kind in [TimberPartKind::Beam, TimberPartKind::Post] {
        for weapon in [Weapon::Standard, Weapon::Piercing, Weapon::Rocket] {
            let mut s = arena();
            let wall = cover(&mut s, CoverKind::Timber, 20.0, 20.0);
            s.damage_cover(wall, 999.0, 999, Team::Blue, None, None);
            let f = fragment_where(&s, |fragment| {
                fragment
                    .timber_part
                    .as_ref()
                    .is_some_and(|part| part.kind == kind)
            });
            park_others(&mut s, &[f]);
            let body = fragment_body(&s, f);
            let height = find(&s, f).unwrap().timber_part.as_ref().unwrap().h;
            level(&mut s, body);
            let y = if kind == TimberPartKind::Beam {
                1.3
            } else {
                height / 2.0
            };
            park(&mut s, body, 0.0, y, -0.1);
            s.world.step();
            let count = s.fragments.len();
            s.events.clear();
            shot(&mut s, weapon, 1.3);
            assert_eq!(s.shots.len(), 0, "{weapon:?} hits {kind:?}");
            assert!(!sleeping(&s, body));
            assert!(
                linvel(&s, body).x > 0.0,
                "{kind:?} moves along the shot direction"
            );
            assert_eq!(
                s.fragments.len(),
                count,
                "hit does not multiply physical debris"
            );
            assert_eq!(
                s.events.iter().any(|e| e.kind == SimEventType::Explosion),
                weapon == Weapon::Rocket
            );
            assert!(s.events.iter().any(|e| {
                e.kind == SimEventType::Impact && e.cover_kind == Some(CoverKind::Timber)
            }));
            if weapon != Weapon::Rocket {
                assert!(
                    angvel(&s, body).y.abs() > 0.01,
                    "off-center shots turn the wood"
                );
                assert!(
                    linvel(&s, body).x <= 5.01,
                    "light wood receives a bounded shove"
                );
            }
        }
    }
}

#[test]
fn timber_shots_respect_nearer_cover_gaps_and_debris_cleanup() {
    let mut s = arena();
    let wall = cover(&mut s, CoverKind::Timber, 20.0, 20.0);
    s.damage_cover(wall, 999.0, 999, Team::Blue, None, None);
    let f = fragment_where(&s, |fragment| {
        fragment
            .timber_part
            .as_ref()
            .is_some_and(|part| part.kind == TimberPartKind::Beam)
    });
    park_others(&mut s, &[f]);
    let body = fragment_body(&s, f);
    let half = find(&s, f).unwrap().timber_part.as_ref().unwrap().h / 2.0;
    level(&mut s, body);
    park(&mut s, body, 0.0, half, 0.0);
    let blocker = cover(&mut s, CoverKind::Concrete, -2.0, 0.0);
    s.covers[blocker].hp = f64::INFINITY;
    shot(&mut s, Weapon::Standard, 1.0);
    assert_eq!(s.shots.len(), 0);
    assert_eq!(linvel(&s, body).x, 0.0, "nearer cover protects the beam");
    let blocker_body = s.covers[blocker].body;
    s.remove_body(blocker_body);
    s.covers.clear();
    s.cover_by_collider.clear();
    park(&mut s, body, 0.0, half, 2.0);
    s.world.step();
    shot(&mut s, Weapon::Standard, 1.0);
    assert_eq!(s.shots.len(), 1, "shot beside the beam misses");
    s.shots.clear();
    park(&mut s, body, 0.0, half, 0.0);
    let collider = s.world.bodies[body].colliders()[0];
    s.world.colliders[collider].set_collision_groups(interaction_groups(group::FRAGMENT));
    s.world.step();
    shot(&mut s, Weapon::Standard, 1.0);
    assert_eq!(
        s.shots.len(),
        1,
        "sinking timber no longer intercepts shells"
    );
}

#[test]
fn a_scout_pushes_fallen_logs_beams_panels_and_drum_pieces_while_small_chips_stay_nonblocking() {
    for (kind, shape) in [
        (CoverKind::Tree, FragmentShape::Log),
        (CoverKind::Timber, FragmentShape::Beam),
        (CoverKind::Cargo, FragmentShape::Panel),
        (CoverKind::Drum, FragmentShape::DrumShell),
        (CoverKind::Drum, FragmentShape::DrumLid),
    ] {
        let mut s = arena();
        // Destroy the source away from the push lane; rooted stumps stay solid.
        let c = cover(&mut s, kind, 20.0, 20.0);
        s.damage_cover(c, 999.0, 999, Team::Blue, None, None);
        let f = fragment_where(&s, |fragment| fragment.shape == Some(shape));
        park_others(&mut s, &[f]);
        let body = fragment_body(&s, f);
        // Lay tall panels and logs flat, exercising ground contact rather than upright props.
        let tipped = shape == FragmentShape::Log || shape == FragmentShape::Panel;
        let rotation = Quat4 {
            x: if tipped { FRAC_1_SQRT_2 } else { 0.0 },
            y: 0.0,
            z: 0.0,
            w: if tipped { FRAC_1_SQRT_2 } else { 1.0 },
        };
        s.world.bodies[body].set_rotation(to_rotation(rotation), true);
        park(&mut s, body, 0.0, 2.0, 0.0);
        s.world.bodies[body].wake_up(true);
        tick(&mut s, 1.5);
        let start_x = s.body_translation(body).x;
        let tank = shove_with_scout(&mut s, -4.0, 120);
        assert!(
            s.body_translation(body).x > start_x + 1.0,
            "{shape:?} must move through tank contact"
        );
        let tank_body = s.tanks[tank].body;
        assert!(
            s.body_translation(tank_body).x > 0.0,
            "{shape:?} must not trap the scout"
        );
        s.fragment(0.0, 0.0, 0x999999, 0.4, FragmentShape::Shard, 1.0);
        let chip = s.fragments.last().unwrap().body;
        assert_eq!(first_collider_groups(&s, chip), group::FRAGMENT);
        let elapsed = s.elapsed;
        let piece = s
            .fragments
            .iter_mut()
            .find(|fragment| fragment.id == f)
            .unwrap();
        piece.life = DEBRIS_CLEANUP_SECONDS + STEP / 2.0;
        piece.expires_at = Some(elapsed + DEBRIS_CLEANUP_SECONDS);
        idle(&mut s);
        assert_eq!(
            first_collider_groups(&s, body),
            group::FRAGMENT,
            "sinking pieces cannot block tanks"
        );
        let life = find(&s, f).unwrap().life;
        let position = s.body_translation(body);
        blast_debris(&mut s, position.planar(), 5.0, 100.0);
        assert_eq!(
            find(&s, f).unwrap().life,
            life,
            "cleanup cannot be restarted by another blast"
        );
        tick(&mut s, 1.1);
        assert!(!s.world.bodies.contains(body));
    }
}

#[test]
fn barrels_rupture_radially_and_a_centered_blast_adds_no_sideways_bias() {
    let mut s = arena();
    let c = cover(&mut s, CoverKind::Drum, 0.0, 0.0);
    s.damage_cover(c, 999.0, 999, Team::Blue, None, None);
    let drum = s.covers[c].clone();
    let scraps: Vec<_> = s
        .fragments
        .iter()
        .filter(|f| f.shape == Some(FragmentShape::DrumShell))
        .collect();
    assert!(scraps.iter().any(|f| linvel(&s, f.body).x < 0.0));
    assert!(scraps.iter().any(|f| linvel(&s, f.body).x > 0.0));
    for f in &scraps {
        let p = s.body_translation(f.body);
        let v = linvel(&s, f.body);
        assert!((p.x - drum.x) * v.x + (p.z - drum.z) * v.z > 0.0);
        let size = f.dimensions.unwrap();
        assert!(size.x < drum.w / 2.0 && size.y < drum.h / 2.0);
    }
    let lid = s
        .fragments
        .iter()
        .find(|f| f.shape == Some(FragmentShape::DrumLid))
        .unwrap()
        .body;
    s.world.bodies[lid].set_linvel(vector(0.0, 0.0, 0.0), true);
    blast_debris(&mut s, Vec2::new(drum.x, drum.z), 6.0, 75.0);
    let v = linvel(&s, lid);
    assert_eq!(v.x, 0.0);
    assert_eq!(v.z, 0.0);
    assert!(v.y > 0.0);
}

#[test]
fn large_wreck_pieces_land_on_each_other_and_settle_instead_of_interpenetrating() {
    // The TS test replaced `rng.next` with a constant 0.1 to separate the gun. The seeded
    // stream cannot be mocked, so pick the first seed whose breakup detaches the gun.
    let (mut s, turret) = (0..)
        .find_map(|seed| {
            let mut s = arena();
            s.rng = Random::new(seed as f64);
            let turret = wreck(&mut s);
            s.fragments
                .iter()
                .any(|f| f.part == Some(WreckPart::Barrel))
                .then_some((s, turret))
        })
        .unwrap();
    let hull = fragment_where(&s, |f| f.part == Some(WreckPart::Hull));
    let gun = fragment_where(&s, |f| f.part == Some(WreckPart::Barrel));
    let (hull, turret, gun) = (
        fragment_body(&s, hull),
        fragment_body(&s, turret),
        fragment_body(&s, gun),
    );
    let initial_hull = s.body_translation(hull);
    let initial_turret = s.body_translation(turret);
    let half_height = |s: &Simulation, body: RigidBodyHandle| {
        let collider = s.world.bodies[body].colliders()[0];
        s.world.colliders[collider]
            .shape()
            .as_cuboid()
            .unwrap()
            .half_extents
            .y as f64
    };
    let half_hull = half_height(&s, hull);
    let half_turret = half_height(&s, turret);
    assert!(
        initial_turret.y - initial_hull.y > half_hull + half_turret,
        "spawn poses start clear"
    );
    let bodies: Vec<_> = s.fragments.iter().map(|f| f.body).collect();
    for body in bodies {
        level(&mut s, body);
        park(&mut s, body, 30.0, 1.0, 0.0);
    }
    park(&mut s, hull, 0.0, half_hull + 0.02, 0.0);
    park(&mut s, turret, 0.0, 3.0, 0.0);
    park(&mut s, gun, 0.0, 5.0, 0.0);
    s.world.bodies[gun].wake_up(true);
    s.world.bodies[turret].wake_up(true);
    for _ in 0..480 {
        s.world.step();
    }
    let bottom = s.body_translation(hull);
    let top = s.body_translation(turret);
    assert!(
        top.y - bottom.y > half_hull + half_turret - 0.04,
        "turret rests above hull: {bottom:?} {top:?}"
    );
    assert!((top.x - bottom.x).abs() < 0.2 && (top.z - bottom.z).abs() < 0.2);
    assert!(sleeping(&s, hull));
    assert!(sleeping(&s, turret));
    assert!(
        s.body_translation(gun).y > top.y + half_turret,
        "gun rests on the turret"
    );
    assert!(sleeping(&s, gun));
}

#[test]
fn all_substantial_debris_shares_contacts_stacks_across_categories_and_excludes_small_scraps() {
    let groups = [group::PUSHABLE_DEBRIS, group::TIMBER_DEBRIS, group::WRECK];
    for a in groups {
        for b in groups {
            assert!(collides(a, b));
        }
        assert!(!collides(a, group::FRAGMENT));
        for b in [
            group::GROUND,
            group::COVER,
            group::MOVABLE_COVER,
            group::TANK,
        ] {
            assert!(collides(a, b));
        }
    }
    let mut s = arena();
    let bodies: Vec<_> = groups
        .iter()
        .enumerate()
        .map(|(i, &groups)| {
            let body = s
                .world
                .insert_body(RigidBodyBuilder::dynamic().translation(vector(
                    0.0,
                    0.52 + i as f64 * 2.0,
                    0.0,
                )));
            s.world.insert_collider(
                ColliderBuilder::cuboid(1.0, 0.5, 1.0)
                    .collision_groups(interaction_groups(groups))
                    .friction(0.8),
                Some(body),
            );
            body
        })
        .collect();
    for _ in 0..480 {
        s.world.step();
    }
    for (i, &body) in bodies.iter().enumerate() {
        let y = s.body_translation(body).y;
        assert!((y - (0.5 + i as f64)).abs() < 0.06, "layer {i} at {y}");
        assert!(sleeping(&s, body));
    }
}

#[test]
fn shells_clear_low_debris_to_hit_a_tank_but_upright_debris_intercepts_the_same_trajectory() {
    for groups in [group::PUSHABLE_DEBRIS, group::TIMBER_DEBRIS, group::WRECK] {
        for upright in [false, true] {
            let mut s = arena();
            let target = s.add_tank(Team::Red, false, VehicleKind::Balanced, 0);
            s.tanks[target].protection = 0.0;
            let target_body = s.tanks[target].body;
            park(&mut s, target_body, 2.0, 0.65, 0.0);
            let half = if upright { 1.0 } else { 0.125 };
            let body = s
                .world
                .insert_body(RigidBodyBuilder::dynamic().translation(vector(
                    -1.0,
                    if upright { 1.0 } else { 0.125 },
                    0.0,
                )));
            s.world.insert_collider(
                ColliderBuilder::cuboid(0.5, half as f32, 0.5)
                    .collision_groups(interaction_groups(groups))
                    .mass(0.3),
                Some(body),
            );
            let id = s.next_id;
            s.next_id += 1;
            let mut debris = Fragment::new(id, body, 8.0, 1.0, 0x805336);
            debris.dimensions = Some(Point3::new(1.0, if upright { 2.0 } else { 0.25 }, 1.0));
            debris.material = Some(DebrisMaterial::Wood);
            s.fragments.push(debris);
            s.world.bodies[body].sleep();
            s.world.step();
            let hp = s.tanks[target].hp;
            shot(&mut s, Weapon::Standard, 1.0);
            assert_eq!(s.shots.len(), 0);
            assert_eq!(s.tanks[target].hp, if upright { hp } else { hp - 40.0 });
            assert_eq!(
                linvel(&s, body).x > 0.0,
                upright,
                "only an actual debris hit pushes it"
            );
        }
    }
}

#[test]
fn identical_hits_and_blasts_move_wood_more_than_hulls_and_hulls_more_than_concrete() {
    let mut s = arena();
    let wall = cover(&mut s, CoverKind::Timber, 20.0, 20.0);
    s.damage_cover(wall, 999.0, 999, Team::Blue, None, None);
    let wood = fragment_where(&s, |f| {
        f.timber_part
            .as_ref()
            .is_some_and(|part| part.kind == TimberPartKind::Beam)
    });
    wreck(&mut s);
    let hull = fragment_where(&s, |f| f.part == Some(WreckPart::Hull));
    let concrete = cover(&mut s, CoverKind::Teeth, 20.0, 20.0);
    let shell = Shot {
        id: 0,
        owner: 999,
        team: Team::Blue,
        x: 1.0,
        z: 0.0,
        y: Some(0.5),
        vx: 25.0,
        vz: 0.0,
        damage: 40.0,
        life: 2.0,
        bounces: 0,
        piercing: 0,
        weapon: Weapon::Standard,
        ..Shot::default()
    };
    let (wood_body, hull_body) = (fragment_body(&s, wood), fragment_body(&s, hull));
    let concrete_body = s.covers[concrete].body;
    for f in [wood, hull] {
        let body = fragment_body(&s, f);
        park(&mut s, body, 1.0, 0.5, 0.0);
        let index = s
            .fragments
            .iter()
            .position(|fragment| fragment.id == f)
            .unwrap();
        hit_projectile_debris(&mut s, index, &shell, Point3::new(1.0, 0.5, 0.0));
    }
    park(&mut s, concrete_body, 1.0, 0.5, 0.0);
    hit_movable_cover(&mut s, concrete, &shell);
    assert!(linvel(&s, wood_body).x > linvel(&s, hull_body).x);
    assert!(linvel(&s, hull_body).x > linvel(&s, concrete_body).x);
    for body in [wood_body, hull_body, concrete_body] {
        park(&mut s, body, 1.0, 0.5, 0.0);
    }
    blast_debris(&mut s, Vec2::ZERO, 5.0, 60.0);
    assert!(linvel(&s, wood_body).x > linvel(&s, hull_body).x);
    assert!(linvel(&s, hull_body).x > linvel(&s, concrete_body).x);
    let mass = |body: RigidBodyHandle| s.world.bodies[body].mass();
    assert!(mass(hull_body) > mass(wood_body));
    assert!(mass(concrete_body) > mass(hull_body));
}
