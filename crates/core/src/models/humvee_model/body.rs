//! The up-armored body in the hull's frame (before its length stretch): the low
//! tub with trapezoid wheel openings, the wide hood with its raised centre, the
//! four-door armored cab and slanted cargo shell, and the fittings that make it
//! read as a HMMWV up close. Parts on one side are built for +x and mirrored.
//!
//! The hull's measured bounds drive hit boxes, so the extremes stay where the
//! simulation expects them: the front brush guard ends at `FRONT`, the rear
//! bumper at `REAR`, and the antenna whips top out at `ANTENNA_TOP`.

use std::f64::consts::PI;

use glam::{DMat4, DVec3};

use super::kit::{Kit, Role, at, frame, mirror_x, place, rect, slab, slope_y};
use super::{AXLE_Y, FRONT_AXLE, REAR_AXLE};

/// Hull-frame extremes measured by `tank_dimensions`.
const FRONT: f64 = 2.17;
const REAR: f64 = -2.15;
const ANTENNA_TOP: f64 = 2.37;
/// Tub sides and bottom.
const TUB_SIDE: f64 = 1.03;
const SILL: f64 = 0.36;
/// Hood: cowl station, front edge and the drop of its top towards the grille.
const COWL: f64 = 0.86;
const HOOD_FRONT: f64 = 2.02;
const HOOD_SLOPE: f64 = -0.045;
const HOOD_CROWN: f64 = 1.12;
const FENDER_TOP: f64 = 1.07;
/// Cab roof and the slantback's start.
const ROOF: f64 = 1.70;
const ROOF_REAR: f64 = -1.08;
const TAIL: f64 = -2.02;
/// Armored doors stand proud of the tub.
const DOOR_IN: f64 = 1.035;
const DOOR_OUT: f64 = 1.095;

/// The trapezoid wheel opening around an axle, as (z, y) from rear-bottom to
/// front-bottom.
fn wheel_opening(axle: f64) -> [[f64; 2]; 6] {
    [
        [axle - 0.53, SILL],
        [axle - 0.38, 0.82],
        [axle - 0.26, 0.90],
        [axle + 0.26, 0.90],
        [axle + 0.38, 0.82],
        [axle + 0.53, SILL],
    ]
}

/// Height of the hood top above `y_at_cowl` at station `z`.
fn hood_y(y_at_cowl: f64, z: f64) -> f64 {
    y_at_cowl + HOOD_SLOPE * (z - COWL)
}

pub(super) fn body_kit() -> Kit {
    let mut kit = Kit::default();
    underbody(&mut kit);
    tub_and_cab(&mut kit);
    hood(&mut kit);
    front(&mut kit);
    windshield(&mut kit);
    for side in [DMat4::IDENTITY, mirror_x()] {
        doors(&mut kit, side);
        flank(&mut kit, side);
    }
    rear(&mut kit);
    // Fuel filler on the right rear quarter.
    let filler = place([-TUB_SIDE - 0.03, 1.12, -1.3], [0.0, 0.0, PI / 2.0]);
    kit.cylinder(Role::Steel, 0.06, 0.03, 12, filler);
    kit
}

fn underbody(kit: &mut Kit) {
    kit.block(Role::Shade, [1.4, 0.18, 3.7], at([0.0, 0.33, 0.0]));
    for axle in [FRONT_AXLE, REAR_AXLE] {
        kit.chamfered(Role::Shade, [0.44, 0.24, 0.3], 0.03, at([0.0, 0.25, axle]));
        // Half shafts and the A-arms of the independent suspension.
        let shaft = place([0.0, AXLE_Y, axle], [0.0, 0.0, PI / 2.0]);
        kit.cylinder(Role::Steel, 0.045, 1.7, 8, shaft);
        for (y, tilt) in [(0.14, 0.12), (0.4, -0.18)] {
            for x in [0.62, -0.62] {
                let arm = place([x, y, axle], [0.0, 0.0, tilt * x.signum()]);
                kit.block(Role::Shade, [0.4, 0.05, 0.16], arm);
            }
        }
    }
}

