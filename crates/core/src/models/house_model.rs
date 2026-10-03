//! The village cottage: a fieldstone plinth under painted clapboard walls whose
//! windows and door are real openings (recessed double-hung sashes, louvred
//! shutters where they fit, a panelled door under a bracketed canopy), corner
//! boards and a water table, and a gable roof with eave and rake overhangs,
//! fascia, bargeboards, stepped shingle courses, a ridge cap, gutters and
//! downspouts, plus a corbelled brick chimney with a clay pot. Window boxes
//! carry soil, shrubs and flowers.
//!
//! The cover's box (`w` along x, `d` along z, `h` to the ridge) is the collision
//! footprint; only eaves, sills, steps and planters reach past the walls. The
//! door faces +z under a gable; the ridge runs along z.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::{DMat4, DVec2, DVec3};

use super::building_kit::{
    CASING, Glazing, Opening, Palette, Surface, add_building, door, matte, shutter_width, shutters,
    uv_per_metre, wall, window,
};
use super::house_surfaces::{HouseSurface, house_material};
use super::model_primitives::{Cache, put, shadowed};
use super::tank_kit::{Kit, aim, pose};
use crate::geometry::math::{js_round, scale_hex_color};
use crate::geometry::{Mesh, octahedron_geometry};
use crate::scene::Node;
use crate::sim::math::Random;

/// Height of the fieldstone plinth (two 21 cm steps up to the door).
const PLINTH: f64 = 0.42;
/// Depth of the window and door reveals behind the wall face.
const REVEAL: f64 = 0.14;
/// Corner boards: width on each face and how far they stand proud.
const CORNER_BOARD: f64 = 0.13;
const TRIM_PROUD: f64 = 0.028;
const WINDOW_WIDTH: f64 = 0.85;
const DOOR_WIDTH: f64 = 0.9;
const DOOR_HEIGHT: f64 = 2.0;
/// Clear wall kept between neighbouring casings and shutters.
const TRIM_GAP: f64 = 0.02;
/// Shutters narrower than this are left off rather than squeezed.
const MIN_SHUTTER: f64 = 0.3;
/// Horizontal roof overhang past the side walls and past the gable walls.
const EAVE_OVERHANG: f64 = 0.4;
const RAKE_OVERHANG: f64 = 0.3;
/// Roof deck thickness, measured square to the slope.
const ROOF_THICKNESS: f64 = 0.1;
/// Shingle exposure the courses aim for, and courses per shingle tile.
const COURSE: f64 = 0.24;
const SHINGLE_ROWS: f64 = 8.0;
/// How far shingles run past the deck at the eave.
const SHINGLE_EAVE: f64 = 0.04;
/// Lift of a course's butt and of its head above the deck: each butt stands
/// proud of the course below it and casts the shadow line real shingles do.
const BUTT_LIFT: f64 = 0.024;
const HEAD_LIFT: f64 = 0.006;
/// Ridge cap: half width across the ridge and the length of one cap shingle.
const RIDGE_HALF: f64 = 0.16;
const RIDGE_PIECE: f64 = 0.3;
/// Chimney: plan size of the stack, height of the brick above the ridge, and
/// the pot's mouth above the ridge, where the smoke leaves.
const CHIMNEY: f64 = 0.62;
const CHIMNEY_TOP: f64 = 0.3;
const POT_MOUTH: f64 = 0.7;

const LEAVES: u32 = 0x3d6b3a;
const LEAVES_LIGHT: u32 = 0x5a8443;
const FLOWERS: [u32; 4] = [0xd9837a, 0xf0cf6a, 0xb59ad0, 0xf2efe6];
const SIDING_PAINTS: [u32; 5] = [0xe0d6bf, 0x9a4436, 0x8a9a7b, 0x7d93a3, 0xcfae6a];
const ACCENT_PAINTS: [u32; 5] = [0x2f4b3a, 0x5c2b27, 0x2c3d55, 0x3a3836, 0x6b4a2a];
const TRIM_PAINT: u32 = 0xeae2cf;
const BRIGHT_TRIM: u32 = 0xf4f1ea;

/// The chimney pot's mouth relative to the cottage's ground centre, where the
/// village's chimney smoke rises.
pub fn chimney_flue(w: f64, d: f64, h: f64) -> DVec3 {
    DVec3::new(-w * 0.25, h + POT_MOUTH, -d * 0.2)
}

/// Stable per-cottage variation from its position.
fn variant(x: f64, z: f64) -> u32 {
    (js_round(x * 7.0).abs() as u32)
        .wrapping_mul(31)
        .wrapping_add(js_round(z * 13.0).abs() as u32)
}

