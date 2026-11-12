//! Physical breakup of destroyed scenery into its authored major components. Dust, foliage
//! and chips remain presentation particles. Dimensions are shared by simple colliders and
//! instanced unit geometry.

use std::f64::consts::PI;

use rapier3d::prelude::{ColliderBuilder, RigidBodyBuilder};

use super::data::group;
use super::debris_cleanup::DEBRIS_CLEANUP_SECONDS;
use super::debris_physics::{DebrisMaterial, debris_material, track_debris_contacts};
use super::math::{Point3, Quat4, Random};
use super::physics::{
    from_rotation, from_vector, interaction_groups, to_rotation, to_vector, vector,
};
use super::simulation::Simulation;
use super::arena::cover_turn;
use super::house_layout::SENDA_STYLES;
use super::senda_entries::is_entry;
use super::senda_yards::{LOGGIA_COLUMN, LOGGIA_EAVE, PATIO_POST, PATIO_ROOF, PERGOLA_WOOD, PatioCover};
use super::types::{Cover, CoverKind, Fragment, FragmentShape};

/// A movable cover's world pose when it was destroyed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoverPose {
    pub position: Point3,
    pub rotation: Quat4,
}

/// Seconds a scenery piece stays solid before its fade, and its hard deadline.
const PIECE_LIFE: f64 = 9.5;
const PIECE_DEADLINE: f64 = 18.0;
/// Draws the old fragment path consumed per piece: 3 placement/size and 8 body/lifetime.
const LEGACY_DRAWS_PER_PIECE: usize = 11;

struct Breakup<'a> {
    cover: &'a Cover,
    rng: Random,
    pieces: Vec<u32>,
}

impl Breakup<'_> {
    /// A box-shaped piece.
    fn piece(
        &mut self,
        simulation: &mut Simulation,
        shape: FragmentShape,
        center: [f64; 3],
        size: [f64; 3],
        color: Option<u32>,
        material: DebrisMaterial,
    ) -> usize {
        let [w, h, d] = size;
        let builder =
            ColliderBuilder::cuboid((w / 2.0) as f32, (h / 2.0) as f32, (d / 2.0) as f32);
        self.piece_with(simulation, shape, center, size, color, material, builder)
    }

    /// A piece centred at `[x, y, z]` (relative to the cover, y from the ground)
    /// whose main collider is `builder`; `[w, h, d]` are its nominal dimensions.
    #[allow(clippy::too_many_arguments)]
    fn piece_with(
        &mut self,
        simulation: &mut Simulation,
        shape: FragmentShape,
        [x, y, z]: [f64; 3],
        [w, h, d]: [f64; 3],
        color: Option<u32>,
        material: DebrisMaterial,
        builder: ColliderBuilder,
    ) -> usize {
        let cover = self.cover;
        simulation.reserve_fragments(1);
        let id = simulation.allocate_id();
        let body = simulation.world.insert_body(
            RigidBodyBuilder::dynamic()
                .translation(vector(cover.x + x, y, cover.z + z))
                .linear_damping(0.12)
                .angular_damping(0.25)
                .can_sleep(true),
        );
        let surface = debris_material(material);
        let density = if material == DebrisMaterial::Metal {
            0.65
        } else {
            0.35
        };
        let collider = simulation.world.insert_collider(
            builder
                .collision_groups(interaction_groups(group::PUSHABLE_DEBRIS))
                .mass(0.18f64.max(w * h * d * density) as f32)
                .friction(surface.friction as f32)
                .restitution(surface.restitution as f32),
            Some(body),
        );
        let angle = self.rng.range(0.0, PI * 2.0);
        let outward = x.hypot(z);
        let (nx, nz) = if outward > 0.1 {
            (x / outward, z / outward)
        } else {
            (angle.cos(), angle.sin())
        };
        let speed = self.rng.range(2.0, 5.0);
        let rigid_body = &mut simulation.world.bodies[body];
        let mass = rigid_body.mass() as f64;
        let lift = self.rng.range(3.0, 7.0);
        rigid_body.apply_impulse_at_point(
            vector(nx * speed * mass, lift * mass, nz * speed * mass),
            vector(cover.x + x + w * 0.15, y - h * 0.2, cover.z + z + d * 0.15),
            true,
        );
        track_debris_contacts(simulation, body, collider, id, material);
        let mut fragment = Fragment::new(
            id,
            body,
            PIECE_LIFE + DEBRIS_CLEANUP_SECONDS,
            1.0,
            color.unwrap_or(cover.color),
        );
        fragment.shape = Some(shape);
        fragment.dimensions = Some(Point3::new(w, h, d));
        fragment.material = Some(material);
        fragment.source_kind = Some(cover.kind);
        fragment.expires_at = Some(simulation.elapsed + PIECE_DEADLINE);
        simulation.fragments.push(fragment);
        self.pieces.push(id);
        simulation.fragments.len() - 1
    }
}

