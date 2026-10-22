//! Pickups as military supply (successor of `pickup-visuals.ts` and
//! `pickup-atlas.ts`): every kind ships as its own piece of military stock, so its
//! silhouette names it from the gameplay camera before its colour does: an open rack
//! of shells, a cradle of rocket tubes, banded shell canisters, a strapped ammunition
//! crate, stacked ammo cans, a bundle of armour plate, jerry cans, a white medical
//! case and a transit case with its mast. Each stands on a wooden pallet laid on a
//! tarp and is painted from one atlas whose tile for the kind carries its surface,
//! colour band and the large sprayed stencil on its top.
//!
//! Faces that show the kind map into the first kind's tile and the `pickup-face`
//! effect moves them into the instance's tile (instance data `xy`); brass, steel,
//! rubber and wood map into shared cells the effect leaves alone. Every kind's item
//! is one mesh, so all pickups of a kind draw as one instanced call, and they share
//! one material. First person fades them through the instance opacity, whose
//! blended variant warm-up compiles.

use std::f64::consts::FRAC_PI_2;
use std::sync::Arc;

use glam::{DMat4, DVec3};

use super::model_primitives::Cache;
use crate::geometry::{Mesh, box_geometry, cone_geometry, cylinder_geometry, merge_geometries};
use crate::scene::{Effect, Material, Node, TextureRef, Wrap};
use crate::sim::types::PickupKind;

/// `PICKUP_ATLAS_TILES[kind]`: (column, row) of the kind's tile.
pub fn pickup_atlas_tile(kind: PickupKind) -> (u32, u32) {
    match kind {
        PickupKind::Spread => (0, 0),
        PickupKind::Rocket => (1, 0),
        PickupKind::Ricochet => (2, 0),
        PickupKind::Piercing => (0, 1),
        PickupKind::Rapid => (1, 1),
        PickupKind::Shield => (2, 1),
        PickupKind::Speed => (0, 2),
        PickupKind::Repair => (1, 2),
        PickupKind::Laser => (2, 2),
    }
}

/// The kind's signal colour: the smoke its drop is marked with and the ring sprayed
/// round it. Nine hues spread round the wheel (and white), so the colour alone
/// names a kind from across the field, in the family of its HUD colour; signal red
/// and teal stand in for the HUD's pink and cyan, which no smoke grenade comes in.
pub fn pickup_signal(kind: PickupKind) -> u32 {
    match kind {
        PickupKind::Spread => 0xdc3426,
        PickupKind::Rocket => 0xf27e1e,
        PickupKind::Rapid => 0xf4b631,
        PickupKind::Speed => 0xd6e23c,
        PickupKind::Repair => 0x3fc45c,
        PickupKind::Piercing => 0x34b4a4,
        PickupKind::Shield => 0x4482e8,
        PickupKind::Ricochet => 0x9a62e2,
        PickupKind::Laser => 0xf0f0ea,
    }
}

/// The kind's colour as supply paint: the band round its load, the stripe its
/// stencil is sprayed across and its smoke canister, its signal colour knocked back
/// toward faded military enamel (`PICKUP_PAINTS` in `scripts/asset-data.ts`). The
/// stencils themselves are sprayed in stencil cream or black, as real stores are.
pub fn pickup_paint(kind: PickupKind) -> u32 {
    match kind {
        PickupKind::Spread => 0xae3328,
        PickupKind::Rocket => 0xc0642a,
        PickupKind::Rapid => 0xc8962e,
        PickupKind::Speed => 0xb4a03c,
        PickupKind::Repair => 0x4a9450,
        PickupKind::Piercing => 0x3a8c80,
        PickupKind::Shield => 0x3e68b0,
        PickupKind::Ricochet => 0x7656a8,
        PickupKind::Laser => 0xd8d8d0,
    }
}

/// The packed atlas under `public/` (`scripts/generate-pickup-atlas.ts`): nine kind
/// tiles in a 3x3 block; in a fourth column the pallet's planks, the ground's spray
/// paint and swatches of steel, rubber and webbing; below them cartridge brass, the
/// sheet a drop is laid on, the nine stencils a restocking drop shows, in a 3x3
/// block of their own laid out like the kind tiles, and the canvas its empty pallet
/// is bundled under.
pub const PICKUP_ATLAS_PATH: &str = "textures/pickups/atlas.webp";
pub const PICKUP_ICON_SIZE: f64 = 256.0;
pub const PICKUP_ATLAS_PADDING: f64 = 16.0;
pub const PICKUP_ATLAS_STRIDE: f64 = PICKUP_ICON_SIZE + PICKUP_ATLAS_PADDING * 2.0;
pub const PICKUP_ATLAS_SIZE: f64 = PICKUP_ATLAS_STRIDE * 4.0;
pub const PICKUP_PALLET_CELL: (u32, u32) = (3, 0);
pub const PICKUP_PAINT_CELL: (u32, u32) = (3, 1);
pub const PICKUP_SWATCH_CELL: (u32, u32) = (3, 2);
pub const PICKUP_BRASS_CELL: (u32, u32) = (0, 3);
pub const PICKUP_TARP_CELL: (u32, u32) = (1, 3);
pub const PICKUP_GLYPH_CELL: (u32, u32) = (2, 3);
pub const PICKUP_BUNDLE_CELL: (u32, u32) = (3, 3);
/// The share of the glyph cell one kind's stencil takes, and the region of the
/// first kind's (u0, v0, u1, v1, `v` up) an empty pallet's stencil maps to.
const GLYPH_SPAN: f64 = 1.0 / 3.0;
pub const PICKUP_GLYPH_REGION: [f64; 4] = [0.0, 1.0 - GLYPH_SPAN, GLYPH_SPAN, 1.0];

