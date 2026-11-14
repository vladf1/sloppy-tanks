//! Senda: its houses front their streets in rows that leave every spawn, pickup and
//! flank lane clear and reachable from both spawn lines, its houses are the
//! neighbourhood's three types built of parts, and a house falls like the village's.
//!
//! The level is laid out in design metres and stands `SENDA_SCALE` times larger: its
//! layout's own measures are read in design metres (`senda_design_layout`), what a
//! simulation does in the world's ([`world`], [`design`]).

use crate::support;

use std::collections::HashSet;

use glam::{DVec2, DVec3};
use sloppy_core::sim::arena::{cover_turn, spawn_positions};
use sloppy_core::sim::extra_levels::extra_level;
use sloppy_core::sim::house_layout::{
    BOARD_AND_BATTEN, CORNER_SPANISH, GarageSide, HouseDesign, REFERENCE_HOUSE, RoofCover,
    SENDA_STYLES, style_design, wall_frames,
};
use sloppy_core::sim::level_rules::standard_rules;
use sloppy_core::sim::map_options::{MapId, is_extra_level};
use sloppy_core::sim::math::{Vec2, distance};
use sloppy_core::sim::model_data::ARENA;
use sloppy_core::sim::senda_layout::{
    CUL_DE_SAC_ISLAND, CUL_DE_SAC_RADIUS, HOUSE_DEPTH, LOT_PITCH, Line, PARKWAY, Row,
    SENDA_FENCE_COLOR, SENDA_MAP, SENDA_PICKUPS, SENDA_SCALE, SENDA_WALL_COLOR, SETBACK, SIDE_YARD,
    SIDEWALK, STREET_HALF, SendaHouse, Street, WALL_THICKNESS, WallFace, bend_park, faces_circle,
    greenbelt_mouth, house_front, inside_boundary, is_senda_paved, left, neblina_bend, paseos,
    past_sidewalk, senda_boundary, senda_design_layout, senda_houses,
};
use sloppy_core::sim::senda_yards::{
    YARD_GATE_COLOR, YARD_PERIMETER_COLOR, YARD_VIEW_COLOR, YARD_WALL_COLOR, back_yard,
    greenbelt_lane, park_corners, senda_lots, yard_walls,
};
use sloppy_core::sim::{CoverKind, PickupKind, Simulation, SimulationSetup, Team};
use support::{assert_spawns_and_pickups_clear, fell_cottage};

/// Clear floor a tank hull needs around a spawn, pickup or flank waypoint.
const HULL_CLEARANCE: f64 = 2.0;
/// A route ends in the navigation cell that contains its goal.
const ARRIVAL: f64 = 1.8;

fn senda(seed: f64) -> Simulation {
    let streets = extra_level(MapId::Neighborhood).expect("the Neighborhood has its own streets");
    Simulation::new(seed, standard_rules().merged(streets))
}

/// A design point where it stands in the world.
fn world(at: DVec2) -> Vec2 {
    Vec2::new(at.x * SENDA_SCALE, at.y * SENDA_SCALE)
}

/// A world point in design metres.
fn design(at: Vec2) -> DVec2 {
    DVec2::new(at.x, at.z) / SENDA_SCALE
}

#[test]
fn the_neighborhood_is_a_standard_map_with_its_own_streets() {
    assert!(!is_extra_level(MapId::Neighborhood));
    let sim = senda(17.0);
    assert_eq!(sim.map_id(), MapId::Neighborhood);
    assert_eq!(sim.map_theme(), "neighborhood");
    assert_eq!(sim.map_name(), "NEIGHBORHOOD");
}

#[test]
fn every_spawn_pickup_and_flank_is_clear_and_reachable_from_both_spawn_lines() {
    let mut sim = senda(41.0);
    let points = assert_spawns_and_pickups_clear(&mut sim);
    let layout = (SENDA_MAP.layout)();
    let starts = [
        spawn_positions(Team::Blue, SENDA_SCALE)[0],
        spawn_positions(Team::Red, SENDA_SCALE)[0],
    ];
    // The bots' flank waypoints and the line they patrol along each side: one in a
    // house or a yard sends a bot to the nearest open cell, which lies inside the wall
    // and in reach of both spawn lines.
    let boundary = senda_boundary();
    let patrol = [-20.0, 20.0]
        .into_iter()
        .flat_map(|x| [-38.0, 0.0, 38.0].map(|z| world(DVec2::new(x, z))))
        .chain(
            [-46.0, 46.0]
                .into_iter()
                .flat_map(|x| [-44.0, -22.0, 0.0, 22.0, 44.0].map(|z| world(DVec2::new(x, z)))),
        );
    for waypoint in patrol {
        let goal = if sim.nav.is_blocked(waypoint) {
            sim.nav.point(sim.nav.nearest(sim.nav.index(waypoint)))
        } else {
            waypoint
        };
        assert!(
            inside_boundary(&boundary, design(goal)),
            "the waypoint {waypoint:?} sends bots past the wall to {goal:?}"
        );
        for start in starts {
            let end = sim.nav.find(start, goal).last().copied();
            assert!(
                end.is_some_and(|end| distance(end, goal) < ARRIVAL),
                "unreachable waypoint {waypoint:?} from {start:?}"
            );
        }
    }
    for point in points {
        for c in &layout {
            // A house turned to its street is measured in its own frame, a lot's wall in
            // its run's.
            let offset = DVec3::new(point.x - c.x, 0.0, point.z - c.z);
            let (local, w, d) = match c.house_plan {
                Some(plan) if c.kind == CoverKind::House => {
                    let design = HouseDesign::planned(c.x, c.z, c.w, c.d, c.h, plan);
                    (design.to_local(offset), design.w, design.d)
                }
                _ => {
                    let (sin, cos) = cover_turn(c.kind, c.house_plan).sin_cos();
                    let local = DVec3::new(
                        offset.x * cos - offset.z * sin,
                        0.0,
                        offset.x * sin + offset.z * cos,
                    );
                    (local, c.w, c.d)
                }
            };
            assert!(
                local.x.abs() >= w / 2.0 + HULL_CLEARANCE
                    || local.z.abs() >= d / 2.0 + HULL_CLEARANCE,
                "hull clearance at {point:?} beside {:?} at {},{}",
                c.kind,
                c.x,
                c.z
            );
        }
        for start in starts {
            let end = sim.nav.find(start, point).last().copied();
            assert!(
                distance(start, point) < ARRIVAL
                    || end.is_some_and(|end| distance(end, point) < ARRIVAL),
                "unreachable {point:?} from {start:?}"
            );
        }
    }
}

