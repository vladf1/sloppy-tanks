//! Quarry simulation rules: the open central crossing, the crate cut that opens only once
//! destroyed, indestructible rock and barriers, barrier collision that follows the visible
//! concrete and steel, and the mirrored barrier belts and supply bays (the simulation parts
//! of the former `tests/quarry.test.ts`; scree, soil, dust and scenery are presentation).

use rapier3d::prelude::{Collider, ColliderHandle, Ray};
use sloppy_core::sim::data::group;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::math::Vec2;
use sloppy_core::sim::physics::{query_filter, vector};
use sloppy_core::sim::quarry_layout::quarry_layout;
use sloppy_core::sim::{CoverKind, Simulation};

fn quarry() -> Simulation {
    let mut sim = Simulation::with_seed(417.0);
    sim.map_mode = MapId::Quarry;
    sim.reset(None);
    sim
}

fn human(s: &Simulation) -> usize {
    s.human_index().expect("local play has a human")
}

fn damage_by_human(sim: &mut Simulation, cover: usize, amount: f64) {
    let h = human(sim);
    let (id, team) = (sim.tanks[h].id, sim.human_team);
    sim.damage_cover(cover, amount, id, team, None, None);
}

#[test]
fn the_central_crossing_is_open_and_the_crate_cut_opens_to_tanks_only_after_destruction() {
    let mut sim = quarry();
    assert!(
        sim.nav
            .clear_line(Vec2::new(-48.0, 0.0), Vec2::new(48.0, 0.0)),
        "central crossing remains open"
    );
    let a = Vec2::new(32.5, 26.0);
    let b = Vec2::new(32.5, 48.0);
    assert!(!sim.nav.clear_line(a, b));
    let crates: Vec<usize> = (0..sim.covers.len())
        .filter(|&c| {
            let cover = &sim.covers[c];
            cover.kind == CoverKind::Cargo && cover.x > 30.0 && cover.z > 30.0
        })
        .collect();
    for c in crates {
        damage_by_human(&mut sim, c, 30.0);
        let at = Vec2::new(sim.covers[c].x, sim.covers[c].z);
        assert_eq!(sim.nav.blocked[sim.nav.index(at)], 1);
        damage_by_human(&mut sim, c, 30.0);
        assert!(!sim.cover_by_collider.contains_key(&sim.covers[c].collider));
    }
    assert!(
        sim.nav.clear_line(a, b),
        "all four supply crates open the rock cut"
    );
    // Barriers and rock survive.
    let permanent: Vec<usize> = (0..sim.covers.len())
        .filter(|&c| {
            matches!(
                sim.covers[c].kind,
                CoverKind::Rock | CoverKind::Teeth | CoverKind::Hedgehog
            )
        })
        .collect();
    for c in permanent {
        damage_by_human(&mut sim, c, 10000.0);
        assert!(sim.covers[c].alive);
        let at = Vec2::new(sim.covers[c].x, sim.covers[c].z);
        assert_eq!(sim.nav.blocked[sim.nav.index(at)], 1);
    }
}

/// Time of impact of a ray toward +Z that only considers `cover`'s own colliders, or -1.
fn ray(sim: &Simulation, cover: usize, x: f64, y: f64) -> f64 {
    let cover = &sim.covers[cover];
    let body = cover.body;
    let own = |_: ColliderHandle, collider: &Collider| collider.parent() == Some(body);
    let ray = Ray::new(vector(cover.x + x, y, cover.z - 4.0), vector(0.0, 0.0, 1.0));
    sim.world
        .cast_ray(
            &ray,
            8.0,
            true,
            query_filter(group::COVER_QUERY).predicate(&own),
        )
        .map_or(-1.0, |(_, toi)| toi as f64)
}

fn leftmost(sim: &Simulation, kind: CoverKind) -> usize {
    (0..sim.covers.len())
        .filter(|&c| sim.covers[c].kind == kind)
        .min_by(|&a, &b| sim.covers[a].x.total_cmp(&sim.covers[b].x))
        .unwrap()
}

/// Drives the human toward +X at 7 m/s for three seconds, starting 4 m before `cover`.
fn ram(sim: &mut Simulation, tank_body: rapier3d::prelude::RigidBodyHandle, cover: usize) {
    let (x, z) = (sim.covers[cover].x, sim.covers[cover].z);
    sim.world.bodies[tank_body].set_translation(vector(x - 4.0, 0.65, z), true);
    for _ in 0..180 {
        sim.world.bodies[tank_body].set_linvel(vector(7.0, 0.0, 0.0), true);
        sim.world.step();
    }
}

