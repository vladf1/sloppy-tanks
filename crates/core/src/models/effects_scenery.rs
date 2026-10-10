//! Custom shading, vertex attributes and generated textures introduced by the
//! scenery port. The TypeScript expressed these with TSL node materials and
//! CanvasTextures; model builders now only name them, and the renderer implements
//! each effect in WGSL (`crates/render/src/shaders/`).
//!
//! Conventions used below:
//! - `time` is the presentation clock in seconds (`Presentation.time`, the value
//!   passed to `Scenery::update`), supplied by the renderer as a uniform.
//! - sRGB textures are decoded to linear when sampled; their alpha is linear.
//! - Unless stated, effects keep the material's standard lighting (sun, sky fill,
//!   shadows, fog, tone mapping) and its fixed-function fields (side,
//!   transparency, depth, polygon offset) from `Material`; an effect replaces only
//!   the stated stages.
//! - Secondary textures are the material's `extra_textures`, named [`GRIT`] or
//!   [`SOIL`]; each effect lists the ones it samples, in slot order.
//! - Per-instance attributes are `Attribute::per_instance` mesh attributes; the
//!   renderer packs them in mesh attribute order into the instance's four floats
//!   of effect data (`instance_data`).
//! - RGBA vertex colors carry their alpha in the mesh's
//!   [`VERTEX_ALPHA`](crate::geometry::VERTEX_ALPHA) attribute; with vertex colors
//!   the renderer multiplies the base alpha by it.

/// Secondary texture: the packed-dirt grit tile ([`GRIT_TEXTURE`]), sampled in
/// world space for the centimetre grit the soil bake cannot resolve.
pub const GRIT: &str = "grit";
/// Secondary texture: the baked quarry soil ([`QUARRY_SOIL_TEXTURE`]), sampled at
/// world x/z.
pub const SOIL: &str = "soil";

/// Planar-reflecting water (`water-surface.ts`): the village creek and the harbor
/// basin. Unlit (`Shading::Basic`), opaque, fogged and tone mapped. `map` is the
/// ripple normal tile (`textures/water/normals.webp`, linear, repeat, 4x
/// anisotropy) — it is not albedo. Params: `[height]`, the world height of the
/// mirror plane (the mesh's plane, normal +y).
///
/// The renderer takes the surface out of the scenery and draws it with its own
/// planar-reflection pass (`shaders/water.wgsl`). Each body's tuning (distortion,
/// ripple scale, tilt, sun, bank colors, clock scale, reflection size, and the calm
/// extent that skips the reflection while only the apron is in view) is the
/// renderer's `WaterSettings::harbor` / `creek`.
pub const WATER: &str = "water";

/// Wind sway of the village meadow tufts (`village-vegetation.ts`), a vertex-only
/// effect on an instanced, double-sided standard material (roughness 1, white;
/// the per-instance color multiplies the diffuse). No params. Per-instance
/// attribute [`WIND_ORIGIN`] (vec2, the tuft's world x/z; `instance_data.xy`). The
/// tufts sway with their height after the instance transform (instanced
/// positions are world space here), before the projection
/// (`effects/meadow_sway.wgsl`). Normals are not changed. Tufts receive but do not
/// cast shadows.
pub const MEADOW_SWAY: &str = "meadow-sway";

/// Tree foliage cards (cover tree crowns, shed boughs and felled crowns): an
/// alpha-tested, double-sided standard material whose vertex normals point out of
/// the crown. No params. After the instance transform, the cards sway with the
/// square of their height above the instance origin and flutter a little, phased by
/// the world x/z so neighbouring trees sway out of step (`effects/foliage.wgsl`).
/// Back faces keep the outward normal instead of flipping it, so a crown shades as
/// one rounded mass from either side of a card. The merged shadow ignores the sway.
pub const FOLIAGE: &str = "foliage";

/// Per-instance attribute of [`MEADOW_SWAY`]: one vec2 per instance
/// (`data.len() == instances * 2`).
pub const WIND_ORIGIN: &str = "windOrigin";

