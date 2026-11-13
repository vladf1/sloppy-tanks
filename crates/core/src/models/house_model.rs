//! The village cottage, built to its [`HouseDesign`]: a fieldstone plinth under
//! painted clapboard or brick walls whose windows and door are real openings
//! (recessed double-hung sashes, louvred shutters where they fit, a panelled door
//! under a bracketed canopy), corner boards and a water table, and a gable or hip
//! roof with overhangs, fascia, stepped shingle courses or standing seams, ridge and
//! hip caps, gutters and downspouts, plus a corbelled brick chimney with a clay pot.
//! Window boxes carry soil, shrubs and flowers.
//!
//! The cover's box (`w` along x, `d` along z, `h` to the ridge) is the collision
//! footprint; only eaves, sills, steps and planters reach past the walls. In the
//! build frame the ridge runs along z and the door is in the +z gable wall or the
//! +x eave wall, turned to face the street.
//!
//! Some cottages have a window boarded over. The arena's cottages are drawn part by
//! part ([`cottage_pieces`]): each of the [`HousePart`]s they are built from apart,
//! its walls lined inside with plaster, lath and framing as a broken wall shows them.
//!
//! Every finish but the textured walls, stone, brick and shingles shares one satin
//! class (glass stays glossy), so a standing cottage costs at most six draws.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::{DMat4, DVec2, DVec3};

use super::building_kit;
use super::building_kit::{
    BRICK_TEXTURE, CASING, Glazing, Opening, Palette, STUCCO_AVERAGE, STUCCO_TEXTURE, Surface,
    door, opening_casing, shutter_width, shutters, surface_material, textured, tint_over,
    uv_per_metre, window,
};
use super::house_surfaces::{HouseSurface, house_material};
use super::model_primitives::{Cache, material, put, shadowed};
use super::quarry_surfaces::SANDSTONE_TEXTURE;
use super::tank_kit::{Kit, KitMeshes, aim, pose, shift};
use crate::geometry::math::scale_hex_color;
use crate::geometry::{Mesh, octahedron_geometry};
use crate::scene::{Color, Material, Node, TextureRef};
use crate::sim::house_layout::{
    BEAN_POLE, BEAN_SPREAD, CHIMNEY, DOOR_HEIGHT, GARAGE_HEIGHT, GARAGE_PAIR, GarageLook,
    GaragePaint, GardenBed, HouseDesign, HousePart, PLINTH, RAFTER_DEPTH, Roof, RoofCover,
    RoofForm, SENDA_BRICK, Shutters, Solar, Veneer, WING_OVERHANG, WallFinish, Wing,
};
use crate::sim::math::Random;

pub use crate::sim::house_layout::{chimney_flue, cottage_turns, door_direction};

/// Depth of the window and door reveals behind the wall face.
const REVEAL: f64 = 0.14;
/// Corner boards: width on each face and how far they stand proud.
const CORNER_BOARD: f64 = 0.13;
const TRIM_PROUD: f64 = 0.028;
const WINDOW_WIDTH: f64 = 0.85;
const DOOR_WIDTH: f64 = 0.9;
/// Clear wall kept between neighbouring casings and shutters.
const TRIM_GAP: f64 = 0.02;
/// Shutters narrower than this are left off rather than squeezed.
const MIN_SHUTTER: f64 = 0.3;
/// Shingle exposure the courses aim for, and courses per shingle tile.
const COURSE: f64 = 0.24;
const SHINGLE_ROWS: f64 = 8.0;
/// How far shingles run past the deck at the eave.
const SHINGLE_EAVE: f64 = 0.04;
/// Lift of a course's butt and of its head above the deck: each butt stands
/// proud of the course below it and casts the shadow line real shingles do.
const BUTT_LIFT: f64 = 0.024;
const HEAD_LIFT: f64 = 0.006;
/// Ridge cap: half width across the ridge and the length of one cap shingle.
const RIDGE_HALF: f64 = 0.16;
const RIDGE_PIECE: f64 = 0.3;
/// Standing seams: spacing, width and height of the ribs.
const SEAM_SPACING: f64 = 0.46;
const SEAM: (f64, f64) = (0.035, 0.04);
/// Half width of a dormer's front.
const DORMER_HALF: f64 = 0.62;

/// Leafy clumps and flowers in each window box.
/// The kitchen garden behind a cottage: the longest its bed runs along the back
/// wall and how deep it is (m), how far out from the wall it starts, the
/// cabbages' spacing along a row, the bean poles' height and the cabbages' green.
const GARDEN_SPACING: f64 = 0.55;
const CABBAGE: u32 = 0x6f9257;
const PLANT_CLUMPS: u32 = 4;
const PLANT_BLOOMS: u32 = 5;
const LEAVES: u32 = 0x3d6b3a;
const LEAVES_LIGHT: u32 = 0x5a8443;
const FLOWERS: [u32; 4] = [0xd9837a, 0xf0cf6a, 0xb59ad0, 0xf2efe6];
/// The shared satin class: every untextured finish but glass draws as one.
const SATIN: (f64, f64) = (0.0, 0.6);
/// Painted stucco walls: how deep their bumps' relief reads, their roughness, and how
/// many times the bump texture repeats across one tile of the walls' coordinates, so
/// a bump is half a centimetre across.
const STUCCO: (f32, f32) = (0.01, 0.92);
const STUCCO_REPEAT: f32 = 4.0;
/// Freshly split timber, pale and warm where the light catches its ends.
const FRESH_SPLIT: u32 = 0xe2bf86;
/// The plaster inside a cottage's walls, which a fallen panel shows: old lime
/// plaster, dulled by smoke and the dust of the fall but lit like any face, so a
/// panel lying lining up reads as the inside of a room, never as a black card.
const PLASTER: u32 = 0x7b6d58;
/// Height of the gravel drip strip round a cottage's plinth.
const DRIP_STRIP: f64 = 0.035;
/// The dark chocolate stain of a stucco house's timber.
const TRACT_STAIN: u32 = 0x4a3326;
/// A tract house's garage door, stained a lighter milk chocolate than its timber (the
/// tract's taupe-brown sectional doors), the plantation shutters behind some of its
/// windows' white vinyl frames (an off-white held below the walls, as blinds sit in the
/// shade of their reveal), its board-and-batten cladding's grey sage and the tint of its stacked stone over the
/// sandstone texture (a cooler buff than the quarry's raw rock).
const TRACT_GARAGE_STAIN: u32 = 0x7a604e;
/// The garage doors of the tract's paler houses: a cream house's a lighter tan stain, a
/// white house's painted off-white, as the street's doors range from taupe-brown to white.
const CREAM_GARAGE_STAIN: u32 = 0x957a60;
const WHITE_GARAGE_PAINT: u32 = 0xd4ccbc;
const PLANTATION_BLINDS: u32 = 0xb8b4aa;
const TRACT_CLADDING: u32 = 0x8a937f;
const TRACT_STONE: u32 = 0xe4e6ec;
/// How much wall (m) one tile of the sandstone texture covers as a tract house's stacked
/// stone: its layers read as courses a few centimetres high.
const LEDGESTONE_TILE: f64 = 0.75;
/// Flagstones in the path from a cottage's door toward its street.
const PATH_STONES: usize = 4;
/// A garage's driveway: how far it reaches past the door's sides, where its apron
/// meets the ground and how high it lies on it there, and how far it runs out (m).
const DRIVEWAY: (f64, f64, f64, f64) = (0.3, 1.4, 0.03, 4.5);
/// The bare earth round a cottage's plinth: how far it reaches out from the walls
/// (m, least and most), how often its ragged edge turns (m) and its lift over the
/// ground.
const SPLASH: (f64, f64) = (0.3, 0.62);
const SPLASH_STEP: f64 = 0.9;
const SPLASH_LIFT: f64 = 0.008;
/// Wallpapers a fallen wall shows inside (sage, duck-egg, rose, buttercup), and
/// how far they cover the plaster's own dull colour.
const WALLPAPERS: [u32; 4] = [0x9fb48c, 0x8fb0be, 0xc89a8c, 0xd2bd7c];
const WALLPAPER_TINT: f64 = 0.24;
/// Foundation shrubs: their greens (the window boxes', so they share those
/// draws), how many leafy lumps each is and how big (m) the main lump is.
const SHRUB_LEAVES: [u32; 2] = [LEAVES, LEAVES_LIGHT];
const SHRUB_LUMPS: usize = 3;
const SHRUB_SIZE: (f64, f64) = (0.26, 0.36);
/// Weathering up a standing cottage's walls: how dark the siding is at the plinth
/// and how much it brightens to the ridge, linear in height so the vertex
/// colours interpolate exactly however a wall is triangulated.
const GRIME: (f32, f32) = (0.7, 0.3);
/// Share of a cottage's side and back windows boarded over.
const BOARDED_WINDOWS: f64 = 0.14;
/// Thickness of the boards nailed over a boarded window.
const PLANK: f64 = 0.05;
/// Patches of plaster fallen off each wall part's lining.
const PLASTER_FALLS: usize = 14;
/// A standing shingle roof's weathering (linear shade at the eave and at the
/// ridge) and the light its ridge caps catch.
const ROOF_WEATHER: (f64, f64) = (0.64, 1.04);
const RIDGE_SHINE: f64 = 1.32;

/// How much of a cottage a build makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Build {
    /// The standing cottage.
    Standing,
    /// Each part apart, lined inside as a broken wall shows it, with the plinth and
    /// the yard kept with the ground: the structural cottage's parts are cut from it.
    Parts,
}

/// Where the parts of a cottage go while it is built: the standing model keeps its
/// roof (and chimney) apart from the walls; the parts build gives each part its own
/// kit.
struct Parts {
    split: bool,
    kits: Vec<Kit<Surface>>,
    courses: Vec<Courses>,
    nodes: Vec<Vec<Node>>,
    /// Each wall's openings as `[u0, y0, u1, y1]` in its frame, where a broken part's
    /// torn edges leave a gap.
    openings: [Vec<[f64; 4]>; 4],
    /// The cottage's yard and garden.
    lot: Kit<Surface>,
    lot_nodes: Vec<Node>,
}

impl Parts {
    fn new(build: Build) -> Self {
        let count = if build == Build::Standing {
            2
        } else {
            HousePart::ALL.len()
        };
        Self {
            split: build != Build::Standing,
            kits: (0..count).map(|_| Kit::new()).collect(),
            courses: (0..count).map(|_| Courses::default()).collect(),
            nodes: (0..count).map(|_| Vec::new()).collect(),
            openings: Default::default(),
            lot: Kit::new(),
            lot_nodes: Vec::new(),
        }
    }

    /// Where what lies on the ground round the cottage goes: the standing cottage's
    /// walls, or the parts' ground.
    fn ground(&mut self) -> &mut Kit<Surface> {
        let slot = self.ground_slot();
        &mut self.kits[slot]
    }

    /// Where the loose nodes lying on the ground go.
    fn ground_nodes(&mut self) -> &mut Vec<Node> {
        let slot = self.ground_slot();
        &mut self.nodes[slot]
    }

    /// The kit of what stays on the ground: the standing cottage's walls, or the
    /// parts' ground.
    fn ground_slot(&self) -> usize {
        if self.split {
            HousePart::Ground as usize
        } else {
            0
        }
    }

    fn slot(&self, piece: HousePart) -> usize {
        if self.split {
            piece as usize
        } else {
            usize::from(piece.is_roof() || piece == HousePart::Chimney)
        }
    }

    fn kit(&mut self, piece: HousePart) -> &mut Kit<Surface> {
        let slot = self.slot(piece);
        &mut self.kits[slot]
    }

    /// Where `piece`'s shingle courses go: the standing cottage draws all of them with
    /// its roof's, a porch roof's on the wall included.
    fn courses(&mut self, piece: HousePart) -> &mut Courses {
        let slot = if self.split { piece as usize } else { 1 };
        &mut self.courses[slot]
    }
}

/// The paints of a cottage: its type's, with the roof gently varied by position.
fn palette(design: &HouseDesign, x: f64, z: f64) -> Palette {
    let style = design.style;
    Palette {
        siding: style.siding,
        trim: style.trim,
        accent: style.accent,
        roof: scale_hex_color(
            style.roof,
            0.92 + 0.12 * (0.5 + 0.5 * (x * 3.7 + z * 1.9).sin()),
        ),
    }
}

/// The material a finish draws with on a cottage: walls in clapboard or brick,
/// brick, stone and glass as the kit paints them and every other finish one satin
/// class (its paint baked into vertex colours).
fn cottage_material(surface: Surface, palette: &Palette, walls: WallFinish) -> Arc<Material> {
    let source = surface_material(surface, palette);
    match surface {
        Surface::Siding if walls == WallFinish::Brick => {
            textured(BRICK_TEXTURE, palette.siding, 0.035, 0.9)
        }
        // Dash-finish stucco under flat paint, matte.
        Surface::Siding if walls == WallFinish::Stucco => stucco(palette.siding),
        // The tract's timber (garage doors, box bays, porch posts, mulch) is stained a
        // dark chocolate, never the village's orange-tan.
        Surface::Wood if walls == WallFinish::Stucco => satin(TRACT_STAIN),
        // Its brick veneer is a dark brown.
        Surface::Brick if walls == WallFinish::Stucco => {
            textured(BRICK_TEXTURE, SENDA_BRICK, 0.035, 0.9)
        }
        // Its stacked stone: the porch piers and the stone wings.
        Surface::Stone if walls == WallFinish::Stucco => ledgestone(),
        // White vinyl frames over white plantation shutters.
        Surface::Curtain if walls == WallFinish::Stucco => satin(PLANTATION_BLINDS),
        Surface::GarageDoor => satin(garage_door_paint(palette.siding)),
        Surface::Cladding => satin(TRACT_CLADDING),
        Surface::Siding
        | Surface::Brick
        | Surface::Stone
        | Surface::Glass
        | Surface::Lamp
        | Surface::Solar => source,
        // Freshly split timber catches the light, so a torn edge reads as torn timber
        // from the battlefield's height.
        Surface::Splinters => satin(FRESH_SPLIT),
        _ => satin(source.color.0),
    }
}

/// A stucco house's garage door by its wall paint ([`CREAM_GARAGE_STAIN`]): the
/// taupe-brown stain on its sands and tans (the reference house's), a lighter one on its
/// creams and paint on its whites.
fn garage_door_paint(siding: u32) -> u32 {
    let [r, g, b] = [16, 8, 0].map(|shift| (siding >> shift) & 0xff);
    let (high, low) = (r.max(g).max(b), r.min(g).min(b));
    // Chroma against brightness, as HSV saturation in 1/256ths.
    let saturation = (high - low) * 256 / high.max(1);
    match saturation {
        0..16 => WHITE_GARAGE_PAINT,
        16..44 => CREAM_GARAGE_STAIN,
        _ => TRACT_GARAGE_STAIN,
    }
}

static STUCCO_WALLS: Cache<u32, Material> = Cache::new("house_model::STUCCO_WALLS");

/// A stucco wall in `paint`: small rounded bumps, the paint's colour on average.
fn stucco(paint: u32) -> Arc<Material> {
    STUCCO_WALLS.get_or_insert(paint, || {
        let map = TextureRef {
            anisotropy: 8,
            repeat: [STUCCO_REPEAT; 2],
            ..TextureRef::file(STUCCO_TEXTURE)
        };
        Material {
            map: Some(map.clone()),
            bump_map: Some(map),
            bump_scale: STUCCO.0,
            color: Color(tint_over(paint, STUCCO_AVERAGE)),
            roughness: STUCCO.1,
            metalness: 0.0,
            ..Material::default()
        }
    })
}

/// The tract's buff stacked-stone veneer: thin courses of split sandstone (at
/// [`LEDGESTONE_TILE`]).
fn ledgestone() -> Arc<Material> {
    textured(SANDSTONE_TEXTURE, TRACT_STONE, 0.05, 0.92)
}

/// The inside of a wall, which shows only on a broken part: old wallpaper over the
/// plaster, a faded colour of its own in each cottage type.
fn wallpaper(palette: &Palette) -> Arc<Material> {
    satin(mix_hex(
        PLASTER,
        WALLPAPERS[(palette.siding / 13) as usize % WALLPAPERS.len()],
        WALLPAPER_TINT,
    ))
}

/// [`wallpaper`] in a broken cottage's open rooms.
fn shaded_wallpaper(palette: &Palette) -> Arc<Material> {
    satin(scale_hex_color(wallpaper(palette).color.0, ROOM_SHADE))
}

/// How much darker a broken cottage's rooms are drawn than their paint: they stand in
/// the shade of their walls and roof, which the lighting does not shadow inside.
const ROOM_SHADE: f64 = 0.78;

/// Kit texture density per finish: brick walls at the brick's scale, a stucco house's
/// stacked stone in its thin courses.
fn cottage_uv(walls: WallFinish) -> impl Fn(Surface) -> f64 {
    move |surface| match surface {
        Surface::Siding if walls == WallFinish::Brick => uv_per_metre(Surface::Brick),
        Surface::Stone if walls == WallFinish::Stucco => 1.0 / LEDGESTONE_TILE,
        _ => uv_per_metre(surface),
    }
}

static GRIMED: Cache<(u32, WallFinish), Material> = Cache::new("house_model::GRIMED");
static WEATHERED_SHINGLES: Cache<u32, Material> = Cache::new("house_model::WEATHERED_SHINGLES");

/// A standing cottage's walls, weathered darker toward the plinth where rain
/// splashes and soil climbs: the siding material taking vertex colours that fall
/// off linearly from the ridge (`ridge` metres up) to the plinth.
pub(super) fn grime(meshes: &mut KitMeshes<Surface>, ridge: f64) {
    let span = (ridge - PLINTH).max(1.0) as f32;
    for (surface, mesh) in meshes.iter_mut() {
        if *surface != Surface::Siding {
            continue;
        }
        let mesh = Arc::make_mut(mesh);
        mesh.colors = mesh
            .positions
            .iter()
            .map(|p| {
                let shade = GRIME.0 + GRIME.1 * (p[1] - PLINTH as f32) / span;
                [shade; 3]
            })
            .collect();
    }
}

/// Add a cottage's meshes, its siding weathered ([`grime`]).
fn add_weathered(
    group: &mut Node,
    meshes: KitMeshes<Surface>,
    palette: &Palette,
    walls: WallFinish,
) {
    add_finished(group, meshes, palette, walls, false);
}

/// [`add_weathered`], its rooms' lining shaded when `rooms` (a broken cottage's).
fn add_finished(
    group: &mut Node,
    meshes: KitMeshes<Surface>,
    palette: &Palette,
    walls: WallFinish,
    rooms: bool,
) {
    for (surface, mesh) in meshes {
        let material = match surface {
            Surface::Siding => GRIMED.get_or_insert((palette.siding, walls), || Material {
                vertex_colors: true,
                ..(*cottage_material(Surface::Siding, palette, walls)).clone()
            }),
            Surface::Lining if rooms => shaded_wallpaper(palette),
            Surface::Lining => wallpaper(palette),
            _ => cottage_material(surface, palette, walls),
        };
        group.children.push(shadowed(mesh, material));
    }
}

/// Matte-looking satin for planting, so it joins the cottage's satin draw.
fn satin(color: u32) -> Arc<Material> {
    material(color, SATIN.0, SATIN.1)
}

static OCTAHEDRON: Cache<(), Mesh> = Cache::new("house_model::OCTAHEDRON");

/// [`palette`] for the imported house ([`super::imported_house`]).
pub(super) fn palette_for(design: &HouseDesign, x: f64, z: f64) -> Palette {
    palette(design, x, z)
}

/// A whole cottage for a cottage cover, built as one standing model: the same look
/// as its parts drawn whole, in the draws and triangles of the standing build.
pub(super) fn standing_house(design: &HouseDesign, x: f64, z: f64) -> Node {
    cottage(design, &palette(design, x, z))
}

/// A cottage whose door faces `street` quarter turns (see [`door_direction`]);
/// scenery outside the arena chooses its own.
#[allow(clippy::too_many_arguments)]
pub(super) fn house_facing(
    group: &mut Node,
    x: f64,
    z: f64,
    w: f64,
    d: f64,
    h: f64,
    _color: u32,
    street: i32,
) {
    let design = HouseDesign::facing(x, z, w, d, h, street);
    let palette = palette(&design, x, z);
    group.children.push(cottage(&design, &palette));
}

/// A structural cottage's model, each part of it apart ([`Build::Parts`]): its walls,
/// roof slopes and chimney finished and painted for the standing cottage, their shingle
/// courses, the nodes they carry, each wall's openings and what stays on the ground
/// (the plinth, the steps and the yard), all in the build frame.
pub(super) struct CottagePieces {
    pub(super) roof: Roof,
    pub(super) palette: Palette,
    pub(super) parts: Vec<CottagePiece>,
    pub(super) openings: [Vec<[f64; 4]>; 4],
    pub(super) ground: Node,
}

/// One part of a [`CottagePieces`].
pub(super) struct CottagePiece {
    pub(super) part: HousePart,
    pub(super) meshes: KitMeshes<Surface>,
    pub(super) shingles: Option<Arc<Mesh>>,
    pub(super) nodes: Vec<Node>,
}

/// A cottage's [`CottagePieces`]; `ground` false leaves the plinth, steps and yard
/// unfinished (an empty [`CottagePieces::ground`]), which a fallen body never draws.
pub(super) fn cottage_pieces(design: &HouseDesign, x: f64, z: f64, ground: bool) -> CottagePieces {
    let palette = palette(design, x, z);
    let (roof, parts) = cottage_parts(design, Build::Parts);
    let walls = design.style.walls;
    let uv = cottage_uv(walls);
    let Parts {
        kits,
        courses,
        nodes,
        openings,
        lot,
        lot_nodes,
        ..
    } = parts;
    let mut yard = Node::group("");
    let mut pieces = Vec::new();
    for (((part, kit), courses), nodes) in
        HousePart::ALL.into_iter().zip(kits).zip(courses).zip(nodes)
    {
        if part == HousePart::Ground {
            if ground {
                let mut meshes = kit.finish_scaled(&uv);
                grime(&mut meshes, roof.ridge);
                add_weathered(&mut yard, meshes, &palette, walls);
                yard.children.extend(nodes);
            }
            continue;
        }
        let mut meshes = kit.finish_scaled(&uv);
        grime(&mut meshes, roof.ridge);
        let shingles =
            (!courses.positions.is_empty()).then(|| Arc::new(courses.weathered_mesh(&roof)));
        pieces.push(CottagePiece {
            part,
            meshes,
            shingles,
            nodes,
        });
    }
    if ground {
        add_weathered(&mut yard, lot.finish_scaled(&uv), &palette, walls);
        yard.children.extend(lot_nodes);
    }
    CottagePieces {
        roof,
        palette,
        parts: pieces,
        openings,
        ground: yard,
    }
}

/// Paint a structural cottage's part meshes (and the torn edges cut for them) as the
/// standing cottage's.
pub(super) fn paint_cottage(
    group: &mut Node,
    meshes: KitMeshes<Surface>,
    design: &HouseDesign,
    palette: &Palette,
) {
    // Siding cut or torn since it was weathered takes its weathering again, so every
    // siding mesh carries the vertex colours its draw expects.
    let mut meshes = meshes;
    grime(&mut meshes, design.h);
    add_finished(group, meshes, palette, design.style.walls, true);
}

/// [`paint_cottage`] for a part's meshes as [`cottage_pieces`] finished them, uncut:
/// their siding already carries the weathering (`grime` from the ridge, which is the
/// design's height), so they are painted as they are, sharing their meshes.
pub(super) fn paint_cottage_whole(
    group: &mut Node,
    meshes: KitMeshes<Surface>,
    design: &HouseDesign,
    palette: &Palette,
) {
    add_finished(group, meshes, palette, design.style.walls, true);
}

/// A cottage's shingle courses, weathered as the standing roof's.
pub(super) fn cottage_shingles(mesh: Arc<Mesh>, palette: &Palette) -> Node {
    let material = WEATHERED_SHINGLES.get_or_insert(palette.roof, || Material {
        vertex_colors: true,
        ..(*house_material(HouseSurface::Shingles, palette.roof)).clone()
    });
    shadowed(mesh, material)
}

/// The texture scale of a cottage's finishes.
pub(super) fn cottage_scale(design: &HouseDesign) -> impl Fn(Surface) -> f64 {
    cottage_uv(design.style.walls)
}

/// A cottage's roof and kits for `build`, built a section at a time: the plinth,
/// each wall, then the roof and the rest.
fn cottage_parts(design: &HouseDesign, build: Build) -> (Roof, Parts) {
    let roof = design.roof();
    let mut parts = Parts::new(build);
    let mut rng = Random::new(f64::from(design.variant) + 41.0);
    house_base(&mut parts, design);
    for wall in 0..4 {
        house_wall(&mut parts, design, &roof, build, &mut rng, wall);
    }
    house_roof(&mut parts, design, &roof, build);
    (roof, parts)
}

/// Build a standing cottage: one turned node.
fn cottage(design: &HouseDesign, palette: &Palette) -> Node {
    let walls = design.style.walls;
    let uv = cottage_uv(walls);
    let (roof, parts) = cottage_parts(design, Build::Standing);
    let Parts {
        kits,
        courses,
        nodes,
        lot,
        lot_nodes,
        ..
    } = parts;
    let [body, roof_kit] = <[Kit<Surface>; 2]>::try_from(kits)
        .ok()
        .expect("body and roof kits");
    let roof_courses = courses.into_iter().nth(1).expect("roof courses");
    let mut cottage = Node::group("");
    let mut body_meshes = body.finish_scaled(&uv);
    body_meshes.retain(|(_, mesh)| !mesh.positions.is_empty());
    grime(&mut body_meshes, roof.ridge);
    add_weathered(&mut cottage, body_meshes, palette, walls);
    add_weathered(&mut cottage, lot.finish_scaled(&uv), palette, walls);
    cottage.children.extend(lot_nodes);
    let mut roof_meshes = roof_kit.finish_scaled(&uv);
    roof_meshes.retain(|(_, mesh)| !mesh.positions.is_empty());
    grime(&mut roof_meshes, roof.ridge);
    add_weathered(&mut cottage, roof_meshes, palette, walls);
    if !roof_courses.positions.is_empty() {
        let material = WEATHERED_SHINGLES.get_or_insert(palette.roof, || Material {
            vertex_colors: true,
            ..(*house_material(HouseSurface::Shingles, palette.roof)).clone()
        });
        cottage.children.push(shadowed(
            Arc::new(roof_courses.weathered_mesh(&roof)),
            material,
        ));
    }
    cottage.children.extend(nodes.into_iter().flatten());
    cottage.set_rotation_euler(0.0, design.yaw(), 0.0);
    cottage
}

/// Wall frames: front (+z), back, right (+x) and left, each with its half length
/// along the wall.
pub(super) fn wall_frames(w: f64, d: f64) -> [(DMat4, f64); 4] {
    [
        (pose(DVec3::new(0.0, 0.0, d / 2.0), DVec3::ZERO), w / 2.0),
        (
            pose(DVec3::new(0.0, 0.0, -d / 2.0), DVec3::new(0.0, PI, 0.0)),
            w / 2.0,
        ),
        (
            pose(
                DVec3::new(w / 2.0, 0.0, 0.0),
                DVec3::new(0.0, PI / 2.0, 0.0),
            ),
            d / 2.0,
        ),
        (
            pose(
                DVec3::new(-w / 2.0, 0.0, 0.0),
                DVec3::new(0.0, -PI / 2.0, 0.0),
            ),
            d / 2.0,
        ),
    ]
}

/// What a wall carries: the door, gable peak, window boxes.
#[derive(Clone, Copy)]
struct WallRole {
    index: usize,
    gable: bool,
    door: bool,
    /// The door wall and the wall behind it carry window boxes.
    planted: bool,
}

fn wall_role(design: &HouseDesign, index: usize) -> WallRole {
    let back_of_door = if design.door_wall == 0 { 1 } else { 3 };
    WallRole {
        index,
        gable: design.style.form == RoofForm::Gable && index < 2,
        door: index == design.door_wall,
        planted: index == design.door_wall || index == back_of_door,
    }
}

fn window_opening(u: f64, roof: &Roof) -> Opening {
    let (y, height) = roof.ground_window();
    Opening {
        u,
        y,
        width: WINDOW_WIDTH,
        height,
    }
}

/// A two-storey house's belt course between its floors: its height (m), its middle at
/// [`Roof::belt_line`].
const BELT: f64 = 0.12;

/// An upper-floor window over `u`, between the belt course and the eave.
fn upper_window(u: f64, roof: &Roof) -> Opening {
    let (y, height) = roof.upper_window();
    Opening {
        u,
        y,
        width: if roof.low_storeys {
            TRACT_UPPER_WINDOW
        } else {
            WINDOW_WIDTH
        },
        height,
    }
}

/// A tract house's upper windows' width (m): small sliders, well apart; over the garage
/// a narrower pair ganged together ([`GARAGE_PAIR`] either side of its middle).
const TRACT_UPPER_WINDOW: f64 = 0.62;
const PAIR_WINDOW: f64 = 0.44;
/// A tract house's opening surround: a flat band of stucco round each window and door,
/// its width and how far it stands proud of the wall (m), and the plain stucco sill under
/// a window (its height and how far it stands out).
const SURROUND: (f64, f64) = (0.07, 0.025);
const TRACT_SILL: (f64, f64) = (0.06, 0.06);
/// A tract window's vinyl frame and its sashes' meeting stiles (m), and the share of its
/// height the glass shows over the plantation shutters behind it.
const TRACT_FRAME: f64 = 0.045;
const GLASS_HEAD: f64 = 0.18;
/// The plantation shutters' slat pitch behind a window's glass and how far a slat's top
/// edge stands out from its bottom (m).
const LOUVRE_PITCH: f64 = 0.075;
const LOUVRE_TILT: f64 = 0.022;
/// The narrowest shutter a tract house hangs (m): its shutters are half a small window wide.
const TRACT_SHUTTER: f64 = 0.2;

