//! Port of `harbor-vessels.ts`: moored container ships and quay gantry cranes
//! beyond the harbor wall, and the tugs working the side channels. Crane loads sway
//! and the tugs run their loops; see [`harbor_fleet`].

use std::f64::consts::{FRAC_PI_2, PI};
use std::sync::Arc;

use glam::DVec3;

use crate::geometry::math::hex_to_linear;
use crate::geometry::{Mesh, torus_geometry};
use crate::scene::Node;

use super::batching::batch;
use super::effects_scenery::ApronMode;
use super::harbor_models::{CargoShape, livery_decal, ship_container};
use super::harbor_surfaces::{apron_box, apron_material, steel_box};
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
/// by `lower_scale` (the inset lower hull), with a deck cap. Its paint is a vertex
/// color under the hull effect's weathering, so every band of a ship merges into
/// one draw.
fn hull_section(bottom: f64, top: f64, lower_scale: f64, color: u32) -> Node {
    hull_band(&OUTLINE, bottom, top, lower_scale, color)
}

/// [`hull_section`] round any `outline`.
fn hull_band(outline: &[[f64; 2]], bottom: f64, top: f64, lower_scale: f64, color: u32) -> Node {
    let mut positions = Vec::with_capacity(outline.len() * 6);
    for (y, scale) in [(bottom, lower_scale), (top, 1.0)] {
        for [x, z] in outline {
            positions.extend([x * scale, y, z * scale]);
        }
    }
    let n = outline.len() as u32;
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
    let [r, g, b] = hex_to_linear(color);
    flat.colors = vec![[r as f32, g as f32, b as f32]; flat.positions.len()];
    Node::mesh(Arc::new(flat), apron_material(ApronMode::Hull, 0xffffff))
}

const CARGO_PAINT: [u32; 5] = [0xcb7d43, 0x3b9394, 0x6e8eae, 0xbaad7c, 0xa76155];
const GRATING_DARK: u32 = 0x3d4548;
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
    // The line's name along the hull, lashing bridges between the bays, draft
    // marks at bow and stern, and mooring winches on the forecastle and poop.
    for (yaw, side) in [(0.0, 1.0), (PI, -1.0)] {
        put(
            &mut ship,
            rotated(livery_decal(15.0, variant % 4), 0.0, yaw, 0.0),
            -1.0,
            0.75,
            side * 7.03,
        );
        for (x, dz) in [(23.0, 7.0), (-29.0, 7.0)] {
            for mark in 0..5 {
                put(
                    &mut ship,
                    box_part(0.5, 0.12, 0.05, SHIP_WHITE, 0.0),
                    x,
                    -1.0 + f64::from(mark) * 0.5,
                    side * (dz + 0.03),
                );
            }
        }
    }
    for bay in 0..5 {
        let x = -19.1 + f64::from(bay) * 10.2;
        put(
            &mut ship,
            box_part(0.35, 2.8, 11.6, 0xc9b65a, 0.0),
            x,
            4.25,
            0.0,
        );
        put(
            &mut ship,
            box_part(0.5, 0.1, 11.8, GRATING_DARK, 0.0),
            x,
            5.7,
            0.0,
        );
    }
    for (x, z) in [(26.0, 2.6), (26.0, -2.6), (-30.5, 2.4), (-30.5, -2.4)] {
        let drum = rotated(cylinder_part(0.45, 1.1, 0x465558, 10), FRAC_PI_2, 0.0, 0.0);
        put(&mut ship, drum, x, if x > 0.0 { 4.75 } else { 3.3 }, z);
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
        steel_box(2.95, 0.4, 3.35, 0x3b3f40),
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
    // A deck crane on the side walkway, its jib stowed forward in a crutch: the
    // feeder's own gear, and the silhouette that breaks the line of boxes.
    let crane_z = if variant.is_multiple_of(2) { 5.4 } else { -5.4 };
    put(
        &mut ship,
        cylinder_part(0.55, 4.6, DECK_CRANE, 10),
        1.3,
        5.0,
        crane_z,
    );
    put(
        &mut ship,
        box_part(1.6, 1.5, 1.4, DECK_CRANE, 0.0),
        1.3,
        7.9,
        crane_z,
    );
    put(
        &mut ship,
        box_part(0.06, 0.6, 1.0, 0x335b6c, 0.0),
        2.12,
        8.0,
        crane_z,
    );
    beam(
        &mut ship,
        [1.6, 8.2, crane_z],
        [15.5, 9.6, crane_z],
        0.45,
        DECK_CRANE,
    );
    beam(
        &mut ship,
        [15.0, 6.0, crane_z],
        [15.0, 9.4, crane_z],
        0.25,
        DECK_CRANE,
    );
    beam(
        &mut ship,
        [1.3, 8.9, crane_z],
        [15.4, 9.5, crane_z],
        0.05,
        CABLE,
    );
    // Foremast with its crosstree and lamp, and the ensign aft.
    beam(
        &mut ship,
        [26.5, 4.4, 0.0],
        [26.5, 12.5, 0.0],
        0.22,
        SHIP_WHITE,
    );
    beam(
        &mut ship,
        [26.5, 10.8, -1.4],
        [26.5, 10.8, 1.4],
        0.12,
        SHIP_WHITE,
    );
    put(
        &mut ship,
        box_part(0.3, 0.3, 0.3, 0xffd9a0, 0.0),
        26.5,
        12.75,
        0.0,
    );
    beam(
        &mut ship,
        [-33.6, 3.0, 0.0],
        [-33.6, 7.4, 0.0],
        0.08,
        SHIP_WHITE,
    );
    put(
        &mut ship,
        box_part(1.3, 0.85, 0.04, FUNNEL_BAND[(variant + 1) % 3], 0.0),
        -34.3,
        6.9,
        0.0,
    );
    // Hatch coamings showing between the stacks, and bitts at the bow and stern.
    for bay in 0..4 {
        put(
            &mut ship,
            box_part(9.2, 0.35, 9.9, 0x6f6a5a, 0.0),
            -14.0 + f64::from(bay) * 10.2,
            2.95,
            0.0,
        );
    }
    for (x, z) in [(27.5, 3.6), (27.5, -3.6), (-31.5, 3.4), (-31.5, -3.4)] {
        for dx in [-0.35, 0.35] {
            put(
                &mut ship,
                cylinder_part(0.18, 0.6, 0x2c3336, 8),
                x + dx,
                if x > 0.0 { 4.75 } else { 3.15 },
                z,
            );
        }
    }
    batch(&mut ship);
    ship
}

