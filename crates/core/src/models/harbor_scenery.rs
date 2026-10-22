//! Port of `harbor-scenery.ts`: Harbor Havoc's retained scenery. The quay apron,
//! painted bays and labels, bollards, berths and warehouses; the water basin; the
//! moored fleet; and blinking beacons. All large scenery stays outside playable
//! cover.

use std::f64::consts::{FRAC_PI_2, PI};
use std::sync::Arc;

use crate::geometry::{plane_geometry, ring_geometry, torus_geometry};
use crate::scene::{Material, Node, TextureRef};

use super::batching::batch;
use super::effects_scenery::ApronMode;
use super::effects_scenery::harbor_label_texture;
use super::harbor_backdrop::harbor_backdrop;
use super::harbor_models::cladding;
use super::harbor_surfaces::{apron_box, apron_material, arrow_marking, marking};
use super::harbor_vessels::{harbor_fleet, update_harbor_fleet};
use super::harbor_water::harbor_water;
use super::house_surfaces::siding_box;
use super::model_primitives::{
    DEFAULT_BOX_RADIUS, TEAM_COLORS, beam, box_part, cylinder_part, paint, put, rotated,
};
use crate::sim::arena::{BOUNDARY_THICKNESS, spawn_positions};
use crate::sim::data::ARENA;
use crate::sim::types::{Team, Vec2};

/// Index of the fleet among [`HarborScenery::root`]'s children; the beacons are
/// the last child.
const FLEET_CHILD: usize = 1;

/// Harbor Havoc scenery, built once and reused across rounds (`HarborScenery`).
pub struct HarborScenery {
    /// Children: the water, the fleet, the painted labels, the batched details,
    /// then the beacons.
    pub root: Node,
}

impl Default for HarborScenery {
    fn default() -> Self {
        Self::new()
    }
}

/// `paintLabel(group, text, x, z, width, depth)`: painted lettering flat on the
/// apron, drawn by the browser (see `effects_scenery::harbor_label_texture`).
fn paint_label(group: &mut Node, text: &'static str, x: f64, z: f64, width: f64, depth: f64) {
    let label = Node::mesh(
        Arc::new(plane_geometry(width, depth)),
        Arc::new(Material {
            map: Some(TextureRef::generated(harbor_label_texture(text))),
            ..(*apron_material(ApronMode::Label, 0xffffff)).clone()
        }),
    );
    put(group, rotated(label, -FRAC_PI_2, 0.0, 0.0), x, 0.035, z);
}

/// Merge a group of ground-level parts, which receive shadows but cast none.
fn batch_receivers(group: &mut Node) {
    batch(group);
    for child in &mut group.children {
        if let Some(drawable) = &mut child.drawable {
            drawable.cast_shadow = false;
        }
    }
}

/// Apron concrete under the playfield, the asphalt of its traffic lanes and the
/// quay walls rising from the water.
const APRON_COLOR: u32 = 0x989c99;
const ASPHALT_COLOR: u32 = 0x4b5052;
const QUAY_WALL_COLOR: u32 = 0x9c9e94;
/// Traffic lanes: (centre, width) of the lanes running along z (centre x) and
/// along x (centre z).
const LANES_ALONG_Z: [(f64, f64); 3] = [(-52.0, 10.0), (0.0, 15.0), (52.0, 10.0)];
const LANES_ALONG_X: [(f64, f64); 3] = [(-45.0, 10.0), (0.0, 10.0), (45.0, 10.0)];
/// Inset of a lane's solid edge line from the asphalt edge, and the dash and gap of
/// its centre line, metres.
const EDGE_LINE_INSET: f64 = 0.35;
const CENTRE_DASH: f64 = 3.0;
const CENTRE_GAP: f64 = 3.0;
/// Lane and bay paint.
const LINE_YELLOW: u32 = 0xe8b83e;
const LINE_WHITE: u32 = 0xe4e2d6;
const HAZARD_DARK: u32 = 0x2b3236;

/// Floor of the transit sheds on the outer quays.
const WAREHOUSE_FLOOR: f64 = -0.5;
const WAREHOUSE_CLADDING: u32 = 0x4f7480;
const WAREHOUSE_ROOF: u32 = 0x7d8a8c;
const SKYLIGHT: u32 = 0xa9c4c8;
const ROLLER_DOOR: u32 = 0x8f9a9b;

