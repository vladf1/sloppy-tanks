//! Tank wrecks: all substantial pieces, including detached guns, remain solid and pushable.

use std::f64::consts::PI;

use rapier3d::prelude::{ColliderBuilder, RigidBodyBuilder};

use super::data::{ARENA, group, vehicle};
use super::debris_cleanup::DEBRIS_CLEANUP_SECONDS;
use super::debris_physics::{DebrisMaterial, track_debris_contacts};
use super::math::{Quat4, Vec2, clamp};
use super::physics::{interaction_groups, to_rotation, vector};
use super::simulation::{Simulation, WreckView};
use super::simulation_rules::GRAVITY;
use super::tank_destruction::humvee_tumble;
use super::tank_dimensions::HUMVEE_BODY_LENGTH_SCALE;
use super::types::{CoverKind, Fragment, VehicleKind, WreckPart};

const WRECK_COLOR: u32 = 0x46534c;
/// Seconds before a wreck piece's hard removal deadline.
const WRECK_DEADLINE: f64 = 18.0;

/// Replace a destroyed tank's body with wreck pieces. `burnout` keeps the hull whole.
pub fn break_tank(simulation: &mut Simulation, tank_index: usize, burnout: bool) {
    let tank = &simulation.tanks[tank_index];
    let origin = simulation.body_translation(tank.body);
    let (kind, id, life, heading, aim, team, old_body) = (
        tank.kind,
        tank.id,
        tank.life,
        tank.heading,
        tank.aim,
        tank.team,
        tank.body,
    );
    let scale = vehicle(kind).scale;
    let mass = vehicle(kind).mass * 1.6;
    let humvee = kind == VehicleKind::Humvee;
    if burnout || humvee {
        let velocity = simulation.body_linvel(old_body);
        // Exploding Humvees tumble whole; quiet burnouts only hop and rock.
        let tumble = humvee && !burnout;
        let motion = humvee_tumble(simulation.seed, id, life);
        let rock = (id + life) as f64;
        let hop_height = if tumble {
            motion.height
        } else {
            0.9 + (rock % 3.0) * 0.1
        };
        let pitch = if tumble {
            motion.pitch
        } else {
            (if rock % 2.0 != 0.0 { 1.0 } else { -1.0 }) * 0.85
        };
        let roll = if tumble { motion.roll } else { 0.0 };
        simulation.remove_body(old_body);
        simulation.reserve_fragments(1);
        let body = simulation.world.insert_body(
            RigidBodyBuilder::dynamic()
                .translation(vector(origin.x, origin.y - 0.4 + 0.55 * scale, origin.z))
                .linvel(vector(
                    velocity.x * 0.2,
                    (2.0 * GRAVITY * hop_height).sqrt(),
                    velocity.z * 0.2,
                ))
                .angvel(vector(
                    heading.cos() * pitch + heading.sin() * roll,
                    if tumble { motion.yaw } else { pitch * 0.15 },
                    -heading.sin() * pitch + heading.cos() * roll,
                ))
                .angular_damping((if tumble { motion.damping } else { 3.5 }) as f32)
                .ccd_enabled(true),
        );
        let rigid_body = &mut simulation.world.bodies[body];
        rigid_body.set_additional_solver_iterations(2);
        rigid_body.set_rotation(to_rotation(Quat4::yaw(heading)), true);
        let half_length = if humvee {
            2.18 * HUMVEE_BODY_LENGTH_SCALE
        } else if kind == VehicleKind::Scout {
            2.2
        } else {
            2.7
        };
        let collider = simulation.world.insert_collider(
            ColliderBuilder::cuboid(
                ((if humvee { 1.16 } else { 1.22 }) * scale) as f32,
                ((if humvee { 0.9 } else { 0.75 }) * scale) as f32,
                (half_length * scale) as f32,
            )
            .collision_groups(interaction_groups(group::WRECK))
            .mass(mass as f32)
            .friction(0.95)
            .restitution(0.05),
            Some(body),
        );
        let fragment_id = simulation.next_id;
        simulation.next_id += 1;
        track_debris_contacts(
            simulation,
            body,
            collider,
            fragment_id,
            DebrisMaterial::Metal,
        );
        let mut fragment = wreck_fragment(
            fragment_id,
            body,
            5.0 + DEBRIS_CLEANUP_SECONDS,
            simulation.elapsed,
        );
        fragment.wreck = Some(kind);
        fragment.team = Some(team);
        fragment.part = Some(WreckPart::Intact);
        simulation.fragments.push(fragment);
        return;
    }
    let detached = simulation.rng.next() < 0.4;
    let pieces: &[WreckPart] = if detached {
        &[WreckPart::Hull, WreckPart::Turret, WreckPart::Barrel]
    } else {
        &[WreckPart::Hull, WreckPart::TurretBarrel]
    };
    // Explosion travel is in world units, independent of visual model scale.
    let half_separation = simulation.rng.range(7.0, 14.0);
    let angle = simulation.rng.range(-0.45, 0.45);
    let high = simulation.rng.next() < 0.25;
    let view = if simulation.multiplayer() {
        None
    } else {
        simulation.wreck_view
    };
    // Only on-screen explosions use view bounds; off-screen combat stays local.
    let bounds = match view {
        Some(view)
            if origin.x > view.min_x
                && origin.x < view.max_x
                && origin.z > view.min_z
                && origin.z < view.max_z =>
        {
            view
        }
        _ => WreckView {
            min_x: origin.x - 18.0,
            max_x: origin.x + 18.0,
            min_z: origin.z - 12.0,
            max_z: origin.z + 12.0,
        },
    };
    let min_x = (-ARENA + 3.0).max(bounds.min_x);
    let max_x = (ARENA - 3.0).min(bounds.max_x);
    let min_z = (-ARENA + 3.0).max(bounds.min_z);
    let max_z = (ARENA - 3.0).min(bounds.max_z);
    // Shift the landing pair inward together at arena/view edges, preserving separation.
    let edge = half_separation.min((max_x - min_x) / 2.0);
    let center = Vec2::new(
        clamp(origin.x, min_x + edge, max_x - edge),
        clamp(origin.z, min_z + 2.0, max_z - 2.0),
    );
    simulation.remove_body(old_body);
    for (index, &part) in pieces.iter().enumerate() {
        simulation.reserve_fragments(1);
        let side = if index == 0 { -1.0 } else { 1.0 };
        let mut landing = Vec2::new(
            clamp(
                center.x + side * half_separation * angle.cos(),
                min_x,
                max_x,
            ),
            clamp(
                center.z
                    + side * half_separation * angle.sin()
                    + if index == 2 { -5.0 } else { 0.0 },
                min_z,
                max_z,
            ),
        );
        let blocked = |simulation: &Simulation, position: Vec2| {
            simulation.covers.iter().any(|cover| {
                cover.alive
                    && cover.kind != CoverKind::Boundary
                    && (position.x - cover.x).abs() < cover.w / 2.0 + 1.5
                    && (position.z - cover.z).abs() < cover.d / 2.0 + 1.5
            })
        };
        let mut attempt = 0;
        while blocked(simulation, landing) && attempt < 12 {
            let x = clamp(landing.x + simulation.rng.range(-3.0, 3.0), min_x, max_x);
            let z = clamp(landing.z + simulation.rng.range(-3.0, 3.0), min_z, max_z);
            landing = Vec2::new(x, z);
            attempt += 1;
        }
        // Start the turret clear of the hull collider now that the two can collide.
        let y = origin.y
            + if part == WreckPart::Hull {
                0.0
            } else {
                0.95 * scale
            };
        // A detached gun starts beyond the turret, rather than inside its collider.
        let gun_offset = if part == WreckPart::Barrel {
            2.1 * scale
        } else {
            0.0
        };
        let x = origin.x + aim.sin() * gun_offset;
        let z = origin.z + aim.cos() * gun_offset;
        let peak = if high && part != WreckPart::Hull {
            simulation.rng.range(20.0, 30.0)
        } else {
            simulation.rng.range(4.5, 8.0)
        };
        let vy = (2.0 * GRAVITY * peak).sqrt();
        let flight = (vy + (vy * vy + 2.0 * GRAVITY * 0f64.max(y - 0.3)).sqrt()) / GRAVITY;
        // Uniform directions on a sphere give cartwheels and barrel rolls as often
        // as yaw spins, with an independent axis and spin speed for every piece.
        let axis_y = simulation.rng.range(-1.0, 1.0);
        let azimuth = simulation.rng.range(0.0, PI * 2.0);
        let radius = (1.0 - axis_y * axis_y).sqrt();
        let spin = simulation.rng.range(7.0, 14.0);
        let body = simulation.world.insert_body(
            RigidBodyBuilder::dynamic()
                .translation(vector(x, y, z))
                .linvel(vector(
                    (landing.x - x) / flight,
                    vy,
                    (landing.z - z) / flight,
                ))
                .angvel(vector(
                    radius * azimuth.cos() * spin,
                    axis_y * spin,
                    radius * azimuth.sin() * spin,
                ))
                .ccd_enabled(true),
        );
        let yaw = if part == WreckPart::Hull {
            heading
        } else {
            aim
        };
        let rigid_body = &mut simulation.world.bodies[body];
        rigid_body.set_additional_solver_iterations(2);
        rigid_body.set_rotation(to_rotation(Quat4::yaw(yaw)), true);
        let size = match part {
            WreckPart::Hull => [1.22, 0.42, 1.45],
            WreckPart::Barrel => [0.2, 0.2, 0.95],
            _ => [
                0.95,
                0.45,
                if part == WreckPart::TurretBarrel {
                    1.55
                } else {
                    1.0
                },
            ],
        };
        let length_scale = if part == WreckPart::Hull && kind == VehicleKind::Heavy {
            1.18
        } else {
            1.0
        };
        let share = match part {
            WreckPart::Hull => 0.7,
            WreckPart::Barrel => 0.07,
            _ if detached => 0.23,
            _ => 0.3,
        };
        let collider = simulation.world.insert_collider(
            ColliderBuilder::cuboid(
                (size[0] * scale) as f32,
                (size[1] * scale) as f32,
                (size[2] * scale * length_scale) as f32,
            )
            .collision_groups(interaction_groups(group::WRECK))
            .mass((mass * share) as f32)
            .friction(0.95)
            .restitution(0.05),
            Some(body),
        );
        let fragment_id = simulation.next_id;
        simulation.next_id += 1;
        track_debris_contacts(
            simulation,
            body,
            collider,
            fragment_id,
            DebrisMaterial::Metal,
        );
        // Preserve the old random cleanup-choice draw in the seeded combat stream.
        simulation.rng.next();
        let mut fragment = wreck_fragment(
            fragment_id,
            body,
            flight + 2.7 + DEBRIS_CLEANUP_SECONDS,
            simulation.elapsed,
        );
        fragment.wreck = Some(kind);
        fragment.team = Some(team);
        fragment.part = Some(part);
        simulation.fragments.push(fragment);
    }
}

fn wreck_fragment(
    id: u32,
    body: rapier3d::prelude::RigidBodyHandle,
    life: f64,
    elapsed: f64,
) -> Fragment {
    let mut fragment = Fragment::new(id, body, life, 1.0, WRECK_COLOR);
    fragment.expires_at = Some(elapsed + WRECK_DEADLINE);
    fragment.created_at = Some(elapsed);
    fragment.material = Some(DebrisMaterial::Metal);
    fragment
}