#[test]
fn every_house_fronts_its_street_with_a_garage_and_the_rows_mix_types_and_mirrors() {
    let houses = senda_houses();
    assert!((35..=80).contains(&houses.len()), "{} houses", houses.len());
    let mut styles = HashSet::new();
    let mut sides = HashSet::new();
    // How far `p` stands from the nearest kerb, of a street or a turning circle.
    let streets: Vec<(f64, Line)> = Street::ALL
        .into_iter()
        .map(|street| (street.half(), street.line()))
        .collect();
    let bulbs: Vec<DVec2> = Street::ALL
        .into_iter()
        .filter_map(Street::cul_de_sac)
        .collect();
    let kerb = |p: DVec2| {
        let lines = streets.iter().map(|(half, line)| line.distance(p) - half);
        let circles = bulbs
            .iter()
            .map(|middle| middle.distance(p) - CUL_DE_SAC_RADIUS);
        lines.chain(circles).fold(f64::INFINITY, f64::min)
    };
    for house in &houses {
        let (x, z, plan) = (house.x, house.z, house.plan);
        let design = house.design();
        assert_eq!(design.storeys(), 2, "two storeys at {x},{z}");
        assert_eq!(design.garage, Some(plan.garage));
        // Two low storeys: the front is wider than its eaves are high, as the tract's
        // are (about twice; Neblina's narrower west row a little less), so the rows
        // read as houses, not towers, and the upper floor still has windows.
        let roof = design.roof();
        let front = 2.0 * wall_frames(design.w, design.d)[design.door_wall].3;
        assert!(
            front > 1.2 * roof.wall,
            "{x},{z}: a {front:.1} m front under {:.2} m walls",
            roof.wall
        );
        assert!(roof.upper_window().1 > 0.6, "{x},{z}: no room upstairs");
        // The door looks out on the nearest kerb across its front yard, sidewalk and
        // parkway: a step out of it comes nearer that kerb, and no corner of the house
        // stands on paving.
        let at = DVec2::new(x, z);
        let (centre, out, _, _) = wall_frames(design.w, design.d)[design.door_wall];
        let (face, out) = (design.to_world(centre), design.to_world(out));
        let face = at + DVec2::new(face.x, face.z);
        let step = face + DVec2::new(out.x, out.z);
        assert!(kerb(step) < kerb(face), "{x},{z} faces its street");
        assert!(
            kerb(face) < SETBACK + SIDEWALK + PARKWAY + 1.0,
            "{x},{z} stands back {} from its kerb",
            kerb(face)
        );
        for (u, v) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            let corner = design.to_world(DVec3::new(u * design.w / 2.0, 0.0, v * design.d / 2.0));
            assert!(
                !is_senda_paved(x + corner.x, z + corner.z),
                "{x},{z} stands on paving"
            );
        }
        styles.insert(plan.style);
        sides.insert(plan.garage);
    }
    // All nine designs (three plans in three styles), and garages on both hands.
    let designs: HashSet<_> = styles.iter().map(|&style| style_design(style)).collect();
    assert_eq!(designs.len(), SENDA_STYLES.len() / 3, "{styles:?}");
    assert_eq!(sides, HashSet::from([GarageSide::Left, GarageSide::Right]));
    // The rows follow the curving streets: houses stand at many angles.
    let angles: HashSet<(i8, i8)> = houses
        .iter()
        .map(|h| (h.plan.street, h.plan.angle))
        .collect();
    assert!(angles.len() > 10, "{angles:?}");
    // Neblina's corner house, the street's one green house, stands on the corner lot at Senda
    // Way, the top of Neblina's west row, however many lots the row holds.
    let way = Street::SendaParkway.line();
    let top = houses
        .iter()
        .filter(|house| house.row == Row::NeblinaWest)
        .min_by(|a, b| {
            let (a, b) = (DVec2::new(a.x, a.z), DVec2::new(b.x, b.z));
            way.distance(a).total_cmp(&way.distance(b))
        })
        .expect("Neblina's west row");
    assert_eq!(top.plan.style, BOARD_AND_BATTEN, "{top:?}");
    assert_eq!(
        houses
            .iter()
            .filter(|house| house.plan.style == BOARD_AND_BATTEN)
            .count(),
        1
    );
}

#[test]
fn a_senda_house_is_built_of_parts_and_falls_like_the_villages() {
    let mut sim = senda(5.0);
    let house = sim
        .covers
        .iter()
        .position(|c| c.kind == CoverKind::House && c.house_plan.is_some())
        .expect("a planned house");
    assert!(sim.covers[house].structure.is_some(), "built of parts");
    fell_cottage(&mut sim, house);
    for _ in 0..240 {
        sim.step(Default::default(), false);
    }
    let state = sim.covers[house].structure.as_ref().expect("its structure");
    assert!(
        state.gone.len() * 2 > state.layout.parts.len(),
        "most of it came down"
    );
}

#[test]
fn a_room_on_senda_replicates_each_houses_plan() {
    let level = extra_level(MapId::Neighborhood).expect("Senda");
    let sim = Simulation::new(
        3.0,
        level.merged(SimulationSetup {
            map_mode: Some(MapId::Neighborhood),
            ..SimulationSetup::default()
        }),
    );
    let mut state = Default::default();
    sim.fill_render_state(&mut state, None);
    let planned = state
        .covers
        .iter()
        .filter(|c| c.kind == CoverKind::House)
        .filter(|c| c.house_plan.is_some())
        .count();
    assert_eq!(planned, senda_houses().len());
}

/// A house's wall corners on the ground, in the world.
fn corners(house: &SendaHouse) -> [DVec2; 4] {
    let design = house.design();
    [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(u, v)| {
        let corner = design.to_world(DVec3::new(u * design.w / 2.0, 0.0, v * design.d / 2.0));
        DVec2::new(house.x + corner.x, house.z + corner.z)
    })
}

/// The least distance between two footprints that do not overlap: from a corner of one
/// to an edge of the other.
fn gap(a: &[DVec2; 4], b: &[DVec2; 4]) -> f64 {
    let to_edges = |p: DVec2, quad: &[DVec2; 4]| {
        (0..4)
            .map(|i| {
                let (s, e) = (quad[i], quad[(i + 1) % 4]);
                let u = ((p - s).dot(e - s) / (e - s).length_squared()).clamp(0.0, 1.0);
                p.distance(s + (e - s) * u)
            })
            .fold(f64::INFINITY, f64::min)
    };
    let one = a.iter().map(|&p| to_edges(p, b));
    let other = b.iter().map(|&p| to_edges(p, a));
    one.chain(other).fold(f64::INFINITY, f64::min)
}

#[test]
fn the_houses_stand_detached_with_side_yards_in_rows_as_long_as_the_streets_hold() {
    let houses = senda_houses();
    // Every pair of neighbours keeps the tract's side yard between their walls (3-3.5 m,
    // about a quarter of a house's width), and most stand no further apart: as the level
    // stands, no wider than a tank's hull and a quarter metre each side, too narrow to
    // steer a tank through (its gates close it anyway), which crosses a block only by
    // breaching it.
    let footprints: Vec<[DVec2; 4]> = houses.iter().map(corners).collect();
    let hull =
        sloppy_core::sim::tank_dimensions::tank_hull(sloppy_core::sim::VehicleKind::Balanced)
            .size
            .x;
    let mut neighbours = 0;
    let mut tight = 0;
    for (i, a) in footprints.iter().enumerate() {
        for b in &footprints[i + 1..] {
            let yard = gap(a, b);
            assert!(
                yard >= SIDE_YARD - 0.01,
                "a side yard of {yard:.2} m between {a:?} and {b:?}"
            );
            if yard < 3.0 {
                neighbours += 1;
                tight += usize::from(yard * SENDA_SCALE < hull + 0.5);
            }
        }
    }
    assert!(
        tight * 10 > neighbours * 8,
        "{tight} of {neighbours} side yards too narrow for a {hull:.2} m hull"
    );
    // The rows: Rumbo's north and south sides, Gaviota's (with the houses on the inside
    // of Neblina's curve), and Neblina's west side from Senda Parkway round the outside
    // of the curve to the park, where it becomes Gaviota's south row. No street runs on
    // south. Each house's row is the side of the street it fronts; the south rows run on
    // a lot past their turning circles.
    let gaviota = Street::NeblinaGaviota.line();
    let count = |row: Row| houses.iter().filter(|house| house.row == row).count();
    let rows = Row::ALL.map(count);
    let least = [9, 8, 7, 6, 7];
    assert!(
        rows.iter().zip(least).all(|(&count, least)| count >= least),
        "houses per row (Rumbo north, south, Gaviota north, Neblina west, Gaviota south): \
         {rows:?}"
    );
    // Rumbo's north row is the longest, and the two rows backing
    // onto the greenbelt from either side hold about as many houses.
    let (rumbo_north, rumbo_south, gaviota_north, gaviota_south) = (
        count(Row::RumboNorth),
        count(Row::RumboSouth),
        count(Row::GaviotaNorth),
        count(Row::GaviotaSouth),
    );
    assert!(
        rumbo_north >= rumbo_south.max(gaviota_north).max(gaviota_south),
        "{rows:?}"
    );
    // The rows either side of the greenbelt hold as many houses as each other.
    assert!(rumbo_south.abs_diff(gaviota_north) <= 1, "{rows:?}");
    assert!(rumbo_south.abs_diff(gaviota_south) <= 2, "{rows:?}");
    // Each house fronts its own street from its row's side of it.
    let rumbo = Street::Rumbo.line();
    for house in &houses {
        let at = DVec2::new(house.x, house.z);
        let side = match house.row {
            Row::RumboNorth => rumbo.signed_distance(at),
            Row::RumboSouth => -rumbo.signed_distance(at),
            Row::GaviotaNorth => gaviota.signed_distance(at),
            Row::NeblinaWest | Row::GaviotaSouth => -gaviota.signed_distance(at),
        };
        let street = match house.row {
            Row::RumboNorth | Row::RumboSouth => Street::Rumbo,
            _ => Street::NeblinaGaviota,
        };
        let bulb = street
            .cul_de_sac()
            .map_or(f64::INFINITY, |middle| middle.distance(at));
        assert!(
            side > 0.0 || bulb < CUL_DE_SAC_RADIUS + house_front() + HOUSE_DEPTH,
            "{:?} at {at:?} stands across its street",
            house.row
        );
    }
    let bend = neblina_bend();
    let mut neblina_west: Vec<f64> = Vec::new();
    let mut west_turns = HashSet::new();
    for house in houses.iter().filter(|house| house.row == Row::NeblinaWest) {
        let at = DVec2::new(house.x, house.z);
        assert!(
            gaviota.along(at) < bend + LOT_PITCH,
            "Neblina's west row runs on past the bend at {at:?}"
        );
        neblina_west.push(gaviota.along(at));
        west_turns.insert((house.plan.street, house.plan.angle));
    }
    // Neblina's west side is one even run from Senda Parkway round the outside of the
    // curve, its houses between the spawn pads with no lot left empty but the paseo's,
    // each turned square to the street where it stands, so the row steps round the curve.
    assert!(
        west_turns.len() > 1,
        "Neblina's west row turns {west_turns:?}"
    );
    neblina_west.sort_by(f64::total_cmp);
    let gaps: Vec<f64> = neblina_west
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .collect();
    assert!(
        gaps.iter().filter(|&&gap| gap > 1.5 * LOT_PITCH).count() == 1
            && gaps.iter().all(|&gap| gap < 2.5 * LOT_PITCH),
        "Neblina's west row's gaps {gaps:.1?}: one lot left for the paseo"
    );
}

