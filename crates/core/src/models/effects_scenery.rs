//! Custom shading, vertex attributes and generated textures introduced by the
//! scenery port. The TypeScript expressed these with TSL node materials and
//! CanvasTextures; model builders now only name them, and the renderer implements
//! each effect in WGSL from the contract documented here.
//!
//! Conventions used below:
//! - `time` is the presentation clock in seconds (`Presentation.time`, the value
//!   passed to `Scenery::update`), supplied by the renderer as a uniform.
//! - `C(0xrrggbb)` is an sRGB hex color converted to linear (`THREE.Color`).
//! - `LUMA = vec3(0.2126, 0.7152, 0.0722)`.
//! - `positionWorld`, `normalWorld` (interpolated vertex normal, normalised),
//!   `positionView`, `normalView`, `cameraPosition` and `uv` are the usual
//!   Three.js node inputs; `faceDirection` is +1 for front faces, -1 for back.
//! - sRGB textures are decoded to linear when sampled; their alpha is linear.
//! - `smoothstep(e0, e1, x)` is always `t = clamp((x - e0) / (e1 - e0), 0, 1);
//!   t * t * (3 - 2 * t)`, including when `e0 > e1` (implement it explicitly).
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
//!
//! ## Shared helper: `derivativeBump(height, scale)`
//! Screen-space bump (Mikkelsen's surface gradient) from a height already
//! sampled for colour, so relief costs no extra texture reads. Returns a
//! view-space normal:
//! ```text
//! slope  = vec2(dFdx(height), dFdy(height)) * scale
//! sigmaX = normalize(dFdx(positionView)); sigmaY = normalize(dFdy(positionView))
//! r1 = cross(sigmaY, normalView); r2 = cross(normalView, sigmaX)
//! det = dot(sigmaX, r1) * faceDirection
//! gradient = sign(det) * (slope.x * r1 + slope.y * r2)
//! normal = normalize(abs(det) * normalView - gradient)
//! ```
//! Use the same derivative functions for height and position (their common sign
//! convention cancels).
//!
//! ## Shared helper: `quarryGrit(amount, relief)`
//! Centimetre grit and pebbles the 10 cm soil bake cannot resolve. `G` is
//! [`GRIT_TEXTURE`] (sRGB, mirrored repeat, 8x anisotropy, mipmaps):
//! ```text
//! p    = positionWorld.xz
//! near = dot(G(p / 3.3).rgb, LUMA)
//! far  = G(vec2(p.x * 0.6 + p.y * 0.8, p.y * 0.6 - p.x * 0.8) / 11.7)
//! detail = near * (dot(far.rgb, LUMA) + 0.327) / (0.327 * 0.327 * 2)
//! color  = mix(1.0, detail, amount)          // luminance multiplier averaging 1
//! normal = derivativeBump(near, relief)      // view-space
//! ```
//!
//! ## Shared helper: `soilAt(position)`
//! The baked quarry soil ([`QUARRY_SOIL_TEXTURE`]) under a world position:
//! `S(vec2(position.x / 210 + 0.5, 0.5 - position.z / 210))`.

/// Secondary texture: the packed-dirt grit tile `G` of `quarryGrit`
/// ([`GRIT_TEXTURE`]).
pub const GRIT: &str = "grit";
/// Secondary texture: the baked quarry soil `S` of `soilAt`
/// ([`QUARRY_SOIL_TEXTURE`]).
pub const SOIL: &str = "soil";

