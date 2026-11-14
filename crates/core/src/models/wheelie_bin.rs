//! The Neighborhood's wheelie bins (an explosive drum's look there): a moulded plastic
//! cart with a hinged lid, a handle and two wheels at its back, out at the kerb for
//! collection. Built with its front (the lid's lip) toward +z, from the ground up.

use std::f64::consts::FRAC_PI_2;

use super::model_primitives::{box_part, cylinder_part, put, rotated};
use crate::scene::Node;
use crate::sim::senda_layout::TRASH_BIN;

/// The handle, hinge, wheels' tyres and the feet: dark moulded plastic and rubber.
const BIN_DARK: u32 = 0x22251f;
const BIN_TYRE: u32 = 0x161616;
const BIN_HUB: u32 = 0x8a8c88;
/// The pale label moulded on the front.
const BIN_LABEL: u32 = 0xd8d6cc;
/// The wheels' radius and how far up the body stands off the ground on them (m).
const WHEEL_RADIUS: f64 = 0.11;
const BODY_LIFT: f64 = 0.07;
const LID_THICKNESS: f64 = 0.05;

/// A real bin's size across, deep and tall (m), which the model is built at.
const FULL_SIZE: (f64, f64, f64) = (0.66, 0.74, 1.08);

/// A wheelie bin in `color`, `TRASH_BIN` across, deep and tall: the full-size cart
/// brought down to the Neighborhood's scale.
pub fn wheelie_bin(color: u32) -> Node {
    let (w, d, h) = FULL_SIZE;
    let mut bin = Node::group("wheelie-bin");
    // The tub, a little inside the footprint so the lid overhangs it.
    let (body_w, body_d) = (w - 0.06, d - 0.08);
    let body_h = h - BODY_LIFT - LID_THICKNESS - 0.02;
    put(
        &mut bin,
        box_part(body_w, body_h, body_d, color, 0.04),
        0.0,
        BODY_LIFT + body_h / 2.0,
        0.0,
    );
    // The thickened rim the lid closes onto, and the lid with its lip over the front.
    let rim = BODY_LIFT + body_h;
    put(&mut bin, box_part(w - 0.02, 0.04, body_d + 0.03, color, 0.01), 0.0, rim, 0.0);
    put(
        &mut bin,
        box_part(w, LID_THICKNESS, d, color, 0.02),
        0.0,
        rim + 0.02 + LID_THICKNESS / 2.0,
        0.02,
    );
    put(&mut bin, box_part(0.3, 0.05, 0.04, color, 0.01), 0.0, rim, d / 2.0 + 0.01);
    // Ribs moulded down each side, and the label on the front.
    for side in [-1.0, 1.0] {
        for z in [-0.14, 0.14] {
            put(
                &mut bin,
                box_part(0.03, body_h * 0.8, 0.045, color, 0.0),
                side * (body_w / 2.0 + 0.01),
                BODY_LIFT + body_h * 0.5,
                z,
            );
        }
    }
    put(
        &mut bin,
        box_part(0.24, 0.14, 0.012, BIN_LABEL, 0.0),
        0.0,
        BODY_LIFT + body_h * 0.68,
        body_d / 2.0 + 0.006,
    );
    // The handle across the back under the hinge, on two brackets.
    let back = -body_d / 2.0;
    put(
        &mut bin,
        rotated(cylinder_part(0.025, w - 0.08, BIN_DARK, 10), 0.0, 0.0, FRAC_PI_2),
        0.0,
        rim - 0.03,
        back - 0.08,
    );
    for side in [-1.0, 1.0] {
        put(
            &mut bin,
            box_part(0.04, 0.05, 0.1, BIN_DARK, 0.0),
            side * (w / 2.0 - 0.08),
            rim - 0.03,
            back - 0.04,
        );
    }
    // Two wheels on an axle under the back, and the moulded feet at the front.
    let axle_z = back - 0.02;
    put(
        &mut bin,
        rotated(cylinder_part(0.016, w + 0.04, BIN_HUB, 8), 0.0, 0.0, FRAC_PI_2),
        0.0,
        WHEEL_RADIUS,
        axle_z,
    );
    for side in [-1.0, 1.0] {
        let x = side * (w / 2.0 - 0.01);
        put(
            &mut bin,
            rotated(cylinder_part(WHEEL_RADIUS, 0.06, BIN_TYRE, 16), 0.0, 0.0, FRAC_PI_2),
            x,
            WHEEL_RADIUS,
            axle_z,
        );
        put(
            &mut bin,
            rotated(cylinder_part(0.05, 0.065, BIN_HUB, 10), 0.0, 0.0, FRAC_PI_2),
            x,
            WHEEL_RADIUS,
            axle_z,
        );
    }
    put(
        &mut bin,
        box_part(body_w - 0.1, BODY_LIFT, 0.08, BIN_DARK, 0.0),
        0.0,
        BODY_LIFT / 2.0,
        body_d / 2.0 - 0.08,
    );
    bin.scale = glam::DVec3::new(TRASH_BIN.0 / w, TRASH_BIN.2 / h, TRASH_BIN.1 / d);
    bin
}
