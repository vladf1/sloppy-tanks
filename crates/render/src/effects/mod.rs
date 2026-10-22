//! Runtime visual effects: particles, blasts and scorches, sparks, muzzle
//! flashes, shell tracers, burning wrecks, tread marks and dust, gravel,
//! quarry wind dust, crate splinters and packing straw, falling leaves,
//! projectiles in flight, laser defense and supply drop smoke, plus
//! the explosion and muzzle light flashes, and the WGSL effect registry their
//! materials use.
//!
//! Every effect is a bounded CPU system that writes instance records into a
//! [`PoolBuffer`]; the renderer draws each pool as one instanced call from a
//! fixed-size GPU buffer, uploading only changed ranges (`gpu::Renderer::sync_pool`).
//! Effects are cosmetic: they read `RenderState` and `SimEvent`s, never move
//! simulation state, and draw from their own [`CosmeticRandom`], never from the
//! seeded simulation stream. `EffectSystems` is the whole CPU side (native, unit
//! tested); `Effects` binds it to the renderer's pools.

pub mod explosions;
pub mod ground;
pub mod laser;
pub mod leaves;
pub mod looks;
pub mod muzzle;
pub mod particles;
pub mod pickups;
pub mod pool;
pub mod projectiles;
pub mod quarry_dust;
pub mod random;
pub mod registry;
pub mod rocket_smoke;
pub mod sparks;
pub mod spawn_pad_decks;
pub mod tracers;
pub mod track_dust;
pub mod track_gravel;
pub mod tracks;
pub mod wood_chips;
pub mod wreck_fire;

use glam::Vec3;
use sloppy_core::sim::{RenderState, SimEvent, SimEventType};

pub use pool::{PoolBuffer, PoolDesc};
pub use random::CosmeticRandom;
pub use registry::*;

use ground::Ground;
use laser::LaserVisuals;
use leaves::LeafFall;
use muzzle::MuzzleFlashes;
use particles::ParticleEffects;
use pickups::PickupEffects;
use projectiles::ProjectileVisuals;
use quarry_dust::QuarryDust;
use rocket_smoke::RocketSmoke;
use tracers::Tracers;
use track_dust::TrackDust;
use tracks::TrackTrails;
use wood_chips::WoodChips;
use wreck_fire::WreckFires;

/// Blasts, hits and burning wrecks light their surroundings through this many
/// point lights, the brightest of them each frame (`EffectSystems::lights`).
pub const FIRE_LIGHTS: usize = 3;
pub const FLASH_DECAY: f32 = 2.0;
/// Below this a light is switched off rather than shading every pixel.
pub const LIGHT_CUTOFF: f32 = 0.05;
/// Default pickup color when an event carries none.
const WHITE: u32 = 0xffffff;
/// The muzzle flash light's color.
pub const MUZZLE_FLASH_COLOR: u32 = 0xffc070;
/// Every puff the shared puff pool may hold: blasts, muzzle smoke, burning wrecks.
pub const PUFF_CAPACITY: usize = explosions::MAX_EXPLOSIONS * explosions::PUFFS_PER_BLAST
    + muzzle::MAX_MUZZLE_FLASHES * muzzle::PUFFS_PER_MUZZLE
    + wreck_fire::MAX_BURNING_WRECKS * wreck_fire::PUFFS_PER_WRECK;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Flash {
    pub position: Vec3,
    pub intensity: f32,
    pub color: u32,
}

/// Keep `light` in `lights` if it outshines the dimmest one there.
pub fn offer_light(lights: &mut [Flash], light: Flash) {
    if light.intensity <= LIGHT_CUTOFF {
        return;
    }
    let dimmest = lights
        .iter_mut()
        .min_by(|a, b| a.intensity.total_cmp(&b.intensity));
    if let Some(slot) = dimmest.filter(|slot| slot.intensity < light.intensity) {
        *slot = light;
    }
}

