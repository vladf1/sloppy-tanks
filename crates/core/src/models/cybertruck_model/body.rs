//! The body in the hull's frame, ground at y = 0, nose towards +z. Two pieces of
//! stainless make the shape: the lower body, a side profile cut by angular wheel
//! openings and extruded across the truck, and above its beltline the upper body,
//! lofted through cross-sections whose sides lean in (one flat plane carries each
//! side window and sail panel) and whose top is the famous straight line from the
//! nose to the apex and down to the tailgate. On them sit the glass, the light
//! bars, black cladding, flares, bumpers and the fittings that read up close.
//! Parts on one side are built for +x and mirrored.

use std::f64::consts::PI;

use glam::{DMat4, DVec3};

use super::super::parts_kit::{Faces, Kit, Role, at, frame, mirror_x, place, slab};
use super::{AXLES, HALF_WIDTH, NOSE, TAIL};

/// The beltline, and the body's lowest steel edge over the bumpers and between
/// the wheel openings.
const BELT: f64 = 1.12;
const BUMPER_TOP: f64 = 0.62;
const ROCKER: f64 = 0.5;
/// The roof apex (z, y), and the tops of the nose and the tailgate.
const APEX: [f64; 2] = [0.25, 1.79];
const NOSE_TOP: f64 = 1.0;
const TAIL_TOP: f64 = 1.25;
/// How far the upper sides lean in per unit of height above the beltline.
const TUMBLEHOME: f64 = 0.4;
/// Where the windshield meets the hood, and where the rear glass meets the tonneau.
const WINDSHIELD_BASE: f64 = 1.6;
const REAR_GLASS_END: f64 = -0.98;
/// Steel left beside the tonneau cover on each side.
const SAIL: f64 = 0.14;
/// Glass, seams and trim lie this far off the surface they cover.
const SKIN: f64 = 0.004;
/// Edge chamfer on the stainless, which catches the light along every crease.
const CHAMFER: f64 = 0.02;

/// The roof line's height at station `z`: one straight line from the nose to the
/// apex, another from the apex down to the tailgate.
fn roof(z: f64) -> f64 {
    if z >= APEX[0] {
        APEX[1] - (APEX[1] - NOSE_TOP) * (z - APEX[0]) / (NOSE - APEX[0])
    } else {
        APEX[1] - (APEX[1] - TAIL_TOP) * (APEX[0] - z) / (APEX[0] - TAIL)
    }
}

/// Half-width of the upper body at height `y`: the leaning sides.
fn lean(y: f64) -> f64 {
    HALF_WIDTH - TUMBLEHOME * (y - BELT).max(0.0)
}

/// A point on the right upper side at station `z` and height `y`, `off` out from it.
fn on_side(z: f64, y: f64, off: f64) -> DVec3 {
    DVec3::new(lean(y), y, z) + side_normal() * off
}

fn side_normal() -> DVec3 {
    DVec3::new(1.0, TUMBLEHOME, 0.0).normalize()
}

/// A point on the roof line at station `z`, `x` across, `off` out from it.
fn on_roof(z: f64, x: f64, off: f64) -> DVec3 {
    DVec3::new(x, roof(z), z) + roof_normal(z) * off
}

fn roof_normal(z: f64) -> DVec3 {
    let (z0, z1) = if z >= APEX[0] {
        (APEX[0], NOSE)
    } else {
        (TAIL, APEX[0])
    };
    let slope = (roof(z1 - 1e-6) - roof(z0 + 1e-6)) / (z1 - z0);
    DVec3::new(0.0, 1.0, -slope).normalize()
}

/// The wheel opening hugs the tire as on the production truck, about 0.1 m clear
/// all round: half-widths along the truck at its bottom, at its shoulder and along
/// its flat top, and the heights of the shoulder and the top.
const OPENING_BOTTOM: f64 = 0.56;
const OPENING_SHOULDER: [f64; 2] = [0.5, 0.7];
const OPENING_TOP: [f64; 2] = [0.3, 0.95];

