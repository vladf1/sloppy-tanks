//! The tract beyond Senda's walls: the next street's row of homes standing back to back
//! with the level past its south and west walls, as the real tract runs on there, drawn
//! as simple low blocks (stucco walls under a hip or gable tile roof, a single-storey
//! office and a garage door on their fronts, which face away), their block back walls
//! and paved back yards, a strip of planting along the level's wall, and a few trees
//! among them. Far scenery: a handful of triangles a house, merged into one draw.

use std::sync::Arc;

use glam::{DVec2, DVec3};

use crate::geometry::Mesh;
use crate::scene::Node;
use crate::sim::senda_layout::{
    HOUSE_WIDTH, Street, WALL_THICKNESS, WAY_PARKWAY, WAY_PATH, inside_boundary, left,
    senda_boundary,
};

use super::batching::batch;
use super::model_primitives::{box_part, paint, put, rotated, shadowed};
use super::tree_models::{TreeDetail, TreeShape, tree_model};

/// The neighbours' homes: their pitch along the wall, their depths (front to back, m) by
/// turns, their eave and ridge heights, the eaves' overhang (m), and the single-storey
/// office on their fronts (width, depth, eave and the shed roof's top, m).
const NEIGHBOUR_PITCH: f64 = 7.9;
const NEIGHBOUR_DEPTHS: [f64; 3] = [8.0, 9.2, 8.6];
const NEIGHBOUR_EAVE: f64 = 3.2;
const NEIGHBOUR_RIDGE: f64 = 4.5;
const NEIGHBOUR_OVERHANG: f64 = 0.3;
const NEIGHBOUR_OFFICE: (f64, f64, f64, f64) = (2.6, 1.4, 1.7, 2.1);
/// From the level wall's inside face out to the neighbours' back walls: the planting
/// strip between (m); their back yards' depth (m); their block back walls' height (m).
const NEIGHBOUR_STRIP: f64 = 1.4;
const NEIGHBOUR_YARD: f64 = 1.8;
const NEIGHBOUR_WALL: f64 = 1.0;
/// How far (m) the row keeps from Senda Parkway's paving where the Way leaves the level.
const NEIGHBOUR_WAY_ROOM: f64 = 4.0;
/// The tract's stucco and its three roof tiles: terracotta, a brown blend and a grey tan.
const NEIGHBOUR_STUCCO: [u32; 3] = [0xc9b594, 0xbea783, 0xcdbd9f];
const NEIGHBOUR_TILE: [u32; 3] = [0x8a6150, 0x735a4b, 0x857d72];
const NEIGHBOUR_DOOR: u32 = 0x5b4636;
const NEIGHBOUR_BLOCK: u32 = 0x8f7762;
const NEIGHBOUR_PATIO: u32 = 0xc9c1b2;
const NEIGHBOUR_MULCH: u32 = 0x5c4a36;
/// A tree beside every this-many-th house (the bank's trees' crown and height ranges, m).
const NEIGHBOUR_TREE_EVERY: usize = 4;
const NEIGHBOUR_TREE_CROWN: (f64, f64) = (2.7, 3.8);
const NEIGHBOUR_TREE_HEIGHT: (f64, f64) = (3.4, 4.6);

/// One house of the row: its back's middle on the ground, the way its front faces (away
/// from the level), and its place in the row.
struct Neighbour {
    back: DVec2,
    out: DVec2,
    k: usize,
}

/// Whether a stretch of the level's wall whose outside lies toward `out` backs onto the
/// next row of the tract: the walls facing south and west.
fn backs_onto_tract(out: DVec2) -> bool {
    out.x < -0.7 || out.y > 0.7
}

/// The row's houses: one every [`NEIGHBOUR_PITCH`] along the south and west walls, square
/// to the wall, short of Senda Parkway and of each other where the wall turns.
fn neighbours() -> Vec<Neighbour> {
    let boundary = senda_boundary();
    let way = Street::SendaParkway.line();
    let way_reach = Street::SendaParkway.half() + WAY_PARKWAY + WAY_PATH + NEIGHBOUR_WAY_ROOM;
    let closed: Vec<DVec2> = boundary.iter().chain(boundary.first()).copied().collect();
    let back = WALL_THICKNESS + NEIGHBOUR_STRIP + NEIGHBOUR_YARD;
    let mut houses: Vec<Neighbour> = Vec::new();
    let (mut walked, mut next) = (0.0, NEIGHBOUR_PITCH / 2.0);
    for pair in closed.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let span = a.distance(b);
        let along = (b - a) / span.max(1e-9);
        let side = left(along);
        let middle = (a + b) / 2.0;
        let out = if inside_boundary(&boundary, middle + side * 0.3) {
            -side
        } else {
            side
        };
        while next < walked + span {
            let point = a + along * (next - walked);
            next += NEIGHBOUR_PITCH;
            if !backs_onto_tract(out) {
                continue;
            }
            let at = point + out * back;
            let k = houses.len();
            let depth = NEIGHBOUR_DEPTHS[k % NEIGHBOUR_DEPTHS.len()];
            let front = at + out * depth;
            let far = way.distance(at).min(way.distance(front)) < way_reach
                || houses
                    .iter()
                    .any(|house| house.back.distance(at) < NEIGHBOUR_PITCH - 0.6)
                || [at, front, (at + front) / 2.0]
                    .iter()
                    .any(|&p| inside_boundary(&boundary, p));
            if !far {
                houses.push(Neighbour { back: at, out, k });
            }
        }
        walked += span;
    }
    houses
}