/// A UV inside the atlas cell at (`column`, `row`), `v` up as in a mesh.
pub fn atlas_cell_uv((column, row): (u32, u32), u: f64, v: f64) -> [f64; 2] {
    [
        (f64::from(column) * PICKUP_ATLAS_STRIDE + PICKUP_ATLAS_PADDING + u * PICKUP_ICON_SIZE)
            / PICKUP_ATLAS_SIZE,
        1.0 - (f64::from(row) * PICKUP_ATLAS_STRIDE
            + PICKUP_ATLAS_PADDING
            + (1.0 - v) * PICKUP_ICON_SIZE)
            / PICKUP_ATLAS_SIZE,
    ]
}

/// `pickupAtlasUV(kind, u, v)`: a face UV mapped into the kind's padded atlas tile.
#[cfg(test)]
fn pickup_atlas_uv(kind: PickupKind, u: f64, v: f64) -> [f64; 2] {
    atlas_cell_uv(pickup_atlas_tile(kind), u, v)
}

/// Where `kind`'s tile sits relative to the first tile, in atlas UV: kind faces are
/// mapped to the first tile and the `pickup-face` effect adds this offset, passed
/// as the instance data's `xy`, to the UVs inside it.
pub fn pickup_tile_offset(kind: PickupKind) -> [f32; 2] {
    let (column, row) = pickup_atlas_tile(kind);
    [
        (f64::from(column) * PICKUP_ATLAS_STRIDE / PICKUP_ATLAS_SIZE) as f32,
        (-f64::from(row) * PICKUP_ATLAS_STRIDE / PICKUP_ATLAS_SIZE) as f32,
    ]
}

/// Where `kind`'s stencil sits in the glyph cell relative to the first kind's, in
/// atlas UV: the marking's effect adds it, passed as the instance data's `yz`.
pub fn pickup_glyph_offset(kind: PickupKind) -> [f32; 2] {
    let (column, row) = pickup_atlas_tile(kind);
    let step = GLYPH_SPAN * PICKUP_ICON_SIZE / PICKUP_ATLAS_SIZE;
    [
        (f64::from(column) * step) as f32,
        (-f64::from(row) * step) as f32,
    ]
}

/// The vertex effect that moves a kind face's UVs into the instance's atlas tile
/// (`crates/render/src/presentation/shaders/pickup_face.wgsl`).
pub const PICKUP_FACE_EFFECT: &str = "pickup-face";

type Cell = (u32, u32);
const KIND: Cell = (0, 0);

/// Regions of a cell (u0, v0, u1, v1), `v` up; the generator paints the same
/// layout. In a kind tile: the whole tile (band and stencil, for lids), the narrow
/// middle of it (for a half lid), the strip of surface and band along its top (for
/// sides), a plain strip of surface above the band (for edges), the band alone, the
/// disc and the boxes round the stencil (for caps and for tops painted across
/// several parts), and the jerry can's embossed panel.
type Region = [f64; 4];
const TILE: Region = [0.0, 0.0, 1.0, 1.0];
const NARROW: Region = [0.18, 0.0, 0.82, 1.0];
const SIDE: Region = [0.0, 0.625, 1.0, 1.0];
const PLAIN: Region = [0.16, 0.914, 0.84, 0.984];
const BAND: Region = [0.1, 0.785, 0.9, 0.86];
const CAP: Region = [0.156, 0.0, 0.844, 0.6875];
const STENCIL: Region = [0.1, 0.03, 0.9, 0.66];
const STENCIL_WIDE: Region = [0.0, 0.03, 1.0, 0.66];
const PANEL: Region = [0.1, 0.1, 0.9, 0.9];
/// Swatches of the shared cell: dark gunmetal (straps, handles, latches), black
/// rubber (caps), olive webbing and bright worn steel (masts, drawer pulls).
const GUNMETAL: Region = [0.06, 0.56, 0.44, 0.94];
const RUBBER: Region = [0.56, 0.56, 0.94, 0.94];
const WEBBING: Region = [0.06, 0.06, 0.44, 0.44];
const BRIGHT: Region = [0.56, 0.06, 0.94, 0.44];
/// The cartridge cell, bottom to top: rim and brass case, then the projectile; and
/// the primer in the middle of its base, seen from above in the rack.
const CASE: Region = [0.0, 0.0, 1.0, 0.6];
const PROJECTILE: Region = [0.0, 0.66, 1.0, 1.0];
const PRIMER: Region = [0.3, 0.02, 0.7, 0.05];
/// Pallet-cell boards for wooden chocks and spacers.
const CHOCK: Region = [0.0, 0.0, 1.0, 0.25];
/// The bundle cell's canvas, and the webbing along its foot (the generator's
/// `BUNDLE_STRAP_ROWS`).
const BUNDLE_CANVAS: Region = [0.0, 0.14, 1.0, 1.0];
const BUNDLE_STRAP: Region = [0.0, 0.01, 1.0, 0.115];