#[test]
fn a_walled_paseo_leads_from_the_west_field_through_neblinas_west_row() {
    // Halfway along the row, one lot left open: a walk 4.5 m or more clear between lot
    // walls from the spawn field behind the row to Neblina's sidewalk, with nothing
    // standing in it, and the bots drive through it.
    let houses = senda_houses();
    let walks = paseos(&houses);
    assert_eq!(walks.len(), 1, "{walks:?}");
    let paseo = walks[0];
    let middle = (paseo.field + paseo.sidewalk) / 2.0;
    assert!(
        (-8.0..8.0).contains(&middle.y),
        "the paseo at {middle:?} is not near the row's middle"
    );
    let layout = senda_design_layout();
    let walls: Vec<_> = layout
        .iter()
        .filter(|c| c.kind == CoverKind::Concrete && c.color == YARD_WALL_COLOR)
        .filter(|c| paseo.distance(DVec2::new(c.x, c.z)) < 4.0)
        .collect();
    let across = left((paseo.sidewalk - paseo.field).normalize());
    let sides: HashSet<bool> = walls
        .iter()
        .map(|c| (DVec2::new(c.x, c.z) - middle).dot(across) > 0.0)
        .collect();
    assert_eq!(sides.len(), 2, "the paseo is walled on one side only");
    for k in 0..=20 {
        let p = paseo.field.lerp(paseo.sidewalk, f64::from(k) / 20.0);
        for c in &layout {
            let room = cover_distance(c, p);
            assert!(
                room >= 2.25,
                "{:?} at {},{} stands {room:.2} m from the paseo's middle at {p:?}",
                c.kind,
                c.x,
                c.z
            );
        }
    }
    let mut sim = senda(23.0);
    let from = spawn_positions(Team::Blue, SENDA_SCALE)[2];
    let to = world(paseo.sidewalk);
    let path = sim.nav.find(from, to);
    assert!(
        path.last().is_some_and(|end| distance(*end, to) < ARRIVAL),
        "no way through the paseo"
    );
    let length = route_length(from, &path);
    assert!(
        length < distance(from, to) * 1.3,
        "the way through the paseo runs {length:.0} m"
    );
}

#[test]
fn the_greenbelt_opens_onto_neblina_between_the_corner_lots() {
    // On Neblina's east side, south of Rumbo's corner lot and short of Gaviota's first
    // lot at the bend, no house stands: the greenbelt meets the street there.
    let gaviota = Street::NeblinaGaviota.line();
    let rumbo = Street::Rumbo.line();
    let corner = gaviota.along(rumbo.0[0]);
    for house in senda_houses() {
        let at = DVec2::new(house.x, house.z);
        let along = gaviota.along(at);
        let past_rumbo_lots = -rumbo.signed_distance(at) > house_front() + HOUSE_DEPTH;
        assert!(
            !(past_rumbo_lots
                && gaviota.signed_distance(at) > 0.0
                && along > corner
                && along < greenbelt_mouth() - LOT_PITCH),
            "a house at {},{} closes the greenbelt's mouth",
            house.x,
            house.z
        );
    }
}

#[test]
fn neblina_sweeps_round_into_gaviota_and_only_senda_parkway_leaves_the_level() {
    let boundary = senda_boundary();
    let way = Street::SendaParkway.line();
    // Rumbo and Neblina-Gaviota stay inside the wall to their cul-de-sacs, their
    // paving clear of it; only Senda Parkway runs on past the wall.
    for street in [Street::Rumbo, Street::NeblinaGaviota] {
        let line = street.line();
        let room = street.half() + PARKWAY + SIDEWALK;
        let mut s = 0.0;
        while s < line.length() {
            let (point, _) = line.at(s);
            for (u, v) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                let edge = point + DVec2::new(u, v) * room;
                assert!(
                    inside_boundary(&boundary, edge)
                        || way.distance(point) < Street::SendaParkway.half() + room,
                    "{street:?} reaches the wall at {point:?}"
                );
            }
            s += 1.0;
        }
    }
    // Neblina runs at a diagonal from Senda Parkway down to its bend, well off square to
    // Rumbo, and becomes Gaviota round a broad curve, not a corner: its heading turns
    // little over any few metres.
    let line = Street::NeblinaGaviota.line();
    let bend = neblina_bend();
    let (top, _) = line.at(0.0);
    let (down, _) = line.at(bend * 0.7);
    let lean = (down.y - top.y).atan2(down.x - top.x).to_degrees();
    assert!(
        (50.0..65.0).contains(&lean),
        "Neblina leans {lean:.0} degrees"
    );
    let heading = |s: f64| {
        let (a, b) = (line.at(s - 2.0).0, line.at(s + 2.0).0);
        (b.y - a.y).atan2(b.x - a.x).to_degrees()
    };
    let mut s = bend - 20.0;
    while s < bend + 20.0 {
        let turn = (heading(s) - heading(s + 4.0)).abs();
        assert!(turn < 20.0, "Neblina turns {turn:.0} degrees at {s:.0} m");
        s += 1.0;
    }
    assert!(
        heading(bend + 20.0).abs() < 15.0,
        "Gaviota runs east past the bend"
    );
    // The park on the outside of the curve, where the name changes, keeps its lot.
    let park = bend_park();
    for house in senda_houses() {
        let at = DVec2::new(house.x, house.z);
        assert!(
            at.distance(park) > LOT_PITCH * 0.95,
            "a house in the park at {at:?}"
        );
    }
}

#[test]
fn every_house_stands_on_rumbo_neblina_or_gaviota() {
    let way = Street::SendaParkway.line();
    for house in senda_houses() {
        let at = DVec2::new(house.x, house.z);
        let kerb = |street: Street| {
            let to_line = street.line().distance(at) - street.half();
            let to_bulb = street.cul_de_sac().map_or(f64::INFINITY, |middle| {
                middle.distance(at) - CUL_DE_SAC_RADIUS
            });
            to_line.min(to_bulb)
        };
        let nearest = kerb(Street::Rumbo).min(kerb(Street::NeblinaGaviota));
        assert!(
            nearest < house_front() - Street::Rumbo.half() + HOUSE_DEPTH + 1.0,
            "the house at {at:?} stands {nearest:.1} m off its street"
        );
        // None fronts Senda Parkway: the corner lot on it (Neblina's corner house) turns its side to
        // the Way past a side yard off its sidewalk.
        let way_walk = way.distance(at) - (Street::SendaParkway.half() + PARKWAY + SIDEWALK);
        assert!(
            way_walk > nearest.min(house.w / 2.0 + 1.0),
            "the house at {at:?} stands on Senda Parkway"
        );
    }
}

