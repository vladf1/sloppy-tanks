//! Finishes and shared joinery of the village buildings (cottages and the
//! watchtower's lookout cabin): clapboard walls with real openings, recessed
//! double-hung windows with casings and sills, louvred shutters and panelled
//! doors. Parts merge per [`Surface`] through the tank [`Kit`], so a building
//! costs one draw per finish however much trim it carries.
//!
//! Walls are built in a frame whose local xy plane is the outside face, x along
//! the wall, y up and +z out of the building; openings are cut through that face
//! and lined with reveals, and the glass and doors sit back inside them.

use std::sync::Arc;

use glam::{DMat4, DVec2, DVec3};

use super::model_primitives::{Cache, material, shadowed};
use super::tank_kit::{Finishes, Kit, KitMeshes, pose};
use crate::geometry::math::{hex_to_linear, linear_to_hex};
use crate::scene::{Color, Material, Node, TextureRef};

/// Painted clapboard (`scripts/generate-house-textures.ts`): 12 courses of 20 cm
/// lap siding per tile, drawn near white so the house paint tints it.
pub const CLAPBOARD_TEXTURE: &str = "textures/houses/clapboard.webp";
/// Common bond brickwork: 16 courses of 75 mm per tile.
pub const BRICK_TEXTURE: &str = "textures/houses/brick.webp";
/// Mortared fieldstone of the cottage plinths and steps.
pub const STONE_TEXTURE: &str = "textures/houses/stone.webp";
/// Metres per texture tile.
const CLAPBOARD_TILE: f64 = 2.4;
const BRICK_TILE: f64 = 1.2;
const STONE_TILE: f64 = 1.6;
/// The average sRGB colour of the clapboard image: paint divided by it keeps
/// the painted wall at the paint's colour on average.
const CLAPBOARD_AVERAGE: u32 = 0xd7d3cc;
/// Kit texture density of untextured finishes (it only shades flat paint).
const PLAIN_UV_PER_METRE: f64 = 0.45;

const GLASS: u32 = 0x5a7184;
/// Net curtains drawn to the sides of cottage windows.
const CURTAIN: u32 = 0xebe4d3;
const DARK: u32 = 0x1e1b18;
const GALVANISED: u32 = 0x9ba3a6;
const IRON: u32 = 0x25272b;
const BRASS: u32 = 0xb08a3e;
const CONCRETE: u32 = 0xa9a59d;
const TERRACOTTA: u32 = 0xa0512f;
const STAINED_WOOD: u32 = 0x76512f;
const LAMP_GLASS: u32 = 0xf1d18f;

/// The finishes of a building, in draw order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Surface {
    /// Painted clapboard walls (textured, tinted by the body paint).
    Siding,
    /// Painted trim: casings, corner boards, fascia, soffits, sashes.
    Trim,
    /// Gloss paint of doors and shutters.
    Accent,
    /// Window glass, glossy enough to reflect the sky.
    Glass,
    /// Unlit openings, vent backs and soil.
    Dark,
    /// Galvanised gutters, downspouts and flashing.
    Metal,
    /// Black iron hardware, antenna and ladder rungs.
    Iron,
    /// Door furniture.
    Brass,
    /// Brick chimneys (textured).
    Brick,
    /// Fieldstone plinths and steps (textured).
    Stone,
    /// Chimney caps and pads.
    Concrete,
    /// Chimney pots.
    Terracotta,
    /// Painted roof metal: standing seams, canopies, ridge caps.
    Roof,
    /// Stained planters and props.
    Wood,
    /// Warm lantern glass.
    Lamp,
    /// Curtains behind cottage windows.
    Curtain,
}

impl Finishes for Surface {
    const ALL: &'static [Self] = &[
        Surface::Siding,
        Surface::Trim,
        Surface::Accent,
        Surface::Glass,
        Surface::Dark,
        Surface::Metal,
        Surface::Iron,
        Surface::Brass,
        Surface::Brick,
        Surface::Stone,
        Surface::Concrete,
        Surface::Terracotta,
        Surface::Roof,
        Surface::Wood,
        Surface::Lamp,
        Surface::Curtain,
    ];
}

/// The paints one building is finished in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Palette {
    pub siding: u32,
    pub trim: u32,
    pub accent: u32,
    pub roof: u32,
}

