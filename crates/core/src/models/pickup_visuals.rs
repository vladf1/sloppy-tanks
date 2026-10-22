//! Pickups as military supply (successor of `pickup-visuals.ts` and
//! `pickup-atlas.ts`): every kind ships as its own piece of military stock, so its
//! silhouette names it from the gameplay camera, and one large block of its own
//! paint on the load itself names it again: an open rack of shells under a magenta
//! lid, red rocket tubes, shell canisters under purple caps, a teal-lidded
//! ammunition crate, ammo cans with orange lids, a blue armour plate propped on its
//! stack, yellow jerry cans, a green tool chest beside spare road wheels and a pale
//! transit case with its mast and dish. Nine hues and no repeats. Each stands on a
//! wooden pallet laid on a tarp and is painted from one atlas whose tile for the
//! kind carries the body's surface, the kind's paint and its sprayed stencil.
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

/// The kind's signal colour: the thin wisp of smoke its drop is marked with, which
/// the plume greys down to a hint. In the family of its paint.
pub fn pickup_signal(kind: PickupKind) -> u32 {
    match kind {
        PickupKind::Spread => 0xc8508c,
        PickupKind::Rocket => 0xc8402e,
        PickupKind::Rapid => 0xe07a2a,
        PickupKind::Speed => 0xe0c840,
        PickupKind::Repair => 0x4cb050,
        PickupKind::Piercing => 0x38a8a0,
        PickupKind::Shield => 0x4a78c8,
        PickupKind::Ricochet => 0x8a62c4,
        PickupKind::Laser => 0xd8d6d0,
    }
}

/// The kind's colour as supply paint (`PICKUP_PAINTS` in `scripts/asset-data.ts`):
/// the big painted block on its load, one hue per kind round the wheel and a pale
/// grey, each in the family of its HUD colour, worn and dusty enamel rather than
/// toy colours, and no brighter than weathered white paint.
pub fn pickup_paint(kind: PickupKind) -> u32 {
    match kind {
        PickupKind::Spread => 0xa83270,
        PickupKind::Rocket => 0xb02c22,
        PickupKind::Rapid => 0xcc6a1a,
        PickupKind::Speed => 0xd0b62a,
        PickupKind::Repair => 0x3a8c3a,
        PickupKind::Piercing => 0x1e8a80,
        PickupKind::Shield => 0x2e5aa8,
        PickupKind::Ricochet => 0x643f9e,
        PickupKind::Laser => 0xbdbab0,
    }
}

/// The packed atlas under `public/` (`scripts/generate-pickup-atlas.ts`): nine kind
/// tiles in a 3x3 block; in a fourth column the pallet's planks and swatches of
/// steel, rubber and webbing; below the block cartridge brass, the sheet a drop is
/// laid on and the broken boards of a crushed pallet.
pub const PICKUP_ATLAS_PATH: &str = "textures/pickups/atlas.webp";
pub const PICKUP_ICON_SIZE: f64 = 256.0;
pub const PICKUP_ATLAS_PADDING: f64 = 16.0;
pub const PICKUP_ATLAS_STRIDE: f64 = PICKUP_ICON_SIZE + PICKUP_ATLAS_PADDING * 2.0;
pub const PICKUP_ATLAS_SIZE: f64 = PICKUP_ATLAS_STRIDE * 4.0;
pub const PICKUP_PALLET_CELL: (u32, u32) = (3, 0);
pub const PICKUP_SWATCH_CELL: (u32, u32) = (3, 2);
pub const PICKUP_BRASS_CELL: (u32, u32) = (0, 3);
pub const PICKUP_TARP_CELL: (u32, u32) = (1, 3);
pub const PICKUP_CRUSHED_CELL: (u32, u32) = (3, 3);

/// Everything a drop shows is drawn this much larger than it is modelled, so a
/// load reads at the gameplay camera; the pickup's reach is the simulation's.
pub const SUPPLY_SCALE: f64 = 1.15;

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

/// The vertex effect that moves a kind face's UVs into the instance's atlas tile
/// and swaps the pallet between whole and crushed
/// (`crates/render/src/presentation/shaders/pickup_face.wgsl`).
pub const PICKUP_FACE_EFFECT: &str = "pickup-face";