/// Paints of the cottage at `x, z`: one of the village paints or the cover's own
/// stained colour, a door and shutter colour, and the red or green roof of its
/// side of the village with gentle weathering.
fn palette(x: f64, z: f64, color: u32) -> Palette {
    let v = variant(x, z);
    let siding_choice = (v % (SIDING_PAINTS.len() as u32 + 1)) as usize;
    let siding = SIDING_PAINTS.get(siding_choice).copied().unwrap_or(color);
    let roof_base = if z.abs() > 35.0 { 0xb84a3c } else { 0x2f6b4f };
    Palette {
        siding,
        trim: if siding_choice == 0 {
            BRIGHT_TRIM
        } else {
            TRIM_PAINT
        },
        accent: ACCENT_PAINTS[(v / 7) as usize % ACCENT_PAINTS.len()],
        roof: scale_hex_color(
            roof_base,
            0.9 + 0.12 * (0.5 + 0.5 * (x * 3.7 + z * 1.9).sin()),
        ),
    }
}

/// Roof measurements shared by the deck, shingles, gutters and chimney.
struct Roof {
    /// Wall top (the eave line on the walls) and ridge height under the deck.
    wall: f64,
    ridge: f64,
    /// Slope rise per metre across.
    pitch: f64,
    /// Half span of the eaves along x and of the rakes along z.
    eave_x: f64,
    rake_z: f64,
    /// Underside height at the eave edge and vertical deck thickness.
    eave_y: f64,
    thick: f64,
}

impl Roof {
    fn new(w: f64, d: f64, h: f64) -> Self {
        let wall = h * 0.68;
        let pitch = (h - wall) / (w / 2.0);
        Self {
            wall,
            ridge: h,
            pitch,
            eave_x: w / 2.0 + EAVE_OVERHANG,
            rake_z: d / 2.0 + RAKE_OVERHANG,
            eave_y: wall - pitch * EAVE_OVERHANG,
            thick: ROOF_THICKNESS * (1.0 + pitch * pitch).sqrt(),
        }
    }

    /// Height of the deck's top surface at `x`.
    fn top(&self, x: f64) -> f64 {
        self.ridge + self.thick - self.pitch * x.abs()
    }
}

static OCTAHEDRON: Cache<(), Mesh> = Cache::new();

/// `house(group, c)`: the cottage for a cover box, its paints chosen from its
/// position.
pub(super) fn house(group: &mut Node, x: f64, z: f64, w: f64, d: f64, h: f64, color: u32) {
    let palette = palette(x, z, color);
    let antenna = (variant(x, z) / 5).is_multiple_of(3);
    let roof = Roof::new(w, d, h);
    let mut kit = Kit::new();
    build_house(&mut kit, w, d, &roof, antenna);
    // Built per cottage rather than cached, so batching moves these meshes into
    // the cottage's merged parts instead of copying them.
    add_building(group, kit.finish_scaled(uv_per_metre), &palette);
    group.children.push(shadowed(
        Arc::new(shingle_courses(&roof)),
        house_material(HouseSurface::Shingles, palette.roof),
    ));
    window_plants(group, w, d, h, x, z);
}

/// The wall frames: front (+z, the door), back, right (+x) and left, each with its
/// half length along the wall.
fn wall_frames(w: f64, d: f64) -> [(DMat4, f64); 4] {
    [
        (pose(DVec3::new(0.0, 0.0, d / 2.0), DVec3::ZERO), w / 2.0),
        (
            pose(DVec3::new(0.0, 0.0, -d / 2.0), DVec3::new(0.0, PI, 0.0)),
            w / 2.0,
        ),
        (
            pose(
                DVec3::new(w / 2.0, 0.0, 0.0),
                DVec3::new(0.0, PI / 2.0, 0.0),
            ),
            d / 2.0,
        ),
        (
            pose(
                DVec3::new(-w / 2.0, 0.0, 0.0),
                DVec3::new(0.0, -PI / 2.0, 0.0),
            ),
            d / 2.0,
        ),
    ]
}

/// Window centres along a wall: two either side of the door or the middle on
/// the gable walls, one or two on the long side walls.
fn window_positions(gable_wall: bool, half: f64) -> Vec<f64> {
    if gable_wall {
        vec![-half * 0.54, half * 0.54]
    } else if half >= 2.75 {
        vec![-half * 0.48, half * 0.48]
    } else {
        vec![0.0]
    }
}

fn window_opening(u: f64, roof: &Roof) -> Opening {
    Opening {
        u,
        y: PLINTH + 0.9,
        width: WINDOW_WIDTH,
        height: (roof.wall - PLINTH - 1.25).min(1.3),
    }
}

