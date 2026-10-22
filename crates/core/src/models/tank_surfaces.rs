//! The vehicle livery: painted armor, bare metal and matte fittings weathered in
//! vertex colors and drawn with three shared materials.
//!
//! Every opaque part of a vehicle model is repainted once when the model is built:
//! its paint becomes a per-vertex color in the model's own frame (before the
//! vehicle scale), weathered by where the vertex sits: dust and mud climbing the
//! lower hull and running gear, a dust film on upward faces, darker undersides, a
//! slight value shift per armor plate and a hand-painted light-to-dark gradient
//! down each upright plate. Team paint and its shade then draw with one white
//! vertex-colored material that carries the paint finish tile (brush strokes,
//! chips, scratches: `scripts/generate-tank-textures.ts`), projected in model space
//! so the finish runs on across neighbouring parts. The paint is a clean team color
//! without a camouflage, so a tank's side reads at a glance from every camera.
//! Bare steel and gun metal share a metal material and the rest (rubber edges,
//! canvas, wood, markings, openings) a matte one, so each joint of a vehicle costs
//! few draws. Team-colored recognition panels and turret bands glow faintly in
//! their team's color, one material per team (each team's meshes are its own
//! already), so a tank's side still reads in shade and at range. Glass and lenses
//! keep their own glossy materials.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use glam::{DMat4, DVec3};

use super::Team;
use super::model_primitives::Cache;
use crate::geometry::Mesh;
use crate::geometry::math::hex_to_linear;
use crate::scene::{Color, Effect, Material, Node, Shading, TextureRef};

/// Service paint per team: a bright cobalt and a deep signal crimson, less
/// saturated than the glowing `TEAM_COLORS` so the armor reads as painted steel
/// while staying instantly team-coded. The blue is kept light enough not to fall
/// to black in shade, the red deep enough not to wash out to salmon in sun. The
/// team colors stay on markings, flags, bars and projectiles.
pub const VEHICLE_PAINT: [u32; 2] = [0x2f80d8, 0xb4302e];

/// A team's vehicle body paint.
pub fn vehicle_paint(team: Team) -> u32 {
    VEHICLE_PAINT[team.index()]
}

/// The worn-metal image under `public/`, also used by the quarry's machinery.
pub const ARMOR_WEAR_TEXTURE: &str = "textures/tanks/armor-wear.webp";
/// The livery's paint finish tile under `public/` (the file keeps the name of the
/// camouflage it replaced).
pub const ARMOR_PAINT_TEXTURE: &str = "textures/tanks/armor-camo.webp";
/// The burnt-out paint of wrecks, on the paint's UVs.
pub const ARMOR_SCORCH_TEXTURE: &str = "textures/tanks/armor-scorch.webp";
/// Burnt paint: dull, rough, a little relief where it blistered.
const SCORCH_ROUGHNESS: f32 = 0.92;
const SCORCH_BUMP_SCALE: f32 = 0.02;
const TEXTURE_ANISOTROPY: u8 = 4;
/// Paint UVs per metre of armor, shared with the wrecks' scorch tile (one tile per
/// 4 m), and how often the paint finish repeats over one UV unit: its chips and
/// brush strokes are sized for 2 m, sharp from the first-person seat.
const PAINT_UV_PER_METRE: f64 = 0.25;
const PAINT_REPEAT: f32 = 2.0;
/// The paint tile's mean multiplier (its base tone); the vertex paint is divided
/// by it so the base tone shows the intended team paint.
const PAINT_TEXTURE_MEAN: f64 = 0.8;
/// Height of the paint's relief (layer edges, chips) in the bump map.
const PAINT_BUMP_SCALE: f32 = 0.012;
const METAL_BUMP_SCALE: f32 = 0.02;
/// Military enamel: satin enough to catch a soft sky sheen along plate edges,
/// rough enough not to read as glossy plastic.
const PAINT_METALNESS: f32 = 0.1;
const PAINT_ROUGHNESS: f32 = 0.5;
/// Bare steel and gun metal.
const METAL_METALNESS: f32 = 0.55;
const METAL_ROUGHNESS: f32 = 0.4;
/// Rubber, canvas, wood, stencils and dark openings.
const MATTE_ROUGHNESS: f32 = 0.85;
/// Materials smoother than this are glass and lenses and keep their own finish.
const GLOSSY_BELOW: f32 = 0.4;

