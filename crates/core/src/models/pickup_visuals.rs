//! Pickups as military supply (successor of `pickup-visuals.ts` and
//! `pickup-atlas.ts`): every kind ships as its own piece of military stock in its
//! own material (raw pine, olive drab, black fibre, rolled steel, gunmetal), so its
//! silhouette names it from the gameplay camera, and one large block of worn enamel
//! in its own colour on top of the load names it again: the lid slid back over an
//! open rack of shells, a canvas tarp lashed over rocket tubes, the caps of shell
//! canisters, a crate's lid, ammo can lids, the top armour plate, whole jerry cans,
//! a tool chest's lid and a radio case's lid. Nine hues, one per kind, none
//! repeated. Each stands on a raw wooden pallet in a scuffed patch of ground, with
//! the smoke grenade that marks it standing at its back corner; no poles, flags,
//! painted rings or pads. An emptied drop is just its bare pallet. Everything is
//! painted from one atlas whose tile for the kind carries the body's surface, the
//! kind's paint and its sprayed stencil.
//!
//! Faces that show the kind map into the first kind's tile and the `pickup-face`
//! effect moves them into the instance's tile (instance data `xy`); brass, steel,
//! rubber and wood map into shared cells the effect leaves alone. Every kind's item
//! is one mesh, so all pickups of a kind draw as one instanced call, and they share
//! one material. First person fades them through the instance opacity, whose
//! blended variant warm-up compiles.

use std::f64::consts::{FRAC_PI_2, PI};
use std::sync::Arc;

use glam::{DMat4, DVec3};

use super::model_primitives::Cache;
use crate::geometry::{
    Attribute, Mesh, VERTEX_ALPHA, box_geometry, cone_geometry, cylinder_geometry, merge_geometries,
};
use crate::scene::{Effect, Material, Node, Shading, TextureRef, Wrap};
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

/// The kind's signal colour: the smoke its grenade gives off, the paint's hue
/// greyed toward smoke and lightened, so a field of drops wears nine soft tints
/// rather than nine bright plumes, yet each still reads apart from dust and soot.
pub fn pickup_signal(kind: PickupKind) -> u32 {
    match kind {
        PickupKind::Spread => 0xc880a8,
        PickupKind::Rocket => 0xd88e66,
        PickupKind::Rapid => 0xb2c272,
        PickupKind::Speed => 0xe0c472,
        PickupKind::Repair => 0x84bc8a,
        PickupKind::Piercing => 0x78b8b0,
        PickupKind::Shield => 0x94b4d2,
        PickupKind::Ricochet => 0xa490cc,
        PickupKind::Laser => 0xdcdad2,
    }
}

/// The kind's colour as supply paint (`PICKUP_PAINTS` in `scripts/asset-data.ts`):
/// the big painted block on top of its load. Nine hues, none repeated (raspberry,
/// burnt orange, violet, teal, lime, pale steel blue, fuel yellow, green and
/// off-white), each the family of the kind's HUD colour where that does not collide
/// with another kind's, worn military enamel rather than a toy colour; the
/// raspberry leans to purple and the steel blue is pale and grey, so neither reads
/// as a team's red or blue.
pub fn pickup_paint(kind: PickupKind) -> u32 {
    match kind {
        PickupKind::Spread => 0xb44c88,
        PickupKind::Rocket => 0xcc602c,
        PickupKind::Rapid => 0x9eb63a,
        PickupKind::Speed => 0xe2b830,
        PickupKind::Repair => 0x3c9a4e,
        PickupKind::Piercing => 0x229a90,
        PickupKind::Shield => 0x7aa4c6,
        PickupKind::Ricochet => 0x7c5cbe,
        PickupKind::Laser => 0xe6e2d6,
    }
}

/// The packed atlas under `public/` (`scripts/generate-pickup-atlas.ts`): nine kind
/// tiles in a 3x3 block; in a fourth column the pallet's planks and swatches of
/// steel, rubber and webbing; below the block cartridge brass.
pub const PICKUP_ATLAS_PATH: &str = "textures/pickups/atlas.webp";
pub const PICKUP_ICON_SIZE: f64 = 256.0;
pub const PICKUP_ATLAS_PADDING: f64 = 16.0;
pub const PICKUP_ATLAS_STRIDE: f64 = PICKUP_ICON_SIZE + PICKUP_ATLAS_PADDING * 2.0;
pub const PICKUP_ATLAS_SIZE: f64 = PICKUP_ATLAS_STRIDE * 4.0;
pub const PICKUP_PALLET_CELL: (u32, u32) = (3, 0);
pub const PICKUP_SWATCH_CELL: (u32, u32) = (3, 2);
pub const PICKUP_BRASS_CELL: (u32, u32) = (0, 3);