/// Break destroyed scenery into physical pieces. Returns false for kinds without authored
/// pieces, whose callers fall back to small fragments.
pub fn break_scenery(
    simulation: &mut Simulation,
    cover_index: usize,
    pose: Option<CoverPose>,
) -> bool {
    let mut cover = simulation.covers[cover_index].clone();
    // Navigation bounds expand as a barrel tips; fragments keep its original dimensions.
    if let Some(motion) = cover.motion {
        cover.w = motion.w;
        cover.d = motion.d;
    }
    let patio = (cover.kind == CoverKind::Concrete)
        .then(|| PatioCover::of_color(cover.color))
        .flatten();
    let entry = cover.kind == CoverKind::Concrete && is_entry(cover.color);
    let legacy_count = match cover.kind {
        CoverKind::Drum => 3,
        // As many as the shards a plain cover falls back to.
        CoverKind::Concrete if patio.is_some() || entry => 3,
        _ => return false,
    };
    // Preserve the old fragment path's draws so cosmetic authoring cannot reshuffle seeded
    // combat and bot decisions. New piece motion uses its own stream below.
    for _ in 0..legacy_count * LEGACY_DRAWS_PER_PIECE {
        simulation.rng.next();
    }
    let mut breakup = Breakup {
        cover: &cover,
        rng: Random::new(cover.id as f64 * 73_856_093.0 + simulation.seed),
        pieces: Vec::new(),
    };
    match cover.kind {
        CoverKind::Drum => {
            // Internal pressure tears the thin wall into small curled sheets, not a
            // surviving cylinder. Offset each sheet so the blast spreads them radially.
            // A wheelie bin's sheets and lid are its own plastic.
            let bin = super::senda_layout::is_trash_bin(cover.color).then_some(cover.color);
            let phase = breakup.rng.range(0.0, PI * 2.0);
            for i in 0..3 {
                let angle = phase + (i as f64 * PI * 2.0) / 3.0;
                let y = cover.h * breakup.rng.range(0.3, 0.6);
                let w = cover.w * breakup.rng.range(0.28, 0.4);
                let h = cover.h * breakup.rng.range(0.25, 0.4);
                let scrap = breakup.piece(
                    simulation,
                    FragmentShape::DrumShell,
                    [angle.cos() * cover.w * 0.3, y, angle.sin() * cover.d * 0.3],
                    [w, h, 0.12],
                    Some(bin.unwrap_or(if i == 1 { 0x493e35 } else { 0x765443 })),
                    DebrisMaterial::Metal,
                );
                let body = simulation.fragments[scrap].body;
                simulation.world.bodies[body].set_rotation(to_rotation(Quat4::yaw(-angle)), true);
            }
            breakup.piece(
                simulation,
                FragmentShape::DrumLid,
                [0.0, cover.h, 0.0],
                [cover.w * 0.65, 0.16, cover.w * 0.65],
                Some(bin.unwrap_or(0x574e3e)),
                DebrisMaterial::Metal,
            );
        }
        CoverKind::Concrete if entry => break_entry(&mut breakup, simulation),
        CoverKind::Concrete => {
            let Some(look) = patio else { return false };
            break_patio_cover(&mut breakup, simulation, look);
        }
        _ => return false,
    }
    if let Some(pose) = pose {
        // Carry the authored breakup into the barrel's current world pose before the blast.
        let rotation = pose.rotation;
        for id in &breakup.pieces {
            let Some(fragment) = simulation
                .fragments
                .iter()
                .find(|fragment| fragment.id == *id)
            else {
                continue;
            };
            let body = &mut simulation.world.bodies[fragment.body];
            let p = from_vector(body.translation());
            let local = Point3::new(p.x - cover.x, p.y - cover.h / 2.0, p.z - cover.z);
            let moved = rotation.rotate(local);
            body.set_translation(
                to_vector(Point3::new(
                    moved.x + pose.position.x,
                    moved.y + pose.position.y,
                    moved.z + pose.position.z,
                )),
                true,
            );
            let q = from_rotation(*body.rotation());
            body.set_rotation(to_rotation(rotation.multiply(q)), true);
            let velocity = rotation.rotate(from_vector(body.linvel()));
            let spin = rotation.rotate(from_vector(body.angvel()));
            body.set_linvel(to_vector(velocity), true);
            body.set_angvel(to_vector(spin), true);
        }
    }
    true
}