static TEXTURED: Cache<(&'static str, u32), Material> = Cache::new();

/// A textured finish tinted `color`; batching bakes the tint into vertex colours,
/// so every building shares one material per texture.
fn textured(path: &'static str, color: u32, bump: f32, roughness: f32) -> Arc<Material> {
    TEXTURED.get_or_insert((path, color), || {
        let map = TextureRef {
            anisotropy: 8,
            ..TextureRef::file(path)
        };
        Material {
            map: Some(map.clone()),
            bump_map: Some(map),
            bump_scale: bump,
            color: Color(color),
            roughness,
            metalness: 0.0,
            ..Material::default()
        }
    })
}

/// The clapboard tint that keeps a wall at `paint` on average.
fn clapboard_tint(paint: u32) -> u32 {
    let (paint, average) = (hex_to_linear(paint), hex_to_linear(CLAPBOARD_AVERAGE));
    linear_to_hex([0, 1, 2].map(|i| (paint[i] / average[i]).min(1.0)))
}

/// Untextured finishes share four (metalness, roughness) classes. Batching bakes
/// colours into vertices, so each class is one draw per scenery cell however many
/// colours use it; every extra class would cost a draw per building.
const MATTE: (f64, f64) = (0.0, 0.88);
const SATIN: (f64, f64) = (0.0, 0.6);
const METALLIC: (f64, f64) = (0.5, 0.45);
const GLOSSY: (f64, f64) = (0.25, 0.07);

fn finish(color: u32, (metalness, roughness): (f64, f64)) -> Arc<Material> {
    material(color, metalness, roughness)
}

/// Matte paint for small props (planting, soil) that join the buildings' batch.
pub(super) fn matte(color: u32) -> Arc<Material> {
    finish(color, MATTE)
}

pub(super) fn surface_material(surface: Surface, palette: &Palette) -> Arc<Material> {
    match surface {
        Surface::Siding => textured(
            CLAPBOARD_TEXTURE,
            clapboard_tint(palette.siding),
            0.03,
            0.78,
        ),
        Surface::Brick => textured(BRICK_TEXTURE, 0xffffff, 0.035, 0.9),
        Surface::Stone => textured(STONE_TEXTURE, 0xffffff, 0.05, 0.92),
        Surface::Trim => finish(palette.trim, SATIN),
        Surface::Accent => finish(palette.accent, SATIN),
        Surface::Glass => finish(GLASS, GLOSSY),
        Surface::Lamp => finish(LAMP_GLASS, GLOSSY),
        Surface::Metal => finish(GALVANISED, METALLIC),
        Surface::Iron => finish(IRON, METALLIC),
        Surface::Brass => finish(BRASS, METALLIC),
        Surface::Roof => finish(palette.roof, METALLIC),
        Surface::Dark => finish(DARK, MATTE),
        Surface::Concrete => finish(CONCRETE, MATTE),
        Surface::Terracotta => finish(TERRACOTTA, MATTE),
        Surface::Wood => finish(STAINED_WOOD, MATTE),
        Surface::Curtain => finish(CURTAIN, MATTE),
    }
}

/// Kit texture density per finish: textured finishes at their true scale.
pub(super) fn uv_per_metre(surface: Surface) -> f64 {
    match surface {
        Surface::Siding => 1.0 / CLAPBOARD_TILE,
        Surface::Brick => 1.0 / BRICK_TILE,
        Surface::Stone => 1.0 / STONE_TILE,
        _ => PLAIN_UV_PER_METRE,
    }
}

/// Merge a building kit and append one shadowed part per finish to `group`.
pub(super) fn add_building(group: &mut Node, meshes: KitMeshes<Surface>, palette: &Palette) {
    for (surface, mesh) in meshes {
        group
            .children
            .push(shadowed(mesh, surface_material(surface, palette)));
    }
}

/// A rectangular opening in a wall: centre `u` along the wall, sill height `y`.
#[derive(Clone, Copy, Debug)]
pub(super) struct Opening {
    pub u: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Opening {
    fn corners(&self) -> Vec<DVec2> {
        let (u0, u1) = (self.u - self.width / 2.0, self.u + self.width / 2.0);
        let (y0, y1) = (self.y, self.y + self.height);
        // Clockwise, against the outline's winding, as holes are given.
        [(u0, y0), (u0, y1), (u1, y1), (u1, y0)]
            .map(|(u, y)| DVec2::new(u, y))
            .to_vec()
    }

    pub(super) fn top(&self) -> f64 {
        self.y + self.height
    }
}

/// A box of `size` centred at `at` in the wall frame.
fn wall_block(kit: &mut Kit<Surface>, coat: Surface, frame: DMat4, size: DVec3, at: DVec3) {
    kit.block(coat, size, frame * DMat4::from_translation(at));
}

/// The outside face of a wall: `outline` (counter-clockwise) in the frame's
/// plane, cut by `openings`, each lined with trim reveals `depth` deep.
pub(super) fn wall(
    kit: &mut Kit<Surface>,
    frame: DMat4,
    outline: &[DVec2],
    openings: &[Opening],
    depth: f64,
) {
    let holes: Vec<Vec<DVec2>> = openings.iter().map(Opening::corners).collect();
    kit.face_with_holes(Surface::Siding, outline, &holes, frame);
    for opening in openings {
        let (u0, u1) = (
            opening.u - opening.width / 2.0,
            opening.u + opening.width / 2.0,
        );
        let (y0, y1) = (opening.y, opening.top());
        let at = |u: f64, y: f64, z: f64| frame.transform_point3(DVec3::new(u, y, z));
        let facing = |x: f64, y: f64| frame.transform_vector3(DVec3::new(x, y, 0.0));
        for (corners, normal) in [
            ([(u0, y0), (u1, y0)], facing(0.0, 1.0)),
            ([(u0, y1), (u1, y1)], facing(0.0, -1.0)),
            ([(u0, y0), (u0, y1)], facing(1.0, 0.0)),
            ([(u1, y0), (u1, y1)], facing(-1.0, 0.0)),
        ] {
            let [(ua, ya), (ub, yb)] = corners;
            kit.quad(
                Surface::Trim,
                [
                    at(ua, ya, 0.0),
                    at(ub, yb, 0.0),
                    at(ub, yb, -depth),
                    at(ua, ya, -depth),
                ],
                normal,
            );
        }
    }
}

/// Glazing bars of one window: `cols` by `rows` lites per sash.
#[derive(Clone, Copy, Debug)]
pub(super) struct Glazing {
    pub cols: u32,
    pub rows: u32,
    /// Two sashes with a meeting rail (double-hung) or one fixed sash.
    pub double_hung: bool,
}

/// Exterior casing around an opening: side boards, a head with its drip cap and,
/// for windows, a sloped sill and apron.
fn casing(kit: &mut Kit<Surface>, frame: DMat4, opening: &Opening, sill: bool) {
    const BOARD: f64 = CASING;
    const PROUD: f64 = 0.03;
    let (u, w, y0, y1) = (opening.u, opening.width, opening.y, opening.top());
    let bottom = if sill { y0 - 0.02 } else { y0 };
    for side in [-1.0, 1.0] {
        wall_block(
            kit,
            Surface::Trim,
            frame,
            DVec3::new(BOARD, y1 - bottom, PROUD),
            DVec3::new(
                u + side * (w + BOARD) / 2.0,
                (y1 + bottom) / 2.0,
                PROUD / 2.0,
            ),
        );
    }
    let head = 0.15;
    wall_block(
        kit,
        Surface::Trim,
        frame,
        DVec3::new(w + 2.0 * BOARD + 0.02, head, PROUD + 0.01),
        DVec3::new(u, y1 + head / 2.0, (PROUD + 0.01) / 2.0),
    );
    // A bevelled drip cap sheds rain clear of the head.
    kit.chamfer_block(
        Surface::Trim,
        DVec3::new(w + 2.0 * BOARD + 0.1, 0.035, 0.075),
        0.012,
        frame * DMat4::from_translation(DVec3::new(u, y1 + head + 0.0175, 0.035)),
    );
    if sill {
        // The sill runs back into the reveal under the sash and tips outward.
        kit.chamfer_block(
            Surface::Trim,
            DVec3::new(w + 2.0 * BOARD + 0.06, 0.05, 0.2),
            0.012,
            frame * pose(DVec3::new(u, y0 - 0.015, 0.0), DVec3::new(0.1, 0.0, 0.0)),
        );
        wall_block(
            kit,
            Surface::Trim,
            frame,
            DVec3::new(w + 2.0 * BOARD - 0.04, 0.09, 0.025),
            DVec3::new(u, y0 - 0.1, 0.0125),
        );
    }
}

/// A window set `depth` back in its opening: glass, sash stiles and rails,
/// glazing bars, then the exterior casing and sill.
pub(super) fn window(
    kit: &mut Kit<Surface>,
    frame: DMat4,
    opening: &Opening,
    glazing: Glazing,
    depth: f64,
) {
    let (u, w, y0, y1) = (opening.u, opening.width, opening.y, opening.top());
    let (u0, u1) = (u - w / 2.0, u + w / 2.0);
    let glass_z = -depth + 0.02;
    let at = |u: f64, y: f64| frame.transform_point3(DVec3::new(u, y, glass_z));
    kit.quad(
        Surface::Glass,
        [at(u0, y0), at(u1, y0), at(u1, y1), at(u0, y1)],
        frame.transform_vector3(DVec3::Z),
    );
    if glazing.double_hung {
        // Curtains drawn back to either side. The glass is opaque, so they lie on
        // its face, under the sash bars, where they read as hanging behind it.
        let at = |u: f64, y: f64| frame.transform_point3(DVec3::new(u, y, glass_z + 0.003));
        for side in [-1.0, 1.0] {
            let (outer, inner) = (u + side * w / 2.0, u + side * w * 0.27);
            kit.quad(
                Surface::Curtain,
                [at(outer, y0), at(inner, y0), at(inner, y1), at(outer, y1)],
                frame.transform_vector3(DVec3::Z),
            );
        }
    }
    const STILE: f64 = 0.055;
    const SASH_DEPTH: f64 = 0.05;
    const BAR: f64 = 0.022;
    let sash_z = glass_z + SASH_DEPTH / 2.0;
    let member = |kit: &mut Kit<Surface>, size: DVec2, center: DVec2, z: f64, depth: f64| {
        wall_block(
            kit,
            Surface::Trim,
            frame,
            DVec3::new(size.x, size.y, depth),
            DVec3::new(center.x, center.y, z),
        );
    };
    for side in [-1.0, 1.0] {
        member(
            kit,
            DVec2::new(STILE, y1 - y0),
            DVec2::new(u + side * (w - STILE) / 2.0, (y0 + y1) / 2.0),
            sash_z,
            SASH_DEPTH,
        );
    }
    member(
        kit,
        DVec2::new(w, 0.07),
        DVec2::new(u, y0 + 0.035),
        sash_z,
        SASH_DEPTH,
    );
    member(
        kit,
        DVec2::new(w, 0.06),
        DVec2::new(u, y1 - 0.03),
        sash_z,
        SASH_DEPTH,
    );
    // Lite bounds of each sash, bottom to top.
    let middle = (y0 + y1) / 2.0;
    let sashes: Vec<(f64, f64)> = if glazing.double_hung {
        // The upper sash sits behind the lower one; their meeting rail is deeper.
        member(
            kit,
            DVec2::new(w, 0.06),
            DVec2::new(u, middle),
            sash_z + 0.01,
            SASH_DEPTH + 0.02,
        );
        vec![(y0 + 0.07, middle - 0.03), (middle + 0.03, y1 - 0.06)]
    } else {
        vec![(y0 + 0.07, y1 - 0.06)]
    };
    let (inner0, inner1) = (u0 + STILE, u1 - STILE);
    for (bottom, top) in sashes {
        for col in 1..glazing.cols {
            let x = inner0 + (inner1 - inner0) * f64::from(col) / f64::from(glazing.cols);
            member(
                kit,
                DVec2::new(BAR, top - bottom),
                DVec2::new(x, (bottom + top) / 2.0),
                glass_z + 0.015,
                0.03,
            );
        }
        for row in 1..glazing.rows {
            let y = bottom + (top - bottom) * f64::from(row) / f64::from(glazing.rows);
            member(
                kit,
                DVec2::new(inner1 - inner0, BAR),
                DVec2::new(u, y),
                glass_z + 0.015,
                0.03,
            );
        }
    }
    casing(kit, frame, opening, true);
}

/// The casing board width beside an opening; shutters hang just outside it.
pub(super) const CASING: f64 = 0.1;

/// The width a full shutter would have: half the window and a little lap.
pub(super) fn shutter_width(opening: &Opening) -> f64 {
    opening.width / 2.0 + 0.03
}

/// A pair of louvred shutters `width` wide hung open against the wall either
/// side of a window's casing, with their shutter dogs.
pub(super) fn shutters(kit: &mut Kit<Surface>, frame: DMat4, opening: &Opening, width: f64) {
    const THICK: f64 = 0.035;
    const STILE: f64 = 0.05;
    const RAIL: f64 = 0.065;
    const SLAT_PITCH: f64 = 0.065;
    let (y0, y1) = (opening.y - 0.02, opening.top());
    let height = y1 - y0;
    let z = 0.04 + THICK / 2.0;
    for side in [-1.0, 1.0] {
        let u = opening.u + side * (opening.width / 2.0 + CASING + 0.01 + width / 2.0);
        let block = |kit: &mut Kit<Surface>, size: DVec3, at: DVec3| {
            wall_block(kit, Surface::Accent, frame, size, at);
        };
        for edge in [-1.0, 1.0] {
            block(
                kit,
                DVec3::new(STILE, height, THICK),
                DVec3::new(u + edge * (width - STILE) / 2.0, (y0 + y1) / 2.0, z),
            );
        }
        let middle = y0 + height * 0.42;
        for y in [y0 + RAIL / 2.0, middle, y1 - RAIL / 2.0] {
            block(
                kit,
                DVec3::new(width - 2.0 * STILE, RAIL, THICK),
                DVec3::new(u, y, z),
            );
        }
        // Angled slats fill both fields; a dark back shows between them.
        wall_block(
            kit,
            Surface::Dark,
            frame,
            DVec3::new(width - 2.0 * STILE, height - RAIL, 0.008),
            DVec3::new(u, (y0 + y1) / 2.0, 0.045),
        );
        for (bottom, top) in [
            (y0 + RAIL, middle - RAIL / 2.0),
            (middle + RAIL / 2.0, y1 - RAIL),
        ] {
            let count = ((top - bottom) / SLAT_PITCH).floor().max(1.0) as u32;
            let pitch = (top - bottom) / f64::from(count);
            // Each slat is one strip sloping down and out, lit from above.
            let half = width / 2.0 - STILE;
            let at =
                |du: f64, y: f64, dz: f64| frame.transform_point3(DVec3::new(u + du, y, z + dz));
            let facing = frame.transform_vector3(DVec3::new(0.0, 0.62, 0.78));
            for i in 0..count {
                let y = bottom + pitch * (f64::from(i) + 0.5);
                kit.quad(
                    Surface::Accent,
                    [
                        at(-half, y - 0.022, 0.016),
                        at(half, y - 0.022, 0.016),
                        at(half, y + 0.022, -0.016),
                        at(-half, y + 0.022, -0.016),
                    ],
                    facing,
                );
            }
        }
        // An S-shaped shutter dog holds the shutter open at its outer stile.
        let dog = u + side * (width / 2.0 + 0.05);
        wall_block(
            kit,
            Surface::Iron,
            frame,
            DVec3::new(0.022, 0.07, 0.016),
            DVec3::new(dog, y0 + 0.22, 0.008),
        );
    }
}

/// A panelled door set back in its opening: stiles, rails and raised panels
/// below a glazed top, brass furniture, a threshold and the casing.
pub(super) fn door(kit: &mut Kit<Surface>, frame: DMat4, opening: &Opening, depth: f64) {
    let (u, w, y0, y1) = (opening.u, opening.width, opening.y, opening.top());
    let back = -depth + 0.02;
    const STILE: f64 = 0.12;
    let height = y1 - y0;
    let block = |kit: &mut Kit<Surface>, coat: Surface, size: DVec3, at: DVec3| {
        wall_block(kit, coat, frame, size, at);
    };
    // Back leaf: the panels are the leaf's face between the frame members.
    block(
        kit,
        Surface::Accent,
        DVec3::new(w, height, 0.03),
        DVec3::new(u, (y0 + y1) / 2.0, back + 0.015),
    );
    let face = back + 0.03;
    let lite_bottom = y0 + height * 0.64;
    for side in [-1.0, 1.0] {
        block(
            kit,
            Surface::Accent,
            DVec3::new(STILE, height, 0.025),
            DVec3::new(u + side * (w - STILE) / 2.0, (y0 + y1) / 2.0, face + 0.0125),
        );
    }
    let inner = w - 2.0 * STILE;
    for (y, rail) in [
        (y0 + 0.11, 0.22),
        (y0 + height * 0.38, 0.14),
        (lite_bottom, 0.09),
        (y1 - 0.06, 0.12),
    ] {
        block(
            kit,
            Surface::Accent,
            DVec3::new(inner, rail, 0.025),
            DVec3::new(u, y, face + 0.0125),
        );
    }
    // Raised fields in the two lower openings, two panels side by side.
    let mullion_width = 0.09;
    block(
        kit,
        Surface::Accent,
        DVec3::new(mullion_width, lite_bottom - y0 - 0.3, 0.025),
        DVec3::new(u, (y0 + 0.22 + lite_bottom) / 2.0, face + 0.0125),
    );
    let panel_width = (inner - mullion_width) / 2.0 - 0.06;
    for (bottom, top) in [
        (y0 + 0.22, y0 + height * 0.38 - 0.07),
        (y0 + height * 0.38 + 0.07, lite_bottom - 0.045),
    ] {
        for side in [-1.0, 1.0] {
            kit.chamfer_block(
                Surface::Accent,
                DVec3::new(panel_width, top - bottom - 0.06, 0.03),
                0.012,
                frame
                    * DMat4::from_translation(DVec3::new(
                        u + side * (mullion_width + panel_width + 0.06) / 2.0,
                        (bottom + top) / 2.0,
                        face + 0.004,
                    )),
            );
        }
    }
    // Three small lites in the top of the door.
    let lite_top = y1 - 0.12;
    let at = |u: f64, y: f64| frame.transform_point3(DVec3::new(u, y, face + 0.002));
    let (l0, l1) = (u - inner / 2.0, u + inner / 2.0);
    kit.quad(
        Surface::Glass,
        [
            at(l0, lite_bottom + 0.045),
            at(l1, lite_bottom + 0.045),
            at(l1, lite_top),
            at(l0, lite_top),
        ],
        frame.transform_vector3(DVec3::Z),
    );
    for i in [-1.0, 1.0] {
        block(
            kit,
            Surface::Accent,
            DVec3::new(0.025, lite_top - lite_bottom, 0.03),
            DVec3::new(
                u + i * inner / 6.0,
                (lite_bottom + lite_top) / 2.0,
                face + 0.015,
            ),
        );
    }
    // Knob, rose and escutcheon on the lock stile; letter plate on the lock rail.
    let lock = u + w / 2.0 - STILE / 2.0;
    let knob_y = y0 + 0.98;
    block(
        kit,
        Surface::Brass,
        DVec3::new(0.05, 0.2, 0.01),
        DVec3::new(lock, knob_y - 0.03, face + 0.03),
    );
    let out = frame.transform_vector3(DVec3::Z);
    let knob = frame.transform_point3(DVec3::new(lock, knob_y, face + 0.035));
    kit.turned(
        Surface::Brass,
        &[
            (0.0, 0.0),
            (0.02, 0.0),
            (0.012, 0.035),
            (0.032, 0.05),
            (0.03, 0.075),
            (0.0, 0.08),
        ]
        .map(|(r, z)| DVec2::new(r, z)),
        10,
        super::tank_kit::aim(knob, knob + out),
    );
    block(
        kit,
        Surface::Brass,
        DVec3::new(0.26, 0.06, 0.012),
        DVec3::new(u, y0 + height * 0.38, face + 0.031),
    );
    block(
        kit,
        Surface::Brass,
        DVec3::new(w - 0.04, 0.16, 0.006),
        DVec3::new(u, y0 + 0.1, face + 0.028),
    );
    // Oak threshold across the bottom of the opening.
    block(
        kit,
        Surface::Wood,
        DVec3::new(w + 0.04, 0.035, depth + 0.06),
        DVec3::new(u, y0 + 0.0175, -depth / 2.0 + 0.03),
    );
    casing(kit, frame, opening, false);
}
