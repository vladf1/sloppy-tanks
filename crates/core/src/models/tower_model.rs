//! The watchtower, part by part.
//!
//! Four sawn posts in steel shoes stand on the two low concrete footings
//! ([`TOWER_BASE`]); X-braces on every face with a girt across each, knee braces out
//! to outrigger beams, main beams and joists under a plank deck with rim boards; a
//! railed walkway round a clapboard lookout cabin whose windows carry top-hinged
//! awning shutters propped open; a hipped standing-seam roof with an anemometer mast;
//! a steel ladder up the front and a searchlight on the corner post.
//!
//! The model is built from the parts of the tower's structure (`tower_structure`),
//! each in tower coordinates where its collider is: [`tower`] draws the parts still
//! standing (an untouched tower is one model every tower shares) and [`tower_debris`]
//! the parts falling together as one body.

use std::f64::consts::PI;

use glam::{DMat4, DVec2, DVec3};

use super::batching::batch;
use super::building_kit::{
    Glazing, Opening, Palette, Surface, add_building, door, uv_per_metre, wall, window,
};
use super::concrete_surfaces::concrete_wall;
use super::house_model::wall_frames;
use super::model_primitives::{put, rotated};
use super::tank_kit::{Kit, aim, pose};
use super::timber_model::timber_member;
use crate::geometry::math::js_round;
use crate::scene::Node;
use crate::sim::house_structure::PartSet;
use crate::sim::tower_layout::{
    BEAM, BENT_BRACE_LOW, CABIN_TOP, CABIN_WALL, CABIN_X, CABIN_Z, DECK_TOP, DECK_X, DECK_Z,
    GIRT_Y, JOIST_DEPTH, LADDER_HALF, LADDER_TOP, LADDER_X, LADDER_Z, OUTRIGGER_Y, PLANK, POST,
    POST_TOP, RAIL_HEIGHT, RAIL_INSET, ROOF_EDGE, ROOF_OVERHANG, ROOF_PITCH, SHOE_TOP, TOWER_BASE,
    WINDOW_HEIGHT, WINDOW_SILL, roof_apex,
};
use crate::sim::tower_structure::{
    CORNERS, DECKS, FACES, FOOTINGS, LADDER, LOWER_LEGS, Member, RAILINGS, ROOF, UPPER_LEGS,
    WALLS, bent_members, face_members, knee_member, tower_structure,
};

const TOWER_POST: u32 = 0x887454;
const TOWER_BRACE: u32 = 0x96734c;
const TOWER_DECK: u32 = 0x887d59;
const TOWER_ROOF: u32 = 0x2b6a4c;
const TOWER_TRIM: u32 = 0xe6dcc2;
const TOWER_SHUTTERS: u32 = 0x7a3a2c;

const WINDOW_WIDTH: f64 = 0.95;
/// How far awning shutters swing out from the wall (radians from hanging).
const AWNING_OPEN: f64 = 1.1;
const SEAM_SPACING: f64 = 0.45;
/// The railing gap above the ladder.
const GAP: (f64, f64) = (1.3, 2.2);
/// The splintered stub a post leaves in its shoe when its lower lift is shot out:
/// how far it reaches over the shoe and its section (inside the post while it stands).
const STUB_OVER: f64 = 0.16;
const STUB: f64 = 0.33;
/// Falling parts smaller than this (m) across cast no shadow.
const SHADOWLESS_UNDER: f64 = 1.8;

fn palette(color: u32) -> Palette {
    Palette {
        siding: color,
        trim: TOWER_TRIM,
        accent: TOWER_SHUTTERS,
        roof: TOWER_ROOF,
    }
}

/// A post section `height` long.
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
    framed(
        &Member {
            from,
            to,
            width,
            depth,
            normal,
        },
        seed,
        color,
    )
}

/// A member of the structure, drawn where its collider is.
fn framed(member: &Member, seed: i32, color: u32) -> Node {
    let (center, half, rotation) = member.frame();
    let mut node = timber_member(half * 2.0, seed, color);
    node.rotation = rotation;
    node.position = center;
    node
}