/// Paint of the ships' deck cranes.
const DECK_CRANE: u32 = 0xe2b443;

const CRANE_GOLD: u32 = 0xe9b347;
const CRANE_BLUE: u32 = 0x2f6c9c;
/// Machinery house roof and the walkway grating around it.
const ROOF_GREY: u32 = 0xb4b9b4;
const GRATING: u32 = 0x5d676a;

/// How one quay crane is painted and where its trolley stands.
#[derive(Clone, Copy)]
struct CraneLivery {
    steel: u32,
    /// Trolley position along the boom (crane frame, metres toward -z).
    trolley: f64,
    /// The box on its spreader.
    cargo: u32,
}
const CRANE_DARK: u32 = 0x394f5d;
const CRANE_WHITE: u32 = 0xe8e4d8;
const CRANE_GLASS: u32 = 0x5f93a3;
const CRANE_RED: u32 = 0xc8473b;
const CABLE: u32 = 0x2b3236;
/// The gantry's wheel bogies: grey steel, greasy but not black.
const BOGIE: u32 = 0x6a7377;

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
/// The spreader's drop below the trolley.
const SPREADER_DROP: f64 = 8.0;

/// A ship-to-shore gantry crane on its quay plinth: four legs on wheel bogies, a
/// braced portal, an A-frame apex whose stays carry the boom, a machinery house
/// on the backreach, and a trolley with its hanging cab. The spreader and the
/// container it carries hang below the trolley as the group's last child.
fn gantry_crane(livery: CraneLivery) -> Node {
    let mut group = Node::group("quay-crane");
    let g = &mut group;
    let (gold, dark) = (livery.steel, CRANE_DARK);
    let trolley_z = livery.trolley;
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
            put(g, box_part(2.6, 0.8, 1.0, BOGIE, 0.0), x, 0.75, z);
            for dx in [-0.75, 0.75] {
                let wheel = rotated(cylinder_part(0.36, 0.5, 0x4a5256, 12), FRAC_PI_2, 0.0, 0.0);
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
        steel_box(4.8, 0.25, 4.4, ROOF_GREY),
        0.0,
        BOOM_Y + 3.65,
        BACKREACH - 2.6,
    );
    // Roof plant: two cooling units with their fan rings, an exhaust and a
    // handrail round the edge, so the house reads as machinery from the air.
    for dx in [-1.2, 1.2] {
        put(
            g,
            box_part(1.4, 0.6, 1.4, 0xd8dcd6, 0.0),
            dx,
            BOOM_Y + 4.08,
            BACKREACH - 3.3,
        );
        put(
            g,
            cylinder_part(0.45, 0.08, 0x384246, 12),
            dx,
            BOOM_Y + 4.42,
            BACKREACH - 3.3,
        );
    }
    put(
        g,
        cylinder_part(0.22, 1.2, 0x6b7375, 10),
        1.5,
        BOOM_Y + 4.35,
        BACKREACH - 1.4,
    );
    for (w, d, x, z) in [
        (4.8, 0.06, 0.0, BACKREACH - 0.4),
        (4.8, 0.06, 0.0, BACKREACH - 4.8),
        (0.06, 4.4, 2.4, BACKREACH - 2.6),
        (0.06, 4.4, -2.4, BACKREACH - 2.6),
    ] {
        put(
            g,
            box_part(w, 0.06, d, CRANE_WHITE, 0.0),
            x,
            BOOM_Y + 4.75,
            z,
        );
    }
    put(
        g,
        box_part(0.4, 0.5, 0.4, CRANE_RED, 0.0),
        -1.9,
        BOOM_Y + 4.05,
        BACKREACH - 0.9,
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
        steel_box(3.4, 1.0, 3.0, CRANE_WHITE),
        0.0,
        BOOM_Y - 1.1,
        trolley_z,
    );
    for dx in [-1.5, 1.5] {
        for dz in [-1.1, 1.1] {
            let wheel = rotated(cylinder_part(0.3, 0.25, 0x252c30, 10), 0.0, 0.0, FRAC_PI_2);
            put(g, wheel, dx, BOOM_Y - 0.55, trolley_z + dz);
        }
    }
    put(
        g,
        box_part(2.6, 0.12, 2.0, GRATING, 0.0),
        0.0,
        BOOM_Y - 0.55,
        trolley_z,
    );
    put(
        g,
        steel_box(2.0, 1.9, 2.2, CRANE_WHITE),
        0.9,
        BOOM_Y - 2.7,
        trolley_z - 0.6,
    );
    put(
        g,
        box_part(2.04, 0.9, 2.24, CRANE_GLASS, 0.0),
        0.9,
        BOOM_Y - 2.55,
        trolley_z - 0.65,
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
            color: livery.cargo,
        },
    );
    cargo.position.y = -SPREADER_DROP - 0.25 - 2.6;
    adopt_children(&mut load, cargo);
    batch(&mut load);
    put(&mut group, load, 0.0, BOOM_Y - 1.6, trolley_z);
    group
}