/// Dry dust and darker wet mud (sRGB) caked on the lower hull and running gear.
const DUST: u32 = 0x8f7c5c;
const MUD: u32 = 0x4e4031;
/// How much dust tints the lowest paint, and how far wet mud climbs (a fraction of
/// the dusty band).
const DUST_STRENGTH: f64 = 0.8;
/// Tracks and tyres (already vertex-colored) take little dry dust and a lighter
/// coat of mud than the hull, so wheels, tyres and links stay legible.
const DUST_ON_RUNNING_GEAR: f64 = 0.25;
const MUD_ON_RUNNING_GEAR: f64 = 0.55;
const MUD_STRENGTH: f64 = 0.68;
const MUD_BAND: f64 = 0.55;
/// Dust settled on upward faces: a trace, so the overhead camera still sees the
/// team paint rather than a brown film.
const TOP_FILM: f64 = 0.05;
/// Darkening at the ground line and on the undersides.
const GROUND_SHADE: f64 = 0.7;
const UNDERSIDE_SHADE: f64 = 0.62;
/// Plate-to-plate paint value variation (fraction either way).
const PLATE_VARIATION: f64 = 0.07;
/// The hand-painted gradient down an upright plate: its paint value at the
/// plate's foot and at its top edge. Plates shorter than `GRADIENT_MIN_HEIGHT`
/// (chamfers, trim) keep their flat value.
const GRADIENT_FOOT: f64 = 0.72;
const GRADIENT_TOP: f64 = 1.1;
const GRADIENT_MIN_HEIGHT: f64 = 0.05;
/// Faces leaning less than this (normal y) from vertical take the gradient.
const UPRIGHT: f64 = 0.6;
/// Worn paint on chamfered plate edges: the paint rubbed lighter toward a pale
/// primer, a hand-painted bevel highlight that outlines every plate.
const WORN_EDGE: u32 = 0xd8ccb8;
const EDGE_WEAR: f64 = 0.5;
const EDGE_LIFT: f64 = 1.45;
/// Painted armor glows this little in its team paint's color, so a tank's side
/// keeps its team hue in shade and against a backlit sky instead of falling to
/// black; on the dark trim it reads as the trim's faint team tint.
const PAINT_GLOW: f32 = 0.05;
/// Recognition panels and turret bands glow this much in their team color.
const SIGNAL_GLOW: f32 = 0.45;
/// A chamfer strip: a flat triangle at least this long and at most this wide,
/// facing diagonally (no axis component above `EDGE_FACING`).
const EDGE_MIN_LENGTH: f64 = 0.08;
const EDGE_MAX_WIDTH: f64 = 0.05;
const EDGE_FACING: f64 = 0.95;

/// Where a vehicle meets the ground and how high its dusty band reaches, in model
/// metres before the vehicle scale.
#[derive(Clone, Copy, Debug)]
pub struct Grime {
    pub ground: f64,
    pub dust_top: f64,
}

/// The armor-wear texture as the quarry machinery configures it: sRGB, repeating,
/// 4x anisotropy.
pub fn armor_wear_texture() -> TextureRef {
    TextureRef {
        anisotropy: TEXTURE_ANISOTROPY,
        ..TextureRef::file(ARMOR_WEAR_TEXTURE)
    }
}

fn armor_paint_texture() -> TextureRef {
    TextureRef {
        anisotropy: TEXTURE_ANISOTROPY,
        repeat: [PAINT_REPEAT; 2],
        ..TextureRef::file(ARMOR_PAINT_TEXTURE)
    }
}

