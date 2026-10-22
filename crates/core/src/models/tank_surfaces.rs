//! The vehicle livery: painted armor, bare metal and matte fittings weathered in
//! vertex data and drawn with a few shared materials.
//!
//! Every opaque part of a vehicle model is repainted once when the model is built.
//! Painted armor (team paint and its deep trim) draws with one material per team
//! whose shader paints the clean team color itself (`VEHICLE_PAINT_EFFECT`); the
//! mesh only tells it where the paint wears: each vertex carries how its armor plate
//! is bounded (the distance to the plate's convex edges, which chip and catch a bevel
//! highlight, and to its recesses, which collect grime), the paint's value (a slight
//! shift per plate and a light-to-dark gradient down each upright plate) and how low
//! it sits in the dusty band. Upper panels stay clean; edges chip, the low hull
//! dusts over and recesses darken. A wear tile (`scripts/generate-tank-textures.ts`)
//! shapes the chips and the dust line, projected in model space so it runs on across
//! neighbouring parts. The paint also takes a fill and a team-colored rim of its own,
//! so a hull never crushes to black in shade and its side reads at range.
//! Bare steel and gun metal share a metal material and the rest (rubber edges,
//! canvas, wood, markings, openings) a matte one; the running gear keeps its
//! vertex-colored material with the paint's fill. So each joint of a vehicle costs
//! few draws. Glass and lenses keep their own glossy materials.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use glam::{DMat4, DVec3};

use super::model_primitives::Cache;
use super::tank_model::MARKING_WHITE;
use super::{Team, part};
use crate::geometry::math::hex_to_linear;
use crate::geometry::{Attribute, Buckets, Mesh, VERTEX_ALPHA};
use crate::scene::{Color, Effect, Material, Node, Shading, TextureRef};

/// Service paint per team: a strong cobalt blue and a deep signal red, each one
/// color without a camouflage, so a tank's side reads at a glance. Neither is a
/// pure primary: the green and blue the red keeps (and the red and green the blue
/// keeps) keep the red off magenta in the menu and off maroon from overhead. The
/// blue stays bright enough not to fall to black in shade. In a round the paint
/// shader holds their saturation back under the map's saturating grade.
pub const VEHICLE_PAINT: [u32; 2] = [0x3864ae, 0x8a1f2c];
/// The Humvee's paint per team: the same team hue a step deeper and flatter, a
/// soft-skinned truck's matte finish rather than the tanks' satin armor enamel.
pub const HUMVEE_PAINT: [u32; 2] = [0x3a62a0, 0x7b2129];

/// A team's vehicle body paint.
pub fn vehicle_paint(team: Team) -> u32 {
    VEHICLE_PAINT[team.index()]
}

/// Lamp lenses (headlights of tanks and the Humvee): the livery paints them with
/// the paint effect's lamp mode, a warm glass lens with a hot core that falls off
/// toward a reflector ring, rather than a flat white or cyan square.
pub const LAMP_LENS: u32 = 0xe8dcc0;

/// The worn-metal image under `public/`, also used by the quarry's machinery.
pub const ARMOR_WEAR_TEXTURE: &str = "textures/tanks/armor-wear.webp";
/// The livery's wear tile under `public/` (the file keeps the name of the
/// camouflage it replaced): alpha shapes chips, the grey the dust line and streaks.
pub const ARMOR_PAINT_TEXTURE: &str = "textures/tanks/armor-camo.webp";
/// The name the paint effect knows its wear tile by.
pub const PAINT_WEAR_TILE: &str = "wear";
/// The burnt-out paint of wrecks, on the paint's UVs.
pub const ARMOR_SCORCH_TEXTURE: &str = "textures/tanks/armor-scorch.webp";
/// Burnt paint: dull, rough, a little relief where it blistered.
const SCORCH_ROUGHNESS: f32 = 0.92;
const SCORCH_BUMP_SCALE: f32 = 0.02;
const TEXTURE_ANISOTROPY: u8 = 4;
/// Paint UVs per metre of armor, shared with the wrecks' scorch tile (one tile per
/// 4 m), and how often the wear tile repeats over one UV unit: one tile per 3.6 m,
/// so a chip is a few centimetres across and still reads at the hero camera.
const PAINT_UV_PER_METRE: f64 = 0.25;
const PAINT_REPEAT: f32 = 1.1;
/// Military enamel: satin enough for chamfered edges to catch the sun, rough
/// enough that the sky's sheen never washes the color out.
const PAINT_METALNESS: f32 = 0.08;
const PAINT_ROUGHNESS: f32 = 0.56;
/// The Humvee's flat truck paint (the shader shifts all its paint roughness by
/// the difference from `PAINT_ROUGHNESS`).
const MATTE_PAINT_ROUGHNESS: f32 = 0.74;
/// Bare steel and gun metal.
const METAL_METALNESS: f32 = 0.55;
const METAL_ROUGHNESS: f32 = 0.4;
/// The vehicles' bare steel and gun metal: worn, oily and less mirror-like, with
/// enough diffuse response that housings, lugs and machine guns in shade read as
/// metal rather than as black boxes with a plastic highlight.
const WORN_METAL_METALNESS: f32 = 0.35;
const WORN_METAL_ROUGHNESS: f32 = 0.5;
const METAL_BUMP_SCALE: f32 = 0.02;
/// Rubber, canvas, wood, stencils and dark openings.
const MATTE_ROUGHNESS: f32 = 0.85;
/// Materials smoother than this are glass and lenses and keep their own finish.
const GLOSSY_BELOW: f32 = 0.4;