/// Ship berths: x, z, yaw, scale, hull color. Side berths make ships visible from
/// both teams' normal deployment cameras.
const SHIPS: [(f64, f64, f64, f64, u32); 3] = [
    (-8.0, -80.0, 0.0, 1.0, 0x3f6d80),
    (-77.0, -8.0, FRAC_PI_2, 0.85, 0x527f79),
    (77.0, 15.0, -FRAC_PI_2, 0.72, 0x9a6257),
];
/// Crane positions (x, z, yaw, scale) and liveries: the main berth's pair in the
/// terminal's gold, working at different reaches, and the older blue pair on the
/// side berths.
const CRANES: [(f64, f64, f64, f64, CraneLivery); 4] = [
    (
        -40.0,
        -65.0,
        0.0,
        1.0,
        CraneLivery {
            steel: CRANE_GOLD,
            trolley: -15.0,
            cargo: 0x3b7fa6,
        },
    ),
    (
        40.0,
        -65.0,
        0.0,
        1.0,
        CraneLivery {
            steel: CRANE_GOLD,
            trolley: -9.5,
            cargo: 0xc8562f,
        },
    ),
    (
        -65.5,
        16.0,
        FRAC_PI_2,
        0.75,
        CraneLivery {
            steel: CRANE_BLUE,
            trolley: -17.0,
            cargo: 0xd8d2c4,
        },
    ),
    (
        65.5,
        -20.0,
        -FRAC_PI_2,
        0.75,
        CraneLivery {
            steel: CRANE_BLUE,
            trolley: -12.0,
            cargo: 0x3f8f6a,
        },
    ),
];

