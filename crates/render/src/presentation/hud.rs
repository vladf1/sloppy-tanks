//! Small HUD calculations shared by the world-space bars and the page HUD:
//! `health-bar.ts`, the protection meters of `tank-bars.ts`, and the spawn cue.

use sloppy_core::sim::data::SHIELD_CAPACITY;
use sloppy_core::sim::simulation_rules::SIMULATION_RULES;

use super::view_settings::FEEDBACK;

/// Which of the bar's three fill colors shows; declared in the order of the
/// bar's fill joints (`joint::BAR_FILLS`), which the bar indexes by it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HealthColor {
    Healthy,
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

/// The healthy, damaged and critical fills, the page HUD's hull bar colours
/// (`style.css`), so every bar reads one value one way: green, amber, red. A bar never
/// wears its team's colour, where a full red-team hull would read as critical; the
/// team shows in the pointer under the bar and in the tank's own paint.
pub const HEALTHY_COLOR: u32 = 0x5fe39a;
pub const WARNING_COLOR: u32 = 0xffc23a;
pub const CRITICAL_COLOR: u32 = 0xff4d5e;

/// V-Tanks' fill thresholds, shared by the overhead bar and the player HUD.
pub fn health_bar_state(hp: f64, maximum: f64) -> HealthBar {
    let ratio = (hp / maximum.max(1.0)).clamp(0.0, 1.0);
    let (tone, color) = if ratio <= 0.3 {
        (HealthColor::Critical, CRITICAL_COLOR)
    } else if ratio <= 0.6 {
        (HealthColor::Warning, WARNING_COLOR)
    } else {
        (HealthColor::Healthy, HEALTHY_COLOR)
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

/// The overhead bar's largest share of the view's height: the follow camera draws
/// it under this size, and a low or close camera (hero shots, the turret seat)
/// shrinks it in the world so it reads as a compact tag over the hull, never a slab
/// or a signboard across the sky.
pub const BAR_MAX_SCREEN_SHARE: f64 = 0.055;
/// A tank this close to the lens (the turret seat, a hero shot) gets a smaller share
/// still, easing up to the full share by `BAR_FAR_DISTANCE`: a near tank fills the view
/// by itself, and its tag should stay a tag.
const BAR_NEAR_SCREEN_SHARE: f64 = 0.035;
const BAR_NEAR_DISTANCE: f64 = 12.0;
const BAR_FAR_DISTANCE: f64 = 30.0;
/// It never shrinks below this, so a tank right under the lens keeps a readable bar.
const BAR_MIN_SCALE: f64 = 0.2;
/// A view shorter than this many CSS pixels (a phone) draws its bars larger, up to
/// `BAR_MAX_BOOST` times, so they stay readable where the authored size is a sliver;
/// laptops, tablets and the overview keep the authored proportions.
const BAR_REFERENCE_CSS_HEIGHT: f64 = 720.0;
const BAR_MAX_BOOST: f64 = 1.8;

/// The world scale of a bar `rim_width` metres wide, `distance` metres from a camera
/// with a vertical field of view of `fov_y` radians, on a view `css_height` CSS pixels
/// tall (0 when unknown).
pub fn bar_scale(distance: f64, fov_y: f64, rim_width: f64, css_height: f64) -> f64 {
    let view_height = 2.0 * distance * (fov_y / 2.0).tan();
    let far =
        ((distance - BAR_NEAR_DISTANCE) / (BAR_FAR_DISTANCE - BAR_NEAR_DISTANCE)).clamp(0.0, 1.0);
    let far = far * far * (3.0 - 2.0 * far);
    let share = BAR_NEAR_SCREEN_SHARE + (BAR_MAX_SCREEN_SHARE - BAR_NEAR_SCREEN_SHARE) * far;
    let cap = share * view_height / rim_width;
    let boost = if css_height > 0.0 {
        (BAR_REFERENCE_CSS_HEIGHT / css_height).clamp(1.0, BAR_MAX_BOOST)
    } else {
        1.0
    };
    (1f64.min(cap) * boost).max(BAR_MIN_SCALE)
}

/// Seconds the damage trail holds the chunk a hit took before it drains, and how much
/// of the bar it drains per second: the chunk reads for a beat and is gone in about
/// 0.4 s, so a wounded bar never looks fuller than it is.
const TRAIL_HOLD_SECONDS: f64 = 0.16;
const TRAIL_DRAIN_PER_SECOND: f64 = 2.2;

/// The dark warm trail behind an overhead bar's fill: it holds the hull ratio from before a
/// hit for a beat, then drains to the fill. Repairs move it up with the fill. It runs on
/// match time, so a paused or frozen battle holds it where it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarTrail {
    pub ratio: f64,
    last: f64,
    hold: f64,
    time: f64,
}

impl BarTrail {
    pub fn new(ratio: f64, time: f64) -> Self {
        Self {
            ratio,
            last: ratio,
            hold: 0.0,
            time,
        }
    }

    /// Advance to match `time` (seconds) toward the hull `ratio`; returns the trail's
    /// ratio.
    pub fn update(&mut self, ratio: f64, time: f64) -> f64 {
        let dt = (time - self.time).clamp(0.0, 0.25);
        self.time = time;
        if ratio < self.last {
            self.hold = TRAIL_HOLD_SECONDS;
        }
        self.last = ratio;
        if ratio >= self.ratio {
            self.ratio = ratio;
            self.hold = 0.0;
        } else if self.hold > 0.0 {
            self.hold -= dt;
        } else {
            self.ratio = (self.ratio - TRAIL_DRAIN_PER_SECOND * dt).max(ratio);
        }
        self.ratio
    }
}

/// How fast an overhead bar fades in or out as cover hides or reveals its tank (1/s).
pub const BAR_FADE_RATE: f64 = 14.0;

/// Ease `current` toward `target` at `rate` per second over `dt` seconds, landing on
/// it once within a hair, so a settled bar draws fully opaque in its usual batch.
pub fn approach(current: f64, target: f64, rate: f64, dt: f64) -> f64 {
    let next = current + (target - current) * (1.0 - (-rate * dt).exp());
    if (next - target).abs() < 0.01 {
        target
    } else {
        next
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
        assert_eq!(health_bar_state(100.0, 100.0).tone, HealthColor::Healthy);
        assert_eq!(health_bar_state(100.0, 100.0).color, HEALTHY_COLOR);
        assert_eq!(health_bar_state(60.0, 100.0).tone, HealthColor::Warning);
        assert_eq!(health_bar_state(30.0, 100.0).color, CRITICAL_COLOR);
        assert_eq!(health_bar_state(-5.0, 100.0).ratio, 0.0);
        let meters = protection_meters(true, 5.0, 60.0, 1.0);
        assert!(meters.shield_visible && meters.spawn_visible);
        assert_eq!(meters.spawn_y, 0.72);
        assert_eq!(spawn_pulse(2.5), (1.0, 1.0));
    }

    #[test]
    fn bars_keep_a_sane_size_on_screen() {
        let fov = 45f64.to_radians();
        // Far cameras draw the bar at its authored size.
        assert_eq!(bar_scale(60.0, fov, 1.84, 900.0), 1.0);
        // A close camera shrinks it to a smaller share of the view than a far one.
        let near = bar_scale(11.8, fov, 1.84, 900.0);
        assert!(near < 0.5, "{near}");
        let share = 1.84 * near / (2.0 * 11.8 * (fov / 2.0).tan());
        assert!((share - BAR_NEAR_SCREEN_SHARE).abs() < 1e-9);
        let mid = bar_scale(21.0, fov, 1.84, 900.0);
        let mid_share = 1.84 * mid / (2.0 * 21.0 * (fov / 2.0).tan());
        assert!(mid_share > BAR_NEAR_SCREEN_SHARE && mid_share < BAR_MAX_SCREEN_SHARE);
        assert_eq!(bar_scale(0.01, fov, 1.84, 900.0), BAR_MIN_SCALE);
        // A phone's short view grows it, within a bound.
        assert_eq!(bar_scale(60.0, fov, 1.84, 400.0), BAR_MAX_BOOST);
        assert_eq!(bar_scale(60.0, fov, 1.84, 600.0), 1.2);
    }

    #[test]
    fn the_trail_holds_a_hit_then_drains_and_follows_repairs_up() {
        let mut trail = BarTrail::new(1.0, 0.0);
        assert_eq!(trail.update(0.6, 0.1), 1.0);
        assert_eq!(trail.update(0.6, 0.4), 1.0);
        let mut time = 0.4;
        for _ in 0..10 {
            time += 0.1;
            trail.update(0.6, time);
        }
        assert!(trail.ratio < 1.0);
        for _ in 0..20 {
            time += 0.1;
            trail.update(0.6, time);
        }
        assert_eq!(trail.ratio, 0.6);
        assert_eq!(trail.update(0.8, time + 0.1), 0.8);
    }
}