/// A tract house's window set `depth` back in its opening: a slim white vinyl frame split
/// by its two sashes' meeting stiles (a slider; a tall window's sashes meet across it)
/// over the white plantation shutters closed behind the glass, which shows as a strip of
/// sky-lit glass over them, so a front reads light as the tract's do, never as dark holes;
/// under no head or drip cap: a flat stucco band round the opening and a plain stucco
/// sill, as the tract's windows are (in `band`: a brick or board face's are trimmed in
/// the trim's paint).
fn tract_window(
    kit: &mut Kit<Surface>,
    frame: DMat4,
    opening: &Opening,
    depth: f64,
    band: Surface,
) {
    let (u, w, y0, y1) = (opening.u, opening.width, opening.y, opening.top());
    let glass_z = -depth + 0.02;
    let at = |u: f64, y: f64| frame.transform_point3(DVec3::new(u, y, glass_z));
    kit.quad(
        Surface::Glass,
        [
            at(u - w / 2.0, y0),
            at(u + w / 2.0, y0),
            at(u + w / 2.0, y1),
            at(u - w / 2.0, y1),
        ],
        frame.transform_vector3(DVec3::Z),
    );
    let blinds = frame.transform_vector3(DVec3::Z * 0.008);
    let shutter_top = y1 - (y1 - y0) * GLASS_HEAD;
    kit.quad(
        Surface::Curtain,
        [
            at(u - w / 2.0, y0),
            at(u + w / 2.0, y0),
            at(u + w / 2.0, shutter_top),
            at(u - w / 2.0, shutter_top),
        ]
        .map(|p| p + blinds),
        frame.transform_vector3(DVec3::Z),
    );
    louvres(
        kit,
        frame,
        (u - w / 2.0, u + w / 2.0),
        (y0, shutter_top),
        glass_z,
    );
    let member = |kit: &mut Kit<Surface>, size: DVec2, center: DVec2| {
        kit.block(
            Surface::Curtain,
            DVec3::new(size.x, size.y, 0.04),
            frame * shift(DVec3::new(center.x, center.y, glass_z + 0.02)),
        );
    };
    let middle = (y0 + y1) / 2.0;
    for side in [-1.0, 1.0] {
        member(
            kit,
            DVec2::new(TRACT_FRAME, y1 - y0),
            DVec2::new(u + side * (w - TRACT_FRAME) / 2.0, middle),
        );
        member(
            kit,
            DVec2::new(w, TRACT_FRAME),
            DVec2::new(u, middle + side * (y1 - y0 - TRACT_FRAME) / 2.0),
        );
    }
    if w >= (y1 - y0) * 0.75 {
        member(
            kit,
            DVec2::new(TRACT_FRAME * 1.4, y1 - y0),
            DVec2::new(u, middle),
        );
    } else {
        member(kit, DVec2::new(w, TRACT_FRAME * 1.4), DVec2::new(u, middle));
    }
    surround(kit, frame, opening, band);
    kit.block(
        band,
        DVec3::new(w + 2.0 * SURROUND.0 + 0.04, TRACT_SILL.0, TRACT_SILL.1),
        frame * shift(DVec3::new(u, y0 - TRACT_SILL.0 / 2.0, TRACT_SILL.1 / 2.0)),
    );
}

/// The slats of the plantation shutters closed behind a window's glass (at `glass_z` in
/// `frame`), across `u0..u1` from `y0` to `y1`: each a strip whose lower edge leans back,
/// facing out and down, so the sky lights it less than the shutters' face and the
/// shutters read as louvres through the glass in their own white paint.
fn louvres(
    kit: &mut Kit<Surface>,
    frame: DMat4,
    (u0, u1): (f64, f64),
    (y0, y1): (f64, f64),
    glass_z: f64,
) {
    let count = ((y1 - y0) / LOUVRE_PITCH).floor();
    if count < 1.0 {
        return;
    }
    let pitch = (y1 - y0) / count;
    let at = |u: f64, y: f64, z: f64| frame.transform_point3(DVec3::new(u, y, z));
    let (back, front) = (glass_z + 0.009, glass_z + 0.009 + LOUVRE_TILT);
    let normal = frame.transform_vector3(DVec3::new(0.0, -LOUVRE_TILT, pitch / 2.0));
    for k in 0..count as u32 {
        let y = y0 + pitch * f64::from(k);
        let top = y + pitch / 2.0;
        kit.quad(
            Surface::Curtain,
            [
                at(u0, y, back),
                at(u1, y, back),
                at(u1, top, front),
                at(u0, top, front),
            ],
            normal,
        );
    }
}

/// The flat band round a tract house's opening (stucco, or `band`), on its jambs and head.
fn surround(kit: &mut Kit<Surface>, frame: DMat4, opening: &Opening, band_surface: Surface) {
    let (u, w, y0, y1) = (opening.u, opening.width, opening.y, opening.top());
    let (band, proud) = SURROUND;
    for side in [-1.0, 1.0] {
        kit.block(
            band_surface,
            DVec3::new(band, y1 - y0 + band, proud),
            frame
                * shift(DVec3::new(
                    u + side * (w + band) / 2.0,
                    (y0 + y1 + band) / 2.0,
                    proud / 2.0,
                )),
        );
    }
    kit.block(
        band_surface,
        DVec3::new(w, band, proud),
        frame * shift(DVec3::new(u, y1 + band / 2.0, proud / 2.0)),
    );
}

/// A tract house's front door set `depth` back in its opening: a stained wood leaf of
/// raised panels under a small lite, a dark handle and a stucco surround, no brass.
fn tract_door(kit: &mut Kit<Surface>, frame: DMat4, opening: &Opening, depth: f64) {
    let (u, w, y0, y1) = (opening.u, opening.width, opening.y, opening.top());
    let back = -depth + 0.02;
    let height = y1 - y0;
    kit.block(
        Surface::Wood,
        DVec3::new(w, height, 0.04),
        frame * shift(DVec3::new(u, (y0 + y1) / 2.0, back + 0.02)),
    );
    let face = back + 0.04;
    let lite = (y1 - height * 0.3, y1 - 0.12);
    let at = |u: f64, y: f64| frame.transform_point3(DVec3::new(u, y, face + 0.004));
    let (l0, l1) = (u - w / 2.0 + 0.16, u + w / 2.0 - 0.16);
    kit.quad(
        Surface::Glass,
        [
            at(l0, lite.0),
            at(l1, lite.0),
            at(l1, lite.1),
            at(l0, lite.1),
        ],
        frame.transform_vector3(DVec3::Z),
    );
    let panel = (w - 0.36, (lite.0 - y0 - 0.42) / 2.0);
    for k in 0..2 {
        kit.chamfer_block(
            Surface::Wood,
            DVec3::new(panel.0, panel.1, 0.03),
            0.012,
            frame
                * shift(DVec3::new(
                    u,
                    y0 + 0.2 + panel.1 / 2.0 + f64::from(k) * (panel.1 + 0.12),
                    face + 0.008,
                )),
        );
    }
    kit.block(
        Surface::Iron,
        DVec3::new(0.03, 0.18, 0.04),
        frame
            * shift(DVec3::new(
                u + w / 2.0 - 0.12,
                y0 + height * 0.48,
                face + 0.02,
            )),
    );
    surround(kit, frame, opening, Surface::Siding);
}

/// The board shutters a tract house hangs beside a window, as `(side, width)` (side ±1
/// along the wall): on each side the widest that fits clear of the wall's ends, the doors
/// and half the room to a window beside it, whose shutter takes the other half; none on
/// the side of a window ganged with it, the pair shuttered as one.
fn tract_shutters_beside(
    opening: &Opening,
    half: f64,
    windows: &[Opening],
    doors: &[Opening],
) -> Vec<(f64, f64)> {
    let edge = |o: &Opening| o.width / 2.0 + SURROUND.0 + 0.02;
    let mut room = [
        opening.u - edge(opening) - (-half + 0.15),
        half - 0.15 - (opening.u + edge(opening)),
    ];
    let mut ganged = [false; 2];
    let neighbours = windows
        .iter()
        .filter(|other| other.u != opening.u)
        .map(|other| (other, true))
        .chain(doors.iter().map(|other| (other, false)));
    for (other, window) in neighbours {
        let gap = (other.u - opening.u).abs() - edge(other) - edge(opening);
        let side = usize::from(other.u > opening.u);
        ganged[side] |= window && gap < 2.0 * TRACT_SHUTTER;
        // A neighbouring window hangs a shutter of its own into the room between.
        room[side] = room[side].min(if window { gap / 2.0 } else { gap });
    }
    [-1.0, 1.0]
        .into_iter()
        .zip(room.into_iter().zip(ganged))
        .filter(|(_, (_, ganged))| !ganged)
        .map(|(side, (room, _))| (side, (room - TRIM_GAP).min(shutter_width(opening))))
        .filter(|&(_, width)| width >= TRACT_SHUTTER)
        .collect()
}

/// The widest shutter that fits beside a window on a garage front: clear of the
/// wall's ends and of the casings of the openings beside it (`others`).
fn shutter_room_among(opening: &Opening, half: f64, others: &[Opening]) -> f64 {
    let edge = |o: &Opening| o.width / 2.0 + CASING + 0.01;
    let (mut from, mut to) = (-half + CORNER_BOARD, half - CORNER_BOARD);
    for other in others.iter().filter(|other| other.y < opening.top()) {
        if other.u < opening.u {
            from = from.max(other.u + edge(other));
        } else if other.u > opening.u {
            to = to.min(other.u - edge(other));
        }
    }
    let left = opening.u - edge(opening) - from - TRIM_GAP;
    let right = to - (opening.u + edge(opening)) - TRIM_GAP;
    let width = left.min(right).min(shutter_width(opening));
    if width >= MIN_SHUTTER { width } else { 0.0 }
}

/// The widest shutter that fits beside a window: clear of the corner boards, the
/// door casing (`inner_limit`) and the neighbouring window's shutter.
fn shutter_room(opening: &Opening, half: f64, inner_limit: f64, alone: bool) -> f64 {
    let casing_edge = opening.width / 2.0 + CASING + 0.01;
    let outer = half - CORNER_BOARD - (opening.u.abs() + casing_edge) - TRIM_GAP;
    let inner = if alone {
        outer
    } else {
        // Twin windows split the wall between them.
        let space = opening.u.abs() - casing_edge - TRIM_GAP - inner_limit;
        if inner_limit > 0.0 {
            space
        } else {
            space / 2.0
        }
    };
    let width = outer.min(inner).min(shutter_width(opening));
    if width >= MIN_SHUTTER { width } else { 0.0 }
}

/// How far a wall part's painted lining sits behind its wall face.
const LINING: f64 = 0.2;
/// How far (m) round an opening a fallen wall's framing rails stop short of it.
const RAIL_CLEAR: f64 = 0.5;
/// How far in from a wall's outline its torn-off edge faces sit (m).
const EDGE_INSET: f64 = 0.012;

/// Studs a fallen wall shows on its back, between its openings, and the top plate.
fn framing(
    kit: &mut Kit<Surface>,
    frame: DMat4,
    outline: &[DVec2],
    openings: &[Opening],
    top: f64,
) {
    const STUD: (f64, f64) = (0.09, 0.06);
    const SPACING: f64 = 0.45;
    let half = outline.iter().map(|p| p.x.abs()).fold(0.0, f64::max);
    let plinth = outline.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let z = -LINING - STUD.1 / 2.0;
    let count = (2.0 * half / SPACING).floor() as i32;
    for i in 0..=count {
        let u = -half + 0.08 + f64::from(i) * (2.0 * half - 0.16) / f64::from(count.max(1));
        if openings
            .iter()
            .any(|o| (u - o.u).abs() < o.width / 2.0 + 0.05)
        {
            continue;
        }
        kit.block(
            Surface::Wood,
            DVec3::new(STUD.0, top - plinth - 0.05, STUD.1),
            frame * shift(DVec3::new(u, (top + plinth) / 2.0, z)),
        );
    }
    // The top plate, and rails across the studs under the sills and, in a
    // two-storey wall, where the upper floor's joists bore, broken by the openings
    // they meet: a fallen wall's back reads as a frame of timber rather than a
    // board.
    let mut rails = vec![top - STUD.0, plinth + 0.72];
    if top - plinth > 3.6 {
        rails.push(plinth + (top - plinth) * 0.5);
    }
    for y in rails {
        let mut runs = vec![(-half + 0.05, half - 0.05)];
        for o in openings
            .iter()
            // Clear of the glass too, which shows what lies behind it.
            .filter(|o| o.y - RAIL_CLEAR < y && o.top() + RAIL_CLEAR > y)
        {
            let (a, b) = (o.u - o.width / 2.0 - 0.05, o.u + o.width / 2.0 + 0.05);
            runs = runs
                .into_iter()
                .flat_map(|(from, to)| [(from, to.min(a)), (from.max(b), to)])
                .filter(|(from, to)| to - from > 0.2)
                .collect();
        }
        for (from, to) in runs {
            kit.block(
                Surface::Wood,
                DVec3::new(to - from, STUD.0, STUD.1),
                frame * shift(DVec3::new((from + to) / 2.0, y, z - STUD.1 * 0.5)),
            );
        }
    }
}

/// Where a fallen wall's plaster has come away from the lath behind it: a few
/// ragged patches of bare, dusty lath on its lining between the openings, so a
/// panel lying lining up reads as a broken wall rather than a clean board.
fn plaster_falls(
    kit: &mut Kit<Surface>,
    frame: DMat4,
    half: f64,
    top: f64,
    openings: &[Opening],
    rng: &mut Random,
) {
    for _ in 0..PLASTER_FALLS {
        let (w, h) = (rng.range(0.6, 1.3), rng.range(0.4, 0.9));
        let u = rng.range(-half + w / 2.0 + 0.1, half - w / 2.0 - 0.1);
        let y = rng.range(PLINTH + h / 2.0 + 0.1, top - h / 2.0 - 0.1);
        let clear = openings.iter().all(|o| {
            (u - o.u).abs() > (w + o.width) / 2.0 + 0.05
                || y + h / 2.0 < o.y - 0.05
                || y - h / 2.0 > o.top() + 0.05
        });
        let outline = jagged(rng, w, h);
        if !clear {
            continue;
        }
        let patch = frame * pose(DVec3::new(u, y, -LINING - 0.006), DVec3::new(0.0, PI, 0.0));
        kit.face_with_holes(Surface::Wood, &outline, &[], patch);
        // The gaps between the laths, dark lines across the bare patch.
        for k in [-0.3, 0.0, 0.3] {
            let ly = k * h + rng.range(-0.03, 0.03);
            let lw = w * rng.range(0.3, 0.42);
            let at = |x: f64, y: f64| patch.transform_point3(DVec3::new(x, y, 0.003));
            kit.quad(
                Surface::Dark,
                [
                    at(-lw, ly - 0.012),
                    at(lw, ly - 0.012),
                    at(lw, ly + 0.012),
                    at(-lw, ly + 0.012),
                ],
                patch.transform_vector3(DVec3::Z),
            );
        }
    }
}

/// An irregular broken outline about `w` by `d` (counter-clockwise, convex): the
/// shape of a torn piece rather than a sawn rectangle.
fn jagged(rng: &mut Random, w: f64, d: f64) -> Vec<DVec2> {
    const CORNERS: usize = 6;
    let phase = rng.range(0.0, PI * 2.0);
    (0..CORNERS)
        .map(|i| {
            let angle = phase + (i as f64 + rng.range(-0.25, 0.25)) * PI * 2.0 / CORNERS as f64;
            let reach = rng.range(0.82, 1.0);
            DVec2::new(angle.cos() * w / 2.0 * reach, angle.sin() * d / 2.0 * reach)
        })
        .collect()
}

/// A polygon seen from behind its plane: mirrored across u, its winding kept.
fn mirrored(polygon: &[DVec2]) -> Vec<DVec2> {
    polygon
        .iter()
        .rev()
        .map(|p| DVec2::new(-p.x, p.y))
        .collect()
}

/// A pair of plank shutters hung open beside a window: boards under two battens
/// and a Z brace, with a strap hinge's shadow line.
fn board_shutters(kit: &mut Kit<Surface>, frame: DMat4, opening: &Opening, width: f64) {
    const THICK: f64 = 0.035;
    let (y0, y1) = (opening.y - 0.02, opening.top());
    let height = y1 - y0;
    let z = 0.04 + THICK / 2.0;
    for side in [-1.0, 1.0] {
        let u = opening.u + side * (opening.width / 2.0 + CASING + 0.01 + width / 2.0);
        let at = |du: f64, y: f64, dz: f64| frame * shift(DVec3::new(u + du, y, z + dz));
        kit.block(
            Surface::Accent,
            DVec3::new(width, height, THICK),
            at(0.0, (y0 + y1) / 2.0, 0.0),
        );
        for y in [y0 + height * 0.18, y1 - height * 0.18] {
            kit.block(
                Surface::Accent,
                DVec3::new(width - 0.04, 0.07, 0.02),
                at(0.0, y, THICK / 2.0 + 0.01),
            );
        }
        let rise = height * 0.64;
        let length = (rise * rise + (width - 0.1).powi(2)).sqrt();
        let lean = side * (rise / (width - 0.1)).atan();
        kit.block(
            Surface::Accent,
            DVec3::new(length, 0.06, 0.02),
            frame
                * pose(
                    DVec3::new(u, (y0 + y1) / 2.0, z + THICK / 2.0 + 0.01),
                    DVec3::new(0.0, 0.0, lean),
                ),
        );
    }
}

/// A tract house's single board-and-batten shutter `width` wide on `side` (±1 along the
/// wall) of a window: a plank leaf in the accent paint, battens over its joints and a
/// rail across its top and foot.
fn batten_shutter(kit: &mut Kit<Surface>, frame: DMat4, opening: &Opening, width: f64, side: f64) {
    const THICK: f64 = 0.03;
    const BATTEN: f64 = 0.035;
    let (y0, y1) = (opening.y - 0.02, opening.top() + 0.02);
    let height = y1 - y0;
    let z = SURROUND.1 + THICK / 2.0;
    let u = opening.u + side * (opening.width / 2.0 + SURROUND.0 + 0.02 + width / 2.0);
    let at = |du: f64, y: f64, dz: f64| frame * shift(DVec3::new(u + du, y, z + dz));
    kit.block(
        Surface::Accent,
        DVec3::new(width, height, THICK),
        at(0.0, (y0 + y1) / 2.0, 0.0),
    );
    for du in [-width / 6.0, width / 6.0] {
        kit.block(
            Surface::Accent,
            DVec3::new(BATTEN, height, 0.016),
            at(du, (y0 + y1) / 2.0, THICK / 2.0 + 0.008),
        );
    }
    for y in [y0 + 0.06, y1 - 0.06] {
        kit.block(
            Surface::Accent,
            DVec3::new(width, 0.05, 0.016),
            at(0.0, y, THICK / 2.0 + 0.008),
        );
    }
}

/// A window boarded over (a shut-up room, not battle damage): the dark room behind,
/// planks nailed across the casing at odd angles and the casing itself, the siding
/// round it whole.
fn boarded_window(kit: &mut Kit<Surface>, frame: DMat4, opening: &Opening, seed: f64) {
    let (u, w, y0, y1) = (opening.u, opening.width, opening.y, opening.top());
    let at = |u: f64, y: f64| frame.transform_point3(DVec3::new(u, y, -REVEAL + 0.02));
    kit.quad(
        Surface::Dark,
        [
            at(u - w / 2.0, y0),
            at(u + w / 2.0, y0),
            at(u + w / 2.0, y1),
            at(u - w / 2.0, y1),
        ],
        frame.transform_vector3(DVec3::Z),
    );
    let mut rng = Random::new(seed + 3.0);
    let planks = 3;
    for k in 0..planks {
        let y = y0 + (y1 - y0) * (f64::from(k) + 0.5 + rng.range(-0.15, 0.15)) / f64::from(planks);
        // Rough-sawn boards with their thickness, some fresh and pale, nailed
        // across at odd angles.
        let fresh = rng.next() < 0.5;
        kit.block(
            if fresh {
                Surface::Splinters
            } else {
                Surface::Wood
            },
            DVec3::new(w + 2.0 * CASING + rng.range(0.05, 0.2), 0.15, PLANK),
            frame
                * pose(
                    DVec3::new(u + rng.range(-0.05, 0.05), y, 0.03 + PLANK / 2.0),
                    DVec3::new(0.0, 0.0, rng.range(-0.22, 0.22)),
                ),
        );
    }
    opening_casing(kit, frame, opening, true);
}

/// The finish of what a house stands on: a cottage's fieldstone plinth, a stucco
/// house's concrete footing.
pub(super) fn footing(design: &HouseDesign) -> Surface {
    match design.style.walls {
        // The tract's houses, sided or stucco, stand on poured slabs.
        _ if design.garage.is_some() => Surface::Concrete,
        WallFinish::Stucco => Surface::Concrete,
        _ => Surface::Stone,
    }
}

/// The finish of the steps up to the door: the footing's, or a garage house's poured
/// concrete.
fn step_finish(design: &HouseDesign) -> Surface {
    if design.garage.is_some() {
        Surface::Concrete
    } else {
        footing(design)
    }
}

/// Where along the door wall the way in is: a wing's entry opening, otherwise the door.
fn entry_u(design: &HouseDesign) -> f64 {
    match (entry(design), wing_plan(design)) {
        (Entry::RoomPorch, Some(wing)) => {
            let (_, (from, to)) = wing.room_porch();
            (from + to) / 2.0
        }
        (Entry::BrickWing, Some(wing)) => wing.recess().0,
        (entry, Some(wing)) if entry.enclosed() => wing.entry,
        _ => design.wall_plan(design.door_wall).door().unwrap_or(0.0),
    }
}

/// The two steps up to the door, in front of its porch or wing where it has one (a brick
/// wing's stoop before its door recess).
fn door_steps(kit: &mut Kit<Surface>, design: &HouseDesign) {
    let (door_frame, _) = wall_frames(design.w, design.d)[design.door_wall];
    if let (Entry::BrickWing, Some(wing)) = (entry(design), wing_plan(design)) {
        let plinth = design.roof().plinth;
        let (u, width) = wing.recess();
        kit.chamfer_block(
            Surface::Concrete,
            DVec3::new(width + 0.3, plinth, 0.42),
            0.02,
            door_frame * shift(DVec3::new(u, plinth / 2.0, WING_DEPTH + 0.21)),
        );
        return;
    }
    let plinth = design.roof().plinth;
    if let (true, Some(wing)) = (design.style.loggia, wing_plan(design)) {
        // A step up to the open arch at the corner end of the wing's face.
        kit.chamfer_block(
            Surface::Concrete,
            DVec3::new(LOGGIA_ARCH + 0.2, plinth, 0.4),
            0.02,
            door_frame
                * shift(DVec3::new(
                    wing.loggia_arch(),
                    plinth / 2.0,
                    WING_DEPTH + 0.2,
                )),
        );
        return;
    }
    let u = entry_u(design);
    let out = entry(design).depth();
    for (width, depth, top) in [(1.5, 0.62, plinth / 2.0), (1.3, 0.32, plinth)] {
        kit.chamfer_block(
            step_finish(design),
            DVec3::new(width, plinth / 2.0, depth),
            0.02,
            door_frame
                * DMat4::from_translation(DVec3::new(
                    u,
                    top - plinth / 4.0,
                    out + 0.05 + depth / 2.0,
                )),
        );
    }
}

/// The plinth and the steps up to the door: a cottage's first section ([`PartsCut`]).
/// A structural cottage's plinth goes with the wall parts it carries
/// (`house_parts_model::footings`), so a breach a tank fits through reaches the ground.
fn house_base(parts: &mut Parts, design: &HouseDesign) {
    let (w, d) = (design.w, design.d);
    let split = parts.split;
    let plinth = design.roof().plinth;
    let base = parts.ground();
    if !split {
        base.chamfer_block(
            footing(design),
            DVec3::new(w + 0.1, plinth, d + 0.1),
            0.03,
            DMat4::from_translation(DVec3::new(0.0, plinth / 2.0, 0.0)),
        );
    }
    door_steps(base, design);
}

/// A wall's openings.
struct WallOpenings {
    /// Its windows, the ground floor's first.
    windows: Vec<Opening>,
    ground_floor: usize,
    /// The windows, the door, the garage door and the gable vent.
    openings: Vec<Opening>,
    door_opening: Option<Opening>,
    garage_opening: Option<Opening>,
    vent_opening: Opening,
    vent: bool,
    two_storey: bool,
}

/// The front door's opening at `u` along its wall.
fn door_opening(u: f64, roof: &Roof) -> Opening {
    Opening {
        u,
        y: roof.plinth,
        width: DOOR_WIDTH,
        height: roof.door_height(),
    }
}

/// The garage door's opening at `u` along its wall.
fn garage_opening(u: f64, width: f64, roof: &Roof) -> Opening {
    Opening {
        u,
        y: roof.plinth,
        width,
        height: GARAGE_HEIGHT,
    }
}

/// The openings of wall `index`, as its [`HouseDesign::wall_plan`] places them.
fn wall_openings(design: &HouseDesign, roof: &Roof, index: usize) -> WallOpenings {
    let role = wall_role(design, index);
    let plan = design.wall_plan(index);
    let mut windows: Vec<Opening> = plan.windows().map(|u| window_opening(u, roof)).collect();
    // The ground floor's windows, which carry window boxes and share a wall's
    // shutters among themselves.
    let ground_floor = windows.len();
    let two_storey = design.storeys() == 2;
    let over_garage = |u: f64| {
        plan.garage()
            .is_some_and(|garage| (u - garage).abs() <= plan.garage_bay)
    };
    // A window ganged with another beside it is one of a narrower pair.
    let paired = |u: f64| {
        plan.upper
            .iter()
            .any(|&other| other != u && (other - u).abs() < 2.0 * GARAGE_PAIR + 1e-6)
    };
    windows.extend(plan.upper.iter().map(|&u| {
        let window = upper_window(u, roof);
        if !(roof.low_storeys && paired(u)) {
            return window;
        }
        if design.style.balcony && over_garage(u) {
            // French doors onto the balcony, down to its deck.
            let y = roof.belt_line() + BALCONY_DECK;
            return Opening {
                y,
                width: PAIR_WINDOW,
                height: window.top() - y,
                ..window
            };
        }
        Opening {
            width: PAIR_WINDOW,
            ..window
        }
    }));
    let door_opening = plan.door().map(|u| door_opening(u, roof));
    let garage_opening = plan
        .garage()
        .map(|u| garage_opening(u, plan.garage_width(), roof));
    let mut openings = windows.clone();
    openings.extend(door_opening);
    openings.extend(garage_opening);
    let vent = role.gable && roof.ridge - roof.wall > 1.0;
    let vent_opening = Opening {
        u: 0.0,
        y: roof.wall + (roof.ridge - roof.wall) * 0.3,
        width: 0.56,
        height: 0.34,
    };
    if vent {
        openings.push(vent_opening);
    }
    WallOpenings {
        windows,
        ground_floor,
        openings,
        door_opening,
        garage_opening,
        vent_opening,
        vent,
        two_storey,
    }
}

