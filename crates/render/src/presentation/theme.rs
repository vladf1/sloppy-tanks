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
/// Spread of the sun shadow's 5×5 filter in shadow-map texels: soft penumbrae
/// about 40 cm wide.
pub const SHADOW_FILTER_RADIUS: f32 = 1.35;
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
    pub ridges: Ridges,
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
    /// Half-size in metres of the square around the arena centre it leaves mostly
    /// clear, and the width over which it fades in beyond that square.
    pub clear_half_size: f32,
    pub clear_fade: f32,
    /// The share of the haze that still settles inside that square: a hint of depth
    /// toward the far side of the overview.
    pub arena_share: f32,
    /// The color thin haze shifts distant surfaces toward (aerial perspective),
    /// times the sky intensity; thick haze becomes the sky itself.
    pub aerial: u32,
}

/// Distant ranges drawn into the sky along the horizon, behind the scenery.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ridges {
    /// Their color before the haze, times the sky intensity.
    pub color: u32,
    /// Ray slope of the tallest peaks (0 = none).
    pub height: f32,
    /// Features around the horizon.
    pub features: f32,
    /// Flat mesa tops, 0..1.
    pub mesa: f32,
    /// Share of the horizon left open between them, 0..1: 0 a continuous range,
    /// higher values scattered headlands and islands.
    pub gaps: f32,
}

impl Ridges {
    pub const NONE: Ridges = Ridges {
        color: 0,
        height: 0.0,
        features: 0.0,
        mesa: 0.0,
        gaps: 0.0,
    };
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
    /// Display level mapped to black: holds the black point down.
    pub black: f32,
    /// Split toning: the shadows' and highlights' tints (sRGB hex, applied at unit
    /// luminance) and their strengths 0..1.
    pub shadow_tint: u32,
    pub shadow_toning: f32,
    pub highlight_tint: u32,
    pub highlight_toning: f32,
    /// Lime foliage pulled toward a deeper green, and orange earth calmed, 0..1.
    pub foliage: f32,
    pub earth: f32,
    /// How much red hulls shed the pink cast the blue sky fill gives them, 0..1.
    pub red_hold: f32,
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
        black: 0.0,
        shadow_tint: 0xffffff,
        shadow_toning: 0.0,
        highlight_tint: 0xffffff,
        highlight_toning: 0.0,
        foliage: 0.0,
        earth: 0.0,
        red_hold: 0.0,
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
                arena_share: 0.0,
                aerial: 0,
            },
            ridges: Ridges::NONE,
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
        fill_color: pick(theme, 0x8fb2f2, 0x98bbee, 0x94b8f2),
        fill_ground: pick(theme, 0x9a7d5e, 0x7c7870, 0x8a7a5c),
        fill_intensity: pick(theme, 1.35, 1.55, 1.6),
        shadow,
        atmosphere: theme_atmosphere(theme),
    }
}

