//! The watchtower and what its collapse leaves behind.
//!
//! Intact: four sawn posts on steel base plates stand on the two concrete
//! footings ([`TOWER_BASE`]); X-braces on opposite faces of each bent and a girt
//! between them, knee braces out to outrigger beams, main beams and joists under
//! a plank deck with rim boards; a railed walkway round a clapboard lookout cabin
//! whose windows carry top-hinged awning shutters propped open; a hipped
//! standing-seam roof with an anemometer mast; a steel ladder up the front and a
//! searchlight on the corner post.
//!
//! The deck top stays at the height the collapse pieces start from
//! (`scenery_pieces`), and the posts keep the section and positions the rubble's
//! cut posts show.

use std::f64::consts::PI;

use glam::{DMat3, DMat4, DQuat, DVec2, DVec3};

use super::building_kit::{
    Glazing, Opening, Palette, Surface, add_building, door, uv_per_metre, wall, window,
};
use super::concrete_surfaces::concrete_wall;
use super::model_primitives::{put, rotated};
use super::tank_kit::{Kit, aim, pose};
use super::timber_model::timber_member;
use crate::geometry::math::js_round;
use crate::scene::Node;
use crate::sim::math::Random;
use crate::sim::tower_layout::TOWER_BASE;

const TOWER_POST: u32 = 0x887454;
const TOWER_BRACE: u32 = 0x96734c;
const TOWER_DECK: u32 = 0x887d59;
const TOWER_ROOF: u32 = 0x2b6a4c;
const TOWER_TRIM: u32 = 0xe6dcc2;
const TOWER_SHUTTERS: u32 = 0x7a3a2c;

/// Post section and the top of the posts under the main beams.
const POST: f64 = 0.35;
const POST_TOP: f64 = 4.645;
/// Main beams, joists and the plank deck whose top the collapse starts from.
const BEAM: f64 = 0.3;
const JOIST_DEPTH: f64 = 0.18;
const PLANK: f64 = 0.05;
const DECK_TOP: f64 = POST_TOP + BEAM + JOIST_DEPTH + PLANK;
/// Half extents of the deck.
const DECK_X: f64 = 3.0;
const DECK_Z: f64 = 2.5;
/// Lookout cabin: half extents, wall top (the flat soffit) and window band.
const CABIN_X: f64 = 2.15;
const CABIN_Z: f64 = 1.75;
const CABIN_TOP: f64 = 7.3;
const WINDOW_SILL: f64 = 6.05;
const WINDOW_HEIGHT: f64 = 0.85;
const WINDOW_WIDTH: f64 = 0.95;
const REVEAL: f64 = 0.1;
/// How far awning shutters swing out from the wall (radians from hanging).
const AWNING_OPEN: f64 = 1.1;
/// Hip roof: overhang past the cabin walls, fascia depth and pitch.
const ROOF_OVERHANG: f64 = 0.5;
const ROOF_EDGE: f64 = 0.15;
const ROOF_PITCH: f64 = 0.55;
const SEAM_SPACING: f64 = 0.45;
/// Walkway railing: height above the deck and the inset from the deck edge.
const RAIL_HEIGHT: f64 = 1.05;
const RAIL_INSET: f64 = 0.06;
/// The ladder up the front, outside the deck edge, and the railing gap above it.
const LADDER_X: f64 = 1.75;
const LADDER_HALF: f64 = 0.275;
const LADDER_Z: f64 = DECK_Z + 0.135;
const GAP: (f64, f64) = (1.3, 2.2);

fn palette(color: u32) -> Palette {
    Palette {
        siding: color,
        trim: TOWER_TRIM,
        accent: TOWER_SHUTTERS,
        roof: TOWER_ROOF,
    }
}

/// A concrete footing at `x`, the shared base of the intact tower and its rubble.
pub(super) fn tower_foundation(group: &mut Node, x: f64) {
    put(
        group,
        concrete_wall(TOWER_BASE.width, TOWER_BASE.height, TOWER_BASE.depth),
        x,
        TOWER_BASE.height / 2.0,
        0.0,
    );
}

