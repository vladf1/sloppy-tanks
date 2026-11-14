//! Senda stands `SENDA_SCALE` times its design: its cover is its design's scaled about
//! the arena's middle, a map larger than the standard arena lays its navigation grid over
//! its whole level (a standard map's grid is unchanged), a house stands as its design's
//! structure scaled, tanks drive every street, and a room's client rebuilds the same
//! cover and houses from the scene.

use crate::net_support;

use glam::{DVec2, DVec3};
use sloppy_core::sim::arena::spawn_positions;
use sloppy_core::sim::extra_levels::extra_level;
use sloppy_core::sim::house_layout::{HouseDesign, design_place};
use sloppy_core::sim::house_structure::{shared_house_structure, standing_house_structure};
use sloppy_core::sim::level_rules::standard_rules;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::math::{Vec2, distance};
use sloppy_core::sim::model_data::ARENA;
use sloppy_core::sim::navigation::{GRID_SIZE, Navigation};
use sloppy_core::sim::senda_layout::{
    CUL_DE_SAC_ISLAND, PARKWAY, SENDA_MAP, SENDA_PARKWAY_HALF, SENDA_SCALE, SIDEWALK, Street,
    in_median_opening, inside_boundary, left, senda_boundary, senda_design_layout, senda_houses,
};
use sloppy_core::sim::structure::{PartShape, structure_of};
use sloppy_core::sim::tree_proportions::tree_proportions;
use sloppy_core::sim::{CoverKind, Simulation, Team};

/// A route ends in the navigation cell that contains its goal.
const ARRIVAL: f64 = 2.3;
/// How far (design metres) in from Senda Parkway's kerb its kerb lane runs.
const KERB_LANE: f64 = 1.0;

fn senda(seed: f64) -> Simulation {
    let streets = extra_level(MapId::Neighborhood).expect("the Neighborhood has its own streets");
    Simulation::new(seed, standard_rules().merged(streets))
}

#[test]
fn the_maps_cover_is_its_design_scaled_about_the_middle() {
    assert_eq!(SENDA_MAP.scale, Some(SENDA_SCALE));
    let design = senda_design_layout();
    let world = (SENDA_MAP.layout)();
    assert_eq!(world.len(), design.len());
    let mut moved = 0.0f64;
    for (d, w) in design.iter().zip(&world) {
        assert_eq!(
            (w.kind, w.color, w.hp, w.house_plan),
            (d.kind, d.color, d.hp, d.house_plan)
        );
        assert_eq!(
            (w.w, w.d, w.h),
            (d.w * SENDA_SCALE, d.d * SENDA_SCALE, d.h * SENDA_SCALE),
            "{d:?}"
        );
        if d.kind == CoverKind::Tree {
            // A tree stands just east of its scaled place, where it grows its family.
            let family = |x: f64, z: f64| tree_proportions(x, z, 1.0, 1.0, 1.0).family;
            assert_eq!(family(w.x, w.z), family(d.x, d.z), "{d:?}");
            assert_eq!(w.z, d.z * SENDA_SCALE);
            let east = w.x - d.x * SENDA_SCALE;
            assert!((0.0..1.0).contains(&east), "a tree {east} m off at {d:?}");
            moved = moved.max(east);
        } else {
            assert_eq!((w.x, w.z), (d.x * SENDA_SCALE, d.z * SENDA_SCALE), "{d:?}");
        }
    }
    // The level fills the arena to its scaled edge.
    let reach = world
        .iter()
        .filter(|c| c.kind == CoverKind::Boundary)
        .map(|c| c.x.abs().max(c.z.abs()))
        .fold(0.0, f64::max);
    assert!(reach > ARENA * SENDA_SCALE, "the wall reaches {reach} m");
    assert!(moved < 0.5, "a tree moved {moved} m");
}