/// A harbor tug's waterline outline (x along the boat, bow toward +x).
const TUG_OUTLINE: [[f64; 2]; 9] = [
    [-5.8, -1.9],
    [-5.2, -2.3],
    [3.0, -2.3],
    [5.0, -1.5],
    [6.0, 0.0],
    [5.0, 1.5],
    [3.0, 2.3],
    [-5.2, 2.3],
    [-5.8, 1.9],
];
const TUG_HULL: u32 = 0x24292d;
const TUG_BULWARK: u32 = 0xc8473b;
const TUG_FUNNEL: u32 = 0xe08a2e;

/// A harbor tug in its own frame, bow toward +x, the water at y = -2.2 like the
/// moored ships: an antifouling bottom, a dark hull with a rubbing strake of
/// fendering, red bulwarks, a white wheelhouse with wraparound glazing, a funnel
/// in the towage line's orange, the towing winch and hook aft, and a mast.
fn tug() -> Node {
    // Seen from across the basin, its hull needs no plating effect: plain paint,
    // so the whole boat batches into one draw.
    let band = |outline: &[[f64; 2]], bottom, top, lower_scale, color| {
        let mut band = hull_band(outline, bottom, top, lower_scale, color);
        if let Some(drawable) = &mut band.drawable {
            drawable.material = paint(color);
        }
        band
    };
    let mut boat = Node::group("harbor-tug");
    let b = &mut boat;
    b.children
        .push(band(&TUG_OUTLINE, -3.1, -2.05, 0.8, 0xa95443));
    b.children
        .push(band(&TUG_OUTLINE, -2.05, -0.95, 1.0, TUG_HULL));
    // Fendering all round the sheer, proud of the hull.
    let fender: Vec<[f64; 2]> = TUG_OUTLINE
        .iter()
        .map(|[x, z]| [x * 1.05, z * 1.12])
        .collect();
    b.children.push(band(&fender, -1.25, -0.75, 1.0, 0x1a1d1f));
    b.children
        .push(band(&TUG_OUTLINE, -0.95, -0.35, 1.0, TUG_BULWARK));
    put(b, box_part(10.4, 0.1, 4.2, 0x6b6f6a, 0.0), -0.2, -0.9, 0.0);
    // Wheelhouse on its deckhouse, glazed all round under a flared roof.
    put(b, box_part(5.0, 1.4, 3.4, 0xe9e5da, 0.0), 0.9, -0.15, 0.0);
    put(b, box_part(3.2, 1.5, 3.0, 0xf0ece2, 0.0), 1.4, 1.3, 0.0);
    put(b, box_part(3.26, 0.65, 3.06, 0x2c4b5a, 0.0), 1.4, 1.45, 0.0);
    put(b, box_part(3.8, 0.16, 3.5, 0xd8d4c8, 0.0), 1.5, 2.12, 0.0);
    // Funnel with its black top, the mast with lights, and a searchlight.
    put(b, box_part(1.1, 1.8, 1.5, TUG_FUNNEL, 0.0), -0.9, 0.9, 0.0);
    put(b, box_part(1.14, 0.35, 1.54, 0x1c1f21, 0.0), -0.9, 1.9, 0.0);
    put(b, box_part(0.12, 2.6, 0.12, 0xe9e5da, 0.0), 1.9, 3.4, 0.0);
    put(b, box_part(0.9, 0.08, 0.08, 0xe9e5da, 0.0), 1.9, 4.1, 0.0);
    put(b, box_part(0.2, 0.2, 0.2, 0xffd9a0, 0.0), 1.9, 4.8, 0.0);
    // Towing winch, its drum and the tow hook on the open after deck.
    put(b, box_part(1.2, 0.7, 2.2, 0x465558, 0.0), -3.2, -0.5, 0.0);
    let drum = rotated(cylinder_part(0.45, 1.6, 0x8a3b30, 10), FRAC_PI_2, 0.0, 0.0);
    put(b, drum, -3.2, -0.15, 0.0);
    put(b, box_part(0.5, 1.1, 0.5, 0xd9b44a, 0.0), -4.5, -0.3, 0.0);
    // Bow fender: the tyre pad tugs push with.
    put(b, box_part(0.9, 1.2, 2.2, 0x1a1d1f, 0.0), 5.7, -1.35, 0.0);
    batch(b);
    // Its shadow could only fall on the unlit water.
    for part in &mut boat.children {
        if let Some(drawable) = &mut part.drawable {
            drawable.cast_shadow = false;
        }
    }
    boat
}

