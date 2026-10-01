//! Port of `humvee-model.ts`: the wheeled launcher vehicle.
//!
//! Same part names as the tanks: `hull` (stretched along z), `track-group` (the
//! wheels, un-stretched so they stay round), `turret`, `barrel` and an empty
//! `muzzle` marker at the launcher mouth.

use std::f64::consts::PI;
use std::sync::{Arc, OnceLock};

use glam::{DVec2, DVec3};

use super::model_primitives::{
    TEAM_COLORS, box_part, cylinder_part, material, paint, put, rotated, shadowed,
};
use super::tank_model::{DARK, STEEL, WRECK_PAINT, WRECK_STEEL, shade_of};
use super::tank_surfaces::{Finish, apply_tank_surface};
use super::{Team, VehicleKind, part};
use crate::geometry::{ExtrudeOptions, Mesh, Path, Shape, extrude_geometry, lathe_geometry, widen};
use crate::scene::Node;

/// The body shell and wheelbase are stretched by this factor along z.
pub const HUMVEE_BODY_LENGTH_SCALE: f64 = 1.16;
/// Height of the roof launcher tube's axis.
const LAUNCHER_Y: f64 = 2.13;
const WRECK_RUBBER: u32 = 0x1d252b;
const RUBBER: u32 = 0x17201d;
const WRECK_GLASS: u32 = 0x1a2328;
const GLASS: u32 = 0x29444b;
const TURN_SIGNAL: u32 = 0xdf923b;
const TAIL_LIGHT: u32 = 0x731c15;
const HEADLAMP: u32 = 0xe5ddbc;
const REAR_LAMP: u32 = 0x931e16;

/// Shared low-poly shells carry the silhouette; small fittings stay simple boxes.
/// A (z, y) side profile extruded across `width` in x, with UVs remapped so sides
/// use z/y, tops z and ends x.
fn profile_geometry(points: &[[f64; 2]], width: f64) -> Mesh {
    let mut profile = Path::new();
    for (i, [z, y]) in points.iter().enumerate() {
        if i == 0 {
            profile.move_to(*z, *y);
        } else {
            profile.line_to(*z, *y);
        }
    }
    profile.close_path();
    let options = ExtrudeOptions {
        depth: width,
        bevel_enabled: false,
        steps: 1,
        ..ExtrudeOptions::default()
    };
    let mut mesh = extrude_geometry(&[Shape::new(profile)], &options);
    mesh.rotate_y(-PI / 2.0).translate(width / 2.0, 0.0, 0.0);
    for i in 0..mesh.positions.len() {
        let (p, n) = (widen(mesh.positions[i]), widen(mesh.normals[i]));
        let side = n.x.abs() > 0.5;
        let top = n.y.abs() > 0.5;
        let u = if side { p.z / 2.0 } else { p.x / width };
        let v = if top { p.z / 2.0 } else { p.y };
        mesh.uvs[i] = [u as f32, v as f32];
    }
    mesh
}

/// Shells shared by every Humvee.
struct Shells {
    cab: Arc<Mesh>,
    side: Arc<Mesh>,
    rear_door_border: Arc<Mesh>,
    rear_door_panel: Arc<Mesh>,
    tire: Arc<Mesh>,
}

