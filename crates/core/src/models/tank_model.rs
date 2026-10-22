//! Port of `tank-model.ts`: the three tracked chassis (Booker-like scout, Abrams-like
//! balanced tank, Type 99-like heavy) and the dispatch to the Humvee.
//!
//! Tree: `root` (named by kind, scaled by the vehicle scale) holds `hull` and
//! `turret`. The hull holds the tracks group `track-group` (top treads that scroll)
//! and every fixed hull part; the turret holds `barrel`, whose bore mesh is
//! `muzzle`. See [`super::part`] for the names.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::DVec3;

use super::humvee_model::humvee_model;
use super::model_primitives::{
    Cache, cylinder_part, material, paint, put, rotated, shadow_receiver, shadowed,
};
use super::tank_details::{
    Assembly, RING_Z, ROOF_RISE, assembly, gun_rise, hatch_tops, muzzle_z, ring_radius, tube_radius,
};
use super::tank_kit::Coat;
use super::tank_surfaces::{Grime, Livery, apply_livery, ember_material, vehicle_paint};
use super::{Team, VehicleKind, part};
use crate::geometry::{CylinderGeometry, Mesh, ring_geometry};
use crate::scene::{Material, Node};
use crate::sim::data::vehicle;

#[path = "tank_running_gear.rs"]
mod running_gear;
use running_gear::running_gear;

/// The shared dark and steel trim colors: a neutral worn steel, so bare metal
/// reads as metal beside either team's paint rather than as a blue trim.
pub(crate) const DARK: u32 = 0x13232c;
pub(crate) const STEEL: u32 = 0x6f706c;
/// The trim every vehicle wears with its team paint: a deep, greyed shade of the
/// team color on the lower hull, mantlet, gun fittings, add-on armor and frames.
/// The team color is the body and the deep trim frames it, so the paint reads as
/// a designed two-tone livery, each class's add-on armor stands out as armor
/// rather than as dark openings, and the trim still reads as its team.
const TRIM: [u32; 2] = [0x23476e, 0x6a2a25];
const GLASS_TINT: u32 = 0x8adeec;
const MARKING_WHITE: u32 = 0xdce7ee;
/// Machine guns, muzzle brakes, clamp bands and cable: dark gun metal.
const GUNMETAL: u32 = 0x2c2e2f;
/// Olive-drab canvas of tarps, bags and the mantlet's dust cover.
const CANVAS: u32 = 0x4f5236;
const WOOD: u32 = 0x6a4c30;
const TAIL_LIGHT: u32 = 0x9a2016;
/// Lamp lenses: just rough enough for the livery to draw them with the matte
/// fittings.
pub(super) const LENS_ROUGHNESS: f64 = 0.45;
/// Fluorescent air-recognition panels per team: brighter and more saturated than
/// the service paint, so a tank's team reads from the overhead camera even under
/// camouflage. Not the glowing `TEAM_COLORS`: the panel is lit cloth.
const SIGNAL_PANEL: [u32; 2] = [0x1f8fff, 0xf0281c];
const SIGNAL_ROUGHNESS: f64 = 0.9;
/// Dust bands: tracked hulls are dusty up to the fenders, the Humvee to its doors.
const TRACKED_GRIME: Grime = Grime {
    ground: -0.197,
    dust_top: 0.56,
};
const HUMVEE_GRIME: Grime = Grime {
    ground: -0.24,
    dust_top: 0.8,
};

static MODELS: Cache<(VehicleKind, Team, bool), Node> = Cache::new();

/// The turret-ring well's wall: an open cylinder turned inside out. Reversed
/// triangles and normals face the wall inward, so it draws with ordinary
/// front-side paint instead of a back-side shader of its own.
fn inward_wall_geometry(radius: f64, height: f64) -> Mesh {
    let mut mesh = CylinderGeometry {
        radius_top: radius,
        radius_bottom: radius,
        height,
        radial_segments: 24,
        height_segments: 1,
        open_ended: true,
    }
    .build();
    if let Some(indices) = &mut mesh.indices {
        for triangle in indices.as_chunks_mut::<3>().0 {
            triangle.swap(1, 2);
        }
    }
    for normal in &mut mesh.normals {
        *normal = normal.map(|n| -n);
    }
    mesh
}

/// The vehicle model for a kind and team: shared, built once. Clone the node to
/// move its parts independently.
pub fn tank_model(kind: VehicleKind, team: Team) -> Arc<Node> {
    tank_model_variant(kind, team, false)
}

