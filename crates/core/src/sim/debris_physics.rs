//! Physical debris response: blasts, shell hits, and contact-force impact cues.

use rapier3d::prelude::{ActiveEvents, ColliderHandle, RigidBodyHandle};
use serde::{Deserialize, Serialize};

use super::debris_cleanup::DEBRIS_CLEANUP_SECONDS;
use super::math::{Point3, Vec2, hypot3};
use super::physics::{from_vector, vector};
use super::simulation::Simulation;
use super::types::{CoverKind, Shot, SimEvent, SimEventType, Weapon};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DebrisMaterial {
    Wood,
    Metal,
    Concrete,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DebrisSurface {
    pub friction: f64,
    pub restitution: f64,
}

pub const fn debris_material(material: DebrisMaterial) -> DebrisSurface {
    match material {
        DebrisMaterial::Wood => DebrisSurface {
            friction: 0.65,
            restitution: 0.16,
        },
        DebrisMaterial::Metal => DebrisSurface {
            friction: 0.95,
            restitution: 0.12,
        },
        // The lightest tank must overcome ground friction under sustained drive.
        // Keep the concrete mass and low bounce; grip must not pin it in place.
        DebrisMaterial::Concrete => DebrisSurface {
            friction: 0.65,
            restitution: 0.02,
        },
    }
}

/// Per-body metadata for contact-force cues (the former Rapier body `userData`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DebrisContact {
    pub id: u32,
    pub material: DebrisMaterial,
    pub impact_after: f64,
}

/// Maximum debris-impact cues per tick.
const IMPACTS_PER_TICK: usize = 8;
/// Seconds before the same body may report another impact.
const IMPACT_REPEAT_SECONDS: f64 = 0.3;
/// Contact force per unit of body mass that counts as an impact rather than resting weight.
const IMPACT_FORCE_PER_MASS: f64 = 65.0;

/// Metadata lives on substantial bodies, allowing contacts to feed presentation later.
pub fn track_debris_contacts(
    simulation: &mut Simulation,
    body: RigidBodyHandle,
    collider: ColliderHandle,
    id: u32,
    material: DebrisMaterial,
) {
    simulation.debris_contacts.insert(
        body,
        DebrisContact {
            id,
            material,
            impact_after: 0.0,
        },
    );
    // Ignore resting weight. This threshold scales with the body's mass.
    let mass = simulation.world.bodies[body].mass() as f64;
    let collider = &mut simulation.world.colliders[collider];
    collider.set_active_events(ActiveEvents::CONTACT_FORCE_EVENTS);
    collider.set_contact_force_event_threshold((mass * IMPACT_FORCE_PER_MASS) as f32);
}