/// The widest shutter that fits beside a window: clear of the corner boards, the
/// door casing (`inner_limit`) and the neighbouring window's shutter.
fn shutter_room(opening: &Opening, half: f64, inner_limit: f64, alone: bool) -> f64 {
    let casing_edge = opening.width / 2.0 + CASING + 0.01;
    let outer = half - CORNER_BOARD - (opening.u.abs() + casing_edge) - TRIM_GAP;
    let inner = if alone {
        outer
    } else {
        // Twin windows split the wall between them.
        let space = opening.u.abs() - casing_edge - TRIM_GAP - inner_limit;
        if inner_limit > 0.0 {
            space
        } else {
            space / 2.0
        }
    };
    let width = outer.min(inner).min(shutter_width(opening));
    if width >= MIN_SHUTTER { width } else { 0.0 }
}

fn build_house(kit: &mut Kit<Surface>, w: f64, d: f64, roof: &Roof, antenna: bool) {
    // Fieldstone plinth and the two steps up to the door.
    kit.chamfer_block(
        Surface::Stone,
        DVec3::new(w + 0.1, PLINTH, d + 0.1),
        0.03,
        DMat4::from_translation(DVec3::new(0.0, PLINTH / 2.0, 0.0)),
    );
    for (width, depth, top) in [(1.5, 0.62, PLINTH / 2.0), (1.3, 0.32, PLINTH)] {
        kit.chamfer_block(
            Surface::Stone,
            DVec3::new(width, PLINTH / 2.0, depth),
            0.02,
            DMat4::from_translation(DVec3::new(
                0.0,
                top - PLINTH / 4.0,
                d / 2.0 + 0.05 + depth / 2.0,
            )),
        );
    }
    let door_opening = Opening {
        u: 0.0,
        y: PLINTH,
        width: DOOR_WIDTH,
        height: DOOR_HEIGHT,
    };
    let door_edge = DOOR_WIDTH / 2.0 + CASING;
    for (index, (frame, half)) in wall_frames(w, d).into_iter().enumerate() {
        let gable_wall = index < 2;
        let front = index == 0;
        let mut outline = vec![
            DVec2::new(-half, PLINTH),
            DVec2::new(half, PLINTH),
            DVec2::new(half, roof.wall),
        ];
        if gable_wall {
            outline.push(DVec2::new(0.0, roof.ridge));
        }
        outline.push(DVec2::new(-half, roof.wall));
        let windows: Vec<Opening> = window_positions(gable_wall, half)
            .into_iter()
            .map(|u| window_opening(u, roof))
            .collect();
        let mut openings = windows.clone();
        if front {
            openings.push(door_opening);
        }
        let vent = gable_wall && roof.ridge - roof.wall > 1.0;
        let vent_opening = Opening {
            u: 0.0,
            y: roof.wall + (roof.ridge - roof.wall) * 0.3,
            width: 0.56,
            height: 0.34,
        };
        if vent {
            openings.push(vent_opening);
        }
        wall(kit, frame, &outline, &openings, REVEAL);
        let glazing = Glazing {
            cols: 2,
            rows: 2,
            double_hung: true,
        };
        for opening in &windows {
            window(kit, frame, opening, glazing, REVEAL);
            let inner_limit = if front { door_edge } else { 0.0 };
            let room = shutter_room(opening, half, inner_limit, windows.len() == 1);
            if room > 0.0 {
                shutters(kit, frame, opening, room);
            }
            if gable_wall {
                window_box(kit, frame, opening);
            }
        }
        if front {
            door(kit, frame, &door_opening, REVEAL);
            canopy(kit, frame, &door_opening);
            lantern(kit, frame, door_edge + 0.14, PLINTH + 1.55);
        }
        if vent {
            gable_vent(kit, frame, &vent_opening);
        }
        wall_trim(
            kit,
            frame,
            half,
            roof,
            gable_wall,
            front.then_some(door_edge),
        );
    }
    roof_deck(kit, roof);
    gutters(kit, w, d, roof);
    chimney(kit, w, d, roof, antenna);
}

/// Corner boards, the water table over the plinth (broken by the door) and, on
/// the side walls, the frieze board under the eaves.
fn wall_trim(
    kit: &mut Kit<Surface>,
    frame: DMat4,
    half: f64,
    roof: &Roof,
    gable_wall: bool,
    door_edge: Option<f64>,
) {
    let block = |kit: &mut Kit<Surface>, size: DVec3, at: DVec3| {
        kit.block(Surface::Trim, size, frame * DMat4::from_translation(at));
    };
    const TABLE: f64 = 0.14;
    let board_bottom = PLINTH + TABLE;
    // Gable-wall boards lap past the side walls' boards to close the corner.
    let (board, reach) = if gable_wall {
        (CORNER_BOARD + TRIM_PROUD, half + TRIM_PROUD)
    } else {
        (CORNER_BOARD, half)
    };
    for side in [-1.0, 1.0] {
        block(
            kit,
            DVec3::new(board, roof.wall - board_bottom, TRIM_PROUD),
            DVec3::new(
                side * (reach - board / 2.0),
                (roof.wall + board_bottom) / 2.0,
                TRIM_PROUD / 2.0,
            ),
        );
    }
    let mut runs = vec![(-reach - 0.01, reach + 0.01)];
    if let Some(edge) = door_edge {
        runs = vec![(-reach - 0.01, -edge), (edge, reach + 0.01)];
    }
    for (from, to) in runs {
        kit.chamfer_block(
            Surface::Trim,
            DVec3::new(to - from, TABLE, 0.04),
            0.012,
            frame
                * DMat4::from_translation(DVec3::new(
                    (from + to) / 2.0,
                    PLINTH + TABLE / 2.0,
                    0.02,
                )),
        );
    }
    if !gable_wall {
        block(
            kit,
            DVec3::new(2.0 * half, 0.18, 0.025),
            DVec3::new(0.0, roof.wall - 0.09, 0.0125),
        );
    }
}