/// A transit shed on an outer quay, its doors toward the arena (+x): corrugated
/// cladding with a shipping line's livery, a flat roof with skylight strips and a
/// ridge vent, roller doors under yellow headers with guard posts, and downpipes.
fn warehouse() -> Node {
    let (width, length, height) = (18.0, 70.0, 7.0);
    let mut shed = Node::group("");
    put(
        &mut shed,
        box_part(width + 0.6, 0.6, length + 0.6, 0x8d8f88, 0.0),
        0.0,
        0.3,
        0.0,
    );
    put(
        &mut shed,
        box_part(width - 0.3, height, length - 0.3, 0x2f3d44, 0.0),
        0.0,
        height / 2.0,
        0.0,
    );
    let face = |livery| {
        let mut wall = Node::group("");
        cladding(&mut wall, length, height, WAREHOUSE_CLADDING, livery);
        wall
    };
    let end = || {
        let mut wall = Node::group("");
        cladding(&mut wall, width, height, WAREHOUSE_CLADDING, None);
        wall
    };
    let half = (width / 2.0 - 0.15, length / 2.0 - 0.15);
    put(
        &mut shed,
        rotated(face(Some((0, 16.0))), 0.0, FRAC_PI_2, 0.0),
        half.0,
        0.0,
        0.0,
    );
    put(
        &mut shed,
        rotated(face(None), 0.0, -FRAC_PI_2, 0.0),
        -half.0,
        0.0,
        0.0,
    );
    put(&mut shed, end(), 0.0, 0.0, half.1);
    put(&mut shed, rotated(end(), 0.0, PI, 0.0), 0.0, 0.0, -half.1);
    put(
        &mut shed,
        box_part(width + 0.6, 0.3, length + 0.6, WAREHOUSE_ROOF, 0.0),
        0.0,
        height + 0.15,
        0.0,
    );
    put(
        &mut shed,
        box_part(1.4, 0.6, length - 4.0, WAREHOUSE_ROOF, 0.0),
        0.0,
        height + 0.6,
        0.0,
    );
    for x in [-4.6, 4.6] {
        put(
            &mut shed,
            box_part(2.4, 0.1, length - 8.0, SKYLIGHT, 0.0),
            x,
            height + 0.33,
            0.0,
        );
    }
    let door_face = width / 2.0 + 0.05;
    for z in [-26.0, -13.0, 13.0, 26.0] {
        put(
            &mut shed,
            box_part(0.2, 4.6, 4.8, ROLLER_DOOR, 0.0),
            door_face,
            2.3 + 0.1,
            z,
        );
        for y in [0.9, 1.7, 2.5, 3.3, 4.1] {
            put(
                &mut shed,
                box_part(0.24, 0.05, 4.8, 0x6d7778, 0.0),
                door_face,
                y,
                z,
            );
        }
        put(
            &mut shed,
            box_part(0.4, 0.4, 5.4, BOLLARD_CAP, 0.0),
            door_face + 0.05,
            4.9,
            z,
        );
        for side in [-1.0, 1.0] {
            put(
                &mut shed,
                cylinder_part(0.15, 1.1, BOLLARD_CAP, 10),
                door_face + 0.7,
                0.55,
                z + side * 2.8,
            );
        }
    }
    for z in [-34.0, -19.5, 19.5, 34.0] {
        put(
            &mut shed,
            box_part(0.16, height, 0.16, 0x2b3236, 0.0),
            door_face + 0.05,
            height / 2.0,
            z,
        );
    }
    shed
}

/// Edge of the apron the lanes run to.
const APRON_EDGE: f64 = 59.0;
/// Lane runs shorter than this get no markings.
const MIN_MARKED_RUN: f64 = 4.0;

/// The stretches of one lane between the lanes that cross it, as (start, end).
fn lane_runs(crossing: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut runs = Vec::new();
    let mut start = -APRON_EDGE;
    for &(centre, width) in crossing {
        runs.push((start, centre - width / 2.0));
        start = centre + width / 2.0;
    }
    runs.push((start, APRON_EDGE));
    runs.retain(|(a, b)| b - a >= MIN_MARKED_RUN);
    runs
}

