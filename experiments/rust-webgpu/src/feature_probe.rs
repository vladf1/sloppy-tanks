//! Executed at browser startup so release linking must retain these game-used APIs.
//! A separate tiny world keeps the rendered demo and its controls unchanged.
use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::prelude::*;
use std::sync::Mutex;

#[derive(Default)]
struct ContactForces(Mutex<u32>);
impl EventHandler for ContactForces {
    fn handle_collision_event(
        &self,
        _: &RigidBodySet,
        _: &ColliderSet,
        _: CollisionEvent,
        _: Option<&ContactPair>,
    ) {
    }
    fn handle_contact_force_event(
        &self,
        _: Real,
        _: &RigidBodySet,
        _: &ColliderSet,
        _: &ContactPair,
        force: Real,
    ) {
        if force.is_finite() && force > 0.0 {
            *self.0.lock().unwrap() += 1;
        }
    }
    fn handle_soft_body_tear_event(&self, _: &SoftBodySet, _: &SoftBodyTearEvent) {}
}

pub const CHECKS: usize = 11;

pub fn run(scale: f32) -> [u32; CHECKS] {
    let scale = scale.clamp(0.5, 2.0);
    let mut world = PhysicsWorld::new();
    world.integration_parameters.dt = 1.0 / 60.0;
    let groups = InteractionGroups::new(Group::GROUP_1, Group::GROUP_1, InteractionTestMode::And);
    let excluded = InteractionGroups::new(Group::GROUP_2, Group::GROUP_2, InteractionTestMode::And);
    let floor = world.insert_collider(
        ColliderBuilder::trimesh(
            vec![
                Vector::new(-10.0, 0.0, -10.0),
                Vector::new(-10.0, 0.0, 10.0),
                Vector::new(10.0, 0.0, 10.0),
                Vector::new(10.0, 0.0, -10.0),
            ],
            vec![[0, 1, 2], [0, 2, 3]],
        )
        .unwrap()
        .collision_groups(groups),
        None,
    );
    let mut vertices = Vec::with_capacity(24);
    for i in 0..12 {
        let angle = i as f32 * std::f32::consts::TAU / 12.0;
        for y in [-0.5, 0.5] {
            vertices.push(Vector::new(
                angle.cos() * scale * 0.5,
                y,
                angle.sin() * scale * 0.5,
            ));
        }
    }
    let (drum, _) = world.insert(
        RigidBodyBuilder::dynamic()
            .translation(Vector::new(0.0, 2.0, 0.0))
            .ccd_enabled(true),
        ColliderBuilder::convex_hull(&vertices)
            .unwrap()
            .mass(0.45)
            .collision_groups(groups)
            .active_events(ActiveEvents::CONTACT_FORCE_EVENTS)
            .contact_force_event_threshold(1.0),
    );
    let (tank, hull) = world.insert(
        RigidBodyBuilder::dynamic()
            .translation(Vector::new(4.0, 0.6, 0.0))
            .enabled_rotations(false, true, false)
            .soft_ccd_prediction(0.5)
            .ccd_enabled(true),
        ColliderBuilder::cuboid(0.8, 0.5, 1.0)
            .mass(3.0)
            .collision_groups(groups),
    );
    let extra = world.insert_collider(
        ColliderBuilder::cuboid(0.8, 0.5, 1.0)
            .mass(0.0)
            .collision_groups(excluded),
        Some(tank),
    );
    let forces = ContactForces::default();
    for _ in 0..120 {
        world.step_with_events(&(), &forces);
    }
    let ray = Ray::new(Vector::new(-4.0, 4.0, 0.0), Vector::NEG_Y);
    let normal_hit =
        world.cast_ray_and_get_normal(&ray, 10.0, true, QueryFilter::default().groups(groups));
    let ray_hit = world.cast_ray(&ray, 10.0, true, QueryFilter::default().groups(groups));
    let excluded_hit = world.cast_ray(&ray, 10.0, true, QueryFilter::default().groups(excluded));
    let options = ShapeCastOptions {
        max_time_of_impact: 10.0,
        ..Default::default()
    };
    let ball_hit = world.cast_shape(
        &Pose::translation(-4.0, 4.0, 0.0),
        Vector::NEG_Y,
        &Ball::new(0.1),
        options,
        QueryFilter::default().groups(groups),
    );
    let box_shape = Cuboid::new(Vector::splat(0.3));
    let box_hit = world.cast_shape(
        &Pose::translation(-4.0, 4.0, 0.0),
        Vector::NEG_Y,
        &box_shape,
        options,
        QueryFilter::default().groups(groups),
    );
    let overlaps = world
        .query_pipeline_with_filter(QueryFilter::default().groups(groups))
        .intersect_shape(Pose::IDENTITY, &Ball::new(10.0))
        .count();
    let local_hit = box_shape.cast_ray(&Pose::translation(-4.0, 1.0, 0.0), &ray, 10.0, true);
    let mut checks = [
        u32::from(
            world.bodies[drum].translation().y > 0.3 && world.bodies[drum].translation().y < 0.8,
        ),
        u32::from(ray_hit.is_some_and(|(hit, _)| hit == floor)),
        u32::from(world.bodies[tank].colliders().len() == 2),
        u32::from(
            world.bodies[tank].soft_ccd_prediction() == 0.5 && world.bodies[tank].is_ccd_enabled(),
        ),
        u32::from(normal_hit.is_some_and(|(_, hit)| hit.normal.y > 0.9) && excluded_hit.is_none()),
        u32::from(ball_hit.is_some() && box_hit.is_some()),
        u32::from(overlaps >= 3),
        u32::from(local_hit.is_some()),
        0,
        0,
        *forces.0.lock().unwrap(),
    ];
    // Trees become cylinders; destruction changes collider offsets and groups.
    world.colliders[extra].set_shape(SharedShape::cylinder(0.8, 0.3));
    world.colliders[extra].set_translation_wrt_parent(Vector::new(0.0, 0.25, 0.0));
    world.colliders[extra].set_collision_groups(groups);
    checks[8] = u32::from(world.colliders[extra].shape().as_cylinder().is_some());
    let body = &mut world.bodies[tank];
    body.set_translation(Vector::new(4.0, 2.0, 0.0), true);
    body.set_rotation(Rotation::from_rotation_y(0.2), true);
    body.apply_impulse(Vector::new(1.0, 0.0, 0.0), true);
    body.apply_impulse_at_point(Vector::new(0.0, 0.0, 1.0), Vector::new(4.5, 2.0, 0.0), true);
    body.sleep();
    body.wake_up(true);
    body.apply_impulse(Vector::new(scale, 0.0, 0.0), true);
    checks[9] = u32::from(!body.is_sleeping() && body.linvel().x > 0.0 && body.mass() > 0.0);
    world.remove_collider(hull);
    world.remove_body(tank);
    checks
}

#[cfg(test)]
mod tests {
    #[test]
    fn game_used_rapier_apis_execute() {
        for scale in [0.5, 1.0, 2.0] {
            let checks = super::run(scale);
            assert!(
                checks.iter().all(|&n| n > 0),
                "failed probe at scale {scale}: {checks:?}"
            );
        }
    }
}
