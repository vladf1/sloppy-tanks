//! Port of `harbor-scenery.ts`: Harbor Havoc's retained scenery. The quay apron,
//! painted bays and labels, bollards, berths and warehouses; the water basin; the
//! moored fleet; and blinking beacons. All large scenery stays outside playable
//! cover.

use std::f64::consts::FRAC_PI_2;
use std::sync::Arc;

use crate::geometry::{plane_geometry, ring_geometry};
use crate::scene::{Material, Node, TextureRef, TextureSource, Wrap};

use super::batching::batch;
use super::effects_scenery::harbor_label_texture;
use super::harbor_surfaces::{HarborSurface, harbor_box};
use super::harbor_vessels::{HarborFleet, harbor_beam};
use super::harbor_water::harbor_water;
use super::house_surfaces::siding_box;
use super::model_primitives::{
    DEFAULT_BOX_RADIUS, TEAM_COLORS, box_part, cylinder_part, paint, put, rotated,
};
use super::pending_scenery::spawn_positions;

/// Indices of the animated groups among [`HarborScenery::root`]'s children.
const FLEET_CHILD: usize = 1;

/// Harbor Havoc scenery, built once and reused across rounds (`HarborScenery`).
pub struct HarborScenery {
    /// Children: the water, the fleet, the painted labels, the batched details,
    /// then the beacons.
    pub root: Node,
    beacons: usize,
}

impl Default for HarborScenery {
    fn default() -> Self {
        Self::new()
    }
}

/// `paintLabel(group, text, x, z, width, depth)`: painted lettering flat on the
/// apron, drawn by the browser (see `effects_scenery::harbor_label_texture`).
fn paint_label(group: &mut Node, text: &'static str, x: f64, z: f64, width: f64, depth: f64) {
    let map = TextureRef {
        source: TextureSource::Generated(harbor_label_texture(text)),
        wrap: Wrap::Clamp,
        ..TextureRef::file("")
    };
    let label = Node::mesh(
        Arc::new(plane_geometry(width, depth)),
        Arc::new(Material {
            map: Some(map),
            transparent: true,
            depth_write: false,
            ..Material::standard(0xffffff, 0.0, 1.0)
        }),
    );
    put(group, rotated(label, -FRAC_PI_2, 0.0, 0.0), x, 0.035, z);
}

const DOCK: HarborSurface = HarborSurface::Dock;