/// The angular wheel opening around an axle, as (z, y) from rear-bottom to
/// front-bottom, `grow` larger all round; its bottoms meet the steel edges either
/// side.
fn wheel_opening(axle: f64, grow: f64, rear_bottom: f64, front_bottom: f64) -> [[f64; 2]; 6] {
    let [shoulder, shoulder_y] = OPENING_SHOULDER;
    let [top, top_y] = OPENING_TOP;
    [
        [axle - OPENING_BOTTOM - grow, rear_bottom],
        [axle - shoulder - grow * 0.95, shoulder_y + grow * 0.4],
        [axle - top - grow * 0.4, top_y + grow],
        [axle + top + grow * 0.4, top_y + grow],
        [axle + shoulder + grow * 0.95, shoulder_y + grow * 0.4],
        [axle + OPENING_BOTTOM + grow, front_bottom],
    ]
}

/// The bottom of the steel either side of each opening (rear, front).
fn opening_bottoms(axle: f64) -> (f64, f64) {
    if axle < 0.0 {
        (BUMPER_TOP, ROCKER)
    } else {
        (ROCKER, BUMPER_TOP)
    }
}

pub(super) fn body_kit() -> Kit {
    let mut kit = Kit::default();
    lower_body(&mut kit);
    upper_body(&mut kit);
    glass(&mut kit);
    for side in [DMat4::IDENTITY, mirror_x()] {
        flank(&mut kit, side);
        wheel_wells(&mut kit, side);
    }
    front(&mut kit);
    rear(&mut kit);
    tonneau(&mut kit);
    underbody(&mut kit);
    kit
}

/// The side profile below the beltline, cut by both wheel openings, extruded
/// across the truck with chamfered edges. In front of the windshield its top is
/// the hood, rising along the roof line until it reaches the beltline.
fn lower_body(kit: &mut Kit) {
    let hood_meets_belt = NOSE - (BELT - NOSE_TOP) * (NOSE - APEX[0]) / (APEX[1] - NOSE_TOP);
    let mut outline = vec![[TAIL, BUMPER_TOP]];
    for axle in AXLES {
        let (rear, front) = opening_bottoms(axle);
        outline.extend(wheel_opening(axle, 0.0, rear, front));
    }
    // The front fascia rakes back a little below the light bar.
    outline.extend([
        [NOSE - 0.06, BUMPER_TOP],
        [NOSE, NOSE_TOP],
        [hood_meets_belt, BELT],
        [TAIL, BELT],
    ]);
    kit.side_profile(Role::Paint, &outline, &[], -HALF_WIDTH, HALF_WIDTH, CHAMFER);
}

/// The upper body from the hood (where the roof line passes the beltline) to the
/// tailgate: lofted between stations at the slope changes, its sides leaning in.
fn upper_body(kit: &mut Kit) {
    let hood_meets_belt = NOSE - (BELT - NOSE_TOP) * (NOSE - APEX[0]) / (APEX[1] - NOSE_TOP);
    let stations = [
        hood_meets_belt,
        WINDSHIELD_BASE,
        APEX[0],
        REAR_GLASS_END,
        TAIL,
    ];
    let mut faces = Faces::default();
    let top = |z: f64, x: f64| DVec3::new(x * lean(roof(z)), roof(z), z);
    let belt = |z: f64, x: f64| DVec3::new(x * HALF_WIDTH, BELT, z);
    for pair in stations.windows(2) {
        let (z0, z1) = (pair[0], pair[1]);
        faces.polygon(
            &[top(z0, 1.0), top(z0, -1.0), top(z1, -1.0), top(z1, 1.0)],
            DVec3::Y,
        );
        for x in [1.0, -1.0] {
            faces.polygon(
                &[belt(z0, x), top(z0, x), top(z1, x), belt(z1, x)],
                DVec3::new(x, TUMBLEHOME, 0.0),
            );
        }
    }
    // The tailgate's top above the beltline.
    faces.polygon(
        &[
            belt(TAIL, 1.0),
            top(TAIL, 1.0),
            top(TAIL, -1.0),
            belt(TAIL, -1.0),
        ],
        -DVec3::Z,
    );
    kit.add(Role::Paint, &faces.mesh(), DMat4::IDENTITY);
}