fn shells() -> &'static Shells {
    static SHELLS: OnceLock<Shells> = OnceLock::new();
    SHELLS.get_or_init(|| {
        let cab = profile_geometry(
            &[
                [-2.02, 0.78],
                [1.06, 0.78],
                [0.74, 1.64],
                [-1.02, 1.64],
                [-2.02, 1.08],
            ],
            1.82,
        );
        // Side panels arch over both wheels.
        let mut side_profile = vec![[-2.08, 0.3]];
        for z in [-1.32, 1.3] {
            for i in 0..=8 {
                let angle = PI - (f64::from(i) * PI) / 8.0;
                side_profile.push([z + angle.cos() * 0.57, 0.25 + angle.sin() * 0.57]);
            }
        }
        side_profile.extend([[2.06, 0.42], [2.06, 0.97], [1.04, 1.08], [-2.08, 0.98]]);
        // The rear door's lower trailing corner clears the rear wheel arch.
        let rear_door_border = profile_geometry(
            &[
                [-0.42, -0.08],
                [-0.18, -0.47],
                [0.42, -0.47],
                [0.42, 0.47],
                [-0.42, 0.47],
            ],
            0.025,
        );
        let rear_door_panel = profile_geometry(
            &[
                [-0.39, -0.065],
                [-0.155, -0.44],
                [0.39, -0.44],
                [0.39, 0.44],
                [-0.39, 0.44],
            ],
            0.025,
        );
        let tire_profile: Vec<DVec2> = [
            [0.235, -0.14],
            [0.36, -0.16],
            [0.46, -0.12],
            [0.48, -0.065],
            [0.48, 0.065],
            [0.46, 0.12],
            [0.36, 0.16],
            [0.235, 0.14],
        ]
        .iter()
        .map(|[radius, axial]| DVec2::new(*radius, *axial))
        .collect();
        let mut tire = lathe_geometry(&tire_profile, 20, 0.0, PI * 2.0);
        tire.rotate_z(PI / 2.0);
        Shells {
            cab: Arc::new(cab),
            side: Arc::new(profile_geometry(&side_profile, 0.22)),
            rear_door_border: Arc::new(rear_door_border),
            rear_door_panel: Arc::new(rear_door_panel),
            tire: Arc::new(tire),
        }
    })
}

/// Paint for one Humvee: team or burnt colors.
struct Palette {
    base: u32,
    shade: u32,
    steel: u32,
    rubber: u32,
    glass: u32,
    wreck: bool,
}

impl Palette {
    /// Lamps and lights go dark on a wreck.
    fn light(&self, color: u32) -> u32 {
        if self.wreck { self.shade } else { color }
    }
}

/// `humveeModel(team, wreck)`.
pub fn humvee_model(team: Team, wreck: bool) -> Node {
    let base = if wreck {
        WRECK_PAINT
    } else {
        TEAM_COLORS[team.index()]
    };
    let palette = Palette {
        base,
        shade: if wreck { DARK } else { shade_of(base) },
        steel: if wreck { WRECK_STEEL } else { STEEL },
        rubber: if wreck { WRECK_RUBBER } else { RUBBER },
        glass: if wreck { WRECK_GLASS } else { GLASS },
        wreck,
    };
    let mut root = Node::group(VehicleKind::Humvee.name());
    let mut hull = Node::group(part::HULL);
    hull.scale.z = HUMVEE_BODY_LENGTH_SCALE;
    let mut track_group = Node::group(part::TRACK_GROUP);
    // Wheels retain their circular section while the shell and wheelbase lengthen.
    track_group.scale.z = 1.0 / HUMVEE_BODY_LENGTH_SCALE;
    body(&mut hull, &palette);
    for side in [-1.0, 1.0] {
        body_side(&mut hull, &palette, side);
        for z in [-1.32, 1.3] {
            wheel(&mut track_group, &palette, side, z);
            wheel_arch(&mut hull, &palette, side, z);
        }
    }
    front_and_rear(&mut hull, &palette);
    hull.children.insert(0, track_group);
    root.children.push(hull);
    root.children.push(turret(&palette));
    root.scale = DVec3::splat(VehicleKind::Humvee.scale());
    let finish = if wreck {
        Finish::Wrecked
    } else {
        Finish::Fresh {
            steel: palette.steel,
        }
    };
    apply_tank_surface(
        &mut root,
        &[palette.base, palette.shade, palette.steel],
        finish,
    );
    root
}