/// Wall `index` with its openings, trim and dressing: one of a cottage's sections
/// ([`PartsCut`]).
fn house_wall(
    parts: &mut Parts,
    design: &HouseDesign,
    roof: &Roof,
    build: Build,
    rng: &mut Random,
    index: usize,
) {
    let standing = build == Build::Standing;
    let (frame, half) = wall_frames(design.w, design.d)[index];
    let door_edge = DOOR_WIDTH / 2.0 + CASING;
    let stucco = design.style.walls == WallFinish::Stucco;
    let role = wall_role(design, index);
    let piece = HousePart::wall(index);
    let mut outline = vec![
        DVec2::new(-half, roof.plinth),
        DVec2::new(half, roof.plinth),
        DVec2::new(half, roof.wall),
    ];
    if role.gable {
        outline.push(DVec2::new(0.0, roof.ridge));
    }
    outline.push(DVec2::new(-half, roof.wall));
    let WallOpenings {
        windows,
        ground_floor,
        openings,
        door_opening,
        garage_opening,
        vent_opening,
        vent,
        two_storey,
    } = wall_openings(design, roof, index);
    let rect = |o: &Opening| [o.u - o.width / 2.0, o.y, o.u + o.width / 2.0, o.top()];
    parts.openings[index] = openings.iter().map(rect).collect();
    if let Some(garage) = garage_opening {
        // Each leaf of the garage door is a column of the structure's: a torn edge keeps
        // clear of the leaf still standing and tears on into the one that went.
        let [u0, y0, u1, y1] = rect(&garage);
        let leaves = [[u0, y0, garage.u, y1], [garage.u, y0, u1, y1]];
        parts.openings[index].retain(|r| *r != rect(&garage));
        parts.openings[index].extend(leaves);
    }
    let kit = parts.kit(piece);
    building_kit::wall(kit, frame, &outline, &openings, REVEAL);
    if !standing {
        // A broken wall shows its back: the painted lining and the cut edges.
        let cut: Vec<Vec<DVec2>> = openings
            .iter()
            .map(|opening| mirrored(&opening.corners()))
            .collect();
        let back = frame * pose(DVec3::new(0.0, 0.0, -LINING), DVec3::new(0.0, PI, 0.0));
        // The rooms' plastered lining up to the ceiling; over it a gable's back is
        // the attic's raw boarding, never the lining's flat paint (a gable fallen
        // face down showed a bare plastered panel).
        let below = |hole: &Vec<DVec2>| hole.iter().all(|p| p.y <= roof.wall + 1e-9);
        let (low, high): (Vec<Vec<DVec2>>, Vec<Vec<DVec2>>) =
            cut.into_iter().partition(|hole| below(hole));
        let room = [
            DVec2::new(-half, roof.plinth),
            DVec2::new(half, roof.plinth),
            DVec2::new(half, roof.wall),
            DVec2::new(-half, roof.wall),
        ];
        kit.face_with_holes(Surface::Lining, &mirrored(&room), &low, back);
        if role.gable {
            let attic = [
                DVec2::new(-half, roof.wall),
                DVec2::new(half, roof.wall),
                DVec2::new(0.0, roof.ridge),
            ];
            kit.face_with_holes(Surface::Wood, &mirrored(&attic), &high, back);
        }
        framing(kit, frame, &outline, &openings, roof.wall);
        plaster_falls(kit, frame, half, roof.wall, &openings, rng);
        let at = |p: DVec2, depth: f64| frame.transform_point3(DVec3::new(p.x, p.y, -depth));
        let center = outline.iter().copied().sum::<DVec2>() / outline.len() as f64;
        // The wall's edges, set in a little from its outline: at a corner the
        // neighbouring wall's face hides them while the cottage stands.
        let inset = |p: DVec2| p + (center - p).normalize_or_zero() * EDGE_INSET;
        for i in 0..outline.len() {
            let (a, b) = (inset(outline[i]), inset(outline[(i + 1) % outline.len()]));
            let out = (a + b) / 2.0 - center;
            kit.quad(
                Surface::Wood,
                [at(a, 0.0), at(b, 0.0), at(b, LINING), at(a, LINING)],
                frame.transform_vector3(DVec3::new(out.x, out.y, 0.0)),
            );
        }
    }
    let glazing = Glazing { double_hung: true };
    for (i, opening) in windows.iter().enumerate() {
        let seed = f64::from(design.variant) * 3.0 + (index * 5 + i) as f64;
        let upper = i >= ground_floor;
        let boarded = !role.door
            && design.garage.is_none()
            && Random::new(seed + 0.7).next() < BOARDED_WINDOWS;
        if boarded {
            boarded_window(kit, frame, opening, seed);
        } else if stucco {
            tract_window(kit, frame, opening, REVEAL, Surface::Siding);
        } else if upper {
            // Bedroom casements upstairs: one sash of four lites.
            window(kit, frame, opening, Glazing { double_hung: false }, REVEAL);
        } else {
            window(kit, frame, opening, glazing, REVEAL);
        }
        if stucco {
            // A tract house shutters its street front alone, upstairs as well, in broad
            // board-and-batten leaves, but not the windows a bay, front gable or balcony
            // stands in front of over its garage.
            let floor = if upper {
                &windows[ground_floor..]
            } else {
                &windows[..ground_floor]
            };
            let doors: Vec<Opening> = if upper {
                Vec::new()
            } else {
                door_opening.into_iter().chain(garage_opening).collect()
            };
            let fronted = upper && under_garage(design, opening.u) && !design.spread_upper();
            if !role.door || fronted {
                continue;
            }
            // The reference house hangs one leaf, beside its upper window away from the garage.
            let away = -design.garage.map_or(1.0, |garage| garage.sign());
            for (side, width) in tract_shutters_beside(opening, half, floor, &doors) {
                let hung = match design.style.shutters {
                    Shutters::BoardLeaf => upper && side == away,
                    Shutters::None => false,
                    Shutters::Louvred | Shutters::Board => true,
                };
                if hung {
                    batten_shutter(kit, frame, opening, width, side);
                }
            }
            continue;
        }
        let room = if garage_opening.is_some() {
            shutter_room_among(opening, half, &openings)
        } else {
            let inner_limit = if role.door { door_edge } else { 0.0 };
            shutter_room(opening, half, inner_limit, ground_floor == 1)
        };
        // Shutters dress the street front and the sides; the back of the
        // cottage, behind its door, makes do without.
        let back = role.planted && !role.door;
        // The upper floor's windows go without: shutters on both floors
        // cost more triangles than the second row of windows itself.
        if room > 0.0 && !back && !upper {
            match design.style.shutters {
                Shutters::Louvred => shutters(kit, frame, opening, room),
                Shutters::Board => board_shutters(kit, frame, opening, room),
                Shutters::BoardLeaf | Shutters::None => {}
            }
        }
        // Tract houses keep no window boxes.
        if role.planted && !upper && design.garage.is_none() {
            window_box(kit, frame, opening);
        }
    }
    if two_storey && (!stucco || design.style.batten_upper) {
        // The belt course between the floors, broken by the door's porch.
        kit.block(
            Surface::Trim,
            DVec3::new(2.0 * half, BELT, TRIM_PROUD * 1.5),
            frame * shift(DVec3::new(0.0, roof.belt_line(), TRIM_PROUD * 0.75)),
        );
    }
    if design.style.batten_upper {
        // The upper floor clad in board-and-batten over the stucco below.
        let upper: Vec<Opening> = windows[ground_floor..].to_vec();
        batten_cladding(
            kit,
            frame,
            (-half, half),
            (roof.belt_line() + BELT / 2.0, roof.wall),
            &upper,
        );
    }
    let entry = entry(design);
    let lamp = if stucco { TRACT_LANTERN } else { 1.0 };
    if let Some(door_opening) = &door_opening {
        if stucco {
            tract_door(kit, frame, door_opening, REVEAL);
        } else {
            door(kit, frame, door_opening, REVEAL);
        }
        match entry {
            Entry::Canopy => canopy(kit, frame, door_opening),
            Entry::GablePorch => porch(kit, frame, half, roof),
            // Built out from the wall with the roof's covering, below.
            Entry::BrickWing
            | Entry::ArchWing
            | Entry::BrickArch
            | Entry::SkirtPorch
            | Entry::RoomPorch => {}
        }
        // A wing's lantern hangs on its face beside the entry (below).
        if !entry.enclosed() {
            lantern(
                kit,
                frame,
                door_opening.u + door_edge + 0.14,
                door_opening.top() - 0.45,
                lamp,
            );
        }
    }
    if let Some(garage) = &garage_opening {
        let paint = match design.style.garage_door {
            GaragePaint::Stained if stucco => Surface::GarageDoor,
            GaragePaint::Stained => Surface::Wood,
            GaragePaint::Accent => Surface::Accent,
            GaragePaint::Trim => Surface::Trim,
        };
        garage_door(
            kit,
            frame,
            garage,
            REVEAL,
            paint,
            stucco,
            design.style.garage_look,
        );
        // A lantern beside it on the corner's side.
        let outward = garage.u.signum();
        lantern(
            kit,
            frame,
            garage.u + outward * (garage.width / 2.0 + CASING + 0.2),
            roof.plinth + GARAGE_HEIGHT + 0.05,
            lamp,
        );
    }
    if vent {
        gable_vent(kit, frame, &vent_opening);
    }
    wall_trim(
        kit,
        frame,
        half,
        roof,
        index < 2,
        role.gable,
        design.style.walls,
        door_opening.map(|door| (door.u, door_edge)),
    );
    if role.planted && design.garage.is_none() {
        // The plants ride on the wall's parts too.
        let slot = parts.slot(piece);
        window_plants(
            &mut parts.nodes[slot],
            frame,
            role,
            &windows[..ground_floor],
            design.variant,
        );
    }
    if let Some(door_opening) = door_opening {
        // A garage house (Senda's) keeps no foundation beds: its front beds are the
        // street's scenery, planted there with shrubs of their own.
        let bedded = design.garage.is_none();
        if bedded {
            let beds = flower_beds(door_opening.u, half, garage_opening, entry);
            foundation_bed(parts, frame, &beds, design.variant);
        }
        if let Some(wing) = wing_plan(design) {
            match entry {
                Entry::BrickWing | Entry::ArchWing | Entry::BrickArch => {
                    front_wing(parts, design, roof, piece, &wing, entry);
                    if bedded {
                        foundation_bed(
                            parts,
                            frame * shift(DVec3::Z * WING_DEPTH),
                            &wing.beds(entry, design.style.loggia),
                            design.variant,
                        );
                    }
                }
                Entry::SkirtPorch => skirt_porch(parts, design, roof, piece, &wing),
                Entry::RoomPorch => {
                    room_porch(parts, design, roof, piece, &wing);
                    if bedded {
                        foundation_bed(
                            parts,
                            frame * shift(DVec3::Z * WING_DEPTH),
                            &wing.beds(entry, false),
                            design.variant,
                        );
                    }
                }
                _ => {}
            }
        }
        // A two-storey front gable (a Spanish house's over its entry side, the reference house's over
        // its garage round a box bay), or over the garage a Craftsman's box bay or a
        // balcony.
        match (garage_opening, garage_gable_side(design)) {
            (Some(garage), Some(side)) => {
                garage_gable(parts, design, roof, piece, garage.u, side);
            }
            (Some(garage), None) if design.box_bay() && two_storey => {
                garage_bay(parts, design, roof, piece, garage.u, 0.0);
            }
            (Some(garage), None) if design.style.balcony && two_storey => {
                garage_balcony(parts, design, roof, piece, garage.u);
            }
            _ => {}
        }
        // A Prairie front's small cross gable over the pair of windows over its porch,
        // breaking its eave, its roof running back into the main slope.
        if design.spread_upper() && two_storey {
            let u = door_opening.u;
            let half = CROSS_GABLET;
            wing_gable(
                parts,
                design,
                frame * shift(DVec3::Z * -WING_DEPTH),
                piece,
                (u - half, u + half),
                roof.wall,
                Surface::Siding,
            );
        }
    }
    if stucco && index == back_wall(design) && design.variant.is_multiple_of(2) {
        patio_cover(parts, design, roof, index);
    }
    if role.door
        && design.style.loggia
        && let (Some(wing), Some(garage)) = (wing_plan(design), garage_opening)
    {
        front_chimney(parts.kit(piece), frame, roof, &wing);
        garage_arches(parts.kit(piece), frame, &garage);
    }
}

/// Rumbo north lot 1's chimney: a tall stucco stack standing against the front wall where the
/// wing (`wing`, along the door wall `frame` is on) meets the two-storey house, rising
/// through the eave past the ridge to a stucco cap under a little clay hood over its flue.
fn front_chimney(kit: &mut Kit<Surface>, frame: DMat4, roof: &Roof, wing: &WingPlan) {
    let (wide, deep) = FRONT_CHIMNEY;
    let toward = wing.toward_garage;
    let garage_end = if toward > 0.0 { wing.to } else { wing.from };
    let u = garage_end - toward * (wide / 2.0 + FRONT_CHIMNEY_INSET);
    let top = roof.ridge + FRONT_CHIMNEY_OVER;
    let bottom = roof.plinth + WING_WALL;
    kit.block(
        Surface::Siding,
        DVec3::new(wide, top - bottom, deep),
        frame * shift(DVec3::new(u, (top + bottom) / 2.0, deep / 2.0)),
    );
    kit.block(
        Surface::Siding,
        DVec3::new(wide + 0.08, 0.1, deep + 0.08),
        frame * shift(DVec3::new(u, top + 0.05, deep / 2.0)),
    );
    kit.block(
        Surface::Dark,
        DVec3::new(wide * 0.5, 0.12, deep * 0.5),
        frame * shift(DVec3::new(u, top + 0.16, deep / 2.0)),
    );
    for side in [-1.0, 1.0] {
        kit.block(
            Surface::Terracotta,
            DVec3::new(wide * 0.62, 0.04, deep + 0.12),
            frame
                * pose(
                    DVec3::new(u + side * wide * 0.22, top + 0.27, deep / 2.0),
                    DVec3::new(0.0, 0.0, -side * 0.42),
                ),
        );
    }
}

/// Rumbo north lot 1's garage front: three shallow segmental arches over its garage door
/// (`garage`), a stucco band standing out from the wall on dark corbels at their springs.
fn garage_arches(kit: &mut Kit<Surface>, frame: DMat4, garage: &Opening) {
    const SIDES: usize = 6;
    let (proud, rise, band) = GARAGE_ARCHES;
    let (u0, u1) = (
        garage.u - garage.width / 2.0 - 0.1,
        garage.u + garage.width / 2.0 + 0.1,
    );
    let spring = garage.top() + 0.02;
    let top = spring + rise + band;
    let span = (u1 - u0) / 3.0;
    // The band's underside, left to right: three arcs rising `rise` at their crowns.
    let mut soffit = Vec::new();
    for arch in 0..3 {
        let a = u0 + span * arch as f64;
        for k in 0..=SIDES {
            if arch > 0 && k == 0 {
                continue;
            }
            let t = k as f64 / SIDES as f64;
            soffit.push(DVec2::new(a + span * t, spring + rise * (PI * t).sin()));
        }
    }
    let mut outline = soffit.clone();
    outline.extend([DVec2::new(u1, top), DVec2::new(u0, top)]);
    let face = frame * shift(DVec3::Z * proud);
    kit.face_with_holes(Surface::Siding, &outline, &[], face);
    let at = |p: DVec2, z: f64| frame.transform_point3(DVec3::new(p.x, p.y, z));
    for pair in soffit.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        kit.quad(
            Surface::Siding,
            [at(a, 0.0), at(b, 0.0), at(b, proud), at(a, proud)],
            frame.transform_vector3(DVec3::new(b.y - a.y, -(b.x - a.x), 0.0)),
        );
    }
    let corners = [DVec2::new(u0, top), DVec2::new(u1, top)];
    kit.quad(
        Surface::Siding,
        [
            at(corners[1], 0.0),
            at(corners[0], 0.0),
            at(corners[0], proud),
            at(corners[1], proud),
        ],
        frame.transform_vector3(DVec3::Y),
    );
    for (u, side) in [(u0, -1.0), (u1, 1.0)] {
        kit.quad(
            Surface::Siding,
            [
                at(DVec2::new(u, spring), 0.0),
                at(DVec2::new(u, spring), proud),
                at(DVec2::new(u, top), proud),
                at(DVec2::new(u, top), 0.0),
            ],
            frame.transform_vector3(DVec3::X * side),
        );
    }
    for k in 0..=3 {
        let u = (u0 + span * k as f64).clamp(u0 + 0.06, u1 - 0.06);
        kit.block(
            Surface::Wood,
            DVec3::new(0.1, 0.16, proud + 0.04),
            frame * shift(DVec3::new(u, spring - 0.06, (proud + 0.04) / 2.0)),
        );
    }
}

/// The wall behind the door wall.
fn back_wall(design: &HouseDesign) -> usize {
    design.door_wall ^ 1
}

/// Whether `u` along a garage house's door wall lies over its garage door: its upper
/// window there goes unshuttered, as the tract's do (a box bay or front gable in front
/// of it glazes its own face).
fn under_garage(design: &HouseDesign, u: f64) -> bool {
    let plan = design.wall_plan(design.door_wall);
    plan.garage()
        .is_some_and(|garage| (u - garage).abs() <= plan.garage_bay + 1e-6)
}

/// The roof with its covering, the chimney, the yard and the dormers: a cottage's
/// last section ([`PartsCut`]).
fn house_roof(parts: &mut Parts, design: &HouseDesign, roof: &Roof, build: Build) {
    let (w, d) = (design.w, design.d);
    let standing = build == Build::Standing;
    match roof.form {
        RoofForm::Gable => gable_deck(parts, roof),
        RoofForm::Hip => hip_deck(parts, roof),
    }
    if !standing {
        rafters(parts, design, roof);
    }
    for slope in slopes(roof) {
        if !standing && roof.form == RoofForm::Hip {
            slope_underside(parts.kit(slope.piece), &slope, roof.thick);
        }
        cover_slope(parts, design.style.cover, &slope);
    }
    ridge_caps(parts, design, roof);
    // A stucco house's gutters and downspouts are painted its walls' colour.
    let metal = if design.style.walls == WallFinish::Stucco {
        Surface::Siding
    } else {
        Surface::Metal
    };
    gutters(parts, w, d, roof, metal);
    if design.garage.is_some() {
        if design.style.cover == RoofCover::FlatTile && design.door_wall == 2 {
            eave_tails(parts, design, roof);
        }
        solar_arrays(parts, design, roof);
    }
    yard(&mut parts.lot, design);
    garden(&mut parts.lot, &mut parts.lot_nodes, design);
    if design.chimney() {
        chimney(parts.kit(HousePart::Chimney), design, roof);
    }
    // Some cottages light their attics with dormers, so the roofscape varies.
    if roof.form == RoofForm::Gable {
        let flue = design.flue();
        if let Some(wing) = design.wing() {
            cross_gable(parts, design, roof, &wing);
        }
        for (side, z) in design.dormers() {
            let clear = flue.x * side < 0.0 || (flue.z - z).abs() > DORMER_HALF + CHIMNEY;
            if clear {
                let piece = if side > 0.0 {
                    HousePart::RightRoof
                } else {
                    HousePart::LeftRoof
                };
                dormer(parts.kit(piece), roof, side, z);
            }
        }
    }
}

/// A side-gable cottage's cross gable over its door: the door wall rises past the
/// eave into a clapboard gable with an attic window under bargeboards, its roof
/// running back from the face into the main slope (shingled or seamed like it),
/// so the roof reads as a T from above. It sits on the right (+x) slope and
/// falls with that slope's slab.
fn cross_gable(parts: &mut Parts, design: &HouseDesign, roof: &Roof, wing: &Wing) {
    const SLAB: f64 = 0.08;
    let piece = HousePart::RightRoof;
    let Wing {
        z,
        half,
        face,
        base,
        wall,
        peak,
    } = *wing;
    let frame = pose(DVec3::new(face, base, z), DVec3::new(0.0, PI / 2.0, 0.0));
    let outline = [
        (-half, 0.0),
        (half, 0.0),
        (half, wall),
        (0.0, wall + peak),
        (-half, wall),
    ]
    .map(|(u, y)| DVec2::new(u, y));
    let opening = Opening {
        u: 0.0,
        y: 0.16,
        width: 0.62,
        height: (wall + peak * 0.5 - 0.26).clamp(0.4, 0.86),
    };
    let kit = parts.kit(piece);
    building_kit::wall(kit, frame, &outline, &[opening], REVEAL);
    window(kit, frame, &opening, Glazing { double_hung: false }, REVEAL);
    if design.style.walls == WallFinish::Clapboard {
        for u in [-half + CORNER_BOARD / 2.0, half - CORNER_BOARD / 2.0] {
            kit.block(
                Surface::Trim,
                DVec3::new(CORNER_BOARD, wall, TRIM_PROUD),
                frame * shift(DVec3::new(u, wall / 2.0, TRIM_PROUD / 2.0)),
            );
        }
    }
    // Cheek walls from the face back until the main slope swallows them.
    let crest = wing.crest();
    let eave = base + wall;
    let back = ((roof.ridge + roof.thick - eave) / roof.pitch).max(0.1);
    for s in [-1.0, 1.0] {
        kit.block(
            Surface::Siding,
            DVec3::new(face - back, wall, 0.1),
            shift(DVec3::new(
                (face + back) / 2.0,
                base + wall / 2.0,
                z + s * (half - 0.05),
            )),
        );
    }
    // The roof: a slab each side from the ridge out past the side walls, from an
    // overhang at the face back into the main slope.
    let slope = (peak / half).atan();
    let reach = half + WING_OVERHANG;
    let slant = reach / slope.cos();
    let fall = reach * slope.tan();
    let front = face + WING_OVERHANG;
    let rear = wing.ridge_end(roof) - 0.12;
    let run = front - rear;
    let mid = (front + rear) / 2.0;
    for s in [-1.0, 1.0] {
        let normal = DVec3::new(0.0, slope.cos(), s * slope.sin());
        let top = DVec3::new(mid, crest - fall / 2.0, z + s * reach / 2.0);
        let kit = parts.kit(piece);
        kit.block_at(
            Surface::Roof,
            DVec3::new(run, SLAB, slant + 0.04),
            top - normal * (SLAB / 2.0),
            DVec3::new(s * slope, 0.0, 0.0),
        );
        // Bargeboards up the face's rakes.
        kit.block_at(
            Surface::Trim,
            DVec3::new(0.05, 0.2, slant + 0.02),
            DVec3::new(front + 0.01, crest - fall / 2.0 - 0.08, z + s * reach / 2.0),
            DVec3::new(s * slope, 0.0, 0.0),
        );
        let wing_slope = Slope {
            piece,
            eave: DVec3::new(mid, crest - fall, z + s * reach),
            up: DVec3::new(0.0, fall, -s * reach).normalize(),
            across: DVec3::X,
            normal,
            length: slant,
            half_eave: run / 2.0,
            half_ridge: run / 2.0,
        };
        cover_slope(parts, design.style.cover, &wing_slope);
    }
    parts.kit(piece).block(
        Surface::Trim,
        DVec3::new(run, 0.07, 0.14),
        shift(DVec3::new(mid, crest + 0.04, z)),
    );
}

/// A gabled dormer on one roof slope (`side` ±1 along x) at `z`: a clapboard
/// front with a double-hung window under a little gable, cheeks running back into
/// the roof and a small roof in the roof's paint.
fn dormer(kit: &mut Kit<Surface>, roof: &Roof, side: f64, z: f64) {
    const PEAK: f64 = 0.3;
    const THICK: f64 = 0.06;
    // Low on the slope, just inside the wall line, and never above the ridge.
    let reach = (roof.eave_x * 0.72).min(roof.eave_x - roof.overhang - 0.1);
    let total = roof.pitch * reach - 0.12;
    if total < 1.0 {
        return;
    }
    let wall_height = (total - PEAK).min(1.1);
    let xf = side * reach;
    let base = roof.top(xf, z) - 0.08;
    let frame = pose(
        DVec3::new(xf, base, z),
        DVec3::new(0.0, side * PI / 2.0, 0.0),
    );
    let outline = [
        (-DORMER_HALF, 0.0),
        (DORMER_HALF, 0.0),
        (DORMER_HALF, wall_height),
        (0.0, wall_height + PEAK),
        (-DORMER_HALF, wall_height),
    ]
    .map(|(u, y)| DVec2::new(u, y));
    let opening = Opening {
        u: 0.0,
        y: 0.17,
        width: 0.56,
        height: (wall_height - 0.32).min(0.6),
    };
    building_kit::wall(kit, frame, &outline, &[opening], 0.08);
    window(kit, frame, &opening, Glazing { double_hung: true }, 0.08);
    // Corner boards up the front.
    for u in [-DORMER_HALF + 0.04, DORMER_HALF - 0.04] {
        kit.block(
            Surface::Trim,
            DVec3::new(0.08, wall_height, 0.03),
            frame * shift(DVec3::new(u, wall_height / 2.0, 0.015)),
        );
    }
    // The roof meets the dormer's eave line this far back up the slope.
    let back = side * (xf.abs() - (wall_height - 0.02) / roof.pitch).max(0.05);
    let length = (xf - back).abs();
    for s in [-1.0, 1.0] {
        kit.block(
            Surface::Siding,
            DVec3::new(length, wall_height, THICK),
            shift(DVec3::new(
                (xf + back) / 2.0,
                base + wall_height / 2.0,
                z + s * (DORMER_HALF - THICK / 2.0),
            )),
        );
    }
    // Roof slabs from the peak down past each cheek, from a small overhang at
    // the front back into the main roof.
    let slope = (PEAK / DORMER_HALF).atan();
    let slant = (DORMER_HALF * DORMER_HALF + PEAK * PEAK).sqrt() + 0.14;
    let run = length + 0.5;
    for s in [-1.0, 1.0] {
        let centre = DVec3::new(
            (xf + back) / 2.0 + side * 0.12,
            base + wall_height + PEAK / 2.0 + 0.05 - 0.07,
            z + s * (slant / 2.0 - 0.03) * slope.cos(),
        );
        kit.block_at(
            Surface::Roof,
            DVec3::new(run, 0.07, slant),
            centre,
            DVec3::new(s * slope, 0.0, 0.0),
        );
    }
    kit.block(
        Surface::Trim,
        DVec3::new(run, 0.06, 0.12),
        shift(DVec3::new(
            (xf + back) / 2.0 + side * 0.12,
            base + wall_height + PEAK + 0.04,
            z,
        )),
    );
}

/// Corner boards (clapboard only), the water table over the plinth (broken by the
/// door at `door`, its middle along the wall and its casing's half width; stucco
/// runs down to its footing without one) and, on eave walls, the frieze board under
/// the eaves.
#[allow(clippy::too_many_arguments)]
fn wall_trim(
    kit: &mut Kit<Surface>,
    frame: DMat4,
    half: f64,
    roof: &Roof,
    end_wall: bool,
    gable: bool,
    finish: WallFinish,
    door: Option<(f64, f64)>,
) {
    let corner_boards = finish == WallFinish::Clapboard;
    let block = |kit: &mut Kit<Surface>, size: DVec3, at: DVec3| {
        kit.block(Surface::Trim, size, frame * DMat4::from_translation(at));
    };
    const TABLE: f64 = 0.14;
    let board_bottom = roof.plinth + TABLE;
    // End-wall boards lap past the eave walls' boards to close the corner.
    let (board, reach) = if end_wall {
        (CORNER_BOARD + TRIM_PROUD, half + TRIM_PROUD)
    } else {
        (CORNER_BOARD, half)
    };
    if corner_boards {
        for side in [-1.0, 1.0] {
            block(
                kit,
                DVec3::new(board, roof.wall - board_bottom, TRIM_PROUD),
                DVec3::new(
                    side * (reach - board / 2.0),
                    (roof.wall + board_bottom) / 2.0,
                    TRIM_PROUD / 2.0,
                ),
            );
        }
    }
    let mut runs = vec![(-reach - 0.01, reach + 0.01)];
    if let Some((u, edge)) = door {
        runs = vec![(-reach - 0.01, u - edge), (u + edge, reach + 0.01)];
    }
    if finish == WallFinish::Stucco {
        runs.clear();
    }
    for (from, to) in runs {
        kit.chamfer_block(
            Surface::Trim,
            DVec3::new(to - from, TABLE, 0.04),
            0.012,
            frame
                * DMat4::from_translation(DVec3::new(
                    (from + to) / 2.0,
                    roof.plinth + TABLE / 2.0,
                    0.02,
                )),
        );
    }
    if !gable {
        block(
            kit,
            DVec3::new(2.0 * half, 0.18, 0.025),
            DVec3::new(0.0, roof.wall - 0.09, 0.0125),
        );
    }
}

/// A louvred gable vent: dark back, sloping slats and a plain casing.
fn gable_vent(kit: &mut Kit<Surface>, frame: DMat4, opening: &Opening) {
    let at = |p: DVec3| frame * DMat4::from_translation(p);
    kit.block(
        Surface::Dark,
        DVec3::new(opening.width, opening.height, 0.01),
        at(DVec3::new(
            opening.u,
            opening.y + opening.height / 2.0,
            -0.1,
        )),
    );
    let slats = 4;
    for i in 0..slats {
        let y = opening.y + opening.height * (f64::from(i) + 0.5) / f64::from(slats);
        kit.block(
            Surface::Trim,
            DVec3::new(opening.width, 0.012, 0.1),
            frame
                * pose(
                    DVec3::new(opening.u, y, -0.05),
                    DVec3::new(PI * 0.28, 0.0, 0.0),
                ),
        );
    }
    for side in [-1.0, 1.0] {
        kit.block(
            Surface::Trim,
            DVec3::new(0.08, opening.height + 0.16, TRIM_PROUD),
            at(DVec3::new(
                opening.u + side * (opening.width / 2.0 + 0.04),
                opening.y + opening.height / 2.0,
                TRIM_PROUD / 2.0,
            )),
        );
    }
    for y in [opening.y - 0.04, opening.y + opening.height + 0.04] {
        kit.block(
            Surface::Trim,
            DVec3::new(opening.width + 0.16, 0.08, TRIM_PROUD),
            at(DVec3::new(opening.u, y, TRIM_PROUD / 2.0)),
        );
    }
}

/// A stained window box on two brackets under a window's sill, its soil level
/// just below the rim. The plants are separate nodes ([`window_plants`]).
fn window_box(kit: &mut Kit<Surface>, frame: DMat4, opening: &Opening) {
    let (u, y) = (opening.u, opening.y);
    let width = opening.width + 0.2;
    kit.chamfer_block(
        Surface::Wood,
        DVec3::new(width, 0.2, 0.28),
        0.015,
        frame * DMat4::from_translation(DVec3::new(u, y - 0.29, 0.17)),
    );
    kit.block(
        Surface::Dark,
        DVec3::new(width - 0.06, 0.012, 0.22),
        frame * DMat4::from_translation(DVec3::new(u, y - 0.19, 0.17)),
    );
    for side in [-1.0, 1.0] {
        let x = u + side * (width / 2.0 - 0.12);
        let from = frame.transform_point3(DVec3::new(x, y - 0.62, 0.01));
        let to = frame.transform_point3(DVec3::new(x, y - 0.4, 0.25));
        let length = (to - from).length();
        kit.block(
            Surface::Wood,
            DVec3::new(0.04, 0.04, length),
            aim(from, to) * DMat4::from_translation(DVec3::Z * (length / 2.0)),
        );
        kit.block(
            Surface::Wood,
            DVec3::new(0.04, 0.24, 0.03),
            frame * DMat4::from_translation(DVec3::new(x, y - 0.51, 0.015)),
        );
    }
}