/// A louvred gable vent: dark back, sloping slats and a plain casing.
fn gable_vent(kit: &mut Kit<Surface>, frame: DMat4, opening: &Opening) {
    let at = |p: DVec3| frame * DMat4::from_translation(p);
    kit.block(
        Surface::Dark,
        DVec3::new(opening.width, opening.height, 0.01),
        at(DVec3::new(
            opening.u,
            opening.y + opening.height / 2.0,
            -0.1,
        )),
    );
    let slats = 4;
    for i in 0..slats {
        let y = opening.y + opening.height * (f64::from(i) + 0.5) / f64::from(slats);
        kit.block(
            Surface::Trim,
            DVec3::new(opening.width, 0.012, 0.1),
            frame
                * pose(
                    DVec3::new(opening.u, y, -0.05),
                    DVec3::new(PI * 0.28, 0.0, 0.0),
                ),
        );
    }
    for side in [-1.0, 1.0] {
        kit.block(
            Surface::Trim,
            DVec3::new(0.08, opening.height + 0.16, TRIM_PROUD),
            at(DVec3::new(
                opening.u + side * (opening.width / 2.0 + 0.04),
                opening.y + opening.height / 2.0,
                TRIM_PROUD / 2.0,
            )),
        );
    }
    for y in [opening.y - 0.04, opening.y + opening.height + 0.04] {
        kit.block(
            Surface::Trim,
            DVec3::new(opening.width + 0.16, 0.08, TRIM_PROUD),
            at(DVec3::new(opening.u, y, TRIM_PROUD / 2.0)),
        );
    }
}

/// A stained window box on two brackets under a window's sill, its soil level
/// just below the rim. The plants are separate nodes ([`window_plants`]).
fn window_box(kit: &mut Kit<Surface>, frame: DMat4, opening: &Opening) {
    let (u, y) = (opening.u, opening.y);
    let width = opening.width + 0.2;
    kit.chamfer_block(
        Surface::Wood,
        DVec3::new(width, 0.2, 0.28),
        0.015,
        frame * DMat4::from_translation(DVec3::new(u, y - 0.29, 0.17)),
    );
    kit.block(
        Surface::Dark,
        DVec3::new(width - 0.06, 0.012, 0.22),
        frame * DMat4::from_translation(DVec3::new(u, y - 0.19, 0.17)),
    );
    for side in [-1.0, 1.0] {
        let x = u + side * (width / 2.0 - 0.12);
        let from = frame.transform_point3(DVec3::new(x, y - 0.62, 0.01));
        let to = frame.transform_point3(DVec3::new(x, y - 0.4, 0.25));
        let length = (to - from).length();
        kit.block(
            Surface::Wood,
            DVec3::new(0.04, 0.04, length),
            aim(from, to) * DMat4::from_translation(DVec3::Z * (length / 2.0)),
        );
        kit.block(
            Surface::Wood,
            DVec3::new(0.04, 0.24, 0.03),
            frame * DMat4::from_translation(DVec3::new(x, y - 0.51, 0.015)),
        );
    }
}