impl HarborScenery {
    pub fn new() -> Self {
        let mut root = Node::group("");
        root.children.push(harbor_water());
        root.children.push(HarborFleet::build());
        let mut details = Node::group("");
        let mut beacons = Node::group("");
        put(
            &mut details,
            harbor_box(124.0, 2.6, 124.0, 0x9aaba1, DOCK),
            0.0,
            -1.4,
            0.0,
        );
        put(
            &mut details,
            harbor_box(120.0, 0.12, 120.0, 0xd3d4cd, DOCK),
            0.0,
            -0.06,
            0.0,
        );
        // Broad circulation lanes surround the numbered container bays.
        for x in [-52.0, 0.0, 52.0] {
            let w = if x == 0.0 { 15.0 } else { 10.0 };
            put(
                &mut details,
                harbor_box(w, 0.015, 118.0, 0x9b9d94, DOCK),
                x,
                0.008,
                0.0,
            );
        }
        for z in [-45.0, 0.0, 45.0] {
            put(
                &mut details,
                harbor_box(118.0, 0.012, 10.0, 0x9b9d94, DOCK),
                0.0,
                0.018,
                z,
            );
        }
        for x in [-46.0, 46.0] {
            for z in (-54..=54).step_by(6) {
                put(
                    &mut details,
                    box_part(0.16, 0.018, 2.8, 0xdacf9a, 0.0),
                    x,
                    0.026,
                    f64::from(z),
                );
            }
        }
        // Worn expansion joints keep the apron from looking flat.
        for x in (-60..=60).step_by(12) {
            let x = f64::from(x);
            put(
                &mut details,
                box_part(0.045, 0.014, 120.0, 0x626e6c, 0.0),
                x,
                0.02,
                0.0,
            );
            put(
                &mut details,
                box_part(120.0, 0.014, 0.045, 0x626e6c, 0.0),
                0.0,
                0.02,
                x,
            );
        }
        for side in [-1.0, 1.0] {
            for z in [-32.0, -12.0, 12.0, 32.0] {
                for dx in [-3.7, 3.7] {
                    put(
                        &mut details,
                        box_part(0.12, 0.02, 15.0, 0xe4bb59, 0.0),
                        side * 30.0 + dx,
                        0.03,
                        z,
                    );
                }
                let bay = if z == -32.0 || z == 32.0 { "B2" } else { "B1" };
                paint_label(&mut root, bay, side * 37.0, z, 2.5, 1.4);
            }
            for x in (-57..=57).step_by(3) {
                let x = f64::from(x);
                put(
                    &mut details,
                    box_part(1.5, 0.025, 0.65, 0xe1b64d, 0.0),
                    x,
                    1.215,
                    side * 60.5,
                );
                put(
                    &mut details,
                    box_part(1.5, 0.025, 0.65, 0x303e42, 0.0),
                    x + 1.5,
                    1.215,
                    side * 60.5,
                );
            }
            for x in [-48.0, -24.0, 0.0, 24.0, 48.0] {
                // Quay bollards and fenders are beyond the wall.
                put(
                    &mut details,
                    cylinder_part(0.42, 0.7, 0x253e45, 12),
                    x,
                    0.05,
                    side * 62.0,
                );
                put(
                    &mut details,
                    box_part(1.4, 0.25, 0.5, 0x253e45, DEFAULT_BOX_RADIUS),
                    x,
                    0.45,
                    side * 62.0,
                );
                let fender = rotated(cylinder_part(0.65, 1.4, 0x25363d, 12), FRAC_PI_2, 0.0, 0.0);
                put(&mut details, fender, x, -1.1, side * 62.2);
            }
            for x in [-54.0, 54.0] {
                put(
                    &mut details,
                    cylinder_part(0.16, 6.0, 0x45575c, 12),
                    x,
                    3.0,
                    side * 62.0,
                );
                put(
                    &mut details,
                    box_part(1.5, 0.25, 0.7, 0xe3cc90, DEFAULT_BOX_RADIUS),
                    x,
                    6.0,
                    side * 62.0,
                );
                put(
                    &mut beacons,
                    cylinder_part(0.18, 0.3, 0xffb54a, 8),
                    x,
                    6.25,
                    side * 62.0,
                );
            }
        }
        // Central loading square is traversable; the laser pickup remains contested at its center.
        for side in [-1.0, 1.0] {
            put(
                &mut details,
                box_part(12.0, 0.025, 0.2, 0xe1b64d, 0.0),
                0.0,
                0.035,
                side * 5.0,
            );
            put(
                &mut details,
                box_part(0.2, 0.025, 10.0, 0xe1b64d, 0.0),
                side * 6.0,
                0.035,
                0.0,
            );
        }
        paint_label(&mut root, "HARBOR HAVOC", 0.0, -48.0, 28.0, 4.0);
        paint_label(&mut root, "PORT 07", 0.0, 48.0, 15.0, 3.5);
        paint_label(&mut root, "LOADING", 0.0, 3.5, 8.0, 1.1);
        for team in [0u8, 1] {
            for (x, z) in spawn_positions(team, 1.0) {
                put(
                    &mut details,
                    cylinder_part(2.65, 0.08, 0x293f4a, 12),
                    x,
                    0.06,
                    z,
                );
                let ring = Node::mesh(
                    Arc::new(ring_geometry(2.1, 2.3, 32)),
                    paint(TEAM_COLORS[usize::from(team)]),
                );
                put(
                    &mut details,
                    rotated(ring, -FRAC_PI_2, 0.0, 0.0),
                    x,
                    0.11,
                    z,
                );
            }
        }
        // Working berths sit beyond the wall: forklifts, pallets, drainage and tied mooring lines.
        for side in [-1.0, 1.0] {
            let x = side * 65.0;
            put(
                &mut details,
                harbor_box(7.0, 1.4, 13.0, 0xb9b9a7, DOCK),
                x,
                -0.65,
                43.0,
            );
            put(
                &mut details,
                box_part(1.7, 0.75, 2.6, 0xdca53b, DEFAULT_BOX_RADIUS),
                x,
                0.65,
                43.0,
            );
            for dx in [-0.9, 0.9] {
                for z in [42.2, 43.9] {
                    let wheel =
                        rotated(cylinder_part(0.43, 0.3, 0x2c383c, 12), 0.0, 0.0, FRAC_PI_2);
                    put(&mut details, wheel, x + dx, 0.45, z);
                }
                put(
                    &mut details,
                    box_part(0.12, 1.7, 0.12, 0x344952, DEFAULT_BOX_RADIUS),
                    x + dx * 0.75,
                    1.6,
                    43.0,
                );
                put(
                    &mut details,
                    box_part(0.12, 2.4, 0.14, 0x344952, DEFAULT_BOX_RADIUS),
                    x + dx * 0.55,
                    1.3,
                    41.7,
                );
                put(
                    &mut details,
                    box_part(0.14, 0.12, 1.8, 0x647174, DEFAULT_BOX_RADIUS),
                    x + dx * 0.55,
                    0.25,
                    40.9,
                );
            }
            put(
                &mut details,
                box_part(1.7, 0.16, 1.7, 0xe5b441, DEFAULT_BOX_RADIUS),
                x,
                2.5,
                43.0,
            );
            put(
                &mut details,
                box_part(0.7, 0.7, 0.6, 0x344952, DEFAULT_BOX_RADIUS),
                x,
                1.35,
                43.25,
            );
            for level in 0..3 {
                let level = f64::from(level);
                for plank in 0..5 {
                    put(
                        &mut details,
                        siding_box(2.1, 0.12, 0.27, 0xb89764),
                        x,
                        0.3 + level * 0.3,
                        46.2 + f64::from(plank) * 0.34,
                    );
                }
                for dx in [-0.8, 0.8] {
                    put(
                        &mut details,
                        siding_box(0.25, 0.18, 1.7, 0x96764e),
                        x + dx,
                        0.14 + level * 0.3,
                        46.85,
                    );
                }
            }
            // Flush grates remain traversable, with no fake solid cover in driving lanes.
            for z in [-39.0, 0.0, 39.0] {
                put(
                    &mut details,
                    box_part(1.2, 0.025, 2.2, 0x36484b, 0.0),
                    side * 58.0,
                    0.035,
                    z,
                );
                for bar in 0..9 {
                    put(
                        &mut details,
                        box_part(1.1, 0.03, 0.06, 0x838c86, 0.0),
                        side * 58.0,
                        0.05,
                        z - 0.9 + f64::from(bar) * 0.22,
                    );
                }
            }
            let rope = 0xbaa377;
            harbor_beam(
                &mut details,
                [side * 62.0, 0.4, -24.0],
                [side * 72.0, 2.3, if side < 0.0 { -27.0 } else { -4.0 }],
                0.075,
                rope,
            );
            harbor_beam(
                &mut details,
                [side * 62.0, 0.4, 24.0],
                [side * 72.0, 2.3, if side < 0.0 { 12.0 } else { 34.0 }],
                0.075,
                rope,
            );
            harbor_beam(
                &mut details,
                [side * 24.0, 0.4, -62.0],
                [side * 20.0 - 8.0, 3.0, -73.0],
                0.075,
                rope,
            );
        }
        // Distant warehouses close the horizon without creating obstacles on the board.
        for x in [-115.0, 115.0] {
            put(
                &mut details,
                harbor_box(22.0, 3.0, 78.0, 0x9aaba1, DOCK),
                x,
                -2.0,
                0.0,
            );
            put(
                &mut details,
                box_part(18.0, 8.0, 70.0, 0x526c76, DEFAULT_BOX_RADIUS),
                x,
                2.0,
                0.0,
            );
            put(
                &mut details,
                box_part(19.0, 0.5, 72.0, 0x364f5d, DEFAULT_BOX_RADIUS),
                x,
                6.2,
                0.0,
            );
            for z in (-28..=28).step_by(14) {
                let wall = if x < 0.0 { 9.1 } else { -9.1 };
                put(
                    &mut details,
                    box_part(0.08, 2.0, 5.0, 0xc7b887, DEFAULT_BOX_RADIUS),
                    x + wall,
                    3.0,
                    f64::from(z),
                );
            }
        }
        batch(&mut details);
        batch(&mut beacons);
        // Painted edge stripes sit on the actual 1.2 m perimeter wall.
        for child in &mut details.children {
            if let Some(drawable) = &mut child.drawable {
                drawable.receive_shadow = true;
            }
        }
        root.children.push(details);
        let beacon_child = root.children.len();
        root.children.push(beacons);
        let mut scenery = Self {
            root,
            beacons: beacon_child,
        };
        scenery.update(0.0);
        scenery
    }

    /// `update(time)`: ships bob, crane loads sway and the beacons blink. The water
    /// animates in its shader from the same clock.
    pub fn update(&mut self, time: f64) {
        HarborFleet::update(&mut self.root.children[FLEET_CHILD], time);
        self.root.children[self.beacons].visible = (time * 2.5).sin() > -0.3;
    }
}