#[test]
fn the_wall_closes_round_every_house_spawn_and_pickup_and_no_tank_can_leave() {
    let boundary = senda_boundary();
    // One loop of runs with the inside on its left (seen from above).
    let n = boundary.len();
    let runs: Vec<(DVec2, DVec2)> = (0..n)
        .map(|i| (boundary[i], boundary[(i + 1) % n]))
        .collect();
    for &(a, b) in &runs {
        let (middle, inward) = ((a + b) / 2.0, left((b - a).normalize()));
        assert!(
            inside_boundary(&boundary, middle + inward * 0.5)
                && !inside_boundary(&boundary, middle - inward * 0.5),
            "the run {a:?} to {b:?} has its inside on the right"
        );
    }
    // Not the square edge all round: it follows the tract's edges. On the south it runs
    // a lane behind the back walls of Gaviota's south row, over the slope's band of trees:
    // each lot's back wall, from the bend park to the turning circle, keeps a tank's way
    // to the level's wall and stands within a lane, a band of trees and the slope's top of
    // it (the wall keeps its course from the street as the lots' depth changes).
    assert!(
        !inside_boundary(&boundary, DVec2::new(0.0, 56.0)),
        "the wall runs round the square edge"
    );
    let wall = Line(boundary.iter().chain(boundary.first()).copied().collect());
    let houses = senda_houses();
    // (A pickup behind a lot cuts its yard short.)
    let by_pickup = |at: DVec2| {
        SENDA_PICKUPS
            .iter()
            .any(|p| DVec2::new(p.x, p.z).distance(at) < 8.0)
    };
    for lot in senda_lots(&houses)
        .iter()
        .filter(|lot| lot.house.row == Row::GaviotaSouth && !by_pickup(lot.at(0.0, lot.rear_v())))
    {
        for rear in [
            lot.sides[0].rear,
            lot.at(0.0, lot.rear_v()),
            lot.sides[1].rear,
        ] {
            let room = wall.distance(rear);
            assert!(
                inside_boundary(&boundary, rear) && room > 4.0,
                "the back wall at {rear:?} keeps {room:.1} m from the level's wall"
            );
            if (-20.0..40.0).contains(&rear.x) {
                assert!(
                    room < 11.0,
                    "the level's wall stands {room:.1} m behind the back wall at {rear:?}"
                );
            }
        }
    }
    // On the north it comes down the top of the bank south of Senda Parkway, leaving the
    // Way's north-eastern stretch outside, crosses the Way square to it, and comes down
    // its north verge, as the neighbouring tract's wall does, leaving the Way inside from
    // there to the west edge and the field north of it out but for the north-western
    // pad's corner.
    let way = Street::SendaParkway.line();
    let paving = Street::SendaParkway.half() + PARKWAY + SIDEWALK;
    assert!(
        !inside_boundary(&boundary, DVec2::new(-35.0, -55.0)),
        "a field north of the Way"
    );
    let neblina = Street::NeblinaGaviota.line().0[0];
    let mouth = way.along(neblina);
    let (north_east, _) = way.at(mouth + 40.0);
    assert!(
        !inside_boundary(&boundary, north_east),
        "the Way's north-eastern stretch at {north_east:?} inside the wall"
    );
    // Past the wall the Way runs along the top of the bank above the whole of Rumbo's
    // north row, just outside the level's north edge and nearly parallel to it, as the
    // real Way runs above the row's back yards to past the turning circle.
    let mut s = mouth;
    let mut above = 0.0;
    while s < way.length() {
        let (point, heading) = way.at(s);
        if (0.0..=45.0).contains(&point.x) {
            assert!(
                (-ARENA - 12.0..-ARENA).contains(&point.y) && !inside_boundary(&boundary, point),
                "the Way at {point:?} strays from the level's north edge"
            );
            if point.x > 10.0 {
                let off = heading.y.atan2(heading.x).to_degrees().abs();
                assert!(
                    off < 10.0,
                    "the Way runs {off:.0} degrees off the row at {point:?}"
                );
            }
            above += 1.0;
        }
        s += 1.0;
    }
    assert!(above > 40.0, "the Way runs above the row for {above} m");
    // The wall bends in short runs rather than corners, round the spawn pads (the
    // north-western on the Way's verge and the southern two) rather than boxing them into
    // pens, and where it crosses the Way and leaves its verge: no corner turns more than
    // a few runs round a pad would, but the arena's own corner at the north-east, where
    // the bank meets the hillside.
    let pads: Vec<DVec2> = [Team::Blue, Team::Red]
        .into_iter()
        .flat_map(|team| spawn_positions(team, 1.0))
        .map(|spawn| DVec2::new(spawn.x, spawn.z))
        .collect();
    for i in 0..n {
        let (before, after) = (runs[(i + n - 1) % n], runs[i]);
        let (a, b) = (
            (before.1 - before.0).normalize(),
            (after.1 - after.0).normalize(),
        );
        let turn = a.perp_dot(b).atan2(a.dot(b)).to_degrees().abs();
        if after.0.distance(DVec2::new(ARENA, -ARENA)) > 0.01 {
            assert!(
                turn < 32.0,
                "the wall turns {turn:.0} degrees at {:?}",
                after.0
            );
        }
    }
    let mut s = 0.0;
    while s < mouth + 12.0 {
        let (point, _) = way.at(s);
        if point.x.abs().max(point.y.abs()) < ARENA - paving - 1.0 {
            assert!(
                inside_boundary(&boundary, point),
                "the Way at {point:?} past the wall"
            );
        }
        s += 1.0;
    }
    // Where the wall stands on the Way's paving, but at the arena's edge, it crosses the
    // Way square to it, once: in one line of runs.
    let mut crossings = 0;
    let mut on_way = false;
    for &(a, b) in &runs {
        let steps = (a.distance(b) / 0.25).ceil() as usize;
        let on_paving = (0..=steps).any(|k| {
            let p = a.lerp(b, k as f64 / steps as f64);
            p.x.abs().max(p.y.abs()) < ARENA - 0.01 && way.distance(p) < paving + 1.0
        });
        if on_paving {
            let middle = (a + b) / 2.0;
            let (_, heading) = way.at(way.along(middle));
            let off_square = (b - a).normalize().dot(heading).abs().asin().to_degrees();
            // Past a bend in the Way the crossing's corner runs lean a little against
            // the span before it.
            assert!(
                off_square < 12.0,
                "the wall from {a:?} to {b:?} crosses the Way {off_square:.0} degrees off square"
            );
            if !on_way {
                crossings += 1;
            }
        }
        on_way = on_paving;
    }
    assert_eq!(crossings, 1, "the wall crosses the Way {crossings} times");
    for house in &houses {
        for corner in corners(house) {
            assert!(
                inside_boundary(&boundary, corner),
                "a house corner {corner:?} past the wall"
            );
        }
    }
    let pickups: Vec<DVec2> = SENDA_PICKUPS.iter().map(|p| DVec2::new(p.x, p.z)).collect();
    for &at in pads.iter().chain(&pickups) {
        assert!(inside_boundary(&boundary, at), "{at:?} past the wall");
    }
    // The wall is drawn: boundary cover stands along every run.
    let layout = senda_design_layout();
    let walls = layout
        .iter()
        .filter(|c| c.kind == CoverKind::Boundary)
        .count();
    assert_eq!(walls, n, "one wall run a side");
    // A slanting run turns by whole degrees with its plan, its box along the run.
    for wall in layout.iter().filter(|c| c.kind == CoverKind::Boundary) {
        let turn = cover_turn(wall.kind, wall.house_plan).to_degrees();
        assert!(
            (turn - turn.round()).abs() < 1e-9,
            "a wall turned {turn} degrees"
        );
    }
    // Each run is drawn as the tract's thin wall on the inside face of its cover, from end
    // to end of its own run: the face its plan carries runs from one corner of the line
    // to the next, the level on its inside, the drawn wall just outside. Along the open
    // ground (the hillside at the east edge and the bank top under the Way) it is a view
    // fence; elsewhere block.
    let closed = Line(boundary.iter().chain(boundary.first()).copied().collect());
    let on_line = |p: DVec2| boundary.iter().any(|&corner| corner.distance(p) < 0.03);
    for wall in layout.iter().filter(|c| c.kind == CoverKind::Boundary) {
        let face = WallFace::of(wall.house_plan.expect("a wall run carries its face"));
        let (length, thick) = if face.along_x {
            (wall.w, wall.d)
        } else {
            (wall.d, wall.w)
        };
        let (sin, cos) = cover_turn(wall.kind, wall.house_plan).sin_cos();
        let world = |u: f64, v: f64| {
            let (x, z) = if face.along_x { (u, v) } else { (v, u) };
            DVec2::new(wall.x + x * cos + z * sin, wall.z - x * sin + z * cos)
        };
        let inner = face.inside * thick / 2.0;
        let (from, to) = (
            world(-length / 2.0 + face.reach[0], inner),
            world(length / 2.0 - face.reach[1], inner),
        );
        assert!(
            on_line(from) && on_line(to),
            "the wall at {},{} is drawn from {from:?} to {to:?}, off the corners of its run",
            wall.x,
            wall.z
        );
        let middle = (from + to) / 2.0;
        let drawn = world(
            (face.reach[0] - face.reach[1]) / 2.0,
            inner - face.inside * WALL_THICKNESS / 2.0,
        );
        assert!(
            closed.distance(middle) < 0.03
                && !inside_boundary(&boundary, drawn)
                && inside_boundary(&boundary, middle + (middle - drawn) * 2.0),
            "the wall at {},{} is drawn off the level's inside line",
            wall.x,
            wall.z
        );
        let east = from.x.min(to.x) > ARENA - 0.01;
        if east || (from.x < -ARENA + 0.01 && to.x < -ARENA + 0.01) {
            let color = if east {
                SENDA_FENCE_COLOR
            } else {
                SENDA_WALL_COLOR
            };
            assert_eq!(wall.color, color, "the wall at {},{}", wall.x, wall.z);
        }
    }
    // From every spawn pad the open navigation cells, stepping to any of the eight about
    // each, never reach past the wall, and reach every pickup.
    let sim = senda(23.0);
    let neighbours = [
        (1, 0),
        (-1, 0),
        (0, 1),
        (0, -1),
        (1, 1),
        (1, -1),
        (-1, 1),
        (-1, -1),
    ];
    let cell_of = |at: DVec2| sim.nav.index(world(at));
    for &pad in &pads {
        let start = cell_of(pad);
        let mut seen = vec![false; sim.nav.blocked.len()];
        let mut open = vec![start];
        seen[start] = true;
        while let Some(cell) = open.pop() {
            let point = sim.nav.point(cell);
            assert!(
                inside_boundary(&boundary, design(point)),
                "a tank from {pad:?} gets out at {point:?}"
            );
            let (x, z) = (
                (cell % sim.nav.grid_size()) as i64,
                (cell / sim.nav.grid_size()) as i64,
            );
            for (dx, dz) in neighbours {
                let (nx, nz) = (x + dx, z + dz);
                let size = sim.nav.grid_size() as i64;
                if nx < 0 || nz < 0 || nx >= size || nz >= size {
                    continue;
                }
                let next = (nz * size + nx) as usize;
                if !seen[next] && sim.nav.blocked[next] == 0 {
                    seen[next] = true;
                    open.push(next);
                }
            }
        }
        for &at in &pickups {
            assert!(seen[cell_of(at)], "{at:?} is out of reach of {pad:?}");
        }
    }
}