/// Dry dust and darker wet mud (sRGB) caked on the running gear and fittings; the
/// paint shader has its own for painted armor.
const DUST: u32 = 0x8f7c5c;
const MUD: u32 = 0x4e4031;
/// How much dust tints the lowest fittings, and how far wet mud climbs (a fraction
/// of the dusty band).
const DUST_STRENGTH: f64 = 0.8;
/// Tracks and tyres (already vertex-colored) take little dry dust and a lighter
/// coat of mud than the hull, so wheels, tyres and links stay legible.
const DUST_ON_RUNNING_GEAR: f64 = 0.05;
const MUD_ON_RUNNING_GEAR: f64 = 0.3;
const MUD_STRENGTH: f64 = 0.68;
const MUD_BAND: f64 = 0.55;
/// Darkening at the ground line and on the undersides.
const GROUND_SHADE: f64 = 0.72;
const UNDERSIDE_SHADE: f64 = 0.62;
/// Plate-to-plate paint value variation (fraction either way).
const PLATE_VARIATION: f64 = 0.07;
/// Upright plates shorter than this (chamfers, trim) keep a flat value.
const GRADIENT_MIN_HEIGHT: f64 = 0.05;
/// Upward faces (normal y above `DECK_FACING`) brighten over `DECK_RAMP` metres
/// above the dusty band (see [`PaintValues`]).
const DECK_FACING: f64 = 0.5;
const DECK_RAMP: f64 = 0.6;
/// Every painted face darkens toward the ground: `HEIGHT_FOOT` at the ground
/// line, full value `HEIGHT_SPAN` metres above it, so the hull reads lighter at the
/// turret and heavier at the skirts like a hand-painted model.
const HEIGHT_FOOT: f64 = 0.8;
const HEIGHT_SPAN: f64 = 1.3;
/// Faces leaning less than this (normal y) from vertical take the gradient.
const UPRIGHT: f64 = 0.6;
/// The custom shading of painted armor and running gear
/// (`shaders/effects/vehicle_paint.wgsl`). Params: paint (linear rgb, rim
/// strength), trim (linear rgb, fill strength), rim hue (linear rgb, wear tile
/// repeat), bare steel (linear rgb, 1 for paint or 0 for running gear). The paint's
/// vertex data is described at [`paint_colors`] and [`paint_alpha`]; its UVs are
/// the wear tile's. Running gear keeps its own vertex colors and takes only the fill.
pub const VEHICLE_PAINT_EFFECT: &str = "vehicle-paint";
/// The team rim's strength at a silhouette, and the paint's own fill light.
const RIM_STRENGTH: f32 = 0.6;
const PAINT_FILL: f32 = 0.55;
/// Running gear sits in the hull's shadow: more fill, no rim.
const GEAR_FILL: f32 = 1.6;
/// Bare steel where the paint chipped through (sRGB): bright enough to read as
/// scuffed metal against either team's paint rather than as a dark fleck of dirt.
const BARE_STEEL: u32 = 0x6e6a63;
/// How a livery shades its paint's value across a model.
#[derive(Clone, Copy, Debug)]
struct PaintValues {
    /// The paint's value on the hull and on the turret: the turret a step lighter,
    /// so it stands off the deck from the overhead camera and its facing reads at a
    /// glance, the hull a step deeper beneath it.
    hull: f64,
    turret: f64,
    /// Upward faces range from `deck_low` at the top of the dusty band to
    /// `deck_high` `DECK_RAMP` metres above it.
    deck_low: f64,
    deck_high: f64,
    /// The hand-painted gradient down an upright plate: its value at the plate's
    /// foot and at its top edge.
    gradient_foot: f64,
    gradient_top: f64,
    /// Bare metal in the vehicles' worn finish rather than the machines' polish.
    worn_metal: bool,
}

/// Tanks and the Humvee: a deep hull deck under a lighter turret and a strong
/// light-to-dark gradient down each side, so the turret, the deck and the flanks
/// part by value from the overhead camera and the boxy plates read as forms.
const VEHICLE_VALUES: PaintValues = PaintValues {
    hull: 0.84,
    turret: 1.14,
    deck_low: 0.6,
    deck_high: 1.04,
    gradient_foot: 0.72,
    gradient_top: 1.1,
    worn_metal: true,
};
/// Machines in the vehicles' worn finish (the quarry fleet) keep the gentler
/// shading their map was graded with.
const MACHINE_VALUES: PaintValues = PaintValues {
    hull: 0.86,
    turret: 1.06,
    deck_low: 0.6,
    deck_high: 1.08,
    gradient_foot: 0.8,
    gradient_top: 1.04,
    worn_metal: false,
};
/// Recognition panels and turret bands: matte cloth and paint in a team's signal
/// color, without a glow of their own (a glowing stripe has no source).
const SIGNAL_GLOW: f32 = 0.0;
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

/// The wear tile: data, sampled linearly; the shader applies its repeat.
fn paint_wear_tile() -> TextureRef {
    TextureRef {
        anisotropy: TEXTURE_ANISOTROPY,
        srgb: false,
        ..TextureRef::file(ARMOR_PAINT_TEXTURE)
    }
}