/// Tinted glass: the windshield and rear window on the roof line, the side windows
/// on the leaning sides, with the black B-pillar and the dark seam over the apex.
fn glass(kit: &mut Kit) {
    let mut glass = Faces::default();
    let mut trim = Faces::default();
    let pane = |z0: f64, z1: f64, inset: f64| {
        let corner = |z: f64, x: f64| on_roof(z, x * (lean(roof(z)) - inset), SKIN);
        [
            corner(z0, 1.0),
            corner(z0, -1.0),
            corner(z1, -1.0),
            corner(z1, 1.0),
        ]
    };
    glass.polygon(&pane(WINDSHIELD_BASE, APEX[0] + 0.1, 0.075), DVec3::Y);
    glass.polygon(&pane(APEX[0] - 0.1, REAR_GLASS_END + 0.04, 0.08), DVec3::Y);
    // The ridge between the panes.
    trim.polygon(&pane(APEX[0] + 0.1, APEX[0] - 0.1, 0.02), DVec3::Y);
    // The wiper parks along the windshield's foot.
    let foot = on_roof(WINDSHIELD_BASE - 0.05, 0.0, SKIN + 0.01);
    let wiper_frame = frame(
        DVec3::X,
        roof_normal(WINDSHIELD_BASE),
        roof_normal(WINDSHIELD_BASE).cross(DVec3::X),
        foot,
    );
    kit.block(
        Role::Gap,
        [1.35, 0.018, 0.03],
        wiper_frame * at([-0.15, 0.0, 0.0]),
    );

    // Side windows: the front and rear doors' glass under the roof line.
    let window: [[f64; 2]; 5] = [
        [1.5, BELT + 0.05],
        [1.32, roof(1.32) - 0.08],
        [APEX[0], APEX[1] - 0.075],
        [-1.06, roof(-1.06) - 0.07],
        [-1.06, BELT + 0.05],
    ];
    for x in [1.0, -1.0] {
        let mirrored = |p: DVec3| DVec3::new(p.x * x, p.y, p.z);
        let outward = DVec3::new(x, TUMBLEHOME, 0.0);
        let points: Vec<DVec3> = window
            .iter()
            .map(|&[z, y]| mirrored(on_side(z, y, SKIN)))
            .collect();
        glass.polygon(&points, outward);
        // The B-pillar between the doors' windows.
        let pillar: Vec<DVec3> = [
            [0.08, BELT + 0.04],
            [0.08, roof(0.08) - 0.06],
            [-0.02, roof(-0.02) - 0.06],
            [-0.02, BELT + 0.04],
        ]
        .iter()
        .map(|&[z, y]| mirrored(on_side(z, y, 2.0 * SKIN)))
        .collect();
        trim.polygon(&pillar, outward);
    }
    kit.add(Role::Glass, &glass.mesh(), DMat4::IDENTITY);
    kit.add(Role::Gap, &trim.mesh(), DMat4::IDENTITY);
}

/// One side (+x, mirrored for the other): door seams, the black rocker cladding,
/// thick arch flares, the mirror and the marker lights.
fn flank(kit: &mut Kit, side: DMat4) {
    let face = HALF_WIDTH + 0.001;
    // Door seams: the front door's leading edge behind the front wheel, the split
    // between the doors and the rear door's trailing edge ahead of the rear wheel.
    let clear = OPENING_BOTTOM + 0.06;
    for z in [AXLES[1] - clear, 0.03, AXLES[0] + clear] {
        let bottom = ROCKER + 0.12;
        kit.block(
            Role::Gap,
            [0.004, BELT - bottom, 0.012],
            side * at([face, (BELT + bottom) / 2.0, z]),
        );
    }
    // Rocker cladding stands proud between the wheel openings.
    let rocker = slab(
        &[
            [AXLES[0] + OPENING_BOTTOM, ROCKER - 0.1],
            [AXLES[1] - OPENING_BOTTOM, ROCKER - 0.1],
            [AXLES[1] - OPENING_BOTTOM + 0.04, ROCKER + 0.13],
            [AXLES[0] + OPENING_BOTTOM - 0.04, ROCKER + 0.13],
        ],
        &[],
        0.05,
        0.012,
    );
    let to_side = frame(
        DVec3::Z,
        DVec3::Y,
        DVec3::X,
        DVec3::new(HALF_WIDTH - 0.015, 0.0, 0.0),
    );
    kit.add(Role::Shade, &rocker, side * to_side);
    // Thick black flares round both openings.
    for axle in AXLES {
        let (rear, front) = opening_bottoms(axle);
        let mut band: Vec<[f64; 2]> = wheel_opening(axle, 0.1, rear - 0.1, front - 0.1).to_vec();
        band.extend(
            wheel_opening(axle, 0.0, rear - 0.1, front - 0.1)
                .iter()
                .rev(),
        );
        let flare = slab(&band, &[], 0.065, 0.016);
        let to_side = frame(
            DVec3::Z,
            DVec3::Y,
            DVec3::X,
            DVec3::new(HALF_WIDTH - 0.02, 0.0, 0.0),
        );
        kit.add(Role::Shade, &flare, side * to_side);
    }
    // The mirror on the front door, on a short arm at the window's corner.
    let arm = side * place([HALF_WIDTH + 0.02, BELT + 0.07, 1.38], [0.0, 0.0, -0.2]);
    kit.chamfered(Role::Gap, [0.1, 0.035, 0.06], 0.008, arm);
    let head = side * place([HALF_WIDTH + 0.13, BELT + 0.12, 1.36], [0.0, 0.25, 0.0]);
    kit.chamfered(Role::Gap, [0.16, 0.12, 0.07], 0.014, head);
    kit.block(
        Role::Glass,
        [0.13, 0.09, 0.006],
        head * at([0.0, 0.0, -0.037]),
    );
    // Side markers at the front and rear corners.
    kit.block(
        Role::Amber,
        [0.006, 0.03, 0.12],
        side * at([face, 0.9, NOSE - 0.12]),
    );
    kit.block(
        Role::Red,
        [0.006, 0.03, 0.14],
        side * at([face, 1.08, TAIL + 0.2]),
    );
}