/// Shrubs and flowers in one wall's window boxes, jittered per cottage (`v` is
/// its variant).
#[allow(clippy::too_many_arguments)]
fn window_plants(nodes: &mut Vec<Node>, frame: DMat4, role: WallRole, windows: &[Opening], v: u32) {
    let octahedron = OCTAHEDRON.get_or_insert((), || octahedron_geometry(1.0, 0));
    let mut rng = Random::new(f64::from(v) + 17.0 + role.index as f64 * 3.0);
    let flower = FLOWERS[(v / 3) as usize % FLOWERS.len()];
    let mut group = Node::group("");
    for opening in windows {
        let u = opening.u;
        let width = opening.width + 0.08;
        let soil = opening.y - 0.18;
        // Overlapping leafy clumps along the box, flowers held above them.
        for i in 0..PLANT_CLUMPS {
            let along = u - width / 2.0 + width * (f64::from(i) + 0.5) / f64::from(PLANT_CLUMPS);
            let local = DVec3::new(
                along + rng.range(-0.03, 0.03),
                soil + rng.range(0.03, 0.07),
                0.17 + rng.range(-0.06, 0.06),
            );
            let leaves = if rng.next() < 0.5 {
                LEAVES
            } else {
                LEAVES_LIGHT
            };
            let mut clump = shadowed(octahedron.clone(), satin(leaves));
            clump.scale = DVec3::new(
                rng.range(0.12, 0.15),
                rng.range(0.09, 0.14),
                rng.range(0.09, 0.12),
            );
            clump.set_rotation_euler(0.0, rng.range(0.0, PI), 0.0);
            let at = frame.transform_point3(local);
            put(&mut group, clump, at.x, at.y, at.z);
        }
        for _ in 0..PLANT_BLOOMS {
            let local = DVec3::new(
                u + rng.range(-width / 2.0, width / 2.0),
                soil + rng.range(0.1, 0.18),
                0.17 + rng.range(-0.09, 0.09),
            );
            let mut bloom = shadowed(octahedron.clone(), satin(flower));
            let size = rng.range(0.028, 0.045);
            bloom.scale = DVec3::new(size, size * 0.7, size);
            let at = frame.transform_point3(local);
            put(&mut group, bloom, at.x, at.y, at.z);
        }
    }
    nodes.extend(group.children);
}

/// A bed along the door wall: `side` of `middle` along it, from `from` to `to` out from
/// `middle`.
#[derive(Clone, Copy)]
struct FlowerBed {
    middle: f64,
    side: f64,
    from: f64,
    to: f64,
}

/// The beds either side of the door's steps (`door` along its wall, `half` long either
/// way): a cottage's both reach to its corners; a garage front's stop short of the
/// garage door and keep clear of its entry (a porch has none).
fn flower_beds(door: f64, half: f64, garage: Option<Opening>, entry: Entry) -> Vec<FlowerBed> {
    const STEPS: f64 = 0.95;
    const SHORTEST: f64 = 0.5;
    let Some(garage) = garage else {
        let (from, to) = (STEPS, half - 0.2);
        if to - from < SHORTEST {
            return Vec::new();
        }
        return [-1.0, 1.0]
            .map(|side| FlowerBed {
                middle: 0.0,
                side,
                from,
                to,
            })
            .to_vec();
    };
    // A wing or porch beside the garage covers the wall; a wing plants before its face.
    if entry.depth() > 0.0 {
        return Vec::new();
    }
    let toward = (garage.u - door).signum();
    let from = STEPS;
    let garage_edge = (garage.u - door).abs() - garage.width / 2.0 - CASING - 0.15;
    let corner = half - 0.2 + door * toward;
    [(toward, garage_edge), (-toward, corner)]
        .into_iter()
        .filter(|&(_, to)| to - from >= SHORTEST)
        .map(|(side, to)| FlowerBed {
            middle: door,
            side,
            from,
            to,
        })
        .collect()
}

/// A bark-mulch bed for each of `beds`, each planted with two shrubs, so the cottage
/// sits in its yard rather than on bare ground.
fn foundation_bed(parts: &mut Parts, frame: DMat4, beds: &[FlowerBed], v: u32) {
    let octahedron = OCTAHEDRON.get_or_insert((), || octahedron_geometry(1.0, 0));
    let mut rng = Random::new(f64::from(v) * 7.0 + 29.0);
    let mut group = Node::group("");
    for &FlowerBed {
        middle,
        side,
        from,
        to,
    } in beds
    {
        let base = parts.ground();
        base.block(
            Surface::Wood,
            DVec3::new(to - from, 0.05, 0.62),
            frame * shift(DVec3::new(middle + side * (from + to) / 2.0, 0.0, 0.36)),
        );
        for u in [from + 0.3, to - 0.3] {
            let size = rng.range(SHRUB_SIZE.0, SHRUB_SIZE.1);
            for lump in 0..SHRUB_LUMPS {
                let scale = if lump == 0 { 1.0 } else { rng.range(0.6, 0.8) };
                let r = size * scale;
                let local = DVec3::new(
                    middle + side * u + rng.range(-0.18, 0.18) * f64::from(u8::from(lump > 0)),
                    r * 0.8 + rng.range(0.0, 0.08),
                    0.36 + rng.range(-0.1, 0.1),
                );
                let leaves = SHRUB_LEAVES[(lump + v as usize) % SHRUB_LEAVES.len()];
                let mut bush = shadowed(octahedron.clone(), satin(leaves));
                bush.scale = DVec3::new(r, r * 0.85, r);
                bush.set_rotation_euler(
                    rng.range(-0.4, 0.4),
                    rng.range(0.0, PI),
                    rng.range(-0.4, 0.4),
                );
                let at = frame.transform_point3(local);
                put(&mut group, bush, at.x, at.y, at.z);
            }
        }
    }
    parts.ground_nodes().extend(group.children);
}

/// `a` taken `t` of the way to `b`, per channel.
fn mix_hex(a: u32, b: u32, t: f64) -> u32 {
    let channel = |c: u32, shift: u32| f64::from((c >> shift) & 0xff);
    [16, 8, 0].into_iter().fold(0, |out, shift| {
        let v = channel(a, shift) + (channel(b, shift) - channel(a, shift)) * t;
        out | ((v.round().clamp(0.0, 255.0) as u32) << shift)
    })
}

/// A small gabled canopy over the door on two knee brackets.
fn canopy(kit: &mut Kit<Surface>, frame: DMat4, opening: &Opening) {
    const PROJECT: f64 = 0.62;
    const HALF: f64 = 0.7;
    const RISE: f64 = 0.3;
    const SLAB: f64 = 0.045;
    let base = opening.top() + 0.27;
    let frame = frame * shift(DVec3::new(opening.u, 0.0, 0.0));
    let at = |u: f64, y: f64, z: f64| frame.transform_point3(DVec3::new(u, y, z));
    for side in [-1.0, 1.0] {
        let ring = |z: f64| {
            vec![
                at(side * HALF, base, z),
                at(0.0, base + RISE, z),
                at(0.0, base + RISE + SLAB, z),
                at(side * HALF, base + SLAB, z),
            ]
        };
        kit.solid(Surface::Roof, &[ring(0.0), ring(PROJECT)]);
        // Fascia under each roof edge.
        kit.block(
            Surface::Trim,
            DVec3::new(0.03, 0.09, PROJECT),
            frame
                * DMat4::from_translation(DVec3::new(
                    side * (HALF - 0.04),
                    base - 0.03,
                    PROJECT / 2.0,
                )),
        );
    }
    // Pediment infill and its bottom board, set back from the roof edge.
    let pediment = |z: f64| {
        vec![
            at(-HALF + 0.06, base - 0.07, z),
            at(HALF - 0.06, base - 0.07, z),
            at(0.0, base + RISE - 0.04, z),
        ]
    };
    kit.solid(
        Surface::Trim,
        &[pediment(PROJECT - 0.08), pediment(PROJECT - 0.04)],
    );
    for side in [-1.0, 1.0] {
        let u = side * (HALF - 0.12);
        kit.block(
            Surface::Trim,
            DVec3::new(0.07, 0.42, 0.06),
            frame * DMat4::from_translation(DVec3::new(u, base - 0.25, 0.03)),
        );
        let from = at(u, base - 0.42, 0.04);
        let to = at(u, base - 0.06, PROJECT - 0.1);
        let length = (to - from).length();
        kit.block(
            Surface::Trim,
            DVec3::new(0.055, 0.07, length),
            aim(from, to) * DMat4::from_translation(DVec3::Z * (length / 2.0)),
        );
    }
}

/// A shed porch roof across the middle of a front-gable door wall: a seamed slab
/// sloping out over the door and the windows beside it on three brackets, with a
/// fascia along its front and sides. It stays under the gable's rakes and clear
/// of the windows' heads, and needs no posts, so tanks pass beneath its edge.
fn porch(kit: &mut Kit<Surface>, frame: DMat4, half: f64, roof: &Roof) {
    const PROJECT: f64 = 1.15;
    const FALL: f64 = 0.32;
    const SLAB: f64 = 0.07;
    const SEAMS: usize = 7;
    let reach = half * 0.74;
    let base = (PLINTH + DOOR_HEIGHT + 0.42).min(roof.wall + (roof.ridge - roof.wall) * 0.12);
    let at = |u: f64, y: f64, z: f64| frame.transform_point3(DVec3::new(u, y, z));
    let ring = |u: f64| {
        vec![
            at(u, base, 0.02),
            at(u, base - FALL, PROJECT),
            at(u, base - FALL + SLAB, PROJECT),
            at(u, base + SLAB, 0.02),
        ]
    };
    kit.solid(Surface::Roof, &[ring(-reach), ring(reach)]);
    // Standing seams down the slope.
    let slope = DVec3::new(0.0, -FALL, PROJECT - 0.02);
    let length = slope.length();
    for k in 0..SEAMS {
        let u = -reach + 2.0 * reach * (k as f64 + 0.5) / SEAMS as f64;
        let from = at(u, base + SLAB + 0.015, 0.02);
        let to = at(u, base - FALL + SLAB + 0.015, PROJECT);
        kit.block(
            Surface::Roof,
            DVec3::new(0.03, 0.03, length),
            aim(from, to) * DMat4::from_translation(DVec3::Z * (length / 2.0)),
        );
    }
    // Fascia along the front edge and down both sides.
    kit.block(
        Surface::Trim,
        DVec3::new(2.0 * reach + 0.04, 0.12, 0.035),
        frame * DMat4::from_translation(DVec3::new(0.0, base - FALL - 0.02, PROJECT + 0.0175)),
    );
    for side in [-1.0, 1.0] {
        let from = at(side * (reach + 0.0175), base - 0.03, 0.02);
        let to = at(side * (reach + 0.0175), base - FALL - 0.03, PROJECT);
        let length = (to - from).length();
        kit.block(
            Surface::Trim,
            DVec3::new(0.035, 0.11, length),
            aim(from, to) * DMat4::from_translation(DVec3::Z * (length / 2.0)),
        );
    }
    // Brackets from the wall up under the slab: at both ends and the middle.
    for u in [-reach + 0.12, 0.0, reach - 0.12] {
        let from = at(u, base - 0.75, 0.03);
        let to = at(u, base - FALL * 0.7 - 0.02, PROJECT * 0.72);
        let length = (to - from).length();
        kit.block(
            Surface::Trim,
            DVec3::new(0.07, 0.08, length),
            aim(from, to) * DMat4::from_translation(DVec3::Z * (length / 2.0)),
        );
        kit.block(
            Surface::Trim,
            DVec3::new(0.08, 0.5, 0.06),
            frame * DMat4::from_translation(DVec3::new(u, base - 0.32, 0.03)),
        );
    }
}

/// How a house dresses its front door.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    /// A small gabled canopy on brackets.
    Canopy,
    /// A front-gable cottage's shed porch roof on brackets across its door wall.
    GablePorch,
    /// A garage front's one-storey brick-veneer wing beside the garage under a front
    /// gable, a shuttered window in its face and the door in its side by the garage
    /// (Craftsman).
    BrickWing,
    /// A garage front's one-storey brick porch beside the garage under a flat-tile shed
    /// roof, its piers framing a square opening the door stands back behind (Rumbo north lot 3's).
    BrickArch,
    /// A garage front's one-storey stucco wing beside the garage under a front gable, the
    /// door standing back behind an arch in its face (Spanish).
    ArchWing,
    /// A garage front's open porch beside the garage under a shed skirt roof on slim
    /// posts (Prairie).
    SkirtPorch,
    /// A garage front's one-storey stucco room beside the garage under a small hip that
    /// runs on past it over an open porch on dark timber posts, the door in the room's side
    /// under the porch (the reference house's).
    RoomPorch,
}

impl Entry {
    /// How far (m) out from the wall the steps up to the door start.
    fn depth(self) -> f64 {
        match self {
            Entry::BrickWing
            | Entry::ArchWing
            | Entry::BrickArch
            | Entry::SkirtPorch
            | Entry::RoomPorch => WING_DEPTH,
            Entry::Canopy | Entry::GablePorch => 0.0,
        }
    }

    /// Whether the door stands back inside a walled wing rather than on the open front.
    fn enclosed(self) -> bool {
        matches!(
            self,
            Entry::BrickWing | Entry::ArchWing | Entry::BrickArch | Entry::RoomPorch
        )
    }
}

/// A house's entry: a cottage's canopy, or a front-gable cottage's porch; a garage front's
/// one-storey wing (brick under a hip roof where its type has one, otherwise stucco under
/// a front gable), or the open skirt-roofed porch of a type with a porch and no wing.
fn entry(design: &HouseDesign) -> Entry {
    let style = design.style;
    match (design.garage, style.porch, style.form) {
        (None, true, RoofForm::Gable) if design.door_wall == 0 => Entry::GablePorch,
        (None, ..) => Entry::Canopy,
        (Some(_), false, _) => Entry::ArchWing,
        (Some(_), true, _) if style.wing => Entry::BrickWing,
        (Some(_), true, _) if style.arched_porch => Entry::BrickArch,
        (Some(_), true, _) if style.room_porch => Entry::RoomPorch,
        (Some(_), true, _) => Entry::SkirtPorch,
    }
}

/// A garage front's one-storey wing: how far (m) it stands out from the wall, its walls'
/// height over the slab (under the upper floor's sills), its entry opening's width and
/// height, the narrowest and widest window its face takes beside the entry and the
/// sill's height over the slab, and the face it keeps beside an opening.
const WING_DEPTH: f64 = 2.0;
const WING_WALL: f64 = 1.98;
const WING_ENTRY: (f64, f64) = (0.9, 1.74);
const WING_WINDOW: (f64, f64) = (0.42, 0.62);
const WING_SILL: f64 = 0.72;
const WING_EDGE: f64 = 0.12;
/// How far the entry may stand off the door behind it (m), so the door shows through it.
const WING_ENTRY_SHIFT: f64 = 0.4;
/// How far a wing's roof overhangs its face and sides, and its front gable's pitch (rise
/// per metre across): a Spanish wing's low, a brick wing's lower still, broad and low
/// across nearly half the front as the tract's are.
const WING_EAVE: f64 = 0.18;
const WING_GABLE_PITCH: f64 = 0.46;
const BRICK_GABLE_PITCH: f64 = 0.4;
/// A wing's front gable's roof slab thickness (m).
const WING_SLAB: f64 = 0.08;
/// Rumbo north lot 1's wing: its arched feature window's widest, its sill over the slab and its
/// tallest, the raised surround's band and how far it stands proud, the open arch in its
/// side's width and how deep the passage behind it runs to the door (m).
const LOGGIA_WINDOW: (f64, f64, f64) = (0.58, 0.62, 0.98);
const LOGGIA_SURROUND: (f64, f64) = (0.08, 0.035);
const LOGGIA_ARCH: f64 = 0.8;
const LOGGIA_PASSAGE: f64 = 0.5;
/// The wall the loggia's face keeps between its arch and the feature window's surround,
/// and the length of one of the surround's decorative tiles (m).
const LOGGIA_PIER: f64 = 0.12;
const LOGGIA_TILE: f64 = 0.09;
/// A brick front-gable wing's walls over the slab and its shuttered window in its face (the
/// widest and its sill over the slab); the door recessed at its garage end: the door's width
/// and height, how deep the recess runs back into the wing, the pier the face keeps
/// between it and the wing's garage side, the lintel over it and the jamb room either side
/// of the door (m).
const BRICK_WING_WALL: f64 = 1.72;
const BRICK_WING_WINDOW: (f64, f64) = (0.66, 0.62);
/// The share of a brick wing's face beside its recess its window takes at most, leaving
/// room for a shutter either side.
const BRICK_WING_WINDOW_SHARE: f64 = 0.45;
const RECESS_DOOR: (f64, f64) = (0.7, 1.56);
const RECESS_DEPTH: f64 = 0.6;
const RECESS_PIER: f64 = 0.1;
const RECESS_LINTEL: f64 = 0.2;
const RECESS_JAMB: f64 = 0.07;
const RECESS_DOOR_REVEAL: f64 = 0.08;
/// A door in the side of a room or loggia: its width and height, where it stands out from
/// the wall and how far it sits back in the side (m).
const SIDE_DOOR: (f64, f64) = (0.72, 1.66);
const SIDE_DOOR_AT: f64 = WING_DEPTH * 0.55;
const SIDE_DOOR_REVEAL: f64 = 0.1;
/// How far under the upper floor's sills a wing's or porch's roof keeps (m), so the
/// windows over it stay in view.
const UNDER_SILLS: f64 = 0.06;
/// How far a window in a wing's face sits back in it (m).
const WING_REVEAL: f64 = 0.1;
/// A skirt roof's fall per metre out (enough that its slope shows from the street), its
/// chunky dark posts' and their stacked-stone piers' plan size and the piers' height,
/// and Neblina's corner house's white square columns (m).
const SKIRT_FALL: f64 = 0.3;
const SKIRT_POST: f64 = 0.22;
const SKIRT_BASE: (f64, f64) = (0.38, 0.5);
const SKIRT_COLUMN: f64 = 0.2;
/// A porch roof's exposed rafter tails: their width and depth and their spacing (m).
const RAFTER_TAIL: (f64, f64) = (0.06, 0.08);
const RAFTER_TAIL_SPACING: f64 = 0.32;
/// Board-and-batten cladding: how far its boards stand off the stucco, its battens'
/// spacing, width and how far they stand proud of the boards (m).
const CLADDING: (f64, f64, f64, f64) = (0.012, 0.24, 0.04, 0.014);
/// A room and porch's share of its width the room takes, by the garage, its hip's pitch
/// (lower than a brick porch's) and its posts' plan size (m).
const ROOM_SHARE: f64 = 0.6;
const ROOM_HIP_PITCH: f64 = 0.36;
const ROOM_POST: f64 = 0.15;
/// How far (m) past the house's corner a room and porch's hip runs on over its post.
const ROOM_PORCH_REACH: f64 = 0.6;
/// Half the width (m) of a Prairie front's small cross gable over its porch-side windows.
const CROSS_GABLET: f64 = 0.72;
/// Rumbo north lot 1's front chimney: its width along the wall and how far it stands out from it,
/// how far in from the wing's garage end it stands and how far it rises past the ridge,
/// and its garage arches' band: how far it stands out, its arches' rise and the band
/// over their crowns (m).
const FRONT_CHIMNEY: (f64, f64) = (0.52, 0.44);
const FRONT_CHIMNEY_INSET: f64 = 0.12;
const FRONT_CHIMNEY_OVER: f64 = 0.55;
const GARAGE_ARCHES: (f64, f64, f64) = (0.18, 0.13, 0.14);
/// A rear patio cover: its width and how far it reaches out from the back wall, its
/// posts' plan size and its fall per metre out (m).
const PATIO_COVER: (f64, f64) = (2.4, 1.5);
const PATIO_POST: f64 = 0.13;
const PATIO_FALL: f64 = 0.24;
/// How much smaller a tract house's lanterns are than a cottage's.
const TRACT_LANTERN: f64 = 0.7;
/// A porch or shed roof's deck thickness (m).
const PORCH_DECK: f64 = 0.1;
/// A sectional garage door's boards' width and its handles' spacing either side of its
/// middle (m).
const GARAGE_PLANK: f64 = 0.12;
const GARAGE_HANDLES: f64 = 0.12;
/// A panelled garage door: its panels across and up, the stiles round them and the rails
/// between them, how far they stand proud, its X braces' width and a carriage door's
/// strap hinges' length (m).
const CARRIAGE_PANELS: (usize, usize) = (4, 2);
const CARRIAGE_STILE: f64 = 0.08;
const CARRIAGE_RAIL: f64 = 0.06;
const CARRIAGE_RAISE: f64 = 0.018;
const CARRIAGE_BRACE: f64 = 0.06;
const CARRIAGE_HINGE: f64 = 0.3;

/// A garage front's wing (or skirt porch) along the door wall: from the corner to just
/// short of the garage door's column, the way along the wall the garage lies, the entry
/// opening's middle in its face and, where the face has room by the corner, a window's
/// middle and width.
struct WingPlan {
    from: f64,
    to: f64,
    toward_garage: f64,
    entry: f64,
    window: Option<(f64, f64)>,
    /// The front door's middle in the wall behind.
    door: f64,
}

impl WingPlan {
    /// The wing's ends along the wall: by the garage, and by the corner.
    fn ends(&self) -> (f64, f64) {
        if self.toward_garage > 0.0 {
            (self.to, self.from)
        } else {
            (self.from, self.to)
        }
    }

    /// The opening of a brick wing's door recess in its face, at its garage end inside a
    /// pier: its middle along the wall and its width.
    fn recess(&self) -> (f64, f64) {
        let (garage_end, _) = self.ends();
        let width = RECESS_DOOR.0 + 2.0 * RECESS_JAMB;
        (
            garage_end - self.toward_garage * (RECESS_PIER + width / 2.0),
            width,
        )
    }

    /// A brick wing's face beside its recess, from the corner end to the recess's pier
    /// (the span its window centres in), as `(from, to)` along the wall.
    fn brick_face(&self) -> (f64, f64) {
        let (_, corner_end) = self.ends();
        let (middle, width) = self.recess();
        let edge = middle - self.toward_garage * (width / 2.0 + RECESS_PIER);
        (corner_end.min(edge), corner_end.max(edge))
    }

    /// A room and porch's spans along the wall: the room's by the garage, at least
    /// [`ROOM_SHARE`] of the wing and over the door in the wall behind (the way in is the
    /// door in its side), the porch's by the corner.
    fn room_porch(&self) -> ((f64, f64), (f64, f64)) {
        let width = self.to - self.from;
        let past_door = DOOR_WIDTH / 2.0 + SURROUND.0 + 0.06;
        let split = if self.toward_garage > 0.0 {
            (self.to - width * ROOM_SHARE).min(self.door - past_door)
        } else {
            (self.from + width * ROOM_SHARE).max(self.door + past_door)
        };
        if self.toward_garage > 0.0 {
            ((split, self.to), (self.from, split))
        } else {
            ((self.from, split), (split, self.to))
        }
    }

    /// The middle of Rumbo north lot 1's open arch along the wall, at its face's corner end.
    fn loggia_arch(&self) -> f64 {
        let corner_end = if self.toward_garage > 0.0 {
            self.from
        } else {
            self.to
        };
        corner_end + self.toward_garage * (WING_EDGE + LOGGIA_ARCH / 2.0)
    }

    /// The planted bed before the wing's face: from the corner to the steps, or along a
    /// brick wing's face up to its door recess, or under Rumbo north lot 1's feature window
    /// (`loggia`).
    fn beds(&self, entry: Entry, loggia: bool) -> Vec<FlowerBed> {
        let along = |(from, to): (f64, f64)| {
            vec![FlowerBed {
                middle: from,
                side: 1.0,
                from: 0.08,
                to: to - from - 0.08,
            }]
        };
        match entry {
            Entry::BrickWing => return along(self.brick_face()),
            Entry::RoomPorch => return along(self.room_porch().0),
            // Under Rumbo north lot 1's feature window, from its arch to the garage end.
            Entry::ArchWing if loggia => {
                let arch = self.loggia_arch();
                let garage_end = if self.toward_garage > 0.0 {
                    self.to
                } else {
                    self.from
                };
                return vec![FlowerBed {
                    middle: arch,
                    side: self.toward_garage,
                    from: LOGGIA_ARCH / 2.0 + 0.15,
                    to: (garage_end - arch).abs(),
                }];
            }
            _ => {}
        }
        let corner = if self.toward_garage > 0.0 {
            self.from
        } else {
            self.to
        };
        let reach = (self.entry - corner).abs() - WING_ENTRY.0 / 2.0 - 0.3;
        if reach < 0.5 {
            return Vec::new();
        }
        vec![FlowerBed {
            middle: self.entry,
            side: -self.toward_garage,
            from: WING_ENTRY.0 / 2.0 + 0.3,
            to: (self.entry - corner).abs() + 0.1,
        }]
    }
}

/// A garage front's [`WingPlan`]; none on a cottage. The entry stands toward the garage
/// as far as keeps the door behind it in view, leaving the face by the corner a window
/// where one fits, as the tract's wings have their entry by the garage and a room past it.
fn wing_plan(design: &HouseDesign) -> Option<WingPlan> {
    let (a, b) = design.door_side()?;
    let plan = design.wall_plan(design.door_wall);
    let (door, garage) = (plan.door()?, plan.garage()?);
    let toward = (garage - door).signum();
    let (from, to) = if toward > 0.0 {
        (a, b - 0.04)
    } else {
        (a + 0.04, b)
    };
    let (garage_end, corner_end) = if toward > 0.0 { (to, from) } else { (from, to) };
    let snug = garage_end - toward * (WING_EDGE + WING_ENTRY.0 / 2.0);
    let entry = (door + (snug - door).clamp(-WING_ENTRY_SHIFT, WING_ENTRY_SHIFT)).clamp(
        from + WING_EDGE + WING_ENTRY.0 / 2.0,
        (to - WING_EDGE - WING_ENTRY.0 / 2.0).max(from + WING_EDGE + WING_ENTRY.0 / 2.0),
    );
    let near = corner_end + toward * (WING_EDGE + 0.03);
    let far = entry - toward * (WING_ENTRY.0 / 2.0 + WING_EDGE + 0.03);
    let room = toward * (far - near);
    let window = (room >= WING_WINDOW.0).then(|| ((near + far) / 2.0, room.min(WING_WINDOW.1)));
    Some(WingPlan {
        from,
        to,
        toward_garage: toward,
        entry,
        window,
        door,
    })
}

/// A two-car garage door set back `depth` in its opening, made as `look`: the tract's
/// one-piece sectional door of vertical boards (most of them), of raised panels two rows
/// high and four across, or of two X-braced fields side by side, each with two small
/// handles low by its middle; or a two-leaf carriage door of raised panels under a meeting
/// seam, iron strap hinges on its outer stiles. Under a plain casing (a tract house's flat
/// stucco surround, `stucco`).
fn garage_door(
    kit: &mut Kit<Surface>,
    frame: DMat4,
    opening: &Opening,
    depth: f64,
    paint: Surface,
    stucco: bool,
    look: GarageLook,
) {
    let (u, w, y0, y1) = (opening.u, opening.width, opening.y, opening.top());
    let back = -depth + 0.03;
    let at = |u: f64, y: f64, z: f64| frame * shift(DVec3::new(u, y, z));
    let height = y1 - y0;
    kit.block(
        paint,
        DVec3::new(w, height, 0.04),
        at(u, (y0 + y1) / 2.0, back),
    );
    let face = back + 0.02;
    // Raised fields `columns` across and `rows` up inside the stiles and rails.
    let fields = |kit: &mut Kit<Surface>, columns: usize, rows: usize| {
        let (margin, gap) = (CARRIAGE_STILE, CARRIAGE_RAIL);
        let panel = DVec2::new(
            (w - 2.0 * margin - (columns - 1) as f64 * gap) / columns as f64,
            (height - 2.0 * margin - (rows - 1) as f64 * gap) / rows as f64,
        );
        let mut middles = Vec::new();
        for column in 0..columns {
            for row in 0..rows {
                let middle = DVec2::new(
                    u - w / 2.0 + margin + panel.x / 2.0 + column as f64 * (panel.x + gap),
                    y0 + margin + panel.y / 2.0 + row as f64 * (panel.y + gap),
                );
                kit.block(
                    paint,
                    DVec3::new(panel.x, panel.y, CARRIAGE_RAISE),
                    at(middle.x, middle.y, face + CARRIAGE_RAISE / 2.0),
                );
                middles.push(middle);
            }
        }
        (panel, middles)
    };
    match look {
        GarageLook::Boards => {
            // The grooves between the boards.
            let boards = (w / GARAGE_PLANK).round().max(2.0) as usize;
            for k in 1..boards {
                let x = u - w / 2.0 + w * k as f64 / boards as f64;
                kit.block(
                    Surface::Dark,
                    DVec3::new(0.014, height, 0.004),
                    at(x, (y0 + y1) / 2.0, face + 0.002),
                );
            }
        }
        GarageLook::Panels => {
            fields(kit, CARRIAGE_PANELS.0, CARRIAGE_PANELS.1);
        }
        GarageLook::XBrace => {
            // Each field crossed corner to corner by a pair of braces.
            let (panel, middles) = fields(kit, 2, 1);
            let length = panel.x.hypot(panel.y);
            for middle in middles {
                for lean in [1.0, -1.0] {
                    kit.block(
                        paint,
                        DVec3::new(length, CARRIAGE_BRACE, CARRIAGE_RAISE),
                        frame
                            * pose(
                                DVec3::new(middle.x, middle.y, face + CARRIAGE_RAISE * 1.5),
                                DVec3::new(0.0, 0.0, lean * panel.y.atan2(panel.x)),
                            ),
                    );
                }
            }
        }
        GarageLook::Carriage => {
            fields(kit, CARRIAGE_PANELS.0, CARRIAGE_PANELS.1);
            // The two leaves' meeting seam.
            kit.block(
                Surface::Dark,
                DVec3::new(0.02, height, 0.004),
                at(u, (y0 + y1) / 2.0, face + 0.002),
            );
            // Strap hinges on the outer stiles, top and bottom.
            for side in [-1.0, 1.0] {
                for y in [y0 + height * 0.2, y1 - height * 0.2] {
                    kit.block(
                        Surface::Iron,
                        DVec3::new(CARRIAGE_HINGE, 0.035, 0.02),
                        at(
                            u + side * (w / 2.0 - CARRIAGE_HINGE / 2.0 - 0.02),
                            y,
                            face + CARRIAGE_RAISE + 0.012,
                        ),
                    );
                }
            }
        }
    }
    // Two small handles by its middle: a carriage door's leaves' pulls, a sectional
    // door's lift handles low on its face.
    let (spread, handle_y) = if look == GarageLook::Carriage {
        (GARAGE_HANDLES, y0 + height * 0.42)
    } else {
        (GARAGE_HANDLES * 1.6, y0 + height * 0.3)
    };
    for side in [-1.0, 1.0] {
        kit.block(
            Surface::Iron,
            DVec3::new(0.03, 0.12, 0.03),
            at(u + side * spread, handle_y, face + 0.03),
        );
    }
    if stucco {
        surround(kit, frame, opening, Surface::Siding);
    } else {
        opening_casing(kit, frame, opening, false);
    }
}

