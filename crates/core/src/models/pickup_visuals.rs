//! Pickups as military supply (successor of `pickup-visuals.ts` and
//! `pickup-atlas.ts`): every kind ships as its own piece of military stock, so its
//! silhouette names it from the gameplay camera, and one large block of its own
//! worn enamel on the load itself names it again: an open rack of shells under a
//! plum lid, burnt-orange rocket tubes, shell canisters under violet caps, a
//! teal-lidded ammunition crate, black ammo cans under chartreuse lids, a black
//! armour plate propped on its stack, sand-yellow jerry cans, a green tool chest
//! beside spare road wheels and an off-white radio case with its whip antenna. Nine
//! hues, no repeats and none near the teams' red and blue. Each stands on a wooden
//! pallet in a soft contact shadow, a pennant in its colour on a pole at the back
//! corner, and is painted from one atlas whose tile for the kind carries the body's
//! surface, the kind's paint and its sprayed stencil.
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

/// The kind's signal colour: the thin wisp of smoke its grenade gives off, which
/// greys as it climbs. Its paint's hue, as coloured smoke is mixed: lighter and
/// milkier than the enamel (the black plate's grenade burns a pale ash grey).
pub fn pickup_signal(kind: PickupKind) -> u32 {
    match kind {
        PickupKind::Spread => 0xc690b8,
        PickupKind::Rocket => 0xdc9e6c,
        PickupKind::Rapid => 0xc4cc7c,
        PickupKind::Speed => 0xe2d284,
        PickupKind::Repair => 0x92c290,
        PickupKind::Piercing => 0x84c2b8,
        PickupKind::Shield => 0xb4b4b0,
        PickupKind::Ricochet => 0xaea2d4,
        PickupKind::Laser => 0xe6e4dc,
    }
}

