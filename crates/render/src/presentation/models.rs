//! Models presentation owns: world-space HUD (tank bars, reticle, player ring
//! and spawn pulse), pickup pallets and their ground markings, mines and debris
//! pieces. Ports of `tank-bars.ts`, `reticle.ts` and the model parts of
//! `presentation.ts`; pickup gems, flags, pads, floors and scenery come from
//! `sloppy_core::models`.
//!
//! Named nodes are joints that instances pose or hide; their names are the
//! constants below. Materials are built once per model; presentation caches the
//! prepared models for the whole session.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::{DVec2, DVec3};
use sloppy_core::geometry::{
    Mesh, RingGeometry, Shape, circle_geometry, cylinder_geometry, merge_geometries,
    plane_geometry, ring_geometry, rounded_box_geometry, shape_geometry, tetrahedron_geometry,
};
use sloppy_core::models::{
    BarrelScrap, DEFAULT_BOX_RADIUS, PICKUP_ATLAS_PATH, PICKUP_PAINT_CELL, TEAM_COLORS,
    atlas_cell_uv, barrel_scrap_geometry, cylinder_part, paint, pickup_pallet, put, shadowed,
    siding_box, trunk_fragment,
};
use sloppy_core::scene::{Effect, Material, Node, Side, TextureRef};
use sloppy_core::sim::data::{MINE_RADIUS, pickup};
use sloppy_core::sim::{FragmentShape, PickupKind, Team};

use super::hud::{CRITICAL_COLOR, HEALTHY_COLOR, METER_LOW_Y, WARNING_COLOR};
use crate::effects::spawn_pad_decks::SpawnPadDecks;

/// Joint names.
pub mod joint {
    /// Tank bar hull fill, one per health color; scaled along x by the ratio.
    pub const BAR_FILLS: [&str; 3] = ["bar-fill-high", "bar-fill-mid", "bar-fill-low"];
    /// The dark warm chunk a hit took, trailing the fill down; scaled along x.
    pub const BAR_TRAIL: &str = "bar-trail";
    pub const BAR_RANKS: [&str; 3] = ["bar-rank-1", "bar-rank-2", "bar-rank-3"];
    pub const BAR_SHIELD: &str = "bar-shield";
    pub const BAR_SHIELD_FILL: &str = "bar-shield-fill";
    pub const BAR_SPAWN: &str = "bar-spawn";
    pub const BAR_SPAWN_FILL: &str = "bar-spawn-fill";
    /// Reticle ink and centre, one set per state.
    pub const RETICLE_READY: &str = "reticle-ready";
    pub const RETICLE_RELOADING: &str = "reticle-reloading";
    pub const RETICLE_CONFIRMED: &str = "reticle-confirmed";
    /// A supply drop's sprayed circle, full or faded, and its refill ticks.
    pub const PICKUP_RING: &str = "pickup-ring";
    pub const PICKUP_RING_DIM: &str = "pickup-ring-dim";
    pub const PICKUP_REFILL: &str = "pickup-refill";
    /// The blinking cap of a mine.
    pub const MINE_LIGHT: &str = "mine-light";
}

/// Custom effects presentation registers (WGSL beside this file).
pub const FLAG_CLOTH_EFFECT: &str = sloppy_core::models::effects_props::FLAG_CLOTH;
pub const PICKUP_REFILL_EFFECT: &str = "pickup-refill";

pub const SPAWN_PROTECTION_COLOR: u32 = 0xffdf86;
/// The hull fill's full width and height; the track and rim frame it.
const BAR_WIDTH: f64 = 1.7;
const BAR_FILL_HEIGHT: f64 = 0.18;
/// The bar's outer width, which presentation keeps to a share of the screen.
pub const BAR_RIM_WIDTH: f64 = 1.84;
const BAR_RIM_HEIGHT: f64 = 0.32;
/// Hull segments, as the page HUD's bar shows them.
const BAR_SEGMENTS: usize = 4;
const METER_FILL_WIDTH: f64 = 1.34;
const METER_FILL_X: f64 = -0.51;