/// Tugs working the side channels between the moored ships and the outer quays,
/// each round its own stadium-shaped loop: (centre x, half the straight along z,
/// turning radius, metres along the loop at time 0), all at [`TUG_SPEED`]. The
/// water draws their wakes from the same loops (`shaders/water.wgsl`, `TUGS`).
const TUG_LANES: [(f64, f64, f64, f64); 2] = [(-93.5, 46.0, 5.0, 0.0), (93.0, 38.0, 4.5, 120.0)];
const TUG_SPEED: f64 = 3.2;

/// Where a tug on `lane` is at `time`: x, z and its heading as a unit vector.
fn tug_pose(lane: (f64, f64, f64, f64), time: f64) -> (f64, f64, [f64; 2]) {
    let (centre, half, radius, start) = lane;
    let straight = 2.0 * half;
    let turn = PI * radius;
    let s = (start + time * TUG_SPEED).rem_euclid(2.0 * (straight + turn));
    if s < straight {
        (centre + radius, -half + s, [0.0, 1.0])
    } else if s < straight + turn {
        let a = (s - straight) / radius;
        (
            centre + radius * a.cos(),
            half + radius * a.sin(),
            [-a.sin(), a.cos()],
        )
    } else if s < 2.0 * straight + turn {
        (centre - radius, half - (s - straight - turn), [0.0, -1.0])
    } else {
        let a = PI + (s - 2.0 * straight - turn) / radius;
        (
            centre + radius * a.cos(),
            -half + radius * a.sin(),
            [-a.sin(), a.cos()],
        )
    }
}

/// `HarborFleet`: the fleet group, three ships (the first children), then four
/// cranes, then a lane per tug whose only (last) child is the moving tug.
pub fn harbor_fleet() -> Node {
    let mut group = Node::group("");
    for (i, &(x, z, yaw, scale, color)) in SHIPS.iter().enumerate() {
        let mut ship = rotated(container_ship(color, i), 0.0, yaw, 0.0);
        ship.scale = DVec3::splat(scale);
        put(&mut group, ship, x, 0.0, z);
    }
    for &(x, z, yaw, scale, livery) in &CRANES {
        let mut crane = rotated(gantry_crane(livery), 0.0, yaw, 0.0);
        crane.scale = DVec3::splat(scale);
        put(&mut group, crane, x, 0.0, z);
    }
    for _ in TUG_LANES {
        let mut lane = Node::group("tug-lane");
        lane.children.push(tug());
        group.children.push(lane);
    }
    group
}

/// `HarborFleet.update(time)`: crane loads sway and the tugs run their loops. The
/// moored ships lie still: their bob was a few centimetres no camera could see, and
/// holding them lets the renderer bake them into the static scenery, so a view of
/// the basin no longer draws every ship part again, nor shadows nor reflects it.
pub fn update_harbor_fleet(fleet: &mut Node, time: f64) {
    for i in 0..CRANES.len() {
        let crane = &mut fleet.children[SHIPS.len() + i];
        let load = crane.children.last_mut().expect("crane load");
        load.set_rotation_euler(0.0, 0.0, (time * 0.7 + i as f64).sin() * 0.025);
    }
    for (i, &lane) in TUG_LANES.iter().enumerate() {
        let (x, z, [dx, dz]) = tug_pose(lane, time);
        let tug = fleet.children[SHIPS.len() + CRANES.len() + i]
            .children
            .last_mut()
            .expect("tug");
        // Pitching a little to the chop, heeling into the turns.
        let turning = 1.0 - dx.abs().max(dz.abs()).powi(8);
        tug.position = DVec3::new(x, (time * 1.3 + i as f64).sin() * 0.06, z);
        tug.rotation = glam::DQuat::from_rotation_y((-dz).atan2(dx))
            * glam::DQuat::from_rotation_x(turning * 0.06)
            * glam::DQuat::from_rotation_z((time * 1.1 + i as f64 * 2.0).sin() * 0.012);
    }
}