#[test]
fn barrier_collision_follows_tapered_concrete_and_open_steel_rather_than_invisible_boxes() {
    let mut sim = quarry();
    sim.world.step();
    let tooth = leftmost(&sim, CoverKind::Teeth);
    let hedgehog = leftmost(&sim, CoverKind::Hedgehog);
    assert!(ray(&sim, tooth, 0.0, 1.0) >= 0.0);
    assert_eq!(
        ray(&sim, tooth, 0.8, 1.7),
        -1.0,
        "shot clears the sloping shoulder"
    );
    // The tank footprint never creates invisible cover for shells.
    let tooth_body = sim.covers[tooth].body;
    let own = |_: ColliderHandle, collider: &Collider| collider.parent() == Some(tooth_body);
    let shoulder = Ray::new(
        vector(sim.covers[tooth].x + 0.8, 1.7, sim.covers[tooth].z - 4.0),
        vector(0.0, 0.0, 1.0),
    );
    assert_eq!(
        sim.world.cast_ray(
            &shoulder,
            8.0,
            true,
            query_filter(group::COVER_QUERY).predicate(&own)
        ),
        None,
        "the tank footprint never creates invisible cover for shells"
    );
    assert!(
        ray(&sim, hedgehog, 0.0, 1.3) >= 0.0,
        "central steel stops a shot"
    );
    assert_eq!(
        ray(&sim, hedgehog, 0.95, 1.3),
        -1.0,
        "visible opening between steel arms remains open"
    );
    let tank_body = sim.human().body;
    ram(&mut sim, tank_body, tooth);
    assert!(
        sim.body_translation(tank_body).x < sim.body_translation(sim.covers[tooth].body).x,
        "a tank pushes concrete but cannot pass through it"
    );
    ram(&mut sim, tank_body, hedgehog);
    let steel = sim.body_translation(sim.covers[hedgehog].body);
    let authored = &sim.covers[hedgehog];
    assert!(
        sim.body_translation(tank_body).x < steel.x
            || (steel.x - authored.x).hypot(steel.z - authored.z) > 0.8,
        "a tank remains blocked unless it physically pushes the steel aside"
    );
}

/// `Math.sign`: zero stays zero, unlike `f64::signum`.
fn sign(value: f64) -> f64 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        0.0
    }
}

#[test]
fn quarry_defenses_form_mirrored_belts_and_supply_bays_use_individual_crates() {
    let layout = quarry_layout();
    for side in [-1.0, 1.0] {
        let teeth: Vec<_> = layout
            .iter()
            .filter(|c| c.kind == CoverKind::Teeth && sign(c.x) == side)
            .collect();
        assert_eq!(teeth.len(), 8);
        assert_eq!(
            teeth.iter().filter(|c| c.x.abs() < 43.0).count(),
            4,
            "inner staggered rank"
        );
        assert_eq!(
            teeth.iter().filter(|c| c.x.abs() > 44.0).count(),
            4,
            "outer staggered rank"
        );
        let mut xs: Vec<f64> = teeth.iter().map(|c| c.x).collect();
        xs.sort_by(f64::total_cmp);
        xs.dedup();
        assert!(xs.len() > 4, "individual placement breaks straight lines");
        let steel: Vec<_> = layout
            .iter()
            .filter(|c| c.kind == CoverKind::Hedgehog && sign(c.x) == side)
            .collect();
        assert_eq!(steel.len(), 4);
        let mut zs: Vec<f64> = steel.iter().map(|c| c.z).collect();
        zs.sort_by(f64::total_cmp);
        zs.dedup();
        assert_eq!(zs.len(), 1, "one aligned steel belt");
        let barriers: Vec<_> = teeth.iter().chain(&steel).copied().collect();
        for barrier in &barriers {
            assert!(
                barriers.iter().any(|other| !std::ptr::eq(*other, *barrier)
                    && other.kind == barrier.kind
                    && (other.x - barrier.x).hypot(other.z - barrier.z) < 3.6),
                "every obstacle belongs to a connected barrier"
            );
        }
        let crates: Vec<_> = layout
            .iter()
            .filter(|c| c.kind == CoverKind::Cargo && c.x * side > 30.0 && c.z * side > 30.0)
            .collect();
        assert_eq!(crates.len(), 4);
        assert!(crates.iter().all(|c| c.w <= 2.4 && c.d <= 2.8));
        for crate_def in &crates {
            assert!(
                layout
                    .iter()
                    .filter(|c| c.kind == CoverKind::Rock)
                    .all(
                        |rock| (crate_def.x - rock.x).abs() >= (crate_def.w + rock.w) / 2.0
                            || (crate_def.z - rock.z).abs() >= (crate_def.d + rock.d) / 2.0
                    ),
                "supply crates never intersect rock footprints"
            );
        }
    }
}