/// A post standing on a footing from its base plate to `height` above it.
fn post(height: f64, seed: i32) -> Node {
    timber_member(DVec3::new(POST, height, POST), seed, TOWER_POST)
}

/// A timber member from `from` to `to`: `width` in the plane it braces, `depth`
/// along `normal` (the plane's normal).
fn member(
    from: DVec3,
    to: DVec3,
    width: f64,
    depth: f64,
    normal: DVec3,
    seed: i32,
    color: u32,
) -> Node {
    let along = to - from;
    let x = along.normalize();
    let z = (normal - x * normal.dot(x)).normalize();
    let y = z.cross(x);
    let mut node = timber_member(DVec3::new(along.length(), width, depth), seed, color);
    node.rotation = DQuat::from_mat3(&DMat3::from_cols(x, y, z));
    node.position = (from + to) / 2.0;
    node
}

/// The intact watchtower, its cabin painted `color`.
pub(super) fn tower(group: &mut Node, color: u32) {
    for side in [-1.0, 1.0] {
        tower_foundation(group, side * TOWER_BASE.offset);
    }
    frame(group);
    deck(group);
    railing(group);
    let mut kit = Kit::new();
    hardware(&mut kit);
    cabin(&mut kit);
    hip_roof(&mut kit);
    ladder(&mut kit);
    searchlight(&mut kit);
    add_building(group, kit.finish_scaled(uv_per_metre), &palette(color));
}

/// Posts, X-braces, girts, knee braces, main and outrigger beams.
fn frame(group: &mut Node) {
    let mut seed = 701;
    let mut next = || {
        seed += 1;
        seed
    };
    let base = TOWER_BASE.height + 0.03;
    for side in [-1.0, 1.0] {
        let x = side * TOWER_BASE.offset;
        for z in [-TOWER_BASE.post_z, TOWER_BASE.post_z] {
            put(
                group,
                post(POST_TOP - base, next()),
                x,
                (POST_TOP + base) / 2.0,
                z,
            );
        }
        // X-braces on the outer and inner faces of this bent, a girt between.
        for (face, rise) in [(1.0, 1.0), (-1.0, -1.0)] {
            let bx = x + side * face * (POST / 2.0 + 0.04);
            let (low, high) = (1.15, POST_TOP - 0.2);
            let z = TOWER_BASE.post_z * rise;
            group.children.push(member(
                DVec3::new(bx, low, -z),
                DVec3::new(bx, high, z),
                0.18,
                0.08,
                DVec3::X,
                next(),
                TOWER_BRACE,
            ));
        }
        let span = 2.0 * TOWER_BASE.post_z - POST;
        group.children.push(member(
            DVec3::new(x, 2.8, -span / 2.0),
            DVec3::new(x, 2.8, span / 2.0),
            0.2,
            0.15,
            DVec3::X,
            next(),
            TOWER_BRACE,
        ));
    }
    for edge in [-1.0, 1.0] {
        let z = edge * TOWER_BASE.post_z;
        for (face, rise) in [(1.0, 1.0), (-1.0, -1.0)] {
            let bz = z + edge * face * (POST / 2.0 + 0.04);
            let x = TOWER_BASE.offset * rise;
            group.children.push(member(
                DVec3::new(-x, 1.2, bz),
                DVec3::new(x, POST_TOP - 0.25, bz),
                0.2,
                0.08,
                DVec3::Z,
                next(),
                TOWER_BRACE,
            ));
        }
        let span = 2.0 * TOWER_BASE.offset - POST;
        group.children.push(member(
            DVec3::new(-span / 2.0, 2.8, z),
            DVec3::new(span / 2.0, 2.8, z),
            0.2,
            0.15,
            DVec3::Z,
            next(),
            TOWER_BRACE,
        ));
        // Main beam over the posts, an outrigger beam under the joists' overhang
        // and knee braces from each post out to it.
        let beam_y = POST_TOP + BEAM / 2.0;
        group.children.push(member(
            DVec3::new(-DECK_X + 0.05, beam_y, z),
            DVec3::new(DECK_X - 0.05, beam_y, z),
            BEAM,
            BEAM,
            DVec3::Z,
            next(),
            TOWER_POST,
        ));
        let outrigger_z = edge * (DECK_Z - 0.45);
        let outrigger_y = POST_TOP + BEAM - 0.09;
        group.children.push(member(
            DVec3::new(-DECK_X + 0.2, outrigger_y, outrigger_z),
            DVec3::new(DECK_X - 0.2, outrigger_y, outrigger_z),
            0.18,
            0.18,
            DVec3::Z,
            next(),
            TOWER_POST,
        ));
        for side in [-1.0, 1.0] {
            let x = side * TOWER_BASE.offset;
            group.children.push(member(
                DVec3::new(x, 3.55, z + edge * (POST / 2.0 - 0.02)),
                DVec3::new(x, outrigger_y - 0.06, outrigger_z - edge * 0.08),
                0.15,
                0.15,
                DVec3::X,
                next(),
                TOWER_BRACE,
            ));
        }
    }
}

