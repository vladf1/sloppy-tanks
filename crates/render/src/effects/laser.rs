//! Laser defense (`laser-visuals.ts`): short beam flashes from a tank's lens to
//! the shell it shot down, and the lens and mount above each equipped tank.

use std::collections::VecDeque;

use glam::{Mat4, Quat, Vec3};
use sloppy_core::sim::render_state::RenderTank;
use sloppy_core::sim::tank_dimensions::tank_muzzle;
use sloppy_core::sim::{MatchPhase, RenderState, SimEvent, SimEventType};

use super::pool::{PoolBuffer, record};

pub const LASER_CAPACITY: usize = 64;
pub const FLASH_SECONDS: f64 = 0.12;
/// The lens sits this far above the muzzle line; the mount just below it.
const LENS_RISE: f64 = 0.3;
const MOUNT_DROP: f32 = 0.075;
/// Tanks rest 0.4 m above their body's origin height.
const HULL_OFFSET: f64 = 0.4;
/// A beam hitting nothing in particular strikes at this height.
const DEFAULT_TARGET_HEIGHT: f64 = 1.0;

#[derive(Clone, Copy, Debug)]
struct Beam {
    /// The firing tank; while it is alive the beam leaves its lens as drawn this frame.
    tank: Option<u32>,
    /// Where the beam started in the simulation, for a tank that is gone.
    from: Vec3,
    to: Vec3,
    life: f64,
}

#[derive(Clone, Debug)]
pub struct LaserVisuals {
    beams: VecDeque<Beam>,
    pub halo: PoolBuffer,
    pub core: PoolBuffer,
    pub mount: PoolBuffer,
    pub lens: PoolBuffer,
}

impl Default for LaserVisuals {
    fn default() -> Self {
        Self {
            beams: VecDeque::with_capacity(LASER_CAPACITY),
            halo: PoolBuffer::new(LASER_CAPACITY),
            core: PoolBuffer::new(LASER_CAPACITY),
            mount: PoolBuffer::new(LASER_CAPACITY),
            lens: PoolBuffer::new(LASER_CAPACITY),
        }
    }
}

impl LaserVisuals {
    pub fn reset(&mut self) {
        self.beams.clear();
        self.clear_layers();
    }

    fn clear_layers(&mut self) {
        for layer in [
            &mut self.halo,
            &mut self.core,
            &mut self.mount,
            &mut self.lens,
        ] {
            layer.clear();
        }
    }

    pub fn beams(&self) -> usize {
        self.beams.len()
    }

    pub fn event(&mut self, event: &SimEvent) {
        let (SimEventType::Laser, Some(from)) = (event.kind, event.from) else {
            return;
        };
        if self.beams.len() == LASER_CAPACITY {
            self.beams.pop_front();
        }
        self.beams.push_back(Beam {
            tank: event.id,
            from: Vec3::new(from.x as f32, from.y as f32, from.z as f32),
            to: Vec3::new(
                event.x as f32,
                event.height.unwrap_or(DEFAULT_TARGET_HEIGHT) as f32,
                event.z as f32,
            ),
            life: FLASH_SECONDS,
        });
    }

    pub fn update(&mut self, state: &RenderState, alpha: f64, dt: f64) {
        self.clear_layers();
        let playing = state.match_state.phase == MatchPhase::Playing;
        self.beams.retain_mut(|beam| {
            if playing {
                beam.life -= dt;
            }
            beam.life > 0.0
        });
        for beam in &self.beams {
            let from = beam
                .tank
                .and_then(|id| state.tanks.iter().find(|tank| tank.id == id && tank.alive))
                .map_or(beam.from, |tank| lens_position(tank, alpha));
            let direction = beam.to - from;
            let length = direction.length();
            if length < 1e-6 {
                continue;
            }
            let thickness = (beam.life / FLASH_SECONDS) as f32;
            let world = Mat4::from_scale_rotation_translation(
                Vec3::new(thickness, length, thickness),
                Quat::from_rotation_arc(Vec3::Y, direction / length),
                (from + beam.to) * 0.5,
            );
            self.halo.push(record(world, [1.0; 4], [0.0; 4]));
            self.core.push(record(world, [1.0; 4], [0.0; 4]));
        }
        for tank in &state.tanks {
            if !tank.alive || tank.laser <= 0.0 || self.lens.is_full() {
                continue;
            }
            let lens = lens_position(tank, alpha);
            self.lens
                .push(record(Mat4::from_translation(lens), [1.0; 4], [0.0; 4]));
            let mount = lens - Vec3::Y * MOUNT_DROP;
            self.mount
                .push(record(Mat4::from_translation(mount), [1.0; 4], [0.0; 4]));
        }
    }
}