#[test]
fn a_larger_maps_navigation_covers_its_level_and_a_standard_maps_is_unchanged() {
    let standard = Navigation::new();
    for scale in [1.0, 0.65] {
        let mut nav = Navigation::new();
        nav.set_map_scale(scale);
        assert_eq!(
            (nav.grid_size(), nav.cell_size(), nav.half_extent()),
            (GRID_SIZE, standard.cell_size(), standard.half_extent())
        );
        for cell in [0, 1, GRID_SIZE + 7, GRID_SIZE * GRID_SIZE - 1] {
            assert_eq!(nav.point(cell), standard.point(cell));
        }
    }
    assert_eq!(standard.half_extent(), ARENA);
    // The Neighborhood's grid has more cells of the same size, out to its scaled edge.
    let mut sim = senda(5.0);
    assert_eq!(sim.nav.half_extent(), ARENA * SENDA_SCALE);
    assert_eq!(sim.nav.cell_size(), standard.cell_size());
    assert_eq!(sim.nav.grid_size(), 100);
    assert_eq!(sim.nav.blocked.len(), 100 * 100);
    let corner = sim.nav.point(0);
    assert!(corner.x < -ARENA * SENDA_SCALE + sim.nav.cell_size());
    // Every corner of the wall falls inside it, and a route crosses the whole of it.
    let nav = &mut sim.nav;
    for at in senda_boundary() {
        let world = at * SENDA_SCALE;
        assert!(world.x.abs().max(world.y.abs()) <= nav.half_extent() + 1e-9);
    }
    let (from, to) = (
        spawn_positions(Team::Blue, SENDA_SCALE)[4],
        spawn_positions(Team::Red, SENDA_SCALE)[4],
    );
    let path = nav.find(from, to);
    assert!(
        path.last().is_some_and(|end| distance(*end, to) < ARRIVAL),
        "{from:?} to {to:?}"
    );
    // A standard map's grid is the standard one after a larger map's.
    nav.clear();
    nav.set_map_scale(1.0);
    assert_eq!(
        (nav.grid_size(), nav.blocked.len()),
        (GRID_SIZE, GRID_SIZE * GRID_SIZE)
    );
    // A standard map's simulation keeps the standard grid.
    let village = Simulation::new(5.0, standard_rules());
    assert_eq!(village.nav.half_extent(), ARENA);
    assert_eq!(village.nav.grid_size(), GRID_SIZE);
}