/// A flat-shaded mesh of `triangles`, each wound counter-clockwise seen from its face.
fn faces(triangles: &[[DVec3; 3]]) -> Mesh {
    let mut positions = Vec::with_capacity(triangles.len() * 9);
    let mut normals = Vec::with_capacity(triangles.len() * 9);
    let mut uvs = Vec::with_capacity(triangles.len() * 6);
    for &[a, b, c] in triangles {
        let normal = (b - a).cross(c - a).normalize_or_zero();
        for p in [a, b, c] {
            positions.extend([p.x, p.y, p.z]);
            normals.extend([normal.x, normal.y, normal.z]);
            uvs.extend([p.x / 4.0, p.z / 4.0]);
        }
    }
    Mesh::from_f64(&positions, &normals, &uvs, None)
}

/// A quad's two triangles, its corners counter-clockwise seen from its face.
fn quad([a, b, c, d]: [DVec3; 4]) -> [[DVec3; 3]; 2] {
    [[a, b, c], [a, c, d]]
}

/// One house's walls, roof and front: built in its frame (x across, z out to its front
/// from its back wall), placed and turned onto the ground.
fn house(group: &mut Node, house: &Neighbour) {
    let k = house.k;
    let (w, d) = (HOUSE_WIDTH, NEIGHBOUR_DEPTHS[k % NEIGHBOUR_DEPTHS.len()]);
    let across = left(house.out);
    let to_world = |p: DVec3| {
        let ground = house.back + across * p.x + house.out * p.z;
        DVec3::new(ground.x, p.y, ground.y)
    };
    let world = |tris: Vec<[DVec3; 3]>| -> Vec<[DVec3; 3]> {
        tris.into_iter().map(|t| t.map(to_world)).collect()
    };
    let (hw, eave) = (w / 2.0, NEIGHBOUR_EAVE);
    let corner = |x: f64, y: f64, z: f64| DVec3::new(x, y, z);
    // The walls, as four quads facing out.
    let mut walls: Vec<[DVec3; 3]> = Vec::new();
    for (p, q) in [
        ((-hw, 0.0), (hw, 0.0)),
        ((hw, 0.0), (hw, d)),
        ((hw, d), (-hw, d)),
        ((-hw, d), (-hw, 0.0)),
    ] {
        walls.extend(quad([
            corner(p.0, 0.0, p.1),
            corner(p.0, eave, p.1),
            corner(q.0, eave, q.1),
            corner(q.0, 0.0, q.1),
        ]));
    }
    // The roof over the eaves' overhang: a hip, or on every third house a gable whose
    // ridge runs front to back, as most of the tract's do.
    let o = NEIGHBOUR_OVERHANG;
    let (x0, x1, z0, z1) = (-hw - o, hw + o, -o, d + o);
    let rise = NEIGHBOUR_RIDGE - eave;
    let low = eave - o * rise / hw;
    let mut roof: Vec<[DVec3; 3]> = Vec::new();
    let mut gable_ends: Vec<[DVec3; 3]> = Vec::new();
    if k % 3 == 1 {
        let (r0, r1) = (
            corner(0.0, NEIGHBOUR_RIDGE, z0),
            corner(0.0, NEIGHBOUR_RIDGE, z1),
        );
        roof.extend(quad([corner(x0, low, z1), r1, r0, corner(x0, low, z0)]));
        roof.extend(quad([corner(x1, low, z0), r0, r1, corner(x1, low, z1)]));
        // The stucco gables under the roof's ends.
        for (z, flip) in [(0.0, false), (d, true)] {
            let (a, b, c) = (
                corner(-hw, eave, z),
                corner(0.0, NEIGHBOUR_RIDGE - 0.1, z),
                corner(hw, eave, z),
            );
            gable_ends.push(if flip { [c, b, a] } else { [a, b, c] });
        }
    } else {
        let inset = (x1 - x0) / 2.0;
        let (r0, r1) = (
            corner(0.0, NEIGHBOUR_RIDGE, z0 + inset),
            corner(0.0, NEIGHBOUR_RIDGE, z1 - inset),
        );
        let (a, b, c, e) = (
            corner(x0, low, z0),
            corner(x1, low, z0),
            corner(x1, low, z1),
            corner(x0, low, z1),
        );
        roof.extend(quad([e, r1, r0, a]));
        roof.extend(quad([b, r0, r1, c]));
        roof.push([a, r0, b]);
        roof.push([c, r1, e]);
    }
    // The single-storey office on the front's door half under a shed roof, and the garage
    // door on the other half.
    let (ow, od, oe, top) = NEIGHBOUR_OFFICE;
    let side = if k.is_multiple_of(2) { 1.0 } else { -1.0 };
    let (a0, a1) = if side > 0.0 {
        (hw - ow, hw - 0.2)
    } else {
        (-hw + 0.2, -hw + ow)
    };
    let zf = d + od;
    for (p, q) in [
        ((a1, d), (a1, zf)),
        ((a1, zf), (a0, zf)),
        ((a0, zf), (a0, d)),
    ] {
        walls.extend(quad([
            corner(p.0, 0.0, p.1),
            corner(p.0, oe, p.1),
            corner(q.0, oe, q.1),
            corner(q.0, 0.0, q.1),
        ]));
    }
    roof.extend(quad([
        corner(a0 - 0.15, oe, zf + 0.2),
        corner(a1 + 0.15, oe, zf + 0.2),
        corner(a1 + 0.15, top, d),
        corner(a0 - 0.15, top, d),
    ]));
    let (g0, g1) = (-side * (hw - 0.3), -side * (hw - 0.3 - 2.6));
    let (g0, g1) = (g0.min(g1), g0.max(g1));
    let door = quad([
        corner(g0, 0.05, d + 0.02),
        corner(g1, 0.05, d + 0.02),
        corner(g1, 1.2, d + 0.02),
        corner(g0, 1.2, d + 0.02),
    ]);
    let stucco = NEIGHBOUR_STUCCO[k % NEIGHBOUR_STUCCO.len()];
    let tile = NEIGHBOUR_TILE[(k * 2 + k / 3) % NEIGHBOUR_TILE.len()];
    walls.extend(gable_ends);
    for (tris, color) in [
        (walls, stucco),
        (roof, tile),
        (door.to_vec(), NEIGHBOUR_DOOR),
    ] {
        group
            .children
            .push(shadowed(Arc::new(faces(&world(tris))), paint(color)));
    }
    // The paved back yard, its block back wall and the planting strip along the level's
    // wall behind it.
    let yaw = house.out.x.atan2(house.out.y);
    let flat = |group: &mut Node, (z, depth): (f64, f64), (h, y): (f64, f64), color: u32| {
        let middle = house.back + house.out * z;
        let piece = rotated(
            box_part(NEIGHBOUR_PITCH - 0.1, h, depth, color, 0.0),
            0.0,
            yaw,
            0.0,
        );
        put(group, piece, middle.x, y, middle.y);
    };
    flat(
        group,
        (-NEIGHBOUR_YARD / 2.0, NEIGHBOUR_YARD),
        (0.05, 0.02),
        NEIGHBOUR_PATIO,
    );
    flat(
        group,
        (-NEIGHBOUR_YARD - 0.1, 0.2),
        (NEIGHBOUR_WALL, NEIGHBOUR_WALL / 2.0),
        NEIGHBOUR_BLOCK,
    );
    flat(
        group,
        (
            -NEIGHBOUR_YARD - 0.2 - NEIGHBOUR_STRIP / 2.0,
            NEIGHBOUR_STRIP,
        ),
        (0.05, 0.02),
        NEIGHBOUR_MULCH,
    );
}