fn arc(mesh: Mesh) -> Arc<Mesh> {
    Arc::new(mesh)
}

/// World-space HUD paint: unlit, drawn over everything in the transparent pass.
fn hud_material(color: u32) -> Arc<Material> {
    Arc::new(Material {
        depth_test: false,
        depth_write: false,
        transparent: true,
        tone_mapped: false,
        ..Material::basic(color)
    })
}

fn mesh(mesh: Arc<Mesh>, material: Arc<Material>, render_order: i32) -> Node {
    let mut node = Node::mesh(mesh, material);
    if let Some(drawable) = &mut node.drawable {
        drawable.render_order = render_order;
    }
    node
}

fn at(mut node: Node, x: f64, y: f64, z: f64) -> Node {
    node.position = DVec3::new(x, y, z);
    node
}

fn named(mut node: Node, name: &str) -> Node {
    node.name = name.into();
    node
}

fn flat(mut node: Node) -> Node {
    node.set_rotation_euler(-PI / 2.0, 0.0, 0.0);
    node
}

fn polygon(points: &[[f64; 2]]) -> Mesh {
    let points: Vec<DVec2> = points.iter().map(|p| DVec2::new(p[0], p[1])).collect();
    shape_geometry(&[Shape::from_points(&points)])
}

fn translated(mut mesh: Mesh, x: f64, y: f64, z: f64) -> Mesh {
    mesh.translate(x, y, z);
    mesh
}

/// Pickup armor badge or spawn timer: a rimmed badge and a fill bar.
fn protection_meter(name: &str, fill_name: &str, color: u32, segmented: bool) -> Node {
    let paint = hud_material(color);
    let dark = hud_material(0x07141f);
    let badge_shape = arc(polygon(&[
        [-0.13, 0.12],
        [0.13, 0.12],
        [0.11, -0.035],
        [0.0, -0.14],
        [-0.11, -0.035],
    ]));
    let mut group = Node::group(name);
    let mut rim = at(mesh(badge_shape.clone(), dark.clone(), 13), -0.8, 0.0, 0.0);
    rim.scale = DVec3::splat(1.3);
    group.children.push(rim);
    group
        .children
        .push(at(mesh(badge_shape, paint.clone(), 14), -0.8, 0.0, 0.0));
    group.children.push(at(
        mesh(arc(plane_geometry(1.42, 0.18)), dark.clone(), 13),
        0.16,
        0.0,
        0.0,
    ));
    let fill = arc(translated(
        plane_geometry(METER_FILL_WIDTH, 0.1),
        METER_FILL_WIDTH / 2.0,
        0.0,
        0.0,
    ));
    group.children.push(named(
        at(mesh(fill, paint, 14), METER_FILL_X, 0.0, 0.0),
        fill_name,
    ));
    // Three charge sections distinguish pickup armor from the continuous spawn timer.
    if segmented {
        let divider = arc(plane_geometry(0.035, 0.12));
        for fraction in [1.0 / 3.0, 2.0 / 3.0] {
            group.children.push(at(
                mesh(divider.clone(), dark.clone(), 15),
                METER_FILL_X + METER_FILL_WIDTH * fraction,
                0.0,
                0.0,
            ));
        }
    }
    group.visible = false;
    group
}

/// A rectangle with rounded corners, centred on the origin, facing +z.
fn rounded_rect(width: f64, height: f64, radius: f64) -> Mesh {
    shape_geometry(&[rounded_rect_shape(width, height, radius)])
}

