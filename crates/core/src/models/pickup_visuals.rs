//! Pickups as military supply (successor of `pickup-visuals.ts` and
//! `pickup-atlas.ts`): what each kind ships in, an ammunition crate, a pair of jerry
//! cans or a steel case, sitting on a wooden pallet, painted from one atlas whose
//! tile for the kind carries its colour band and sprayed stencil.
//!
//! Every face, strap and handle of an item maps into the first kind's tile and the
//! `pickup-face` effect moves it into the instance's tile (instance data `xy`), so
//! every kind of a shape shares one mesh and material and draws as one instanced
//! call. The pallet maps the atlas's bare-plank cell directly. First person fades
//! them through the instance opacity, whose blended variant warm-up compiles.

use std::sync::Arc;

use super::model_primitives::Cache;
use crate::geometry::{Mesh, box_geometry, cylinder_geometry, merge_geometries};
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

/// The packed atlas under `public/` (`scripts/generate-pickup-atlas.ts`): nine kind
/// tiles in a 3x3 block and, in a fourth column, the pallet's planks and the
/// ground's spray paint.
pub const PICKUP_ATLAS_PATH: &str = "textures/pickups/atlas.webp";
pub const PICKUP_ICON_SIZE: f64 = 256.0;
pub const PICKUP_ATLAS_PADDING: f64 = 16.0;
pub const PICKUP_ATLAS_STRIDE: f64 = PICKUP_ICON_SIZE + PICKUP_ATLAS_PADDING * 2.0;
pub const PICKUP_ATLAS_SIZE: f64 = PICKUP_ATLAS_STRIDE * 4.0;
pub const PICKUP_PALLET_CELL: (u32, u32) = (3, 0);
pub const PICKUP_PAINT_CELL: (u32, u32) = (3, 1);

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
pub fn pickup_atlas_uv(kind: PickupKind, u: f64, v: f64) -> [f64; 2] {
    atlas_cell_uv(pickup_atlas_tile(kind), u, v)
}

/// Where `kind`'s tile sits relative to the first tile, in atlas UV: the items are
/// mapped to the first tile and the `pickup-face` effect adds this offset, passed
/// as the instance data's `xy`, so every kind shares one mesh and material and
/// all pickups of a shape draw as one instanced call.
pub fn pickup_tile_offset(kind: PickupKind) -> [f32; 2] {
    let (column, row) = pickup_atlas_tile(kind);
    [
        (f64::from(column) * PICKUP_ATLAS_STRIDE / PICKUP_ATLAS_SIZE) as f32,
        (-f64::from(row) * PICKUP_ATLAS_STRIDE / PICKUP_ATLAS_SIZE) as f32,
    ]
}

/// The vertex effect that moves an item's UVs into the instance's atlas tile
/// (`crates/render/src/presentation/shaders/pickup_face.wgsl`).
pub const PICKUP_FACE_EFFECT: &str = "pickup-face";

/// What a kind ships in, which its silhouette shows from above before its colour:
/// weapons and ammunition in wooden crates, speed in jerry cans, defences in steel
/// cases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickupShape {
    Crate,
    JerryCans,
    Case,
}

impl PickupShape {
    pub fn of(kind: PickupKind) -> Self {
        match kind {
            PickupKind::Spread
            | PickupKind::Rocket
            | PickupKind::Ricochet
            | PickupKind::Piercing
            | PickupKind::Rapid => Self::Crate,
            PickupKind::Speed => Self::JerryCans,
            PickupKind::Shield | PickupKind::Repair | PickupKind::Laser => Self::Case,
        }
    }
}

/// Regions of a kind tile (u0, v0, u1, v1), `v` up; the generator paints the same
/// layout: the whole tile (band and stencil, for lids and faces), the strip of
/// surface and band along its top (for the sides), a plain strip of surface above
/// the band (for edges), a steel corner plate (straps, handles, latches), and the
/// jerry can's embossed panel inside its border.
type Region = [f64; 4];
const TILE: Region = [0.0, 0.0, 1.0, 1.0];
const SIDE: Region = [0.0, 0.57, 1.0, 1.0];
const PLAIN: Region = [0.16, 0.914, 0.84, 0.984];
const STEEL: Region = [0.016, 0.914, 0.086, 0.984];
const PANEL: Region = [0.1, 0.1, 0.9, 0.9];