/// Joists across the beams, rim boards round the edge and the plank floor.
fn deck(group: &mut Node) {
    let joists = 11;
    let joist_y = POST_TOP + BEAM + JOIST_DEPTH / 2.0;
    for i in 0..joists {
        let x = -DECK_X + 0.1 + (2.0 * DECK_X - 0.2) * f64::from(i) / f64::from(joists - 1);
        group.children.push(member(
            DVec3::new(x, joist_y, -DECK_Z + 0.04),
            DVec3::new(x, joist_y, DECK_Z - 0.04),
            JOIST_DEPTH,
            0.07,
            DVec3::X,
            800 + i,
            TOWER_POST,
        ));
    }
    let rim_y = DECK_TOP - 0.13;
    for edge in [-1.0, 1.0] {
        group.children.push(member(
            DVec3::new(-DECK_X, rim_y, edge * (DECK_Z - 0.02)),
            DVec3::new(DECK_X, rim_y, edge * (DECK_Z - 0.02)),
            0.26,
            0.05,
            DVec3::Z,
            830 + js_round(edge) as i32,
            TOWER_DECK,
        ));
        group.children.push(member(
            DVec3::new(edge * (DECK_X - 0.02), rim_y, -DECK_Z + 0.05),
            DVec3::new(edge * (DECK_X - 0.02), rim_y, DECK_Z - 0.05),
            0.26,
            0.05,
            DVec3::X,
            840 + js_round(edge) as i32,
            TOWER_DECK,
        ));
    }
    let planks = 33;
    let pitch = 2.0 * DECK_Z / f64::from(planks);
    for i in 0..planks {
        let z = -DECK_Z + pitch * (f64::from(i) + 0.5);
        put(
            group,
            timber_member(
                DVec3::new(2.0 * DECK_X - 0.02, PLANK, pitch - 0.012),
                900 + i,
                TOWER_DECK,
            ),
            0.0,
            DECK_TOP - PLANK / 2.0,
            z,
        );
    }
}

