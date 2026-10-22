//! Tank wrecks: all substantial pieces, including detached guns, remain solid and pushable.

use std::f64::consts::PI;

use rapier3d::prelude::{ColliderBuilder, RigidBodyBuilder, RigidBodyHandle};

use super::data::{ARENA, group, vehicle};
use super::debris_cleanup::DEBRIS_CLEANUP_SECONDS;
use super::debris_physics::{DebrisMaterial, track_debris_contacts};
use super::math::{Quat4, Vec2, clamp};
use super::physics::{interaction_groups, to_rotation, vector};
use super::simulation::{Simulation, WreckView};
use super::simulation_rules::GRAVITY;
use super::tank_destruction::humvee_tumble;
use super::tank_dimensions::HUMVEE_BODY_LENGTH_SCALE;
use super::types::{CoverKind, Fragment, Team, VehicleKind, WreckPart};

const WRECK_COLOR: u32 = 0x46534c;
/// Seconds before a wreck piece's hard removal deadline.
const WRECK_DEADLINE: f64 = 18.0;
/// Seconds a landed wreck piece rests (burning, then smouldering) before it
/// sinks away, and a whole hull's rest after a burnout or a Humvee's tumble.
const WRECK_REST: f64 = 9.0;
const WHOLE_WRECK_REST: f64 = 11.0;
/// How far (m) a breakup throws the hull and turret apart: the heavy hull is
/// shoved only this share of the way, the turret the rest; a detached gun lands
/// this far behind them.
const WRECK_SEPARATION: [f64; 2] = [2.5, 5.0];
const HULL_SHOVE: f64 = 0.35;
const GUN_DROP: f64 = 2.0;
/// The far throws breakups once made, whose landing search still sets how many
/// combat draws a breakup takes (the seeded stream's contract): the separation
/// range and the detached gun's drop (m).
const LEGACY_SEPARATION: [f64; 2] = [7.0, 14.0];
const LEGACY_GUN_DROP: f64 = 5.0;
/// Where a landing blocked by cover looks next, without drawing from the combat
/// stream: these offsets (m) in turn.
const CLEAR_LANDINGS: [[f64; 2]; 8] = [
    [2.5, 0.0],
    [-2.5, 0.0],
    [0.0, 2.5],
    [0.0, -2.5],
    [4.0, 4.0],
    [-4.0, 4.0],
    [4.0, -4.0],
    [-4.0, -4.0],
];
/// Peak heights (m) a breakup lifts the hull, the turret, and the turret on a
/// one-in-four ammunition cook-off: tens of tonnes jump, they do not fly.
const HULL_PEAK: [f64; 2] = [0.5, 1.2];
const TURRET_PEAK: [f64; 2] = [1.8, 3.6];
const COOK_OFF_PEAK: [f64; 2] = [4.5, 6.5];
/// Spin (rad/s) of a thrown turret or gun; the hull turns at this share of it,
/// and tips (pitch and roll) at a smaller share again, so it lands on its tracks
/// a little askew rather than on its end.
const PIECE_SPIN: [f64; 2] = [4.0, 8.0];
const HULL_SPIN: f64 = 0.2;
const HULL_TIP: f64 = 0.35;
/// A tumbling Humvee is thrown the way it tips: m/s per rad/s of its roll and pitch.
const HUMVEE_THROW: f64 = 0.45;
/// A quiet burnout's hop (m): a jolt as the ammunition goes, not a launch.
const BURNOUT_HOP: f64 = 0.35;

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
            BURNOUT_HOP + (rock % 3.0) * 0.08
        };
        let pitch = if tumble {
            motion.pitch
        } else {
            (if rock % 2.0 != 0.0 { 1.0 } else { -1.0 }) * 0.85
        };
        let roll = if tumble { motion.roll } else { 0.0 };
        // Rolling about its length tips the roof toward (-cos, sin) of the heading;
        // pitching tips it forward. The blast throws it that way, so it tumbles
        // across the ground instead of turning over on the spot.
        let (throw_x, throw_z) = if tumble {
            (
                HUMVEE_THROW * (-heading.cos() * roll + heading.sin() * pitch),
                HUMVEE_THROW * (heading.sin() * roll + heading.cos() * pitch),
            )
        } else {
            (0.0, 0.0)
        };
        simulation.remove_body(old_body);
        simulation.reserve_fragments(1);
        let body = simulation.world.insert_body(
            RigidBodyBuilder::dynamic()
                .translation(vector(origin.x, origin.y - 0.4 + 0.55 * scale, origin.z))
                .linvel(vector(
                    velocity.x * 0.2 + throw_x,
                    (2.0 * GRAVITY * hop_height).sqrt(),
                    velocity.z * 0.2 + throw_z,
                ))
                .angvel(vector(
                    heading.cos() * pitch + heading.sin() * roll,
                    if tumble { motion.yaw } else { pitch * 0.15 },
                    -heading.sin() * pitch + heading.cos() * roll,
                ))
                .angular_damping((if tumble { motion.damping } else { 3.5 }) as f32)
                .ccd_enabled(true),
        );
        let half_length = if humvee {
            2.18 * HUMVEE_BODY_LENGTH_SCALE
        } else if kind == VehicleKind::Scout {
            2.2
        } else {
            2.7
        };
        let half_extents = [
            (if humvee { 1.16 } else { 1.22 }) * scale,
            (if humvee { 0.9 } else { 0.75 }) * scale,
            half_length * scale,
        ];
        add_wreck_piece(
            simulation,
            body,
            heading,
            half_extents,
            mass,
            WHOLE_WRECK_REST + DEBRIS_CLEANUP_SECONDS,
            kind,
            team,
            WreckPart::Intact,
        );
        return;
    }
    let detached = simulation.rng.next() < 0.4;
    let pieces: &[WreckPart] = if detached {
        &[WreckPart::Hull, WreckPart::Turret, WreckPart::Barrel]
    } else {
        &[WreckPart::Hull, WreckPart::TurretBarrel]
    };
    // Explosion travel is in world units, independent of visual model scale. One
    // draw places both the throw and the far throw breakups once made.
    let throw = simulation.rng.next();
    let half_separation = WRECK_SEPARATION[0] + (WRECK_SEPARATION[1] - WRECK_SEPARATION[0]) * throw;
    let legacy_separation =
        LEGACY_SEPARATION[0] + (LEGACY_SEPARATION[1] - LEGACY_SEPARATION[0]) * throw;
    let angle = simulation.rng.range(-0.45, 0.45);
    let high = simulation.rng.next() < 0.25;
    // Only on-screen explosions use view bounds; off-screen combat stays local. Online
    // simulations never set a view.
    let bounds = match simulation.wreck_view {
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
    let centre = |separation: f64| {
        let edge = separation.min((max_x - min_x) / 2.0);
        Vec2::new(
            clamp(origin.x, min_x + edge, max_x - edge),
            clamp(origin.z, min_z + 2.0, max_z - 2.0),
        )
    };
    let (center, legacy_center) = (centre(half_separation), centre(legacy_separation));
    simulation.remove_body(old_body);
    for (index, &part) in pieces.iter().enumerate() {
        simulation.reserve_fragments(1);
        let place = |center: Vec2, side: f64, separation: f64, drop: f64| {
            Vec2::new(
                clamp(center.x + side * separation * angle.cos(), min_x, max_x),
                clamp(
                    center.z + side * separation * angle.sin()
                        - if index == 2 { drop } else { 0.0 },
                    min_z,
                    max_z,
                ),
            )
        };
        let side = if index == 0 { -HULL_SHOVE } else { 1.0 };
        let mut landing = place(center, side, half_separation, GUN_DROP);
        let blocked = |simulation: &Simulation, position: Vec2| {
            simulation.covers.iter().any(|cover| {
                cover.alive
                    && cover.kind != CoverKind::Boundary
                    && (position.x - cover.x).abs() < cover.w / 2.0 + 1.5
                    && (position.z - cover.z).abs() < cover.d / 2.0 + 1.5
            })
        };
        // The far throw's landing search still takes its draws, so a kill leaves
        // the combat stream where it always did.
        let mut legacy = place(
            legacy_center,
            if index == 0 { -1.0 } else { 1.0 },
            legacy_separation,
            LEGACY_GUN_DROP,
        );
        let mut attempt = 0;
        while blocked(simulation, legacy) && attempt < 12 {
            let x = clamp(legacy.x + simulation.rng.range(-3.0, 3.0), min_x, max_x);
            let z = clamp(legacy.z + simulation.rng.range(-3.0, 3.0), min_z, max_z);
            legacy = Vec2::new(x, z);
            attempt += 1;
        }
        if blocked(simulation, landing) {
            let start = landing;
            for [dx, dz] in CLEAR_LANDINGS {
                let next = Vec2::new(
                    clamp(start.x + dx, min_x, max_x),
                    clamp(start.z + dz, min_z, max_z),
                );
                if !blocked(simulation, next) {
                    landing = next;
                    break;
                }
            }
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
        let [low, top] = match part {
            WreckPart::Hull => HULL_PEAK,
            _ if high => COOK_OFF_PEAK,
            _ => TURRET_PEAK,
        };
        let peak = simulation.rng.range(low, top);
        let vy = (2.0 * GRAVITY * peak).sqrt();
        let flight = (vy + (vy * vy + 2.0 * GRAVITY * 0f64.max(y - 0.3)).sqrt()) / GRAVITY;
        // Uniform directions on a sphere give cartwheels and barrel rolls as often
        // as yaw spins, with an independent axis and spin speed for every piece.
        let axis_y = simulation.rng.range(-1.0, 1.0);
        let azimuth = simulation.rng.range(0.0, PI * 2.0);
        let radius = (1.0 - axis_y * axis_y).sqrt();
        let hull = part == WreckPart::Hull;
        let spin =
            simulation.rng.range(PIECE_SPIN[0], PIECE_SPIN[1]) * if hull { HULL_SPIN } else { 1.0 };
        let tip = if hull { HULL_TIP } else { 1.0 };
        let body = simulation.world.insert_body(
            RigidBodyBuilder::dynamic()
                .translation(vector(x, y, z))
                .linvel(vector(
                    (landing.x - x) / flight,
                    vy,
                    (landing.z - z) / flight,
                ))
                .angvel(vector(
                    radius * azimuth.cos() * spin * tip,
                    axis_y * spin,
                    radius * azimuth.sin() * spin * tip,
                ))
                .ccd_enabled(true),
        );
        let yaw = if part == WreckPart::Hull {
            heading
        } else {
            aim
        };
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
        let half_extents = [
            size[0] * scale,
            size[1] * scale,
            size[2] * scale * length_scale,
        ];
        // Preserve the old random cleanup-choice draw in the seeded combat stream.
        simulation.rng.next();
        add_wreck_piece(
            simulation,
            body,
            yaw,
            half_extents,
            mass * share,
            flight + WRECK_REST + DEBRIS_CLEANUP_SECONDS,
            kind,
            team,
            part,
        );
    }
}

/// Give a launched wreck body its yaw and solid box collider, then record it as a metal
/// fragment with the wreck deadline.
#[allow(clippy::too_many_arguments)]
fn add_wreck_piece(
    simulation: &mut Simulation,
    body: RigidBodyHandle,
    yaw: f64,
    half_extents: [f64; 3],
    mass: f64,
    life: f64,
    kind: VehicleKind,
    team: Team,
    part: WreckPart,
) {
    let rigid_body = &mut simulation.world.bodies[body];
    rigid_body.set_additional_solver_iterations(2);
    rigid_body.set_rotation(to_rotation(Quat4::yaw(yaw)), true);
    let [x, y, z] = half_extents.map(|extent| extent as f32);
    let collider = simulation.world.insert_collider(
        ColliderBuilder::cuboid(x, y, z)
            .collision_groups(interaction_groups(group::WRECK))
            .mass(mass as f32)
            .friction(0.95)
            .restitution(0.05),
        Some(body),
    );
    let id = simulation.allocate_id();
    track_debris_contacts(simulation, body, collider, id, DebrisMaterial::Metal);
    let elapsed = simulation.elapsed;
    let mut fragment = Fragment::new(id, body, life, 1.0, WRECK_COLOR);
    fragment.expires_at = Some(elapsed + WRECK_DEADLINE);
    fragment.created_at = Some(elapsed);
    fragment.material = Some(DebrisMaterial::Metal);
    fragment.wreck = Some(kind);
    fragment.team = Some(team);
    fragment.part = Some(part);
    simulation.fragments.push(fragment);
}