/// The pallet's size (x, z), its height, its deck boards and their width (close
/// laid, so the stencil sprayed across an empty deck lies on wood).
pub const PALLET_SIZE: [f64; 2] = [1.6, 1.2];
pub const PALLET_HEIGHT: f64 = 0.15;
const PALLET_BOARDS: u32 = 5;
const PALLET_BOARD_WIDTH: f64 = 0.3;
/// The smoke grenade marking the drop, standing on the sheet at the pallet's back
/// right corner, where the overhead camera sees it past the load and its smoke
/// shears away behind it: its top (x, height, z) where the signal smoke leaves it,
/// and its radius.
pub const SMOKE_MARKER: [f64; 3] = [0.92, 0.27, -0.44];
const MARKER_RADIUS: f64 = 0.08;
/// The canvas sheet the pallet stands on, shaded under it, setting the drop apart
/// from any ground: its size (x, z) and thickness.
const TARP_SIZE: [f64; 2] = [2.0, 1.4];
const TARP_THICKNESS: f64 = 0.014;
/// A restocking drop's empty pallet is bundled under canvas, strapped down: the
/// bundle's size (x, height, z) on the deck, and the size (x, z) of its flat top,
/// where the marking sprays the kind's stencil (`BUNDLE_TOP` above the ground). The
/// pallet's instance data `z` is 1 while the supply is stocked, and the
/// `pickup-face` effect folds the bundle away then.
const BUNDLE_SIZE: [f64; 3] = [1.36, 0.27, 0.96];
pub const BUNDLE_TOP_SIZE: [f64; 2] = [1.22, 0.84];
const BUNDLE_PILLOW: f64 = 0.05;
pub const BUNDLE_TOP: f64 = PALLET_HEIGHT + BUNDLE_SIZE[1] + BUNDLE_PILLOW;

static SHARED: Cache<u8, Mesh> = Cache::new();
static MATERIALS: Cache<u8, Material> = Cache::new();

/// How a face takes its paint: its own UVs into a region of a cell, or (for faces
/// looking up) projected from above over an item-space rectangle (x0, z0, x1, z1),
/// so one stencil spans the tops of several parts.
#[derive(Clone, Copy)]
enum Paint {
    Face(Cell, Region),
    Above(Cell, Region, [f64; 4]),
}

const fn kind(region: Region) -> Paint {
    Paint::Face(KIND, region)
}

const fn swatch(region: Region) -> Paint {
    Paint::Face(PICKUP_SWATCH_CELL, region)
}

const fn brass(region: Region) -> Paint {
    Paint::Face(PICKUP_BRASS_CELL, region)
}

const fn wood(region: Region) -> Paint {
    Paint::Face(PICKUP_PALLET_CELL, region)
}

const fn canvas(region: Region) -> Paint {
    Paint::Face(PICKUP_BUNDLE_CELL, region)
}

/// One primitive of an item: its mesh, built about the origin, the paint of the
/// faces looking up, sideways along z (and along x when `end` is given) and down,
/// and where it goes in the item.
struct Part {
    mesh: Mesh,
    top: Paint,
    side: Paint,
    end: Option<Paint>,
    bottom: Paint,
    place: DMat4,
}

impl Part {
    fn new(mesh: Mesh, paint: Paint) -> Self {
        Self {
            mesh,
            top: paint,
            side: paint,
            end: None,
            bottom: paint,
            place: DMat4::IDENTITY,
        }
    }

    fn cube([w, h, d]: [f64; 3], paint: Paint) -> Self {
        Self::new(box_geometry(w, h, d), paint)
    }

    /// An upright cylinder of `radius` and `height`.
    fn drum(radius: f64, height: f64, segments: u32, paint: Paint) -> Self {
        Self::new(cylinder_geometry(radius, radius, height, segments), paint)
    }

    fn top(mut self, paint: Paint) -> Self {
        self.top = paint;
        self
    }

    fn side(mut self, paint: Paint) -> Self {
        self.side = paint;
        self
    }

    fn end(mut self, paint: Paint) -> Self {
        self.end = Some(paint);
        self
    }

    fn at(mut self, x: f64, y: f64, z: f64) -> Self {
        self.place = DMat4::from_translation(DVec3::new(x, y, z)) * self.place;
        self
    }

    /// Turn about the vertical.
    fn turned(mut self, angle: f64) -> Self {
        self.place = DMat4::from_rotation_y(angle) * self.place;
        self
    }