/// The pallet's size (x, z), its height, its deck boards and their width.
pub const PALLET_SIZE: [f64; 2] = [1.72, 1.32];
pub const PALLET_HEIGHT: f64 = 0.15;
const PALLET_BOARDS: u32 = 5;
const PALLET_BOARD_WIDTH: f64 = 0.27;
/// The smoke grenade marking the drop, on the ground beside the pallet's end: its
/// top (x, height, z) where the signal smoke leaves it, and its radius.
pub const SMOKE_MARKER: [f64; 3] = [1.0, 0.2, 0.42];
const MARKER_RADIUS: f64 = 0.07;

static SHARED: Cache<u8, Mesh> = Cache::new();
static MATERIALS: Cache<u8, Material> = Cache::new();

/// A box of size `size` centred at `at`, each face's UVs mapped into `region` by
/// the face's direction: `top` for faces looking up, `side` for those looking
/// along x and z (`end` for the ones along x when given), `bottom` below.
struct Part {
    size: [f64; 3],
    at: [f64; 3],
    top: Region,
    side: Region,
    end: Option<Region>,
    bottom: Region,
}

impl Part {
    fn plain(size: [f64; 3], at: [f64; 3], region: Region) -> Self {
        Self {
            size,
            at,
            top: region,
            side: region,
            end: None,
            bottom: region,
        }
    }
}

fn to_region(uv: [f32; 2], [u0, v0, u1, v1]: Region) -> [f64; 2] {
    [
        u0 + (u1 - u0) * f64::from(uv[0]),
        v0 + (v1 - v0) * f64::from(uv[1]),
    ]
}

fn build(parts: &[Part], cell: impl Fn(f64, f64) -> [f64; 2], turn: f64) -> Mesh {
    let meshes: Vec<Mesh> = parts
        .iter()
        .map(|part| {
            let [w, h, d] = part.size;
            let mut mesh = box_geometry(w, h, d);
            for (uv, normal) in mesh.uvs.iter_mut().zip(&mesh.normals) {
                let region = if normal[1] > 0.5 {
                    part.top
                } else if normal[1] < -0.5 {
                    part.bottom
                } else if normal[0].abs() > 0.5 {
                    part.end.unwrap_or(part.side)
                } else {
                    part.side
                };
                let [u, v] = to_region(*uv, region);
                let [u, v] = cell(u, v);
                *uv = [u as f32, v as f32];
            }
            let [x, y, z] = part.at;
            mesh.rotate_y(turn);
            let (sin, cos) = turn.sin_cos();
            mesh.translate(x * cos + z * sin, y, -x * sin + z * cos);
            mesh
        })
        .collect();
    merge_geometries(&meshes.iter().collect::<Vec<_>>()).expect("boxes share one layout")
}

fn first_tile(u: f64, v: f64) -> [f64; 2] {
    pickup_atlas_uv(PickupKind::Spread, u, v)
}

/// An ammunition crate: a pine box under an overhanging lid, two steel straps
/// over it and rope-grip steel handles at its ends.
fn crate_geometry() -> Mesh {
    let [w, h, d] = [1.5, 0.62, 1.0];
    let lid = 0.08;
    let mut parts = vec![
        Part {
            size: [w, h, d],
            at: [0.0, h / 2.0, 0.0],
            top: PLAIN,
            side: SIDE,
            end: None,
            bottom: PLAIN,
        },
        Part {
            size: [w + 0.06, lid, d + 0.06],
            at: [0.0, h + lid / 2.0, 0.0],
            top: TILE,
            side: PLAIN,
            end: None,
            bottom: PLAIN,
        },
    ];
    for x in [-0.46, 0.46] {
        parts.push(Part::plain(
            [0.07, 0.012, d + 0.1],
            [x, h + lid + 0.006, 0.0],
            STEEL,
        ));
        for z in [-1.0, 1.0] {
            parts.push(Part::plain(
                [0.07, h + lid, 0.012],
                [x, (h + lid) / 2.0, z * (d / 2.0 + 0.036)],
                STEEL,
            ));
        }
    }
    for x in [-1.0, 1.0] {
        parts.push(Part::plain(
            [0.05, 0.07, 0.42],
            [x * (w / 2.0 + 0.03), h * 0.68, 0.0],
            STEEL,
        ));
    }
    build(&parts, first_tile, 0.0)
}

