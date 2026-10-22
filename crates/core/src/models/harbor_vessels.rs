//! Port of `harbor-vessels.ts`: moored container ships and quay gantry cranes
//! beyond the harbor wall. Ships bob and crane loads sway; see [`harbor_fleet`].

use std::f64::consts::FRAC_PI_2;
use std::sync::Arc;

use glam::DVec3;

use crate::geometry::{Mesh, torus_geometry};
use crate::scene::Node;

use super::batching::batch;
use super::effects_scenery::ApronMode;
use super::harbor_models::{CargoShape, ship_container};
use super::harbor_surfaces::{HarborSurface, apron_box, harbor_material, steel_box};
use super::model_primitives::{DEFAULT_BOX_RADIUS, box_part, cylinder_part, paint, put, rotated};
use super::model_primitives::{adopt_children, beam};

/// Hull cross-section outline (x along the ship, z across): a tapered stern and a
/// pointed bow.
const OUTLINE: [[f64; 2]; 9] = [
    [-34.0, -4.8],
    [-30.0, -7.0],
    [24.0, -7.0],
    [31.0, -4.8],
    [36.0, 0.0],
    [31.0, 4.8],
    [24.0, 7.0],
    [-30.0, 7.0],
    [-34.0, 4.8],
];

/// One flat-shaded hull band between `bottom` and `top`, its lower outline scaled
/// by `lower_scale` (the inset lower hull), with a deck cap.
fn hull_section(bottom: f64, top: f64, lower_scale: f64, color: u32) -> Node {
    let mut positions = Vec::with_capacity(OUTLINE.len() * 6);
    for (y, scale) in [(bottom, lower_scale), (top, 1.0)] {
        for [x, z] in OUTLINE {
            positions.extend([x * scale, y, z * scale]);
        }
    }
    let n = OUTLINE.len() as u32;
    let mut indices = Vec::new();
    for i in 0..n {
        let j = (i + 1) % n;
        indices.extend([i, i + n, j, j, i + n, j + n]);
    }
    for i in 1..n - 1 {
        indices.extend([n, n + i + 1, n + i]);
    }
    let mut flat = Mesh::from_f64(&positions, &[], &[], Some(indices)).to_non_indexed();
    flat.compute_vertex_normals();
    flat.uvs = flat
        .positions
        .iter()
        .map(|p| {
            [
                ((f64::from(p[0]) + f64::from(p[2])) / 5.0) as f32,
                (f64::from(p[1]) / 5.0) as f32,
            ]
        })
        .collect();
    Node::mesh(Arc::new(flat), harbor_material(HarborSurface::Steel, color))
}

const CARGO_PAINT: [u32; 5] = [0xcb7d43, 0x3b9394, 0x6e8eae, 0xbaad7c, 0xa76155];
const SHIP_WHITE: u32 = 0xf0ebdc;
/// The shipping lines' funnel colors, one per ship.
const FUNNEL_BAND: [u32; 3] = [0x2f6fa8, 0xc8473b, 0x2f8a6a];

