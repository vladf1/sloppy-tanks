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

/// The frame-wide look beyond Three's lights (`gpu::Renderer::set_atmosphere`): a
/// drawn sky with sun and clouds, aerial-perspective haze, cloud shadows, a sky rim
/// light, bloom, ambient occlusion and a color grade. Colors are sRGB hex. The
/// default is off: a flat background, Three's linear fog and plain ACES output,
/// which the labs' reference frames use.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Atmosphere {
    pub enabled: bool,
    /// Sky overhead and at the horizon, times `sky_intensity` (linear).
    pub zenith: u32,
    pub horizon: u32,
    pub sky_intensity: f32,
    /// The sun's halo and disc, times `sun_glow_intensity`.
    pub sun_glow: u32,
    pub sun_glow_intensity: f32,
    /// Angular radius of the sun disc, radians.
    pub sun_radius: f32,
    pub clouds: Clouds,
    pub haze: Haze,
    /// Strength of the sky rim on grazing walls and hulls.
    pub rim: f32,
    pub bloom: Bloom,
    /// Multiplies the theme's exposure.
    pub exposure: f32,
    /// Ambient occlusion strength 0..1.
    pub occlusion: f32,
    pub grade: Grade,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clouds {
    /// Share of the sky they cover, 0..1.
    pub coverage: f32,
    /// Sunlit tops and shaded undersides.
    pub lit: u32,
    pub shade: u32,
    /// Sunlight their shadows take from the ground, 0..1.
    pub shadow: f32,
}

/// Exponential height haze tinted by the sky behind it (aerial perspective).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Haze {
    /// Extinction per metre at height 0.
    pub density: f32,
    /// How fast it thins with height, per metre.
    pub falloff: f32,
    /// Metres from the eye before it begins: the play area stays clear.
    pub start: f32,
    /// The most of a surface it hides, 0..1.
    pub max: f32,
    /// Half-size in metres of the square around the arena centre it leaves clear,
    /// and the width over which it fades in beyond that square.
    pub clear_half_size: f32,
    pub clear_fade: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bloom {
    /// Share of the blurred highlights added back.
    pub strength: f32,
    /// Linear brightness where highlights start to bloom, and the soft knee below it.
    pub threshold: f32,
    pub knee: f32,
}

/// The color grade on the tone-mapped image.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grade {
    /// Blacks raised toward this color (0 = none).
    pub lift: [f32; 3],
    /// Midtone power per channel (1 = none; above 1 brightens).
    pub gamma: [f32; 3],
    /// Whites scaled per channel (1 = none).
    pub gain: [f32; 3],
    pub saturation: f32,
    /// Extra saturation for muted colors only (0 = none).
    pub vibrance: f32,
    pub contrast: f32,
    /// Corner darkening, 0..1.
    pub vignette: f32,
}

impl Grade {
    pub const NEUTRAL: Grade = Grade {
        lift: [0.0; 3],
        gamma: [1.0; 3],
        gain: [1.0; 3],
        saturation: 1.0,
        vibrance: 0.0,
        contrast: 1.0,
        vignette: 0.0,
    };
}