/// Planar-reflecting water (`water-surface.ts`): the village creek and the harbor
/// basin. Unlit (`Shading::Basic`), opaque, fogged and tone mapped. `map` is the
/// ripple normal tile `N` (`textures/water/normals.webp`, linear, repeat, 4x
/// anisotropy) — it is not albedo.
///
/// Params, in order:
/// 0. `harbor`: 1 for the harbor (shore from world position), 0 for the creek
///    (shore from `uv.x`, which runs across the stream).
/// 1. `height`: world height of the mirror plane (the mesh's plane, normal +y).
/// 2. `distortionScale` (1.8 harbor, 0.65 creek).
/// 3. `normalScale`: world-to-ripple scale (4 harbor, 7 creek).
/// 4. `tilt`: horizontal normal strength (1.3 harbor, 0.75 creek).
/// 5. `sunDirection` x, y, z at 5–7 (normalised, towards the sun).
/// 8. `sunColor` r, g, b at 8–10 (linear).
/// 11. `deep` r, g, b at 11–13: bank color far from shore (linear).
/// 14. `shallow` r, g, b at 14–16: bank color at the shore (linear).
/// 17. `clockScale`: `clock = time * clockScale` (0.7 harbor, 0.65 creek).
/// 18. reflection target size in pixels (512, fixed, not viewport-relative).
/// 19. reflection MSAA samples (4; the main view's, so pipelines are shared).
/// 20. `calmExtent` (62): skip rendering the reflection this frame when the four
///     rays through NDC `(±1, ±1, 0.5)` from the camera all hit the plane
///     `y = height` with `max(|x|, |z|) <= calmExtent` (only apron is in view).
///
/// Reflection: render the scene (HUD layer excluded) from the camera mirrored in
/// the plane, with an oblique near plane on the water plane (clip bias 0), into a
/// linear-color target. `mirror` samples it at `vec2(1 - screenUV.x, screenUV.y) +
/// distortion` (Three's ReflectorNode default UV, flipped in x).
///
/// ```text
/// t = time * clockScale
/// p = positionWorld.xz * normalScale
/// n = N(p / 103 + vec2(t / 17, t / 29)) + N(p / 107 - vec2(t / -19, t / 31))
///   + N(p / vec2(8907, 9803) + vec2(t / 101, t / 97))
///   + N(p / vec2(1091, 1027) - vec2(t / 109, t / -113))
/// n = n * 0.5 - 1                                   // vec4
/// normal = normalize(n.xzy * vec3(tilt, 1, tilt))
/// worldToEye = cameraPosition - positionWorld; eye = normalize(worldToEye)
/// specular = pow(max(0, dot(eye, normalize(reflect(-sunDirection, normal)))), 100) * sunColor * 2
/// diffuse  = max(dot(sunDirection, normal), 0) * sunColor * 0.5
/// distortion = normal.xz * (0.001 + 1 / length(worldToEye)) * distortionScale
/// mirror = reflection(vec2(1 - screenUV.x, screenUV.y) + distortion).rgb
/// reflectance = pow(1 - max(dot(eye, normal), 0), 5) * 0.82 + 0.18
/// shore = harbor ? max(|positionWorld.x|, |positionWorld.z|) - 62
///                : (1 - |uv.x * 2 - 1|) * 6.5
/// bank = mix(deep, shallow, exp(max(shore, 0) * -0.55) * 0.65)
/// scatter = max(0, dot(normal, eye)) * bank
/// albedo = mix(scatter * (vec3(0.85) + diffuse * 0.25), mirror + specular, reflectance)
/// wash = shore + sin(t * 1.8 + positionWorld.x * 0.35 + positionWorld.z * 0.3) * 0.18
/// breakup = N(positionWorld.xz * 0.11 + t * 0.025).r
/// foam = (1 - smoothstep(0.12, 0.85, wash)) * smoothstep(0.46, 0.64, breakup)
/// color = mix(albedo, vec3(0.54, 0.67, 0.61), foam * 0.5)
/// ```
pub const WATER: &str = "water";

/// Wind sway of the village meadow tufts (`village-vegetation.ts`), a vertex-only
/// effect on an instanced, double-sided standard material (roughness 1, white;
/// the per-instance color multiplies the diffuse). No params. Per-instance
/// attribute [`WIND_ORIGIN`] (vec2, the tuft's world x/z; `instance_data.xy`). After the instance
/// transform (so `p` is the instanced model-space position, which is world space
/// here), before the model-view projection:
/// ```text
/// p += vec3(sin(time * 1.2 + o.x * 0.7 + o.y * 0.4) * p.y * 0.2,
///           0,
///           cos(time * 0.8 + o.y * 0.5) * p.y * 0.12)
/// ```
/// Normals are not changed. Tufts receive but do not cast shadows.
pub const MEADOW_SWAY: &str = "meadow-sway";

