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
//! Shells leave their mark: holes punched through the siding where they struck,
//! then broken windows, and once the cottage is wrecked a sagging, holed roof, a
//! toppled chimney and an empty doorway; some cottages already have a window or
//! two boarded over from earlier shelling. A collapsed cottage falls apart into
//! the [`HousePart`]s it is built from ([`house_pieces`]), torn timber split pale
//! along every break and plaster and lath behind, and leaves a heap of thick broken
//! wall and roof under the ragged stub of its back wall and corners, and a burnt
//! floor ([`house_heap`]).
//!
//! Every finish but the textured walls, stone, brick and shingles shares one satin
//! class (glass stays glossy), so a standing cottage costs at most six draws; a
//! collapse piece and the heap paint every finish and cost one each.

use std::f64::consts::PI;
use std::sync::Arc;

use glam::{DMat4, DVec2, DVec3};

use super::batching::batch;
use super::building_kit::{
    BRICK_TEXTURE, CASING, Glazing, Opening, Palette, Surface, door, opening_casing, shutter_width,
    shutters, surface_material, textured, uv_per_metre, wall_with_breaches, window,
};
use super::house_collapse::{
    chimney_breaks, clip_mesh, clip_meshes, fold_shard, mottle, roof_breaks, wall_breaks,
};
use super::house_surfaces::{HouseSurface, house_material};
use super::model_primitives::{Cache, material, put, shadowed};
use super::tank_kit::{Kit, KitMeshes, aim, pose, shift};
use crate::geometry::math::{multiply_hex, scale_hex_color};
use crate::geometry::{Mesh, narrow, octahedron_geometry, widen};
use crate::scene::{Material, Node};
use crate::sim::house_layout::{
    CHIMNEY, EAVE_OVERHANG, HOUSE_HEAP, HouseDesign, HousePart, HousePiece, PLINTH, RAFTER_DEPTH,
    Roof, RoofCover, RoofForm, Shutters, WING_OVERHANG, WallFinish, Wing, heap_house,
};
use crate::sim::math::Random;
use crate::sim::timber_layout::TimberHit;

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

/// Shell holes: radius per unit of mark size, the reveal depth of the torn edge
/// and how far the dark interior sits behind the face.
const HOLE_RADIUS: f64 = 0.42;
const HOLE_EDGE: f64 = 0.14;
const HOLE_BACK: f64 = 0.16;
/// How deep the fresh-split face of a hole's torn boards shows before the
/// charred timber behind.
const SPLIT_FACE: f64 = 0.03;
/// What a breach tears back at a time, so its outline steps up its wall course by
/// course with every board (or brick) broken off at its own length: a clapboard
/// course, and two courses of brick.
const CLAPBOARD_COURSE: f64 = 0.2;
const BRICK_STEP: f64 = 0.15;
/// Stud spacing in the frame a breach lays bare.
const STUD_SPACING: f64 = 0.4;
/// Siding ripped off a shelled wall down to the sheathing (clapboard walls only):
/// patches per wall by damage stage, and their size in courses and metres.
const STRIPPED: [usize; 3] = [0, 1, 2];
const STRIPPED_COURSES: (i32, i32) = (2, 4);
const STRIPPED_WIDTH: (f64, f64) = (0.7, 1.4);
/// How far behind the face the bare sheathing lies.
const SHEATHING: f64 = 0.03;
/// Clearance a hole keeps from openings, trim and other holes.
const HOLE_MARGIN: f64 = 0.06;
/// Share of windows a damaged cottage has lost.
const BROKEN_WINDOWS: f64 = 0.5;
/// How far a wrecked roof sags at mid-ridge, and its hole along and across the slope.
const ROOF_SAG: f64 = 0.7;
/// How a shelled cottage's frame gives, by damage stage: its rack (metres of
/// sideways lean per metre up the walls), how far (m) each wall bows out at its
/// middle, and how deep inside a wall's face (m) its bow still carries what is
/// built into it, so a wrecked cottage stands visibly failing before it falls.
const RACK: [f64; 3] = [0.0, 0.02, 0.06];
const BULGE: [f64; 3] = [0.0, 0.06, 0.18];
const BULGE_BAND: f64 = 0.75;
/// Patches of shingles a shelled roof has shed (by stage), their radius (m), and
/// the loose shingles each sheds onto the roof and the ground.
const SHED_PATCHES: [usize; 3] = [0, 3, 6];
/// Patches torn bare on a fallen roof's slabs: the fall strips far more of it
/// than shelling did, so the slabs read as a broken roof rather than one sheet.
const FALLEN_SHED_PATCHES: usize = 12;
const SHED_RADIUS: (f64, f64) = (0.4, 0.75);
const SHED_SHINGLES: usize = 5;
const ROOF_HOLE: (f64, f64) = (1.25, 1.15);

/// Leafy clumps and flowers in each window box.
const PLANT_CLUMPS: u32 = 4;
const PLANT_BLOOMS: u32 = 5;
const LEAVES: u32 = 0x3d6b3a;
const LEAVES_LIGHT: u32 = 0x5a8443;
const FLOWERS: [u32; 4] = [0xd9837a, 0xf0cf6a, 0xb59ad0, 0xf2efe6];
/// The shared satin class: every untextured finish but glass draws as one.
const SATIN: (f64, f64) = (0.0, 0.6);
/// Average colours of the brick and stone textures, for painted pieces.
const BRICK_AVERAGE: u32 = 0x864c3b;
const STONE_AVERAGE: u32 = 0x6b6459;

/// Old framing and decking a collapse lays bare: weathered, dusty timber, darker
/// than the split ends ([`Surface::Splinters`]) that catch the light.
const RAW_TIMBER: u32 = 0x7a5d42;
/// The plaster inside a cottage's walls, which a fallen panel shows: old lime
/// plaster, dulled by smoke and the dust of the fall but lit like any face, so a
/// panel lying lining up reads as the inside of a room, never as a black card.
const PLASTER: u32 = 0x87795f;
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
/// Share of a cottage's side and back windows boarded over after shelling.
const BOARDED_WINDOWS: f64 = 0.14;
/// Thickness of the boards nailed over a shelled window.
const PLANK: f64 = 0.05;
/// Plaster and mortar dust over a collapse's pieces: its colour and how far it
/// covers their paint.
const COLLAPSE_DUST: (u32, f64) = (0xa89a84, 0.24);
/// Patches of plaster fallen off each collapsed wall's lining.
const PLASTER_FALLS: usize = 9;
/// A standing shingle roof's weathering (linear shade at the eave and at the
/// ridge) and the light its ridge caps catch.
const ROOF_WEATHER: (f64, f64) = (0.74, 1.0);
const RIDGE_SHINE: f64 = 1.2;

/// How much of a cottage a build makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Build {
    /// The standing cottage at a damage stage (0 intact, 1 damaged, 2 wrecked).
    Standing(u32),
    /// Each collapse piece apart, as the cottage was when it fell.
    Pieces,
}

impl Build {
    fn stage(self) -> u32 {
        match self {
            Build::Standing(stage) => stage,
            Build::Pieces => 2,
        }
    }
}

/// Where the parts of a cottage go while it is built: the standing model keeps
/// its roof (and chimney) apart from the walls so a wrecked roof can sag; the
/// collapse gives each piece its own kit.
struct Parts {
    split: bool,
    kits: Vec<Kit<Surface>>,
    courses: Vec<Courses>,
    nodes: Vec<Vec<Node>>,
    /// Each wall's openings and shell holes as `[u0, y0, u1, y1]` in its frame, where
    /// a collapse's torn edges leave a gap.
    openings: [Vec<[f64; 4]>; 4],
}

