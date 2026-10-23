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
/// Secondary texture: the dock concrete tile (`textures/harbor/dock.webp`), the
/// grain [`HARBOR_APRON`] samples at world x/z.
pub const DOCK_GRAIN: &str = "dock-grain";
/// Secondary texture: the harbor ground's baked wear ([`HARBOR_APRON_WEAR`]), sampled
/// at world x/z by the concrete and the asphalt lanes.
pub const APRON_WEAR: &str = "apron-wear";
/// The generated texture of the harbor ground's wear: grime, stains, oil, wheel
/// tracks, prints, damp and contact shade, drawn once on the GPU by the renderer's
/// bake (`effects::bakes::HARBOR_APRON_WEAR`), linear, over the 128 m round the arena.
pub const HARBOR_APRON_WEAR: &str = "harbor-apron-wear";

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

/// Harbor Havoc's quay concrete, asphalt lanes and the paint on them
/// (`harbor_surfaces`): standard PBR whose surface is worked out in world space, so
/// it never shows the tile. Extra texture [`DOCK_GRAIN`] is the dock concrete tile
/// (sRGB, repeat), sampled at world x/z at several scales and angles for grain; the
/// material color (and vertex color, for markings) tints it. Params: `[mode]`, an
/// [`ApronMode`] as a float. The concrete and the lanes also take extra texture
/// [`APRON_WEAR`], their slowly varying wear baked once. Concrete adds six-metre slabs with sawn and sealed
/// expansion joints, per-slab tone and hue, macro variation, hairline cracks, oil
/// spills with a wet core, damp patches and puddles (lower roughness); asphalt lanes
/// add aggregate, ravelled edges, polished wheel paths with an oil strip between
/// them, tar-sealed cracks and repair patches; both take cast-iron covers, contact
/// grime against the fixed cover and rust under the container castings. Paint modes
/// keep their color, dirty it and chip it with world-space wear that follows the
/// traffic (markings alpha-test the chips away, labels fade their alpha).
/// Quay walls take a wet tide band, barnacles, weed at the waterline and rust runs,
/// with sunlight thrown off the ripples dancing above the water (as on ship hulls,
/// which add their plating's weathering); spawn pads their
/// tread plate, team paint and chasing light ring; cover containers a shade of
/// their line's color per box; the sea wall's parapet its precast panels, capping
/// stones, stains, scuffs and worn hazard paint.
/// Implemented in `effects/harbor_apron.wgsl`.
pub const HARBOR_APRON: &str = "harbor-apron";

/// Harbor Havoc's far shore ([`ApronMode::Shore`] materials, `harbor_backdrop`):
/// vertex colors, flat shaded, fog off, lit and hazed by the effect from their
/// distance to the eye and emitted, so the coast's layers keep their separation
/// whatever the scene fog; pure red vertices are warning lights, and parts flagged
/// by their uv.y take windows, container stacks, countryside, pines, welded tanks,
/// slipformed bins or cladding. Params: `[mode]` as for [`HARBOR_APRON`] (unused).
/// A shader of its own, so neither it nor the apron compiles the other's code.
/// Implemented in `effects/harbor_shore.wgsl`.
pub const HARBOR_SHORE: &str = "harbor-shore";

/// What a [`HARBOR_APRON`] surface is (`params[0]`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ApronMode {
    /// Bare quay concrete.
    Concrete = 0,
    /// An asphalt traffic lane running along z, its tyre polish along z.
    LaneAlongZ = 1,
    /// An asphalt traffic lane running along x.
    LaneAlongX = 2,
    /// Painted marking, alpha-tested where the paint has chipped.
    Marking = 3,
    /// Painted lettering (transparent texture), faded where worn.
    Label = 4,
    /// Vertical quay faces rising from the harbor water.
    QuayWall = 5,
    /// Ship hull plating (vertex colors over the steel tile): welded seams, rust
    /// runs, waterline slime and fender scrapes.
    Hull = 6,
    /// The far shore (vertex colors, flat shaded, fog off): lit and hazed in the
    /// effect by its distance from the eye and emitted, so its layers keep their
    /// separation whatever the scene fog; pure red vertices are warning lights.
    Shore = 7,
    /// A spawn pad: the team color is the vertex color and the uv is the pad's own
    /// x/z over its radius. Its steel deck takes a tread plate, hazard rim,
    /// chevrons and bolts; the light ring set into it (uv.x offset by 10) shines
    /// in the team's color with a pulse chasing round it.
    Pad = 8,
    /// The paint of a cover container's walls, doors and roof (standard, over the
    /// wear tile): each box standing in its own 20 m cell of the yard takes its own
    /// shade of its line's color, so a row of one color never reads as clones.
    Container = 10,
    /// The sea wall's parapet round the apron (the harbor's boundary cover): a
    /// precast body under a capping beam. Its panel and capping joints, form-tie
    /// holes, rain and rust stains, tank scuffs and the worn hazard paint of its
    /// kerb band and top edge are all the effect's, in world space.
    Parapet = 11,
}

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