/// Tree foliage cards (cover tree crowns, shed boughs and felled crowns): an
/// alpha-tested, double-sided standard material whose vertex normals point out of
/// the crown. No params. After the instance transform, the cards sway with the
/// square of their height `h` above the instance origin and flutter a little:
/// ```text
/// sway    = (sin(time * 0.9 + phase) * 0.6 + sin(time * 1.7 + phase * 1.3) * 0.25) * 0.0025 * h * h
/// flutter = sin(time * 6 + dot(p, (3.1, 2.3, 2.7))) * 0.01 * min(h, 1)
/// ```
/// with `phase` from the world x/z, so neighbouring trees sway out of step. Back
/// faces keep the outward normal instead of flipping it, so a crown shades as one
/// rounded mass from either side of a card. The merged shadow ignores the sway.
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
/// [`SMOKE_PHASE`] (float; `instance_data.w`).
/// ```text
/// // vertex
/// t = fract(time * 0.065 + phase)                 // pass to the fragment
/// p = smokeOrigin + vec3(t * 1.5 + sin(time * 0.55 + phase * 20) * t * 0.25, t * 4.2, t * 0.45)
/// view = modelViewMatrix * vec4(p, 1); clip = projection * view
/// size = clamp((t * 1.6 + 0.22) * 720 / -view.z, 1, 65)      // pixels
/// clipPosition = vec4(clip.xy + positionGeometry.xy * size * 2 / viewportSize * clip.w, clip.zw)
/// // fragment
/// color = vec3(0.72, 0.75, 0.7)
/// alpha = (1 - smoothstep(0.2, 1, length(uv - 0.5) * 2)) * smoothstep(0, 0.15, t)
///       * pow(1 - t, 1.5) * 0.2
/// ```
/// `viewportSize` is the render target size in physical pixels.
pub const CHIMNEY_SMOKE: &str = "chimney-smoke";
/// Per-instance vec3 attribute of [`CHIMNEY_SMOKE`] (192 slots).
pub const SMOKE_ORIGIN: &str = "smokeOrigin";
/// Per-instance float attribute of [`CHIMNEY_SMOKE`] (192 slots).
pub const SMOKE_PHASE: &str = "phase";

/// Baked soil with world-space grit (`quarry-terrain.ts` `soilMaterial`): the
/// quarry floor, spoil heaps, talus, the haul ramp and scree fans. Standard PBR,
/// roughness 1, metalness 0, opaque, vertex colors. No params. `map` is the soil
/// bake `S` ([`QUARRY_SOIL_TEXTURE`]), sampled at the mesh `uv` (which maps world
/// x/z onto it). Extra texture 0: [`GRIT`].
/// ```text
/// baked  = S(uv)
/// stony  = (baked.a - 0.5) * 2
/// grit   = quarryGrit(mix(0.55, 1.5, stony), mix(0.35, 1.6, stony))
/// diffuse = vec4(baked.rgb * grit.color, 1) * vec4(vertexColor.rgb, 1)
/// normal (view space) = grit.normal
/// ```
pub const QUARRY_SOIL: &str = "quarry-soil";