/// The tract's next row past the level's south and west walls, its houses merged into
/// one draw, and a tree beside every few of them.
pub fn senda_neighbours() -> Node {
    let mut group = Node::group("senda-neighbours");
    let row = neighbours();
    for neighbour in &row {
        house(&mut group, neighbour);
    }
    batch(&mut group);
    for neighbour in row.iter().filter(|n| n.k % NEIGHBOUR_TREE_EVERY == 2) {
        let seed = neighbour.k as f64 * 1.37;
        let share = |a: f64| ((a * 12.9898).sin() * 43_758.545).fract().abs();
        let (least, most) = NEIGHBOUR_TREE_CROWN;
        let (low, high) = NEIGHBOUR_TREE_HEIGHT;
        // In the side yard's gap between this house and the next, toward its back.
        let across = left(neighbour.out);
        let at = neighbour.back + across * (NEIGHBOUR_PITCH / 2.0) + neighbour.out * 2.0;
        let shape = TreeShape {
            x: at.x,
            z: at.y,
            w: least + (most - least) * share(seed),
            d: least + (most - least) * share(seed + 0.3),
            h: low + (high - low) * share(seed + 0.6),
        };
        put(
            &mut group,
            tree_model(&shape, TreeDetail::Background),
            at.x,
            0.0,
            at.y,
        );
    }
    group
}
