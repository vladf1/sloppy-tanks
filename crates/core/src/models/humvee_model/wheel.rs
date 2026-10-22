//! One road wheel: a 37-inch run-flat tire with chunky staggered lugs on a
//! two-piece beadlock rim and a protruding geared hub. Built about the local x
//! axis with the outer face towards +x; the left wheels are the same wheel turned
//! half a revolution about y, as on the real vehicle.
//!
//! Sized for the game cameras, where a wheel spans a few dozen pixels: a dozen
//! lug pairs and 16 tread facets keep the chunky outline, and the rim keeps its
//! dish and lug nuts, at about half the triangles of a showroom wheel.

use std::f64::consts::PI;

use glam::DVec3;

use super::kit::{Faces, Kit, Role, place, revolve};

/// Outer radius over the lugs. The lowest lug corner sits exactly here below the
/// axle, so the measured hull keeps its ground line.
pub(super) const TIRE_RADIUS: f64 = 0.49;
/// The hub cap's face: the outermost point of the wheel (and of the hull's width).
pub(super) const HUB_FACE: f64 = 0.166;
/// Lug pairs around the tread.
const LUG_PITCHES: u32 = 12;
/// Fraction of a pitch each lug covers.
const LUG_FILL: f64 = 0.46;
/// Lugs lean back from the centre line (radians of turn per unit of width).
const LUG_SWEEP: f64 = 0.55;
const TREAD_SEGMENTS: u32 = 16;
const RIM_SEGMENTS: u32 = 14;
const HUB_SEGMENTS: u32 = 10;
const LUG_NUTS: u32 = 6;

/// Carcass profile (radius, axial) from the outer bead over the tread to the inner
/// bead: bulging run-flat sidewalls and a flat tread base under the lugs.
const CARCASS: [[f64; 2]; 8] = [
    [0.234, -0.128],
    [0.34, -0.157],
    [0.438, -0.146],
    [0.458, -0.112],
    [0.458, 0.112],
    [0.438, 0.146],
    [0.34, 0.157],
    [0.234, 0.128],
];

/// One lug's cross-section (axial, radius) on the +x half: from the centre rib
/// over the shoulder and down onto the sidewall. Convex, so caps fan cleanly.
const LUG: [[f64; 2]; 5] = [
    [0.012, 0.452],
    [0.012, TIRE_RADIUS],
    [0.122, TIRE_RADIUS],
    [0.156, 0.452],
    [0.140, 0.424],
];

/// Rim profile (radius, axial): flange, beadlock ring, dished disc.
const RIM: [[f64; 2]; 6] = [
    [0.238, 0.06],
    [0.238, 0.14],
    [0.184, 0.146],
    [0.164, 0.110],
    [0.104, 0.110],
    [0.104, 0.095],
];

/// Geared hub and its cap, ending on the wheel's outer face.
const HUB: [[f64; 2]; 5] = [
    [0.094, 0.100],
    [0.094, 0.140],
    [0.060, 0.150],
    [0.050, HUB_FACE],
    [0.0, HUB_FACE],
];

/// Tire (rubber), rim (shade) and hub with bolts (steel) of one wheel.
pub(super) fn wheel_kit() -> Kit {
    let mut kit = Kit::default();
    kit.add(
        Role::Rubber,
        &revolve(&CARCASS, TREAD_SEGMENTS, true),
        glam::DMat4::IDENTITY,
    );
    lugs(&mut kit);
    kit.add(
        Role::Shade,
        &revolve(&RIM, RIM_SEGMENTS, false),
        glam::DMat4::IDENTITY,
    );
    kit.add(
        Role::Steel,
        &revolve(&HUB, HUB_SEGMENTS, false),
        glam::DMat4::IDENTITY,
    );
    // Hex lug nuts stand on the disc.
    let (radius, face, size) = (0.132, 0.110, 0.015);
    for i in 0..LUG_NUTS {
        let angle = (f64::from(i) + 0.5) * 2.0 * PI / f64::from(LUG_NUTS);
        let at = [face, radius * angle.cos(), radius * angle.sin()];
        kit.bolt(
            Role::Steel,
            size,
            size * 1.2,
            place(at, [0.0, 0.0, -PI / 2.0]),
        );
    }
    kit
}

/// Staggered lugs on both halves of the tread. One lug corner lies exactly at
/// the bottom of the wheel so the tire touches the ground line at `TIRE_RADIUS`.
fn lugs(kit: &mut Kit) {
    let pitch = 2.0 * PI / f64::from(LUG_PITCHES);
    let span = pitch * LUG_FILL;
    let mut faces = Faces::default();
    for i in 0..LUG_PITCHES {
        for half in [1.0, -1.0] {
            let stagger = if half > 0.0 { 0.0 } else { pitch / 2.0 };
            let start = PI - LUG_SWEEP * LUG[1][0] + f64::from(i) * pitch + stagger;
            let corner = |[axial, radius]: [f64; 2], angle: f64| {
                let angle = angle + LUG_SWEEP * axial;
                DVec3::new(half * axial, radius * angle.cos(), radius * angle.sin())
            };
            let lo: Vec<DVec3> = LUG.iter().map(|p| corner(*p, start)).collect();
            let hi: Vec<DVec3> = LUG.iter().map(|p| corner(*p, start + span)).collect();
            let center = lo.iter().chain(&hi).sum::<DVec3>() / (2 * LUG.len()) as f64;
            let lo_center = lo.iter().sum::<DVec3>() / LUG.len() as f64;
            let hi_center = hi.iter().sum::<DVec3>() / LUG.len() as f64;
            faces.polygon(&lo, lo_center - center);
            faces.polygon(&hi, hi_center - center);
            // The last edge (back to the centre rib) is buried in the carcass.
            for j in 0..LUG.len() - 1 {
                let quad = [lo[j], lo[j + 1], hi[j + 1], hi[j]];
                let mid = quad.iter().sum::<DVec3>() / 4.0;
                faces.polygon(&quad, mid - center);
            }
        }
    }
    kit.add(Role::Rubber, &faces.mesh(), glam::DMat4::IDENTITY);
}
