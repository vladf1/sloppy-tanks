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
    default_zoom: 31.0,
    min_zoom: 17.0,
    max_zoom: 52.0,
    max_pixel_ratio: 1.5,
};

/// How the follow camera carries weight and reacts to the battle. Shake follows
/// the trauma model: events add trauma (0..1), which decays linearly, and the
/// shake grows with its square, so small knocks stay calm and big blasts land.
pub struct CameraFeel {
    /// Angular frequency (rad/s) of the critically damped focus spring; the focus
    /// leads a moving tank by the spring's lag, so steady driving stays centred.
    pub follow_frequency: f64,
    /// The view leans toward the turret's aim by this fraction of the pointer's
    /// reach from the tank, up to `max_aim_look_ahead` of the zoom.
    pub aim_look_ahead: f64,
    pub max_aim_look_ahead: f64,
    /// And toward the nearest enemy within `threat_range` by this fraction of the
    /// way, up to `max_threat_look_ahead` of the zoom.
    pub threat_look_ahead: f64,
    pub max_threat_look_ahead: f64,
    pub threat_range: f64,
    /// The focus sits this fraction of the zoom up the screen from the tank.
    pub frame_ahead: f64,
    /// A big blast close by draws the view this fraction of the way toward it, up
    /// to `max_blast_look_ahead` of the zoom, letting go over `blast_look_seconds`.
    pub blast_look_ahead: f64,
    pub max_blast_look_ahead: f64,
    pub blast_look_seconds: f64,
    /// All of the leads together stay within this fraction of the zoom.
    pub max_look_ahead: f64,
    /// And ahead along the drive by this many seconds of travel, up to a cap.
    pub travel_look_ahead_seconds: f64,
    pub max_travel_look_ahead: f64,
    /// A focus this far from its target (a respawn, leaving the overview) snaps.
    pub snap_distance: f64,
    /// The leads ride their own critically damped spring of this frequency (rad/s).
    pub lead_frequency: f64,
    /// The follow camera frames at `calm_framing` of the chosen zoom. With an
    /// enemy near it frames the duel instead: as tight as `combat_framing` and as
    /// wide as `duel_framing`, whatever keeps that enemy inside `duel_frame`. It
    /// eases there at `framing_rate` per second.
    pub calm_framing: f64,
    pub combat_framing: f64,
    pub duel_framing: f64,
    pub framing_rate: f64,
    /// Screen boxes in NDC (x left, x right, y bottom, y top). The leads never
    /// carry the player's tank out of `player_frame`, clear of the HUD panels,
    /// and a lagging spring never past `player_guard`. The duel framing fits
    /// the enemy inside `duel_frame`.
    pub player_frame: [f32; 4],
    pub player_guard: [f32; 4],
    pub duel_frame: [f32; 4],
    /// A blast or kill still holding more than this share of its pull on the
    /// view is framed like an enemy.
    pub framed_blast_weight: f64,
    /// At `full_speed` (m/s) the camera eases back by this fraction of the zoom,
    /// approaching it at `speed_zoom_rate` per second.
    pub speed_zoom: f64,
    pub full_speed: f64,
    pub speed_zoom_rate: f64,
    /// Trauma lost per second.
    pub trauma_decay: f64,
    /// Shake at full trauma: yaw, pitch and roll (rad) about the view, and its
    /// noise frequency (Hz).
    pub shake_angles: [f64; 3],
    pub shake_frequency: f64,
    /// Kicks push the camera on a spring of this frequency (rad/s) and damping ratio.
    pub kick_frequency: f64,
    pub kick_damping: f64,
    pub max_kick_offset: f64,
    /// Knocks also tip the horizon on a spring of this frequency (rad/s) and
    /// damping ratio, within `max_roll` (rad): the viewer's shot, a kill, a hit at
    /// full strength and a blast close by, as angular speeds (rad/s).
    pub roll_frequency: f64,
    pub roll_damping: f64,
    pub max_roll: f64,
    pub fire_roll: f64,
    pub kill_roll: f64,
    pub hurt_roll: f64,
    pub blast_roll: f64,
    /// The viewer's own shot: trauma and backward kick speed (m/s); rockets scale both.
    pub fire_trauma: f64,
    pub fire_kick: f64,
    /// The viewer hurt: trauma per hit point, its cap, and the kick speed.
    pub hurt_trauma_per_damage: f64,
    pub max_hurt_trauma: f64,
    pub hurt_kick: f64,
    /// The viewer destroyed, or one of its shots finishing an enemy.
    pub death_trauma: f64,
    pub kill_trauma: f64,
    /// A blast's trauma per metre of radius, fading to nothing this far away
    /// (squared falloff), and its kick speed down and away at the blast.
    pub blast_trauma_per_radius: f64,
    pub blast_reach: f64,
    pub blast_kick: f64,
    /// Destroying an enemy dollies the drawn camera this fraction of the way to
    /// its focus, arriving at `punch_attack` and easing out at `punch_release`
    /// (per second), so a kill lands with a beat of emphasis.
    pub kill_punch_in: f64,
    pub punch_attack: f64,
    pub punch_release: f64,
    /// Any tank destroyed nearby pulls the camera in and knocks the horizon by up
    /// to this share of a kill's, fading over `blast_reach`.
    pub nearby_death_punch: f64,
    /// A big blast close by pulls it in by up to this share.
    pub blast_punch: f64,
    /// The viewer's own shot narrows the lens by this many degrees for a blink.
    pub fire_fov_kick: f64,
    pub fov_kick_release: f64,
    /// A reticle that follows the turret rather than a pointer (autoplay) glides to
    /// its point at this rate (per second); a jump farther than `snap_distance` snaps.
    pub aim_track_rate: f64,
    /// First person shakes less: the view is already close to the action.
    pub first_person_shake: f64,
    /// Share of the hull's pitch and roll the first-person view rides.
    pub first_person_tilt: f64,
    /// Degrees the first-person view widens at full speed.
    pub first_person_speed_fov: f64,
    /// The seated view sways with the treads: one side-to-side cycle per this many
    /// metres driven, and its bob (m), roll (rad) and engine shudder (m) at full
    /// speed.
    pub first_person_bob_metres: f64,
    pub first_person_bob: f64,
    pub first_person_sway_roll: f64,
    pub first_person_shudder: f64,
    /// Share of that shudder the idling engine keeps.
    pub first_person_idle_rumble: f64,
}

