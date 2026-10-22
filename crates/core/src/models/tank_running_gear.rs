//! Running gear of the three tracked chassis: dished twin road wheels, a toothed
//! drive sprocket, the idler, return rollers, a continuous track belt, fenders,
//! bolted side skirts and guarded headlights.
//!
//! Everything is laid out in the hull's y/z plane per side. The track follows a
//! belt path: the convex hull of the end road wheels, the sprocket and the idler.
//! The belt is one mesh per side whose links the paint shader draws and scrolls
//! with the tank's travel, so nothing moves in the model itself.
//!
//! Hit boxes are measured from the hull bounds (track group included), so the gear
//! keeps the former envelope exactly: fenders span the overall width and reach
//! [`track_reach`] fore and aft, and ground-run pads rest on [`TRACK_GROUND`].
//! Whole wheels, sprockets and links are single meshes repeated per station; the
//! renderer merges a joint's parts per material, so detail costs triangles, not
//! draw calls.

use std::f64::consts::{PI, TAU};
use std::sync::Arc;

use glam::{DVec2, DVec3};

use super::super::Team;
use super::super::model_primitives::{Cache, box_part, material, put, shadowed};
use super::super::tank_kit::planar_uv;
use super::super::tank_surfaces::{TRACK_BELT_PERIOD, track_belt_material, vehicle_glass};
use super::super::vertex_material;
use super::{Chassis, GLASS_TINT, STENCIL};
use crate::geometry::Mesh;
use crate::geometry::math::hex_to_linear;
use crate::scene::{Material, Node};

/// Bottom of the ground-run pads in hull space: the hull's lowest point.
const TRACK_GROUND: f64 = -0.197;
/// Fender plate thickness; its top stays where the former fender's was.
const FENDER_THICKNESS: f64 = 0.05;
const FENDER_WIDTH: f64 = 0.46;
/// Bolt heads stand this far proud of a skirt, which hangs that far inside the
/// fender edge so the heads stay within the overall width.
const BOLT_HEAD: f64 = 0.012;
/// Seam between neighbouring skirt panels.
const PANEL_SEAM: f64 = 0.014;
/// Road wheel, idler and roller facets: enough to read round close up.
const WHEEL_SIDES: u32 = 10;
const ROLLER_SIDES: u32 = 10;
const HUB_BOLTS: u32 = 5;

/// Rubber pads and skirt edges: near black.
const RUBBER: u32 = 0x202122;
/// Road wheel tyres: a black rubber sidewall and a lighter, scuffed tread band,
/// so each wheel reads as a rubber tyre round its painted disc.
const TYRE_SIDEWALL: u32 = 0x232425;
const TYRE_TREAD: u32 = 0x2e2f31;
/// Dark worn track steel: the vertex-colored running gear's base color.
const TRACK_STEEL: u32 = 0x4a4844;
/// Road wheel discs: dark cast steel rather than the team paint, a step darker
/// than the hull, so the running gear reads as its own material under the painted
/// armor: a row of dark discs ringed by black rubber over the dark belt, each with
/// a bright hub cap and bolt heads catching the light in its centre.
const WHEEL_DISC: u32 = 0x404342;
const HUB_STEEL: u32 = 0x7d7a73;
const HUB_BOLT: u32 = 0x9c978d;
/// The sprocket and return rollers: bare dark steel.
const SPROCKET_STEEL: u32 = 0x55585a;
/// Dirty track steel, wheels and rubber share one vertex-colored material, so a
/// tank's links, wheels, tyres and rubber edges draw together; satin enough for
/// the worn steel to catch the light.
const TRACK_METALNESS: f64 = 0.35;
const TRACK_ROUGHNESS: f64 = 0.6;

/// How a chassis dresses its sides.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SkirtStyle {
    /// M10 Booker: shorter bolted panels with a rubber edge.
    Booker,
    /// Abrams: long skirt, thick ballistic front panels with a raked leading edge.
    Abrams,
    /// Type 99: plates hung with add-on armour blocks over a deep rubber flap.
    Type99,
}

/// Per-chassis running gear dimensions in hull space (metres before the vehicle
/// scale). Heights are wheel-centre y; the end hubs' z follows from the reach.
struct GearLayout {
    /// Track centre line inset from the overall half width.
    track_inset: f64,
    track_width: f64,
    /// Link depth from the inner running surface to the ground face of the pad.
    link_thickness: f64,
    /// Link pitch on the fixed part of the track.
    link_pitch: f64,
    road_wheels: u32,
    wheel_radius: f64,
    /// z of the rearmost and frontmost road wheel.
    wheel_span: [f64; 2],
    sprocket_radius: f64,
    sprocket_height: f64,
    idler_radius: f64,
    idler_height: f64,
    /// ASCOD-derived hulls drive from a front sprocket and idle at the rear.
    front_drive: bool,
    rollers: &'static [f64],
    roller_radius: f64,
    skirt: SkirtStyle,
    /// Bottom edge of the armour panels (above any rubber edge).
    skirt_bottom: f64,
}

fn gear_layout(c: &Chassis) -> GearLayout {
    if c.scout {
        GearLayout {
            track_inset: 0.245,
            track_width: 0.38,
            link_thickness: 0.055,
            link_pitch: 0.18,
            road_wheels: 6,
            wheel_radius: 0.185,
            wheel_span: [-1.33, 1.27],
            sprocket_radius: 0.19,
            sprocket_height: 0.28,
            idler_radius: 0.17,
            idler_height: 0.27,
            front_drive: true,
            rollers: &[0.78, -0.08, -0.94],
            roller_radius: 0.06,
            skirt: SkirtStyle::Booker,
            skirt_bottom: 0.24,
        }
    } else if c.heavy {
        GearLayout {
            track_inset: 0.33,
            track_width: 0.42,
            link_thickness: 0.065,
            link_pitch: 0.22,
            // Seven stations of the biggest wheels: the heavy's running gear reads
            // as long and heavy as its hull.
            road_wheels: 7,
            wheel_radius: 0.235,
            wheel_span: [-1.6, 1.45],
            sprocket_radius: 0.24,
            sprocket_height: 0.31,
            idler_radius: 0.22,
            idler_height: 0.3,
            front_drive: false,
            rollers: &[0.95, 0.0, -0.95],
            roller_radius: 0.07,
            skirt: SkirtStyle::Type99,
            skirt_bottom: 0.27,
        }
    } else {
        GearLayout {
            track_inset: 0.285,
            track_width: 0.4,
            link_thickness: 0.06,
            link_pitch: 0.21,
            road_wheels: 7,
            wheel_radius: 0.2,
            wheel_span: [-1.56, 1.44],
            sprocket_radius: 0.22,
            sprocket_height: 0.315,
            idler_radius: 0.2,
            idler_height: 0.27,
            front_drive: false,
            rollers: &[0.5, -0.85],
            roller_radius: 0.065,
            skirt: SkirtStyle::Abrams,
            skirt_bottom: 0.2,
        }
    }
}

