//! Small HUD calculations shared by the world-space bars and the page HUD:
//! `health-bar.ts`, the protection meters of `tank-bars.ts`, and the spawn cue.

use sloppy_core::sim::Team;
use sloppy_core::sim::data::{SHIELD_CAPACITY, TEAM_COLORS};
use sloppy_core::sim::simulation_rules::SIMULATION_RULES;

use super::view_settings::FEEDBACK;

/// Which of the bar's three fill colors shows; declared in the order of the
/// bar's fill joints (`joint::BAR_FILLS`), which the bar indexes by it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HealthColor {
    Team,
    Warning,
    Critical,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HealthBar {
    pub ratio: f64,
    pub tone: HealthColor,
    /// sRGB hex, for the page HUD.
    pub color: u32,
}

/// V-Tanks' fill thresholds, shared by the overhead bar and the player HUD.
pub fn health_bar_state(hp: f64, maximum: f64, team: Team) -> HealthBar {
    let ratio = (hp / maximum.max(1.0)).clamp(0.0, 1.0);
    let (tone, color) = if ratio <= 0.3 {
        (HealthColor::Critical, 0xff7c73)
    } else if ratio <= 0.6 {
        (HealthColor::Warning, 0xffe27a)
    } else {
        (HealthColor::Team, TEAM_COLORS[team.index()])
    };
    HealthBar { ratio, tone, color }
}

/// The shield and spawn-protection meters over a tank (`updateTankProtection`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProtectionMeters {
    pub shield_visible: bool,
    pub shield_fill: f64,
    pub spawn_visible: bool,
    pub spawn_fill: f64,
    pub spawn_y: f64,
}

/// Meter heights on the bar: the shield meter always sits low (the bar model
/// places it there), and the spawn meter rises above a shown shield.
pub const METER_LOW_Y: f64 = 0.38;
const METER_HIGH_Y: f64 = 0.72;

pub fn protection_meters(
    alive: bool,
    shield: f64,
    shield_points: f64,
    protection: f64,
) -> ProtectionMeters {
    let shield_visible = alive && shield > 0.0 && shield_points > 0.0;
    ProtectionMeters {
        shield_visible,
        shield_fill: (shield_points / SHIELD_CAPACITY).clamp(0.0, 1.0),
        spawn_visible: alive && protection > 0.0,
        spawn_fill: (protection / SIMULATION_RULES.spawn_protection_seconds).clamp(0.0, 1.0),
        spawn_y: if shield_visible {
            METER_HIGH_Y
        } else {
            METER_LOW_Y
        },
    }
}

/// The spawn pulse's scale and opacity with `cue` seconds of the cue left.
pub fn spawn_pulse(cue: f64) -> (f64, f64) {
    let phase = (FEEDBACK.spawn_cue_seconds - cue) % FEEDBACK.spawn_pulse_seconds;
    (
        1.0 + phase * 2.0,
        cue.min(1.0) * (1.0 - phase / FEEDBACK.spawn_pulse_seconds),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_colors_follow_the_thresholds() {
        assert_eq!(
            health_bar_state(100.0, 100.0, Team::Red).tone,
            HealthColor::Team
        );
        assert_eq!(
            health_bar_state(60.0, 100.0, Team::Red).tone,
            HealthColor::Warning
        );
        assert_eq!(health_bar_state(30.0, 100.0, Team::Red).color, 0xff7c73);
        assert_eq!(health_bar_state(-5.0, 100.0, Team::Blue).ratio, 0.0);
        let meters = protection_meters(true, 5.0, 60.0, 1.0);
        assert!(meters.shield_visible && meters.spawn_visible);
        assert_eq!(meters.spawn_y, 0.72);
        assert_eq!(spawn_pulse(2.5), (1.0, 1.0));
    }
}