#[test]
fn a_senda_house_stands_as_its_designs_structure_scaled() {
    let sim = senda(5.0);
    let mut checked = 0;
    for cover in sim.covers.iter().filter(|c| c.kind == CoverKind::House) {
        let plan = cover.house_plan.expect("a planned house");
        let design =
            HouseDesign::for_cover(cover.x, cover.z, cover.w, cover.d, cover.h, Some(plan));
        assert_eq!(design.scale(), SENDA_SCALE);
        let built = shared_house_structure(&design);
        let standing = &cover.structure.as_ref().expect("built of parts").layout;
        assert!(std::sync::Arc::ptr_eq(
            standing,
            &standing_house_structure(&design)
        ));
        assert_eq!(standing.parts.len(), built.parts.len());
        assert_eq!(standing.pieces.len(), built.pieces.len());
        assert_eq!(standing.supports, built.supports);
        assert_eq!(standing.joints, built.joints);
        assert_eq!(standing.breakups, built.breakups);
        assert_eq!(standing.yaw, built.yaw);
        for (scaled, part) in standing
            .parts
            .iter()
            .chain(&standing.pieces)
            .zip(built.parts.iter().chain(&built.pieces))
        {
            assert_eq!(scaled.min, part.min * SENDA_SCALE);
            assert_eq!(scaled.max, part.max * SENDA_SCALE);
            assert_eq!(scaled.mass_center, part.mass_center * SENDA_SCALE);
            assert_eq!(scaled.mass, part.mass, "the design's weight");
            for (a, b) in scaled.shapes.iter().zip(&part.shapes) {
                match (a, b) {
                    (
                        PartShape::Box {
                            center,
                            half,
                            rotation,
                        },
                        PartShape::Box {
                            center: c,
                            half: h,
                            rotation: r,
                        },
                    ) => {
                        assert_eq!(
                            (*center, *half, *rotation),
                            (*c * SENDA_SCALE, *h * SENDA_SCALE, *r)
                        );
                    }
                    (PartShape::Hull(a), PartShape::Hull(b)) => {
                        let b: Vec<DVec3> = b.iter().map(|p| *p * SENDA_SCALE).collect();
                        assert_eq!(a, &b);
                    }
                    _ => panic!("a part's shapes changed kind"),
                }
            }
        }
        // Its box is the cover's (the house's own, front and depth whichever way it
        // faces): the design's, scaled.
        let (low, high) = standing.bounds();
        assert!(
            (high.y - cover.h).abs() < 0.3,
            "{high:?} under a {} m ridge",
            cover.h
        );
        let mut built = [design.w * SENDA_SCALE, design.d * SENDA_SCALE];
        let mut boxed = [cover.w, cover.d];
        built.sort_by(f64::total_cmp);
        boxed.sort_by(f64::total_cmp);
        assert!(
            (built[0] - boxed[0]).abs() < 1e-9 && (built[1] - boxed[1]).abs() < 1e-9,
            "{built:?} against the cover's {boxed:?}"
        );
        assert!(low.y.abs() < 1e-9);
        checked += 1;
    }
    assert_eq!(checked, senda_houses().len());
    // The design a cover's box stands for is the layout's own house.
    for house in senda_houses() {
        let (x, z) = (house.x * SENDA_SCALE, house.z * SENDA_SCALE);
        let design = HouseDesign::for_cover(
            x,
            z,
            house.w * SENDA_SCALE,
            house.d * SENDA_SCALE,
            house.h * SENDA_SCALE,
            Some(house.plan),
        );
        let own = house.design();
        assert_eq!(
            (design.variant, design.turns, design.style),
            (own.variant, own.turns, own.style)
        );
        assert!((design.w - own.w).abs() < 1e-9 && (design.h - own.h).abs() < 1e-9);
        assert_eq!(
            (design_place(x, SENDA_SCALE), design_place(z, SENDA_SCALE)),
            (house.x, house.z)
        );
    }
}

#[test]
fn a_tank_drives_every_street_from_both_spawn_lines() {
    let mut sim = senda(23.0);
    let boundary = senda_boundary();
    let starts = [
        spawn_positions(Team::Blue, SENDA_SCALE)[2],
        spawn_positions(Team::Red, SENDA_SCALE)[2],
    ];
    let mut reached = 0;
    for street in Street::ALL {
        let line = street.line();
        // Down the middle of each street, and Senda Parkway's either side of its median in
        // the kerb lane (the median's trees keep a tank's clearance off the inner one).
        let lanes: &[f64] = if street == Street::SendaParkway {
            let lane = SENDA_PARKWAY_HALF - KERB_LANE;
            &[-lane, lane]
        } else {
            &[0.0]
        };
        let mut s = 0.0;
        while s <= line.length() {
            let (middle, along) = line.at(s);
            s += 4.0;
            // Round a turning circle's island rather than over it.
            let island = street
                .cul_de_sac()
                .is_some_and(|bulb| bulb.distance(middle) < CUL_DE_SAC_ISLAND + 2.0);
            for point in lanes.iter().map(|&offset| middle + left(along) * offset) {
                // Inside the wall, and well clear of it (Senda Parkway leaves the level).
                let inside = [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)]
                    .into_iter()
                    .all(|(u, v)| inside_boundary(&boundary, point + DVec2::new(u, v) * 4.0));
                if island || !inside || in_median_opening(point, 0.0) {
                    continue;
                }
                // Where a tree or lamp by the kerb keeps a tank's clearance off the spot,
                // the nearest open cell, still on the street or its sidewalk.
                let spot = Vec2::new(point.x * SENDA_SCALE, point.y * SENDA_SCALE);
                let to = sim.nav.point(sim.nav.nearest(sim.nav.index(spot)));
                let off = line.distance(DVec2::new(to.x, to.z) / SENDA_SCALE) - street.half();
                assert!(
                    !sim.nav.is_blocked(to) && off < PARKWAY + SIDEWALK,
                    "{street:?} is blocked at {spot:?}: its nearest open cell is {to:?}"
                );
                for &from in &starts {
                    let path = sim.nav.find(from, to);
                    assert!(
                        path.last().is_some_and(|end| distance(*end, to) < ARRIVAL),
                        "no way along {street:?} to {to:?} from {from:?}"
                    );
                }
                reached += 1;
            }
        }
    }
    assert!(reached > 40, "only {reached} points along the streets");
}