fn container_ship(color: u32, variant: usize) -> Node {
    let mut ship = Node::group("container-ship");
    ship.children.push(hull_section(-3.8, -1.3, 0.85, 0xa95443));
    ship.children.push(hull_section(-1.3, 2.6, 1.0, color));
    // A white sheer strake along the top of the hull.
    ship.children.push(hull_section(2.6, 3.05, 1.0, SHIP_WHITE));
    put(
        &mut ship,
        steel_box(58.0, 0.2, 12.7, 0xbbbaa5),
        -2.0,
        2.72,
        0.0,
    );
    for bay in 0..4 {
        for row in [-1.0, 1.0] {
            let levels = if bay == variant % 4 { 1 } else { 2 };
            for level in 0..levels {
                let mut cargo = Node::group("");
                let paint_index = (bay + level * 2 + variant + usize::from(row > 0.0)) % 5;
                ship_container(
                    &mut cargo,
                    CargoShape {
                        w: 9.6,
                        d: 4.5,
                        h: 3.2,
                        color: CARGO_PAINT[paint_index],
                    },
                );
                // Bake cargo into its ship before batching the whole vessel.
                cargo.position = DVec3::new(
                    -14.0 + bay as f64 * 10.2,
                    2.85 + level as f64 * 3.27,
                    row * 2.5,
                );
                adopt_children(&mut ship, cargo);
            }
        }
    }
    // Accommodation block, wraparound bridge glazing, deck rails and twin exhausts.
    put(
        &mut ship,
        steel_box(8.0, 7.5, 10.5, 0xf0e6c9),
        -26.0,
        6.55,
        0.0,
    );
    put(
        &mut ship,
        steel_box(9.3, 2.3, 11.4, 0xf0e6c9),
        -25.5,
        11.35,
        0.0,
    );
    put(
        &mut ship,
        box_part(9.7, 0.22, 11.8, 0xd9d4b9, DEFAULT_BOX_RADIUS),
        -25.5,
        12.62,
        0.0,
    );
    for side in [-1.0, 1.0] {
        for i in 0..6 {
            put(
                &mut ship,
                box_part(1.05, 1.15, 0.08, 0x335b6c, 0.0),
                -29.2 + f64::from(i) * 1.45,
                11.4,
                side * 5.72,
            );
        }
        put(
            &mut ship,
            box_part(0.08, 1.15, 8.8, 0x335b6c, 0.0),
            -20.82,
            11.4,
            0.0,
        );
        for i in 0..3 {
            put(
                &mut ship,
                box_part(0.08, 1.1, 1.1, 0x526e75, DEFAULT_BOX_RADIUS),
                -21.95,
                5.0 + f64::from(i) * 1.7,
                side * 2.8,
            );
        }
        beam(
            &mut ship,
            [-32.0, 3.65, side * 6.3],
            [25.0, 3.65, side * 6.3],
            0.09,
            0xd8d2b9,
        );
        let mut x = -32.0;
        while x < 26.0 {
            beam(
                &mut ship,
                [x, 2.8, side * 6.3],
                [x, 3.65, side * 6.3],
                0.08,
                0xd8d2b9,
            );
            x += 3.0;
        }
        for x in [-30.0, 26.0] {
            put(
                &mut ship,
                cylinder_part(0.5, 0.35, 0x293e4a, 12),
                x,
                3.1,
                side * 4.5,
            );
        }
        for x in [-22.0, 24.0] {
            let lifering = Node::mesh(Arc::new(torus_geometry(0.48, 0.12, 6, 12)), paint(0xe88b4a));
            put(&mut ship, lifering, x, 3.8, side * 6.42);
        }
    }
    put(
        &mut ship,
        steel_box(2.8, 4.1, 3.2, 0xc57943),
        -27.0,
        14.2,
        -1.7,
    );
    for z in [-2.3, -1.1] {
        put(
            &mut ship,
            cylinder_part(0.43, 1.2, 0x2d4149, 12),
            -27.0,
            16.5,
            z,
        );
    }
    beam(
        &mut ship,
        [-22.0, 12.7, 0.0],
        [-22.0, 18.0, 0.0],
        0.1,
        0xe6dec4,
    );
    beam(
        &mut ship,
        [-22.0, 16.8, -2.0],
        [-22.0, 16.8, 2.0],
        0.08,
        0xe6dec4,
    );
    put(
        &mut ship,
        box_part(3.2, 0.18, 0.35, 0xe1d8bf, DEFAULT_BOX_RADIUS),
        -22.0,
        17.7,
        0.0,
    );
    // Raised forecastle, bridge wings with their cabs, a banded funnel, lifeboats
    // in davits and a radar mast: the silhouette details a ship reads by.
    put(&mut ship, steel_box(7.0, 1.5, 11.0, color), 23.5, 3.45, 0.0);
    put(
        &mut ship,
        steel_box(7.2, 0.3, 11.2, SHIP_WHITE),
        23.5,
        4.3,
        0.0,
    );
    put(
        &mut ship,
        steel_box(2.4, 0.35, 15.6, SHIP_WHITE),
        -22.0,
        10.1,
        0.0,
    );
    for side in [-1.0, 1.0] {
        put(
            &mut ship,
            steel_box(2.4, 1.6, 1.4, SHIP_WHITE),
            -22.0,
            11.1,
            side * 7.1,
        );
        put(
            &mut ship,
            box_part(2.44, 0.7, 1.44, 0x335b6c, 0.0),
            -22.0,
            11.35,
            side * 7.1,
        );
        let lifeboat = box_part(4.6, 1.3, 1.7, 0xe8742c, 0.45);
        put(&mut ship, lifeboat, -26.0, 8.4, side * 5.95);
        put(
            &mut ship,
            box_part(4.4, 0.25, 1.75, SHIP_WHITE, 0.0),
            -26.0,
            9.15,
            side * 5.95,
        );
        for x in [-27.8, -24.2] {
            beam(
                &mut ship,
                [x, 6.3, side * 5.3],
                [x, 9.8, side * 6.4],
                0.18,
                0x465558,
            );
        }
        // Anchor in its hawse pocket.
        put(
            &mut ship,
            box_part(1.1, 1.3, 0.3, 0x2c3336, 0.0),
            30.5,
            1.4,
            side * 5.15,
        );
    }
    put(
        &mut ship,
        steel_box(2.95, 0.9, 3.35, SHIP_WHITE),
        -27.0,
        15.0,
        -1.7,
    );
    put(
        &mut ship,
        steel_box(2.95, 0.5, 3.35, FUNNEL_BAND[variant % 3]),
        -27.0,
        15.6,
        -1.7,
    );
    put(
        &mut ship,
        steel_box(2.95, 0.4, 3.35, 0x22282a),
        -27.0,
        16.1,
        -1.7,
    );
    put(
        &mut ship,
        box_part(3.4, 0.14, 0.5, 0x2c3336, 0.0),
        -22.0,
        18.3,
        0.0,
    );
    // Bow windlass and anchor-chain guide.
    put(
        &mut ship,
        cylinder_part(0.75, 0.8, 0x465558, 12),
        29.0,
        3.1,
        0.0,
    );
    beam(
        &mut ship,
        [29.0, 3.0, 0.0],
        [34.0, 2.8, 0.0],
        0.16,
        0x687271,
    );
    batch(&mut ship);
    ship
}