pub const CAMERA_FEEL: CameraFeel = CameraFeel {
    follow_frequency: 7.5,
    aim_look_ahead: 0.32,
    max_aim_look_ahead: 0.2,
    threat_look_ahead: 0.6,
    max_threat_look_ahead: 0.45,
    threat_range: 30.0,
    frame_ahead: 0.02,
    blast_look_ahead: 0.35,
    max_blast_look_ahead: 0.1,
    blast_look_seconds: 1.4,
    max_look_ahead: 0.45,
    travel_look_ahead_seconds: 0.14,
    max_travel_look_ahead: 2.0,
    snap_distance: 14.0,
    lead_frequency: 3.2,
    calm_framing: 0.88,
    combat_framing: 0.72,
    duel_framing: 0.96,
    framing_rate: 1.6,
    player_frame: [-0.14, 0.14, -0.2, 0.22],
    player_guard: [-0.38, 0.38, -0.38, 0.3],
    duel_frame: [-0.9, 0.88, -0.64, 0.76],
    framed_blast_weight: 0.3,
    speed_zoom: 0.06,
    full_speed: 8.0,
    speed_zoom_rate: 2.5,
    trauma_decay: 2.0,
    shake_angles: [0.007, 0.007, 0.006],
    shake_frequency: 9.0,
    kick_frequency: 15.0,
    kick_damping: 0.55,
    max_kick_offset: 0.35,
    roll_frequency: 10.0,
    roll_damping: 0.38,
    max_roll: 0.04,
    fire_roll: 0.06,
    kill_roll: 0.45,
    hurt_roll: 0.16,
    blast_roll: 0.1,
    fire_trauma: 0.08,
    fire_kick: 1.8,
    hurt_trauma_per_damage: 0.008,
    max_hurt_trauma: 0.45,
    hurt_kick: 3.5,
    death_trauma: 0.6,
    kill_trauma: 0.32,
    blast_trauma_per_radius: 0.1,
    blast_reach: 20.0,
    blast_kick: 3.0,
    kill_punch_in: 0.1,
    punch_attack: 16.0,
    punch_release: 2.4,
    nearby_death_punch: 0.55,
    blast_punch: 0.2,
    fire_fov_kick: 1.0,
    fov_kick_release: 9.0,
    aim_track_rate: 10.0,
    first_person_shake: 0.55,
    first_person_tilt: 0.65,
    first_person_speed_fov: 6.0,
    first_person_bob_metres: 3.2,
    first_person_bob: 0.045,
    first_person_sway_roll: 0.016,
    first_person_shudder: 0.008,
    first_person_idle_rumble: 0.3,
};

pub struct Feedback {
    pub hit_confirmation_seconds: f64,
    pub spawn_cue_seconds: f64,
    pub spawn_pulse_seconds: f64,
    pub pickup_seconds: f64,
    pub max_pickup_effects: usize,
    pub flash_decay: f64,
    /// Destroying an enemy holds the action for this long at `hit_stop_time_scale`
    /// (single player only: a room's clock belongs to the server).
    pub hit_stop_seconds: f64,
    pub hit_stop_time_scale: f64,
    /// The viewer's hits on an enemy hold it for this long.
    pub hit_confirm_stop_seconds: f64,
}

