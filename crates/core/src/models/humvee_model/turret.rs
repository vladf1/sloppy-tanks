//! The gunner's protection kit on the roof ring and the TOW launcher: an armored
//! ring, angled shield plates with transparent-armor windows, the traversing
//! unit with the ITAS sight and fire-control box, and the launch tube (the
//! recoiling `barrel`). Built in the turret frame, which is not stretched.

use std::f64::consts::PI;

use glam::DVec3;

use super::LAUNCHER_Y;
use super::kit::{Kit, Role, frame, place, revolve, slab};

/// The ring sits on the roof plate.
const RING_BASE: f64 = 1.745;
const PLATE_BASE: f64 = 1.8;
const PLATE_THICKNESS: f64 = 0.035;

/// One shield plate around the ring.
struct Plate {
    /// Bearing from straight ahead (+z), radians.
    bearing: f64,
    radius: f64,
    width: f64,
    height: f64,
    /// Outward lean of the top edge, radians.
    lean: f64,
    /// Transparent-armor window: half width and bottom/top heights on the plate.
    window: Option<(f64, f64, f64)>,
}

const PLATES: [Plate; 4] = [
    Plate {
        bearing: 0.0,
        radius: 0.64,
        width: 0.44,
        height: 0.16,
        lean: 0.25,
        window: None,
    },
    Plate {
        bearing: 0.72,
        radius: 0.64,
        width: 0.52,
        height: 0.5,
        lean: 0.12,
        window: Some((0.17, 0.25, 0.43)),
    },
    Plate {
        bearing: PI / 2.0,
        radius: 0.66,
        width: 0.62,
        height: 0.42,
        lean: 0.08,
        window: Some((0.2, 0.23, 0.35)),
    },
    Plate {
        bearing: 2.36,
        radius: 0.64,
        width: 0.48,
        height: 0.26,
        lean: 0.1,
        window: None,
    },
];

/// Static turret parts: ring, shields, traversing unit, sight and fire control.
pub(super) fn turret_kit() -> Kit {
    let mut kit = Kit::default();
    let ring = [
        [0.56, RING_BASE],
        [0.66, RING_BASE],
        [0.66, 1.8],
        [0.64, 1.82],
        [0.56, 1.82],
        [0.56, RING_BASE],
    ];
    let up = place([0.0; 3], [0.0, 0.0, PI / 2.0]);
    kit.add(Role::Shade, &revolve(&ring, 24, 0.0, false), up);
    kit.cylinder(
        Role::Gap,
        0.56,
        0.004,
        24,
        place([0.0, 1.765, 0.0], [0.0; 3]),
    );
    for plate in &PLATES {
        let bearings: &[f64] = if plate.bearing == 0.0 {
            &[0.0]
        } else {
            &[plate.bearing, -plate.bearing]
        };
        for bearing in bearings {
            shield(&mut kit, plate, *bearing);
        }
    }
    launcher_mount(&mut kit);
    kit
}

fn shield(kit: &mut Kit, plate: &Plate, bearing: f64) {
    let across = DVec3::new(bearing.cos(), 0.0, -bearing.sin());
    let outward = DVec3::new(bearing.sin(), 0.0, bearing.cos());
    let up = DVec3::Y * plate.lean.cos() + outward * plate.lean.sin();
    let out = outward * plate.lean.cos() - DVec3::Y * plate.lean.sin();
    let to_plate = frame(
        across,
        up,
        out,
        outward * plate.radius + DVec3::Y * PLATE_BASE,
    );
    let (w, h, clip) = (
        plate.width / 2.0,
        plate.height,
        0.06_f64.min(plate.height * 0.3),
    );
    let outline = [
        [-w, 0.0],
        [w, 0.0],
        [w, h - clip],
        [w - clip, h],
        [-w + clip, h],
        [-w, h - clip],
    ];
    let window = plate
        .window
        .map(|(half, bottom, top)| [[-half, bottom], [half, bottom], [half, top], [-half, top]]);
    let holes: Vec<&[[f64; 2]]> = window.iter().map(|hole| hole.as_slice()).collect();
    kit.add(
        Role::Paint,
        &slab(&outline, &holes, PLATE_THICKNESS, 0.008),
        to_plate,
    );
    if let Some((half, bottom, top)) = plate.window {
        let pane = place([0.0, (bottom + top) / 2.0, PLATE_THICKNESS * 0.4], [0.0; 3]);
        kit.block(
            Role::Glass,
            [half * 2.0 + 0.02, top - bottom + 0.02, 0.012],
            to_plate * pane,
        );
    }
    // Bolted stiffener along the plate's foot.
    let foot = place([0.0, 0.03, PLATE_THICKNESS + 0.01], [0.0; 3]);
    kit.block(
        Role::Shade,
        [plate.width - 0.06, 0.04, 0.02],
        to_plate * foot,
    );
}