const CRANE_GOLD: u32 = 0xe9b347;
const CRANE_DARK: u32 = 0x394f5d;
const CRANE_WHITE: u32 = 0xe8e4d8;
const CRANE_GLASS: u32 = 0x5f93a3;
const CRANE_RED: u32 = 0xc8473b;
const CABLE: u32 = 0x2b3236;

/// Ship-to-shore crane geometry, metres, in the crane's frame: the gantry runs
/// along x on the quay, the boom reaches out over the water toward -z.
const GANTRY_HALF_SPAN: f64 = 4.6;
const GANTRY_HALF_GAUGE: f64 = 2.6;
const PORTAL_TOP: f64 = 18.0;
const BOOM_Y: f64 = 19.4;
const BOOM_DEPTH: f64 = 1.5;
const BOOM_TIP: f64 = -24.0;
const BACKREACH: f64 = 5.5;
const APEX: [f64; 3] = [0.0, 29.0, 1.2];
/// Where the trolley parks along the boom, and the spreader's drop below it.
const TROLLEY_Z: f64 = -15.0;
const SPREADER_DROP: f64 = 8.0;

/// A ship-to-shore gantry crane on its quay plinth: four legs on wheel bogies, a
/// braced portal, an A-frame apex whose stays carry the boom, a machinery house
/// on the backreach, and a trolley with its hanging cab. The spreader and the
/// container it carries hang below the trolley as the group's last child.
fn gantry_crane() -> Node {
    let mut group = Node::group("quay-crane");
    let g = &mut group;
    let (gold, dark) = (CRANE_GOLD, CRANE_DARK);
    // The plinth reaches below the waterline so its face takes the tide band.
    put(
        g,
        apron_box(14.0, 3.4, 9.0, 0xa9aba2, ApronMode::QuayWall),
        0.0,
        -1.65,
        0.0,
    );
    for z in [-GANTRY_HALF_GAUGE, GANTRY_HALF_GAUGE] {
        // Crane rail along the plinth.
        put(g, box_part(14.0, 0.12, 0.3, 0x4c5356, 0.0), 0.0, 0.11, z);
        for side in [-1.0, 1.0] {
            let x = side * GANTRY_HALF_SPAN;
            // Bogie, wheels and the leg rising from it.
            put(g, steel_box(2.6, 0.8, 1.0, dark), x, 0.75, z);
            for dx in [-0.75, 0.75] {
                let wheel = rotated(cylinder_part(0.36, 0.5, 0x252c30, 12), FRAC_PI_2, 0.0, 0.0);
                put(g, wheel, x + dx, 0.42, z);
            }
            put(g, box_part(1.2, 0.5, 1.2, gold, 0.0), x, 1.4, z);
            beam(g, [x, 1.4, z], [x, PORTAL_TOP + 0.6, z], 0.9, gold);
            // Red-and-white collision band at the foot of each leg.
            for band in 0..4 {
                let color = if band % 2 == 0 {
                    CRANE_RED
                } else {
                    CRANE_WHITE
                };
                put(
                    g,
                    box_part(0.96, 0.4, 0.96, color, 0.0),
                    x,
                    2.0 + f64::from(band) * 0.4,
                    z,
                );
            }
        }
        // Sill beam low between the bogies, portal beams across the top.
        beam(
            g,
            [-GANTRY_HALF_SPAN, 2.6, z],
            [GANTRY_HALF_SPAN, 2.6, z],
            0.55,
            gold,
        );
        beam(
            g,
            [-GANTRY_HALF_SPAN, PORTAL_TOP, z],
            [GANTRY_HALF_SPAN, PORTAL_TOP, z],
            1.1,
            gold,
        );
    }
    for side in [-1.0, 1.0] {
        let x = side * GANTRY_HALF_SPAN;
        // Leg ties across the gauge and a K-brace on each portal frame.
        for y in [7.0, 12.5] {
            beam(
                g,
                [x, y, -GANTRY_HALF_GAUGE],
                [x, y, GANTRY_HALF_GAUGE],
                0.4,
                gold,
            );
        }
        beam(
            g,
            [x, 7.0, -GANTRY_HALF_GAUGE],
            [x, 12.5, GANTRY_HALF_GAUGE],
            0.28,
            gold,
        );
        beam(
            g,
            [x, 12.5, -GANTRY_HALF_GAUGE],
            [x, PORTAL_TOP, GANTRY_HALF_GAUGE],
            0.28,
            gold,
        );
    }
    // Boom: twin box girders from the backreach out over the water, with cross
    // ties, walkway rails and the trolley rails under them.
    for side in [-1.0, 1.0] {
        let x = side * 1.5;
        put(
            g,
            box_part(0.7, BOOM_DEPTH, BACKREACH - BOOM_TIP, gold, 0.0),
            x,
            BOOM_Y,
            (BACKREACH + BOOM_TIP) / 2.0,
        );
        put(
            g,
            box_part(0.12, 0.12, BACKREACH - BOOM_TIP, CRANE_WHITE, 0.0),
            side * 2.15,
            BOOM_Y + 1.25,
            (BACKREACH + BOOM_TIP) / 2.0,
        );
        let mut z = BOOM_TIP + 0.5;
        while z < BACKREACH {
            put(
                g,
                box_part(0.06, 0.9, 0.06, CRANE_WHITE, 0.0),
                side * 2.15,
                BOOM_Y + 0.8,
                z,
            );
            z += 2.0;
        }
        // Apex legs from the portal up to the A-frame's head, and the stays.
        beam(
            g,
            [x * 2.4, PORTAL_TOP, -GANTRY_HALF_GAUGE],
            APEX,
            0.6,
            gold,
        );
        beam(g, [x * 2.4, PORTAL_TOP, GANTRY_HALF_GAUGE], APEX, 0.6, gold);
        for (z, width) in [
            (BOOM_TIP + 1.5, 0.22),
            (-14.0, 0.26),
            (BACKREACH - 0.5, 0.3),
        ] {
            beam(
                g,
                [APEX[0] + x * 0.3, APEX[1], APEX[2]],
                [x, BOOM_Y + 0.7, z],
                width,
                gold,
            );
        }
    }
    let mut z = BOOM_TIP + 1.0;
    while z < BACKREACH {
        put(g, box_part(3.0, 0.3, 0.3, gold, 0.0), 0.0, BOOM_Y - 0.6, z);
        z += 3.0;
    }
    put(g, box_part(3.6, 1.2, 1.2, gold, 0.0), 0.0, APEX[1], APEX[2]);
    // Warning lights at the apex and the boom tip.
    put(
        g,
        box_part(0.4, 0.5, 0.4, CRANE_RED, 0.0),
        0.0,
        APEX[1] + 0.85,
        APEX[2],
    );
    for side in [-1.0, 1.0] {
        put(
            g,
            box_part(0.3, 0.4, 0.3, CRANE_RED, 0.0),
            side * 1.5,
            BOOM_Y + 0.95,
            BOOM_TIP + 0.3,
        );
    }
    // Machinery house on the backreach, with a vent stripe and its door.
    put(
        g,
        steel_box(4.6, 2.8, 4.2, CRANE_WHITE),
        0.0,
        BOOM_Y + 2.15,
        BACKREACH - 2.6,
    );
    put(
        g,
        steel_box(4.8, 0.25, 4.4, dark),
        0.0,
        BOOM_Y + 3.65,
        BACKREACH - 2.6,
    );
    put(
        g,
        box_part(4.64, 0.35, 3.0, CRANE_GLASS, 0.0),
        0.0,
        BOOM_Y + 2.6,
        BACKREACH - 2.6,
    );
    // Stair tower climbing the landside leg.
    let mut y = 2.2;
    let mut flight = 0;
    while y < PORTAL_TOP - 1.0 {
        let (from, to) = if flight % 2 == 0 {
            (-1.4, 1.4)
        } else {
            (1.4, -1.4)
        };
        let x = -GANTRY_HALF_SPAN - 1.0;
        beam(
            g,
            [x, y, GANTRY_HALF_GAUGE + from * 0.5],
            [x, y + 2.2, GANTRY_HALF_GAUGE + to * 0.5],
            0.12,
            CRANE_WHITE,
        );
        put(
            g,
            box_part(1.0, 0.08, 0.9, dark, 0.0),
            x,
            y + 2.2,
            GANTRY_HALF_GAUGE + to * 0.5,
        );
        y += 2.2;
        flight += 1;
    }
    // Trolley on the boom rails and the operator's cab hanging under it.
    put(
        g,
        steel_box(3.4, 1.0, 3.0, dark),
        0.0,
        BOOM_Y - 1.1,
        TROLLEY_Z,
    );
    put(
        g,
        steel_box(2.0, 1.9, 2.2, CRANE_WHITE),
        0.9,
        BOOM_Y - 2.7,
        TROLLEY_Z - 0.6,
    );
    put(
        g,
        box_part(2.04, 0.9, 2.24, CRANE_GLASS, 0.0),
        0.9,
        BOOM_Y - 2.55,
        TROLLEY_Z - 0.65,
    );
    batch(g);

    let mut load = Node::group("");
    for x in [-1.1, 1.1] {
        for z in [-0.6, 0.6] {
            beam(
                &mut load,
                [x, 0.0, z],
                [x * 1.4, -SPREADER_DROP, z],
                0.05,
                CABLE,
            );
        }
    }
    put(
        &mut load,
        box_part(6.4, 0.5, 2.6, gold, 0.0),
        0.0,
        -SPREADER_DROP,
        0.0,
    );
    for x in [-3.0, 3.0] {
        put(
            &mut load,
            box_part(0.3, 0.5, 2.7, dark, 0.0),
            x,
            -SPREADER_DROP,
            0.0,
        );
    }
    // A twenty-foot box locked under the spreader.
    let mut cargo = Node::group("");
    ship_container(
        &mut cargo,
        CargoShape {
            w: 6.1,
            d: 2.45,
            h: 2.6,
            color: 0x3b7fa6,
        },
    );
    cargo.position.y = -SPREADER_DROP - 0.25 - 2.6;
    adopt_children(&mut load, cargo);
    batch(&mut load);
    put(&mut group, load, 0.0, BOOM_Y - 1.6, TROLLEY_Z);
    group
}