#[test]
fn the_lots_are_walled_behind_and_between_and_their_gates_close_the_side_yards() {
    let houses = senda_houses();
    let lots = senda_lots(&houses);
    let walls = yard_walls(&houses);
    let boundary = senda_boundary();
    let (blocks, views, gates) = walls
        .iter()
        .fold((0, 0, 0), |(blocks, views, gates), wall| {
            assert_eq!(wall.kind, CoverKind::Concrete);
            assert!(wall.hp.is_finite(), "a shell breaches a wall");
            assert!(
                inside_boundary(&boundary, DVec2::new(wall.x, wall.z)),
                "a lot wall at {},{} past the level's wall",
                wall.x,
                wall.z
            );
            match wall.color {
                YARD_GATE_COLOR => (blocks, views, gates + 1),
                YARD_VIEW_COLOR => (blocks, views + 1, gates),
                YARD_WALL_COLOR | YARD_PERIMETER_COLOR => (blocks + 1, views, gates),
                other => panic!("a wall painted {other:x}"),
            }
        });
    // Most lots keep their back wall, side walls and both gates.
    assert!(
        blocks + views > 3 * houses.len(),
        "{blocks} block and {views} view fence runs for {} lots",
        houses.len()
    );
    // The lots backing onto the bank and the greenbelt look out through steel view
    // fences; neighbours keep block between them, as do the rows backing onto roads.
    assert!(
        views > houses.len() && blocks > houses.len(),
        "{views} view fence runs and {blocks} block runs for {} lots",
        houses.len()
    );
    assert!(
        gates > houses.len(),
        "{gates} gates for {} lots",
        houses.len()
    );
    // Neighbours in a row share the lot line between them, halfway across the side
    // yard: it stands clear of both houses' walls.
    let shared = lots
        .iter()
        .flat_map(|lot| lot.sides)
        .filter(|side| side.neighbour.is_some())
        .count();
    assert!(shared > houses.len(), "{shared} shared lot lines");
    for lot in &lots {
        for side in lot.sides {
            let (u, v) = lot.local(side.gate);
            assert!(
                u.abs() > lot.half_width + 0.3,
                "a lot line in a side yard: {u:.2} at {:?}",
                lot.house
            );
            assert!(
                v < lot.half_depth,
                "a gate behind the house's front: {v:.2}"
            );
            // (A shared corner is the mean of both neighbours' frames, a little nearer
            // the house where they turn apart.)
            let (_, rear) = lot.local(side.rear);
            assert!(
                rear < -lot.half_depth - 0.4,
                "a back yard behind the house: {rear:.2} at {:?} {:?}",
                lot.house,
                side
            );
        }
    }
}

#[test]
fn the_back_yards_run_one_to_three_metres_but_leave_the_greenbelt_its_lane_and_the_park_walled() {
    let houses = senda_houses();
    let lots = senda_lots(&houses);
    // The tract's back yards run 1.5-6 m from the house to the block wall, 1-3.2 m at
    // the level's scale; most run 2 m or more, and only a pickup, a spawn pad, the
    // level's wall or the greenbelt's lane cuts one shorter. (A pie-shaped lot round a
    // turning circle widens behind its house, past the depth measured down its middle.)
    let yards: Vec<f64> = lots.iter().map(back_yard).collect();
    assert!(
        yards.iter().all(|&yard| (0.99..3.25).contains(&yard)),
        "back yards {yards:?}"
    );
    let bulbs: Vec<DVec2> = Street::ALL
        .into_iter()
        .filter_map(Street::cul_de_sac)
        .collect();
    let square: Vec<f64> = lots
        .iter()
        .filter(|lot| {
            !bulbs
                .iter()
                .any(|&middle| faces_circle(lot.centre, lot.front, middle))
        })
        .map(back_yard)
        .collect();
    let deep = square.iter().filter(|&&yard| yard >= 2.0).count();
    assert!(
        deep * 4 > square.len() * 3,
        "{deep} of {} back yards on the streets run 2 m or more",
        square.len()
    );
    // (Across the greenbelt the back walls keep a tank's lane between them:
    // `the_greenbelt_lane_runs_open_from_neblina_to_the_hillside`.)
    let (a, b) = park_corners(&lots).expect("the bend park has a back wall");
    let park = bend_park();
    assert!(
        a.distance(park) < 14.0 && b.distance(park) < 14.0 && a.distance(b) > 4.0,
        "the bend park's back wall runs from {a:?} to {b:?}"
    );
}

/// How far (m) `p` lies outside a cover's box, a house's measured in its own frame.
fn cover_distance(c: &sloppy_core::sim::arena::CoverDef, p: DVec2) -> f64 {
    let offset = DVec3::new(p.x - c.x, 0.0, p.y - c.z);
    let (local, w, d) = match c.house_plan {
        Some(plan) if c.kind == CoverKind::House => {
            let design = HouseDesign::planned(c.x, c.z, c.w, c.d, c.h, plan);
            (design.to_local(offset), design.w, design.d)
        }
        _ => {
            let (sin, cos) = cover_turn(c.kind, c.house_plan).sin_cos();
            let local = DVec3::new(
                offset.x * cos - offset.z * sin,
                0.0,
                offset.x * sin + offset.z * cos,
            );
            (local, c.w, c.d)
        }
    };
    DVec2::new(
        (local.x.abs() - w / 2.0).max(0.0),
        (local.z.abs() - d / 2.0).max(0.0),
    )
    .length()
}

/// The length (m) of a navigation route from `from`.
fn route_length(from: Vec2, path: &[Vec2]) -> f64 {
    std::iter::once(from)
        .chain(path.iter().copied())
        .collect::<Vec<_>>()
        .windows(2)
        .map(|pair| distance(pair[0], pair[1]))
        .sum()
}

