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
use super::{Team, VehicleKind};

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
/// Turret-ring centre behind the hull centre, shared with the turret ring.
pub(super) const RING_Z: f64 = -0.12;

/// The merged assemblies of a chassis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Assembly {
    Hull,
    /// The hull of a wreck whose turret flew off, with the ring cut open.
    OpenHull,
    Turret,
    Barrel,
    /// Team insignia on the turret roof.
    Marking(Team),
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
            Assembly::Marking(team) => marking(&mut kit, c, team),
        }
        kit.finish()
    })
}

/// Turret-ring radius (the turret's bearing ring) and the open well's radius.
pub(super) fn ring_radius(c: &Chassis) -> f64 {
    if c.scout { 0.59 } else { 0.76 }
}

/// Height of the turret roof above the deck.
fn roof_rise(c: &Chassis) -> f64 {
    c.pick(0.5, 0.56, 0.58)
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
    rear_plate(kit, c, &f);
    front_fittings(kit, c, &f);
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
        kit.block_at(Coat::Shade, v(width, height, 0.06), at, v(0.0, yaw, 0.0));
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
    flat(kit, Coat::Shade, size / 2.0, f.roof + 0.003);
    flat(kit, Coat::Dark, size / 2.0 - 0.025, f.roof + 0.005);
    let pitch = (size.y - 0.08) / f64::from(slats);
    let half_x = size.x / 2.0 - 0.035;
    for i in 0..slats {
        // Each louvre slopes down toward the rear, over a dark gap.
        let z0 = center.y - size.y / 2.0 + 0.04 + f64::from(i) * pitch;
        let (front, back) = (z0 + pitch * 0.15, z0 + pitch * 0.85);
        let (low, high) = (f.roof + 0.006, f.top - 0.002);
        kit.quad(
            Coat::Shade,
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

/// A dark weld seam or panel line lying on a flat roof at height `y`, from and
/// to plan points (x, z).
fn seam(kit: &mut Kit, y: f64, from: DVec2, to: DVec2) {
    let across = (to - from).normalize().perp() * 0.005;
    let at = |p: DVec2| v(p.x, y + 0.0025, p.y);
    kit.quad(
        Coat::Dark,
        [
            at(from - across),
            at(to - across),
            at(to + across),
            at(from + across),
        ],
        DVec3::Y,
    );
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
                Coat::Shade,
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
            Coat::Shade,
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
            Coat::Shade,
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
            Coat::Shade,
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
                    Coat::Shade,
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

/// Gun shield, canvas dust boot and coaxial machine-gun port.
fn mantlet(kit: &mut Kit, gun_y: f64, face_z: f64, size: DVec3, tube: f64, coax_x: f64) {
    kit.chamfer_block(
        Coat::Shade,
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
    kit.chamfer_block(Coat::Paint, size, 0.02, pose(center, v(0.0, yaw, 0.0)));
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
                Coat::Shade,
                v(window.x / 2.0, window.y + 0.03, 0.012),
                yawed(hinge, door_yaw, v(side * window.x / 4.0, 0.0, 0.0)),
                v(0.0, door_yaw, 0.0),
            );
        }
    }
}

/// A commander's cupola: a raised ring of vision blocks and its hatch.
fn cupola(kit: &mut Kit, base: DVec3, radius: f64, blocks: u32) {
    let height = 0.1;
    let body = p2(&[
        (radius, -0.01),
        (radius, height),
        (radius - 0.025, height + 0.015),
        (radius - 0.05, height + 0.015),
    ]);
    kit.turned(Coat::Paint, &body, 12, pose(base, v(-PI / 2.0, 0.0, 0.0)));
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
        0.04,
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
    kit.rod(Coat::Gunmetal, 0.02, v(pivot.x, roof, pivot.z), pivot, 6);
    kit.block_at(
        Coat::Gunmetal,
        receiver,
        local(v(0.0, 0.035, 0.0)),
        v(0.0, yaw, 0.0),
    );
    let muzzle_from = local(v(0.0, 0.045, receiver.z / 2.0));
    let muzzle_to = local(v(0.0, 0.045, receiver.z / 2.0 + barrel_length));
    kit.rod(Coat::Gunmetal, barrel_radius, muzzle_from, muzzle_to, 6);
    if heavy {
        // Perforated barrel jacket of the .50 cal.
        kit.rod(
            Coat::Gunmetal,
            0.026,
            muzzle_from,
            local(v(0.0, 0.045, receiver.z / 2.0 + 0.16)),
            6,
        );
    }
    kit.block_at(
        Coat::Canvas,
        v(0.07, 0.09, 0.15),
        local(v(receiver.x / 2.0 + 0.04, 0.0, 0.0)),
        v(0.0, yaw, 0.0),
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
    kit.block_at(Coat::Shade, plate, at, v(0.0, yaw, 0.0));
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
            kit.turned(Coat::Shade, &tube, 6, aim(base, end));
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
        Coat::Shade,
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
        Coat::Shade,
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
                Coat::Shade,
                v(rail, rail, length),
                aim(a, b) * DMat4::from_translation(v(0.0, 0.0, length / 2.0 - rail / 2.0)),
            );
        }
    }
    let posts_back = 3;
    for i in 0..posts_back {
        let x = -half_x + 2.0 * half_x * f64::from(i) / f64::from(posts_back - 1);
        kit.block(
            Coat::Shade,
            v(rail * 0.8, top - floor, rail * 0.8),
            shift(v(x, (floor + top) / 2.0, z_back)),
        );
    }
    // Front posts where the side rails meet the turret.
    for side in [-1.0, 1.0] {
        kit.block(
            Coat::Shade,
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

/// M1 Abrams: flat-faced wedge turret, long bustle with its rack, commander's
/// cupola with the .50 cal, loader's M240, gunner's sight doghouse and CITV.
fn balanced_turret(kit: &mut Kit, c: &Chassis, roof: f64, gun_y: f64) {
    let d = c.deck;
    let levels = [
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
    ];
    kit.solid(Coat::Paint, &chamfered_levels(&levels, 0.03));
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
    kit.post(Coat::Shade, 0.08, 0.1, v(0.5, roof - 0.01, 0.42), 10);
    sight_head(
        kit,
        v(0.5, roof + 0.18, 0.42),
        v(0.22, 0.16, 0.22),
        0.25,
        false,
    );
    // Commander's cupola with the .50 cal, loader's hatch with the M240.
    cupola(kit, v(-0.42, roof, -0.32), 0.27, 6);
    machine_gun(kit, roof + 0.11, v(-0.3, roof + 0.27, -0.12), 0.1, true);
    hatch_lid(kit, shift(v(0.42, roof - 0.005, -0.4)), 0.22, 0.045);
    machine_gun(kit, roof, v(0.62, roof + 0.2, -0.2), -0.15, false);
    // M250 smoke-grenade dischargers behind the cheeks.
    for side in [-1.0, 1.0] {
        smoke_cluster(kit, v(side * 0.96, d + 0.42, 0.12), side, side * 0.35, 2, 3);
        // Turret side stowage boxes with latches.
        let box_at = v(side * 1.03, d + 0.33, -0.74);
        kit.chamfer_block(Coat::Paint, v(0.2, 0.3, 0.8), 0.025, shift(box_at));
        for dz in [-0.2, 0.2] {
            kit.block(
                Coat::Steel,
                v(0.012, 0.05, 0.035),
                shift(box_at + v(side * 0.103, 0.08, dz)),
            );
        }
    }
    // Roof plate welds: the bustle joint and the armor cavity covers.
    seam(kit, roof, DVec2::new(-0.78, -1.12), DVec2::new(0.78, -1.12));
    for x in [-0.2, 0.2] {
        seam(kit, roof, DVec2::new(x, 0.95), DVec2::new(x * 3.4, 0.2));
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

/// Type 99: welded centre turret behind arrowhead add-on wedges with bolted
/// tiles, panoramic commander's sight, laser dazzler and the 12.7 mm HMG.
fn heavy_turret(kit: &mut Kit, c: &Chassis, roof: f64, gun_y: f64) {
    let d = c.deck;
    let levels = [
        (
            turret_plan(0.48, 0.55, 0.78, 0.3, -1.1, 0.68, -1.5),
            d + 0.1,
        ),
        (
            turret_plan(0.55, 0.72, 0.86, 0.4, -1.15, 0.74, -1.58),
            d + 0.16,
        ),
        (
            turret_plan(0.5, 0.66, 0.82, 0.36, -1.15, 0.7, -1.55),
            roof - 0.03,
        ),
    ];
    kit.solid(Coat::Paint, &chamfered_levels(&levels, 0.03));
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
        .map(|p| DVec2::new(p.x * side, p.y))
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
            let (x, z) = (at.x * side, at.y);
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
    kit.post(Coat::Shade, 0.075, 0.12, pano, 10);
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
    cupola(kit, v(-0.36, roof, -0.32), 0.24, 5);
    machine_gun(kit, roof + 0.11, v(-0.26, roof + 0.26, -0.1), 0.05, true);
    hatch_lid(kit, shift(v(0.36, roof - 0.005, -0.36)), 0.21, 0.045);
    // JD-3 laser dazzler behind the commander: rotating base, box and aperture.
    let dazzler = v(-0.42, roof, -0.9);
    kit.post(Coat::Shade, 0.12, 0.06, dazzler, 10);
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
        smoke_cluster(kit, v(side * 0.86, d + 0.44, -0.52), side, 0.0, 1, 5);
        let box_at = v(side * 0.93, d + 0.32, -1.18);
        kit.chamfer_block(Coat::Paint, v(0.24, 0.36, 0.62), 0.03, shift(box_at));
        for dz in [-0.16, 0.16] {
            kit.block(
                Coat::Steel,
                v(0.012, 0.05, 0.035),
                shift(box_at + v(side * 0.123, 0.1, dz)),
            );
        }
    }
    // Roof plate welds.
    seam(kit, roof, DVec2::new(-0.66, -1.05), DVec2::new(0.66, -1.05));
    seam(kit, roof, DVec2::new(0.0, 0.62), DVec2::new(0.0, -0.05));
    // Rear stowage box across the bustle.
    kit.chamfer_block(
        Coat::Shade,
        v(1.2, 0.3, 0.26),
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
    for (x, z) in [(0.7, -0.3), (-0.7, -0.3), (0.6, -1.45), (-0.6, -1.45)] {
        lifting_eye(kit, v(x, roof, z));
    }
}

/// M10 Booker: compact faceted welded turret, bolted side armor modules, sight
/// heads, cupola with .50 cal, loader's M240 and a rear basket.
fn scout_turret(kit: &mut Kit, c: &Chassis, roof: f64, gun_y: f64) {
    let d = c.deck;
    let levels = [
        (
            turret_plan(0.44, 0.8, 0.74, 0.5, -1.02, 0.68, -1.3),
            d + 0.08,
        ),
        (
            turret_plan(0.5, 0.97, 0.84, 0.63, -1.08, 0.78, -1.4),
            d + 0.16,
        ),
        (
            turret_plan(0.48, 0.9, 0.8, 0.58, -1.08, 0.76, -1.38),
            roof - 0.025,
        ),
    ];
    kit.solid(Coat::Paint, &chamfered_levels(&levels, 0.025));
    // Bolted side armor modules, leaning with the turret sides.
    for side in [-1.0, 1.0] {
        let lean = side * 0.06;
        let module = pose(v(side * 0.865, d + 0.3, -0.42), v(0.0, 0.0, lean));
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
    mantlet(kit, gun_y, 1.0, v(0.46, 0.3, 0.2), 0.062, -0.17);
    sight_head(
        kit,
        v(-0.44, roof + 0.09, 0.5),
        v(0.26, 0.18, 0.32),
        0.0,
        true,
    );
    kit.post(Coat::Shade, 0.07, 0.09, v(0.4, roof - 0.01, 0.12), 10);
    sight_head(
        kit,
        v(0.4, roof + 0.16, 0.12),
        v(0.2, 0.15, 0.2),
        -0.2,
        false,
    );
    cupola(kit, v(-0.36, roof, -0.42), 0.24, 6);
    machine_gun(kit, roof + 0.11, v(-0.24, roof + 0.26, -0.22), 0.1, true);
    hatch_lid(kit, shift(v(0.36, roof - 0.005, -0.46)), 0.2, 0.045);
    machine_gun(kit, roof, v(0.55, roof + 0.19, -0.28), -0.15, false);
    for side in [-1.0, 1.0] {
        smoke_cluster(kit, v(side * 0.86, d + 0.36, 0.38), side, side * 0.3, 2, 2);
    }
    seam(kit, roof, DVec2::new(-0.7, -0.88), DVec2::new(0.7, -0.88));
    seam(kit, roof, DVec2::new(-0.44, 0.84), DVec2::new(0.44, 0.84));
    stowage_rack(kit, 0.8, -0.98, -1.62, d + 0.16, d + 0.42);
    bag(kit, v(0.36, d + 0.165, -1.48), v(0.5, 0.24, 0.24), 0.04);
    rolled_tarp(
        kit,
        v(-0.74, d + 0.25, -1.58),
        v(0.74, d + 0.25, -1.58),
        0.05,
    );
    jerrycan(kit, v(-0.3, d + 0.165, -1.5), PI / 2.0);
    jerrycan(kit, v(-0.58, d + 0.165, -1.5), PI / 2.0);
    antenna(kit, v(0.55, roof, -1.12), 0.8);
    antenna(kit, v(-0.55, roof, -1.12), 0.65);
    for (x, z) in [(0.5, 0.42), (-0.5, 0.42), (0.6, -1.18), (-0.6, -1.18)] {
        lifting_eye(kit, v(x, roof, z));
    }
}

/// Team insignia on the turret roof: a diamond for blue, twin bars for red.
fn marking(kit: &mut Kit, c: &Chassis, team: Team) {
    let roof = c.deck + roof_rise(c);
    let y = roof + 0.003;
    if team == Team::Blue {
        kit.block_at(
            Coat::Marking,
            v(0.2, 0.006, 0.2),
            v(0.0, y, 0.12),
            v(0.0, PI / 4.0, 0.0),
        );
    } else {
        for x in [-0.07, 0.07] {
            kit.block(Coat::Marking, v(0.06, 0.006, 0.24), shift(v(x, y, 0.12)));
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
    let sleeve_end = muzzle - if c.scout { 0.4 } else { 0.24 };
    let mut stops: Vec<(f64, f64)> = bands.iter().map(|&z| (z, z + 0.04)).collect();
    stops.push((evacuator[0], evacuator[1]));
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut from = 1.32;
    for &(start, end) in &stops {
        kit.turned(
            Coat::Shade,
            &p2(&[(sleeve, from), (sleeve, start)]),
            GUN_SIDES,
            axis,
        );
        from = end;
    }
    let taper = r * 1.02;
    kit.turned(
        Coat::Shade,
        &p2(&[(sleeve, from), (taper, sleeve_end)]),
        GUN_SIDES,
        axis,
    );
    for &z in bands {
        let band = r * 1.24;
        kit.turned(
            Coat::Steel,
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
            Coat::Steel,
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
    } else {
        let collar = r * 1.1;
        kit.turned(
            Coat::Steel,
            &p2(&[
                (taper, sleeve_end),
                (collar, sleeve_end + 0.02),
                (collar, muzzle),
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

/// Outer radius of the bare gun tube.
pub(super) fn tube_radius(c: &Chassis) -> f64 {
    c.pick(0.062, 0.068, 0.074)
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
