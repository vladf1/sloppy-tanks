//! Senda: its houses front their streets in rows that leave every spawn, pickup and
//! flank lane clear and reachable from both spawn lines, its houses are the
//! neighbourhood's three types built of parts, and a house falls like the village's.

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
use sloppy_core::sim::navigation::GRID_SIZE;
use sloppy_core::sim::senda_layout::{
    CUL_DE_SAC_RADIUS, HOUSE_DEPTH, HOUSE_WIDTH, LOT_PITCH, Line, PARKWAY, Row, SENDA_FENCE_COLOR,
    SENDA_MAP, SENDA_PICKUPS, SENDA_WALL_COLOR, SETBACK, SIDEWALK, STREET_HALF, SendaHouse, Street,
    WALL_THICKNESS, WallFace, bend_park, greenbelt_mouth, house_front, inside_boundary,
    is_senda_paved, left, neblina_bend, senda_boundary, senda_houses,
};
use sloppy_core::sim::senda_yards::{
    YARD_GATE_COLOR, YARD_PERIMETER_COLOR, YARD_VIEW_COLOR, YARD_WALL_COLOR, greenbelt_lane,
    park_corners, senda_lots, yard_walls,
};
use sloppy_core::sim::{CoverKind, Simulation, SimulationSetup, Team};
use support::{assert_spawns_and_pickups_clear, fell_cottage};

/// Clear floor a tank hull needs around a spawn, pickup or flank waypoint.
const HULL_CLEARANCE: f64 = 2.0;
/// A route ends in the navigation cell that contains its goal.
const ARRIVAL: f64 = 1.8;

