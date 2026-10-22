//! Armor bodies and equipment of the three tracked tanks, merged per finish with
//! [`Kit`]: the hull (everything but the running gear), the turret with its
//! mantlet and roof equipment, and the gun.
//!
//! References: the scout is an M10 Booker (compact welded turret, bolted side
//! armor, multi-baffle muzzle brake), the balanced tank an M1 Abrams (shallow
//! glacis, flat-faced wedge turret with a long bustle rack) and the heavy a Type 99
//! (arrowhead add-on wedges, laser dazzler, spare track on the glacis).
//!
//! Frame: x left, y up, z forward, in model metres before the vehicle scale.
//! The hull's mesh bounds are its hit box (see `tank_dimensions`), so hull parts
//! stay below [`HULL_TOP_RISE`] above the deck and within [`END_REACH`] of the end
//! plates, inside the box the track ends already define.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::{DMat4, DVec2, DVec3};

use super::model_primitives::Cache;
use super::tank_kit::{
    Coat, Kit, KitMeshes, aim, chamfered_levels, inset, pose, prism_x, shift, sloped_ring,
};
use super::tank_model::Chassis;
use super::{Team, VehicleKind, part};

/// Hull roof height above the deck line.
pub(super) const ROOF_RISE: f64 = 0.045;
/// The highest hull point above the deck line (the turret-ring guard's top): the
/// hull hit box height. Roof fittings stay at or below it.
pub(super) const HULL_TOP_RISE: f64 = 0.06;
/// How far a fitting may stand proud of the hull's front or rear plate; the hull
/// hit box ends about 2.6 cm beyond them, at the track ends.
const END_REACH: f64 = 0.022;
/// Height of the lower hull's belly plate.
const BELLY_Y: f64 = 0.07;
/// Sides of the gun tube's turned sections.
const GUN_SIDES: u32 = 10;
/// The crew livery's variant number (see [`part::CREW_LIVERY`]).
const CREW_LIVERY: u8 = part::CREW_LIVERY as u8;
/// Turret-ring centre behind the hull centre, shared with the turret ring.
pub(super) const RING_Z: f64 = -0.12;
/// The Big Rig's turret, wedges and turret stowage are this much wider than their
/// Type 99 proportions, so its turret mass reads heavier than the Bruiser's from
/// any camera (the turret is not part of the hull's hit box).
const HEAVY_TURRET_WIDTH: f64 = 1.2;
/// The Skipper's turret is this much narrower than its Booker proportions: a
/// compact, low turret on the shortest hull, so the scout reads light and quick
/// beside the other two.
const SCOUT_TURRET_WIDTH: f64 = 0.88;
/// Where the Skipper's bolted side armor modules hang, out from the centre line.
const SCOUT_MODULE_X: f64 = 0.865 * SCOUT_TURRET_WIDTH;

/// The merged assemblies of a chassis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Assembly {
    Hull,
    /// The hull of a wreck whose turret flew off, with the ring cut open.
    OpenHull,
    Turret,
    Barrel,
    /// Team insignia, unit numbers and a tank's own extra stowage: one of the
    /// turret's alternative liveries.
    Marking(Team, u8),
}

static KITS: Cache<(VehicleKind, Assembly), KitMeshes> = Cache::new();

/// The merged meshes of one assembly, built once per chassis and shared by both
/// teams and every round.
pub(super) fn assembly(c: &Chassis, which: Assembly) -> Arc<KitMeshes> {
    KITS.get_or_insert((c.kind, which), || {
        let mut kit = Kit::new();
        match which {
            Assembly::Hull => hull(&mut kit, c, false),
            Assembly::OpenHull => hull(&mut kit, c, true),
            Assembly::Turret => turret(&mut kit, c),
            Assembly::Barrel => gun(&mut kit, c),
            Assembly::Marking(team, variant) => marking(&mut kit, c, team, variant),
        }
        kit.finish()
    })
}

/// Turret-ring radius (the turret's bearing ring) and the open well's radius.
pub(super) fn ring_radius(c: &Chassis) -> f64 {
    if c.scout { 0.59 } else { 0.76 }
}

/// Height of the turret roof above the deck: the Skipper's turret crouches low
/// over its gun, the Big Rig's stands tall behind its wedges, so the three
/// classes part by silhouette as well as size.
fn roof_rise(c: &Chassis) -> f64 {
    c.pick(0.42, 0.56, 0.7)
}

/// Height of the gun axis above the deck.
pub(super) fn gun_rise(c: &Chassis) -> f64 {
    if c.scout { 0.27 } else { 0.31 }
}