/// The quads lining a hole (clockwise, in `frame`'s plane) `depth` back from its face.
fn reveal(kit: &mut Kit<Surface>, coat: Surface, frame: DMat4, hole: &[DVec2], depth: f64) {
    let center = hole.iter().copied().sum::<DVec2>() / hole.len() as f64;
    let at = |p: DVec2, z: f64| frame.transform_point3(DVec3::new(p.x, p.y, z));
    for k in 0..hole.len() {
        let (a, b) = (hole[k], hole[(k + 1) % hole.len()]);
        let inward = center - (a + b) / 2.0;
        kit.quad(
            coat,
            [at(a, 0.0), at(b, 0.0), at(b, -depth), at(a, -depth)],
            frame.transform_vector3(DVec3::new(inward.x, inward.y, 0.0)),
        );
    }
}

/// An arched opening's outline, clockwise as holes are given: up its left jamb from
/// `bottom`, over its round head (its crown at `top`) and down.
fn arch_outline(u: f64, width: f64, bottom: f64, top: f64) -> Vec<DVec2> {
    const SIDES: usize = 8;
    let radius = width / 2.0;
    let spring = (top - radius).max(bottom);
    let mut outline = vec![DVec2::new(u - radius, bottom)];
    outline.extend((0..=SIDES).map(|k| {
        let angle = PI * (1.0 - k as f64 / SIDES as f64);
        DVec2::new(u + radius * angle.cos(), spring + radius * angle.sin())
    }));
    outline.push(DVec2::new(u + radius, bottom));
    outline
}

/// An arched window in a face (`frame`, the face's plane): glass `depth` back in its
/// arched hole over white plantation shutters below its spring, a slim frame across the
/// spring and a plain stucco sill. The caller cuts
/// the hole and lines it.
fn arched_window(kit: &mut Kit<Surface>, frame: DMat4, opening: &Opening, depth: f64) {
    let (u, w, y0, y1) = (opening.u, opening.width, opening.y, opening.top());
    let outline = arch_outline(u, w, y0, y1);
    let glass: Vec<DVec2> = outline.iter().rev().copied().collect();
    kit.face_with_holes(
        Surface::Glass,
        &glass,
        &[],
        frame * shift(DVec3::Z * (0.02 - depth)),
    );
    let spring = y1 - w / 2.0;
    // White plantation shutters closed behind the glass under the arch's spring.
    let blinds_z = 0.028 - depth;
    let at = |u: f64, y: f64| frame.transform_point3(DVec3::new(u, y, blinds_z));
    kit.quad(
        Surface::Curtain,
        [
            at(u - w / 2.0, y0),
            at(u + w / 2.0, y0),
            at(u + w / 2.0, spring),
            at(u - w / 2.0, spring),
        ],
        frame.transform_vector3(DVec3::Z),
    );
    louvres(
        kit,
        frame,
        (u - w / 2.0, u + w / 2.0),
        (y0, spring),
        blinds_z - 0.009,
    );
    for (size, at) in [
        (
            DVec3::new(w, TRACT_FRAME, 0.04),
            DVec3::new(u, spring, 0.04 - depth),
        ),
        (
            DVec3::new(w, TRACT_FRAME, 0.04),
            DVec3::new(u, y0 + TRACT_FRAME / 2.0, 0.04 - depth),
        ),
        (
            DVec3::new(TRACT_FRAME, spring - y0, 0.04),
            DVec3::new(u, (y0 + spring) / 2.0, 0.04 - depth),
        ),
    ] {
        kit.block(Surface::Trim, size, frame * shift(at));
    }
    kit.block(
        Surface::Siding,
        DVec3::new(w + 0.12, TRACT_SILL.0, TRACT_SILL.1),
        frame * shift(DVec3::new(u, y0 - TRACT_SILL.0 / 2.0, TRACT_SILL.1 / 2.0)),
    );
}

/// The height (m) of a wing's or porch's walls (or the underside of its eave) whose roof
/// rises `rise` over them: as high as [`WING_WALL`] (or `wall`), but under the upper
/// floor's sills where the roof would hide the window over the door.
fn under_sills(roof: &Roof, wall: f64, rise: f64) -> f64 {
    (roof.plinth + wall).min(roof.upper_window().0 - UNDER_SILLS - rise)
}

/// A garage front's one-storey wing (`wing`) standing [`WING_DEPTH`] out from the door
/// wall beside the garage, as the tract's fronts step down from the two-storey block,
/// its sides closed back to the wall. A Craftsman's brick or stone wing has a shuttered
/// window in its face under a broad, low front gable with a small vent, the door recessed
/// in the face at its garage end; a Spanish house's stucco wing an arched entry the door stands back
/// behind and a window beside it under a low front gable with a clay vent; Rumbo north lot 3's
/// porch piers round a square entry under a shed roof, under the upper windows.
/// Its roofs are in the roof's covering. It goes with the door wall.
fn front_wing(
    parts: &mut Parts,
    design: &HouseDesign,
    roof: &Roof,
    piece: HousePart,
    wing: &WingPlan,
    entry: Entry,
) {
    let (frame, _) = wall_frames(design.w, design.d)[design.door_wall];
    let (from, to) = (wing.from, wing.to);
    let plinth = roof.plinth;
    let recessed = entry == Entry::BrickWing;
    let loggia = entry == Entry::ArchWing && design.style.loggia;
    let finish = if entry == Entry::ArchWing {
        Surface::Siding
    } else {
        masonry(design)
    };
    // Rumbo north lot 3's porch roof: a shed out from under the upper floor's sills.
    let shed_depth = WING_DEPTH + WING_EAVE;
    let shed_rise = shed_depth * SKIRT_FALL + PORCH_DECK;
    let top = match entry {
        Entry::BrickWing => plinth + BRICK_WING_WALL,
        Entry::BrickArch => under_sills(roof, WING_WALL, shed_rise),
        _ => plinth + WING_WALL,
    };
    let face = frame * shift(DVec3::Z * WING_DEPTH);
    let at = |u: f64, y: f64, z: f64| frame.transform_point3(DVec3::new(u, y, z));
    let facing = |x: f64, y: f64, z: f64| frame.transform_vector3(DVec3::new(x, y, z));
    let width = if loggia { LOGGIA_ARCH } else { WING_ENTRY.0 };
    let height = WING_ENTRY.1.min(top - plinth - 0.14);
    // Rumbo north lot 1's open arch stands at its face's corner end, the feature window beside it.
    let toward = wing.toward_garage;
    let corner_end = if toward > 0.0 { from } else { to };
    let entry_u = if loggia {
        wing.loggia_arch()
    } else {
        wing.entry
    };
    let (e0, e1) = (entry_u - width / 2.0, entry_u + width / 2.0);
    let entry_top = plinth + height;
    // Rumbo north lot 3's porch opens square between its piers; the others' entries are arched.
    let entry_hole = (!recessed).then(|| {
        if entry == Entry::BrickArch {
            [(e0, plinth), (e0, entry_top), (e1, entry_top), (e1, plinth)]
                .map(|(u, y)| DVec2::new(u, y))
                .to_vec()
        } else {
            arch_outline(entry_u, width, plinth, entry_top)
        }
    });
    // A brick wing's door recess: an opening in its face at its garage end.
    let recess = recessed.then(|| {
        let (middle, width) = wing.recess();
        Opening {
            u: middle,
            y: plinth,
            width,
            height: top - RECESS_LINTEL - plinth,
        }
    });
    let window = if loggia {
        // The arched feature window between the arch and the garage end.
        let (w, sill, tall) = LOGGIA_WINDOW;
        let y = plinth + sill;
        let near = entry_u + toward * (width / 2.0 + LOGGIA_PIER);
        let far = corner_end + toward * (to - from - WING_EDGE - LOGGIA_SURROUND.0);
        Some(Opening {
            u: (near + far) / 2.0,
            y,
            width: w.min((far - near).abs() - 2.0 * LOGGIA_SURROUND.0),
            height: tall.min(top - 0.2 - y),
        })
    } else if recessed {
        // Centred in the face beside the recess, shutters either side of it.
        let (w, sill) = BRICK_WING_WINDOW;
        let (a, b) = wing.brick_face();
        Some(Opening {
            u: (a + b) / 2.0,
            y: plinth + sill,
            width: w.min((b - a) * BRICK_WING_WINDOW_SHARE),
            height: top - 0.24 - plinth - sill,
        })
    } else {
        wing.window.map(|(u, w)| Opening {
            u,
            y: plinth + WING_SILL,
            width: w,
            height: entry_top - 0.05 - plinth - WING_SILL,
        })
    };
    let window_hole = window.map(|o| {
        if entry == Entry::ArchWing {
            arch_outline(o.u, o.width, o.y, o.top())
        } else {
            o.corners()
        }
    });
    let mut holes: Vec<Vec<DVec2>> = entry_hole.iter().cloned().collect();
    holes.extend(window_hole.clone());
    holes.extend(recess.map(|o| o.corners()));
    let outline = [(from, 0.0), (to, 0.0), (to, top), (from, top)].map(|(u, y)| DVec2::new(u, y));
    let kit = parts.kit(piece);
    kit.face_with_holes(finish, &outline, &holes, face);
    if let Some(recess) = &recess {
        door_recess(kit, frame, recess, finish);
    }
    if let (true, Some(entry_hole)) = (loggia, &entry_hole) {
        // An open loggia behind the arch, the door at its back.
        loggia_passage(
            kit, face, entry_hole, entry_u, width, plinth, entry_top, None,
        );
    } else if let Some(entry_hole) = &entry_hole {
        // The entry's jambs and head back to the wall, and its floor.
        for pair in entry_hole.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let middle = (a + b) / 2.0;
            let toward = DVec2::new(entry_u, plinth + height / 2.0) - middle;
            kit.quad(
                finish,
                [
                    at(a.x, a.y, WING_DEPTH),
                    at(b.x, b.y, WING_DEPTH),
                    at(b.x, b.y, 0.0),
                    at(a.x, a.y, 0.0),
                ],
                facing(toward.x, toward.y, 0.0),
            );
        }
        kit.quad(
            Surface::Concrete,
            [
                at(e0, plinth, 0.0),
                at(e1, plinth, 0.0),
                at(e1, plinth, WING_DEPTH),
                at(e0, plinth, WING_DEPTH),
            ],
            facing(0.0, 1.0, 0.0),
        );
    }
    if let (Some(opening), Some(hole)) = (window, window_hole) {
        reveal(kit, finish, face, &hole, WING_REVEAL);
        if loggia {
            // A raised surround of small decorative tiles round its arch, terracotta and
            // blue-grey in turn, with a clay sill under it.
            let (band, proud) = LOGGIA_SURROUND;
            let ring = arch_outline(
                opening.u,
                opening.width + band,
                opening.y - band / 2.0,
                opening.top() + band / 2.0,
            );
            let mut tile = 0;
            for pair in ring.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let run = b - a;
                let count = (run.length() / LOGGIA_TILE).round().max(1.0) as usize;
                for k in 0..count {
                    let middle = a + run * ((k as f64 + 0.5) / count as f64);
                    let coat = if tile % 2 == 0 {
                        Surface::Terracotta
                    } else {
                        Surface::Metal
                    };
                    kit.block(
                        coat,
                        DVec3::new(run.length() / count as f64 - 0.006, band, proud),
                        face * pose(
                            DVec3::new(middle.x, middle.y, proud / 2.0),
                            DVec3::new(0.0, 0.0, run.y.atan2(run.x)),
                        ),
                    );
                    tile += 1;
                }
            }
            kit.block(
                Surface::Terracotta,
                DVec3::new(opening.width + 2.0 * band + 0.06, 0.05, 0.08),
                face * shift(DVec3::new(opening.u, opening.y - band - 0.025, 0.04)),
            );
        }
        if entry == Entry::ArchWing {
            arched_window(kit, face, &opening, WING_REVEAL);
        } else {
            tract_window(kit, face, &opening, WING_REVEAL, Surface::Trim);
        }
        if recessed && design.style.shutters != Shutters::None {
            let (a, b) = wing.brick_face();
            let room = (opening.width * 0.5).min((b - a - opening.width) / 2.0 - SURROUND.0 - 0.03);
            if room >= TRACT_SHUTTER {
                for side in [-1.0, 1.0] {
                    batten_shutter(kit, face, &opening, room, side);
                }
            }
        }
    }
    // Its sides, closed back to the wall.
    for (u, side) in [(from, -1.0), (to, 1.0)] {
        kit.quad(
            finish,
            [
                at(u, 0.0, 0.0),
                at(u, 0.0, WING_DEPTH),
                at(u, top, WING_DEPTH),
                at(u, top, 0.0),
            ],
            facing(side, 0.0, 0.0),
        );
    }
    if finish != Surface::Siding {
        // A frieze board along a brick or stone face's top.
        kit.block(
            Surface::Trim,
            DVec3::new(to - from + 0.02, 0.12, 0.03),
            face * shift(DVec3::new((from + to) / 2.0, top - 0.06, 0.015)),
        );
    }
    match entry {
        Entry::BrickArch => {
            shed_roof(
                parts,
                design,
                piece,
                frame,
                (from - WING_EAVE, to + WING_EAVE),
                shed_depth,
                top + shed_rise,
                SKIRT_FALL,
                Surface::Trim,
            );
        }
        Entry::BrickWing => wing_gable(parts, design, frame, piece, (from, to), top, finish),
        _ => wing_gable(parts, design, frame, piece, (from, to), top, finish),
    }
}

/// A brick wing's door recess behind `recess`, the opening cut in its face (`frame` the
/// door wall's, the face [`WING_DEPTH`] out): lined in the wing's `finish` on its sides,
/// a stucco ceiling under the lintel and a concrete floor, and at its back a stucco wall
/// [`RECESS_DEPTH`] in holding the front door, a lantern beside it.
fn door_recess(kit: &mut Kit<Surface>, frame: DMat4, recess: &Opening, finish: Surface) {
    let (u, w, y0, y1) = (recess.u, recess.width, recess.y, recess.top());
    let (u0, u1) = (u - w / 2.0, u + w / 2.0);
    let (front, back) = (WING_DEPTH, WING_DEPTH - RECESS_DEPTH);
    let at = |u: f64, y: f64, z: f64| frame.transform_point3(DVec3::new(u, y, z));
    let facing = |x: f64, y: f64| frame.transform_vector3(DVec3::new(x, y, 0.0));
    for (side, inward) in [(u0, 1.0), (u1, -1.0)] {
        kit.quad(
            finish,
            [
                at(side, y0, back),
                at(side, y0, front),
                at(side, y1, front),
                at(side, y1, back),
            ],
            facing(inward, 0.0),
        );
    }
    kit.quad(
        Surface::Siding,
        [
            at(u0, y1, back),
            at(u1, y1, back),
            at(u1, y1, front),
            at(u0, y1, front),
        ],
        facing(0.0, -1.0),
    );
    kit.quad(
        Surface::Concrete,
        [
            at(u0, y0, front),
            at(u1, y0, front),
            at(u1, y0, back),
            at(u0, y0, back),
        ],
        facing(0.0, 1.0),
    );
    let wall = frame * shift(DVec3::Z * back);
    let door = Opening {
        u: u - (RECESS_JAMB - 0.04) * 0.5,
        y: y0,
        width: RECESS_DOOR.0,
        height: RECESS_DOOR.1.min(y1 - y0 - 0.1),
    };
    let outline = [(u0, y0), (u1, y0), (u1, y1), (u0, y1)].map(|(u, y)| DVec2::new(u, y));
    let hole = door.corners();
    kit.face_with_holes(Surface::Siding, &outline, std::slice::from_ref(&hole), wall);
    reveal(kit, Surface::Siding, wall, &hole, RECESS_DOOR_REVEAL);
    tract_door(kit, wall, &door, RECESS_DOOR_REVEAL);
    lantern(kit, wall, u1 - 0.07, door.top() - 0.42, TRACT_LANTERN * 0.8);
}

/// What a garage house's masonry entry (a Craftsman's wing, Rumbo north lot 3's arched porch) is
/// faced in: its veneer's brick, or stacked stone.
fn masonry(design: &HouseDesign) -> Surface {
    match design.style.veneer {
        Veneer::Brick => Surface::Brick,
        Veneer::Stone => Surface::Stone,
    }
}

/// Rumbo north lot 1's open loggia behind the arch `hole` cut in a face (`wall`, the face's plane;
/// the arch `width` wide round `middle`, its crown at `arch_top`): a passage behind the
/// arch lined in stucco to a second arch of wall holding the front door, which faces out
/// through it, and its floor; a lantern beside the arch on `lantern`'s side where given.
#[allow(clippy::too_many_arguments)]
fn loggia_passage(
    kit: &mut Kit<Surface>,
    wall: DMat4,
    hole: &[DVec2],
    middle: f64,
    width: f64,
    plinth: f64,
    arch_top: f64,
    lantern_side: Option<f64>,
) {
    // The passage: its jambs and head lined back to a stucco wall holding the door, and
    // its floor.
    let at = |u: f64, y: f64, z: f64| wall.transform_point3(DVec3::new(u, y, z));
    let centre = DVec2::new(middle, (plinth + arch_top) / 2.0);
    for pair in hole.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let inward = centre - (a + b) / 2.0;
        kit.quad(
            Surface::Siding,
            [
                at(a.x, a.y, 0.0),
                at(b.x, b.y, 0.0),
                at(b.x, b.y, -LOGGIA_PASSAGE),
                at(a.x, a.y, -LOGGIA_PASSAGE),
            ],
            wall.transform_vector3(DVec3::new(inward.x, inward.y, 0.0)),
        );
    }
    let back = wall * shift(DVec3::Z * -LOGGIA_PASSAGE);
    let filled: Vec<DVec2> = hole.iter().rev().copied().collect();
    let side = lantern_side.unwrap_or(1.0);
    let door = Opening {
        u: middle,
        y: plinth,
        width: SIDE_DOOR.0.min(width - 0.2),
        height: SIDE_DOOR.1.min(arch_top - plinth - width / 2.0),
    };
    let door_hole = door.corners();
    kit.face_with_holes(
        Surface::Siding,
        &filled,
        std::slice::from_ref(&door_hole),
        back,
    );
    tract_door(kit, back, &door, 0.06);
    let (e0, e1) = (middle - width / 2.0, middle + width / 2.0);
    kit.quad(
        Surface::Concrete,
        [
            at(e0, plinth, 0.0),
            at(e1, plinth, 0.0),
            at(e1, plinth, -LOGGIA_PASSAGE),
            at(e0, plinth, -LOGGIA_PASSAGE),
        ],
        wall.transform_vector3(DVec3::Y),
    );
    if lantern_side.is_some() {
        lantern(
            kit,
            wall,
            middle - side * (width / 2.0 + 0.16),
            arch_top - 0.4,
            TRACT_LANTERN,
        );
    }
}

/// A wing's hip roof across `from`–`to` (along the wall `frame` is on) over walls `top`
/// high: its ridge along the wall halfway out, hips at both ends and the back slope
/// running down into the wall, every slope covered like the roof and its eaves boxed with
/// fascia.
fn wing_hip(
    parts: &mut Parts,
    design: &HouseDesign,
    frame: DMat4,
    piece: HousePart,
    (from, to): (f64, f64),
    eave: f64,
    pitch: f64,
) {
    let (u0, u1) = (from - WING_EAVE, to + WING_EAVE);
    let reach = WING_DEPTH + WING_EAVE;
    let run = reach / 2.0;
    let ridge = eave + pitch * run;
    let at = |u: f64, y: f64, z: f64| frame.transform_point3(DVec3::new(u, y, z));
    let (r0, r1) = (u0 + run, (u1 - run).max(u0 + run));
    let kit = parts.kit(piece);
    kit.solid(
        Surface::Trim,
        &[
            vec![
                at(u0, eave, 0.02),
                at(u1, eave, 0.02),
                at(u1, eave, reach),
                at(u0, eave, reach),
            ],
            vec![
                at(r0, ridge, run),
                at(r1, ridge, run),
                at(r1, ridge, run),
                at(r0, ridge, run),
            ],
        ],
    );
    kit.block(
        Surface::Trim,
        DVec3::new(u1 - u0 + 0.04, 0.16, 0.04),
        frame * shift(DVec3::new((u0 + u1) / 2.0, eave - 0.06, reach + 0.02)),
    );
    for u in [u0 - 0.02, u1 + 0.02] {
        kit.block(
            Surface::Trim,
            DVec3::new(0.04, 0.16, reach),
            frame * shift(DVec3::new(u, eave - 0.06, reach / 2.0)),
        );
    }
    let slant = run.hypot(ridge - eave);
    let half = (u1 - u0) / 2.0;
    let middle = (u0 + u1) / 2.0;
    let across = frame.transform_vector3(DVec3::X);
    let out = frame.transform_vector3(DVec3::Z);
    let rise = |toward: DVec3| (toward * run + DVec3::Y * (ridge - eave)).normalize();
    let slopes = [
        (
            at(middle, eave, reach),
            rise(-out),
            across,
            half,
            half - run,
        ),
        (at(middle, eave, 0.02), rise(out), across, half, half - run),
        (at(u0, eave, run), rise(across), out, run, 0.0),
        (at(u1, eave, run), rise(-across), out, run, 0.0),
    ];
    for (eave_middle, up, along, half_eave, half_ridge) in slopes {
        let slope = make_slope(
            piece,
            eave_middle,
            up,
            along,
            slant,
            half_eave,
            half_ridge.max(0.0),
        );
        cover_slope(parts, design.style.cover, &slope);
    }
}

/// A balcony's railing: its height over the deck (low, at the houses' reduced scale), its
/// rails' and balusters' size and the balusters' spacing, the deck's thickness and how
/// far it stands out from the wall, how far past the French doors' surround it reaches
/// either side, its stucco cheek walls' thickness and the stucco head across their tops
/// (m).
const RAILING: f64 = 0.42;
const RAIL: f64 = 0.04;
const BALUSTER: f64 = 0.02;
const BALUSTER_SPACING: f64 = 0.11;
const BALCONY_DECK: f64 = 0.1;
const BALCONY_DEPTH: f64 = 0.42;
const BALCONY_MARGIN: f64 = 0.16;
const BALCONY_CHEEK: f64 = 0.12;
const BALCONY_HEAD: f64 = 0.2;

/// A balcony off the upper floor over the garage (at `garage` along the door wall), as
/// Rumbo north lot 3's and a Neblina house's: a shallow deck at the floor before the French doors
/// there, framed by two stucco cheek walls rising to the eave and a stucco head across
/// them, so the balcony reads as a loggia recessed in the upper floor, railed low across
/// its front in slim white balusters.
fn garage_balcony(
    parts: &mut Parts,
    design: &HouseDesign,
    roof: &Roof,
    piece: HousePart,
    garage: f64,
) {
    let (frame, _) = wall_frames(design.w, design.d)[design.door_wall];
    let half = GARAGE_PAIR + PAIR_WINDOW / 2.0 + SURROUND.0 + BALCONY_MARGIN;
    let (u0, u1) = (garage - half, garage + half);
    let floor = roof.belt_line();
    let kit = parts.kit(piece);
    let block = |kit: &mut Kit<Surface>, surface, size: DVec3, at: DVec3| {
        kit.block(surface, size, frame * shift(at));
    };
    block(
        kit,
        Surface::Siding,
        DVec3::new(u1 - u0 + 2.0 * BALCONY_CHEEK, BALCONY_DECK, BALCONY_DEPTH),
        DVec3::new(garage, floor + BALCONY_DECK / 2.0, BALCONY_DEPTH / 2.0),
    );
    block(
        kit,
        Surface::Trim,
        DVec3::new(u1 - u0 + 2.0 * BALCONY_CHEEK + 0.02, 0.05, 0.03),
        DVec3::new(garage, floor + 0.02, BALCONY_DEPTH + 0.015),
    );
    // The cheek walls, up to the eave's soffit, and the head across them.
    let cheek = roof.eave_y - 0.02 - floor;
    for u in [u0 - BALCONY_CHEEK / 2.0, u1 + BALCONY_CHEEK / 2.0] {
        block(
            kit,
            Surface::Siding,
            DVec3::new(BALCONY_CHEEK, cheek, BALCONY_DEPTH),
            DVec3::new(u, floor + cheek / 2.0, BALCONY_DEPTH / 2.0),
        );
    }
    block(
        kit,
        Surface::Siding,
        DVec3::new(u1 - u0, BALCONY_HEAD, BALCONY_DEPTH),
        DVec3::new(
            garage,
            floor + cheek - BALCONY_HEAD / 2.0,
            BALCONY_DEPTH / 2.0,
        ),
    );
    // The railing across the front between them.
    let deck = floor + BALCONY_DECK;
    let out = BALCONY_DEPTH - 0.05;
    for y in [deck + 0.06, deck + RAILING - RAIL / 2.0] {
        block(
            kit,
            Surface::Curtain,
            DVec3::new(u1 - u0, RAIL, RAIL),
            DVec3::new(garage, y, out),
        );
    }
    let count = ((u1 - u0) / BALUSTER_SPACING).round().max(1.0) as usize;
    for k in 1..count {
        let u = u0 + (u1 - u0) * k as f64 / count as f64;
        block(
            kit,
            Surface::Curtain,
            DVec3::new(BALUSTER, RAILING, BALUSTER),
            DVec3::new(u, deck + RAILING / 2.0, out),
        );
    }
}

/// A wing's front gable across `from`–`to` (along the wall `frame` is on) over walls
/// `top` high: a pediment on its face in `finish` (a stucco one with a small round clay
/// vent under a low pitch, a brick one with a small louvred vent under a steeper one),
/// under two slopes running back to the wall from past the face, covered like the roof.
#[allow(clippy::too_many_arguments)]
fn wing_gable(
    parts: &mut Parts,
    design: &HouseDesign,
    frame: DMat4,
    piece: HousePart,
    (from, to): (f64, f64),
    top: f64,
    finish: Surface,
) {
    const SLAB: f64 = WING_SLAB;
    let brick = finish != Surface::Siding;
    let pitch = if brick {
        BRICK_GABLE_PITCH
    } else {
        WING_GABLE_PITCH
    };
    let middle = (from + to) / 2.0;
    let half = (to - from) / 2.0;
    let rise = half * pitch;
    let kit = parts.kit(piece);
    let triangle = [
        DVec2::new(from, top),
        DVec2::new(to, top),
        DVec2::new(middle, top + rise),
    ];
    let face = frame * shift(DVec3::Z * WING_DEPTH);
    kit.face_with_holes(finish, &triangle, &[], face);
    // A Spanish wing's clay pipe vents; a brick or Prairie gable's louvred vent.
    if brick || design.style.porch {
        // A small louvred vent framed in trim under the peak.
        let y = top + rise * 0.42;
        kit.block(
            Surface::Trim,
            DVec3::new(0.22, 0.14, 0.03),
            face * shift(DVec3::new(middle, y, 0.015)),
        );
        kit.block(
            Surface::Dark,
            DVec3::new(0.15, 0.08, 0.03),
            face * shift(DVec3::new(middle, y, 0.022)),
        );
    } else {
        pipe_vents(kit, face, DVec2::new(middle, top + rise * 0.36));
    }
    // The slopes in the wall's frame: each a slab from the ridge down past the side.
    let reach = half + WING_EAVE;
    let slope = pitch.atan();
    let (sin, cos) = slope.sin_cos();
    let slant = reach / cos;
    let (z0, z1) = (0.02, WING_DEPTH + WING_EAVE);
    let run = z1 - z0;
    let crest = top + rise + SLAB / cos;
    for s in [-1.0, 1.0] {
        let down = DVec3::new(s * cos, -sin, 0.0);
        let normal = DVec3::new(s * sin, cos, 0.0);
        let ridge = DVec3::new(middle, crest, (z0 + z1) / 2.0);
        let center = ridge + down * (slant / 2.0) - normal * (SLAB / 2.0);
        let basis = DMat4::from_cols(
            down.extend(0.0),
            normal.extend(0.0),
            (DVec3::Z * s).extend(0.0),
            center.extend(1.0),
        );
        parts
            .kit(piece)
            .block(Surface::Roof, DVec3::new(slant, SLAB, run), frame * basis);
        let eave = ridge + down * slant;
        let cover = make_slope(
            piece,
            frame.transform_point3(eave),
            frame.transform_vector3(-down),
            frame.transform_vector3(DVec3::Z),
            slant,
            run / 2.0,
            run / 2.0,
        );
        cover_slope(parts, design.style.cover, &cover);
    }
    parts.kit(piece).block(
        Surface::Roof,
        DVec3::new(0.14, 0.07, run),
        frame * shift(DVec3::new(middle, crest + 0.03, (z0 + z1) / 2.0)),
    );
}