    /// Lay an upright part along x.
    fn along_x(mut self) -> Self {
        self.place = DMat4::from_rotation_z(-FRAC_PI_2) * self.place;
        self
    }
}

fn region_uv(cell: Cell, [u0, v0, u1, v1]: Region, u: f64, v: f64) -> [f32; 2] {
    let [u, v] = atlas_cell_uv(
        cell,
        u0 + (u1 - u0) * u.clamp(0.0, 1.0),
        v0 + (v1 - v0) * v.clamp(0.0, 1.0),
    );
    [u as f32, v as f32]
}

/// Paint every part by the way its faces look before it is placed, place it, and
/// merge them into one mesh.
fn build(parts: Vec<Part>) -> Mesh {
    let meshes: Vec<Mesh> = parts
        .into_iter()
        .map(|part| {
            let mut mesh = part.mesh;
            let paints: Vec<Paint> = mesh
                .normals
                .iter()
                .map(|normal| {
                    if normal[1] > 0.5 {
                        part.top
                    } else if normal[1] < -0.5 {
                        part.bottom
                    } else if normal[0].abs() > 0.5 {
                        part.end.unwrap_or(part.side)
                    } else {
                        part.side
                    }
                })
                .collect();
            mesh.apply_matrix4(&part.place);
            for ((uv, position), paint) in mesh.uvs.iter_mut().zip(&mesh.positions).zip(paints) {
                *uv = match paint {
                    Paint::Face(cell, region) => {
                        region_uv(cell, region, f64::from(uv[0]), f64::from(uv[1]))
                    }
                    Paint::Above(cell, region, [x0, z0, x1, z1]) => {
                        let u = (f64::from(position[0]) - x0) / (x1 - x0);
                        let v = (z1 - f64::from(position[2])) / (z1 - z0);
                        region_uv(cell, region, u, v)
                    }
                };
            }
            mesh
        })
        .collect();
    merge_geometries(&meshes.iter().collect::<Vec<_>>()).expect("parts share one layout")
}

/// Two steel bands `x` either side of the middle, over a load `height` tall and
/// `depth` deep standing on y = 0.
fn straps(parts: &mut Vec<Part>, height: f64, depth: f64, x: f64) {
    for x in [-x, x] {
        parts.push(
            Part::cube([0.07, 0.012, depth + 0.03], swatch(GUNMETAL)).at(x, height + 0.006, 0.0),
        );
        for z in [-1.0, 1.0] {
            parts.push(Part::cube([0.07, height, 0.012], swatch(GUNMETAL)).at(
                x,
                height / 2.0,
                z * (depth / 2.0 + 0.008),
            ));
        }
    }
}

/// Spread: an open crate racked with cartridges, its lid slid back over one end
/// with the stencil on it. From above, rows of brass heads beside the lid.
fn shell_rack() -> Mesh {
    let [w, h, d] = [1.44, 0.36, 1.0];
    let wall = 0.05;
    let mut parts = vec![Part::cube([w, 0.04, d], kind(PLAIN)).at(0.0, 0.02, 0.0)];
    for z in [-1.0, 1.0] {
        parts.push(Part::cube([w, h, wall], kind(PLAIN)).side(kind(SIDE)).at(
            0.0,
            h / 2.0,
            z * (d - wall) / 2.0,
        ));
    }
    for x in [-1.0, 1.0] {
        parts.push(
            Part::cube([wall, h, d - 2.0 * wall], kind(PLAIN))
                .end(kind(SIDE))
                .at(x * (w - wall) / 2.0, h / 2.0, 0.0),
        );
    }
    // The lid, pushed back off the cartridges and lying a little askew.
    parts.push(
        Part::cube([0.66, 0.06, d + 0.06], kind(PLAIN))
            .top(kind(NARROW))
            .turned(0.05)
            .at(-0.4, h + 0.03, 0.02),
    );
    let case = 0.56;
    for x in [0.08, 0.32, 0.56] {
        for z in [-0.28, 0.0, 0.28] {
            parts.push(Part::drum(0.1, case, 8, brass(CASE)).top(brass(PRIMER)).at(
                x,
                0.04 + case / 2.0,
                z,
            ));
            parts.push(
                Part::new(cone_geometry(0.094, 0.22, 8), brass(PROJECTILE)).at(
                    x,
                    0.04 + case + 0.11,
                    z,
                ),
            );
        }
    }
    build(parts)
}