/// The finishes a livery paints with; each is one shared material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Finish {
    /// Painted armor in a team's paint and trim.
    Paint {
        paint: u32,
        trim: u32,
        /// The Humvee's flat finish rather than the tanks' satin enamel.
        matte: bool,
    },
    /// Bare steel and gun metal, `worn` in the vehicles' finish.
    Metal {
        worn: bool,
    },
    Matte,
    /// Recognition panels and turret bands in a team's signal color: matte, kept
    /// apart so wrecks can burn them off.
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
    static WORN_METAL: OnceLock<Arc<Material>> = OnceLock::new();
    static MATTE: OnceLock<Arc<Material>> = OnceLock::new();
    match finish {
        Finish::Paint { paint, trim, matte } => {
            static PAINTS: Cache<(u32, u32, bool), Material> = Cache::new();
            let roughness = if matte {
                MATTE_PAINT_ROUGHNESS
            } else {
                PAINT_ROUGHNESS
            };
            return PAINTS.get_or_insert((paint, trim, matte), || Material {
                effect: Effect::Custom {
                    name: VEHICLE_PAINT_EFFECT,
                    params: paint_params(paint, trim),
                },
                extra_textures: vec![(PAINT_WEAR_TILE, paint_wear_tile())],
                ..white_vertex_material(PAINT_METALNESS, roughness)
            });
        }
        Finish::Metal { worn } => {
            let (cell, metalness, roughness) = if worn {
                (&WORN_METAL, WORN_METAL_METALNESS, WORN_METAL_ROUGHNESS)
            } else {
                (&METAL, METAL_METALNESS, METAL_ROUGHNESS)
            };
            cell.get_or_init(|| {
                let wear = armor_wear_texture();
                Arc::new(Material {
                    map: Some(wear.clone()),
                    bump_map: Some(wear),
                    bump_scale: METAL_BUMP_SCALE,
                    ..white_vertex_material(metalness, roughness)
                })
            })
        }
        Finish::Matte => {
            MATTE.get_or_init(|| Arc::new(white_vertex_material(0.0, MATTE_ROUGHNESS)))
        }
        Finish::Signal(color) => return signal_material(color),
    }
    .clone()
}

/// The vehicle paint effect's params for a team's paint and trim: its rim is the
/// paint's hue at full brightness.
fn paint_params(paint: u32, trim: u32) -> Vec<f32> {
    let hue = hex_to_linear(paint);
    let peak = hue.iter().copied().fold(1e-6, f64::max);
    let mut params = Vec::with_capacity(16);
    params.extend(hue.map(|c| c as f32));
    params.push(RIM_STRENGTH);
    params.extend(hex_to_linear(trim).map(|c| c as f32));
    params.push(PAINT_FILL);
    params.extend(hue.map(|c| (c / peak) as f32));
    params.push(PAINT_REPEAT);
    params.extend(hex_to_linear(BARE_STEEL).map(|c| c as f32));
    params.push(1.0);
    params
}

/// The running gear's material: its own vertex colors and finish with the paint
/// effect's fill, so wheels and links read in the hull's shadow.
fn gear_material(source: &Material) -> Arc<Material> {
    static GEAR: Cache<(u32, u32), Material> = Cache::new();
    GEAR.get_or_insert(
        (source.metalness.to_bits(), source.roughness.to_bits()),
        || {
            let mut params = vec![0.0; 16];
            params[7] = GEAR_FILL;
            params[11] = PAINT_REPEAT;
            Material {
                effect: Effect::Custom {
                    name: VEHICLE_PAINT_EFFECT,
                    params,
                },
                ..source.clone()
            }
        },
    )
}

/// The track belt's scroll wraps after this many model units of travel: every
/// belt holds a whole number of links per period, so the wrap never jumps.
pub const TRACK_BELT_PERIOD: f64 = 2.0;
/// Dark worn track steel: satin enough for the polished faces to catch the light.
const BELT_METALNESS: f32 = 0.4;
const BELT_ROUGHNESS: f32 = 0.6;

/// The track belt's material: the paint effect's belt mode, which draws the links
/// from the belt's UVs and scrolls them `links_per_unit` links per model unit of
/// the tank's travel (fed per instance), with the running gear's fill.
pub fn track_belt_material(links_per_unit: f64) -> Arc<Material> {
    static BELTS: Cache<u64, Material> = Cache::new();
    BELTS.get_or_insert(links_per_unit.to_bits(), || {
        let mut params = vec![0.0; 16];
        params[0] = links_per_unit as f32;
        params[7] = GEAR_FILL;
        params[11] = PAINT_REPEAT;
        params[15] = 3.0;
        Material {
            effect: Effect::Custom {
                name: VEHICLE_PAINT_EFFECT,
                params,
            },
            ..white_vertex_material(BELT_METALNESS, BELT_ROUGHNESS)
        }
    })
}

/// Window and sight glass in `tint` (sRGB): the paint effect's glass mode, a dark
/// tint that mirrors the sky toward grazing angles.
pub fn vehicle_glass(tint: u32) -> Arc<Material> {
    static GLASS: Cache<u32, Material> = Cache::new();
    GLASS.get_or_insert(tint, || {
        let mut params = vec![0.0; 16];
        for (i, c) in hex_to_linear(tint).into_iter().enumerate() {
            params[i] = c as f32;
        }
        params[11] = PAINT_REPEAT;
        params[15] = 2.0;
        Material {
            effect: Effect::Custom {
                name: VEHICLE_PAINT_EFFECT,
                params,
            },
            ..Material::standard(tint, 0.0, 0.08)
        }
    })
}