/// Posts, top and mid rails and toe boards round the walkway, open above the
/// ladder.
fn railing(group: &mut Node) {
    let (rx, rz) = (DECK_X - RAIL_INSET, DECK_Z - RAIL_INSET);
    let mut seed = 1000;
    let mut next = || {
        seed += 1;
        seed
    };
    let mut posts: Vec<(f64, f64)> = Vec::new();
    // (from, to) runs along each edge; the front edge stops at the ladder gap.
    let mut runs: Vec<(DVec2, DVec2)> = Vec::new();
    for edge in [-1.0, 1.0] {
        for i in 0..=6 {
            let x = -rx + 2.0 * rx * f64::from(i) / 6.0;
            if edge > 0.0 && x > GAP.0 - 0.1 && x < GAP.1 + 0.1 {
                continue;
            }
            posts.push((x, edge * rz));
        }
        for i in 1..5 {
            posts.push((edge * rx, -rz + 2.0 * rz * f64::from(i) / 5.0));
        }
        runs.push((DVec2::new(-edge * rx, -rz), DVec2::new(-edge * rx, rz)));
    }
    posts.push((GAP.0, rz));
    posts.push((GAP.1, rz));
    runs.push((DVec2::new(-rx, -rz), DVec2::new(rx, -rz)));
    runs.push((DVec2::new(-rx, rz), DVec2::new(GAP.0, rz)));
    runs.push((DVec2::new(GAP.1, rz), DVec2::new(rx, rz)));
    for (x, z) in posts {
        put(
            group,
            timber_member(DVec3::new(0.09, RAIL_HEIGHT, 0.09), next(), TOWER_BRACE),
            x,
            DECK_TOP + RAIL_HEIGHT / 2.0,
            z,
        );
    }
    for (from, to) in runs {
        let normal = if (to.x - from.x).abs() > (to.y - from.y).abs() {
            DVec3::Z
        } else {
            DVec3::X
        };
        for (y, width, depth) in [
            (RAIL_HEIGHT + 0.03, 0.06, 0.12),
            (RAIL_HEIGHT * 0.52, 0.09, 0.05),
            (0.07, 0.12, 0.03),
        ] {
            group.children.push(member(
                DVec3::new(from.x, DECK_TOP + y, from.y),
                DVec3::new(to.x, DECK_TOP + y, to.y),
                width,
                depth,
                normal,
                next(),
                TOWER_BRACE,
            ));
        }
    }
}

/// Base plates with anchor nuts under each post and bolt heads where the braces
/// cross and land.
fn hardware(kit: &mut Kit<Surface>) {
    for side in [-1.0, 1.0] {
        let x = side * TOWER_BASE.offset;
        for z in [-TOWER_BASE.post_z, TOWER_BASE.post_z] {
            kit.block(
                Surface::Iron,
                DVec3::new(0.52, 0.03, 0.52),
                DMat4::from_translation(DVec3::new(x, TOWER_BASE.height + 0.015, z)),
            );
            for (dx, dz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                kit.post(
                    Surface::Iron,
                    0.025,
                    0.05,
                    DVec3::new(x + dx * 0.21, TOWER_BASE.height + 0.03, z + dz * 0.21),
                    6,
                );
            }
        }
        // Bolts through each bent's brace crossing, both faces.
        for face in [-1.0, 1.0] {
            let bx = x + side * face * (POST / 2.0 + 0.125);
            kit.block(
                Surface::Iron,
                DVec3::new(0.02, 0.06, 0.06),
                DMat4::from_translation(DVec3::new(bx, (1.15 + POST_TOP - 0.2) / 2.0, 0.0)),
            );
        }
    }
}

