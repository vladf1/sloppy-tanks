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
    Assembly, ROOF_RISE, assembly, gun_rise, muzzle_z, ring_radius, tube_radius,
};
use super::tank_kit::Coat;
use super::tank_surfaces::{Finish, apply_tank_surface, vehicle_paint};
use super::{Team, VehicleKind, part};
use crate::geometry::math::scale_hex_color;
use crate::geometry::{CylinderGeometry, Mesh, ring_geometry};
use crate::scene::{Material, Node};

#[path = "tank_running_gear.rs"]
mod running_gear;
use running_gear::running_gear;

/// Wreck paint and the shared dark/steel trim colors.
pub(crate) const WRECK_PAINT: u32 = 0x3c4650;
pub(crate) const DARK: u32 = 0x13232c;
pub(crate) const STEEL: u32 = 0x637581;
pub(crate) const WRECK_STEEL: u32 = 0x37424c;
/// Shade paint is team paint darkened in linear space by this factor.
pub(crate) const SHADE_FACTOR: f64 = 0.62;
const GLASS_TINT: u32 = 0x8adeec;
const MARKING_WHITE: u32 = 0xdce7ee;
/// Machine guns and cable: dark blued steel.
const GUNMETAL: u32 = 0x2b3134;
/// Olive-drab canvas of tarps, bags and the mantlet's dust cover.
const CANVAS: u32 = 0x4f5236;
const WOOD: u32 = 0x6a4c30;
const TAIL_LIGHT: u32 = 0x9a2016;

/// The darker variant of a team paint (`new THREE.Color(c).multiplyScalar(0.62)`).
pub(crate) fn shade_of(color: u32) -> u32 {
    scale_hex_color(color, SHADE_FACTOR)
}

static MODELS: Cache<(VehicleKind, Team, bool, bool), Node> = Cache::new();

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
    tank_model_variant(kind, team, false, false)
}

/// `tankModel(kind, team, wreck, openTurretRing)`: `wreck` swaps the paint for
/// burnt colors; `open_turret_ring` cuts the ring well into the hull roof (used by
/// hull-only wrecks whose turret flew off).
pub fn tank_model_variant(
    kind: VehicleKind,
    team: Team,
    wreck: bool,
    open_turret_ring: bool,
) -> Arc<Node> {
    MODELS.get_or_insert((kind, team, wreck, open_turret_ring), || {
        if kind == VehicleKind::Humvee {
            humvee_model(team, wreck)
        } else {
            tracked_model(kind, team, wreck, open_turret_ring)
        }
    })
}

/// Chassis measurements shared by the hull and turret builders.
pub(super) struct Chassis {
    pub(super) kind: VehicleKind,
    pub(super) scout: bool,
    pub(super) heavy: bool,
    pub(super) color: u32,
    pub(super) shade: u32,
    pub(super) steel: u32,
    pub(super) overall_width: f64,
    pub(super) width: f64,
    pub(super) length: f64,
    pub(super) deck: f64,
}

fn tracked_model(kind: VehicleKind, team: Team, wreck: bool, open_turret_ring: bool) -> Node {
    let scout = kind == VehicleKind::Scout;
    let heavy = kind == VehicleKind::Heavy;
    let color = if wreck {
        WRECK_PAINT
    } else {
        vehicle_paint(team)
    };
    // Dimensions include the tracks and skirts, not just the center armor slab.
    // Hull length/overall width: compact scout ~1.94, Abrams/Type 99 ~2.17.
    let overall_width = if scout {
        2.3
    } else if heavy {
        2.5
    } else {
        2.42
    };
    let chassis = Chassis {
        kind,
        scout,
        heavy,
        color,
        shade: if wreck { DARK } else { shade_of(color) },
        steel: if wreck { WRECK_STEEL } else { STEEL },
        overall_width,
        width: overall_width - 0.42,
        length: if scout {
            4.415
        } else if heavy {
            5.425
        } else {
            5.244
        },
        deck: if scout {
            0.58
        } else if heavy {
            0.67
        } else {
            0.65
        },
    };
    let mut root = Node::group(kind.name());
    root.children.push(hull(&chassis, open_turret_ring));
    root.children.push(turret(&chassis, team));
    root.scale = DVec3::splat(kind.scale());
    let finish = if wreck {
        Finish::Wrecked
    } else {
        Finish::Fresh {
            steel: chassis.steel,
        }
    };
    apply_tank_surface(
        &mut root,
        &[chassis.color, chassis.shade, chassis.steel],
        finish,
    );
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
        Coat::TailLight => material(TAIL_LIGHT, 0.1, 0.35),
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
    put(hull, rim, 0.0, roof_y, -0.12);
    let wall_mesh = inward_wall_geometry(opening, roof_y - floor_y);
    let wall = shadow_receiver(Arc::new(wall_mesh), material(0x3b454a, 0.15, 0.9));
    put(hull, wall, 0.0, (roof_y + floor_y) / 2.0, -0.12);
    let floor = cylinder_part(opening, 0.02, 0x293238, 24);
    put(hull, floor, 0.0, floor_y - 0.01, -0.12);
}

/// The turret with its armor, roof equipment, barrel and team marking.
fn turret(c: &Chassis, team: Team) -> Node {
    let mut turret = Node::group(part::TURRET);
    add_assembly(&mut turret, c, Assembly::Turret);
    turret.children.push(barrel(c));
    add_assembly(&mut turret, c, Assembly::Marking(team));
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
