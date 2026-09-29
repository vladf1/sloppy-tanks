//! Authored map data keeps both teams equal: point-symmetric pickups, spawns and cover, and
//! every spawn, pickup and flank lane clear of cover and reachable from both spawn lines
//! (the former `tests/map-layout.test.ts`).

use rapier3d::prelude::{ColliderHandle, RigidBodyHandle};
use sloppy_core::sim::ammunition::is_special_ammo;
use sloppy_core::sim::arena::{CoverDef, PICKUP_LAYOUT, spawn_positions};
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::maps::MAPS;
use sloppy_core::sim::math::{Vec2, distance};
use sloppy_core::sim::navigation::Navigation;
use sloppy_core::sim::{Cover, PickupKind, Team};

/// Clear floor a tank hull needs around a spawn, pickup or flank waypoint.
const HULL_CLEARANCE: f64 = 2.0;
/// A route ends in the navigation cell that contains its goal.
const ARRIVAL: f64 = 1.8;

/// Open lanes along both far edges that each map must keep reachable.
fn flanks() -> Vec<Vec2> {
    [-52.0, 52.0]
        .into_iter()
        .flat_map(|z| [-45.0, 0.0, 45.0].map(|x| Vec2::new(x, z)))
        .collect()
}

fn spawns() -> Vec<Vec2> {
    let mut points = spawn_positions(Team::Blue, 1.0).to_vec();
    points.extend(spawn_positions(Team::Red, 1.0));
    points
}

fn pickup_points() -> Vec<Vec2> {
    PICKUP_LAYOUT.iter().map(|p| Vec2::new(p.x, p.z)).collect()
}

/// A living cover record for navigation only; it has no physics body.
fn nav_cover(def: &CoverDef) -> Cover {
    Cover {
        id: 0,
        kind: def.kind,
        x: def.x,
        z: def.z,
        w: def.w,
        d: def.d,
        h: def.h,
        hp: def.hp,
        max_hp: def.hp,
        alive: true,
        destructible: def.hp.is_finite(),
        body: RigidBodyHandle::invalid(),
        collider: ColliderHandle::invalid(),
        color: def.color,
        debris_seed: def.debris_seed,
        timber_hits: Vec::new(),
        timber_join: def.timber_join,
        timber_kick: None,
        motion: None,
        fallen_at: None,
    }
}

#[test]
fn pickups_and_spawn_slots_are_point_symmetric_and_ammo_and_repairs_sit_away_from_spawn_pads() {
    for p in &PICKUP_LAYOUT {
        assert!(
            PICKUP_LAYOUT
                .iter()
                .any(|o| o.kind == p.kind && o.x == -p.x && o.z == -p.z),
            "{p:?}"
        );
    }
    let (a, b) = (
        spawn_positions(Team::Blue, 1.0),
        spawn_positions(Team::Red, 1.0),
    );
    assert_eq!(a.len(), b.len());
    for i in 0..a.len() {
        assert_eq!([a[i].x, a[i].z], [-b[i].x, -b[i].z]);
    }
    let ammo: Vec<_> = PICKUP_LAYOUT
        .iter()
        .filter(|p| is_special_ammo(p.kind))
        .collect();
    assert_eq!(ammo.len(), 8);
    let mut kinds: Vec<PickupKind> = Vec::new();
    for p in &ammo {
        if !kinds.contains(&p.kind) {
            kinds.push(p.kind);
        }
    }
    for kind in kinds {
        assert_eq!(
            ammo.iter().filter(|p| p.kind == kind).count(),
            2,
            "{kind:?}"
        );
    }
    for p in &ammo {
        assert!(p.x.hypot(p.z) > 20.0, "ammo is contested away from center");
    }
    let repairs: Vec<_> = PICKUP_LAYOUT
        .iter()
        .filter(|p| p.kind == PickupKind::Repair)
        .collect();
    assert_eq!(repairs.len(), 4);
    for p in &repairs {
        for spawn in spawns() {
            assert!(
                distance(Vec2::new(p.x, p.z), spawn) >= 8.0,
                "repair at {},{} is too close to a spawn pad",
                p.x,
                p.z
            );
        }
    }
}

fn assert_map_is_symmetric_clear_and_reachable(id: MapId) {
    let map = MAPS.iter().find(|map| map.id == id).expect("standard map");
    let layout = (map.layout)();
    for c in &layout {
        assert!(
            layout.iter().any(|o| o.kind == c.kind
                && o.x == -c.x
                && o.z == -c.z
                && o.w == c.w
                && o.d == c.d
                && o.hp == c.hp),
            "unpaired cover {c:?}"
        );
    }
    let mut nav = Navigation::new();
    let covers: Vec<Cover> = layout.iter().map(nav_cover).collect();
    nav.rebuild(&covers, None);
    let mut points = spawns();
    points.extend(pickup_points());
    points.extend(flanks());
    let starts = [
        spawn_positions(Team::Blue, 1.0)[0],
        spawn_positions(Team::Red, 1.0)[0],
    ];
    for point in points {
        let label = format!("{{x: {}, z: {}}}", point.x, point.z);
        for c in &layout {
            assert!(
                (point.x - c.x).abs() >= c.w / 2.0 + HULL_CLEARANCE
                    || (point.z - c.z).abs() >= c.d / 2.0 + HULL_CLEARANCE,
                "hull clearance {label} / {:?} at {},{}",
                c.kind,
                c.x,
                c.z
            );
        }
        assert_eq!(nav.blocked[nav.index(point)], 0, "blocked {label}");
        // Both teams' spawn lines must reach every point, not only the nearer one.
        for start in starts {
            if start.x == point.x && start.z == point.z {
                continue;
            }
            let end = nav.find(start, point).last().copied();
            assert!(
                end.is_some_and(|end| distance(end, point) < ARRIVAL),
                "unreachable {label}"
            );
        }
    }
}

#[test]
fn pine_village_cover_is_point_symmetric_and_leaves_every_spawn_pickup_and_flank_clear_and_reachable()
 {
    assert_map_is_symmetric_clear_and_reachable(MapId::Village);
}

#[test]
fn harbor_cover_is_point_symmetric_and_leaves_every_spawn_pickup_and_flank_clear_and_reachable() {
    assert_map_is_symmetric_clear_and_reachable(MapId::Harbor);
}

#[test]
fn quarry_cover_is_point_symmetric_and_leaves_every_spawn_pickup_and_flank_clear_and_reachable() {
    assert_map_is_symmetric_clear_and_reachable(MapId::Quarry);
}

#[test]
fn every_standard_map_has_a_layout_test() {
    let covered = [MapId::Village, MapId::Harbor, MapId::Quarry];
    assert!(MAPS.iter().all(|map| covered.contains(&map.id)));
}