/// Live effect counts, for tests and "Stats for nerds".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectStats {
    /// Chips, clods and falling leaves: every exported "particles" count
    /// (glowing sparks are streak instances).
    pub particles: u32,
    /// Crate splinters and packing straw (their pools' instances).
    pub wood_chips: u32,
    pub blasts: u32,
    pub puffs: u32,
    pub blast_rings: u32,
    pub track_marks: u32,
    pub track_dust: u32,
    pub gravel: u32,
    pub quarry_dust: u32,
    pub projectiles: u32,
    pub laser_beams: u32,
    pub pickup_effects: u32,
    /// Instances across every pool.
    pub instances: u32,
}

/// Every effect's CPU state.
#[derive(Clone, Debug, Default)]
pub struct EffectSystems {
    pub random: CosmeticRandom,
    pub particles: ParticleEffects,
    pub leaves: LeafFall,
    pub wood: WoodChips,
    pub tracks: TrackTrails,
    pub track_dust: TrackDust,
    pub quarry_dust: QuarryDust,
    pub projectiles: ProjectileVisuals,
    pub tracers: Tracers,
    pub rocket_smoke: RocketSmoke,
    pub muzzle: MuzzleFlashes,
    pub wrecks: WreckFires,
    pub laser: LaserVisuals,
    pub pickups: PickupEffects,
    /// This frame's brightest fire lights.
    pub lights: [Flash; FIRE_LIGHTS],
}

impl EffectSystems {
    pub fn reset(&mut self) {
        self.particles.reset();
        self.leaves.reset();
        self.wood.reset();
        self.tracks.reset();
        self.track_dust.reset();
        self.quarry_dust.reset();
        self.projectiles.reset();
        self.tracers.reset();
        self.rocket_smoke.reset();
        self.muzzle.reset();
        self.wrecks.reset();
        self.laser.reset();
        self.pickups.reset();
        self.lights = [Flash::default(); FIRE_LIGHTS];
    }

    /// The visual response to one simulation event (`presentation.ts` `event`).
    pub fn event(&mut self, event: &SimEvent) {
        use SimEventType as T;
        // A hard landing raises dust; notices have no visual response.
        if event.kind == T::DebrisImpact {
            self.particles.explosions.landing(event);
            return;
        }
        if event.kind == T::Notice {
            return;
        }
        self.laser.event(event);
        if event.kind == T::Hurt && (event.id.is_none() || event.size.unwrap_or(0.0) <= 0.0) {
            return;
        }
        if event.kind == T::Respawn {
            return;
        }
        if matches!(event.kind, T::Pickup | T::Promotion) {
            self.pickups
                .add(event.x, event.z, event.color.unwrap_or(WHITE));
        }
        self.wood.event(event, &mut self.random);
        self.muzzle.event(event);
        self.wrecks.event(event, &mut self.random);
        self.particles.event(event, &mut self.random);
    }

    /// Leaves shaken from `crown` by a tree hit or felling, once presentation knows
    /// which tree the event belongs to.
    pub fn shed_leaves(&mut self, event: &SimEvent, crown: &leaves::Crown) {
        self.leaves.event(event, crown, &mut self.random);
    }