/// Everything a drop shows is drawn this much larger than it is modelled, so a
/// load reads at the gameplay camera; the pickup's reach is the simulation's.
pub const SUPPLY_SCALE: f64 = 1.2;

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

/// The vertex effect that moves a kind face's UVs into the instance's atlas tile,
/// crushes a pallet a hull rolls onto and wears each drop a little differently (`crates/render/src/presentation/shaders/pickup_face.wgsl`).
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
/// The smoke grenade marking the drop, standing on the ground at the pallet's back
/// right corner, where the overhead camera sees it past the load: its top (x,
/// height, z) where the signal smoke leaves it, and its radius.
pub const SMOKE_MARKER: [f64; 3] = [0.92, 0.32, -0.44];
const MARKER_RADIUS: f64 = 0.1;
/// The scuffed ground under the drop: darkness under the pallet, dust banked
/// `DRIFT_BANK` out from its edges in `DRIFT_COLOR` (linear) at `DRIFT_OPACITY`,
/// fading out `CONTACT_REACH` past them, at this height over the ground.
const CONTACT_DARKNESS: f32 = 0.66;
const CONTACT_REACH: f64 = 0.62;
const CONTACT_LIFT: f64 = 0.01;
const DRIFT_BANK: f64 = 0.1;
const DRIFT_OPACITY: f32 = 0.5;
const DRIFT_COLOR: [f32; 3] = [0.2, 0.16, 0.11];
/// The parcel's box (x, height, z), without its lid.
pub const PARCEL_SIZE: [f64; 3] = [0.56, 0.3, 0.38];

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