/// The finishes a livery paints with; each is one shared material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Finish {
    /// Painted armor, glowing faintly in the team paint's color.
    Paint {
        glow: u32,
    },
    Metal,
    Matte,
    /// Recognition panels and turret bands in a team's signal color: matte, glowing
    /// faintly, kept apart so wrecks can burn them off.
    Signal(u32),
}

fn white_vertex_material(metalness: f32, roughness: f32) -> Material {
    Material {
        vertex_colors: true,
        ..Material::standard(0xffffff, metalness, roughness)
    }
}

impl Finish {
    fn is_paint(self) -> bool {
        matches!(self, Finish::Paint { .. })
    }
}

/// The shared material of a finish.
fn finish_material(finish: Finish) -> Arc<Material> {
    static METAL: OnceLock<Arc<Material>> = OnceLock::new();
    static MATTE: OnceLock<Arc<Material>> = OnceLock::new();
    match finish {
        Finish::Paint { glow } => {
            static GLOWING: Cache<u32, Material> = Cache::new();
            return GLOWING.get_or_insert(glow, || Material {
                emissive: Color(glow),
                emissive_intensity: PAINT_GLOW,
                ..paint_material()
            });
        }
        Finish::Metal => METAL.get_or_init(|| {
            let wear = armor_wear_texture();
            Arc::new(Material {
                map: Some(wear.clone()),
                bump_map: Some(wear),
                bump_scale: METAL_BUMP_SCALE,
                ..white_vertex_material(METAL_METALNESS, METAL_ROUGHNESS)
            })
        }),
        Finish::Matte => {
            MATTE.get_or_init(|| Arc::new(white_vertex_material(0.0, MATTE_ROUGHNESS)))
        }
        Finish::Signal(color) => return signal_material(color),
    }
    .clone()
}

fn paint_material() -> Material {
    let finish = armor_paint_texture();
    Material {
        map: Some(finish.clone()),
        bump_map: Some(finish),
        bump_scale: PAINT_BUMP_SCALE,
        ..white_vertex_material(PAINT_METALNESS, PAINT_ROUGHNESS)
    }
}

/// The recognition material of a team's signal color: matte cloth that glows
/// faintly in that color.
fn signal_material(color: u32) -> Arc<Material> {
    static SIGNALS: Cache<u32, Material> = Cache::new();
    SIGNALS.get_or_insert(color, || Material {
        emissive: Color(color),
        emissive_intensity: SIGNAL_GLOW,
        ..white_vertex_material(0.0, MATTE_ROUGHNESS)
    })
}

/// Whether a part wears the team paint finish.
pub fn is_camouflaged(material: &Material) -> bool {
    material.map.as_ref().map(|map| &map.source) == Some(&armor_paint_texture().source)
}

/// The material of a wreck's burnt paint: the scorch tile over a vertex-colored ash
/// base, on the paint's UVs.
pub fn scorched_paint_material() -> Arc<Material> {
    static SCORCHED: OnceLock<Arc<Material>> = OnceLock::new();
    SCORCHED
        .get_or_init(|| {
            let scorch = TextureRef {
                anisotropy: TEXTURE_ANISOTROPY,
                ..TextureRef::file(ARMOR_SCORCH_TEXTURE)
            };
            Arc::new(Material {
                map: Some(scorch.clone()),
                bump_map: Some(scorch),
                bump_scale: SCORCH_BUMP_SCALE,
                ..white_vertex_material(0.0, SCORCH_ROUGHNESS)
            })
        })
        .clone()
}

/// Embers glowing in a wreck's open hatches and turret well: charred black under a
/// strong orange glow that the wreck's aging dims as it cools.
const EMBER: u32 = 0x1c0c06;
const EMBER_GLOW: u32 = 0xff5a16;
const EMBER_GLOW_INTENSITY: f32 = 2.6;