impl Parts {
    fn new(build: Build) -> Self {
        let count = if build == Build::Pieces {
            HousePart::ALL.len()
        } else {
            2
        };
        Self {
            split: build == Build::Pieces,
            kits: (0..count).map(|_| Kit::new()).collect(),
            courses: (0..count).map(|_| Courses::default()).collect(),
            nodes: (0..count).map(|_| Vec::new()).collect(),
            openings: Default::default(),
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

    /// The plinth, steps and splash blocks: the standing cottage only.
    fn base(&mut self) -> Option<&mut Kit<Surface>> {
        (!self.split).then(|| &mut self.kits[0])
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
/// class (its paint baked into vertex colours). Collapse pieces and the heap paint
/// every finish, textured ones in their texture's average colour, so each is one
/// draw.
fn cottage_material(
    surface: Surface,
    palette: &Palette,
    walls: WallFinish,
    pieces: bool,
) -> Arc<Material> {
    let source = surface_material(surface, palette);
    if pieces {
        let paint = match surface {
            Surface::Siding if walls == WallFinish::Brick => {
                multiply_hex(BRICK_AVERAGE, palette.siding)
            }
            Surface::Siding => palette.siding,
            Surface::Brick => BRICK_AVERAGE,
            Surface::Stone => STONE_AVERAGE,
            Surface::Wood => RAW_TIMBER,
            // Collapse pieces line their walls with old wallpaper in the curtains'
            // place, a faded colour of its own in each cottage type.
            Surface::Curtain => mix_hex(
                PLASTER,
                WALLPAPERS[(palette.siding / 13) as usize % WALLPAPERS.len()],
                WALLPAPER_TINT,
            ),
            _ => source.color.0,
        };
        // The lining is already dust-dulled plaster: dusting it again would grey
        // the inside of a room into the paint outside.
        return satin(if surface == Surface::Curtain {
            paint
        } else {
            dusted(paint)
        });
    }
    match surface {
        Surface::Siding if walls == WallFinish::Brick => {
            textured(BRICK_TEXTURE, palette.siding, 0.035, 0.9)
        }
        Surface::Siding | Surface::Brick | Surface::Stone | Surface::Glass | Surface::Lamp => {
            source
        }
        _ => satin(source.color.0),
    }
}

/// Kit texture density per finish: brick walls at the brick's scale.
fn cottage_uv(walls: WallFinish) -> impl Fn(Surface) -> f64 {
    move |surface| match surface {
        Surface::Siding if walls == WallFinish::Brick => uv_per_metre(Surface::Brick),
        _ => uv_per_metre(surface),
    }
}

fn add_cottage(
    group: &mut Node,
    meshes: KitMeshes<Surface>,
    palette: &Palette,
    walls: WallFinish,
    pieces: bool,
) {
    for (surface, mesh) in meshes {
        group.children.push(shadowed(
            mesh,
            cottage_material(surface, palette, walls, pieces),
        ));
    }
}

static GRIMED: Cache<(u32, bool), Material> = Cache::new();
static WEATHERED_SHINGLES: Cache<u32, Material> = Cache::new();

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

/// Add a standing cottage's meshes, its siding weathered ([`grime`]).
fn add_weathered(
    group: &mut Node,
    meshes: KitMeshes<Surface>,
    palette: &Palette,
    walls: WallFinish,
) {
    for (surface, mesh) in meshes {
        let material = if surface == Surface::Siding {
            GRIMED.get_or_insert((palette.siding, walls == WallFinish::Brick), || Material {
                vertex_colors: true,
                ..(*cottage_material(Surface::Siding, palette, walls, false)).clone()
            })
        } else {
            cottage_material(surface, palette, walls, false)
        };
        group.children.push(shadowed(mesh, material));
    }
}

/// A collapse piece's paint under the plaster and mortar dust its fall raised:
/// every finish dulled toward the dust, so the wreck reads as one heavy, dusty
/// ruin rather than freshly painted card.
fn dusted(paint: u32) -> u32 {
    mix_hex(paint, COLLAPSE_DUST.0, COLLAPSE_DUST.1)
}

/// Matte-looking satin for planting, so it joins the cottage's satin draw.
fn satin(color: u32) -> Arc<Material> {
    material(color, SATIN.0, SATIN.1)
}

static OCTAHEDRON: Cache<(), Mesh> = Cache::new();

/// `house(group, c)`: the village cottage for a cover box at a damage stage, its
/// type chosen from its position and marked by the shells that hit it.
#[allow(clippy::too_many_arguments)]
pub(super) fn house(
    group: &mut Node,
    x: f64,
    z: f64,
    w: f64,
    d: f64,
    h: f64,
    stage: u32,
    hits: &[TimberHit],
) {
    let design = HouseDesign::of(x, z, w, d, h);
    let palette = palette(&design, x, z);
    group.children.push(
        cottage(&design, &palette, Build::Standing(stage), hits)
            .remove(0)
            .1,
    );
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
    group.children.push(
        cottage(&design, &palette, Build::Standing(0), &[])
            .remove(0)
            .1,
    );
}

/// The pieces a destroyed cottage falls apart into, each drawn as the part it was
/// (centred on its collider, in the build frame) and batched.
pub fn house_pieces(
    x: f64,
    z: f64,
    w: f64,
    d: f64,
    h: f64,
    hits: &[TimberHit],
) -> Vec<(HousePiece, Node)> {
    let design = HouseDesign::of(x, z, w, d, h);
    let palette = palette(&design, x, z);
    cottage(&design, &palette, Build::Pieces, hits)
}

/// Build a cottage: one turned node for a standing build, or one batched node per
/// collapse piece.
fn cottage(
    design: &HouseDesign,
    palette: &Palette,
    build: Build,
    hits: &[TimberHit],
) -> Vec<(HousePiece, Node)> {
    let roof = design.roof();
    let mut parts = Parts::new(build);
    let mut rng = Random::new(f64::from(design.variant) + 41.0);
    build_house(&mut parts, design, &roof, build, hits, &mut rng);
    let walls = design.style.walls;
    let uv = cottage_uv(walls);
    let shingles = |mesh: Mesh| {
        // A fallen slab keeps its shingles, dulled by the dust of the fall, so it
        // reads as a broken roof rather than a painted board.
        let material = if build == Build::Pieces {
            house_material(HouseSurface::Shingles, dusted(palette.roof))
        } else {
            house_material(HouseSurface::Shingles, palette.roof)
        };
        shadowed(Arc::new(mesh), material)
    };
    let Parts {
        kits,
        courses,
        nodes,
        openings,
        ..
    } = parts;
    match build {
        Build::Standing(stage) => {
            let mut cottage = Node::group("");
            let mut kits = kits.into_iter();
            let mut courses = courses.into_iter();
            let body = kits.next().expect("body kit");
            let roof_kit = kits.next().expect("roof kit");
            let mut body_meshes = body.finish_scaled(&uv);
            buckle(&mut body_meshes, design, &roof, stage);
            grime(&mut body_meshes, roof.ridge);
            add_weathered(&mut cottage, body_meshes, palette, walls);
            let mut roof_meshes = roof_kit.finish_scaled(&uv);
            let mut roof_courses = courses.nth(1).expect("roof courses");
            if stage >= 2 {
                for (_, mesh) in &mut roof_meshes {
                    sag(Arc::make_mut(mesh), &roof);
                }
                roof_courses.sag(&roof);
            }
            buckle(&mut roof_meshes, design, &roof, stage);
            roof_courses.buckle(design, &roof, stage);
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
            for mut node in nodes.into_iter().flatten() {
                node.position = give(design, &roof, stage, node.position);
                cottage.children.push(node);
            }
            cottage.set_rotation_euler(0.0, design.yaw(), 0.0);
            vec![(HousePiece::new(HousePart::FrontWall, 0), cottage)]
        }
        Build::Pieces => {
            let shapes = design.pieces();
            let mut rng = Random::new(f64::from(design.variant) * 13.0 + 5.0);
            let mut pieces = Vec::new();
            for ((kit, courses), part) in kits.into_iter().zip(courses).zip(HousePart::ALL) {
                let kit = if part == HousePart::Mound {
                    rubble_mound(design)
                } else {
                    kit
                };
                let meshes = kit.finish_scaled(&uv);

                let shingled = (!courses.positions.is_empty()).then(|| courses.mesh());
                for shape in shapes.iter().filter(|shape| shape.piece.part == part) {
                    let whole = shape.region.is_whole();
                    let mut shard = Node::group("");
                    // Each half of a shard buckled along its inner crack.
                    for (half, fold) in fold_shard(shape, &roof, &mut rng) {
                        let mut bits = Node::group("");
                        let cut = if whole {
                            meshes.clone()
                        } else {
                            clip_meshes(&meshes, &half)
                        };
                        add_cottage(&mut bits, cut, palette, walls, true);
                        let mut breaks = Kit::new();
                        if let Some(index) = part.wall_index() {
                            wall_breaks(&mut breaks, &half, &openings[index], &mut rng);
                        } else if part.is_roof() && !whole {
                            roof_breaks(&mut breaks, &half, &roof, &mut rng);
                        } else if part == HousePart::Chimney {
                            chimney_breaks(
                                &mut breaks,
                                &half,
                                design.flue(),
                                design.chimney_snap(),
                                &mut rng,
                            );
                        }
                        add_cottage(&mut bits, breaks.finish_scaled(&uv), palette, walls, true);
                        if let Some(mesh) = &shingled {
                            let mesh = if whole {
                                Some(mesh.clone())
                            } else {
                                clip_mesh(mesh, &half.plane, &half.region)
                            };
                            if let Some(mesh) = mesh {
                                bits.children.push(shingles(mesh));
                            }
                        }
                        // A collapse starts from the wrecked cottage's racked, bowed
                        // frame.
                        for child in &mut bits.children {
                            if let Some(drawable) = &mut child.drawable {
                                let mesh = Arc::make_mut(&mut drawable.mesh);
                                buckle_mesh(mesh, design, &roof, 2);
                                mesh.apply_matrix4(&fold);
                            }
                        }
                        shard.children.append(&mut bits.children);
                    }
                    shard.position = -shape.center;
                    let mut root = Node::default();
                    root.children.push(shard);
                    flatten(&mut root);
                    batch(&mut root);
                    let seed = f64::from(design.variant) * 0.37
                        + f64::from(shape.piece.shard) * 1.91
                        + part as usize as f64 * 0.73;
                    for child in &mut root.children {
                        if let Some(drawable) = &mut child.drawable {
                            mottle(Arc::make_mut(&mut drawable.mesh), seed);
                        }
                    }
                    // Only the side slopes' ridge slabs and the chimney's top cast
                    // shadows as they fall: they shade the wreck from overhead, the
                    // smaller pieces' shadows are lost under them, and each caster
                    // is a draw.
                    let casts = match part {
                        HousePart::RightRoof | HousePart::LeftRoof => {
                            shape.piece.shard.is_multiple_of(2)
                        }
                        HousePart::Chimney => shape.piece.shard == 1,
                        _ => false,
                    };
                    if !casts {
                        for child in &mut root.children {
                            if let Some(drawable) = &mut child.drawable {
                                drawable.cast_shadow = false;
                            }
                        }
                    }
                    pieces.push((shape.piece, root));
                }
            }
            pieces
        }
    }
}

/// Carry a group's child offset into its meshes' parent, so batching sees the
/// pieces' parts as direct children.
fn flatten(root: &mut Node) {
    let mut lifted = Vec::new();
    for mut group in std::mem::take(&mut root.children) {
        let offset = group.position;
        for mut child in std::mem::take(&mut group.children) {
            child.position += offset;
            lifted.push(child);
        }
    }
    root.children = lifted;
}

/// Where a point of a shelled cottage (build frame) has moved to as its frame
/// gives at `stage`: racked sideways in proportion to its height over the plinth
/// (the gables lean into parallelograms), and each wall bowed out between its
/// corners, most at mid-height, with whatever is built into it.
fn give(design: &HouseDesign, roof: &Roof, stage: u32, p: DVec3) -> DVec3 {
    let stage = stage.min(2) as usize;
    if stage == 0 {
        return p;
    }
    let lean = if design.variant.is_multiple_of(2) {
        1.0
    } else {
        -1.0
    };
    let mut q = p + DVec3::X * lean * RACK[stage] * (p.y - PLINTH).max(0.0);
    let t = (p.y - PLINTH) / (roof.wall - PLINTH);
    if (0.0..=1.0).contains(&t) {
        let bow = (t * PI).sin() * BULGE[stage];
        let (hw, hd) = (design.w / 2.0, design.d / 2.0);
        if p.x.abs() > hw - BULGE_BAND {
            q.x += p.x.signum() * bow * (PI / 2.0 * (p.z / hd).clamp(-1.0, 1.0)).cos();
        }
        if p.z.abs() > hd - BULGE_BAND {
            q.z += p.z.signum() * bow * (PI / 2.0 * (p.x / hw).clamp(-1.0, 1.0)).cos();
        }
    }
    q
}

fn buckle_mesh(mesh: &mut Mesh, design: &HouseDesign, roof: &Roof, stage: u32) {
    if stage == 0 {
        return;
    }
    for p in &mut mesh.positions {
        *p = narrow(give(design, roof, stage, widen(*p)));
    }
}

/// A shelled cottage's meshes moved as its frame gives ([`give`]).
fn buckle(meshes: &mut KitMeshes<Surface>, design: &HouseDesign, roof: &Roof, stage: u32) {
    for (_, mesh) in meshes.iter_mut() {
        buckle_mesh(Arc::make_mut(mesh), design, roof, stage);
    }
}

/// Patches where a shelled roof has shed its shingles or panels down to the
/// boards, on its side slopes clear of the chimney, dormers and any hole, with
/// loose shingles lying about them and fallen into the yard.
fn shed_shingles(parts: &mut Parts, design: &HouseDesign, roof: &Roof, stage: u32) {
    let count = if parts.split {
        FALLEN_SHED_PATCHES
    } else {
        SHED_PATCHES[stage.min(2) as usize]
    };
    let mut rng = Random::new(f64::from(design.variant) * 11.0 + 401.0);
    let flue = design.flue();
    let dormers = design.dormers();
    let mut placed = 0;
    for _ in 0..count * 6 {
        if placed == count {
            break;
        }
        let side = if rng.next() < 0.5 { 1.0 } else { -1.0 };
        let r = rng.range(SHED_RADIUS.0, SHED_RADIUS.1);
        let x = side * rng.range(0.3, 0.82) * roof.eave_x;
        let z = rng.range(-0.8, 0.8) * roof.rake_z;
        // Wholly on its side slope, below the ridge and above the eave.
        let hip_room = x.abs() - (z.abs() - roof.ridge_half);
        let clear_flue = DVec2::new(x - flue.x, z - flue.z).length() > r + CHIMNEY;
        let clear_dormers = dormers
            .iter()
            .all(|&(s, dz)| s != side || (dz - z).abs() > DORMER_HALF + r + 0.2);
        if x.abs() < r + 0.25
            || x.abs() > roof.eave_x - r - 0.15
            || hip_room < r + 0.2
            || !clear_flue
            || !clear_dormers
        {
            continue;
        }
        placed += 1;
        let normal = DVec3::new(side * roof.pitch, 1.0, 0.0).normalize();
        let along = DVec3::Z;
        let down = normal.cross(along);
        let lift = BUTT_LIFT + 0.014;
        let center = DVec3::new(x, roof.top(x, z) + lift, z);
        let frame = DMat4::from_cols(
            along.extend(0.0),
            down.extend(0.0),
            normal.extend(0.0),
            center.extend(1.0),
        );
        let piece = if side > 0.0 {
            HousePart::RightRoof
        } else {
            HousePart::LeftRoof
        };
        let kit = parts.kit(piece);
        // Weathered boards round torn, tarred felt, and on a wrecked roof the
        // boards broken through to the dark attic.
        // The torn edge follows the courses: wide along them, ragged across.
        let outline = ragged(&mut rng, 2.0 * r * 1.4, 2.0 * r * 0.9);
        kit.face_with_holes(Surface::Wood, &outline, &[], frame);
        // The deck's board seams across the bare patch.
        for k in [-1.0, 0.0, 1.0] {
            let y = k * r * 0.32 + rng.range(-0.04, 0.04);
            kit.block(
                Surface::Dark,
                DVec3::new(r * rng.range(1.2, 1.9), 0.018, 0.004),
                frame * shift(DVec3::new(rng.range(-0.1, 0.1), y, 0.003)),
            );
        }
        let felt = ragged(&mut rng, r * 1.3, r * 0.75);
        kit.face_with_holes(
            Surface::Dark,
            &felt,
            &[],
            frame * shift(DVec3::new(rng.range(-0.15, 0.15), 0.0, 0.005)),
        );
        // Shingles torn half loose round the edge, curled up off the deck,
        // showing their weathered undersides: a fallen roof's patches keep only
        // a couple each, so its slabs do not read as scattered with confetti.
        let curls = if parts.split { 2 } else { SHED_SHINGLES };
        let kit = parts.kit(piece);
        for k in 0..curls {
            let angle = (f64::from(k as u32) + rng.range(0.0, 0.6)) * PI * 2.0 / curls as f64;
            let at = DVec3::new(angle.cos() * r * 1.3, angle.sin() * r * 0.85, 0.03);
            kit.block(
                Surface::Wood,
                DVec3::new(0.3, 0.22, 0.02),
                frame
                    * pose(
                        at,
                        DVec3::new(
                            rng.range(-0.45, 0.45),
                            rng.range(-0.3, 0.3),
                            rng.range(-0.5, 0.5),
                        ),
                    ),
            );
        }
    }
    // Shingles fallen off the eaves into the yard.
    if let Some(base) = parts.base() {
        for _ in 0..count * 3 {
            let side = if rng.next() < 0.5 { 1.0 } else { -1.0 };
            let x = side * (design.w / 2.0 + rng.range(0.25, 1.1));
            let z = rng.range(-0.9, 0.9) * design.d / 2.0;
            base.block(
                Surface::Wood,
                DVec3::new(0.3, 0.02, 0.22),
                pose(
                    DVec3::new(x, 0.03, z),
                    DVec3::new(rng.range(-0.12, 0.12), rng.range(0.0, PI), 0.0),
                ),
            );
        }
    }
}

/// Lower a wrecked roof's vertices: most at mid-ridge, nothing at the gables or
/// the eaves, so it bows between walls that still stand.
fn sag(mesh: &mut Mesh, roof: &Roof) {
    for p in &mut mesh.positions {
        let drop = sag_at(roof, f64::from(p[0]), f64::from(p[2]));
        p[1] -= drop as f32;
    }
}

fn sag_at(roof: &Roof, x: f64, z: f64) -> f64 {
    let along = (z / roof.rake_z).clamp(-1.0, 1.0);
    let across = (1.0 - x.abs() / roof.eave_x).clamp(0.0, 1.0);
    ROOF_SAG * (along * PI / 2.0).cos().powi(2) * across
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
    if role.door || (role.gable && half >= 2.8) {
        vec![-half * 0.54, half * 0.54]
    } else if half >= 2.75 {
        vec![-half * 0.48, half * 0.48]
    } else {
        vec![0.0]
    }
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

/// How far a collapse piece's painted lining sits behind its wall face.
const LINING: f64 = 0.2;

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
    kit.block(
        Surface::Wood,
        DVec3::new(2.0 * half - 0.1, STUD.0, STUD.1),
        frame * shift(DVec3::new(0.0, top - STUD.0, z)),
    );
    // Rails across the studs under the sills and, in a two-storey wall, where the
    // upper floor's joists bore, broken by the openings they meet: a fallen
    // wall's back reads as a frame of timber rather than a board.
    let mut rails = vec![PLINTH + 0.72];
    if top - PLINTH > 3.6 {
        rails.push(PLINTH + (top - PLINTH) * 0.5);
    }
    for y in rails {
        let mut runs = vec![(-half + 0.05, half - 0.05)];
        for o in openings
            .iter()
            .filter(|o| o.y - 0.05 < y && o.top() + 0.05 > y)
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
        let (w, h) = (rng.range(0.45, 0.95), rng.range(0.35, 0.7));
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
        kit.face_with_holes(
            Surface::Wood,
            &outline,
            &[],
            frame * pose(DVec3::new(u, y, -LINING - 0.006), DVec3::new(0.0, PI, 0.0)),
        );
    }
}

/// A polygon seen from behind its plane: mirrored across u, its winding kept.
fn mirrored(polygon: &[DVec2]) -> Vec<DVec2> {
    polygon
        .iter()
        .rev()
        .map(|p| DVec2::new(-p.x, p.y))
        .collect()
}

/// A shell hole through a wall face: centre and radius in the wall frame.
#[derive(Clone, Copy, Debug)]
struct Hole {
    u: f64,
    y: f64,
    radius: f64,
    seed: f64,
    /// The height its outline steps by: a course of the wall it is torn through.
    course: f64,
}

/// A rectangle in a wall frame that holes keep clear of: (u0, y0, u1, y1).
type Keepout = [f64; 4];

fn clear_of(keepouts: &[Keepout], u: f64, y: f64, radius: f64) -> bool {
    let r = radius + HOLE_MARGIN;
    keepouts
        .iter()
        .all(|k| u + r <= k[0] || u - r >= k[2] || y + r <= k[1] || y - r >= k[3])
}

/// The holes shells punched through one wall: each hit is carried into the build
/// frame, given to the wall it struck and slid along it clear of the openings,
/// their trim and the holes before it; a hit with no room leaves no hole.
#[allow(clippy::too_many_arguments)]
fn wall_holes(
    design: &HouseDesign,
    hits: &[TimberHit],
    index: usize,
    half: f64,
    top: f64,
    windows: &[Opening],
    mut keepouts: Vec<Keepout>,
) -> (Vec<Hole>, Vec<usize>) {
    let along = crate::sim::house_layout::wall_frames(design.w, design.d)[index].2;
    let mut holes = Vec::new();
    // Windows a shell blew in where no hole fits beside them.
    let mut blasted = Vec::new();
    for (i, hit) in hits.iter().enumerate() {
        let local = design.to_local(DVec3::new(hit.x, hit.y, hit.z));
        let front = (local.z.abs() / (design.d / 2.0)) >= (local.x.abs() / (design.w / 2.0));
        let wall = match (front, local.z > 0.0, local.x > 0.0) {
            (true, true, _) => 0,
            (true, false, _) => 1,
            (false, _, true) => 2,
            (false, _, false) => 3,
        };
        if wall != index {
            continue;
        }
        let radius = HOLE_RADIUS * hit.size;
        let reach = half - CORNER_BOARD - radius - HOLE_MARGIN;
        let (low, high) = (PLINTH + 0.16 + radius * 0.8, top - 0.12 - radius);
        let struck = DVec2::new(local.dot(along), local.y);
        // With no room for a hole, the shell takes out the window nearest it.
        let blast_window = |blasted: &mut Vec<usize>| {
            let nearest = windows.iter().enumerate().min_by(|(_, a), (_, b)| {
                let at = |o: &Opening| DVec2::new(o.u, o.y + o.height / 2.0).distance(struck);
                at(a).total_cmp(&at(b))
            });
            if let Some((k, _)) = nearest.filter(|(k, _)| !blasted.contains(k)) {
                blasted.push(k);
            }
        };
        if reach <= 0.0 || high <= low {
            blast_window(&mut blasted);
            continue;
        }
        let u0 = struck.x.clamp(-reach, reach);
        let y0 = struck.y.clamp(low, high);
        let spot = (0..25).find_map(|k| {
            let step = f64::from((k + 1) / 2) * 0.16 * if k % 2 == 0 { 1.0 } else { -1.0 };
            [0.0, 0.5, -0.5].into_iter().find_map(|dy| {
                let (u, y) = (u0 + step, (y0 + dy).clamp(low, high));
                (u.abs() <= reach && clear_of(&keepouts, u, y, radius)).then_some((u, y))
            })
        });
        if let Some((u, y)) = spot {
            keepouts.push([u - radius, y - radius, u + radius, y + radius]);
            holes.push(Hole {
                u,
                y,
                radius,
                seed: f64::from(design.variant) * 7.0 + i as f64 * 131.0 + f64::from(index as u32),
                course: wall_course(design),
            });
        } else {
            blast_window(&mut blasted);
        }
    }
    (holes, blasted)
}

/// Soot fanned up the siding round a window a shell blew in, from its casing.
fn window_soot(kit: &mut Kit<Surface>, frame: DMat4, opening: &Opening, seed: f64) {
    let reach = opening.width / 2.0 + CASING;
    let (y0, y1) = (opening.y - 0.08, opening.top() + CASING);
    let center = DVec2::new(opening.u, (y0 + y1) / 2.0);
    let mut rng = Random::new(seed + 17.0);
    // Round the casing, clockwise from its bottom left.
    let mut inner = Vec::new();
    for k in 0..4 {
        let t = f64::from(k) / 4.0;
        inner.push(DVec2::new(opening.u - reach, y0 + (y1 - y0) * t));
    }
    for k in 0..4 {
        let t = f64::from(k) / 4.0;
        inner.push(DVec2::new(opening.u - reach + 2.0 * reach * t, y1));
    }
    for k in 0..4 {
        let t = f64::from(k) / 4.0;
        inner.push(DVec2::new(opening.u + reach, y1 - (y1 - y0) * t));
    }
    let at = |p: DVec2| frame.transform_point3(DVec3::new(p.x, p.y, 0.006));
    let outer: Vec<DVec2> = inner
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let from = (p - center).normalize_or_zero();
            let up = from.y.max(0.0);
            let flare = if i % 2 == 0 {
                rng.range(0.08, 0.16)
            } else {
                rng.range(0.05, 0.1)
            };
            p + from * rng.range(0.04, 0.1) + DVec2::Y * up * flare
        })
        .collect();
    for i in 0..inner.len() - 1 {
        kit.quad(
            Surface::Soot,
            [
                at(inner[i]),
                at(inner[i + 1]),
                at(outer[i + 1]),
                at(outer[i]),
            ],
            frame.transform_vector3(DVec3::Z),
        );
    }
}

/// The height a breach in this cottage's walls steps by ([`CLAPBOARD_COURSE`],
/// [`BRICK_STEP`]).
fn wall_course(design: &HouseDesign) -> f64 {
    match design.style.walls {
        WallFinish::Clapboard => CLAPBOARD_COURSE,
        WallFinish::Brick => BRICK_STEP,
    }
}

/// The courses a breach `radius` round `(u, y)` tears through, bottom up, each
/// as `(y0, y1, left, right)`: whole courses, each broken back to its own length
/// either side, longest across the middle.
fn torn_courses(
    rng: &mut Random,
    u: f64,
    y: f64,
    radius: f64,
    course: f64,
) -> Vec<(f64, f64, f64, f64)> {
    let (mut bottom, mut top) = (
        ((y - radius) / course).ceil() * course,
        ((y + radius) / course).floor() * course,
    );
    if top - bottom < course - 1e-9 {
        (bottom, top) = (y - radius * 0.9, y + radius * 0.9);
    }
    let rows = ((top - bottom) / course).round().max(1.0) as usize;
    let step = (top - bottom) / rows as f64;
    (0..rows)
        .map(|k| {
            let (y0, y1) = (bottom + k as f64 * step, bottom + (k + 1) as f64 * step);
            let mid = ((y0 + y1) / 2.0 - y) / radius;
            let span = (1.0 - mid * mid).max(0.0).sqrt();
            let mut reach = || (radius * span * rng.range(0.55, 1.12)).clamp(0.09, radius * 1.08);
            let (left, right) = (u - reach(), u + reach());
            (y0, y1, left, right)
        })
        .collect()
}

/// A stepped outline round torn courses, clockwise in the wall plane: each board
/// end snapped a little ragged at its middle.
fn course_outline(rng: &mut Random, rows: &[(f64, f64, f64, f64)]) -> Vec<DVec2> {
    let mut outline = Vec::with_capacity(rows.len() * 6);
    for &(y0, y1, _, right) in rows {
        outline.push(DVec2::new(right, y0));
        outline.push(DVec2::new(right + rng.range(-0.05, 0.03), (y0 + y1) / 2.0));
        outline.push(DVec2::new(right, y1));
    }
    for &(y0, y1, left, _) in rows.iter().rev() {
        outline.push(DVec2::new(left, y1));
        outline.push(DVec2::new(left + rng.range(-0.03, 0.05), (y0 + y1) / 2.0));
        outline.push(DVec2::new(left, y0));
    }
    outline.reverse();
    outline
}

/// A hole's ragged outline, clockwise in the wall plane: the siding torn back
/// course by course, every board broken off at its own length.
fn hole_outline(hole: &Hole) -> Vec<DVec2> {
    let mut rng = Random::new(hole.seed);
    let rows = torn_courses(&mut rng, hole.u, hole.y, hole.radius, hole.course);
    course_outline(&mut rng, &rows)
}

/// Line a shell hole: the torn wall's thickness round its edge, splintered board
/// ends sticking into it course by course, the studs of the frame behind (one
/// snapped), the dark room beyond, soot fanned up the siding above it and
/// splintered boards bent out of the face. A standing wall also drops the boards
/// and plaster the shell knocked out on the ground under it.
fn line_hole(kit: &mut Kit<Surface>, frame: DMat4, hole: &Hole, outline: &[DVec2], standing: bool) {
    let at = |p: DVec2, z: f64| frame.transform_point3(DVec3::new(p.x, p.y, z));
    let center = DVec2::new(hole.u, hole.y);
    for i in 0..outline.len() {
        let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
        let edge = b - a;
        // Into the hole: the outline runs clockwise, so the inside lies to the
        // edge's right.
        let inward = DVec2::new(edge.y, -edge.x);
        let inward = if inward.dot(center - (a + b) / 2.0) < 0.0 {
            -inward
        } else {
            inward
        };
        // Fresh-split board ends at the face, charred and shadowed timber behind.
        let facing = frame.transform_vector3(DVec3::new(inward.x, inward.y, 0.0));
        kit.quad(
            Surface::Splinters,
            [
                at(a, 0.0),
                at(b, 0.0),
                at(b, -SPLIT_FACE),
                at(a, -SPLIT_FACE),
            ],
            facing,
        );
        kit.quad(
            Surface::Dark,
            [
                at(a, -SPLIT_FACE),
                at(b, -SPLIT_FACE),
                at(b, -HOLE_EDGE),
                at(a, -HOLE_EDGE),
            ],
            facing,
        );
    }
    // Soot fanned up the siding over the hole in streaks where the blast vented,
    // barely past its lower edge.
    let mut soot = Random::new(hole.seed + 9.0);
    let outer: Vec<DVec2> = outline
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let from = p - center;
            let up = (from.y / from.length().max(1e-6)).max(0.0);
            let flare = if i % 2 == 0 {
                soot.range(0.3, 0.6)
            } else {
                soot.range(0.1, 0.25)
            };
            center + from * (soot.range(1.12, 1.3) + up * flare)
        })
        .collect();
    for i in 0..outline.len() {
        let j = (i + 1) % outline.len();
        kit.quad(
            Surface::Soot,
            [
                at(outline[i], 0.006),
                at(outline[j], 0.006),
                at(outer[j], 0.006),
                at(outer[i], 0.006),
            ],
            frame.transform_vector3(DVec3::Z),
        );
    }
    // The dark room behind, which a fallen panel no longer has: its hole shows
    // whatever it lies on.
    if standing {
        let r = hole.radius * 1.3;
        kit.quad(
            Surface::Dark,
            [
                at(center + DVec2::new(-r, -r), -HOLE_BACK),
                at(center + DVec2::new(r, -r), -HOLE_BACK),
                at(center + DVec2::new(r, r), -HOLE_BACK),
                at(center + DVec2::new(-r, r), -HOLE_BACK),
            ],
            frame.transform_vector3(DVec3::Z),
        );
    }
    let mut rng = Random::new(hole.seed + 5.0);
    // Board ends: each torn course leaves a splinter of board jutting into the
    // hole at one end or the other, bent a little out of the face.
    let mut steps = Random::new(hole.seed + 3.0);
    let rows = torn_courses(&mut steps, hole.u, hole.y, hole.radius, hole.course);
    for (k, &(y0, y1, left, right)) in rows.iter().enumerate() {
        for (end, side) in [(right, -1.0), (left, 1.0)] {
            if (k + usize::from(side > 0.0)) % 2 == 1 || rng.next() < 0.4 {
                continue;
            }
            let length = rng.range(0.08, 0.2);
            let height = (y1 - y0) * rng.range(0.4, 0.75);
            kit.block(
                Surface::Splinters,
                DVec3::new(length, height, 0.022),
                frame
                    * pose(
                        DVec3::new(
                            end + side * length * 0.4,
                            y0 + (y1 - y0) * rng.range(0.35, 0.65),
                            0.0,
                        ),
                        DVec3::new(0.0, side * rng.range(0.15, 0.5), rng.range(-0.2, 0.2)),
                    ),
            );
        }
    }
    // The stud behind the siding that the shell struck, snapped, its raw ends
    // bent apart: only that one shows, as more would fill a hole that has to read
    // dark from the battlefield's height.
    let first = ((hole.u - hole.radius * 0.8) / STUD_SPACING).ceil() as i32;
    let last = ((hole.u + hole.radius * 0.8) / STUD_SPACING).floor() as i32;
    if last >= first {
        let stud = first + (rng.next() * f64::from(last - first + 1)) as i32;
        let u = f64::from(stud) * STUD_SPACING;
        for (end, lean) in [(1.0, rng.range(-0.3, 0.3)), (-1.0, rng.range(-0.3, 0.3))] {
            let reach = hole.radius * rng.range(0.6, 0.95);
            kit.block(
                Surface::Splinters,
                DVec3::new(0.09, reach * 1.6, 0.05),
                frame
                    * pose(
                        DVec3::new(u, hole.y + end * hole.radius * 1.1, -HOLE_EDGE * 0.6),
                        DVec3::new(lean * 0.6, 0.0, lean),
                    ),
            );
        }
    }
    for k in 0..2 {
        let p = outline[(k * 7 + 2) % outline.len()];
        let inward = (center - p).normalize_or_zero();
        let length = rng.range(0.22, 0.42);
        let mid = p + inward * length * 0.25;
        let yaw = rng.range(0.3, 0.6) * if inward.x > 0.0 { -1.0 } else { 1.0 };
        kit.block(
            Surface::Siding,
            DVec3::new(length, 0.07, 0.02),
            frame
                * pose(
                    DVec3::new(mid.x, mid.y, 0.03),
                    DVec3::new(0.0, yaw, rng.range(-0.25, 0.25)),
                ),
        );
    }
    if standing {
        // What the shell knocked out lies on the ground under the hole: broken
        // boards, plaster and lath.
        for k in 0..4 {
            let (coat, size) = match k % 4 {
                0 | 1 => (
                    Surface::Siding,
                    DVec3::new(rng.range(0.3, 0.65), 0.025, hole.course * 0.7),
                ),
                2 => (
                    Surface::Curtain,
                    DVec3::new(rng.range(0.14, 0.24), 0.07, rng.range(0.12, 0.2)),
                ),
                _ => (
                    Surface::Splinters,
                    DVec3::new(rng.range(0.4, 0.7), 0.04, 0.06),
                ),
            };
            kit.block(
                coat,
                size,
                frame
                    * pose(
                        DVec3::new(
                            hole.u + rng.range(-1.0, 1.0) * hole.radius,
                            size.y / 2.0 + 0.01,
                            rng.range(0.22, 0.75),
                        ),
                        DVec3::new(
                            rng.range(-0.12, 0.12),
                            rng.range(0.0, PI),
                            rng.range(-0.12, 0.12),
                        ),
                    ),
            );
        }
    }
}