pub const FEEDBACK: Feedback = Feedback {
    hit_confirmation_seconds: 0.16,
    spawn_cue_seconds: 2.5,
    spawn_pulse_seconds: 1.25,
    pickup_seconds: 0.8,
    max_pickup_effects: 24,
    flash_decay: 12.0,
    hit_stop_seconds: 0.07,
    hit_stop_time_scale: 0.06,
    hit_confirm_stop_seconds: 0.035,
};

/// How tanks carry their weight on screen (render-only; physics keeps the true pose).
pub struct TankFeel {
    /// Barrel travel at full recoil (model units) and its muzzle climb (rad).
    pub recoil_travel: f64,
    pub recoil_climb: f64,
    /// The hull rocks back from its own shot (rad/s of pitch and roll), harder for
    /// a rocket.
    pub fire_rock: f64,
    pub rocket_rock: f64,
    /// The shot also shoves the tank back (m/s at `fire_rock`, scaled with the rock).
    pub fire_shove: f64,
    /// A hit throws the hull's top away from the shot (rad/s per hit point, capped)
    /// and shoves the whole tank (m/s per hit point, capped).
    pub hit_rock_per_damage: f64,
    pub max_hit_rock: f64,
    pub hit_shove_per_damage: f64,
    pub max_hit_shove: f64,
    /// A blast rocks tanks within `blast_reach_per_radius` of its radius plus
    /// `blast_reach` metres, by up to `blast_rock` rad/s and `blast_shove` m/s,
    /// fading with the square of the distance.
    pub blast_reach_per_radius: f64,
    pub blast_reach: f64,
    pub blast_rock: f64,
    pub blast_shove: f64,
    /// The shove's spring frequency (rad/s) and damping ratio.
    pub shove_frequency: f64,
    pub shove_damping: f64,
    /// A hit flashes the tank: peak brightness added per hit point (capped), the
    /// flash's per-second decay, and the warm color it fades through.
    pub flash_per_damage: f64,
    pub max_flash: f64,
    pub flash_decay: f64,
    pub flash_color: [f64; 3],
    /// The drawn gun slams back over `recoil_slam` seconds, holds for
    /// `recoil_hold` and runs out again over `recoil_return`, however quickly the
    /// simulation's own recoil recovers.
    pub recoil_slam: f64,
    pub recoil_hold: f64,
    pub recoil_return: f64,
    /// Top-run tread scroll (model units per metre driven) and its wrap length
    /// (the top run's link pitch).
    pub track_scroll: f64,
    pub track_period: f64,
    /// The engine shudders the hull by this many radians idling and at full
    /// speed (`CAMERA_FEEL.full_speed`), about this many times a second.
    pub idle_shudder: f64,
    pub drive_shudder: f64,
    pub shudder_frequency: f64,
}

pub const TANK_FEEL: TankFeel = TankFeel {
    recoil_travel: 0.5,
    recoil_climb: 0.09,
    fire_rock: 1.6,
    rocket_rock: 2.1,
    fire_shove: 2.0,
    hit_rock_per_damage: 0.05,
    max_hit_rock: 2.2,
    hit_shove_per_damage: 0.09,
    max_hit_shove: 4.0,
    blast_reach_per_radius: 2.0,
    blast_reach: 4.0,
    blast_rock: 1.8,
    blast_shove: 1.6,
    shove_frequency: 16.0,
    shove_damping: 0.5,
    flash_per_damage: 0.03,
    max_flash: 1.0,
    flash_decay: 13.0,
    flash_color: [2.2, 2.0, 1.8],
    recoil_slam: 0.035,
    recoil_hold: 0.06,
    recoil_return: 0.42,
    track_scroll: 0.5,
    track_period: 0.25,
    idle_shudder: 0.0035,
    drive_shudder: 0.009,
    shudder_frequency: 9.0,
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
    /// The eye sits this far left of the gun's axis (so the barrel runs in from the
    /// lower right, clear of the weapon panel), this much below and ahead of the
    /// vehicle's eye (model units).
    pub eye_side: f64,
    pub eye_drop: f64,
    pub eye_ahead: f64,
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
    field_of_view: 52.0,
    eye_side: 0.3,
    eye_drop: 0.05,
    eye_ahead: -0.1,
    pitch: -0.1,
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
/// Its size at the default zoom; it scales with the drawn zoom to keep its size
/// on screen, a little smaller than the target tank so the hit stays visible.
pub const OVERHEAD_RETICLE_SCALE: f64 = 0.9;
/// The overhead aiming plane (`presentation.ts` `groundPlane`).
pub const AIM_PLANE_HEIGHT: f64 = 1.0;
/// Tank bars float above the hull; the player's sits higher.
pub const BAR_HEIGHT: f64 = 2.15;
pub const PLAYER_BAR_HEIGHT: f64 = 2.85;