fn rounded_rect_shape(width: f64, height: f64, radius: f64) -> Shape {
    const CORNER_STEPS: usize = 4;
    let r = radius.min(height / 2.0).min(width / 2.0);
    let (hw, hh) = (width / 2.0 - r, height / 2.0 - r);
    let mut points = Vec::with_capacity(4 * (CORNER_STEPS + 1));
    for (i, (cx, cy)) in [(hw, hh), (-hw, hh), (-hw, -hh), (hw, -hh)]
        .into_iter()
        .enumerate()
    {
        for step in 0..=CORNER_STEPS {
            let angle = (i as f64 + step as f64 / CORNER_STEPS as f64) * PI / 2.0;
            points.push([cx + r * angle.cos(), cy + r * angle.sin()]);
        }
    }
    let points: Vec<DVec2> = points.iter().map(|p| DVec2::new(p[0], p[1])).collect();
    Shape::from_points(&points)
}

/// World-space HUD paint with its own opacity.
fn hud_paint(color: u32, opacity: f32) -> Arc<Material> {
    Arc::new(Material {
        opacity,
        ..(*hud_material(color)).clone()
    })
}

/// `createTankBar(team)`: the hull bar with its three health colors, the shield
/// and spawn meters, and one to three rank chevrons. Billboarded by the instance.
/// The fill is green, amber or red like the page HUD's hull bar, whatever the team: a
/// team-coloured pointer under the bar names the side and ties the bar to its tank. A
/// dark rounded rim keeps it legible on bright grass and pale concrete; the empty track
/// stays a visible navy so lost hull reads at a glance, a dark ember trail shows the
/// chunk the last hit took, and a sheen and segment notches give the fill the page
/// HUD's look.
pub fn tank_bar(team: Team) -> Node {
    let mut bar = Node::group("tank-bar");
    // The rim and, under it, the pointer's dark notch, in one mesh; the team's colour
    // fills the notch.
    let below = -BAR_RIM_HEIGHT / 2.0;
    let notch = [
        DVec2::new(-0.15, below + 0.01),
        DVec2::new(0.0, below - 0.15),
        DVec2::new(0.15, below + 0.01),
    ];
    bar.children.push(mesh(
        arc(shape_geometry(&[
            rounded_rect_shape(BAR_RIM_WIDTH, BAR_RIM_HEIGHT, 0.08),
            Shape::from_points(&notch),
        ])),
        hud_paint(0x050a12, 0.9),
        10,
    ));
    bar.children.push(at(
        mesh(
            arc(polygon(&[
                [-0.095, below - 0.005],
                [0.0, below - 0.105],
                [0.095, below - 0.005],
            ])),
            hud_material(TEAM_COLORS[team.index()]),
            12,
        ),
        0.0,
        0.0,
        0.006,
    ));
    bar.children.push(at(
        mesh(
            arc(rounded_rect(BAR_WIDTH + 0.06, BAR_FILL_HEIGHT + 0.06, 0.06)),
            hud_paint(0x1a2638, 0.96),
            11,
        ),
        0.0,
        0.0,
        0.005,
    ));
    let fill = arc(translated(
        rounded_rect(BAR_WIDTH, BAR_FILL_HEIGHT, 0.045),
        BAR_WIDTH / 2.0,
        0.0,
        0.0,
    ));
    let mut trail = named(
        at(
            mesh(fill.clone(), hud_paint(0xa8432a, 0.95), 12),
            -BAR_WIDTH / 2.0,
            0.0,
            0.008,
        ),
        joint::BAR_TRAIL,
    );
    trail.visible = false;
    bar.children.push(trail);
    let colors = [HEALTHY_COLOR, WARNING_COLOR, CRITICAL_COLOR];
    // A glassy highlight along the top of the fill, scaled with it.
    let sheen = arc(translated(
        rounded_rect(BAR_WIDTH - 0.03, 0.035, 0.0175),
        BAR_WIDTH / 2.0,
        BAR_FILL_HEIGHT / 2.0 - 0.032,
        0.002,
    ));
    let sheen_paint = hud_paint(0xffffff, 0.3);
    for (name, color) in joint::BAR_FILLS.iter().zip(colors) {
        let mut node = named(
            at(
                mesh(fill.clone(), hud_material(color), 13),
                -BAR_WIDTH / 2.0,
                0.0,
                0.01,
            ),
            name,
        );
        node.children
            .push(mesh(sheen.clone(), sheen_paint.clone(), 14));
        node.visible = false;
        bar.children.push(node);
    }
    let notches: Vec<Shape> = (1..BAR_SEGMENTS)
        .map(|i| {
            let x = -BAR_WIDTH / 2.0 + BAR_WIDTH * i as f64 / BAR_SEGMENTS as f64;
            let (hw, hh) = (0.016, BAR_FILL_HEIGHT / 2.0 + 0.01);
            Shape::from_points(&[
                DVec2::new(x - hw, -hh),
                DVec2::new(x + hw, -hh),
                DVec2::new(x + hw, hh),
                DVec2::new(x - hw, hh),
            ])
        })
        .collect();
    bar.children.push(at(
        mesh(arc(shape_geometry(&notches)), hud_paint(0x050a12, 0.7), 15),
        0.0,
        0.0,
        0.014,
    ));
    bar.children.push(at(
        protection_meter(
            joint::BAR_SHIELD,
            joint::BAR_SHIELD_FILL,
            pickup(PickupKind::Shield).color,
            true,
        ),
        0.0,
        METER_LOW_Y,
        0.0,
    ));
    bar.children.push(protection_meter(
        joint::BAR_SPAWN,
        joint::BAR_SPAWN_FILL,
        SPAWN_PROTECTION_COLOR,
        false,
    ));
    let chevron = arc(polygon(&[
        [-0.16, -0.015],
        [0.0, 0.075],
        [0.16, -0.015],
        [0.16, -0.075],
        [0.0, 0.015],
        [-0.16, -0.075],
    ]));
    let gold = hud_material(0xffd477);
    for (i, name) in joint::BAR_RANKS.iter().enumerate() {
        let mut node = named(
            at(
                mesh(chevron.clone(), gold.clone(), 16),
                -1.2,
                0.15 - i as f64 * 0.15,
                0.015,
            ),
            name,
        );
        node.visible = false;
        bar.children.push(node);
    }
    bar
}