/// Patches where shelling has ripped a clapboard wall's siding off down to the
/// sheathing (`stage` sets how many): outlines like a hole's, stepped course by
/// course, kept clear of the openings and holes (`keepouts`, which they join).
fn stripped_patches(
    design: &HouseDesign,
    index: usize,
    half: f64,
    top: f64,
    stage: u32,
    keepouts: &mut Vec<Keepout>,
) -> Vec<Vec<DVec2>> {
    let count = STRIPPED[stage.min(2) as usize];
    if count == 0 || design.style.walls != WallFinish::Clapboard {
        return Vec::new();
    }
    let mut rng = Random::new(f64::from(design.variant) * 11.0 + index as f64 * 37.0 + 300.0);
    let mut patches = Vec::new();
    for _ in 0..count * 4 {
        if patches.len() >= count {
            break;
        }
        let courses = rng.range(
            f64::from(STRIPPED_COURSES.0),
            f64::from(STRIPPED_COURSES.1) + 1.0,
        ) as i32;
        let height = f64::from(courses) * CLAPBOARD_COURSE;
        let width = rng.range(STRIPPED_WIDTH.0, STRIPPED_WIDTH.1);
        let reach = half - CORNER_BOARD - width / 2.0 - HOLE_MARGIN;
        let (low, high) = (PLINTH + 0.3, top - 0.25 - height);
        if reach <= 0.0 || high <= low {
            break;
        }
        let u = rng.range(-reach, reach);
        let y0 = (rng.range(low, high) / CLAPBOARD_COURSE).round() * CLAPBOARD_COURSE;
        let box_ = [u - width / 2.0, y0, u + width / 2.0, y0 + height];
        let clear = keepouts.iter().all(|k| {
            box_[2] + HOLE_MARGIN <= k[0]
                || box_[0] - HOLE_MARGIN >= k[2]
                || box_[3] + HOLE_MARGIN <= k[1]
                || box_[1] - HOLE_MARGIN >= k[3]
        });
        if !clear {
            continue;
        }
        keepouts.push(box_);
        // Each course torn back to its own length, the middle ones furthest.
        let rows: Vec<(f64, f64, f64, f64)> = (0..courses)
            .map(|k| {
                let (a, b) = (
                    y0 + f64::from(k) * CLAPBOARD_COURSE,
                    y0 + f64::from(k + 1) * CLAPBOARD_COURSE,
                );
                let t = (f64::from(k) + 0.5) / f64::from(courses) * 2.0 - 1.0;
                let span = width / 2.0 * (1.0 - 0.35 * t * t);
                (
                    a,
                    b,
                    u - span * rng.range(0.6, 1.0),
                    u + span * rng.range(0.6, 1.0),
                )
            })
            .collect();
        patches.push(course_outline(&mut rng, &rows));
    }
    patches
}