/// How far the track reaches fore and aft of the hull centre: the former stadium
/// belt's ends plus half a shoe. Hit boxes were measured with it, so the fenders
/// span exactly this and nothing else may pass it.
fn track_reach(length: f64) -> f64 {
    length * 1.235 / 2.46 + 0.0175
}

/// Wheels, sprocket, idler, rollers, track belt, fender, skirt and headlight on
/// one side (`side` is -1 left, +1 right).
pub(super) fn running_gear(hull: &mut Node, c: &Chassis, side: f64) {
    let gear = gear_layout(c);
    let track_x = side * (c.overall_width / 2.0 - gear.track_inset);
    let reach = track_reach(c.length);
    let link = LinkShape::new(&gear);
    let path = belt_path(&gear, reach, &link);

    // The outer face of every wheel sits just inside the end connectors.
    let wheel_face = track_x + side * (gear.track_width / 2.0 - link.connector_width - 0.004);
    let wheel_width = gear.track_width - link.connector_width - 0.03;
    let wheel = wheel_mesh(gear.wheel_radius, wheel_width, WHEEL_DISC);
    let steel = material(SPROCKET_STEEL, 0.55, 0.45);
    let rubber = track_material();
    let wheel_y = wheel_height(&gear);
    for j in 0..gear.road_wheels {
        let t = f64::from(j) / f64::from(gear.road_wheels - 1);
        let z = gear.wheel_span[0] + t * (gear.wheel_span[1] - gear.wheel_span[0]);
        place_wheel(hull, &wheel, &rubber, [wheel_face, wheel_y, z], side, 1.0);
    }
    // The idler is a road wheel on a raised, adjustable arm.
    let idler_scale = gear.idler_radius / gear.wheel_radius;
    place_wheel(
        hull,
        &wheel,
        &rubber,
        [wheel_face, path.idler.y, path.idler.x],
        side,
        idler_scale,
    );
    sprocket(hull, &steel, &gear, &link, wheel_face, path.sprocket, side);
    let drum = roller_mesh(gear.roller_radius, wheel_width * 0.55);
    for &z in gear.rollers {
        let y = path.top_run_surface(z, &link) - gear.roller_radius;
        let mut roller = shadowed(drum.clone(), steel.clone());
        roller.set_rotation_euler(0.0, if side < 0.0 { PI } else { 0.0 }, 0.0);
        put(hull, roller, wheel_face, y, z);
    }
    track_belt(hull, &gear, &link, &path, track_x, side);
    fender(hull, c, reach, side);
    skirt(hull, c, &gear, &path, side);
    headlight(hull, c, side);
}

fn wheel_height(gear: &GearLayout) -> f64 {
    TRACK_GROUND + gear.link_thickness + gear.wheel_radius
}

/// A road wheel (or the idler, scaled radially) with its outer face at `at.x`.
fn place_wheel(
    hull: &mut Node,
    wheel: &Arc<Mesh>,
    rubber: &Arc<Material>,
    at: [f64; 3],
    side: f64,
    radial_scale: f64,
) {
    let mut part = shadowed(wheel.clone(), rubber.clone());
    part.scale = DVec3::new(1.0, radial_scale, radial_scale);
    part.set_rotation_euler(0.0, if side < 0.0 { PI } else { 0.0 }, 0.0);
    put(hull, part, at[0], at[1], at[2]);
}

/// The steel drive sprocket at a hub centre, one tooth per link around it.
fn sprocket(
    hull: &mut Node,
    paint: &Arc<Material>,
    gear: &GearLayout,
    link: &LinkShape,
    wheel_face: f64,
    centre: DVec2,
    side: f64,
) {
    let radius = gear.sprocket_radius;
    let teeth = ((TAU * radius) / link.pitch).round() as u32;
    let mesh = sprocket_mesh(radius, teeth, link.connector_width * 1.2);
    let mut part = shadowed(mesh, paint.clone());
    part.set_rotation_euler(0.0, if side < 0.0 { PI } else { 0.0 }, 0.0);
    // The boss stands proud of the web; keep it inside the end connectors.
    put(hull, part, wheel_face - side * 0.03, centre.y, centre.x);
}

// ---------------------------------------------------------------------------
// Track path and links.

/// One link's proportions, shared by every link of a chassis.
struct LinkShape {
    pitch: f64,
    thickness: f64,
    width: f64,
    /// Shoe length along the track; the rest of the pitch is the gap between shoes.
    shoe: f64,
    connector_width: f64,
}

impl LinkShape {
    fn new(gear: &GearLayout) -> Self {
        Self {
            pitch: gear.link_pitch,
            thickness: gear.link_thickness,
            width: gear.track_width,
            shoe: gear.link_pitch * 0.84,
            connector_width: 0.035,
        }
    }

    /// The end connector bridges the joint ahead of the shoe.
    fn connector_span(&self) -> [f64; 2] {
        [self.pitch * 0.3, self.pitch * 0.7]
    }

    /// Furthest any point of a link lies from its centre along the track.
    fn reach(&self) -> f64 {
        (self.shoe / 2.0).max(self.connector_span()[1])
    }
}

#[derive(Clone, Copy)]
enum Piece {
    Line {
        from: DVec2,
        to: DVec2,
    },
    /// Counterclockwise arc (z right, y up) around `centre`.
    Arc {
        centre: DVec2,
        radius: f64,
        from: f64,
        sweep: f64,
    },
}

impl Piece {
    fn length(&self) -> f64 {
        match *self {
            Piece::Line { from, to } => from.distance(to),
            Piece::Arc { radius, sweep, .. } => radius * sweep,
        }
    }
}

/// The track's pin line (the joints, mid-way through the links) in (z, y),
/// traversed counterclockwise from the rear road wheel: ground run, front rise,
/// front hub, top run, rear hub.
struct BeltPath {
    /// Fixed pieces from the end of the top run around to its start.
    fixed: Vec<Piece>,
    /// The straight top run's rear and front pin points.
    top_run: [DVec2; 2],
    sprocket: DVec2,
    idler: DVec2,
}

impl BeltPath {
    /// Height of the top run's running surface (its underside) at `z`.
    fn top_run_surface(&self, z: f64, link: &LinkShape) -> f64 {
        let [from, to] = self.top_run;
        from.y + (to.y - from.y) * (z - from.x) / (to.x - from.x) - link.thickness / 2.0
    }
}