/// Shrubs and flowers in every window box, jittered per cottage.
fn window_plants(group: &mut Node, w: f64, d: f64, h: f64, x: f64, z: f64) {
    let roof = Roof::new(w, d, h);
    let octahedron = OCTAHEDRON.get_or_insert((), || octahedron_geometry(1.0, 0));
    let mut rng = Random::new(f64::from(variant(x, z)) + 17.0);
    let flower = FLOWERS[(variant(x, z) / 3) as usize % FLOWERS.len()];
    for (frame, half) in wall_frames(w, d).into_iter().take(2) {
        for u in window_positions(true, half) {
            let opening = window_opening(u, &roof);
            let width = opening.width + 0.08;
            let soil = opening.y - 0.18;
            // Overlapping leafy clumps along the box, flowers held above them.
            for i in 0..9 {
                let along = u - width / 2.0 + width * (f64::from(i) + 0.5) / 9.0;
                let local = DVec3::new(
                    along + rng.range(-0.03, 0.03),
                    soil + rng.range(0.03, 0.07),
                    0.17 + rng.range(-0.06, 0.06),
                );
                let leaves = if rng.next() < 0.5 {
                    LEAVES
                } else {
                    LEAVES_LIGHT
                };
                let mut clump = shadowed(octahedron.clone(), matte(leaves));
                clump.scale = DVec3::new(
                    rng.range(0.07, 0.1),
                    rng.range(0.07, 0.12),
                    rng.range(0.07, 0.1),
                );
                clump.set_rotation_euler(0.0, rng.range(0.0, PI), 0.0);
                let at = frame.transform_point3(local);
                put(group, clump, at.x, at.y, at.z);
            }
            for _ in 0..12 {
                let local = DVec3::new(
                    u + rng.range(-width / 2.0, width / 2.0),
                    soil + rng.range(0.1, 0.18),
                    0.17 + rng.range(-0.09, 0.09),
                );
                let mut bloom = shadowed(octahedron.clone(), matte(flower));
                let size = rng.range(0.028, 0.045);
                bloom.scale = DVec3::new(size, size * 0.7, size);
                let at = frame.transform_point3(local);
                put(group, bloom, at.x, at.y, at.z);
            }
        }
    }
}

/// A small gabled canopy over the door on two knee brackets.
fn canopy(kit: &mut Kit<Surface>, frame: DMat4, opening: &Opening) {
    const PROJECT: f64 = 0.62;
    const HALF: f64 = 0.7;
    const RISE: f64 = 0.3;
    const SLAB: f64 = 0.045;
    let base = opening.top() + 0.27;
    let at = |u: f64, y: f64, z: f64| frame.transform_point3(DVec3::new(u, y, z));
    for side in [-1.0, 1.0] {
        let ring = |z: f64| {
            vec![
                at(side * HALF, base, z),
                at(0.0, base + RISE, z),
                at(0.0, base + RISE + SLAB, z),
                at(side * HALF, base + SLAB, z),
            ]
        };
        kit.solid(Surface::Roof, &[ring(0.0), ring(PROJECT)]);
        // Fascia under each roof edge.
        kit.block(
            Surface::Trim,
            DVec3::new(0.03, 0.09, PROJECT),
            frame
                * DMat4::from_translation(DVec3::new(
                    side * (HALF - 0.04),
                    base - 0.03,
                    PROJECT / 2.0,
                )),
        );
    }
    // Pediment infill and its bottom board, set back from the roof edge.
    let pediment = |z: f64| {
        vec![
            at(-HALF + 0.06, base - 0.07, z),
            at(HALF - 0.06, base - 0.07, z),
            at(0.0, base + RISE - 0.04, z),
        ]
    };
    kit.solid(
        Surface::Trim,
        &[pediment(PROJECT - 0.08), pediment(PROJECT - 0.04)],
    );
    for side in [-1.0, 1.0] {
        let u = side * (HALF - 0.12);
        kit.block(
            Surface::Trim,
            DVec3::new(0.07, 0.42, 0.06),
            frame * DMat4::from_translation(DVec3::new(u, base - 0.25, 0.03)),
        );
        let from = at(u, base - 0.42, 0.04);
        let to = at(u, base - 0.06, PROJECT - 0.1);
        let length = (to - from).length();
        kit.block(
            Surface::Trim,
            DVec3::new(0.055, 0.07, length),
            aim(from, to) * DMat4::from_translation(DVec3::Z * (length / 2.0)),
        );
    }
}

/// A wall lantern: back plate, arm, glass box under a cap and finial.
fn lantern(kit: &mut Kit<Surface>, frame: DMat4, u: f64, y: f64) {
    let at = |p: DVec3| frame * DMat4::from_translation(p);
    kit.block(
        Surface::Iron,
        DVec3::new(0.09, 0.24, 0.02),
        at(DVec3::new(u, y, 0.01)),
    );
    kit.block(
        Surface::Iron,
        DVec3::new(0.03, 0.03, 0.14),
        at(DVec3::new(u, y + 0.06, 0.08)),
    );
    let center = DVec3::new(u, y - 0.04, 0.17);
    kit.block(Surface::Lamp, DVec3::new(0.12, 0.17, 0.12), at(center));
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        kit.block(
            Surface::Iron,
            DVec3::new(0.018, 0.19, 0.018),
            at(center + DVec3::new(sx * 0.06, 0.0, sz * 0.06)),
        );
    }
    kit.chamfer_block(
        Surface::Iron,
        DVec3::new(0.17, 0.05, 0.17),
        0.02,
        at(center + DVec3::new(0.0, 0.11, 0.0)),
    );
    kit.block(
        Surface::Iron,
        DVec3::new(0.14, 0.02, 0.14),
        at(center - DVec3::new(0.0, 0.095, 0.0)),
    );
    let top = frame.transform_point3(center + DVec3::new(0.0, 0.135, 0.0));
    kit.post(Surface::Iron, 0.012, 0.06, top, 6);
}

