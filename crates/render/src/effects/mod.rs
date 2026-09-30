//! Runtime visual effects: particles, blasts, tread marks and dust, gravel,
//! quarry wind dust, projectiles in flight, laser defense and pickup rings, plus
//! the explosion light flash, and the WGSL effect registry their materials use.
//!
//! Every effect is a bounded CPU system that writes instance records into a
//! [`PoolBuffer`]; the renderer draws each pool as one instanced call from a
//! fixed-size GPU buffer, uploading only changed ranges (`gpu::Renderer::sync_pool`).
//! Effects are cosmetic: they read `RenderState` and `SimEvent`s, never move
//! simulation state, and draw from their own [`CosmeticRandom`], never from the
//! seeded simulation stream. `EffectSystems` is the whole CPU side (native, unit
//! tested); `Effects` binds it to the renderer's pools.

pub mod explosions;
pub mod laser;
pub mod looks;
pub mod particles;
pub mod pickups;
pub mod pool;
pub mod projectiles;
pub mod quarry_dust;
pub mod random;
pub mod registry;
pub mod spawn_pad_decks;
pub mod track_dust;
pub mod track_gravel;
pub mod tracks;

use glam::Vec3;
use sloppy_core::sim::{RenderState, SimEvent, SimEventType};

pub use pool::{PoolBuffer, PoolDesc};
pub use random::CosmeticRandom;
pub use registry::*;

use laser::LaserVisuals;
use particles::ParticleEffects;
use pickups::PickupEffects;
use projectiles::ProjectileVisuals;
use quarry_dust::QuarryDust;
use track_dust::TrackDust;
use tracks::TrackTrails;

use crate::presentation::view_settings::FEEDBACK;

/// The explosion flash (`presentation.ts` `flash`): a warm point light that game
/// presentation and the effects lab both draw from `EffectSystems::flash`.
pub const FLASH_COLOR: u32 = 0xffc178;
pub const FLASH_INTENSITY: f32 = 45.0;
pub const FLASH_DISTANCE: f32 = 20.0;
pub const FLASH_DECAY: f32 = 2.0;
const FLASH_HEIGHT: f32 = 3.0;
/// Default pickup color when an event carries none.
const WHITE: u32 = 0xffffff;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Flash {
    pub position: Vec3,
    pub intensity: f32,
}

/// Live effect counts, for tests and "Stats for nerds".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectStats {
    pub particles: u32,
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
    pub tracks: TrackTrails,
    pub track_dust: TrackDust,
    pub quarry_dust: QuarryDust,
    pub projectiles: ProjectileVisuals,
    pub laser: LaserVisuals,
    pub pickups: PickupEffects,
    pub flash: Flash,
}

impl EffectSystems {
    pub fn reset(&mut self) {
        self.particles.reset();
        self.tracks.reset();
        self.track_dust.reset();
        self.quarry_dust.reset();
        self.projectiles.reset();
        self.laser.reset();
        self.pickups.reset();
        self.flash = Flash::default();
    }

    /// The visual response to one simulation event (`presentation.ts` `event`).
    pub fn event(&mut self, event: &SimEvent) {
        use SimEventType as T;
        // Contact telemetry and notices have no visual response.
        if matches!(event.kind, T::DebrisImpact | T::Notice) {
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
                .add(event.x, event.z, event.color.unwrap_or(WHITE), event.id);
        }
        if self.particles.event(event, &mut self.random) {
            self.flash = Flash {
                position: Vec3::new(event.x as f32, FLASH_HEIGHT, event.z as f32),
                intensity: FLASH_INTENSITY,
            };
        }
    }

    /// Advance one rendered frame, in the order `presentation.ts` `render` used.
    pub fn update(&mut self, state: &RenderState, alpha: f64, dt: f64, time: f64) {
        self.pickups.update(state, alpha, dt);
        self.tracks.update(state, alpha);
        self.particles.explosions.pads.sync(state);
        self.track_dust.update(state, &mut self.random);
        self.quarry_dust.update(state, dt, &mut self.random);
        self.flash.intensity *= (-dt * FEEDBACK.flash_decay).exp() as f32;
        self.projectiles.update(&state.shots, time, alpha);
        self.laser.update(state, alpha, dt);
        self.particles.update(dt, time);
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
        visit(&mut self.laser.halo);
        visit(&mut self.laser.core);
        visit(&mut self.laser.mount);
        visit(&mut self.laser.lens);
        visit(&mut self.pickups.rings);
        visit(&mut self.pickups.glows);
        visit(&mut self.particles.records);
        visit(&mut self.particles.explosions.rings);
        visit(&mut self.particles.explosions.puffs);
    }

