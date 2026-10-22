//! Port of `tank-model.ts`: the three tracked chassis (Booker-like scout, Abrams-like
//! balanced tank, Type 99-like heavy) and the dispatch to the Humvee.
//!
//! Tree: `root` (named by kind, scaled by the vehicle scale) holds `hull` and
//! `turret`. The hull holds an empty `track-group` (the Humvee's wheels) and every
//! hull part, its track belts among them; the turret holds `barrel`, whose bore mesh is
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
use super::tank_surfaces::{
    Grime, Livery, apply_livery, ember_material, vehicle_glass, vehicle_paint,
};
use super::{Team, VehicleKind, part};
use crate::geometry::{Attribute, CylinderGeometry, Mesh, VERTEX_ALPHA, ring_geometry};
use crate::scene::{Material, Node, Shading};
use crate::sim::data::vehicle;

#[path = "tank_running_gear.rs"]
mod running_gear;
use running_gear::running_gear;

/// The shared dark and steel trim colors: a neutral worn steel, so bare metal
/// reads as metal beside either team's paint rather than as a blue trim.
pub(crate) const DARK: u32 = 0x13232c;
pub(crate) const STEEL: u32 = 0x6f706c;
/// The trim every vehicle wears with its team paint: a deep shade of the team color
/// on the lower hull, mantlet, gun fittings, add-on armor and frames. The team
/// color is the body and the deep trim frames it, so the paint reads as a designed
/// two-tone livery and the trim still reads as its team rather than as a black
/// outline round the hull.
const TRIM: [u32; 2] = [0x1c2f57, 0x4a1a14];
/// Sight and vision-block glass: a deep blue-green tint that mirrors the sky.
pub(super) const GLASS_TINT: u32 = 0x25505a;
const MARKING_WHITE: u32 = 0xdce7ee;
/// Tactical signs (the roof and skirt insignia): a faded, mid-value stencil
/// paint, so a sign reads as a worn marking on the plate rather than as the
/// brightest thing on the vehicle or a hole in its roof from overhead.
pub(crate) const STENCIL: u32 = 0xa3a19a;
/// Machine guns, muzzle brakes, clamp bands and cable: dark gun metal.
const GUNMETAL: u32 = 0x2c2e2f;
/// Olive-drab canvas of tarps, bags and the mantlet's dust cover.
const CANVAS: u32 = 0x4f5236;
const WOOD: u32 = 0x6a4c30;
const TAIL_LIGHT: u32 = 0x9a2016;
/// Lamp lenses: just rough enough for the livery to draw them with the matte
/// fittings.
pub(super) const LENS_ROUGHNESS: f64 = 0.45;
/// The livery's recognition color per team, kept matte and burnt off wrecks. No
/// part wears it today: the deep team paint reads from overhead by itself, and a
/// flat fluorescent sheet read as a missing texture.
const SIGNAL_PANEL: [u32; 2] = [0x1f8fff, 0xf0281c];
/// Dust bands: tracked hulls are dusty up to the fenders, the Humvee to its doors.
const TRACKED_GRIME: Grime = Grime {
    ground: -0.197,
    dust_top: 0.66,
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
    // The Humvee has no turret ring to open: both variants are one model, built once.
    let open_turret_ring = open_turret_ring && kind != VehicleKind::Humvee;
    MODELS.get_or_insert((kind, team, open_turret_ring), || {
        // Team paint, trim and bare steel: the colors the worn finish recognises.
        let colors = [vehicle_paint(team), TRIM[team.index()], STEEL];
        let mut root = if kind == VehicleKind::Humvee {
            humvee_model(team, colors)
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
        root.children.push(contact_shadow(kind));
        root.scale = DVec3::splat(vehicle(kind).scale);
        root
    })
}

/// The soft contact shadow every vehicle sits on: a dark pool on the ground under
/// it, darkest beneath the hull and fading out a little beyond its tracks or
/// wheels, so it rests on the ground under any sun rather than hovering over its
/// cast shadow. Not part of the hull (whose bounds are its hit box) nor of a wreck.
const CONTACT_SHADOW_REACH: f64 = 0.32;
/// Its darkness under the hull (alpha over the ground) and how far in from its edge
/// (a share of its half extent) it reaches that darkness.
const CONTACT_SHADOW_DARKNESS: f32 = 0.72;
const CONTACT_SHADOW_CORE: f64 = 0.72;
/// Height above the ground line, so it never fights the ground's depth.
const CONTACT_SHADOW_LIFT: f64 = 0.012;

fn contact_shadow(kind: VehicleKind) -> Node {
    let (half_width, half_length, ground) = if kind == VehicleKind::Humvee {
        (1.06, 2.27, HUMVEE_GRIME.ground)
    } else {
        let c = chassis(kind, Team::Blue, [0; 3]);
        let reach = c.length * 1.235 / 2.46 + 0.0175;
        (c.overall_width / 2.0, reach, TRACKED_GRIME.ground)
    };
    let mut node = Node::mesh(contact_shadow_mesh(), contact_shadow_material());
    node.position = DVec3::new(0.0, ground + CONTACT_SHADOW_LIFT, 0.0);
    node.scale = DVec3::new(
        half_width + CONTACT_SHADOW_REACH,
        1.0,
        half_length + CONTACT_SHADOW_REACH,
    );
    node.name = "contact-shadow".into();
    node
}

/// A unit rounded square facing up: full darkness inside `CONTACT_SHADOW_CORE`,
/// fading to none at its rounded rim. Shared by every vehicle (each scales it to
/// its footprint), so all of them draw in one call.
fn contact_shadow_mesh() -> Arc<Mesh> {
    static MESH: std::sync::OnceLock<Arc<Mesh>> = std::sync::OnceLock::new();
    MESH.get_or_init(|| {
        const ARC: usize = 3;
        const ROUND: f64 = 0.16;
        let mut positions = Vec::new();
        let mut alpha = Vec::new();
        for (extent, round, strength) in [
            (CONTACT_SHADOW_CORE, 0.0, CONTACT_SHADOW_DARKNESS),
            (1.0, ROUND, 0.0),
        ] {
            for (corner, (sx, sz)) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]
                .into_iter()
                .enumerate()
            {
                for k in 0..ARC {
                    let angle = (corner as f64 + k as f64 / (ARC - 1) as f64) * PI / 2.0;
                    let x = sx * (extent - round) + round * angle.cos();
                    let z = sz * (extent - round) + round * angle.sin();
                    positions.extend([x, 0.0, z]);
                    alpha.push(strength);
                }
            }
        }
        let ring = (4 * ARC) as u32;
        // The core: a fan over its corners; then the band out to the rim.
        let mut indices: Vec<u32> = (1..ring - 1).flat_map(|i| [0, i + 1, i]).collect();
        for i in 0..ring {
            let j = (i + 1) % ring;
            indices.extend([i, j, ring + j, i, ring + j, ring + i]);
        }
        let mut mesh = Mesh::from_f64(&positions, &[], &[], Some(indices));
        mesh.compute_vertex_normals();
        if mesh.normals.iter().map(|n| n[1]).sum::<f32>() < 0.0
            && let Some(indices) = &mut mesh.indices
        {
            for triangle in indices.as_chunks_mut::<3>().0 {
                triangle.swap(1, 2);
            }
            mesh.compute_vertex_normals();
        }
        mesh.colors = vec![[0.0, 0.0, 0.0]; mesh.positions.len()];
        mesh.set_attribute(Attribute::vertex(VERTEX_ALPHA, 1, alpha));
        Arc::new(mesh)
    })
    .clone()
}