/// The watchtower's standing parts (those not in `broken`), its cabin painted
/// `color`.
pub(super) fn tower(group: &mut Node, color: u32, broken: &PartSet) {
    let standing = (0..tower_structure().parts.len()).filter(|&index| !broken.contains(index));
    parts_model(group, color, standing);
}

/// The parts `parts` of a watchtower painted `color`, fallen together: drawn about the
/// tower's ground centre, where their body started, and batched.
pub fn tower_debris(color: u32, parts: &PartSet) -> Node {
    let mut node = Node::default();
    parts_model(&mut node, color, parts.iter());
    batch(&mut node);
    // A small piece casts no shadow: only the big sections' shadows show from the
    // battlefield.
    let layout = tower_structure();
    let (low, high) = parts.iter().fold(
        (DVec3::INFINITY, DVec3::NEG_INFINITY),
        |(low, high), index| (low.min(layout.parts[index].min), high.max(layout.parts[index].max)),
    );
    if (high - low).max_element() < SHADOWLESS_UNDER {
        uncast(&mut node);
    }
    let mut root = Node::group("tower-debris");
    root.children.push(node);
    root
}

fn uncast(node: &mut Node) {
    if let Some(drawable) = &mut node.drawable {
        drawable.cast_shadow = false;
    }
    for child in &mut node.children {
        uncast(child);
    }
}

/// The members and finishes of `parts` in tower coordinates.
fn parts_model(group: &mut Node, color: u32, parts: impl Iterator<Item = usize>) {
    let mut kit = Kit::new();
    for part in parts {
        part_model(group, &mut kit, part);
    }
    add_building(group, kit.finish_scaled(uv_per_metre), &palette(color));
}

/// One part's members (timber nodes into `group`) and finishes (into `kit`).
fn part_model(group: &mut Node, kit: &mut Kit<Surface>, part: usize) {
    let mut next = seeds(700 + 37 * part as i32);
    let corner_of = |legs: &[usize; 4]| legs.iter().position(|&leg| leg == part);
    if let Some(side) = FOOTINGS.iter().position(|&footing| footing == part) {
        footing(group, kit, if side == 0 { -1.0 } else { 1.0 }, &mut next);
    } else if let Some(corner) = corner_of(&LOWER_LEGS) {
        let (sx, sz) = CORNERS[corner];
        let low = TOWER_BASE.height + 0.03;
        put(
            group,
            post(GIRT_Y - low, next()),
            sx * TOWER_BASE.offset,
            (GIRT_Y + low) / 2.0,
            sz * TOWER_BASE.post_z,
        );
    } else if let Some(corner) = corner_of(&UPPER_LEGS) {
        let (sx, sz) = CORNERS[corner];
        put(
            group,
            post(POST_TOP - GIRT_Y, next()),
            sx * TOWER_BASE.offset,
            (POST_TOP + GIRT_Y) / 2.0,
            sz * TOWER_BASE.post_z,
        );
        group
            .children
            .push(framed(&knee_member(corner), next(), TOWER_BRACE));
    } else if let Some(face) = FACES.iter().position(|members| members.contains(&part)) {
        let k = FACES[face].iter().position(|&member| member == part).expect("a member");
        let members = match face {
            0 => bent_members(-1.0),
            1 => bent_members(1.0),
            2 => face_members(-1.0),
            _ => face_members(1.0),
        };
        group.children.push(framed(&members[k], next(), TOWER_BRACE));
        if face < 2 && k < 2 {
            // The bolts through a bent's brace crossing.
            let side = if face == 0 { -1.0 } else { 1.0 };
            let x = side * TOWER_BASE.offset;
            let out = if k == 0 { 1.0 } else { -1.0 };
            kit.block(
                Surface::Iron,
                DVec3::new(0.02, 0.06, 0.06),
                DMat4::from_translation(DVec3::new(
                    x + side * out * (POST / 2.0 + 0.125),
                    (BENT_BRACE_LOW + POST_TOP - 0.2) / 2.0,
                    0.0,
                )),
            );
        }
    } else if let Some(side) = DECKS.iter().position(|&deck| deck == part) {
        deck_half(group, if side == 0 { -1.0 } else { 1.0 });
    } else if let Some(side) = RAILINGS.iter().position(|&railing| railing == part) {
        railing_half(group, if side == 0 { -1.0 } else { 1.0 });
        if side == 0 {
            searchlight(kit);
        }
    } else if let Some(wall) = WALLS.iter().position(|&wall| wall == part) {
        cabin_wall(kit, wall);
    } else if part == ROOF {
        hip_roof(kit);
    } else if part == LADDER {
        ladder(kit);
    }
}