/// Rocket: three launch tubes stacked on a pair of wooden chocks, banded in the
/// kind's colour near the muzzle, capped in rubber, webbing round the stack.
fn rocket_tubes() -> Mesh {
    let radius = 0.15;
    let length = 1.5;
    let chock = 0.14;
    let low = chock + radius;
    let high = low + 0.26;
    let mut parts = Vec::new();
    for x in [-0.48, 0.48] {
        parts.push(Part::cube([0.18, chock, 0.8], wood(CHOCK)).at(x, chock / 2.0, 0.0));
        parts.push(Part::cube([0.07, 0.02, 0.2], swatch(WEBBING)).at(x, high + radius + 0.01, 0.0));
        for z in [-1.0, 1.0] {
            parts.push(Part::cube([0.07, high - chock, 0.02], swatch(WEBBING)).at(
                x,
                (high + chock) / 2.0,
                z * (0.16 + radius + 0.01),
            ));
        }
    }
    for (z, y) in [(-0.16, low), (0.16, low), (0.0, high)] {
        parts.push(
            Part::drum(radius, length, 10, kind(PLAIN))
                .top(swatch(RUBBER))
                .along_x()
                .at(0.0, y, z),
        );
        // A band of the kind's paint near the muzzle, a collar at the breech.
        parts.push(
            Part::drum(radius + 0.008, 0.12, 10, kind(BAND))
                .along_x()
                .at(length / 2.0 - 0.2, y, z),
        );
        parts.push(
            Part::drum(radius + 0.012, 0.07, 10, swatch(GUNMETAL))
                .along_x()
                .at(-length / 2.0 + 0.1, y, z),
        );
    }
    build(parts)
}

/// Ricochet: four fibre shell canisters banded together, their caps stencilled.
fn shell_canisters() -> Mesh {
    let radius = 0.24;
    let height = 0.76;
    let pitch = 0.27;
    let mut parts = Vec::new();
    for x in [-pitch, pitch] {
        for z in [-pitch, pitch] {
            parts.push(
                Part::drum(radius, height, 12, kind(SIDE))
                    .top(kind(CAP))
                    .at(x, height / 2.0, z),
            );
        }
    }
    // A steel band round the four of them.
    let span = 2.0 * (pitch + radius) + 0.02;
    for y in [0.2, 0.56] {
        for z in [-1.0, 1.0] {
            parts.push(Part::cube([span, 0.06, 0.016], swatch(GUNMETAL)).at(
                0.0,
                y,
                z * span / 2.0,
            ));
        }
        for x in [-1.0, 1.0] {
            parts.push(Part::cube([0.016, 0.06, span], swatch(GUNMETAL)).at(
                x * span / 2.0,
                y,
                0.0,
            ));
        }
    }
    build(parts)
}

/// Piercing: the classic ammunition crate, a pine box under an overhanging lid,
/// two steel straps over it and rope-grip steel handles at its ends.
fn ammo_crate() -> Mesh {
    let [w, h, d] = [1.5, 0.6, 1.0];
    let lid = 0.08;
    let mut parts = vec![
        Part::cube([w, h, d], kind(PLAIN))
            .side(kind(SIDE))
            .at(0.0, h / 2.0, 0.0),
        Part::cube([w + 0.06, lid, d + 0.06], kind(PLAIN))
            .top(kind(TILE))
            .at(0.0, h + lid / 2.0, 0.0),
    ];
    straps(&mut parts, h + lid, d + 0.06, 0.5);
    for x in [-1.0, 1.0] {
        parts.push(Part::cube([0.05, 0.07, 0.42], swatch(GUNMETAL)).at(
            x * (w / 2.0 + 0.03),
            h * 0.68,
            0.0,
        ));
    }
    build(parts)
}

/// Rapid: olive ammo cans, six in the bottom layer and three across them, the
/// stencil sprayed over the top three.
fn ammo_cans() -> Mesh {
    let [length, height, width] = [0.6, 0.36, 0.28];
    let lid = 0.05;
    let can = |parts: &mut Vec<Part>, at: [f64; 3], turn: f64, top: Paint| {
        let place = |part: Part| part.turned(turn).at(at[0], at[1], at[2]);
        parts.push(place(
            Part::cube([length, height, width], kind(PLAIN))
                .side(kind(SIDE))
                .at(0.0, height / 2.0, 0.0),
        ));
        parts.push(place(
            Part::cube([length + 0.02, lid, width + 0.02], kind(PLAIN))
                .top(top)
                .at(0.0, height + lid / 2.0, 0.0),
        ));
        parts.push(place(Part::cube([0.24, 0.025, 0.05], swatch(GUNMETAL)).at(
            0.0,
            height + lid + 0.03,
            0.0,
        )));
        parts.push(place(Part::cube([0.035, 0.12, 0.12], swatch(GUNMETAL)).at(
            length / 2.0 + 0.015,
            height - 0.02,
            0.0,
        )));
    };
    let mut parts = Vec::new();
    for x in [-0.32, 0.32] {
        for z in [-0.31, 0.0, 0.31] {
            can(&mut parts, [x, 0.0, z], 0.0, kind(PLAIN));
        }
    }
    let stencil = Paint::Above(KIND, STENCIL, [-0.47, -0.32, 0.47, 0.32]);
    for x in [-0.31, 0.0, 0.31] {
        can(&mut parts, [x, height + lid, 0.0], FRAC_PI_2, stencil);
    }
    build(parts)
}