type Cell = (u32, u32);
const KIND: Cell = (0, 0);

/// Regions of a cell (u0, v0, u1, v1), `v` up; the generator paints the same
/// layout (`TILE_ROWS` there). A kind tile has three bands, top to bottom: the
/// body's side strip (its surface with a band of the kind's paint near the top,
/// for side faces), a strip of plain paint (for tubes, caps, ends and edges in the
/// kind's colour) and the painted lid with the stencil sprayed on it.
type Region = [f64; 4];
const SIDE: Region = [0.0, 0.75, 1.0, 1.0];
const PLAIN: Region = [0.1, 0.773, 0.9, 0.852];
const PAINT: Region = [0.02, 0.633, 0.98, 0.742];
const LID: Region = [0.0, 0.0, 1.0, 0.625];
/// Swatches of the shared cell: dark gunmetal (straps, handles, latches), black
/// rubber (caps, tyres), olive webbing and bright worn steel (masts, drawer pulls).
const GUNMETAL: Region = [0.06, 0.56, 0.44, 0.94];
const RUBBER: Region = [0.56, 0.56, 0.94, 0.94];
const WEBBING: Region = [0.06, 0.06, 0.44, 0.44];
const BRIGHT: Region = [0.56, 0.06, 0.94, 0.44];
/// The cartridge cell, bottom to top: rim and brass case, then the projectile; and
/// the primer in the middle of its base, seen from above in the rack.
const CASE: Region = [0.0, 0.0, 1.0, 0.6];
const PROJECTILE: Region = [0.0, 0.66, 1.0, 1.0];
const PRIMER: Region = [0.3, 0.02, 0.7, 0.05];
/// The pallet cell's four boards, each inset from its neighbours so the effect
/// can tell the one a pallet may be missing (`MISSING_BOARD`) apart by its UVs.
const fn board(row: u32) -> Region {
    let v = row as f64 * 0.25;
    [0.0, v + 0.02, 1.0, v + 0.23]
}
/// Wooden chocks, stringers and spacers.
const CHOCK: Region = board(0);
/// The board a worn pallet has lost: only the deck's edge board uses it.
const MISSING_BOARD: u32 = 3;

/// The pallet's size (x, z), its height, its deck boards and their width.
pub const PALLET_SIZE: [f64; 2] = [1.6, 1.2];
pub const PALLET_HEIGHT: f64 = 0.15;
const PALLET_BOARDS: usize = 5;
const PALLET_BOARD_WIDTH: f64 = 0.27;
/// The smoke grenade marking the drop, standing on the sheet at the pallet's back
/// right corner, where the overhead camera sees it past the load: its top (x,
/// height, z) where the signal smoke leaves it, and its radius.
pub const SMOKE_MARKER: [f64; 3] = [0.92, 0.27, -0.44];
const MARKER_RADIUS: f64 = 0.08;
/// The canvas sheet the pallet stands on, shaded under it, setting the drop apart
/// from any ground: its size (x, z) and thickness.
const TARP_SIZE: [f64; 2] = [2.0, 1.4];
const TARP_THICKNESS: f64 = 0.014;
/// The lid a crushed crate throws: its size (x, thickness, z).
pub const LID_SIZE: [f64; 3] = [0.96, 0.05, 0.68];

static SHARED: Cache<u8, Mesh> = Cache::new();
static MATERIALS: Cache<u8, Material> = Cache::new();

/// How a face takes its paint: its own UVs into a region of a cell, or (for faces
/// looking up) projected from above over a rectangle (x0, z0, x1, z1): in item
/// space, so one stencil spans the tops of several parts, or in the part's own
/// space before it is placed, so a stencil turns and tips with its lid.
#[derive(Clone, Copy)]
enum Paint {
    Face(Cell, Region),
    Above(Cell, Region, [f64; 4]),
    Own(Cell, Region, [f64; 4]),
}

const fn kind(region: Region) -> Paint {
    Paint::Face(KIND, region)
}

/// The kind's stencilled lid projected from above over an item-space `rect`.
const fn lid_over(rect: [f64; 4]) -> Paint {
    Paint::Above(KIND, LID, rect)
}