/// Wheel circles of the pin line in counterclockwise order, then the tangent
/// lines and wrapped arcs between them. Links bend round the hubs as chords
/// between pins, so a hub's pin circle is sized for a link's middle to rest on
/// its rim. End hubs stand back from the reach by the farthest a link's bounding
/// box can swing out around them.
fn belt_path(gear: &GearLayout, reach: f64, link: &LinkShape) -> BeltPath {
    let wheel_y = wheel_height(gear);
    let half = link.thickness / 2.0;
    let end_z = |radius: f64| reach - (radius + link.thickness).hypot(link.reach()) - 0.006;
    let pin_circle = |radius: f64| (radius + half).hypot(link.pitch / 2.0);
    let sprocket_z = end_z(gear.sprocket_radius);
    let idler_z = end_z(gear.idler_radius);
    let sprocket = (gear.sprocket_height, pin_circle(gear.sprocket_radius));
    let idler = (gear.idler_height, pin_circle(gear.idler_radius));
    let (front, rear) = if gear.front_drive {
        (
            (DVec2::new(sprocket_z, sprocket.0), sprocket.1),
            (DVec2::new(-idler_z, idler.0), idler.1),
        )
    } else {
        (
            (DVec2::new(idler_z, idler.0), idler.1),
            (DVec2::new(-sprocket_z, sprocket.0), sprocket.1),
        )
    };
    // Road wheels keep the ground run flat: the pins ride half a link below them.
    let circles = [
        (
            DVec2::new(gear.wheel_span[0], wheel_y),
            gear.wheel_radius + half,
        ),
        (
            DVec2::new(gear.wheel_span[1], wheel_y),
            gear.wheel_radius + half,
        ),
        front,
        rear,
    ];
    // Outward normal of the tangent leaving each circle for the next.
    let normals: Vec<DVec2> = (0..circles.len())
        .map(|i| {
            let (a, ra) = circles[i];
            let (b, rb) = circles[(i + 1) % circles.len()];
            let d = b - a;
            let along = (ra - rb) / d.length();
            let direction = d.normalize();
            let right = DVec2::new(direction.y, -direction.x);
            direction * along + right * (1.0 - along * along).sqrt()
        })
        .collect();
    let mut pieces = Vec::new();
    for i in 0..circles.len() {
        let next = (i + 1) % circles.len();
        let (a, ra) = circles[i];
        let (b, rb) = circles[next];
        pieces.push(Piece::Line {
            from: a + normals[i] * ra,
            to: b + normals[i] * rb,
        });
        let from = normals[i].y.atan2(normals[i].x);
        let to = normals[next].y.atan2(normals[next].x);
        pieces.push(Piece::Arc {
            centre: b,
            radius: rb,
            from,
            sweep: (to - from).rem_euclid(TAU),
        });
    }
    // Pieces: 0 ground, 1 front wheel, 2 rise, 3 front hub, 4 top run, 5 rear hub,
    // 6 drop, 7 rear wheel. Start the fixed run just after the top run.
    let Piece::Line { from, to } = pieces[4] else {
        unreachable!("the top run is straight");
    };
    let fixed = pieces[5..].iter().chain(&pieces[..4]).copied().collect();
    let (sprocket, idler) = if gear.front_drive {
        (front.0, rear.0)
    } else {
        (rear.0, front.0)
    };
    BeltPath {
        fixed,
        top_run: [from, to],
        sprocket,
        idler,
    }
}

/// The track as one continuous belt per side, following the pin line round the
/// wheels: its ground face, the running face the wheels roll on and both edges.
/// The links, pads, end connectors and guide horns are drawn by the paint shader's
/// belt mode from the belt's UVs, and slide round the loop with the tank's travel
/// (`TRACK_BELT_PERIOD`), so the track reads as a closed, moving belt rather than
/// a row of separate blocks, for a few hundred triangles a side.
fn track_belt(
    hull: &mut Node,
    gear: &GearLayout,
    link: &LinkShape,
    path: &BeltPath,
    track_x: f64,
    side: f64,
) {
    let links_per_period = (TRACK_BELT_PERIOD / gear.link_pitch).round();
    let mesh = belt_mesh(gear, link, path, side, TRACK_BELT_PERIOD / links_per_period);
    let belt = shadowed(
        mesh,
        track_belt_material(links_per_period / TRACK_BELT_PERIOD),
    );
    // The mesh is built up from the ground line, so its lowest point stands
    // exactly on it: the hull bounds (its hit box) are measured from the belt.
    put(hull, belt, track_x, TRACK_GROUND, 0.0);
}

/// Arc segments per full turn where the belt wraps a hub.
const BELT_ARC_SEGMENTS: f64 = 22.0;

/// The faces of the belt, as the integer part of its v coordinate.
const BELT_GROUND_FACE: f64 = 0.0;
const BELT_RUNNING_FACE: f64 = 1.0;
const BELT_OUTER_EDGE: f64 = 2.0;
const BELT_INNER_EDGE: f64 = 3.0;
/// v stays this far inside a face's unit range, so interpolation never crosses
/// into the next face's code.
const BELT_V_MARGIN: f64 = 0.02;

static BELTS: Cache<([u64; 4], [i64; 2]), Mesh> = Cache::new();