/// Shield: a bundle of armour plate on wooden spacers, fanned a little, strapped,
/// the stencil sprayed on the top plate.
fn armour_plates() -> Mesh {
    let mut parts = Vec::new();
    let mut y = 0.0;
    let plates = [
        ([1.5, 0.11, 1.04], 0.0, 0.0),
        ([1.42, 0.11, 0.98], 0.07, 0.04),
        ([1.46, 0.11, 1.0], -0.05, -0.03),
    ];
    for (index, (size, turn, shift)) in plates.into_iter().enumerate() {
        let spacer = if index == 0 { 0.06 } else { 0.05 };
        for x in [-0.46, 0.46] {
            parts.push(Part::cube([0.12, spacer, 0.86], wood(CHOCK)).at(x, y + spacer / 2.0, 0.0));
        }
        y += spacer;
        let top = if index + 1 == plates.len() {
            kind(TILE)
        } else {
            kind(PLAIN)
        };
        parts.push(Part::cube(size, kind(PLAIN)).top(top).turned(turn).at(
            shift,
            y + size[1] / 2.0,
            0.0,
        ));
        y += size[1];
    }
    straps(&mut parts, y, 1.04, 0.42);
    build(parts)
}

/// Speed: two jerry cans side by side, their embossed faces out, each with its
/// carry handles and spout, one turned a little against the other.
fn jerry_cans() -> Mesh {
    let [w, h, d] = [0.7, 1.04, 0.36];
    let mut parts = Vec::new();
    for (turn, offset) in [(0.06, -0.38), (-0.1, 0.4)] {
        let place = |part: Part| part.turned(turn).at(offset, 0.0, 0.0);
        parts.push(place(
            Part::cube([w, h, d], kind(PLAIN))
                .side(kind(PANEL))
                .end(kind(PLAIN))
                .at(0.0, h / 2.0, 0.0),
        ));
        for (size, [x, y]) in [
            ([0.42, 0.05, 0.08], [-0.06, h + 0.1]),
            ([0.05, 0.1, 0.08], [-0.25, h + 0.05]),
            ([0.05, 0.1, 0.08], [0.13, h + 0.05]),
            ([0.11, 0.13, 0.11], [0.24, h + 0.06]),
        ] {
            parts.push(place(Part::cube(size, swatch(GUNMETAL)).at(x, y, 0.0)));
        }
    }
    build(parts)
}

/// Repair: a white two-tier medical and repair case, drawer pulls on its front and
/// back, the cross plate on its top tier and a carry bar over it.
fn tool_chest() -> Mesh {
    let [w, h, d] = [1.32, 0.46, 0.84];
    let [tw, th, td] = [1.12, 0.22, 0.66];
    let mut parts = vec![
        Part::cube([w, h, d], kind(PLAIN))
            .side(kind(SIDE))
            .at(0.0, h / 2.0, 0.0),
        Part::cube([tw, th, td], kind(PLAIN))
            .top(Paint::Above(
                KIND,
                STENCIL_WIDE,
                [-tw / 2.0, -td / 2.0, tw / 2.0, td / 2.0],
            ))
            .at(0.0, h + th / 2.0, 0.0),
    ];
    for z in [-1.0, 1.0] {
        for (x, y) in [(-0.34, 0.3), (0.34, 0.3), (-0.34, 0.13), (0.34, 0.13)] {
            parts.push(Part::cube([0.22, 0.035, 0.03], swatch(BRIGHT)).at(
                x,
                y,
                z * (d / 2.0 + 0.012),
            ));
        }
    }
    let top = h + th;
    parts.push(Part::cube([0.86, 0.04, 0.05], swatch(GUNMETAL)).at(0.0, top + 0.1, 0.0));
    for x in [-0.41, 0.41] {
        parts.push(Part::cube([0.04, 0.1, 0.05], swatch(GUNMETAL)).at(x, top + 0.05, 0.0));
    }
    for x in [-1.0, 1.0] {
        parts.push(Part::cube([0.04, 0.06, 0.34], swatch(GUNMETAL)).at(
            x * (w / 2.0 + 0.02),
            h * 0.7,
            0.0,
        ));
    }
    build(parts)
}

/// Laser: a ribbed transit case for the defence set, latched, its whip antenna
/// raised from one corner and the emitter's capped lens on the other.
fn transit_case() -> Mesh {
    let [w, h, d] = [1.24, 0.48, 0.84];
    let lid = 0.12;
    let mut parts = vec![
        Part::cube([w, h, d], kind(PLAIN))
            .side(kind(SIDE))
            .at(0.0, h / 2.0, 0.0),
        Part::cube([w + 0.04, lid, d + 0.04], kind(PLAIN))
            .top(Paint::Above(
                KIND,
                STENCIL_WIDE,
                [-0.56, -(d + 0.04) / 2.0, 0.56, (d + 0.04) / 2.0],
            ))
            .at(0.0, h + lid / 2.0, 0.0),
    ];
    for x in [-0.4, 0.4] {
        for z in [-1.0, 1.0] {
            parts.push(Part::cube([0.12, 0.15, 0.035], swatch(GUNMETAL)).at(
                x,
                h - 0.03,
                z * (d / 2.0 + 0.02),
            ));
        }
    }
    let top = h + lid;
    parts.push(Part::cube([0.12, 0.08, 0.12], swatch(GUNMETAL)).at(0.52, top + 0.04, -0.32));
    parts.push(Part::drum(0.018, 1.05, 5, swatch(BRIGHT)).at(0.52, top + 0.08 + 0.525, -0.32));
    parts.push(
        Part::drum(0.13, 0.12, 10, swatch(GUNMETAL))
            .top(swatch(RUBBER))
            .at(-0.5, top + 0.06, 0.3),
    );
    build(parts)
}

