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
use std::sync::Arc;

use glam::{DMat4, DQuat, DVec2, DVec3};

use super::batching::batch;
use super::building_kit::{
    Glazing, Opening, Palette, Surface, add_building, door, uv_per_metre, wall, window,
};
use super::concrete_surfaces::concrete_wall;
use super::house_model::wall_frames;
use super::model_primitives::put;
use super::tank_kit::{Kit, aim, pose};
use super::timber_model::timber_member;
use crate::geometry::Mesh;
use crate::geometry::math::js_round;
use crate::scene::Node;
use crate::sim::structure::PartSet;
use crate::sim::math::Random;
use crate::sim::tower_layout::{
    BEAM, BENT_BRACE_LOW, CABIN_TOP, CABIN_WALL, CABIN_X, CABIN_Z, DECK_TOP, DECK_X, DECK_Z,
    GIRT_Y, JOIST_DEPTH, LADDER_HALF, LADDER_TOP, LADDER_X, LADDER_Z, OUTRIGGER_Y, PLANK, POST,
    POST_TOP, RAIL_HEIGHT, RAIL_INSET, ROOF_EDGE, ROOF_OVERHANG, ROOF_PITCH, SHOE_TOP, TOWER_BASE,
    WINDOW_HEIGHT, WINDOW_SILL, roof_apex,
};
use crate::sim::tower_structure::{
    CORNERS, DECK_CUT, DECKS, DOOR_HEIGHT, DOOR_U, DOOR_WIDTH, FACES, FOOTINGS, LADDER,
    LOWER_LEGS, Member, PieceLook, RAILINGS, ROOF, SHUTTER_X, STUB_REACH, UPPER_LEGS, WALLS,
    face_members_of, knee_member, piece_look, tower_structure,
};

const TOWER_POST: u32 = 0x887454;
const TOWER_BRACE: u32 = 0x96734c;
const TOWER_DECK: u32 = 0x887d59;
const TOWER_ROOF: u32 = 0x2b6a4c;
const TOWER_TRIM: u32 = 0xe6dcc2;
const TOWER_SHUTTERS: u32 = 0x7a3a2c;
/// Freshly torn timber, and timber a shell's burst charred.
const FRESH_WOOD: u32 = 0xc5a073;
const SCORCH: u32 = 0x2e241c;

const WINDOW_WIDTH: f64 = 0.95;
/// How far awning shutters swing out from the wall (radians from hanging).
const AWNING_OPEN: f64 = 1.1;
const SEAM_SPACING: f64 = 0.45;
/// The railing gap above the ladder.
const GAP: (f64, f64) = (1.3, 2.2);
/// A stub's section (a little inside the post's).
const STUB: f64 = 0.33;
/// The bite a shell that did not cut a leg took out of it: its middle (shell height),
/// half its height, the share of the post's section it took and the fibres torn out.
const BITE_Y: f64 = 1.0;
const BITE_HALF: f64 = 0.16;
const BITE_DEPTH: f64 = 0.4;
const BITE_FIBRES: usize = 9;
/// How far (m) over the bite's middle a shot-out leg's stub stands, ragged and charred.
const SHOT_STUB_OVER: f64 = 0.1;
/// Lumps of torn wood in a bite's hollow and teeth on each side of its rim.
const BITE_LUMPS: usize = 5;
/// Bundles of fibres across a bite, each torn to a depth of its own.
const BITE_BUNDLES: usize = 7;
const BITE_TEETH: usize = 5;
/// Scorch fading out from a burst's mark, darkest first, and a stub charred through.
const SCORCH_FADE: [u32; 3] = [0x231a14, 0x3e3024, 0x5f4b37];
const CHARRED: u32 = 0x3a2b20;
/// How far (m) the top of a stub the fall tore out is bent over the way it went.
const TORN_STUB_LEAN: f64 = 0.24;
/// Fibres torn out of a broken member's end.
const SPLINTERS: usize = 5;
/// Where (a share of its length) each landed leg's upper length cracks through, and
/// how far (rad) a cracked member kinks there.
const LEG_CRACK: [f64; 4] = [0.45, 0.6, 0.35, 0.52];
const CRACK_KINK: f64 = 0.6;
/// The length (m) a cracked member loses at its break, and the share of what lies
/// beyond the break it keeps.
const CRACK_GAP: f64 = 0.18;
const CRACK_KEEP: f64 = 0.8;
/// How far (m) a joined X-brace panel's bent diagonal bows out of it.
const PANEL_BOW: f64 = 0.22;
/// A deck plank snaps within this far (m) of the cut, and this share of planks is lost.
const PLANK_SNAP: f64 = 0.35;
const PLANKS_LOST: f64 = 0.12;
/// A landed roof's crease (x, m from the middle) and how far (rad) its east end is
/// folded under there; its boards' width and its rafters' spacing and depth.
const ROOF_CREASE_X: f64 = 0.7;
const ROOF_FOLD: f64 = 0.6;
/// How far (m) in from the front eave a landed roof bends under, and by how much (rad).
const EAVE_BEND_AT: f64 = 0.85;
const EAVE_BEND: f64 = 0.55;
/// The sheet torn back off the back slope: its span along x, its length down the slope
/// (m) and the turns (rad) of its two halves curling back over it.
const TORN_SHEET_X: (f64, f64) = (-2.3, -0.5);
const TORN_SHEET_FRONT_X: (f64, f64) = (-1.3, 0.4);
/// How deep (m) a landed roof's sheet is dented, and the facets its shell is cut into:
/// columns along a long slope and rows up every slope.
const ROOF_DENT: f64 = 0.16;
const ROOF_COLUMNS: usize = 12;
const ROOF_ROWS: usize = 5;
/// How far (m) a torn cabin wall is racked at its top, and the height (m) and angle
/// (rad) its top is bent out at.
const WALL_RACK: f64 = 0.35;
const WALL_BEND_Y: f64 = 6.55;
const WALL_BEND: f64 = 0.4;
const TORN_SHEET_LENGTH: f64 = 1.7;
const TORN_SHEET_CURL: [f64; 2] = [2.3, 3.2];
/// Scrapes through the paint on each long slope of a landed roof.
const ROOF_SCRAPES: usize = 10;
/// A torn-off shutter's split: how far (rad) its half beyond the crack is bent back.
const SHUTTER_SPLIT: f64 = 0.45;
/// The bare timber of a landed roof's underside: its boarding and its rafters.
const ROOF_BOARDS: u32 = 0x9a7a52;
const ROOF_RAFTERS: u32 = 0x4a3524;
const RAFTER_SPACING: f64 = 0.62;
const RAFTER_DEPTH: f64 = 0.14;
/// How thick (m) a landed roof's boarding is under its sheet.
const ROOF_SHEATHING: f64 = 0.03;
/// A landed roof's rafters: the longest straight length (m) that follows the dents,
/// the gap (m) each side of a break, and the share snapped part way along.
const RAFTER_PIECE: f64 = 1.3;
const RAFTER_GAP: f64 = 0.05;
const RAFTERS_SNAPPED: f64 = 0.35;
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