/// Seeds for one part's members, distinct between parts.
fn seeds(start: i32) -> impl FnMut() -> i32 {
    let mut seed = start;
    move || {
        seed += 1;
        seed
    }
}

/// A concrete footing at `side` with its steel shoes, and in each shoe the
/// splintered stub its post leaves when the leg is shot out (hidden inside the post
/// while it stands).
fn footing(group: &mut Node, kit: &mut Kit<Surface>, side: f64, next: &mut impl FnMut() -> i32) {
    let x = side * TOWER_BASE.offset;
    put(
        group,
        concrete_wall(TOWER_BASE.width, TOWER_BASE.height, TOWER_BASE.depth),
        x,
        TOWER_BASE.height / 2.0,
        0.0,
    );
    shoes(kit, x);
    for z in [-TOWER_BASE.post_z, TOWER_BASE.post_z] {
        let base = TOWER_BASE.height + 0.03;
        let height = SHOE_TOP + STUB_OVER - base;
        let seed = next();
        put(
            group,
            timber_member(DVec3::new(STUB, height, STUB), seed, TOWER_POST),
            x,
            base + height / 2.0,
            z,
        );
        let splinter = rotated(
            timber_member(DVec3::new(0.09, 0.16, 0.12), seed + 1, 0xc5a073),
            0.0,
            0.0,
            0.3 * side,
        );
        put(group, splinter, x + 0.04 * side, base + height + 0.03, z);
    }
}

/// Steel shoes on one footing: base plates with anchor nuts and side plates
/// clasping the foot of each post. They stay behind when the posts snap.
fn shoes(kit: &mut Kit<Surface>, x: f64) {
    let base = TOWER_BASE.height;
    for z in [-TOWER_BASE.post_z, TOWER_BASE.post_z] {
        kit.block(
            Surface::Iron,
            DVec3::new(0.52, 0.03, 0.52),
            DMat4::from_translation(DVec3::new(x, base + 0.015, z)),
        );
        for (dx, dz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            kit.post(
                Surface::Iron,
                0.025,
                0.05,
                DVec3::new(x + dx * 0.21, base + 0.03, z + dz * 0.21),
                6,
            );
        }
        let height = SHOE_TOP - base - 0.03;
        let y = base + 0.03 + height / 2.0;
        let face = POST / 2.0 + 0.012;
        for sign in [-1.0, 1.0] {
            kit.block(
                Surface::Iron,
                DVec3::new(0.024, height, POST + 0.048),
                DMat4::from_translation(DVec3::new(x + sign * face, y, z)),
            );
            kit.block(
                Surface::Iron,
                DVec3::new(POST, height, 0.024),
                DMat4::from_translation(DVec3::new(x, y, z + sign * face)),
            );
        }
    }
}

/// Whether `x` lies on the deck half `side` (the middle belongs to the east).
fn on_half(x: f64, side: f64) -> bool {
    if side > 0.0 { x > -1e-6 } else { x < -1e-6 }
}