#[test]
fn the_greenbelt_lane_runs_open_from_neblina_to_the_hillside() {
    // A tank's lane between the back walls of Rumbo's south row and Gaviota's north row:
    // from Neblina's sidewalk at the greenbelt's mouth to the arena's east edge, 4.5 m
    // clear of every cover all the way, and the bots' navigation drives it end to end.
    let lane = greenbelt_lane();
    assert!(lane.len() > 40, "the greenbelt keeps a lane");
    let (west, east) = (lane[0], lane[lane.len() - 1]);
    assert!(
        past_sidewalk(Street::NeblinaGaviota, west) < 0.0 && west.x < -10.0,
        "the lane's west end at {west:?} is not on Neblina's sidewalk"
    );
    assert!(east.x >= ARENA - 1.0, "the lane ends at {east:?}");
    let layout = senda_design_layout();
    for &p in lane.iter().filter(|p| p.x < ARENA - 3.0) {
        for c in &layout {
            let room = cover_distance(c, p);
            assert!(
                room >= 2.25,
                "{:?} at {},{} stands {room:.2} m from the lane's middle at {p:?}",
                c.kind,
                c.x,
                c.z
            );
        }
    }
    let mut sim = senda(23.0);
    let (from, to) = (world(west), world(DVec2::new(east.x - 3.0, east.y)));
    let path = sim.nav.find(from, to);
    assert!(
        path.last().is_some_and(|end| distance(*end, to) < ARRIVAL),
        "no way down the lane from {from:?} to {to:?}"
    );
    let line = Line(lane.clone());
    for p in &path {
        assert!(
            line.distance(design(*p)) < 5.0,
            "the way down the lane strays to {p:?}"
        );
    }
    let length = route_length(from, &path);
    assert!(
        length < Line(lane).length() * SENDA_SCALE * 1.2,
        "the way down the lane runs {length:.0} m"
    );
}

#[test]
fn a_shell_breaches_a_lot_wall_and_the_way_opens_through_it() {
    let mut sim = senda(23.0);
    let wall = sim
        .covers
        .iter()
        .position(|c| c.kind == CoverKind::Concrete && c.color == YARD_WALL_COLOR)
        .expect("a lot wall");
    let at = Vec2::new(sim.covers[wall].x, sim.covers[wall].z);
    assert!(sim.nav.is_blocked(at), "a standing wall blocks the way");
    sim.damage_cover(wall, 50.0, 0, Team::Blue, None, None);
    assert!(!sim.covers[wall].alive, "one shell's worth breaches it");
}

/// A house's front wall middle and the way it faces, in the world.
fn front(house: &SendaHouse) -> (DVec2, DVec2) {
    let design = house.design();
    let (centre, out, _, _) = wall_frames(design.w, design.d)[design.door_wall];
    let (face, out) = (design.to_world(centre), design.to_world(out));
    (
        DVec2::new(house.x + face.x, house.z + face.z),
        DVec2::new(out.x, out.z).normalize(),
    )
}

#[test]
fn the_rows_run_on_round_true_size_turning_circles_whose_far_ends_open_onto_the_hillside() {
    // As the tract's do: each row's last lots turn round the turning circle, pie-shaped,
    // their homes facing its middle across the same front yard as on the street; the
    // circle's far end, toward the spawn lane on the hillside, stays open.
    assert_eq!((CUL_DE_SAC_RADIUS, CUL_DE_SAC_ISLAND), (6.8, 2.0));
    let houses = senda_houses();
    let pads: Vec<DVec2> = [Team::Blue, Team::Red]
        .into_iter()
        .flat_map(|team| spawn_positions(team, 1.0))
        .map(|spawn| DVec2::new(spawn.x, spawn.z))
        .collect();
    for (street, sides) in [
        (Street::Rumbo, [Row::RumboNorth, Row::RumboSouth]),
        (
            Street::NeblinaGaviota,
            [Row::GaviotaNorth, Row::GaviotaSouth],
        ),
    ] {
        let middle = street.cul_de_sac().expect("a turning circle");
        // Its sidewalk ring keeps clear of the spawn pads and pickups.
        let ring = CUL_DE_SAC_RADIUS + PARKWAY + SIDEWALK;
        for &pad in &pads {
            assert!(
                pad.distance(middle) > ring + 2.3 + 0.5,
                "the {street:?} circle's sidewalk at the pad {pad:?}"
            );
        }
        for pickup in SENDA_PICKUPS.iter() {
            let at = DVec2::new(pickup.x, pickup.z);
            assert!(
                at.distance(middle) > ring + HULL_CLEARANCE,
                "{pickup:?} on the {street:?} circle"
            );
        }
        let facing: Vec<&SendaHouse> = houses
            .iter()
            .filter(|house| {
                let (face, out) = front(house);
                faces_circle(face - out * HOUSE_DEPTH / 2.0, out, middle)
            })
            .collect();
        for row in sides {
            assert!(
                facing.iter().any(|house| house.row == row),
                "no home of {row:?} faces the {street:?} circle"
            );
        }
        let line = street.line();
        let (_, heading) = line.at(line.length());
        for house in &facing {
            // Its front, and so its garage door, a front yard, sidewalk and parkway from
            // the circle's kerb, no nearer than a street's homes stand.
            let (face, _) = front(house);
            let kerb = face.distance(middle) - CUL_DE_SAC_RADIUS;
            assert!(
                (3.39..4.0).contains(&kerb),
                "{house:?} stands {kerb:.2} m from the {street:?} circle's kerb"
            );
        }
        // Past the circle's middle the homes stand to either side of its far end, none
        // across it.
        for house in &houses {
            let from = DVec2::new(house.x, house.z) - middle;
            let ahead = from.dot(heading);
            let aside = from.perp_dot(heading).abs();
            assert!(
                ahead < 0.0 || aside > ahead * 0.5 || from.length() > 25.0,
                "{house:?} closes the {street:?} circle's far end"
            );
        }
    }
}

#[test]
fn each_turning_circle_has_homes_round_it_and_a_bruiser_drives_round_its_island() {
    let houses = senda_houses();
    // The tank's own size against the circle as it stands in the world.
    let layout = (SENDA_MAP.layout)();
    let (island, radius) = (
        CUL_DE_SAC_ISLAND * SENDA_SCALE,
        CUL_DE_SAC_RADIUS * SENDA_SCALE,
    );
    let sim = senda(5.0);
    let hull =
        sloppy_core::sim::tank_dimensions::tank_hull(sloppy_core::sim::VehicleKind::Balanced).size;
    // A Bruiser 1.95 m wide and 4.27 m long, its middle on a circle round the island: its
    // inner side clears the island's kerb and its outer corners the circle's.
    let (width, length) = (hull.x, hull.z);
    assert!(
        (width - 1.95).abs() < 0.05 && (length - 4.27).abs() < 0.05,
        "{hull:?}"
    );
    let drive = (island + radius) / 2.0;
    let inner = drive - width / 2.0;
    let outer = (drive + width / 2.0).hypot(length / 2.0);
    assert!(
        inner > island + 0.2 && outer < radius - 0.2,
        "a Bruiser round the island spans {inner:.2} to {outer:.2} m"
    );
    for middle in Street::ALL.into_iter().filter_map(Street::cul_de_sac) {
        let homes = houses
            .iter()
            .filter(|house| {
                let (face, out) = front(house);
                faces_circle(face - out * HOUSE_DEPTH / 2.0, out, middle)
            })
            .count();
        assert!(homes >= 2, "{homes} homes face the circle at {middle:?}");
        let middle = middle * SENDA_SCALE;
        // Nothing stands in the ring it drives, and the way round is open.
        for c in &layout {
            // The circle's middle in the cover's turned box frame: the box's nearest and
            // furthest reach from it.
            let (sin, cos) = cover_turn(c.kind, c.house_plan).sin_cos();
            let (x, z) = (middle.x - c.x, middle.y - c.z);
            let local = DVec2::new(x * cos - z * sin, x * sin + z * cos);
            let half = DVec2::new(c.w, c.d) / 2.0;
            let nearest = (local.abs() - half).max(DVec2::ZERO).length();
            let furthest = (local.abs() + half).length();
            assert!(
                furthest < inner || nearest > outer,
                "{:?} at {},{} in the way round the circle at {middle:?}",
                c.kind,
                c.x,
                c.z
            );
        }
        // The navigation's open cells between the island's kerb and the circle's join up
        // all the way round.
        let size = sim.nav.grid_size() as i64;
        let in_ring = |cell: usize| {
            let p = sim.nav.point(cell);
            let gap = DVec2::new(p.x, p.z).distance(middle);
            (island..radius).contains(&gap) && sim.nav.blocked[cell] == 0
        };
        let start = (0..sim.nav.blocked.len())
            .find(|&cell| in_ring(cell))
            .expect("an open cell round the island");
        let mut seen = HashSet::from([start]);
        let mut open = vec![start];
        while let Some(cell) = open.pop() {
            let (x, z) = (
                (cell % sim.nav.grid_size()) as i64,
                (cell / sim.nav.grid_size()) as i64,
            );
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, nz) = (x + dx, z + dz);
                if nx < 0 || nz < 0 || nx >= size || nz >= size {
                    continue;
                }
                let next = (nz * size + nx) as usize;
                if in_ring(next) && seen.insert(next) {
                    open.push(next);
                }
            }
        }
        let sectors: HashSet<i64> = seen
            .iter()
            .map(|&cell| {
                let p = sim.nav.point(cell);
                let angle = (p.z - middle.y).atan2(p.x - middle.x);
                (angle.to_degrees() / 30.0).floor() as i64
            })
            .collect();
        assert_eq!(
            sectors.len(),
            12,
            "the way round the circle at {middle:?} is shut: open in {sectors:?}"
        );
    }
}