/// The lookout cabin: clapboard walls on a trimmed sill, glazed all round under
/// propped awning shutters, a door onto the walkway beside the ladder.
fn cabin(kit: &mut Kit<Surface>) {
    let frames = [
        (
            pose(DVec3::new(0.0, 0.0, CABIN_Z), DVec3::ZERO),
            CABIN_X,
            true,
        ),
        (
            pose(DVec3::new(0.0, 0.0, -CABIN_Z), DVec3::new(0.0, PI, 0.0)),
            CABIN_X,
            false,
        ),
        (
            pose(
                DVec3::new(CABIN_X, 0.0, 0.0),
                DVec3::new(0.0, PI / 2.0, 0.0),
            ),
            CABIN_Z,
            false,
        ),
        (
            pose(
                DVec3::new(-CABIN_X, 0.0, 0.0),
                DVec3::new(0.0, -PI / 2.0, 0.0),
            ),
            CABIN_Z,
            false,
        ),
    ];
    let glazing = Glazing {
        cols: 2,
        rows: 2,
        double_hung: false,
    };
    for (frame, half, front) in frames {
        let centres: Vec<f64> = if front {
            vec![-1.25, 0.05]
        } else if half > 2.0 {
            vec![-1.3, 0.0, 1.3]
        } else {
            vec![-0.8, 0.8]
        };
        let windows: Vec<Opening> = centres
            .into_iter()
            .map(|u| Opening {
                u,
                y: WINDOW_SILL,
                width: WINDOW_WIDTH,
                height: WINDOW_HEIGHT,
            })
            .collect();
        let entry = Opening {
            u: 1.35,
            y: DECK_TOP,
            width: 0.8,
            height: 1.85,
        };
        let mut openings = windows.clone();
        if front {
            openings.push(entry);
        }
        let outline = [
            (-half, DECK_TOP),
            (half, DECK_TOP),
            (half, CABIN_TOP),
            (-half, CABIN_TOP),
        ]
        .map(|(u, y)| DVec2::new(u, y));
        wall(kit, frame, &outline, &openings, REVEAL);
        for opening in &windows {
            window(kit, frame, opening, glazing, REVEAL);
            awning(kit, frame, opening);
        }
        if front {
            door(kit, frame, &entry, REVEAL);
        }
        let block = |kit: &mut Kit<Surface>, size: DVec3, at: DVec3| {
            kit.block(Surface::Trim, size, frame * DMat4::from_translation(at));
        };
        for side in [-1.0, 1.0] {
            block(
                kit,
                DVec3::new(0.15, CABIN_TOP - DECK_TOP, 0.03),
                DVec3::new(side * (half - 0.06), (CABIN_TOP + DECK_TOP) / 2.0, 0.015),
            );
        }
        let mut sill_runs = vec![(-half - 0.03, half + 0.03)];
        if front {
            sill_runs = vec![(-half - 0.03, entry.u - 0.5), (entry.u + 0.5, half + 0.03)];
        }
        for (from, to) in sill_runs {
            block(
                kit,
                DVec3::new(to - from, 0.14, 0.035),
                DVec3::new((from + to) / 2.0, DECK_TOP + 0.07, 0.0175),
            );
        }
        block(
            kit,
            DVec3::new(2.0 * half + 0.06, 0.16, 0.03),
            DVec3::new(0.0, CABIN_TOP - 0.08, 0.015),
        );
    }
}

/// A board-and-batten shutter hinged above a window and swung out on two props.
fn awning(kit: &mut Kit<Surface>, frame: DMat4, opening: &Opening) {
    let width = opening.width + 0.22;
    let height = opening.height + 0.06;
    let hinge = DVec3::new(opening.u, opening.top() + 0.2, 0.05);
    let tilt = DVec3::new(-AWNING_OPEN, 0.0, 0.0);
    let hung = frame * pose(hinge, tilt);
    let at = |p: DVec3| hung * DMat4::from_translation(p);
    kit.block(
        Surface::Accent,
        DVec3::new(width, height, 0.03),
        at(DVec3::new(0.0, -height / 2.0, 0.015)),
    );
    for i in 0..4 {
        let u = -width / 2.0 + 0.08 + (width - 0.16) * f64::from(i) / 3.0;
        kit.block(
            Surface::Accent,
            DVec3::new(0.06, height - 0.04, 0.025),
            at(DVec3::new(u, -height / 2.0, 0.042)),
        );
    }
    for y in [-0.12, -height + 0.12] {
        kit.block(
            Surface::Trim,
            DVec3::new(width - 0.04, 0.08, 0.02),
            at(DVec3::new(0.0, y, -0.01)),
        );
    }
    for side in [-1.0, 1.0] {
        kit.block(
            Surface::Iron,
            DVec3::new(0.05, 0.16, 0.012),
            at(DVec3::new(side * (width / 2.0 - 0.2), -0.06, 0.05)),
        );
        let foot = frame.transform_point3(DVec3::new(
            opening.u + side * (opening.width / 2.0 - 0.05),
            opening.y - 0.02,
            0.08,
        ));
        let tip =
            hung.transform_point3(DVec3::new(side * (width / 2.0 - 0.12), -height + 0.04, 0.0));
        kit.rod(Surface::Trim, 0.018, foot, tip, 5);
    }
}