/// One half of the deck: main and outrigger beams, joists, rim boards and
/// planks, split where the deck breaks along its middle.
fn deck_half(group: &mut Node, side: f64) {
    let mut next = seeds(800 + js_round(side * 100.0) as i32);
    let (inner, outer) = (side * 0.01, side * (DECK_X - 0.05));
    for edge in [-1.0, 1.0] {
        let beam_y = POST_TOP + BEAM / 2.0;
        group.children.push(member(
            DVec3::new(inner, beam_y, edge * TOWER_BASE.post_z),
            DVec3::new(outer, beam_y, edge * TOWER_BASE.post_z),
            BEAM,
            BEAM,
            DVec3::Z,
            next(),
            TOWER_POST,
        ));
        group.children.push(member(
            DVec3::new(inner, OUTRIGGER_Y, edge * (DECK_Z - 0.45)),
            DVec3::new(side * (DECK_X - 0.2), OUTRIGGER_Y, edge * (DECK_Z - 0.45)),
            0.18,
            0.18,
            DVec3::Z,
            next(),
            TOWER_POST,
        ));
    }
    let joists = 11;
    let joist_y = POST_TOP + BEAM + JOIST_DEPTH / 2.0;
    for i in 0..joists {
        let x = -DECK_X + 0.1 + (2.0 * DECK_X - 0.2) * f64::from(i) / f64::from(joists - 1);
        if !on_half(x, side) {
            continue;
        }
        group.children.push(member(
            DVec3::new(x, joist_y, -DECK_Z + 0.04),
            DVec3::new(x, joist_y, DECK_Z - 0.04),
            JOIST_DEPTH,
            0.07,
            DVec3::X,
            next(),
            TOWER_POST,
        ));
    }
    let rim_y = DECK_TOP - 0.13;
    for edge in [-1.0, 1.0] {
        group.children.push(member(
            DVec3::new(inner, rim_y, edge * (DECK_Z - 0.02)),
            DVec3::new(side * DECK_X, rim_y, edge * (DECK_Z - 0.02)),
            0.26,
            0.05,
            DVec3::Z,
            next(),
            TOWER_DECK,
        ));
    }
    group.children.push(member(
        DVec3::new(side * (DECK_X - 0.02), rim_y, -DECK_Z + 0.05),
        DVec3::new(side * (DECK_X - 0.02), rim_y, DECK_Z - 0.05),
        0.26,
        0.05,
        DVec3::X,
        next(),
        TOWER_DECK,
    ));
    let planks = 33;
    let pitch = 2.0 * DECK_Z / f64::from(planks);
    let length = DECK_X - 0.02;
    for i in 0..planks {
        let z = -DECK_Z + pitch * (f64::from(i) + 0.5);
        put(
            group,
            timber_member(DVec3::new(length, PLANK, pitch - 0.012), next(), TOWER_DECK),
            side * (0.01 + length / 2.0),
            DECK_TOP - PLANK / 2.0,
            z,
        );
    }
}