fn reticle_material(color: u32, opacity: f32) -> Arc<Material> {
    Arc::new(Material {
        side: Side::Double,
        opacity,
        ..(*hud_material(color)).clone()
    })
}

/// Ring, ticks and centre in one paint (`createReticle`), lying flat. The ring and its
/// four ticks are one mesh, so each paint is a single draw.
fn reticle_layer(outline: bool, ink: Arc<Material>, center: Option<Arc<Material>>) -> Vec<Node> {
    let order = if outline { 50 } else { 51 };
    let (inner, outer) = if outline { (0.37, 0.67) } else { (0.46, 0.57) };
    let (length, width) = if outline { (0.47, 0.21) } else { (0.31, 0.075) };
    let ring = ring_geometry(inner, outer, 40);
    let ticks: Vec<Mesh> = [(-0.83, 0.0), (0.83, 0.0), (0.0, -0.83), (0.0, 0.83)]
        .into_iter()
        .map(|(x, y)| {
            let (w, h) = if x == 0.0 {
                (width, length)
            } else {
                (length, width)
            };
            translated(plane_geometry(w, h), x, y, 0.0)
        })
        .collect();
    let parts: Vec<&Mesh> = std::iter::once(&ring).chain(ticks.iter()).collect();
    let mut sight = merge_geometries(&parts).expect("ring and ticks share a layout");
    sight.rotate_x(-PI / 2.0);
    let mut nodes = vec![mesh(arc(sight), ink, order)];
    if let Some(center) = center {
        nodes.push(flat(mesh(arc(circle_geometry(0.075, 16)), center, 52)));
    }
    nodes
}

