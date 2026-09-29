use glam::{Mat4, Quat, Vec3};
use rapier3d::prelude::*;

pub const STEP: f64 = 1.0 / 60.0;
pub const MAX_SHOTS: usize = 16;
pub const INITIAL_BLOCKS: usize = 24;
pub const STATIC_BLOCKS: usize = 6;
const MAX_FRAME_SECONDS: f64 = 0.1;

pub struct Block {
    pub body: RigidBodyHandle,
    pub half: Vec3,
    pub color: [f32; 4],
    previous_position: Vec3,
    previous_rotation: Quat,
    pub projectile: bool,
}

pub struct Simulation {
    pub world: PhysicsWorld,
    pub blocks: Vec<Block>,
    pub ticks: u32,
    accumulator: f64,
}

impl Default for Simulation {
    fn default() -> Self {
        Self::new()
    }
}

impl Simulation {
    pub fn new() -> Self {
        let mut simulation = Self {
            world: PhysicsWorld::new(),
            blocks: Vec::with_capacity(48),
            ticks: 0,
            accumulator: 0.0,
        };
        simulation.world.integration_parameters.dt = STEP as f32;
        simulation.add(
            Vec3::new(0.0, -0.35, 0.0),
            Vec3::new(7.0, 0.35, 7.0),
            0.0,
            false,
            [0.22, 0.30, 0.34, 1.0],
        );
        for (position, half) in [
            (Vec3::new(-7.1, 0.25, 0.0), Vec3::new(0.1, 0.6, 7.2)),
            (Vec3::new(7.1, 0.25, 0.0), Vec3::new(0.1, 0.6, 7.2)),
            (Vec3::new(0.0, 0.25, -7.1), Vec3::new(7.0, 0.6, 0.1)),
            (Vec3::new(0.0, 0.25, 7.1), Vec3::new(7.0, 0.6, 0.1)),
        ] {
            simulation.add(position, half, 0.0, false, [0.09, 0.15, 0.18, 0.0]);
        }
        simulation.add(
            Vec3::new(-4.2, 0.6, 1.2),
            Vec3::new(1.15, 0.18, 2.2),
            -0.26,
            false,
            [0.49, 0.56, 0.52, 0.0],
        );
        for row in 0..4 {
            for column in 0..5 {
                let color = if (row + column) % 2 == 0 {
                    [0.12, 0.65, 0.57, 0.0]
                } else {
                    [0.72, 0.79, 0.68, 0.0]
                };
                simulation.add(
                    Vec3::new((column as f32 - 2.0) * 1.02, 0.5 + row as f32 * 1.02, -0.8),
                    Vec3::splat(0.49),
                    0.0,
                    true,
                    color,
                );
            }
        }
        for row in 0..4 {
            simulation.add(
                Vec3::new(4.5, 0.5 + row as f32 * 1.02, -3.5),
                Vec3::splat(0.49),
                0.0,
                true,
                [0.57, 0.70, 0.79, 0.0],
            );
        }
        simulation
    }

    fn add(&mut self, position: Vec3, half: Vec3, tilt: f32, dynamic: bool, color: [f32; 4]) {
        let builder = if dynamic {
            RigidBodyBuilder::dynamic()
        } else {
            RigidBodyBuilder::fixed()
        };
        let (body, _) = self.world.insert(
            builder
                .translation(Vector::new(position.x, position.y, position.z))
                .rotation(Vector::new(tilt, 0.0, 0.0))
                .linear_damping(0.08)
                .angular_damping(0.15)
                .ccd_enabled(dynamic),
            ColliderBuilder::cuboid(half.x, half.y, half.z)
                .friction(0.7)
                .restitution(0.12),
        );
        self.blocks.push(Block {
            body,
            half,
            color,
            previous_position: position,
            previous_rotation: Quat::from_rotation_x(tilt),
            projectile: false,
        });
    }

