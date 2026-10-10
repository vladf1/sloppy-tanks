//! One wheel: a 35-inch all-terrain tire with staggered, swept tread blocks on a
//! dark rim behind the faceted aero cover, built about the local x axis with the
//! outer face towards +x. The left wheels are the same wheel turned half a
//! revolution about y.

use std::f64::consts::PI;

use glam::{DMat4, DVec3};

use super::super::parts_kit::{Faces, Kit, Role, revolve};
use super::CYBERTRUCK_WHEEL_RADIUS;

/// Tread block pairs around the tire, the share of a pitch each block covers and
/// how far the blocks sweep back from the centre line (radians per unit of width).
const LUG_PITCHES: u32 = 14;
const LUG_FILL: f64 = 0.5;
const LUG_SWEEP: f64 = 0.6;
const TREAD_SEGMENTS: u32 = 24;
const RIM_SEGMENTS: u32 = 18;
/// The aero cover: sides of its hexagons, the outer one's corner radius and the
/// inner one's.
const COVER_SIDES: u32 = 6;
const COVER_RADIUS: f64 = 0.255;
const COVER_INNER: f64 = 0.135;

/// Carcass profile (radius, axial) from the outer bead over the tread to the inner
/// bead: tall sidewalls and a flat tread base under the blocks.
const CARCASS: [[f64; 2]; 10] = [
    [0.262, -0.13],
    [0.33, -0.15],
    [0.405, -0.152],
    [0.43, -0.134],
    [0.436, -0.104],
    [0.436, 0.104],
    [0.43, 0.134],
    [0.405, 0.152],
    [0.33, 0.15],
    [0.262, 0.13],
];

/// One tread block's cross-section (axial, radius) on the +x half, from the centre
/// rib over the shoulder and down onto the sidewall. Convex, so caps fan cleanly.
const LUG: [[f64; 2]; 6] = [
    [0.012, 0.43],
    [0.012, CYBERTRUCK_WHEEL_RADIUS],
    [0.112, CYBERTRUCK_WHEEL_RADIUS],
    [0.138, CYBERTRUCK_WHEEL_RADIUS - 0.024],
    [0.15, 0.414],
    [0.13, 0.41],
];

/// Rim profile (radius, axial): the flange and the dished barrel behind the cover.
const RIM: [[f64; 2]; 6] = [
    [0.265, 0.07],
    [0.265, 0.128],
    [0.255, 0.14],
    [0.236, 0.14],
    [0.226, 0.12],
    [0.12, 0.1],
];

/// Tire (rubber), rim (cladding) and aero cover with its cap (steel).
pub(super) fn wheel_kit() -> Kit {
    let mut kit = Kit::default();
    kit.add(
        Role::Rubber,
        &revolve(&CARCASS, TREAD_SEGMENTS, true),
        DMat4::IDENTITY,
    );
    lugs(&mut kit);
    kit.add(
        Role::Shade,
        &revolve(&RIM, RIM_SEGMENTS, false),
        DMat4::IDENTITY,
    );
    aero_cover(&mut kit);
    kit
}

/// Staggered blocks on both halves of the tread. One block corner lies exactly at
/// the bottom of the wheel so the tire touches the ground line at its radius.
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
    kit.add(Role::Rubber, &faces.mesh(), DMat4::IDENTITY);
}

/// The aero cover: a hexagon of triangular facets over the rim, an outer ring of
/// triangles between two hexagons turned half a step, and a low hexagonal cap, so
/// each facet catches the light on its own as on the production cover.
fn aero_cover(kit: &mut Kit) {
    let mut faces = Faces::default();
    let point = |radius: f64, angle: f64, axial: f64| {
        DVec3::new(axial, radius * angle.cos(), radius * angle.sin())
    };
    let step = 2.0 * PI / f64::from(COVER_SIDES);
    let outer = |i: u32| point(COVER_RADIUS, f64::from(i) * step, 0.138);
    let inner = |i: u32| point(COVER_INNER, (f64::from(i) + 0.5) * step, 0.154);
    let hub = DVec3::new(0.162, 0.0, 0.0);
    for i in 0..COVER_SIDES {
        faces.triangle(outer(i), outer(i + 1), inner(i), DVec3::X);
        faces.triangle(inner(i), outer(i + 1), inner(i + 1), DVec3::X);
        faces.triangle(inner(i), inner(i + 1), hub, DVec3::X);
    }
    kit.add(Role::Steel, &faces.mesh(), DMat4::IDENTITY);
}
