//! Port of `harbor-scenery.ts`: Harbor Havoc's retained scenery. The quay apron,
//! painted bays and labels, bollards, berths and warehouses; the water basin; the
//! moored fleet; and blinking beacons. All large scenery stays outside playable
//! cover.

use std::f64::consts::{FRAC_PI_2, PI};
use std::sync::Arc;

use crate::geometry::math::hex_to_linear;
use crate::geometry::{Mesh, cylinder_geometry, plane_geometry, ring_geometry, torus_geometry};
use crate::scene::{Material, Node, TextureRef};

use super::batching::{batch, batch_in_cells};
use super::effects_scenery::{
    ApronMode, HARBOR_LABEL_ROWS, HARBOR_LABEL_TEXTURE, harbor_label_row,
};
use super::harbor_backdrop::{harbor_backdrop, wedge_part};
use super::harbor_models::{CargoShape, cladding, ship_container};
use super::harbor_surfaces::{apron_box, apron_material, arrow_marking, marking};
use super::harbor_vessels::{harbor_fleet, update_harbor_fleet};
use super::harbor_water::harbor_water;
use super::model_primitives::{
    Cache, DEFAULT_BOX_RADIUS, TEAM_COLORS, adopt_children, beam, box_part, cylinder_part, paint,
    put, rotated, shadow_receiver,
};
use crate::sim::arena::{BOUNDARY_THICKNESS, spawn_positions};
use crate::sim::data::ARENA;
use crate::sim::types::{Team, Vec2};

/// Index of the fleet among [`HarborScenery::root`]'s children; the beacons are
/// the last child.
const FLEET_CHILD: usize = 1;
/// The quay dressing merges per square of this edge, metres (the renderer's own
/// scenery cell): merged whole, its bollards, sheds and the outer quays' stacks made
/// one mesh round the harbor that every view and the water's reflection drew.
const DETAIL_CELL: f64 = 60.0;

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
/// apron, its row of the shared label sheet drawn by the browser (see
/// `effects_scenery::HARBOR_LABEL_TEXTURE`), so every label merges into one draw.
fn paint_label(group: &mut Node, text: &'static str, x: f64, z: f64, width: f64, depth: f64) {
    static MATERIAL: crate::models::shared_values::SharedOnce<Material> =
        crate::models::shared_values::SharedOnce::new("harbor_scenery::MATERIAL");
    let material = MATERIAL
        .get_or_init(|| {
            Arc::new(Material {
                map: Some(TextureRef::generated(HARBOR_LABEL_TEXTURE)),
                ..(*apron_material(ApronMode::Label, 0xffffff)).clone()
            })
        })
        .clone();
    // The sheet's row 0 is its top, uploaded at v = 1.
    let (row, rows) = (harbor_label_row(text) as f32, HARBOR_LABEL_ROWS as f32);
    let mut plane = plane_geometry(width, depth);
    for uv in &mut plane.uvs {
        uv[1] = (uv[1] + rows - 1.0 - row) / rows;
    }
    let label = Node::mesh(Arc::new(plane), material);
    put(group, rotated(label, -FRAC_PI_2, 0.0, 0.0), x, 0.035, z);
}

/// A spawn pad: a twelve-sided steel deck and the light ring set into it, both
/// in the team's paint as vertex colors with the pad's own coordinates as uv, so
/// the apron effect draws the tread plate, chevrons and chasing light, and every
/// pad of both teams is two draws. Heights and footprints are the decks
/// `effects::spawn_pad_decks` lays decals and mines on.
fn spawn_pad(details: &mut Node, team: Team, x: f64, z: f64) {
    static PADS: Cache<(u32, bool), Mesh> = Cache::new("harbor_scenery::PADS");
    let color = TEAM_COLORS[team.index()];
    // The ring's uv is offset by `PAD_LIGHT_FLAG`, which the effect reads as the
    // light strip, so deck and ring share one material.
    let team_mesh = |mut mesh: Mesh, flag: f32| {
        let [r, g, b] = hex_to_linear(color);
        mesh.colors = vec![[r as f32, g as f32, b as f32]; mesh.positions.len()];
        mesh.uvs = mesh
            .positions
            .iter()
            .map(|p| [p[0] / PAD_RADIUS as f32 + flag, p[2] / PAD_RADIUS as f32])
            .collect();
        mesh
    };
    let deck = PADS.get_or_insert((color, false), || {
        team_mesh(cylinder_geometry(PAD_RADIUS, PAD_RADIUS, 0.08, 12), 0.0)
    });
    let material = apron_material(ApronMode::Pad, PAD_STEEL);
    put(details, shadow_receiver(deck, material.clone()), x, 0.06, z);
    let ring = PADS.get_or_insert((color, true), || {
        let mut ring = ring_geometry(2.1, 2.3, 32);
        ring.rotate_x(-FRAC_PI_2);
        team_mesh(ring, PAD_LIGHT_FLAG)
    });
    put(details, shadow_receiver(ring, material), x, 0.11, z);
}
/// The uv.x offset that marks a spawn pad's light ring (`effects/harbor_apron.wgsl`).
const PAD_LIGHT_FLAG: f32 = 10.0;
/// Radius of a spawn pad's deck and its steel.
const PAD_RADIUS: f64 = 2.65;
const PAD_STEEL: u32 = 0x3a4850;