fn body(hull: &mut Node, p: &Palette) {
    let (base, shade, steel) = (p.base, p.shade, p.steel);
    put(hull, box_part(1.4, 0.22, 3.82, shade, 0.0), 0.0, 0.42, -0.1);
    put(
        hull,
        box_part(1.65, 0.15, 3.74, base, 0.0),
        0.0,
        0.85,
        -0.05,
    );
    hull.children
        .push(shadowed(shells().cab.clone(), paint(base)));
    put(hull, box_part(1.94, 0.1, 1.84, base, 0.02), 0.0, 1.67, -0.1);
    let hood = rotated(box_part(1.9, 0.18, 1.13, base, 0.025), 0.1, 0.0, 0.0);
    put(hull, hood, 0.0, 1.0, 1.48);
    put(
        hull,
        box_part(0.95, 0.025, 0.32, shade, 0.0),
        0.0,
        1.11,
        1.18,
    );
    for i in -4..=4 {
        let slat = box_part(0.045, 0.025, 0.29, steel, 0.0);
        put(hull, slat, f64::from(i) * 0.1, 1.13, 1.18);
    }
    for x in [-0.4, 0.4] {
        let windshield = rotated(box_part(0.72, 0.49, 0.025, p.glass, 0.0), -0.356, 0.0, 0.0);
        put(hull, windshield, x, 1.36, 0.86);
        let wiper = rotated(
            box_part(0.035, 0.34, 0.018, p.rubber, 0.0),
            -0.356,
            0.0,
            -0.38,
        );
        put(hull, wiper, x + 0.07, 1.29, 0.902);
    }
}

/// Side panel, doors, windows, hinges, mirrors and lights on one side.
fn body_side(hull: &mut Node, p: &Palette, side: f64) {
    let (base, shade, steel, rubber) = (p.base, p.shade, p.steel, p.rubber);
    put(
        hull,
        shadowed(shells().side.clone(), paint(base)),
        side * 0.97,
        0.0,
        0.0,
    );
    for z in [-0.56, 0.34] {
        let rear = z < 0.0;
        let border = if rear {
            shadowed(shells().rear_door_border.clone(), paint(shade))
        } else {
            box_part(0.025, 0.94, 0.84, shade, 0.0)
        };
        put(hull, border, side * 1.09, 0.91, z);
        let panel = if rear {
            shadowed(shells().rear_door_panel.clone(), paint(base))
        } else {
            box_part(0.025, 0.88, 0.78, base, 0.0)
        };
        put(hull, panel, side * 1.105, 0.91, z);
        put(
            hull,
            box_part(0.026, 0.34, 0.62, p.glass, 0.0),
            side * 0.925,
            1.39,
            z,
        );
        put(
            hull,
            box_part(0.055, 0.055, 0.17, rubber, 0.0),
            side * 1.125,
            1.12,
            z - 0.28,
        );
        for tilt in [-0.65, 0.65] {
            let rib = box_part(0.024, 0.045, if rear { 0.56 } else { 0.72 }, shade, 0.0);
            put(hull, rotated(rib, tilt, 0.0, 0.0), side * 1.123, 0.76, z);
        }
        // Raised armored window frames and external door hinges.
        for y in [1.2, 1.58] {
            put(
                hull,
                box_part(0.055, 0.045, 0.7, shade, 0.0),
                side * 0.94,
                y,
                z,
            );
        }
        for edge in [-0.35, 0.35] {
            put(
                hull,
                box_part(0.055, 0.4, 0.045, shade, 0.0),
                side * 0.94,
                1.39,
                z + edge,
            );
        }
        for y in [0.74, 1.08] {
            put(
                hull,
                box_part(0.07, 0.07, 0.13, steel, 0.0),
                side * 1.13,
                y,
                z + 0.31,
            );
        }
    }
    put(
        hull,
        box_part(0.05, 0.05, 0.3, steel, 0.0),
        side * 1.015,
        1.26,
        0.74,
    );
    put(
        hull,
        box_part(0.09, 0.2, 0.14, shade, 0.0),
        side * 1.11,
        1.32,
        0.85,
    );
    put(
        hull,
        box_part(0.18, 0.06, 1.2, steel, 0.0),
        side * 1.01,
        0.44,
        -0.1,
    );
    put(
        hull,
        box_part(0.06, 0.16, 0.08, rubber, 0.0),
        side * 0.975,
        0.94,
        1.7,
    );
    let signal = box_part(0.16, 0.08, 0.06, p.light(TURN_SIGNAL), 0.0);
    put(hull, signal, side * 0.73, 0.99, 2.06);
    let tail = box_part(0.13, 0.16, 0.035, p.light(TAIL_LIGHT), 0.0);
    put(hull, tail, side * 0.74, 0.85, -2.09);
}