fn tub_and_cab(kit: &mut Kit) {
    let mut tub = vec![[TAIL, SILL]];
    tub.extend(wheel_opening(REAR_AXLE));
    tub.extend(wheel_opening(FRONT_AXLE));
    tub.extend([[1.9, 0.4], [1.9, 0.98], [TAIL, 0.98]]);
    kit.side_profile(Role::Paint, &tub, &[], -TUB_SIDE, TUB_SIDE, 0.02);
    // Dark liners line the wheel wells.
    for axle in [FRONT_AXLE, REAR_AXLE] {
        let edge = wheel_opening(axle);
        let inset = edge.map(|[z, y]| {
            let toward = glam::DVec2::new(axle - z, AXLE_Y - y).normalize();
            [z + toward.x * 0.025, y + toward.y * 0.025]
        });
        let liner: Vec<[f64; 2]> = edge.iter().chain(inset.iter().rev()).copied().collect();
        kit.side_profile(
            Role::Gap,
            &liner,
            &[],
            -TUB_SIDE + 0.01,
            TUB_SIDE - 0.01,
            0.0,
        );
    }
    // Cab with the windshield slope and the slantback cargo shell.
    let cab = [
        [0.88, 0.96],
        [0.88, 1.1],
        [0.7, 1.64],
        [0.65, ROOF],
        [ROOF_REAR, ROOF],
        [-1.96, 1.18],
        [TAIL, 1.1],
        [TAIL, 0.96],
    ];
    kit.side_profile(Role::Paint, &cab, &[], -TUB_SIDE, TUB_SIDE, 0.02);
    // Roof plate overhangs the cab a little, like the armor kit's roof.
    kit.chamfered(
        Role::Paint,
        [2.1, 0.04, 1.78],
        0.012,
        at([0.0, ROOF + 0.02, -0.22]),
    );
    // Turret ring base: the ring itself turns with the turret.
    kit.block(Role::Shade, [1.3, 0.03, 1.15], at([0.0, ROOF + 0.045, 0.0]));
}

fn hood(kit: &mut Kit) {
    // Front view of the hood: flat fender tops, a raised centre and fender skirts
    // that hang over the wheel openings.
    let section = [
        [-1.08, 0.84],
        [1.08, 0.84],
        [1.08, 1.03],
        [1.045, FENDER_TOP],
        [0.66, FENDER_TOP],
        [0.56, HOOD_CROWN],
        [-0.56, HOOD_CROWN],
        [-0.66, FENDER_TOP],
        [-1.045, FENDER_TOP],
        [-1.08, 1.03],
    ];
    kit.section(
        Role::Paint,
        &section,
        COWL,
        HOOD_FRONT,
        0.018,
        slope_y(HOOD_SLOPE, COWL),
    );
    // Louvred vents on each fender top and the crown's hinge line.
    for side in [DMat4::IDENTITY, mirror_x()] {
        let z = 1.2;
        let y = hood_y(FENDER_TOP, z);
        let vent = side * place([0.86, y, z], [-HOOD_SLOPE, 0.0, 0.0]);
        kit.block(Role::Gap, [0.28, 0.01, 0.34], vent);
        for i in 0..6 {
            let slat = vent * place([0.0, 0.012, -0.14 + f64::from(i) * 0.056], [0.5, 0.0, 0.0]);
            kit.block(Role::Paint, [0.27, 0.012, 0.04], slat);
        }
        // Rubber hood latch at the front of each fender.
        let latch = side * at([1.088, 0.955, 1.86]);
        kit.block(Role::Gap, [0.016, 0.09, 0.035], latch);
        kit.block(
            Role::Gap,
            [0.02, 0.025, 0.07],
            latch * at([0.0, -0.045, 0.0]),
        );
        // Lifting eye by the windshield.
        let eye = side
            * place(
                [0.96, hood_y(FENDER_TOP, 0.95) + 0.03, 0.95],
                [0.0, PI / 2.0, 0.0],
            );
        kit.torus(Role::Steel, 0.03, 0.009, eye);
    }
    // Engine air intake beside the windshield on the right, with its grille.
    let intake = at([-0.82, 1.15, 0.98]);
    kit.chamfered(Role::Paint, [0.3, 0.16, 0.2], 0.015, intake);
    kit.block(Role::Gap, [0.24, 0.1, 0.01], intake * at([0.0, 0.0, 0.1]));
    for i in 0..5 {
        let slat = intake * at([0.0, -0.04 + f64::from(i) * 0.02, 0.105]);
        kit.block(Role::Paint, [0.24, 0.008, 0.012], slat);
    }
    // Blackout drive light on the left fender.
    let blackout = at([0.62, hood_y(FENDER_TOP, 1.9) + 0.04, 1.9]);
    kit.chamfered(Role::Gap, [0.1, 0.08, 0.08], 0.01, blackout);
    kit.block(
        Role::Amber,
        [0.06, 0.02, 0.01],
        blackout * at([0.0, 0.0, 0.042]),
    );
}

