//! Port of `harbor-models.ts`: shipping containers (cover and ship cargo).
//! A cover container is a steel frame (rails, corner posts and castings) carrying
//! modelled trapezoidal corrugation on its walls and roof, cargo doors with
//! locking bars at one end, and a shipping line's livery wrapped over the ribs.
//! Every wall shares the weathered paint tile (`textures/harbor/container.webp`,
//! `scripts/generate-harbor-textures.ts`) in container space, so rust runs from the
//! roof rail and grime climbs from the floor whatever the size. Meshes are cached
//! per size. Cargo crates are in [`super::crate_model`].

use std::f64::consts::{FRAC_PI_2, PI};
use std::sync::Arc;

use crate::geometry::math::scale_hex_color;
use crate::geometry::{Mesh, box_geometry};
use crate::scene::{Material, Node, TextureRef, Wrap};

use super::effects_scenery::{CONTAINER_LIVERIES, CONTAINER_LIVERY_TEXTURE};
use super::model_primitives::{Cache, adopt_children, box_part, put, rotated, shadowed};

/// `Pick<Cover, "w" | "d" | "h" | "color">`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CargoShape {
    pub w: f64,
    pub d: f64,
    pub h: f64,
    pub color: u32,
}

const LOCK_SILVER: u32 = 0xc4c4ad;
const GASKET: u32 = 0x22282a;
const DATA_PLATE: u32 = 0xe1d9b7;

const CONTAINER_TEXTURE: &str = "textures/harbor/container.webp";
/// Metres of wall per repeat of the paint wear tile.
const WEAR_TILE: f64 = 3.0;
/// Wall corrugation: pitch and depth, metres. The profile is a trapezoid: outer
/// flat, falling flank, inner flat, rising flank.
const WALL_PITCH: f64 = 0.46;
const WALL_DEPTH: f64 = 0.06;
/// The roof's shallower corrugation.
const ROOF_PITCH: f64 = 0.92;
const ROOF_DEPTH: f64 = 0.035;
/// Fractions of a pitch where the profile's four segments start.
const PROFILE_BREAKS: [f64; 4] = [0.0, 0.4, 0.5, 0.9];
/// Section of the steel frame's rails and posts, metres.
const RAIL: f64 = 0.16;
const POST: f64 = 0.24;
/// The livery sits this far proud of the ribs so it never fights them for depth.
const LIVERY_LIFT: f64 = 0.012;

static PAINT: Cache<u32, Material> = Cache::new();
static PANELS: Cache<[u64; 6], Mesh> = Cache::new();
static WEAR_BOXES: Cache<[u64; 4], Mesh> = Cache::new();

/// Container paint: the wear tile multiplied by the container's color.
fn container_paint(color: u32) -> Arc<Material> {
    PAINT.get_or_insert(color, || Material {
        map: Some(TextureRef {
            wrap: Wrap::Repeat,
            anisotropy: 4,
            ..TextureRef::file(CONTAINER_TEXTURE)
        }),
        ..Material::standard(color, 0.25, 0.58)
    })
}

/// The livery decals of every container: the browser-drawn logo sheet.
fn livery_paint() -> Arc<Material> {
    static LIVERY: std::sync::OnceLock<Arc<Material>> = std::sync::OnceLock::new();
    LIVERY
        .get_or_init(|| {
            Arc::new(Material {
                map: Some(TextureRef {
                    anisotropy: 4,
                    ..TextureRef::generated(CONTAINER_LIVERY_TEXTURE)
                }),
                alpha_test: 0.5,
                ..Material::standard(0xffffff, 0.1, 0.55)
            })
        })
        .clone()
}

/// Depth of the corrugation profile at `x`, for ribs starting at `origin`.
fn profile(x: f64, origin: f64, pitch: f64, depth: f64) -> f64 {
    let t = ((x - origin) / pitch).rem_euclid(1.0);
    if t < 0.4 {
        depth
    } else if t < 0.5 {
        depth * (0.5 - t) / 0.1
    } else if t < 0.9 {
        0.0
    } else {
        depth * (t - 0.9) / 0.1
    }
}

/// Where a corrugated panel lies in its own frame, facing +z: the span
/// `x0..x1` by `y0..y1`, ribs of `pitch` and `depth` starting at `origin`, lifted
/// `lift` off the profile. `uv` maps a point of the panel to texture coordinates.
struct Panel {
    x: [f64; 2],
    y: [f64; 2],
    origin: f64,
    pitch: f64,
    depth: f64,
    lift: f64,
}