/// The kind's colour as supply paint (`PICKUP_PAINTS` in `scripts/asset-data.ts`):
/// the big painted block on its load and its pennant. Nine hues as far apart as the
/// wheel allows once the teams' red and blue are left out (plum, burnt orange,
/// chartreuse, sand yellow, green, teal, violet, black and off-white), each a
/// muted, dusty military enamel rather than a toy colour.
pub fn pickup_paint(kind: PickupKind) -> u32 {
    match kind {
        PickupKind::Spread => 0x8a4474,
        PickupKind::Rocket => 0xa85a2a,
        PickupKind::Rapid => 0x8e9a36,
        PickupKind::Speed => 0xbc9c36,
        PickupKind::Repair => 0x447e40,
        PickupKind::Piercing => 0x2c7a72,
        PickupKind::Shield => 0x2e3032,
        PickupKind::Ricochet => 0x5e5294,
        PickupKind::Laser => 0xc2beb0,
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

/// The vertex effect that moves a kind face's UVs into the instance's atlas tile,
/// hoists and flutters the pallet's pennant and wears each drop a little
/// differently (`crates/render/src/presentation/shaders/pickup_face.wgsl`).
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
/// The pennant's cloth: the kind's paint inside the paint strip, and the only
/// vertices of any pickup mesh whose `v` lies strictly inside it, so the effect
/// can find them by their UVs alone (every other face meets that strip only at
/// its rows).
const FLAG: Region = [0.15, 0.66, 0.85, 0.715];
/// Swatches of the shared cell: dark gunmetal (straps, handles, latches), black
/// rubber (caps, tyres), olive webbing and bright worn steel (masts, drawer pulls).
const GUNMETAL: Region = [0.06, 0.56, 0.44, 0.94];
const RUBBER: Region = [0.56, 0.56, 0.94, 0.94];
const WEBBING: Region = [0.06, 0.06, 0.44, 0.44];
const BRIGHT: Region = [0.56, 0.06, 0.94, 0.44];
/// The marker pole: olive webbing drab, inset from that swatch's edges, where no
/// other part has a vertex, so the effect can find the pole by its UVs and bend it.
const POLE: Region = [0.1, 0.1, 0.4, 0.4];
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
pub const SMOKE_MARKER: [f64; 3] = [0.92, 0.27, -0.44];
const MARKER_RADIUS: f64 = 0.08;
/// The supply point's marker pole at the pallet's back left corner (x, z), its
/// height, and its pennant in the kind's paint flying from its top toward +x: its
/// length, depth, the segments it bends in, and how far it is lowered down the pole
/// while the drop is empty. The pennant is hoisted back up as the supply restocks
/// (the effect's instance `z`), so an empty drop shows when it will return. The
/// pole is a flexible whip: a hull driving over it bends it flat, pole and pennant,
/// by up to `POLE_BEND` (rad), and it springs back up once the hull has passed.
pub const FLAG_POLE: [f64; 2] = [-0.9, -0.52];
const POLE_HEIGHT: f64 = 2.0;
const FLAG_LENGTH: f64 = 0.78;
const FLAG_DEPTH: f64 = 0.46;
const FLAG_SEGMENTS: usize = 4;
const FLAG_DROP: f64 = 0.9;
const POLE_BEND: f64 = 1.45;
/// The bend arrives packed into the instance `z` in this many steps (twice the
/// step count added to the hoist, which stays within 0..1).
pub const POLE_BEND_STEPS: f32 = 15.0;
/// The soft shade the drop casts on the ground round its foot: darkness under the
/// pallet, fading out this far past its edges, at this height over the ground.
const CONTACT_DARKNESS: f32 = 0.5;
const CONTACT_REACH: f64 = 0.42;
const CONTACT_LIFT: f64 = 0.01;

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

/// Laser: a ribbed off-white radio transit case for the defence set, latched, its
/// whip antenna raised from one corner on its mount and a field handset in its
/// cradle on the lid's other end.
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

/// The pallet every pickup sits on: three stringers and five deck boards, the
/// smoke grenade on the ground beside it in the kind's paint, and the marker pole
/// with the kind's pennant at its back left corner. The `pickup-face` effect
/// lowers the pennant while the drop is empty and hoists it as it restocks.
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
    // The smoke grenade: a canister in the kind's paint, its fuse head and lever.
    let [mx, top, mz] = SMOKE_MARKER;
    let fuse = 0.05;
    parts.push(
        Part::drum(MARKER_RADIUS, top - fuse, 10, kind(PAINT))
            .top(swatch(GUNMETAL))
            .bottom(swatch(GUNMETAL))
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
    // The marker pole, driven into the ground with a cap on top, and its pennant
    // in segments along its length so the effect can ripple it.
    let [px, pz] = FLAG_POLE;
    parts.push(Part::drum(0.034, POLE_HEIGHT, 6, swatch(POLE)).at(px, POLE_HEIGHT / 2.0, pz));
    parts.push(Part::drum(0.052, 0.05, 6, swatch(POLE)).at(px, POLE_HEIGHT + 0.02, pz));
    let piece = FLAG_LENGTH / FLAG_SEGMENTS as f64;
    for index in 0..FLAG_SEGMENTS {
        parts.push(Part::cube([piece, FLAG_DEPTH, 0.012], kind(FLAG)).at(
            px + 0.03 + piece * (index as f64 + 0.5),
            POLE_HEIGHT - 0.06 - FLAG_DEPTH / 2.0,
            pz,
        ));
    }
    build(parts)
}

/// The soft shade the drop casts round its foot: a rounded rectangle over the
/// pallet's footprint, darkest under it and fading to nothing `CONTACT_REACH` past
/// its edges, in black blended by the vertex alpha, so the drop sits in the ground
/// rather than on a plate. One mesh for every drop.
fn contact_shade_geometry() -> Mesh {
    const ARC: usize = 3;
    let [w, d] = PALLET_SIZE.map(|v| v / 2.0);
    let mut positions = Vec::new();
    let mut alpha = Vec::new();
    for (grow, round, strength) in [
        (-0.05, 0.05, CONTACT_DARKNESS),
        (CONTACT_REACH, CONTACT_REACH, 0.0),
    ] {
        for (corner, (sx, sz)) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]
            .into_iter()
            .enumerate()
        {
            for k in 0..ARC {
                let angle = (corner as f64 + k as f64 / (ARC - 1) as f64) * PI / 2.0;
                let x = sx * (w + grow - round) + round * angle.cos();
                let z = sz * (d + grow - round) + round * angle.sin();
                positions.extend([x, CONTACT_LIFT, z]);
                alpha.push(strength);
            }
        }
    }
    let ring = (4 * ARC) as u32;
    // The core as a fan over its corners; then the band out to the rim.
    let mut indices: Vec<u32> = (1..ring - 1).flat_map(|i| [0, i + 1, i]).collect();
    for i in 0..ring {
        let j = (i + 1) % ring;
        indices.extend([i, j, ring + j, i, ring + j, ring + i]);
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
    mesh.colors = vec![[0.0, 0.0, 0.0]; mesh.positions.len()];
    mesh.set_attribute(Attribute::vertex(VERTEX_ALPHA, 1, alpha));
    mesh
}

/// The atlas `v` above which a pallet-cell UV belongs to the board a worn pallet
/// may be missing: halfway between that board's region and the one below it.
fn missing_board_from() -> f64 {
    let [_, from] = atlas_cell_uv(PICKUP_PALLET_CELL, 0.0, board(MISSING_BOARD)[1]);
    let [_, below] = atlas_cell_uv(PICKUP_PALLET_CELL, 0.0, board(MISSING_BOARD - 1)[3]);
    (from + below) / 2.0
}

/// The atlas `v` range (low, high) inside which a kind-tile UV belongs to the
/// pennant: strictly inside its region, clear of the plain strip's rows.
fn flag_rows() -> [f64; 2] {
    let [_, low] = atlas_cell_uv(KIND, 0.0, FLAG[1] - 0.004);
    let [_, high] = atlas_cell_uv(KIND, 0.0, FLAG[3] + 0.004);
    [low, high]
}

/// The pole's UV rectangle in the atlas (u0, v0, u1, v1), a little wider than its
/// region and well inside its swatch.
fn pole_rect() -> [f64; 4] {
    let [u0, v0] = atlas_cell_uv(PICKUP_SWATCH_CELL, POLE[0] - 0.02, POLE[1] - 0.02);
    let [u1, v1] = atlas_cell_uv(PICKUP_SWATCH_CELL, POLE[2] + 0.02, POLE[3] + 0.02);
    [u0, v0, u1, v1]
}

/// The shared supply paint: the atlas, worn and matte, with the `pickup-face`
/// effect choosing the kind's tile, the pennant's height and the pole's bend; its
/// params are [`missing_board_from`], the pole's x, the pennant's length and drop;
/// the pennant's atlas rows ([`flag_rows`]), the pole's z and its greatest bend;
/// the pole's UV rectangle ([`pole_rect`]).
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
                params: vec![
                    missing_board_from() as f32,
                    FLAG_POLE[0] as f32,
                    FLAG_LENGTH as f32,
                    FLAG_DROP as f32,
                    flag_rows()[0] as f32,
                    flag_rows()[1] as f32,
                    FLAG_POLE[1] as f32,
                    POLE_BEND as f32,
                ]
                .into_iter()
                .chain(pole_rect().map(|v| v as f32))
                .collect(),
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

/// The pallet's and its shade's keys in the mesh cache, clear of every kind's.
const PALLET_KEY: u8 = 100;
const SHADE_KEY: u8 = 101;

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

/// The pallet under every pickup, shared by every kind, in its soft contact shade;
/// its instance data's `xy` paints the smoke grenade and pennant for the kind and
/// its `z` hoists the pennant (1 while stocked).
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
        ..Material::standard(0x000000, 0.0, 1.0)
    });
    group.children.push(Node::mesh(
        SHARED.get_or_insert(SHADE_KEY, contact_shade_geometry),
        shade,
    ));
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

    /// Only the pallet's pennant has UVs inside the pennant's rows of a kind tile,
    /// so the effect hoists and ripples nothing else.
    #[test]
    fn only_the_pennant_lies_in_its_rows() {
        let [low, high] = flag_rows();
        let [u_end, _] = atlas_cell_uv(KIND, 1.0, 0.0);
        let flagged = |mesh: &Mesh| {
            mesh.uvs
                .iter()
                .filter(|&&[u, v]| {
                    f64::from(u) < u_end && f64::from(v) > low && f64::from(v) < high
                })
                .count()
        };
        assert_eq!(flagged(&pallet_geometry()), FLAG_SEGMENTS * 24);
        for kind in PickupKind::ALL {
            assert_eq!(flagged(&item_geometry(kind)), 0, "{kind:?}");
        }
    }

    /// Only the marker pole has UVs inside the pole's rectangle, so the effect
    /// bends nothing else.
    #[test]
    fn only_the_pole_lies_in_its_rectangle() {
        let [u0, v0, u1, v1] = pole_rect();
        let poled = |mesh: &Mesh| {
            mesh.uvs
                .iter()
                .filter(|&&[u, v]| {
                    let (u, v) = (f64::from(u), f64::from(v));
                    u > u0 && u < u1 && v > v0 && v < v1
                })
                .count()
        };
        assert!(poled(&pallet_geometry()) > 0);
        for kind in PickupKind::ALL {
            assert_eq!(poled(&item_geometry(kind)), 0, "{kind:?}");
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