/// Ship berths: x, z, yaw, scale, hull color. Side berths make ships visible from
/// both teams' normal deployment cameras.
const SHIPS: [(f64, f64, f64, f64, u32); 3] = [
    (-8.0, -80.0, 0.0, 1.0, 0x3f6d80),
    (-77.0, -8.0, FRAC_PI_2, 0.85, 0x527f79),
    (77.0, 15.0, -FRAC_PI_2, 0.72, 0x9a6257),
];
/// Crane positions: x, z, yaw, scale.
const CRANES: [(f64, f64, f64, f64); 4] = [
    (-40.0, -65.0, 0.0, 1.0),
    (40.0, -65.0, 0.0, 1.0),
    (-65.5, 16.0, FRAC_PI_2, 0.75),
    (65.5, -20.0, -FRAC_PI_2, 0.75),
];

/// `HarborFleet`: the fleet group, three ships (the first children), then four
/// cranes.
pub fn harbor_fleet() -> Node {
    let mut group = Node::group("");
    for (i, &(x, z, yaw, scale, color)) in SHIPS.iter().enumerate() {
        let mut ship = rotated(container_ship(color, i), 0.0, yaw, 0.0);
        ship.scale = DVec3::splat(scale);
        put(&mut group, ship, x, 0.0, z);
    }
    for &(x, z, yaw, scale) in &CRANES {
        let mut crane = rotated(gantry_crane(), 0.0, yaw, 0.0);
        crane.scale = DVec3::splat(scale);
        put(&mut group, crane, x, 0.0, z);
    }
    group
}

/// `HarborFleet.update(time)`: ships bob and roll slightly, crane loads sway.
pub fn update_harbor_fleet(fleet: &mut Node, time: f64) {
    for (i, &(_, _, yaw, _, _)) in SHIPS.iter().enumerate() {
        let ship = &mut fleet.children[i];
        let i = i as f64;
        ship.position.y = (time * 0.55 + i * 2.0).sin() * 0.07;
        ship.set_rotation_euler((time * 0.42 + i).sin() * 0.0018, yaw, 0.0);
    }
    for i in 0..CRANES.len() {
        let crane = &mut fleet.children[SHIPS.len() + i];
        let load = crane.children.last_mut().expect("crane load");
        load.set_rotation_euler(0.0, 0.0, (time * 0.7 + i as f64).sin() * 0.025);
    }
}