/// Rocket: three olive drab launch tubes, longer than the pallet, stacked on a
/// pair of wooden chocks, capped in rubber and collared in steel, a canvas tarp in
/// the kind's paint lashed over the middle of the stack with webbing.
fn rocket_tubes() -> Mesh {
    let radius = 0.17;
    let length = 1.76;
    let chock = 0.14;
    let low = chock + radius;
    let high = low + 0.29;
    let mut parts = Vec::new();
    for x in [-0.5, 0.5] {
        parts.push(Part::cube([0.18, chock, 0.86], wood(CHOCK)).at(x, chock / 2.0, 0.0));
    }
    for (z, y) in [(-0.18, low), (0.18, low), (0.0, high)] {
        parts.push(
            Part::drum(radius, length, 10, kind(PLAIN))
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
    // The tarp: a sheet over the top of the stack, its sides hanging down past the
    // low tubes, stencilled on top; webbing lashed round it near each end.
    let [tw, td] = [0.98, 2.0 * (0.18 + radius) + 0.06];
    let top = high + radius + 0.02;
    let drape = top - low + 0.04;
    parts.push(
        Part::cube([tw, 0.05, td], kind(PAINT))
            .top(lid_on([tw, td]))
            .at(0.0, top, 0.0),
    );
    for z in [-1.0, 1.0] {
        parts.push(
            Part::cube([tw - 0.04, drape, 0.03], kind(PAINT))
                .tipped(0.0, -z * 0.1)
                .at(0.0, top - drape / 2.0, z * (td / 2.0 - 0.02)),
        );
    }
    for x in [-0.36, 0.36] {
        parts.push(Part::cube([0.07, 0.02, td + 0.04], swatch(WEBBING)).at(x, top + 0.03, 0.0));
        for z in [-1.0, 1.0] {
            parts.push(Part::cube([0.07, drape, 0.02], swatch(WEBBING)).at(
                x,
                top - drape / 2.0,
                z * (td / 2.0 + 0.02),
            ));
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
                    .bottom(swatch(RUBBER))
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

/// Repair: a squat ribbed gunmetal tool chest under an overhanging green lid
/// stencilled with a spanner, two rows of drawer pulls down its front and back, a
/// carry bar over the lid and a pry bar and a sledge lying along the deck in front
/// of it.
fn tool_chest() -> Mesh {
    let [w, h, d] = [1.12, 0.6, 0.74];
    let [tw, th, td] = [1.16, 0.09, 0.78];
    let cz = -0.1;
    let mut parts = vec![
        Part::cube([w, h, d], kind(PLAIN))
            .side(kind(SIDE))
            .at(0.0, h / 2.0, cz),
        Part::cube([tw, th, td], kind(PAINT))
            .top(lid_on([tw, td]))
            .at(0.0, h + th / 2.0, cz),
    ];
    for z in [-1.0, 1.0] {
        for (x, y) in [(-0.28, 0.34), (0.28, 0.34), (-0.28, 0.15), (0.28, 0.15)] {
            parts.push(Part::cube([0.22, 0.035, 0.03], swatch(BRIGHT)).at(
                x,
                y,
                cz + z * (d / 2.0 + 0.012),
            ));
        }
    }
    let top = h + th;
    parts.push(Part::cube([0.62, 0.04, 0.05], swatch(GUNMETAL)).at(0.0, top + 0.1, cz));
    for x in [-0.29, 0.29] {
        parts.push(Part::cube([0.04, 0.1, 0.05], swatch(GUNMETAL)).at(x, top + 0.05, cz));
    }
    // A pry bar and a sledge laid along the front of the deck.
    parts.push(
        Part::drum(0.025, 1.3, 5, swatch(GUNMETAL))
            .along_x()
            .turned(0.05)
            .at(0.0, 0.025, 0.36),
    );
    parts.push(
        Part::drum(0.03, 0.9, 5, wood(CHOCK))
            .along_x()
            .turned(-0.04)
            .at(-0.12, 0.03, 0.5),
    );
    parts.push(Part::cube([0.12, 0.1, 0.1], swatch(GUNMETAL)).at(0.38, 0.05, 0.48));
    build(parts)
}

/// Laser: a ribbed olive drab radio transit case for the defence set under an
/// off-white lid, latched, its whip antenna raised from one corner on its mount
/// and a field handset in its cradle on the lid's other end.
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
    // The handset lying in its cradle: a dark bar with an earpiece and mouthpiece.
    parts.push(Part::cube([0.4, 0.04, 0.16], swatch(GUNMETAL)).at(-0.36, top + 0.02, 0.22));
    for x in [-0.2, 0.2] {
        parts.push(Part::cube([0.1, 0.07, 0.13], swatch(RUBBER)).at(-0.36 + x, top + 0.07, 0.22));
    }
    parts.push(Part::cube([0.22, 0.05, 0.08], swatch(RUBBER)).at(-0.36, top + 0.09, 0.22));
    build(parts)
}

/// The pallet every pickup sits on, bare: three stringers and five deck boards of
/// raw pine, one of which a worn pallet has lost, and the smoke grenade on the
/// ground beside it, olive drab with its top in the kind's paint, as M18 grenades
/// are marked. An emptied drop is just this.
fn pallet_geometry() -> Mesh {
    let [w, d] = PALLET_SIZE;
    let deck = 0.035;
    let stringer = PALLET_HEIGHT - deck;
    let mut parts = Vec::new();
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
    // The smoke grenade: an olive canister, its top band in the kind's paint, its
    // fuse head and lever.
    let [mx, top, mz] = SMOKE_MARKER;
    let fuse = 0.05;
    let band = 0.07;
    let body = top - fuse - band;
    parts.push(
        Part::drum(MARKER_RADIUS, body, 10, swatch(WEBBING))
            .bottom(swatch(GUNMETAL))
            .at(mx, body / 2.0, mz),
    );
    parts.push(
        Part::drum(MARKER_RADIUS, band, 10, kind(PAINT))
            .top(swatch(GUNMETAL))
            .at(mx, body + band / 2.0, mz),
    );
    parts.push(
        Part::drum(MARKER_RADIUS * 0.55, fuse, 8, swatch(GUNMETAL)).at(mx, top - fuse / 2.0, mz),
    );
    parts.push(Part::cube([0.024, top * 0.7, 0.03], swatch(GUNMETAL)).at(
        mx + MARKER_RADIUS + 0.008,
        top * 0.6,
        mz,
    ));
    build(parts)
}

/// The scuffed ground round the drop's foot: a rounded rectangle over the pallet's
/// footprint, dark under it, then a band of dust drifted against its edges, ragged
/// on its outer edge and fading to nothing `CONTACT_REACH` out, in vertex colour
/// blended by the vertex alpha, so the drop sits in the ground rather than on a
/// plate. One mesh for every drop.
fn contact_shade_geometry() -> Mesh {
    const ARC: usize = 4;
    let [w, d] = PALLET_SIZE.map(|v| v / 2.0);
    let mut positions = Vec::new();
    let mut alpha = Vec::new();
    let mut colors = Vec::new();
    let rings = [
        (-0.05, 0.05, CONTACT_DARKNESS, [0.0, 0.0, 0.0], 0.0),
        (DRIFT_BANK, 0.1, DRIFT_OPACITY, DRIFT_COLOR, 0.04),
        (CONTACT_REACH, CONTACT_REACH, 0.0, DRIFT_COLOR, 0.22),
    ];
    for (ring, (grow, round, strength, color, ragged)) in rings.into_iter().enumerate() {
        for (corner, (sx, sz)) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]
            .into_iter()
            .enumerate()
        {
            for k in 0..ARC {
                let angle = (corner as f64 + k as f64 / (ARC - 1) as f64) * PI / 2.0;
                // Drifts lie deeper in some places than others: a fixed wobble.
                let wobble =
                    ragged * ((angle * 3.0 + ring as f64).sin() * 0.6 + (angle * 7.0).cos() * 0.4);
                let x = sx * (w + grow - round) + (round + wobble) * angle.cos();
                let z = sz * (d + grow - round) + (round + wobble) * angle.sin();
                positions.extend([x, CONTACT_LIFT, z]);
                alpha.push(strength);
                colors.push(color);
            }
        }
    }
    let ring = (4 * ARC) as u32;
    // The core as a fan over its corners; then the bands out to the rim.
    let mut indices: Vec<u32> = (1..ring - 1).flat_map(|i| [0, i + 1, i]).collect();
    for band in 0..rings.len() as u32 - 1 {
        let (inner, outer) = (band * ring, (band + 1) * ring);
        for i in 0..ring {
            let j = (i + 1) % ring;
            indices.extend([
                inner + i,
                inner + j,
                outer + j,
                inner + i,
                outer + j,
                outer + i,
            ]);
        }
    }
    let mut mesh = Mesh::from_f64(&positions, &[], &[], Some(indices));
    mesh.compute_vertex_normals();
    // Wound to face up, toward the overhead camera.
    if mesh.normals.iter().map(|n| n[1]).sum::<f32>() < 0.0
        && let Some(indices) = &mut mesh.indices
    {
        for triangle in indices.as_chunks_mut::<3>().0 {
            triangle.swap(1, 2);
        }
        mesh.compute_vertex_normals();
    }
    mesh.colors = colors;
    mesh.set_attribute(Attribute::vertex(VERTEX_ALPHA, 1, alpha));
    mesh
}

/// The parcel a collected drop sends aboard the tank that took it: a small ammo
/// box with a lid in the kind's paint, its stencil, a strap and a handle, built
/// about its middle so it can tumble.
fn parcel_geometry() -> Mesh {
    let [w, h, d] = PARCEL_SIZE;
    let lid = [w + 0.03, 0.06, d + 0.03];
    let mut parts = vec![
        Part::cube([w, h, d], kind(PLAIN))
            .side(kind(SIDE))
            .at(0.0, -lid[1] / 2.0, 0.0),
        Part::cube(lid, kind(PAINT))
            .top(lid_on([lid[0], lid[2]]))
            .at(0.0, h / 2.0, 0.0),
    ];
    parts.push(Part::cube([0.06, h + lid[1] + 0.01, d + 0.04], swatch(GUNMETAL)).at(0.0, 0.0, 0.0));
    parts.push(Part::cube([0.22, 0.03, 0.05], swatch(GUNMETAL)).at(0.0, h / 2.0 + lid[1], 0.0));
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
/// effect choosing the kind's tile, crushing a pallet under a hull and dropping a
/// worn pallet's edge board; its one param is [`missing_board_from`].
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

/// The pallet's, its shade's and the parcel's keys in the mesh cache, clear of
/// every kind's.
const PALLET_KEY: u8 = 100;
const SHADE_KEY: u8 = 101;
const PARCEL_KEY: u8 = 102;

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

/// The pallet under every pickup, shared by every kind, in its scuffed patch of
/// ground; its instance data's `xy` paints its smoke grenade's top for the kind.
pub fn pickup_pallet() -> Node {
    let mesh = SHARED.get_or_insert(PALLET_KEY, pallet_geometry);
    let mut group = Node::default();
    group.children.push(part(mesh));
    let shade = MATERIALS.get_or_insert(1, || Material {
        shading: Shading::Basic,
        vertex_colors: true,
        transparent: true,
        depth_write: false,
        polygon_offset: Some((-1.0, -4.0)),
        ..Material::standard(0xffffff, 0.0, 1.0)
    });
    group.children.push(Node::mesh(
        SHARED.get_or_insert(SHADE_KEY, contact_shade_geometry),
        shade,
    ));
    group
}

/// The parcel a collected drop sends aboard, shared by every kind; its instance
/// data's `xy` paints its lid for the kind.
pub fn pickup_parcel() -> Node {
    let mut group = Node::default();
    group
        .children
        .push(part(SHARED.get_or_insert(PARCEL_KEY, parcel_geometry)));
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
        assert!(inside(&parcel_geometry()));
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