/// A round clay pipe vent in a stucco face (`frame`'s plane) at `at`, `radius` across:
/// a clay ring round its dark mouth.
fn clay_vent(kit: &mut Kit<Surface>, frame: DMat4, at: DVec2, radius: f64) {
    const SIDES: usize = 10;
    let ring = |r: f64| -> Vec<DVec2> {
        (0..SIDES)
            .map(|k| {
                let angle = PI * 2.0 * k as f64 / SIDES as f64;
                at + DVec2::new(angle.cos(), angle.sin()) * r
            })
            .collect()
    };
    kit.face_with_holes(
        Surface::Terracotta,
        &ring(radius),
        &[],
        frame * shift(DVec3::Z * 0.01),
    );
    kit.face_with_holes(
        Surface::Dark,
        &ring(radius * 0.42),
        &[],
        frame * shift(DVec3::Z * 0.016),
    );
}

/// A garage house's open porch across its wing (`wing`): a shed skirt roof out from the
/// wall under the upper floor's sills, covered like the roof, on chunky posts over brick
/// bases, over a concrete floor. A stucco house's posts are stained dark; a sided house's
/// painted in its trim.
fn skirt_porch(
    parts: &mut Parts,
    design: &HouseDesign,
    roof: &Roof,
    piece: HousePart,
    wing: &WingPlan,
) {
    let (frame, _) = wall_frames(design.w, design.d)[design.door_wall];
    let (from, to) = (wing.from, wing.to);
    let base = roof.upper_window().0 - 0.05;
    // Neblina's corner house's porch is painted white on white columns; the others' timber is stained
    // dark on stacked-stone piers, the wall behind clad in board-and-batten.
    let columns = design.style.batten_upper;
    let timber = if columns {
        Surface::Trim
    } else {
        Surface::Wood
    };
    let depth = WING_DEPTH + 0.15;
    let front = shed_roof(
        parts,
        design,
        piece,
        frame,
        (from - 0.1, to + 0.1),
        depth,
        base,
        SKIRT_FALL,
        timber,
    );
    let kit = parts.kit(piece);
    if !columns {
        // Exposed rafter tails under its eave.
        let fall = depth * SKIRT_FALL;
        let mut u = from - 0.02;
        while u <= to + 0.02 {
            let slope = |z: f64| base - fall * z / depth - RAFTER_TAIL.1 / 2.0;
            let (a, b) = (
                frame.transform_point3(DVec3::new(u, slope(depth - 0.5), depth - 0.5)),
                frame.transform_point3(DVec3::new(u, slope(depth + 0.1), depth + 0.1)),
            );
            let length = (b - a).length();
            kit.block(
                Surface::Wood,
                DVec3::new(RAFTER_TAIL.0, RAFTER_TAIL.1, length),
                aim(a, b) * DMat4::from_translation(DVec3::Z * (length / 2.0)),
            );
            u += RAFTER_TAIL_SPACING;
        }
        let openings: Vec<Opening> = wall_openings(design, roof, design.door_wall)
            .openings
            .into_iter()
            .filter(|o| o.u > from && o.u < to && o.y < base)
            .collect();
        batten_cladding(kit, frame, (from, to), (roof.plinth, base), &openings);
    }
    let out = WING_DEPTH - 0.1;
    for u in porch_posts(from, to) {
        let (post, pier) = if columns {
            (SKIRT_COLUMN, 0.0)
        } else {
            (SKIRT_POST, SKIRT_BASE.1)
        };
        let foot = roof.plinth + pier;
        kit.block(
            timber,
            DVec3::new(post, front - foot, post),
            frame * shift(DVec3::new(u, (front + foot) / 2.0, out)),
        );
        if columns {
            // A white capital and a pair of curved brackets' worth of trim under the beam.
            kit.block(
                Surface::Trim,
                DVec3::new(post + 0.08, 0.06, post + 0.08),
                frame * shift(DVec3::new(u, front - 0.03, out)),
            );
            kit.block(
                Surface::Trim,
                DVec3::new(post + 0.06, 0.08, post + 0.06),
                frame * shift(DVec3::new(u, roof.plinth + 0.04, out)),
            );
        } else {
            kit.chamfer_block(
                Surface::Stone,
                DVec3::new(SKIRT_BASE.0, SKIRT_BASE.1, SKIRT_BASE.0),
                0.015,
                frame * shift(DVec3::new(u, roof.plinth + SKIRT_BASE.1 / 2.0, out)),
            );
        }
    }
    porch_floor(parts, frame, from, to, roof.plinth, WING_DEPTH);
}

/// Board-and-batten cladding over a stretch of wall (`frame`, `from`–`to` along it,
/// `bottom`–`top` up it): a painted board face over the stucco, cut round `openings`
/// (whose surrounds stand proud of it), battens up it between them.
fn batten_cladding(
    kit: &mut Kit<Surface>,
    frame: DMat4,
    (from, to): (f64, f64),
    (bottom, top): (f64, f64),
    openings: &[Opening],
) {
    let clear = |o: &Opening| o.y < top - 0.05 && o.top() > bottom + 0.05;
    let holes: Vec<Vec<DVec2>> = openings
        .iter()
        .filter(|o| clear(o))
        .map(|o| {
            let (u0, u1) = (o.u - o.width / 2.0, o.u + o.width / 2.0);
            let (y0, y1) = (o.y.max(bottom + 0.01), o.top().min(top - 0.01));
            [(u0, y0), (u0, y1), (u1, y1), (u1, y0)]
                .map(|(u, y)| DVec2::new(u, y))
                .to_vec()
        })
        .collect();
    let outline =
        [(from, bottom), (to, bottom), (to, top), (from, top)].map(|(u, y)| DVec2::new(u, y));
    kit.face_with_holes(
        Surface::Cladding,
        &outline,
        &holes,
        frame * shift(DVec3::Z * CLADDING.0),
    );
    let mut u = from + CLADDING.1 / 2.0;
    while u < to {
        // Each batten runs up the wall, broken round the openings it meets.
        let mut runs = vec![(bottom, top)];
        for o in openings.iter().filter(|o| clear(o)) {
            if (u - o.u).abs() < o.width / 2.0 + SURROUND.0 + 0.03 {
                let (a, b) = (o.y - TRACT_SILL.0 - 0.02, o.top() + SURROUND.0 + 0.02);
                runs = runs
                    .into_iter()
                    .flat_map(|(y0, y1)| [(y0, y1.min(a)), (y0.max(b), y1)])
                    .filter(|(y0, y1)| y1 - y0 > 0.08)
                    .collect();
            }
        }
        for (y0, y1) in runs {
            kit.block(
                Surface::Cladding,
                DVec3::new(CLADDING.2, y1 - y0, CLADDING.3),
                frame
                    * shift(DVec3::new(
                        u,
                        (y0 + y1) / 2.0,
                        CLADDING.0 + CLADDING.3 / 2.0,
                    )),
            );
        }
        u += CLADDING.1;
    }
}

/// The reference house's entry across its wing (`wing`): a one-storey stucco room by the garage, a
/// window in its face, under a low clay hip that runs on toward the corner over an open
/// porch on two plain dark timber posts, the front door in the room's side under it, as 7
/// Rumbo's is. It goes with the door wall; the porch's floor stays with the ground.
fn room_porch(
    parts: &mut Parts,
    design: &HouseDesign,
    roof: &Roof,
    piece: HousePart,
    wing: &WingPlan,
) {
    let (frame, _) = wall_frames(design.w, design.d)[design.door_wall];
    let plinth = roof.plinth;
    let rise = ROOM_HIP_PITCH * (WING_DEPTH + WING_EAVE) / 2.0 + PORCH_DECK;
    let eave = under_sills(roof, WING_WALL, rise);
    let ((r0, r1), (p0, p1)) = wing.room_porch();
    let toward = wing.toward_garage;
    // The porch runs on past the house's corner toward the lot line, its post standing
    // clear of the room.
    let (p0, p1) = if toward > 0.0 {
        (p0 - ROOM_PORCH_REACH, p1)
    } else {
        (p0, p1 + ROOM_PORCH_REACH)
    };
    wing_hip(
        parts,
        design,
        frame,
        piece,
        (wing.from.min(p0), wing.to.max(p1)),
        eave,
        ROOM_HIP_PITCH,
    );
    let at = |u: f64, y: f64, z: f64| frame.transform_point3(DVec3::new(u, y, z));
    let facing = |x: f64| frame.transform_vector3(DVec3::new(x, 0.0, 0.0));
    let face = frame * shift(DVec3::Z * WING_DEPTH);
    let window = Opening {
        u: (r0 + r1) / 2.0,
        y: plinth + WING_SILL,
        width: WING_WINDOW.1.min(r1 - r0 - 0.3),
        height: eave - 0.2 - plinth - WING_SILL,
    };
    let hole = window.corners();
    let outline = [(r0, 0.0), (r1, 0.0), (r1, eave), (r0, eave)].map(|(u, y)| DVec2::new(u, y));
    let kit = parts.kit(piece);
    kit.face_with_holes(Surface::Siding, &outline, std::slice::from_ref(&hole), face);
    reveal(kit, Surface::Siding, face, &hole, WING_REVEAL);
    tract_window(kit, face, &window, WING_REVEAL, Surface::Siding);
    // The room's side by the garage, closed back to the wall.
    let garage_side = if toward > 0.0 { r1 } else { r0 };
    kit.quad(
        Surface::Siding,
        [
            at(garage_side, 0.0, 0.0),
            at(garage_side, 0.0, WING_DEPTH),
            at(garage_side, eave, WING_DEPTH),
            at(garage_side, eave, 0.0),
        ],
        facing(toward),
    );
    // Its side under the porch, the front door in it facing the corner.
    let side = -toward;
    let split = if toward > 0.0 { r0 } else { r1 };
    let wall = frame * shift(DVec3::X * split) * DMat4::from_rotation_y(side * PI / 2.0);
    let ends = [0.0, -side * WING_DEPTH];
    let (a0, a1) = (ends[0].min(ends[1]), ends[0].max(ends[1]));
    let door = Opening {
        u: -side * SIDE_DOOR_AT,
        y: plinth,
        width: SIDE_DOOR.0,
        height: SIDE_DOOR.1.min(eave - plinth - 0.1),
    };
    let side_outline =
        [(a0, 0.0), (a1, 0.0), (a1, eave), (a0, eave)].map(|(u, y)| DVec2::new(u, y));
    let door_hole = door.corners();
    kit.face_with_holes(
        Surface::Siding,
        &side_outline,
        std::slice::from_ref(&door_hole),
        wall,
    );
    reveal(kit, Surface::Siding, wall, &door_hole, SIDE_DOOR_REVEAL);
    tract_door(kit, wall, &door, SIDE_DOOR_REVEAL);
    lantern(
        kit,
        wall,
        door.u + side * (door.width / 2.0 + 0.16),
        door.top() - 0.4,
        TRACT_LANTERN,
    );
    // Two plain dark posts down the porch's open side, no brick under them.
    let corner = if toward > 0.0 { p0 } else { p1 };
    let u = corner + toward * (ROOM_POST / 2.0 + 0.04);
    for out in [WING_DEPTH - 0.1, 0.45] {
        kit.block(
            Surface::Wood,
            DVec3::new(ROOM_POST, eave - plinth, ROOM_POST),
            frame * shift(DVec3::new(u, (eave + plinth) / 2.0, out)),
        );
    }
    porch_floor(parts, frame, p0, p1, plinth, WING_DEPTH);
}

/// A covered patio off a tract house's back wall (`index`): a shed roof under the upper
/// floor's sills, covered like the roof, on two slim posts.
fn patio_cover(parts: &mut Parts, design: &HouseDesign, roof: &Roof, index: usize) {
    let (frame, half) = wall_frames(design.w, design.d)[index];
    let span = (PATIO_COVER.0 / 2.0).min(half - 0.4);
    let base = roof.upper_window().0 - 0.05;
    let piece = HousePart::wall(index);
    let front = shed_roof(
        parts,
        design,
        piece,
        frame,
        (-span, span),
        PATIO_COVER.1,
        base,
        PATIO_FALL,
        Surface::Trim,
    );
    let kit = parts.kit(piece);
    for u in [-span + 0.12, span - 0.12] {
        kit.block(
            Surface::Trim,
            DVec3::new(PATIO_POST, front, PATIO_POST),
            frame * shift(DVec3::new(u, front / 2.0, PATIO_COVER.1 - 0.12)),
        );
    }
}

/// A roof slope from its eave's middle (on the deck's top) up `up` to its ridge
/// `length` away, its eave along `across`, `half_eave` and `half_ridge` either way.
fn make_slope(
    piece: HousePart,
    eave: DVec3,
    up: DVec3,
    across: DVec3,
    length: f64,
    half_eave: f64,
    half_ridge: f64,
) -> Slope {
    let mut normal = across.cross(up).normalize();
    if normal.y < 0.0 {
        normal = -normal;
    }
    Slope {
        piece,
        eave,
        up,
        across,
        normal,
        length,
        half_eave,
        half_ridge,
    }
}

/// One slope covered as the house's roof is: shingle courses, standing seams or clay
/// barrel tile.
fn cover_slope(parts: &mut Parts, cover: RoofCover, slope: &Slope) {
    match cover {
        RoofCover::Shingles => shingle_slope(parts.courses(slope.piece), slope),
        RoofCover::Seams => seam_slope(parts.kit(slope.piece), slope),
        RoofCover::Tile => tile_slope(parts.kit(slope.piece), slope),
        RoofCover::FlatTile => flat_tile_slope(parts.kit(slope.piece), slope),
    }
}

/// Where along a porch its posts stand: at both ends and, on a long porch, between.
fn porch_posts(from: f64, to: f64) -> Vec<f64> {
    let (a, b) = (from + 0.15, to - 0.15);
    let bays = ((b - a) / 2.2).ceil().max(1.0) as usize;
    (0..=bays)
        .map(|k| a + (b - a) * k as f64 / bays as f64)
        .collect()
}

/// A porch's concrete floor at the plinth's height, across `from`–`to` along the wall
/// and `depth` out: it stays with the ground.
fn porch_floor(parts: &mut Parts, frame: DMat4, from: f64, to: f64, plinth: f64, depth: f64) {
    parts.ground().block(
        Surface::Concrete,
        DVec3::new(to - from, plinth, depth),
        frame * shift(DVec3::new((from + to) / 2.0, plinth / 2.0, depth / 2.0)),
    );
}

/// A shed roof out from a wall (`frame`) across `from`–`to` along it, `depth` deep: a
/// deck sloping out from the wall at `base`, falling `fall` a metre out, covered like
/// the roof, its edges and fascia along the front in `edge`. Returns its front edge's
/// underside height.
#[allow(clippy::too_many_arguments)]
fn shed_roof(
    parts: &mut Parts,
    design: &HouseDesign,
    piece: HousePart,
    frame: DMat4,
    (from, to): (f64, f64),
    depth: f64,
    base: f64,
    fall: f64,
    edge: Surface,
) -> f64 {
    let fall = depth * fall;
    let at = |u: f64, y: f64, z: f64| frame.transform_point3(DVec3::new(u, y, z));
    let ring = |u: f64| {
        vec![
            at(u, base, 0.02),
            at(u, base - fall, depth),
            at(u, base - fall + PORCH_DECK, depth),
            at(u, base + PORCH_DECK, 0.02),
        ]
    };
    let kit = parts.kit(piece);
    kit.solid(edge, &[ring(from), ring(to)]);
    kit.block(
        edge,
        DVec3::new(to - from + 0.04, 0.16, 0.04),
        frame
            * shift(DVec3::new(
                (from + to) / 2.0,
                base - fall + 0.02,
                depth + 0.02,
            )),
    );
    let up = frame.transform_vector3(DVec3::new(0.0, fall, 0.02 - depth));
    let slope = make_slope(
        piece,
        at((from + to) / 2.0, base - fall + PORCH_DECK, depth),
        up.normalize(),
        frame.transform_vector3(DVec3::X),
        up.length(),
        (to - from) / 2.0,
        (to - from) / 2.0,
    );
    cover_slope(parts, design.style.cover, &slope);
    base - fall
}

/// A Craftsman's box bay over its garage: how far it stands out from the upper floor,
/// how far its windows sit back in its face, how far past their surrounds it reaches,
/// its battens' spacing, how far under the main eave's underside its hood stays, its
/// slim knee brackets' drop under it, its three ganged windows' width and the mullions
/// between them, and its tile hood's fall per metre out (m).
const BAY_DEPTH: f64 = 0.45;
const BAY_REVEAL: f64 = 0.08;
const BAY_MARGIN: f64 = 0.14;
const BAY_BOARD: f64 = 0.2;
const BAY_UNDER_EAVE: f64 = 0.22;
const BAY_KNEE: f64 = 0.15;
const BAY_LITE: f64 = 0.3;
const BAY_MULLION: f64 = 0.07;
const BAY_HOOD_FALL: f64 = 0.16;
/// The reference house's box bay: its two windows' width, and how far over the garage door its
/// skirt starts (m).
const HOME_BAY_LITE: f64 = 0.44;
const HOME_BAY_OVER_GARAGE: f64 = 0.2;

/// A box bay over the garage (at `garage` along the door wall, its back `offset` out
/// from the wall: on the wall, or on the reference house's front gable): a board-and-batten box
/// stained dark standing out from the upper floor, three narrow windows ganged in its
/// face in pale frames, on two slim knee brackets, under a small hood in the roof's
/// covering tucked under the eave, as the tract's cantilevered bays are.
fn garage_bay(
    parts: &mut Parts,
    design: &HouseDesign,
    roof: &Roof,
    piece: HousePart,
    garage: f64,
    offset: f64,
) {
    let (wall, _) = wall_frames(design.w, design.d)[design.door_wall];
    let frame = wall * shift(DVec3::Z * offset);
    let paint = Surface::Wood;
    let y1 = roof.eave_y - BAY_UNDER_EAVE;
    let over = upper_window(garage, roof);
    let height = over.height.min(y1 - 0.12 - over.y);
    // The reference house's bay on its front gable holds two double-hung windows over a tall
    // board-and-batten skirt reaching down toward the garage door; a Craftsman's three
    // narrow ones.
    let home = offset > 0.0;
    let (lites, lite) = if home {
        (2, HOME_BAY_LITE)
    } else {
        (3, BAY_LITE)
    };
    let windows: Vec<Opening> = (0..lites)
        .map(|k| Opening {
            u: garage + (k as f64 - (lites - 1) as f64 / 2.0) * (lite + BAY_MULLION),
            y: over.y,
            width: lite,
            height,
        })
        .collect();
    let reach = lites as f64 / 2.0 * lite
        + (lites - 1) as f64 / 2.0 * BAY_MULLION
        + SURROUND.0
        + BAY_MARGIN;
    let (u0, u1) = (garage - reach, garage + reach);
    let y0 = if home {
        roof.plinth + GARAGE_HEIGHT + HOME_BAY_OVER_GARAGE
    } else {
        roof.belt_line() - 0.04
    };
    let kit = parts.kit(piece);
    let at = |u: f64, y: f64, z: f64| frame.transform_point3(DVec3::new(u, y, z));
    let facing = |x: f64, y: f64, z: f64| frame.transform_vector3(DVec3::new(x, y, z));
    let holes: Vec<Vec<DVec2>> = windows.iter().map(Opening::corners).collect();
    let outline = [(u0, y0), (u1, y0), (u1, y1), (u0, y1)].map(|(u, y)| DVec2::new(u, y));
    let face = frame * shift(DVec3::Z * BAY_DEPTH);
    kit.face_with_holes(paint, &outline, &holes, face);
    for (hole, opening) in holes.iter().zip(&windows) {
        reveal(kit, paint, face, hole, BAY_REVEAL);
        tract_window(kit, face, opening, BAY_REVEAL, Surface::Trim);
    }
    // Its sides and soffit.
    for (u, side) in [(u0, -1.0), (u1, 1.0)] {
        kit.quad(
            paint,
            [
                at(u, y0, 0.0),
                at(u, y0, BAY_DEPTH),
                at(u, y1, BAY_DEPTH),
                at(u, y1, 0.0),
            ],
            facing(side, 0.0, 0.0),
        );
    }
    kit.quad(
        paint,
        [
            at(u0, y0, 0.0),
            at(u1, y0, 0.0),
            at(u1, y0, BAY_DEPTH),
            at(u0, y0, BAY_DEPTH),
        ],
        facing(0.0, -1.0, 0.0),
    );
    if home {
        // Its back, where it hangs below the gable's floor.
        kit.quad(
            paint,
            [
                at(u1, y0, 0.0),
                at(u0, y0, 0.0),
                at(u0, roof.belt_line(), 0.0),
                at(u1, roof.belt_line(), 0.0),
            ],
            facing(0.0, 0.0, -1.0),
        );
    }
    // Battens up the face, clear of the windows' surrounds, and under them down the
    // skirt.
    let skirt = over.y - TRACT_SILL.0 - 0.02;
    let mut u = u0 + 0.05;
    while u < u1 {
        let clear = windows
            .iter()
            .all(|o| (u - o.u).abs() > o.width / 2.0 + SURROUND.0 + 0.04);
        let top = if clear { y1 } else { skirt };
        if top - y0 > 0.1 {
            kit.block(
                paint,
                DVec3::new(0.05, top - y0, 0.03),
                frame * shift(DVec3::new(u, (y0 + top) / 2.0, BAY_DEPTH + 0.015)),
            );
        }
        u += BAY_BOARD;
    }
    // The slim knee brackets under it.
    for u in [u0 + 0.1, u1 - 0.1] {
        let (from, to) = (
            at(u, y0 - BAY_KNEE, 0.03),
            at(u, y0 - 0.02, BAY_DEPTH - 0.08),
        );
        let length = (to - from).length();
        kit.block(
            paint,
            DVec3::new(0.05, 0.05, length),
            aim(from, to) * DMat4::from_translation(DVec3::Z * (length / 2.0)),
        );
    }
    // Its hood, falling off the bay's front from under the eave.
    let depth = BAY_DEPTH + 0.12;
    shed_roof(
        parts,
        design,
        piece,
        frame,
        (u0 - 0.06, u1 + 0.06),
        depth,
        y1 + 0.03 + depth * BAY_HOOD_FALL,
        BAY_HOOD_FALL,
        Surface::Trim,
    );
}

/// A Spanish house's two-storey front over its entry side: how far its upper floor stands
/// out from the wall (past the main eave and its gutter, which run on inside it), how
/// far its gable's tile overhangs the face, its clay pipe vents' radius and spacing under
/// its peak, how far its outer slope stands off the main roof's side slope, which it
/// carries forward, and how far its arched windows sit back in its face.
const GABLE_BAY: f64 = 0.5;
const GABLE_RAKE: f64 = 0.18;
const GABLE_VENT: (f64, f64) = (0.03, 0.1);
/// A Spanish gable's twin arched windows: each one's width and the gap between them (m),
/// close together in the middle of the broad gable.
const GABLE_ARCH: (f64, f64) = (0.42, 0.1);
const GABLE_LIFT: f64 = 0.03;
const GABLE_REVEAL: f64 = 0.1;
/// How far over a Spanish wing's ridge the arched windows in the gable behind it start,
/// how far under the gable's rakes they stop, and the lowest they get (m).
const ARCH_OVER_WING: f64 = 0.1;
const ARCH_UNDER_RAKE: f64 = 0.08;
const ARCH_LEAST: f64 = 0.4;
/// A Spanish house's clay pent roof over its garage door at the upper floor: how far it
/// reaches out from the wall (m).
const PENT_DEPTH: f64 = 0.5;

/// Which side (±1 along the build frame's x) a garage house's two-storey front gable
/// stands: a Spanish house's over its entry side, its arched wing before it (none on 1
/// Rumbo's, whose wing stands before a plain upper floor), or the reference house's over its
/// garage, on a front that is the hip end of a roof with low storeys.
fn garage_gable_side(design: &HouseDesign) -> Option<f64> {
    let garage = design.wall_plan(design.door_wall).garage()?.signum();
    if design.door_wall != 0 || !design.roof().low_storeys {
        return None;
    }
    match entry(design) {
        Entry::ArchWing if !design.style.loggia => Some(-garage),
        _ if design.style.gable_front => Some(garage),
        _ => None,
    }
}

/// The height (m) of the top of a Spanish wing's front gable (`wing`), its ridge cap's.
fn wing_crest(roof: &Roof, wing: &WingPlan) -> f64 {
    let rise = (wing.to - wing.from) / 2.0 * WING_GABLE_PITCH;
    roof.plinth + WING_WALL + rise + WING_SLAB / WING_GABLE_PITCH.atan().cos() + 0.07
}

/// A two-storey front gable on the front wall (the garage at `garage` along it), on
/// `side` of the front: its upper floor stands out from the corner to the garage's inner
/// column, its stucco face rising into a front gable. A Spanish house's stands over its
/// entry side, twin round-arched windows close together high in its face over the arched
/// wing standing before it, a clay pent roof over the garage door beside it; the reference house's
/// over its garage round a dark box bay, a diamond tile vent under its peak. The gable's
/// roof takes the main roof's pitch, so its outer slope carries the side slope forward
/// over it and its inner slope dies into the hip end behind it, as the tract's do. The
/// face goes with the front wall, the gable's roof with the front slope.
fn garage_gable(
    parts: &mut Parts,
    design: &HouseDesign,
    roof: &Roof,
    piece: HousePart,
    garage: f64,
    side: f64,
) {
    const SLAB: f64 = 0.08;
    let (frame, _) = wall_frames(design.w, design.d)[design.door_wall];
    let outer = side * design.w / 2.0;
    let bay = design.wall_plan(design.door_wall).garage_bay;
    let inner = garage - garage.signum() * (2.0 * bay + 0.05);
    let (u0, u1) = (inner.min(outer), inner.max(outer));
    let (middle, half) = ((u0 + u1) / 2.0, (u1 - u0) / 2.0);
    let (y0, y1) = (roof.belt_line(), roof.wall);
    let rise = roof.pitch * half;
    let at = |u: f64, y: f64, z: f64| frame.transform_point3(DVec3::new(u, y, z));
    let facing = |x: f64, y: f64, z: f64| frame.transform_vector3(DVec3::new(x, y, z));
    let spanish = entry(design) == Entry::ArchWing;
    let (arch, gap) = GABLE_ARCH;
    let windows: Vec<Opening> = match wing_plan(design) {
        Some(wing) if spanish => {
            // High in the gable, over the wing's ridge and under the rakes.
            let upper = upper_window(middle, roof);
            let reach = arch + gap / 2.0;
            let sill = upper.y.max(wing_crest(roof, &wing) + ARCH_OVER_WING);
            let top = (sill + upper.height)
                .min(y1 + rise * (1.0 - reach / half) - ARCH_UNDER_RAKE)
                .max(sill + ARCH_LEAST);
            [-1.0, 1.0]
                .map(|k| Opening {
                    u: middle + k * (arch + gap) / 2.0,
                    y: sill,
                    width: arch,
                    height: top - sill,
                })
                .to_vec()
        }
        _ => Vec::new(),
    };
    let holes: Vec<Vec<DVec2>> = windows
        .iter()
        .map(|o| arch_outline(o.u, o.width, o.y, o.top()))
        .collect();
    let outline = [(u0, y0), (u1, y0), (u1, y1), (middle, y1 + rise), (u0, y1)]
        .map(|(u, y)| DVec2::new(u, y));
    let kit = parts.kit(piece);
    kit.face_with_holes(
        Surface::Siding,
        &outline,
        &holes,
        frame * shift(DVec3::Z * GABLE_BAY),
    );
    let face = frame * shift(DVec3::Z * GABLE_BAY);
    for (hole, opening) in holes.iter().zip(&windows) {
        reveal(kit, Surface::Siding, face, hole, GABLE_REVEAL);
        arched_window(kit, face, opening, GABLE_REVEAL);
    }
    for (u, out) in [(u0, -1.0), (u1, 1.0)] {
        kit.quad(
            Surface::Siding,
            [
                at(u, y0, 0.0),
                at(u, y0, GABLE_BAY),
                at(u, y1, GABLE_BAY),
                at(u, y1, 0.0),
            ],
            facing(out, 0.0, 0.0),
        );
    }
    kit.quad(
        Surface::Siding,
        [
            at(u0, y0, 0.0),
            at(u1, y0, 0.0),
            at(u1, y0, GABLE_BAY),
            at(u0, y0, GABLE_BAY),
        ],
        facing(0.0, -1.0, 0.0),
    );
    if spanish {
        garage_pent(parts, design, roof, piece, garage);
    } else {
        diamond_vent(kit, face, DVec2::new(middle, y1 + rise * 0.55));
        garage_bay(parts, design, roof, piece, garage, GABLE_BAY);
    }
    // The gable's roof in the build frame (the front wall faces +z, along +x): a slab
    // each side of its ridge from the face's overhang back to where the ridge meets the
    // hip end, each covered like the main roof.
    let crest = roof.ridge + roof.thick - roof.pitch * middle.abs();
    let front = design.d / 2.0 + GABLE_BAY + GABLE_RAKE;
    let rear = roof.ridge_half + middle.abs();
    let run = front - rear;
    let reach = half + roof.overhang;
    let slope = roof.pitch.atan();
    let (sin, cos) = slope.sin_cos();
    let slant = reach / cos;
    for s in [-1.0, 1.0] {
        // The outer slope lies on the main side slope's plane, a little off it.
        let lift = if s == side { GABLE_LIFT } else { 0.0 };
        let down = DVec3::new(s * cos, -sin, 0.0);
        let normal = DVec3::new(s * sin, cos, 0.0);
        let ridge = DVec3::new(middle, crest + lift, (front + rear) / 2.0);
        let center = ridge + down * (slant / 2.0) - normal * (SLAB / 2.0);
        let basis = DMat4::from_cols(
            down.extend(0.0),
            normal.extend(0.0),
            (DVec3::Z * s).extend(0.0),
            center.extend(1.0),
        );
        parts
            .kit(HousePart::FrontRoof)
            .block(Surface::Roof, DVec3::new(slant, SLAB, run), basis);
        let eave = ridge + down * slant;
        let cover = make_slope(
            HousePart::FrontRoof,
            eave,
            -down,
            DVec3::Z,
            slant,
            run / 2.0,
            run / 2.0,
        );
        cover_slope(parts, design.style.cover, &cover);
    }
    parts.kit(HousePart::FrontRoof).block(
        Surface::Roof,
        DVec3::new(0.16, 0.08, run),
        shift(DVec3::new(middle, crest + 0.04, (front + rear) / 2.0)),
    );
}