/// The shared material of smouldering embers.
pub fn ember_material() -> Arc<Material> {
    static EMBERS: OnceLock<Arc<Material>> = OnceLock::new();
    EMBERS
        .get_or_init(|| {
            Arc::new(Material {
                emissive: Color(EMBER_GLOW),
                emissive_intensity: EMBER_GLOW_INTENSITY,
                ..Material::standard(EMBER, 0.0, 0.95)
            })
        })
        .clone()
}

/// Whether a repainted part is a recognition panel or turret band, which burns
/// away on a wreck.
pub fn is_signal_panel(material: &Arc<Material>) -> bool {
    material.vertex_colors
        && material.emissive.0 != 0
        && material.emissive_intensity == SIGNAL_GLOW
        && material.map.is_none()
}

/// The livery of one vehicle: its painted colors (team paint and shade), the
/// colors that are bare metal, its recognition panel's color and its grime band.
pub struct Livery<'a> {
    pub paint: &'a [u32],
    pub metal: &'a [u32],
    pub signal: u32,
    pub grime: Grime,
}

impl Livery<'_> {
    /// The finish a part's material takes, or `None` to leave it alone.
    fn finish(&self, material: &Material) -> Option<Finish> {
        let opaque = material.shading == Shading::Standard
            && material.effect == Effect::None
            && !material.transparent
            && material.alpha_test == 0.0
            && material.emissive_map.is_none();
        let color = material.color.0;
        let ember = std::ptr::eq(material, &*ember_material());
        if !opaque || ember || material.vertex_colors || material.roughness < GLOSSY_BELOW {
            return None;
        }
        Some(if self.paint.contains(&color) {
            // Trim shares the team paint's material (and so its glow): one
            // painted material per team keeps a joint's paint in one draw.
            Finish::Paint {
                glow: self.paint[0],
            }
        } else if color == self.signal {
            Finish::Signal(color)
        } else if self.metal.contains(&color) || material.metalness >= 0.45 {
            Finish::Metal
        } else {
            Finish::Matte
        })
    }
}

/// Repaint every opaque part of `root` (a vehicle model in its own frame; the
/// root's transform is ignored) with the livery's weathered finishes. Parts that
/// are already vertex-colored (tracks, tyres) keep their material and gain the
/// grime.
pub fn apply_livery(root: &mut Node, livery: &Livery) {
    let mut weathered_meshes = HashMap::new();
    for child in &mut root.children {
        repaint(child, DMat4::IDENTITY, livery, &mut weathered_meshes);
    }
}

/// A shared part mesh placed at the same height and tilt (the Humvee's four
/// wheels) weathers the same, so its copies keep sharing one weathered mesh:
/// the source mesh and material, and the world matrix row that gives height.
type WeatherKey = (usize, usize, [i64; 4]);

fn weather_key(mesh: &Arc<Mesh>, material: &Arc<Material>, world: DMat4) -> WeatherKey {
    let row = world.row(1);
    let quantize = |value: f64| (value * 1e4).round() as i64;
    (
        Arc::as_ptr(mesh) as usize,
        Arc::as_ptr(material) as usize,
        [row.x, row.y, row.z, row.w].map(quantize),
    )
}

fn repaint(
    node: &mut Node,
    parent: DMat4,
    livery: &Livery,
    weathered_meshes: &mut HashMap<WeatherKey, (Arc<Mesh>, Arc<Material>)>,
) {
    let world = parent * node.local_matrix();
    if let Some(drawable) = &mut node.drawable
        && drawable.instances.is_none()
    {
        let finish = livery.finish(&drawable.material);
        let regrimed = drawable.material.vertex_colors && !drawable.mesh.colors.is_empty();
        if finish.is_some() || regrimed {
            let key = weather_key(&drawable.mesh, &drawable.material, world);
            let (mesh, material) = weathered_meshes
                .entry(key)
                .or_insert_with(|| match finish {
                    Some(finish) => (
                        Arc::new(weathered(
                            &drawable.mesh,
                            world,
                            Some(hex_to_linear(drawable.material.color.0)),
                            finish,
                            livery.grime,
                        )),
                        finish_material(finish),
                    ),
                    None => (
                        Arc::new(weathered(
                            &drawable.mesh,
                            world,
                            None,
                            Finish::Matte,
                            livery.grime,
                        )),
                        drawable.material.clone(),
                    ),
                })
                .clone();
            drawable.mesh = mesh;
            drawable.material = material;
        }
    }
    for child in &mut node.children {
        repaint(child, world, livery, weathered_meshes);
    }
}