/// The belt mesh of one side in hull space (y, z; x about the track centre line):
/// u counts links along the loop (a whole number of them round it, each
/// `pitch` long within a few per cent), v the face and the position across it.
fn belt_mesh(
    gear: &GearLayout,
    link: &LinkShape,
    path: &BeltPath,
    side: f64,
    pitch: f64,
) -> Arc<Mesh> {
    let key = [gear.link_pitch, gear.track_width, gear.wheel_radius, pitch].map(f64::to_bits);
    BELTS.get_or_insert((key, [side as i64, i64::from(gear.road_wheels)]), || {
        let mut pieces = path.fixed.clone();
        pieces.push(Piece::Line {
            from: path.top_run[0],
            to: path.top_run[1],
        });
        // Pin points round the loop with their inward normals and distance.
        let mut points: Vec<(DVec2, DVec2, f64)> = Vec::new();
        let mut travelled = 0.0;
        for piece in &pieces {
            match *piece {
                Piece::Line { from, to } => {
                    let along = (to - from).normalize();
                    points.push((from, DVec2::new(-along.y, along.x), travelled));
                }
                Piece::Arc {
                    centre,
                    radius,
                    from,
                    sweep,
                } => {
                    let steps = (sweep / TAU * BELT_ARC_SEGMENTS).ceil().max(1.0) as u32;
                    for j in 0..steps {
                        let angle = from + sweep * f64::from(j) / f64::from(steps);
                        let outward = DVec2::new(angle.cos(), angle.sin());
                        points.push((
                            centre + outward * radius,
                            -outward,
                            travelled + radius * sweep * f64::from(j) / f64::from(steps),
                        ));
                    }
                }
            }
            travelled += piece.length();
        }
        let (first, normal, _) = points[0];
        points.push((first, normal, travelled));
        let links = (travelled / pitch).round().max(1.0);
        let half = link.thickness / 2.0;
        let (x_out, x_in) = (side * link.width / 2.0, -side * link.width / 2.0);
        let at = |x: f64, q: DVec2| DVec3::new(x, (q.y - TRACK_GROUND).max(0.0), q.x);
        let mut mesh = Mesh::default();
        let mut quad = |corners: [DVec3; 4], normals: [DVec3; 4], uvs: [[f64; 2]; 4]| {
            let facing = normals.iter().copied().sum::<DVec3>();
            let order = if (corners[1] - corners[0])
                .cross(corners[2] - corners[0])
                .dot(facing)
                < 0.0
            {
                [0, 2, 1, 0, 3, 2]
            } else {
                [0, 1, 2, 0, 2, 3]
            };
            for k in order {
                mesh.positions.push(corners[k].as_vec3().to_array());
                mesh.normals.push(normals[k].as_vec3().to_array());
                mesh.uvs.push(uvs[k].map(|value| value as f32));
                mesh.colors.push([1.0; 3]);
            }
        };
        let (low, high) = (BELT_V_MARGIN, 1.0 - BELT_V_MARGIN);
        for pair in points.windows(2) {
            let [(p0, n0, s0), (p1, n1, s1)] = [pair[0], pair[1]];
            let (u0, u1) = (s0 / travelled * links, s1 / travelled * links);
            let (outer0, outer1) = (p0 - n0 * half, p1 - n1 * half);
            let (inner0, inner1) = (p0 + n0 * half, p1 + n1 * half);
            let normal = |n: DVec2| DVec3::new(0.0, n.y, n.x);
            // Ground face, then the running face the wheels roll on.
            let ground = BELT_GROUND_FACE;
            quad(
                [
                    at(x_in, outer0),
                    at(x_out, outer0),
                    at(x_out, outer1),
                    at(x_in, outer1),
                ],
                [normal(-n0), normal(-n0), normal(-n1), normal(-n1)],
                [
                    [u0, ground + low],
                    [u0, ground + high],
                    [u1, ground + high],
                    [u1, ground + low],
                ],
            );
            let running = BELT_RUNNING_FACE;
            quad(
                [
                    at(x_in, inner0),
                    at(x_out, inner0),
                    at(x_out, inner1),
                    at(x_in, inner1),
                ],
                [normal(n0), normal(n0), normal(n1), normal(n1)],
                [
                    [u0, running + low],
                    [u0, running + high],
                    [u1, running + high],
                    [u1, running + low],
                ],
            );
            // Both edges, v running from the ground face to the running face.
            for (x, face) in [(x_out, BELT_OUTER_EDGE), (x_in, BELT_INNER_EDGE)] {
                let outward = DVec3::X * x.signum();
                quad(
                    [at(x, outer0), at(x, outer1), at(x, inner1), at(x, inner0)],
                    [outward; 4],
                    [
                        [u0, face + low],
                        [u1, face + low],
                        [u1, face + high],
                        [u0, face + high],
                    ],
                );
            }
        }
        mesh
    })
}

fn track_material() -> Arc<Material> {
    vertex_material(&material(TRACK_STEEL, TRACK_METALNESS, TRACK_ROUGHNESS))
}

// ---------------------------------------------------------------------------
// Wheels.

static WHEELS: Cache<([u64; 2], u32), Mesh> = Cache::new();

/// A twin road wheel about the x axis with its outer face at x = 0 and the pair
/// reaching back to -`width`, vertex-colored and drawn with the track: a raised
/// steel hub cap ringed by bright bolts, a web in `paint` dished in
/// toward the rim, and a rubber tyre whose sidewall rolls over into a lighter,
/// scuffed tread band.
fn wheel_mesh(radius: f64, width: f64, paint: u32) -> Arc<Mesh> {
    WHEELS.get_or_insert(([radius, width].map(f64::to_bits), paint), || {
        let r = radius;
        let cap = [r * 0.3, -0.01];
        let rim = [r * 0.78, -0.04];
        let mut wheel = MeshBuilder::default();
        wheel.revolve(&[[0.0, 0.0], cap], WHEEL_SIDES, None, HUB_STEEL);
        wheel.revolve(&[cap, rim], WHEEL_SIDES, None, paint);
        // Bolt heads stand on the web just outside the cap.
        let bolt_r = r * 0.38;
        let along = (bolt_r - cap[0]) / (rim[0] - cap[0]);
        let bolt_x = cap[1] + along * (rim[1] - cap[1]);
        for i in 0..HUB_BOLTS {
            let angle = (f64::from(i) + 0.5) * TAU / f64::from(HUB_BOLTS);
            wheel.stud(
                DVec3::new(bolt_x, bolt_r * angle.cos(), bolt_r * angle.sin()),
                r * 0.06,
                r * 0.07,
                HUB_BOLT,
            );
        }
        wheel.revolve(&[rim, [r, -0.012]], WHEEL_SIDES, None, TYRE_SIDEWALL);
        wheel.revolve(&[[r, -0.012], [r, -width]], WHEEL_SIDES, None, TYRE_TREAD);
        wheel.finish()
    })
}

static ROLLERS: Cache<[u64; 2], Mesh> = Cache::new();

/// A return roller: a short steel drum with a flat cap, outer face at x = 0.
fn roller_mesh(radius: f64, width: f64) -> Arc<Mesh> {
    ROLLERS.get_or_insert([radius, width].map(f64::to_bits), || {
        let mut b = MeshBuilder::default();
        b.revolve(
            &[[0.0, 0.0], [radius, 0.0], [radius, -width]],
            ROLLER_SIDES,
            None,
            UNCOLORED,
        );
        b.finish_uncolored()
    })
}

static SPROCKETS: Cache<([u64; 2], u32), Mesh> = Cache::new();

