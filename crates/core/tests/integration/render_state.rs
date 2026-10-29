//! Local presentation views follow the live simulation and reuse their arrays between frames
//! (the former `tests/render-state.test.ts`).
//!
//! The TS view kept live getters over the entities; the Rust view is refilled in place with
//! `fill_render_state`, so "live" means a refill reflects the moved body without
//! reallocating.

use sloppy_core::sim::physics::vector;
use sloppy_core::sim::{RenderState, Simulation};

fn human_body_position(simulation: &Simulation) -> sloppy_core::sim::math::Point3 {
    simulation.body_translation(simulation.human().body)
}

#[test]
fn local_presentation_views_stay_live_and_reuse_arrays() {
    let mut simulation = Simulation::with_seed(4242.0);
    let mut view = RenderState::default();
    simulation.fill_render_state(&mut view, None);
    let tanks = view.tanks.as_ptr();
    let viewer_id = view.viewer_id;
    let before = view
        .viewer()
        .expect("the human is the default viewer")
        .position;
    let body = simulation.human().body;
    simulation.world.bodies[body].set_translation(vector(before.x + 1.0, before.y, before.z), true);
    simulation.fill_render_state(&mut view, None);
    assert_eq!(view.tanks.as_ptr(), tanks, "refills reuse the tank array");
    assert_eq!(view.viewer_id, viewer_id);
    let viewer = view.viewer().expect("viewer");
    assert_eq!(viewer.position.x, before.x + 1.0);
    assert_eq!(viewer.position, human_body_position(&simulation));

    simulation.reset(None);
    simulation.fill_render_state(&mut view, None);
    assert_eq!(
        view.tanks.as_ptr(),
        tanks,
        "a reset refills the same tank array"
    );
    // The TS view replaced its cached entity objects on reset; here the refilled viewer
    // must describe the new round's tank rather than the moved one.
    let viewer = view.viewer().expect("viewer");
    assert_eq!(viewer.position, human_body_position(&simulation));
    assert_ne!(viewer.position.x, before.x + 1.0);
}