fn mix(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The armor plate a face lies in: coplanar triangles share it.
type PlateKey = [i64; 4];

fn plate_key(normal: DVec3, point: DVec3) -> PlateKey {
    let quantize = |v: f64, step: f64| (v / step).round() as i64;
    [
        quantize(normal.x, 0.05),
        quantize(normal.y, 0.05),
        quantize(normal.z, 0.05),
        quantize(normal.dot(point), 0.03),
    ]
}

/// A stable value in [-1, 1] per armor plate.
fn plate_variation(key: PlateKey) -> f64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for value in key {
        hash ^= value as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
        hash ^= hash >> 29;
    }
    (hash % 2001) as f64 / 1000.0 - 1.0
}

/// Weathered paint of a vertex at model-space `p` on a face with normal `n`.
fn grimed(
    base: [f64; 3],
    p: DVec3,
    n: DVec3,
    finish: Finish,
    grime: Grime,
    dust_scale: f64,
) -> [f64; 3] {
    let dust = hex_to_linear(DUST);
    let mud = hex_to_linear(MUD);
    let height = ((p.y - grime.ground) / (grime.dust_top - grime.ground)).clamp(0.0, 1.0);
    let low = 1.0 - height;
    let mut color = mix(base, dust, low.powf(1.7) * DUST_STRENGTH * dust_scale);
    let mud_strength = if dust_scale < 1.0 {
        MUD_STRENGTH * MUD_ON_RUNNING_GEAR
    } else {
        MUD_STRENGTH
    };
    color = mix(
        color,
        mud,
        (1.0 - height / MUD_BAND).clamp(0.0, 1.0).powi(2) * mud_strength,
    );
    if finish.is_paint() && n.y > 0.5 {
        color = mix(color, dust, TOP_FILM * (n.y - 0.5) * 2.0);
    }
    let mut shade = GROUND_SHADE + (1.0 - GROUND_SHADE) * smoothstep(0.0, 1.6, height);
    if n.y < 0.0 {
        shade *= 1.0 + (UNDERSIDE_SHADE - 1.0) * -n.y;
    }
    color.map(|c| c * shade)
}