fn senda(seed: f64) -> Simulation {
    let streets = extra_level(MapId::Neighborhood).expect("the Neighborhood has its own streets");
    Simulation::new(seed, standard_rules().merged(streets))
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
        spawn_positions(Team::Blue, 1.0)[0],
        spawn_positions(Team::Red, 1.0)[0],
    ];
    // The bots' flank waypoints and the line they patrol along each side: one in a
    // house or a yard sends a bot to the nearest open cell, which lies inside the wall
    // and in reach of both spawn lines.
    let boundary = senda_boundary();
    let patrol = [-20.0, 20.0]
        .into_iter()
        .flat_map(|x| [-38.0, 0.0, 38.0].map(|z| Vec2::new(x, z)))
        .chain(
            [-46.0, 46.0]
                .into_iter()
                .flat_map(|x| [-44.0, -22.0, 0.0, 22.0, 44.0].map(|z| Vec2::new(x, z))),
        );
    for waypoint in patrol {
        let goal = if sim.nav.is_blocked(waypoint) {
            sim.nav.point(sim.nav.nearest(sim.nav.index(waypoint)))
        } else {
            waypoint
        };
        assert!(
            inside_boundary(&boundary, DVec2::new(goal.x, goal.z)),
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
    assert!((40..=80).contains(&houses.len()), "{} houses", houses.len());
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
    // Every pair of neighbours keeps a side yard that shows from overhead between
    // their walls and eaves (the real lots' is about a third of a house's width).
    let footprints: Vec<[DVec2; 4]> = houses.iter().map(corners).collect();
    for (i, a) in footprints.iter().enumerate() {
        for b in &footprints[i + 1..] {
            let yard = gap(a, b);
            assert!(
                yard >= 0.36 * HOUSE_WIDTH,
                "a side yard of {yard:.2} m between {a:?} and {b:?}"
            );
        }
    }
    // The rows: Rumbo's north and south sides, Gaviota's (with the houses on the inside
    // of Neblina's curve), and Neblina's west side from Senda Parkway round the outside
    // of the curve to the park, where it becomes Gaviota's south row. No street runs on
    // south. Each house's row is the side of the street it fronts; the south rows run on
    // a lot past their turning circles.
    let gaviota = Street::NeblinaGaviota.line();
    let count = |row: Row| houses.iter().filter(|house| house.row == row).count();
    let rows = Row::ALL.map(count);
    let least = [10, 9, 8, 8, 8];
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
    // curve, its houses between the spawn pads with no lot left empty, each turned
    // square to the street where it stands, so the row steps round the curve.
    assert!(
        west_turns.len() > 1,
        "Neblina's west row turns {west_turns:?}"
    );
    neblina_west.sort_by(f64::total_cmp);
    for pair in neblina_west.windows(2) {
        assert!(
            pair[1] - pair[0] < 1.5 * LOT_PITCH,
            "a gap of {:.1} m in Neblina's west row: {neblina_west:?}",
            pair[1] - pair[0]
        );
    }
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
    // to the level's wall and stands within a lane and a tree of it.
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
        for u in [lot.line_u(0), 0.0, lot.line_u(1)] {
            let rear = lot.at(u, lot.rear_v());
            let room = wall.distance(rear);
            assert!(
                inside_boundary(&boundary, rear) && room > 4.0,
                "the back wall at {rear:?} keeps {room:.1} m from the level's wall"
            );
            if (-20.0..40.0).contains(&rear.x) {
                assert!(
                    room < 8.0,
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
    let layout = (SENDA_MAP.layout)();
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
    let cell_of = |at: DVec2| sim.nav.index(Vec2::new(at.x, at.y));
    for &pad in &pads {
        let start = cell_of(pad);
        let mut seen = vec![false; sim.nav.blocked.len()];
        let mut open = vec![start];
        seen[start] = true;
        while let Some(cell) = open.pop() {
            let point = sim.nav.point(cell);
            assert!(
                inside_boundary(&boundary, DVec2::new(point.x, point.z)),
                "a tank from {pad:?} gets out at {point:?}"
            );
            let (x, z) = ((cell % GRID_SIZE) as i64, (cell / GRID_SIZE) as i64);
            for (dx, dz) in neighbours {
                let (nx, nz) = (x + dx, z + dz);
                let size = GRID_SIZE as i64;
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
                "a lot line in a side yard: {u:.2}"
            );
            assert!(
                v < lot.half_depth,
                "a gate behind the house's front: {v:.2}"
            );
            let (_, rear) = lot.local(side.rear);
            assert!(
                rear < -lot.half_depth - 1.0,
                "a back yard behind the house: {rear:.2}"
            );
        }
    }
}

#[test]
fn the_back_yards_run_deep_but_leave_the_greenbelt_its_lane_and_the_bend_park_is_walled() {
    let houses = senda_houses();
    let lots = senda_lots(&houses);
    // Most yards show from the bank and the slopes: the tract's run 4.5-6 m, about
    // 2-4 m at the level's scale; only a pickup, a spawn pad, the level's wall or the
    // greenbelt's lane cuts one short.
    let deep = lots.iter().filter(|lot| lot.yard >= 3.0).count();
    assert!(
        deep * 2 > lots.len(),
        "{deep} of {} back yards run 3 m or more",
        lots.len()
    );
    // Across the greenbelt the back walls keep a tank's lane between them.
    let lane = Line(greenbelt_lane());
    assert!(lane.0.len() > 1, "the greenbelt keeps a lane");
    for wall in yard_walls(&houses) {
        let at = DVec2::new(wall.x, wall.z);
        let along = lane.along(at);
        // Clear of the lane's ends, where it leaves through the mouth and onto the hillside.
        if along > 10.0 && along < lane.length() - 10.0 {
            assert!(
                lane.distance(at) > 2.0,
                "a lot wall at {at:?} stands in the greenbelt's lane"
            );
        }
    }
    let (a, b) = park_corners(&lots).expect("the bend park has a back wall");
    let park = bend_park();
    assert!(
        a.distance(park) < 14.0 && b.distance(park) < 14.0 && a.distance(b) > 4.0,
        "the bend park's back wall runs from {a:?} to {b:?}"
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

#[test]
fn the_rows_run_straight_to_the_cul_de_sacs_and_their_far_rims_open_onto_the_hillside() {
    // As the tract's do: each row runs on square to its street beside the turning
    // circle, a house on either side at its rim; the north rows end short of the
    // circle's far side, which opens onto the hillside, and the south rows hold one lot
    // more, under its south-east rim.
    let houses = senda_houses();
    for (street, sides) in [
        (Street::Rumbo, [Row::RumboNorth, Row::RumboSouth]),
        (
            Street::NeblinaGaviota,
            [Row::GaviotaNorth, Row::GaviotaSouth],
        ),
    ] {
        let middle = street.cul_de_sac().expect("a turning circle");
        for row in sides {
            let rim = houses
                .iter()
                .filter(|house| house.row == row)
                .map(|house| {
                    corners(house)
                        .iter()
                        .map(|corner| corner.distance(middle) - CUL_DE_SAC_RADIUS)
                        .fold(f64::INFINITY, f64::min)
                })
                .fold(f64::INFINITY, f64::min);
            assert!(rim < 6.0, "{row:?} keeps {rim:.1} m off its turning circle");
        }
        for house in houses.iter().filter(|house| sides.contains(&house.row)) {
            let south = house.row == sides[1];
            let reach = if south {
                CUL_DE_SAC_RADIUS + LOT_PITCH
            } else {
                CUL_DE_SAC_RADIUS
            };
            assert!(
                house.x < middle.x + reach,
                "a house at {},{} past the {street:?} circle's far rim",
                house.x,
                house.z
            );
            // Past the circle's far rim only on its south side.
            if house.x > middle.x + CUL_DE_SAC_RADIUS {
                assert!(south && house.z > middle.y, "{house:?}");
            }
        }
        let past = houses
            .iter()
            .filter(|house| house.row == sides[1] && house.x > middle.x)
            .count();
        assert!(past >= 1, "no lot past the {street:?} circle's middle");
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
        north.len() >= 10,
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
        assert!((0.6..1.2).contains(&apart), "{apart}");
        for bin in pair {
            // Explosive drums, in the street against its kerb.
            assert_eq!(bin.kind, CoverKind::Drum);
            assert!(bin.destructible && bin.hp > 0.0);
            assert_eq!((bin.w, bin.d, bin.h), TRASH_BIN);
            let at = DVec2::new(bin.x, bin.z);
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
        patios(PERGOLA_WOOD) >= 2,
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
    assert_eq!(
        pieces
            .iter()
            .filter(|f| f.shape == Some(FragmentShape::Panel))
            .count(),
        2
    );
}
