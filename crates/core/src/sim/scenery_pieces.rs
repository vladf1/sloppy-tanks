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
use super::tree_proportions::tree_proportions;
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
    /// A box-shaped piece (a cylinder for logs).
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
        let builder = if shape == FragmentShape::Log {
            ColliderBuilder::cylinder((h / 2.0) as f32, (w / 2.0) as f32)
        } else {
            ColliderBuilder::cuboid((w / 2.0) as f32, (h / 2.0) as f32, (d / 2.0) as f32)
        };
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
    let legacy_count = match cover.kind {
        CoverKind::Tree => 9,
        CoverKind::Drum => 3,
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
    let wood = DebrisMaterial::Wood;
    match cover.kind {
        CoverKind::Tree => {
            let proportions = tree_proportions(cover.x, cover.z, cover.w, cover.d, cover.h);
            let (family, height, radius, stump) = (
                proportions.family,
                proportions.height,
                proportions.radius,
                proportions.stump_height,
            );
            let length = height * if family < 3 { 0.98 } else { 0.78 } - stump;
            let center = stump + length / 2.0;
            let trunk = breakup.piece(
                simulation,
                FragmentShape::Log,
                [0.0, center, 0.0],
                [radius * 2.0, length, radius * 2.0],
                Some(0x98734f),
                wood,
            );
            simulation.fragments[trunk].tree_cover_id = Some(cover.id);
            simulation.fragments[trunk].tree_center_y = Some(center);
            let trunk_body = simulation.fragments[trunk].body;
            // A light crown volume keeps foliage above the ground as the trunk rolls.
            simulation.world.insert_collider(
                ColliderBuilder::ball((cover.w.min(cover.d) * 0.34) as f32)
                    .translation(vector(0.0, height * 0.72 - center, 0.0))
                    .collision_groups(interaction_groups(group::FRAGMENT))
                    .mass(0.12)
                    .friction(0.9)
                    .restitution(0.05),
                Some(trunk_body),
            );
            // A small sideways lean initiates a gravity-driven fall instead of a launch.
            let angle = breakup.rng.range(0.0, PI * 2.0);
            let rigid_body = &mut simulation.world.bodies[trunk_body];
            rigid_body.set_linvel(vector(angle.sin() * 0.45, 0.0, angle.cos() * 0.45), true);
            rigid_body.set_angvel(vector(angle.cos() * 0.65, 0.0, -angle.sin() * 0.65), true);
            breakup.piece(
                simulation,
                FragmentShape::Beam,
                [radius, stump, 0.0],
                [radius * 0.3, radius * 1.4, radius * 0.25],
                Some(0xb59a69),
                wood,
            );
        }
        CoverKind::Drum => {
            // Internal pressure tears the thin wall into small curled sheets, not a
            // surviving cylinder. Offset each sheet so the blast spreads them radially.
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
                    Some(if i == 1 { 0x493e35 } else { 0x765443 }),
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
                Some(0x574e3e),
                DebrisMaterial::Metal,
            );
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