#[test]
fn the_reference_house_is_the_fourth_on_rumbos_north_side_past_the_corner_lot() {
    // Driving into Rumbo from Neblina, the reference house is the fourth on the north side:
    // the first stands on the corner lot, turned with the curve, its front on Rumbo and a
    // planted side yard toward Neblina.
    let (rumbo, neblina) = (Street::Rumbo.line(), Street::NeblinaGaviota.line());
    let mut north: Vec<SendaHouse> = senda_houses()
        .into_iter()
        .filter(|house| house.row == Row::RumboNorth)
        .collect();
    north.sort_by(|a, b| {
        let along = |house: &SendaHouse| rumbo.along(DVec2::new(house.x, house.z));
        along(a).total_cmp(&along(b))
    });
    assert!(
        north.len() >= 9,
        "{} houses on Rumbo's north side",
        north.len()
    );
    assert_eq!(north[0].plan.style, CORNER_SPANISH, "{:?}", north[0]);
    assert_eq!(
        north[3].plan.style, REFERENCE_HOUSE,
        "the fourth house: {:?}",
        north[3]
    );
    let style = SENDA_STYLES[usize::from(REFERENCE_HOUSE)];
    assert!(style.gable_front && style.room_porch && style.cover == RoofCover::Tile);
    assert_eq!(
        north
            .iter()
            .filter(|house| house.plan.style == REFERENCE_HOUSE)
            .count(),
        1
    );
    // The corner house: its door looks out on Rumbo, nearer Rumbo's kerb than
    // Neblina's, and its flank stands back from Neblina's sidewalk past a side yard,
    // within a lot of it.
    let corner = north[0];
    let design = corner.design();
    let at = DVec2::new(corner.x, corner.z);
    let (centre, out, _, _) = wall_frames(design.w, design.d)[design.door_wall];
    let face = design.to_world(centre);
    let face = at + DVec2::new(face.x, face.z);
    let out = design.to_world(out);
    let step = face + DVec2::new(out.x, out.z);
    assert!(
        rumbo.distance(step) < rumbo.distance(face),
        "the corner house fronts Rumbo"
    );
    assert!(
        rumbo.distance(face) < neblina.distance(face),
        "the corner house fronts Neblina"
    );
    let neblina_walk = corners(&corner)
        .iter()
        .map(|&p| neblina.distance(p) - (STREET_HALF + PARKWAY + SIDEWALK))
        .fold(f64::INFINITY, f64::min);
    assert!(
        (2.5..LOT_PITCH).contains(&neblina_walk),
        "the corner house stands {neblina_walk:.1} m off Neblina's sidewalk"
    );
}

#[test]
fn wheelie_bins_stand_in_pairs_against_the_kerb_and_explode_as_drums() {
    use sloppy_core::sim::senda_layout::{TRASH_BIN, TRASH_BIN_COLORS, is_trash_bin};
    let sim = senda(5.0);
    let bins: Vec<_> = sim
        .covers
        .iter()
        .filter(|cover| is_trash_bin(cover.color))
        .collect();
    assert!(bins.len() >= 8, "{} bins", bins.len());
    let lines = Street::ALL.map(|street| (street, street.line()));
    for pair in bins.chunks(2) {
        // A trash bin and a recycling bin side by side, turned alike.
        let [trash, recycling] = pair else {
            panic!("an odd bin out")
        };
        assert_eq!([trash.color, recycling.color], TRASH_BIN_COLORS);
        assert_eq!(trash.house_plan, recycling.house_plan);
        let apart = DVec2::new(trash.x - recycling.x, trash.z - recycling.z).length();
        assert!(
            (0.6 * SENDA_SCALE..1.2 * SENDA_SCALE).contains(&apart),
            "{apart}"
        );
        for bin in pair {
            // Explosive drums, in the street against its kerb.
            assert_eq!(bin.kind, CoverKind::Drum);
            assert!(bin.destructible && bin.hp > 0.0);
            let (w, d, h) = TRASH_BIN;
            let size = (w * SENDA_SCALE, d * SENDA_SCALE, h * SENDA_SCALE);
            assert_eq!((bin.w, bin.d, bin.h), size);
            let at = DVec2::new(bin.x, bin.z) / SENDA_SCALE;
            let inside_kerb = lines
                .iter()
                .map(|(street, line)| street.half() - line.distance(at))
                .filter(|off| *off > 0.0)
                .fold(f64::INFINITY, f64::min);
            assert!(
                inside_kerb < 1.3,
                "a bin {inside_kerb:.2} m off the kerb at {at:?}"
            );
        }
    }
}

#[test]
fn a_turned_house_blocks_navigation_by_its_turned_box() {
    use sloppy_core::sim::arena::box_turn;
    use sloppy_core::sim::navigation::Footprint;
    use sloppy_core::sim::structure_physics::ground_footprints;
    let sim = senda(5.0);
    let house = sim
        .covers
        .iter()
        .filter(|cover| cover.kind == CoverKind::House)
        .max_by(|a, b| {
            let turn =
                |cover: &&sloppy_core::sim::Cover| box_turn(cover.kind, cover.house_plan).abs();
            turn(a).total_cmp(&turn(b))
        })
        .expect("a house");
    let turn = box_turn(house.kind, house.house_plan);
    assert!(turn.abs() > 20f64.to_radians(), "{turn}");
    let mut squares: Vec<Footprint> = Vec::new();
    ground_footprints(house, &mut squares);
    let covered = |p: DVec2| {
        squares
            .iter()
            .any(|f| (p.x - f.x).abs() <= f.w / 2.0 && (p.y - f.z).abs() <= f.d / 2.0)
    };
    let (sin, cos) = turn.sin_cos();
    // Into the world as a house's build frame turns, and back.
    let world =
        |u: f64, v: f64| DVec2::new(house.x + u * cos + v * sin, house.z - u * sin + v * cos);
    let local = |p: DVec2| {
        let (x, z) = (p.x - house.x, p.y - house.z);
        (x * cos - z * sin, x * sin + z * cos)
    };
    let (hw, hd) = (house.w / 2.0 - 0.3, house.d / 2.0 - 0.3);
    for (u, v) in [(hw, hd), (-hw, hd), (hw, -hd), (-hw, -hd)] {
        assert!(covered(world(u, v)), "the turned corner ({u}, {v}) is open");
    }
    // The square box's corners that stand well outside the turned one stay open.
    let outside = [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)]
        .map(|(sx, sz)| DVec2::new(house.x + sx * hw, house.z + sz * hd))
        .into_iter()
        .filter(|&p| {
            let (u, v) = local(p);
            u.abs() > house.w / 2.0 + 0.5 || v.abs() > house.d / 2.0 + 0.5
        })
        .collect::<Vec<_>>();
    assert!(!outside.is_empty());
    for p in outside {
        assert!(!covered(p), "the square box's corner {p:?} blocks");
    }
}

#[test]
fn a_patio_loggia_stands_as_cover_and_falls_in_pieces_when_shot() {
    use sloppy_core::sim::FragmentShape;
    use sloppy_core::sim::senda_yards::{LOGGIA_COLOR, PERGOLA_WOOD};
    let mut sim = senda(5.0);
    let patios = |color: u32| sim.covers.iter().filter(|c| c.color == color).count();
    assert!(
        patios(LOGGIA_COLOR) >= 10,
        "{} loggias",
        patios(LOGGIA_COLOR)
    );
    assert!(
        patios(PERGOLA_WOOD) >= 1,
        "{} pergolas",
        patios(PERGOLA_WOOD)
    );
    let loggia = sim
        .covers
        .iter()
        .position(|c| c.color == LOGGIA_COLOR)
        .expect("a loggia");
    assert!(sim.covers[loggia].destructible && sim.covers[loggia].alive);
    let fragments = sim.fragments.len();
    sim.damage_cover(loggia, 1_000.0, 999, Team::Red, None, None);
    assert!(!sim.covers[loggia].alive);
    let pieces: Vec<_> = sim.fragments[fragments..].iter().collect();
    // Two columns and the beam, and its roof in two halves, not three anonymous shards.
    assert_eq!(pieces.len(), 5, "{pieces:?}");
    assert!(
        pieces.iter().all(|f| f.shape == Some(FragmentShape::Panel)),
        "{pieces:?}"
    );
}