/// The lens above a tank at its interpolated pose.
fn lens_position(tank: &RenderTank, alpha: f64) -> Vec3 {
    let position = tank.position;
    Vec3::new(
        (tank.previous.x + (position.x - tank.previous.x) * alpha) as f32,
        (position.y - HULL_OFFSET + tank_muzzle(tank.kind).y + LENS_RISE) as f32,
        (tank.previous.z + (position.z - tank.previous.z) * alpha) as f32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::sim::Point3;
    use sloppy_core::sim::math::Vec2;

    fn laser(from: Point3, x: f64, z: f64) -> SimEvent {
        SimEvent {
            from: Some(from),
            height: Some(1.2),
            ..SimEvent::at(SimEventType::Laser, x, z)
        }
    }

    #[test]
    fn beams_flash_briefly_stay_bounded_and_equipped_tanks_show_a_lens() {
        let mut state = RenderState::default();
        state.match_state.phase = MatchPhase::Playing;
        state.tanks.push(RenderTank {
            id: 1,
            alive: true,
            laser: 5.0,
            ..RenderTank::default()
        });
        let mut visuals = LaserVisuals::default();
        let origin = Point3 {
            x: 0.0,
            y: 1.5,
            z: 0.0,
        };
        for i in 0..100 {
            visuals.event(&laser(origin, 4.0, i as f64));
        }
        visuals.event(&SimEvent::at(SimEventType::Laser, 0.0, 0.0));
        assert_eq!(visuals.beams(), LASER_CAPACITY);
        visuals.update(&state, 1.0, 0.01);
        assert_eq!(visuals.halo.len(), LASER_CAPACITY);
        assert_eq!(visuals.core.len(), LASER_CAPACITY);
        assert_eq!(visuals.lens.len(), 1);
        assert_eq!(visuals.mount.len(), 1);
        // The beam spans from the lens to the target.
        let world = visuals.core.records()[0].world();
        let top = world.transform_point3(Vec3::new(0.0, 0.5, 0.0));
        let bottom = world.transform_point3(Vec3::new(0.0, -0.5, 0.0));
        let ends = [top, bottom];
        assert!(
            ends.iter()
                .any(|p| p.distance(Vec3::new(0.0, 1.5, 0.0)) < 1e-4)
        );
        assert!(
            ends.iter()
                .any(|p| p.distance(Vec3::new(4.0, 1.2, 36.0)) < 1e-3)
        );
        state.match_state.phase = MatchPhase::Paused;
        visuals.update(&state, 1.0, 1.0);
        assert_eq!(visuals.beams(), LASER_CAPACITY, "beams freeze while paused");
        state.match_state.phase = MatchPhase::Playing;
        visuals.update(&state, 1.0, FLASH_SECONDS);
        assert_eq!(visuals.halo.len(), 0);
        state.tanks[0].laser = 0.0;
        visuals.update(&state, 1.0, 0.01);
        assert!(visuals.lens.is_empty() && visuals.mount.is_empty());
    }

    #[test]
    fn a_beam_leaves_its_moving_tank_and_stays_on_the_zapped_shell() {
        let mut state = RenderState::default();
        state.match_state.phase = MatchPhase::Playing;
        state.tanks.push(RenderTank {
            id: 7,
            alive: true,
            laser: 5.0,
            ..RenderTank::default()
        });
        let mut visuals = LaserVisuals::default();
        let mut event = laser(Point3::new(0.0, 1.5, 0.0), 0.0, 7.0);
        event.id = Some(7);
        visuals.event(&event);
        // The tank drives 2 m sideways while the beam is still flashing.
        state.tanks[0].previous = Vec2::new(2.0, 0.0);
        state.tanks[0].position = Point3::new(2.0, 0.65, 0.0);
        visuals.update(&state, 1.0, 0.01);
        let world = visuals.core.records()[0].world();
        let ends = [
            world.transform_point3(Vec3::new(0.0, 0.5, 0.0)),
            world.transform_point3(Vec3::new(0.0, -0.5, 0.0)),
        ];
        let lens = lens_position(&state.tanks[0], 1.0);
        assert!(ends.iter().any(|p| p.distance(lens) < 1e-4), "{ends:?}");
        assert!(
            ends.iter()
                .any(|p| p.distance(Vec3::new(0.0, 1.2, 7.0)) < 1e-4)
        );
    }
}