/// A sooted wreck's stand-in for vehicle glass: the tint as plain, dull paint, so
/// it batches with the wreck's other parts and mirrors no sky. `None` for any
/// other material.
pub fn sooted_glass(material: &Material) -> Option<Arc<Material>> {
    let Effect::Custom { name, params } = &material.effect else {
        return None;
    };
    (*name == VEHICLE_PAINT_EFFECT && params.get(15) == Some(&2.0))
        .then(|| super::model_primitives::material(material.color.0, 0.0, 0.6))
}

/// The recognition material of a team's signal color: matte cloth.
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
    matches!(&material.effect, Effect::Custom { name, params }
        if *name == VEHICLE_PAINT_EFFECT && params.get(15) == Some(&1.0))
}

/// Give a painted mesh plain paint colors again (the team paint or trim at the
/// vertex's value) in place of the paint shader's edge data, for a wreck that burns
/// it with a material of its own. `material` is the paint material it wore.
pub fn unpaint(mesh: &mut Mesh, material: &Material) {
    let Effect::Custom { params, .. } = &material.effect else {
        return;
    };
    let paint = [params[0], params[1], params[2]];
    let trim = [params[4], params[5], params[6]];
    for color in &mut mesh.colors {
        let (base, value) = paint_value_decoded(color[2]);
        let rgb = if base { paint } else { trim };
        *color = rgb.map(|c| c * value);
    }
}

/// The scorched material of a wreck's burnt paint: the scorch tile over a
/// vertex-colored ash base, on the paint's UVs.
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
        && material.effect == Effect::None
}

/// The livery of one vehicle: its painted colors (team paint and trim), the
/// colors that are bare metal, its recognition panel's color and its grime band.
pub struct Livery<'a> {
    pub paint: &'a [u32],
    /// Flat truck paint (the Humvee) rather than satin armor enamel.
    pub matte: bool,
    pub metal: &'a [u32],
    pub signal: u32,
    pub grime: Grime,
}

impl Livery<'_> {
    /// The finish a part's material takes, or `None` to leave it alone.
    fn finish(&self, material: &Material) -> Option<Finish> {
        let color = material.color.0;
        if !is_opaque_standard(material) || material.vertex_colors {
            return None;
        }
        if material.roughness < GLOSSY_BELOW {
            return None;
        }
        Some(
            if self.paint.contains(&color) || color == MARKING_WHITE || color == LAMP_LENS {
                // Trim, markings and lamp lenses share the team paint's material:
                // one painted material per team keeps a joint's paint in one draw,
                // and markings wear through to the paint under them.
                Finish::Paint {
                    paint: self.paint[0],
                    trim: self.paint.get(1).copied().unwrap_or(self.paint[0]),
                    matte: self.matte,
                }
            } else if color == self.signal {
                Finish::Signal(color)
            } else if self.metal.contains(&color) || material.metalness >= 0.45 {
                Finish::Metal { worn: false }
            } else {
                Finish::Matte
            },
        )
    }
}

/// Opaque standard shading without an effect or cut-out, and not an ember.
fn is_opaque_standard(material: &Material) -> bool {
    material.shading == Shading::Standard
        && material.effect == Effect::None
        && !material.transparent
        && material.alpha_test == 0.0
        && material.emissive_map.is_none()
        && !std::ptr::eq(material, &*ember_material())
}

/// Repaint every opaque part of `root` (a vehicle model in its own frame; the
/// root's transform is ignored) with the livery's weathered finishes. Parts that
/// are already vertex-colored (tracks, tyres) keep their colors and finish, gain
/// the grime and take the running gear's fill.
pub fn apply_livery(root: &mut Node, livery: &Livery) {
    apply_livery_shaded(root, livery, &MACHINE_VALUES);
}

/// [`apply_livery`] for a tank or the Humvee, with the vehicles' value hierarchy.
pub fn apply_vehicle_livery(root: &mut Node, livery: &Livery) {
    apply_livery_shaded(root, livery, &VEHICLE_VALUES);
}

fn apply_livery_shaded(root: &mut Node, livery: &Livery, values: &PaintValues) {
    let mut weathered_meshes = HashMap::new();
    for child in &mut root.children {
        repaint(
            child,
            DMat4::IDENTITY,
            livery,
            values,
            values.hull,
            &mut weathered_meshes,
        );
    }
}

/// A shared part mesh placed at the same height and tilt (the Humvee's four
/// wheels) weathers the same, so its copies keep sharing one weathered mesh:
/// the source mesh and material, and the world matrix row that gives height.
/// The paint's value lift is part of the key: a turret part never shares a hull
/// part's weathered mesh.
type WeatherKey = (usize, usize, [i64; 5]);

fn weather_key(mesh: &Arc<Mesh>, material: &Arc<Material>, world: DMat4, lift: f64) -> WeatherKey {
    let row = world.row(1);
    let quantize = |value: f64| (value * 1e4).round() as i64;
    (
        Arc::as_ptr(mesh) as usize,
        Arc::as_ptr(material) as usize,
        [row.x, row.y, row.z, row.w, lift].map(quantize),
    )
}