/// `tankModel(kind, team, false, openTurretRing)`: `open_turret_ring` cuts the ring
/// well into the hull roof (used by hull-only wrecks whose turret flew off). Wrecks
/// keep the team paint; presentation darkens them.
pub fn tank_model_variant(kind: VehicleKind, team: Team, open_turret_ring: bool) -> Arc<Node> {
    MODELS.get_or_insert((kind, team, open_turret_ring), || {
        // Team paint, trim and bare steel: the colors the worn finish recognises.
        let colors = [vehicle_paint(team), TRIM[team.index()], STEEL];
        let mut root = if kind == VehicleKind::Humvee {
            humvee_model(colors, SIGNAL_PANEL[team.index()])
        } else {
            tracked_model(kind, team, colors, open_turret_ring)
        };
        let livery = Livery {
            paint: &colors[..2],
            metal: &[STEEL, GUNMETAL],
            signal: SIGNAL_PANEL[team.index()],
            grime: if kind == VehicleKind::Humvee {
                HUMVEE_GRIME
            } else {
                TRACKED_GRIME
            },
        };
        apply_livery(&mut root, &livery);
        root.scale = DVec3::splat(vehicle(kind).scale);
        root
    })
}

/// Chassis measurements shared by the hull and turret builders.
pub(super) struct Chassis {
    pub(super) kind: VehicleKind,
    pub(super) team: Team,
    pub(super) scout: bool,
    pub(super) heavy: bool,
    pub(super) color: u32,
    pub(super) shade: u32,
    pub(super) steel: u32,
    pub(super) signal: u32,
    pub(super) overall_width: f64,
    pub(super) width: f64,
    pub(super) length: f64,
    pub(super) deck: f64,
}

impl Chassis {
    /// This chassis' value of a measurement given for each tracked kind.
    pub(super) fn pick(&self, scout: f64, balanced: f64, heavy: f64) -> f64 {
        match self.kind {
            VehicleKind::Scout => scout,
            VehicleKind::Heavy => heavy,
            _ => balanced,
        }
    }
}

/// The measurements of a tracked chassis painted in `colors` (paint, shade, steel).
fn chassis(kind: VehicleKind, team: Team, colors: [u32; 3]) -> Chassis {
    let [color, shade, steel] = colors;
    // Dimensions include the tracks and skirts, not just the center armor slab.
    // Hull length/overall width: compact scout ~1.94, Abrams/Type 99 ~2.17.
    let (overall_width, length, deck) = match kind {
        VehicleKind::Scout => (2.3, 4.415, 0.58),
        VehicleKind::Heavy => (2.5, 5.425, 0.67),
        _ => (2.42, 5.244, 0.65),
    };
    Chassis {
        kind,
        team,
        scout: kind == VehicleKind::Scout,
        heavy: kind == VehicleKind::Heavy,
        color,
        shade,
        steel,
        signal: SIGNAL_PANEL[team.index()],
        overall_width,
        width: overall_width - 0.42,
        length,
        deck,
    }
}

/// A tracked tank in `colors` (paint, shade, steel), before the vehicle scale and
/// worn finish.
fn tracked_model(kind: VehicleKind, team: Team, colors: [u32; 3], open_turret_ring: bool) -> Node {
    let chassis = chassis(kind, team, colors);
    let mut root = Node::group(kind.as_str());
    root.children.push(hull(&chassis, open_turret_ring));
    root.children.push(turret(&chassis, team));
    root
}

/// The material a merged [`Coat`] is painted with on this chassis. Paint, shade
/// and steel are the colors the worn finish recognises.
fn coat_material(coat: Coat, c: &Chassis) -> Arc<Material> {
    match coat {
        Coat::Paint => material(c.color, 0.18, 0.58),
        Coat::Shade => material(c.shade, 0.18, 0.58),
        Coat::Steel => paint(c.steel),
        Coat::Dark => paint(DARK),
        Coat::Gunmetal => material(GUNMETAL, 0.55, 0.45),
        Coat::Glass => material(GLASS_TINT, 0.3, 0.12),
        Coat::Canvas => material(CANVAS, 0.0, 0.92),
        Coat::Wood => material(WOOD, 0.0, 0.85),
        Coat::Marking => paint(MARKING_WHITE),
        // Lenses a few centimetres across draw with the matte fittings (one
        // draw fewer per hull) rather than in a glossy material of their own.
        Coat::TailLight => material(TAIL_LIGHT, 0.0, LENS_ROUGHNESS),
        Coat::Signal => material(c.signal, 0.0, SIGNAL_ROUGHNESS),
    }
}

/// Append an assembly's merged meshes to `group`, one shadowed part per coat.
fn add_assembly(group: &mut Node, c: &Chassis, which: Assembly) {
    for (coat, mesh) in assembly(c, which).iter() {
        group
            .children
            .push(shadowed(mesh.clone(), coat_material(*coat, c)));
    }
}