impl Panel {
    fn mesh(&self, uv: impl Fn(f64, f64) -> [f64; 2]) -> Mesh {
        let [x0, x1] = self.x;
        let mut breaks = vec![x0];
        let first = ((x0 - self.origin) / self.pitch).floor() as i64;
        let last = ((x1 - self.origin) / self.pitch).ceil() as i64;
        for k in first..=last {
            for t in PROFILE_BREAKS {
                let x = self.origin + (k as f64 + t) * self.pitch;
                if x > x0 + 1e-6 && x < x1 - 1e-6 {
                    breaks.push(x);
                }
            }
        }
        breaks.push(x1);
        let [y0, y1] = self.y;
        let (mut positions, mut normals, mut uvs) = (Vec::new(), Vec::new(), Vec::new());
        for pair in breaks.windows(2) {
            let (xa, xb) = (pair[0], pair[1]);
            let za = profile(xa, self.origin, self.pitch, self.depth) + self.lift;
            let zb = profile(xb, self.origin, self.pitch, self.depth) + self.lift;
            let (dx, dz) = (xb - xa, zb - za);
            let length = dx.hypot(dz);
            let normal = [-dz / length, 0.0, dx / length];
            // Counter-clockwise seen from +z.
            for (x, y, z) in [
                (xa, y0, za),
                (xb, y0, zb),
                (xb, y1, zb),
                (xa, y0, za),
                (xb, y1, zb),
                (xa, y1, za),
            ] {
                positions.extend([x, y, z]);
                normals.extend(normal);
                uvs.extend(uv(x, y));
            }
        }
        Mesh::from_f64(&positions, &normals, &uvs, None)
    }
}

fn panel_key(panel: &Panel, kind: u64) -> [u64; 6] {
    [
        panel.x[0].to_bits() ^ kind,
        panel.x[1].to_bits(),
        panel.y[0].to_bits(),
        panel.y[1].to_bits(),
        panel.origin.to_bits() ^ panel.lift.to_bits(),
        panel.pitch.to_bits() ^ panel.depth.to_bits(),
    ]
}

/// A corrugated wall in container paint; `height` is the container's, so the wear
/// tile runs from the floor (v 0) to the roof (v 1).
fn wall_panel(panel: &Panel, height: f64, color: u32) -> Node {
    let mesh = PANELS.get_or_insert(panel_key(panel, 1), || {
        panel.mesh(|x, y| [x / WEAR_TILE, y / height])
    });
    shadowed(mesh, container_paint(color))
}

/// A box in container paint, its faces mapped to the wear tile in container space
/// (`height` again the container's, `floor` the box's bottom above the floor).
fn wear_box(w: f64, h: f64, d: f64, height: f64, floor: f64, color: u32) -> Node {
    let key = [w, h, d, height + floor * 7.0].map(f64::to_bits);
    let mesh = WEAR_BOXES.get_or_insert(key, || {
        let mut geometry = box_geometry(w, h, d);
        for i in 0..geometry.positions.len() {
            let [x, y, z] = geometry.positions[i].map(f64::from);
            let [nx, ny, _] = geometry.normals[i].map(f64::from);
            let v = (y + h / 2.0 + floor) / height;
            let uv = if ny.abs() > 0.5 {
                // Roofs read the mid wall, mottled but clean of rails.
                [x / WEAR_TILE, 0.45 + z / (WEAR_TILE * 4.0)]
            } else if nx.abs() > 0.5 {
                [z / WEAR_TILE, v]
            } else {
                [x / WEAR_TILE, v]
            };
            geometry.uvs[i] = [uv[0] as f32, uv[1] as f32];
        }
        geometry
    });
    shadowed(mesh, container_paint(color))
}

/// A building's corrugated steel cladding in weathered paint, facing +z from
/// `x = ±length / 2`, `y = 0..height`, with a shipping line's livery
/// `livery_width` wide centred on it when given.
pub(super) fn cladding(
    group: &mut Node,
    length: f64,
    height: f64,
    color: u32,
    livery: Option<(usize, f64)>,
) {
    let wall = Panel {
        x: [-length / 2.0, length / 2.0],
        y: [0.0, height],
        origin: -length / 2.0,
        pitch: WALL_PITCH * 1.5,
        depth: WALL_DEPTH * 1.5,
        lift: 0.0,
    };
    group.children.push(wall_panel(&wall, height, color));
    if let Some((row, width)) = livery {
        let livery = Panel {
            x: [-width / 2.0, width / 2.0],
            y: [height * 0.62 - width / 8.0, height * 0.62 + width / 8.0],
            lift: LIVERY_LIFT * 2.0,
            ..wall
        };
        group.children.push(livery_strip(&livery, row));
    }
}