/// `createReticle()`: a two-tone reticle, legible over bright ground, paint and
/// cover. The ink has three looks (ready, reloading, hit confirmed), toggled by
/// joint visibility.
pub fn reticle() -> Node {
    let mut root = Node::group("reticle");
    root.children
        .extend(reticle_layer(true, reticle_material(0x081626, 1.0), None));
    for (name, ink, center, opacity) in [
        (joint::RETICLE_READY, 0xfff9da, 0xffdf38, 1.0),
        (joint::RETICLE_RELOADING, 0xfff9da, 0xffdf38, 0.3),
        (joint::RETICLE_CONFIRMED, 0xffffff, 0xffffff, 1.0),
    ] {
        let mut group = Node::group(name);
        group.children = reticle_layer(
            false,
            reticle_material(ink, opacity),
            Some(reticle_material(center, opacity)),
        );
        group.visible = name == joint::RETICLE_READY;
        root.children.push(group);
    }
    root
}

/// World heights of the player ring's dark rim and yellow paint. The player spawns
/// on a pad, so both clear the tallest spawn-pad deck (the village and extra-level
/// badge tops out at 0.2325 m) by more than the depth precision of the farthest
/// camera: at the TypeScript heights (0.1 and 0.12, also scaled by the vehicle) the
/// ring sat just under the harbor deck and z-fought with it, and it was buried in
/// the village and quarry pads. Presentation scales the ring's radius, not these.
pub const PLAYER_RING_RIM_HEIGHT: f64 = 0.26;
pub const PLAYER_RING_PAINT_HEIGHT: f64 = 0.28;
/// The spawn pulse spreads just above the ring, clear of the pad it marks.
pub const SPAWN_PULSE_HEIGHT: f64 = 0.3;

/// A thin dark rim reads on sand; the yellow ring identifies the player on either team.
pub fn player_ring() -> Node {
    let mut root = Node::group("player-ring");
    for (inner, outer, color, y) in [
        (1.48, 1.9, 0x172f4a, PLAYER_RING_RIM_HEIGHT),
        (1.56, 1.8, 0xffe522, PLAYER_RING_PAINT_HEIGHT),
    ] {
        let material = Arc::new(Material {
            side: Side::Double,
            tone_mapped: false,
            ..Material::basic(color)
        });
        root.children.push(at(
            flat(mesh(arc(ring_geometry(inner, outer, 48)), material, 0)),
            0.0,
            y,
            0.0,
        ));
    }
    root
}

/// The spawn cue: a yellow ring pulsing outward; instance opacity fades it.
pub fn spawn_pulse() -> Node {
    let material = Arc::new(Material {
        transparent: true,
        depth_write: false,
        side: Side::Double,
        tone_mapped: false,
        ..Material::basic(0xffe522)
    });
    let mut root = Node::group("spawn-pulse");
    root.children
        .push(flat(mesh(arc(ring_geometry(1.8, 1.96, 48)), material, 0)));
    root
}

/// The supply drop's ground marking, sprayed in the kind's paint round its pallet:
/// a thin hand-sprayed circle (its radii, just clear of the pallet's corners, so
/// neighbouring drops barely touch), darkened while the pickup is being restocked,
/// and outside it a ring of tick marks (radii, count, the gap between ticks as a
/// share of one, and how faint a tick still to fill is) sprayed in one by one until
/// the supply returns. Paint on the ground, lit and shadowed like it, its ragged
/// edges anti-aliased by alpha to coverage, never glowing.
const MARKING_RADII: [f64; 2] = [1.04, 1.18];
const MARKING_SEGMENTS: u32 = 64;
/// The spent ring's darkening of the kind's paint (linear).
const MARKING_FADED: u32 = 0x5a5a5a;
const MARKING_CUTOFF: f32 = 0.35;
const REFILL_RADII: [f64; 2] = [1.22, 1.32];
pub const REFILL_TICKS: u32 = 24;
const REFILL_GAP: f32 = 0.42;
const REFILL_FAINT: f32 = 0.18;
/// How high the paint lies over the ground; a depth bias keeps it there.
const MARKING_HEIGHT: f64 = 0.012;
const PAINT_ROUGHNESS: f32 = 0.95;

