//! Running gear of the three tracked chassis: dished twin road wheels, a toothed
//! drive sprocket, the idler, return rollers, a track of individual links,
//! fenders, bolted side skirts and guarded headlights.
//!
//! Everything is laid out in the hull's y/z plane per side. The track follows a
//! belt path: the convex hull of the end road wheels, the sprocket and the idler.
//! Links on the top run go into `track-group`, which the renderer slides along z
//! by up to one [`TOP_RUN_PITCH`] as the tank drives; every other link is fixed to
//! the hull, so the ground run never slides off the wheels.
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

use super::super::model_primitives::{Cache, box_part, material, put, shadowed};
use super::super::vertex_material;
use super::{Chassis, WRECK_STEEL};
use crate::geometry::Mesh;
use crate::geometry::math::hex_to_linear;
use crate::scene::{Material, Node};

/// Bottom of the ground-run pads in hull space: the hull's lowest point.
const TRACK_GROUND: f64 = -0.197;
/// Slide of the top run per wrap of the renderer's tread scroll (its
/// `TRACK_PERIOD`): top-run links repeat at this pitch so the wrap is seamless.
const TOP_RUN_PITCH: f64 = 0.25;
/// Fender plate thickness; its top stays where the former fender's was.
const FENDER_THICKNESS: f64 = 0.05;
const FENDER_WIDTH: f64 = 0.46;
/// Bolt heads stand this far proud of a skirt, which hangs that far inside the
/// fender edge so the heads stay within the overall width.
const BOLT_HEAD: f64 = 0.012;
/// Seam between neighbouring skirt panels.
const PANEL_SEAM: f64 = 0.014;
/// Road wheel, idler and roller facets: enough to read round close up.
const WHEEL_SIDES: u32 = 16;
const ROLLER_SIDES: u32 = 10;
const HUB_BOLTS: u32 = 5;

const HEADLIGHT: u32 = 0xd9e6df;
/// Rubber tyres, pads and skirt edges.
const RUBBER: u32 = 0x27292a;
const WRECK_RUBBER: u32 = 0x151515;
/// Track shoes and their end connectors: dark worn steel, not paint.
const TRACK_STEEL: u32 = 0x605f5b;
const TRACK_CONNECTOR: u32 = 0x76736d;
const WRECK_TRACK_STEEL: u32 = 0x2f2b28;
const WRECK_TRACK_CONNECTOR: u32 = 0x3a3430;
/// Dirty track steel and rubber share one matte vertex-colored material, so a
/// tank's links, tyres and rubber edges draw together.
const TRACK_METALNESS: f64 = 0.2;
const TRACK_ROUGHNESS: f64 = 0.8;

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
            road_wheels: 6,
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
            skirt_bottom: 0.17,
        }
    }
}

/// How far the track reaches fore and aft of the hull centre: the former stadium
/// belt's ends plus half a shoe. Hit boxes were measured with it, so the fenders
/// span exactly this and nothing else may pass it.
fn track_reach(length: f64) -> f64 {
    length * 1.235 / 2.46 + 0.0175
}

/// Wheels, sprocket, idler, rollers, track links, fender, skirt and headlight on
/// one side (`side` is -1 left, +1 right). Top-run links go into `track_group`.
pub(super) fn running_gear(hull: &mut Node, track_group: &mut Node, c: &Chassis, side: f64) {
    let gear = gear_layout(c);
    let wreck = c.steel == WRECK_STEEL;
    let track_x = side * (c.overall_width / 2.0 - gear.track_inset);
    let reach = track_reach(c.length);
    let link = LinkShape::new(&gear);
    let path = belt_path(&gear, reach, &link);

    // The outer face of every wheel sits just inside the end connectors.
    let wheel_face = track_x + side * (gear.track_width / 2.0 - link.connector_width - 0.004);
    let wheel_width = gear.track_width - link.connector_width - 0.03;
    let wheel = wheel_meshes(gear.wheel_radius, wheel_width, wreck);
    let paint = material(c.shade, 0.2, 0.65);
    let rubber = track_material();
    let wheel_y = wheel_height(&gear);
    for j in 0..gear.road_wheels {
        let t = f64::from(j) / f64::from(gear.road_wheels - 1);
        let z = gear.wheel_span[0] + t * (gear.wheel_span[1] - gear.wheel_span[0]);
        place_wheel(
            hull,
            &wheel,
            &paint,
            &rubber,
            [wheel_face, wheel_y, z],
            side,
            1.0,
        );
    }
    // The idler is a road wheel on a raised, adjustable arm.
    let idler_scale = gear.idler_radius / gear.wheel_radius;
    place_wheel(
        hull,
        &wheel,
        &paint,
        &rubber,
        [wheel_face, path.idler.y, path.idler.x],
        side,
        idler_scale,
    );
    sprocket(hull, c, &gear, &link, wheel_face, path.sprocket, side);
    for &z in gear.rollers {
        let y = path.top_run_surface(z, &link) - gear.roller_radius;
        let mesh = roller_mesh(gear.roller_radius, wheel_width * 0.55);
        let mut roller = shadowed(mesh, material(c.shade, 0.2, 0.65));
        roller.set_rotation_euler(0.0, if side < 0.0 { PI } else { 0.0 }, 0.0);
        put(hull, roller, wheel_face, y, z);
    }
    track_links(hull, track_group, &gear, &link, &path, track_x, side, wreck);
    fender(hull, c, reach, side);
    skirt(hull, c, &gear, &path, side, wreck);
    headlight(hull, c, side);
}