/// The roof deck (soffits, rake undersides and the eave ends), fascia boards,
/// bargeboards on both gables and a drip edge under the shingle eave.
///
/// The deck has no top face. The shingle courses cover all of it with their heads
/// only `HEAD_LIFT` above it, a few depth-buffer steps from the overhead camera, so
/// a top face would show through the course heads as pale streaks wherever it won
/// the depth test.
fn roof_deck(kit: &mut Kit<Surface>, roof: &Roof) {
    /// The section's side from the ridge top down to the eave top.
    const DECK_TOP: (usize, usize) = (0, 2);
    let (xe, ye, zr, t) = (roof.eave_x, roof.eave_y, roof.rake_z, roof.thick);
    for side in [-1.0, 1.0] {
        let section = |z: f64| {
            vec![
                DVec3::new(side * xe, ye, z),
                DVec3::new(0.0, roof.ridge, z),
                DVec3::new(0.0, roof.ridge + t, z),
                DVec3::new(side * xe, ye + t, z),
            ]
        };
        kit.solid_skipping(Surface::Trim, &[section(-zr), section(zr)], Some(DECK_TOP));
        // Fascia on the eave ends, standing below the soffit.
        kit.block(
            Surface::Trim,
            DVec3::new(0.035, t + 0.13, 2.0 * zr + 0.09),
            DMat4::from_translation(DVec3::new(
                side * (xe + 0.0175),
                ye + (t - 0.1 + 0.03) / 2.0,
                0.0,
            )),
        );
        // Bargeboards follow each rake just proud of the deck.
        for gable in [-1.0, 1.0] {
            let board = |z: f64| {
                vec![
                    DVec3::new(side * (xe + 0.035), ye - 0.1, z),
                    DVec3::new(0.0, roof.ridge - 0.1, z),
                    DVec3::new(0.0, roof.ridge + t + 0.03, z),
                    DVec3::new(side * (xe + 0.035), ye + t + 0.03, z),
                ]
            };
            kit.solid(
                Surface::Trim,
                &[board(gable * zr), board(gable * (zr + 0.045))],
            );
        }
    }
}

/// K-style gutters along both eaves, open-looking from above, and two
/// downspouts at opposite corners with kick-outs onto splash blocks.
fn gutters(kit: &mut Kit<Surface>, w: f64, d: f64, roof: &Roof) {
    let (xe, ye, zr) = (roof.eave_x, roof.eave_y, roof.rake_z);
    for side in [-1.0, 1.0] {
        let x = |offset: f64| side * (xe + offset);
        let profile = |z: f64| {
            vec![
                DVec3::new(x(0.035), ye + 0.02, z),
                DVec3::new(x(0.17), ye + 0.04, z),
                DVec3::new(x(0.15), ye - 0.07, z),
                DVec3::new(x(0.05), ye - 0.08, z),
            ]
        };
        kit.solid(Surface::Metal, &[profile(-zr - 0.02), profile(zr + 0.02)]);
        // A dark channel just above the gutter's top reads as the open trough.
        let lip = |offset: f64, y: f64, z: f64| DVec3::new(x(offset), ye + y, z);
        let z = zr - 0.01;
        kit.quad(
            Surface::Dark,
            [
                lip(0.05, 0.026, -z),
                lip(0.155, 0.04, -z),
                lip(0.155, 0.04, z),
                lip(0.05, 0.026, z),
            ],
            DVec3::Y,
        );
    }
    for (side, end) in [(1.0, 1.0), (-1.0, -1.0)] {
        let top = DVec3::new(side * (xe + 0.1), ye - 0.07, end * (zr - 0.18));
        let bend = DVec3::new(side * (w / 2.0 + 0.07), ye - 0.45, end * (d / 2.0 - 0.18));
        let foot = DVec3::new(bend.x, 0.32, bend.z);
        let spout = DVec3::new(side * (w / 2.0 + 0.32), 0.1, bend.z);
        for (from, to) in [(top, bend), (bend, foot), (foot, spout)] {
            kit.rod(Surface::Metal, 0.04, from, to, 8);
            kit.chamfer_block(
                Surface::Metal,
                DVec3::splat(0.095),
                0.025,
                DMat4::from_translation(to),
            );
        }
        for y in [1.2, 2.3] {
            kit.block(
                Surface::Metal,
                DVec3::new(0.11, 0.03, 0.11),
                DMat4::from_translation(DVec3::new(bend.x - side * 0.01, y, bend.z)),
            );
        }
        kit.chamfer_block(
            Surface::Concrete,
            DVec3::new(0.48, 0.05, 0.26),
            0.015,
            DMat4::from_translation(DVec3::new(side * (w / 2.0 + 0.36), 0.025, bend.z)),
        );
    }
}