    /// Advance one rendered frame, in the order `presentation.ts` `render` used.
    pub fn update(&mut self, state: &RenderState, alpha: f64, dt: f64, time: f64) {
        self.pickups.update(dt);
        self.tracks.update(state, alpha);
        self.particles.explosions.pads.sync(state);
        let ground = Ground::from_theme(&state.map_theme);
        self.particles.explosions.ground = ground;
        self.track_dust.update(state, &mut self.random);
        self.quarry_dust.update(state, dt, &mut self.random);
        self.projectiles.update(&state.shots, time, alpha);
        self.rocket_smoke
            .update(&state.shots, alpha, dt, &mut self.random);
        self.laser.update(state, alpha, dt);
        // The sparks and blasts rebuild the streak and puff pools; tracers,
        // muzzle flashes and burning wrecks add theirs after them.
        self.particles.update(dt, time);
        let particles = &mut self.particles;
        self.tracers
            .update(&state.shots, alpha, &mut particles.sparks.records);
        self.muzzle.update(
            state,
            alpha,
            dt,
            ground,
            &mut particles.sparks,
            &mut particles.explosions.puffs,
            &mut self.random,
        );
        self.wrecks.update(
            state,
            dt,
            &mut particles.explosions.puffs,
            &mut particles.sparks,
            &mut self.random,
        );
        self.pickups.draw(state, &mut particles.explosions.puffs);
        self.leaves.update(dt, &mut self.random);
        self.wood.update(dt, &mut self.random);
        self.lights = [Flash::default(); FIRE_LIGHTS];
        self.particles.explosions.lights(&mut self.lights);
        self.wrecks.lights(&mut self.lights);
    }

    /// Every pool buffer, in `looks::pool_descs` order.
    pub fn for_each_pool(&mut self, mut visit: impl FnMut(&mut PoolBuffer)) {
        visit(&mut self.tracks.records);
        visit(&mut self.track_dust.records);
        visit(&mut self.track_dust.gravel.records);
        visit(&mut self.quarry_dust.records);
        for batch in &mut self.projectiles.batches {
            visit(&mut batch.body);
            visit(&mut batch.team);
            if let Some(exhaust) = &mut batch.exhaust {
                visit(exhaust);
            }
        }
        visit(&mut self.rocket_smoke.records);
        visit(&mut self.laser.halo);
        visit(&mut self.laser.core);
        visit(&mut self.laser.mount);
        visit(&mut self.laser.lens);
        visit(&mut self.particles.records);
        visit(&mut self.leaves.records);
        visit(&mut self.wood.wood_records);
        visit(&mut self.wood.straw_records);
        visit(&mut self.particles.explosions.rings);
        visit(&mut self.particles.explosions.puffs);
        visit(&mut self.particles.sparks.records);
    }