/// One row of the livery sheet wrapped over a corrugated panel.
fn livery_strip(livery: &Panel, row: usize) -> Node {
    let rows = CONTAINER_LIVERIES as f64;
    let mesh = PANELS.get_or_insert(panel_key(livery, 2 + row as u64), || {
        let [x0, x1] = livery.x;
        let [y0, y1] = livery.y;
        // Row 0 of the sheet is its top, uploaded at v = 1.
        livery.mesh(|x, y| {
            [
                (x - x0) / (x1 - x0),
                1.0 - (row as f64 + 1.0 - (y - y0) / (y1 - y0)) / rows,
            ]
        })
    });
    Node::mesh(mesh, livery_paint())
}

/// Which livery a container color wears.
fn livery_for(color: u32) -> usize {
    (color.wrapping_mul(0x9e37_79b9) >> 28) as usize % CONTAINER_LIVERIES
}

/// `shippingContainer(group, c)`: append a container's parts to `group`, long
/// axis along x unless `d > w`.
pub fn shipping_container(group: &mut Node, c: CargoShape) {
    let along_z = c.d > c.w;
    let (length, depth) = if along_z { (c.d, c.w) } else { (c.w, c.d) };
    let mut body = Node::group("");
    container_parts(&mut body, length, depth, c.h, c.color);
    if along_z {
        body = rotated(body, 0.0, FRAC_PI_2, 0.0);
    }
    adopt_children(group, body);
}

