//! Port of `tank-model.ts`: the three tracked chassis (Booker-like scout, Abrams-like
//! balanced tank, Type 99-like heavy) and the dispatch to the Humvee.
//!
//! Tree: `root` (named by kind, scaled by the vehicle scale) holds `hull` and
//! `turret`. The hull holds an empty `track-group` (the Humvee's wheels) and every
//! hull part, its track belts among them; the turret holds `barrel`, whose bore mesh is
//! `muzzle`. See [`super::part`] for the names.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::{DQuat, DVec2, DVec3};

use super::humvee_model::humvee_model;
use super::model_primitives::{
    Cache, cylinder_part, material, paint, put, rotated, shadow_receiver, shadowed,
};
use super::tank_details::{
    Assembly, RING_Z, ROOF_RISE, assembly, engine_vent, gun_rise, hatch_tops, muzzle_z,
    ring_radius, tube_radius, turret_outline,
};
use super::tank_kit::Coat;
use super::tank_surfaces::{
    Grime, HUMVEE_PAINT, Livery, apply_vehicle_livery, ember_material, release_vehicle_weathering,
    vehicle_glass, vehicle_paint,
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
pub(crate) const DARK: u32 = 0x1b1c1d;
pub(crate) const STEEL: u32 = 0x6f706c;
/// The trim every vehicle wears with its team paint: a deep shade of the team color
/// on the lower hull, mantlet, gun fittings, add-on armor and frames. The team
/// color is the body and the deep trim frames it, so the paint reads as a designed
/// two-tone livery and the trim still reads as its team rather than as a black
/// outline round the hull.
const TRIM: [u32; 2] = [0x26406f, 0x5a1b1c];
/// Sight and vision-block glass: a deep teal optical coating that mirrors the sky
/// and glints in the sun, the turret's focal sparkle, rather than a black slot or a
/// flat cyan square.
pub(super) const GLASS_TINT: u32 = 0x1a4148;
pub(crate) const MARKING_WHITE: u32 = 0xdce7ee;
/// Tactical signs (the roof and skirt insignia): a faded, mid-value stencil
/// paint, so a sign reads as a worn marking on the plate rather than as the
/// brightest thing on the vehicle or a hole in its roof from overhead.
pub(crate) const STENCIL: u32 = 0xa3a19a;
/// Machine guns, muzzle brakes, clamp bands and cable: dark gun metal.
const GUNMETAL: u32 = 0x3a3d3e;
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

const MODELS_STORE: &str = "tank_model::MODELS";
static MODELS: Cache<(VehicleKind, Team, bool), Node> = Cache::new(MODELS_STORE);

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

/// Let go of the built vehicle models (about 10 MB of geometry), once everything that
/// needed them has its own copy: presentation uploads every vehicle and wreck before
/// its first round and keeps them, and the Wasm heap can then reuse the space. A later
/// [`tank_model`] builds its model again.
pub fn release_tank_models() {
    MODELS.clear();
    super::shared_values::forget(MODELS_STORE);
    release_vehicle_weathering();
}

/// Every distinct vehicle model's key, in the order a cached group holds them.
fn tank_model_keys() -> Vec<(VehicleKind, Team, bool)> {
    let mut keys = Vec::new();
    for kind in VehicleKind::ALL {
        for team in [Team::Blue, Team::Red] {
            for open_turret_ring in [false, true] {
                // The Humvee's two variants are one model.
                if !(open_turret_ring && kind == VehicleKind::Humvee) {
                    keys.push((kind, team, open_turret_ring));
                }
            }
        }
    }
    keys
}

/// Every vehicle model as one group, children in a fixed order, once all are built:
/// what a page's geometry cache keeps, written in one piece so the models' shared
/// meshes stay shared.
pub fn built_tank_models() -> Option<Node> {
    let mut group = Node::group("vehicles");
    for key in tank_model_keys() {
        group.children.push((*MODELS.get(&key)?).clone());
    }
    Some(group)
}

/// Hand [`tank_model_variant`] the models of a [`built_tank_models`] group read back
/// from a page's geometry cache: each is returned instead of built from now on,
/// unless it is built already. False for a group of another shape.
pub fn offer_tank_models(group: Node) -> bool {
    let keys = tank_model_keys();
    if group.children.len() != keys.len() {
        return false;
    }
    for (key, model) in keys.into_iter().zip(group.children) {
        super::shared_values::adopt(
            MODELS_STORE,
            super::shared_values::key_hash(&key),
            Arc::new(model),
        );
    }
    true
}

/// `tankModel(kind, team, false, openTurretRing)`: `open_turret_ring` cuts the ring
/// well into the hull roof (used by hull-only wrecks whose turret flew off). Wrecks
/// keep the team paint; presentation darkens them.
pub fn tank_model_variant(kind: VehicleKind, team: Team, open_turret_ring: bool) -> Arc<Node> {
    // The Humvee has no turret ring to open: both variants are one model, built once.
    let open_turret_ring = open_turret_ring && kind != VehicleKind::Humvee;
    MODELS.get_or_insert((kind, team, open_turret_ring), || {
        // Team paint, trim and bare steel: the colors the worn finish recognises.
        let humvee = kind == VehicleKind::Humvee;
        let body = if humvee {
            HUMVEE_PAINT[team.index()]
        } else {
            vehicle_paint(team)
        };
        let colors = [body, TRIM[team.index()], STEEL];
        let mut root = if kind == VehicleKind::Humvee {
            humvee_model(team, colors)
        } else {
            tracked_model(kind, team, colors, open_turret_ring)
        };
        let livery = Livery {
            paint: &colors[..2],
            matte: humvee,
            metal: &[STEEL, GUNMETAL],
            signal: SIGNAL_PANEL[team.index()],
            grime: if kind == VehicleKind::Humvee {
                HUMVEE_GRIME
            } else {
                TRACKED_GRIME
            },
        };
        apply_vehicle_livery(&mut root, &livery);
        root.scale = DVec3::splat(vehicle(kind).scale);
        root
    })
}

/// Height of the turret band above the deck, so it never fights the deck's depth.
const OCCLUSION_LIFT: f64 = 0.012;
/// Blended draws sort back to front within a render order, so turret bands
/// interleaved with each other kind of blended draw would cost one draw per
/// vehicle. They draw first among the blended draws, in a render order per kind
/// (each kind's band is its own mesh), so a field of vehicles costs one draw per
/// kind for its turret bands.
const TURRET_OCCLUSION_ORDER: i32 = -3;

/// Unlit black, blended over the deck by the mesh's vertex alpha.
fn occlusion_material() -> Arc<Material> {
    static MATERIAL: crate::models::shared_values::SharedOnce<Material> =
        crate::models::shared_values::SharedOnce::new("tank_model::MATERIAL");
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

/// Ambient occlusion where the turret meets the deck: a soft dark band on the deck
/// round the turret's widest outline, from just under its overhang out by
/// `TURRET_OCCLUSION_REACH`, so the turret sits on the hull rather than hovering
/// over an evenly lit deck. It is a turret part, so it turns with the turret and
/// stretches with its shell, drawn in a blended black material; both teams'
/// tanks of a kind share its mesh. Wrecks leave it out.
const TURRET_OCCLUSION_REACH: f64 = 0.3;
const TURRET_OCCLUSION_INSET: f64 = 0.05;
const TURRET_OCCLUSION_DARKNESS: f32 = 0.82;
/// The longest a corner's mitred offset may grow, as a multiple of the reach.
const TURRET_OCCLUSION_MITRE: f64 = 1.6;

fn turret_occlusion(c: &Chassis) -> Node {
    static MESHES: Cache<VehicleKind, Mesh> = Cache::new("tank_model::MESHES");
    let mesh = MESHES.get_or_insert(c.kind, || turret_occlusion_mesh(c));
    let mut node = Node::mesh(mesh, occlusion_material());
    if let Some(drawable) = &mut node.drawable {
        drawable.render_order = TURRET_OCCLUSION_ORDER + c.pick(0.0, 1.0, 2.0) as i32;
    }
    node.name = part::TURRET_OCCLUSION.into();
    node
}

fn turret_occlusion_mesh(c: &Chassis) -> Mesh {
    let outline = turret_outline(c);
    let n = outline.len();
    let centre = outline.iter().copied().sum::<DVec2>() / n as f64;
    // Each edge's outward normal in plan, pointing away from the outline's centre.
    let normal = |i: usize| {
        let (a, b) = (outline[i], outline[(i + 1) % n]);
        let d = (b - a).normalize_or_zero();
        let perp = DVec2::new(d.y, -d.x);
        if perp.dot((a + b) / 2.0 - centre) < 0.0 {
            -perp
        } else {
            perp
        }
    };
    // Just over the hull roof (the deck plate's top).
    let y = c.deck + ROOF_RISE + OCCLUSION_LIFT;
    let mut positions = Vec::with_capacity(n * 6);
    let mut alpha = Vec::with_capacity(n * 2);
    for (i, &p) in outline.iter().enumerate() {
        let (before, after) = (normal((i + n - 1) % n), normal(i));
        let direction = (before + after).normalize_or_zero();
        let mitre = (1.0 / direction.dot(after).max(1e-3)).min(TURRET_OCCLUSION_MITRE);
        let inner = p - direction * TURRET_OCCLUSION_INSET * mitre;
        let outer = p + direction * TURRET_OCCLUSION_REACH * mitre;
        positions.extend([inner.x, y, inner.y, outer.x, y, outer.y]);
        alpha.extend([TURRET_OCCLUSION_DARKNESS, 0.0]);
    }
    let mut indices = Vec::with_capacity(n * 6);
    for i in 0..n as u32 {
        let j = (i + 1) % n as u32;
        indices.extend([2 * i, 2 * j, 2 * j + 1, 2 * i, 2 * j + 1, 2 * i + 1]);
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
    mesh
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
        turret_ring_well(&mut hull, c.deck, ring_opening(c), ring_radius(c));
    }
    // The renderer still poses the (empty) tread group the Humvee shares; the
    // belt scrolls in its shader.
    hull.children.push(Node::group(part::TRACK_GROUP));
    for side in [-1.0, 1.0] {
        running_gear(&mut hull, c, side);
    }
    hull
}

/// The radius of the hole a thrown turret leaves in the hull roof.
fn ring_opening(c: &Chassis) -> f64 {
    if c.scout { 0.5 } else { 0.65 }
}

/// The height of the open turret ring's floor.
const RING_WELL_FLOOR: f64 = 0.395;

/// A real cutout with visible inner walls and a dark recessed floor, not a black
/// decal on the closed deck. Kept inside the original hull bounds.
fn turret_ring_well(hull: &mut Node, deck: f64, opening: f64, ring_radius: f64) {
    let roof_y = deck + ROOF_RISE;
    let floor_y = RING_WELL_FLOOR;
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
    /// Those hatches as (centre of the opening's top, radius), in the turret's frame.
    pub(super) hatches: [(DVec3, f64); 2],
}

/// The openings a tracked wreck burns out of, in the vehicle's frame before its
/// scale, each as the middle of its top and its half size in plan.
pub(super) struct WreckOpenings {
    /// The hole a thrown turret leaves in the roof, and how deep its well is.
    pub(super) ring: (DVec3, DVec2, f64),
    /// The engine deck's grilles.
    pub(super) engine: (DVec3, DVec2),
    /// The hatches a burnout blows open, on the turret knocked round on its ring.
    pub(super) hatches: [(DVec3, DVec2); 2],
}

pub(super) fn wreck_openings(kind: VehicleKind) -> Option<WreckOpenings> {
    let pose = wreck_pose(kind)?;
    let c = chassis(kind, Team::Blue, [0; 3]);
    let traverse = DQuat::from_rotation_y(pose.turret_yaw);
    Some(WreckOpenings {
        ring: (
            DVec3::new(0.0, c.deck + ROOF_RISE, RING_Z),
            DVec2::splat(ring_opening(&c)),
            c.deck + ROOF_RISE - RING_WELL_FLOOR,
        ),
        engine: engine_vent(&c),
        hatches: pose.hatches.map(|(top, radius)| {
            (
                pose.ring + traverse * (top - pose.ring),
                DVec2::splat(radius),
            )
        }),
    })
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
    let hatches = hatch_tops(&c).map(|(top, radius)| {
        (
            top * stretch + DVec3::new(0.0, 0.0, face_z * (1.0 - stretch.z)),
            radius,
        )
    });
    for (top, radius) in hatches {
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
        hatches,
    })
}

/// The turret with its armor, roof equipment, barrel and its alternative liveries
/// (team marking, unit numbers, extra stowage), the first shown by default.
fn turret(c: &Chassis, team: Team) -> Node {
    let mut turret = Node::group(part::TURRET);
    add_assembly(&mut turret, c, Assembly::Turret);
    turret.children.push(turret_occlusion(c));
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
    let (across, along) = (c.pick(0.85, 1.0, 1.14), c.pick(0.82, 1.0, 1.1));
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