    pub fn stats(&mut self) -> EffectStats {
        let mut instances = 0;
        self.for_each_pool(|pool| instances += pool.len() as u32);
        let explosions = &self.particles.explosions;
        EffectStats {
            particles: self.particles.particles.len() as u32,
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
pub use browser::{Effects, FLASH_LIGHT};

#[cfg(target_arch = "wasm32")]
mod browser {
    use sloppy_core::sim::{RenderState, SimEvent};

    use super::{EffectStats, EffectSystems, FLASH_COLOR, FLASH_DECAY, FLASH_DISTANCE};
    use crate::gpu::{Lifetime, PointLight, PoolId, Renderer};

    /// The point-light slot the explosion flash uses.
    pub const FLASH_LIGHT: usize = 0;
    /// Below this the light is switched off rather than shading every pixel.
    const FLASH_CUTOFF: f32 = 0.01;

    /// The effect systems bound to renderer pools (created once, kept across rounds).
    pub struct Effects {
        systems: EffectSystems,
        pools: Vec<PoolId>,
    }

    impl Effects {
        pub fn new(renderer: &mut Renderer) -> Self {
            let pools = super::looks::pool_descs()
                .iter()
                .map(|desc| renderer.add_pool(desc, Lifetime::Shared))
                .collect();
            Self {
                systems: EffectSystems::default(),
                pools,
            }
        }

        pub fn reset(&mut self, renderer: &mut Renderer, state: &RenderState) {
            self.systems.reset();
            // Quarry wind dust is shown only on the quarry; update decides that.
            self.systems.quarry_dust.visible = state.map_theme == "quarry";
            self.sync(renderer);
        }

        /// The visual response to one simulation event.
        pub fn event(&mut self, event: &SimEvent) {
            self.systems.event(event);
        }

        /// Once per rendered frame after entity poses are updated; `alpha`
        /// interpolates physics poses.
        pub fn update(
            &mut self,
            renderer: &mut Renderer,
            state: &RenderState,
            alpha: f32,
            dt: f32,
            time: f64,
        ) {
            self.systems
                .update(state, f64::from(alpha), f64::from(dt), time);
            self.sync(renderer);
        }

        /// Register any pool the renderer no longer has. Pools exist from `new`,
        /// so their pipelines are already part of `prepare_step`/`warm_up` and
        /// the first blast, mark or shell never compiles mid-round.
        pub fn warm_up_samples(&mut self, renderer: &mut Renderer) {
            let descs = super::looks::pool_descs();
            for (id, desc) in self.pools.iter_mut().zip(&descs) {
                if !renderer.has_pool(*id) {
                    *id = renderer.add_pool(desc, Lifetime::Shared);
                }
            }
        }

        pub fn stats(&mut self) -> EffectStats {
            self.systems.stats()
        }

        /// The CPU systems, for inspection (lab pages, tests).
        pub fn systems(&self) -> &EffectSystems {
            &self.systems
        }

        /// Restart the cosmetic random stream (repeatable lab screenshots).
        pub fn set_seed(&mut self, seed: u64) {
            self.systems.random = super::CosmeticRandom::seeded(seed);
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
            let flash = systems.flash;
            renderer.set_point_light(
                FLASH_LIGHT,
                (flash.intensity > FLASH_CUTOFF).then_some(PointLight {
                    position: flash.position,
                    color: FLASH_COLOR,
                    intensity: flash.intensity,
                    distance: FLASH_DISTANCE,
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
        assert_eq!(systems.flash.intensity, 0.0);
        let mut blast = SimEvent::at(SimEventType::Explosion, 5.0, -4.0);
        blast.size = Some(4.0);
        systems.event(&blast);
        assert_eq!(systems.flash.intensity, FLASH_INTENSITY);
        assert_eq!(systems.flash.position, Vec3::new(5.0, 3.0, -4.0));
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
            systems.update(&state, 1.0, dt, state.elapsed);
            let stats = systems.stats();
            peak.instances = peak.instances.max(stats.instances);
            peak.particles = peak.particles.max(stats.particles);
            peak.track_marks = peak.track_marks.max(stats.track_marks);
        }
        let stats = systems.stats();
        assert!(peak.particles as usize <= particles::MAX_PARTICLES);
        assert!(peak.particles > 1000, "the particle pool saturates");
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
