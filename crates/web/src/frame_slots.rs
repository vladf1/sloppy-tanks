//! The packed results of `Game.frame` and `NetGame.frame`, slot by slot. The page reads
//! them by index (`src/game/engine-api.ts`, `src/net/client.ts`), the desktop by name.
//! They build for every target, unlike the game types, so the page catalog test pins
//! the page's copies to them.

/// Slots of `Game.frame`'s result.
pub mod frame_slot {
    /// 0 ready, 1 playing, 2 paused, 3 results.
    pub const PHASE: usize = 0;
    pub const HUMAN_ALIVE: usize = 1;
    /// Show the first-person cockpit HUD (from the start of the flight in).
    pub const COCKPIT: usize = 2;
    /// The hull's clockwise screen angle with the turret straight up (compass).
    pub const HULL_ANGLE: usize = 3;
    /// Events waiting for `drain_events`.
    pub const EVENTS: usize = 4;
    /// 1 when the page should drop held input (paused, dead, results).
    pub const CLEAR_INPUT: usize = 5;
    /// 1 on frames the HUD should refresh (`hud_json`).
    pub const HUD_DUE: usize = 6;
    pub const SIM_MS: usize = 7;
    pub const RENDER_MS: usize = 8;
    /// 1 while first person is enabled (the page captures the pointer).
    pub const FIRST_PERSON: usize = 9;
    /// This frame's capped delta in seconds (the HUD advances its timers by it).
    pub const DT: usize = 10;
    // Slots 11 and 12 are the room page's own (`net_frame_slot`).
    /// The viewer's tank on screen (NDC, y up) for the damage ring; NaN in first
    /// person or off screen.
    pub const VIEWER_X: usize = 13;
    pub const VIEWER_Y: usize = 14;
    /// 1 when the frame ran; 0 when it was skipped while the GPU still ran earlier
    /// frames (`framesInFlight`), and the other slots repeat the last frame's.
    pub const RAN: usize = 15;
    pub const LENGTH: usize = 16;

    use sloppy_core::sim::MatchPhase;

    /// The match phase `PHASE` holds.
    pub fn phase(code: f32) -> MatchPhase {
        match code as u32 {
            1 => MatchPhase::Playing,
            2 => MatchPhase::Paused,
            3 => MatchPhase::Results,
            _ => MatchPhase::Ready,
        }
    }

    /// `PHASE`'s number for `phase`.
    pub fn phase_code(phase: MatchPhase) -> f32 {
        match phase {
            MatchPhase::Ready => 0.0,
            MatchPhase::Playing => 1.0,
            MatchPhase::Paused => 2.0,
            MatchPhase::Results => 3.0,
        }
    }
}

/// Slots of `NetGame.frame`'s result. Slots 2-4, 6-10 and 13-15 are `Game.frame`'s, except that
/// `SIM_MS` times interpolation, input and event routing, and `HUD_DUE` needs a drawn
/// frame and is also due while events wait. Slots 0, 1 and 5 stay 0.
pub mod net_frame_slot {
    pub use super::frame_slot::{
        COCKPIT, DT, EVENTS, FIRST_PERSON, HUD_DUE, HULL_ANGLE, RAN, RENDER_MS, SIM_MS, VIEWER_X,
        VIEWER_Y,
    };
    /// 1 when the arena drew this frame.
    pub const DRAWN: usize = 11;
    /// 1 when the page should free a captured pointer: a menu, a drop or no battle.
    /// A death keeps it captured for the respawn.
    pub const POINTER_FREE: usize = 12;
    pub const LENGTH: usize = 16;
}