/// The hipped roof: a boxed soffit at the wall top, a trimmed fascia, standing
/// seams up each slope, hip and ridge caps, and an anemometer mast on the ridge.
fn hip_roof(kit: &mut Kit<Surface>) {
    let (ex, ez) = (CABIN_X + ROOF_OVERHANG, CABIN_Z + ROOF_OVERHANG);
    let ridge_x = ex - ez;
    let eave = CABIN_TOP;
    let top = eave + ROOF_EDGE;
    let apex = top + ROOF_PITCH * ez;
    let rect = |hx: f64, hz: f64, y: f64| {
        [(hx, -hz), (hx, hz), (-hx, hz), (-hx, -hz)]
            .map(|(x, z)| DVec3::new(x, y, z))
            .to_vec()
    };
    kit.solid(
        Surface::Roof,
        &[
            rect(ex, ez, eave),
            rect(ex, ez, top),
            rect(ridge_x, 0.0, apex),
        ],
    );
    kit.block(
        Surface::Trim,
        DVec3::new(2.0 * ex - 0.02, 0.01, 2.0 * ez - 0.02),
        DMat4::from_translation(DVec3::new(0.0, eave - 0.005, 0.0)),
    );
    for (size, at) in [
        (
            DVec3::new(2.0 * ex + 0.06, ROOF_EDGE + 0.04, 0.03),
            DVec3::new(0.0, 0.0, ez + 0.015),
        ),
        (
            DVec3::new(2.0 * ex + 0.06, ROOF_EDGE + 0.04, 0.03),
            DVec3::new(0.0, 0.0, -ez - 0.015),
        ),
        (
            DVec3::new(0.03, ROOF_EDGE + 0.04, 2.0 * ez),
            DVec3::new(ex + 0.015, 0.0, 0.0),
        ),
        (
            DVec3::new(0.03, ROOF_EDGE + 0.04, 2.0 * ez),
            DVec3::new(-ex - 0.015, 0.0, 0.0),
        ),
    ] {
        kit.block(
            Surface::Trim,
            size,
            DMat4::from_translation(at + DVec3::Y * (eave + ROOF_EDGE / 2.0 - 0.02)),
        );
    }
    // Seams stand on each slope from the eave to the hip or ridge.
    let lift = 0.018;
    let height_at = |inward: f64| top + ROOF_PITCH * inward + lift;
    let seam = |kit: &mut Kit<Surface>, from: DVec3, to: DVec3| {
        let length = (to - from).length();
        if length > 0.05 {
            kit.block(
                Surface::Roof,
                DVec3::new(0.03, 0.03, length),
                aim(from, to) * DMat4::from_translation(DVec3::Z * (length / 2.0)),
            );
        }
    };
    let count_x = (2.0 * ex / SEAM_SPACING).floor() as i32;
    for i in 1..count_x {
        let x = -ex + 2.0 * ex * f64::from(i) / f64::from(count_x);
        let end = (x.abs() - ridge_x).max(0.0);
        for edge in [-1.0, 1.0] {
            seam(
                kit,
                DVec3::new(x, height_at(0.0), edge * ez),
                DVec3::new(x, height_at(ez - end), edge * end),
            );
        }
    }
    let count_z = (2.0 * ez / SEAM_SPACING).floor() as i32;
    for i in 1..count_z {
        let z = -ez + 2.0 * ez * f64::from(i) / f64::from(count_z);
        let end = ridge_x + z.abs();
        for side in [-1.0, 1.0] {
            seam(
                kit,
                DVec3::new(side * ex, height_at(0.0), z),
                DVec3::new(side * end, height_at(ex - end), z),
            );
        }
    }
    let cap = |kit: &mut Kit<Surface>, from: DVec3, to: DVec3| {
        kit.rod(Surface::Roof, 0.045, from, to, 6);
    };
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        cap(
            kit,
            DVec3::new(sx * ex, top + 0.02, sz * ez),
            DVec3::new(sx * ridge_x, apex + 0.02, 0.0),
        );
    }
    cap(
        kit,
        DVec3::new(-ridge_x, apex + 0.02, 0.0),
        DVec3::new(ridge_x, apex + 0.02, 0.0),
    );
    // Anemometer: mast, cross arms and three cups.
    let mast_top = apex + 1.25;
    kit.post(Surface::Iron, 0.025, 1.25, DVec3::new(0.0, apex, 0.0), 6);
    kit.post(
        Surface::Iron,
        0.045,
        0.1,
        DVec3::new(0.0, mast_top - 0.05, 0.0),
        8,
    );
    for i in 0..3 {
        let angle = f64::from(i) * PI * 2.0 / 3.0 + 0.4;
        let out = DVec3::new(angle.cos(), 0.0, angle.sin());
        let hub = DVec3::new(0.0, mast_top, 0.0);
        let cup = hub + out * 0.26;
        kit.rod(Surface::Iron, 0.008, hub, cup, 4);
        let tangent = DVec3::new(-out.z, 0.0, out.x);
        kit.turned(
            Surface::Iron,
            &[(0.0, 0.0), (0.03, 0.01), (0.045, 0.045), (0.0, 0.045)]
                .map(|(r, z)| DVec2::new(r, z)),
            8,
            aim(cup - tangent * 0.02, cup + tangent),
        );
    }
}