#[test]
fn each_porch_patch_is_clear_and_each_loggia_fits_its_yard() {
    use sloppy_core::sim::senda_yards::{
        PATIO_COVER, PATIO_ROOF, PORCH, front_openings, patio_covers, porch,
    };
    let houses = senda_houses();
    let lots = senda_lots(&houses);
    let layout = senda_design_layout();
    let (low, eave) = PATIO_ROOF;
    assert!((low, eave) == (1.75, 2.1), "single-storey roofs");
    for lot in &lots {
        // The patch the house model's entry porch stands on: on the front door's half,
        // within the front yard, off the driveway and clear of every cover but the house.
        let (middle, width, depth) = porch(lot).expect("a porch before every house");
        assert!(width <= PORCH.0 && depth <= PORCH.1 && depth < lot.house.setback);
        let (door, garage) = front_openings(lot).expect("a door and a garage");
        let design = lot.house.design();
        let driveway = design.wall_plan(design.door_wall).garage_width() / 2.0;
        assert!(
            (middle - garage).signum() == (door - garage).signum()
                && middle.abs() + width / 2.0 <= lot.half_width + 1e-9
                && (middle - garage).abs() > driveway + width / 2.0 - 1e-9,
            "a porch at {middle:.2} across a front with its door at {door:.2}, garage at \
             {garage:.2}"
        );
        // Trees, bins and lamps (the walls run along the lot lines and the level's edge).
        let small = |c: &&sloppy_core::sim::arena::CoverDef| {
            c.kind == CoverKind::Tree || c.kind == CoverKind::Drum || c.w.max(c.d) < 0.5
        };
        for c in layout.iter().filter(small) {
            let (u, v) = lot.local(DVec2::new(c.x, c.z));
            let reach = c.w.max(c.d) / 2.0;
            assert!(
                (u - middle).abs() > width / 2.0 + reach
                    || v - reach > lot.half_depth + depth
                    || v + reach < lot.half_depth,
                "{:?} at {},{} on the porch of {:?}",
                c.kind,
                c.x,
                c.z,
                lot.house
            );
        }
    }
    for (lot, cover) in patio_covers(&houses).iter().map(|cover| {
        let lot = lots
            .iter()
            .min_by(|a, b| {
                let at = DVec2::new(cover.x, cover.z);
                a.centre.distance(at).total_cmp(&b.centre.distance(at))
            })
            .expect("a lot");
        (lot, cover)
    }) {
        // A rear wing 1.2-2.0 m deep and no wider than 3.6 m, short of the back bed.
        assert!(cover.w <= PATIO_COVER.0 && (1.2..=PATIO_COVER.1).contains(&cover.d));
        assert!(cover.d <= back_yard(lot) - 0.5, "{cover:?} fills its yard");
    }
}

#[test]
fn street_trees_stand_in_the_parkway_strips_before_most_lots_never_before_a_driveway() {
    let houses = senda_houses();
    let lots = senda_lots(&houses);
    let lines = Street::ALL.map(|street| (street, street.line()));
    let layout = senda_design_layout();
    let street_trees: Vec<_> = layout
        .iter()
        .filter(|c| c.kind == CoverKind::Tree)
        .filter(|c| {
            let at = DVec2::new(c.x, c.z);
            // In a street's parkway or a turning circle's (its island's tree is not one).
            let on_island = Street::ALL
                .into_iter()
                .filter_map(Street::cul_de_sac)
                .any(|middle| middle.distance(at) < CUL_DE_SAC_ISLAND);
            !on_island
                && lines.iter().any(|(street, line)| {
                    *street != Street::SendaParkway && line.distance(at) - street.half() < PARKWAY
                })
                || Street::ALL
                    .into_iter()
                    .filter_map(Street::cul_de_sac)
                    .any(|middle| {
                        (CUL_DE_SAC_RADIUS..CUL_DE_SAC_RADIUS + PARKWAY)
                            .contains(&middle.distance(at))
                    })
        })
        .collect();
    assert!(
        street_trees.len() * 2 > houses.len(),
        "{} street trees for {} lots",
        street_trees.len(),
        houses.len()
    );
    for tree in &street_trees {
        let at = DVec2::new(tree.x, tree.z);
        // Its trunk between the kerb and the sidewalk, its box a quarter metre over each.
        assert!(
            tree.w <= PARKWAY + 0.5 && tree.d <= PARKWAY + 0.5,
            "{tree:?}"
        );
        for (street, line) in &lines {
            let off = line.distance(at) - street.half();
            assert!(
                off > 0.0 && !(PARKWAY..PARKWAY + SIDEWALK).contains(&off),
                "a tree at {at:?} off {street:?}'s parkway"
            );
        }
        // Never before a driveway: clear of every garage door's span out to the kerb.
        for lot in &lots {
            let Some((_, garage)) = sloppy_core::sim::senda_yards::front_openings(lot) else {
                continue;
            };
            let design = lot.house.design();
            let half = design.wall_plan(design.door_wall).garage_width() / 2.0;
            let (u, v) = lot.local(at);
            let reach = lot.half_depth + lot.house.setback + SIDEWALK + PARKWAY;
            assert!(
                (u - garage).abs() > half + tree.w / 2.0 || v < lot.half_depth || v > reach,
                "a tree at {at:?} before the driveway of {:?}",
                lot.house
            );
        }
    }
}

#[test]
fn the_street_lamps_inside_the_level_are_cover_a_tank_cannot_drive_through() {
    use sloppy_core::sim::senda_layout::street_lamp_look;
    let sim = senda(5.0);
    let lamps: Vec<_> = sim
        .covers
        .iter()
        .filter(|cover| street_lamp_look(cover.color).is_some())
        .collect();
    assert!(lamps.len() >= 5, "{} lamps", lamps.len());
    for lamp in lamps {
        assert!(lamp.alive && lamp.destructible);
        assert!(sim.world.bodies.contains(lamp.body), "a lamp with no body");
        assert!(sim.nav.is_blocked(Vec2::new(lamp.x, lamp.z)), "{lamp:?}");
    }
}

/// A tank's shortest drive (m) from `team`'s spawn pads to `to`, by the navigation.
fn drive(sim: &mut Simulation, team: Team, to: Vec2) -> f64 {
    spawn_positions(team, SENDA_SCALE)
        .into_iter()
        .map(|start| {
            let path = sim.nav.find(start, to);
            if path.last().is_some_and(|end| distance(*end, to) < ARRIVAL) {
                route_length(start, &path)
            } else {
                f64::INFINITY
            }
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn each_teams_pickups_lie_as_far_from_its_spawns_as_the_other_teams() {
    // Pickups come in pairs, one nearer each team's spawns: the blue team's drive to its
    // one and the red team's to theirs within 25%, and the laser, alone in the middle,
    // as far from both.
    let mut sim = senda(41.0);
    let drives: Vec<(PickupKind, Vec2, f64, f64)> = SENDA_PICKUPS
        .iter()
        .map(|p| {
            let at = world(DVec2::new(p.x, p.z));
            (
                p.kind,
                at,
                drive(&mut sim, Team::Blue, at),
                drive(&mut sim, Team::Red, at),
            )
        })
        .collect();
    let table: Vec<String> = drives
        .iter()
        .map(|(kind, at, blue, red)| {
            format!(
                "{kind:?} ({:.0}, {:.0}): blue {blue:.0} m, red {red:.0} m",
                at.x, at.z
            )
        })
        .collect();
    let even = |a: f64, b: f64| (a - b).abs() <= 0.25 * a.max(b);
    let mut kinds: Vec<PickupKind> = drives.iter().map(|d| d.0).collect();
    kinds.dedup();
    for kind in kinds {
        let of: Vec<_> = drives.iter().filter(|d| d.0 == kind).collect();
        if of.len() == 1 {
            let (_, _, blue, red) = of[0];
            assert!(even(*blue, *red), "{kind:?} uneven in {table:#?}");
            continue;
        }
        let mut blue: Vec<f64> = of.iter().filter(|d| d.2 < d.3).map(|d| d.2).collect();
        let mut red: Vec<f64> = of.iter().filter(|d| d.3 <= d.2).map(|d| d.3).collect();
        assert_eq!(
            blue.len(),
            red.len(),
            "{kind:?} is not in pairs in {table:#?}"
        );
        blue.sort_by(f64::total_cmp);
        red.sort_by(f64::total_cmp);
        for (b, r) in blue.iter().zip(&red) {
            assert!(
                even(*b, *r),
                "{kind:?}: blue's {b:.0} m against red's {r:.0} m in {table:#?}"
            );
        }
    }
}
