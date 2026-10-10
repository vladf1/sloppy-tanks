//! Authored map data keeps both teams equal: point-symmetric pickups, spawns and cover, and
//! every spawn, pickup and flank lane clear of cover and reachable from both spawn lines
//! (the former `tests/map-layout.test.ts`).

use sloppy_core::sim::arena::{PICKUP_LAYOUT, spawn_positions};
use sloppy_core::sim::maps::MAPS;
use sloppy_core::sim::math::{Vec2, distance};
use sloppy_core::sim::{PickupKind, Simulation, SimulationSetup, Team};

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
        .filter(|p| p.kind.special_ammo().is_some())
        .collect();
    assert_eq!(ammo.len(), 8);
    for p in &ammo {
        assert_eq!(
            ammo.iter().filter(|o| o.kind == p.kind).count(),
            2,
            "{:?}",
            p.kind
        );
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

#[test]
fn every_standard_map_is_point_symmetric_and_leaves_every_spawn_pickup_and_flank_clear_and_reachable()
 {
    for map in &MAPS {
        let id = map.id.as_str();
        let layout = (map.layout)();
        for c in &layout {
            assert!(
                layout.iter().any(|o| o.kind == c.kind
                    && o.x == -c.x
                    && o.z == -c.z
                    && o.w == c.w
                    && o.d == c.d
                    && o.hp == c.hp),
                "{id}: unpaired cover {c:?}"
            );
        }
        // The navigation the game builds for this map.
        let mut nav = Simulation::new(
            123.0,
            SimulationSetup {
                map_mode: Some(map.id),
                ..SimulationSetup::default()
            },
        )
        .nav;
        let mut points = spawns();
        points.extend(pickup_points());
        points.extend(flanks());
        let starts = [
            spawn_positions(Team::Blue, 1.0)[0],
            spawn_positions(Team::Red, 1.0)[0],
        ];
        for point in points {
            let label = format!("{id} {{x: {}, z: {}}}", point.x, point.z);
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
            assert!(!nav.is_blocked(point), "blocked {label}");
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
}