/// A loggia or pergola brought down: its posts or columns, the beam across them and its
/// roof (a loggia's tile in two halves, a pergola's slats in a few bundles) fall from
/// where they stood, turned as the cover was, in the house's paint or the stained wood.
fn break_patio_cover(breakup: &mut Breakup, simulation: &mut Simulation, look: PatioCover) {
    let cover = breakup.cover;
    let (w, d) = (cover.w, cover.d);
    let turn = cover_turn(cover.kind, cover.house_plan);
    let (sin, cos) = turn.sin_cos();
    // From the box's own frame (+z toward the house) into the world's, about its middle.
    let place = |x: f64, y: f64, z: f64| [x * cos + z * sin, y, -x * sin + z * cos];
    let (low, eave) = PATIO_ROOF;
    let style = SENDA_STYLES[usize::from(cover.house_plan.map_or(0, |plan| plan.style))
        % SENDA_STYLES.len()];
    let mut pieces = Vec::new();
    match look {
        PatioCover::Loggia => {
            let (column, beam) = LOGGIA_COLUMN;
            let cover_w = w - 2.0 * LOGGIA_EAVE;
            let reach = d - LOGGIA_EAVE;
            let front = d / 2.0 - reach + column / 2.0;
            let wall = DebrisMaterial::Concrete;
            for end in [-1.0, 1.0] {
                let at = place(end * (cover_w - PATIO_POST) / 2.0, low / 2.0, front);
                pieces.push((FragmentShape::Panel, at, [column, low, column], style.siding, wall, 0.0));
            }
            let at = place(0.0, low - beam / 2.0, front);
            pieces.push((FragmentShape::Panel, at, [cover_w, beam, column], style.siding, wall, 0.0));
            let pitch = (eave - low).atan2(reach);
            let slope = reach.hypot(eave - low) + LOGGIA_EAVE;
            for end in [-1.0, 1.0] {
                let at = place(end * w / 4.0, (low + eave) / 2.0 + 0.08, 0.0);
                pieces.push((FragmentShape::Panel, at, [w / 2.0, 0.09, slope], style.roof, wall, pitch));
            }
        }
        PatioCover::Pergola => {
            let wood = DebrisMaterial::Wood;
            let cover_w = w - 0.1;
            let reach = d - 0.1;
            let front = d / 2.0 - reach + PATIO_POST / 2.0;
            for end in [-1.0, 1.0] {
                let at = place(end * (cover_w - PATIO_POST) / 2.0, low / 2.0, front);
                pieces.push((FragmentShape::Panel, at, [PATIO_POST, low, PATIO_POST], PERGOLA_WOOD, wood, 0.0));
            }
            for z in [front, d / 2.0 - 0.06] {
                let at = place(0.0, low + 0.06, z);
                pieces.push((FragmentShape::Panel, at, [cover_w, 0.12, 0.1], PERGOLA_WOOD, wood, 0.0));
            }
            for x in [-0.3, 0.0, 0.3] {
                let at = place(x * cover_w, low + 0.15, d / 2.0 - reach / 2.0);
                pieces.push((FragmentShape::Wood, at, [0.3, 0.06, reach], PERGOLA_WOOD, wood, 0.0));
            }
        }
    }
    for (shape, at, size, color, material, pitch) in pieces {
        let piece = breakup.piece(simulation, shape, at, size, Some(color), material);
        let body = simulation.fragments[piece].body;
        let rotation = Quat4::yaw(turn).multiply(Quat4::from_euler_xyz(-pitch, 0.0, 0.0));
        simulation.world.bodies[body].set_rotation(to_rotation(rotation), true);
    }
}

/// How high (m) an entry's walls stand under its roof, as a share of its height, and how
/// thick its stucco slabs and its tile fall (m).
const ENTRY_WALL_SHARE: f64 = 0.78;
const ENTRY_SLAB: f64 = 0.15;
const ENTRY_TILE: f64 = 0.09;

/// An office and entry brought down: its stucco front in two slabs, its side wall between
/// the room and the porch bay, and its tile roof in two halves fall from where they stood,
/// turned as the cover was, in its house's paint and tile. Nothing stays standing: the
/// house's own front stands behind it.
fn break_entry(breakup: &mut Breakup, simulation: &mut Simulation) {
    let cover = breakup.cover;
    let (w, d, h) = (cover.w, cover.d, cover.h);
    let turn = cover_turn(cover.kind, cover.house_plan);
    let (sin, cos) = turn.sin_cos();
    // From the box's own frame (+z out from the house) into the world's, about its middle.
    let place = |x: f64, y: f64, z: f64| [x * cos + z * sin, y, -x * sin + z * cos];
    let style = SENDA_STYLES[usize::from(cover.house_plan.map_or(0, |plan| plan.style))
        % SENDA_STYLES.len()];
    let wall = h * ENTRY_WALL_SHARE;
    let stucco = DebrisMaterial::Concrete;
    let mut pieces = Vec::new();
    for side in [-1.0, 1.0] {
        let at = place(side * w / 4.0, wall / 2.0, (d - ENTRY_SLAB) / 2.0);
        pieces.push((at, [w / 2.0, wall, ENTRY_SLAB], style.siding, 0.0));
    }
    let at = place(0.0, wall / 2.0, 0.0);
    pieces.push((at, [ENTRY_SLAB, wall, d], style.siding, 0.0));
    let pitch = ((h - wall) / d).atan();
    for side in [-1.0, 1.0] {
        let at = place(side * w / 4.0, (h + wall) / 2.0, 0.0);
        pieces.push((at, [w / 2.0, ENTRY_TILE, d], style.roof, pitch));
    }
    for (at, size, color, pitch) in pieces {
        let piece = breakup.piece(simulation, FragmentShape::Panel, at, size, Some(color), stucco);
        let body = simulation.fragments[piece].body;
        let rotation = Quat4::yaw(turn).multiply(Quat4::from_euler_xyz(pitch, 0.0, 0.0));
        simulation.world.bodies[body].set_rotation(to_rotation(rotation), true);
    }
}