/// The kind's stencilled lid over the top of one part (x, z) in size.
const fn lid_on([w, d]: [f64; 2]) -> Paint {
    Paint::Own(KIND, LID, [-w / 2.0, -d / 2.0, w / 2.0, d / 2.0])
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

    fn bottom(mut self, paint: Paint) -> Self {
        self.bottom = paint;
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

    /// Tip about the z axis (raising one end along x) and then about the x axis.
    fn tipped(mut self, about_z: f64, about_x: f64) -> Self {
        self.place = DMat4::from_rotation_x(about_x) * DMat4::from_rotation_z(about_z) * self.place;
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
    let above = |cell, region, [x0, z0, x1, z1]: [f64; 4], position: [f32; 3]| {
        let u = (f64::from(position[0]) - x0) / (x1 - x0);
        let v = (z1 - f64::from(position[2])) / (z1 - z0);
        region_uv(cell, region, u, v)
    };
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
            for ((uv, &position), paint) in mesh.uvs.iter_mut().zip(&mesh.positions).zip(&paints) {
                *uv = match *paint {
                    Paint::Face(cell, region) => {
                        region_uv(cell, region, f64::from(uv[0]), f64::from(uv[1]))
                    }
                    Paint::Own(cell, region, rect) => above(cell, region, rect, position),
                    Paint::Above(..) => *uv,
                };
            }
            mesh.apply_matrix4(&part.place);
            for ((uv, &position), paint) in mesh.uvs.iter_mut().zip(&mesh.positions).zip(&paints) {
                if let Paint::Above(cell, region, rect) = *paint {
                    *uv = above(cell, region, rect, position);
                }
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

/// Spread: an open crate racked with cartridges, its magenta lid slid back over
/// one end with the stencil on it. From above, rows of brass heads beside the lid.
fn shell_rack() -> Mesh {
    let [w, h, d] = [1.44, 0.38, 1.0];
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
    // The lid, pushed back off the cartridges and lying a little askew; built
    // lengthwise and turned, so its stencil runs across it.
    let lid = [d + 0.06, 0.74];
    parts.push(
        Part::cube([lid[0], 0.06, lid[1]], kind(PAINT))
            .top(lid_on(lid))
            .turned(FRAC_PI_2 + 0.05)
            .at(-0.36, h + 0.03, 0.02),
    );
    let case = 0.56;
    for x in [0.14, 0.38, 0.6] {
        for z in [-0.29, 0.0, 0.29] {
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

/// Rocket: three launch tubes in red, longer than the pallet, stacked on a pair of
/// wooden chocks, capped in rubber, collared in steel, webbing round the stack.
fn rocket_tubes() -> Mesh {
    let radius = 0.17;
    let length = 1.76;
    let chock = 0.14;
    let low = chock + radius;
    let high = low + 0.29;
    let mut parts = Vec::new();
    for x in [-0.5, 0.5] {
        parts.push(Part::cube([0.18, chock, 0.86], wood(CHOCK)).at(x, chock / 2.0, 0.0));
        parts.push(Part::cube([0.07, 0.02, 0.22], swatch(WEBBING)).at(
            x,
            high + radius + 0.01,
            0.0,
        ));
        for z in [-1.0, 1.0] {
            parts.push(Part::cube([0.07, high - chock, 0.02], swatch(WEBBING)).at(
                x,
                (high + chock) / 2.0,
                z * (0.18 + radius + 0.01),
            ));
        }
    }
    for (z, y) in [(-0.18, low), (0.18, low), (0.0, high)] {
        parts.push(
            Part::drum(radius, length, 10, kind(PAINT))
                .top(swatch(RUBBER))
                .bottom(swatch(RUBBER))
                .along_x()
                .at(0.0, y, z),
        );
        // Steel collars at the breech and the muzzle.
        for x in [-length / 2.0 + 0.12, length / 2.0 - 0.1] {
            parts.push(
                Part::drum(radius + 0.012, 0.07, 10, swatch(GUNMETAL))
                    .along_x()
                    .at(x, y, z),
            );
        }
    }
    build(parts)
}

/// Ricochet: four black fibre shell canisters banded together, their caps painted
/// purple with one stencil sprayed across the four of them.
fn shell_canisters() -> Mesh {
    let radius = 0.25;
    let height = 0.88;
    let pitch = 0.28;
    let reach = pitch + radius;
    let mut parts = Vec::new();
    for x in [-pitch, pitch] {
        for z in [-pitch, pitch] {
            parts.push(
                Part::drum(radius, height, 12, kind(SIDE))
                    .top(lid_over([-reach, -reach, reach, reach]))
                    .at(x, height / 2.0, z),
            );
        }
    }
    // Steel bands round the four of them.
    let span = 2.0 * reach + 0.02;
    for y in [0.22, 0.62] {
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

/// Piercing: the classic ammunition crate, a pine box under an overhanging teal
/// lid, two steel straps over it and steel handles at its ends.
fn ammo_crate() -> Mesh {
    let [w, h, d] = [1.5, 0.62, 1.0];
    let lid = [w + 0.06, 0.09, d + 0.06];
    let mut parts = vec![
        Part::cube([w, h, d], kind(PLAIN))
            .side(kind(SIDE))
            .at(0.0, h / 2.0, 0.0),
        Part::cube(lid, kind(PAINT))
            .top(lid_on([lid[0], lid[2]]))
            .at(0.0, h + lid[1] / 2.0, 0.0),
    ];
    straps(&mut parts, h + lid[1], lid[2], 0.52);
    for x in [-1.0, 1.0] {
        parts.push(Part::cube([0.05, 0.07, 0.42], swatch(GUNMETAL)).at(
            x * (w / 2.0 + 0.03),
            h * 0.68,
            0.0,
        ));
    }
    build(parts)
}

/// Rapid: olive ammo cans with orange lids, six in the bottom layer and three
/// across them, the stencil sprayed over the top three.
fn ammo_cans() -> Mesh {
    let [length, height, width] = [0.62, 0.36, 0.28];
    let lid = 0.05;
    let can = |parts: &mut Vec<Part>, at: [f64; 3], turn: f64, top: Paint| {
        let place = |part: Part| part.turned(turn).at(at[0], at[1], at[2]);
        parts.push(place(
            Part::cube([length, height, width], kind(PLAIN))
                .side(kind(SIDE))
                .at(0.0, height / 2.0, 0.0),
        ));
        parts.push(place(
            Part::cube([length + 0.02, lid, width + 0.02], kind(PAINT))
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
    for x in [-0.33, 0.33] {
        for z in [-0.31, 0.0, 0.31] {
            can(&mut parts, [x, 0.0, z], 0.0, kind(PAINT));
        }
    }
    let stencil = lid_over([-0.47, -0.33, 0.47, 0.33]);
    for x in [-0.31, 0.0, 0.31] {
        can(&mut parts, [x, height + lid, 0.0], FRAC_PI_2, stencil);
    }
    build(parts)
}

/// Shield: a stack of armour plate on wooden spacers, strapped, and its top plate
/// painted blue and propped up at one end on a chock, its face to the sky.
fn armour_plates() -> Mesh {
    let mut parts = Vec::new();
    let mut y = 0.0;
    for (index, (size, turn)) in [([1.5, 0.12, 1.04], 0.0), ([1.42, 0.12, 0.98], 0.06)]
        .into_iter()
        .enumerate()
    {
        let spacer = if index == 0 { 0.06 } else { 0.05 };
        for x in [-0.46, 0.46] {
            parts.push(Part::cube([0.12, spacer, 0.86], wood(CHOCK)).at(x, y + spacer / 2.0, 0.0));
        }
        y += spacer;
        parts.push(
            Part::cube(size, kind(PLAIN))
                .turned(turn)
                .at(0.0, y + size[1] / 2.0, 0.0),
        );
        y += size[1];
    }
    straps(&mut parts, y, 1.04, 0.36);
    // The blue plate, its left end on the stack and its right end on a chock.
    let [pw, ph, pd] = [1.46, 0.1, 1.0];
    let chock = 0.2;
    parts.push(Part::cube([0.16, chock, 0.8], wood(CHOCK)).at(0.56, y + chock / 2.0, 0.0));
    let tilt = (chock / (pw * 0.9)).atan();
    parts.push(
        Part::cube([pw, ph, pd], kind(PAINT))
            .top(lid_on([pw, pd]))
            .tipped(tilt, 0.0)
            .at(-0.02, y + ph / 2.0 + chock * 0.5, 0.0),
    );
    build(parts)
}

/// Speed: four yellow jerry cans standing two by two, their embossed faces out,
/// each with its carry handles and spout, each turned a little against the next.
fn jerry_cans() -> Mesh {
    let [w, h, d] = [0.66, 1.0, 0.34];
    let mut parts = Vec::new();
    for (turn, [x, z]) in [
        (0.04, [-0.36, -0.2]),
        (-0.07, [0.37, -0.21]),
        (0.08, [-0.37, 0.21]),
        (-0.03, [0.36, 0.2]),
    ] {
        let place = |part: Part| part.turned(turn).at(x, 0.0, z);
        parts.push(place(
            Part::cube([w, h, d], kind(PAINT))
                .side(kind(LID))
                .at(0.0, h / 2.0, 0.0),
        ));
        for (size, [px, py]) in [
            ([0.4, 0.05, 0.08], [-0.06, h + 0.1]),
            ([0.05, 0.1, 0.08], [-0.24, h + 0.05]),
            ([0.05, 0.1, 0.08], [0.12, h + 0.05]),
            ([0.1, 0.12, 0.1], [0.23, h + 0.06]),
        ] {
            parts.push(place(Part::cube(size, swatch(GUNMETAL)).at(px, py, 0.0)));
        }
    }
    build(parts)
}

/// Repair: an olive tool chest with a green lid stencilled with a spanner, drawer
/// pulls on its front and a carry bar over it, and two spare road wheels stacked
/// flat beside it.
fn tool_chest() -> Mesh {
    let [w, h, d] = [0.84, 0.46, 0.76];
    let [tw, th, td] = [0.76, 0.2, 0.66];
    let cx = 0.36;
    let mut parts = vec![
        Part::cube([w, h, d], kind(PLAIN))
            .side(kind(SIDE))
            .at(cx, h / 2.0, 0.0),
        Part::cube([tw, th, td], kind(PAINT))
            .top(lid_on([tw, td]))
            .at(cx, h + th / 2.0, 0.0),
    ];
    for z in [-1.0, 1.0] {
        for (x, y) in [(-0.2, 0.32), (0.2, 0.32), (-0.2, 0.14), (0.2, 0.14)] {
            parts.push(Part::cube([0.18, 0.035, 0.03], swatch(BRIGHT)).at(
                cx + x,
                y,
                z * (d / 2.0 + 0.012),
            ));
        }
    }
    let top = h + th;
    parts.push(Part::cube([0.5, 0.04, 0.05], swatch(GUNMETAL)).at(cx, top + 0.1, 0.0));
    for x in [-0.23, 0.23] {
        parts.push(Part::cube([0.04, 0.1, 0.05], swatch(GUNMETAL)).at(cx + x, top + 0.05, 0.0));
    }
    // Two road wheels, rubber tyres on steel hubs, the upper one off-centre.
    let tyre = 0.13;
    for (index, [x, z]) in [[-0.4, 0.0], [-0.36, 0.05]].into_iter().enumerate() {
        let y = index as f64 * tyre;
        parts.push(Part::drum(0.37, tyre, 14, swatch(RUBBER)).at(x, y + tyre / 2.0, z));
        parts.push(Part::drum(0.17, tyre + 0.02, 10, swatch(GUNMETAL)).at(
            x,
            y + tyre / 2.0 + 0.01,
            z,
        ));
    }
    build(parts)
}

/// Laser: a ribbed pale transit case for the defence set, latched, its whip
/// antenna raised from one corner and the emitter's dish on the other.
fn transit_case() -> Mesh {
    let [w, h, d] = [1.28, 0.5, 0.86];
    let lid = [w + 0.04, 0.12, d + 0.04];
    let mut parts = vec![
        Part::cube([w, h, d], kind(PLAIN))
            .side(kind(SIDE))
            .at(0.0, h / 2.0, 0.0),
        Part::cube(lid, kind(PAINT))
            .top(lid_on([lid[0], lid[2]]))
            .at(0.0, h + lid[1] / 2.0, 0.0),
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
    let top = h + lid[1];
    parts.push(Part::cube([0.12, 0.08, 0.12], swatch(GUNMETAL)).at(0.54, top + 0.04, -0.33));
    parts.push(Part::drum(0.02, 1.1, 5, swatch(BRIGHT)).at(0.54, top + 0.08 + 0.55, -0.33));
    // The dish on its stub at the back corner, tipped up and back.
    parts.push(Part::drum(0.05, 0.16, 6, swatch(GUNMETAL)).at(-0.5, top + 0.08, -0.3));
    parts.push(
        Part::new(cylinder_geometry(0.2, 0.05, 0.1, 12), kind(PAINT))
            .tipped(0.0, 0.4)
            .at(-0.5, top + 0.19, -0.3),
    );
    build(parts)
}

/// The pallet every pickup sits on: a tarp, the smoke grenade beside it in the
/// kind's paint, and the pallet itself twice: whole (three stringers and five
/// deck boards) while the supply is stocked, and crushed flat once a tank has
/// driven over it (its boards broken and splayed on the tarp, mapped into the
/// crushed cell). The `pickup-face` effect shows one or the other by the instance
/// data's `z` (1 while stocked).
fn pallet_geometry() -> Mesh {
    let [w, d] = PALLET_SIZE;
    let deck = 0.035;
    let stringer = PALLET_HEIGHT - deck;
    let [tw, td] = TARP_SIZE;
    let mut parts = vec![
        Part::cube(
            [tw, TARP_THICKNESS, td],
            Paint::Face(PICKUP_TARP_CELL, [0.0, 0.0, 1.0, 1.0]),
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
    // its length; the first takes the board a worn pallet may have lost.
    let pitch = (w - PALLET_BOARD_WIDTH) / (PALLET_BOARDS - 1) as f64;
    for (index, row) in [MISSING_BOARD, 1, 2, 1, 2].into_iter().enumerate() {
        let x = -w / 2.0 + PALLET_BOARD_WIDTH / 2.0 + pitch * index as f64;
        parts.push(
            Part::cube([d, deck, PALLET_BOARD_WIDTH], wood(board(row)))
                .turned(FRAC_PI_2)
                .at(x, stringer + deck / 2.0, 0.0),
        );
    }
    // The smoke grenade: a canister in the kind's paint, its fuse head and lever.
    let [mx, top, mz] = SMOKE_MARKER;
    let fuse = 0.05;
    parts.push(
        Part::drum(MARKER_RADIUS, top - fuse, 10, kind(PAINT))
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
    crushed_pallet(&mut parts);
    build(parts)
}

/// The pallet a tank has driven over: deck boards snapped and splayed flat over
/// the tarp, stringers cracked, the odd broken end propped on another. Every face
/// maps into the crushed cell, which the `pickup-face` effect folds away while the
/// supply is stocked.
fn crushed_pallet(parts: &mut Vec<Part>) {
    // (length, width, x, z, turn, lift, tip): boards lie flat, or propped on a
    // neighbour, `lift` higher and tipped by `tip` about their length.
    let pieces: [(f64, f64, f64, f64, f64, f64, f64); 9] = [
        (0.86, 0.27, -0.56, 0.04, 1.62, 0.0, 0.0),
        (0.52, 0.27, -0.22, -0.3, 1.38, 0.0, 0.0),
        (0.44, 0.26, -0.18, 0.34, 1.9, 0.0, 0.0),
        (0.7, 0.27, 0.18, 0.02, 1.47, 0.035, 0.08),
        (0.38, 0.25, 0.5, -0.32, 1.2, 0.0, 0.0),
        (0.46, 0.27, 0.56, 0.28, 1.75, 0.03, -0.1),
        (0.62, 0.11, 0.06, -0.46, 0.08, 0.0, 0.0),
        (0.7, 0.11, -0.3, 0.5, -0.12, 0.0, 0.0),
        (0.5, 0.11, 0.42, 0.48, 0.22, 0.04, 0.12),
    ];
    for (index, (length, width, x, z, turn, lift, tip)) in pieces.into_iter().enumerate() {
        let thick = if width < 0.15 { 0.1 } else { 0.035 };
        let row = (index % 4) as f64 * 0.25;
        parts.push(
            Part::cube(
                [length, thick, width],
                Paint::Face(PICKUP_CRUSHED_CELL, [0.0, row + 0.02, 1.0, row + 0.23]),
            )
            .tipped(0.0, tip)
            .turned(turn)
            .at(x, TARP_THICKNESS + thick / 2.0 + lift, z),
        );
    }
}

/// The lid a crushed crate throws out from under the tank that took it: a board
/// panel in the kind's paint with its stencil on top and two battens under it.
fn lid_geometry() -> Mesh {
    let [w, h, d] = LID_SIZE;
    let mut parts = vec![Part::cube([w, h, d], kind(PAINT)).top(lid_on([w, d]))];
    for x in [-0.3, 0.3] {
        parts.push(Part::cube([0.08, 0.04, d - 0.06], wood(board(1))).at(x, -h / 2.0 - 0.02, 0.0));
    }
    build(parts)
}

/// The atlas `v` above which a pallet-cell UV belongs to the board a worn pallet
/// may be missing: halfway between that board's region and the one below it.
fn missing_board_from() -> f64 {
    let [_, from] = atlas_cell_uv(PICKUP_PALLET_CELL, 0.0, board(MISSING_BOARD)[1]);
    let [_, below] = atlas_cell_uv(PICKUP_PALLET_CELL, 0.0, board(MISSING_BOARD - 1)[3]);
    (from + below) / 2.0
}

/// The shared supply paint: the atlas, worn and matte, with the `pickup-face`
/// effect choosing the kind's tile and the pallet's state; its first param is
/// [`missing_board_from`].
pub fn pickup_material() -> Arc<Material> {
    MATERIALS.get_or_insert(0, || {
        let atlas = TextureRef {
            wrap: Wrap::Clamp,
            anisotropy: 4,
            ..TextureRef::file(PICKUP_ATLAS_PATH)
        };
        Material {
            map: Some(atlas),
            roughness: 0.84,
            metalness: 0.06,
            effect: Effect::Custom {
                name: PICKUP_FACE_EFFECT,
                params: vec![missing_board_from() as f32, 0.0, 0.0, 0.0],
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

/// The pallet's and the lid's keys in the mesh cache, clear of every kind's.
const PALLET_KEY: u8 = 100;
const LID_KEY: u8 = 101;

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
/// paints the smoke grenade for the kind and its `z` shows it whole or crushed.
pub fn pickup_pallet() -> Node {
    let mesh = SHARED.get_or_insert(PALLET_KEY, pallet_geometry);
    let mut group = Node::default();
    group.children.push(part(mesh));
    group
}

/// The lid a crushed crate throws, shared by every kind, centred on its panel;
/// its instance data's `xy` paints it for the kind.
pub fn pickup_lid() -> Node {
    let mesh = SHARED.get_or_insert(LID_KEY, lid_geometry);
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
            PICKUP_CRUSHED_CELL,
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
        assert!(inside(&lid_geometry()));
    }

    /// Only the pallet's edge board takes the board a worn pallet may lose, so the
    /// effect can drop it without touching a neighbour; no item uses it.
    #[test]
    fn only_one_deck_board_can_go_missing() {
        let from = missing_board_from() as f32;
        let [u0, _] = atlas_cell_uv(PICKUP_PALLET_CELL, 0.0, 0.0);
        let missing = |mesh: &Mesh| {
            mesh.uvs
                .iter()
                .filter(|&&[u, v]| f64::from(u) > u0 - 1e-6 && v > from)
                .count()
        };
        // One board: a box's 24 vertices.
        assert_eq!(missing(&pallet_geometry()), 24);
        assert_eq!(missing(&lid_geometry()), 0);
        for kind in PickupKind::ALL {
            assert_eq!(missing(&item_geometry(kind)), 0, "{kind:?}");
        }
    }

    /// Every kind stands on its pallet's deck within its footprint (rocket tubes
    /// may overhang its ends), low enough to read as a load rather than cover.
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
            assert!(high.x - low.x <= w + 0.2, "{kind:?} {low} {high}");
            assert!(high.z - low.z <= d + 0.05, "{kind:?} {low} {high}");
            assert!(high.y < 1.9, "{kind:?}");
        }
    }
}