/// A brick stack rising through the left slope with a corbelled top, a concrete
/// cap, lead flashing at the roof and a clay pot; some cottages strap a TV aerial
/// to it.
fn chimney(kit: &mut Kit<Surface>, w: f64, d: f64, roof: &Roof, antenna: bool) {
    let flue = chimney_flue(w, d, roof.ridge);
    let (cx, cz) = (flue.x, flue.z);
    let top = roof.ridge + CHIMNEY_TOP;
    let bottom = roof.ridge - roof.pitch * (cx.abs() + CHIMNEY);
    kit.block(
        Surface::Brick,
        DVec3::new(CHIMNEY, top - bottom, CHIMNEY),
        DMat4::from_translation(DVec3::new(cx, (top + bottom) / 2.0, cz)),
    );
    kit.chamfer_block(
        Surface::Brick,
        DVec3::new(CHIMNEY + 0.1, 0.1, CHIMNEY + 0.1),
        0.01,
        DMat4::from_translation(DVec3::new(cx, top + 0.05, cz)),
    );
    kit.chamfer_block(
        Surface::Concrete,
        DVec3::new(CHIMNEY + 0.18, 0.06, CHIMNEY + 0.18),
        0.015,
        DMat4::from_translation(DVec3::new(cx, top + 0.13, cz)),
    );
    let pot_base = DVec3::new(cx, top + 0.16, cz);
    let pot_height = flue.y - pot_base.y;
    kit.turned(
        Surface::Terracotta,
        &[
            (0.0, 0.0),
            (0.12, 0.0),
            (0.12, 0.05),
            (0.095, 0.09),
            (0.085, pot_height - 0.04),
            (0.105, pot_height - 0.03),
            (0.105, pot_height),
            (0.0, pot_height),
        ]
        .map(|(r, z)| DVec2::new(r, z)),
        12,
        aim(pot_base, pot_base + DVec3::Y),
    );
    kit.post(Surface::Dark, 0.075, 0.004, DVec3::new(cx, flue.y, cz), 12);
    // Flashing lies on the slope around the stack's base.
    kit.block(
        Surface::Metal,
        DVec3::new(CHIMNEY + 0.16, 0.02, CHIMNEY + 0.16),
        pose(
            DVec3::new(cx, roof.top(cx) + BUTT_LIFT, cz),
            DVec3::new(0.0, 0.0, roof.pitch.atan() * -cx.signum()),
        ),
    );
    if antenna {
        let mast = cx + CHIMNEY / 2.0 + 0.04;
        for y in [top - 0.25, top - 0.05] {
            kit.block(
                Surface::Metal,
                DVec3::new(CHIMNEY + 0.02, 0.025, CHIMNEY + 0.02),
                DMat4::from_translation(DVec3::new(cx, y, cz)),
            );
        }
        kit.post(
            Surface::Iron,
            0.02,
            1.45,
            DVec3::new(mast, top - 0.3, cz),
            6,
        );
        let boom_y = top + 1.0;
        kit.rod(
            Surface::Iron,
            0.012,
            DVec3::new(mast, boom_y, cz - 0.55),
            DVec3::new(mast, boom_y, cz + 0.55),
            5,
        );
        for i in 0..6 {
            let z = cz - 0.5 + f64::from(i) * 0.2;
            let half = 0.28 - f64::from(i) * 0.025;
            kit.rod(
                Surface::Iron,
                0.007,
                DVec3::new(mast - half, boom_y, z),
                DVec3::new(mast + half, boom_y, z),
                4,
            );
        }
    }
}

/// Collects textured quads with flat normals for the shingle courses.
#[derive(Default)]
struct Courses {
    positions: Vec<f64>,
    normals: Vec<f64>,
    uvs: Vec<f64>,
}

impl Courses {
    /// A quad `a b c d` facing roughly along `facing`, with its texture coordinates.
    fn quad(&mut self, corners: [DVec3; 4], uvs: [DVec2; 4], facing: DVec3) {
        let [a, b, c, _] = corners;
        let mut normal = (b - a).cross(c - a).normalize_or_zero();
        let order = if normal.dot(facing) < 0.0 {
            normal = -normal;
            [0, 2, 1, 0, 3, 2]
        } else {
            [0, 1, 2, 0, 2, 3]
        };
        for i in order {
            self.positions.extend(corners[i].to_array());
            self.normals.extend(normal.to_array());
            self.uvs.extend(uvs[i].to_array());
        }
    }
}