/// Two jerry cans side by side, their embossed faces out, each with its carry
/// handles and spout, one turned a little against the other.
fn jerry_can_geometry() -> Mesh {
    let [w, h, d] = [0.7, 0.92, 0.36];
    let can = |turn: f64, offset: f64| {
        let parts = [
            Part {
                size: [w, h, d],
                at: [offset, h / 2.0, 0.0],
                top: PLAIN,
                side: PANEL,
                end: Some(PLAIN),
                bottom: PLAIN,
            },
            Part::plain([0.42, 0.05, 0.08], [offset - 0.06, h + 0.1, 0.0], STEEL),
            Part::plain([0.05, 0.1, 0.08], [offset - 0.25, h + 0.05, 0.0], STEEL),
            Part::plain([0.05, 0.1, 0.08], [offset + 0.13, h + 0.05, 0.0], STEEL),
            Part::plain([0.11, 0.13, 0.11], [offset + 0.24, h + 0.06, 0.0], STEEL),
        ];
        build(&parts, first_tile, turn)
    };
    let left = can(0.06, -0.38);
    let right = can(-0.1, 0.4);
    merge_geometries(&[&left, &right]).expect("cans share one layout")
}

/// A steel case: ribbed olive box under a lid, a carry handle on the lid, two
/// latches on its front and grips at its ends.
fn case_geometry() -> Mesh {
    let [w, h, d] = [1.36, 0.66, 0.9];
    let lid = 0.11;
    let mut parts = vec![
        Part {
            size: [w, h, d],
            at: [0.0, h / 2.0, 0.0],
            top: PLAIN,
            side: SIDE,
            end: None,
            bottom: PLAIN,
        },
        Part {
            size: [w + 0.05, lid, d + 0.05],
            at: [0.0, h + lid / 2.0, 0.0],
            top: TILE,
            side: PLAIN,
            end: None,
            bottom: PLAIN,
        },
        Part::plain([0.42, 0.05, 0.06], [0.0, h + lid + 0.09, 0.0], STEEL),
        Part::plain([0.05, 0.09, 0.06], [-0.19, h + lid + 0.045, 0.0], STEEL),
        Part::plain([0.05, 0.09, 0.06], [0.19, h + lid + 0.045, 0.0], STEEL),
    ];
    for x in [-0.42, 0.42] {
        parts.push(Part::plain(
            [0.12, 0.16, 0.04],
            [x, h - 0.04, d / 2.0 + 0.02],
            STEEL,
        ));
    }
    for x in [-1.0, 1.0] {
        parts.push(Part::plain(
            [0.05, 0.06, 0.36],
            [x * (w / 2.0 + 0.025), h * 0.7, 0.0],
            STEEL,
        ));
    }
    build(&parts, first_tile, 0.0)
}