/// Layered sandstone (`quarry-surfaces.ts` `sandstoneMaterial`): every quarry
/// rock, bench, butte and rubble mesh. Standard PBR, roughness 0.95, metalness 0,
/// opaque, vertex colors. No params. `map` is the sandstone photo `T`
/// (`textures/quarry/sandstone.webp`, sRGB, mirrored repeat, 4x anisotropy)
/// sampled triplanar in world space (the mesh UVs are unused). Extra texture 0:
/// [`SOIL`], for `soilAt`.
/// ```text
/// weights = pow(abs(normalWorld), 6); blend = weights / max(weights.x + weights.y + weights.z, 0.0001)
/// p = positionWorld; q = p / 6.5
/// grain = (T(q.zy + vec2(0.31, 0.11)) * blend.x + T(q.xz + vec2(0.57, 0.43)) * blend.y
///        + T(q.xy + vec2(0.13, 0.79)) * blend.z).rgb
/// relief = mix(vec3(dot(grain, LUMA)), grain, 0.35) / 0.4
/// normal (view space) = derivativeBump(dot(grain, LUMA), 0.6)
/// warp = sin(p.x * 0.061 + p.z * 0.047) * 0.9 + sin(p.x * 0.19 - p.z * 0.23) * 0.35
/// bed = p.y + warp
/// broad = sin(bed * 1.3 + sin(bed * 0.47) * 1.8) * 0.5 + 0.5
/// parting = smoothstep(0.82, 1, sin(bed * 4.7 + sin(bed * 1.9) * 2.1))
/// layered = mix(C(0xdbc6a4), C(0xc99f7f), broad)
/// tone = sin(p.x * 0.043 + sin(p.z * 0.031) * 2) * sin(p.z * 0.057 + p.y * 0.11 - p.x * 0.02) * 0.5 + 0.5
/// layered = mix(layered, C(0xb8a58c), smoothstep(0.62, 0.9, tone) * 0.55)
/// layered = mix(layered, C(0xc78a5c), smoothstep(0.35, 0.08, tone) * 0.35)
/// steep = 1 - abs(normalWorld.y); along = p.x + p.z
/// streak = smoothstep(0.55, 1, sin(along * 2.3 + sin(along * 0.61) * 3) * sin(along * 0.37 + p.y * 0.35)) * steep
/// rock = relief * layered * (1 - parting * 0.15 - streak * 0.14)
/// up = smoothstep(0.5, 0.92, normalWorld.y)
/// patch = sin(p.x * 0.37 + sin(p.z * 0.29) * 1.7) * sin(p.z * 0.41 - p.x * 0.13) * 0.5 + 0.5
/// ground = soilAt(p).rgb
/// rock = mix(rock, ground * (dot(relief, LUMA) * 0.3 + 0.75), up * mix(0.2, 0.7, patch))
/// floor = -min(max(max(|p.x|, |p.z|) - 60, 0) * 0.3, 1.8)
/// foot = 1 - smoothstep(0.02, 0.75, p.y - floor)
/// rock = mix(rock, ground * 0.9, foot * 0.6)
/// diffuse = vec4(rock, 1) * vec4(vertexColor.rgb, 1)
/// ```
pub const SANDSTONE: &str = "sandstone";

/// Sand drifted around rock cover (`quarry-surfaces.ts` `sandstoneFooting`).
/// Standard PBR, roughness 1, RGBA vertex colors, transparent, no depth write,
/// polygon offset (-1, -1) to stay above the coplanar floor, receives shadows
/// only; geometric normals. No params. `map` is the soil bake (for `soilAt`).
/// Extra texture 0: [`GRIT`].
/// ```text
/// diffuse = vec4(soilAt(positionWorld).rgb * quarryGrit(0.7, 0).color * 1.05, 1)
///         * vec4(vertexColor.rgb, vertexAlpha)      // RGBA vertex color
/// ```
pub const SAND_DRIFT: &str = "sand-drift";

/// The packed-dirt grit tile sampled by `quarryGrit` (also the packed-dirt ground).
pub const GRIT_TEXTURE: &str = "textures/ground/packed-dirt.webp";

// ---------------------------------------------------------------------------
// Generated textures (`TextureSource::Generated` keys).

/// Dusty Dig's baked soil: 2048x2048 RGBA8, sRGB color with linear alpha
/// (grittiness), clamp-to-edge, mipmapped, 8x anisotropy. Baked in Rust by
/// `quarry_soil::bake_quarry_soil` (row bands, any split) or
/// `quarry_soil::quarry_soil_pixels` (whole, cached). Row 0 is world
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

/// Pixels of a Rust-generated texture key (RGBA8, row-major, top row first), with
/// its width and height. Browser-drawn keys return `None`; see [`canvas_texture`].
pub fn generated_texture(key: &str) -> Option<(u32, u32, &'static [u8])> {
    match key {
        QUARRY_SOIL_TEXTURE => {
            let size = super::quarry_soil::QUARRY_SOIL_SIZE as u32;
            Some((size, size, super::quarry_soil::quarry_soil_pixels()))
        }
        _ => None,
    }
}