/// The drive sprocket about the x axis, outer face at x = 0: a bolted hub boss
/// proud of a flat web whose teeth stop just inside the links' running surface at
/// `radius`.
fn sprocket_mesh(radius: f64, teeth: u32, depth: f64) -> Arc<Mesh> {
    SPROCKETS.get_or_insert(([radius, depth].map(f64::to_bits), teeth), || {
        let mut b = MeshBuilder::default();
        let (root, tip, inner) = (radius - 0.045, radius - 0.004, radius * 0.24);
        let boss = 0.03;
        b.revolve(
            &[[0.0, boss], [inner * 0.75, boss], [inner, 0.0]],
            WHEEL_SIDES,
            None,
            UNCOLORED,
        );
        let bolt_r = radius * 0.33;
        for i in 0..HUB_BOLTS {
            let angle = (f64::from(i) + 0.5) * TAU / f64::from(HUB_BOLTS);
            b.stud(
                DVec3::new(0.0, bolt_r * angle.cos(), bolt_r * angle.sin()),
                radius * 0.045,
                radius * 0.05,
                UNCOLORED,
            );
        }
        let step = TAU / f64::from(teeth);
        // Root valley, then the tooth's flat tip, per tooth.
        let mut outline = Vec::new();
        for k in 0..teeth {
            let a = f64::from(k) * step;
            outline.push((a, root));
            outline.push((a + step * 0.32, tip));
            outline.push((a + step * 0.68, tip));
        }
        let point = |x: f64, angle: f64, r: f64| DVec3::new(x, r * angle.cos(), r * angle.sin());
        for i in 0..outline.len() {
            let (a0, r0) = outline[i];
            let (a1, r1) = outline[(i + 1) % outline.len()];
            let a1 = if i + 1 == outline.len() { a1 + TAU } else { a1 };
            b.oriented_quad(
                [
                    point(0.0, a0, inner),
                    point(0.0, a1, inner),
                    point(0.0, a1, r1),
                    point(0.0, a0, r0),
                ],
                DVec3::X,
                UNCOLORED,
            );
            let mid = (a0 + a1) / 2.0;
            let outward = DVec3::new(0.0, mid.cos(), mid.sin());
            let edge = [point(0.0, a0, r0), point(0.0, a1, r1)];
            let normal = {
                let along = edge[1] - edge[0];
                let n = DVec3::X.cross(along).normalize();
                if n.dot(outward) < 0.0 { -n } else { n }
            };
            b.oriented_quad(
                [
                    edge[0],
                    edge[1],
                    edge[1] - DVec3::X * depth,
                    edge[0] - DVec3::X * depth,
                ],
                normal,
                UNCOLORED,
            );
        }
        b.finish_uncolored()
    })
}

/// Placeholder color for meshes drawn with an ordinary (non-vertex-color)
/// material; [`MeshBuilder::finish_uncolored`] drops it.
const UNCOLORED: u32 = 0xffffff;

// ---------------------------------------------------------------------------
// Fender, skirts, headlight.

/// The track guard along the hull edge, spanning the full track reach. Its outer
/// edge is the hull's widest point. It wears the deep trim, so from overhead the
/// team-painted hull and turret sit between two dark strips over the tracks.
fn fender(hull: &mut Node, c: &Chassis, reach: f64, side: f64) {
    let x = side * (c.overall_width / 2.0 - FENDER_WIDTH / 2.0);
    let y = c.deck + 0.035 - FENDER_THICKNESS / 2.0;
    let plate = box_part(FENDER_WIDTH, FENDER_THICKNESS, reach * 2.0, c.shade, 0.0);
    put(hull, plate, x, y, 0.0);
}

/// A skirt panel: `z` its rear and front ends, `rake` how far its lower front
/// corner is cut back.
struct Panel {
    z: [f64; 2],
    thickness: f64,
    rake: f64,
}

fn skirt(hull: &mut Node, c: &Chassis, gear: &GearLayout, path: &BeltPath, side: f64) {
    let top = c.deck + 0.035 - FENDER_THICKNESS;
    let bottom = gear.skirt_bottom;
    let half_width = c.overall_width / 2.0;
    // Panels hang inside the fender edge by the bolt heads (Type 99: by its blocks).
    let face = half_width
        - if gear.skirt == SkirtStyle::Type99 {
            0.045
        } else {
            BOLT_HEAD
        };
    // Skirts stop over the rear hub, leaving the sprocket (Booker: idler) in view;
    // the Booker's front sprocket shows too, the others' idlers stay covered.
    let (rear_hub, rear_radius, front_hub, front_radius) = if gear.front_drive {
        (
            path.idler,
            gear.idler_radius,
            path.sprocket,
            gear.sprocket_radius,
        )
    } else {
        (
            path.sprocket,
            gear.sprocket_radius,
            path.idler,
            gear.idler_radius,
        )
    };
    let rear = rear_hub.x + rear_radius * 0.5;
    let front = track_reach(c.length) - 0.03;
    // Bolted modules of unequal widths, as real skirts are hung: a long middle
    // module over the stations, shorter ones toward the ends.
    let panels: Vec<Panel> = match gear.skirt {
        SkirtStyle::Booker => modular_panels(
            rear,
            front_hub.x - front_radius * 0.3,
            &[0.8, 1.25, 1.0, 0.9],
            |_| (0.04, 0.0),
        ),
        SkirtStyle::Abrams => {
            modular_panels(rear, front, &[0.75, 1.2, 0.85, 1.15, 0.95, 1.1], |j| {
                // Thick ballistic panels over the front two stations, raked leading edge.
                match j {
                    5 => (0.07, 0.34),
                    4 => (0.07, 0.0),
                    _ => (0.05, 0.0),
                }
            })
        }
        SkirtStyle::Type99 => {
            modular_panels(rear, front - 0.05, &[0.85, 1.2, 0.95, 1.0, 1.05], |j| {
                (0.04, if j == 4 { 0.2 } else { 0.0 })
            })
        }
    };
    let paint = material(c.color, 0.05, 0.65);
    let bolt = bolt_mesh();
    for (j, panel) in panels.iter().enumerate() {
        let part = shadowed(panel_mesh(panel, top - bottom, side), paint.clone());
        put(hull, part, side * face, bottom, panel.z[0]);
        if gear.skirt == SkirtStyle::Type99 && j >= 2 {
            // The blocks cover the front panels and their fixings.
            armour_blocks(hull, c, panel, top, bottom, face, side);
            continue;
        }
        let length = panel.z[1] - panel.z[0];
        let bolts = (length / 0.3).round().max(2.0) as u32;
        // A row along the top hanger and one just above the lower lip.
        let rows = [top - 0.03, bottom + SKIRT_LIP_HEIGHT + 0.03];
        for (row, &y) in rows.iter().enumerate() {
            for k in 0..bolts {
                let mut bz =
                    panel.z[0] + 0.06 + (length - 0.12) * f64::from(k) / f64::from(bolts - 1);
                if row > 0 {
                    // The lower row follows the raked edge.
                    bz = bz.min(panel.z[1] - panel.rake - 0.05);
                }
                let mut head = shadowed(bolt.clone(), paint.clone());
                if side < 0.0 {
                    head.set_rotation_euler(0.0, PI, 0.0);
                }
                put(hull, head, side * face, y, bz);
            }
        }
    }
    // The team's tactical sign, stencilled small on one panel under the turret.
    let insignia_panel = match gear.skirt {
        SkirtStyle::Booker => 1,
        SkirtStyle::Abrams => 2,
        SkirtStyle::Type99 => 1,
    };
    if let Some(panel) = panels.get(insignia_panel) {
        let height = (top - bottom) * INSIGNIA_HEIGHT;
        let part = shadowed(
            insignia_mesh(c.team, height, side),
            material(STENCIL, 0.0, 0.85),
        );
        put(
            hull,
            part,
            side * (face + INSIGNIA_PROUD),
            (top + bottom) / 2.0,
            (panel.z[0] + panel.z[1]) / 2.0,
        );
    }
    // Rubber edge under the panels (Type 99: a deep flap).
    let edge = match gear.skirt {
        SkirtStyle::Booker => 0.05,
        SkirtStyle::Abrams => 0.035,
        SkirtStyle::Type99 => 0.09,
    };
    let first = panels.first().map_or(rear, |p| p.z[0]);
    let last = panels.last().map_or(front, |p| p.z[1] - p.rake);
    let mut b = MeshBuilder::default();
    let half = [0.008, edge / 2.0, (last - first) / 2.0];
    b.box_faces(half.map(|h| -h), half, Faces::all(RUBBER));
    let strip = shadowed(Arc::new(b.finish()), track_material());
    put(
        hull,
        strip,
        side * (face - 0.02),
        bottom - edge / 2.0 + 0.004,
        (first + last) / 2.0,
    );
}

