//! Each map theme's light, sky and fog, from `presentation.ts` `reset()` and
//! `scenery.ts` (`createLighting`, `defaultSunShadow`, `fitSunShadow`). Dusty Dig
//! bakes low and warm: a raking sun, cool sky fill with warm sand bounce from
//! below, and a pale dusty haze that softens the far cuts. Every value is set on
//! each reset, so switching maps restores the other themes. Extra levels use
//! the village look with plain pads and floors.

use glam::Vec3;
use sloppy_core::sim::data::ARENA;

use crate::camera::ShadowCamera;

/// `SHADOW_DEPTH` in scenery.ts: the sun box's depth span the bias was tuned for.
pub const SHADOW_DEPTH: f32 = 219.5;
pub const SHADOW_MAP_SIZE: u32 = 2048;
pub const SHADOW_BIAS: f32 = -0.0002;
pub const SHADOW_NORMAL_BIAS: f32 = 0.05;
/// No map shows a surface that receives the sun's shadow below this height (the
/// deepest, the village creek bed, is under 5 m down), so the renderer skips
/// casters whose shadows could only land outside the views.
pub const SHADOW_RECEIVER_FLOOR: f32 = -10.0;
/// The quarry's fitted shadow box: the arena square and the apron props beside
/// its wall, heights from its floor cuts to its machinery.
const QUARRY_SHADOW_HALF: f32 = 68.0;
const QUARRY_SHADOW_LOW: f32 = -2.0;
const QUARRY_SHADOW_HIGH: f32 = 9.0;
/// Every theme reflects its sky at full strength (see `environment_radiance`).
const REFLECTIONS: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Village,
    Harbor,
    Quarry,
    /// An extra level's plain yard.
    Custom,
}

impl Theme {
    pub fn from_name(name: &str) -> Theme {
        match name {
            "village" => Theme::Village,
            "harbor" => Theme::Harbor,
            "quarry" => Theme::Quarry,
            _ => Theme::Custom,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Theme::Village => "village",
            Theme::Harbor => "harbor",
            Theme::Quarry => "quarry",
            Theme::Custom => "custom",
        }
    }
}

/// Renderer-independent environment values (converted to `gpu::Environment`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThemeLook {
    pub sky: u32,
    pub fog_near: f32,
    pub fog_far: f32,
    pub sun_color: u32,
    pub sun_intensity: f32,
    pub sun_position: Vec3,
    pub fill_color: u32,
    pub fill_ground: u32,
    pub fill_intensity: f32,
    pub exposure: f32,
    pub reflections: f32,
    pub shadow: ShadowCamera,
}

pub fn theme_look(theme: Theme) -> ThemeLook {
    let quarry = theme == Theme::Quarry;
    let harbor = theme == Theme::Harbor;
    let pick = |q: u32, h: u32, v: u32| {
        if quarry {
            q
        } else if harbor {
            h
        } else {
            v
        }
    };
    let pickf = |q: f32, h: f32, v: f32| {
        if quarry {
            q
        } else if harbor {
            h
        } else {
            v
        }
    };
    let sun_position = Vec3::new(
        if quarry { -50.0 } else { -45.0 },
        pickf(43.0, 55.0, 68.0),
        if quarry { 28.0 } else { 25.0 },
    );
    let shadow = if quarry {
        ShadowCamera::fit_square(
            sun_position,
            Vec3::ZERO,
            QUARRY_SHADOW_HALF,
            QUARRY_SHADOW_LOW,
            QUARRY_SHADOW_HIGH,
            SHADOW_DEPTH,
        )
    } else {
        ShadowCamera::square(
            sun_position,
            Vec3::ZERO,
            ARENA as f32 + 10.0,
            0.5,
            SHADOW_DEPTH,
        )
    };
    ThemeLook {
        sky: pick(0xd6c9b0, 0xa7bdc5, 0xaacbc2),
        fog_near: pickf(110.0, 150.0, 210.0),
        fog_far: pickf(380.0, 260.0, 380.0),
        sun_color: pick(0xffd6ab, 0xffbf85, 0xffd59b),
        sun_intensity: if quarry { 3.0 } else { 2.8 },
        sun_position,
        fill_color: pick(0xb9cff2, 0xafcfee, 0xbdd5f5),
        fill_ground: pick(0x8a7b68, 0x63778e, 0x75859b),
        fill_intensity: if quarry { 1.1 } else { 1.65 },
        exposure: 1.0,
        reflections: REFLECTIONS,
        shadow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn themes_restore_their_own_light() {
        let village = theme_look(Theme::Village);
        assert_eq!(village.sky, 0xaacbc2);
        assert_eq!(village.sun_position, Vec3::new(-45.0, 68.0, 25.0));
        assert_eq!(theme_look(Theme::Custom), village);
        let harbor = theme_look(Theme::Harbor);
        assert_eq!((harbor.fog_near, harbor.fog_far), (150.0, 260.0));
        let quarry = theme_look(Theme::Quarry);
        assert_eq!(quarry.fill_intensity, 1.1);
        assert!(quarry.shadow.right - quarry.shadow.left > 136.0);
        assert_eq!(Theme::from_name("superstress"), Theme::Custom);
    }
}