/// Dark liners inside the wheel openings and a wall behind each wheel, so the
/// openings read as wells rather than holes through the truck.
fn wheel_wells(kit: &mut Kit, side: DMat4) {
    for axle in AXLES {
        let (rear, front) = opening_bottoms(axle);
        let edge = wheel_opening(axle, 0.0, rear, front);
        let inset = edge.map(|[z, y]| {
            let toward = glam::DVec2::new(axle - z, 0.45 - y).normalize();
            [z + toward.x * 0.025, y + toward.y * 0.025]
        });
        let liner: Vec<[f64; 2]> = edge.iter().chain(inset.iter().rev()).copied().collect();
        let mesh = slab(&liner, &[], HALF_WIDTH - 0.02, 0.0);
        let to_side = frame(DVec3::Z, DVec3::Y, DVec3::X, DVec3::ZERO);
        kit.add(Role::Gap, &mesh, side * to_side);
        kit.block(
            Role::Gap,
            [0.03, 0.5, 2.0 * OPENING_BOTTOM],
            side * at([0.62, 0.72, axle]),
        );
    }
}

/// The nose: the light bar across the leading edge, a seam under it, the black
/// bumper with its intake, corner lamps and the skid plate beneath.
fn front(kit: &mut Kit) {
    // The fascia rakes back by 0.06 between the nose top and the bumper.
    let rake = (0.06_f64).atan2(NOSE_TOP - BUMPER_TOP);
    let fascia = |y: f64| NOSE - 0.06 * (NOSE_TOP - y) / (NOSE_TOP - BUMPER_TOP);
    let lamp = place(
        [0.0, NOSE_TOP - 0.03, fascia(NOSE_TOP - 0.03) + 0.004],
        [-rake, 0.0, 0.0],
    );
    kit.block(
        Role::Headlamp,
        [2.0 * HALF_WIDTH - 0.05, 0.032, 0.012],
        lamp,
    );
    let seam = place(
        [0.0, NOSE_TOP - 0.065, fascia(NOSE_TOP - 0.065) + 0.003],
        [-rake, 0.0, 0.0],
    );
    kit.block(Role::Gap, [2.0 * HALF_WIDTH - 0.08, 0.008, 0.008], seam);
    // The bumper, its lower intake and the corner lamps.
    let bumper = at([0.0, 0.47, NOSE - 0.2]);
    kit.chamfered(
        Role::Shade,
        [2.0 * HALF_WIDTH + 0.02, 0.3, 0.34],
        0.025,
        bumper,
    );
    kit.block(
        Role::Gap,
        [1.1, 0.1, 0.02],
        bumper * at([0.0, -0.03, 0.165]),
    );
    for x in [0.82, -0.82] {
        kit.block(
            Role::Amber,
            [0.16, 0.035, 0.012],
            bumper * at([x, 0.08, 0.171]),
        );
        // Tow points either side of the intake.
        kit.block(
            Role::Gap,
            [0.06, 0.06, 0.02],
            bumper * at([x * 0.78, -0.03, 0.17]),
        );
    }
    // Skid plate sloping up from under the bumper to the front axle.
    let skid = place([0.0, 0.36, NOSE - 0.62], [0.28, 0.0, 0.0]);
    kit.chamfered(Role::Shade, [1.5, 0.04, 0.7], 0.012, skid);
}