fn wheel_height(gear: &GearLayout) -> f64 {
    TRACK_GROUND + gear.link_thickness + gear.wheel_radius
}

fn rubber(wreck: bool) -> u32 {
    if wreck { WRECK_RUBBER } else { RUBBER }
}

/// A road wheel (or the idler, scaled radially) with its outer face at `at.x`.
fn place_wheel(
    hull: &mut Node,
    wheel: &WheelMeshes,
    paint: &Arc<Material>,
    rubber: &Arc<Material>,
    at: [f64; 3],
    side: f64,
    radial_scale: f64,
) {
    for (mesh, material) in [(&wheel.disc, paint), (&wheel.tyre, rubber)] {
        let mut part = shadowed(mesh.clone(), material.clone());
        part.scale = DVec3::new(1.0, radial_scale, radial_scale);
        part.set_rotation_euler(0.0, if side < 0.0 { PI } else { 0.0 }, 0.0);
        put(hull, part, at[0], at[1], at[2]);
    }
}

/// The painted drive sprocket at a hub centre, one tooth per link around it.
fn sprocket(
    hull: &mut Node,
    c: &Chassis,
    gear: &GearLayout,
    link: &LinkShape,
    wheel_face: f64,
    centre: DVec2,
    side: f64,
) {
    let radius = gear.sprocket_radius;
    let teeth = ((TAU * radius) / link.pitch).round() as u32;
    let mesh = sprocket_mesh(radius, teeth, link.connector_width * 1.2);
    let mut part = shadowed(mesh, material(c.shade, 0.2, 0.65));
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

    fn steel_depth(&self) -> f64 {
        self.thickness * 0.55
    }

    fn chamfer(&self) -> f64 {
        self.shoe * 0.16
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

    /// Point on the path and the direction of travel at `s`.
    fn at(&self, s: f64) -> (DVec2, DVec2) {
        match *self {
            Piece::Line { from, to } => {
                let direction = (to - from).normalize();
                (from + direction * s, direction)
            }
            Piece::Arc {
                centre,
                radius,
                from,
                ..
            } => {
                let angle = from + s / radius;
                let (sin, cos) = angle.sin_cos();
                (
                    centre + DVec2::new(cos, sin) * radius,
                    DVec2::new(-sin, cos),
                )
            }
        }
    }
}

/// The track's pin line (the joints, mid-way through the links) in (z, y),
/// traversed counterclockwise from the rear road wheel: ground run, front rise,
/// front hub, top run, rear hub.
struct BeltPath {
    /// Fixed pieces from the end of the top run around to its start.
    fixed: Vec<Piece>,
    top_run: Piece,
    sprocket: DVec2,
    idler: DVec2,
}

impl BeltPath {
    /// Height of the top run's running surface (its underside) at `z`.
    fn top_run_surface(&self, z: f64, link: &LinkShape) -> f64 {
        let Piece::Line { from, to } = self.top_run else {
            unreachable!("the top run is straight");
        };
        from.y + (to.y - from.y) * (z - from.x) / (to.x - from.x) - link.thickness / 2.0
    }

    /// The pin-line point `s` along the fixed run, and the piece it lies on.
    fn fixed_at(&self, mut s: f64) -> (DVec2, Piece) {
        let mut piece = self.fixed[0];
        for candidate in &self.fixed {
            piece = *candidate;
            if s <= candidate.length() {
                break;
            }
            s -= candidate.length();
        }
        (piece.at(s.clamp(0.0, piece.length())).0, piece)
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
    let top_run = pieces[4];
    let fixed = pieces[5..].iter().chain(&pieces[..4]).copied().collect();
    let (sprocket, idler) = if gear.front_drive {
        (front.0, rear.0)
    } else {
        (rear.0, front.0)
    };
    BeltPath {
        fixed,
        top_run,
        sprocket,
        idler,
    }
}

/// Fixed links evenly around the belt (bare links on the ground, horned links
/// where the track wraps the hubs), and the top run's scrolling links.
#[allow(clippy::too_many_arguments)]
fn track_links(
    hull: &mut Node,
    track_group: &mut Node,
    gear: &GearLayout,
    link: &LinkShape,
    path: &BeltPath,
    track_x: f64,
    side: f64,
    wreck: bool,
) {
    let material = track_material();
    let ground = link_mesh(link, LinkDetail::Ground, wreck);
    let wrapped = link_mesh(link, LinkDetail::Wrapped, wreck);
    let total: f64 = path.fixed.iter().map(Piece::length).sum();
    let count = (total / link.pitch).round().max(1.0);
    let spacing = total / count;
    let wheel_y = wheel_height(gear);
    for k in 0..count as u32 {
        // Each link is the chord between two pins.
        let (back, _) = path.fixed_at(f64::from(k) * spacing);
        let (ahead, _) = path.fixed_at(f64::from(k + 1) * spacing);
        let (_, piece) = path.fixed_at((f64::from(k) + 0.5) * spacing);
        let direction = (ahead - back).normalize();
        let inward = DVec2::new(-direction.y, direction.x);
        let surface = (back + ahead) / 2.0 + inward * (link.thickness / 2.0);
        // Links curling around a hub carry visible guide horns; the rest are
        // hidden between the twin road wheels.
        let around_hub = matches!(piece, Piece::Arc { centre, .. } if centre.y > wheel_y + 0.01);
        let mesh = if around_hub { &wrapped } else { &ground };
        let node = placed_link(mesh, &material, surface, direction, side);
        put(hull, node.0, track_x, node.1.y, node.1.x);
    }
    // The top run scrolls forward by up to one pitch: start at its rear end and
    // stop a pitch short of the front so the run never slides into the hub.
    let top = link_mesh(link, LinkDetail::TopRun, wreck);
    let Piece::Line { from, to } = path.top_run else {
        unreachable!("the top run is straight");
    };
    let length = from.distance(to);
    let direction = (to - from).normalize();
    let inward = DVec2::new(-direction.y, direction.x);
    let mut d = link.reach() + 0.01;
    while d + TOP_RUN_PITCH + link.reach() <= length {
        let point = to - direction * d + inward * (link.thickness / 2.0);
        let node = placed_link(&top, &material, point, direction, side);
        put(track_group, node.0, track_x, node.1.y, node.1.x);
        d += TOP_RUN_PITCH;
    }
}

/// A link at a running-surface point, turned to the travel direction and mirrored
/// on the left side; returns the node and its (z, y) position, nudged up where
/// the track bends onto the ground run. The hull bounds measure each part by its
/// transformed bounding box, so that box's corners must stay above the ground.
fn placed_link(
    mesh: &Arc<Mesh>,
    material: &Arc<Material>,
    point: DVec2,
    direction: DVec2,
    side: f64,
) -> (Node, DVec2) {
    // Link y points into the loop (left of travel), z along travel.
    let inward = DVec2::new(-direction.y, direction.x);
    // Mirrored links (turned half round y) lead with their other end.
    let along = direction * side.signum();
    let bounds = mesh.bounding_box();
    let lowest = [bounds.min, bounds.max]
        .iter()
        .flat_map(|a| [bounds.min, bounds.max].map(|b| DVec2::new(a.z, b.y)))
        .map(|corner| (point + along * corner.x + inward * corner.y).y)
        .fold(f64::INFINITY, f64::min);
    let mut position = point;
    if lowest < TRACK_GROUND {
        position.y += TRACK_GROUND - lowest;
    }
    let pitch = (-direction.y).atan2(direction.x);
    let mut node = shadowed(mesh.clone(), material.clone());
    node.set_rotation_euler(pitch, if side < 0.0 { PI } else { 0.0 }, 0.0);
    (node, position)
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
enum LinkDetail {
    /// Shoe with rubber pad and outer end connector.
    Ground,
    /// Also the inner edge and the centre guide horn, seen where the track wraps.
    Wrapped,
    /// Hidden under the fender: pad face, shoe ends and outer edge only.
    TopRun,
}

static LINKS: Cache<([u64; 3], LinkDetail, bool), Mesh> = Cache::new();

/// One track link as a vertex-colored mesh in link space: x across the track
/// (end connector at +x), y toward the wheels with the running surface at 0, z
/// along travel. Steel shoe, rubber pad with chamfered ends, connector bridging
/// the joint ahead.
fn link_mesh(link: &LinkShape, detail: LinkDetail, wreck: bool) -> Arc<Mesh> {
    let key = [link.pitch, link.thickness, link.width].map(f64::to_bits);
    LINKS.get_or_insert((key, detail, wreck), || {
        let (steel, connector, rubber) = if wreck {
            (WRECK_TRACK_STEEL, WRECK_TRACK_CONNECTOR, WRECK_RUBBER)
        } else {
            (TRACK_STEEL, TRACK_CONNECTOR, RUBBER)
        };
        let mut b = MeshBuilder::default();
        let t = link.thickness;
        let half = link.shoe / 2.0;
        let x0 = -link.width / 2.0;
        let x1 = link.width / 2.0 - link.connector_width;
        if detail == LinkDetail::TopRun {
            b.box_faces(
                [x0, -t, -half],
                [x1, 0.0, half],
                Faces {
                    bottom: Some(rubber),
                    front: Some(steel),
                    back: Some(steel),
                    right: Some(steel),
                    ..Faces::NONE
                },
            );
            return b.finish();
        }
        // Profile in (z, y): shoe above, pad with chamfered ends below.
        let steel_y = -link.steel_depth();
        let chamfer = link.chamfer();
        let profile = [
            DVec2::new(-half, 0.0),
            DVec2::new(half, 0.0),
            DVec2::new(half, steel_y),
            DVec2::new(half - chamfer, -t),
            DVec2::new(-half + chamfer, -t),
            DVec2::new(-half, steel_y),
        ];
        let colors = [steel, steel, rubber, rubber, rubber, steel];
        for i in 0..profile.len() {
            let (a, c) = (profile[i], profile[(i + 1) % profile.len()]);
            let p = |x: f64, q: DVec2| DVec3::new(x, q.y, q.x);
            b.quad([p(x0, a), p(x0, c), p(x1, c), p(x1, a)], colors[i]);
        }
        let mut caps = vec![(x1, 1.0)];
        if detail == LinkDetail::Wrapped {
            caps.push((x0, -1.0));
        }
        for (x, facing) in caps {
            let p = |q: DVec2| DVec3::new(x, q.y, q.x);
            let [a, bb, c, d, e, f] = profile;
            b.oriented_quad([p(a), p(bb), p(c), p(f)], DVec3::X * facing, steel);
            b.oriented_quad([p(f), p(c), p(d), p(e)], DVec3::X * facing, rubber);
        }
        let [c0, c1] = link.connector_span();
        b.box_faces(
            [x1, -0.8 * t, c0],
            [link.width / 2.0, 0.1 * t, c1],
            Faces {
                left: None,
                ..Faces::all(connector)
            },
        );
        if detail == LinkDetail::Wrapped {
            // Centre guide horn: a ridge standing into the wheels.
            let (w, base, height) = (0.022, half * 0.55, t * 1.1);
            let corner = |x: f64, z: f64, y: f64| DVec3::new(x, y, z);
            for x in [-w, w] as [f64; 2] {
                let facing = DVec3::X * x.signum();
                b.oriented_tri(
                    [
                        corner(x, -base, 0.0),
                        corner(x, base, 0.0),
                        corner(x, 0.0, height),
                    ],
                    facing,
                    steel,
                );
            }
            b.oriented_quad(
                [
                    corner(-w, base, 0.0),
                    corner(w, base, 0.0),
                    corner(w, 0.0, height),
                    corner(-w, 0.0, height),
                ],
                DVec3::new(0.0, base, height).normalize(),
                connector,
            );
            b.oriented_quad(
                [
                    corner(-w, -base, 0.0),
                    corner(w, -base, 0.0),
                    corner(w, 0.0, height),
                    corner(-w, 0.0, height),
                ],
                DVec3::new(0.0, base, -height).normalize(),
                connector,
            );
        }
        b.finish()
    })
}

fn track_material() -> Arc<Material> {
    vertex_material(&material(TRACK_STEEL, TRACK_METALNESS, TRACK_ROUGHNESS))
}

// ---------------------------------------------------------------------------
// Wheels.

struct WheelMeshes {
    /// Painted disc: hub cap, bolt circle and dished web.
    disc: Arc<Mesh>,
    /// Rubber tyre: sidewall and the tread band across both wheels of the pair.
    tyre: Arc<Mesh>,
}

static WHEELS: Cache<([u64; 2], bool), WheelMeshes> = Cache::new();

/// A twin road wheel about the x axis with its outer face at x = 0 and the pair
/// reaching back to -`width`: a raised hub cap ringed by bolts, a web dished in
/// toward the rim, and a rubber tyre (vertex-colored, drawn with the track).
fn wheel_meshes(radius: f64, width: f64, wreck: bool) -> Arc<WheelMeshes> {
    let key = ([radius, width].map(f64::to_bits), wreck);
    WHEELS.get_or_insert(key, || {
        let r = radius;
        let cap = [r * 0.3, -0.01];
        let rim = [r * 0.78, -0.04];
        let mut disc = MeshBuilder::default();
        disc.revolve(&[[0.0, 0.0], cap, rim], WHEEL_SIDES, None, UNCOLORED);
        // Bolt heads stand on the web just outside the cap.
        let bolt_r = r * 0.38;
        let along = (bolt_r - cap[0]) / (rim[0] - cap[0]);
        let bolt_x = cap[1] + along * (rim[1] - cap[1]);
        for i in 0..HUB_BOLTS {
            let angle = (f64::from(i) + 0.5) * TAU / f64::from(HUB_BOLTS);
            disc.stud(
                DVec3::new(bolt_x, bolt_r * angle.cos(), bolt_r * angle.sin()),
                r * 0.06,
                r * 0.07,
            );
        }
        let mut tyre = MeshBuilder::default();
        // The sidewall rolls over into the tread band (shared normal).
        tyre.revolve(
            &[rim, [r, -0.012], [r, -width]],
            WHEEL_SIDES,
            Some(1),
            rubber(wreck),
        );
        WheelMeshes {
            disc: Arc::new(disc.finish_uncolored()),
            tyre: Arc::new(tyre.finish()),
        }
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
/// edge is the hull's widest point.
fn fender(hull: &mut Node, c: &Chassis, reach: f64, side: f64) {
    let x = side * (c.overall_width / 2.0 - FENDER_WIDTH / 2.0);
    let y = c.deck + 0.035 - FENDER_THICKNESS / 2.0;
    let plate = box_part(FENDER_WIDTH, FENDER_THICKNESS, reach * 2.0, c.color, 0.0);
    put(hull, plate, x, y, 0.0);
}

/// A skirt panel: `z` its rear and front ends, `rake` how far its lower front
/// corner is cut back.
struct Panel {
    z: [f64; 2],
    thickness: f64,
    rake: f64,
}

fn skirt(hull: &mut Node, c: &Chassis, gear: &GearLayout, path: &BeltPath, side: f64, wreck: bool) {
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
    let panels: Vec<Panel> = match gear.skirt {
        SkirtStyle::Booker => {
            even_panels(rear, front_hub.x - front_radius * 0.3, 4, |_| (0.04, 0.0))
        }
        SkirtStyle::Abrams => even_panels(rear, front, 6, |j| {
            // Thick ballistic panels over the front two stations, raked leading edge.
            match j {
                5 => (0.07, 0.34),
                4 => (0.07, 0.0),
                _ => (0.05, 0.0),
            }
        }),
        SkirtStyle::Type99 => even_panels(rear, front - 0.05, 5, |j| {
            (0.04, if j == 4 { 0.2 } else { 0.0 })
        }),
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
        let bolts = (length / 0.24).round().max(2.0) as u32;
        let mut rows = vec![top - 0.03];
        if panel.thickness > 0.06 {
            rows.push(bottom + 0.035);
        }
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
    b.box_faces(half.map(|h| -h), half, Faces::all(rubber(wreck)));
    let strip = shadowed(Arc::new(b.finish()), track_material());
    put(
        hull,
        strip,
        side * (face - 0.02),
        bottom - edge / 2.0 + 0.004,
        (first + last) / 2.0,
    );
}

fn even_panels(rear: f64, front: f64, count: u32, style: impl Fn(u32) -> (f64, f64)) -> Vec<Panel> {
    let length = (front - rear) / f64::from(count);
    (0..count)
        .map(|j| {
            let (thickness, rake) = style(j);
            let z0 = rear + f64::from(j) * length + PANEL_SEAM / 2.0;
            Panel {
                z: [z0, z0 + length - PANEL_SEAM],
                thickness,
                rake,
            }
        })
        .collect()
}

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
        b.stud(DVec3::ZERO, 0.016, BOLT_HEAD - 0.001);
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

/// A lamp in a steel housing behind a three-bar brush guard.
fn headlight(hull: &mut Node, c: &Chassis, side: f64) {
    let (x, y, z) = (side * 0.65, c.deck - 0.1, c.length / 2.0 - 0.07);
    put(hull, box_part(0.17, 0.12, 0.1, c.steel, 0.0), x, y, z);
    put(
        hull,
        box_part(0.12, 0.075, 0.012, HEADLIGHT, 0.0),
        x,
        y,
        z + 0.052,
    );
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
    const NONE: Faces = Faces {
        left: None,
        right: None,
        bottom: None,
        top: None,
        back: None,
        front: None,
    };

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
        let n = normal.abs();
        let uv = if n.y >= n.x && n.y >= n.z {
            [position.x, position.z]
        } else if n.x >= n.z {
            [position.z, position.y]
        } else {
            [position.x, position.y]
        };
        let [r, g, b] = hex_to_linear(color);
        self.mesh.positions.push(position.as_vec3().to_array());
        self.mesh.normals.push(normal.as_vec3().to_array());
        self.mesh
            .uvs
            .push([(uv[0] + 0.5) as f32, (uv[1] + 0.5) as f32]);
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

    /// A planar quad facing away from the shape it bounds: the corner order's own
    /// winding (counterclockwise seen from outside) decides.
    fn quad(&mut self, corners: [DVec3; 4], color: u32) {
        let [a, b, c, _] = corners;
        let facing = (b - a).cross(c - a);
        self.oriented_quad(corners, facing, color);
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
    fn stud(&mut self, base: DVec3, half: f64, height: f64) {
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
            self.oriented_tri([a, b, tip], outward, UNCOLORED);
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
    use super::TOP_RUN_PITCH;
    use crate::geometry::node_bounds;
    use crate::models::{Team, VehicleKind, WreckPart, part, wreck_model};
    use crate::scene::Node;

    const TRACKED: [VehicleKind; 3] = [
        VehicleKind::Scout,
        VehicleKind::Balanced,
        VehicleKind::Heavy,
    ];

    fn triangles(node: &Node) -> usize {
        let mut total = 0;
        node.traverse(DMat4::IDENTITY, &mut |part, _| {
            if let Some(drawable) = &part.drawable {
                total += drawable.mesh.triangle_count();
            }
        });
        total
    }

    /// Wrecks merge parts per material and drop any batch whose meshes disagree on
    /// attributes, so every running-gear mesh must carry normals and UVs, and
    /// colors exactly when its material is vertex-colored.
    #[test]
    fn wrecks_keep_every_running_gear_triangle() {
        for kind in TRACKED {
            let live = tank_model(kind, Team::Red);
            let wreck = wreck_model(kind, Team::Red, WreckPart::Intact);
            assert_eq!(triangles(&wreck), triangles(&live), "{kind:?}");
        }
    }

    /// The renderer slides the top run forward by up to one scroll period; over
    /// the whole slide it must stay within the track's reach (the fender ends).
    #[test]
    fn top_run_stays_on_the_track_while_scrolling() {
        for kind in TRACKED {
            let model = tank_model(kind, Team::Blue);
            let hull = model.find(part::HULL).unwrap();
            let reach = node_bounds(hull, DMat4::IDENTITY).max.z;
            let track = hull.find(part::TRACK_GROUP).unwrap();
            assert!(track.children.len() > 10, "{kind:?} has a top run");
            let run = node_bounds(track, DMat4::IDENTITY);
            assert!(run.min.z > -reach, "{kind:?} rear end");
            assert!(run.max.z + TOP_RUN_PITCH < reach, "{kind:?} front end");
        }
    }
}