/// The shipping lines' liveries painted on harbor containers (browser-drawn, see
/// [`canvas_texture`]): [`CONTAINER_LIVERIES`] rows of white emblem, name and
/// tagline on transparency, each a 4:1 strip, row 0 at the top.
pub const CONTAINER_LIVERY_TEXTURE: &str = "harbor-container-liveries";
pub const CONTAINER_LIVERIES: usize = 4;
/// Name and tagline of each livery row.
const LIVERY_ROWS: [(&str, &str); CONTAINER_LIVERIES] = [
    ("SLOPCO", ""),
    ("TANKLINE", ""),
    ("HAVOC", "SHIPPING CO."),
    ("SLOPPY", "EXPRESS"),
];
const LIVERY_PAINT: &str = "#f4f1e6";

fn container_liveries() -> CanvasTexture {
    use CanvasOp::*;
    let text = |font, text, x, y, align| FillText {
        color: LIVERY_PAINT,
        font,
        align,
        baseline: "alphabetic",
        text,
        at: [x, y],
        max_width: Some(780.0),
    };
    let mut ops = Vec::new();
    for (row, &(logo, tagline)) in LIVERY_ROWS.iter().enumerate() {
        let top = row as f64 * 256.0;
        // An emblem left of the name: a disc crossed by a dark wave for the even rows,
        // three speed bars for the odd ones.
        if row % 2 == 0 {
            let (cx, cy, r) = (110.0, top + 110.0, 78.0);
            let disc = (0..=32)
                .map(|i| {
                    let a = f64::from(i) / 32.0 * std::f64::consts::TAU;
                    [cx + r * a.cos(), cy + r * a.sin()]
                })
                .collect();
            ops.push(FillPolygon {
                color: LIVERY_PAINT,
                points: disc,
            });
            ops.push(FillPolygon {
                color: "#27343c",
                points: vec![
                    [cx - r, cy + 6.0],
                    [cx - 30.0, cy - 22.0],
                    [cx + 10.0, cy + 8.0],
                    [cx + r, cy - 26.0],
                    [cx + r, cy - 4.0],
                    [cx + 10.0, cy + 30.0],
                    [cx - 30.0, cy],
                    [cx - r, cy + 28.0],
                ],
            });
        } else {
            for bar in 0..3 {
                let y = top + 60.0 + f64::from(bar) * 44.0;
                let x = 30.0 + f64::from(bar) * 22.0;
                ops.push(FillPolygon {
                    color: LIVERY_PAINT,
                    points: vec![
                        [x + 30.0, y],
                        [x + 170.0, y],
                        [x + 150.0, y + 28.0],
                        [x, y + 28.0],
                    ],
                });
            }
        }
        let name_y = if tagline.is_empty() {
            top + 160.0
        } else {
            top + 130.0
        };
        ops.push(text(
            "italic 900 150px sans-serif",
            logo,
            230.0,
            name_y,
            "start",
        ));
        if !tagline.is_empty() {
            ops.push(text(
                "bold 52px sans-serif",
                tagline,
                236.0,
                top + 186.0,
                "start",
            ));
        }
    }
    CanvasTexture {
        width: 1024,
        height: 1024,
        ops,
    }
}

/// Painted harbor apron labels and their paint: white on the asphalt lanes,
/// traffic yellow in the concrete bays. They share one browser-drawn sheet,
/// [`HARBOR_LABEL_TEXTURE`], a 1024 x 256 row each (row 0 at the top), so every
/// label on the apron is one draw.
const HARBOR_LABELS: [(&str, &str); 5] = [
    ("B1", "#f0c24a"),
    ("B2", "#f0c24a"),
    ("HARBOR HAVOC", "#f1efe6"),
    ("PORT 07", "#f1efe6"),
    ("LOADING", "#f0c24a"),
];
/// The generated texture key of the harbor label sheet.
pub const HARBOR_LABEL_TEXTURE: &str = "harbor-labels";
/// Rows of the label sheet.
pub const HARBOR_LABEL_ROWS: usize = HARBOR_LABELS.len();

/// The row of a harbor apron label on [`HARBOR_LABEL_TEXTURE`].
pub fn harbor_label_row(text: &str) -> usize {
    HARBOR_LABELS
        .iter()
        .position(|(label, _)| *label == text)
        .unwrap_or_else(|| panic!("unknown harbor label {text}"))
}

fn harbor_label_sheet() -> CanvasTexture {
    let ops = HARBOR_LABELS
        .iter()
        .enumerate()
        .map(|(row, &(text, color))| CanvasOp::FillText {
            color,
            font: "900 140px sans-serif",
            align: "center",
            baseline: "middle",
            text,
            at: [512.0, 128.0 + row as f64 * 256.0],
            max_width: Some(990.0),
        })
        .collect();
    CanvasTexture {
        width: 1024,
        height: 256 * HARBOR_LABELS.len() as u32,
        ops,
    }
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
    match key {
        HARBOR_LABEL_TEXTURE => Some(harbor_label_sheet()),
        CONTAINER_LIVERY_TEXTURE => Some(container_liveries()),
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