/// The railing on one deck half: posts, top and mid rails and toe boards, open
/// above the ladder.
fn railing_half(group: &mut Node, side: f64) {
    let (rx, rz) = (DECK_X - RAIL_INSET, DECK_Z - RAIL_INSET);
    let mut next = seeds(1000 + js_round(side * 100.0) as i32);
    let mut posts: Vec<(f64, f64)> = Vec::new();
    for edge in [-1.0, 1.0] {
        for i in 0..=6 {
            let x = -rx + 2.0 * rx * f64::from(i) / 6.0;
            if edge > 0.0 && x > GAP.0 - 0.1 && x < GAP.1 + 0.1 {
                continue;
            }
            posts.push((x, edge * rz));
        }
    }
    for i in 1..5 {
        posts.push((side * rx, -rz + 2.0 * rz * f64::from(i) / 5.0));
    }
    posts.push((GAP.0, rz));
    posts.push((GAP.1, rz));
    let inner = side * 0.01;
    // (from, to) runs; the front edge stops at the ladder gap.
    let mut runs = vec![
        (DVec2::new(inner, -rz), DVec2::new(side * rx, -rz)),
        (DVec2::new(side * rx, -rz), DVec2::new(side * rx, rz)),
    ];
    if side < 0.0 {
        runs.push((DVec2::new(-rx, rz), DVec2::new(inner, rz)));
    } else {
        runs.push((DVec2::new(inner, rz), DVec2::new(GAP.0, rz)));
        runs.push((DVec2::new(GAP.1, rz), DVec2::new(rx, rz)));
    }
    for (x, z) in posts {
        if !on_half(x, side) {
            continue;
        }
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

/// One wall of the lookout cabin (`wall` in [`WALLS`] order: front, back, west,
/// east): clapboard over a trimmed sill, glazed under propped awning shutters, the
/// front with a door onto the walkway beside the ladder, lined inside so a fallen
/// wall is solid from behind.
fn cabin_wall(kit: &mut Kit<Surface>, wall_index: usize) {
    let [front_wall, back_wall, east_wall, west_wall] = wall_frames(2.0 * CABIN_X, 2.0 * CABIN_Z);
    let (frame, half) = [front_wall, back_wall, west_wall, east_wall][wall_index];
    let front = wall_index == 0;
    let glazing = Glazing { double_hung: false };
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
    let outline = vec![
        DVec2::new(-half, DECK_TOP),
        DVec2::new(half, DECK_TOP),
        DVec2::new(half, CABIN_TOP),
        DVec2::new(-half, CABIN_TOP),
    ];
    wall(kit, frame, &outline, &openings, CABIN_WALL);
    // The lining faces into the cabin, so its outline and openings mirror.
    let inside = frame * pose(DVec3::new(0.0, 0.0, -CABIN_WALL), DVec3::new(0.0, PI, 0.0));
    let holes: Vec<Vec<DVec2>> = openings
        .iter()
        .map(|o| Opening { u: -o.u, ..*o }.corners())
        .collect();
    kit.face_with_holes(Surface::Siding, &outline, &holes, inside);
    // End, top and bottom edges between the face and the lining.
    let at = |u: f64, y: f64, z: f64| frame.transform_point3(DVec3::new(u, y, z));
    let facing = |x: f64, y: f64| frame.transform_vector3(DVec3::new(x, y, 0.0));
    for (a, b, normal) in [
        ((-half, DECK_TOP), (-half, CABIN_TOP), facing(-1.0, 0.0)),
        ((half, DECK_TOP), (half, CABIN_TOP), facing(1.0, 0.0)),
        ((-half, CABIN_TOP), (half, CABIN_TOP), facing(0.0, 1.0)),
        ((-half, DECK_TOP), (half, DECK_TOP), facing(0.0, -1.0)),
    ] {
        kit.quad(
            Surface::Trim,
            [
                at(a.0, a.1, 0.0),
                at(b.0, b.1, 0.0),
                at(b.0, b.1, -CABIN_WALL),
                at(a.0, a.1, -CABIN_WALL),
            ],
            normal,
        );
    }
    for opening in &windows {
        window(kit, frame, opening, glazing, CABIN_WALL);
        awning(kit, frame, opening);
    }
    if front {
        door(kit, frame, &entry, CABIN_WALL);
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
    let apex = roof_apex();
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
    let (board, y) = (ROOF_EDGE + 0.04, eave + ROOF_EDGE / 2.0 - 0.02);
    for sign in [1.0, -1.0] {
        kit.block(
            Surface::Trim,
            DVec3::new(2.0 * ex + 0.06, board, 0.03),
            DMat4::from_translation(DVec3::new(0.0, y, sign * (ez + 0.015))),
        );
    }
    for sign in [1.0, -1.0] {
        kit.block(
            Surface::Trim,
            DVec3::new(0.03, board, 2.0 * ez),
            DMat4::from_translation(DVec3::new(sign * (ex + 0.015), y, 0.0)),
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

/// A steel ladder from the ground to grab handles above the deck, tied to the
/// deck rim.
fn ladder(kit: &mut Kit<Surface>) {
    for side in [-1.0, 1.0] {
        let x = LADDER_X + side * LADDER_HALF;
        kit.block(
            Surface::Metal,
            DVec3::new(0.05, LADDER_TOP - 0.01, 0.07),
            DMat4::from_translation(DVec3::new(x, (LADDER_TOP + 0.01) / 2.0, LADDER_Z)),
        );
        kit.block(
            Surface::Metal,
            DVec3::new(0.12, 0.012, 0.12),
            DMat4::from_translation(DVec3::new(x, 0.016, LADDER_Z)),
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