/// Tire, rim, hub, tread blocks and bolts of one wheel, in the un-stretched
/// wheel group (so positions multiply the stretch back in).
fn wheel(track_group: &mut Node, p: &Palette, side: f64, z: f64) {
    let (shade, steel, rubber) = (p.shade, p.steel, p.rubber);
    let axle_z = z * HUMVEE_BODY_LENGTH_SCALE;
    let tire = shadowed(shells().tire.clone(), material(rubber, 0.0, 0.92));
    put(track_group, tire, side, 0.25, axle_z);
    let rim = rotated(cylinder_part(0.24, 0.028, shade, 16), 0.0, 0.0, PI / 2.0);
    put(track_group, rim, side * 1.145, 0.25, axle_z);
    let hub = rotated(cylinder_part(0.13, 0.025, steel, 10), 0.0, 0.0, PI / 2.0);
    put(track_group, hub, side * 1.15, 0.25, axle_z);
    for i in 0..20 {
        let angle = (f64::from(i) * PI * 2.0) / 20.0;
        for offset in [-0.07, 0.07] {
            let yaw = if offset > 0.0 { 0.3 } else { -0.3 };
            let tread = rotated(box_part(0.115, 0.028, 0.095, rubber, 0.0), angle, yaw, 0.0);
            put(
                track_group,
                tread,
                side + offset,
                0.25 + angle.cos() * 0.476,
                axle_z + angle.sin() * 0.476,
            );
        }
        if i % 2 == 0 {
            let bolt = rotated(cylinder_part(0.016, 0.008, steel, 5), 0.0, 0.0, PI / 2.0);
            put(
                track_group,
                bolt,
                side * 1.162,
                0.25 + angle.cos() * 0.19,
                axle_z + angle.sin() * 0.19,
            );
        }
    }
}

/// Thin arch lip follows the opening instead of a rectangular shelf over the tire.
fn wheel_arch(hull: &mut Node, p: &Palette, side: f64, z: f64) {
    for i in 0..8 {
        let angle = ((f64::from(i) + 0.5) * PI) / 8.0;
        let lip = rotated(
            box_part(0.06, 0.06, 0.235, p.shade, 0.0),
            PI / 2.0 - angle,
            0.0,
            0.0,
        );
        put(
            hull,
            lip,
            side * 1.105,
            0.25 + angle.sin() * 0.59,
            z + angle.cos() * 0.59,
        );
    }
}