/// Fixed-size fragment pool + a short movable-cover list; no all-pairs debris work.
/// Sleeping bodies are deliberately included, and this never consumes combat RNG.
pub fn blast_debris(simulation: &mut Simulation, origin: Vec2, radius: f64, power: f64) {
    for i in 0..simulation.fragments.len() {
        let fragment = &simulation.fragments[i];
        if fragment.life <= DEBRIS_CLEANUP_SECONDS {
            continue;
        }
        let lever = if fragment.wreck.is_some() {
            0.35
        } else if let Some(size) = fragment.dimensions {
            0.4f64.min(size.x.max(size.y).max(size.z) * 0.2)
        } else {
            fragment.size * 0.25
        };
        let max_velocity = if fragment.wreck.is_some() { 8.0 } else { 14.0 };
        let body = fragment.body;
        if blast_body(simulation, body, origin, radius, power, false, lever, max_velocity) {
            let elapsed = simulation.elapsed;
            let fragment = &mut simulation.fragments[i];
            // Let a second launch finish, but never extend life beyond the original deadline.
            if let Some(expires_at) = fragment.expires_at {
                fragment.life = fragment.life.max(5.0).min(expires_at - elapsed);
            }
        }
    }
    for m in 0..simulation.movable_covers.len() {
        let cover = &simulation.covers[simulation.movable_covers[m]];
        if cover.alive {
            let drum = cover.kind == CoverKind::Drum;
            let body = cover.body;
            blast_body(simulation, body, origin, radius, power, !drum, 0.35, if drum { 14.0 } else { 4.0 });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn blast_body(
    simulation: &mut Simulation,
    handle: RigidBodyHandle,
    origin: Vec2,
    radius: f64,
    power: f64,
    heavy: bool,
    lever: f64,
    max_velocity: f64,
) -> bool {
    if radius <= 0.0 || power <= 0.0 {
        return false;
    }
    let body = &mut simulation.world.bodies[handle];
    let p = from_vector(body.center_of_mass());
    let dx = p.x - origin.x;
    let dz = p.z - origin.z;
    let dy = 0f64.max(p.y - 0.75);
    let distance = hypot3(dx, dy, dz);
    if distance >= radius {
        return false;
    }
    let horizontal = dx.hypot(dz);
    // At the epicenter pressure has no preferred horizontal direction.
    let nx = if horizontal > 0.001 { dx / horizontal } else { 0.0 };
    let nz = if horizontal > 0.001 { dz / horizontal } else { 0.0 };
    let falloff = (1.0 - distance / radius).powi(2);
    let strength = (power / 60.0).min(1.8) * falloff;
    // Apply bounded force instead of cancelling mass: heavier pieces resist the same blast.
    let impulse = (body.mass() as f64 * max_velocity).min(if heavy { 24.0 } else { 12.0 }) * strength;
    body.apply_impulse_at_point(
        vector(nx * impulse, impulse * if heavy { 0.65 } else { 0.85 }, nz * impulse),
        // Pressure catches a facing edge above the centre, producing real pitch and roll.
        vector(
            p.x - nx * lever + nz * lever * 0.5,
            p.y + lever * 0.5,
            p.z - nz * lever - nx * lever * 0.5,
        ),
        true,
    );
    true
}

fn shell_impulse(shot: &Shot, rocket: f64, piercing: f64, other: f64) -> f64 {
    match shot.weapon {
        Weapon::Rocket => rocket,
        Weapon::Piercing => piercing,
        _ => other,
    }
}

/// Shells shove dragon's teeth and hedgehogs at the struck point.
pub fn hit_movable_cover(simulation: &mut Simulation, cover_index: usize, shot: &Shot) {
    let cover = &simulation.covers[cover_index];
    if (cover.kind != CoverKind::Teeth && cover.kind != CoverKind::Hedgehog) || cover.motion.is_none() {
        return;
    }
    let speed = shot.vx.hypot(shot.vz);
    if speed == 0.0 {
        return;
    }
    let impulse = shell_impulse(shot, 10.0, 8.4, 6.0);
    simulation.world.bodies[cover.body].apply_impulse_at_point(
        vector((shot.vx / speed) * impulse, 0.0, (shot.vz / speed) * impulse),
        vector(shot.x, shot.combat_y(), shot.z),
        true,
    );
}

/// Substantial debris absorbs the round and takes only physical impulse.
pub fn hit_projectile_debris(simulation: &mut Simulation, fragment_index: usize, shot: &Shot, point: Point3) {
    let fragment = &simulation.fragments[fragment_index];
    let speed = shot.vx.hypot(shot.vz);
    if (fragment.wreck.is_none() && fragment.dimensions.is_none()) || speed == 0.0 {
        return;
    }
    let impulse = shell_impulse(shot, 10.0, 7.0, 5.0);
    let body = &mut simulation.world.bodies[fragment.body];
    // The same impulse moves light wood more than heavy wreckage; cap tiny-piece launches.
    let strength = impulse.min(body.mass() as f64 * 5.0);
    body.apply_impulse_at_point(
        vector((shot.vx / speed) * strength, 0.0, (shot.vz / speed) * strength),
        vector(point.x, point.y, point.z),
        true,
    );
}

/// Real Rapier contact forces, bounded and rate-limited per body. No effect allocation
/// for sleeping contacts; presentation may map material/force to dust or sound.
pub fn drain_debris_contacts(simulation: &mut Simulation) {
    let mut count = 0;
    for event in simulation.contact_forces.drain() {
        if count >= IMPACTS_PER_TICK {
            continue;
        }
        for handle in [event.collider1, event.collider2] {
            let Some(body) = simulation.world.colliders.get(handle).and_then(|collider| collider.parent()) else {
                continue;
            };
            let elapsed = simulation.elapsed;
            let Some(data) = simulation.debris_contacts.get_mut(&body) else {
                continue;
            };
            if elapsed < data.impact_after {
                continue;
            }
            data.impact_after = elapsed + IMPACT_REPEAT_SECONDS;
            let (id, material) = (data.id, data.material);
            let p = simulation.body_translation(body);
            let mut impact = SimEvent::at(SimEventType::DebrisImpact, p.x, p.z);
            impact.id = Some(id);
            impact.height = Some(p.y);
            impact.material = Some(material);
            impact.force = Some(event.total_force_magnitude as f64);
            simulation.events.push(impact);
            count += 1;
            break;
        }
    }
}