/// The pallet under every pickup, shared by every kind: planks and a spent smoke
/// grenade (`sloppy_core::models::pickup_pallet`).
pub fn pickup_base() -> Node {
    let mut root = Node::group("pickup");
    root.children.push(pickup_pallet());
    root
}

/// The sprayed circle: the atlas's paint cell bent once round the pallet; the
/// generator paints it seamless across the cell, its strokes, gaps and overspray in
/// alpha.
fn marking_geometry() -> Mesh {
    let [inner, outer] = MARKING_RADII;
    let mut mesh = Mesh::default();
    for step in 0..=MARKING_SEGMENTS {
        let along = f64::from(step) / f64::from(MARKING_SEGMENTS);
        let (sin, cos) = (std::f64::consts::TAU * along).sin_cos();
        for (radius, v) in [(inner, 0.0), (outer, 1.0)] {
            mesh.positions
                .push([(radius * cos) as f32, 0.0, (radius * sin) as f32]);
            mesh.normals.push([0.0, 1.0, 0.0]);
            let [u, v] = atlas_cell_uv(PICKUP_PAINT_CELL, along, v);
            mesh.uvs.push([u as f32, v as f32]);
        }
    }
    let mut indices = Vec::new();
    for step in 0..MARKING_SEGMENTS {
        let a = step * 2;
        indices.extend([a, a + 2, a + 1, a + 2, a + 3, a + 1]);
    }
    mesh.indices = Some(indices);
    mesh
}

/// A part that takes shadows but casts none, as paint on the ground does.
fn shadow_receiver(mesh: Arc<Mesh>, material: Arc<Material>) -> Node {
    let mut node = Node::mesh(mesh, material);
    if let Some(drawable) = &mut node.drawable {
        drawable.receive_shadow = true;
    }
    node
}

/// Matte paint on the ground, white so the instance tint is the kind's colour,
/// lying a little proud of whatever it is sprayed on.
fn ground_paint(color: u32) -> Material {
    Material {
        polygon_offset: Some((-1.0, -4.0)),
        ..Material::standard(color, 0.0, PAINT_ROUGHNESS)
    }
}

/// A supply drop's marking: the sprayed circle, full or darkened while the pickup
/// is restocked, and the refill ticks shown while it is
/// (`instance_data.x`: ticks sprayed, its fraction the next tick's coverage).
pub fn pickup_rim() -> Node {
    let mut root = Node::group("pickup-rim");
    let circle = arc(marking_geometry());
    let atlas = TextureRef {
        wrap: sloppy_core::scene::Wrap::Clamp,
        anisotropy: 4,
        ..TextureRef::file(PICKUP_ATLAS_PATH)
    };
    for (name, color) in [
        (joint::PICKUP_RING, 0xffffff),
        (joint::PICKUP_RING_DIM, MARKING_FADED),
    ] {
        let material = Material {
            map: Some(atlas.clone()),
            alpha_test: MARKING_CUTOFF,
            alpha_to_coverage: true,
            ..ground_paint(color)
        };
        let mut ring = named(
            at(
                shadow_receiver(circle.clone(), Arc::new(material)),
                0.0,
                MARKING_HEIGHT,
                0.0,
            ),
            name,
        );
        ring.visible = name == joint::PICKUP_RING;
        root.children.push(ring);
    }
    let refill = RingGeometry {
        inner_radius: REFILL_RADII[0],
        outer_radius: REFILL_RADII[1],
        theta_segments: REFILL_TICKS * 2,
        theta_start: PI / 2.0,
        theta_length: PI * 2.0,
    }
    .build();
    // Opaque with an alpha test: the effect cuts the gaps between ticks and fades
    // the ticks still to spray.
    let material = Material {
        alpha_test: 0.5,
        effect: Effect::Custom {
            name: PICKUP_REFILL_EFFECT,
            params: vec![
                REFILL_TICKS as f32,
                REFILL_RADII[1] as f32,
                REFILL_GAP,
                REFILL_FAINT,
            ],
        },
        ..ground_paint(0xffffff)
    };
    let mut node = named(
        at(
            flat(shadow_receiver(arc(refill), Arc::new(material))),
            0.0,
            MARKING_HEIGHT,
            0.0,
        ),
        joint::PICKUP_REFILL,
    );
    node.visible = false;
    root.children.push(node);
    root
}