    pub fn launch(&mut self) {
        if self.blocks.iter().filter(|b| b.projectile).count() == MAX_SHOTS {
            let index = self.blocks.iter().position(|b| b.projectile).unwrap();
            let block = self.blocks.remove(index);
            self.world.remove_body(block.body);
        }
        self.add(
            Vec3::new(0.0, 1.5, 5.8),
            Vec3::splat(0.55),
            0.15,
            true,
            [1.0, 0.32, 0.09, 0.0],
        );
        let block = self.blocks.last_mut().unwrap();
        block.projectile = true;
        let body = &mut self.world.bodies[block.body];
        body.set_linvel(Vector::new(0.0, 2.0, -17.0), true);
        body.set_angvel(Vector::new(2.0, 1.0, 0.5), true);
    }

    pub fn advance(&mut self, seconds: f64) {
        if !seconds.is_finite() || seconds < 0.0 {
            return;
        }
        self.accumulator += seconds.min(MAX_FRAME_SECONDS);
        while self.accumulator + 1e-12 >= STEP {
            self.step();
            self.accumulator = (self.accumulator - STEP).max(0.0);
        }
    }

    fn step(&mut self) {
        for block in &mut self.blocks {
            let body = &self.world.bodies[block.body];
            let p = body.translation();
            let q = body.rotation();
            block.previous_position = Vec3::new(p.x, p.y, p.z);
            block.previous_rotation = Quat::from_xyzw(q.x, q.y, q.z, q.w);
        }
        self.world.step();
        self.ticks += 1;
    }

    pub fn transform(&self, block: &Block) -> Mat4 {
        let body = &self.world.bodies[block.body];
        let p = body.translation();
        let q = body.rotation();
        let alpha = (self.accumulator / STEP) as f32;
        Mat4::from_scale_rotation_translation(
            block.half * 2.0,
            block
                .previous_rotation
                .slerp(Quat::from_xyzw(q.x, q.y, q.z, q.w), alpha),
            block
                .previous_position
                .lerp(Vec3::new(p.x, p.y, p.z), alpha),
        )
    }

    pub fn active_count(&self) -> usize {
        self.world
            .bodies
            .iter()
            .filter(|(_, b)| b.is_dynamic() && !b.is_sleeping())
            .count()
    }

    pub fn dynamic_count(&self) -> usize {
        self.blocks.len() - STATIC_BLOCKS
    }

    pub fn stack_displacement(&self) -> f32 {
        self.blocks
            .iter()
            .skip(STATIC_BLOCKS)
            .take(20)
            .map(|b| (self.world.bodies[b.body].translation().z + 0.8).abs())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_settle_on_platform_and_launch_moves_the_wall() {
        let mut simulation = Simulation::new();
        for _ in 0..360 {
            simulation.advance(STEP);
        }
        assert_eq!(simulation.dynamic_count(), INITIAL_BLOCKS);
        for block in simulation.blocks.iter().skip(STATIC_BLOCKS) {
            let p = simulation.world.bodies[block.body].translation();
            assert!(p.y.is_finite() && p.y > 0.35 && p.y < 4.1);
        }
        let before = simulation.stack_displacement();
        simulation.launch();
        for _ in 0..180 {
            simulation.advance(STEP);
        }
        assert!(simulation.stack_displacement() > before + 2.0);
    }

    #[test]
    fn refresh_rate_does_not_change_physics() {
        let mut slow = Simulation::new();
        let mut fast = Simulation::new();
        slow.launch();
        fast.launch();
        for _ in 0..120 {
            slow.advance(1.0 / 30.0);
        }
        for _ in 0..576 {
            fast.advance(1.0 / 144.0);
        }
        assert_eq!(slow.ticks, fast.ticks);
        for (a, b) in slow.blocks.iter().zip(&fast.blocks) {
            assert_eq!(
                slow.world.bodies[a.body].translation(),
                fast.world.bodies[b.body].translation()
            );
        }
    }

    #[test]
    fn repeated_launches_are_bounded_and_reset_rebuilds_world() {
        let mut simulation = Simulation::new();
        for _ in 0..80 {
            simulation.launch();
            simulation.advance(STEP);
        }
        assert_eq!(simulation.dynamic_count(), INITIAL_BLOCKS + MAX_SHOTS);
        assert_eq!(simulation.world.bodies.len(), simulation.blocks.len());
        assert_eq!(simulation.world.colliders.len(), simulation.blocks.len());
        simulation = Simulation::new();
        assert_eq!(simulation.dynamic_count(), INITIAL_BLOCKS);
        assert_eq!(simulation.ticks, 0);
    }
}