/// The skirt insignia's height as a share of the skirt's, and how far it stands
/// proud of the panel (still inside the fender edge).
const INSIGNIA_HEIGHT: f64 = 0.34;
const INSIGNIA_PROUD: f64 = 0.003;

static INSIGNIA: Cache<(Team, [u64; 2]), Mesh> = Cache::new();

/// A team's insignia painted on a skirt, `height` tall, facing out along x on
/// `side`: a diamond for blue, an upright triangle for red. A small tactical
/// sign beside the turret's unit number, never a second number.
fn insignia_mesh(team: Team, height: f64, side: f64) -> Arc<Mesh> {
    INSIGNIA.get_or_insert((team, [height, side].map(f64::to_bits)), || {
        let mut b = MeshBuilder::default();
        let facing = DVec3::X * side;
        let at = |z: f64, y: f64| DVec3::new(0.0, y, z);
        let h = height / 2.0;
        if team == Team::Blue {
            b.oriented_quad(
                [at(0.0, -h), at(h, 0.0), at(0.0, h), at(-h, 0.0)],
                facing,
                UNCOLORED,
            );
        } else {
            let half_base = h * 1.1;
            b.oriented_tri(
                [at(-half_base, -h), at(half_base, -h), at(0.0, h)],
                facing,
                UNCOLORED,
            );
        }
        b.finish_uncolored()
    })
}

/// Skirt modules from `rear` to `front`, their lengths in proportion to
/// `weights` (rear first), each styled by `style(j)` as (thickness, rake).
fn modular_panels(
    rear: f64,
    front: f64,
    weights: &[f64],
    style: impl Fn(u32) -> (f64, f64),
) -> Vec<Panel> {
    let unit = (front - rear) / weights.iter().sum::<f64>();
    let mut z = rear;
    (0u32..)
        .zip(weights)
        .map(|(j, weight)| {
            let (thickness, rake) = style(j);
            let length = weight * unit;
            let z0 = z + PANEL_SEAM / 2.0;
            z += length;
            Panel {
                z: [z0, z0 + length - PANEL_SEAM],
                thickness,
                rake,
            }
        })
        .collect()
}

/// The stiffening lip along a skirt module's lower edge: its height and how far
/// it stands proud of the face (within the bolt heads' allowance, so it stays
/// inside the overall width).
const SKIRT_LIP_HEIGHT: f64 = 0.04;
const SKIRT_LIP_PROUD: f64 = 0.009;

static PANELS: Cache<[u64; 5], Mesh> = Cache::new();

/// A skirt panel: a slab from its outer face at x = 0 inward by its thickness
/// (toward -`side`), y from 0 to `height`, z from 0 to its length, with the lower
/// front corner raked back.
fn panel_mesh(panel: &Panel, height: f64, side: f64) -> Arc<Mesh> {
    let length = panel.z[1] - panel.z[0];
    let key = [length, height, panel.thickness, panel.rake, side].map(f64::to_bits);
    PANELS.get_or_insert(key, || {
        let mut b = MeshBuilder::default();
        // Side outline in (z, y), counterclockwise seen from +x.
        let outline = [
            DVec2::new(0.0, 0.0),
            DVec2::new(length - panel.rake, 0.0),
            DVec2::new(length, height),
            DVec2::new(0.0, height),
        ];
        let p = |x: f64, q: DVec2| DVec3::new(x, q.y, q.x);
        let back = -side * panel.thickness;
        let outward = DVec3::X * side;
        // The lower lip: a turned-out strip along the bottom edge, catching the
        // light on its top so each module reads as a bolted plate with an edge.
        let lip_end = length - panel.rake;
        let (lo, hi) = (side * -0.001, side * SKIRT_LIP_PROUD);
        b.box_faces(
            [lo.min(hi), 0.0, 0.0],
            [lo.max(hi), SKIRT_LIP_HEIGHT, lip_end],
            Faces {
                left: (side < 0.0).then_some(UNCOLORED),
                right: (side > 0.0).then_some(UNCOLORED),
                top: Some(UNCOLORED),
                bottom: Some(UNCOLORED),
                back: Some(UNCOLORED),
                front: Some(UNCOLORED),
            },
        );
        b.oriented_quad(outline.map(|q| p(0.0, q)), outward, UNCOLORED);
        b.oriented_quad(outline.map(|q| p(back, q)), -outward, UNCOLORED);
        for i in 0..4 {
            let (a, c) = (outline[i], outline[(i + 1) % 4]);
            let edge = c - a;
            // Outline runs counterclockwise in (z, y): its edges face right of travel.
            let normal = DVec3::new(0.0, -edge.x, edge.y).normalize();
            b.oriented_quad(
                [p(0.0, a), p(0.0, c), p(back, c), p(back, a)],
                normal,
                UNCOLORED,
            );
        }
        b.finish_uncolored()
    })
}

static BOLT: std::sync::OnceLock<Arc<Mesh>> = std::sync::OnceLock::new();