fn front(kit: &mut Kit) {
    // Grille panel between the fenders: vertical slots and square headlamp
    // recesses, backed by a dark radiator face.
    let slots: Vec<[[f64; 2]; 4]> = (-4..=4)
        .map(|i| rect(f64::from(i) * 0.108, 0.665, 0.058, 0.3))
        .collect();
    let lamps = [rect(-0.8, 0.67, 0.26, 0.24), rect(0.8, 0.67, 0.26, 0.24)];
    let holes: Vec<&[[f64; 2]]> = slots.iter().chain(&lamps).map(|h| h.as_slice()).collect();
    let panel = slab(&rect(0.0, 0.65, 2.06, 0.46), &holes, 0.05, 0.008);
    kit.add(Role::Paint, &panel, at([0.0, 0.0, 1.93]));
    kit.block(Role::Gap, [2.0, 0.42, 0.01], at([0.0, 0.65, 1.92]));
    for x in [-0.8, 0.8] {
        let lamp = place([x, 0.67, 1.945], [PI / 2.0, 0.0, 0.0]);
        kit.cylinder(Role::Steel, 0.105, 0.025, 16, lamp);
        kit.cylinder(Role::Headlamp, 0.085, 0.04, 16, lamp);
        // Composite marker and turn lamps on the fender corners above.
        let marker = at([x * 1.1, hood_y(FENDER_TOP, HOOD_FRONT) + 0.03, 1.97]);
        kit.chamfered(Role::Gap, [0.15, 0.065, 0.08], 0.01, marker);
        kit.block(
            Role::Amber,
            [0.11, 0.04, 0.01],
            marker * at([0.0, 0.0, 0.041]),
        );
    }
    // Heavy bumper with recovery shackles and a tubular brush guard whose front
    // face is the hull's foremost point.
    kit.chamfered(Role::Shade, [2.1, 0.2, 0.13], 0.015, at([0.0, 0.44, 2.055]));
    for x in [-0.66, 0.66] {
        kit.block(Role::Shade, [0.12, 0.13, 0.05], at([x, 0.44, 2.13]));
        kit.torus(
            Role::Steel,
            0.042,
            0.012,
            place([x, 0.4, 2.105], [0.0, PI / 2.0, 0.0]),
        );
    }
    let guard_z = FRONT - 0.025;
    for x in [-0.56, 0.56] {
        kit.block(Role::Shade, [0.05, 0.5, 0.05], at([x, 0.79, guard_z]));
    }
    kit.block(Role::Shade, [1.17, 0.05, 0.05], at([0.0, 1.015, guard_z]));
    kit.block(
        Role::Shade,
        [1.17, 0.04, 0.04],
        at([0.0, 0.6, guard_z - 0.005]),
    );
    for x in [-0.33, -0.11, 0.11, 0.33] {
        kit.block(Role::Shade, [0.03, 0.4, 0.03], at([x, 0.8, guard_z - 0.01]));
    }
}