#[test]
fn a_room_client_rebuilds_the_hosts_cover_and_houses_from_the_scene() {
    use net_support::baseline;
    use sloppy_core::net::multiplayer_simulation::{
        MultiplayerOptions, create_multiplayer_simulation,
    };
    use sloppy_core::net::replication::{StateMirror, StateStream};
    use sloppy_core::net::scene_codec::Scene;
    use sloppy_core::sim::types::{PlayerAssignment, VehicleKind};
    let players = [PlayerAssignment {
        player_id: "one".into(),
        name: "One".into(),
        team: Team::Blue,
        slot: 0,
        kind: VehicleKind::Balanced,
    }];
    let host = create_multiplayer_simulation(
        77.0,
        &players,
        MultiplayerOptions {
            map_mode: Some(MapId::Neighborhood),
            ..MultiplayerOptions::default()
        },
    )
    .unwrap();
    assert_eq!(host.map_scale(), SENDA_SCALE);
    let viewer = host.human().id;
    let mut stream = StateStream::new("room", 1);
    let mut mirror = StateMirror::default();
    let scene = Scene::capture(&host);
    mirror
        .apply_full(
            &baseline(&stream.full(0, 0, [], || scene.clone())),
            "room",
            1,
        )
        .unwrap();
    let shown = mirror.render(viewer).unwrap();
    assert_eq!(shown.map_scale, SENDA_SCALE);
    assert_eq!(shown.covers.len(), host.covers.len());
    let mut houses = 0;
    for cover in &host.covers {
        let seen = shown
            .covers
            .iter()
            .find(|seen| seen.id == cover.id)
            .expect("every cover reaches the client");
        // To the millimetre the wire carries.
        for (a, b) in [
            (seen.x, cover.x),
            (seen.z, cover.z),
            (seen.w, cover.w),
            (seen.d, cover.d),
            (seen.h, cover.h),
        ] {
            assert!(
                (a - b).abs() <= 0.0005 + 1e-9,
                "{a} against {b} for {:?}",
                cover.kind
            );
        }
        assert_eq!(seen.house_plan, cover.house_plan);
        if cover.kind != CoverKind::House {
            continue;
        }
        let layout = &cover.structure.as_ref().expect("a structure").layout;
        let rebuilt = structure_of(
            seen.kind,
            seen.x,
            seen.z,
            seen.w,
            seen.d,
            seen.h,
            seen.timber_join,
            seen.house_plan,
        )
        .expect("a house's structure");
        let (design, theirs) = (rebuilt.design().unwrap(), layout.design().unwrap());
        assert_eq!(
            (design.variant, design.turns, design.style, design.door_wall),
            (theirs.variant, theirs.turns, theirs.style, theirs.door_wall)
        );
        assert_eq!(rebuilt.parts.len(), layout.parts.len());
        for (a, b) in rebuilt.parts.iter().zip(&layout.parts) {
            assert!(
                (a.min - b.min).abs().max_element() < 0.005
                    && (a.max - b.max).abs().max_element() < 0.005,
                "a part {:?}-{:?} rebuilt as {:?}-{:?}",
                b.min,
                b.max,
                a.min,
                a.max
            );
        }
        houses += 1;
    }
    assert_eq!(houses, senda_houses().len());
}