/// How far to raise a mine at `(x, z)` so it lies on a spawn pad's deck rather
/// than inside it; zero on open ground. Only the drawn model moves: the mine's
/// simulated position and trigger radius stay on the ground plane.
pub fn mine_lift(pads: &SpawnPadDecks, x: f64, z: f64) -> f64 {
    pads.top_within(x, z, MINE_RADIUS).unwrap_or(0.0)
}

/// A mine painted like the pickups' bases, with a blinking team cap.
pub fn mine(team: Team) -> Node {
    let mut root = Node::group("mine");
    put(
        &mut root,
        cylinder_part(MINE_RADIUS, 0.17, 0x384f47, 12),
        0.0,
        0.12,
        0.0,
    );
    put(
        &mut root,
        named(
            cylinder_part(0.17, 0.07, TEAM_COLORS[team.index()], 12),
            joint::MINE_LIGHT,
        ),
        0.0,
        0.24,
        0.0,
    );
    root
}

/// The instanced debris pieces (`presentation.ts`), unit-colored (white) so the
/// instance tint paints each piece. Wood, panels, beams and logs use scenery
/// surfaces: `sidingBox(1.5, 0.18, 0.45)`, `sidingBox(1, 1, 1)` and
/// `trunkFragment()`.
pub fn debris_piece(shape: FragmentShape) -> Node {
    let piece = |geometry: Mesh| shadowed(arc(geometry), paint(0xffffff));
    let node = match shape {
        FragmentShape::Armor => piece(rounded_box_geometry(
            1.25,
            0.16,
            0.85,
            1,
            DEFAULT_BOX_RADIUS,
        )),
        FragmentShape::Wheel => piece(cylinder_geometry(0.48, 0.48, 0.28, 10)),
        FragmentShape::Track => piece(rounded_box_geometry(0.5, 0.2, 1.5, 1, DEFAULT_BOX_RADIUS)),
        FragmentShape::Shard => piece(tetrahedron_geometry(0.75, 0)),
        FragmentShape::DrumShell => piece(barrel_scrap_geometry(BarrelScrap::Shell)),
        FragmentShape::DrumLid => piece(barrel_scrap_geometry(BarrelScrap::Lid)),
        FragmentShape::Wood => siding_box(1.5, 0.18, 0.45, 0xffffff),
        FragmentShape::Panel | FragmentShape::Beam => siding_box(1.0, 1.0, 1.0, 0xffffff),
        FragmentShape::Log => (*trunk_fragment()).clone(),
    };
    let mut root = Node::group("debris");
    root.children.push(node);
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(node: &Node, out: &mut Vec<String>) {
        if !node.name.is_empty() {
            out.push(node.name.clone());
        }
        for child in &node.children {
            names(child, out);
        }
    }

    #[test]
    fn hud_models_name_their_joints() {
        let mut found = Vec::new();
        names(&tank_bar(Team::Red), &mut found);
        for name in joint::BAR_FILLS.iter().chain(&joint::BAR_RANKS).chain(&[
            joint::BAR_TRAIL,
            joint::BAR_SHIELD,
            joint::BAR_SHIELD_FILL,
            joint::BAR_SPAWN,
            joint::BAR_SPAWN_FILL,
        ]) {
            assert!(found.iter().any(|n| n == name), "{name}");
        }
        let mut found = Vec::new();
        names(&reticle(), &mut found);
        assert!(found.iter().any(|n| n == joint::RETICLE_CONFIRMED));
        let mut found = Vec::new();
        names(&pickup_rim(), &mut found);
        for name in [
            joint::PICKUP_RING,
            joint::PICKUP_RING_DIM,
            joint::PICKUP_REFILL,
        ] {
            assert!(found.iter().any(|n| n == name), "{name}");
        }
    }

    #[test]
    fn player_ring_and_spawn_pulse_clear_every_spawn_pad_deck() {
        use glam::DMat4;
        use sloppy_core::geometry::node_bounds;
        use sloppy_core::models::{SpawnPadShape, create_spawn_pads, quarry_spawn_pad_pieces};
        // Village and extra-level plinths; harbor decks (0.1, team ring 0.11) are lower.
        let mut deck = node_bounds(&create_spawn_pads(1.0), DMat4::IDENTITY).max.y;
        for team in [Team::Blue, Team::Red] {
            for piece in quarry_spawn_pad_pieces(team) {
                if !matches!(piece.shape, SpawnPadShape::Post | SpawnPadShape::Cap) {
                    deck = deck.max(piece.y + piece.h / 2.0);
                }
            }
        }
        // Several depth-buffer steps at the farthest overhead zoom.
        const CLEARANCE: f64 = 0.02;
        let ring = node_bounds(&player_ring(), DMat4::IDENTITY);
        assert!(
            ring.min.y > deck + CLEARANCE,
            "ring {} deck {deck}",
            ring.min.y
        );
        assert!(SPAWN_PULSE_HEIGHT > ring.max.y, "pulse under the ring");
    }

    #[test]
    fn mines_lie_on_every_spawn_pad_deck_and_on_open_ground_as_before() {
        use crate::effects::spawn_pad_decks::tests::{pad_models, surface_top, triangles_near};
        use glam::DMat4;
        use sloppy_core::geometry::node_bounds;
        use sloppy_core::sim::arena::spawn_positions;
        const STEP: f64 = 0.3;
        const SPAN: i32 = 11;
        // Samples around a mine's rim can miss the smallest pad fittings.
        const MAX_FLOAT: f64 = 0.06;
        let bottom = node_bounds(&mine(Team::Red), DMat4::IDENTITY).min.y;
        assert!(bottom > 0.0, "the mine sits on the ground");
        for (theme, pads) in pad_models() {
            let decks = SpawnPadDecks::new(theme, 1.0);
            for team in [Team::Blue, Team::Red] {
                let center = spawn_positions(team, 1.0)[1];
                let triangles = triangles_near(&pads, center, STEP * f64::from(SPAN) + 1.0);
                let mut on_pad = 0;
                for i in -SPAN..=SPAN {
                    for j in -SPAN..=SPAN {
                        let x = center.x + f64::from(i) * STEP + 0.013;
                        let z = center.z + f64::from(j) * STEP + 0.017;
                        // The highest deck anywhere under the mine's disc.
                        let mut deck = 0.0_f64;
                        for ring in 0..=3 {
                            let r = MINE_RADIUS * f64::from(ring) / 3.0;
                            for k in 0..32 {
                                let a = f64::from(k) * std::f64::consts::TAU / 32.0;
                                let top = surface_top(&triangles, x + r * a.sin(), z + r * a.cos());
                                deck = deck.max(top.unwrap_or(0.0));
                            }
                        }
                        let lift = mine_lift(&decks, x, z);
                        if deck <= 0.05 {
                            // Off the pads a mine stays where it always was, unless its
                            // rim grazes a deck corner between the samples.
                            let graze = decks.top_within(x, z, MINE_RADIUS - 0.05).is_none();
                            assert!(lift == 0.0 || graze, "{theme} lifted {lift} at {x} {z}");
                            continue;
                        }
                        on_pad += 1;
                        assert!(
                            lift + bottom > deck + 0.01 && lift < deck + MAX_FLOAT,
                            "{theme} mine lifted {lift} over deck {deck} at {x} {z}"
                        );
                    }
                }
                assert!(on_pad > 150, "{theme}: {on_pad} mines on the pad");
            }
            assert_eq!(mine_lift(&decks, 0.0, 0.0), 0.0);
        }
    }
}