/// Two thick armored panes in heavy frames on the sloped cab front.
fn windshield(kit: &mut Kit) {
    let base = DVec3::new(0.0, 1.1, 0.88);
    let up = (DVec3::new(0.0, 1.64, 0.7) - base).normalize();
    let out = DVec3::X.cross(up);
    let length = (DVec3::new(0.0, 1.64, 0.7) - base).length();
    let to_glass = frame(DVec3::X, up, out, base);
    let panes = [
        rect(-0.49, length * 0.5, 0.82, length - 0.14),
        rect(0.49, length * 0.5, 0.82, length - 0.14),
    ];
    let holes: Vec<&[[f64; 2]]> = panes.iter().map(|p| p.as_slice()).collect();
    let surround = slab(&rect(0.0, length * 0.5, 2.04, length), &holes, 0.05, 0.012);
    kit.add(Role::Paint, &surround, to_glass);
    for x in [-0.49, 0.49] {
        let pane = to_glass * at([x, length * 0.5, 0.012]);
        kit.block(Role::Glass, [0.84, length - 0.12, 0.012], pane);
        // Wiper parked along the bottom of each pane.
        let wiper = to_glass * place([x - 0.05, 0.11, 0.035], [0.0, 0.0, 1.38]);
        kit.block(Role::Gap, [0.018, 0.5, 0.012], wiper);
        let arm = to_glass * place([x + 0.2, 0.06, 0.04], [PI / 2.0, 0.0, 0.0]);
        kit.cylinder(Role::Gap, 0.018, 0.02, 6, arm);
    }
}

/// Armored doors with small thick windows, hinges, handles and raised lower
/// plates, on the +x side (the caller mirrors).
fn doors(kit: &mut Kit, side: DMat4) {
    let front_door = [
        [0.02, 0.42],
        [0.8, 0.42],
        [0.8, 1.12],
        [0.66, 1.62],
        [0.02, 1.62],
    ];
    let front_window = [[0.1, 1.18], [0.66, 1.18], [0.56, 1.54], [0.1, 1.54]];
    let rear_door = [
        [-0.02, 0.42],
        [-0.74, 0.42],
        [-0.88, 0.84],
        [-0.88, 1.62],
        [-0.02, 1.62],
    ];
    let rear_window = [[-0.1, 1.18], [-0.8, 1.18], [-0.8, 1.54], [-0.1, 1.54]];
    let mut doors = Kit::default();
    for (outline, window, leading, trailing, plate) in [
        (
            front_door.as_slice(),
            front_window.as_slice(),
            0.8,
            0.02,
            rect(0.41, 0.8, 0.62, 0.52),
        ),
        (
            rear_door.as_slice(),
            rear_window.as_slice(),
            -0.02,
            -0.86,
            rect(-0.4, 0.86, 0.58, 0.42),
        ),
    ] {
        doors.side_profile(Role::Paint, outline, &[window], DOOR_IN, DOOR_OUT, 0.012);
        // Heavy frame bolted around the armored glass.
        let center = window
            .iter()
            .fold([0.0, 0.0], |c, p| [c[0] + p[0] / 4.0, c[1] + p[1] / 4.0]);
        let frame_outline: Vec<[f64; 2]> = window
            .iter()
            .map(|[z, y]| {
                let (dz, dy) = (z - center[0], y - center[1]);
                [z + 0.045 * dz.signum(), y + 0.045 * dy.signum()]
            })
            .collect();
        doors.side_profile(
            Role::Paint,
            &frame_outline,
            &[window],
            DOOR_OUT,
            DOOR_OUT + 0.014,
            0.005,
        );
        let z0 = window.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        let z1 = window
            .iter()
            .map(|p| p[0])
            .fold(f64::NEG_INFINITY, f64::max);
        doors.block(
            Role::Glass,
            [0.012, 0.37, z1 - z0 + 0.02],
            at([DOOR_IN + 0.03, 1.36, (z0 + z1) / 2.0]),
        );
        doors.side_profile(Role::Paint, &plate, &[], DOOR_OUT, DOOR_OUT + 0.016, 0.006);
        // Hinges on the leading edge, the lever handle by the trailing edge.
        for y in [0.66, 1.06, 1.46] {
            doors.block(Role::Steel, [0.04, 0.1, 0.07], at([DOOR_OUT, y, leading]));
        }
        let handle_z = trailing + (leading - trailing).signum() * 0.14;
        doors.block(
            Role::Steel,
            [0.03, 0.05, 0.05],
            at([DOOR_OUT + 0.01, 1.1, handle_z]),
        );
        doors.block(
            Role::Steel,
            [0.022, 0.03, 0.17],
            at([
                DOOR_OUT + 0.035,
                1.1,
                handle_z - 0.04 * (leading - trailing).signum(),
            ]),
        );
    }
    // The B-pillar shows as a dark seam between the doors.
    doors.block(Role::Gap, [0.01, 1.2, 0.035], at([DOOR_IN, 1.02, 0.0]));
    kit.absorb(&doors, side);
}