/// The sheathing a stripped patch shows: weathered boards behind the torn
/// siding, the siding's split edge round them.
fn line_stripped(kit: &mut Kit<Surface>, frame: DMat4, outline: &[DVec2]) {
    let at = |p: DVec2, z: f64| frame.transform_point3(DVec3::new(p.x, p.y, z));
    let center = outline.iter().copied().sum::<DVec2>() / outline.len() as f64;
    for i in 0..outline.len() {
        let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
        let inward = center - (a + b) / 2.0;
        kit.quad(
            Surface::Splinters,
            [at(a, 0.0), at(b, 0.0), at(b, -SHEATHING), at(a, -SHEATHING)],
            frame.transform_vector3(DVec3::new(inward.x, inward.y, 0.0)),
        );
    }
    kit.face_with_holes(
        Surface::Wood,
        outline,
        &[],
        frame * shift(DVec3::new(0.0, 0.0, -SHEATHING)),
    );
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

/// A window whose glass a shell blew in: the dark room behind a few shards left in
/// the sash, a torn curtain, the sash frame and the casing.
fn broken_window(kit: &mut Kit<Surface>, frame: DMat4, opening: &Opening, seed: f64) {
    let (u, w, y0, y1) = (opening.u, opening.width, opening.y, opening.top());
    let glass_z = -REVEAL + 0.02;
    let at = |u: f64, y: f64, z: f64| frame.transform_point3(DVec3::new(u, y, z));
    let facing = frame.transform_vector3(DVec3::Z);
    kit.quad(
        Surface::Dark,
        [
            at(u - w / 2.0, y0, glass_z - 0.04),
            at(u + w / 2.0, y0, glass_z - 0.04),
            at(u + w / 2.0, y1, glass_z - 0.04),
            at(u - w / 2.0, y1, glass_z - 0.04),
        ],
        facing,
    );
    let mut rng = Random::new(seed);
    // Shards still held in two corners of the sash.
    for (cu, cy) in [(-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
        if rng.next() < 0.3 {
            continue;
        }
        let corner = (
            u + cu * (w / 2.0 - 0.05),
            if cy > 0.0 { y1 - 0.06 } else { y0 + 0.07 },
        );
        let a = rng.range(0.14, 0.3);
        let b = rng.range(0.12, 0.26);
        kit.quad(
            Surface::Glass,
            [
                at(corner.0, corner.1, glass_z),
                at(corner.0 - cu * a, corner.1, glass_z),
                at(corner.0, corner.1 - cy * b, glass_z),
                at(corner.0, corner.1 - cy * b, glass_z),
            ],
            facing,
        );
    }
    // A curtain torn down to one side.
    let side = if rng.next() < 0.5 { -1.0 } else { 1.0 };
    let (outer, inner) = (u + side * w / 2.0, u + side * w * 0.15);
    kit.quad(
        Surface::Curtain,
        [
            at(outer, y0 + 0.2, glass_z - 0.02),
            at(inner, y0 + 0.35, glass_z - 0.02),
            at(inner + side * 0.1, y1, glass_z - 0.02),
            at(outer, y1, glass_z - 0.02),
        ],
        facing,
    );
    const STILE: f64 = 0.055;
    let sash_z = glass_z + 0.025;
    for side in [-1.0, 1.0] {
        kit.block(
            Surface::Trim,
            DVec3::new(STILE, y1 - y0, 0.05),
            frame
                * shift(DVec3::new(
                    u + side * (w - STILE) / 2.0,
                    (y0 + y1) / 2.0,
                    sash_z,
                )),
        );
    }
    for (y, rail) in [(y0 + 0.035, 0.07), (y1 - 0.03, 0.06)] {
        kit.block(
            Surface::Trim,
            DVec3::new(w, rail, 0.05),
            frame * shift(DVec3::new(u, y, sash_z)),
        );
    }
    // The meeting rail hangs from one end.
    let droop = rng.range(0.15, 0.35) * side;
    kit.block(
        Surface::Trim,
        DVec3::new(w * 0.9, 0.05, 0.05),
        frame
            * pose(
                DVec3::new(u, (y0 + y1) / 2.0 - 0.06, sash_z + 0.01),
                DVec3::new(0.0, 0.0, droop),
            ),
    );
    opening_casing(kit, frame, opening, true);
}

/// A window boarded over after earlier shelling: the dark room behind, planks
/// nailed across the casing at odd angles and the casing itself.
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
    // The shelling that broke it: soot licked up the siding round the casing, and
    // clapboards split off beside it.
    let center = DVec2::new(u, (y0 + y1) / 2.0);
    let half = DVec2::new(w / 2.0 + CASING + 0.03, (y1 - y0) / 2.0 + CASING + 0.03);
    let mut soot = Random::new(seed + 11.0);
    let ring: Vec<(DVec2, DVec2)> = (0..14)
        .map(|i| {
            let angle = f64::from(i) / 14.0 * PI * 2.0;
            let ray = DVec2::new(angle.cos(), angle.sin());
            let inner =
                center + ray * (half.x / ray.x.abs().max(1e-6)).min(half.y / ray.y.abs().max(1e-6));
            let reach = soot.range(0.06, 0.24) * if ray.y > 0.3 { 1.6 } else { 1.0 };
            (inner, inner + ray * reach)
        })
        .collect();
    let at = |p: DVec2| frame.transform_point3(DVec3::new(p.x, p.y, 0.006));
    for i in 0..ring.len() {
        let ((a, a2), (b, b2)) = (ring[i], ring[(i + 1) % ring.len()]);
        kit.quad(
            Surface::Dark,
            [at(a), at(b), at(b2), at(a2)],
            frame.transform_vector3(DVec3::Z),
        );
    }
    for side in [-1.0, 1.0] {
        if soot.next() < 0.35 {
            continue;
        }
        let y = y0 + (y1 - y0) * soot.range(0.2, 0.8);
        kit.block(
            Surface::Splinters,
            DVec3::new(soot.range(0.18, 0.34), 0.09, 0.02),
            frame
                * pose(
                    DVec3::new(u + side * (half.x + 0.12), y, 0.035),
                    DVec3::new(0.0, side * soot.range(0.2, 0.5), soot.range(-0.3, 0.3)),
                ),
        );
    }
}

/// A doorway whose door was blown off its hinges: the dark hall, the casing and the
/// door swung out and hanging from its top hinge.
fn broken_door(kit: &mut Kit<Surface>, frame: DMat4, opening: &Opening) {
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
    let hinge = DVec3::new(u - w / 2.0, 0.0, 0.0);
    let leaf = frame
        * pose(hinge, DVec3::new(0.0, -1.15, 0.0))
        * pose(
            DVec3::new(w / 2.0, (y0 + y1) / 2.0 - 0.06, 0.03),
            DVec3::new(0.0, 0.0, -0.09),
        );
    kit.block(Surface::Accent, DVec3::new(w, y1 - y0 - 0.04, 0.05), leaf);
    opening_casing(kit, frame, opening, false);
}

fn build_house(
    parts: &mut Parts,
    design: &HouseDesign,
    roof: &Roof,
    build: Build,
    hits: &[TimberHit],
    rng: &mut Random,
) {
    let (w, d) = (design.w, design.d);
    let stage = build.stage();
    let standing = matches!(build, Build::Standing(_));
    let frames = wall_frames(w, d);
    if let Some(base) = parts.base() {
        // Fieldstone plinth and the two steps up to the door.
        base.chamfer_block(
            Surface::Stone,
            DVec3::new(w + 0.1, PLINTH, d + 0.1),
            0.03,
            DMat4::from_translation(DVec3::new(0.0, PLINTH / 2.0, 0.0)),
        );
        let (door_frame, _) = frames[design.door_wall];
        for (width, depth, top) in [(1.5, 0.62, PLINTH / 2.0), (1.3, 0.32, PLINTH)] {
            base.chamfer_block(
                Surface::Stone,
                DVec3::new(width, PLINTH / 2.0, depth),
                0.02,
                door_frame
                    * DMat4::from_translation(DVec3::new(
                        0.0,
                        top - PLINTH / 4.0,
                        0.05 + depth / 2.0,
                    )),
            );
        }
    }
    let door_opening = Opening {
        u: 0.0,
        y: PLINTH,
        width: DOOR_WIDTH,
        height: DOOR_HEIGHT,
    };
    let door_edge = DOOR_WIDTH / 2.0 + CASING;
    let brick = design.style.walls == WallFinish::Brick;
    for (index, (frame, half)) in frames.into_iter().enumerate() {
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
        // Holes keep clear of every opening with its casing and sill, and of the
        // door canopy; a shutter where a hole lands is blown away.
        let mut keepouts: Vec<Keepout> = openings
            .iter()
            .map(|o| {
                let reach = o.width / 2.0 + CASING + 0.04;
                [o.u - reach, o.y - 0.16, o.u + reach, o.top() + 0.2]
            })
            .collect();
        if role.door {
            keepouts.push([-0.8, PLINTH, 0.8, door_opening.top() + 0.75]);
        }
        if two_storey {
            let belt = belt_line(roof);
            keepouts.push([-half, belt - BELT, half, belt + BELT]);
        }
        let (holes, blasted) = wall_holes(
            design,
            hits,
            index,
            half,
            roof.wall,
            &windows,
            keepouts.clone(),
        );
        keepouts.extend(holes.iter().map(|h| {
            let r = h.radius * 1.1;
            [h.u - r, h.y - r, h.u + r, h.y + r]
        }));
        let stripped = stripped_patches(design, index, half, roof.wall, stage, &mut keepouts);
        let breaches: Vec<Vec<DVec2>> = holes.iter().map(hole_outline).collect();
        parts.openings[index] = openings
            .iter()
            .map(|o| [o.u - o.width / 2.0, o.y, o.u + o.width / 2.0, o.top()])
            .chain(holes.iter().map(|h| {
                [
                    h.u - h.radius,
                    h.y - h.radius,
                    h.u + h.radius,
                    h.y + h.radius,
                ]
            }))
            .collect();
        let kit = parts.kit(piece);
        let cuts: Vec<Vec<DVec2>> = breaches.iter().chain(&stripped).cloned().collect();
        wall_with_breaches(kit, frame, &outline, &openings, REVEAL, &cuts);
        for (hole, outline) in holes.iter().zip(&breaches) {
            line_hole(kit, frame, hole, outline, standing);
        }
        for patch in &stripped {
            line_stripped(kit, frame, patch);
        }
        if !standing {
            // A falling wall shows its back: the painted lining and the cut edges.
            let mut holes: Vec<Vec<DVec2>> = openings.iter().map(Opening::corners).collect();
            holes.extend(breaches.iter().cloned());
            let holes: Vec<Vec<DVec2>> = holes.iter().map(|hole| mirrored(hole)).collect();
            kit.face_with_holes(
                Surface::Curtain,
                &mirrored(&outline),
                &holes,
                frame * pose(DVec3::new(0.0, 0.0, -LINING), DVec3::new(0.0, PI, 0.0)),
            );
            framing(kit, frame, &outline, &openings, roof.wall);
            plaster_falls(kit, frame, half, roof.wall, &openings, rng);
            let at = |p: DVec2, depth: f64| frame.transform_point3(DVec3::new(p.x, p.y, -depth));
            let center = outline.iter().copied().sum::<DVec2>() / outline.len() as f64;
            for i in 0..outline.len() {
                let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
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
            let blown = blasted.contains(&i);
            let upper = i >= ground_floor;
            let broken =
                blown || stage >= 2 || (stage == 1 && Random::new(seed).next() < BROKEN_WINDOWS);
            let boarded = !role.door && Random::new(seed + 0.7).next() < BOARDED_WINDOWS;
            if blown {
                window_soot(kit, frame, opening, seed);
            }
            if broken {
                broken_window(kit, frame, opening, seed);
            } else if boarded {
                boarded_window(kit, frame, opening, seed);
            } else if upper {
                // Bedroom casements upstairs: one sash of four lites.
                window(kit, frame, opening, Glazing { double_hung: false }, REVEAL);
            } else {
                window(kit, frame, opening, glazing, REVEAL);
            }
            let inner_limit = if role.door { door_edge } else { 0.0 };
            let room = shutter_room(opening, half, inner_limit, ground_floor == 1);
            // A wrecked cottage has lost some of its shutters, and a hole beside a
            // window takes its shutters with it.
            let reach = opening.width / 2.0 + CASING + 0.01 + room;
            let holed = holes.iter().any(|hole| {
                (hole.u - opening.u).abs() < reach + hole.radius
                    && hole.y + hole.radius > opening.y
                    && hole.y - hole.radius < opening.top()
            });
            let lost = holed || blown || (stage >= 2 && Random::new(seed + 0.5).next() < 0.5);
            // Shutters dress the street front and the sides; the back of the
            // cottage, behind its door, makes do without.
            let back = role.planted && !role.door;
            // The upper floor's windows go without: shutters on both floors
            // cost more triangles than the second row of windows itself.
            if room > 0.0 && !lost && !back && !upper {
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
            if stage >= 2 {
                broken_door(kit, frame, &door_opening);
            } else {
                door(kit, frame, &door_opening, REVEAL);
            }
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
        if standing && role.planted {
            window_plants(&mut parts.nodes[0], frame, half, role, roof, design.variant);
        }
        if standing && role.door {
            foundation_bed(parts, frame, half, design.variant);
        }
    }
    match roof.form {
        RoofForm::Gable => gable_deck(parts, roof),
        RoofForm::Hip => hip_deck(parts, roof),
    }
    if !standing {
        rafters(parts, design, roof);
    }
    let wrecked = stage >= 2 && standing;
    let hole = wrecked.then(|| roof_hole(roof, rng));
    for slope in slopes(roof) {
        if !standing && roof.form == RoofForm::Hip {
            slope_underside(parts.kit(slope.piece), &slope, roof.thick);
        }
        match design.style.cover {
            RoofCover::Shingles => {
                shingle_slope(parts.courses(slope.piece), &slope, hole.as_ref());
                if let Some(hole) = hole.as_ref().filter(|hole| hole.piece == slope.piece) {
                    attic(parts.kit(slope.piece), &slope, hole, roof);
                }
            }
            RoofCover::Seams => {
                seam_slope(parts.kit(slope.piece), &slope, hole.as_ref());
                if let Some(hole) = hole.as_ref().filter(|hole| hole.piece == slope.piece) {
                    attic(parts.kit(slope.piece), &slope, hole, roof);
                }
            }
        }
    }
    ridge_caps(parts, design, roof);
    gutters(parts, w, d, roof);
    if stage >= 1 {
        shed_shingles(parts, design, roof, stage);
    }

    if let Some(base) = parts.base() {
        yard(base, design);
    }
    chimney(parts.kit(HousePart::Chimney), design, roof, wrecked);
    // Some cottages light their attics with dormers, so the roofscape varies.
    if roof.form == RoofForm::Gable {
        let flue = design.flue();
        if let Some(wing) = design.wing() {
            cross_gable(parts, design, roof, &wing, stage >= 2);
        }
        for (side, z) in design.dormers() {
            let clear = flue.x * side < 0.0 || (flue.z - z).abs() > DORMER_HALF + CHIMNEY;
            let in_hole = hole
                .as_ref()
                .is_some_and(|hole| hole.side == side && (hole.across - z).abs() < 1.6);
            if clear && !in_hole {
                let piece = if side > 0.0 {
                    HousePart::RightRoof
                } else {
                    HousePart::LeftRoof
                };
                dormer(parts.kit(piece), roof, side, z, stage >= 2);
            }
        }
    }
}

/// A side-gable cottage's cross gable over its door: the door wall rises past the
/// eave into a clapboard gable with an attic window under bargeboards, its roof
/// running back from the face into the main slope (shingled or seamed like it),
/// so the roof reads as a T from above. It sits on the right (+x) slope and
/// falls with that slope's slab.
fn cross_gable(parts: &mut Parts, design: &HouseDesign, roof: &Roof, wing: &Wing, broken: bool) {
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
    wall_with_breaches(kit, frame, &outline, &[opening], REVEAL, &[]);
    if broken {
        broken_window(kit, frame, &opening, z * 13.0 + 7.0);
    } else {
        window(kit, frame, &opening, Glazing { double_hung: false }, REVEAL);
    }
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
            RoofCover::Shingles => shingle_slope(parts.courses(piece), &wing_slope, None),
            RoofCover::Seams => seam_slope(parts.kit(piece), &wing_slope, None),
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
fn dormer(kit: &mut Kit<Surface>, roof: &Roof, side: f64, z: f64, broken: bool) {
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
    wall_with_breaches(kit, frame, &outline, &[opening], 0.08, &[]);
    if broken {
        broken_window(kit, frame, &opening, z * 17.0 + side);
    } else {
        window(kit, frame, &opening, Glazing { double_hung: true }, 0.08);
    }
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
        if let Some(base) = parts.base() {
            base.block(
                Surface::Wood,
                DVec3::new(to - from, 0.05, 0.62),
                frame * shift(DVec3::new(side * (from + to) / 2.0, 0.0, 0.36)),
            );
        }
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
    parts.nodes[0].extend(group.children);
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
    let reach = roof.ridge_half.min(roof.rake_z) - 0.2;
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
        let deck = if parts.split {
            Surface::Wood
        } else {
            Surface::Trim
        };
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

/// A hole torn through a wrecked roof: its slope, centre (along the slope and
/// across it) and half extents, ragged per course.
#[derive(Clone, Copy, Debug)]
struct RoofHole {
    piece: HousePart,
    side: f64,
    along: f64,
    across: f64,
    half_along: f64,
    half_across: f64,
    seed: f64,
}

fn roof_hole(roof: &Roof, rng: &mut Random) -> RoofHole {
    let side = if rng.next() < 0.5 { 1.0 } else { -1.0 };
    let length = ((roof.eave_x).powi(2) + (roof.ridge - roof.eave_y).powi(2)).sqrt();
    let room = (roof.ridge_half.min(roof.rake_z) - ROOF_HOLE.1).max(0.0);
    RoofHole {
        piece: if side > 0.0 {
            HousePart::RightRoof
        } else {
            HousePart::LeftRoof
        },
        side,
        along: length * rng.range(0.38, 0.55),
        across: rng.range(-room, room) * 0.6,
        half_along: ROOF_HOLE.0 / 2.0,
        half_across: ROOF_HOLE.1 / 2.0,
        seed: rng.next() * 1000.0,
    }
}

impl RoofHole {
    /// The hole's ragged extent across the slope at distance `s` up it.
    fn span(&self, s: f64, course: u32) -> Option<(f64, f64)> {
        let t = (s - self.along) / self.half_along;
        if t.abs() >= 1.0 {
            return None;
        }
        let mut rng = Random::new(self.seed + f64::from(course) * 13.0);
        let width = self.half_across * (1.0 - t * t).sqrt().max(0.35);
        Some((
            self.across - width * rng.range(0.7, 1.15),
            self.across + width * rng.range(0.7, 1.15),
        ))
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

    fn sag(&mut self, roof: &Roof) {
        for p in self.positions.as_chunks_mut::<3>().0 {
            p[1] -= sag_at(roof, p[0], p[2]);
        }
    }

    fn buckle(&mut self, design: &HouseDesign, roof: &Roof, stage: u32) {
        for p in self.positions.as_chunks_mut::<3>().0 {
            *p = give(design, roof, stage, DVec3::from_array(*p)).to_array();
        }
    }

    fn mesh(&self) -> Mesh {
        Mesh::from_f64(&self.positions, &self.normals, &self.uvs, None)
    }
}

/// One slope's shingle courses from eave to ridge. Each course's texture row
/// matches its geometry: the butt edge sits on the top of a row of `shingles.webp`.
/// A hole leaves each course it crosses in two ragged parts.
fn shingle_slope(out: &mut Courses, slope: &Slope, hole: Option<&RoofHole>) {
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
        let gap = hole
            .filter(|hole| hole.piece == slope.piece)
            .and_then(|hole| hole.span((s0 + s1) / 2.0 - SHINGLE_EAVE, k));
        let runs = match gap {
            Some((from, to)) => vec![(-1.0, from), (to, 1.0)],
            None => vec![(-1.0, 1.0)],
        };
        for (from, to) in runs {
            // Run ends as fractions of the course's own width, or a hole's edge.
            let ends = |h: f64| {
                let a = if from == -1.0 { -h } else { from.max(-h) };
                let b = if to == 1.0 { h } else { to.min(h) };
                (a, b)
            };
            let ((a0, b0), (a1, b1)) = (ends(h0), ends(h1));
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
}

/// One slope of painted standing-seam steel: the sheet (cut by a hole) and ribs
/// from the eave to the ridge or the hip.
fn seam_slope(kit: &mut Kit<Surface>, slope: &Slope, hole: Option<&RoofHole>) {
    const LIFT: f64 = 0.004;
    let hole = hole.filter(|hole| hole.piece == slope.piece);
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
    let holes: Vec<Vec<DVec2>> = hole
        .map(|hole| {
            let mut rng = Random::new(hole.seed);
            (0..10)
                .map(|i| {
                    let angle = -f64::from(i) / 10.0 * PI * 2.0;
                    let r = rng.range(0.7, 1.1);
                    DVec2::new(
                        hole.across + angle.cos() * hole.half_across * r,
                        hole.along + angle.sin() * hole.half_along * r,
                    )
                })
                .collect()
        })
        .into_iter()
        .collect();
    kit.face_with_holes(Surface::Roof, &outline, &holes, frame);
    let count = (slope.half_eave * 2.0 / SEAM_SPACING).floor() as i32;
    for i in -count / 2..=count / 2 {
        let a = f64::from(i) * SEAM_SPACING;
        // A rib ends where the slope narrows past it (at a hip).
        let reach = if slope.half_ridge >= a.abs() {
            slope.length
        } else {
            slope.length * (slope.half_eave - a.abs()) / (slope.half_eave - slope.half_ridge)
        };
        let mut runs = vec![(0.0, reach - 0.05)];
        if let Some(hole) = hole
            && (a - hole.across).abs() < hole.half_across
        {
            runs = vec![
                (0.0, hole.along - hole.half_along),
                (hole.along + hole.half_along, reach - 0.05),
            ];
        }
        for (from, to) in runs {
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
}

/// The dark attic under a roof hole and two rafters across it, one snapped.
fn attic(kit: &mut Kit<Surface>, slope: &Slope, hole: &RoofHole, roof: &Roof) {
    let depth = -roof.thick * 0.6;
    let (a, s) = (hole.across, hole.along);
    let (ha, hs) = (hole.half_across * 1.25, hole.half_along * 1.2);
    kit.quad(
        Surface::Dark,
        [
            slope.at(s - hs, a - ha, depth),
            slope.at(s - hs, a + ha, depth),
            slope.at(s + hs, a + ha, depth),
            slope.at(s + hs, a - ha, depth),
        ],
        slope.normal,
    );
    for (k, offset) in [-0.3, 0.25].into_iter().enumerate() {
        let (from, to) = if k == 0 {
            (s - hs * 1.1, s + hs * 1.1)
        } else {
            (s + hs * 0.1, s + hs * 1.1)
        };
        let center = slope.at((from + to) / 2.0, a + offset, depth + 0.06);
        let droop = if k == 1 { -0.35 } else { 0.0 };
        let basis = DMat4::from_cols(
            slope.across.extend(0.0),
            slope.normal.extend(0.0),
            slope.up.extend(0.0),
            center.extend(1.0),
        ) * pose(DVec3::ZERO, DVec3::new(droop, 0.0, 0.0));
        kit.block(Surface::Wood, DVec3::new(0.09, 0.14, to - from), basis);
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
        parts.kit(HousePart::RightRoof).block(
            Surface::Metal,
            DVec3::new(0.26, 0.07, 2.0 * roof.ridge_half + 0.04),
            shift(DVec3::new(0.0, crest + 0.03, 0.0)),
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
        kit.solid(Surface::Metal, &[profile(-zr - 0.02), profile(zr + 0.02)]);
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
        if let Some(base) = parts.base() {
            base.chamfer_block(
                Surface::Concrete,
                DVec3::new(0.48, 0.05, 0.26),
                0.015,
                DMat4::from_translation(DVec3::new(side * (w / 2.0 + 0.36), 0.025, bend.z)),
            );
        }
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
                kit.block(
                    Surface::Wood,
                    DVec3::new(0.44 + rng.range(-0.04, 0.06), LOG, LOG),
                    pose(
                        DVec3::new(x, y, z),
                        DVec3::new(rng.range(-0.2, 0.2), 0.0, 0.0),
                    ),
                );
            }
        }
        // Two posts and a plank roof shedding rain off the stack.
        let top = 0.04 + STACK.1 as f64 * LOG + 0.12;
        for end in [-1.0, 1.0] {
            kit.block(
                Surface::Trim,
                DVec3::new(0.07, top, 0.07),
                shift(DVec3::new(
                    x - 0.2,
                    top / 2.0,
                    z0 + length / 2.0 + end * (length / 2.0 + 0.05),
                )),
            );
        }
        kit.block(
            Surface::Roof,
            DVec3::new(0.7, 0.04, length + 0.3),
            pose(
                DVec3::new(x - 0.02, top + 0.06, z0 + length / 2.0),
                DVec3::new(0.0, 0.0, 0.28),
            ),
        );
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
            kit.block(
                Surface::Stone,
                DVec3::new(length, DRIP_STRIP, width),
                frame
                    * pose(
                        DVec3::new(u + length / 2.0, DRIP_STRIP / 2.0, 0.05 + width / 2.0),
                        DVec3::new(0.0, rng.range(-0.04, 0.04), 0.0),
                    ),
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

/// One flagstone of a door path, `out` metres from the wall, in the door wall's
/// frame.
#[allow(clippy::too_many_arguments)]
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
/// it. A wrecked cottage's chimney has toppled to a ragged stub, its bricks
/// strewn down the slope.
fn chimney(kit: &mut Kit<Surface>, design: &HouseDesign, roof: &Roof, toppled: bool) {
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
    if toppled {
        let mut rng = Random::new(f64::from(design.variant) + 77.0);
        let stub = roof.top(cx, cz) + 0.3;
        kit.block(
            Surface::Brick,
            DVec3::new(CHIMNEY, stub - bottom, CHIMNEY),
            DMat4::from_translation(DVec3::new(cx, (stub + bottom) / 2.0, cz)),
        );
        // Ragged courses still standing on the stub.
        for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0)] {
            let height = rng.range(0.08, 0.3);
            kit.block(
                Surface::Brick,
                DVec3::new(CHIMNEY / 2.0, height, CHIMNEY / 2.0),
                shift(DVec3::new(
                    cx + sx * CHIMNEY / 4.0,
                    stub + height / 2.0,
                    cz + sz * CHIMNEY / 4.0,
                )),
            );
        }
        // Loose bricks down the slope below it.
        let down = DVec3::new(cx.signum(), 0.0, 0.0);
        for _ in 0..6 {
            let reach = rng.range(0.5, 1.6);
            let x = cx + down.x * reach + rng.range(-0.2, 0.2);
            let z = cz + rng.range(-0.6, 0.6);
            kit.block(
                Surface::Brick,
                DVec3::new(0.22, 0.07, 0.11),
                pose(
                    DVec3::new(x, roof.top(x, z) + 0.06, z),
                    DVec3::new(0.0, rng.range(0.0, PI), roof.pitch.atan() * -x.signum()),
                ),
            );
        }
        return;
    }
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

/// What a collapsed cottage leaves at a heap cover: the stub of its back wall
/// with the plinth under it, broken siding, roof boards, shingles and bricks piled
/// against it, and the burnt floor and foundation kerb of the rest of the
/// footprint, open for driving. `seed` is the heap's debris seed, which also
/// carries the cottage it fell from.
pub(super) fn house_heap(group: &mut Node, x: f64, z: f64, w: f64, d: f64, seed: f64) {
    let (center, design) = heap_house(x, z, w, d, seed, 4.8);
    let palette = palette(&design, center.x, center.y);
    let walls = design.style.walls;
    let (_, _, scatter) = crate::sim::house_layout::heap_layout(seed);
    let mut rng = Random::new(scatter + 911.0);
    let (bw, bd) = (design.w, design.d);
    let back = -bd / 2.0;
    let mut kit: Kit<Surface> = Kit::new();
    // Foundation kerb round the footprint and the plinth under the heap.
    for (size, at) in [
        (
            DVec3::new(bw, 0.14, 0.32),
            DVec3::new(0.0, 0.07, bd / 2.0 - 0.16),
        ),
        (
            DVec3::new(0.32, 0.14, bd),
            DVec3::new(bw / 2.0 - 0.16, 0.07, 0.0),
        ),
        (
            DVec3::new(0.32, 0.14, bd),
            DVec3::new(-bw / 2.0 + 0.16, 0.07, 0.0),
        ),
    ] {
        kit.chamfer_block(Surface::Stone, size, 0.03, shift(at));
    }
    let heap_depth = HOUSE_HEAP.depth + HOUSE_HEAP.inset;
    kit.chamfer_block(
        Surface::Stone,
        DVec3::new(bw + 0.1, PLINTH, heap_depth),
        0.03,
        shift(DVec3::new(0.0, PLINTH / 2.0, back + heap_depth / 2.0)),
    );
    // Charred floorboards across the open footprint, with ash where it burnt.
    let floor_from = back + heap_depth;
    let floor_to = bd / 2.0 - 0.32;
    let boards = ((bw - 0.64) / 0.3).floor() as i32;
    for i in 0..boards {
        let u = -bw / 2.0 + 0.32 + 0.15 + f64::from(i) * 0.3;
        let missing = rng.next() < 0.18;
        if missing {
            continue;
        }
        let (from, to) = (
            floor_from + rng.range(0.0, 0.5),
            floor_to - rng.range(0.0, 0.7),
        );
        kit.block(
            Surface::Wood,
            DVec3::new(0.27, 0.04, to - from),
            shift(DVec3::new(u, 0.02, (from + to) / 2.0)),
        );
    }
    for _ in 0..3 {
        let (u, v) = (
            rng.range(-bw / 2.0 + 0.8, bw / 2.0 - 0.8),
            rng.range(floor_from + 0.6, floor_to - 0.6),
        );
        let r = rng.range(0.35, 0.7);
        kit.quad(
            Surface::Dark,
            [
                DVec3::new(u - r, 0.045, v - r * 0.8),
                DVec3::new(u + r, 0.045, v - r),
                DVec3::new(u + r * 0.8, 0.045, v + r),
                DVec3::new(u - r, 0.045, v + r * 0.7),
            ],
            DVec3::Y,
        );
    }
    // The back wall's ragged stub, standing on its plinth where its shards broke off.
    let half = bw / 2.0;
    let stub = design.back_stub();
    let steps = stub.len() - 1;
    let mut outline = vec![DVec2::new(half, PLINTH), DVec2::new(-half, PLINTH)];
    outline.extend(&stub);
    outline.reverse();
    let wall_frame = pose(DVec3::new(0.0, 0.0, back + 0.02), DVec3::new(0.0, PI, 0.0));
    kit.face_with_holes(Surface::Siding, &outline, &[], wall_frame);
    let inner = pose(DVec3::new(0.0, 0.0, back + 0.2), DVec3::ZERO);
    let mirrored: Vec<DVec2> = outline
        .iter()
        .rev()
        .map(|p| DVec2::new(-p.x, p.y))
        .collect();
    kit.face_with_holes(Surface::Wood, &mirrored, &[], inner);
    // Its broken top, between the two faces.
    let tops: Vec<DVec2> = outline[..=steps].to_vec();
    for pair in tops.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let at = |p: DVec2, z: f64| DVec3::new(-p.x, p.y, z);
        kit.quad(
            Surface::Wood,
            [
                at(a, back + 0.02),
                at(b, back + 0.02),
                at(b, back + 0.2),
                at(a, back + 0.2),
            ],
            DVec3::Y,
        );
    }
    corner_remnants(&mut kit, &design, &mut rng, heap_depth);
    // The heap: a mound of broken masonry under siding, boards, roof and bricks.
    let (lo_x, hi_x) = (-bw / 2.0 + 0.35, bw / 2.0 - 0.35);
    let (lo_z, hi_z) = (back + 0.35, back + heap_depth - 0.15);
    let mound = |rng: &mut Random| DVec3::new(rng.range(lo_x, hi_x), 0.0, rng.range(lo_z, hi_z));
    for _ in 0..4 {
        let at = mound(&mut rng);
        let size = DVec3::new(
            rng.range(0.8, 1.4),
            rng.range(0.45, 0.75),
            rng.range(0.6, 1.0),
        );
        kit.chamfer_block(
            Surface::Stone,
            size,
            0.12,
            pose(
                at + DVec3::Y * size.y / 2.0,
                DVec3::new(
                    rng.range(-0.15, 0.15),
                    rng.range(0.0, PI),
                    rng.range(-0.15, 0.15),
                ),
            ),
        );
    }
    // Torn pieces of wall and roof with their thickness: siding over plaster with
    // split timber round the break, shingled deck over rafters, and lumps of
    // plaster and lath, heaped at every angle.
    for i in 0..14 {
        let at = mound(&mut rng);
        let y = rng.range(0.35, HOUSE_HEAP.height - 0.15);
        let placement = pose(
            at + DVec3::Y * y,
            DVec3::new(
                -PI / 2.0 + rng.range(-0.75, 0.75),
                rng.range(0.0, PI),
                rng.range(-0.6, 0.6),
            ),
        );
        let (w, d) = (rng.range(0.7, 1.4), rng.range(0.45, 0.8));
        match i % 4 {
            0 | 1 => {
                let outline = jagged(&mut rng, w, d);
                slab(
                    &mut kit,
                    [Surface::Siding, Surface::Curtain, Surface::Splinters],
                    &outline,
                    WALL_SLAB,
                    placement,
                );
            }
            2 => {
                let outline = jagged(&mut rng, w * 0.75, d);
                slab(
                    &mut kit,
                    [Surface::Roof, Surface::Wood, Surface::Splinters],
                    &outline,
                    ROOF_SLAB,
                    placement,
                );
            }
            _ => {
                let outline = jagged(&mut rng, w * 0.45, d * 0.6);
                slab(
                    &mut kit,
                    [Surface::Curtain, Surface::Curtain, Surface::Splinters],
                    &outline,
                    0.2,
                    placement,
                );
            }
        }
    }
    for _ in 0..10 {
        let at = mound(&mut rng);
        kit.block(
            Surface::Brick,
            DVec3::new(0.22, 0.07, 0.11),
            pose(
                at + DVec3::Y * rng.range(0.3, 0.8),
                DVec3::new(
                    rng.range(-0.5, 0.5),
                    rng.range(0.0, PI),
                    rng.range(-0.5, 0.5),
                ),
            ),
        );
    }
    // A few boards and a window frame thrown across the floor.
    for _ in 0..3 {
        let (u, v) = (
            rng.range(-bw / 2.0 + 0.6, bw / 2.0 - 0.6),
            rng.range(floor_from + 0.4, floor_to - 0.4),
        );
        kit.block(
            Surface::Trim,
            DVec3::new(rng.range(0.8, 1.6), 0.05, 0.1),
            pose(
                DVec3::new(u, 0.07, v),
                DVec3::new(0.0, rng.range(0.0, PI), 0.0),
            ),
        );
    }
    heap_remains(&mut kit, &design, &mut rng, back + heap_depth, floor_to);
    // The household's things in the heap: two broken in its rubble, the door
    // flat on the floor in front.
    let mut things = Random::new(scatter + 47.0);
    let first = things.range(0.0, 6.0) as usize;
    for k in 0..2 {
        let u = things.range(lo_x + 0.5, hi_x - 0.5);
        let v = things.range(lo_z + 0.3, hi_z - 0.3);
        let kind = [0, 2, 3, 5][(first + k * 2) % 4];
        furniture(
            &mut kit,
            kind,
            pose(
                DVec3::new(u, HOUSE_HEAP.height * 0.35, v),
                DVec3::new(
                    things.range(-0.45, 0.45),
                    things.range(0.0, PI),
                    things.range(-0.45, 0.45),
                ),
            ),
        );
    }
    furniture(
        &mut kit,
        4,
        pose(
            DVec3::new(
                things.range(-bw / 2.0 + 1.0, bw / 2.0 - 1.0),
                0.04,
                things.range(floor_from + 1.1, (floor_to - 1.1).max(floor_from + 1.1)),
            ),
            DVec3::new(0.0, things.range(0.0, PI), 0.0),
        ),
    );
    let mut node = Node::group("");
    add_cottage(
        &mut node,
        kit.finish_scaled(cottage_uv(walls)),
        &palette,
        walls,
        true,
    );
    batch(&mut node);
    for child in &mut node.children {
        if let Some(drawable) = &mut child.drawable {
            mottle(Arc::make_mut(&mut drawable.mesh), seed);
        }
    }
    // The cottage's frame about the heap cover's centre.
    let offset = DVec3::new(center.x - x, 0.0, center.y - z);
    node.set_rotation_euler(0.0, design.yaw(), 0.0);
    node.position = offset;
    group.children.push(node);
}

/// What stays of the roof and chimney on a heap once the collapse's shards have
/// sunk into it: broken slabs of roof with their rafters lying across the mound and
/// the floor, the chimney's broken stack and bricks where it fell, splintered
/// framing poking out, and shingles and clapboards strewn round the footprint.
fn heap_remains(
    kit: &mut Kit<Surface>,
    design: &HouseDesign,
    rng: &mut Random,
    floor_from: f64,
    floor_to: f64,
) {
    let (bw, bd) = (design.w, design.d);
    let back = -bd / 2.0;
    let tilt = |rng: &mut Random, lean: f64| {
        DVec3::new(
            rng.range(-lean, lean),
            rng.range(0.0, PI),
            rng.range(-lean, lean),
        )
    };
    // Roof slabs: shingled tops over their decking and a rafter or two, one leaning
    // off the mound, the others flat over the floor.
    for k in 0..4 {
        let on_mound = k < 2;
        let (u, v) = (
            rng.range(-bw / 2.0 + 1.0, bw / 2.0 - 1.0),
            if on_mound {
                back + HOUSE_HEAP.depth * rng.range(0.35, 0.8)
            } else {
                rng.range(floor_from + 0.5, floor_to - 0.6)
            },
        );
        let size = DVec3::new(rng.range(1.2, 1.9), ROOF_SLAB, rng.range(0.8, 1.3));
        let (y, lean) = if on_mound {
            (HOUSE_HEAP.height - 0.25, 0.45)
        } else {
            (0.16, 0.12)
        };
        let placement = pose(DVec3::new(u, y, v), tilt(rng, lean));
        let outline = jagged(rng, size.x, size.z);
        slab(
            kit,
            [Surface::Roof, Surface::Wood, Surface::Splinters],
            &outline,
            ROOF_SLAB,
            placement * pose(DVec3::ZERO, DVec3::new(-PI / 2.0, 0.0, 0.0)),
        );
        for side in [-0.3, 0.3] {
            kit.block(
                Surface::Wood,
                DVec3::new(0.06, 0.12, size.z + rng.range(0.1, 0.5)),
                placement * shift(DVec3::new(side * size.x, -0.15, 0.0)),
            );
        }
    }
    // Wall panels flattened over the floor, siding up or plaster up, low enough to
    // drive over, one propped against the mound.
    for k in 0..4 {
        let (u, v) = (
            rng.range(-bw / 2.0 + 0.9, bw / 2.0 - 0.9),
            rng.range(floor_from - 0.3, floor_to - 0.5),
        );
        let size = DVec3::new(rng.range(1.2, 2.0), WALL_SLAB, rng.range(0.8, 1.4));
        let propped = k == 0;
        let (y, lean) = if propped { (0.5, 0.7) } else { (0.12, 0.14) };
        let placement = pose(DVec3::new(u, y, v), tilt(rng, lean));
        let faces = if rng.next() < 0.7 {
            [Surface::Siding, Surface::Curtain, Surface::Splinters]
        } else {
            [Surface::Curtain, Surface::Siding, Surface::Splinters]
        };
        let outline = jagged(rng, size.x, size.z);
        slab(
            kit,
            faces,
            &outline,
            WALL_SLAB,
            placement * pose(DVec3::ZERO, DVec3::new(-PI / 2.0, 0.0, 0.0)),
        );
        kit.block(
            Surface::Wood,
            DVec3::new(0.06, 0.08, size.z + 0.2),
            placement * shift(DVec3::new(rng.range(-0.4, 0.4) * size.x, -0.08, 0.0)),
        );
    }
    // The chimney's broken stack where it fell, toward the flue's side, and bricks.
    let flue = design.flue();
    let fall = DVec2::new(flue.x, flue.z).normalize_or(DVec2::X);
    let reach = DVec2::new(bw / 2.0 - 0.6, bd / 2.0 - 0.6);
    let at = (fall * 1.6).clamp(-reach, reach);
    for (k, length) in [rng.range(0.6, 0.9), rng.range(0.35, 0.55)]
        .into_iter()
        .enumerate()
    {
        let offset = DVec2::new(rng.range(-0.4, 0.4), rng.range(-0.4, 0.4)) * k as f64;
        kit.chamfer_block(
            Surface::Brick,
            DVec3::new(CHIMNEY * 0.9, length, CHIMNEY * 0.9),
            0.03,
            pose(
                DVec3::new(at.x + offset.x, CHIMNEY * 0.45, at.y + offset.y),
                DVec3::new(PI / 2.0 + rng.range(-0.3, 0.3), rng.range(0.0, PI), 0.0),
            ),
        );
    }
    for _ in 0..8 {
        let p = at + DVec2::new(rng.range(-0.9, 0.9), rng.range(-0.9, 0.9));
        kit.block(
            Surface::Brick,
            DVec3::new(0.22, 0.07, 0.11),
            pose(DVec3::new(p.x, 0.05, p.y), tilt(rng, 0.3)),
        );
    }
    // Splintered studs poking out of the mound.
    for _ in 0..4 {
        let (u, v) = (
            rng.range(-bw / 2.0 + 0.5, bw / 2.0 - 0.5),
            rng.range(back + 0.4, floor_from - 0.2),
        );
        kit.block(
            Surface::Wood,
            DVec3::new(0.06, rng.range(0.6, 1.1), 0.09),
            pose(
                DVec3::new(u, HOUSE_HEAP.height * 0.7, v),
                DVec3::new(
                    rng.range(-0.7, 0.7),
                    rng.range(0.0, PI),
                    rng.range(-0.7, 0.7),
                ),
            ),
        );
    }
    // Rubble strewn round the footprint past its kerb, thickest against the walls
    // and thinning out: shingles, clapboards, lumps of plaster, bricks and
    // splintered boards, so the heap spills onto the ground it fell on.
    for k in 0..HEAP_SPILL {
        let angle = rng.range(0.0, PI * 2.0);
        let near = rng.next();
        let out = 0.05 + 1.5 * near * near;
        let ray = DVec2::new(angle.cos(), angle.sin());
        // Where the ray from the centre leaves the footprint, then on past the kerb.
        let edge = (bw / 2.0 / ray.x.abs().max(1e-6)).min(bd / 2.0 / ray.y.abs().max(1e-6));
        let p = ray * (edge + out);
        let (coat, size, lean) = match k % 6 {
            0 => (
                Surface::Siding,
                DVec3::new(rng.range(0.5, 1.0), 0.03, 0.18),
                0.1,
            ),
            1 | 2 => (Surface::Roof, DVec3::new(0.32, 0.025, 0.24), 0.2),
            3 => (
                Surface::Concrete,
                DVec3::new(rng.range(0.14, 0.32), 0.08, rng.range(0.12, 0.26)),
                0.4,
            ),
            4 => (Surface::Brick, DVec3::new(0.22, 0.07, 0.11), 0.4),
            _ => (
                Surface::Splinters,
                DVec3::new(rng.range(0.4, 0.9), 0.04, 0.08),
                0.15,
            ),
        };
        kit.block(
            coat,
            size,
            pose(
                DVec3::new(p.x, size.y * 0.3, p.y),
                DVec3::new(
                    rng.range(-lean, lean),
                    rng.range(0.0, PI),
                    rng.range(-lean, lean),
                ),
            ),
        );
    }
}

/// A household's belongings broken in the wreck, at `placement` (its foot on the
/// placement's origin): `kind` picks a bed, an upturned table, a clawfoot bath, a
/// cast-iron stove, a fallen panelled door or a chest of drawers.
fn furniture(kit: &mut Kit<Surface>, kind: usize, placement: DMat4) {
    let part = |kit: &mut Kit<Surface>, coat: Surface, size: DVec3, at: DVec3| {
        kit.block(coat, size, placement * shift(at));
    };
    match kind % 6 {
        0 => {
            // Bed: frame, ticking mattress, headboard and footboard.
            part(
                kit,
                Surface::Wood,
                DVec3::new(1.0, 0.22, 1.95),
                DVec3::new(0.0, 0.2, 0.0),
            );
            part(
                kit,
                Surface::Trim,
                DVec3::new(0.92, 0.16, 1.82),
                DVec3::new(0.0, 0.38, 0.02),
            );
            part(
                kit,
                Surface::Curtain,
                DVec3::new(0.8, 0.08, 0.5),
                DVec3::new(0.0, 0.49, -0.6),
            );
            part(
                kit,
                Surface::Wood,
                DVec3::new(1.04, 0.85, 0.07),
                DVec3::new(0.0, 0.42, -0.99),
            );
            part(
                kit,
                Surface::Wood,
                DVec3::new(1.04, 0.5, 0.07),
                DVec3::new(0.0, 0.25, 0.99),
            );
        }
        1 => {
            // Kitchen table upside down, legs in the air, one snapped short.
            part(
                kit,
                Surface::Wood,
                DVec3::new(1.2, 0.05, 0.75),
                DVec3::new(0.0, 0.025, 0.0),
            );
            for (k, (x, z)) in [(-0.52, -0.3), (0.52, -0.3), (-0.52, 0.3), (0.52, 0.3)]
                .into_iter()
                .enumerate()
            {
                let length = if k == 2 { 0.3 } else { 0.68 };
                part(
                    kit,
                    Surface::Wood,
                    DVec3::new(0.06, length, 0.06),
                    DVec3::new(x, 0.05 + length / 2.0, z),
                );
            }
        }
        2 => {
            // Enamelled bath on its side walls, open to the sky.
            part(
                kit,
                Surface::Trim,
                DVec3::new(0.72, 0.08, 1.5),
                DVec3::new(0.0, 0.12, 0.0),
            );
            for x in [-0.33, 0.33] {
                part(
                    kit,
                    Surface::Trim,
                    DVec3::new(0.08, 0.5, 1.5),
                    DVec3::new(x, 0.33, 0.0),
                );
            }
            for z in [-0.71, 0.71] {
                part(
                    kit,
                    Surface::Trim,
                    DVec3::new(0.72, 0.5, 0.08),
                    DVec3::new(0.0, 0.33, z),
                );
            }
            for (x, z) in [(-0.28, -0.6), (0.28, -0.6), (-0.28, 0.6), (0.28, 0.6)] {
                part(
                    kit,
                    Surface::Iron,
                    DVec3::new(0.08, 0.1, 0.08),
                    DVec3::new(x, 0.04, z),
                );
            }
        }
        3 => {
            // Cast-iron stove with its flue pipe knocked askew.
            part(
                kit,
                Surface::Iron,
                DVec3::new(0.7, 0.75, 0.55),
                DVec3::new(0.0, 0.375, 0.0),
            );
            part(
                kit,
                Surface::Metal,
                DVec3::new(0.74, 0.04, 0.58),
                DVec3::new(0.0, 0.77, 0.0),
            );
            part(
                kit,
                Surface::Dark,
                DVec3::new(0.3, 0.22, 0.02),
                DVec3::new(0.0, 0.4, 0.28),
            );
            kit.block(
                Surface::Iron,
                DVec3::new(0.13, 0.9, 0.13),
                placement * pose(DVec3::new(0.15, 1.1, -0.1), DVec3::new(0.5, 0.0, 0.4)),
            );
        }
        4 => {
            // A panelled door flat on its face-up side, its knob and panels.
            part(
                kit,
                Surface::Accent,
                DVec3::new(0.9, 0.05, 2.0),
                DVec3::new(0.0, 0.025, 0.0),
            );
            for z in [-0.5, 0.45] {
                part(
                    kit,
                    Surface::Trim,
                    DVec3::new(0.6, 0.02, 0.7),
                    DVec3::new(0.0, 0.06, z),
                );
            }
            part(
                kit,
                Surface::Brass,
                DVec3::new(0.07, 0.07, 0.07),
                DVec3::new(0.34, 0.08, 0.05),
            );
        }
        _ => {
            // Chest of drawers with a drawer spilled out.
            part(
                kit,
                Surface::Wood,
                DVec3::new(0.95, 0.9, 0.48),
                DVec3::new(0.0, 0.45, 0.0),
            );
            for y in [0.22, 0.48, 0.74] {
                part(
                    kit,
                    Surface::Dark,
                    DVec3::new(0.86, 0.02, 0.01),
                    DVec3::new(0.0, y + 0.12, 0.245),
                );
            }
            part(
                kit,
                Surface::Splinters,
                DVec3::new(0.8, 0.2, 0.42),
                DVec3::new(0.1, 0.1, 0.6),
            );
        }
    }
}

/// Pieces of rubble a heap spills round its footprint.
const HEAP_SPILL: usize = 32;

/// Thickness of a torn piece of wall (siding, sheathing and plaster) and of roof
/// (shingles on decking) lying in the rubble.
const WALL_SLAB: f64 = 0.16;
const ROOF_SLAB: f64 = 0.13;

/// Cells across the rubble mound's surface along each side, and how far (m) its
/// rubble stands proud of the collider the shards rest on (low, high): a little
/// under it in places, more over it, so shards bed into it rather than float.
const MOUND_CELLS: usize = 7;
const MOUND_LUMPS: (f64, f64) = (-0.04, 0.14);

/// The rubble the walls and floors crush into ([`HousePart::Mound`]), heaped in the
/// footprint under the falling shards: a lumpy mound of plaster dust, broken
/// masonry and soot over its collider, bedded with lumps of plaster and stone,
/// bricks, torn lath and boards, joists and studs jutting out of it at every angle
/// and a few torn pieces of wall, so the collapse comes to rest on a heap with
/// mass rather than spread flat on clean ground.
fn rubble_mound(design: &HouseDesign) -> Kit<Surface> {
    use crate::sim::house_layout::{MOUND_HEIGHT, MOUND_INSET, MOUND_SLOPE_RUN};
    let mut kit: Kit<Surface> = Kit::new();
    let mut rng = Random::new(f64::from(design.variant) * 7.0 + 613.0);
    let (fx, fz) = (design.w / 2.0 - MOUND_INSET, design.d / 2.0 - MOUND_INSET);
    // The collider's height over a plan point.
    let rise = |x: f64, z: f64| {
        let t = ((fx - x.abs()).min(fz - z.abs()) / MOUND_SLOPE_RUN).clamp(0.0, 1.0);
        MOUND_HEIGHT * t
    };
    let n = MOUND_CELLS;
    let heights: Vec<Vec<f64>> = (0..=n)
        .map(|i| {
            (0..=n)
                .map(|k| {
                    let (x, z) = (
                        -fx + 2.0 * fx * i as f64 / n as f64,
                        -fz + 2.0 * fz * k as f64 / n as f64,
                    );
                    let edge = i == 0 || k == 0 || i == n || k == n;
                    if edge {
                        -0.03
                    } else {
                        rise(x, z) + rng.range(MOUND_LUMPS.0, MOUND_LUMPS.1)
                    }
                })
                .collect()
        })
        .collect();
    let point = |i: usize, k: usize| {
        DVec3::new(
            -fx + 2.0 * fx * i as f64 / n as f64,
            heights[i][k],
            -fz + 2.0 * fz * k as f64 / n as f64,
        )
    };
    for i in 0..n {
        for k in 0..n {
            let corners = [
                point(i, k),
                point(i, k + 1),
                point(i + 1, k + 1),
                point(i + 1, k),
            ];
            let facing = (corners[2] - corners[0])
                .cross(corners[1] - corners[3])
                .normalize_or(DVec3::Y);
            let facing = if facing.y < 0.0 { -facing } else { facing };
            let roll = rng.next();
            // Mostly pale plaster dust and crushed mortar, with patches of broken
            // stone, soot and splintered lath.
            let coat = if roll < 0.5 {
                Surface::Concrete
            } else if roll < 0.72 {
                Surface::Stone
            } else if roll < 0.84 {
                Surface::Curtain
            } else if roll < 0.93 {
                Surface::Wood
            } else {
                Surface::Dark
            };
            kit.quad(coat, corners, facing);
        }
    }
    let on = |rng: &mut Random, edge: f64| {
        let (x, z) = (
            rng.range(-fx + edge, fx - edge),
            rng.range(-fz + edge, fz - edge),
        );
        DVec3::new(x, rise(x, z), z)
    };
    let tilt = |rng: &mut Random, lean: f64| {
        DVec3::new(
            rng.range(-lean, lean),
            rng.range(0.0, PI),
            rng.range(-lean, lean),
        )
    };
    // Lumps of plaster, wall and stone bedded in the heap.
    for k in 0..11 {
        let at = on(&mut rng, 0.25);
        let size = DVec3::new(
            rng.range(0.3, 0.7),
            rng.range(0.14, 0.32),
            rng.range(0.25, 0.55),
        );
        let coat = [Surface::Concrete, Surface::Stone, Surface::Curtain][k % 3];
        kit.chamfer_block(
            coat,
            size,
            0.05,
            pose(at + DVec3::Y * size.y * 0.25, tilt(&mut rng, 0.4)),
        );
    }
    // Bricks spilt over it.
    for _ in 0..14 {
        let at = on(&mut rng, 0.1);
        kit.block(
            Surface::Brick,
            DVec3::new(0.22, 0.07, 0.11),
            pose(at + DVec3::Y * 0.04, tilt(&mut rng, 0.6)),
        );
    }
    // Torn lath and boards half buried in it.
    for k in 0..9 {
        let at = on(&mut rng, 0.3);
        let coat = if k % 3 == 0 {
            Surface::Splinters
        } else {
            Surface::Wood
        };
        kit.block(
            coat,
            DVec3::new(rng.range(0.6, 1.4), 0.03, rng.range(0.07, 0.16)),
            pose(at + DVec3::Y * 0.03, tilt(&mut rng, 0.35)),
        );
    }
    // Joists and studs jutting out of the heap at every angle.
    for _ in 0..5 {
        let at = on(&mut rng, 0.5);
        let length = rng.range(1.1, 2.0);
        let lean = rng.range(0.5, 1.1);
        let yaw = rng.range(0.0, PI * 2.0);
        let dir = DVec3::new(lean.sin() * yaw.cos(), lean.cos(), lean.sin() * yaw.sin());
        let foot = at - dir * length * 0.35;
        kit.block(
            Surface::Wood,
            DVec3::new(0.09, 0.14, length),
            aim(foot, foot + dir * length) * shift(DVec3::Z * (length / 2.0)),
        );
    }
    // Torn pieces of wall lying on it, siding or plaster up, with their thickness.
    for k in 0..4 {
        let at = on(&mut rng, 0.6);
        let (w, d) = (rng.range(0.8, 1.3), rng.range(0.5, 0.9));
        let outline = jagged(&mut rng, w, d);

        let faces = if k % 2 == 0 {
            [Surface::Siding, Surface::Curtain, Surface::Splinters]
        } else {
            [Surface::Curtain, Surface::Siding, Surface::Splinters]
        };
        slab(
            &mut kit,
            faces,
            &outline,
            WALL_SLAB,
            pose(at + DVec3::Y * 0.06, tilt(&mut rng, 0.3))
                * pose(DVec3::ZERO, DVec3::new(-PI / 2.0, 0.0, 0.0)),
        );
    }
    // The household's things broken in the heap, half buried at every angle.
    let mut things = Random::new(f64::from(design.variant) * 5.0 + 87.0);
    let first = things.range(0.0, 6.0) as usize;
    for k in 0..MOUND_FURNITURE {
        let at = on(&mut things, 0.9);
        let lean = tilt(&mut things, 0.5);
        furniture(&mut kit, first + k * 2, pose(at - DVec3::Y * 0.15, lean));
    }
    kit
}

/// Pieces of furniture broken in a collapse's rubble mound.
const MOUND_FURNITURE: usize = 3;

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

/// A ragged torn outline about `w` by `d` (counter-clockwise, star-shaped about
/// its centre): every other corner bitten in, so a patch torn out of a roof reads
/// as ripped shingles rather than a cut polygon.
fn ragged(rng: &mut Random, w: f64, d: f64) -> Vec<DVec2> {
    const CORNERS: usize = 12;
    let phase = rng.range(0.0, PI * 2.0);
    (0..CORNERS)
        .map(|i| {
            let angle = phase + (i as f64 + rng.range(-0.3, 0.3)) * PI * 2.0 / CORNERS as f64;
            let reach = if i % 2 == 0 {
                rng.range(0.8, 1.0)
            } else {
                rng.range(0.5, 0.78)
            };
            DVec2::new(angle.cos() * w / 2.0 * reach, angle.sin() * d / 2.0 * reach)
        })
        .collect()
}

/// A torn slab in `placement`'s xy plane, `thick` deep about it: `faces` are its
/// front (+z), back and broken edge finishes.
fn slab(
    kit: &mut Kit<Surface>,
    [front, back, edge]: [Surface; 3],
    outline: &[DVec2],
    thick: f64,
    placement: DMat4,
) {
    kit.face_with_holes(
        front,
        outline,
        &[],
        placement * shift(DVec3::Z * (thick / 2.0)),
    );
    kit.face_with_holes(
        back,
        &mirrored(outline),
        &[],
        placement * pose(DVec3::new(0.0, 0.0, -thick / 2.0), DVec3::new(0.0, PI, 0.0)),
    );
    let center = outline.iter().copied().sum::<DVec2>() / outline.len() as f64;
    let at = |p: DVec2, z: f64| placement.transform_point3(DVec3::new(p.x, p.y, z));
    for i in 0..outline.len() {
        let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
        let out = (a + b) / 2.0 - center;
        kit.quad(
            edge,
            [
                at(a, thick / 2.0),
                at(b, thick / 2.0),
                at(b, -thick / 2.0),
                at(a, -thick / 2.0),
            ],
            placement.transform_vector3(DVec3::new(out.x, out.y, 0.0)),
        );
    }
}

/// The back corners of a collapsed cottage still standing over its heap: each a
/// ragged run of the side wall, highest at the corner and broken off lower toward
/// the open floor, with its siding out, its plaster in, a corner board up its edge
/// and split timber along the break, so the heap reads as the ruin of a house.
fn corner_remnants(kit: &mut Kit<Surface>, design: &HouseDesign, rng: &mut Random, depth: f64) {
    const STEPS: usize = 4;
    let (bw, bd) = (design.w, design.d);
    let frames = wall_frames(bw, bd);
    for (index, corner_sign) in [(2, 1.0), (3, -1.0)] {
        let (frame, half) = frames[index];
        let corner = corner_sign * half;
        let reach = depth - rng.range(0.15, 0.45);
        let tall = PLINTH + rng.range(1.05, 1.35);
        let low = PLINTH + rng.range(0.35, 0.6);
        // The broken top, from the corner toward the open floor.
        let top: Vec<DVec2> = (0..=STEPS)
            .map(|k| {
                let f = k as f64 / STEPS as f64;
                let jag = if k == 0 || k == STEPS {
                    0.0
                } else {
                    rng.range(-0.18, 0.18)
                };
                DVec2::new(
                    corner - corner_sign * reach * f,
                    tall + (low - tall) * f.powf(0.8) + jag,
                )
            })
            .collect();
        let mut outline = vec![
            DVec2::new(corner - corner_sign * reach, PLINTH),
            DVec2::new(corner, PLINTH),
        ];
        outline.extend(&top);
        if corner_sign < 0.0 {
            outline.reverse();
        }
        kit.face_with_holes(Surface::Siding, &outline, &[], frame);
        kit.face_with_holes(
            Surface::Curtain,
            &mirrored(&outline),
            &[],
            frame * pose(DVec3::new(0.0, 0.0, -LINING), DVec3::new(0.0, PI, 0.0)),
        );
        let at = |p: DVec2, z: f64| frame.transform_point3(DVec3::new(p.x, p.y, z));
        let middle = DVec2::new(corner - corner_sign * reach / 2.0, (PLINTH + low) / 2.0);
        let edges = top
            .windows(2)
            .map(|w| (w[0], w[1]))
            .chain([(top[STEPS], DVec2::new(corner - corner_sign * reach, PLINTH))]);
        for (a, b) in edges {
            let out = (a + b) / 2.0 - middle;
            kit.quad(
                Surface::Splinters,
                [at(a, 0.0), at(b, 0.0), at(b, -LINING), at(a, -LINING)],
                frame.transform_vector3(DVec3::new(out.x, out.y, 0.0)),
            );
        }
        // The corner board, standing to the corner's full height.
        kit.block(
            Surface::Trim,
            DVec3::new(CORNER_BOARD, tall - PLINTH + 0.06, TRIM_PROUD * 2.0),
            frame
                * shift(DVec3::new(
                    corner - corner_sign * CORNER_BOARD / 2.0,
                    (tall + PLINTH) / 2.0 + 0.03,
                    TRIM_PROUD,
                )),
        );
        // A snapped stud standing out of the break.
        let u = corner - corner_sign * reach * rng.range(0.35, 0.65);
        kit.block(
            Surface::Splinters,
            DVec3::new(0.06, 0.55, 0.09),
            frame
                * pose(
                    DVec3::new(u, low + 0.45, -LINING - 0.05),
                    DVec3::new(0.0, 0.0, rng.range(-0.3, 0.3)),
                ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangles(node: &Node) -> usize {
        let own = node
            .drawable
            .as_ref()
            .map_or(0, |drawable| drawable.mesh.triangle_count());
        own + node.children.iter().map(triangles).sum::<usize>()
    }

    fn village() -> Vec<(f64, f64, f64, f64, f64)> {
        crate::sim::arena::arena_layout()
            .into_iter()
            .filter(|c| c.kind == crate::sim::types::CoverKind::House)
            .map(|c| (c.x, c.z, c.w, c.d, c.h))
            .collect()
    }

    /// Variety never costs more than the one cottage design did (about 6,400
    /// triangles), a second floor's windows and belt course about half again, and
    /// each damage stage's breaches, studs and stripped siding at most 1,200 more
    /// (only struck houses carry them).
    #[test]
    fn cottages_stay_within_the_former_triangle_budget() {
        let hits: Vec<TimberHit> = (0..6)
            .map(|i| TimberHit {
                x: [-3.0, 3.0, 0.5, -0.5, 2.6, -2.6][i],
                y: 1.2,
                z: [0.5, -0.5, 3.5, -3.5, 1.0, -1.0][i],
                size: 1.5,
            })
            .collect();
        let mut over = Vec::new();
        for (x, z, w, d, h) in village() {
            for (stage, hits) in [(0, &[][..]), (1, &hits[..3]), (2, &hits[..])] {
                let mut group = Node::group("");
                house(&mut group, x, z, w, d, h, stage, hits);
                let count = triangles(&group);
                println!(
                    "{x} {z} {:?} stage {stage}: {count}",
                    HouseDesign::of(x, z, w, d, h).style.form
                );
                let floors = if h >= crate::sim::house_layout::TWO_STOREY {
                    9_000
                } else {
                    6_400
                };
                if count > floors + 1_200 * stage as usize {
                    over.push(format!("{x} {z} stage {stage}: {count}"));
                }
            }
        }
        assert!(over.is_empty(), "{over:?}");
    }

    fn draws(node: &Node) -> usize {
        let mut count = usize::from(node.drawable.is_some());
        for child in &node.children {
            count += draws(child);
        }
        count
    }

    #[test]
    fn collapse_pieces_and_heaps_draw_once_each() {
        for (x, z, w, d, h) in [
            (-45.0, -39.0, 5.0, 6.0, 4.6),
            (45.0, 39.0, 5.0, 6.0, 4.6),
            (17.0, 46.0, 7.0, 5.0, 5.2),
        ] {
            let pieces = house_pieces(x, z, w, d, h, &[]);
            let design = HouseDesign::of(x, z, w, d, h);
            assert_eq!(pieces.len(), design.pieces().len());
            // One draw a piece, and a shingled roof slab's shingles one more.
            let shingled = design.style.cover == RoofCover::Shingles;
            for (piece, node) in &pieces {
                let expected = if piece.part.is_roof() && shingled {
                    2
                } else {
                    1
                };

                assert_eq!(draws(node), expected, "{piece:?}");
            }

            let mut heap = Node::group("");
            house_heap(&mut heap, x, z, w, d - HOUSE_HEAP.depth, 77.0 * 512.0);
            batch(&mut heap.children[0]);
            assert_eq!(draws(&heap), 1, "heap");
        }
    }
}