fn repaint(
    node: &mut Node,
    parent: DMat4,
    livery: &Livery,
    values: &PaintValues,
    lift: f64,
    weathered_meshes: &mut HashMap<WeatherKey, (Arc<Mesh>, Arc<Material>)>,
) {
    let world = parent * node.local_matrix();
    let lift = if node.name == part::TURRET {
        values.turret
    } else {
        lift
    };
    if let Some(drawable) = &mut node.drawable
        && drawable.instances.is_none()
    {
        let finish = livery
            .finish(&drawable.material)
            .map(|finish| match finish {
                Finish::Metal { .. } => Finish::Metal {
                    worn: values.worn_metal,
                },
                other => other,
            });
        let regrimed = drawable.material.vertex_colors
            && !drawable.mesh.colors.is_empty()
            && is_opaque_standard(&drawable.material);
        if finish.is_some() || regrimed {
            let key = weather_key(&drawable.mesh, &drawable.material, world, lift);
            let (mesh, material) = weathered_meshes
                .entry(key)
                .or_insert_with(|| match finish {
                    Some(finish) => (
                        Arc::new(weathered(
                            &drawable.mesh,
                            world,
                            Some(drawable.material.color.0),
                            finish,
                            livery.grime,
                            values,
                            lift,
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
                            values,
                            1.0,
                        )),
                        gear_material(&drawable.material),
                    ),
                })
                .clone();
            drawable.mesh = mesh;
            drawable.material = material;
        }
    }
    for child in &mut node.children {
        repaint(child, world, livery, values, lift, weathered_meshes);
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

/// Dust and mud in linear RGB, converted once rather than for every vertex.
fn grime_colors() -> &'static [[f64; 3]; 2] {
    static COLORS: OnceLock<[[f64; 3]; 2]> = OnceLock::new();
    COLORS.get_or_init(|| [hex_to_linear(DUST), hex_to_linear(MUD)])
}

/// Dust and mud on a fitting or the running gear at model-space `p`.
fn grimed(base: [f64; 3], p: DVec3, n: DVec3, grime: Grime, dust_scale: f64) -> [f64; 3] {
    let [dust, mud] = *grime_colors();
    let height = band_height(p, grime);
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
    color.map(|c| c * ground_shade(height, n))
}

/// Upward faces lighten with height above the dusty band, so from overhead the
/// turret roof stands off the deck and the deck off the fenders.
fn deck_value(p: DVec3, n: DVec3, grime: Grime, values: &PaintValues) -> f64 {
    if n.y <= DECK_FACING {
        return 1.0;
    }
    let rise = smoothstep(grime.dust_top, grime.dust_top + DECK_RAMP, p.y);
    values.deck_low + (values.deck_high - values.deck_low) * rise
}

/// The paint's value by height above the ground line.
fn height_value(p: DVec3, grime: Grime) -> f64 {
    HEIGHT_FOOT + (1.0 - HEIGHT_FOOT) * smoothstep(grime.ground, grime.ground + HEIGHT_SPAN, p.y)
}

/// Darker at the ground line and on undersides.
fn ground_shade(height: f64, n: DVec3) -> f64 {
    let mut shade = GROUND_SHADE + (1.0 - GROUND_SHADE) * smoothstep(0.0, 1.6, height);
    if n.y < 0.0 {
        shade *= 1.0 + (UNDERSIDE_SHADE - 1.0) * -n.y;
    }
    shade
}

/// How high a model-space point sits in the dusty band: 0 at the ground, 1 at its
/// top and above.
fn band_height(p: DVec3, grime: Grime) -> f64 {
    ((p.y - grime.ground) / (grime.dust_top - grime.ground)).clamp(0.0, 1.0)
}

/// The vertex alpha the paint shader reads: 0.2 plus 0.3 x how low the vertex sits
/// in the dusty band, raised by 0.4 on a chamfer strip. A chamfer flags whole
/// triangles, so interpolation never mixes the two.
fn paint_alpha(p: DVec3, grime: Grime, edge: bool) -> f32 {
    let flag = if edge { 0.4 } else { 0.0 };
    (0.2 + flag + 0.3 * (1.0 - band_height(p, grime))) as f32
}

/// The paint value's code in the blue channel: trim in [0, 0.5), paint in
/// [0.5, 1], each holding a value up to `VALUE_RANGE`.
const VALUE_RANGE: f64 = 1.45;

fn paint_value_code(paint: bool, value: f64) -> f32 {
    let code = (value / VALUE_RANGE).clamp(0.0, 0.998) * 0.5;
    (if paint { 0.5 + code } else { code }) as f32
}

/// What a painted vertex wears: the team paint or trim (`paint_value_code`), the
/// markings' white (the code plus 1) or a lamp lens (plus 2). Markings and lenses
/// draw in the paint's own material, so they cost no draw of their own and wear
/// with the paint: the white flakes to the team color under it and takes the dust.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Coat {
    Paint,
    Trim,
    Marking,
    Lamp,
}

fn coat_code(coat: Coat, value: f64) -> f32 {
    match coat {
        Coat::Paint => paint_value_code(true, value),
        Coat::Trim => paint_value_code(false, value),
        Coat::Marking => 1.0 + paint_value_code(true, value),
        Coat::Lamp => 2.0 + paint_value_code(true, value),
    }
}

/// Whether a code is the team paint (rather than trim; markings and lamps count as
/// paint), and its value.
fn paint_value_decoded(code: f32) -> (bool, f32) {
    let code = code.fract();
    let paint = code >= 0.5;
    let fraction = if paint { code - 0.5 } else { code };
    (paint, fraction * 2.0 * VALUE_RANGE as f32)
}