/// A low bolt head standing out of a +x face at x = 0.
fn bolt_mesh() -> Arc<Mesh> {
    BOLT.get_or_init(|| {
        let mut b = MeshBuilder::default();
        b.stud(DVec3::ZERO, 0.016, BOLT_HEAD - 0.001, UNCOLORED);
        Arc::new(b.finish_uncolored())
    })
    .clone()
}

static BLOCKS: Cache<[u64; 3], Mesh> = Cache::new();

/// Type 99 add-on armour: a 2 x 2 grid of bolted-on blocks over a front panel.
fn armour_blocks(
    hull: &mut Node,
    c: &Chassis,
    panel: &Panel,
    top: f64,
    bottom: f64,
    face: f64,
    side: f64,
) {
    let length = panel.z[1] - panel.z[0] - panel.rake;
    let (cols, rows) = (2, 2);
    let gap = 0.025;
    let block_z = (length - gap * f64::from(cols + 1)) / f64::from(cols);
    let block_y = (top - bottom - gap * f64::from(rows + 1)) / f64::from(rows);
    let depth = 0.043;
    let mesh = BLOCKS.get_or_insert([block_y, block_z, side].map(f64::to_bits), || {
        // Open at the back, where the block sits on its panel.
        let mut b = MeshBuilder::default();
        let faces = Faces::all(UNCOLORED);
        let faces = if side < 0.0 {
            Faces {
                right: None,
                ..faces
            }
        } else {
            Faces {
                left: None,
                ..faces
            }
        };
        let half = [depth / 2.0, block_y / 2.0, block_z / 2.0];
        b.box_faces(half.map(|h| -h), half, faces);
        b.finish_uncolored()
    });
    for col in 0..cols {
        for row in 0..rows {
            let block = shadowed(mesh.clone(), material(c.shade, 0.05, 0.65));
            put(
                hull,
                block,
                side * (face + depth / 2.0),
                bottom + gap + (f64::from(row) + 0.5) * block_y + f64::from(row) * gap,
                panel.z[0] + gap + (f64::from(col) + 0.5) * block_z + f64::from(col) * gap,
            );
        }
    }
}

/// A lamp in a steel housing behind a three-bar brush guard. The lens is the
/// sights' glass, so it mirrors the sky like a lamp's glass rather than reading as
/// a flat white block (it shares the hull's glass draw).
fn headlight(hull: &mut Node, c: &Chassis, side: f64) {
    let (x, y, z) = (side * 0.65, c.deck - 0.1, c.length / 2.0 - 0.07);
    put(hull, box_part(0.17, 0.12, 0.1, c.steel, 0.0), x, y, z);
    let mut lens = box_part(0.12, 0.075, 0.012, 0, 0.0);
    if let Some(drawable) = &mut lens.drawable {
        drawable.material = vehicle_glass(GLASS_TINT);
    }
    put(hull, lens, x, y, z + 0.052);
    // A brush guard: two uprights under a top bar that reaches back to the housing.
    for dx in [-0.1, 0.1] {
        put(
            hull,
            box_part(0.016, 0.15, 0.016, c.steel, 0.0),
            x + dx,
            y,
            z + 0.062,
        );
    }
    put(
        hull,
        box_part(0.216, 0.016, 0.034, c.steel, 0.0),
        x,
        y + 0.067,
        z + 0.053,
    );
}

// ---------------------------------------------------------------------------
// Mesh building.

/// Faces of an axis-aligned box and their colors (`None` leaves a face out).
#[derive(Clone, Copy)]
struct Faces {
    left: Option<u32>,
    right: Option<u32>,
    bottom: Option<u32>,
    top: Option<u32>,
    back: Option<u32>,
    front: Option<u32>,
}

impl Faces {
    fn all(color: u32) -> Faces {
        Faces {
            left: Some(color),
            right: Some(color),
            bottom: Some(color),
            top: Some(color),
            back: Some(color),
            front: Some(color),
        }
    }
}

/// Non-indexed triangles with flat or given normals, planar UVs (for the wear
/// texture on painted parts) and per-vertex linear colors.
#[derive(Default)]
struct MeshBuilder {
    mesh: Mesh,
}

impl MeshBuilder {
    fn vertex(&mut self, position: DVec3, normal: DVec3, color: u32) {
        let (u, v) = planar_uv(position, normal);
        let [r, g, b] = hex_to_linear(color);
        self.mesh.positions.push(position.as_vec3().to_array());
        self.mesh.normals.push(normal.as_vec3().to_array());
        self.mesh.uvs.push([(u + 0.5) as f32, (v + 0.5) as f32]);
        self.mesh.colors.push([r as f32, g as f32, b as f32]);
    }

    /// A triangle with per-corner normals, wound to face `facing`.
    fn smooth_tri(&mut self, corners: [DVec3; 3], normals: [DVec3; 3], facing: DVec3, color: u32) {
        let [a, mut b, mut c] = corners;
        let [na, mut nb, mut nc] = normals;
        if (b - a).cross(c - a).dot(facing) < 0.0 {
            std::mem::swap(&mut b, &mut c);
            std::mem::swap(&mut nb, &mut nc);
        }
        self.vertex(a, na, color);
        self.vertex(b, nb, color);
        self.vertex(c, nc, color);
    }

    fn oriented_tri(&mut self, corners: [DVec3; 3], facing: DVec3, color: u32) {
        let [a, b, c] = corners;
        let normal = (b - a).cross(c - a).normalize_or_zero();
        let normal = if normal.dot(facing) < 0.0 {
            -normal
        } else {
            normal
        };
        self.smooth_tri(corners, [normal; 3], facing, color);
    }

    /// A planar quad (corners in order around it) facing `facing`.
    fn oriented_quad(&mut self, corners: [DVec3; 4], facing: DVec3, color: u32) {
        let [a, b, c, d] = corners;
        self.oriented_tri([a, b, c], facing, color);
        self.oriented_tri([a, c, d], facing, color);
    }

    /// The chosen faces of the axis-aligned box from `min` to `max`.
    fn box_faces(&mut self, min: [f64; 3], max: [f64; 3], faces: Faces) {
        let colors = [
            faces.left,
            faces.right,
            faces.bottom,
            faces.top,
            faces.back,
            faces.front,
        ];
        for (index, color) in colors.into_iter().enumerate() {
            let Some(color) = color else {
                continue;
            };
            // Faces come in -/+ pairs per axis; the other two axes span the face.
            let (axis, high) = (index / 2, index % 2 == 1);
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let corner = |at_u: bool, at_v: bool| {
                let mut p = min;
                if high {
                    p[axis] = max[axis];
                }
                if at_u {
                    p[u] = max[u];
                }
                if at_v {
                    p[v] = max[v];
                }
                DVec3::from_array(p)
            };
            let mut facing = DVec3::ZERO;
            facing[axis] = if high { 1.0 } else { -1.0 };
            let corners = [
                corner(false, false),
                corner(true, false),
                corner(true, true),
                corner(false, true),
            ];
            self.oriented_quad(corners, facing, color);
        }
    }

