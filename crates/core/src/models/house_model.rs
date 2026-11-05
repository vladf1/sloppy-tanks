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
    BRICK_TEXTURE, CASING, Glazing, Opening, Palette, Surface, door, opening_casing, shutter_width,
    shutters, surface_material, textured, uv_per_metre, window,
};
use super::house_surfaces::{HouseSurface, house_material};
use super::model_primitives::{Cache, material, put, shadowed};
use super::tank_kit::{Kit, KitMeshes, aim, pose, shift};
use crate::geometry::math::scale_hex_color;
use crate::geometry::{Mesh, octahedron_geometry};
use crate::scene::{Material, Node};
use crate::sim::house_layout::{
    CHIMNEY, EAVE_OVERHANG, HouseDesign, HousePart, PLINTH, RAFTER_DEPTH, Roof, RoofCover,
    RoofForm, Shutters, WING_OVERHANG, WallFinish, Wing, window_columns,
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
const DOOR_HEIGHT: f64 = 2.0;
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
const GARDEN: (f64, f64) = (3.8, 1.8);
const GARDEN_GAP: f64 = 0.85;
const GARDEN_SPACING: f64 = 0.55;
const BEAN_POLE: f64 = 1.15;
const CABBAGE: u32 = 0x6f9257;
const PLANT_CLUMPS: u32 = 4;
const PLANT_BLOOMS: u32 = 5;
const LEAVES: u32 = 0x3d6b3a;
const LEAVES_LIGHT: u32 = 0x5a8443;
const FLOWERS: [u32; 4] = [0xd9837a, 0xf0cf6a, 0xb59ad0, 0xf2efe6];
/// The shared satin class: every untextured finish but glass draws as one.
const SATIN: (f64, f64) = (0.0, 0.6);
/// Freshly split timber, pale and warm where the light catches its ends.
const FRESH_SPLIT: u32 = 0xe2bf86;
/// The plaster inside a cottage's walls, which a fallen panel shows: old lime
/// plaster, dulled by smoke and the dust of the fall but lit like any face, so a
/// panel lying lining up reads as the inside of a room, never as a black card.
const PLASTER: u32 = 0x7b6d58;
/// Height of the gravel drip strip round a cottage's plinth.
const DRIP_STRIP: f64 = 0.035;
/// Flagstones in the path from a cottage's door toward its street.
const PATH_STONES: usize = 4;
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

    fn courses(&mut self, piece: HousePart) -> &mut Courses {
        let slot = self.slot(piece);
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
        Surface::Siding | Surface::Brick | Surface::Stone | Surface::Glass | Surface::Lamp => {
            source
        }
        // Freshly split timber catches the light, so a torn edge reads as torn timber
        // from the battlefield's height.
        Surface::Splinters => satin(FRESH_SPLIT),
        _ => satin(source.color.0),
    }
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

/// Kit texture density per finish: brick walls at the brick's scale.
fn cottage_uv(walls: WallFinish) -> impl Fn(Surface) -> f64 {
    move |surface| match surface {
        Surface::Siding if walls == WallFinish::Brick => uv_per_metre(Surface::Brick),
        _ => uv_per_metre(surface),
    }
}

static GRIMED: Cache<(u32, bool), Material> = Cache::new("house_model::GRIMED");
static WEATHERED_SHINGLES: Cache<u32, Material> = Cache::new("house_model::WEATHERED_SHINGLES");

/// A standing cottage's walls, weathered darker toward the plinth where rain
/// splashes and soil climbs: the siding material taking vertex colours that fall
/// off linearly from the ridge (`ridge` metres up) to the plinth.
fn grime(meshes: &mut KitMeshes<Surface>, ridge: f64) {
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
    for (surface, mesh) in meshes {
        let material = match surface {
            Surface::Siding => {
                GRIMED.get_or_insert((palette.siding, walls == WallFinish::Brick), || Material {
                    vertex_colors: true,
                    ..(*cottage_material(Surface::Siding, palette, walls)).clone()
                })
            }
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
    pub(super) shingles: Option<Mesh>,
    pub(super) nodes: Vec<Node>,
}

pub(super) fn cottage_pieces(design: &HouseDesign, x: f64, z: f64) -> CottagePieces {
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
    let mut ground = Node::group("");
    let mut pieces = Vec::new();
    for (((part, kit), courses), nodes) in
        HousePart::ALL.into_iter().zip(kits).zip(courses).zip(nodes)
    {
        let mut meshes = kit.finish_scaled(&uv);
        grime(&mut meshes, roof.ridge);
        if part == HousePart::Ground {
            add_weathered(&mut ground, meshes, &palette, walls);
            ground.children.extend(nodes);
            continue;
        }
        let shingles = (!courses.positions.is_empty()).then(|| courses.weathered_mesh(&roof));
        pieces.push(CottagePiece {
            part,
            meshes,
            shingles,
            nodes,
        });
    }
    add_weathered(&mut ground, lot.finish_scaled(&uv), &palette, walls);
    ground.children.extend(lot_nodes);
    CottagePieces {
        roof,
        palette,
        parts: pieces,
        openings,
        ground,
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
    add_weathered(group, meshes, palette, design.style.walls);
}

/// A cottage's shingle courses, weathered as the standing roof's.
pub(super) fn cottage_shingles(mesh: Mesh, palette: &Palette) -> Node {
    let material = WEATHERED_SHINGLES.get_or_insert(palette.roof, || Material {
        vertex_colors: true,
        ..(*house_material(HouseSurface::Shingles, palette.roof)).clone()
    });
    shadowed(Arc::new(mesh), material)
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

/// Window centres along a wall: either side of the door, a pair or one on a
/// gable or long wall.
fn window_positions(role: WallRole, half: f64) -> Vec<f64> {
    window_columns(role.door, role.gable, half)
}

fn window_opening(u: f64, roof: &Roof) -> Opening {
    Opening {
        u,
        y: PLINTH + 0.9,
        width: WINDOW_WIDTH,
        height: (roof.wall - PLINTH - 1.25).min(1.3),
    }
}

/// A two-storey house's belt course between its floors: its height (m) and how
/// far over the ground-floor windows' heads it runs, and how far over it the
/// upper windows' sills sit and the tallest they get.
const BELT: f64 = 0.12;
const BELT_OVER: f64 = 0.3;
const UPPER_SILL: f64 = 0.35;
const UPPER_WINDOW: f64 = 1.2;

/// Where a two-storey house's belt course runs (its middle).
fn belt_line(roof: &Roof) -> f64 {
    window_opening(0.0, roof).top() + BELT_OVER
}

/// An upper-floor window over `u`, between the belt course and the eave.
fn upper_window(u: f64, roof: &Roof) -> Opening {
    let y = belt_line(roof) + UPPER_SILL;
    Opening {
        u,
        y,
        width: WINDOW_WIDTH,
        height: (roof.wall - 0.35 - y).min(UPPER_WINDOW),
    }
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
            DVec3::new(STUD.0, top - PLINTH - 0.05, STUD.1),
            frame * shift(DVec3::new(u, (top + PLINTH) / 2.0, z)),
        );
    }
    // The top plate, and rails across the studs under the sills and, in a
    // two-storey wall, where the upper floor's joists bore, broken by the openings
    // they meet: a fallen wall's back reads as a frame of timber rather than a
    // board.
    let mut rails = vec![top - STUD.0, PLINTH + 0.72];
    if top - PLINTH > 3.6 {
        rails.push(PLINTH + (top - PLINTH) * 0.5);
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

/// The two fieldstone steps up to the door.
fn door_steps(kit: &mut Kit<Surface>, design: &HouseDesign) {
    let (door_frame, _) = wall_frames(design.w, design.d)[design.door_wall];
    for (width, depth, top) in [(1.5, 0.62, PLINTH / 2.0), (1.3, 0.32, PLINTH)] {
        kit.chamfer_block(
            Surface::Stone,
            DVec3::new(width, PLINTH / 2.0, depth),
            0.02,
            door_frame
                * DMat4::from_translation(DVec3::new(0.0, top - PLINTH / 4.0, 0.05 + depth / 2.0)),
        );
    }
}

/// The fieldstone plinth and the steps up to the door: a cottage's first section
/// ([`PartsCut`]). A structural cottage's plinth goes with the wall parts it carries
/// (`house_parts_model::footings`), so a breach a tank fits through reaches the ground.
fn house_base(parts: &mut Parts, design: &HouseDesign) {
    let (w, d) = (design.w, design.d);
    let split = parts.split;
    let base = parts.ground();
    if !split {
        base.chamfer_block(
            Surface::Stone,
            DVec3::new(w + 0.1, PLINTH, d + 0.1),
            0.03,
            DMat4::from_translation(DVec3::new(0.0, PLINTH / 2.0, 0.0)),
        );
    }
    door_steps(base, design);
}

/// A wall's openings.
struct WallOpenings {
    /// Its windows, the ground floor's first.
    windows: Vec<Opening>,
    ground_floor: usize,
    /// The windows, the door and the gable vent.
    openings: Vec<Opening>,
    door_opening: Opening,
    vent_opening: Opening,
    vent: bool,
    two_storey: bool,
}

/// The openings of wall `index`.
fn wall_openings(design: &HouseDesign, roof: &Roof, index: usize) -> WallOpenings {
    let half = wall_frames(design.w, design.d)[index].1;
    let role = wall_role(design, index);
    let door_opening = Opening {
        u: 0.0,
        y: PLINTH,
        width: DOOR_WIDTH,
        height: DOOR_HEIGHT,
    };
    let mut windows: Vec<Opening> = window_positions(role, half)
        .into_iter()
        .map(|u| window_opening(u, roof))
        .collect();
    // The ground floor's windows, which carry window boxes and share a wall's
    // shutters among themselves.
    let ground_floor = windows.len();
    let two_storey = design.storeys() == 2;
    if two_storey {
        // The upper floor lights each ground-floor window's bay and the
        // stair over the door.
        let mut above: Vec<f64> = windows.iter().map(|o| o.u).collect();
        if role.door {
            above.push(0.0);
        }
        windows.extend(above.into_iter().map(|u| upper_window(u, roof)));
    }
    let mut openings = windows.clone();
    if role.door {
        openings.push(door_opening);
    }
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
    let brick = design.style.walls == WallFinish::Brick;
    let role = wall_role(design, index);
    let piece = HousePart::wall(index);
    let mut outline = vec![
        DVec2::new(-half, PLINTH),
        DVec2::new(half, PLINTH),
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
        vent_opening,
        vent,
        two_storey,
    } = wall_openings(design, roof, index);
    parts.openings[index] = openings
        .iter()
        .map(|o| [o.u - o.width / 2.0, o.y, o.u + o.width / 2.0, o.top()])
        .collect();
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
            DVec2::new(-half, PLINTH),
            DVec2::new(half, PLINTH),
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
        let boarded = !role.door && Random::new(seed + 0.7).next() < BOARDED_WINDOWS;
        if boarded {
            boarded_window(kit, frame, opening, seed);
        } else if upper {
            // Bedroom casements upstairs: one sash of four lites.
            window(kit, frame, opening, Glazing { double_hung: false }, REVEAL);
        } else {
            window(kit, frame, opening, glazing, REVEAL);
        }
        let inner_limit = if role.door { door_edge } else { 0.0 };
        let room = shutter_room(opening, half, inner_limit, ground_floor == 1);
        // Shutters dress the street front and the sides; the back of the
        // cottage, behind its door, makes do without.
        let back = role.planted && !role.door;
        // The upper floor's windows go without: shutters on both floors
        // cost more triangles than the second row of windows itself.
        if room > 0.0 && !back && !upper {
            match design.style.shutters {
                Shutters::Louvred => shutters(kit, frame, opening, room),
                Shutters::Board => board_shutters(kit, frame, opening, room),
                Shutters::None => {}
            }
        }
        if role.planted && !upper {
            window_box(kit, frame, opening);
        }
    }
    if two_storey {
        // The belt course between the floors, broken by the door's porch.
        kit.block(
            Surface::Trim,
            DVec3::new(2.0 * half, BELT, TRIM_PROUD * 1.5),
            frame * shift(DVec3::new(0.0, belt_line(roof), TRIM_PROUD * 0.75)),
        );
    }
    if role.door {
        door(kit, frame, &door_opening, REVEAL);
        if design.style.porch && role.gable {
            porch(kit, frame, half, roof);
        } else {
            canopy(kit, frame, &door_opening);
        }
        lantern(kit, frame, door_edge + 0.14, PLINTH + 1.55);
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
        !brick,
        role.door.then_some(door_edge),
    );
    if role.planted {
        // The plants ride on the wall's parts too.
        let slot = parts.slot(piece);
        window_plants(
            &mut parts.nodes[slot],
            frame,
            half,
            role,
            roof,
            design.variant,
        );
    }
    if role.door {
        foundation_bed(parts, frame, half, design.variant);
    }
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
        match design.style.cover {
            RoofCover::Shingles => shingle_slope(parts.courses(slope.piece), &slope),
            RoofCover::Seams => seam_slope(parts.kit(slope.piece), &slope),
        }
    }
    ridge_caps(parts, design, roof);
    gutters(parts, w, d, roof);
    yard(&mut parts.lot, design);
    garden(&mut parts.lot, &mut parts.lot_nodes, design);
    chimney(parts.kit(HousePart::Chimney), design, roof);
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
        match design.style.cover {
            RoofCover::Shingles => shingle_slope(parts.courses(piece), &wing_slope),
            RoofCover::Seams => seam_slope(parts.kit(piece), &wing_slope),
        }
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
    let reach = (roof.eave_x * 0.72).min(roof.eave_x - EAVE_OVERHANG - 0.1);
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
/// door) and, on eave walls, the frieze board under the eaves.
#[allow(clippy::too_many_arguments)]
fn wall_trim(
    kit: &mut Kit<Surface>,
    frame: DMat4,
    half: f64,
    roof: &Roof,
    end_wall: bool,
    gable: bool,
    corner_boards: bool,
    door_edge: Option<f64>,
) {
    let block = |kit: &mut Kit<Surface>, size: DVec3, at: DVec3| {
        kit.block(Surface::Trim, size, frame * DMat4::from_translation(at));
    };
    const TABLE: f64 = 0.14;
    let board_bottom = PLINTH + TABLE;
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
    if let Some(edge) = door_edge {
        runs = vec![(-reach - 0.01, -edge), (edge, reach + 0.01)];
    }
    for (from, to) in runs {
        kit.chamfer_block(
            Surface::Trim,
            DVec3::new(to - from, TABLE, 0.04),
            0.012,
            frame
                * DMat4::from_translation(DVec3::new(
                    (from + to) / 2.0,
                    PLINTH + TABLE / 2.0,
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
fn window_plants(
    nodes: &mut Vec<Node>,
    frame: DMat4,
    half: f64,
    role: WallRole,
    roof: &Roof,
    v: u32,
) {
    let octahedron = OCTAHEDRON.get_or_insert((), || octahedron_geometry(1.0, 0));
    let mut rng = Random::new(f64::from(v) + 17.0 + role.index as f64 * 3.0);
    let flower = FLOWERS[(v / 3) as usize % FLOWERS.len()];
    let mut group = Node::group("");
    for u in window_positions(role, half) {
        let opening = window_opening(u, roof);
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

/// A bark-mulch bed either side of the door steps, each planted with two shrubs,
/// so the cottage sits in its yard rather than on bare ground.
fn foundation_bed(parts: &mut Parts, frame: DMat4, half: f64, v: u32) {
    let octahedron = OCTAHEDRON.get_or_insert((), || octahedron_geometry(1.0, 0));
    let mut rng = Random::new(f64::from(v) * 7.0 + 29.0);
    let (from, to) = (0.95, half - 0.2);
    if to - from < 0.5 {
        return;
    }
    let mut group = Node::group("");
    for side in [-1.0, 1.0] {
        let base = parts.ground();
        base.block(
            Surface::Wood,
            DVec3::new(to - from, 0.05, 0.62),
            frame * shift(DVec3::new(side * (from + to) / 2.0, 0.0, 0.36)),
        );
        for u in [from + 0.3, to - 0.3] {
            let size = rng.range(SHRUB_SIZE.0, SHRUB_SIZE.1);
            for lump in 0..SHRUB_LUMPS {
                let scale = if lump == 0 { 1.0 } else { rng.range(0.6, 0.8) };
                let r = size * scale;
                let local = DVec3::new(
                    side * u + rng.range(-0.18, 0.18) * f64::from(u8::from(lump > 0)),
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

/// A wall lantern: back plate, arm, glass box under a cap and finial.
fn lantern(kit: &mut Kit<Surface>, frame: DMat4, u: f64, y: f64) {
    let at = |p: DVec3| frame * DMat4::from_translation(p);
    kit.block(
        Surface::Iron,
        DVec3::new(0.09, 0.24, 0.02),
        at(DVec3::new(u, y, 0.01)),
    );
    kit.block(
        Surface::Iron,
        DVec3::new(0.03, 0.03, 0.14),
        at(DVec3::new(u, y + 0.06, 0.08)),
    );
    let center = DVec3::new(u, y - 0.04, 0.17);
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
    kit.post(Surface::Iron, 0.012, 0.06, top, 6);
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
        // A falling slab shows the boards of its deck, not a painted soffit.
        // The painted soffit, on the roof's parts as it stood.
        let deck = Surface::Trim;
        let kit = parts.kit(piece);
        let section = |z: f64| {
            vec![
                DVec3::new(side * xe, ye, z),
                DVec3::new(0.0, roof.ridge, z),
                DVec3::new(0.0, roof.ridge + t, z),
                DVec3::new(side * xe, ye + t, z),
            ]
        };
        kit.solid_skipping(deck, &[section(-zr), section(zr)], Some(DECK_TOP));
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
    let (xw, zw) = (xe - EAVE_OVERHANG, ze - EAVE_OVERHANG);
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
        let inner = half - EAVE_OVERHANG;
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
            length: (ridge - eave).length(),
            half_eave,
            half_ridge,
        }
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
    kit.quad(Surface::Wood, low, -slope.normal);
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

/// Cap shingles or a seamed cap along the ridge, and caps up every hip.
fn ridge_caps(parts: &mut Parts, design: &HouseDesign, roof: &Roof) {
    let crest = roof.ridge + roof.thick;
    let shingles = design.style.cover == RoofCover::Shingles;
    if shingles && roof.ridge_half > 0.01 {
        ridge_shingles(parts, roof);
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
    for (sx, sz) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
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
        // Galvanised hip flashing: the hips stand out as lines from overhead.
        parts.kit(piece).block(
            Surface::Metal,
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
fn gutters(parts: &mut Parts, w: f64, d: f64, roof: &Roof) {
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
        kit.solid(Surface::Metal, &[profile(from), profile(to)]);
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
            kit.rod(Surface::Metal, 0.04, from, to, 6);
            kit.block(
                Surface::Metal,
                DVec3::splat(0.095),
                DMat4::from_translation(to),
            );
        }
        for y in [1.2, 2.3] {
            kit.block(
                Surface::Metal,
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
    if !design.variant.is_multiple_of(3) {
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
    // crooked, flush with the ground.
    let (door, _) = wall_frames(w, d)[design.door_wall];
    let mut out = 0.78;
    for k in 0..PATH_STONES {
        let length = rng.range(0.42, 0.56);
        let width = rng.range(0.55, 0.72);
        let drift = rng.range(-0.08, 0.08) + if k % 2 == 0 { 0.04 } else { -0.04 };
        door_stone(
            kit,
            door,
            drift,
            out + length / 2.0,
            width,
            length,
            rng.range(-0.18, 0.18),
        );
        out += length + rng.range(0.1, 0.16);
    }
    // A gravel drip strip along the walls the door is not in, where rain off the
    // eaves has washed the soil away, bedding the plinth into the ground: laid in
    // uneven runs, a little ragged.
    for (index, (frame, half)) in wall_frames(w, d).into_iter().enumerate() {
        if index == design.door_wall {
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
    // path cross it.
    for (index, (frame, half)) in wall_frames(w, d).into_iter().enumerate() {
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
    if design.variant.is_multiple_of(2) {
        // A rain barrel under the right wall's downspout.
        let at = DVec3::new(w / 2.0 + 0.36, 0.0, d / 2.0 - 0.5);
        kit.post(Surface::Wood, 0.27, 0.82, at, 10);
        for y in [0.15, 0.62] {
            kit.post(Surface::Iron, 0.285, 0.035, at + DVec3::Y * y, 10);
        }
        kit.post(Surface::Dark, 0.23, 0.01, at + DVec3::Y * 0.82, 10);
    }
}

/// The kitchen garden behind a cottage, off its back wall (the one opposite the
/// door): a bed of turned soil edged with boards, planted in rows of cabbages, or
/// with bean poles and a compost heap, so every lot reads as lived in and no two
/// neighbours' look alike from above. Low enough to drive over.
fn garden(kit: &mut Kit<Surface>, plants: &mut Vec<Node>, design: &HouseDesign) {
    // A cottage stacking its firewood by the wall keeps no garden ([`yard`]).
    if !design.variant.is_multiple_of(3) {
        return;
    }
    let octahedron = OCTAHEDRON.get_or_insert((), || octahedron_geometry(1.0, 0));
    let lot = (design.style.siding >> 3) ^ design.turns.rem_euclid(4) as u32;
    let mut rng = Random::new(f64::from(design.variant) * 3.0 + f64::from(lot % 97) + 77.0);
    let (frame, half) = wall_frames(design.w, design.d)[1];
    let length = (2.0 * half - 0.9).min(GARDEN.0);
    let offset = rng.range(-0.5, 0.5) * (2.0 * half - 0.6 - length).max(0.0);
    let depth = GARDEN.1 + rng.range(-0.15, 0.2);
    let (near, far) = (GARDEN_GAP, GARDEN_GAP + depth);
    let at = |u: f64, out: f64, y: f64| frame.transform_point3(DVec3::new(u, y, out));
    let (u0, u1) = (offset - length / 2.0, offset + length / 2.0);
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
    if lot.is_multiple_of(2) {
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
        let count = (length / 0.8).floor().clamp(2.0, 4.0) as usize;
        for k in 0..count {
            let u = u0 + 0.3 + k as f64 * (length - 0.9) / (count - 1) as f64;
            let top = at(u, (near + far) / 2.0, BEAN_POLE);
            for side in [-1.0, 1.0] {
                let foot = at(u, (near + far) / 2.0 + side * 0.3, 0.0);
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

fn door_stone(
    kit: &mut Kit<Surface>,
    door: DMat4,
    u: f64,
    out: f64,
    width: f64,
    length: f64,
    turn: f64,
) {
    kit.block(
        Surface::Stone,
        DVec3::new(width, 0.06, length),
        door * pose(DVec3::new(u, 0.0, out), DVec3::new(0.0, turn, 0.0)),
    );
}

/// A brick stack rising through the roof with a corbelled top, a concrete cap,
/// lead flashing at the roof and a clay pot; some cottages strap a TV aerial to
/// it. A chimney a roof strike toppled stands as a ragged stub, its bricks
/// strewn down the slope.
fn chimney(kit: &mut Kit<Surface>, design: &HouseDesign, roof: &Roof) {
    let flue = design.flue();
    let (cx, cz) = (flue.x, flue.z);
    let (bottom, full_top) = design.chimney_span();
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