/// Each map's sky, haze and grade: a crisp pine-village morning under a deep blue
/// sky, fair-weather cumulus and far blue ranges; a salty harbor haze over a coastal
/// sky with a low distant shore; and a warm golden quarry under a dusty sky with far
/// mesas. The haze thickens with distance and thins with height, so the overhead
/// cameras stay clean while the backdrop recedes in steps. Grades hold the black
/// point, cool the shadows, warm the highlights and tame lime grass and orange earth.
fn theme_atmosphere(theme: Theme) -> Atmosphere {
    /// Mostly clear of haze: the arena and its walls, then a fade over the near backdrop.
    const HAZE_CLEAR_HALF_SIZE: f32 = ARENA as f32 + 8.0;
    const HAZE_CLEAR_FADE: f32 = 60.0;
    /// Bloom only what is hotter than sunlit white: emissives, fire and the sun.
    const BLOOM: Bloom = Bloom {
        strength: 0.22,
        threshold: 1.45,
        knee: 0.35,
    };
    match theme {
        Theme::Village | Theme::Custom => Atmosphere {
            enabled: true,
            zenith: 0x1c5bc4,
            horizon: 0xb8d6ec,
            sky_intensity: 1.15,
            sun_glow: 0xfff0d2,
            sun_glow_intensity: 1.25,
            sun_radius: 0.022,
            clouds: Clouds {
                coverage: 0.56,
                lit: 0xffffff,
                shade: 0x8094b2,
                shadow: 0.1,
            },
            haze: Haze {
                density: 0.0021,
                falloff: 0.022,
                start: 45.0,
                max: 0.88,
                clear_half_size: HAZE_CLEAR_HALF_SIZE,
                clear_fade: HAZE_CLEAR_FADE,
                arena_share: 0.3,
                aerial: 0x9cb6d6,
            },
            ridges: Ridges {
                color: 0x56739a,
                height: 0.055,
                features: 5.0,
                mesa: 0.0,
                gaps: 0.0,
            },
            rim: 0.3,
            bloom: BLOOM,
            exposure: 1.12,
            occlusion: 0.9,
            grade: Grade {
                lift: [0.0; 3],
                gamma: [1.0; 3],
                gain: [1.03, 1.0, 0.97],
                saturation: 1.0,
                vibrance: 0.12,
                contrast: 1.07,
                vignette: 0.14,
                black: 0.008,
                shadow_tint: 0x86a2e0,
                shadow_toning: 0.35,
                highlight_tint: 0xfff0d0,
                highlight_toning: 0.3,
                foliage: 0.6,
                earth: 0.45,
                red_hold: 0.7,
            },
        },
        Theme::Harbor => Atmosphere {
            enabled: true,
            zenith: 0x2468c0,
            horizon: 0xc0d8e6,
            sky_intensity: 1.15,
            sun_glow: 0xffe6c6,
            sun_glow_intensity: 1.25,
            sun_radius: 0.022,
            clouds: Clouds {
                coverage: 0.5,
                lit: 0xfbfcfc,
                shade: 0x8499ae,
                shadow: 0.1,
            },
            haze: Haze {
                density: 0.0017,
                falloff: 0.03,
                start: 45.0,
                max: 0.9,
                clear_half_size: HAZE_CLEAR_HALF_SIZE,
                clear_fade: HAZE_CLEAR_FADE,
                arena_share: 0.3,
                aerial: 0xa4b9cc,
            },
            ridges: Ridges {
                color: 0x8aa2b4,
                height: 0.026,
                features: 4.0,
                mesa: 0.0,
                gaps: 0.42,
            },
            rim: 0.3,
            bloom: BLOOM,
            exposure: 1.13,
            occlusion: 0.9,
            grade: Grade {
                lift: [0.0; 3],
                gamma: [1.0; 3],
                gain: [1.04, 1.0, 0.95],
                saturation: 1.04,
                vibrance: 0.15,
                contrast: 1.08,
                vignette: 0.14,
                black: 0.01,
                shadow_tint: 0x86a2e0,
                shadow_toning: 0.35,
                highlight_tint: 0xfff0d4,
                highlight_toning: 0.3,
                foliage: 0.6,
                earth: 0.3,
                red_hold: 0.7,
            },
        },
        Theme::Quarry => Atmosphere {
            enabled: true,
            zenith: 0x3270bd,
            horizon: 0xe6d0ab,
            sky_intensity: 1.1,
            sun_glow: 0xffcf8a,
            sun_glow_intensity: 1.2,
            sun_radius: 0.024,
            clouds: Clouds {
                coverage: 0.4,
                lit: 0xfff4e2,
                shade: 0xa8958f,
                shadow: 0.08,
            },
            haze: Haze {
                density: 0.0022,
                falloff: 0.026,
                start: 55.0,
                max: 0.82,
                clear_half_size: HAZE_CLEAR_HALF_SIZE,
                clear_fade: HAZE_CLEAR_FADE,
                arena_share: 0.25,
                aerial: 0xc4ad92,
            },
            ridges: Ridges {
                color: 0x9c7c66,
                height: 0.045,
                features: 4.0,
                mesa: 1.0,
                gaps: 0.0,
            },
            rim: 0.28,
            bloom: BLOOM,
            exposure: 1.04,
            occlusion: 0.9,
            grade: Grade {
                lift: [0.0; 3],
                gamma: [1.0; 3],
                gain: [1.06, 1.02, 0.86],
                saturation: 1.06,
                vibrance: 0.3,
                contrast: 1.07,
                vignette: 0.14,
                black: 0.01,
                shadow_tint: 0x8a9ae0,
                shadow_toning: 0.42,
                highlight_tint: 0xffeaa8,
                highlight_toning: 0.4,
                foliage: 0.5,
                earth: 0.0,
                red_hold: 0.7,
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
        assert_eq!(quarry.fill_intensity, 1.35);
        assert!(quarry.shadow.right - quarry.shadow.left > 136.0);
        assert_eq!(Theme::from_name("superstress"), Theme::Custom);
    }
}