/// The pallet every pickup sits on: a tarp under three stringers and five close-laid
/// deck boards, and the smoke grenade beside it, painted in the kind's colour. An
/// emptied pallet shows the kind's stencil sprayed across its deck (the marking's).
fn pallet_geometry() -> Mesh {
    let [w, d] = PALLET_SIZE;
    let deck = 0.035;
    let stringer = PALLET_HEIGHT - deck;
    let [tw, td] = TARP_SIZE;
    let mut parts = vec![
        Part::cube(
            [tw, TARP_THICKNESS, td],
            Paint::Face(PICKUP_TARP_CELL, TILE),
        )
        .at(0.0, TARP_THICKNESS / 2.0, 0.0),
    ];
    for z in [-1.0, 0.0, 1.0] {
        parts.push(Part::cube([w, stringer, 0.12], wood(CHOCK)).at(
            0.0,
            stringer / 2.0,
            z * (d / 2.0 - 0.06),
        ));
    }
    // Each board is built lengthwise along x and turned, so its grain runs down
    // its length and each takes one plank of the cell.
    let pitch = (w - PALLET_BOARD_WIDTH) / f64::from(PALLET_BOARDS - 1);
    for board in 0..PALLET_BOARDS {
        let x = -w / 2.0 + PALLET_BOARD_WIDTH / 2.0 + pitch * f64::from(board);
        let plank = f64::from(board % 4) * 0.25;
        parts.push(
            Part::cube(
                [d, deck, PALLET_BOARD_WIDTH],
                wood([0.0, plank, 1.0, plank + 0.25]),
            )
            .turned(FRAC_PI_2)
            .at(x, stringer + deck / 2.0, 0.0),
        );
    }
    // The smoke grenade: a canister in the kind's paint, its fuse head and lever.
    let [mx, top, mz] = SMOKE_MARKER;
    let fuse = 0.05;
    parts.push(
        Part::drum(MARKER_RADIUS, top - fuse, 10, kind(BAND))
            .top(swatch(GUNMETAL))
            .at(mx, (top - fuse) / 2.0, mz),
    );
    parts.push(
        Part::drum(MARKER_RADIUS * 0.55, fuse, 8, swatch(GUNMETAL)).at(mx, top - fuse / 2.0, mz),
    );
    parts.push(Part::cube([0.024, top * 0.7, 0.03], swatch(GUNMETAL)).at(
        mx + MARKER_RADIUS + 0.008,
        top * 0.6,
        mz,
    ));
    restock_bundle(&mut parts);
    build(parts)
}

/// The canvas bundle over a restocking drop's empty pallet: the load's cover drawn
/// down to the deck, a slack pillow of canvas on top, a fold thrown back at one end
/// and two webbing straps over it. Every face maps into the bundle cell, which the
/// `pickup-face` effect folds away while the supply is stocked.
fn restock_bundle(parts: &mut Vec<Part>) {
    let [w, h, d] = BUNDLE_SIZE;
    let [tw, td] = BUNDLE_TOP_SIZE;
    let deck = PALLET_HEIGHT;
    parts.push(Part::cube([w, h, d], canvas(BUNDLE_CANVAS)).at(0.0, deck + h / 2.0, 0.0));
    parts.push(
        Part::cube([tw, BUNDLE_PILLOW, td], canvas(BUNDLE_CANVAS)).at(
            0.0,
            deck + h + BUNDLE_PILLOW / 2.0,
            0.0,
        ),
    );
    // The fold thrown back over the left end, and the slack hanging at the right.
    parts.push(
        Part::cube([0.3, 0.05, d * 0.8], canvas(BUNDLE_CANVAS))
            .turned(0.12)
            .at(-w / 2.0 + 0.1, BUNDLE_TOP + 0.02, 0.02),
    );
    parts.push(
        Part::cube([0.05, h * 0.8, d * 0.86], canvas(BUNDLE_CANVAS)).at(
            w / 2.0 + 0.02,
            deck + h * 0.42,
            0.0,
        ),
    );
    for x in [-0.36, 0.42] {
        parts.push(
            Part::cube([0.08, 0.012, td + 0.02], canvas(BUNDLE_STRAP)).at(
                x,
                BUNDLE_TOP + 0.006,
                0.0,
            ),
        );
        for z in [-1.0, 1.0] {
            parts.push(
                Part::cube([0.08, h + 0.04, 0.012], canvas(BUNDLE_STRAP)).at(
                    x,
                    deck + (h + 0.04) / 2.0,
                    z * (d / 2.0 + 0.007),
                ),
            );
        }
    }
}