/// What a standing part's model shows of the fight: the parts gone (`broken`) and
/// those struck (`struck`, gone or standing), so a struck leg wears its bite, a shot-out
/// leg leaves a tall ragged stub, and the legs torn out in the fall leave short stubs
/// leaning the way the tower went.
#[derive(Clone, Copy, Debug, Default)]
struct Scars {
    broken: PartSet,
    struck: PartSet,
}

impl Scars {
    /// The way the tower fell, across the ground: toward its shot-out legs.
    fn fall(&self) -> DVec2 {
        let shot: DVec2 = LOWER_LEGS
            .iter()
            .enumerate()
            .filter(|&(_, &leg)| self.broken.contains(leg) && self.struck.contains(leg))
            .map(|(corner, _)| {
                let (sx, sz) = CORNERS[corner];
                DVec2::new(sx * TOWER_BASE.offset, sz * TOWER_BASE.post_z)
            })
            .sum();
        if shot.length() > 1e-6 {
            shot.normalize()
        } else {
            DVec2::X
        }
    }
}

/// The watchtower's standing parts (those not in `broken`), its cabin painted
/// `color`; `struck` names the parts shells or blasts have hit.
pub(super) fn tower(group: &mut Node, color: u32, broken: &PartSet, struck: &PartSet) {
    let scars = Scars {
        broken: *broken,
        struck: *struck,
    };
    let standing = (0..tower_structure().parts.len()).filter(|&index| !broken.contains(index));
    parts_model(group, color, standing, &scars);
}