/// Lane paint, laid out along z (`along_z`) or along x: solid white edge lines,
/// a dashed centre line, and a pair of direction arrows at each end of a run.
fn lane_markings(markings: &mut Node) {
    for (along_z, lanes, crossing) in [
        (true, LANES_ALONG_Z, LANES_ALONG_X),
        (false, LANES_ALONG_X, LANES_ALONG_Z),
    ] {
        // `along` runs down the lane, `across` from its centre; y is the paint top.
        let y = if along_z { 0.026 } else { 0.034 };
        let mut put_on_lane = |part: Node, across: f64, along: f64, yaw: f64| {
            let (x, z) = if along_z {
                (across, along)
            } else {
                (along, across)
            };
            let yaw = if along_z { yaw } else { yaw + FRAC_PI_2 };
            put(markings, rotated(part, 0.0, yaw, 0.0), x, y, z);
        };
        for (centre, width) in lanes {
            for (start, end) in lane_runs(&crossing) {
                let (mid, length) = ((start + end) / 2.0, end - start);
                for side in [-1.0, 1.0] {
                    let edge = centre + side * (width / 2.0 - EDGE_LINE_INSET);
                    put_on_lane(marking(0.15, 0.018, length, LINE_WHITE), edge, mid, 0.0);
                }
                let mut dash = start + CENTRE_GAP / 2.0;
                while dash + CENTRE_DASH <= end - CENTRE_GAP / 2.0 {
                    put_on_lane(
                        marking(0.13, 0.018, CENTRE_DASH, LINE_WHITE),
                        centre,
                        dash + CENTRE_DASH / 2.0,
                        0.0,
                    );
                    dash += CENTRE_DASH + CENTRE_GAP;
                }
                if length < 12.0 {
                    continue;
                }
                // Keep-right traffic: arrows on each half of the lane, entering the run.
                for (half, toward, at) in [(1.0, 0.0, start + 4.0), (-1.0, PI, end - 4.0)] {
                    put_on_lane(
                        arrow_marking(LINE_WHITE),
                        centre + half * width / 4.0,
                        at,
                        toward,
                    );
                }
            }
        }
    }
}

/// The quay face: the outer face of the harbor's boundary wall.
const QUAY_EDGE: f64 = ARENA + BOUNDARY_THICKNESS;
/// Top of the harbor's boundary wall (`harbor_layout`).
const WALL_TOP: f64 = 1.2;
/// Mooring bollards along the wall top, just inside the quay face.
const BOLLARD_LINE: f64 = QUAY_EDGE - 0.6;
const BOLLARD_DARK: u32 = 0x262d30;
const BOLLARD_CAP: u32 = 0xe1b64d;
/// Flood-light masts at the quay corners.
const LAMP_HEIGHT: f64 = 9.0;
const LAMP_HOUSING: u32 = 0x2f3a3f;
const LAMP_LENS: u32 = 0xfff1c4;
/// Mooring lines: hemp color, thickness, sag at mid-span as a fraction of the
/// span, and the segments a line is drawn with.
const ROPE: u32 = 0xbaa377;
const ROPE_WIDTH: f64 = 0.085;
const ROPE_SAG: f64 = 0.06;
const ROPE_SEGMENTS: usize = 8;

/// A mooring line hanging between two points in a shallow catenary.
fn mooring_line(group: &mut Node, a: [f64; 3], b: [f64; 3]) {
    let span = ((b[0] - a[0]).powi(2) + (b[2] - a[2]).powi(2)).sqrt();
    let point = |t: f64| {
        let sag = 4.0 * t * (1.0 - t) * span * ROPE_SAG;
        [
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t - sag,
            a[2] + (b[2] - a[2]) * t,
        ]
    };
    for i in 0..ROPE_SEGMENTS {
        let t0 = i as f64 / ROPE_SEGMENTS as f64;
        let t1 = (i + 1) as f64 / ROPE_SEGMENTS as f64;
        beam(group, point(t0), point(t1), ROPE_WIDTH, ROPE);
    }
}