/// A Spanish house's clay pent roof over its garage door (at `garage` along the front
/// wall) at the upper floor, from the corner to the wing beside it, under the plain upper
/// wall with its pair of windows.
fn garage_pent(
    parts: &mut Parts,
    design: &HouseDesign,
    roof: &Roof,
    piece: HousePart,
    garage: f64,
) {
    let Some(wing) = wing_plan(design) else {
        return;
    };
    let (frame, _) = wall_frames(design.w, design.d)[design.door_wall];
    let corner = garage.signum() * (design.w / 2.0 + 0.05);
    let wing_side = if wing.toward_garage > 0.0 {
        wing.to + WING_EAVE
    } else {
        wing.from - WING_EAVE
    };
    shed_roof(
        parts,
        design,
        piece,
        frame,
        (corner.min(wing_side), corner.max(wing_side)),
        PENT_DEPTH,
        roof.belt_line() - 0.02,
        SKIRT_FALL,
        Surface::Trim,
    );
}

/// Three small clay pipe vents in a triangle in a stucco face (`frame`'s plane) round
/// `at`.
fn pipe_vents(kit: &mut Kit<Surface>, frame: DMat4, at: DVec2) {
    let (radius, spacing) = GABLE_VENT;
    for (du, dy) in [
        (-spacing / 2.0, 0.0),
        (spacing / 2.0, 0.0),
        (0.0, spacing * 0.8),
    ] {
        clay_vent(kit, frame, at + DVec2::new(du, dy), radius);
    }
}

/// A diamond tile vent in a stucco face (`frame`'s plane) at `at`: four small clay tiles
/// set flush round a dark diamond, under a gable's peak (the reference house's).
fn diamond_vent(kit: &mut Kit<Surface>, frame: DMat4, at: DVec2) {
    const HALF: f64 = 0.11;
    const TILE: f64 = 0.035;
    kit.block(
        Surface::Dark,
        DVec3::new(HALF, HALF, 0.008),
        frame
            * pose(
                DVec3::new(at.x, at.y, 0.004),
                DVec3::new(0.0, 0.0, PI / 4.0),
            ),
    );
    let side = HALF * std::f64::consts::SQRT_2;
    for (du, dy) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
        let lean = if du * dy > 0.0 { -PI / 4.0 } else { PI / 4.0 };
        kit.block(
            Surface::Terracotta,
            DVec3::new(side + TILE, TILE, 0.012),
            frame
                * pose(
                    DVec3::new(at.x + du * HALF / 2.0, at.y + dy * HALF / 2.0, 0.006),
                    DVec3::new(0.0, 0.0, lean),
                ),
        );
    }
}

/// Flat concrete tile: a course's exposure down the slope and how far its butt stands
/// over the course below (m).
const FLAT_TILE_COURSE: f64 = 0.34;
const FLAT_TILE_BUTT: f64 = 0.018;

/// One slope of flat concrete tile: courses lying flat in the roof's paint from the eave
/// to the ridge, each butt a thin step over the course below, so from above the roof
/// reads smooth, lined only by its courses.
fn flat_tile_slope(kit: &mut Kit<Surface>, slope: &Slope) {
    const HEAD: f64 = 0.004;
    let courses = (slope.length / FLAT_TILE_COURSE).round().max(1.0);
    let exposure = slope.length / courses;
    let half = |s: f64| slope.half(s) + 0.01;
    for k in 0..courses as u32 {
        let (s0, s1) = (f64::from(k) * exposure, f64::from(k + 1) * exposure);
        let (h0, h1) = (half(s0), half(s1));
        if h0 < 0.02 && h1 < 0.02 {
            continue;
        }
        kit.quad(
            Surface::Roof,
            [
                slope.at(s0, -h0, FLAT_TILE_BUTT),
                slope.at(s0, h0, FLAT_TILE_BUTT),
                slope.at(s1, h1, HEAD),
                slope.at(s1, -h1, HEAD),
            ],
            slope.normal,
        );
        let below = if k == 0 { -0.01 } else { HEAD };
        kit.quad(
            Surface::Roof,
            [
                slope.at(s0, -h0, below),
                slope.at(s0, h0, below),
                slope.at(s0, h0, FLAT_TILE_BUTT),
                slope.at(s0, -h0, FLAT_TILE_BUTT),
            ],
            -slope.up,
        );
    }
}

/// A solar panel's size across a slope and up it (drawn at the houses' own reduced
/// scale), its frame's depth and how far its glass stands off the roof's covering, how
/// near a slope's edges an array comes, where up the slope its lowest row starts, and
/// the spacing of its panels across and up (m).
const PANEL: (f64, f64) = (0.5, 0.84);
const PANEL_DEPTH: f64 = 0.035;
const PANEL_STANDOFF: f64 = 0.07;
const PANEL_MARGIN: f64 = 0.25;
const PANEL_BOTTOM: f64 = PANEL_MARGIN + 0.12;
const PANEL_PITCH: (f64, f64) = (PANEL.0 + 0.02, PANEL.1 + 0.02);
/// How much of a slope's eave an array spans at most: the tract's arrays are big dark
/// blocks covering most of one plane, but stop short of its hips.
const ARRAY_SHARE: f64 = 0.72;
/// How far (m) a panel's glass stands proud of its frame: far enough that the two never
/// fight for depth from the overhead camera.
const GLASS_PROUD: f64 = 0.025;
/// A small array's panels, and each of the two further blocks of a house whose arrays
/// cover three slopes.
const SMALL_ARRAY: usize = 4;
const SIDE_ARRAY: usize = 8;
const FULL_SLOPE_ARRAY: usize = 24;
/// Multiplier spreading a house's variant into its lot's own draw for its arrays
/// (Knuth's multiplicative hash).
const LOT_MIX: u32 = 2_654_435_761;

/// A garage house's solar arrays, laid out as its type has them ([`Solar`]) or, on most
/// lots, as its lot has them: none on about one lot in five, one small block of two rows
/// of two on about one in three, and on the rest a block of ten to eighteen panels over
/// no more than [`ARRAY_SHARE`] of a slope, a quarter of those with a second block on the
/// opposite slope. A block lies flush on the slope that takes the most (of two alike, the
/// one facing its street more, or beside a front gable the one away from it; never the
/// hip end a front gable takes), up to three rows where the slope is long enough. 5
/// Rumbo's covers three slopes. Each panel is dark, satin glass in a silver frame and goes
/// with the slope it sits on.
fn solar_arrays(parts: &mut Parts, design: &HouseDesign, roof: &Roof) {
    let street = if design.door_wall == 0 {
        DVec3::Z
    } else {
        DVec3::X
    };
    let v = design.variant;
    // The lot's own draw, spread from its variant.
    let lot = v.wrapping_mul(LOT_MIX) >> 20;
    let gable = garage_gable_side(design);
    // A front-gable house's side slopes face the street alike: the one away from a
    // two-storey front gable, otherwise the variant's.
    let side = match gable {
        Some(gable) if gable > 0.0 => HousePart::LeftRoof,
        Some(_) => HousePart::RightRoof,
        None if v.is_multiple_of(2) => HousePart::RightRoof,
        None => HousePart::LeftRoof,
    };
    let preference = |slope: &Slope| {
        let facing = slope.normal.with_y(0.0).normalize_or_zero().dot(street);
        facing + if slope.piece == side { 0.1 } else { 0.0 }
    };
    let mut all = slopes(roof);
    // A front gable takes the hip end before it.
    if gable.is_some() {
        all.retain(|slope| slope.piece != HousePart::FrontRoof);
    }
    all.sort_by(|a, b| preference(b).total_cmp(&preference(a)));
    let lift = PANEL_STANDOFF
        + if design.style.cover == RoofCover::Tile {
            TILE_CAP.1 + TILE_LAP
        } else {
            0.0
        };
    let full = 10 + (v / 7) as usize % 9;
    let (panels, rows) = match design.style.solar {
        Solar::Lot if lot % 15 < 3 => return,
        Solar::Lot if lot % 15 < 8 => (SMALL_ARRAY, 2),
        Solar::Small => (SMALL_ARRAY, 2),
        // Rumbo north lot 3's main block fills its street slope.
        Solar::Three => (FULL_SLOPE_ARRAY, 3),
        Solar::Lot => (full, 3),
    };
    let wing = |slope: &Slope| {
        design
            .wing()
            .filter(|_| slope.piece == HousePart::RightRoof)
    };
    let layouts: Vec<(usize, SolarBlock)> = all
        .iter()
        .enumerate()
        .filter_map(|(k, slope)| Some((k, solar_layout(slope, panels, rows, wing(slope))?)))
        .collect();
    // The slope that takes the most panels (a long side slope of a hip roof rather than
    // its small end), the one nearer facing the street of two that take as many.
    let most = layouts.iter().map(|(_, block)| block.count()).max();
    let Some(&(first, block)) = layouts
        .iter()
        .find(|(_, block)| Some(block.count()) == most)
    else {
        return;
    };
    solar_block(parts, &all[first], block, lift);
    match design.style.solar {
        Solar::Three => {
            // Two more blocks on the next slopes the street sees most of.
            let others = all
                .iter()
                .enumerate()
                .filter(|&(k, _)| k != first)
                .map(|(_, slope)| slope)
                .take(2);
            for slope in others {
                if let Some(block) = solar_layout(slope, SIDE_ARRAY, 2, wing(slope)) {
                    solar_block(parts, slope, block, lift);
                }
            }
        }
        Solar::Lot if panels == full && (lot / 15).is_multiple_of(4) => {
            let opposite = all
                .iter()
                .find(|slope| slope.normal.dot(all[first].normal) < -0.5);
            if let Some(opposite) = opposite
                && let Some(block) =
                    solar_layout(opposite, 4 + (v / 11) as usize % 5, 2, wing(opposite))
            {
                solar_block(parts, opposite, block, lift);
            }
        }
        _ => {}
    }
}

/// Where a block of panels lies on a slope: its rows and columns and its middle across
/// the slope.
#[derive(Clone, Copy)]
struct SolarBlock {
    rows: usize,
    columns: usize,
    middle: f64,
}

impl SolarBlock {
    fn count(&self) -> usize {
        self.rows * self.columns
    }
}

/// Up to `count` panels in up to `most_rows` rows on `slope`, low on it and centred in
/// the widest stretch along it clear of its edges (and of a cross gable, `wing`); none
/// where not one fits.
fn solar_layout(
    slope: &Slope,
    count: usize,
    most_rows: usize,
    wing: Option<Wing>,
) -> Option<SolarBlock> {
    let (across, up) = PANEL_PITCH;
    // The stretch along the slope clear of the cross gable, `top` up it.
    let stretch = |top: f64| {
        let reach = slope.half(top) - PANEL_MARGIN;
        let mut stretches = vec![(-reach, reach)];
        if let Some(wing) = wing {
            let clear = wing.half + WING_OVERHANG + 0.3;
            stretches = vec![
                (-reach, (wing.z - clear).min(reach)),
                ((wing.z + clear).max(-reach), reach),
            ];
        }
        stretches
            .into_iter()
            .max_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0)))
            .expect("a stretch")
    };
    // As many rows as give the most panels, `most_rows` at most.
    let layout = |rows: usize| {
        let top = PANEL_BOTTOM + rows as f64 * up;
        let (from, to) = stretch(top);
        let span = (to - from).min(2.0 * slope.half(PANEL_BOTTOM) * ARRAY_SHARE);
        let fits = (span / across).floor().max(0.0) as usize;
        let columns = count.div_ceil(rows).min(fits);
        (top <= slope.length - PANEL_MARGIN && columns > 0).then_some(SolarBlock {
            rows,
            columns,
            middle: (from + to) / 2.0,
        })
    };
    [3, 2, 1]
        .into_iter()
        .filter(|&rows| rows <= most_rows)
        .filter_map(layout)
        .max_by_key(|block| (block.count(), block.rows))
}

/// The panels of `block` on `slope`, standing `lift` off its deck.
fn solar_block(parts: &mut Parts, slope: &Slope, block: SolarBlock, lift: f64) {
    let (across, up) = PANEL_PITCH;
    let kit = parts.kit(slope.piece);
    for row in 0..block.rows {
        let s = PANEL_BOTTOM + (row as f64 + 0.5) * up;
        for column in 0..block.columns {
            let a = block.middle + (column as f64 - (block.columns as f64 - 1.0) / 2.0) * across;
            let basis = DMat4::from_cols(
                slope.across.extend(0.0),
                slope.normal.extend(0.0),
                slope.up.extend(0.0),
                slope.at(s, a, lift).extend(1.0),
            );
            // A thin silver aluminium frame under the glass, which lies on it rather
            // than in it.
            kit.block(
                Surface::Metal,
                DVec3::new(PANEL.0, PANEL_DEPTH, PANEL.1),
                basis,
            );
            kit.block(
                Surface::Solar,
                DVec3::new(PANEL.0 - 0.05, GLASS_PROUD, PANEL.1 - 0.05),
                basis * shift(DVec3::Y * ((PANEL_DEPTH + GLASS_PROUD) / 2.0)),
            );
        }
    }
}

/// A wall lantern `size` times a cottage's: back plate, arm, glass box under a cap and
/// finial.
fn lantern(kit: &mut Kit<Surface>, frame: DMat4, u: f64, y: f64, size: f64) {
    let frame = frame
        * DMat4::from_translation(DVec3::new(u, y, 0.0))
        * DMat4::from_scale(DVec3::splat(size));
    let at = |p: DVec3| frame * DMat4::from_translation(p);
    kit.block(
        Surface::Iron,
        DVec3::new(0.09, 0.24, 0.02),
        at(DVec3::new(0.0, 0.0, 0.01)),
    );
    kit.block(
        Surface::Iron,
        DVec3::new(0.03, 0.03, 0.14),
        at(DVec3::new(0.0, 0.06, 0.08)),
    );
    let center = DVec3::new(0.0, -0.04, 0.17);
    kit.block(Surface::Lamp, DVec3::new(0.12, 0.17, 0.12), at(center));
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        kit.block(
            Surface::Iron,
            DVec3::new(0.018, 0.19, 0.018),
            at(center + DVec3::new(sx * 0.06, 0.0, sz * 0.06)),
        );
    }
    kit.chamfer_block(
        Surface::Iron,
        DVec3::new(0.17, 0.05, 0.17),
        0.02,
        at(center + DVec3::new(0.0, 0.11, 0.0)),
    );
    let top = frame.transform_point3(center + DVec3::new(0.0, 0.135, 0.0));
    kit.post(Surface::Iron, 0.012 * size, 0.06 * size, top, 6);
}

/// A gable roof's deck: soffits, rake undersides and the eave ends, fascia boards
/// and bargeboards on both gables, each slope with its piece.
///
/// The deck has no top face. The shingle courses or the seamed sheet cover all of
/// it a few millimetres above, a few depth-buffer steps from the overhead camera,
/// so a top face would show through as pale streaks wherever it won the depth test.
/// The rafters under a falling roof's side slopes, from the ridge down to the wall
/// line, so a broken slab shows its framing.
fn rafters(parts: &mut Parts, design: &HouseDesign, roof: &Roof) {
    const SPACING: f64 = 0.6;
    const WIDTH: f64 = 0.06;
    let run = design.w / 2.0 - 0.25;
    // Inside the gable walls: past them, under the rake, they showed while the
    // cottage stood.
    let reach = (roof.ridge_half.min(roof.rake_z) - 0.2).min(design.d / 2.0 - LINING - 0.05);
    let count = (2.0 * reach / SPACING).floor().max(1.0) as i32;
    for side in [-1.0, 1.0] {
        let part = if side > 0.0 {
            HousePart::RightRoof
        } else {
            HousePart::LeftRoof
        };
        let kit = parts.kit(part);
        for i in 0..=count {
            let z = -reach + 2.0 * reach * f64::from(i) / f64::from(count);
            let under = |x: f64| {
                DVec3::new(
                    side * x,
                    roof.top(side * x, z) - roof.thick - RAFTER_DEPTH / 2.0,
                    z,
                )
            };
            let (from, to) = (under(0.05), under(run));
            let length = (to - from).length();
            kit.block(
                Surface::Wood,
                DVec3::new(WIDTH, RAFTER_DEPTH, length),
                aim(from, to) * DMat4::from_translation(DVec3::Z * (length / 2.0)),
            );
        }
    }
}

fn gable_deck(parts: &mut Parts, roof: &Roof) {
    /// The section's side from the ridge top down to the eave top.
    const DECK_TOP: (usize, usize) = (0, 2);
    let (xe, ye, zr, t) = (roof.eave_x, roof.eave_y, roof.rake_z, roof.thick);
    for side in [-1.0, 1.0] {
        let piece = if side > 0.0 {
            HousePart::RightRoof
        } else {
            HousePart::LeftRoof
        };
        let split = parts.split;
        let kit = parts.kit(piece);
        // The deck between `outer` and `inner` (m out from the ridge).
        let section = |z: f64, outer: f64, inner: f64| {
            // Exactly the eave's and the ridge's heights at their ends.
            let under = |x: f64| {
                if x <= 0.0 {
                    roof.ridge
                } else if x >= xe {
                    ye
                } else {
                    ye + (roof.ridge - ye) * (1.0 - x / xe)
                }
            };
            // The ridge line at +0, as the deck always had it.
            let across = |x: f64| if x == 0.0 { 0.0 } else { side * x };
            vec![
                DVec3::new(across(outer), under(outer), z),
                DVec3::new(across(inner), under(inner), z),
                DVec3::new(across(inner), under(inner) + t, z),
                DVec3::new(across(outer), under(outer) + t, z),
            ]
        };
        if split {
            // A broken roof shows the raw boards of its deck from inside, and keeps
            // its painted soffit only under the overhang, outside the wall line.
            let wall = (xe - roof.overhang).clamp(0.0, xe);
            let deck = section;
            kit.solid_skipping(
                Surface::Trim,
                &[deck(-zr, xe, wall), deck(zr, xe, wall)],
                Some(DECK_TOP),
            );
            kit.solid_skipping(
                Surface::Sheathing,
                &[deck(-zr, wall, 0.0), deck(zr, wall, 0.0)],
                Some(DECK_TOP),
            );
        } else {
            // The painted soffit, on the roof as it stands whole.
            kit.solid_skipping(
                Surface::Trim,
                &[section(-zr, xe, 0.0), section(zr, xe, 0.0)],
                Some(DECK_TOP),
            );
        }
        // Fascia on the eave ends, standing below the soffit.
        kit.block(
            Surface::Trim,
            DVec3::new(0.035, t + 0.13, 2.0 * zr + 0.09),
            DMat4::from_translation(DVec3::new(
                side * (xe + 0.0175),
                ye + (t - 0.1 + 0.03) / 2.0,
                0.0,
            )),
        );
        // Bargeboards follow each rake just proud of the deck.
        for gable in [-1.0, 1.0] {
            let board = |z: f64| {
                vec![
                    DVec3::new(side * (xe + 0.035), ye - 0.1, z),
                    DVec3::new(0.0, roof.ridge - 0.1, z),
                    DVec3::new(0.0, roof.ridge + t + 0.03, z),
                    DVec3::new(side * (xe + 0.035), ye + t + 0.03, z),
                ]
            };
            kit.solid(
                Surface::Trim,
                &[board(gable * zr), board(gable * (zr + 0.045))],
            );
        }
    }
}

/// A hip roof's deck: the boxed soffit round all four sides under the overhang and
/// a fascia board along every eave, each side with its slope's piece.
fn hip_deck(parts: &mut Parts, roof: &Roof) {
    let (xe, ye, ze, t) = (roof.eave_x, roof.eave_y, roof.rake_z, roof.thick);
    let (xw, zw) = (xe - roof.overhang, ze - roof.overhang);
    let edges = [
        (HousePart::RightRoof, DVec3::X, DVec3::Z, xe, xw, ze),
        (HousePart::LeftRoof, -DVec3::X, -DVec3::Z, xe, xw, ze),
        (HousePart::FrontRoof, DVec3::Z, -DVec3::X, ze, zw, xe),
        (HousePart::BackRoof, -DVec3::Z, DVec3::X, ze, zw, xe),
    ];
    for (piece, out, along, edge, wall, half) in edges {
        let kit = parts.kit(piece);
        // The soffit, a trapezoid from the eave in to the wall line.
        let at = |reach: f64, run: f64| out * reach + along * run + DVec3::Y * ye;
        let inner = half - roof.overhang;
        kit.quad(
            Surface::Trim,
            [
                at(edge, -half),
                at(edge, half),
                at(wall, inner),
                at(wall, -inner),
            ],
            -DVec3::Y,
        );
        let length = 2.0 * half + 0.07;
        let size = if out.x != 0.0 {
            DVec3::new(0.035, t + 0.13, length)
        } else {
            DVec3::new(length, t + 0.13, 0.035)
        };
        kit.block(
            Surface::Trim,
            size,
            DMat4::from_translation(out * (edge + 0.0175) + DVec3::Y * (ye + (t - 0.07) / 2.0)),
        );
    }
}

/// One roof slope in the build frame: its eave edge's midpoint on the deck top,
/// unit vectors up the slope and along the eave, its length to the ridge and its
/// half width at the eave and at the ridge (a hip narrows to the ridge).
struct Slope {
    piece: HousePart,
    eave: DVec3,
    up: DVec3,
    across: DVec3,
    normal: DVec3,
    length: f64,
    half_eave: f64,
    half_ridge: f64,
}

impl Slope {
    fn half(&self, s: f64) -> f64 {
        let t = (s / self.length).clamp(0.0, 1.0);
        self.half_eave + (self.half_ridge - self.half_eave) * t
    }

    fn at(&self, s: f64, a: f64, lift: f64) -> DVec3 {
        self.eave + self.up * s + self.across * a + self.normal * lift
    }
}

fn slopes(roof: &Roof) -> Vec<Slope> {
    let top = roof.eave_y + roof.thick;
    let crest = roof.ridge + roof.thick;
    let mut slopes = Vec::new();
    let make = |piece, eave: DVec3, ridge: DVec3, across: DVec3, half_eave, half_ridge| {
        let up = (ridge - eave).normalize();
        let length = (ridge - eave).length();
        make_slope(piece, eave, up, across, length, half_eave, half_ridge)
    };
    for side in [1.0, -1.0] {
        let piece = if side > 0.0 {
            HousePart::RightRoof
        } else {
            HousePart::LeftRoof
        };
        slopes.push(make(
            piece,
            DVec3::new(side * roof.eave_x, top, 0.0),
            DVec3::new(0.0, crest, 0.0),
            DVec3::Z,
            roof.rake_z,
            roof.ridge_half,
        ));
    }
    if roof.form == RoofForm::Hip {
        for end in [1.0, -1.0] {
            let piece = if end > 0.0 {
                HousePart::FrontRoof
            } else {
                HousePart::BackRoof
            };
            slopes.push(make(
                piece,
                DVec3::new(0.0, top, end * roof.rake_z),
                DVec3::new(0.0, crest, end * roof.ridge_half),
                DVec3::X,
                roof.eave_x,
                0.0,
            ));
        }
    }
    slopes
}

/// The underside and edges of a hip slope's deck, which a falling slope shows.
fn slope_underside(kit: &mut Kit<Surface>, slope: &Slope, thick: f64) {
    let corners = [
        (0.0, -slope.half_eave),
        (0.0, slope.half_eave),
        (slope.length, slope.half_ridge),
        (slope.length, -slope.half_ridge),
    ];
    let low = corners.map(|(s, a)| slope.at(s, a, -thick));
    kit.quad(Surface::Sheathing, low, -slope.normal);
    let high = corners.map(|(s, a)| slope.at(s, a, 0.0));
    let middle = high.iter().copied().sum::<DVec3>() / 4.0;
    for i in 0..4 {
        let j = (i + 1) % 4;
        let out = (high[i] + high[j]) / 2.0 - middle;
        kit.quad(Surface::Trim, [high[i], high[j], low[j], low[i]], out);
    }
}

/// Collects textured quads with flat normals for the shingle courses.
#[derive(Default)]
struct Courses {
    positions: Vec<f64>,
    normals: Vec<f64>,
    uvs: Vec<f64>,
    /// Whether each vertex is a ridge cap's, which a standing roof lights up.
    caps: Vec<bool>,
    /// Whether the quads being added now are ridge caps.
    capping: bool,
}

impl Courses {
    /// A quad `a b c d` facing roughly along `facing`, with its texture coordinates.
    fn quad(&mut self, corners: [DVec3; 4], uvs: [DVec2; 4], facing: DVec3) {
        let [a, b, c, _] = corners;
        let mut normal = (b - a).cross(c - a).normalize_or_zero();
        let order = if normal.dot(facing) < 0.0 {
            normal = -normal;
            [0, 2, 1, 0, 3, 2]
        } else {
            [0, 1, 2, 0, 2, 3]
        };
        for i in order {
            self.positions.extend(corners[i].to_array());
            self.normals.extend(normal.to_array());
            self.uvs.extend(uvs[i].to_array());
            self.caps.push(self.capping);
        }
    }

    /// The courses of a standing roof, weathered by height: darker and dirtier
    /// toward the eaves where water runs off, cleaner toward the ridge, and the
    /// ridge caps catching the light, so from overhead the roof keeps its ridge
    /// line and slopes instead of reading as one flat dark plane.
    fn weathered_mesh(&self, roof: &Roof) -> Mesh {
        let mut mesh = self.mesh();
        let (eave, crest) = (roof.eave_y, roof.ridge + roof.thick);
        mesh.colors = self
            .positions
            .as_chunks::<3>()
            .0
            .iter()
            .zip(&self.caps)
            .map(|(p, &cap)| {
                let shade = if cap {
                    RIDGE_SHINE
                } else {
                    let up = ((p[1] - eave) / (crest - eave)).clamp(0.0, 1.0);
                    ROOF_WEATHER.0 + (ROOF_WEATHER.1 - ROOF_WEATHER.0) * up
                };
                [shade as f32; 3]
            })
            .collect();
        mesh
    }

    fn mesh(&self) -> Mesh {
        Mesh::from_f64(&self.positions, &self.normals, &self.uvs, None)
    }
}

/// One slope's shingle courses from eave to ridge. Each course's texture row
/// matches its geometry: the butt edge sits on the top of a row of `shingles.webp`.
fn shingle_slope(out: &mut Courses, slope: &Slope) {
    let length = slope.length + SHINGLE_EAVE;
    let courses = (length / COURSE).round().max(1.0);
    let exposure = length / courses;
    let tile = exposure * SHINGLE_ROWS;
    let at = |s: f64, a: f64, lift: f64| slope.at(s - SHINGLE_EAVE, a, lift);
    let half = |s: f64| slope.half(s - SHINGLE_EAVE) + 0.01;
    let down = -slope.up;
    for k in 0..courses as u32 {
        let kf = f64::from(k);
        let (s0, s1) = (kf * exposure, (kf + 1.0) * exposure);
        let (v0, v1) = (kf / SHINGLE_ROWS, (kf + 1.0) / SHINGLE_ROWS);
        let (h0, h1) = (half(s0), half(s1));
        let ((a0, b0), (a1, b1)) = ((-h0, h0), (-h1, h1));
        if b0 - a0 < 0.02 && b1 - a1 < 0.02 {
            continue;
        }
        out.quad(
            [
                at(s0, a0, BUTT_LIFT),
                at(s0, b0, BUTT_LIFT),
                at(s1, b1, HEAD_LIFT),
                at(s1, a1, HEAD_LIFT),
            ],
            [
                DVec2::new(a0 / tile, v0),
                DVec2::new(b0 / tile, v0),
                DVec2::new(b1 / tile, v1),
                DVec2::new(a1 / tile, v1),
            ],
            slope.normal,
        );
        let below = if k == 0 { -0.01 } else { HEAD_LIFT };
        out.quad(
            [
                at(s0, a0, below),
                at(s0, b0, below),
                at(s0, b0, BUTT_LIFT),
                at(s0, a0, BUTT_LIFT),
            ],
            [
                DVec2::new(a0 / tile, v0),
                DVec2::new(b0 / tile, v0),
                DVec2::new(b0 / tile, v0 + 0.004),
                DVec2::new(a0 / tile, v0 + 0.004),
            ],
            down,
        );
    }
}

