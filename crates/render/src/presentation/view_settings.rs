//! Port of `view-settings.ts`: camera distances are world metres, animation
//! durations are seconds.

use sloppy_core::sim::VehicleKind;

pub struct Camera {
    pub field_of_view: f64,
    pub near: f64,
    pub far: f64,
    pub default_zoom: f64,
    pub min_zoom: f64,
    pub max_zoom: f64,
    pub max_pixel_ratio: f64,
}

pub const CAMERA: Camera = Camera {
    field_of_view: 43.0,
    near: 0.1,
    far: 320.0,
    default_zoom: 34.0,
    min_zoom: 17.0,
    max_zoom: 52.0,
    max_pixel_ratio: 1.5,
};

pub struct Feedback {
    pub hit_confirmation_seconds: f64,
    pub recoil_seconds: f64,
    pub spawn_cue_seconds: f64,
    pub spawn_pulse_seconds: f64,
    pub pickup_seconds: f64,
    pub max_pickup_effects: usize,
    pub flash_decay: f64,
}

pub const FEEDBACK: Feedback = Feedback {
    hit_confirmation_seconds: 0.16,
    recoil_seconds: 0.28,
    spawn_cue_seconds: 2.5,
    spawn_pulse_seconds: 1.25,
    pickup_seconds: 0.8,
    max_pickup_effects: 24,
    flash_decay: 12.0,
};

/// Where the first-person eye sits in turret-local model units (before vehicle
/// scale): behind the mantlet, raised like a commander standing in the hatch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Eye {
    pub height: f64,
    pub forward: f64,
}

/// The camera mounted on the player's turret.
pub struct FirstPerson {
    pub field_of_view: f64,
    /// A slight downward tilt shows the ground ahead without hiding the horizon.
    pub pitch: f64,
    pub mouse_radians_per_pixel: f64,
    pub touch_turn_radians_per_second: f64,
    /// The reticle floats this far from the eye, drawn over the scene; its scale
    /// keeps it about as large on screen as the overhead ground reticle.
    pub reticle_distance: f64,
    pub reticle_scale: f64,
    /// Metres along the shell lane where the reticle sits: a typical engagement.
    pub reticle_range: f64,
    /// Pickups hover at eye height; fading them keeps tanks behind them visible.
    pub pickup_opacity: f64,
    /// Switching views flies the camera between the overhead pose and the eye.
    pub transition_seconds: f64,
    /// The flight keeps the overhead gaze on the tank until this fraction, then
    /// swings to the turret's heading.
    pub transition_turn_start: f64,
    /// Destroyed in first person, the camera eases out of the wreck to this many
    /// metres behind (along the look) and above the last eye, tilted down to
    /// watch it, over `destroyed_step_seconds`.
    pub destroyed_back: f64,
    pub destroyed_rise: f64,
    pub destroyed_pitch: f64,
    pub destroyed_step_seconds: f64,
}

pub const FIRST_PERSON: FirstPerson = FirstPerson {
    field_of_view: 58.0,
    pitch: -0.07,
    mouse_radians_per_pixel: 0.0032,
    touch_turn_radians_per_second: 2.4,
    reticle_distance: 12.0,
    reticle_scale: 0.3,
    reticle_range: 25.0,
    pickup_opacity: 0.7,
    transition_seconds: 0.8,
    transition_turn_start: 0.4,
    destroyed_back: 4.5,
    destroyed_rise: 2.2,
    destroyed_pitch: -0.5,
    destroyed_step_seconds: 0.6,
};

pub const fn first_person_eye(kind: VehicleKind) -> Eye {
    match kind {
        VehicleKind::Scout => Eye {
            height: 2.22,
            forward: -0.55,
        },
        VehicleKind::Balanced => Eye {
            height: 2.32,
            forward: -0.75,
        },
        VehicleKind::Heavy => Eye {
            height: 2.36,
            forward: -0.7,
        },
        VehicleKind::Humvee => Eye {
            height: 2.45,
            forward: -0.45,
        },
    }
}

/// Overhead, the reticle lies just above the aiming plane (`reticle.ts`).
pub const RETICLE_HEIGHT: f64 = 1.05;
/// The overhead aiming plane (`presentation.ts` `groundPlane`).
pub const AIM_PLANE_HEIGHT: f64 = 1.0;
/// Tank bars float above the hull; the player's sits higher.
pub const BAR_HEIGHT: f64 = 2.15;
pub const PLAYER_BAR_HEIGHT: f64 = 2.85;
