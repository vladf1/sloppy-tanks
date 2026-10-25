//! Pickups as military supply (successor of `pickup-visuals.ts` and
//! `pickup-atlas.ts`): every kind ships as its own piece of military stock in its
//! own drab material (raw pine, olive drab, black fibre, rolled steel, gunmetal), so
//! its silhouette names it from the gameplay camera, and one broad band of worn
//! enamel in its own colour names it again, with the kind's code sprayed in it:
//! across the middle of a crate's lid and on down its faces, across a tarp lashed
//! over rocket tubes, round shell canisters near their caps, over the tops of
//! jerry cans. Nine hues, one per kind, none repeated. Each stands on a raw wooden
//! pallet in a scuffed patch of ground, drawn larger than its pallet's deck so the
//! kind reads from the gameplay camera, the smoke grenade that marks it standing in
//! the dirt off the pallet's back corner with its fuse; no poles, flags, painted rings
//! or pads.
//! An emptied drop is its bare pallet, its grenade still smoking thinly. Everything
//! is painted from one atlas whose tile for the kind carries the body's surface,
//! the kind's band and its sprayed stencil. A collected drop sends one of its own
//! pieces aboard the tank that took it ([`pickup_parcel`]) and pops its lid off
//! ([`pickup_board`]).
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

/// The kind's signal colour: the smoke its grenade gives off where it leaves the
/// fuse, the paint's hue a little stronger than the worn band but short of a
/// flare's, so each wisp names its kind before it thins and greys as it climbs.
pub fn pickup_signal(kind: PickupKind) -> u32 {
    match kind {
        PickupKind::Spread => 0xc8569a,
        PickupKind::Rocket => 0xde7036,
        PickupKind::Rapid => 0xa2c242,
        PickupKind::Speed => 0xe6c23c,
        PickupKind::Repair => 0x4cae5e,
        PickupKind::Piercing => 0x30aa9e,
        PickupKind::Shield => 0x74a6d4,
        PickupKind::Ricochet => 0x8c68cc,
        PickupKind::Laser => 0xe6e4dc,
    }
}

/// The kind's colour as supply paint (`PICKUP_PAINTS` in `scripts/asset-data.ts`):
/// the broad band on its load. Nine hues, none repeated (raspberry,
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
/// The load on the pallet is drawn this much larger again, the pallet staying its
/// size: the kind's silhouette and band name it at the gameplay camera, and the load
/// overhangs the deck's edges a little, as a big load does.
pub const LOAD_SCALE: f64 = 1.3;

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
/// body's side strip (for side faces: its surface, the kind's band down its middle
/// or round it near the top, dark at its foot), a strip of plain paint (small parts
/// in the kind's colour) and the lid, the band across its middle with the code
/// sprayed in it. `BODY` is the bare surface beside the lid's band, for faces that
/// show no paint; `BAND` is the middle of the lid, for a fragment of it.
type Region = [f64; 4];
const SIDE: Region = [0.0, 0.75, 1.0, 1.0];
const BODY: Region = [0.1, 0.12, 0.27, 0.5];
const PAINT: Region = [0.02, 0.633, 0.98, 0.742];
const LID: Region = [0.0, 0.0, 1.0, 0.625];
const BAND: Region = [0.2, 0.08, 0.8, 0.5];
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
pub const PALLET_SIZE: [f64; 2] = [1.86, 1.3];
pub const PALLET_HEIGHT: f64 = 0.15;
const PALLET_BOARDS: usize = 5;
const PALLET_BOARD_WIDTH: f64 = 0.3;
/// The smoke grenade marking the drop, standing in the dirt off the pallet's back
/// right corner, clear of every load however wide, where the overhead camera sees
/// it: its top (x, height, z) where the signal smoke leaves it, and its radius.
pub const SMOKE_MARKER: [f64; 3] = [1.1, 0.35, -0.82];
const MARKER_RADIUS: f64 = 0.085;
/// The scuffed ground under the drop: darkness under the pallet and just past its
/// edges (`CONTACT_SPILL`), dust banked `DRIFT_BANK` out from its edges in
/// `DRIFT_COLOR` (linear) at `DRIFT_OPACITY`, fading out `CONTACT_REACH` past them,
/// at this height over the ground.
const CONTACT_DARKNESS: f32 = 0.74;
const CONTACT_SPILL: f64 = 0.03;
const CONTACT_REACH: f64 = 0.72;
const CONTACT_LIFT: f64 = 0.01;
const DRIFT_BANK: f64 = 0.15;
const DRIFT_OPACITY: f32 = 0.56;
const DRIFT_COLOR: [f32; 3] = [0.2, 0.16, 0.11];

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