/// Rocker, fender trim, quarter armor, mirror and cargo-shell fittings on +x.
fn flank(kit: &mut Kit, side: DMat4) {
    let mut flank = Kit::default();
    flank.block(Role::Shade, [0.06, 0.12, 1.5], at([1.06, 0.36, -0.01]));
    // Trim lips around both wheel openings.
    for axle in [FRONT_AXLE, REAR_AXLE] {
        let inner = wheel_opening(axle);
        let outer = [
            [axle - 0.6, SILL],
            [axle - 0.43, 0.88],
            [axle - 0.29, 0.97],
            [axle + 0.29, 0.97],
            [axle + 0.43, 0.88],
            [axle + 0.6, SILL],
        ];
        let lip: Vec<[f64; 2]> = inner.iter().chain(outer.iter().rev()).copied().collect();
        flank.side_profile(Role::Shade, &lip, &[], TUB_SIDE - 0.01, 1.078, 0.01);
    }
    // Quarter armor above the rear wheel, following the slantback.
    let quarter = [
        [-0.92, 0.98],
        [-0.92, 1.64],
        [ROOF_REAR - 0.02, 1.64],
        [-1.9, 1.16],
        [-1.98, 1.08],
        [-1.98, 0.98],
    ];
    flank.side_profile(Role::Paint, &quarter, &[], TUB_SIDE, TUB_SIDE + 0.03, 0.01);
    // Mirror on an arm from the A-pillar.
    flank.block(Role::Steel, [0.11, 0.025, 0.025], at([1.09, 1.22, 0.84]));
    let mirror = at([1.135, 1.33, 0.86]);
    flank.chamfered(Role::Shade, [0.04, 0.26, 0.15], 0.008, mirror);
    flank.block(
        Role::Glass,
        [0.03, 0.22, 0.006],
        mirror * at([0.0, 0.0, -0.077]),
    );
    // Side marker reflectors: amber at the front fender, red at the tail.
    flank.block(Role::Amber, [0.01, 0.05, 0.1], at([1.083, 0.96, 1.78]));
    flank.block(
        Role::Red,
        [0.01, 0.05, 0.1],
        at([TUB_SIDE + 0.033, 0.88, -1.93]),
    );
    // Mud flap behind the rear wheel.
    flank.block(Role::Rubber, [0.26, 0.3, 0.02], at([0.98, 0.22, -1.9]));
    // Antenna mount at the rear corner: bracket, spring base and whip.
    let deck_y = 1.2;
    flank.chamfered(
        Role::Shade,
        [0.1, 0.1, 0.12],
        0.01,
        at([0.82, deck_y + 0.04, -1.9]),
    );
    flank.cylinder(Role::Gap, 0.032, 0.34, 8, at([0.82, deck_y + 0.25, -1.92]));
    let whip_height = 0.78;
    let whip = at([0.82, ANTENNA_TOP - whip_height / 2.0, -1.92]);
    flank.cylinder(Role::Gap, 0.011, whip_height, 6, whip);
    // Step below the rear door and grab handle on the cargo shell.
    flank.block(Role::Steel, [0.03, 0.03, 0.26], at([1.07, 1.3, -1.05]));
    kit.absorb(&flank, side);
}