/// GPU-animated chimney wisps (`village-atmosphere.ts`): camera-facing,
/// screen-sized quads. Unlit, transparent (normal blending), no depth write, no
/// fog, tone mapped; never frustum culled. No params. The mesh is a unit plane
/// (`positionGeometry.xy` in ±0.5, `uv`) drawn once per instance (the drawable's
/// instance count; instance matrices are identity and unused), with per-instance
/// attributes [`SMOKE_ORIGIN`] (vec3, world; `instance_data.xyz`) and
/// [`SMOKE_PHASE`] (float; `instance_data.w`). Each wisp rises and drifts from its
/// origin over a looping lifetime, growing in screen pixels as it fades
/// (`effects/chimney_smoke.wgsl`).
pub const CHIMNEY_SMOKE: &str = "chimney-smoke";
/// Per-instance vec3 attribute of [`CHIMNEY_SMOKE`] (192 slots).
pub const SMOKE_ORIGIN: &str = "smokeOrigin";
/// Per-instance float attribute of [`CHIMNEY_SMOKE`] (192 slots).
pub const SMOKE_PHASE: &str = "phase";

/// Baked soil with world-space grit (`quarry-terrain.ts` `soilMaterial`): the
/// quarry floor, spoil heaps, talus, the haul ramp and scree fans. Standard PBR,
/// roughness 1, metalness 0, opaque, vertex colors. No params. `map` is the soil
/// bake ([`QUARRY_SOIL_TEXTURE`]), sampled at the mesh `uv` (which maps world
/// x/z onto it); its alpha (grittiness) sets how strong the grit is. Extra texture
/// 0: [`GRIT`]. Implemented in `effects/quarry_soil.wgsl`.
pub const QUARRY_SOIL: &str = "quarry-soil";

/// Layered sandstone (`quarry-surfaces.ts` `sandstoneMaterial`): every quarry
/// rock, bench, butte and rubble mesh. Standard PBR, roughness 0.95, metalness 0,
/// opaque, vertex colors. No params. `map` is the sandstone photo
/// (`textures/quarry/sandstone.webp`, sRGB, mirrored repeat, 4x anisotropy)
/// sampled triplanar in world space (the mesh UVs are unused). Extra texture 0:
/// [`SOIL`], which dusts upward faces and the rock's foot. Implemented in
/// `effects/sandstone.wgsl`.
pub const SANDSTONE: &str = "sandstone";

/// Sand drifted around rock cover (`quarry-surfaces.ts` `sandstoneFooting`).
/// Standard PBR, roughness 1, RGBA vertex colors, transparent, no depth write,
/// polygon offset (-1, -1) to stay above the coplanar floor, receives shadows
/// only; geometric normals. No params. `map` is the soil bake, sampled at world
/// x/z; the vertex alpha fades the drift out. Extra texture 0: [`GRIT`].
/// Implemented in `effects/sand_drift.wgsl`.
pub const SAND_DRIFT: &str = "sand-drift";

/// The packed-dirt tile the quarry effects sample as grit (also the packed-dirt
/// ground).
pub const GRIT_TEXTURE: &str = "textures/ground/packed-dirt.webp";

// ---------------------------------------------------------------------------
// Generated textures (`TextureSource::Generated` keys).

/// Dusty Dig's baked soil: 2048x2048 RGBA8, sRGB color with linear alpha
/// (grittiness), clamp-to-edge, mipmapped, 8x anisotropy. Baked in Rust by
/// `quarry_soil::bake_quarry_soil` (row bands, any split). Row 0 is world
/// `z = -105` and was the canvas' top row; like every CanvasTexture it uploads
/// with Three's default `flipY`, so row 0 lands at `v = 1` (the soil UVs,
/// `v = 0.5 - z / 210`, rely on this).
pub const QUARRY_SOIL_TEXTURE: &str = "quarry-soil";
/// The "PINE VILLAGE" hanging sign (browser-drawn, see [`canvas_texture`]).
pub const VILLAGE_SIGN_TEXTURE: &str = "village-sign";
/// Dusty Dig's site sign (browser-drawn, see [`canvas_texture`]).
pub const QUARRY_SIGN_TEXTURE: &str = "quarry-sign";

/// Painted harbor apron labels and their texture keys.
const HARBOR_LABELS: [(&str, &str); 5] = [
    ("B1", "harbor-label-b1"),
    ("B2", "harbor-label-b2"),
    ("HARBOR HAVOC", "harbor-label-harbor-havoc"),
    ("PORT 07", "harbor-label-port-07"),
    ("LOADING", "harbor-label-loading"),
];