impl Default for Atmosphere {
    fn default() -> Self {
        Self {
            enabled: false,
            zenith: 0,
            horizon: 0,
            sky_intensity: 0.0,
            sun_glow: 0,
            sun_glow_intensity: 0.0,
            sun_radius: 0.0,
            clouds: Clouds {
                coverage: 0.0,
                lit: 0,
                shade: 0,
                shadow: 0.0,
            },
            haze: Haze {
                density: 0.0,
                falloff: 0.0,
                start: 0.0,
                max: 0.0,
                clear_half_size: 0.0,
                clear_fade: 0.0,
            },
            rim: 0.0,
            bloom: Bloom {
                strength: 0.0,
                threshold: 1.0,
                knee: 0.5,
            },
            exposure: 1.0,
            occlusion: 0.0,
            grade: Grade::NEUTRAL,
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
    pub shadow: ShadowCamera,
    pub atmosphere: Atmosphere,
}

pub fn theme_look(theme: Theme) -> ThemeLook {
    /// The theme's value; extra levels take the village look.
    fn pick<T>(theme: Theme, quarry: T, harbor: T, village: T) -> T {
        match theme {
            Theme::Quarry => quarry,
            Theme::Harbor => harbor,
            Theme::Village | Theme::Custom => village,
        }
    }
    let quarry = theme == Theme::Quarry;
    let sun_position = Vec3::new(
        if quarry { -50.0 } else { -45.0 },
        pick(theme, 36.0, 46.0, 50.0),
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
        sky: pick(theme, 0xd6c9b0, 0xa7bdc5, 0xaacbc2),
        fog_near: pick(theme, 110.0, 150.0, 210.0),
        fog_far: pick(theme, 380.0, 260.0, 380.0),
        sun_color: pick(theme, 0xffd29a, 0xffe2bc, 0xffe4bc),
        sun_intensity: pick(theme, 4.0, 3.9, 4.0),
        sun_position,
        fill_color: pick(theme, 0x9ebdf0, 0xa2c2ec, 0xa0c2f0),
        fill_ground: pick(theme, 0x8a7b68, 0x6b7584, 0x857b68),
        fill_intensity: pick(theme, 1.1, 1.3, 1.35),
        shadow,
        atmosphere: theme_atmosphere(theme),
    }
}

/// Each map's sky, haze and grade: a crisp pine-village morning under a deep blue
/// sky and fair-weather clouds, a salty harbor haze that softens the open water's
/// horizon under a bright coastal sky, and a warm golden quarry under a dusty sky.
/// The haze begins beyond the arena so every gameplay camera stays crisp; it only
/// grounds the backdrop. Grades keep shadows cool and highlights warm.
fn theme_atmosphere(theme: Theme) -> Atmosphere {
    /// Clear of haze: the arena and its walls, then a fade over the near backdrop.
    const HAZE_CLEAR_HALF_SIZE: f32 = ARENA as f32 + 8.0;
    const HAZE_CLEAR_FADE: f32 = 50.0;
    match theme {
        Theme::Village | Theme::Custom => Atmosphere {
            enabled: true,
            zenith: 0x2468cf,
            horizon: 0xcfe7f4,
            sky_intensity: 1.15,
            sun_glow: 0xfff0d2,
            sun_glow_intensity: 1.2,
            sun_radius: 0.022,
            clouds: Clouds {
                coverage: 0.5,
                lit: 0xffffff,
                shade: 0x9cb2cc,
                shadow: 0.28,
            },
            haze: Haze {
                density: 0.0026,
                falloff: 0.03,
                start: 40.0,
                max: 0.7,
                clear_half_size: HAZE_CLEAR_HALF_SIZE,
                clear_fade: HAZE_CLEAR_FADE,
            },
            rim: 0.25,
            bloom: Bloom {
                strength: 0.16,
                threshold: 1.0,
                knee: 0.6,
            },
            exposure: 1.07,
            occlusion: 0.85,
            grade: Grade {
                lift: [0.0, 0.01, 0.012],
                gamma: [1.0, 1.0, 1.0],
                gain: [1.04, 1.01, 0.97],
                saturation: 1.0,
                vibrance: 0.4,
                contrast: 1.06,
                vignette: 0.2,
            },
        },
        Theme::Harbor => Atmosphere {
            enabled: true,
            zenith: 0x2f78cc,
            horizon: 0xd3e6ec,
            sky_intensity: 1.15,
            sun_glow: 0xffe6c6,
            sun_glow_intensity: 1.2,
            sun_radius: 0.022,
            clouds: Clouds {
                coverage: 0.4,
                lit: 0xfbfcfc,
                shade: 0x93a9bb,
                shadow: 0.28,
            },
            haze: Haze {
                density: 0.0012,
                falloff: 0.04,
                start: 40.0,
                max: 0.6,
                clear_half_size: HAZE_CLEAR_HALF_SIZE,
                clear_fade: HAZE_CLEAR_FADE,
            },
            rim: 0.25,
            bloom: Bloom {
                strength: 0.16,
                threshold: 1.0,
                knee: 0.6,
            },
            exposure: 1.04,
            occlusion: 0.85,
            grade: Grade {
                lift: [0.0, 0.008, 0.014],
                gamma: [1.0, 1.0, 1.0],
                gain: [1.0, 1.0, 1.0],
                saturation: 1.0,
                vibrance: 0.3,
                contrast: 1.06,
                vignette: 0.2,
            },
        },
        Theme::Quarry => Atmosphere {
            enabled: true,
            zenith: 0x2f76c8,
            horizon: 0xf4d9aa,
            sky_intensity: 1.1,
            sun_glow: 0xffd08e,
            sun_glow_intensity: 1.4,
            sun_radius: 0.024,
            clouds: Clouds {
                coverage: 0.28,
                lit: 0xfff6e6,
                shade: 0xc4ab96,
                shadow: 0.24,
            },
            haze: Haze {
                density: 0.004,
                falloff: 0.035,
                start: 40.0,
                max: 0.78,
                clear_half_size: HAZE_CLEAR_HALF_SIZE,
                clear_fade: HAZE_CLEAR_FADE,
            },
            rim: 0.22,
            bloom: Bloom {
                strength: 0.16,
                threshold: 1.0,
                knee: 0.6,
            },
            exposure: 0.97,
            occlusion: 0.85,
            grade: Grade {
                lift: [0.0, 0.008, 0.012],
                gamma: [1.0, 1.0, 1.0],
                gain: [1.05, 1.0, 0.92],
                saturation: 1.0,
                vibrance: 0.4,
                contrast: 1.08,
                vignette: 0.2,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn themes_restore_their_own_light() {
        let village = theme_look(Theme::Village);
        assert_eq!(village.sky, 0xaacbc2);
        assert_eq!(village.sun_position, Vec3::new(-45.0, 50.0, 25.0));
        assert_eq!(theme_look(Theme::Custom), village);
        let harbor = theme_look(Theme::Harbor);
        assert_eq!((harbor.fog_near, harbor.fog_far), (150.0, 260.0));
        let quarry = theme_look(Theme::Quarry);
        assert_eq!(quarry.fill_intensity, 1.1);
        assert!(quarry.shadow.right - quarry.shadow.left > 136.0);
        assert_eq!(Theme::from_name("superstress"), Theme::Custom);
    }
}