/// One slope of painted standing-seam steel: the sheet and ribs from the eave to the
/// ridge or the hip.
fn seam_slope(kit: &mut Kit<Surface>, slope: &Slope) {
    const LIFT: f64 = 0.004;
    let frame = DMat4::from_cols(
        slope.across.extend(0.0),
        slope.up.extend(0.0),
        slope.normal.extend(0.0),
        (slope.eave + slope.normal * LIFT).extend(1.0),
    );
    let outline = [
        DVec2::new(-slope.half_eave - 0.01, -0.02),
        DVec2::new(slope.half_eave + 0.01, -0.02),
        DVec2::new(slope.half_ridge + 0.01, slope.length),
        DVec2::new(-slope.half_ridge - 0.01, slope.length),
    ];
    let outline: Vec<DVec2> = if slope.half_ridge < 0.02 {
        vec![outline[0], outline[1], DVec2::new(0.0, slope.length)]
    } else {
        outline.to_vec()
    };
    kit.face_with_holes(Surface::Roof, &outline, &[], frame);
    let count = (slope.half_eave * 2.0 / SEAM_SPACING).floor() as i32;
    for i in -count / 2..=count / 2 {
        let a = f64::from(i) * SEAM_SPACING;
        // A rib ends where the slope narrows past it (at a hip).
        let reach = if slope.half_ridge >= a.abs() {
            slope.length
        } else {
            slope.length * (slope.half_eave - a.abs()) / (slope.half_eave - slope.half_ridge)
        };
        let (from, to) = (0.0, reach - 0.05);
        if to - from < 0.1 {
            continue;
        }
        let center = slope.at((from + to) / 2.0, a, LIFT + SEAM.1 / 2.0);
        let basis = DMat4::from_cols(
            slope.across.extend(0.0),
            slope.normal.extend(0.0),
            slope.up.extend(0.0),
            center.extend(1.0),
        );
        kit.block(Surface::Roof, DVec3::new(SEAM.0, SEAM.1, to - from), basis);
    }
}

/// Clay barrel tile: the caps' rows' spacing across a slope, a cap's width and rise, a
/// course's length down the slope and how far a course's butt stands over the course
/// below (m): the tract's fine-grained S-tile. About eight triangles a tile.
const TILE_SPACING: f64 = 0.22;
const TILE_CAP: (f64, f64) = (0.13, 0.05);
const TILE_COURSE: f64 = 0.32;
const TILE_LAP: f64 = 0.016;

/// One slope of clay barrel tile: the pans as one sheet in the clay's colour, and over
/// them rows of rounded caps running from the eave to the ridge or the hip, each row in
/// courses whose butts stand proud of the course below, so from above the roof reads as
/// ribs of tile crossed by the courses' shadow lines.
fn tile_slope(kit: &mut Kit<Surface>, slope: &Slope) {
    const LIFT: f64 = 0.004;
    let frame = DMat4::from_cols(
        slope.across.extend(0.0),
        slope.up.extend(0.0),
        slope.normal.extend(0.0),
        (slope.eave + slope.normal * LIFT).extend(1.0),
    );
    let mut outline = vec![
        DVec2::new(-slope.half_eave - 0.01, -0.03),
        DVec2::new(slope.half_eave + 0.01, -0.03),
        DVec2::new(slope.half_ridge + 0.01, slope.length),
    ];
    if slope.half_ridge >= 0.02 {
        outline.push(DVec2::new(-slope.half_ridge - 0.01, slope.length));
    } else {
        outline[2].x = 0.0;
    }
    kit.face_with_holes(Surface::Roof, &outline, &[], frame);
    let (cap, rise) = TILE_CAP;
    let count = ((slope.half_eave - cap) / TILE_SPACING).floor() as i32;
    for i in -count..=count {
        let a = f64::from(i) * TILE_SPACING;
        // A row ends where a hip narrows past it.
        let reach = if slope.half_ridge >= a.abs() + cap / 2.0 {
            slope.length
        } else {
            slope.length * (slope.half_eave - a.abs() - cap / 2.0)
                / (slope.half_eave - slope.half_ridge)
        };
        let courses = (reach / TILE_COURSE).round();
        if courses < 1.0 {
            continue;
        }
        let course = reach / courses;
        for k in 0..courses as u32 {
            let clay = Surface::Roof;
            let s0 = course * f64::from(k) - 0.03;
            let s1 = s0 + course + 0.03;
            // Each tile tapers up the slope, its butt fuller and standing proud.
            let butt = tile_profile(cap / 2.0, rise + TILE_LAP);
            let head = tile_profile(cap * 0.4, rise * 0.75);
            let at = |s: f64, p: DVec2| slope.at(s, a + p.x, LIFT + p.y);
            for j in 0..butt.len() - 1 {
                let corners = [
                    at(s0, butt[j]),
                    at(s0, butt[j + 1]),
                    at(s1, head[j + 1]),
                    at(s1, head[j]),
                ];
                let edge = butt[j + 1] - butt[j];
                let out = slope.across * -edge.y + slope.normal * edge.x;
                kit.quad(clay, corners, out);
            }
            // The butt's end, facing down the slope.
            let end: Vec<DVec3> = butt.iter().map(|&p| at(s0, p)).collect();
            kit.quad(clay, [end[0], end[1], end[2], end[3]], -slope.up);
        }
    }
}

/// A barrel tile's cross-section `half` wide either side and `rise` high: its side
/// walls up to a flat crown, from one edge over to the other.
fn tile_profile(half: f64, rise: f64) -> [DVec2; 4] {
    [
        DVec2::new(-half, 0.0),
        DVec2::new(-half * 0.55, rise),
        DVec2::new(half * 0.55, rise),
        DVec2::new(half, 0.0),
    ]
}

/// Cap shingles or a seamed cap along the ridge, and caps up every hip.
fn ridge_caps(parts: &mut Parts, design: &HouseDesign, roof: &Roof) {
    let crest = roof.ridge + roof.thick;
    let shingles = design.style.cover == RoofCover::Shingles;
    let tiled = matches!(design.style.cover, RoofCover::Tile | RoofCover::FlatTile);
    if shingles && roof.ridge_half > 0.01 {
        ridge_shingles(parts, roof);
    } else if tiled && roof.ridge_half > 0.01 {
        // A row of ridge tiles over the crest.
        let end = roof.ridge_half + 0.06;
        let profile = tile_profile(0.15, 0.1);
        let kit = parts.kit(HousePart::RightRoof);
        let at = |z: f64, p: DVec2| DVec3::new(p.x, crest + 0.02 + p.y, z);
        for k in 0..3 {
            let (a, b) = (profile[k], profile[k + 1]);
            let edge = b - a;
            kit.quad(
                Surface::Roof,
                [at(-end, a), at(-end, b), at(end, b), at(end, a)],
                DVec3::new(-edge.y, edge.x, 0.0),
            );
        }
    } else if roof.ridge_half > 0.01 {
        // A galvanised ridge roll, catching the light along the crest.
        let end = roof.ridge_half + 0.02;
        let (a, b) = (-end, end);
        parts.kit(HousePart::RightRoof).block(
            Surface::Metal,
            DVec3::new(0.26, 0.07, b - a),
            shift(DVec3::new(0.0, crest + 0.03, (a + b) / 2.0)),
        );
    }
    if roof.form != RoofForm::Hip {
        return;
    }
    // A Spanish house's front gable carries the side slope forward over its hip there.
    let gable = garage_gable_side(design);
    for (sx, sz) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
        if sz > 0.0 && gable == Some(sx) {
            continue;
        }
        let from = DVec3::new(0.0, crest + 0.03, sz * roof.ridge_half);
        let to = DVec3::new(
            sx * roof.eave_x,
            roof.eave_y + roof.thick + 0.03,
            sz * roof.rake_z,
        );
        let length = (to - from).length();
        let piece = if sz > 0.0 {
            HousePart::FrontRoof
        } else {
            HousePart::BackRoof
        };
        // Galvanised hip flashing (on a tile roof, hip tiles): the hips stand out as
        // lines from overhead.
        parts.kit(piece).block(
            if tiled { Surface::Roof } else { Surface::Metal },
            DVec3::new(0.18, 0.05, length),
            aim(from, to) * DMat4::from_translation(DVec3::Z * (length / 2.0)),
        );
    }
}

/// Cap shingles lapping along the ridge, butts facing +z like the walls' front;
/// each half goes with its slope.
fn ridge_shingles(parts: &mut Parts, roof: &Roof) {
    let z_end = roof.ridge_half + 0.01;
    let pieces = (2.0 * z_end / RIDGE_PIECE).round().max(1.0);
    let piece = 2.0 * z_end / pieces;
    let crest = roof.ridge + roof.thick;
    let edge_y = roof.top(RIDGE_HALF, 0.0);
    for j in 0..pieces as u32 {
        let j = f64::from(j);
        let (z1, z0) = (z_end - j * piece, z_end - (j + 1.0) * piece);
        let apex = |z: f64, lift: f64| DVec3::new(0.0, crest + 0.04 + lift, z);
        let edge = |side: f64, z: f64, lift: f64| DVec3::new(side * RIDGE_HALF, edge_y + lift, z);
        let (v0, v1) = (j / SHINGLE_ROWS, (j + 1.0) / SHINGLE_ROWS);
        let u = RIDGE_HALF / (piece * SHINGLE_ROWS);
        for side in [-1.0, 1.0] {
            let out = parts.courses(if side > 0.0 {
                HousePart::RightRoof
            } else {
                HousePart::LeftRoof
            });
            out.capping = true;
            let facing = DVec3::new(side * roof.pitch, 1.0, 0.0);
            out.quad(
                [
                    edge(side, z1, BUTT_LIFT + HEAD_LIFT),
                    apex(z1, BUTT_LIFT),
                    apex(z0, 0.0),
                    edge(side, z0, HEAD_LIFT),
                ],
                [
                    DVec2::new(side * u, v0),
                    DVec2::new(0.0, v0),
                    DVec2::new(0.0, v1),
                    DVec2::new(side * u, v1),
                ],
                facing,
            );
            out.quad(
                [
                    edge(side, z1, 0.0),
                    apex(z1, -0.03),
                    apex(z1, BUTT_LIFT),
                    edge(side, z1, BUTT_LIFT + HEAD_LIFT),
                ],
                [
                    DVec2::new(side * u, v0),
                    DVec2::new(0.0, v0),
                    DVec2::new(0.0, v0 + 0.004),
                    DVec2::new(side * u, v0 + 0.004),
                ],
                DVec3::Z,
            );
            out.capping = false;
        }
    }
}

/// K-style gutters along both long eaves, open-looking from above, with their
/// slopes, and two downspouts at opposite corners with kick-outs onto splash
/// blocks, with their walls.
/// The exposed rafter tails under a garage house's street eave (its +x eave wall's),
/// dark timber out from the wall to the fascia under the soffit, between the hips.
fn eave_tails(parts: &mut Parts, design: &HouseDesign, roof: &Roof) {
    let kit = parts.kit(HousePart::RightRoof);
    let (inner, outer) = (design.w / 2.0 + 0.02, roof.eave_x - 0.06);
    let reach = design.d / 2.0 - 0.3;
    let count = (2.0 * reach / RAFTER_TAIL_SPACING).floor().max(1.0) as i32;
    for i in 0..=count {
        let z = -reach + 2.0 * reach * f64::from(i) / f64::from(count);
        let under = |x: f64| {
            DVec3::new(
                x,
                roof.top(x, z) - roof.thick - RAFTER_TAIL.1 / 2.0 - 0.01,
                z,
            )
        };
        let (from, to) = (under(inner), under(outer));
        let length = (to - from).length();
        kit.block(
            Surface::Wood,
            DVec3::new(RAFTER_TAIL.0, RAFTER_TAIL.1, length),
            aim(from, to) * DMat4::from_translation(DVec3::Z * (length / 2.0)),
        );
    }
}

fn gutters(parts: &mut Parts, w: f64, d: f64, roof: &Roof, metal: Surface) {
    let (xe, ye, zr) = (roof.eave_x, roof.eave_y, roof.rake_z);
    for side in [-1.0, 1.0] {
        let kit = parts.kit(if side > 0.0 {
            HousePart::RightRoof
        } else {
            HousePart::LeftRoof
        });
        let x = |offset: f64| side * (xe + offset);
        let profile = |z: f64| {
            vec![
                DVec3::new(x(0.035), ye + 0.02, z),
                DVec3::new(x(0.17), ye + 0.04, z),
                DVec3::new(x(0.15), ye - 0.07, z),
                DVec3::new(x(0.05), ye - 0.08, z),
            ]
        };
        let (from, to) = (-zr - 0.02, zr + 0.02);
        kit.solid(metal, &[profile(from), profile(to)]);
        // A dark channel just above the gutter's top reads as the open trough.
        let lip = |offset: f64, y: f64, z: f64| DVec3::new(x(offset), ye + y, z);
        let z = zr - 0.01;
        kit.quad(
            Surface::Dark,
            [
                lip(0.05, 0.026, -z),
                lip(0.155, 0.04, -z),
                lip(0.155, 0.04, z),
                lip(0.05, 0.026, z),
            ],
            DVec3::Y,
        );
    }
    for (side, end) in [(1.0, 1.0), (-1.0, -1.0)] {
        let top = DVec3::new(side * (xe + 0.1), ye - 0.07, end * (zr - 0.18));
        let bend = DVec3::new(side * (w / 2.0 + 0.07), ye - 0.45, end * (d / 2.0 - 0.18));
        let foot = DVec3::new(bend.x, 0.32, bend.z);
        let spout = DVec3::new(side * (w / 2.0 + 0.32), 0.1, bend.z);
        let kit = parts.kit(if side > 0.0 {
            HousePart::RightWall
        } else {
            HousePart::LeftWall
        });
        for (from, to) in [(top, bend), (bend, foot), (foot, spout)] {
            kit.rod(metal, 0.04, from, to, 6);
            kit.block(metal, DVec3::splat(0.095), DMat4::from_translation(to));
        }
        for y in [1.2, 2.3] {
            kit.block(
                metal,
                DVec3::new(0.11, 0.03, 0.11),
                DMat4::from_translation(DVec3::new(bend.x - side * 0.01, y, bend.z)),
            );
        }
        let base = parts.ground();
        base.chamfer_block(
            Surface::Concrete,
            DVec3::new(0.48, 0.05, 0.26),
            0.015,
            DMat4::from_translation(DVec3::new(side * (w / 2.0 + 0.36), 0.025, bend.z)),
        );
    }
}

/// Lived-in clutter against a cottage's walls, chosen per cottage: a stack of split
/// logs under a little plank roof against its left wall, and a rain barrel under
/// the right wall's downspout. Both stay within a step of the walls, like the
/// planters.
fn yard(kit: &mut Kit<Surface>, design: &HouseDesign) {
    const LOG: f64 = 0.19;
    const STACK: (usize, usize) = (6, 3);
    let (w, d) = (design.w, design.d);
    let mut rng = Random::new(f64::from(design.variant) * 5.0 + 3.0);
    // A tract house keeps no firewood or rain barrel.
    let stucco = design.style.walls == WallFinish::Stucco || design.garage.is_some();
    if !design.variant.is_multiple_of(3) && !stucco {
        // The stack runs along the left (-x) wall, clear of its downspout at the back.
        let x = -w / 2.0 - 0.3;
        let length = STACK.0 as f64 * (LOG + 0.02);
        let z0 = d * 0.12 - length / 2.0;
        for row in 0..STACK.1 {
            let count = STACK.0 - row % 2;
            for k in 0..count {
                let z = z0 + (k as f64 + 0.5 + (row % 2) as f64 * 0.5) * (LOG + 0.02);
                let y = 0.04 + LOG / 2.0 + row as f64 * (LOG - 0.02);
                let length = 0.44 + rng.range(-0.04, 0.06);
                let placement = pose(
                    DVec3::new(x, y, z),
                    DVec3::new(rng.range(-0.2, 0.2), 0.0, 0.0),
                );
                kit.block(Surface::Wood, DVec3::new(length, LOG, LOG), placement);
                // The pale sawn end facing the yard, so the stack reads as
                // firewood rather than a dark hole in the wall.
                let end = |y: f64, z: f64| {
                    placement.transform_point3(DVec3::new(-(length / 2.0 + 0.004), y, z))
                };
                let r = LOG * 0.43;
                kit.quad(
                    Surface::Splinters,
                    [end(-r, -r), end(-r, r), end(r, r), end(r, -r)],
                    placement.transform_vector3(-DVec3::X),
                );
            }
        }
        // A stake at each end holding the stack, open to the sun: under a plank
        // roof it sat in its own shade and read as a dark hole in the wall.
        let top = 0.04 + STACK.1 as f64 * LOG + 0.05;
        for end in [-1.0, 1.0] {
            kit.block(
                Surface::Wood,
                DVec3::new(0.06, top, 0.06),
                shift(DVec3::new(
                    x - 0.18,
                    top / 2.0,
                    z0 + length / 2.0 + end * (length / 2.0 + 0.05),
                )),
            );
        }
    }
    // Flagstones from the door steps out toward the street, laid a little
    // crooked, flush with the ground (a stucco house's concrete pavers).
    let (door, _) = wall_frames(w, d)[design.door_wall];
    let door_u = entry_u(design);
    let paving = if stucco {
        Surface::Concrete
    } else {
        Surface::Stone
    };
    let mut out = entry(design).depth() + 0.78;
    let stones = if entry(design) == Entry::BrickWing {
        0
    } else {
        PATH_STONES
    };
    for k in 0..stones {
        let length = rng.range(0.42, 0.56);
        let width = rng.range(0.55, 0.72);
        let drift = rng.range(-0.08, 0.08) + if k % 2 == 0 { 0.04 } else { -0.04 };
        let turn = rng.range(-0.18, 0.18);
        kit.block(
            paving,
            DVec3::new(width, 0.06, length),
            door * pose(
                DVec3::new(door_u + drift, 0.0, out + length / 2.0),
                DVec3::new(0.0, turn, 0.0),
            ),
        );
        out += length + rng.range(0.1, 0.16);
    }
    // A gravel drip strip along the walls the door is not in, where rain off the
    // eaves has washed the soil away, bedding the plinth into the ground: laid in
    // uneven runs, a little ragged. A stucco house's footing stands in its beds.
    for (index, (frame, half)) in wall_frames(w, d).into_iter().enumerate() {
        if index == design.door_wall || stucco {
            continue;
        }
        let mut u = -half;
        while u < half - 0.25 {
            let length = rng.range(0.9, 1.7).min(half - u);
            let width = rng.range(0.24, 0.36);
            // Only its top shows over the splash band: a quad, not a block.
            let skew = rng.range(-0.04, 0.04);
            let (near, far) = (0.05, 0.05 + width);
            kit.quad(
                Surface::Stone,
                [
                    DVec3::new(u, DRIP_STRIP, near + skew),
                    DVec3::new(u, DRIP_STRIP, far + skew),
                    DVec3::new(u + length, DRIP_STRIP, far - skew),
                    DVec3::new(u + length, DRIP_STRIP, near - skew),
                ]
                .map(|p| frame.transform_point3(p)),
                frame.transform_vector3(DVec3::Y),
            );
            u += length + rng.range(0.02, 0.14);
        }
    }
    // A band of bare, rain-splashed earth round the plinth, where the eaves drip
    // and nothing grows, ragged where the grass creeps back: it beds the cottage
    // into the ground instead of standing it on the turf. The door's steps and
    // path cross it; a garage front's porch, beds and driveway cover it.
    for (index, (frame, half)) in wall_frames(w, d).into_iter().enumerate() {
        if index == design.door_wall && design.garage.is_some() {
            continue;
        }
        let steps = if index == design.door_wall { 0.85 } else { 0.0 };
        let reach = half + SPLASH.1;
        let count = (2.0 * reach / SPLASH_STEP).ceil() as usize;
        let edge: Vec<DVec3> = (0..=count)
            .map(|k| {
                let u = -reach + 2.0 * reach * k as f64 / count as f64;
                let out = rng.range(SPLASH.0, SPLASH.1);
                DVec3::new(u, SPLASH_LIFT, out)
            })
            .collect();
        for pair in edge.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if a.x.abs().max(b.x.abs()) < steps || (a.x < steps && b.x > -steps && steps > 0.0) {
                continue;
            }
            let inner = |p: DVec3| DVec3::new(p.x, SPLASH_LIFT, 0.0);
            kit.quad(
                Surface::Wood,
                [inner(a), inner(b), b, a].map(|p| frame.transform_point3(p)),
                DVec3::Y,
            );
        }
    }
    let plan = design.wall_plan(design.door_wall);
    if let Some(garage) = plan.garage() {
        driveway(kit, door, garage, plan.garage_width(), design.roof().plinth);
    }
    if design.variant.is_multiple_of(2) && !stucco {
        // A rain barrel under the right wall's downspout.
        let at = DVec3::new(w / 2.0 + 0.36, 0.0, d / 2.0 - 0.5);
        kit.post(Surface::Wood, 0.27, 0.82, at, 10);
        for y in [0.15, 0.62] {
            kit.post(Surface::Iron, 0.285, 0.035, at + DVec3::Y * y, 10);
        }
        kit.post(Surface::Dark, 0.23, 0.01, at + DVec3::Y * 0.82, 10);
    }
}

/// A garage's concrete driveway: its apron sloping down from the garage door's sill
/// (the plinth) to the ground, then flat out toward the street, a little wider than the
/// door, with a control joint across it.
fn driveway(kit: &mut Kit<Surface>, door: DMat4, garage: f64, width: f64, plinth: f64) {
    let half = width / 2.0 + DRIVEWAY.0;
    let at = |u: f64, y: f64, out: f64| door.transform_point3(DVec3::new(garage + u, y, out));
    let section = |out: f64, top: f64| {
        vec![
            at(-half, 0.0, out),
            at(half, 0.0, out),
            at(half, top, out),
            at(-half, top, out),
        ]
    };
    // Two convex solids: a loft's faces turn away from its middle.
    kit.solid(
        Surface::Concrete,
        &[
            section(0.05, plinth - 0.01),
            section(DRIVEWAY.1, DRIVEWAY.2),
        ],
    );
    kit.solid(
        Surface::Concrete,
        &[
            section(DRIVEWAY.1, DRIVEWAY.2),
            section(DRIVEWAY.3, DRIVEWAY.2),
        ],
    );
    kit.quad(
        Surface::Dark,
        [
            at(-half, DRIVEWAY.2 + 0.003, DRIVEWAY.1 + 1.6),
            at(half, DRIVEWAY.2 + 0.003, DRIVEWAY.1 + 1.6),
            at(half, DRIVEWAY.2 + 0.003, DRIVEWAY.1 + 1.63),
            at(-half, DRIVEWAY.2 + 0.003, DRIVEWAY.1 + 1.63),
        ],
        door.transform_vector3(DVec3::Y),
    );
}

/// The kitchen garden behind a cottage, off its back wall (the one opposite the
/// door): a bed of turned soil edged with boards, planted in rows of cabbages, or
/// with bean poles and a compost heap, so every lot reads as lived in and no two
/// neighbours' look alike from above. Low enough to drive over.
fn garden(kit: &mut Kit<Surface>, plants: &mut Vec<Node>, design: &HouseDesign) {
    // A cottage stacking its firewood by the wall keeps no garden ([`yard`]).
    let Some((bed, mut rng)) = design.garden_bed() else {
        return;
    };
    let octahedron = OCTAHEDRON.get_or_insert((), || octahedron_geometry(1.0, 0));
    let (frame, _) = wall_frames(design.w, design.d)[1];
    let GardenBed {
        u0,
        u1,
        near,
        far,
        length,
        offset,
        depth,
        beans,
    } = bed;
    let at = |u: f64, out: f64, y: f64| frame.transform_point3(DVec3::new(u, y, out));
    // Turned soil inside boards laid round it.
    kit.quad(
        Surface::Wood,
        [
            at(u0, near, 0.05),
            at(u1, near, 0.05),
            at(u1, far, 0.05),
            at(u0, far, 0.05),
        ],
        DVec3::Y,
    );
    for (size, center) in [
        (
            DVec3::new(length + 0.08, 0.1, 0.05),
            DVec3::new(offset, 0.0, near),
        ),
        (
            DVec3::new(length + 0.08, 0.1, 0.05),
            DVec3::new(offset, 0.0, far),
        ),
        (
            DVec3::new(0.05, 0.1, depth),
            DVec3::new(u0, 0.0, (near + far) / 2.0),
        ),
        (
            DVec3::new(0.05, 0.1, depth),
            DVec3::new(u1, 0.0, (near + far) / 2.0),
        ),
    ] {
        kit.block(
            Surface::Splinters,
            size,
            frame * shift(center + DVec3::Y * 0.04),
        );
    }
    let mut plant = |p: DVec3, r: f64, color: u32, rng: &mut Random| {
        let mut clump = shadowed(octahedron.clone(), satin(color));
        clump.scale = DVec3::new(r, r * 0.75, r);
        clump.set_rotation_euler(rng.range(-0.3, 0.3), rng.range(0.0, PI), 0.0);
        clump.position = p;
        plants.push(clump);
    };
    if !beans {
        // Rows of cabbages along the bed, each a little out of line.
        let rows = if depth > 1.75 { 3 } else { 2 };
        for row in 0..rows {
            let out = near + depth * (row as f64 + 0.5) / rows as f64;
            let count = (length / GARDEN_SPACING).floor() as usize;
            for k in 0..count {
                let u = u0 + (k as f64 + 0.5) * length / count as f64;
                let r = rng.range(0.13, 0.18);
                let color = if (k + row) % 3 == 0 {
                    LEAVES_LIGHT
                } else {
                    CABBAGE
                };
                let p = at(
                    u + rng.range(-0.05, 0.05),
                    out + rng.range(-0.05, 0.05),
                    r * 0.6,
                );
                plant(p, r, color, &mut rng);
            }
        }
    } else {
        // Bean poles in pairs leaning together with vines up them, and a heap of
        // compost in the bed's far corner.
        for u in bed.bean_frames() {
            let top = at(u, (near + far) / 2.0, BEAN_POLE);
            for side in [-1.0, 1.0] {
                let foot = at(u, (near + far) / 2.0 + side * BEAN_SPREAD, 0.0);
                kit.rod(Surface::Splinters, 0.02, foot, top, 4);
                let p = foot.lerp(top, rng.range(0.4, 0.6));
                plant(p, rng.range(0.16, 0.22), LEAVES, &mut rng);
            }
        }
        let compost = rng.range(-0.4, 0.4);
        kit.block(
            Surface::Dark,
            DVec3::new(0.6, 0.24, 0.5),
            frame
                * pose(
                    DVec3::new(u1 - 0.4, 0.0, far - 0.35),
                    DVec3::new(0.0, compost, 0.0),
                ),
        );
    }
}

/// A brick stack rising from the floor up through the attic and the roof with a
/// corbelled top, a concrete cap, lead flashing at the roof and a clay pot; some
/// cottages strap a TV aerial to it. A ruin keeps the stack under the roof standing.
fn chimney(kit: &mut Kit<Surface>, design: &HouseDesign, roof: &Roof) {
    let flue = design.flue();
    let (cx, cz) = (flue.x, flue.z);
    let (_, full_top) = design.chimney_span();
    let bottom = PLINTH;
    // Flashing lies on the slope around the stack's base.
    let tilt = if roof.form == RoofForm::Hip && cz.abs() - roof.ridge_half > cx.abs() {
        DVec3::new(roof.pitch.atan() * cz.signum(), 0.0, 0.0)
    } else {
        DVec3::new(0.0, 0.0, roof.pitch.atan() * -cx.signum())
    };
    kit.block(
        Surface::Metal,
        DVec3::new(CHIMNEY + 0.16, 0.02, CHIMNEY + 0.16),
        pose(DVec3::new(cx, roof.top(cx, cz) + BUTT_LIFT, cz), tilt),
    );
    let top = full_top;
    kit.block(
        Surface::Brick,
        DVec3::new(CHIMNEY, top - bottom, CHIMNEY),
        DMat4::from_translation(DVec3::new(cx, (top + bottom) / 2.0, cz)),
    );
    kit.chamfer_block(
        Surface::Brick,
        DVec3::new(CHIMNEY + 0.1, 0.1, CHIMNEY + 0.1),
        0.01,
        DMat4::from_translation(DVec3::new(cx, top + 0.05, cz)),
    );
    kit.chamfer_block(
        Surface::Concrete,
        DVec3::new(CHIMNEY + 0.18, 0.06, CHIMNEY + 0.18),
        0.015,
        DMat4::from_translation(DVec3::new(cx, top + 0.13, cz)),
    );
    let pot_base = DVec3::new(cx, top + 0.16, cz);
    let pot_height = flue.y - pot_base.y;
    kit.turned(
        Surface::Terracotta,
        &[
            (0.0, 0.0),
            (0.12, 0.0),
            (0.12, 0.05),
            (0.095, 0.09),
            (0.085, pot_height - 0.04),
            (0.105, pot_height - 0.03),
            (0.105, pot_height),
            (0.0, pot_height),
        ]
        .map(|(r, z)| DVec2::new(r, z)),
        10,
        aim(pot_base, pot_base + DVec3::Y),
    );
    kit.post(Surface::Dark, 0.075, 0.004, DVec3::new(cx, flue.y, cz), 10);
    if design.antenna {
        let mast = cx + CHIMNEY / 2.0 + 0.04;
        for y in [top - 0.25, top - 0.05] {
            kit.block(
                Surface::Metal,
                DVec3::new(CHIMNEY + 0.02, 0.025, CHIMNEY + 0.02),
                DMat4::from_translation(DVec3::new(cx, y, cz)),
            );
        }
        kit.post(
            Surface::Iron,
            0.02,
            1.45,
            DVec3::new(mast, top - 0.3, cz),
            6,
        );
        let boom_y = top + 1.0;
        kit.rod(
            Surface::Iron,
            0.012,
            DVec3::new(mast, boom_y, cz - 0.55),
            DVec3::new(mast, boom_y, cz + 0.55),
            5,
        );
        for i in 0..6 {
            let z = cz - 0.5 + f64::from(i) * 0.2;
            let half = 0.28 - f64::from(i) * 0.025;
            kit.rod(
                Surface::Iron,
                0.007,
                DVec3::new(mast - half, boom_y, z),
                DVec3::new(mast + half, boom_y, z),
                4,
            );
        }
    }
}