/// The shared supply paint: the atlas, worn and matte, with the `pickup-face`
/// effect choosing the kind's tile.
pub fn pickup_material() -> Arc<Material> {
    MATERIALS.get_or_insert(0, || {
        let atlas = TextureRef {
            wrap: Wrap::Clamp,
            anisotropy: 4,
            ..TextureRef::file(PICKUP_ATLAS_PATH)
        };
        Material {
            map: Some(atlas),
            roughness: 0.8,
            metalness: 0.08,
            effect: Effect::Custom {
                name: PICKUP_FACE_EFFECT,
                params: Vec::new(),
            },
            ..Material::default()
        }
    })
}

/// The kind's item mesh, kind faces mapped to the first tile.
fn item_geometry(kind: PickupKind) -> Arc<Mesh> {
    SHARED.get_or_insert(kind as u8, || match kind {
        PickupKind::Spread => shell_rack(),
        PickupKind::Rocket => rocket_tubes(),
        PickupKind::Ricochet => shell_canisters(),
        PickupKind::Piercing => ammo_crate(),
        PickupKind::Rapid => ammo_cans(),
        PickupKind::Shield => armour_plates(),
        PickupKind::Speed => jerry_cans(),
        PickupKind::Repair => tool_chest(),
        PickupKind::Laser => transit_case(),
    })
}

/// The pallet's key in the mesh cache, clear of every kind's.
const PALLET_KEY: u8 = 100;

fn part(mesh: Arc<Mesh>) -> Node {
    let mut node = Node::mesh(mesh, pickup_material());
    if let Some(drawable) = &mut node.drawable {
        drawable.cast_shadow = true;
        drawable.receive_shadow = true;
    }
    node
}

/// `pickupCube(kind)`: the supply the kind ships as, standing on y = 0; the
/// instance data's `xy` ([`pickup_tile_offset`]) picks the kind's paint.
/// Presentation stands it on its pallet.
pub fn pickup_cube(kind: PickupKind) -> Node {
    let mut group = Node::default();
    group.children.push(part(item_geometry(kind)));
    group
}

/// The pallet under every pickup, shared by every kind; its instance data's `xy`
/// paints the smoke grenade for the kind.
pub fn pickup_pallet() -> Node {
    let mesh = SHARED.get_or_insert(PALLET_KEY, pallet_geometry);
    let mut group = Node::default();
    group.children.push(part(mesh));
    group
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tile_offset_moves_the_first_tile_onto_each_kinds_tile() {
        for kind in PickupKind::ALL {
            let [du, dv] = pickup_tile_offset(kind);
            for (u, v) in [(0.0, 0.0), (1.0, 1.0), (0.3, 0.8)] {
                let [u0, v0] = pickup_atlas_uv(PickupKind::Spread, u, v);
                let [u1, v1] = pickup_atlas_uv(kind, u, v);
                assert!((u0 + f64::from(du) - u1).abs() < 1e-6, "{kind:?}");
                assert!((v0 + f64::from(dv) - v1).abs() < 1e-6, "{kind:?}");
            }
        }
    }

    /// Every UV lies inside the first tile, which the effect moves, or a shared
    /// cell, which it leaves alone; nothing samples a gutter or a neighbour.
    #[test]
    fn every_item_and_the_pallet_stay_inside_their_atlas_cells() {
        let cells = [
            KIND,
            PICKUP_PALLET_CELL,
            PICKUP_SWATCH_CELL,
            PICKUP_BRASS_CELL,
            PICKUP_TARP_CELL,
            PICKUP_BUNDLE_CELL,
        ];
        let inside = |mesh: &Mesh| {
            mesh.uvs.iter().all(|&[u, v]| {
                let (u, v) = (f64::from(u), f64::from(v));
                cells.iter().any(|&cell| {
                    let [u0, v1] = atlas_cell_uv(cell, 0.0, 1.0);
                    let [u1, v0] = atlas_cell_uv(cell, 1.0, 0.0);
                    (u0 - 1e-6..=u1 + 1e-6).contains(&u) && (v0 - 1e-6..=v1 + 1e-6).contains(&v)
                })
            })
        };
        for kind in PickupKind::ALL {
            assert!(inside(&item_geometry(kind)), "{kind:?}");
        }
        assert!(inside(&pallet_geometry()));
    }

    /// Every kind stands on its pallet's deck within its footprint, low enough to
    /// read as a load rather than cover.
    #[test]
    fn every_item_fits_its_pallet() {
        let [w, d] = PALLET_SIZE;
        for kind in PickupKind::ALL {
            let mesh = item_geometry(kind);
            let mut low = DVec3::splat(f64::MAX);
            let mut high = DVec3::splat(f64::MIN);
            for p in &mesh.positions {
                let p = DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]));
                low = low.min(p);
                high = high.max(p);
            }
            assert!(low.y > -1e-6, "{kind:?} stands on the deck");
            assert!(high.x - low.x <= w + 0.05, "{kind:?} {low} {high}");
            assert!(high.z - low.z <= d + 0.05, "{kind:?} {low} {high}");
            assert!(high.y < 1.8, "{kind:?}");
        }
    }
}