/// A steel ladder from a concrete pad to grab handles above the deck, tied to
/// the deck rim and braced back to the nearest post.
fn ladder(kit: &mut Kit<Surface>) {
    let top = DECK_TOP + RAIL_HEIGHT + 0.05;
    for side in [-1.0, 1.0] {
        let x = LADDER_X + side * LADDER_HALF;
        kit.block(
            Surface::Metal,
            DVec3::new(0.05, top, 0.07),
            DMat4::from_translation(DVec3::new(x, top / 2.0, LADDER_Z)),
        );
        // Stand-offs to the deck rim.
        kit.block(
            Surface::Metal,
            DVec3::new(0.05, 0.06, LADDER_Z - DECK_Z + 0.02),
            DMat4::from_translation(DVec3::new(x, DECK_TOP - 0.13, (LADDER_Z + DECK_Z) / 2.0)),
        );
    }
    let rungs = ((DECK_TOP - 0.3) / 0.3).floor() as i32;
    for i in 0..=rungs {
        let y = 0.3 + f64::from(i) * 0.3;
        kit.rod(
            Surface::Iron,
            0.017,
            DVec3::new(LADDER_X - LADDER_HALF, y, LADDER_Z),
            DVec3::new(LADDER_X + LADDER_HALF, y, LADDER_Z),
            6,
        );
    }
    kit.chamfer_block(
        Surface::Concrete,
        DVec3::new(0.95, 0.1, 0.5),
        0.02,
        DMat4::from_translation(DVec3::new(LADDER_X, 0.05, LADDER_Z)),
    );
    let post = DVec3::new(TOWER_BASE.offset, 2.8, TOWER_BASE.post_z + POST / 2.0);
    for side in [-1.0, 1.0] {
        kit.rod(
            Surface::Metal,
            0.02,
            DVec3::new(LADDER_X + side * LADDER_HALF, 2.8, LADDER_Z - 0.03),
            post + DVec3::new(-0.1 + side * 0.08, 0.0, 0.0),
            6,
        );
    }
}