/// Tail panel, lamps, jerrycan rack, stowage on the slantback, bumper and pintle.
fn rear(kit: &mut Kit) {
    // Slantback hatch panel with its hinge.
    let top = DVec3::new(0.0, ROOF, ROOF_REAR);
    let down = (DVec3::new(0.0, 1.18, -1.96) - top).normalize();
    let out = DVec3::X.cross(down);
    let hatch = frame(DVec3::X, down, out, top);
    let length = (DVec3::new(0.0, 1.18, -1.96) - top).length();
    kit.add(
        Role::Paint,
        &slab(
            &rect(0.0, length * 0.5, 1.7, length - 0.1),
            &[],
            0.02,
            0.006,
        ),
        hatch,
    );
    kit.cylinder(
        Role::Steel,
        0.025,
        1.5,
        8,
        hatch * place([0.0, 0.06, 0.03], [0.0, 0.0, PI / 2.0]),
    );
    // Stowage rack on the hatch: rails, canvas bags and an ammo can.
    for x in [-0.72, 0.72] {
        kit.block(
            Role::Steel,
            [0.03, length - 0.3, 0.03],
            hatch * at([x, length * 0.5, 0.035]),
        );
    }
    kit.block(
        Role::Steel,
        [1.47, 0.03, 0.03],
        hatch * at([0.0, length - 0.18, 0.035]),
    );
    kit.rounded(
        Role::Canvas,
        [0.6, 0.42, 0.18],
        0.06,
        hatch * place([0.33, length * 0.46, 0.11], [0.0, 0.0, 0.05]),
    );
    kit.rounded(
        Role::Canvas,
        [0.42, 0.34, 0.15],
        0.05,
        hatch * place([-0.2, length * 0.52, 0.1], [0.0, 0.0, -0.1]),
    );
    kit.chamfered(
        Role::Shade,
        [0.26, 0.16, 0.16],
        0.012,
        hatch * at([-0.56, length * 0.4, 0.1]),
    );
    // Tail lamps: composite red and amber lenses in dark housings.
    for x in [-0.8, 0.8] {
        let housing = at([x, 0.74, TAIL - 0.015]);
        kit.chamfered(Role::Gap, [0.17, 0.22, 0.03], 0.006, housing);
        kit.block(
            Role::Red,
            [0.13, 0.08, 0.008],
            housing * at([0.0, 0.05, -0.017]),
        );
        kit.block(
            Role::Amber,
            [0.13, 0.05, 0.008],
            housing * at([0.0, -0.04, -0.017]),
        );
        kit.block(
            Role::Red,
            [0.05, 0.03, 0.008],
            housing * at([0.0, -0.085, -0.017]),
        );
    }
    // Jerrycans in a rack between the lamps.
    for x in [-0.27, 0.27] {
        let can = at([x, 0.72, TAIL - 0.067]);
        kit.rounded(Role::Shade, [0.32, 0.46, 0.11], 0.025, can);
        for dx in [-0.08, 0.0, 0.08] {
            kit.block(Role::Shade, [0.025, 0.04, 0.05], can * at([dx, 0.24, 0.0]));
        }
        kit.cylinder(Role::Steel, 0.025, 0.05, 8, can * at([0.11, 0.24, 0.0]));
        kit.block(Role::Gap, [0.36, 0.035, 0.122], can * at([0.0, 0.06, 0.0]));
    }
    kit.block(
        Role::Steel,
        [0.76, 0.03, 0.13],
        at([0.0, 0.475, TAIL - 0.06]),
    );
    // Rear bumper: its back face is the hull's rearmost point.
    kit.block(
        Role::Shade,
        [2.08, 0.14, 0.14],
        at([0.0, 0.38, REAR + 0.07]),
    );
    kit.chamfered(
        Role::Steel,
        [0.14, 0.1, 0.1],
        0.01,
        at([0.0, 0.28, REAR + 0.06]),
    );
    kit.torus(
        Role::Steel,
        0.04,
        0.012,
        place([0.0, 0.22, REAR + 0.06], [0.0, PI / 2.0, 0.0]),
    );
    for x in [-0.88, 0.88] {
        kit.torus(
            Role::Steel,
            0.035,
            0.011,
            place([x, 0.3, REAR + 0.05], [0.0, PI / 2.0, 0.0]),
        );
    }
}