fn front_and_rear(hull: &mut Node, p: &Palette) {
    let (base, shade, steel, rubber) = (p.base, p.shade, p.steel, p.rubber);
    // Recessed vertical grille, circular lamps and a full-width bumper.
    put(hull, box_part(1.83, 0.34, 0.1, base, 0.0), 0.0, 0.79, 2.0);
    put(
        hull,
        box_part(0.99, 0.24, 0.025, rubber, 0.0),
        0.0,
        0.8,
        2.06,
    );
    for i in -3..=3 {
        put(
            hull,
            box_part(0.045, 0.25, 0.025, base, 0.0),
            f64::from(i) * 0.14,
            0.8,
            2.08,
        );
    }
    for x in [-0.71, 0.71] {
        let lamp = rotated(
            cylinder_part(0.115, 0.035, p.light(HEADLAMP), 12),
            PI / 2.0,
            0.0,
            0.0,
        );
        put(hull, lamp, x, 0.82, 2.07);
    }
    put(hull, box_part(2.1, 0.2, 0.18, shade, 0.02), 0.0, 0.44, 2.08);
    put(hull, box_part(1.8, 0.4, 0.12, base, 0.02), 0.0, 0.85, -2.01);
    // Slanted rear hatch, inset tail panel and bumper replace the open pickup-like tail.
    let hatch = rotated(
        box_part(1.45, 0.025, 0.9, shade, 0.0),
        -(0.56f64).atan2(1.0),
        0.0,
        0.0,
    );
    put(hull, hatch, 0.0, 1.375, -1.55);
    put(
        hull,
        box_part(1.38, 0.25, 0.025, shade, 0.0),
        0.0,
        0.79,
        -2.08,
    );
    put(
        hull,
        box_part(2.08, 0.14, 0.14, shade, 0.0),
        0.0,
        0.38,
        -2.08,
    );
    for side in [-1.0, 1.0] {
        let lamp = rotated(
            cylinder_part(0.07, 0.03, p.light(REAR_LAMP), 10),
            PI / 2.0,
            0.0,
            0.0,
        );
        put(hull, lamp, side * 0.79, 0.83, -2.1);
        put(
            hull,
            box_part(0.27, 0.3, 0.035, rubber, 0.0),
            side,
            0.22,
            -1.91,
        );
    }
    // Raised intake, tie-downs and recovery eyes stay inside the existing hull footprint.
    put(hull, cylinder_part(0.065, 0.65, shade, 8), -0.82, 1.36, 0.9);
    put(
        hull,
        cylinder_part(0.095, 0.06, rubber, 8),
        -0.82,
        1.71,
        0.9,
    );
    for x in [-0.7, 0.7] {
        put(hull, box_part(0.08, 0.1, 0.08, steel, 0.0), x, 0.49, 2.12);
        put(hull, box_part(0.08, 0.06, 0.22, steel, 0.0), x, 1.75, -0.7);
    }
    put(
        hull,
        cylinder_part(0.018, 0.78, steel, 6),
        -0.82,
        1.98,
        -1.92,
    );
}

/// Roof ring, launcher tube with collars and a sight box; the empty `muzzle`
/// marker sits at the tube mouth.
fn turret(p: &Palette) -> Node {
    let (base, shade, steel) = (p.base, p.shade, p.steel);
    let mut turret = Node::group(part::TURRET);
    let mut barrel = Node::group(part::BARREL);
    put(
        &mut turret,
        cylinder_part(0.44, 0.1, shade, 16),
        0.0,
        1.78,
        0.0,
    );
    put(
        &mut turret,
        box_part(0.22, 0.27, 0.36, steel, 0.02),
        0.0,
        1.93,
        0.0,
    );
    let tube = rotated(cylinder_part(0.15, 1.5, shade, 12), PI / 2.0, 0.0, 0.0);
    put(&mut barrel, tube, 0.0, LAUNCHER_Y, 0.42);
    for z in [-0.3, 1.14] {
        let collar = rotated(cylinder_part(0.18, 0.1, steel, 12), PI / 2.0, 0.0, 0.0);
        put(&mut barrel, collar, 0.0, LAUNCHER_Y, z);
    }
    let opening = rotated(cylinder_part(0.13, 0.012, p.rubber, 12), PI / 2.0, 0.0, 0.0);
    put(&mut barrel, opening, 0.0, LAUNCHER_Y, 1.196);
    put(
        &mut turret,
        box_part(0.36, 0.35, 0.55, shade, 0.025),
        0.36,
        LAUNCHER_Y,
        0.32,
    );
    for x in [0.28, 0.44] {
        let lens = rotated(cylinder_part(0.067, 0.025, p.glass, 12), PI / 2.0, 0.0, 0.0);
        put(&mut turret, lens, x, LAUNCHER_Y, 0.61);
    }
    for side in [-1.0, 1.0] {
        put(
            &mut turret,
            box_part(0.065, 0.32, 0.75, base, 0.0),
            side * 0.52,
            1.96,
            -0.13,
        );
    }
    put(
        &mut turret,
        box_part(1.05, 0.32, 0.06, shade, 0.0),
        0.0,
        1.96,
        -0.5,
    );
    put(&mut barrel, Node::group(part::MUZZLE), 0.0, LAUNCHER_Y, 1.7);
    turret.children.push(barrel);
    turret
}