/// The tail: the light bar across the top of the tailgate, the tailgate's seams,
/// the chunky bumper with its step and the reflectors.
fn rear(kit: &mut Kit) {
    let face = TAIL - 0.004;
    kit.block(
        Role::Red,
        [2.0 * HALF_WIDTH - 0.04, 0.035, 0.012],
        at([0.0, TAIL_TOP - 0.045, face]),
    );
    // The light bar wraps a little way onto each side.
    for x in [1.0, -1.0] {
        kit.block(
            Role::Red,
            [0.012, 0.035, 0.12],
            at([
                x * (lean(TAIL_TOP - 0.045) + 0.001),
                TAIL_TOP - 0.045,
                TAIL + 0.06,
            ]),
        );
    }
    // Tailgate seams: along its foot and up each side.
    kit.block(
        Role::Gap,
        [2.0 * HALF_WIDTH - 0.1, 0.008, 0.008],
        at([0.0, BUMPER_TOP + 0.03, face]),
    );
    for x in [0.94, -0.94] {
        kit.block(Role::Gap, [0.008, 0.52, 0.008], at([x, 0.93, face]));
    }
    let bumper = at([0.0, 0.46, TAIL + 0.16]);
    kit.chamfered(
        Role::Shade,
        [2.0 * HALF_WIDTH + 0.04, 0.34, 0.46],
        0.03,
        bumper,
    );
    // The step in its centre, and the reflectors at its corners.
    kit.chamfered(
        Role::Gap,
        [0.62, 0.05, 0.16],
        0.01,
        bumper * at([0.0, 0.15, -0.16]),
    );
    for x in [0.86, -0.86] {
        kit.block(
            Role::Red,
            [0.18, 0.03, 0.01],
            bumper * at([x, 0.06, -0.232]),
        );
    }
}

/// The tonneau: a dark slatted cover over the bed, between the steel sails, on the
/// roof line from the rear window to the tailgate.
fn tonneau(kit: &mut Kit) {
    let width = |z: f64| lean(roof(z)) - SAIL;
    let (z0, z1) = (REAR_GLASS_END - 0.02, TAIL + 0.03);
    let mut cover = Faces::default();
    cover.polygon(
        &[
            on_roof(z0, width(z0), SKIN),
            on_roof(z0, -width(z0), SKIN),
            on_roof(z1, -width(z1), SKIN),
            on_roof(z1, width(z1), SKIN),
        ],
        DVec3::Y,
    );
    kit.add(Role::Gap, &cover.mesh(), DMat4::IDENTITY);
    let normal = roof_normal(z1);
    let along = normal.cross(DVec3::X);
    let slats = 13;
    for i in 0..slats {
        let z = z0 + (z1 - z0) * (f64::from(i) + 0.5) / f64::from(slats);
        let slat = frame(DVec3::X, normal, along, on_roof(z, 0.0, SKIN + 0.004));
        kit.block(Role::Canvas, [2.0 * width(z) - 0.02, 0.008, 0.1], slat);
    }
}

/// What shows between the wheels: a dark floor, the drive units and the arms of
/// the independent suspension.
fn underbody(kit: &mut Kit) {
    kit.block(Role::Gap, [1.5, 0.16, 4.4], at([0.0, 0.42, 0.0]));
    for axle in AXLES {
        kit.chamfered(Role::Shade, [0.6, 0.26, 0.42], 0.03, at([0.0, 0.42, axle]));
        let shaft = place([0.0, 0.45, axle], [0.0, 0.0, PI / 2.0]);
        kit.cylinder(Role::Shade, 0.04, 1.6, 8, shaft);
        for (y, tilt) in [(0.3, 0.1), (0.6, -0.15)] {
            for x in [0.5, -0.5] {
                let arm = place([x, y, axle], [0.0, 0.0, tilt * f64::signum(x)]);
                kit.block(Role::Shade, [0.42, 0.05, 0.14], arm);
            }
        }
    }
}