/// Render order of the paint blended over the apron (markings and lettering): before
/// every other blended draw, the tanks' turret bands (`tank_model`) and the tread
/// marks, smoke and sparks, so nothing over the paint is painted over.
const PAINT_RENDER_ORDER: i32 = -5;

/// Merge a group of ground-level parts, which receive shadows but cast none; the
/// blended paint among them draws first ([`PAINT_RENDER_ORDER`]).
fn batch_receivers(group: &mut Node) {
    batch(group);
    for child in &mut group.children {
        if let Some(drawable) = &mut child.drawable {
            drawable.cast_shadow = false;
            if drawable.material.transparent {
                drawable.render_order = PAINT_RENDER_ORDER;
            }
        }
    }
}

/// Apron concrete under the playfield, the asphalt of its traffic lanes and the
/// quay walls rising from the water.
const APRON_COLOR: u32 = 0xa5a7a1;
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

/// Floor of the transit sheds on the outer quays.
const WAREHOUSE_FLOOR: f64 = -0.5;
/// Rise of the warehouse roof's ridge over its eaves, metres.
const WAREHOUSE_RIDGE: f64 = 1.7;
const WAREHOUSE_CLADDING: u32 = 0x6a8f9c;
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
    // A low-pitched roof in two slopes over a fascia, a ridge cap with roof
    // ventilators along it and a run of skylights let into each slope, so the
    // shed has an eaves line and a ridge against the sky, not a flat lid.
    put(
        &mut shed,
        box_part(width + 0.6, 0.3, length + 0.6, WAREHOUSE_ROOF, 0.0),
        0.0,
        height + 0.15,
        0.0,
    );
    let half_span = width / 2.0 + 0.3;
    let pitch = WAREHOUSE_RIDGE.atan2(half_span);
    for side in [-1.0, 1.0] {
        let slope = rotated(
            wedge_part(half_span, WAREHOUSE_RIDGE, length + 0.6, WAREHOUSE_ROOF),
            0.0,
            if side > 0.0 { PI } else { 0.0 },
            0.0,
        );
        put(
            &mut shed,
            slope,
            side * half_span / 2.0,
            height + 0.3 + WAREHOUSE_RIDGE / 2.0,
            0.0,
        );
        let at = 4.6;
        put(
            &mut shed,
            rotated(
                box_part(2.4, 0.1, length - 8.0, SKYLIGHT, 0.0),
                0.0,
                0.0,
                -side * pitch,
            ),
            side * at,
            height + 0.35 + WAREHOUSE_RIDGE * (1.0 - at / half_span),
            0.0,
        );
    }
    put(
        &mut shed,
        box_part(1.0, 0.35, length - 2.0, WAREHOUSE_ROOF, 0.0),
        0.0,
        height + 0.4 + WAREHOUSE_RIDGE,
        0.0,
    );
    for k in 0..6 {
        let z = -length / 2.0 + 8.0 + f64::from(k) * (length - 16.0) / 5.0;
        put(
            &mut shed,
            cylinder_part(0.45, 0.9, 0x9a9e98, 10),
            0.0,
            height + 0.9 + WAREHOUSE_RIDGE,
            z,
        );
        put(
            &mut shed,
            cylinder_part(0.6, 0.12, 0x7f847f, 10),
            0.0,
            height + 1.4 + WAREHOUSE_RIDGE,
            z,
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

/// A group with its nested groups' parts moved up into it, transforms baked, so
/// [`batch`] sees every part as a direct child.
fn flattened(group: Node) -> Node {
    fn gather(parent: &mut Node, group: Node) {
        let mut moved = Node::group("");
        adopt_children(&mut moved, group);
        for child in moved.children {
            if child.drawable.is_none() {
                gather(parent, child);
            } else {
                parent.children.push(child);
            }
        }
    }
    let mut flat = Node::group("");
    flat.position = group.position;
    flat.rotation = group.rotation;
    flat.scale = group.scale;
    for child in group.children {
        if child.drawable.is_none() {
            gather(&mut flat, child);
        } else {
            flat.children.push(child);
        }
    }
    flat
}

/// Length of the outer quays the transit sheds and their yards stand on.
const OUTER_QUAY_LENGTH: f64 = 132.0;

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

/// Lettering painted down the lanes along x, at (centre x, lane centre z, width,
/// depth): the yard's name in the run west of the north crossing, clear of the
/// fighting round it, and the berth number across the south crossing. The centre
/// line's dashes and the arrows stop short of them.
const LANE_TITLES: [(&str, f64, f64, f64, f64); 2] = [
    ("HARBOR HAVOC", -27.25, -45.0, 24.0, 3.6),
    ("PORT 07", 0.0, 45.0, 15.0, 3.5),
];

/// Whether lane paint at `along` on the lane along x centred at `centre` would
/// run into its title.
fn under_title(along_z: bool, centre: f64, along: f64, reach: f64) -> bool {
    !along_z
        && LANE_TITLES
            .iter()
            .any(|&(_, x, z, width, _)| z == centre && (along - x).abs() < width / 2.0 + reach)
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
                    let at = dash + CENTRE_DASH / 2.0;
                    if !under_title(along_z, centre, at, CENTRE_DASH) {
                        put_on_lane(
                            marking(0.13, 0.018, CENTRE_DASH, LINE_WHITE),
                            centre,
                            at,
                            0.0,
                        );
                    }
                    dash += CENTRE_DASH + CENTRE_GAP;
                }
                if length < 12.0 {
                    continue;
                }
                // Keep-right traffic: arrows on each half of the lane, entering the run.
                for (half, toward, at) in [(1.0, 0.0, start + 4.0), (-1.0, PI, end - 4.0)] {
                    if under_title(along_z, centre, at, 2.5) {
                        continue;
                    }
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
/// Mooring bollards down the middle of the wall top, clear of its chamfers.
const BOLLARD_LINE: f64 = QUAY_EDGE - 0.6;
const BOLLARD_DARK: u32 = 0x262d30;
const BOLLARD_CAP: u32 = 0xe1b64d;
/// Flood-light masts at the quay corners.
const LAMP_HEIGHT: f64 = 9.0;
const LAMP_HOUSING: u32 = 0x4b585e;
const LAMP_LENS: u32 = 0xfff1c4;
/// Mooring lines: hemp color, thickness, sag at mid-span as a fraction of the
/// span, and the segments a line is drawn with.
const ROPE: u32 = 0x9c8862;
const ROPE_WIDTH: f64 = 0.13;
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

/// One-off paint that breaks the yard's mirror symmetry without touching play: a
/// zebra crossing on the central lane, a hatched keep-clear box in the north-west
/// bay and parking stalls for the reach stackers in the south-east one.
fn yard_paint(markings: &mut Node) {
    for bar in 0..14 {
        put(
            markings,
            marking(0.5, 0.02, 3.0, LINE_WHITE),
            -6.5 + f64::from(bar),
            0.03,
            29.0,
        );
    }
    let (cx, cz, half) = (-43.0, -26.0, 3.0);
    for (w, d, x, z) in [
        (half * 2.0, 0.15, cx, cz - half),
        (half * 2.0, 0.15, cx, cz + half),
        (0.15, half * 2.0, cx - half, cz),
        (0.15, half * 2.0, cx + half, cz),
    ] {
        put(markings, marking(w, 0.02, d, LINE_YELLOW), x, 0.03, z);
    }
    // Hatching at 45 degrees, each stroke clipped to the square's diagonal band.
    let mut offset = -half * 2.0 + 0.9;
    while offset < half * 2.0 - 0.5 {
        let length = (half * 2.0 - offset.abs()) * std::f64::consts::SQRT_2 - 0.3;
        let (x, z) = (cx + offset / 2.0, cz - offset / 2.0);
        put(
            markings,
            rotated(
                marking(0.14, 0.02, length, LINE_YELLOW),
                0.0,
                std::f64::consts::FRAC_PI_4,
                0.0,
            ),
            x,
            0.031,
            z,
        );
        offset += 1.1;
    }
    for stall in 0..6 {
        put(
            markings,
            marking(5.0, 0.02, 0.12, LINE_WHITE),
            41.5,
            0.03,
            20.0 + f64::from(stall) * 2.6,
        );
    }
    put(
        markings,
        marking(0.12, 0.02, 13.0, LINE_WHITE),
        44.0,
        0.03,
        26.5,
    );
}

/// Spans of the east and west quay faces (z from, z to) that a berth or a crane
/// plinth stands against, where no fender hangs: west first.
const SIDE_BERTHS: [[(f64, f64); 2]; 2] = [
    [(36.5, 49.5), (10.75, 21.25)],
    [(36.5, 49.5), (-25.25, -14.75)],
];
/// Fender rubber and steel, chain, and the galvanised quay ladders.
const FENDER_RUBBER: u32 = 0x22282b;
const FENDER_PANEL: u32 = 0x3b4448;
const CABLE_GREY: u32 = 0x59605f;
const LADDER: u32 = 0x9aa3a3;
/// Life-ring stations along the wall tops, between the bollards.
const LIFE_RING: u32 = 0xe8742c;
const POST_WHITE: u32 = 0xe9e6dc;

/// Something fixed to a quay face at (x, z), its local +z turned by `yaw` to
/// point out over the water.
fn on_quay_face(details: &mut Node, group: Node, x: f64, z: f64, yaw: f64) {
    let mut group = rotated(group, 0.0, yaw, 0.0);
    group.position = glam::DVec3::new(x, 0.0, z);
    adopt_children(details, group);
}

/// A cone fender on the quay face: the rubber cone, its steel frontal panel with
/// pads and yellow trim, and the chains that hang it from the wall top.
fn fender(details: &mut Node, x: f64, z: f64, yaw: f64) {
    static CONE: crate::models::shared_values::SharedOnce<crate::geometry::Mesh> =
        crate::models::shared_values::SharedOnce::new("harbor_scenery::CONE");
    let cone = CONE
        .get_or_init(|| Arc::new(crate::geometry::cylinder_geometry(0.42, 0.62, 0.8, 12)))
        .clone();
    let mut fender = Node::group("");
    let body = Node::mesh(cone, paint(FENDER_RUBBER));
    put(
        &mut fender,
        rotated(body, FRAC_PI_2, 0.0, 0.0),
        0.0,
        -0.7,
        0.4,
    );
    put(
        &mut fender,
        box_part(1.7, 1.9, 0.16, FENDER_PANEL, 0.0),
        0.0,
        -0.7,
        0.88,
    );
    for y in [0.28, -1.68] {
        put(
            &mut fender,
            box_part(1.8, 0.12, 0.2, BOLLARD_CAP, 0.0),
            0.0,
            y,
            0.9,
        );
    }
    for (pad_x, pad_y) in [(-0.45, -0.25), (0.45, -0.25), (-0.45, -1.15), (0.45, -1.15)] {
        put(
            &mut fender,
            box_part(0.7, 0.75, 0.05, FENDER_RUBBER, 0.0),
            pad_x,
            pad_y,
            0.98,
        );
    }
    for side in [-0.7, 0.7] {
        beam(
            &mut fender,
            [side, 0.3, 0.9],
            [side * 1.25, WALL_TOP - 0.05, 0.05],
            0.05,
            CABLE_GREY,
        );
    }
    on_quay_face(details, fender, x, z, yaw);
}

/// A galvanised ladder down the quay face into the water, with grab hoops above
/// the wall top.
fn ladder(details: &mut Node, x: f64, z: f64, yaw: f64) {
    let mut ladder = Node::group("");
    for side in [-0.25, 0.25] {
        beam(
            &mut ladder,
            [side, -2.8, 0.1],
            [side, WALL_TOP + 0.9, 0.1],
            0.06,
            LADDER,
        );
        beam(
            &mut ladder,
            [side, WALL_TOP + 0.9, 0.1],
            [side, WALL_TOP + 0.9, -0.5],
            0.05,
            LADDER,
        );
        beam(
            &mut ladder,
            [side, WALL_TOP + 0.9, -0.5],
            [side, WALL_TOP, -0.5],
            0.05,
            LADDER,
        );
    }
    let mut y = -2.5;
    while y < WALL_TOP {
        put(
            &mut ladder,
            box_part(0.5, 0.045, 0.045, LADDER, 0.0),
            0.0,
            y,
            0.1,
        );
        y += 0.3;
    }
    on_quay_face(details, ladder, x, z, yaw);
}

/// A mushroom bollard bolted to the wall top at (x, z).
fn bollard(details: &mut Node, x: f64, z: f64) {
    put(
        details,
        box_part(0.95, 0.08, 0.95, BOLLARD_DARK, 0.0),
        x,
        WALL_TOP + 0.04,
        z,
    );
    put(
        details,
        cylinder_part(0.27, 0.5, BOLLARD_DARK, 12),
        x,
        WALL_TOP + 0.3,
        z,
    );
    put(
        details,
        cylinder_part(0.42, 0.14, BOLLARD_CAP, 12),
        x,
        WALL_TOP + 0.6,
        z,
    );
}

/// Channel buoys marking the approaches round the port: x, z and whether it is a
/// port-hand (red) can or a starboard-hand (green) one. They float in the open water
/// past the berths and the tugs' loops, so a view across the wall has something
/// between the quay and the far shore.
const BUOYS: [(f64, f64, bool); 8] = [
    (-74.0, -101.0, true),
    (52.0, -106.0, false),
    (-41.0, 86.0, true),
    (12.0, 99.0, false),
    (58.0, 83.0, true),
    (-141.0, -18.0, false),
    (139.0, 28.0, true),
    (-96.0, 92.0, false),
];
const BUOY_RED: u32 = 0xc8473b;
const BUOY_GREEN: u32 = 0x2f8a6a;
/// Harbor water level (`harbor_water::HARBOR_WATER_HEIGHT`).
const SEA_LEVEL: f64 = -2.2;

/// A channel buoy floating at (x, z): a float with a white reflective band, the
/// lattice tower painted like it with the lantern on top.
fn buoy(details: &mut Node, x: f64, z: f64, red: bool) {
    let color = if red { BUOY_RED } else { BUOY_GREEN };
    put(
        details,
        cylinder_part(0.8, 1.1, color, 12),
        x,
        SEA_LEVEL + 0.25,
        z,
    );
    put(
        details,
        cylinder_part(0.82, 0.16, POST_WHITE, 12),
        x,
        SEA_LEVEL + 0.62,
        z,
    );
    put(
        details,
        cylinder_part(0.3, 1.5, color, 8),
        x,
        SEA_LEVEL + 1.55,
        z,
    );
    let mark = if red {
        box_part(0.5, 0.5, 0.5, color, 0.0)
    } else {
        cylinder_part(0.32, 0.5, color, 8)
    };
    put(details, mark, x, SEA_LEVEL + 2.55, z);
    put(
        details,
        box_part(0.18, 0.22, 0.18, LAMP_LENS, 0.0),
        x,
        SEA_LEVEL + 2.95,
        z,
    );
}

/// A life-ring station on the wall top: a white post with the ring hung facing
/// along `yaw`.
fn life_ring(details: &mut Node, x: f64, z: f64, yaw: f64) {
    put(
        details,
        box_part(0.14, 1.2, 0.14, POST_WHITE, 0.0),
        x,
        WALL_TOP + 0.6,
        z,
    );
    put(
        details,
        box_part(0.5, 0.08, 0.5, POST_WHITE, 0.0),
        x,
        WALL_TOP + 1.24,
        z,
    );
    let ring = Node::mesh(
        Arc::new(torus_geometry(0.32, 0.075, 6, 16)),
        paint(LIFE_RING),
    );
    put(details, rotated(ring, 0.0, yaw, 0.0), x, WALL_TOP + 0.85, z);
}

/// A flood-light mast standing at (x, floor, z) on its concrete plinth: a tapered
/// galvanised pole, a headframe platform and three floodlights aimed down at
/// `toward` (a unit vector on the ground toward the apron), and the beacon on top.
fn flood_mast(details: &mut Node, beacons: &mut Node, [x, floor, z]: [f64; 3], toward: [f64; 2]) {
    static POLE: crate::models::shared_values::SharedOnce<Mesh> =
        crate::models::shared_values::SharedOnce::new("harbor_scenery::POLE");
    let pole = POLE
        .get_or_init(|| Arc::new(cylinder_geometry(0.12, 0.24, LAMP_HEIGHT, 8)))
        .clone();
    let mut mast = Node::group("");
    put(
        &mut mast,
        box_part(1.0, 0.4, 1.0, 0x8d8c84, 0.0),
        0.0,
        0.2,
        0.0,
    );
    put(
        &mut mast,
        Node::mesh(pole, paint(MAST_GALVANISED)),
        0.0,
        LAMP_HEIGHT / 2.0,
        0.0,
    );
    // Headframe: the service platform's grating and the lamp bar under it.
    put(
        &mut mast,
        box_part(3.0, 0.12, 1.1, GRATING_STEEL, 0.0),
        0.0,
        LAMP_HEIGHT - 0.35,
        0.0,
    );
    put(
        &mut mast,
        box_part(3.0, 0.16, 0.16, LAMP_HOUSING, 0.0),
        0.0,
        LAMP_HEIGHT - 0.55,
        0.5,
    );
    for dx in [-1.05, 0.0, 1.05] {
        let mut lamp = Node::group("");
        put(
            &mut lamp,
            box_part(0.8, 0.62, 0.32, LAMP_HOUSING, 0.0),
            0.0,
            0.0,
            0.0,
        );
        put(
            &mut lamp,
            box_part(0.66, 0.48, 0.04, LAMP_LENS, 0.0),
            0.0,
            0.0,
            0.17,
        );
        put(
            &mut mast,
            rotated(lamp, 0.6, 0.0, 0.0),
            dx,
            LAMP_HEIGHT - 0.85,
            0.72,
        );
    }
    // The lamps face local +z; turn them toward the apron.
    let mut mast = rotated(mast, 0.0, toward[0].atan2(toward[1]), 0.0);
    mast.position = glam::DVec3::new(x, floor, z);
    adopt_children(details, mast);
    put(
        beacons,
        cylinder_part(0.18, 0.3, 0xffb54a, 8),
        x,
        floor + LAMP_HEIGHT - 0.1,
        z,
    );
}
/// Galvanised steel of the masts, and the platforms' grating.
const MAST_GALVANISED: u32 = 0x8e979a;
const GRATING_STEEL: u32 = 0x4d575b;

/// A yard block of boxes along z on an outer quay: `columns` of them end to end
/// from (x, z), `rows` side by side toward +x, each stack up to `tiers` high as the
/// `seed`ed hash allows, painted from [`YARD_PAINT`].
fn container_block(
    details: &mut Node,
    origin: [f64; 2],
    columns: u32,
    rows: u32,
    tiers: u32,
    seed: u32,
) {
    let [x, z] = origin;
    for column in 0..columns {
        for row in 0..rows {
            let key = column * 7 + row * 13 + seed * 31;
            let height = 1 + (key.wrapping_mul(2_654_435_761) >> 29) % tiers;
            for tier in 0..height {
                let color = YARD_PAINT[((key + tier * 5).wrapping_mul(2_246_822_519) >> 28)
                    as usize
                    % YARD_PAINT.len()];
                let mut cargo = Node::group("");
                ship_container(
                    &mut cargo,
                    CargoShape {
                        w: 2.45,
                        d: 6.1,
                        h: 2.6,
                        color,
                    },
                );
                cargo.position = glam::DVec3::new(
                    x + f64::from(row) * 2.7,
                    WAREHOUSE_FLOOR + f64::from(tier) * 2.62,
                    z + f64::from(column) * 6.4,
                );
                adopt_children(details, cargo);
            }
        }
    }
}
/// A rubber-tyred gantry straddling a yard block from (x, z) (its `rows` of boxes
/// toward +x, `columns` toward +z, as [`container_block`] lays them): four legs on
/// bogies, sill beams, the girders across the top, the trolley and the driver's
/// cab hanging under it, clear of the block's highest tier.
fn yard_gantry(details: &mut Node, origin: [f64; 2], columns: u32, rows: u32, tiers: u32) {
    let [x, z] = origin;
    let (left, right) = (x - 1.35 - 1.3, x + f64::from(rows) * 2.7 - 1.35 + 1.3);
    let centre_z = z + (f64::from(columns) - 1.0) * 3.2;
    let top = WAREHOUSE_FLOOR + f64::from(tiers) * 2.62 + 3.4;
    let span = right - left;
    let mid = (left + right) / 2.0;
    for leg_x in [left, right] {
        for dz in [-3.4, 3.4] {
            put(
                details,
                box_part(0.6, top - WAREHOUSE_FLOOR - 0.9, 0.6, RTG_PAINT, 0.0),
                leg_x,
                (top + WAREHOUSE_FLOOR + 0.9) / 2.0,
                centre_z + dz,
            );
            put(
                details,
                box_part(0.9, 1.0, 1.3, BOLLARD_DARK, 0.0),
                leg_x,
                WAREHOUSE_FLOOR + 0.5,
                centre_z + dz,
            );
        }
        put(
            details,
            box_part(0.8, 0.7, 7.6, RTG_PAINT, 0.0),
            leg_x,
            WAREHOUSE_FLOOR + 1.25,
            centre_z,
        );
        // The engine house on one sill.
        if leg_x == left {
            put(
                details,
                box_part(1.6, 1.8, 2.6, RTG_TRIM, 0.0),
                leg_x - 0.9,
                WAREHOUSE_FLOOR + 2.4,
                centre_z,
            );
        }
    }
    for dz in [-3.4, 3.4] {
        put(
            details,
            box_part(span + 0.6, 0.9, 0.7, RTG_PAINT, 0.0),
            mid,
            top,
            centre_z + dz,
        );
    }
    put(
        details,
        box_part(2.6, 1.1, 7.4, RTG_TRIM, 0.0),
        mid - span * 0.15,
        top + 0.9,
        centre_z,
    );
    put(
        details,
        box_part(1.5, 1.6, 1.6, RTG_TRIM, 0.0),
        right - 0.6,
        top - 1.4,
        centre_z - 2.4,
    );
    put(
        details,
        box_part(1.52, 0.7, 1.2, 0x335b6c, 0.0),
        right - 0.6,
        top - 1.2,
        centre_z - 2.3,
    );
}
/// The yard gantries' paint and the trim of their trolley and cab.
const RTG_PAINT: u32 = 0xdcae3c;
const RTG_TRIM: u32 = 0xe6e2d6;

const YARD_PAINT: [u32; 7] = [
    0xc96f3b, 0x3d8a8c, 0x5f7fa6, 0xb7a26e, 0x9b4f45, 0xe0dccd, 0x4f6b3f,
];

/// A reefer: a white box with the refrigeration unit's grille and fan at one end.
fn reefer(details: &mut Node, x: f64, y: f64, z: f64) {
    let mut cargo = Node::group("");
    ship_container(
        &mut cargo,
        CargoShape {
            w: 2.45,
            d: 6.1,
            h: 2.6,
            color: 0xd8dbd6,
        },
    );
    put(
        &mut cargo,
        box_part(2.1, 2.1, 0.12, 0x5b6468, 0.0),
        0.0,
        1.3,
        -3.08,
    );
    put(
        &mut cargo,
        rotated(cylinder_part(0.5, 0.06, 0x2c3336, 12), FRAC_PI_2, 0.0, 0.0),
        0.0,
        1.75,
        -3.15,
    );
    put(
        &mut cargo,
        box_part(0.5, 0.35, 0.08, 0x2f8f6a, 0.0),
        0.6,
        0.6,
        -3.16,
    );
    // The reefer line's band along both sides under the roof rail.
    for side in [-1.0, 1.0] {
        put(
            &mut cargo,
            box_part(0.04, 0.4, 5.6, 0x2f6f9a, 0.0),
            side * 1.25,
            2.1,
            0.0,
        );
    }
    cargo.position = glam::DVec3::new(x, y, z);
    adopt_children(details, cargo);
}

/// A counterbalance forklift at (x, z), its forks toward local -z turned by `yaw`,
/// carrying a loaded pallet.
fn forklift(details: &mut Node, x: f64, z: f64, yaw: f64) {
    let mut truck = Node::group("");
    put(
        &mut truck,
        box_part(1.7, 0.75, 2.6, 0xdca53b, DEFAULT_BOX_RADIUS),
        0.0,
        0.65,
        0.0,
    );
    for dx in [-0.9, 0.9] {
        for dz in [-0.8, 0.9] {
            let wheel = rotated(cylinder_part(0.43, 0.3, 0x2c383c, 12), 0.0, 0.0, FRAC_PI_2);
            put(&mut truck, wheel, dx, 0.45, dz);
        }
        put(
            &mut truck,
            box_part(0.12, 1.7, 0.12, 0x344952, DEFAULT_BOX_RADIUS),
            dx * 0.75,
            1.6,
            0.0,
        );
        put(
            &mut truck,
            box_part(0.12, 2.4, 0.14, 0x344952, DEFAULT_BOX_RADIUS),
            dx * 0.55,
            1.3,
            -1.3,
        );
        put(
            &mut truck,
            box_part(0.14, 0.12, 1.8, 0x647174, DEFAULT_BOX_RADIUS),
            dx * 0.55,
            0.45,
            -2.1,
        );
    }
    put(
        &mut truck,
        box_part(1.7, 0.16, 1.7, 0xe5b441, DEFAULT_BOX_RADIUS),
        0.0,
        2.5,
        0.0,
    );
    put(
        &mut truck,
        box_part(0.7, 0.7, 0.6, 0x344952, DEFAULT_BOX_RADIUS),
        0.0,
        1.35,
        0.25,
    );
    put(
        &mut truck,
        box_part(1.5, 0.9, 0.5, 0x3a4246, 0.0),
        0.0,
        0.95,
        1.35,
    );
    put(
        &mut truck,
        box_part(1.2, 0.14, 1.2, 0x96764e, 0.0),
        0.0,
        0.6,
        -2.1,
    );
    put(
        &mut truck,
        box_part(1.1, 0.8, 1.1, 0x7c8f5c, 0.0),
        0.0,
        1.07,
        -2.1,
    );
    let mut truck = rotated(truck, 0.0, yaw, 0.0);
    truck.position = glam::DVec3::new(x, 0.0, z);
    adopt_children(details, truck);
}

/// A stack of `levels` timber pallets at (x, z).
fn pallet_stack(details: &mut Node, x: f64, z: f64, levels: u32) {
    for level in 0..levels {
        let level = f64::from(level);
        for plank in 0..5 {
            put(
                details,
                box_part(1.2, 0.04, 0.2, 0xb89764, 0.0),
                x,
                0.14 + level * 0.16,
                z - 0.5 + f64::from(plank) * 0.25,
            );
        }
        for dx in [-0.5, 0.0, 0.5] {
            put(
                details,
                box_part(0.12, 0.1, 1.2, 0x96764e, 0.0),
                x + dx,
                0.07 + level * 0.16,
                z,
            );
        }
    }
}

/// Bollards, fenders, life rings, masts and berth clutter along the quays, laid
/// out differently on each side so the port does not read as a mirror image.
fn quay_dressing(details: &mut Node, beacons: &mut Node) {
    for (index, side) in [-1.0, 1.0].into_iter().enumerate() {
        // East and west quay faces: bollards along the wall top and fenders below,
        // except where a berth or a crane plinth stands against the face.
        for z in [-48.0, -24.0, 0.0, 24.0, 48.0] {
            bollard(details, side * BOLLARD_LINE, z);
            if SIDE_BERTHS[index]
                .iter()
                .any(|&(a, b)| z > a - 1.0 && z < b + 1.0)
            {
                continue;
            }
            fender(details, side * QUAY_EDGE, z, side * FRAC_PI_2);
        }
        ladder(details, side * QUAY_EDGE, -36.0, side * FRAC_PI_2);
        ladder(details, side * QUAY_EDGE, 30.0, side * FRAC_PI_2);
        ladder(details, side * 12.0, -QUAY_EDGE, PI);
        ladder(details, side * 30.0, QUAY_EDGE, 0.0);
        for (along, ring) in [(-36.0, true), (12.0, true), (36.0, false)] {
            if ring {
                life_ring(details, along * side, side * (QUAY_EDGE - 1.0), 0.0);
                life_ring(details, side * (QUAY_EDGE - 1.0), along * -side, FRAC_PI_2);
            }
        }
        flood_mast(
            details,
            beacons,
            [side * 62.0, 0.0, -side * 30.0],
            [-side, 0.0],
        );
    }
    // The outer quays' yards: container blocks either side of the transit sheds,
    // stacked and painted differently on each.
    container_block(details, [-125.0, 44.0], 2, 4, 3, 1);
    yard_gantry(details, [-125.0, 44.0], 2, 4, 3);
    for z in [45.0, 38.6] {
        reefer(details, -110.0, WAREHOUSE_FLOOR, z);
    }
    container_block(details, [-125.0, -60.0], 2, 3, 2, 2);
    container_block(details, [109.0, 44.0], 2, 3, 2, 3);
    container_block(details, [109.0, -58.0], 2, 4, 3, 4);
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
        let mut labels = Node::group("");
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
        yard_paint(&mut markings);
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
                paint_label(&mut labels, bay, side * 37.0, z, 2.5, 1.4);
            }
            // The hazard paint of the wall tops and kerbs is the parapet's own
            // (`ApronMode::Parapet`), worn panel by panel.
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
                fender(
                    &mut details,
                    x,
                    side * QUAY_EDGE,
                    if side < 0.0 { PI } else { 0.0 },
                );
            }
            // Flood-light masts at the quay corners, aimed in at the apron.
            for x in [-54.0, 54.0] {
                flood_mast(
                    &mut details,
                    &mut beacons,
                    [x, 0.0, side * 62.0],
                    [0.0, -side],
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
        for (text, x, z, width, depth) in LANE_TITLES {
            paint_label(&mut labels, text, x, z, width, depth);
        }
        paint_label(&mut labels, "LOADING", 0.0, 3.5, 8.0, 1.1);
        batch_receivers(&mut labels);
        root.children.append(&mut labels.children);
        for team in [Team::Blue, Team::Red] {
            for Vec2 { x, z } in spawn_positions(team, 1.0) {
                spawn_pad(&mut markings, team, x, z);
            }
        }
        // Working berths sit beyond the wall, each kept differently: on the west a
        // forklift by a reefer stack and spent tyres, on the east a site office, a
        // light tower, drums and a cable reel. Flush drainage grates line the apron
        // and mooring lines run out to the ships.
        for side in [-1.0, 1.0] {
            put(
                &mut ground,
                apron_box(7.0, 3.4, 13.0, QUAY_WALL_COLOR, ApronMode::QuayWall),
                side * 65.0,
                -1.65,
                43.0,
            );
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
                [side * BOLLARD_LINE, WALL_TOP + 0.5, -24.0],
                [
                    if side < 0.0 { -71.3 } else { 72.3 },
                    2.3,
                    if side < 0.0 { -27.0 } else { -4.0 },
                ],
            );
            mooring_line(
                &mut details,
                [side * BOLLARD_LINE, WALL_TOP + 0.5, 24.0],
                [
                    if side < 0.0 { -71.3 } else { 72.3 },
                    2.3,
                    if side < 0.0 { 12.0 } else { 34.0 },
                ],
            );
            mooring_line(
                &mut details,
                [side * BOLLARD_LINE, WALL_TOP + 0.5, 0.0],
                [
                    if side < 0.0 { -71.3 } else { 72.3 },
                    2.3,
                    if side < 0.0 { 8.0 } else { 6.0 },
                ],
            );
            mooring_line(
                &mut details,
                [side * 24.0, WALL_TOP + 0.5, -BOLLARD_LINE],
                [side * 20.0 - 8.0, 3.0, -73.0],
            );
            mooring_line(
                &mut details,
                [side * 48.0, WALL_TOP + 0.5, -BOLLARD_LINE],
                [if side < 0.0 { -38.0 } else { 22.0 }, 3.0, -73.2],
            );
        }
        forklift(&mut details, -64.2, 46.2, PI);
        for (x, y, z) in [
            (-66.9, 0.05, 39.6),
            (-64.3, 0.05, 39.6),
            (-66.9, 2.67, 39.6),
        ] {
            reefer(&mut details, x, y, z);
        }
        put(
            &mut details,
            box_part(0.5, 1.4, 0.4, 0xd9b44a, 0.0),
            -62.5,
            0.75,
            43.3,
        );
        put(
            &mut details,
            box_part(0.42, 0.3, 0.06, 0x2f3b40, 0.0),
            -62.5,
            1.1,
            43.08,
        );
        for level in 0..3 {
            let tyre = Node::mesh(Arc::new(torus_geometry(0.42, 0.17, 8, 14)), paint(0x23292b));
            put(
                &mut details,
                rotated(tyre, FRAC_PI_2, 0.0, 0.0),
                -67.4,
                0.17 + f64::from(level) * 0.32,
                48.4,
            );
        }
        pallet_stack(&mut details, 66.6, 46.3, 3);
        pallet_stack(&mut details, 66.6, 48.3, 2);
        let office = 0xd8d0b8;
        put(
            &mut details,
            box_part(2.6, 2.7, 5.6, office, 0.0),
            66.8,
            1.4,
            40.0,
        );
        put(
            &mut details,
            box_part(2.8, 0.15, 5.8, 0x8d9396, 0.0),
            66.8,
            2.83,
            40.0,
        );
        put(
            &mut details,
            box_part(0.06, 0.9, 1.6, 0x335b6c, 0.0),
            65.48,
            1.75,
            38.6,
        );
        put(
            &mut details,
            box_part(0.06, 1.9, 0.9, 0x6d7778, 0.0),
            65.48,
            1.1,
            41.2,
        );
        for step in 0..3 {
            let step = f64::from(step);
            put(
                &mut details,
                box_part(0.8, 0.12, 1.0, 0x6d7778, 0.0),
                64.6 + step * 0.25,
                0.12 + step * 0.22,
                41.2,
            );
        }
        put(
            &mut details,
            box_part(1.4, 0.9, 2.2, 0xd9b44a, 0.0),
            63.4,
            0.5,
            47.6,
        );
        put(
            &mut details,
            box_part(0.2, 6.5, 0.2, 0x45575c, 0.0),
            63.4,
            4.2,
            47.6,
        );
        put(
            &mut details,
            box_part(1.6, 0.9, 0.3, LAMP_HOUSING, 0.0),
            63.4,
            7.6,
            47.2,
        );
        put(
            &mut details,
            box_part(1.4, 0.7, 0.05, LAMP_LENS, 0.0),
            63.4,
            7.6,
            47.03,
        );
        for (dx, dz, color) in [
            (0.0, 0.0, 0xc8473b),
            (0.7, 0.25, 0x2f6f8a),
            (0.25, 0.8, 0x6b7a3a),
        ] {
            put(
                &mut details,
                cylinder_part(0.3, 0.9, color, 12),
                67.6 + dx,
                0.5,
                44.0 + dz,
            );
        }
        for dz in [-0.45, 0.45] {
            let disc = rotated(cylinder_part(0.85, 0.08, 0x9b7a4e, 16), FRAC_PI_2, 0.0, 0.0);
            put(&mut details, disc, 64.0, 0.85, 37.9 + dz);
        }
        let core = rotated(cylinder_part(0.5, 0.82, 0x2b3236, 12), FRAC_PI_2, 0.0, 0.0);
        put(&mut details, core, 64.0, 0.85, 37.9);
        // Distant warehouses close the horizon without creating obstacles on the board.
        for x in [-115.0, 115.0] {
            put(
                &mut ground,
                apron_box(
                    22.0,
                    3.0,
                    OUTER_QUAY_LENGTH,
                    QUAY_WALL_COLOR,
                    ApronMode::QuayWall,
                ),
                x,
                -2.0,
                0.0,
            );
            // Flattened and batched so its cladding bakes into vertex colors and
            // shares the container paint's one material with the rest of its cell.
            let mut shed = flattened(warehouse());
            batch(&mut shed);
            let shed = rotated(shed, 0.0, if x < 0.0 { 0.0 } else { PI }, 0.0);
            put(&mut details, shed, x, WAREHOUSE_FLOOR, 0.0);
        }
        quay_dressing(&mut details, &mut beacons);
        for (x, z, red) in BUOYS {
            buoy(&mut details, x, z, red);
        }
        root.children.push(harbor_backdrop());
        batch_in_cells(&mut details, DETAIL_CELL);
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