/// The generated texture key of a harbor apron label.
pub fn harbor_label_texture(text: &str) -> &'static str {
    HARBOR_LABELS
        .iter()
        .find(|(label, _)| *label == text)
        .map(|(_, key)| *key)
        .unwrap_or_else(|| panic!("unknown harbor label {text}"))
}

/// One Canvas 2D drawing step, replayed by the browser adapter in order on a
/// fresh, transparent canvas (`fillStyle`/`font`/`textAlign`/`textBaseline` are
/// set before each step as given).
#[derive(Clone, Debug, PartialEq)]
pub enum CanvasOp {
    FillRect {
        color: &'static str,
        rect: [f64; 4],
    },
    StrokeRect {
        color: &'static str,
        line_width: f64,
        rect: [f64; 4],
    },
    /// `fillText(text, x, y, maxWidth?)`.
    FillText {
        color: &'static str,
        font: &'static str,
        align: &'static str,
        baseline: &'static str,
        text: &'static str,
        at: [f64; 2],
        max_width: Option<f64>,
    },
    /// `beginPath(); moveTo(first); lineTo(rest...); fill()`.
    FillPolygon {
        color: &'static str,
        points: Vec<[f64; 2]>,
    },
}

/// A texture the TypeScript drew with Canvas 2D text, which needs the browser's
/// font rendering: upload as sRGB, clamp-to-edge, mipmapped, default orientation
/// (`flipY`, like `CanvasTexture`).
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasTexture {
    pub width: u32,
    pub height: u32,
    pub ops: Vec<CanvasOp>,
}

/// The canvas drawing behind a browser-drawn generated texture key.
pub fn canvas_texture(key: &str) -> Option<CanvasTexture> {
    use CanvasOp::*;
    if let Some((text, _)) = HARBOR_LABELS.iter().find(|(_, k)| *k == key) {
        return Some(CanvasTexture {
            width: 1024,
            height: 256,
            ops: vec![FillText {
                color: "#d9ce9d",
                font: "900 140px sans-serif",
                align: "center",
                baseline: "middle",
                text,
                at: [512.0, 128.0],
                max_width: Some(990.0),
            }],
        });
    }
    match key {
        VILLAGE_SIGN_TEXTURE => Some(CanvasTexture {
            width: 512,
            height: 128,
            ops: vec![
                FillRect {
                    color: "#816b45",
                    rect: [0.0, 0.0, 512.0, 128.0],
                },
                StrokeRect {
                    color: "#c9b384",
                    line_width: 5.0,
                    rect: [8.0, 8.0, 496.0, 112.0],
                },
                FillText {
                    color: "#efe5c7",
                    font: "bold 48px Georgia",
                    align: "center",
                    baseline: "middle",
                    text: "PINE VILLAGE",
                    at: [256.0, 68.0],
                    max_width: None,
                },
            ],
        }),
        QUARRY_SIGN_TEXTURE => {
            let text = |color, font, text, x, y| FillText {
                color,
                font,
                align: "start",
                baseline: "alphabetic",
                text,
                at: [x, y],
                max_width: None,
            };
            let mut ops = vec![
                FillRect {
                    color: "#d6cbb0",
                    rect: [0.0, 0.0, 512.0, 256.0],
                },
                FillRect {
                    color: "#353e3c",
                    rect: [0.0, 0.0, 512.0, 62.0],
                },
                text(
                    "#eee4c9",
                    "bold 30px sans-serif",
                    "DUSTY DIG / 03",
                    25.0,
                    42.0,
                ),
                text(
                    "#353e3c",
                    "bold 49px sans-serif",
                    "ACTIVE QUARRY",
                    24.0,
                    129.0,
                ),
                text(
                    "#353e3c",
                    "24px sans-serif",
                    "HAUL ROAD   \u{2022}   KEEP CLEAR",
                    25.0,
                    175.0,
                ),
                FillRect {
                    color: "#b58b39",
                    rect: [0.0, 211.0, 512.0, 45.0],
                },
            ];
            let mut x = -40.0;
            while x < 550.0 {
                ops.push(FillPolygon {
                    color: "#353e3c",
                    points: vec![
                        [x, 256.0],
                        [x + 40.0, 211.0],
                        [x + 65.0, 211.0],
                        [x + 25.0, 256.0],
                    ],
                });
                x += 64.0;
            }
            Some(CanvasTexture {
                width: 512,
                height: 256,
                ops,
            })
        }
        _ => None,
    }
}