fn v(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

fn p2(points: &[(f64, f64)]) -> Vec<DVec2> {
    points.iter().map(|&(a, b)| DVec2::new(a, b)).collect()
}

/// A point `offset` (in the yawed frame) from `origin`.
fn yawed(origin: DVec3, yaw: f64, offset: DVec3) -> DVec3 {
    origin + DMat4::from_rotation_y(yaw).transform_vector3(offset)
}

// ---------------------------------------------------------------------------
// Hull

/// Side profile of a hull: glacis, nose and rear plate in (z, y).
struct HullShape {
    /// Height of the split between the full-width upper hull and the narrower
    /// lower hull between the tracks.
    split: f64,
    /// Top of the short vertical nose under the glacis.
    nose_top: f64,
    /// Horizontal run of the upper glacis from the nose to the roof edge.
    glacis_run: f64,
    /// Horizontal run of the lower front plate down to the belly.
    lower_run: f64,
    /// The rear plate's bevelled top edge: run along the roof and drop.
    rear_cut: f64,
    rear_drop: f64,
    /// How far the rear plate leans in at the split.
    rear_lean: f64,
    /// Horizontal run of the lower rear plate up from the belly.
    belly_rear_run: f64,
}

fn hull_shape(c: &Chassis) -> HullShape {
    if c.scout {
        HullShape {
            split: 0.38,
            nose_top: 0.42,
            glacis_run: 0.74,
            lower_run: 0.36,
            rear_cut: 0.05,
            rear_drop: 0.06,
            rear_lean: 0.02,
            belly_rear_run: 0.22,
        }
    } else if c.heavy {
        HullShape {
            split: 0.38,
            nose_top: 0.42,
            glacis_run: 0.88,
            lower_run: 0.32,
            rear_cut: 0.07,
            rear_drop: 0.1,
            rear_lean: 0.04,
            belly_rear_run: 0.26,
        }
    } else {
        HullShape {
            split: 0.4,
            nose_top: 0.45,
            glacis_run: 1.0,
            lower_run: 0.42,
            rear_cut: 0.05,
            rear_drop: 0.06,
            rear_lean: 0.02,
            belly_rear_run: 0.24,
        }
    }
}

/// Measurements derived from a chassis and its hull shape.
struct HullFrame {
    half: f64,
    roof: f64,
    top: f64,
    /// Half width of the upper hull (over the tracks) and of the lower hull.
    upper_half_width: f64,
    lower_half_width: f64,
    /// z of the glacis' top edge and the glacis slope angle.
    glacis_z: f64,
    glacis_angle: f64,
    shape: HullShape,
}

impl HullFrame {
    fn new(c: &Chassis) -> Self {
        let shape = hull_shape(c);
        let half = c.length / 2.0;
        let roof = c.deck + ROOF_RISE;
        let glacis_angle = ((roof - shape.nose_top) / shape.glacis_run).atan();
        Self {
            half,
            roof,
            top: c.deck + HULL_TOP_RISE,
            upper_half_width: c.width / 2.0,
            // Just inside the tracks' inner faces.
            lower_half_width: c.overall_width / 2.0 - 0.45,
            glacis_z: half - shape.glacis_run,
            glacis_angle,
            shape,
        }
    }

    /// The glacis surface `s` metres down its slope from the roof edge.
    fn glacis(&self, s: f64) -> (f64, f64) {
        (
            self.glacis_z + s * self.glacis_angle.cos(),
            self.roof - s * self.glacis_angle.sin(),
        )
    }

    /// Placement on the glacis: `s` down the slope, `x` across, `lift` off the
    /// plate along its normal, with local y along the plate normal.
    fn on_glacis(&self, x: f64, s: f64, lift: f64, yaw: f64) -> DMat4 {
        let (z, y) = self.glacis(s);
        let normal = v(0.0, self.glacis_angle.cos(), self.glacis_angle.sin());
        pose(v(x, y, z) + normal * lift, v(self.glacis_angle, 0.0, 0.0))
            * DMat4::from_rotation_y(yaw)
    }

    /// z of the rear plate's outer face at height `y` (between split and roof).
    fn rear_face(&self, y: f64) -> f64 {
        let s = &self.shape;
        let (y0, z0) = (s.split, -self.half + s.rear_lean);
        let (y1, z1) = (self.roof - s.rear_drop, -self.half);
        z0 + (z1 - z0) * (y - y0) / (y1 - y0)
    }
}

fn hull(kit: &mut Kit, c: &Chassis, open_ring: bool) {
    let f = HullFrame::new(c);
    let s = &f.shape;
    let half = f.half;
    // Full-width upper hull: nose, glacis, roof and rear plate over the tracks.
    let upper = p2(&[
        (half, s.split),
        (half, s.nose_top),
        (f.glacis_z, f.roof),
        (-half + s.rear_cut, f.roof),
        (-half, f.roof - s.rear_drop),
        (-half + s.rear_lean, s.split),
    ]);
    // Lower hull between the tracks: lower front plate, belly and lower rear plate.
    let lower = p2(&[
        (half, s.split),
        (half - s.lower_run, BELLY_Y),
        (-half + s.belly_rear_run, BELLY_Y),
        (-half + s.rear_lean, s.split),
    ]);
    kit.solid(Coat::Shade, &prism_x(&lower, f.lower_half_width, 0.02));
    let bevel = 0.03;
    let upper_rings = prism_x(&upper, f.upper_half_width, bevel);
    if open_ring {
        // The roof is edge 2 of the profile, in the middle band of the prism.
        kit.solid_skipping(Coat::Paint, &upper_rings, Some((1, 2)));
        let x = f.upper_half_width - bevel;
        kit.plate_with_hole(
            Coat::Paint,
            [
                DVec2::new(-x, -half + s.rear_cut),
                DVec2::new(x, f.glacis_z),
            ],
            f.roof,
            DVec2::new(0.0, RING_Z),
            ring_radius(c),
            24,
        );
    } else {
        kit.solid(Coat::Paint, &upper_rings);
    }
    belly(kit, c);
    ring_guard(kit, c, &f);
    driver_station(kit, c, &f);
    engine_deck(kit, c, &f);
    deck_tarp(kit, c, &f);
    rear_plate(kit, c, &f);
    front_fittings(kit, c, &f);
    glacis_details(kit, c, &f);
}

/// Armored belly and bolted service covers, seen on overturned wrecks.
fn belly(kit: &mut Kit, c: &Chassis) {
    let (width, length) = (c.width, c.length);
    kit.chamfer_block(
        Coat::Shade,
        v(width * 0.7, 0.1, length * 0.83),
        0.03,
        shift(v(0.0, 0.035, 0.0)),
    );
    for z in [-length * 0.25, length * 0.18] {
        let cover_width = width * if z < 0.0 { 0.43 } else { 0.3 };
        let cover_length = length * if z < 0.0 { 0.2 } else { 0.15 };
        kit.block(
            Coat::Dark,
            v(cover_width + 0.045, 0.025, cover_length + 0.045),
            shift(v(0.0, -0.025, z)),
        );
        kit.block(
            Coat::Shade,
            v(cover_width, 0.035, cover_length),
            shift(v(0.0, -0.045, z)),
        );
        for side in [-1.0, 1.0] {
            for end in [-1.0, 1.0] {
                kit.block(
                    Coat::Steel,
                    v(0.04, 0.018, 0.04),
                    shift(v(
                        side * (cover_width / 2.0 - 0.055),
                        -0.068,
                        z + end * (cover_length / 2.0 - 0.055),
                    )),
                );
            }
        }
    }
}

/// A raised splash guard around the turret ring; its top is the hull's top.
fn ring_guard(kit: &mut Kit, c: &Chassis, f: &HullFrame) {
    let inner = ring_radius(c) + 0.012;
    let outer = inner + 0.06;
    let base = f.roof - 0.01;
    let height = f.top - base;
    // Counter-clockwise in (radius, height): outer wall, top, inner wall.
    let profile = p2(&[(outer, 0.0), (outer, height), (inner, height), (inner, 0.0)]);
    kit.turned(
        Coat::Shade,
        &profile,
        16,
        pose(v(0.0, base, RING_Z), v(-PI / 2.0, 0.0, 0.0)),
    );
}

/// A low round painted hatch lid lying on a surface, local y up.
fn hatch_lid(kit: &mut Kit, placement: DMat4, radius: f64, thickness: f64) {
    let profile = p2(&[
        (radius, 0.0),
        (radius, thickness * 0.6),
        (radius - 0.02, thickness),
        (0.0, thickness),
    ]);
    kit.turned(
        Coat::Paint,
        &profile,
        10,
        placement * DMat4::from_rotation_x(-PI / 2.0),
    );
    // Hinge at the back, grab handle at the front.
    kit.block(
        Coat::Steel,
        v(radius * 0.7, thickness * 0.7, 0.035),
        placement * DMat4::from_translation(v(0.0, thickness * 0.4, -radius - 0.005)),
    );
    kit.block(
        Coat::Steel,
        v(0.07, 0.008, 0.014),
        placement * DMat4::from_translation(v(0.0, thickness + 0.001, radius * 0.55)),
    );
}

/// Driver's hatch behind the glacis and the periscopes in front of it.
fn driver_station(kit: &mut Kit, c: &Chassis, f: &HullFrame) {
    let hatch_z = f.glacis_z - if c.heavy { 0.2 } else { 0.22 };
    let radius = if c.scout { 0.17 } else { 0.19 };
    // Lid top stays below the hull top.
    hatch_lid(
        kit,
        shift(v(0.0, f.roof - 0.004, hatch_z)),
        radius,
        HULL_TOP_RISE - ROOF_RISE - 0.002,
    );
    let periscopes: &[(f64, f64)] = if c.heavy {
        &[(0.0, 0.0)]
    } else {
        &[(-0.13, 0.25), (0.0, 0.0), (0.13, -0.25)]
    };
    for &(x, yaw) in periscopes {
        // Periscope heads sit where the glacis meets the roof, tops level with
        // the hull top.
        let s = 0.12;
        let (z, y) = f.glacis(s);
        let bottom = y - 0.012;
        let height = f.top - 0.002 - bottom;
        let width = if c.heavy { 0.2 } else { 0.1 };
        let at = v(x, bottom + height / 2.0, z - 0.035);
        kit.block_at(Coat::Gunmetal, v(width, height, 0.06), at, v(0.0, yaw, 0.0));
        kit.block_at(
            Coat::Glass,
            v(width - 0.02, height * 0.45, 0.006),
            yawed(at, yaw, v(0.0, height * 0.12, 0.031)),
            v(0.0, yaw, 0.0),
        );
    }
}

/// An engine-deck grille: a painted frame, a dark recess and tilted louvres,
/// all lying within the thin layer between roof and hull top. Only upward faces
/// are drawn; their edges are a few millimetres tall.
fn grille(kit: &mut Kit, f: &HullFrame, center: DVec2, size: DVec2, slats: u32) {
    let flat = |kit: &mut Kit, coat: Coat, half: DVec2, y: f64| {
        let (lo, hi) = (center - half, center + half);
        kit.quad(
            coat,
            [
                v(lo.x, y, lo.y),
                v(hi.x, y, lo.y),
                v(hi.x, y, hi.y),
                v(lo.x, y, hi.y),
            ],
            DVec3::Y,
        );
    };
    flat(kit, Coat::Gunmetal, size / 2.0, f.roof + 0.003);
    flat(kit, Coat::Dark, size / 2.0 - 0.025, f.roof + 0.005);
    let pitch = (size.y - 0.08) / f64::from(slats);
    let half_x = size.x / 2.0 - 0.035;
    for i in 0..slats {
        // Each louvre slopes down toward the rear, over a dark gap.
        let z0 = center.y - size.y / 2.0 + 0.04 + f64::from(i) * pitch;
        let (front, back) = (z0 + pitch * 0.15, z0 + pitch * 0.85);
        let (low, high) = (f.roof + 0.006, f.top - 0.002);
        kit.quad(
            Coat::Gunmetal,
            [
                v(center.x - half_x, high, front),
                v(center.x + half_x, high, front),
                v(center.x + half_x, low, back),
                v(center.x - half_x, low, back),
            ],
            v(0.0, back - front, high - low),
        );
    }
}

/// A weld bead's width and how far it stands proud of its plate.
const BEAD_WIDTH: f64 = 0.016;
const BEAD_PROUD: f64 = 0.006;

/// A raised weld bead in the team paint along a plate joint, from `from` to `to`
/// on a face whose outward normal is `normal`: the paint shader's bevels light its
/// edges, so the joint reads as relief rather than as a line drawn on.
fn weld_bead(kit: &mut Kit, from: DVec3, to: DVec3, normal: DVec3) {
    let along = (to - from).normalize();
    let up = normal.normalize();
    let across = up.cross(along).normalize();
    let up = along.cross(across);
    let centre = (from + to) / 2.0 + up * (BEAD_PROUD / 2.0 - 0.001);
    let placement = DMat4::from_cols(
        across.extend(0.0),
        up.extend(0.0),
        along.extend(0.0),
        centre.extend(1.0),
    );
    kit.block(
        Coat::Paint,
        v(BEAD_WIDTH, BEAD_PROUD + 0.002, from.distance(to)),
        placement,
    );
}

/// A weld seam lying on a flat roof at height `y`, from and to plan points (x, z).
fn seam(kit: &mut Kit, y: f64, from: DVec2, to: DVec2) {
    weld_bead(kit, v(from.x, y, from.y), v(to.x, y, to.y), DVec3::Y);
}

/// Side of a square bolt head and how far it stands proud.
const BOLT_SIZE: f64 = 0.024;
const BOLT_HEIGHT: f64 = 0.01;

/// A row of `count` painted bolt heads from `from` to `to` on a face whose
/// outward normal is `normal`.
fn bolt_row(kit: &mut Kit, from: DVec3, to: DVec3, count: u32, normal: DVec3) {
    for i in 0..count {
        let t = if count > 1 {
            f64::from(i) / f64::from(count - 1)
        } else {
            0.5
        };
        let at = from.lerp(to, t);
        kit.block(
            Coat::Paint,
            v(BOLT_SIZE, BOLT_SIZE, BOLT_HEIGHT * 2.0),
            aim(at, at + normal),
        );
    }
}

/// A welded plate joint on a flat face with outward `normal`, from `from` to `to`.
fn panel_line(kit: &mut Kit, from: DVec3, to: DVec3, normal: DVec3) {
    weld_bead(kit, from, to, normal);
}

/// Welded and bolted glacis details: a bolt row along its top edge, the plate
/// joints running down it and bolts along the nose.
fn glacis_details(kit: &mut Kit, c: &Chassis, f: &HullFrame) {
    let normal = v(0.0, f.glacis_angle.cos(), f.glacis_angle.sin());
    let w = f.upper_half_width;
    let length = f.shape.glacis_run / f.glacis_angle.cos();
    let at = |x: f64, s: f64| {
        let (z, y) = f.glacis(s);
        v(x, y, z)
    };
    let count = if c.scout { 9 } else { 11 };
    bolt_row(kit, at(-w + 0.2, 0.05), at(w - 0.2, 0.05), count, normal);
    for x in [-w * 0.42, w * 0.42] {
        panel_line(kit, at(x, 0.1), at(x, length - 0.06), normal);
    }
    panel_line(
        kit,
        at(-w + 0.12, length * 0.55),
        at(-w * 0.42, length * 0.55),
        normal,
    );
    panel_line(
        kit,
        at(w * 0.42, length * 0.55),
        at(w - 0.12, length * 0.55),
        normal,
    );
}

/// Bolts round the edge of a turret roof outline at `roof`, inset from its rim.
fn roof_bolts(kit: &mut Kit, outline: &[DVec2], roof: f64, inset_by: f64, pitch: f64) {
    let inner = inset(outline, inset_by);
    let n = inner.len();
    for i in 0..n {
        let (a, b) = (inner[i], inner[(i + 1) % n]);
        let length = a.distance(b);
        let count = (length / pitch).floor() as u32;
        if count < 1 {
            continue;
        }
        // Leave the corner itself to the neighbouring edge.
        let step = (b - a) / f64::from(count);
        let from = a + step * 0.5;
        let to = b - step * 0.5;
        bolt_row(
            kit,
            v(from.x, roof, from.y),
            v(to.x, roof, to.y),
            count,
            DVec3::Y,
        );
    }
}

/// Bolts round the gun on the mantlet face.
fn mantlet_bolts(kit: &mut Kit, gun_y: f64, face_z: f64, size: DVec3) {
    let (x, y) = (size.x / 2.0 - 0.045, size.y / 2.0 - 0.045);
    for (bx, by) in [(-x, y), (x, y), (-x, -y), (x, -y), (-x, 0.0), (x, 0.0)] {
        let at = v(bx, gun_y + by, face_z);
        kit.block(
            Coat::Steel,
            v(0.03, 0.03, 0.02),
            shift(at + v(0.0, 0.0, 0.005)),
        );
    }
}

fn engine_deck(kit: &mut Kit, c: &Chassis, f: &HullFrame) {
    let half = f.half;
    let w = f.upper_half_width;
    if c.heavy {
        // T-72 lineage: a transverse grille across the rear, two louvred doors
        // ahead of it and the access-door seams.
        grille(
            kit,
            f,
            DVec2::new(0.0, -half + 0.3),
            DVec2::new(1.5, 0.36),
            4,
        );
        for side in [-1.0, 1.0] {
            grille(
                kit,
                f,
                DVec2::new(side * 0.46, -half + 0.95),
                DVec2::new(0.62, 0.72),
                7,
            );
        }
        seam(
            kit,
            f.roof,
            DVec2::new(-w + 0.08, -half + 1.42),
            DVec2::new(w - 0.08, -half + 1.42),
        );
    } else if c.scout {
        for side in [-1.0, 1.0] {
            grille(
                kit,
                f,
                DVec2::new(side * 0.36, -half + 0.6),
                DVec2::new(0.5, 0.8),
                7,
            );
        }
        grille(
            kit,
            f,
            DVec2::new(0.0, -half + 0.16),
            DVec2::new(1.3, 0.16),
            2,
        );
        seam(
            kit,
            f.roof,
            DVec2::new(-w + 0.08, -half + 1.12),
            DVec2::new(w - 0.08, -half + 1.12),
        );
    } else {
        // Abrams: two large grille doors over the turbine and the deck seams.
        for side in [-1.0, 1.0] {
            grille(
                kit,
                f,
                DVec2::new(side * 0.43, -half + 0.62),
                DVec2::new(0.66, 0.92),
                8,
            );
        }
        seam(
            kit,
            f.roof,
            DVec2::new(-w + 0.08, -half + 1.22),
            DVec2::new(w - 0.08, -half + 1.22),
        );
        seam(
            kit,
            f.roof,
            DVec2::new(0.0, -half + 1.22),
            DVec2::new(0.0, -half + 1.6),
        );
    }
}

/// An olive canvas tarp tied over the rear of the engine deck, slightly askew: a
/// folded sheet lying on the grilles at the hull's top height whose edges drape
/// down to the roof. From overhead it is the hull's second hue beside the team
/// paint.
fn deck_tarp(kit: &mut Kit, c: &Chassis, f: &HullFrame) {
    let size = DVec2::new(c.pick(0.92, 1.1, 1.16), c.pick(0.4, 0.5, 0.5));
    let center = DVec2::new(c.pick(-0.1, -0.12, 0.1), -f.half + c.pick(0.36, 0.42, 0.4));
    let yaw = c.pick(0.05, -0.06, 0.04);
    let (sin, cos) = yaw.sin_cos();
    let rotate = |p: DVec2| DVec2::new(p.x * cos - p.y * sin, p.x * sin + p.y * cos);
    let drape = 0.025;
    let top = f.top - 0.001;
    let low = f.roof + 0.004;
    let corner = |sx: f64, sz: f64, grow: f64, y: f64| {
        let p = center
            + rotate(DVec2::new(
                sx * (size.x / 2.0 + grow),
                sz * (size.y / 2.0 + grow),
            ));
        v(p.x, y, p.y)
    };
    let signs = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    kit.quad(
        Coat::Canvas,
        signs.map(|(sx, sz)| corner(sx, sz, 0.0, top)),
        DVec3::Y,
    );
    for i in 0..4 {
        let (a, b) = (signs[i], signs[(i + 1) % 4]);
        let mid = DVec2::new(a.0 + b.0, a.1 + b.1) / 2.0;
        let out = rotate(mid);
        kit.quad(
            Coat::Canvas,
            [
                corner(a.0, a.1, 0.0, top),
                corner(a.0, a.1, drape, low),
                corner(b.0, b.1, drape, low),
                corner(b.0, b.1, 0.0, top),
            ],
            v(out.x, 0.6, out.y),
        );
    }
    // Tie-down cords at the corners.
    for (sx, sz) in signs {
        let at = corner(sx, sz, drape * 0.5, (top + low) / 2.0);
        kit.block(Coat::Dark, v(0.03, top - low, 0.03), shift(at));
    }
}

/// Ring radius and bar thickness of a tow eye.
const TOW_EYE_RADIUS: f64 = 0.045;
const TOW_EYE_BAR: f64 = 0.022;

/// A tow eye standing off an end plate at `plate_z`, facing `facing` (+1 front,
/// -1 rear), drawn back where needed so its tip stays within [`END_REACH`].
fn tow_eye(kit: &mut Kit, f: &HullFrame, x: f64, y: f64, plate_z: f64, facing: f64) {
    // The eye's legs start inside the plate, which hides their ends.
    let (r, t) = (TOW_EYE_RADIUS, TOW_EYE_BAR);
    let limit = f.half + END_REACH - t / 2.0 - r * 2.0;
    let base_z = facing * (facing * plate_z - 0.02).min(limit);
    let base = v(x, y, base_z);
    let tip = base + v(0.0, 0.0, facing * r * 2.0);
    // A squared ring of three bars reads as a forged eye at this size.
    for (a, b) in [
        (base + v(0.0, r, 0.0), tip + v(0.0, r * 0.6, 0.0)),
        (base - v(0.0, r, 0.0), tip - v(0.0, r * 0.6, 0.0)),
        (
            tip + v(0.0, r * 0.6 + t / 2.0, 0.0),
            tip - v(0.0, r * 0.6 + t / 2.0, 0.0),
        ),
    ] {
        let length = (b - a).length();
        kit.block(
            Coat::Steel,
            v(t, t, length + t),
            aim(a, b) * DMat4::from_translation(v(0.0, 0.0, length / 2.0)),
        );
    }
}

/// Rear plate: exhaust louvres, tail lights in guards, tow eyes and cable.
fn rear_plate(kit: &mut Kit, c: &Chassis, f: &HullFrame) {
    let half = f.half;
    let w = f.upper_half_width;
    let lean = (f.shape.rear_lean / (f.roof - f.shape.rear_drop - f.shape.split)).atan();
    // Faces on the rear plate, pitched to its lean; local +z points backward.
    let on_rear = |x: f64, y: f64, proud: f64| -> DMat4 {
        pose(v(x, y, f.rear_face(y) - proud), v(-lean, PI, 0.0))
    };
    let exhaust: &[(f64, f64, f64)] = if c.heavy {
        // Offset outlet on the left, as on the T-72 family.
        &[(0.55, 0.36, 0.16)]
    } else if c.scout {
        &[(-0.42, 0.42, 0.14), (0.42, 0.42, 0.14)]
    } else {
        // The Abrams' wide turbine exhaust grille.
        &[(0.0, 0.96, 0.2)]
    };
    let grille_y = (f.roof - f.shape.rear_drop + f.shape.split) / 2.0;
    for &(x, width, height) in exhaust {
        kit.block(
            Coat::Dark,
            v(width, height, 0.012),
            on_rear(x, grille_y, 0.004),
        );
        for i in 0..3 {
            let y = grille_y - height / 2.0 + height * (f64::from(i) + 0.5) / 3.0;
            kit.block(
                Coat::Gunmetal,
                v(width + 0.02, 0.02, 0.012),
                on_rear(x, y, 0.012) * DMat4::from_rotation_x(0.5),
            );
        }
    }
    // Tail lights near the top corners, under guard hoods.
    let light_y = f.roof - f.shape.rear_drop - 0.07;
    for side in [-1.0, 1.0] {
        let x = side * (w - 0.14);
        kit.block(Coat::Dark, v(0.12, 0.07, 0.02), on_rear(x, light_y, 0.008));
        kit.block(
            Coat::TailLight,
            v(0.045, 0.035, 0.006),
            on_rear(x - side * 0.025, light_y, 0.018),
        );
        kit.block(
            Coat::Glass,
            v(0.035, 0.035, 0.006),
            on_rear(x + side * 0.03, light_y, 0.018),
        );
        // Guard hood over the lenses.
        kit.block(
            Coat::Gunmetal,
            v(0.15, 0.012, 0.02),
            on_rear(x, light_y + 0.045, 0.012),
        );
    }
    // Tow eyes on the lower rear plate.
    let s = &f.shape;
    let eye_y = s.split - 0.08;
    let eye_z = -half
        + s.rear_lean
        + (s.belly_rear_run - s.rear_lean) * (s.split - eye_y) / (s.split - BELLY_Y);
    for side in [-1.0, 1.0] {
        tow_eye(kit, f, side * 0.48, eye_y, eye_z, -1.0);
    }
    if c.heavy {
        // Tow cable slung across the rear plate between its eyes.
        let cable_y = f.shape.split + 0.025;
        let z = f.rear_face(cable_y) - 0.012;
        kit.rod(
            Coat::Gunmetal,
            0.014,
            v(-w + 0.18, cable_y, z),
            v(w - 0.18, cable_y, z),
            6,
        );
        for side in [-1.0, 1.0] {
            kit.block(
                Coat::Steel,
                v(0.05, 0.05, 0.02),
                on_rear(side * (w - 0.16), cable_y, 0.01),
            );
        }
    } else if !c.scout {
        // Infantry phone box at the right rear corner and the tow pintle.
        kit.block(
            Coat::Gunmetal,
            v(0.14, 0.12, 0.016),
            on_rear(-w + 0.4, light_y - 0.02, 0.008),
        );
        kit.block(
            Coat::Steel,
            v(0.1, 0.07, 0.03),
            shift(v(0.0, eye_y + 0.02, eye_z)),
        );
    }
}

/// Glacis and nose fittings: tow eyes, splash guard and spare track or tools.
fn front_fittings(kit: &mut Kit, c: &Chassis, f: &HullFrame) {
    let half = f.half;
    let s = &f.shape;
    let eye_y = s.split - 0.07;
    let eye_z = half - s.lower_run * (s.split - eye_y) / (s.split - BELLY_Y);
    for side in [-1.0, 1.0] {
        tow_eye(kit, f, side * 0.46, eye_y, eye_z, 1.0);
    }
    let glacis_length = s.glacis_run / f.glacis_angle.cos();
    if c.heavy {
        // Splash guard across the glacis and a row of spare track links below it.
        let guard_s = glacis_length * 0.32;
        kit.block(
            Coat::Shade,
            v(1.4, 0.07, 0.016),
            f.on_glacis(0.0, guard_s, 0.03, 0.0) * DMat4::from_rotation_x(0.6),
        );
        let link_s = glacis_length * 0.66;
        for i in 0..4 {
            let x = (f64::from(i) - 1.5) * 0.34;
            let placement = f.on_glacis(x, link_s, 0.018, 0.0);
            kit.block(Coat::Steel, v(0.3, 0.03, 0.15), placement);
            for dx in [-0.05, 0.05] {
                kit.block(
                    Coat::Steel,
                    v(0.025, 0.05, 0.05),
                    placement * DMat4::from_translation(v(dx, 0.03, 0.0)),
                );
            }
        }
        // Retaining bar over the links.
        kit.block(
            Coat::Gunmetal,
            v(1.3, 0.02, 0.025),
            f.on_glacis(0.0, link_s + 0.09, 0.03, 0.0),
        );
    } else {
        // Pioneer tools clipped to the glacis edges: shovel and crowbar.
        // Inboard of the fenders, which roof over the glacis' outer strips.
        let edge = c.overall_width / 2.0 - 0.58;
        let start = glacis_length * 0.18;
        let shovel = f.on_glacis(edge, start, 0.02, 0.0);
        let handle = glacis_length * 0.42;
        let along = |placement: DMat4, z: f64| placement.transform_point3(v(0.0, 0.0, z));
        kit.rod(
            Coat::Wood,
            0.016,
            along(shovel, 0.0),
            along(shovel, handle),
            5,
        );
        kit.block(
            Coat::Steel,
            v(0.13, 0.012, 0.17),
            shovel * DMat4::from_translation(v(0.0, 0.0, handle + 0.09)),
        );
        let bar = f.on_glacis(-edge, start, 0.018, 0.0);
        kit.rod(
            Coat::Gunmetal,
            0.014,
            along(bar, 0.0),
            along(bar, glacis_length * 0.6),
            5,
        );
        for z in [0.12, glacis_length * 0.45] {
            for x in [edge, -edge] {
                kit.block(
                    Coat::Gunmetal,
                    v(0.07, 0.024, 0.025),
                    f.on_glacis(x, start + z, 0.012, 0.0),
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Turret

/// Plan outline of a turret level: flat front face, angled cheeks, straight
/// sides and a squared rear.
fn turret_plan(
    front_x: f64,
    front_z: f64,
    cheek_x: f64,
    cheek_z: f64,
    side_z: f64,
    rear_x: f64,
    rear_z: f64,
) -> Vec<DVec2> {
    p2(&[
        (front_x, front_z),
        (cheek_x, cheek_z),
        (cheek_x, side_z),
        (rear_x, rear_z),
        (-rear_x, rear_z),
        (-cheek_x, side_z),
        (-cheek_x, cheek_z),
        (-front_x, front_z),
    ])
}

/// The band painted round a turret's cheeks, sides and rear just under the roof
/// edge, above the side stowage: its height, the gap it leaves below the roof
/// chamfer and how far it stands proud of the armor. It is the deep trim; the crew
/// livery (the human player's command tank) paints it over in white, standing a
/// little prouder, so the band stays one draw with the turret paint.
const BAND_HEIGHT: f64 = 0.075;
const BAND_BELOW_ROOF: f64 = 0.01;
const BAND_PROUD: f64 = 0.004;
const COMMAND_BAND_PROUD: f64 = 0.008;

/// The turret band on the wall between the `lower` and `upper` outline levels
/// (matching outlines, front face from the last point to the first), `proud` of
/// the armor: every edge but the front face, where the mantlet sits.
fn turret_band(
    kit: &mut Kit,
    lower: (&[DVec2], f64),
    upper: (&[DVec2], f64),
    proud: f64,
    coat: Coat,
) {
    let ((low, low_y), (high, high_y)) = (lower, upper);
    let (top, bottom) = (
        high_y - BAND_BELOW_ROOF,
        high_y - BAND_BELOW_ROOF - BAND_HEIGHT,
    );
    let level = |y: f64| -> Vec<DVec3> {
        let t = (y - low_y) / (high_y - low_y);
        let outline: Vec<DVec2> = low.iter().zip(high).map(|(a, b)| a.lerp(*b, t)).collect();
        inset(&outline, -proud)
            .into_iter()
            .map(|p| v(p.x, y, p.y))
            .collect()
    };
    let (under, over) = (level(bottom), level(top));
    let n = under.len();
    let centre = under.iter().fold(DVec3::ZERO, |sum, p| sum + *p) / n as f64;
    for i in 0..n - 1 {
        let (a, b) = (i, i + 1);
        let mid = (under[a] + under[b]) / 2.0;
        let out = DVec3::new(mid.x - centre.x, 0.0, mid.z - centre.z);
        kit.quad(coat, [under[a], under[b], over[b], over[a]], out);
    }
}

fn turret(kit: &mut Kit, c: &Chassis) {
    let roof = c.deck + roof_rise(c);
    let gun_y = c.deck + gun_rise(c);
    // The bearing ring the turret turns on.
    kit.rod(
        Coat::Dark,
        ring_radius(c),
        v(0.0, c.deck + 0.005, RING_Z),
        v(0.0, c.deck + 0.105, RING_Z),
        16,
    );
    if c.scout {
        scout_turret(kit, c, roof, gun_y);
    } else if c.heavy {
        heavy_turret(kit, c, roof, gun_y);
    } else {
        balanced_turret(kit, c, roof, gun_y);
    }
}

/// Gun shield in bare gun metal (the turret's second material beside the team
/// paint), canvas dust boot and coaxial machine-gun port.
fn mantlet(kit: &mut Kit, gun_y: f64, face_z: f64, size: DVec3, tube: f64, coax_x: f64) {
    kit.chamfer_block(
        Coat::Gunmetal,
        size,
        0.025,
        shift(v(0.0, gun_y, face_z - size.z / 2.0)),
    );
    let boot = p2(&[
        (tube * 1.9, 0.0),
        (tube * 1.75, 0.04),
        (tube * 1.55, 0.08),
        (tube * 1.3, 0.12),
    ]);
    kit.turned(Coat::Canvas, &boot, 12, shift(v(0.0, gun_y, face_z - 0.01)));
    mantlet_bolts(kit, gun_y, face_z, size);
    let port = v(coax_x, gun_y + 0.035, face_z);
    kit.rod(
        Coat::Dark,
        0.022,
        port - v(0.0, 0.0, 0.03),
        port + v(0.0, 0.0, 0.025),
        8,
    );
}

/// A boxy sight head with a glass window in front, optionally with its two
/// armored doors swung open.
fn sight_head(kit: &mut Kit, center: DVec3, size: DVec3, yaw: f64, doors: bool) {
    kit.chamfer_block(Coat::Shade, size, 0.02, pose(center, v(0.0, yaw, 0.0)));
    let face = size.z / 2.0;
    let window = v(size.x * 0.7, size.y * 0.42, 0.012);
    let glass_at = yawed(center, yaw, v(0.0, size.y * 0.06, face));
    kit.block_at(Coat::Glass, window, glass_at, v(0.0, yaw, 0.0));
    if doors {
        for side in [-1.0, 1.0] {
            let hinge = yawed(
                center,
                yaw,
                v(side * (window.x / 2.0 + 0.01), size.y * 0.06, face + 0.008),
            );
            let door_yaw = yaw + side * 1.1;
            kit.block_at(
                Coat::Gunmetal,
                v(window.x / 2.0, window.y + 0.03, 0.012),
                yawed(hinge, door_yaw, v(side * window.x / 4.0, 0.0, 0.0)),
                v(0.0, door_yaw, 0.0),
            );
        }
    }
}

/// Height of a cupola's ring above the roof, and its hatch lid's thickness.
const CUPOLA_HEIGHT: f64 = 0.1;
const CUPOLA_LID: f64 = 0.04;
/// Thickness of the loader's flush hatch lid.
const HATCH_LID: f64 = 0.045;

/// The commander's cupola (base on the roof, ring radius) of a chassis.
fn cupola_at(c: &Chassis) -> (DVec3, f64) {
    let roof = c.deck + roof_rise(c);
    let (x, z) = (c.pick(-0.36, -0.42, -0.36), c.pick(-0.42, -0.32, -0.32));
    (v(x, roof, z), c.pick(0.24, 0.27, 0.24))
}

/// The loader's (the heavy's gunner's) flush roof hatch: centre and lid radius.
fn hatch_at(c: &Chassis) -> (DVec3, f64) {
    let roof = c.deck + roof_rise(c);
    let (x, z) = (c.pick(0.36, 0.42, 0.36), c.pick(-0.46, -0.4, -0.36));
    (v(x, roof - 0.005, z), c.pick(0.2, 0.22, 0.21))
}

/// The turret's two roof hatches as (centre of the lid's top face, lid radius):
/// where a wreck's hatches were blown open.
pub(super) fn hatch_tops(c: &Chassis) -> [(DVec3, f64); 2] {
    let (cupola, ring) = cupola_at(c);
    let (hatch, radius) = hatch_at(c);
    [
        (
            cupola + v(0.0, CUPOLA_HEIGHT + 0.01 + CUPOLA_LID, 0.0),
            ring - 0.05,
        ),
        (hatch + v(0.0, HATCH_LID, 0.0), radius),
    ]
}

/// A commander's cupola: a raised ring of vision blocks and its hatch.
fn cupola(kit: &mut Kit, base: DVec3, radius: f64, blocks: u32) {
    let height = CUPOLA_HEIGHT;
    let body = p2(&[
        (radius, -0.01),
        (radius, height),
        (radius - 0.025, height + 0.015),
        (radius - 0.05, height + 0.015),
    ]);
    kit.turned(Coat::Shade, &body, 12, pose(base, v(-PI / 2.0, 0.0, 0.0)));
    for i in 0..blocks {
        let yaw = f64::from(i) / f64::from(blocks) * PI * 2.0;
        kit.block_at(
            Coat::Glass,
            v(0.075, 0.04, 0.012),
            yawed(base, yaw, v(0.0, height * 0.55, radius - 0.002)),
            v(0.0, yaw, 0.0),
        );
    }
    hatch_lid(
        kit,
        shift(base + v(0.0, height + 0.01, 0.0)),
        radius - 0.05,
        CUPOLA_LID,
    );
}

/// A pintle machine gun: post, receiver, barrel and ammunition can.
fn machine_gun(kit: &mut Kit, roof: f64, pivot: DVec3, yaw: f64, heavy: bool) {
    let (receiver, barrel_radius, barrel_length) = if heavy {
        (v(0.08, 0.09, 0.3), 0.016, 0.62)
    } else {
        (v(0.055, 0.065, 0.24), 0.011, 0.46)
    };
    let local = |offset: DVec3| yawed(pivot, yaw, offset);
    let turn = v(0.0, yaw, 0.0);
    kit.rod(Coat::Gunmetal, 0.02, v(pivot.x, roof, pivot.z), pivot, 6);
    // The cradle the gun rides in on its pintle, and the receiver with its raised
    // feed cover.
    kit.block_at(
        Coat::Steel,
        v(receiver.x + 0.03, 0.03, receiver.z * 0.6),
        local(v(0.0, 0.0, 0.0)),
        turn,
    );
    kit.block_at(Coat::Gunmetal, receiver, local(v(0.0, 0.035, 0.0)), turn);
    kit.block_at(
        Coat::Gunmetal,
        v(receiver.x * 0.8, 0.02, receiver.z * 0.55),
        local(v(0.0, 0.035 + receiver.y / 2.0 + 0.008, 0.02)),
        turn,
    );
    let muzzle_from = local(v(0.0, 0.045, receiver.z / 2.0));
    let muzzle_to = local(v(0.0, 0.045, receiver.z / 2.0 + barrel_length));
    kit.rod(Coat::Gunmetal, barrel_radius, muzzle_from, muzzle_to, 6);
    // Flash hider at the muzzle.
    kit.rod(
        Coat::Gunmetal,
        barrel_radius * 1.6,
        local(v(0.0, 0.045, receiver.z / 2.0 + barrel_length - 0.05)),
        muzzle_to,
        6,
    );
    // The barrel's ventilated jacket (the .50 cal's) or heat shield (the M240's)
    // in worn steel, so the gun reads as a machine gun rather than one dark rod.
    let jacket = if heavy { 0.026 } else { 0.019 };
    kit.rod(
        Coat::Steel,
        jacket,
        muzzle_from,
        local(v(0.0, 0.045, receiver.z / 2.0 + barrel_length * 0.38)),
        6,
    );
    // Canvas ammunition bag on its tray beside the receiver.
    kit.block_at(
        Coat::Canvas,
        v(0.07, 0.09, 0.15),
        local(v(receiver.x / 2.0 + 0.04, 0.0, 0.0)),
        turn,
    );
}

/// A cluster of smoke-grenade dischargers on a turret side, tubes raised and
/// splayed outward.
fn smoke_cluster(kit: &mut Kit, at: DVec3, side: f64, yaw: f64, rows: u32, cols: u32) {
    let pitch = 0.075;
    let plate = v(
        0.04,
        f64::from(rows) * pitch + 0.03,
        f64::from(cols) * pitch + 0.03,
    );
    kit.block_at(Coat::Gunmetal, plate, at, v(0.0, yaw, 0.0));
    let direction = DMat4::from_rotation_y(yaw)
        .transform_vector3(v(side * 0.55, 0.6, 0.6))
        .normalize();
    for row in 0..rows {
        for col in 0..cols {
            let offset = v(
                side * 0.02,
                (f64::from(row) - f64::from(rows - 1) / 2.0) * pitch,
                (f64::from(col) - f64::from(cols - 1) / 2.0) * pitch,
            );
            let base = yawed(at, yaw, offset);
            let end = base + direction * 0.15;
            let tube = p2(&[(0.031, 0.0), (0.031, 0.15)]);
            kit.turned(Coat::Gunmetal, &tube, 6, aim(base, end));
            let cap = p2(&[(0.024, 0.146), (0.0, 0.146)]);
            kit.turned(Coat::Dark, &cap, 6, aim(base, end));
        }
    }
}

/// A radio antenna: base and whip.
fn antenna(kit: &mut Kit, base: DVec3, height: f64) {
    let mount = p2(&[(0.04, 0.0), (0.04, 0.06), (0.018, 0.1), (0.0, 0.1)]);
    kit.turned(Coat::Dark, &mount, 6, pose(base, v(-PI / 2.0, 0.0, 0.0)));
    kit.post(Coat::Dark, 0.007, height, base + v(0.0, 0.1, 0.0), 4);
}

/// A lifting eye: a lug welded upright to the roof.
fn lifting_eye(kit: &mut Kit, at: DVec3) {
    kit.block(
        Coat::Gunmetal,
        v(0.07, 0.05, 0.018),
        shift(at + v(0.0, 0.02, 0.0)),
    );
}

/// A wind sensor mast: pole and crossbar with its two sensor heads.
fn wind_sensor(kit: &mut Kit, base: DVec3, height: f64) {
    kit.post(Coat::Dark, 0.012, height, base, 5);
    let top = base + v(0.0, height, 0.0);
    kit.rod(
        Coat::Dark,
        0.01,
        top - v(0.07, 0.0, 0.0),
        top + v(0.07, 0.0, 0.0),
        4,
    );
    for side in [-1.0, 1.0] {
        kit.block(
            Coat::Dark,
            v(0.025, 0.05, 0.025),
            shift(top + v(side * 0.07, 0.02, 0.0)),
        );
    }
}

/// A jerrycan standing upright.
fn jerrycan(kit: &mut Kit, at: DVec3, yaw: f64) {
    let size = v(0.11, 0.3, 0.22);
    kit.chamfer_block(
        Coat::Canvas,
        size,
        0.018,
        pose(at + v(0.0, size.y / 2.0, 0.0), v(0.0, yaw, 0.0)),
    );
    kit.block_at(
        Coat::Gunmetal,
        v(0.05, 0.03, 0.12),
        yawed(at, yaw, v(0.0, size.y + 0.015, -0.03)),
        v(0.0, yaw, 0.0),
    );
}

/// A soft bag or tarp bundle: a block with heavily rounded-off edges.
fn bag(kit: &mut Kit, at: DVec3, size: DVec3, yaw: f64) {
    let bevel = size.min_element() * 0.3;
    kit.chamfer_block(
        Coat::Canvas,
        size,
        bevel,
        pose(at + v(0.0, size.y / 2.0, 0.0), v(0.0, yaw, 0.0)),
    );
}

/// A rolled tarp lying across.
fn rolled_tarp(kit: &mut Kit, from: DVec3, to: DVec3, radius: f64) {
    kit.rod(Coat::Canvas, radius, from, to, 7);
}

/// An open stowage rack: floor and top rails round three sides, with posts.
/// `z_front` is where its side rails start, `z_back` its rear rail.
fn stowage_rack(kit: &mut Kit, half_x: f64, z_front: f64, z_back: f64, floor: f64, top: f64) {
    let rail = 0.028;
    let corners = [
        v(half_x, 0.0, z_front),
        v(half_x, 0.0, z_back),
        v(-half_x, 0.0, z_back),
        v(-half_x, 0.0, z_front),
    ];
    for y in [floor, top] {
        for pair in corners.windows(2) {
            let (a, b) = (pair[0] + v(0.0, y, 0.0), pair[1] + v(0.0, y, 0.0));
            let length = (b - a).length() + rail;
            kit.block(
                Coat::Gunmetal,
                v(rail, rail, length),
                aim(a, b) * DMat4::from_translation(v(0.0, 0.0, length / 2.0 - rail / 2.0)),
            );
        }
    }
    let posts_back = 3;
    for i in 0..posts_back {
        let x = -half_x + 2.0 * half_x * f64::from(i) / f64::from(posts_back - 1);
        kit.block(
            Coat::Gunmetal,
            v(rail * 0.8, top - floor, rail * 0.8),
            shift(v(x, (floor + top) / 2.0, z_back)),
        );
    }
    // Front posts where the side rails meet the turret.
    for side in [-1.0, 1.0] {
        kit.block(
            Coat::Gunmetal,
            v(rail * 0.8, top - floor, rail * 0.8),
            shift(v(side * half_x, (floor + top) / 2.0, z_front)),
        );
    }
    // Mesh floor read as a dark grid plate.
    kit.block(
        Coat::Dark,
        v(half_x * 2.0, 0.01, (z_front - z_back).abs()),
        shift(v(0.0, floor, (z_front + z_back) / 2.0)),
    );
}

/// The turret armor's plan outlines at its foot, its widest point and under the
/// roof chamfer, with their heights.
fn turret_levels(c: &Chassis) -> [(Vec<DVec2>, f64); 3] {
    let d = c.deck;
    let roof = d + roof_rise(c);
    if c.scout {
        let w = SCOUT_TURRET_WIDTH;
        [
            (
                turret_plan(0.44 * w, 0.8, 0.74 * w, 0.5, -1.02, 0.68 * w, -1.3),
                d + 0.08,
            ),
            (
                turret_plan(0.5 * w, 0.97, 0.84 * w, 0.63, -1.08, 0.78 * w, -1.4),
                d + 0.16,
            ),
            (
                turret_plan(0.48 * w, 0.9, 0.8 * w, 0.58, -1.08, 0.76 * w, -1.38),
                roof - 0.025,
            ),
        ]
    } else if c.heavy {
        let w = HEAVY_TURRET_WIDTH;
        [
            (
                turret_plan(0.48 * w, 0.55, 0.78 * w, 0.3, -1.1, 0.68 * w, -1.5),
                d + 0.1,
            ),
            (
                turret_plan(0.55 * w, 0.72, 0.86 * w, 0.4, -1.15, 0.74 * w, -1.58),
                d + 0.16,
            ),
            (
                turret_plan(0.5 * w, 0.66, 0.82 * w, 0.36, -1.15, 0.7 * w, -1.55),
                roof - 0.03,
            ),
        ]
    } else {
        [
            (
                turret_plan(0.4, 0.8, 0.82, 0.22, -1.2, 0.74, -1.78),
                d + 0.09,
            ),
            (
                turret_plan(0.52, 1.12, 0.98, 0.3, -1.22, 0.86, -1.88),
                d + 0.2,
            ),
            (
                turret_plan(0.5, 1.05, 0.93, 0.26, -1.22, 0.82, -1.85),
                roof - 0.03,
            ),
        ]
    }
}

/// The turret armor and its band in the deep trim.
fn turret_armor(kit: &mut Kit, c: &Chassis, bevel: f64) -> [(Vec<DVec2>, f64); 3] {
    let levels = turret_levels(c);
    kit.solid(Coat::Paint, &chamfered_levels(&levels, bevel));
    turret_band(
        kit,
        (&levels[1].0, levels[1].1),
        (&levels[2].0, levels[2].1),
        BAND_PROUD,
        Coat::Shade,
    );
    levels
}

/// M1 Abrams: flat-faced wedge turret, long bustle with its rack, commander's
/// cupola with the .50 cal, loader's M240, gunner's sight doghouse and CITV.
fn balanced_turret(kit: &mut Kit, c: &Chassis, roof: f64, gun_y: f64) {
    let d = c.deck;
    let levels = turret_armor(kit, c, 0.03);
    roof_bolts(kit, &levels[2].0, roof, 0.08, 0.17);
    mantlet(kit, gun_y, 1.13, v(0.46, 0.32, 0.2), 0.068, -0.17);
    // Gunner's primary sight doghouse (right front) with open ballistic doors.
    sight_head(
        kit,
        v(-0.45, roof + 0.1, 0.64),
        v(0.3, 0.2, 0.36),
        0.0,
        true,
    );
    // Commander's independent thermal viewer (left front).
    kit.post(Coat::Gunmetal, 0.08, 0.1, v(0.5, roof - 0.01, 0.42), 10);
    sight_head(
        kit,
        v(0.5, roof + 0.18, 0.42),
        v(0.22, 0.16, 0.22),
        0.25,
        false,
    );
    // Commander's cupola with the .50 cal, loader's hatch with the M240.
    let (cupola_base, cupola_radius) = cupola_at(c);
    cupola(kit, cupola_base, cupola_radius, 6);
    machine_gun(kit, roof + 0.11, v(-0.3, roof + 0.27, -0.12), 0.1, true);
    let (hatch, hatch_radius) = hatch_at(c);
    hatch_lid(kit, shift(hatch), hatch_radius, HATCH_LID);
    machine_gun(kit, roof, v(0.62, roof + 0.2, -0.2), -0.15, false);
    // M250 smoke-grenade dischargers behind the cheeks.
    for side in [-1.0, 1.0] {
        smoke_cluster(kit, v(side * 0.96, d + 0.42, 0.12), side, side * 0.35, 2, 3);
        // Turret side stowage boxes with latches.
        let (box_at, box_size) = side_stowage(c, side);
        kit.chamfer_block(Coat::Paint, box_size, 0.025, shift(box_at));
        for dz in [-0.2, 0.2] {
            kit.block(
                Coat::Steel,
                v(0.012, 0.05, 0.035),
                shift(box_at + v(side * 0.103, 0.08, dz)),
            );
        }
    }
    // Roof plate welds at the bustle joint, and the bolted covers of the cheek
    // armor cavities in the deep trim: the roof the gunner looks over reads as
    // separate plates rather than one painted slab.
    seam(kit, roof, DVec2::new(-0.78, -1.12), DVec2::new(0.78, -1.12));
    for side in [-1.0, 1.0] {
        cavity_cover(kit, roof, side);
    }
    // Bustle blow-out panels.
    for side in [-1.0, 1.0] {
        kit.block(
            Coat::Shade,
            v(0.58, 0.008, 0.46),
            shift(v(side * 0.36, roof + 0.002, -1.42)),
        );
    }
    // Bustle rack and its load: bags, rolled tarp, water cans and an ammo box.
    stowage_rack(kit, 0.96, -1.3, -2.18, d + 0.2, d + 0.5);
    bag(kit, v(0.5, d + 0.205, -2.02), v(0.5, 0.26, 0.26), 0.05);
    bag(kit, v(-0.05, d + 0.205, -2.03), v(0.42, 0.22, 0.24), -0.08);
    rolled_tarp(kit, v(-0.9, d + 0.29, -2.13), v(0.9, d + 0.29, -2.13), 0.06);
    jerrycan(kit, v(-0.42, d + 0.205, -2.04), PI / 2.0);
    jerrycan(kit, v(-0.72, d + 0.205, -2.04), PI / 2.0);
    for side in [-1.0, 1.0] {
        bag(
            kit,
            v(side * 0.9, d + 0.205, -1.55),
            v(0.12, 0.2, 0.42),
            0.0,
        );
    }
    wind_sensor(kit, v(0.0, roof, -1.62), 0.3);
    antenna(kit, v(0.62, roof, -1.62), 0.95);
    antenna(kit, v(-0.62, roof, -1.62), 0.75);
    for (x, z) in [(0.62, 0.55), (-0.62, 0.55), (0.66, -1.72), (-0.66, -1.72)] {
        lifting_eye(kit, v(x, roof, z));
    }
}

/// Thickness of an armor cavity cover on the turret roof.
const COVER_THICKNESS: f64 = 0.016;

/// The Abrams' bolted cover over one cheek's armor cavity (`side` -1 or +1): a
/// low plate between the gun's roof seam and the cheek edge, a bolt at each
/// corner.
fn cavity_cover(kit: &mut Kit, roof: f64, side: f64) {
    let outline: Vec<DVec2> = p2(&[(0.25, 0.93), (0.45, 0.96), (0.82, 0.3), (0.7, 0.22)])
        .into_iter()
        .map(|p| DVec2::new(p.x * side, p.y))
        .collect();
    let rings = vec![
        sloped_ring(&outline, |_| roof - 0.002),
        sloped_ring(&outline, |_| roof + COVER_THICKNESS - 0.006),
        sloped_ring(&inset(&outline, 0.006), |_| roof + COVER_THICKNESS),
    ];
    kit.solid(Coat::Shade, &rings);
    for corner in inset(&outline, 0.035) {
        kit.block(
            Coat::Steel,
            v(0.022, 0.012, 0.022),
            shift(v(corner.x, roof + COVER_THICKNESS + 0.004, corner.y)),
        );
    }
}

/// Type 99: welded centre turret behind arrowhead add-on wedges with bolted
/// tiles, panoramic commander's sight, laser dazzler and the 12.7 mm HMG.
fn heavy_turret(kit: &mut Kit, c: &Chassis, roof: f64, gun_y: f64) {
    let d = c.deck;
    let levels = turret_armor(kit, c, 0.03);
    roof_bolts(kit, &levels[2].0, roof, 0.08, 0.17);
    // Arrowhead wedges, their tops dipping toward the tips.
    for side in [-1.0, 1.0] {
        let outline: Vec<DVec2> = p2(&[
            (0.21, 0.4),
            (0.21, 1.22),
            (0.3, 1.3),
            (1.0, 0.05),
            (0.88, -0.1),
        ])
        .into_iter()
        .map(|p| DVec2::new(p.x * side * HEAVY_TURRET_WIDTH, p.y))
        .collect();
        let top = |z: f64| roof - 0.02 - 0.1 * (z.max(0.0) / 1.3);
        let bevel = 0.025;
        let rings = vec![
            sloped_ring(&inset(&outline, 0.03), |_| d + 0.13),
            sloped_ring(&outline, |_| d + 0.17),
            sloped_ring(&outline, |z| top(z) - bevel),
            sloped_ring(&inset(&outline, bevel), top),
        ];
        kit.solid(Coat::Paint, &rings);
        // Bolted appliqué tiles along the wedge top.
        for i in 0..3 {
            let t = (f64::from(i) + 0.5) / 3.0;
            let at = DVec2::new(0.3, 1.02).lerp(DVec2::new(0.74, 0.22), t);
            let (x, z) = (at.x * side * HEAVY_TURRET_WIDTH, at.y);
            let slope = (0.1 / 1.3_f64).atan();
            let tile = pose(v(x, top(z) + 0.01, z), v(-slope, side * 0.5, 0.0));
            kit.chamfer_block(Coat::Shade, v(0.2, 0.025, 0.26), 0.008, tile);
            for (bx, bz) in [(-0.06, -0.09), (0.06, -0.09), (-0.06, 0.09), (0.06, 0.09)] {
                kit.block(
                    Coat::Steel,
                    v(0.022, 0.012, 0.022),
                    tile * DMat4::from_translation(v(bx, 0.015, bz)),
                );
            }
        }
    }
    mantlet(kit, gun_y, 1.3, v(0.38, 0.3, 0.42), 0.074, -0.13);
    // Commander's panoramic sight (right front) and gunner's sight (left).
    let pano = v(-0.48, roof, 0.06);
    kit.post(Coat::Gunmetal, 0.075, 0.12, pano, 10);
    let head = p2(&[
        (0.0, 0.0),
        (0.1, 0.0),
        (0.1, 0.13),
        (0.07, 0.16),
        (0.0, 0.16),
    ]);
    kit.turned(
        Coat::Paint,
        &head,
        10,
        pose(pano + v(0.0, 0.12, 0.0), v(-PI / 2.0, 0.0, 0.0)),
    );
    kit.block(
        Coat::Glass,
        v(0.12, 0.06, 0.012),
        shift(pano + v(0.0, 0.19, 0.098)),
    );
    sight_head(
        kit,
        v(0.46, roof + 0.1, 0.04),
        v(0.26, 0.2, 0.32),
        0.0,
        true,
    );
    // Commander's hatch with vision blocks and the 12.7 mm HMG; gunner's hatch.
    let (cupola_base, cupola_radius) = cupola_at(c);
    cupola(kit, cupola_base, cupola_radius, 5);
    machine_gun(kit, roof + 0.11, v(-0.26, roof + 0.26, -0.1), 0.05, true);
    let (hatch, hatch_radius) = hatch_at(c);
    hatch_lid(kit, shift(hatch), hatch_radius, HATCH_LID);
    // JD-3 laser dazzler behind the commander: rotating base, box and aperture.
    let dazzler = v(-0.42, roof, -0.9);
    kit.post(Coat::Gunmetal, 0.12, 0.06, dazzler, 10);
    kit.chamfer_block(
        Coat::Paint,
        v(0.3, 0.26, 0.36),
        0.025,
        shift(dazzler + v(0.0, 0.19, 0.0)),
    );
    let aperture = p2(&[(0.085, 0.0), (0.085, 0.02), (0.0, 0.02)]);
    kit.turned(
        Coat::Glass,
        &aperture,
        12,
        shift(dazzler + v(0.0, 0.2, 0.178)),
    );
    // Smoke dischargers on the turret sides, stowage boxes behind them.
    for side in [-1.0, 1.0] {
        smoke_cluster(
            kit,
            v(side * 0.86 * HEAVY_TURRET_WIDTH, d + 0.44, -0.52),
            side,
            0.0,
            1,
            5,
        );
        let (box_at, box_size) = side_stowage(c, side);
        kit.chamfer_block(Coat::Paint, box_size, 0.03, shift(box_at));
        for dz in [-0.16, 0.16] {
            kit.block(
                Coat::Steel,
                v(0.012, 0.05, 0.035),
                shift(box_at + v(side * 0.123, 0.1, dz)),
            );
        }
    }
    // Roof plate weld at the bustle joint (the centre stays clear for the team
    // insignia).
    seam(kit, roof, DVec2::new(-0.66, -1.05), DVec2::new(0.66, -1.05));
    // Rear stowage box across the bustle.
    kit.chamfer_block(
        Coat::Shade,
        v(1.2 * HEAVY_TURRET_WIDTH, 0.3, 0.26),
        0.03,
        shift(v(0.0, d + 0.33, -1.7)),
    );
    for x in [-0.4, 0.0, 0.4] {
        kit.block(
            Coat::Steel,
            v(0.035, 0.05, 0.012),
            shift(v(x, d + 0.42, -1.835)),
        );
    }
    wind_sensor(kit, v(0.1, roof, -1.12), 0.26);
    antenna(kit, v(0.56, roof, -1.36), 0.9);
    antenna(kit, v(-0.56, roof, -1.36), 0.7);
    for (x, z) in [(0.76, -0.3), (-0.76, -0.3), (0.66, -1.45), (-0.66, -1.45)] {
        lifting_eye(kit, v(x, roof, z));
    }
}

/// M10 Booker: compact faceted welded turret, bolted side armor modules, sight
/// heads, cupola with .50 cal, loader's M240 and a rear basket.
fn scout_turret(kit: &mut Kit, c: &Chassis, roof: f64, gun_y: f64) {
    let d = c.deck;
    let levels = turret_armor(kit, c, 0.025);
    roof_bolts(kit, &levels[2].0, roof, 0.07, 0.16);
    // Bolted side armor modules, leaning with the turret sides.
    for side in [-1.0, 1.0] {
        let lean = side * 0.06;
        let module = pose(v(side * SCOUT_MODULE_X, d + 0.3, -0.42), v(0.0, 0.0, lean));
        kit.chamfer_block(Coat::Shade, v(0.07, 0.26, 0.86), 0.02, module);
        for row in [-0.07, 0.07] {
            for z in [-0.3, 0.0, 0.3] {
                kit.block(
                    Coat::Steel,
                    v(0.012, 0.024, 0.024),
                    module * DMat4::from_translation(v(side * 0.038, row, z)),
                );
            }
        }
    }
    mantlet(kit, gun_y, 1.0, v(0.46, 0.26, 0.2), 0.062, -0.17);
    sight_head(
        kit,
        v(-0.44, roof + 0.09, 0.5),
        v(0.26, 0.18, 0.32),
        0.0,
        true,
    );
    kit.post(Coat::Gunmetal, 0.07, 0.09, v(0.4, roof - 0.01, 0.12), 10);
    sight_head(
        kit,
        v(0.4, roof + 0.16, 0.12),
        v(0.2, 0.15, 0.2),
        -0.2,
        false,
    );
    let (cupola_base, cupola_radius) = cupola_at(c);
    cupola(kit, cupola_base, cupola_radius, 6);
    machine_gun(kit, roof + 0.11, v(-0.24, roof + 0.26, -0.22), 0.1, true);
    let (hatch, hatch_radius) = hatch_at(c);
    hatch_lid(kit, shift(hatch), hatch_radius, HATCH_LID);
    machine_gun(kit, roof, v(0.55, roof + 0.19, -0.28), -0.15, false);
    for side in [-1.0, 1.0] {
        smoke_cluster(
            kit,
            v(side * 0.86 * SCOUT_TURRET_WIDTH, d + 0.36, 0.38),
            side,
            side * 0.3,
            2,
            2,
        );
    }
    seam(kit, roof, DVec2::new(-0.62, -0.88), DVec2::new(0.62, -0.88));
    seam(kit, roof, DVec2::new(-0.39, 0.84), DVec2::new(0.39, 0.84));
    stowage_rack(kit, 0.72, -0.98, -1.62, d + 0.16, d + 0.42);
    bag(kit, v(0.36, d + 0.165, -1.48), v(0.5, 0.24, 0.24), 0.04);
    rolled_tarp(
        kit,
        v(-0.66, d + 0.25, -1.58),
        v(0.66, d + 0.25, -1.58),
        0.05,
    );
    jerrycan(kit, v(-0.3, d + 0.165, -1.5), PI / 2.0);
    jerrycan(kit, v(-0.58, d + 0.165, -1.5), PI / 2.0);
    antenna(kit, v(0.55, roof, -1.12), 0.8);
    antenna(kit, v(-0.55, roof, -1.12), 0.65);
    for (x, z) in [(0.46, 0.42), (-0.46, 0.42), (0.54, -1.18), (-0.54, -1.18)] {
        lifting_eye(kit, v(x, roof, z));
    }
}

/// The turret's side stowage box (the scout's bolted side armor module): centre
/// and size, before the scout module's lean.
fn side_stowage(c: &Chassis, side: f64) -> (DVec3, DVec3) {
    let d = c.deck;
    if c.scout {
        (
            v(side * SCOUT_MODULE_X, d + 0.3, -0.42),
            v(0.07, 0.26, 0.86),
        )
    } else if c.heavy {
        (
            v(
                side * (0.93 + 0.84 * (HEAVY_TURRET_WIDTH - 1.0)),
                d + 0.32,
                -1.18,
            ),
            v(0.24, 0.36, 0.62),
        )
    } else {
        (v(side * 1.03, d + 0.33, -0.74), v(0.2, 0.3, 0.8))
    }
}

/// A livery's extra stowage strapped on the turret's side boxes: none, a bag on
/// the left, or a rolled tarp and a bag on the right.
fn extra_stowage(kit: &mut Kit, c: &Chassis, variant: u8) {
    let side = if variant == 1 { 1.0 } else { -1.0 };
    let (at, size) = side_stowage(c, side);
    let top = at + v(0.0, size.y / 2.0, 0.0);
    match variant {
        1 => bag(
            kit,
            top + v(-side * 0.02, 0.0, 0.08),
            v(0.26, 0.17, 0.42),
            0.12 * side,
        ),
        2 => {
            rolled_tarp(
                kit,
                top + v(0.0, 0.07, size.z * 0.45),
                top + v(0.0, 0.07, -size.z * 0.45),
                0.07,
            );
            bag(
                kit,
                top + v(-side * 0.04, 0.0, -size.z * 0.2),
                v(0.22, 0.14, 0.3),
                -0.2 * side,
            );
        }
        _ => {}
    }
}

/// The team's tactical sign, small and stencilled in faded paint on the turret
/// roof, the same on every tank of a team (a diamond for blue, a forward-pointing
/// arrowhead for red, like the skirts' sign), so team and facing still read from
/// the overhead camera without relying on hue alone, while the turret roof keeps
/// its shape (a large white sign read as a hole in it). Stencilled
/// unit numbers on both turret sides, the turret band and the livery's extra
/// stowage. The crew livery (the one a human player's tank wears) paints its band
/// white: the command tank's mark. The gun's white bands give the class.
fn marking(kit: &mut Kit, c: &Chassis, team: Team, variant: u8) {
    extra_stowage(kit, c, variant);
    if variant == CREW_LIVERY {
        let levels = turret_levels(c);
        turret_band(
            kit,
            (&levels[1].0, levels[1].1),
            (&levels[2].0, levels[2].1),
            COMMAND_BAND_PROUD,
            Coat::Marking,
        );
    }
    let roof = c.deck + roof_rise(c);
    let length = c.pick(0.34, 0.4, 0.42);
    let half_width = c.pick(0.16, 0.19, 0.2);
    let centre = DVec2::new(0.0, c.pick(0.34, 0.42, 0.34));
    let outline: Vec<DVec2> = if team == Team::Blue {
        p2(&[(0.0, 0.5), (-1.0, 0.0), (0.0, -0.5), (1.0, 0.0)])
    } else {
        p2(&[(0.0, 0.5), (-1.0, -0.5), (0.0, -0.32), (1.0, -0.5)])
    }
    .into_iter()
    .map(|p| centre + DVec2::new(p.x * half_width, p.y * length))
    .collect();
    roof_sign(kit, Coat::Stencil, &outline, roof + 0.003);
    let number = unit_number(c.kind, team, variant);
    let d = c.deck;
    for side in [-1.0, 1.0] {
        // On the turret's side stowage boxes (the scout's bolted side modules).
        let (center, up) = if c.scout {
            let lean = side * 0.06;
            let module = pose(v(side * SCOUT_MODULE_X, d + 0.3, -0.42), v(0.0, 0.0, lean));
            (
                module.transform_point3(v(side * 0.039, 0.0, 0.15)),
                module.transform_vector3(DVec3::Y),
            )
        } else if c.heavy {
            let (box_at, size) = side_stowage(c, side);
            (
                v(box_at.x + side * (size.x / 2.0 + 0.004), d + 0.27, -1.18),
                DVec3::Y,
            )
        } else {
            (v(side * 1.134, d + 0.3, -0.74), DVec3::Y)
        };
        let right = v(0.0, 0.0, -side);
        stencil_number(kit, number, center, right, up, c.pick(0.2, 0.24, 0.27));
    }
}

/// A flat sign painted on a roof at height `y`, from its plan outline (fanned from
/// its first corner, which must see every other).
fn roof_sign(kit: &mut Kit, coat: Coat, outline: &[DVec2], y: f64) {
    for i in 1..outline.len() - 1 {
        let corners = [outline[0], outline[i], outline[i + 1]].map(|p| v(p.x, y, p.y));
        kit.quad(
            coat,
            [corners[0], corners[1], corners[2], corners[2]],
            DVec3::Y,
        );
    }
}

/// The unit number a livery paints on a chassis of a team; the crew livery is
/// the platoon commander's "01".
fn unit_number(kind: VehicleKind, team: Team, variant: u8) -> &'static str {
    if variant == CREW_LIVERY {
        return "01";
    }
    let numbers = match (kind, team) {
        (VehicleKind::Scout, Team::Blue) => ["17", "12", "19"],
        (VehicleKind::Heavy, Team::Blue) => ["31", "33", "38"],
        (_, Team::Blue) => ["24", "21", "26"],
        (VehicleKind::Scout, _) => ["52", "55", "57"],
        (VehicleKind::Heavy, _) => ["73", "76", "79"],
        _ => ["68", "61", "64"],
    };
    numbers[usize::from(variant) % numbers.len()]
}

/// Lit segments of a stencilled digit: a, b, c, d, e, f, g (top, upper right,
/// lower right, bottom, lower left, upper left, middle).
fn digit_segments(digit: char) -> [bool; 7] {
    let bits: u8 = match digit {
        '0' => 0b0111111,
        '1' => 0b0000110,
        '2' => 0b1011011,
        '3' => 0b1001111,
        '4' => 0b1100110,
        '5' => 0b1101101,
        '6' => 0b1111101,
        '7' => 0b0000111,
        '8' => 0b1111111,
        _ => 0b1101111,
    };
    std::array::from_fn(|i| bits & (1 << i) != 0)
}

/// Stencil numerals centred on `center` of a flat face: `right` runs along the
/// text and `up` up the digits; they lie a few millimetres proud of the face.
/// Segments stop short of each other, leaving the stencil's bridges.
fn stencil_number(kit: &mut Kit, text: &str, center: DVec3, right: DVec3, up: DVec3, height: f64) {
    let width = height * 0.56;
    let stroke = height * 0.2;
    let bridge = height * 0.07;
    let pitch = width + height * 0.22;
    let normal = right.cross(up).normalize();
    let count = text.chars().count() as f64;
    let start = -(pitch * (count - 1.0) + width) / 2.0;
    let half = height / 2.0;
    for (i, digit) in text.chars().enumerate() {
        let left = start + pitch * i as f64;
        let rect = |u0: f64, u1: f64, v0: f64, v1: f64| {
            let at =
                |u: f64, w: f64| center + right * (left + u) + up * (w - half) + normal * 0.004;
            [at(u0, v0), at(u1, v0), at(u1, v1), at(u0, v1)]
        };
        let (w, h, t, g) = (width, height, stroke, bridge);
        let mid = h / 2.0;
        let segments = [
            rect(g, w - g, h - t, h),
            rect(w - t, w, mid + g / 2.0, h - g),
            rect(w - t, w, g, mid - g / 2.0),
            rect(g, w - g, 0.0, t),
            rect(0.0, t, g, mid - g / 2.0),
            rect(0.0, t, mid + g / 2.0, h - g),
            rect(g, w - g, mid - t / 2.0, mid + t / 2.0),
        ];
        for (lit, corners) in digit_segments(digit).into_iter().zip(segments) {
            if lit {
                kit.quad(Coat::Marking, corners, normal);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Gun

/// The gun tube from the breech (inside the turret) to the muzzle face: thermal
/// sleeve with clamp bands, bore evacuator, muzzle brake or reference sensor.
/// The bore disc named `muzzle` is added by the caller.
fn gun(kit: &mut Kit, c: &Chassis) {
    let r = tube_radius(c);
    let muzzle = muzzle_z(c);
    let gun_y = c.deck + gun_rise(c);
    let axis = shift(v(0.0, gun_y, 0.0));
    let sleeve = r * 1.1;
    // Breech end and the thick tube root behind the mantlet.
    kit.turned(
        Coat::Steel,
        &p2(&[(0.0, 0.6), (r * 1.4, 0.6), (r * 1.4, 1.2), (sleeve, 1.32)]),
        GUN_SIDES,
        axis,
    );
    let (bands, evacuator): (&[f64], [f64; 2]) = if c.scout {
        (&[1.55, 2.75], [1.95, 2.33])
    } else if c.heavy {
        (&[1.9, 2.55, 3.72], [2.85, 3.32])
    } else {
        (&[1.72, 2.82, 3.28], [2.05, 2.47])
    };
    // Sleeve runs between clamp bands, around the evacuator.
    let sleeve_end = muzzle - c.pick(0.4, 0.24, 0.5);
    let mut stops: Vec<(f64, f64)> = bands.iter().map(|&z| (z, z + 0.04)).collect();
    stops.push((evacuator[0], evacuator[1]));
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut from = 1.32;
    let mut widest = (0.0, from, from);
    for &(start, end) in &stops {
        kit.turned(
            Coat::Paint,
            &p2(&[(sleeve, from), (sleeve, start)]),
            GUN_SIDES,
            axis,
        );
        if start - from > widest.0 {
            widest = (start - from, from, start);
        }
        from = end;
    }
    let taper = r * 1.02;
    kit.turned(
        Coat::Paint,
        &p2(&[(sleeve, from), (taper, sleeve_end)]),
        GUN_SIDES,
        axis,
    );
    // Class bands round the middle of the sleeve's longest run: one on the
    // Skipper, two on the Bruiser, three on the Big Rig, readable from overhead.
    let count = c.pick(1.0, 2.0, 3.0);
    let span = count * ID_BAND_WIDTH + (count - 1.0) * ID_BAND_GAP;
    let first = (widest.1 + widest.2 - span) / 2.0;
    for i in 0..count as u32 {
        let z = first + f64::from(i) * (ID_BAND_WIDTH + ID_BAND_GAP);
        let band = sleeve + 0.003;
        kit.turned(
            Coat::Marking,
            &p2(&[(band, z), (band, z + ID_BAND_WIDTH)]),
            GUN_SIDES,
            axis,
        );
    }
    for &z in bands {
        let band = r * 1.24;
        kit.turned(
            Coat::Gunmetal,
            &p2(&[(sleeve, z), (band, z + 0.02), (sleeve, z + 0.04)]),
            GUN_SIDES,
            axis,
        );
    }
    let bulge = r * 1.62;
    let [e0, e1] = evacuator;
    kit.turned(
        Coat::Shade,
        &p2(&[
            (sleeve, e0),
            (bulge, e0 + 0.07),
            (bulge, e1 - 0.07),
            (sleeve, e1),
        ]),
        GUN_SIDES,
        axis,
    );
    let bore = r * 0.76;
    if c.scout {
        // Multi-baffle muzzle brake.
        let body = r * 1.6;
        let core = r * 1.08;
        kit.turned(
            Coat::Gunmetal,
            &p2(&[
                (taper, sleeve_end),
                (taper, muzzle - 0.36),
                (body, muzzle - 0.34),
                (body, muzzle - 0.24),
                (core, muzzle - 0.24),
                (core, muzzle - 0.17),
                (body, muzzle - 0.17),
                (body, muzzle - 0.1),
                (core, muzzle - 0.1),
                (core, muzzle - 0.05),
                (body, muzzle - 0.05),
                (body, muzzle),
                (bore, muzzle),
                (bore, muzzle - 0.06),
                (0.0, muzzle - 0.06),
            ]),
            GUN_SIDES,
            axis,
        );
    } else if c.heavy {
        // Two-chamber muzzle brake with side ports.
        let body = r * 1.7;
        let core = r * 1.12;
        kit.turned(
            Coat::Gunmetal,
            &p2(&[
                (taper, sleeve_end),
                (taper, muzzle - 0.44),
                (body, muzzle - 0.42),
                (body, muzzle - 0.3),
                (core, muzzle - 0.3),
                (core, muzzle - 0.2),
                (body, muzzle - 0.2),
                (body, muzzle - 0.03),
                (r * 1.45, muzzle),
                (bore, muzzle),
                (bore, muzzle - 0.08),
                (0.0, muzzle - 0.08),
            ]),
            GUN_SIDES,
            axis,
        );
        for side in [-1.0, 1.0] {
            for z in [muzzle - 0.36, muzzle - 0.115] {
                kit.block(
                    Coat::Dark,
                    v(0.01, r * 1.3, 0.075),
                    shift(v(side * (body - 0.002), gun_y, z)),
                );
            }
        }
    } else {
        // A plain muzzle that ends in a thickened lip, so the long gun finishes
        // in a deliberate shape rather than a cut tube.
        let collar = r * 1.1;
        let lip = r * 1.32;
        kit.turned(
            Coat::Gunmetal,
            &p2(&[
                (taper, sleeve_end),
                (collar, sleeve_end + 0.02),
                (collar, muzzle - 0.09),
                (lip, muzzle - 0.07),
                (lip, muzzle),
                (bore, muzzle),
                (bore, muzzle - 0.08),
                (0.0, muzzle - 0.08),
            ]),
            GUN_SIDES,
            axis,
        );
        // Muzzle reference sensor on its bracket.
        let sensor = v(0.0, gun_y + collar + 0.02, muzzle - 0.1);
        kit.block(Coat::Steel, v(0.05, 0.035, 0.08), shift(sensor));
        kit.block(
            Coat::Dark,
            v(0.03, 0.02, 0.006),
            shift(sensor + v(0.0, 0.0, -0.043)),
        );
    }
}

/// The white class bands on a gun sleeve: their width and the gap between them.
const ID_BAND_WIDTH: f64 = 0.08;
const ID_BAND_GAP: f64 = 0.06;

/// Outer radius of the bare gun tube.
pub(super) fn tube_radius(c: &Chassis) -> f64 {
    c.pick(0.046, 0.068, 0.094)
}

/// z of the muzzle face; the bore disc sits just in front of it.
pub(super) fn muzzle_z(c: &Chassis) -> f64 {
    c.pick(3.72, 3.84, 4.44)
}

#[cfg(test)]
mod tests {
    use glam::DVec3;

    use super::super::{Team, VehicleKind, part, tank_model_variant};
    use super::RING_Z;

    /// Upward-facing team-paint triangles of the hull whose centres lie inside
    /// the turret ring.
    fn roof_triangles_over_ring(kind: VehicleKind, open: bool) -> usize {
        let ring_radius = if kind == VehicleKind::Scout {
            0.59
        } else {
            0.76
        };
        let model = tank_model_variant(kind, Team::Blue, open);
        let hull = model.find(part::HULL).expect("hull");
        let paint = hull.children[1].drawable.as_ref().expect("team paint");
        paint
            .mesh
            .positions
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| t.map(|p| DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))))
            .filter(|[a, b, c]| {
                let up = (*b - *a).cross(*c - *a).normalize().y > 0.99;
                let centre = (*a + *b + *c) / 3.0;
                up && centre.x.hypot(centre.z - RING_Z) < ring_radius - 1e-6
            })
            .count()
    }

    #[test]
    fn wrecks_without_a_turret_show_an_open_ring() {
        for kind in VehicleKind::PLAYABLE {
            assert!(
                roof_triangles_over_ring(kind, false) > 0,
                "{kind:?} closed roof"
            );
            assert_eq!(
                roof_triangles_over_ring(kind, true),
                0,
                "{kind:?} open ring"
            );
        }
    }
}