impl HarborScenery {
    pub fn new() -> Self {
        let mut root = Node::group("");
        root.children.push(harbor_water());
        root.children.push(harbor_fleet());
        let mut details = Node::group("");
        let mut ground = Node::group("");
        let mut markings = Node::group("");
        let mut beacons = Node::group("");
        put(
            &mut ground,
            apron_box(
                QUAY_EDGE * 2.0,
                2.6,
                QUAY_EDGE * 2.0,
                QUAY_WALL_COLOR,
                ApronMode::QuayWall,
            ),
            0.0,
            -1.4,
            0.0,
        );
        // Slab joints, stains and tyre wear are the apron effect's, in world space.
        put(
            &mut ground,
            apron_box(120.0, 0.12, 120.0, APRON_COLOR, ApronMode::Concrete),
            0.0,
            -0.06,
            0.0,
        );
        // Broad asphalt circulation lanes surround the numbered container bays.
        for (x, w) in LANES_ALONG_Z {
            put(
                &mut ground,
                apron_box(w, 0.015, 118.0, ASPHALT_COLOR, ApronMode::LaneAlongZ),
                x,
                0.008,
                0.0,
            );
        }
        for (z, w) in LANES_ALONG_X {
            put(
                &mut ground,
                apron_box(118.0, 0.012, w, ASPHALT_COLOR, ApronMode::LaneAlongX),
                0.0,
                0.018,
                z,
            );
        }
        lane_markings(&mut markings);
        for side in [-1.0, 1.0] {
            for z in [-32.0, -12.0, 12.0, 32.0] {
                for dx in [-3.7, 3.7] {
                    put(
                        &mut markings,
                        marking(0.12, 0.02, 15.0, LINE_YELLOW),
                        side * 30.0 + dx,
                        0.03,
                        z,
                    );
                }
                let bay = if z == -32.0 || z == 32.0 { "B2" } else { "B1" };
                paint_label(&mut root, bay, side * 37.0, z, 2.5, 1.4);
            }
            for x in (-57..=57).step_by(3) {
                let x = f64::from(x);
                put(
                    &mut markings,
                    marking(1.5, 0.025, 0.65, LINE_YELLOW),
                    x,
                    1.215,
                    side * 60.5,
                );
                put(
                    &mut markings,
                    marking(1.5, 0.025, 0.65, HAZARD_DARK),
                    x + 1.5,
                    1.215,
                    side * 60.5,
                );
            }
            for x in [-48.0, -24.0, 0.0, 24.0, 48.0] {
                // Mushroom bollards stand on the wall's outer edge, bolted to a base
                // plate; fenders hang on the quay face.
                let z = side * BOLLARD_LINE;
                put(
                    &mut details,
                    box_part(0.95, 0.08, 0.95, BOLLARD_DARK, 0.0),
                    x,
                    WALL_TOP + 0.04,
                    z,
                );
                put(
                    &mut details,
                    cylinder_part(0.27, 0.5, BOLLARD_DARK, 12),
                    x,
                    WALL_TOP + 0.3,
                    z,
                );
                put(
                    &mut details,
                    cylinder_part(0.42, 0.14, BOLLARD_CAP, 12),
                    x,
                    WALL_TOP + 0.6,
                    z,
                );
                let fender = rotated(cylinder_part(0.65, 1.4, 0x25363d, 12), FRAC_PI_2, 0.0, 0.0);
                put(&mut details, fender, x, -1.1, side * (QUAY_EDGE + 0.2));
            }
            // Flood-light masts at the quay corners: a crossarm with two lamps
            // aimed down at the apron, and the blinking beacon on top.
            for x in [-54.0, 54.0] {
                let z = side * 62.0;
                put(
                    &mut details,
                    box_part(0.8, 0.3, 0.8, 0x7b8584, 0.0),
                    x,
                    0.15,
                    z,
                );
                put(
                    &mut details,
                    cylinder_part(0.17, LAMP_HEIGHT, 0x45575c, 12),
                    x,
                    LAMP_HEIGHT / 2.0,
                    z,
                );
                put(
                    &mut details,
                    box_part(2.6, 0.16, 0.2, 0x45575c, 0.0),
                    x,
                    LAMP_HEIGHT - 0.2,
                    z,
                );
                for dx in [-1.05, 1.05] {
                    let head = rotated(
                        box_part(0.7, 0.42, 0.55, LAMP_HOUSING, 0.0),
                        -side * 0.35,
                        0.0,
                        0.0,
                    );
                    put(
                        &mut details,
                        head,
                        x + dx,
                        LAMP_HEIGHT - 0.5,
                        z - side * 0.15,
                    );
                    let lens = rotated(
                        box_part(0.6, 0.05, 0.44, LAMP_LENS, 0.0),
                        -side * 0.35,
                        0.0,
                        0.0,
                    );
                    put(
                        &mut details,
                        lens,
                        x + dx,
                        LAMP_HEIGHT - 0.73,
                        z - side * 0.07,
                    );
                }
                put(
                    &mut beacons,
                    cylinder_part(0.18, 0.3, 0xffb54a, 8),
                    x,
                    LAMP_HEIGHT + 0.15,
                    z,
                );
            }
        }
        // Central loading square is traversable; the laser pickup remains contested at its center.
        for side in [-1.0, 1.0] {
            put(
                &mut markings,
                marking(12.0, 0.025, 0.2, LINE_YELLOW),
                0.0,
                0.035,
                side * 5.0,
            );
            put(
                &mut markings,
                marking(0.2, 0.025, 10.0, LINE_YELLOW),
                side * 6.0,
                0.035,
                0.0,
            );
        }
        paint_label(&mut root, "HARBOR HAVOC", 0.0, -48.0, 28.0, 4.0);
        paint_label(&mut root, "PORT 07", 0.0, 48.0, 15.0, 3.5);
        paint_label(&mut root, "LOADING", 0.0, 3.5, 8.0, 1.1);
        for team in [Team::Blue, Team::Red] {
            for Vec2 { x, z } in spawn_positions(team, 1.0) {
                put(
                    &mut details,
                    cylinder_part(2.65, 0.08, 0x293f4a, 12),
                    x,
                    0.06,
                    z,
                );
                let ring = Node::mesh(
                    Arc::new(ring_geometry(2.1, 2.3, 32)),
                    paint(TEAM_COLORS[team.index()]),
                );
                put(
                    &mut details,
                    rotated(ring, -FRAC_PI_2, 0.0, 0.0),
                    x,
                    0.11,
                    z,
                );
            }
        }
        // Working berths sit beyond the wall: forklifts, pallets, drainage and tied mooring lines.
        for side in [-1.0, 1.0] {
            let x = side * 65.0;
            put(
                &mut ground,
                apron_box(7.0, 3.4, 13.0, QUAY_WALL_COLOR, ApronMode::QuayWall),
                x,
                -1.65,
                43.0,
            );
            put(
                &mut details,
                box_part(1.7, 0.75, 2.6, 0xdca53b, DEFAULT_BOX_RADIUS),
                x,
                0.65,
                43.0,
            );
            for dx in [-0.9, 0.9] {
                for z in [42.2, 43.9] {
                    let wheel =
                        rotated(cylinder_part(0.43, 0.3, 0x2c383c, 12), 0.0, 0.0, FRAC_PI_2);
                    put(&mut details, wheel, x + dx, 0.45, z);
                }
                put(
                    &mut details,
                    box_part(0.12, 1.7, 0.12, 0x344952, DEFAULT_BOX_RADIUS),
                    x + dx * 0.75,
                    1.6,
                    43.0,
                );
                put(
                    &mut details,
                    box_part(0.12, 2.4, 0.14, 0x344952, DEFAULT_BOX_RADIUS),
                    x + dx * 0.55,
                    1.3,
                    41.7,
                );
                put(
                    &mut details,
                    box_part(0.14, 0.12, 1.8, 0x647174, DEFAULT_BOX_RADIUS),
                    x + dx * 0.55,
                    0.25,
                    40.9,
                );
            }
            put(
                &mut details,
                box_part(1.7, 0.16, 1.7, 0xe5b441, DEFAULT_BOX_RADIUS),
                x,
                2.5,
                43.0,
            );
            put(
                &mut details,
                box_part(0.7, 0.7, 0.6, 0x344952, DEFAULT_BOX_RADIUS),
                x,
                1.35,
                43.25,
            );
            for level in 0..3 {
                let level = f64::from(level);
                for plank in 0..5 {
                    put(
                        &mut details,
                        siding_box(2.1, 0.12, 0.27, 0xb89764),
                        x,
                        0.3 + level * 0.3,
                        46.2 + f64::from(plank) * 0.34,
                    );
                }
                for dx in [-0.8, 0.8] {
                    put(
                        &mut details,
                        siding_box(0.25, 0.18, 1.7, 0x96764e),
                        x + dx,
                        0.14 + level * 0.3,
                        46.85,
                    );
                }
            }
            // Flush grates remain traversable, with no fake solid cover in driving lanes.
            for z in [-39.0, 0.0, 39.0] {
                put(
                    &mut details,
                    box_part(1.2, 0.025, 2.2, 0x36484b, 0.0),
                    side * 58.0,
                    0.035,
                    z,
                );
                for bar in 0..9 {
                    put(
                        &mut details,
                        box_part(1.1, 0.03, 0.06, 0x838c86, 0.0),
                        side * 58.0,
                        0.05,
                        z - 0.9 + f64::from(bar) * 0.22,
                    );
                }
            }
            mooring_line(
                &mut details,
                [side * QUAY_EDGE, 0.4, -24.0],
                [side * 72.0, 2.3, if side < 0.0 { -27.0 } else { -4.0 }],
            );
            mooring_line(
                &mut details,
                [side * QUAY_EDGE, 0.4, 24.0],
                [side * 72.0, 2.3, if side < 0.0 { 12.0 } else { 34.0 }],
            );
            mooring_line(
                &mut details,
                [side * 24.0, WALL_TOP + 0.5, -BOLLARD_LINE],
                [side * 20.0 - 8.0, 3.0, -73.0],
            );
            // Berth clutter: drums, a cable reel and a stack of spent tyres.
            for (i, (dx, dz)) in [(-2.4, -4.6), (-1.6, -5.1), (-2.2, -5.7)]
                .into_iter()
                .enumerate()
            {
                let color = [0x2f6fa8, 0xc8473b, 0x2f6fa8][i];
                put(
                    &mut details,
                    cylinder_part(0.3, 0.9, color, 12),
                    side * 65.0 + side * dx,
                    0.45,
                    43.0 + dz,
                );
            }
            for dz in [-0.45, 0.45] {
                let disc = rotated(cylinder_part(0.85, 0.08, 0x9b7a4e, 16), FRAC_PI_2, 0.0, 0.0);
                put(&mut details, disc, side * 66.8, 0.85, 37.6 + dz);
            }
            let core = rotated(cylinder_part(0.5, 0.82, 0x2b3236, 12), FRAC_PI_2, 0.0, 0.0);
            put(&mut details, core, side * 66.8, 0.85, 37.6);
            for level in 0..3 {
                let tyre = Node::mesh(Arc::new(torus_geometry(0.42, 0.17, 8, 14)), paint(0x23292b));
                put(
                    &mut details,
                    rotated(tyre, FRAC_PI_2, 0.0, 0.0),
                    side * 63.6,
                    0.17 + f64::from(level) * 0.32,
                    48.6,
                );
            }
        }
        // Distant warehouses close the horizon without creating obstacles on the board.
        for x in [-115.0, 115.0] {
            put(
                &mut ground,
                apron_box(22.0, 3.0, 78.0, QUAY_WALL_COLOR, ApronMode::QuayWall),
                x,
                -2.0,
                0.0,
            );
            let shed = rotated(warehouse(), 0.0, if x < 0.0 { 0.0 } else { PI }, 0.0);
            put(&mut details, shed, x, WAREHOUSE_FLOOR, 0.0);
        }
        root.children.push(harbor_backdrop());
        batch(&mut details);
        batch_receivers(&mut ground);
        batch_receivers(&mut markings);
        batch(&mut beacons);
        details.children.append(&mut ground.children);
        details.children.append(&mut markings.children);
        root.children.push(details);
        root.children.push(beacons);
        let mut scenery = Self { root };
        scenery.update(0.0);
        scenery
    }

    /// `update(time)`: ships bob, crane loads sway and the beacons blink. The water
    /// animates in its shader from the same clock.
    pub fn update(&mut self, time: f64) {
        update_harbor_fleet(&mut self.root.children[FLEET_CHILD], time);
        self.root.children.last_mut().expect("beacons").visible = (time * 2.5).sin() > -0.3;
    }
}