/// Both slopes' shingle courses from eave to ridge and the overlapping cap
/// shingles along the ridge. Each course's texture row matches its geometry: the
/// butt edge sits on the top of a row of `shingles.webp`.
fn shingle_courses(roof: &Roof) -> Mesh {
    let mut out = Courses::default();
    let z_end = roof.rake_z + 0.01;
    for side in [-1.0, 1.0] {
        let eave = DVec2::new(side * roof.eave_x, roof.eave_y + roof.thick);
        let ridge = DVec2::new(0.0, roof.ridge + roof.thick);
        let slope = (ridge - eave).length();
        let up = (ridge - eave) / slope;
        let normal = DVec2::new(-up.y, up.x);
        let normal = if normal.y < 0.0 { -normal } else { normal };
        let length = slope + SHINGLE_EAVE;
        let courses = (length / COURSE).round().max(1.0);
        let exposure = length / courses;
        let tile = exposure * SHINGLE_ROWS;
        let at = |along: f64, lift: f64, z: f64| {
            let p = eave + up * (along - SHINGLE_EAVE) + normal * lift;
            DVec3::new(p.x, p.y, z)
        };
        let facing = DVec3::new(normal.x, normal.y, 0.0);
        let down = DVec3::new(-up.x, -up.y, 0.0);
        for k in 0..courses as u32 {
            let k = f64::from(k);
            let (s0, s1) = (k * exposure, (k + 1.0) * exposure);
            let (v0, v1) = (k / SHINGLE_ROWS, (k + 1.0) / SHINGLE_ROWS);
            let (u0, u1) = (-z_end / tile, z_end / tile);
            out.quad(
                [
                    at(s0, BUTT_LIFT, -z_end),
                    at(s0, BUTT_LIFT, z_end),
                    at(s1, HEAD_LIFT, z_end),
                    at(s1, HEAD_LIFT, -z_end),
                ],
                [
                    DVec2::new(u0, v0),
                    DVec2::new(u1, v0),
                    DVec2::new(u1, v1),
                    DVec2::new(u0, v1),
                ],
                facing,
            );
            let below = if k == 0.0 { -0.01 } else { HEAD_LIFT };
            out.quad(
                [
                    at(s0, below, -z_end),
                    at(s0, below, z_end),
                    at(s0, BUTT_LIFT, z_end),
                    at(s0, BUTT_LIFT, -z_end),
                ],
                [
                    DVec2::new(u0, v0),
                    DVec2::new(u1, v0),
                    DVec2::new(u1, v0 + 0.004),
                    DVec2::new(u0, v0 + 0.004),
                ],
                down,
            );
        }
    }
    // Cap shingles lap along the ridge, butts facing +z like the walls' front.
    let pieces = (2.0 * z_end / RIDGE_PIECE).round().max(1.0);
    let piece = 2.0 * z_end / pieces;
    let crest = roof.ridge + roof.thick;
    let edge_y = roof.top(RIDGE_HALF);
    for j in 0..pieces as u32 {
        let j = f64::from(j);
        let (z1, z0) = (z_end - j * piece, z_end - (j + 1.0) * piece);
        let apex = |z: f64, lift: f64| DVec3::new(0.0, crest + 0.04 + lift, z);
        let edge = |side: f64, z: f64, lift: f64| DVec3::new(side * RIDGE_HALF, edge_y + lift, z);
        let (v0, v1) = (j / SHINGLE_ROWS, (j + 1.0) / SHINGLE_ROWS);
        let u = RIDGE_HALF / (piece * SHINGLE_ROWS);
        for side in [-1.0, 1.0] {
            let facing = DVec3::new(side * roof.pitch, 1.0, 0.0);
            out.quad(
                [
                    edge(side, z1, BUTT_LIFT + HEAD_LIFT),
                    apex(z1, BUTT_LIFT),
                    apex(z0, 0.0),
                    edge(side, z0, HEAD_LIFT),
                ],
                [
                    DVec2::new(side * u, v0),
                    DVec2::new(0.0, v0),
                    DVec2::new(0.0, v1),
                    DVec2::new(side * u, v1),
                ],
                facing,
            );
            out.quad(
                [
                    edge(side, z1, 0.0),
                    apex(z1, -0.03),
                    apex(z1, BUTT_LIFT),
                    edge(side, z1, BUTT_LIFT + HEAD_LIFT),
                ],
                [
                    DVec2::new(side * u, v0),
                    DVec2::new(0.0, v0),
                    DVec2::new(0.0, v0 + 0.004),
                    DVec2::new(side * u, v0 + 0.004),
                ],
                DVec3::Z,
            );
        }
    }
    Mesh::from_f64(&out.positions, &out.normals, &out.uvs, None)
}