/// The lid of a box `size` (x, height, z): the band down its edges, bare ends and
/// underside, its top stencilled.
fn lid(size: [f64; 3]) -> Part {
    Part::cube(size, kind(SIDE))
        .end(kind(BODY))
        .bottom(kind(BODY))
        .top(lid_on([size[0], size[2]]))
}

/// A box `size` (x, height, z) standing on y = 0: the band down its faces along
/// z, bare ends, top and underside.
fn body(size: [f64; 3]) -> Part {
    Part::cube(size, kind(BODY))
        .side(kind(SIDE))
        .at(0.0, size[1] / 2.0, 0.0)
}

/// Spread: an open pine crate racked with cartridges, its lid slid back over one
/// end with the band and code on it. From above, rows of brass heads beside it.
fn shell_rack() -> Mesh {
    let [w, h, d] = [1.44, 0.38, 1.0];
    let wall = 0.05;
    let mut parts = vec![Part::cube([w, 0.04, d], kind(BODY)).at(0.0, 0.02, 0.0)];
    for z in [-1.0, 1.0] {
        parts.push(Part::cube([w, h, wall], kind(BODY)).side(kind(SIDE)).at(
            0.0,
            h / 2.0,
            z * (d - wall) / 2.0,
        ));
    }
    for x in [-1.0, 1.0] {
        parts.push(Part::cube([wall, h, d - 2.0 * wall], kind(BODY)).at(
            x * (w - wall) / 2.0,
            h / 2.0,
            0.0,
        ));
    }
    // The lid, pushed back off the cartridges and lying a little askew; built
    // lengthwise and turned, so its band runs along the crate.
    parts.push(
        lid([d + 0.06, 0.06, 0.74])
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

/// Rocket: three olive drab launch tubes, as long as the pallet, stacked on a pair
/// of wooden chocks, capped in rubber and collared in steel, a drab tarp lashed
/// over the middle of the stack with webbing, the band painted across it.
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
            Part::drum(radius, length, 10, kind(BODY))
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
    parts.push(lid([tw, 0.05, td]).at(0.0, top, 0.0));
    for z in [-1.0, 1.0] {
        parts.push(
            Part::cube([tw - 0.04, drape, 0.03], kind(SIDE))
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

/// Ricochet: four black fibre shell canisters banded together, each painted round
/// near its cap, the code sprayed across the four caps.
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

/// Piercing: the classic ammunition crate, a pine box under an overhanging lid,
/// the band over both, two steel straps over it and steel handles at its ends.
fn ammo_crate() -> Mesh {
    let [w, h, d] = [1.42, 0.62, 0.96];
    let top = [w + 0.06, 0.09, d + 0.06];
    let mut parts = vec![body([w, h, d]), lid(top).at(0.0, h + top[1] / 2.0, 0.0)];
    straps(&mut parts, h + top[1], top[2], 0.52);
    for x in [-1.0, 1.0] {
        parts.push(Part::cube([0.05, 0.07, 0.42], swatch(GUNMETAL)).at(
            x * (w / 2.0 + 0.03),
            h * 0.68,
            0.0,
        ));
    }
    build(parts)
}

/// Rapid: black ammo cans, each with the band down its faces, six in the bottom
/// layer and three across them, the band and code sprayed over the top three.
fn ammo_cans() -> Mesh {
    let [length, height, width] = [0.62, 0.36, 0.28];
    let lid = 0.05;
    let can = |parts: &mut Vec<Part>, at: [f64; 3], turn: f64, top: Paint| {
        let place = |part: Part| part.turned(turn).at(at[0], at[1], at[2]);
        parts.push(place(body([length, height, width])));
        parts.push(place(
            Part::cube([length + 0.02, lid, width + 0.02], kind(SIDE))
                .end(kind(BODY))
                .bottom(kind(BODY))
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
            can(&mut parts, [x, 0.0, z], 0.0, kind(BODY));
        }
    }
    let stencil = lid_over([-0.47, -0.33, 0.47, 0.33]);
    for x in [-0.31, 0.0, 0.31] {
        can(&mut parts, [x, height + lid, 0.0], FRAC_PI_2, stencil);
    }
    build(parts)
}

/// Shield: a stack of armour plate on wooden spacers, strapped, and its top plate
/// propped up at one end on a chock, its face to the sky with the band across it.
fn armour_plates() -> Mesh {
    let mut parts = Vec::new();
    let mut y = 0.0;
    for (index, (size, turn)) in [([1.44, 0.12, 1.0], 0.0), ([1.38, 0.12, 0.94], 0.06)]
        .into_iter()
        .enumerate()
    {
        let spacer = if index == 0 { 0.06 } else { 0.05 };
        for x in [-0.46, 0.46] {
            parts.push(Part::cube([0.12, spacer, 0.86], wood(CHOCK)).at(x, y + spacer / 2.0, 0.0));
        }
        y += spacer;
        parts.push(
            Part::cube(size, kind(BODY))
                .turned(turn)
                .at(0.0, y + size[1] / 2.0, 0.0),
        );
        y += size[1];
    }
    straps(&mut parts, y, 1.0, 0.36);
    // The top plate, its left end on the stack and its right end on a chock.
    let [pw, ph, pd] = [1.4, 0.1, 0.96];
    let chock = 0.2;
    parts.push(Part::cube([0.16, chock, 0.8], wood(CHOCK)).at(0.56, y + chock / 2.0, 0.0));
    let tilt = (chock / (pw * 0.9)).atan();
    parts.push(
        lid([pw, ph, pd])
            .tipped(tilt, 0.0)
            .at(-0.02, y + ph / 2.0 + chock * 0.5, 0.0),
    );
    build(parts)
}

/// Speed: four olive drab jerry cans standing two by two, their embossed faces
/// out, each with its carry handles and spout, each turned a little against the
/// next; their tops and the upper part of their faces painted, the code on them.
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
                .bottom(kind(BODY))
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

/// Repair: a squat ribbed gunmetal tool chest under an overhanging lid, the band
/// over both, two rows of drawer pulls down its front and back, a carry bar over
/// the lid and a pry bar and a sledge lying along the deck in front of it.
fn tool_chest() -> Mesh {
    let [w, h, d] = [1.12, 0.6, 0.74];
    let [tw, th, td] = [1.16, 0.09, 0.78];
    let cz = -0.1;
    let mut parts = vec![
        body([w, h, d]).at(0.0, 0.0, cz),
        lid([tw, th, td]).at(0.0, h + th / 2.0, cz),
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

/// Laser: a ribbed olive drab radio transit case for the defence set, the band
/// over its lid and down its faces, latched, its whip antenna raised from one
/// corner on its mount and a field handset in its cradle on the lid's other end.
fn transit_case() -> Mesh {
    let [w, h, d] = [1.28, 0.5, 0.86];
    let top = [w + 0.04, 0.12, d + 0.04];
    let mut parts = vec![body([w, h, d]), lid(top).at(0.0, h + top[1] / 2.0, 0.0)];
    for x in [-0.4, 0.4] {
        for z in [-1.0, 1.0] {
            parts.push(Part::cube([0.12, 0.15, 0.035], swatch(GUNMETAL)).at(
                x,
                h - 0.03,
                z * (d / 2.0 + 0.02),
            ));
        }
    }
    let top = h + top[1];
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
/// raw pine, one of which a worn pallet has lost, and the smoke grenade standing off
/// its back corner, olive drab with its top in the kind's paint, as M18 grenades
/// are marked, its steel fuse, spoon and pull ring catching the light. An emptied
/// drop is just this.
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
    // The smoke grenade in the dirt: an olive canister, its top band in the kind's
    // paint, its bright fuse head, the spoon down its side and the pull ring.
    let [mx, top, mz] = SMOKE_MARKER;
    let fuse = 0.06;
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
            .at(mx, top - fuse - band / 2.0, mz),
    );
    parts.push(Part::drum(MARKER_RADIUS * 0.6, fuse, 8, swatch(BRIGHT)).at(
        mx,
        top - fuse / 2.0,
        mz,
    ));
    parts.push(Part::cube([0.026, body * 0.8, 0.034], swatch(BRIGHT)).at(
        mx + MARKER_RADIUS + 0.01,
        top - fuse - body * 0.4,
        mz,
    ));
    parts.push(
        Part::cube([0.07, 0.012, 0.012], swatch(BRIGHT))
            .tipped(0.5, 0.0)
            .at(mx - MARKER_RADIUS * 0.7, top - 0.01, mz + 0.03),
    );
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
        (CONTACT_SPILL, 0.05, CONTACT_DARKNESS, [0.0, 0.0, 0.0], 0.0),
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

/// Build `parts` into one mesh centred on its bounds, so it can tumble about its
/// middle and be stood on a deck by its half height.
fn centred(parts: Vec<Part>) -> Mesh {
    let mut mesh = build(parts);
    let bounds = mesh.bounding_box();
    let center = bounds.center();
    mesh.apply_matrix4(&DMat4::from_translation(-center));
    mesh
}

/// What a collected drop sends aboard the tank that took it: one piece of its own
/// stock, small enough to stow on a rear deck, with the kind's band on it.
fn parcel_geometry(supply: PickupKind) -> Mesh {
    match supply {
        // A bundle of cartridges lying along x, a painted strap round it.
        PickupKind::Spread => {
            let mut parts = Vec::new();
            let case = 0.46;
            for (z, y) in [(-0.1, 0.0), (0.1, 0.0), (0.0, 0.17)] {
                parts.push(
                    Part::drum(0.09, case, 8, brass(CASE))
                        .bottom(brass(PRIMER))
                        .along_x()
                        .at(-0.08, y, z),
                );
                parts.push(
                    Part::new(cone_geometry(0.085, 0.2, 8), brass(PROJECTILE))
                        .along_x()
                        .at(-0.08 + case / 2.0 + 0.1, y, z),
                );
            }
            for x in [-0.2, 0.05] {
                parts.push(
                    Part::drum(0.2, 0.08, 10, kind(PAINT))
                        .along_x()
                        .at(x, 0.07, 0.0),
                );
            }
            centred(parts)
        }
        // A launch tube, rubber capped, the band painted round its middle.
        PickupKind::Rocket => {
            let radius = 0.13;
            centred(vec![
                Part::drum(radius, 0.96, 10, kind(BODY))
                    .top(swatch(RUBBER))
                    .bottom(swatch(RUBBER))
                    .along_x(),
                Part::drum(radius + 0.008, 0.32, 10, kind(PAINT)).along_x(),
                Part::drum(radius + 0.012, 0.06, 10, swatch(GUNMETAL))
                    .along_x()
                    .at(0.4, 0.0, 0.0),
            ])
        }
        // A shell canister, standing, painted round near its cap.
        PickupKind::Ricochet => {
            let [radius, height] = [0.2, 0.66];
            centred(vec![
                Part::drum(radius, height, 12, kind(SIDE))
                    .top(lid_on([2.0 * radius, 2.0 * radius]))
                    .bottom(swatch(RUBBER)),
            ])
        }
        // A long ammunition box under its lid, strapped.
        PickupKind::Piercing => {
            let [w, h, d] = [0.82, 0.24, 0.36];
            let top = [w + 0.04, 0.05, d + 0.04];
            let mut parts = vec![body([w, h, d]), lid(top).at(0.0, h + top[1] / 2.0, 0.0)];
            straps(&mut parts, h + top[1], top[2], 0.28);
            centred(parts)
        }
        // An ammo can with its latch and carry handle.
        PickupKind::Rapid => {
            let [w, h, d] = [0.52, 0.3, 0.24];
            let top = [w + 0.02, 0.04, d + 0.02];
            centred(vec![
                body([w, h, d]),
                lid(top).at(0.0, h + top[1] / 2.0, 0.0),
                Part::cube([0.22, 0.025, 0.05], swatch(GUNMETAL)).at(0.0, h + top[1] + 0.02, 0.0),
                Part::cube([0.035, 0.1, 0.1], swatch(GUNMETAL)).at(w / 2.0 + 0.015, h - 0.03, 0.0),
            ])
        }
        // An armour plate, the band across its face.
        PickupKind::Shield => centred(vec![lid([0.8, 0.07, 0.58])]),
        // A jerry can lying on its face-down side, the painted end and code up.
        PickupKind::Speed => {
            let [w, h, d] = [0.46, 0.66, 0.24];
            let mut parts = vec![
                Part::cube([w, h, d], kind(PAINT))
                    .side(kind(LID))
                    .bottom(kind(BODY))
                    .at(0.0, h / 2.0, 0.0),
            ];
            for (size, [px, py]) in [
                ([0.28, 0.04, 0.07], [-0.04, h + 0.07]),
                ([0.04, 0.07, 0.07], [-0.17, h + 0.035]),
                ([0.04, 0.07, 0.07], [0.09, h + 0.035]),
                ([0.08, 0.09, 0.08], [0.16, h + 0.045]),
            ] {
                parts.push(Part::cube(size, swatch(GUNMETAL)).at(px, py, 0.0));
            }
            let parts = parts
                .into_iter()
                .map(|part| part.tipped(0.0, -FRAC_PI_2))
                .collect();
            centred(parts)
        }
        // A tool box under its lid, a carry bar over it.
        PickupKind::Repair => {
            let [w, h, d] = [0.6, 0.26, 0.3];
            let top = [w + 0.03, 0.05, d + 0.03];
            let rise = h + top[1];
            centred(vec![
                body([w, h, d]),
                lid(top).at(0.0, h + top[1] / 2.0, 0.0),
                Part::cube([0.36, 0.03, 0.04], swatch(GUNMETAL)).at(0.0, rise + 0.07, 0.0),
                Part::cube([0.03, 0.07, 0.04], swatch(GUNMETAL)).at(-0.17, rise + 0.035, 0.0),
                Part::cube([0.03, 0.07, 0.04], swatch(GUNMETAL)).at(0.17, rise + 0.035, 0.0),
            ])
        }
        // A field radio set under its lid, its short whip raised.
        PickupKind::Laser => {
            let [w, h, d] = [0.54, 0.3, 0.34];
            let top = [w + 0.02, 0.06, d + 0.02];
            let rise = h + top[1];
            centred(vec![
                body([w, h, d]),
                lid(top).at(0.0, h + top[1] / 2.0, 0.0),
                Part::drum(0.015, 0.5, 5, swatch(BRIGHT)).at(0.2, rise + 0.25, -0.1),
                Part::cube([0.07, 0.05, 0.07], swatch(GUNMETAL)).at(0.2, rise + 0.025, -0.1),
            ])
        }
    }
}

/// A board of a broken-open load: a plank of the lid, its top a piece of the band,
/// centred so it can tumble; a collection throws it off and two thin splinters of
/// it (the same board, scaled).
fn board_geometry() -> Mesh {
    centred(vec![
        Part::cube([0.62, 0.045, 0.32], kind(SIDE))
            .end(kind(BODY))
            .bottom(kind(BODY))
            .top(kind(BAND)),
    ])
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

/// The pallet's, its shade's and the board's keys in the mesh cache, and the
/// first of the kinds' parcels', clear of every kind's item.
const PALLET_KEY: u8 = 100;
const SHADE_KEY: u8 = 101;
const BOARD_KEY: u8 = 102;
const PARCEL_KEYS: u8 = 110;

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

/// The piece of `kind`'s stock a collected drop sends aboard, centred on its
/// bounds; its instance data's `xy` paints it for the kind.
pub fn pickup_parcel(kind: PickupKind) -> Node {
    let mut group = Node::default();
    group.children.push(part(
        SHARED.get_or_insert(PARCEL_KEYS + kind as u8, || parcel_geometry(kind)),
    ));
    group
}

/// A board of a broken-open load, centred on its bounds, shared by every kind; its
/// instance data's `xy` paints it for the kind.
pub fn pickup_board() -> Node {
    let mut group = Node::default();
    group
        .children
        .push(part(SHARED.get_or_insert(BOARD_KEY, board_geometry)));
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
        assert!(inside(&board_geometry()));
        for kind in PickupKind::ALL {
            assert!(inside(&parcel_geometry(kind)), "{kind:?}");
        }
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

    /// A kind's load as drawn on its pallet (`LOAD_SCALE`): its lowest and highest
    /// corners in the pallet's units.
    fn drawn_bounds(kind: PickupKind) -> (DVec3, DVec3) {
        let mut low = DVec3::splat(f64::MAX);
        let mut high = DVec3::splat(f64::MIN);
        for p in &item_geometry(kind).positions {
            let p = DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]));
            low = low.min(p * LOAD_SCALE);
            high = high.max(p * LOAD_SCALE);
        }
        (low, high)
    }

    /// Every kind stands on its pallet's deck, overhanging its edges only a little as
    /// drawn (rocket tubes further past its ends), low enough to read as a load
    /// rather than cover.
    #[test]
    fn every_item_fits_its_pallet() {
        let [w, d] = PALLET_SIZE;
        for kind in PickupKind::ALL {
            let (low, high) = drawn_bounds(kind);
            assert!(low.y > -1e-6, "{kind:?} stands on the deck");
            assert!(high.x - low.x <= w + 0.5, "{kind:?} {low} {high}");
            assert!(high.z - low.z <= d + 0.25, "{kind:?} {low} {high}");
            assert!(high.y < 2.4, "{kind:?}");
        }
    }

    /// The smoke grenade stands off the pallet, clear of every load as drawn, so the
    /// overhead camera always sees what marks the drop.
    #[test]
    fn the_smoke_grenade_stands_clear_of_every_load() {
        let [mx, _, mz] = SMOKE_MARKER;
        let [w, d] = PALLET_SIZE;
        assert!(mx - MARKER_RADIUS > w / 2.0 && -mz - MARKER_RADIUS > d / 2.0);
        for kind in PickupKind::ALL {
            let (low, high) = drawn_bounds(kind);
            let clear = mx - MARKER_RADIUS > high.x || mz + MARKER_RADIUS < low.z;
            assert!(clear, "{kind:?} {low} {high}");
        }
    }
}