/// A non-indexed copy of `mesh` moved into the model frame for weathering (the
/// stored positions stay in the part's frame). Paint gets the paint shader's
/// vertex data and model-space wear UVs (`base` is the part's paint color); other
/// finishes get weathered vertex colors (`None` weathers the mesh's own colors).
/// `values` shade the paint and `lift` scales its value (turret or hull).
fn weathered(
    mesh: &Mesh,
    world: DMat4,
    base: Option<u32>,
    finish: Finish,
    grime: Grime,
    values: &PaintValues,
    lift: f64,
) -> Mesh {
    let mut out = mesh.to_non_indexed();
    let count = out.positions.len();
    let own_colors = std::mem::take(&mut out.colors);
    let mut colors = Vec::with_capacity(count);
    let mut alphas = Vec::with_capacity(if finish.is_paint() { count } else { 0 });
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
    let flat: Vec<bool> = (0..faces.len())
        .map(|t| {
            let n = mesh_normals(&out, t);
            n[0] == n[1] && n[1] == n[2]
        })
        .collect();
    let plates = finish.is_paint().then(|| Plates::new(&faces, &flat));
    let (heights, edges) = match &plates {
        Some(plates) => (plates.heights(&faces), plate_edges(&faces, &flat, plates)),
        None => (Vec::new(), Vec::new()),
    };
    let base_linear = base.map(hex_to_linear);
    let coat = match (finish, base) {
        (_, Some(MARKING_WHITE)) => Coat::Marking,
        (_, Some(LAMP_LENS)) => Coat::Lamp,
        (Finish::Paint { trim, paint, .. }, Some(base)) if base == trim && trim != paint => {
            Coat::Trim
        }
        _ => Coat::Paint,
    };
    for (t, &(p, face)) in faces.iter().enumerate() {
        let root = plates.as_ref().map_or(t, |plates| plates.roots[t]);
        let gradient = |y: f64| match heights.get(root).copied().flatten() {
            Some((foot, top)) if top - foot >= GRADIENT_MIN_HEIGHT => {
                values.gradient_foot
                    + (values.gradient_top - values.gradient_foot) * (y - foot) / (top - foot)
            }
            _ => 1.0,
        };
        let chamfer = finish.is_paint() && flat[t] && is_chamfer(&p, face);
        for (k, point) in p.iter().enumerate() {
            let index = t * 3 + k;
            if finish.is_paint() {
                let (q, n) = faces[root];
                let value = lift
                    * (1.0 + PLATE_VARIATION * plate_variation(plate_key(n, q[0])))
                    * gradient(point.y)
                    * deck_value(*point, face, grime, values)
                    * height_value(*point, grime)
                    * ground_shade(band_height(*point, grime), face);
                // Markings lie on their plate: their own outline marks no edge.
                let [r, g] = if coat == Coat::Marking {
                    [NO_EDGES; 2]
                } else {
                    edges[index]
                };
                colors.push([r, g, coat_code(coat, value)]);
                let (u, v) = paint_uv(*point, face);
                out.uvs[index] = [u as f32, v as f32];
                alphas.push(paint_alpha(*point, grime, chamfer));
            } else {
                let (paint, dust_scale) = match base_linear {
                    Some(linear) => (linear, 1.0),
                    None => (
                        own_colors.get(index).map_or([1.0; 3], |c| c.map(f64::from)),
                        DUST_ON_RUNNING_GEAR,
                    ),
                };
                let color = grimed(paint, *point, face, grime, dust_scale);
                colors.push(color.map(|c| c as f32));
            }
        }
    }
    out.colors = colors;
    if finish.is_paint() {
        out.set_attribute(Attribute::vertex(VERTEX_ALPHA, 1, alphas));
    }
    out
}

/// The stored normals of triangle `t`.
fn mesh_normals(mesh: &Mesh, t: usize) -> [[f32; 3]; 3] {
    let at = |k: usize| mesh.normals.get(t * 3 + k).copied().unwrap_or([0.0; 3]);
    [at(0), at(1), at(2)]
}

/// Whether a flat triangle is part of a chamfer strip along a plate edge: long and
/// thin, and facing diagonally between the plates it joins.
fn is_chamfer(p: &[DVec3; 3], face: DVec3) -> bool {
    let longest = (0..3)
        .map(|i| p[i].distance(p[(i + 1) % 3]))
        .fold(0.0, f64::max);
    let area = (p[1] - p[0]).cross(p[2] - p[0]).length() / 2.0;
    let width = 2.0 * area / longest.max(1e-9);
    longest > EDGE_MIN_LENGTH && width < EDGE_MAX_WIDTH && face.abs().max_element() < EDGE_FACING
}

/// Paint coordinates of a model-space point on a face: projected along the face's
/// dominant axis, with v running up the vertical faces so the tile's streaks run
/// down the armor.
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

/// What borders an armor plate along one of its sides, as the paint shader reads
/// it: nothing it marks, a convex edge (chips, bevel highlight) or a recess (grime).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Border {
    Plain = 0,
    Edge = 1,
    Recess = 2,
}