/// The parts `parts` of a watchtower painted `color`, fallen together (or the pieces a
/// landing broke them into): drawn about the tower's ground centre, where their body
/// started, and batched.
pub fn tower_debris(color: u32, parts: &PartSet) -> Node {
    let mut node = Node::default();
    parts_model(&mut node, color, parts.iter(), &Scars::default());
    batch(&mut node);
    // A small piece casts no shadow: only the big sections' shadows show from the
    // battlefield.
    let layout = tower_structure();
    let (low, high) = parts.iter().fold(
        (DVec3::INFINITY, DVec3::NEG_INFINITY),
        |(low, high), index| {
            let part = layout.part(index);
            (low.min(part.min), high.max(part.max))
        },
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

/// The members and finishes of `parts` (and pieces) in tower coordinates.
fn parts_model(group: &mut Node, color: u32, parts: impl Iterator<Item = usize>, scars: &Scars) {
    let mut kit = Kit::new();
    for part in parts {
        match piece_look(&tower_structure(), part) {
            Some(look) => piece_model(group, &mut kit, color, part, look),
            None => part_model(group, &mut kit, part, scars),
        }
    }
    add_building(group, kit.finish_scaled(uv_per_metre), &palette(color));
}

/// One part's members (timber nodes into `group`) and finishes (into `kit`).
fn part_model(group: &mut Node, kit: &mut Kit<Surface>, part: usize, scars: &Scars) {
    let mut next = seeds(700 + 37 * part as i32);
    let corner_of = |legs: &[usize; 4]| legs.iter().position(|&leg| leg == part);
    if let Some(side) = FOOTINGS.iter().position(|&footing| footing == part) {
        footing(group, kit, if side == 0 { -1.0 } else { 1.0 }, scars, &mut next);
    } else if let Some(corner) = corner_of(&LOWER_LEGS) {
        let low = TOWER_BASE.height + 0.03;
        if scars.struck.contains(part) {
            bitten_leg(group, kit, corner, low, &mut next);
        } else {
            group.children.push(leg_run(corner, low, GIRT_Y, next()));
        }
    } else if let Some(corner) = corner_of(&UPPER_LEGS) {
        group.children.push(leg_run(corner, GIRT_Y, POST_TOP, next()));
        group
            .children
            .push(framed(&knee_member(corner), next(), TOWER_BRACE));
    } else if let Some(face) = FACES.iter().position(|members| members.contains(&part)) {
        let k = FACES[face].iter().position(|&member| member == part).expect("a member");
        group
            .children
            .push(framed(&face_members_of(face)[k], next(), TOWER_BRACE));
        if face < 2 && k < 2 {
            bent_bolts(kit, face, k);
        }
    } else if let Some(side) = DECKS.iter().position(|&deck| deck == part) {
        deck_piece(group, if side == 0 { -1.0 } else { 1.0 }, 0.0, DECK_X, None);
    } else if let Some(side) = RAILINGS.iter().position(|&railing| railing == part) {
        railing_half(group, if side == 0 { -1.0 } else { 1.0 });
        if side == 0 {
            searchlight(kit);
        }
    } else if let Some(wall) = WALLS.iter().position(|&wall| wall == part) {
        cabin_wall(kit, wall, false);
    } else if part == ROOF {
        hip_roof(kit, false);
    } else if part == LADDER {
        ladder(kit);
    }
}

/// The bolts through a bent's brace crossing, on the `k`th diagonal's face.
fn bent_bolts(kit: &mut Kit<Surface>, face: usize, k: usize) {
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

/// A leg's post at `corner` from `from` up to `to`.
fn leg_run(corner: usize, from: f64, to: f64, seed: i32) -> Node {
    let (sx, sz) = CORNERS[corner];
    let mut node = post(to - from, seed);
    node.position = DVec3::new(sx * TOWER_BASE.offset, (from + to) / 2.0, sz * TOWER_BASE.post_z);
    node
}

/// A leg's lower lift with the bite a shell took out of it at shell height on its
/// outer face: a ragged hollow of torn wood, charred in it, its rim teeth and fibres
/// curling out the way the burst pushed them, the post scorched round it and fading out.
fn bitten_leg(
    group: &mut Node,
    kit: &mut Kit<Surface>,
    corner: usize,
    low: f64,
    next: &mut impl FnMut() -> i32,
) {
    let (sx, sz) = CORNERS[corner];
    let (x, z) = (sx * TOWER_BASE.offset, sz * TOWER_BASE.post_z);
    let (bottom, top) = (BITE_Y - BITE_HALF, BITE_Y + BITE_HALF);
    group.children.push(leg_run(corner, low, bottom, next()));
    group.children.push(leg_run(corner, top, GIRT_Y, next()));
    let mut rng = Random::new(f64::from(corner as u32) * 13.7 + 0.31);
    // What is left across the bite: the post less its outer face, its torn face raw.
    let bite = POST * BITE_DEPTH;
    let core = POST - bite;
    let face = x + sx * (POST / 2.0 - bite);
    put(
        group,
        timber_member(DVec3::new(core, top - bottom, POST), next(), TOWER_POST),
        x - sx * bite / 2.0,
        BITE_Y,
        z,
    );
    // The hollow's outline: the post's fibres in bundles across it, each torn back to a
    // depth and a height of its own, so no edge of it runs straight.
    for k in 0..BITE_BUNDLES {
        let across = -POST / 2.0 + POST * (k as f64 + 0.5) / BITE_BUNDLES as f64;
        let depth = rng.range(0.15, 0.95) * bite;
        let (from, to) = (bottom + rng.range(0.0, 0.1), top - rng.range(0.0, 0.1));
        let at = DVec3::new(face + sx * depth / 2.0, (from + to) / 2.0, z + across);
        put(
            group,
            timber_member(
                DVec3::new(depth, to - from, POST / BITE_BUNDLES as f64 - 0.004),
                next(),
                if k % 2 == 0 { SCORCH } else { SCORCH_FADE[1] },
            ),
            at.x,
            at.y,
            at.z,
        );
        // Hanging from the rims: torn ends reaching into the hollow.
        for (rim, down) in [(top, -1.0), (bottom, 1.0)] {
            let reach = rng.range(0.02, 0.1);
            let y = rim + down * reach / 2.0;
            let out = rng.range(depth, bite);
            put(
                group,
                timber_member(DVec3::new(out, reach, POST / BITE_BUNDLES as f64 - 0.01), next(), SCORCH_FADE[2]),
                face + sx * out / 2.0,
                y,
                z + across,
            );
        }
    }
    // The hollow's floor: torn wood in uneven lumps, charred in patches.
    for k in 0..BITE_LUMPS {
        let y = rng.range(bottom + 0.02, top - 0.02);
        let across = rng.range(-0.13, 0.13);
        let size = DVec3::new(
            rng.range(0.02, 0.07),
            rng.range(0.04, 0.12),
            rng.range(0.05, 0.12),
        );
        let raw = k % 3 != 0;
        let at = DVec3::new(face + sx * size.x / 2.0, y, z + across);
        if raw {
            kit.block(Surface::Splinters, size, DMat4::from_translation(at));
        } else {
            put(group, timber_member(size, next(), SCORCH), at.x, at.y, at.z);
        }
    }
    // Teeth along the rim, and fibres curling out of it.
    let out = DVec3::X * sx;
    for (edge, inward) in [(top, -1.0), (bottom, 1.0)] {
        for _ in 0..BITE_TEETH {
            let zz = z + rng.range(-0.15, 0.15);
            let from = DVec3::new(face + sx * rng.range(0.03, bite), edge, zz);
            let to = from + DVec3::Y * inward * rng.range(0.03, 0.09) + out * rng.range(0.0, 0.05);
            group.children.push(member(
                from,
                to,
                rng.range(0.02, 0.05),
                rng.range(0.02, 0.04),
                DVec3::Z,
                next(),
                if rng.next() < 0.5 { SCORCH } else { FRESH_WOOD },
            ));
        }
    }
    for i in 0..BITE_FIBRES {
        let from = DVec3::new(
            face + sx * rng.range(0.0, bite),
            if i % 2 == 0 { top - 0.01 } else { bottom + 0.01 },
            z + rng.range(-0.14, 0.14),
        );
        let curl = if i % 2 == 0 { 1.0 } else { -1.0 };
        let middle = from + out * rng.range(0.05, 0.1) + DVec3::Y * curl * rng.range(0.0, 0.04);
        let tip = middle + out * rng.range(0.02, 0.05) + DVec3::Y * curl * rng.range(0.04, 0.09);
        let (width, depth) = (rng.range(0.012, 0.025), rng.range(0.01, 0.02));
        for (a, b) in [(from, middle), (middle, tip)] {
            group.children.push(member(a, b, width, depth, DVec3::Z, next(), FRESH_WOOD));
        }
    }
    // Scorch on the post round the bite, darkest at it and fading out.
    let outer = x + sx * (POST / 2.0 + 0.003);
    for (k, &paint) in SCORCH_FADE.iter().enumerate() {
        let reach = BITE_HALF + 0.05 * (k as f64 + 1.0);
        let span = POST * (0.95 - 0.2 * k as f64);
        for y in [BITE_Y - reach, BITE_Y + reach - 0.05] {
            put(
                group,
                timber_member(DVec3::new(0.006, 0.05, span), next(), paint),
                outer,
                y + 0.025,
                z,
            );
        }
    }
}

/// Fibres torn out of a member's broken end at `end`, pointing `out` along it, across
/// its section (`width` along `across`, `depth` along `normal`).
#[allow(clippy::too_many_arguments)]
fn splinters(
    group: &mut Node,
    end: DVec3,
    out: DVec3,
    across: DVec3,
    normal: DVec3,
    width: f64,
    depth: f64,
    seed: i32,
) {
    let mut rng = Random::new(f64::from(seed) * 7.13 + 0.7);
    for i in 0..SPLINTERS {
        let from = end - out * 0.03
            + across * rng.range(-0.4, 0.4) * width
            + normal * rng.range(-0.4, 0.4) * depth;
        let reach = rng.range(0.06, 0.32) * (width.max(depth) / POST).clamp(0.4, 1.0);
        let to = from + out * reach + across * rng.range(-0.05, 0.05);
        group.children.push(member(
            from,
            to,
            rng.range(0.25, 0.45) * width,
            rng.range(0.25, 0.45) * depth,
            normal,
            seed * 31 + i as i32,
            if i == 0 { TOWER_POST } else { FRESH_WOOD },
        ));
    }
}

/// A member broken at the ends named (`[from, to]`): its timber and splinters there.
fn snapped(group: &mut Node, member: &Member, broken: [bool; 2], seed: i32, color: u32) {
    group.children.push(framed(member, seed, color));
    let along = (member.to - member.from).normalize();
    let normal = (member.normal - along * member.normal.dot(along)).normalize();
    let across = normal.cross(along);
    for (end, out, at) in [(member.from, -along, broken[0]), (member.to, along, broken[1])] {
        if at {
            splinters(group, end, out, across, normal, member.width, member.depth, seed + if out == along { 1 } else { 2 });
        }
    }
}

/// A member cracked through at `at` (a share of its length) and kinked there, its two
/// lengths still hanging together by splinters; broken at the ends named too.
fn cracked(group: &mut Node, member: &Member, at: f64, broken: [bool; 2], seed: i32, color: u32) {
    let crack = member.from.lerp(member.to, at);
    let along = (member.to - member.from).normalize();
    let normal = (member.normal - along * member.normal.dot(along)).normalize();
    // A length is lost at the break, and what is left beyond it hangs off at an angle.
    let beyond = crack + along * CRACK_GAP;
    let turn = DQuat::from_axis_angle(normal.cross(along), CRACK_KINK);
    let bent = beyond + turn * (member.to - beyond) * CRACK_KEEP;
    snapped(group, &Member { to: crack, ..*member }, [broken[0], true], seed, color);
    snapped(
        group,
        &Member {
            from: beyond,
            to: bent,
            ..*member
        },
        [true, broken[1]],
        seed + 7,
        color,
    );
}

/// A piece a landing broke off, drawn as it lay in the tower before it broke.
fn piece_model(group: &mut Node, kit: &mut Kit<Surface>, color: u32, index: usize, look: PieceLook) {
    let seed = 900 + 41 * index as i32;
    match look {
        PieceLook::Leg { corner, from, to } => {
            let (sx, sz) = CORNERS[corner];
            let run = Member {
                from: DVec3::new(sx * TOWER_BASE.offset, from, sz * TOWER_BASE.post_z),
                to: DVec3::new(sx * TOWER_BASE.offset, to, sz * TOWER_BASE.post_z),
                width: POST,
                depth: POST,
                normal: DVec3::X,
            };
            if to >= POST_TOP {
                // Torn off the beams and cracked through part way down, its knee brace
                // torn off the outrigger.
                cracked(group, &run, LEG_CRACK[corner], [true, true], seed, TOWER_POST);
                snapped(group, &knee_member(corner), [false, true], seed + 5, TOWER_BRACE);
            } else {
                snapped(group, &run, [true, true], seed, TOWER_POST);
            }
        }
        PieceLook::Diagonal { face, upper } => {
            let outer = face_members_of(face)[0];
            let middle = outer.from.lerp(outer.to, 0.5);
            let half = if upper {
                Member {
                    from: middle,
                    ..outer
                }
            } else {
                Member { to: middle, ..outer }
            };
            cracked(group, &half, 0.55, [true, true], seed, TOWER_BRACE);
        }
        PieceLook::Panel { face, snapped: torn } => {
            let [outer, inner, girt] = face_members_of(face);
            let middle = outer.from.lerp(outer.to, 0.5);
            if torn {
                // The outer diagonal snapped at the bolts, the inner one cracked part way
                // and the girt torn from a leg: short broken lengths on the panel.
                let lower = Member { to: middle, ..outer };
                snapped(group, &lower, [true, true], seed, TOWER_BRACE);
                cracked(group, &inner, 0.3, [true, true], seed + 2, TOWER_BRACE);
                cracked(group, &girt, 0.62, [true, true], seed + 3, TOWER_BRACE);
            } else {
                // Twisted: bent out of the panel at its middle, still on its bolts there,
                // torn from the legs at its ends.
                let kink = middle + outer.normal * PANEL_BOW;
                snapped(group, &Member { to: kink, ..outer }, [true, false], seed, TOWER_BRACE);
                snapped(
                    group,
                    &Member {
                        from: kink,
                        ..outer
                    },
                    [false, true],
                    seed + 1,
                    TOWER_BRACE,
                );
                snapped(group, &inner, [true, true], seed + 2, TOWER_BRACE);
                snapped(group, &girt, [true, true], seed + 3, TOWER_BRACE);
            }
            if face < 2 {
                bent_bolts(kit, face, 1);
            }
        }
        PieceLook::Deck { side, outer } => {
            let sign = if side == 0 { -1.0 } else { 1.0 };
            let (from, to) = if outer { (DECK_CUT, DECK_X) } else { (0.0, DECK_CUT) };
            deck_piece(group, sign, from, to, Some(seed));
        }
        PieceLook::Wall { wall } => {
            // Racked out of square, its top torn half off the corners and bent out.
            let mut own = Kit::new();
            cabin_wall(&mut own, wall, true);
            let (frame, _) = cabin_frames()[wall];
            let along = frame.transform_vector3(DVec3::X);
            let face = frame.transform_point3(DVec3::ZERO);
            let rack = if wall % 2 == 0 { WALL_RACK } else { -WALL_RACK };
            let bend = Fold {
                pivot: DVec3::new(face.x, WALL_BEND_Y, face.z),
                axis: along,
                side: DVec3::Y,
                angle: -WALL_BEND,
            };
            let mut meshes = own.finish_scaled(uv_per_metre);
            for (surface, mesh) in &mut meshes {
                let mut bent: Mesh = if *surface == Surface::Glass {
                    shattered(mesh, index as u32)
                } else {
                    (**mesh).clone()
                };
                for position in &mut bent.positions {
                    let p = DVec3::from_array(position.map(f64::from));
                    let lift = ((p.y - DECK_TOP) / (CABIN_TOP - DECK_TOP)).clamp(0.0, 1.0);
                    let racked = p + along * rack * lift;
                    *position = bend.point(racked).as_vec3().to_array();
                }
                flat_normals(&mut bent);
                *mesh = Arc::new(bent);
            }
            add_building(group, meshes, &palette(color));
        }
        PieceLook::Door => {
            let (frame, _) = cabin_frames()[0];
            door(kit, frame, &entry_opening(), CABIN_WALL);
        }
        PieceLook::Shutter { wall } => {
            // Split along a slat: half the shutter bent back off the other.
            let (frame, _) = cabin_frames()[wall];
            let u = if wall == 0 { SHUTTER_X[0] } else { -SHUTTER_X[1] };
            let mut own = Kit::new();
            awning(&mut own, frame, &window_opening(u));
            let x = SHUTTER_X[wall];
            let z = if wall == 0 { 1.0 } else { -1.0 } * (CABIN_Z + 0.4);
            let crack = Fold {
                pivot: DVec3::new(x + 0.1, 0.0, z),
                axis: DVec3::Y,
                side: DVec3::X,
                angle: SHUTTER_SPLIT,
            };
            add_building(group, folded(own, &[crack]), &palette(color));
        }
        PieceLook::Roof => {
            let mut own = Kit::new();
            hip_roof(&mut own, true);
            for (side, (x0, x1)) in [(-1.0, TORN_SHEET_X), (1.0, TORN_SHEET_FRONT_X)] {
                torn_sheet(&mut own, side, x0, x1);
            }
            scrapes(&mut own);
            let folds = roof_folds();
            add_building(group, crumpled(own, &folds), &palette(color));
            // The boarding under the sheet, crumpled with it, and the rafters under that.
            let mut boarding = Kit::new();
            let (ex, ez) = (CABIN_X + ROOF_OVERHANG, CABIN_Z + ROOF_OVERHANG);
            let top = CABIN_TOP + ROOF_EDGE;
            roof_shell(&mut boarding, ex, ez, ex - ez, CABIN_TOP, top, roof_apex(), true);
            let boards = Palette {
                roof: ROOF_BOARDS,
                ..palette(color)
            };
            add_building(group, crumpled(boarding, &folds), &boards);
            roof_underside(group, seed, &folds);
        }
    }
}

/// The jagged shards a pane leaves in its frame when it breaks: from each corner of
/// each glass triangle, a shard reaching a random way along its two edges.
fn shattered(glass: &Mesh, seed: u32) -> Mesh {
    let mut rng = Random::new(f64::from(seed) * 9.73 + 3.1);
    let mut mesh = Mesh::default();
    for ((corners, normals), uvs) in glass
        .positions
        .as_chunks::<3>()
        .0
        .iter()
        .zip(glass.normals.as_chunks::<3>().0)
        .zip(glass.uvs.as_chunks::<3>().0)
    {
        let at = |i: usize| DVec3::from_array(corners[i % 3].map(f64::from));
        for i in 0..3 {
            let (a, b, c) = (at(i), at(i + 1), at(i + 2));
            let reach = [rng.range(0.1, 0.3), rng.range(0.1, 0.3)];
            for p in [a, a + (b - a) * reach[0], a + (c - a) * reach[1]] {
                mesh.positions.push(p.as_vec3().to_array());
                mesh.normals.push(normals[i]);
                mesh.uvs.push(uvs[i]);
            }
        }
    }
    mesh
}

/// A bend of a landed piece: the points on the `side` of the plane through `pivot`
/// containing `axis` turn about that axis by `angle`.
#[derive(Clone, Copy, Debug)]
struct Fold {
    pivot: DVec3,
    axis: DVec3,
    side: DVec3,
    angle: f64,
}

impl Fold {
    fn turn(&self) -> DQuat {
        DQuat::from_axis_angle(self.axis.normalize(), self.angle)
    }

    fn reaches(&self, p: DVec3) -> bool {
        (p - self.pivot).dot(self.side) > 0.0
    }

    fn point(&self, p: DVec3) -> DVec3 {
        if self.reaches(p) {
            self.pivot + self.turn() * (p - self.pivot)
        } else {
            p
        }
    }
}

/// `p` and its normal `n` bent by `folds` in turn (each judged where the point lies
/// before it).
fn bend(folds: &[Fold], mut p: DVec3, mut n: DVec3) -> (DVec3, DVec3) {
    for fold in folds {
        if fold.reaches(p) {
            p = fold.point(p);
            n = fold.turn() * n;
        }
    }
    (p, n)
}

/// A kit's meshes bent by `folds`.
fn folded(kit: Kit<Surface>, folds: &[Fold]) -> super::tank_kit::KitMeshes<Surface> {
    let mut meshes = kit.finish_scaled(uv_per_metre);
    for (_, mesh) in &mut meshes {
        let mut bent: Mesh = (**mesh).clone();
        for (position, normal) in bent.positions.iter_mut().zip(&mut bent.normals) {
            let (p, n) = bend(
                folds,
                DVec3::from_array(position.map(f64::from)),
                DVec3::from_array(normal.map(f64::from)),
            );
            *position = p.as_vec3().to_array();
            *normal = n.as_vec3().to_array();
        }
        *mesh = Arc::new(bent);
    }
    meshes
}

/// A kit's meshes dented ([`dent`]) and bent by `folds`, each triangle shaded flat so
/// the creases and dents catch the light.
fn crumpled(kit: Kit<Surface>, folds: &[Fold]) -> super::tank_kit::KitMeshes<Surface> {
    let mut meshes = kit.finish_scaled(uv_per_metre);
    for (_, mesh) in &mut meshes {
        let mut bent: Mesh = (**mesh).clone();
        for position in &mut bent.positions {
            let p = DVec3::from_array(position.map(f64::from));
            let (p, _) = bend(folds, dent(p), DVec3::Y);
            *position = p.as_vec3().to_array();
        }
        flat_normals(&mut bent);
        *mesh = Arc::new(bent);
    }
    meshes
}

/// Each triangle's normal its face's, kept on the side it faced.
fn flat_normals(mesh: &mut Mesh) {
    let points = mesh.positions.clone();
    for (corners, normals) in points
        .as_chunks::<3>()
        .0
        .iter()
        .zip(mesh.normals.as_chunks_mut::<3>().0)
    {
        let at = |i: usize| DVec3::from_array(corners[i].map(f64::from));
        let face = (at(1) - at(0)).cross(at(2) - at(0));
        if face.length_squared() < 1e-12 {
            continue;
        }
        let was = DVec3::from_array(normals[0].map(f64::from));
        let face = face.normalize() * if face.dot(was) < 0.0 { -1.0 } else { 1.0 };
        *normals = [face.as_vec3().to_array(); 3];
    }
}

/// Where a landed roof's sheet is dented in or bulged out where it struck: a smooth
/// field over the plan, pushed along the slope's normal, none at the eave band.
fn dent(p: DVec3) -> DVec3 {
    let (ex, ez) = (CABIN_X + ROOF_OVERHANG, CABIN_Z + ROOF_OVERHANG);
    let top = CABIN_TOP + ROOF_EDGE;
    if p.y < top - 2.0 * ROOF_SHEATHING || p.x.abs() > ex + 0.1 || p.z.abs() > ez + 0.1 {
        return p;
    }
    let ridge = ex - ez;
    let normal = if p.z.abs() >= p.x.abs() - ridge {
        DVec3::new(0.0, 1.0, p.z.signum() * ROOF_PITCH)
    } else {
        DVec3::new(p.x.signum() * ROOF_PITCH, 1.0, 0.0)
    }
    .normalize();
    let field = (1.7 * p.x + 0.3).sin() * (2.3 * p.z + 1.1).cos()
        + 0.6 * (3.1 * p.x - 2.0 * p.z + 0.7).sin()
        - 0.5;
    p + normal * ROOF_DENT * field
}

/// The landed roof's sheet as a fine shell: each slope a grid of facets from the eave
/// to the ridge or hip, and the fascia band round it. `inside`, the boarding under it
/// instead: the same facets a sheathing's depth in, facing in, so an overturned roof
/// shows the same creases and dents from beneath.
#[allow(clippy::too_many_arguments, reason = "the roof's measures, as `hip_roof` has them")]
fn roof_shell(
    kit: &mut Kit<Surface>,
    ex: f64,
    ez: f64,
    ridge_x: f64,
    eave: f64,
    top: f64,
    apex: f64,
    inside: bool,
) {
    let centre = DVec3::new(0.0, eave, 0.0);
    let facet = |kit: &mut Kit<Surface>, corners: [DVec3; 4]| {
        let middle = corners.iter().copied().sum::<DVec3>() / 4.0;
        let face = (corners[1] - corners[0]).cross(corners[2] - corners[0]);
        let facing = if face.dot(middle - centre) < 0.0 { -face } else { face };
        if inside {
            let inward = -facing.normalize();
            kit.quad(Surface::Roof, corners.map(|c| c + inward * ROOF_SHEATHING), inward);
        } else {
            kit.quad(Surface::Roof, corners, facing);
        }
    };
    // A slope from its eave edge `a`-`b` up to its ridge edge `c`-`d`.
    let slope = |kit: &mut Kit<Surface>, a: DVec3, b: DVec3, c: DVec3, d: DVec3, columns: usize| {
        let at = |u: f64, v: f64| a.lerp(b, u).lerp(d.lerp(c, u), v);
        for i in 0..columns {
            for j in 0..ROOF_ROWS {
                let (u0, u1) = (i as f64 / columns as f64, (i + 1) as f64 / columns as f64);
                let (v0, v1) = (j as f64 / ROOF_ROWS as f64, (j + 1) as f64 / ROOF_ROWS as f64);
                facet(kit, [at(u0, v0), at(u1, v0), at(u1, v1), at(u0, v1)]);
            }
        }
    };
    let p = |x: f64, y: f64, z: f64| DVec3::new(x, y, z);
    slope(kit, p(-ex, top, ez), p(ex, top, ez), p(ridge_x, apex, 0.0), p(-ridge_x, apex, 0.0), ROOF_COLUMNS);
    slope(kit, p(ex, top, -ez), p(-ex, top, -ez), p(-ridge_x, apex, 0.0), p(ridge_x, apex, 0.0), ROOF_COLUMNS);
    slope(kit, p(ex, top, ez), p(ex, top, -ez), p(ridge_x, apex, 0.0), p(ridge_x, apex, 0.0), ROOF_ROWS);
    slope(kit, p(-ex, top, -ez), p(-ex, top, ez), p(-ridge_x, apex, 0.0), p(-ridge_x, apex, 0.0), ROOF_ROWS);
    // The fascia band, seen from outside or from within.
    let corners = [p(ex, 0.0, ez), p(ex, 0.0, -ez), p(-ex, 0.0, -ez), p(-ex, 0.0, ez)];
    for i in 0..4 {
        let (a, b) = (corners[i], corners[(i + 1) % 4]);
        let outward = ((a + b) / 2.0).with_y(0.0).normalize();
        let (shift, facing) = if inside {
            (-outward * ROOF_SHEATHING, -outward)
        } else {
            (DVec3::ZERO, outward)
        };
        kit.quad(
            Surface::Roof,
            [a.with_y(eave), b.with_y(eave), b.with_y(top), a.with_y(top)].map(|c| c + shift),
            facing,
        );
    }
}

/// How a landed roof is crumpled: its east end folded down across the ridge where it
/// struck, and its front eave bent under.
fn roof_folds() -> [Fold; 2] {
    let ez = CABIN_Z + ROOF_OVERHANG;
    let eave = ez - EAVE_BEND_AT;
    [
        Fold {
            pivot: DVec3::new(ROOF_CREASE_X, CABIN_TOP, 0.0),
            axis: DVec3::Z,
            side: DVec3::X,
            angle: -ROOF_FOLD,
        },
        Fold {
            pivot: DVec3::new(0.0, CABIN_TOP + ROOF_EDGE + ROOF_PITCH * EAVE_BEND_AT, eave),
            axis: DVec3::X,
            side: DVec3::Z,
            angle: EAVE_BEND,
        },
    ]
}

/// A sheet torn back off a long slope (`side` -1 the back, +1 the front) between `x0`
/// and `x1`, curled up over itself, and the darker boards bared under it.
fn torn_sheet(kit: &mut Kit<Surface>, side: f64, x0: f64, x1: f64) {
    let ez = CABIN_Z + ROOF_OVERHANG;
    let slope = ROOF_PITCH.atan();
    // Up the slope toward the ridge, and out of it.
    let up = DVec3::new(0.0, slope.sin(), -side * slope.cos());
    let out = DVec3::new(0.0, slope.cos(), side * slope.sin());
    let surface = |run: f64| {
        DVec3::new(0.0, CABIN_TOP + ROOF_EDGE + ROOF_PITCH * run, side * (ez - run))
    };
    let width = x1 - x0;
    let middle = DVec3::X * (x0 + x1) / 2.0;
    let hinge = surface(TORN_SHEET_LENGTH) + middle + out * 0.02;
    // The bared deck: dark boards across the gap.
    for k in 0..4 {
        let run = TORN_SHEET_LENGTH * (f64::from(k) + 0.5) / 4.0;
        let at = surface(run) + middle + out * 0.012;
        kit.block(
            Surface::Splinters,
            DVec3::new(width - 0.04, 0.02, TORN_SHEET_LENGTH / 4.0 - 0.03),
            DMat4::from_translation(at) * DMat4::from_rotation_x(side * slope),
        );
    }
    // The sheet: hinged at its top, peeled back over in two bends.
    let mut from = hinge;
    for (k, turn) in TORN_SHEET_CURL.into_iter().enumerate() {
        let length = TORN_SHEET_LENGTH / 2.0;
        let direction = DQuat::from_axis_angle(DVec3::X, -side * turn) * -up;
        let to = from + direction * length;
        kit.block(
            Surface::Roof,
            DVec3::new(width - 0.02 * k as f64, 0.02, length),
            aim(from, to) * DMat4::from_translation(DVec3::Z * (length / 2.0)),
        );
        from = to;
    }
}

/// Where a landed roof struck the ground: bare steel scraped through its paint in
/// streaks down both long slopes, so a slope lying face up reads as battered sheet.
fn scrapes(kit: &mut Kit<Surface>) {
    let (ex, ez) = (CABIN_X + ROOF_OVERHANG, CABIN_Z + ROOF_OVERHANG);
    let slope = ROOF_PITCH.atan();
    let mut rng = Random::new(5.3);
    for side in [-1.0, 1.0] {
        for _ in 0..ROOF_SCRAPES {
            let x = rng.range(-ex + 0.4, ex - 0.4);
            let run = rng.range(0.3, ez - 0.3);
            let z = side * (ez - run);
            let y = CABIN_TOP + ROOF_EDGE + ROOF_PITCH * run + 0.012;
            let tilt = DMat4::from_rotation_x(side * slope);
            let turn = DMat4::from_rotation_y(rng.range(-0.5, 0.5));
            kit.block(
                Surface::Metal,
                DVec3::new(rng.range(0.6, 1.5), 0.012, rng.range(0.08, 0.2)),
                DMat4::from_translation(DVec3::new(x, y, z)) * tilt * turn,
            );
        }
    }
}

/// A landed roof's rafters under its boarding: up each slope from the eave to the
/// ridge or hip, the four hips and the ridge, each following the sheet's dents and
/// folds, snapped where the sheet is folded and some part way along.
fn roof_underside(group: &mut Node, seed: i32, folds: &[Fold]) {
    let (ex, ez) = (CABIN_X + ROOF_OVERHANG, CABIN_Z + ROOF_OVERHANG);
    let ridge_x = ex - ez;
    let top = CABIN_TOP + ROOF_EDGE;
    let apex = roof_apex();
    let mut next = seeds(seed);
    let mut rng = Random::new(f64::from(seed) * 4.31 + 0.7);
    // A rafter under the sheet between two points on it (unbent), `normal` out of its
    // slope.
    let mut rafter = |group: &mut Node, from: DVec3, to: DVec3, normal: DVec3| {
        let length = from.distance(to);
        let mut cuts = vec![0.0, 1.0];
        for fold in folds {
            let (a, b) = (
                (from - fold.pivot).dot(fold.side),
                (to - fold.pivot).dot(fold.side),
            );
            if a * b < 0.0 {
                cuts.push(a / (a - b));
            }
        }
        if rng.range(0.0, 1.0) < RAFTERS_SNAPPED {
            cuts.push(rng.range(0.3, 0.7));
        }
        cuts.sort_by(f64::total_cmp);
        let under = |t: f64| {
            let (p, n) = bend(folds, dent(from.lerp(to, t)), normal);
            (p - n * (ROOF_SHEATHING + RAFTER_DEPTH / 2.0), n)
        };
        let gap = RAFTER_GAP / length;
        for pair in cuts.windows(2) {
            let t0 = if pair[0] > 0.0 { pair[0] + gap } else { 0.0 };
            let t1 = if pair[1] < 1.0 { pair[1] - gap } else { 1.0 };
            if (t1 - t0) * length < 0.15 {
                continue;
            }
            // In straight lengths short enough to follow the dents.
            let pieces = ((t1 - t0) * length / RAFTER_PIECE).ceil().max(1.0);
            for k in 0..pieces as usize {
                let (a, _) = under(t0 + (t1 - t0) * k as f64 / pieces);
                let (b, _) = under(t0 + (t1 - t0) * (k + 1) as f64 / pieces);
                let (_, n) = under(t0 + (t1 - t0) * (k as f64 + 0.5) / pieces);
                group
                    .children
                    .push(member(a, b, 0.08, RAFTER_DEPTH, n, next(), ROOF_RAFTERS));
            }
        }
    };
    let up = |run: f64| top + ROOF_PITCH * run;
    // Up the long slopes, each to the ridge or the hip it meets.
    let columns = (2.0 * ex / RAFTER_SPACING).floor() as usize;
    for side in [-1.0, 1.0] {
        let normal = DVec3::new(0.0, 1.0, side * ROOF_PITCH).normalize();
        for i in 0..=columns {
            let x = -ex + 0.15 + (2.0 * ex - 0.3) * i as f64 / columns as f64;
            let run = ez.min(ex - x.abs()) - 0.08;
            if run > 0.3 {
                let from = DVec3::new(x, top, side * ez);
                let to = DVec3::new(x, up(run), side * (ez - run));
                rafter(group, from, to, normal);
            }
        }
    }
    // Up the hip ends.
    let rows = (2.0 * ez / RAFTER_SPACING).floor() as usize;
    for side in [-1.0, 1.0] {
        let normal = DVec3::new(side * ROOF_PITCH, 1.0, 0.0).normalize();
        for i in 1..rows {
            let z = -ez + 2.0 * ez * i as f64 / rows as f64;
            let run = ez - z.abs() - 0.08;
            if run > 0.3 {
                let from = DVec3::new(side * ex, top, z);
                let to = DVec3::new(side * (ex - run), up(run), z);
                rafter(group, from, to, normal);
            }
        }
    }
    // The hips and the ridge.
    for (sx, sz) in [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
        let normal = DVec3::new(sx * ROOF_PITCH, 2.0, sz * ROOF_PITCH).normalize();
        let from = DVec3::new(sx * ex, top, sz * ez);
        rafter(group, from, DVec3::new(sx * ridge_x, apex, 0.0), normal);
    }
    rafter(
        group,
        DVec3::new(-ridge_x, apex, 0.0),
        DVec3::new(ridge_x, apex, 0.0),
        DVec3::Y,
    );
}

/// The front, back, west and east walls' frames and half lengths ([`WALLS`] order).
fn cabin_frames() -> [(DMat4, f64); 4] {
    let [front, back, east, west] = wall_frames(2.0 * CABIN_X, 2.0 * CABIN_Z);
    [front, back, west, east]
}

/// The front wall's door opening onto the walkway.
fn entry_opening() -> Opening {
    Opening {
        u: DOOR_U,
        y: DECK_TOP,
        width: DOOR_WIDTH,
        height: DOOR_HEIGHT,
    }
}

/// A cabin window at `u` along its wall.
fn window_opening(u: f64) -> Opening {
    Opening {
        u,
        y: WINDOW_SILL,
        width: WINDOW_WIDTH,
        height: WINDOW_HEIGHT,
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

/// A concrete footing at `side` with its steel shoes, and in each shoe whose leg is
/// gone the stub it left: a tall ragged scorched one where shells cut it, a short
/// splintered one leaning the way the tower fell where the fall tore it out.
fn footing(
    group: &mut Node,
    kit: &mut Kit<Surface>,
    side: f64,
    scars: &Scars,
    next: &mut impl FnMut() -> i32,
) {
    let x = side * TOWER_BASE.offset;
    put(
        group,
        concrete_wall(TOWER_BASE.width, TOWER_BASE.height, TOWER_BASE.depth),
        x,
        TOWER_BASE.height / 2.0,
        0.0,
    );
    shoes(kit, x);
    let fall = scars.fall();
    for (corner, &(sx, sz)) in CORNERS.iter().enumerate() {
        if sx != side || !scars.broken.contains(LOWER_LEGS[corner]) {
            continue;
        }
        let z = sz * TOWER_BASE.post_z;
        let base = TOWER_BASE.height + 0.03;
        let seed = next();
        if scars.struck.contains(LOWER_LEGS[corner]) {
            // Cut by shells at shell height: up to the bite, charred black and torn.
            let stub = Member {
                from: DVec3::new(x, base, z),
                to: DVec3::new(x, BITE_Y + SHOT_STUB_OVER, z),
                width: STUB,
                depth: STUB,
                normal: DVec3::X,
            };
            snapped(group, &stub, [false, true], seed, CHARRED);
            for (k, &paint) in SCORCH_FADE.iter().enumerate() {
                put(
                    group,
                    timber_member(DVec3::new(STUB + 0.008, 0.05, STUB + 0.008), seed + 3 + k as i32, paint),
                    x,
                    BITE_Y + SHOT_STUB_OVER - 0.03 - 0.05 * k as f64,
                    z,
                );
            }
        } else {
            // Torn out by the fall: split in two and bent over the way the tower went.
            let top = TOWER_BASE.height + STUB_REACH;
            let lean = DVec3::new(fall.x, 0.0, fall.y) * TORN_STUB_LEAN;
            let across = DVec3::new(-fall.y, 0.0, fall.x) * STUB / 4.0;
            for (k, side) in [-1.0, 1.0].into_iter().enumerate() {
                let sliver = Member {
                    from: DVec3::new(x, base, z) + across * side,
                    to: DVec3::new(x, top, z) + across * side * 1.6 + lean * (1.0 - 0.3 * k as f64),
                    width: STUB / 2.0,
                    depth: STUB,
                    normal: DVec3::new(-fall.y, 0.0, fall.x),
                };
                snapped(group, &sliver, [false, true], seed + 11 * k as i32, TOWER_POST);
            }
        }
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

/// The length of a deck half `side` from `from` to `to` out from the middle: main and
/// outrigger beams, joists, rim boards and planks. A landed piece (`ragged`, its seed)
/// broke across its planks at [`DECK_CUT`]: each plank snapped at a length of its own,
/// the beams and rims splintered there.
fn deck_piece(group: &mut Node, side: f64, from: f64, to: f64, ragged: Option<i32>) {
    let mut next = seeds(800 + js_round(side * 100.0) as i32 + ragged.unwrap_or(0));
    let mut rng = Random::new(f64::from(ragged.unwrap_or(0)) * 3.7 + 1.9);
    let cut_low = from > 0.0;
    let cut_high = to < DECK_X;
    // A run along x from `a` to `b` (out from the middle) clipped to the piece.
    let span = |a: f64, b: f64| {
        let (a, b) = (a.max(from), b.min(to));
        (b > a + 0.02).then_some((side * a, side * b))
    };
    let along_x = |group: &mut Node, a: f64, b: f64, y: f64, z: f64, width: f64, depth: f64, color: u32, seed: i32| {
        if let Some((x0, x1)) = span(a, b) {
            let run = Member {
                from: DVec3::new(x0, y, z),
                to: DVec3::new(x1, y, z),
                width,
                depth,
                normal: DVec3::Z,
            };
            // Broken where it was cut: its ends at the cut, toward the middle or out.
            let at_cut = |x: f64| (x.abs() - DECK_CUT).abs() < 0.03;
            let broken = [
                ragged.is_some() && at_cut(x0),
                ragged.is_some() && at_cut(x1),
            ];
            if broken.iter().any(|&b| b) {
                snapped(group, &run, broken, seed, color);
            } else {
                group.children.push(framed(&run, seed, color));
            }
        }
    };
    for edge in [-1.0, 1.0] {
        let beam_y = POST_TOP + BEAM / 2.0;
        along_x(group, 0.01, DECK_X - 0.05, beam_y, edge * TOWER_BASE.post_z, BEAM, BEAM, TOWER_POST, next());
        along_x(group, 0.01, DECK_X - 0.2, OUTRIGGER_Y, edge * (DECK_Z - 0.45), 0.18, 0.18, TOWER_POST, next());
    }
    let joists = 11;
    let joist_y = POST_TOP + BEAM + JOIST_DEPTH / 2.0;
    for i in 0..joists {
        let x = -DECK_X + 0.1 + (2.0 * DECK_X - 0.2) * f64::from(i) / f64::from(joists - 1);
        let seed = next();
        if !on_half(x, side) || x.abs() < from || x.abs() >= to {
            continue;
        }
        group.children.push(member(
            DVec3::new(x, joist_y, -DECK_Z + 0.04),
            DVec3::new(x, joist_y, DECK_Z - 0.04),
            JOIST_DEPTH,
            0.07,
            DVec3::X,
            seed,
            TOWER_POST,
        ));
    }
    let rim_y = DECK_TOP - 0.13;
    for edge in [-1.0, 1.0] {
        along_x(group, 0.01, DECK_X, rim_y, edge * (DECK_Z - 0.02), 0.26, 0.05, TOWER_DECK, next());
    }
    let seed = next();
    if to >= DECK_X {
        group.children.push(member(
            DVec3::new(side * (DECK_X - 0.02), rim_y, -DECK_Z + 0.05),
            DVec3::new(side * (DECK_X - 0.02), rim_y, DECK_Z - 0.05),
            0.26,
            0.05,
            DVec3::X,
            seed,
            TOWER_DECK,
        ));
    }
    let planks = 33;
    let pitch = 2.0 * DECK_Z / f64::from(planks);
    for i in 0..planks {
        let z = -DECK_Z + pitch * (f64::from(i) + 0.5);
        // Each plank broke at a length of its own; a few are gone.
        let snap = DECK_CUT + rng.range(-PLANK_SNAP, PLANK_SNAP);
        let missing = rng.next() < PLANKS_LOST;
        let seed = next();
        let (a, b) = match ragged {
            None => (0.01, DECK_X - 0.01),
            Some(_) if missing => continue,
            Some(_) if cut_low => (snap, DECK_X - 0.01),
            Some(_) if cut_high => (0.01, snap),
            Some(_) => (0.01, DECK_X - 0.01),
        };
        let length = b - a;
        put(
            group,
            timber_member(DVec3::new(length, PLANK, pitch - 0.012), seed, TOWER_DECK),
            side * (a + length / 2.0),
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
/// A `torn` wall came apart from the cabin on landing: its door and shutters are torn
/// off (pieces of their own) and its glass is knocked out.
fn cabin_wall(kit: &mut Kit<Surface>, wall_index: usize, torn: bool) {
    let (frame, half) = cabin_frames()[wall_index];
    let front = wall_index == 0;
    let glazing = Glazing { double_hung: false };
    let centres: Vec<f64> = if front {
        vec![-1.25, 0.05]
    } else if half > 2.0 {
        vec![-1.3, 0.0, 1.3]
    } else {
        vec![-0.8, 0.8]
    };
    let windows: Vec<Opening> = centres.into_iter().map(window_opening).collect();
    let entry = entry_opening();
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
        if !torn {
            awning(kit, frame, opening);
        }
    }
    if front && !torn {
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
/// A `landed` roof has lost its soffit (its rafters show, `roof_underside`) and its
/// anemometer is bent over on a snapped mast.
fn hip_roof(kit: &mut Kit<Surface>, landed: bool) {
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
    if landed {
        // A shell fine enough to crease and dent (`crumple`).
        roof_shell(kit, ex, ez, ridge_x, eave, top, apex, false);
    } else {
        kit.solid(
            Surface::Roof,
            &[
                rect(ex, ez, eave),
                rect(ex, ez, top),
                rect(ridge_x, 0.0, apex),
            ],
        );
    }
    if !landed {
        kit.block(
            Surface::Trim,
            DVec3::new(2.0 * ex - 0.02, 0.01, 2.0 * ez - 0.02),
            DMat4::from_translation(DVec3::new(0.0, eave - 0.005, 0.0)),
        );
    }
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
    if landed {
        // The mast bent over at its foot and again halfway, its cups gone.
        let bend = DVec3::new(-0.15, apex + 0.45, 0.3);
        kit.rod(Surface::Iron, 0.025, DVec3::new(0.0, apex, 0.0), bend, 6);
        kit.rod(Surface::Iron, 0.025, bend, bend + DVec3::new(-0.65, 0.2, 0.35), 6);
        return;
    }
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