/// The pallet every pickup sits on: three stringers under five deck boards, and
/// the smoke grenade beside it.
fn pallet_geometry() -> Mesh {
    let [w, d] = PALLET_SIZE;
    let deck = 0.035;
    let stringer = PALLET_HEIGHT - deck;
    let cell = |u: f64, v: f64| atlas_cell_uv(PICKUP_PALLET_CELL, u, v);
    let mut parts = Vec::new();
    for z in [-1.0, 0.0, 1.0] {
        parts.push(Part::plain(
            [w, stringer, 0.12],
            [0.0, stringer / 2.0, z * (d / 2.0 - 0.06)],
            [0.0, 0.0, 1.0, 0.25],
        ));
    }
    let pitch = (w - PALLET_BOARD_WIDTH) / f64::from(PALLET_BOARDS - 1);
    for board in 0..PALLET_BOARDS {
        let x = -w / 2.0 + PALLET_BOARD_WIDTH / 2.0 + pitch * f64::from(board);
        let band = f64::from(board % 4) * 0.25;
        parts.push(Part::plain(
            [PALLET_BOARD_WIDTH, deck, d],
            [x, stringer + deck / 2.0, 0.0],
            [band, 0.0, band + 0.25, 1.0],
        ));
    }
    let mut pallet = build(&parts, cell, 0.0);
    let [mx, top, mz] = SMOKE_MARKER;
    let mut marker = cylinder_geometry(MARKER_RADIUS, MARKER_RADIUS, top, 8);
    for uv in &mut marker.uvs {
        let [u, v] = to_region(*uv, STEEL);
        let [u, v] = first_tile(u, v);
        *uv = [u as f32, v as f32];
    }
    marker.translate(mx, top / 2.0, mz);
    pallet = merge_geometries(&[&pallet, &marker]).expect("pallet parts share a layout");
    pallet
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
            roughness: 0.82,
            metalness: 0.08,
            effect: Effect::Custom {
                name: PICKUP_FACE_EFFECT,
                params: Vec::new(),
            },
            ..Material::default()
        }
    })
}

/// The shape's item mesh, every face mapped to the first tile.
fn item_geometry(shape: PickupShape) -> Arc<Mesh> {
    SHARED.get_or_insert(shape as u8, || match shape {
        PickupShape::Crate => crate_geometry(),
        PickupShape::JerryCans => jerry_can_geometry(),
        PickupShape::Case => case_geometry(),
    })
}

fn part(mesh: Arc<Mesh>, shadows: bool) -> Node {
    let mut node = Node::mesh(mesh, pickup_material());
    if let Some(drawable) = &mut node.drawable {
        drawable.cast_shadow = shadows;
        drawable.receive_shadow = true;
    }
    node
}

/// `pickupCube(kind)`: the supply a kind ships in, standing on y = 0. Every kind of
/// a shape shares it; the instance data's `xy` ([`pickup_tile_offset`]) picks the
/// kind's paint. Presentation stands it on its pallet.
pub fn pickup_cube(kind: PickupKind) -> Node {
    let mut group = Node::default();
    group
        .children
        .push(part(item_geometry(PickupShape::of(kind)), true));
    group
}

/// [`pickup_cube`] casting no shadow: the item a collection lifts into the tank
/// that took it, gone too quickly for a shadow to read as anything but a stain.
pub fn pickup_icon(kind: PickupKind) -> Node {
    let mut group = Node::default();
    group
        .children
        .push(part(item_geometry(PickupShape::of(kind)), false));
    group
}

/// The pallet under every pickup, shared by every kind.
pub fn pickup_pallet() -> Node {
    let mesh = SHARED.get_or_insert(10, pallet_geometry);
    let mut group = Node::default();
    group.children.push(part(mesh, true));
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

    #[test]
    fn every_item_and_the_pallet_stay_inside_their_atlas_cells() {
        let inside = |mesh: &Mesh, (column, row): (u32, u32)| {
            let [u0, v1] = atlas_cell_uv((column, row), 0.0, 1.0);
            let [u1, v0] = atlas_cell_uv((column, row), 1.0, 0.0);
            mesh.uvs.iter().all(|&[u, v]| {
                let (u, v) = (f64::from(u), f64::from(v));
                (u0 - 1e-6..=u1 + 1e-6).contains(&u) && (v0 - 1e-6..=v1 + 1e-6).contains(&v)
            })
        };
        for shape in [
            PickupShape::Crate,
            PickupShape::JerryCans,
            PickupShape::Case,
        ] {
            assert!(inside(&item_geometry(shape), (0, 0)), "{shape:?}");
        }
        let pallet = pallet_geometry();
        let deck = pallet.positions.len() - cylinder_geometry(1.0, 1.0, 1.0, 8).positions.len();
        let boards = Mesh {
            uvs: pallet.uvs[..deck].to_vec(),
            ..Mesh::default()
        };
        assert!(inside(&boards, PICKUP_PALLET_CELL));
    }
}