/// Traversing unit under the tube, the ITAS sight on its left and the
/// fire-control box on its right.
fn launcher_mount(kit: &mut Kit) {
    kit.cylinder(
        Role::Steel,
        0.05,
        0.14,
        10,
        place([0.0, 1.85, 0.32], [0.0; 3]),
    );
    kit.chamfered(
        Role::Shade,
        [0.22, 0.16, 0.3],
        0.015,
        place([0.0, 1.96, 0.32], [0.0; 3]),
    );
    // Elevation and traverse handles reach back to the gunner.
    for x in [-0.14, 0.14] {
        kit.block(
            Role::Steel,
            [0.025, 0.025, 0.24],
            place([x, 1.95, 0.08], [0.0; 3]),
        );
        kit.cylinder(Role::Gap, 0.022, 0.1, 8, place([x, 1.95, -0.04], [0.0; 3]));
    }
    let sight = place([0.27, 1.99, 0.36], [0.0; 3]);
    kit.rounded(Role::Shade, [0.24, 0.26, 0.5], 0.03, sight);
    kit.block(
        Role::Glass,
        [0.17, 0.11, 0.012],
        sight * place([0.0, 0.035, 0.252], [0.0; 3]),
    );
    let lens = sight * place([0.0, -0.075, 0.25], [PI / 2.0, 0.0, 0.0]);
    kit.cylinder(Role::Steel, 0.05, 0.02, 14, lens);
    kit.cylinder(Role::Glass, 0.038, 0.03, 14, lens);
    // Sun hood over the window.
    kit.block(
        Role::Shade,
        [0.24, 0.014, 0.09],
        sight * place([0.0, 0.112, 0.29], [-0.12, 0.0, 0.0]),
    );
    for x in [-0.115, 0.115] {
        kit.block(
            Role::Shade,
            [0.012, 0.12, 0.08],
            sight * place([x, 0.05, 0.285], [0.0; 3]),
        );
    }
    let eyepiece = sight * place([-0.03, 0.02, -0.29], [PI / 2.0, 0.0, 0.0]);
    kit.cylinder(Role::Gap, 0.038, 0.09, 12, eyepiece);
    kit.chamfered(
        Role::Shade,
        [0.16, 0.2, 0.26],
        0.012,
        place([-0.22, 1.93, 0.3], [0.0; 3]),
    );
    kit.block(
        Role::Gap,
        [0.1, 0.06, 0.01],
        place([-0.22, 1.97, 0.432], [0.0; 3]),
    );
}

/// The launch tube along +z with its collars, flared rear and open mouth.
pub(super) fn barrel_kit() -> Kit {
    let mut kit = Kit::default();
    let tube = [
        [0.0, -0.62],
        [0.12, -0.62],
        [0.14, -0.56],
        [0.14, -0.46],
        [0.118, -0.42],
        [0.118, -0.06],
        [0.128, -0.05],
        [0.128, 0.05],
        [0.118, 0.06],
        [0.118, 0.86],
        [0.128, 0.87],
        [0.128, 0.97],
        [0.122, 0.98],
        [0.122, 1.56],
        [0.132, 1.57],
        [0.132, 1.64],
        [0.104, 1.64],
        [0.104, 1.5],
    ];
    let along_z = place([0.0, LAUNCHER_Y, 0.0], [0.0, -PI / 2.0, 0.0]);
    kit.add(Role::Shade, &revolve(&tube, 16, 0.0, false), along_z);
    let mouth = place([0.0, LAUNCHER_Y, 1.505], [PI / 2.0, 0.0, 0.0]);
    kit.cylinder(Role::Gap, 0.104, 0.01, 16, mouth);
    // Carrying handle on top and the umbilical connector at the rear.
    let top = LAUNCHER_Y + 0.118;
    for z in [0.3, 0.62] {
        kit.block(
            Role::Steel,
            [0.03, 0.07, 0.03],
            place([0.0, top + 0.03, z], [0.0; 3]),
        );
    }
    kit.block(
        Role::Steel,
        [0.03, 0.025, 0.36],
        place([0.0, top + 0.07, 0.46], [0.0; 3]),
    );
    kit.chamfered(
        Role::Gap,
        [0.08, 0.06, 0.1],
        0.01,
        place([-0.12, LAUNCHER_Y - 0.02, -0.3], [0.0; 3]),
    );
    kit
}