    /// A low four-sided bolt head standing out of a +x face at `base`.
    fn stud(&mut self, base: DVec3, half: f64, height: f64, color: u32) {
        let tip = base + DVec3::X * height;
        let corners = [
            base + DVec3::new(0.0, half, 0.0),
            base + DVec3::new(0.0, 0.0, half),
            base + DVec3::new(0.0, -half, 0.0),
            base + DVec3::new(0.0, 0.0, -half),
        ];
        for i in 0..4 {
            let (a, b) = (corners[i], corners[(i + 1) % 4]);
            let outward = (a + b) / 2.0 - base + DVec3::X * height;
            self.oriented_tri([a, b, tip], outward, color);
        }
    }

    /// Revolve a profile of (radius, x) points about the x axis. Each segment has
    /// its own ring (hard edges) except the profile points listed in `smooth_at`,
    /// whose two segments share an averaged normal there (a rounded shoulder).
    /// A segment starting on the axis closes as a fan.
    fn revolve(&mut self, profile: &[[f64; 2]], sides: u32, smooth_at: Option<usize>, color: u32) {
        let segment_normal = |i: usize| {
            let ([r0, x0], [r1, x1]) = (profile[i], profile[i + 1]);
            // (radial, axial) perpendicular, outward for profiles that run from
            // the axis outward and then back along the rim.
            DVec2::new(x0 - x1, r1 - r0).normalize()
        };
        let point = |[r, x]: [f64; 2], angle: f64| DVec3::new(x, r * angle.cos(), r * angle.sin());
        let normal = |n: DVec2, angle: f64| DVec3::new(n.y, n.x * angle.cos(), n.x * angle.sin());
        for i in 0..profile.len() - 1 {
            let own = segment_normal(i);
            let at_start = if smooth_at == Some(i) {
                (own + segment_normal(i - 1)).normalize()
            } else {
                own
            };
            let at_end = if smooth_at == Some(i + 1) {
                (own + segment_normal(i + 1)).normalize()
            } else {
                own
            };
            for k in 0..sides {
                let a0 = f64::from(k) * TAU / f64::from(sides);
                let a1 = f64::from(k + 1) * TAU / f64::from(sides);
                let mid = (a0 + a1) / 2.0;
                let facing = normal((at_start + at_end) / 2.0, mid);
                let (p0, p1) = (profile[i], profile[i + 1]);
                if p0[0] == 0.0 {
                    self.smooth_tri(
                        [point(p0, a0), point(p1, a0), point(p1, a1)],
                        [
                            normal(at_start, mid),
                            normal(at_end, a0),
                            normal(at_end, a1),
                        ],
                        facing,
                        color,
                    );
                    continue;
                }
                let corners = [point(p0, a0), point(p1, a0), point(p1, a1), point(p0, a1)];
                let normals = [
                    normal(at_start, a0),
                    normal(at_end, a0),
                    normal(at_end, a1),
                    normal(at_start, a1),
                ];
                self.smooth_tri(
                    [corners[0], corners[1], corners[2]],
                    [normals[0], normals[1], normals[2]],
                    facing,
                    color,
                );
                self.smooth_tri(
                    [corners[0], corners[2], corners[3]],
                    [normals[0], normals[2], normals[3]],
                    facing,
                    color,
                );
            }
        }
    }

    fn finish(self) -> Mesh {
        self.mesh
    }

    /// The mesh without vertex colors, for parts drawn with a plain material.
    fn finish_uncolored(mut self) -> Mesh {
        self.mesh.colors.clear();
        self.mesh
    }
}

#[cfg(test)]
mod tests {
    use glam::DMat4;

    use super::super::tank_model;
    use crate::geometry::node_bounds;
    use crate::models::tank_surfaces::{ember_material, is_signal_panel};
    use crate::models::{Team, VehicleKind, WreckPart, part, wreck_model};
    use crate::scene::Node;

    /// Triangles of every shown part but the recognition panels and turret bands,
    /// which burn away on wrecks, the embers wrecks add in their hatches, and the
    /// live vehicle's contact shadow.
    fn triangles(node: &Node) -> usize {
        if !node.visible || node.name == "contact-shadow" {
            return 0;
        }
        let own = node.drawable.as_ref().map_or(0, |drawable| {
            let burnt = is_signal_panel(&drawable.material)
                || drawable.material.emissive.0 == ember_material().emissive.0;
            if burnt {
                0
            } else {
                drawable.mesh.triangle_count()
            }
        });
        own + node.children.iter().map(triangles).sum::<usize>()
    }

    /// Wrecks merge parts per material and drop any batch whose meshes disagree on
    /// attributes, so every running-gear mesh must carry normals and UVs, and
    /// colors exactly when its material is vertex-colored.
    #[test]
    fn wrecks_keep_every_running_gear_triangle() {
        for kind in VehicleKind::PLAYABLE {
            let live = tank_model(kind, Team::Red);
            let wreck = wreck_model(kind, Team::Red, WreckPart::Intact);
            assert_eq!(triangles(&wreck), triangles(&live), "{kind:?}");
        }
    }

    /// The belt scrolls in its shader, so nothing in the model moves: the tread
    /// group stays empty and the belt rests on the ground inside the track's reach.
    #[test]
    fn belt_rests_on_the_ground_within_the_track_reach() {
        for kind in VehicleKind::PLAYABLE {
            let model = tank_model(kind, Team::Blue);
            let hull = model.find(part::HULL).unwrap();
            let bounds = node_bounds(hull, DMat4::IDENTITY);
            assert!(hull.find(part::TRACK_GROUP).unwrap().children.is_empty());
            let belts: Vec<_> = hull
                .children
                .iter()
                .filter(|child| {
                    child.drawable.as_ref().is_some_and(|drawable| {
                        matches!(&drawable.material.effect,
                            crate::scene::Effect::Custom { params, .. } if params[15] == 3.0)
                    })
                })
                .collect();
            assert_eq!(belts.len(), 2, "{kind:?} has a belt a side");
            for belt in belts {
                let own = node_bounds(belt, DMat4::IDENTITY);
                assert!(
                    (own.min.y - super::TRACK_GROUND).abs() < 1e-6,
                    "{kind:?} rests"
                );
                assert!(
                    own.min.z >= bounds.min.z && own.max.z <= bounds.max.z,
                    "{kind:?}"
                );
            }
        }
    }
}