/// The paint shader's edge data: each of the red and green channels describes a
/// pair of parallel border lines a triangle lies between (or one line, with a
/// plain far side), as `state + EDGE_MARGIN + EDGE_SPAN * t`. `t` runs from 0 on
/// the near line to 1 on the far one and is affine across the triangle, so the
/// shader recovers both distances in metres from the channel's screen-space
/// gradient; the integer `state` is the near border plus 3 x the far border.
/// Triangles without borders (smooth surfaces) hold `NO_EDGES`, which has no
/// gradient and so no edge anywhere.
const EDGE_MARGIN: f64 = 0.04;
const EDGE_SPAN: f64 = 0.92;
const NO_EDGES: f32 = 0.5;
/// Corners closer than this are the same point; a vertex this close to a border
/// segment lies on it.
const POINT_TOLERANCE: f64 = 1e-4;
/// Neighbouring faces closer in direction than this (cosine; about 12 degrees)
/// continue one surface rather than meeting at an edge.
const CONTINUOUS: f64 = 0.978;

type PointKey = [i64; 3];

fn point_key(p: DVec3) -> PointKey {
    [p.x, p.y, p.z].map(|v| (v / POINT_TOLERANCE).round() as i64)
}

/// One side of a plate: a triangle edge no other triangle of the plate shares.
struct Segment {
    a: DVec3,
    b: DVec3,
    border: Border,
}

/// A border line a triangle touches: its unit normal into the plate and offset
/// (`inward · p - offset` is the distance from it), its border, how many of the
/// triangle's corners lie on its segment and their mean distance from it.
struct Line {
    inward: DVec3,
    offset: f64,
    border: Border,
    touches: usize,
    mean: f64,
}

impl Line {
    fn distance(&self, p: DVec3) -> f64 {
        self.inward.dot(p) - self.offset
    }
}

fn distance_to_segment(q: DVec3, a: DVec3, dir: DVec3, length: f64) -> f64 {
    let t = dir.dot(q - a).clamp(0.0, length);
    q.distance(a + dir * t)
}

fn find_root(parent: &mut [usize], mut t: usize) -> usize {
    while parent[t] != t {
        parent[t] = parent[parent[t]];
        t = parent[t];
    }
    t
}

/// A triangle edge by its corners, in either order.
type EdgeKey = (PointKey, PointKey);

fn edge_key(a: PointKey, b: PointKey) -> EdgeKey {
    if a <= b { (a, b) } else { (b, a) }
}

/// The armor plates of a mesh: flat triangles joined by an edge within a few
/// degrees of each other (a plate and its gently faceted continuation) share a
/// root triangle.
struct Plates {
    keys: Vec<[PointKey; 3]>,
    /// Triangle sides (side `i` of triangle `t` is item `3 * t + i`, running from
    /// corner `i`) grouped by the edge they lie on.
    sides: Buckets,
    roots: Vec<usize>,
}

impl Plates {
    fn new(faces: &[([DVec3; 3], DVec3)], flat: &[bool]) -> Self {
        let keys: Vec<[PointKey; 3]> = faces.iter().map(|(p, _)| p.map(point_key)).collect();
        let sides = Buckets::new(
            keys.iter()
                .flat_map(|k| (0..3).map(move |i| edge_key(k[i], k[(i + 1) % 3]))),
        );
        let mut parent: Vec<usize> = (0..faces.len()).collect();
        for edge in sides.groups() {
            for (i, &a) in edge.iter().enumerate() {
                for &b in &edge[i + 1..] {
                    let (a, b) = (a / 3, b / 3);
                    if flat[a] && flat[b] && faces[a].1.dot(faces[b].1) > CONTINUOUS {
                        // The lower root wins, so a plate's root is its first
                        // triangle whatever order the edges come in.
                        let (ra, rb) = (find_root(&mut parent, a), find_root(&mut parent, b));
                        parent[ra.max(rb)] = ra.min(rb);
                    }
                }
            }
        }
        let roots = (0..faces.len())
            .map(|t| find_root(&mut parent, t))
            .collect();
        Self { keys, sides, roots }
    }

    /// The triangles on the edge of side `i` of triangle `t`, in order.
    fn side(&self, t: usize, i: usize) -> impl Iterator<Item = usize> + Clone + '_ {
        self.sides.with(3 * t + i).iter().map(|side| side / 3)
    }

    /// The height range of every upright plate, by root (`None` for other roots).
    fn heights(&self, faces: &[([DVec3; 3], DVec3)]) -> Vec<Option<(f64, f64)>> {
        let mut heights = vec![None; faces.len()];
        for (t, (p, face)) in faces.iter().enumerate() {
            if face.y.abs() >= UPRIGHT {
                continue;
            }
            let range = heights[self.roots[t]].get_or_insert((f64::INFINITY, f64::NEG_INFINITY));
            for corner in p {
                range.0 = range.0.min(corner.y);
                range.1 = range.1.max(corner.y);
            }
        }
        heights
    }
}