/// A non-indexed copy of `mesh` moved into the model frame for weathering (the
/// stored positions stay in the part's frame), with weathered vertex colors and,
/// for paint, model-space paint UVs. `base` is the part's paint; `None`
/// weathers the mesh's own vertex colors.
fn weathered(
    mesh: &Mesh,
    world: DMat4,
    base: Option<[f64; 3]>,
    finish: Finish,
    grime: Grime,
) -> Mesh {
    let mut out = mesh.to_non_indexed();
    let count = out.positions.len();
    let own_colors = std::mem::take(&mut out.colors);
    let mut colors = Vec::with_capacity(count);
    let flip = world.determinant() < 0.0;
    out.uvs.resize(count, [0.0; 2]);
    let faces: Vec<([DVec3; 3], DVec3)> = out
        .positions
        .as_chunks::<3>()
        .0
        .iter()
        .map(|corners| {
            let p = corners.map(|c| world.transform_point3(DVec3::from(c.map(f64::from))));
            let face = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
            (p, if flip { -face } else { face })
        })
        .collect();
    let heights = plate_heights(&faces);
    for (t, &(p, face)) in faces.iter().enumerate() {
        let key = plate_key(face, p[0]);
        let variation = if finish.is_paint() {
            1.0 + PLATE_VARIATION * plate_variation(key)
        } else {
            1.0
        };
        let gradient = |y: f64| match heights.get(&key) {
            Some(&(foot, top)) if finish.is_paint() && top - foot >= GRADIENT_MIN_HEIGHT => {
                GRADIENT_FOOT + (GRADIENT_TOP - GRADIENT_FOOT) * (y - foot) / (top - foot)
            }
            _ => 1.0,
        };
        let normals = &mesh_normals(&out, t);
        let edge = finish.is_paint() && is_chamfer(&p, face, normals);
        for (k, point) in p.iter().enumerate() {
            let index = t * 3 + k;
            let paint = base
                .unwrap_or_else(|| own_colors.get(index).map_or([1.0; 3], |c| c.map(f64::from)));
            let dust_scale = if base.is_some() {
                1.0
            } else {
                DUST_ON_RUNNING_GEAR
            };
            let shade = variation * gradient(point.y);
            let mut color =
                grimed(paint, *point, face, finish, grime, dust_scale).map(|c| c * shade);
            if edge {
                let lifted = paint.map(|c| c * EDGE_LIFT);
                color = mix(
                    mix(color, lifted, EDGE_WEAR),
                    hex_to_linear(WORN_EDGE),
                    EDGE_WEAR * 0.5,
                );
            }
            if finish.is_paint() {
                color = color.map(|c| c / PAINT_TEXTURE_MEAN);
                let (u, v) = paint_uv(*point, face);
                out.uvs[index] = [u as f32, v as f32];
            }
            colors.push(color.map(|c| c as f32));
        }
    }
    out.colors = colors;
    out
}

/// The height range of every upright plate among `faces` (corners, normal).
fn plate_heights(faces: &[([DVec3; 3], DVec3)]) -> HashMap<PlateKey, (f64, f64)> {
    let mut heights = HashMap::new();
    for (p, face) in faces {
        if face.y.abs() >= UPRIGHT {
            continue;
        }
        let range = heights
            .entry(plate_key(*face, p[0]))
            .or_insert((f64::INFINITY, f64::NEG_INFINITY));
        for corner in p {
            range.0 = range.0.min(corner.y);
            range.1 = range.1.max(corner.y);
        }
    }
    heights
}

/// The stored normals of triangle `t`.
fn mesh_normals(mesh: &Mesh, t: usize) -> [[f32; 3]; 3] {
    let at = |k: usize| mesh.normals.get(t * 3 + k).copied().unwrap_or([0.0; 3]);
    [at(0), at(1), at(2)]
}

/// Whether a triangle is part of a chamfer strip along a plate edge: flat shaded,
/// long and thin, and facing diagonally between the plates it joins. Smooth
/// surfaces (gun tubes, wheels) have differing vertex normals and are left alone.
fn is_chamfer(p: &[DVec3; 3], face: DVec3, normals: &[[f32; 3]; 3]) -> bool {
    if normals[0] != normals[1] || normals[1] != normals[2] {
        return false;
    }
    let longest = (0..3)
        .map(|i| p[i].distance(p[(i + 1) % 3]))
        .fold(0.0, f64::max);
    let area = (p[1] - p[0]).cross(p[2] - p[0]).length() / 2.0;
    let width = 2.0 * area / longest.max(1e-9);
    longest > EDGE_MIN_LENGTH && width < EDGE_MAX_WIDTH && face.abs().max_element() < EDGE_FACING
}

/// Camouflage coordinates of a model-space point on a face: projected along the
/// face's dominant axis, with v running up the vertical faces so the tile's rain
/// streaks run down the armor.
fn paint_uv(p: DVec3, face: DVec3) -> (f64, f64) {
    let n = face.abs();
    let (u, v) = if n.y >= n.x && n.y >= n.z {
        (p.x, -p.z)
    } else if n.x >= n.z {
        (p.z, p.y)
    } else {
        (p.x, p.y)
    };
    (u * PAINT_UV_PER_METRE, v * PAINT_UV_PER_METRE)
}