    pub fn stats(&mut self) -> EffectStats {
        let mut instances = 0;
        self.for_each_pool(|pool| instances += pool.len() as u32);
        let explosions = &self.particles.explosions;
        EffectStats {
            particles: (self.particles.particles.len() + self.leaves.len()) as u32,
            wood_chips: self.wood.len() as u32,
            blasts: explosions.active() as u32,
            puffs: explosions.puffs.len() as u32,
            blast_rings: explosions.rings.len() as u32,
            track_marks: self.tracks.len() as u32,
            track_dust: self.track_dust.len() as u32,
            gravel: self.track_dust.gravel.len() as u32,
            quarry_dust: self.quarry_dust.len() as u32,
            projectiles: self
                .projectiles
                .batches
                .iter()
                .map(|batch| batch.body.len() as u32)
                .sum(),
            laser_beams: self.laser.beams() as u32,
            pickup_effects: self.pickups.len() as u32,
            instances,
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use browser::Effects;

#[cfg(target_arch = "wasm32")]
mod browser {
    use sloppy_core::sim::RenderState;

    use super::muzzle::MUZZLE_LIGHT_DISTANCE;
    use glam::Vec3;

    use super::explosions::BLAST_LIGHT_DISTANCE;
    use super::{EffectSystems, FLASH_DECAY, LIGHT_CUTOFF, MUZZLE_FLASH_COLOR};
    use crate::gpu::{PointLight, PoolId, Renderer};

    /// The point-light slots the fire lights and the muzzle flash use.
    const FIRE_LIGHT_SLOTS: [usize; super::FIRE_LIGHTS] = [0, 2, 3];
    const MUZZLE_LIGHT: usize = 1;

    /// The effect systems bound to renderer pools (created once, kept across rounds).
    pub struct Effects {
        /// The CPU side; callers send it events and read its stats directly.
        pub systems: EffectSystems,
        pools: Vec<PoolId>,
    }

    impl Effects {
        /// Register every pool now, so their pipelines are part of
        /// `prepare_step`/`warm_up` and the first blast, mark or shell never compiles
        /// mid-round.
        pub fn new(renderer: &mut Renderer) -> Self {
            let pools = super::looks::pool_descs()
                .iter()
                .map(|desc| renderer.add_pool(desc))
                .collect();
            Self {
                systems: EffectSystems::default(),
                pools,
            }
        }

        pub fn reset(&mut self, renderer: &mut Renderer) {
            self.systems.reset();
            self.sync(renderer);
        }

        /// Once per rendered frame after entity poses are updated; `alpha`
        /// interpolates physics poses.
        pub fn update(
            &mut self,
            renderer: &mut Renderer,
            state: &RenderState,
            alpha: f64,
            dt: f64,
            time: f64,
        ) {
            self.systems.update(state, alpha, dt, time);
            // Smoke puffs overlap heavily: composite them back to front from this
            // frame's eye (presentation sets the camera before effects update).
            let eye = renderer.camera().position;
            let puffs = &mut self.systems.particles.explosions.puffs;
            puffs.sort_back_to_front(eye);
            if let Some(viewer) = state.viewer().filter(|tank| tank.alive) {
                let target = Vec3::new(
                    viewer.position.x as f32,
                    viewer.position.y as f32 + 0.4,
                    viewer.position.z as f32,
                );
                super::explosions::clear_sight_line(puffs, eye, target);
            }
            self.sync(renderer);
        }

        fn sync(&mut self, renderer: &mut Renderer) {
            let Self { systems, pools } = self;
            let mut next = pools.iter();
            systems.for_each_pool(|buffer| {
                if let Some(&id) = next.next() {
                    renderer.sync_pool(id, buffer);
                }
            });
            // The tread-mark fade clock: pool 0 is the track marks.
            let mut params = [[0.0; 4]; 4];
            params[0][0] = systems.tracks.clock as f32;
            renderer.set_pool_params(pools[0], params);
            for (slot, light) in FIRE_LIGHT_SLOTS.into_iter().zip(systems.lights) {
                renderer.set_point_light(
                    slot,
                    (light.intensity > LIGHT_CUTOFF).then_some(PointLight {
                        position: light.position,
                        color: light.color,
                        intensity: light.intensity,
                        distance: BLAST_LIGHT_DISTANCE,
                        decay: FLASH_DECAY,
                    }),
                );
            }
            let muzzle = systems.muzzle.light;
            renderer.set_point_light(
                MUZZLE_LIGHT,
                (muzzle.intensity > LIGHT_CUTOFF).then_some(PointLight {
                    position: muzzle.position,
                    color: MUZZLE_FLASH_COLOR,
                    intensity: muzzle.intensity,
                    distance: MUZZLE_LIGHT_DISTANCE,
                    decay: FLASH_DECAY,
                }),
            );
        }
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use sloppy_core::sim::render_state::RenderTank;
    use sloppy_core::sim::{CoverKind, MatchPhase, Point3, SimEvent, Weapon};

    #[test]
    fn every_pool_has_a_look_with_its_capacity() {
        let descs = looks::pool_descs();
        let mut systems = EffectSystems::default();
        let mut capacities = Vec::new();
        systems.for_each_pool(|pool| capacities.push(pool.capacity() as u32));
        assert_eq!(capacities.len(), descs.len());
        for (capacity, desc) in capacities.iter().zip(&descs) {
            assert_eq!(*capacity, desc.capacity, "{}", desc.label);
            assert!(desc.mesh.triangle_count() > 0, "{}", desc.label);
        }
        assert_eq!(
            descs[0].label, "track marks",
            "the track clock pool comes first"
        );
        // Every custom effect a look uses exists in the registry.
        let registry = EffectRegistry::default();
        for desc in &descs {
            if let sloppy_core::scene::Effect::Custom { name, .. } = &desc.material.effect {
                assert!(registry.id(name).is_some(), "{name}");
            }
        }
    }

    #[test]
    fn falling_leaves_count_among_the_exported_particles() {
        let mut systems = EffectSystems::default();
        let mut hit = SimEvent::at(SimEventType::Impact, 0.0, 0.0);
        hit.cover_kind = Some(sloppy_core::sim::CoverKind::Tree);
        systems.event(&hit);
        let chips = systems.particles.particles.len();
        let crown = leaves::Crown {
            x: 0.0,
            z: 0.0,
            foliage: sloppy_core::models::TreeFoliage {
                conifer: false,
                color: 0x6a8a3c,
                bottom: 2.5,
                top: 5.5,
                radius: 1.1,
            },
        };
        systems.shed_leaves(&hit, &crown);
        let stats = systems.stats();
        assert!(!systems.leaves.is_empty());
        assert_eq!(stats.particles as usize, chips + systems.leaves.len());
    }

    #[test]
    fn events_route_like_presentation() {
        let mut systems = EffectSystems::default();
        let mut hurt = SimEvent::at(SimEventType::Hurt, 0.0, 0.0);
        systems.event(&hurt);
        assert!(
            systems.particles.particles.is_empty(),
            "hurt without a tank id"
        );
        hurt.id = Some(1);
        hurt.size = Some(10.0);
        systems.event(&hurt);
        assert_eq!(systems.particles.particles.len(), 12);
        systems.event(&SimEvent::at(SimEventType::Respawn, 0.0, 0.0));
        systems.event(&SimEvent::at(SimEventType::Notice, 0.0, 0.0));
        systems.event(&SimEvent::at(SimEventType::DebrisImpact, 0.0, 0.0));
        assert_eq!(systems.particles.particles.len(), 12);
        let mut promotion = SimEvent::at(SimEventType::Promotion, 2.0, 3.0);
        promotion.id = Some(1);
        promotion.color = Some(0xffcf54);
        systems.event(&promotion);
        assert_eq!(systems.pickups.len(), 1);
        systems.update(&RenderState::default(), 1.0, 1.0 / 60.0, 0.0);
        assert!(systems.lights.iter().all(|light| light.intensity == 0.0));
        let mut blast = SimEvent::at(SimEventType::Explosion, 5.0, -4.0);
        blast.size = Some(4.0);
        systems.event(&blast);
        systems.update(&RenderState::default(), 1.0, 1.0 / 60.0, 0.0);
        let lit = systems
            .lights
            .iter()
            .find(|light| light.intensity > 10.0)
            .expect("the blast lights its surroundings");
        assert_eq!((lit.position.x, lit.position.z), (5.0, -4.0));
        let mut drum = SimEvent::at(SimEventType::Destroy, 0.0, 0.0);
        drum.cover_kind = Some(CoverKind::Drum);
        systems.event(&drum);
        assert_eq!(
            systems.particles.explosions.active(),
            1,
            "drums blast via their explosion"
        );
    }

    #[test]
    fn crate_hits_and_bursts_throw_lit_wood_and_dust_without_a_flash() {
        let mut systems = EffectSystems::default();
        for kind in [SimEventType::Impact, SimEventType::Destroy] {
            let mut event = SimEvent::at(kind, 0.0, 0.0);
            event.cover_kind = Some(CoverKind::Cargo);
            event.from = Some(Point3::new(0.0, 1.2, 1.0));
            systems.event(&event);
        }
        assert!(systems.particles.particles.is_empty());
        assert_eq!(systems.particles.explosions.active(), 2);
        systems.update(&RenderState::default(), 1.0, 1.0 / 60.0, 0.0);
        assert!(systems.lights.iter().all(|light| light.intensity == 0.0));
        let stats = systems.stats();
        assert_eq!(stats.particles, 0);
        assert_eq!(stats.wood_chips as usize, systems.wood.len());
        assert_eq!(
            systems.wood.wood_records.len() + systems.wood.straw_records.len(),
            systems.wood.len()
        );
        systems.reset();
        let cleared = systems.stats();
        assert_eq!(cleared.wood_chips + cleared.instances, 0);
    }

    #[test]
    fn a_busy_round_stays_bounded_and_reset_clears_every_pool() {
        let mut systems = EffectSystems::default();
        let mut state = RenderState::default();
        state.map_theme = "quarry".into();
        state.match_state.phase = MatchPhase::Playing;
        for id in 0..30 {
            state.tanks.push(RenderTank {
                id,
                alive: true,
                laser: 1.0,
                position: Point3 {
                    x: 0.0,
                    y: 0.65,
                    z: f64::from(id) * 3.0 - 45.0,
                },
                ..RenderTank::default()
            });
        }
        let dt = 1.0 / 60.0;
        let mut peak = EffectStats::default();
        for frame in 0..60 * 30 {
            state.elapsed = frame as f64 * dt;
            for tank in &mut state.tanks {
                tank.previous.x = tank.position.x;
                tank.previous.z = tank.position.z;
                tank.heading += 0.02;
                tank.position.x += tank.heading.sin() * 12.0 * dt;
                tank.position.z += tank.heading.cos() * 12.0 * dt;
                tank.velocity = Point3 {
                    x: tank.heading.sin() * 12.0,
                    y: 0.0,
                    z: tank.heading.cos() * 12.0,
                };
            }
            state.shots = (0..40)
                .map(|i| sloppy_core::sim::render_state::RenderShot {
                    id: i,
                    x: f64::from(i),
                    z: 0.0,
                    vx: 30.0,
                    weapon: [Weapon::Rocket, Weapon::Standard, Weapon::Tow][i as usize % 3],
                    ..Default::default()
                })
                .collect();
            for kind in [
                SimEventType::Explosion,
                SimEventType::Death,
                SimEventType::Pickup,
                SimEventType::Shot,
                SimEventType::Impact,
                SimEventType::Ricochet,
            ] {
                let mut event = SimEvent::at(kind, frame as f64 % 50.0, 0.0);
                event.size = Some(4.0);
                systems.event(&event);
            }
            let mut laser = SimEvent::at(SimEventType::Laser, 3.0, 3.0);
            laser.from = Some(Point3::ZERO);
            systems.event(&laser);
            if frame % 10 == 0 {
                for kind in [SimEventType::Impact, SimEventType::Destroy] {
                    let mut crate_event = SimEvent::at(kind, frame as f64 % 30.0, 8.0);
                    crate_event.cover_kind = Some(CoverKind::Cargo);
                    systems.event(&crate_event);
                }
            }
            systems.update(&state, 1.0, dt, state.elapsed);
            let stats = systems.stats();
            peak.instances = peak.instances.max(stats.instances);
            peak.particles = peak.particles.max(stats.particles);
            peak.track_marks = peak.track_marks.max(stats.track_marks);
            peak.wood_chips = peak.wood_chips.max(stats.wood_chips);
        }
        let stats = systems.stats();
        assert!(peak.particles as usize <= particles::MAX_PARTICLES);
        assert!(peak.particles > 1000, "the particle pool saturates");
        let wood_capacity = wood_chips::MAX_WOOD_CHIPS + wood_chips::MAX_STRAW;
        assert_eq!(
            peak.wood_chips as usize, wood_capacity,
            "the wood pools saturate"
        );
        assert!(stats.track_marks > 1000 && stats.track_dust > 0 && stats.quarry_dust > 0);
        assert_eq!(stats.projectiles, 40);
        assert!(stats.puffs > 0 && stats.blast_rings > 0 && stats.pickup_effects > 0);
        let capacity: u32 = looks::pool_descs().iter().map(|d| d.capacity).sum();
        assert!(peak.instances <= capacity);
        systems.reset();
        let cleared = systems.stats();
        assert_eq!(cleared.instances, 0);
        assert_eq!(
            cleared.blasts + cleared.laser_beams + cleared.pickup_effects,
            0
        );
    }
}