fn container_parts(group: &mut Node, length: f64, depth: f64, h: f64, color: u32) {
    let dark = scale_hex_color(color, 0.62);
    let (half_l, half_d) = (length / 2.0, depth / 2.0);
    // Steel frame: floor and roof rails along both sides and across both ends,
    // corner posts, and castings standing just proud of the roof (coplanar tops
    // flicker as the camera moves).
    for s in [-1.0, 1.0] {
        put(
            group,
            box_part(length, 0.24, RAIL, dark, 0.0),
            0.0,
            0.12,
            s * (half_d - RAIL / 2.0),
        );
        put(
            group,
            box_part(length, RAIL, RAIL, dark, 0.0),
            0.0,
            h - RAIL / 2.0,
            s * (half_d - RAIL / 2.0),
        );
        put(
            group,
            box_part(RAIL, 0.24, depth, dark, 0.0),
            s * (half_l - RAIL / 2.0),
            0.12,
            0.0,
        );
        put(
            group,
            box_part(RAIL, 0.26, depth, dark, 0.0),
            s * (half_l - RAIL / 2.0),
            h - 0.13,
            0.0,
        );
        for t in [-1.0, 1.0] {
            let (x, z) = (s * (half_l - POST / 2.0), t * (half_d - POST / 2.0));
            put(group, box_part(POST, h, POST, dark, 0.0), x, h / 2.0, z);
            for y in [0.13, h - 0.11] {
                put(group, box_part(0.28, 0.26, 0.28, LOCK_SILVER, 0.0), x, y, z);
            }
        }
    }

    // Corrugated side walls with the livery wrapped over their ribs.
    let wall = Panel {
        x: [-half_l + POST, half_l - POST],
        y: [0.24, h - RAIL],
        origin: -half_l + POST,
        pitch: WALL_PITCH,
        depth: WALL_DEPTH,
        lift: 0.0,
    };
    let livery_width = (length * 0.55).min(7.5);
    let livery_height = (livery_width / 4.0).min(h * 0.4);
    let livery = Panel {
        x: [-livery_width / 2.0, livery_width / 2.0],
        y: [h * 0.6 - livery_height / 2.0, h * 0.6 + livery_height / 2.0],
        lift: LIVERY_LIFT,
        ..wall
    };
    let row = livery_for(color);
    let wall_z = half_d - RAIL / 2.0 - WALL_DEPTH;
    for (yaw, side) in [(0.0, 1.0), (PI, -1.0)] {
        put(
            group,
            rotated(wall_panel(&wall, h, color), 0.0, yaw, 0.0),
            0.0,
            0.0,
            side * wall_z,
        );
        put(
            group,
            rotated(livery_strip(&livery, row), 0.0, yaw, 0.0),
            0.0,
            0.0,
            side * wall_z,
        );
    }

    // Roof: shallow ribs across the container.
    let roof = Panel {
        x: [-half_l + RAIL, half_l - RAIL],
        y: [-half_d + RAIL, half_d - RAIL],
        origin: -half_l + RAIL,
        pitch: ROOF_PITCH,
        depth: ROOF_DEPTH,
        lift: 0.0,
    };
    let roof_mesh = PANELS.get_or_insert(panel_key(&roof, 9), || {
        roof.mesh(|x, y| [x / WEAR_TILE, 0.45 + y / (WEAR_TILE * 4.0)])
    });
    put(
        group,
        rotated(
            shadowed(roof_mesh, container_paint(color)),
            -FRAC_PI_2,
            0.0,
            0.0,
        ),
        0.0,
        h - 0.07,
        0.0,
    );

    // Front end wall, corrugated across the container.
    let front = Panel {
        x: [-half_d + POST, half_d - POST],
        y: [0.24, h - 0.26],
        origin: -half_d + POST,
        pitch: WALL_PITCH,
        depth: WALL_DEPTH,
        lift: 0.0,
    };
    put(
        group,
        rotated(wall_panel(&front, h, color), 0.0, -FRAC_PI_2, 0.0),
        -half_l + RAIL / 2.0 + WALL_DEPTH,
        0.0,
        0.0,
    );

    // Cargo doors: two leaves, each with two locking bars, cam keepers, handles
    // and hinges; a rubber gasket between them and a data plate on the right.
    let door_x = half_l - 0.07;
    let leaf_width = depth / 2.0 - POST - 0.02;
    let leaf_height = h - 0.24 - 0.26;
    for side in [-1.0, 1.0] {
        let leaf_z = side * (leaf_width / 2.0 + 0.02);
        put(
            group,
            wear_box(0.06, leaf_height, leaf_width, h, 0.24, color),
            door_x,
            0.24 + leaf_height / 2.0,
            leaf_z,
        );
        for y in [0.24 + leaf_height * 0.2, 0.24 + leaf_height * 0.8] {
            put(
                group,
                box_part(0.04, 0.09, leaf_width - 0.1, dark, 0.0),
                door_x + 0.04,
                y,
                leaf_z,
            );
        }
        for bar in [0.27, 0.73] {
            let z = side * (0.02 + leaf_width * bar);
            put(
                group,
                box_part(0.06, h - 0.3, 0.06, LOCK_SILVER, 0.0),
                door_x + 0.08,
                h / 2.0,
                z,
            );
            put(
                group,
                box_part(0.06, 0.16, 0.38, LOCK_SILVER, 0.0),
                door_x + 0.12,
                1.15,
                z - side * 0.16,
            );
            for y in [0.3, h - 0.3] {
                put(
                    group,
                    box_part(0.1, 0.12, 0.14, dark, 0.0),
                    door_x + 0.07,
                    y,
                    z,
                );
            }
        }
        for y in [0.6, h / 2.0, h - 0.6] {
            put(
                group,
                box_part(0.08, 0.2, 0.12, dark, 0.0),
                door_x + 0.03,
                y,
                side * (half_d - POST - 0.04),
            );
        }
    }
    put(
        group,
        box_part(0.05, leaf_height, 0.05, GASKET, 0.0),
        door_x + 0.02,
        0.24 + leaf_height / 2.0,
        0.0,
    );
    put(
        group,
        box_part(0.03, 0.35, 0.55, DATA_PLATE, 0.0),
        door_x + 0.045,
        h * 0.7,
        -(leaf_width * 0.5 + 0.02),
    );
}

/// A container stacked on a moored ship. Seen only across the basin, it is a
/// painted box with its rails and corner castings: modelled ribs on every cargo
/// container made each ship tens of thousands of triangles, drawn again for
/// shadows and the water reflection, for no visible difference.
pub fn ship_container(group: &mut Node, c: CargoShape) {
    let along_z = c.d > c.w;
    let (length, depth) = if along_z { (c.d, c.w) } else { (c.w, c.d) };
    let mut body = Node::group("");
    let dark = scale_hex_color(c.color, 0.62);
    put(
        &mut body,
        wear_box(length - 0.1, c.h - 0.1, depth - 0.1, c.h, 0.05, c.color),
        0.0,
        c.h / 2.0,
        0.0,
    );
    for s in [-1.0, 1.0] {
        for y in [0.1, c.h - 0.08] {
            put(
                &mut body,
                box_part(length, RAIL, RAIL, dark, 0.0),
                0.0,
                y,
                s * (depth / 2.0 - RAIL / 2.0),
            );
        }
        put(
            &mut body,
            box_part(RAIL, c.h, depth, dark, 0.0),
            s * (length / 2.0 - RAIL / 2.0),
            c.h / 2.0,
            0.0,
        );
    }
    if along_z {
        body = rotated(body, 0.0, FRAC_PI_2, 0.0);
    }
    adopt_children(group, body);
}