/// Unlit black, blended over the ground by the mesh's vertex alpha.
fn contact_shadow_material() -> Arc<Material> {
    static MATERIAL: std::sync::OnceLock<Arc<Material>> = std::sync::OnceLock::new();
    MATERIAL
        .get_or_init(|| {
            Arc::new(Material {
                shading: Shading::Basic,
                vertex_colors: true,
                transparent: true,
                depth_write: false,
                polygon_offset: Some((-1.0, -4.0)),
                ..Material::standard(0x000000, 0.0, 1.0)
            })
        })
        .clone()
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
        Coat::Glass => vehicle_glass(GLASS_TINT),
        Coat::Canvas => material(CANVAS, 0.0, 0.92),
        Coat::Wood => material(WOOD, 0.0, 0.85),
        Coat::Marking => paint(MARKING_WHITE),
        Coat::Stencil => paint(STENCIL),
        // Lenses a few centimetres across draw with the matte fittings (one
        // draw fewer per hull) rather than in a glossy material of their own.
        Coat::TailLight => material(TAIL_LIGHT, 0.0, LENS_ROUGHNESS),
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
    // The renderer still poses the (empty) tread group the Humvee shares; the
    // belt scrolls in its shader.
    hull.children.push(Node::group(part::TRACK_GROUP));
    for side in [-1.0, 1.0] {
        running_gear(&mut hull, c, side);
    }
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
    let (stretch, face_z) = turret_stretch(&c);
    for (top, radius) in hatch_tops(&c) {
        let top = top * stretch + DVec3::new(0.0, 0.0, face_z * (1.0 - stretch.z));
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
    for (variant, name) in (0u8..).zip(part::LIVERIES) {
        let mut livery = Node::group(name);
        livery.visible = variant == 0;
        add_assembly(&mut livery, c, Assembly::Marking(team, variant));
        turret.children.push(livery);
    }
    // The turret shell (not the gun, whose muzzle is the combat launch point)
    // stretches per class in plan about the mantlet face, so the gun still leaves
    // the mantlet where it did.
    let (stretch, face_z) = turret_stretch(c);
    for child in &mut turret.children {
        child.scale = stretch;
        child.position = DVec3::new(0.0, 0.0, face_z * (1.0 - stretch.z));
    }
    turret.children.push(barrel(c));
    turret
}

/// The turret shell's plan stretch (x across, z along) per class and the mantlet
/// face it stretches about: the Skipper's turret is a small, tight box on its
/// short hull and the Big Rig's a broad, long slab over its long one, so the
/// three classes part by silhouette at card size and from the overhead camera.
fn turret_stretch(c: &Chassis) -> (DVec3, f64) {
    let (across, along) = (c.pick(0.9, 1.0, 1.1), c.pick(0.88, 1.0, 1.08));
    (DVec3::new(across, 1.0, along), c.pick(1.0, 1.13, 1.3))
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