fn hull(c: &Chassis, open_turret_ring: bool) -> Node {
    let mut hull = Node::group(part::HULL);
    // Armor, deck fittings and end plates; the lower armor (shade) and the team
    // paint come first.
    let armor = if open_turret_ring {
        Assembly::OpenHull
    } else {
        Assembly::Hull
    };
    add_assembly(&mut hull, c, armor);
    if open_turret_ring {
        let opening = if c.scout { 0.5 } else { 0.65 };
        turret_ring_well(&mut hull, c.deck, opening, ring_radius(c));
    }
    let track_group_index = hull.children.len();
    let mut track_group = Node::group(part::TRACK_GROUP);
    for side in [-1.0, 1.0] {
        running_gear(&mut hull, &mut track_group, c, side);
    }
    hull.children.insert(track_group_index, track_group);
    hull
}

/// A real cutout with visible inner walls and a dark recessed floor, not a black
/// decal on the closed deck. Kept inside the original hull bounds.
fn turret_ring_well(hull: &mut Node, deck: f64, opening: f64, ring_radius: f64) {
    let roof_y = deck + ROOF_RISE;
    let floor_y = 0.395;
    let mut rim_mesh = ring_geometry(opening, ring_radius, 24);
    rim_mesh.rotate_x(-PI / 2.0);
    let rim = shadowed(Arc::new(rim_mesh), material(0x4a5358, 0.55, 0.8));
    put(hull, rim, 0.0, roof_y, RING_Z);
    let wall_mesh = inward_wall_geometry(opening, roof_y - floor_y);
    let wall = shadow_receiver(Arc::new(wall_mesh), material(0x3b454a, 0.15, 0.9));
    put(hull, wall, 0.0, (roof_y + floor_y) / 2.0, RING_Z);
    // The well only opens on wrecks: its floor smoulders.
    let mut floor = cylinder_part(opening, 0.02, 0x293238, 24);
    if let Some(drawable) = &mut floor.drawable {
        drawable.material = ember_material();
    }
    put(hull, floor, 0.0, floor_y - 0.01, RING_Z);
}

/// How a tracked tank's wreck sits apart from the live model: its turret knocked
/// round on the ring and its gun sagging on the trunnions, with embers glowing in
/// the hatches the blast blew open.
pub(super) struct WreckPose {
    /// Turret traverse about the ring centre (radians) and the ring centre.
    pub(super) turret_yaw: f64,
    pub(super) ring: DVec3,
    /// Gun droop about the trunnion axis (radians, muzzle down) and a point on it.
    pub(super) gun_droop: f64,
    pub(super) trunnion: DVec3,
    /// Glowing discs over the open hatches, in the turret's frame.
    pub(super) embers: Node,
}

/// Ember disc thickness, and how much of a hatch opening it fills.
const EMBER_DISC: f64 = 0.006;
const EMBER_FILL: f64 = 0.94;

/// The wreck pose of a tracked chassis (`None` for the Humvee).
pub(super) fn wreck_pose(kind: VehicleKind) -> Option<WreckPose> {
    if kind == VehicleKind::Humvee {
        return None;
    }
    let c = chassis(kind, Team::Blue, [0; 3]);
    let mut embers = Node::group("embers");
    for (top, radius) in hatch_tops(&c) {
        let mut disc = cylinder_part(radius * EMBER_FILL, EMBER_DISC, 0x000000, 16);
        if let Some(drawable) = &mut disc.drawable {
            drawable.material = ember_material();
        }
        put(&mut embers, disc, top.x, top.y + EMBER_DISC / 2.0, top.z);
    }
    Some(WreckPose {
        turret_yaw: c.pick(0.38, -0.3, 0.24),
        ring: DVec3::new(0.0, 0.0, RING_Z),
        gun_droop: c.pick(0.07, 0.085, 0.06),
        trunnion: DVec3::new(0.0, c.deck + gun_rise(&c), c.pick(1.0, 1.13, 1.3)),
        embers,
    })
}

/// The turret with its armor, roof equipment, barrel and its alternative liveries
/// (team marking, unit numbers, extra stowage), the first shown by default.
fn turret(c: &Chassis, team: Team) -> Node {
    let mut turret = Node::group(part::TURRET);
    add_assembly(&mut turret, c, Assembly::Turret);
    turret.children.push(barrel(c));
    for (variant, name) in (0u8..).zip(part::LIVERIES) {
        let mut livery = Node::group(name);
        livery.visible = variant == 0;
        add_assembly(&mut livery, c, Assembly::Marking(team, variant));
        turret.children.push(livery);
    }
    turret
}

/// The gun (see `tank_details::gun`) and the bore disc named `muzzle` whose
/// world position is the launch point.
fn barrel(c: &Chassis) -> Node {
    let mut barrel = Node::group(part::BARREL);
    add_assembly(&mut barrel, c, Assembly::Barrel);
    let tube_radius = tube_radius(c);
    let mut bore = rotated(
        cylinder_part(tube_radius * 0.76, 0.008, DARK, 12),
        PI / 2.0,
        0.0,
        0.0,
    );
    bore.name = part::MUZZLE.into();
    put(
        &mut barrel,
        bore,
        0.0,
        c.deck + gun_rise(c),
        muzzle_z(c) + 0.005,
    );
    barrel
}