/// The paint shader's edge channels for every corner of `faces` (corners, normal),
/// `flat` marking the flat-shaded triangles that form plates. Coplanar flat
/// triangles joined by an edge make a plate; its sides are convex edges where the
/// neighbouring face falls away, recesses where it rises (and along the open foot of
/// an upright plate, which stands on something), plain otherwise.
fn plate_edges(faces: &[([DVec3; 3], DVec3)], flat: &[bool], plates: &Plates) -> Vec<[f32; 2]> {
    let count = faces.len();
    let Plates { keys, roots, .. } = plates;
    // Per plate root: its lowest corner, and its border segments.
    let mut feet = vec![f64::INFINITY; count];
    for (t, (p, _)) in faces.iter().enumerate() {
        let low = p.iter().map(|q| q.y).fold(f64::INFINITY, f64::min);
        let foot = &mut feet[roots[t]];
        *foot = foot.min(low);
    }
    let mut sides: Vec<Vec<Segment>> = (0..count).map(|_| Vec::new()).collect();
    for t in (0..count).filter(|&t| flat[t]) {
        let (p, normal) = faces[t];
        for i in 0..3 {
            let (j, (a, b)) = ((i + 1) % 3, (p[i], p[(i + 1) % 3]));
            let triangles = plates.side(t, i);
            if triangles.clone().any(|u| u != t && roots[u] == roots[t]) {
                continue;
            }
            let mut border = Border::Plain;
            let mut open = true;
            for u in triangles.filter(|&u| u != t) {
                open = false;
                let Some(k) =
                    (0..3).find(|&k| keys[u][k] != keys[t][i] && keys[u][k] != keys[t][j])
                else {
                    continue;
                };
                // A neighbour within a few degrees of the plate continues it (a
                // gently faceted surface), so the seam between them marks nothing.
                if normal.dot(faces[u].1) > CONTINUOUS {
                    break;
                }
                let rise = normal.dot(faces[u].0[k] - a);
                if rise < -1e-5 {
                    border = Border::Edge;
                    break;
                }
                if rise > 1e-5 {
                    border = Border::Recess;
                    break;
                }
            }
            if open {
                border = Border::Edge;
            }
            // The level foot of an upright plate stands on something (a deck, a
            // box's own floor): it gathers grime rather than chipping.
            let upright = normal.y.abs() < UPRIGHT;
            let level = (b.y - a.y).abs() < 0.3 * a.distance(b);
            let foot = (a.y + b.y) / 2.0 < feet[roots[t]] + 0.01;
            if border == Border::Edge && upright && level && foot {
                border = Border::Recess;
            }
            if border != Border::Plain {
                sides[roots[t]].push(Segment { a, b, border });
            }
        }
    }
    let mut out = vec![[NO_EDGES; 2]; count * 3];
    for t in (0..count).filter(|&t| flat[t]) {
        let segments = &sides[roots[t]];
        if segments.is_empty() {
            continue;
        }
        let lines = border_lines(&faces[t], segments);
        let mut used = vec![false; lines.len()];
        let mut channel = 0;
        for i in 0..lines.len() {
            if channel == 2 {
                break;
            }
            if used[i] {
                continue;
            }
            used[i] = true;
            let near = &lines[i];
            let far = (0..lines.len())
                .find(|&j| !used[j] && lines[j].inward.dot(near.inward) < -0.9999)
                .inspect(|&j| used[j] = true)
                .map(|j| &lines[j]);
            let p = faces[t].0;
            let (width, far_border) = match far {
                Some(far) => (near.distance(p[0]) + far.distance(p[0]), far.border),
                None => (
                    p.iter().map(|q| near.distance(*q)).fold(0.0, f64::max),
                    Border::Plain,
                ),
            };
            if width < 1e-6 {
                continue;
            }
            let state = f64::from(near.border as u8 + 3 * far_border as u8);
            for (k, q) in p.iter().enumerate() {
                let along = (near.distance(*q) / width).clamp(0.0, 1.0);
                out[t * 3 + k][channel] = (state + EDGE_MARGIN + EDGE_SPAN * along) as f32;
            }
            channel += 1;
        }
    }
    out
}

/// The border lines of a plate's `segments` that the triangle `face` touches, the
/// ones running along its own edges first, then the nearest.
fn border_lines(face: &([DVec3; 3], DVec3), segments: &[Segment]) -> Vec<Line> {
    let (p, normal) = *face;
    let centroid = (p[0] + p[1] + p[2]) / 3.0;
    let mut lines: Vec<Line> = Vec::new();
    for segment in segments {
        let along = segment.b - segment.a;
        let length = along.length();
        if length < 1e-6 {
            continue;
        }
        let dir = along / length;
        let touches = p
            .iter()
            .filter(|q| distance_to_segment(**q, segment.a, dir, length) < POINT_TOLERANCE)
            .count();
        if touches == 0 {
            continue;
        }
        let mut inward = normal.cross(dir).normalize_or_zero();
        if inward.dot(centroid - segment.a) < 0.0 {
            inward = -inward;
        }
        let offset = inward.dot(segment.a);
        let distances = p.map(|q| inward.dot(q) - offset);
        // A line that cuts the triangle (a reflex corner) marks no side of it.
        if distances.iter().any(|&d| d < -POINT_TOLERANCE) {
            continue;
        }
        let mean = distances.iter().sum::<f64>() / 3.0;
        if let Some(line) = lines.iter_mut().find(|line| {
            line.inward.dot(inward) > 0.9999 && (line.offset - offset).abs() < POINT_TOLERANCE
        }) {
            line.touches = line.touches.max(touches);
            if segment.border == Border::Edge {
                line.border = Border::Edge;
            }
            continue;
        }
        lines.push(Line {
            inward,
            offset,
            border: segment.border,
            touches,
            mean,
        });
    }
    lines.sort_by(|x, y| y.touches.cmp(&x.touches).then(x.mean.total_cmp(&y.mean)));
    lines
}