/// A searchlight on a yoke atop the front-left railing post, aimed out and down.
fn searchlight(kit: &mut Kit<Surface>) {
    let base = DVec3::new(
        -(DECK_X - RAIL_INSET),
        DECK_TOP + RAIL_HEIGHT,
        DECK_Z - RAIL_INSET,
    );
    kit.post(Surface::Iron, 0.06, 0.06, base, 8);
    kit.block(
        Surface::Iron,
        DVec3::new(0.42, 0.04, 0.06),
        DMat4::from_translation(base + DVec3::new(0.0, 0.08, 0.0)),
    );
    for side in [-1.0, 1.0] {
        kit.block(
            Surface::Iron,
            DVec3::new(0.03, 0.26, 0.05),
            DMat4::from_translation(base + DVec3::new(side * 0.2, 0.2, 0.0)),
        );
    }
    let pivot = base + DVec3::new(0.0, 0.28, 0.0);
    let ahead = DVec3::new(-0.45, -0.25, 0.85).normalize();
    let back = pivot - ahead * 0.2;
    let housing = aim(back, pivot + ahead);
    kit.turned(
        Surface::Roof,
        &[
            (0.0, 0.0),
            (0.1, 0.0),
            (0.15, 0.06),
            (0.16, 0.34),
            (0.18, 0.36),
            (0.18, 0.4),
        ]
        .map(|(r, z)| DVec2::new(r, z)),
        14,
        housing,
    );
    kit.turned(
        Surface::Lamp,
        &[(0.165, 0.0), (0.12, 0.012), (0.0, 0.018)].map(|(r, z)| DVec2::new(r, z)),
        14,
        housing * DMat4::from_translation(DVec3::Z * 0.4),
    );
}

/// One surviving footing with its base plates, cut posts and a scatter of
/// boards, stable per collapse through `debris_seed`.
pub(super) fn rubble(group: &mut Node, x: f64, z: f64, color: u32, debris_seed: Option<f64>) {
    tower_foundation(group, 0.0);
    let mut rng =
        Random::new(debris_seed.unwrap_or_else(|| js_round(x * 73_856_093.0 + z * 19_349_663.0)));
    fn choose<T: Copy>(rng: &mut Random, values: &[T]) -> T {
        values[(rng.next() * values.len() as f64).floor() as usize]
    }
    let base = TOWER_BASE.height + 0.03;
    for post_z in [-TOWER_BASE.post_z, TOWER_BASE.post_z] {
        put(
            group,
            super::model_primitives::box_part(0.52, 0.03, 0.52, 0x25272b, 0.0),
            0.0,
            TOWER_BASE.height + 0.015,
            post_z,
        );
        // Cut posts keep their original position, section and grain direction.
        let height = choose(&mut rng, &[0.12, 0.2, 0.28, 0.34]);
        let seed = js_round(rng.next() * 1000.0) as i32;
        put(group, post(height, seed), 0.0, base + height / 2.0, post_z);
        if rng.next() < 0.7 {
            let rz = rng.range(-0.4, 0.4);
            let splinter = rotated(
                timber_member(DVec3::new(0.09, 0.16, 0.12), seed + 1, 0xc5a073),
                0.0,
                0.0,
                rz,
            );
            let sx = rng.range(-0.1, 0.1);
            put(group, splinter, sx, base + height + 0.03, post_z);
        }
    }
    // Discrete sizes keep the pile varied; each footing gets its own scatter.
    let count = choose(&mut rng, &[2, 3, 4]);
    for i in 0..count {
        let width = choose(&mut rng, &[0.16, 0.3, 0.55]);
        let length = choose(&mut rng, &[0.7, 1.1, 1.5]);
        let yaw = rng.range(-0.55, 0.55);
        let board_color = choose(&mut rng, &[color, TOWER_DECK, TOWER_BRACE]);
        let board = rotated(
            timber_member(DVec3::new(length, 0.09, width), 1200 + i, board_color),
            0.0,
            yaw + PI / 2.0,
            0.0,
        );
        // Keep the pile inside its foundation, preserving the opened centre route.
        let room_x = ((TOWER_BASE.width - width * yaw.cos() - length * yaw.sin().abs()) / 2.0
            - 0.02)
            .max(0.0);
        let room_z = (TOWER_BASE.depth - length * yaw.cos() - width * yaw.sin().abs()) / 2.0 - 0.02;
        let bx = rng.range(-room_x, room_x);
        let bz = rng.range(-room_z, room_z);
        put(
            group,
            board,
            bx,
            TOWER_BASE.height + 0.045 + f64::from(i) * 0.055,
            bz,
        );
    }
}
