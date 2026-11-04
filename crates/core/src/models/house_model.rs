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
//! Shells leave their mark where they struck and nowhere else: a breach torn through
//! the siding that grows with every shell landing on it, the glass of the windows
//! beside it, and once it has taken out much of a wall the eave over it sagging and
//! the window frames beside it knocked askew. A blast on the roof holes the roof
//! where it landed and topples the chimney if it landed by the stack. Some cottages
//! have a window boarded over. A collapsed cottage falls apart into
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
    BRICK_TEXTURE, CASING, CLAPBOARD_AVERAGE, CLAPBOARD_TEXTURE, Glazing, Opening, Palette,
    Surface, door, opening_casing, shutter_width, shutters, surface_material, textured, tint_over,
    uv_per_metre, wall_with_breaches, window,
};
use super::house_collapse::{
    ShardClip, chimney_breaks, clip_mesh, clip_meshes, fold_shard, mottle, roof_breaks, wall_breaks,
};
use super::house_surfaces::{HouseSurface, house_material};
use super::model_primitives::{Cache, material, put, shadowed};
use super::tank_kit::{Kit, KitMeshes, aim, pose, shift};
use crate::geometry::math::scale_hex_color;
use crate::geometry::{Mesh, narrow, octahedron_geometry, widen};
use crate::scene::{Material, Node};
use crate::sim::house_layout::{
    CHIMNEY, EAVE_OVERHANG, HOUSE_HEAP, HouseDesign, HousePart, HousePiece, PLINTH, PieceShape,
    RAFTER_DEPTH, Roof, RoofCover, RoofForm, Shutters, WALL_DEPTH, WING_OVERHANG, WallFinish, Wing,
    heap_house, is_roof_mark,
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

/// Shell holes: how far a breach reaches either way from where it was struck per
/// unit of its mark's size (wider than tall, as siding tears along its courses), the
/// reveal depth of the torn edge and how far the dark interior sits behind the face.
const HOLE_REACH: (f64, f64) = (0.58, 0.45);
const HOLE_EDGE: f64 = 0.14;
const HOLE_BACK: f64 = 0.16;
/// The smallest breach worth cutting (each side, metres): a hit with no more room
/// than this blows in the window beside it instead.
const HOLE_LEAST: f64 = 0.3;
/// A breach this big (square metres) has torn out a stretch of wall: the room behind
/// it shows (its floor and walls, [`ROOM_DEPTH`] deep), its studs hang snapped across
/// it and the siding round it is stripped back to the sheathing.
const BIG_BREACH: f64 = 0.9;
const ROOM_DEPTH: f64 = 1.3;
/// How far beyond a big breach the siding is stripped (metres, most at the middle
/// courses), and the courses stripped above and below it.
const STRIPPED_REACH: (f64, f64) = (0.18, 0.5);
const STRIPPED_ROWS: usize = 2;
/// How deep the fresh-split face of a hole's torn boards shows before the
/// charred timber behind.
const SPLIT_FACE: f64 = 0.03;
/// How deep the charred boards and sheathing go before the plaster behind.
const PLASTER_LIP: f64 = 0.08;
/// Splintered boards bent out of the face round a hole.
const BENT_BOARDS: usize = 3;
/// What a breach tears back at a time, so its outline steps up its wall course by
/// course with every board (or brick) broken off at its own length: a clapboard
/// course, and two courses of brick.
const CLAPBOARD_COURSE: f64 = 0.2;
const BRICK_STEP: f64 = 0.15;
/// Stud spacing in the frame a breach lays bare, and the most snapped studs a big
/// breach shows.
const STUD_SPACING: f64 = 0.4;
const SNAPPED_STUDS: usize = 8;
/// Clearance a hole keeps from openings, trim and other holes.
const HOLE_MARGIN: f64 = 0.06;
/// A window within this reach (m) of a breach loses its glass; one beside a big
/// breach is also knocked askew by up to [`ASKEW`] radians.
const WINDOW_BLAST: f64 = 0.45;
const ASKEW: f64 = 0.08;
/// A boarded window's boards hold until a breach comes this close (m) to it.
const BOARDS_HOLD: f64 = 0.12;
/// A breach of this many shells' size tears out the windows it reaches rather than
/// stopping at their casings.
const SWALLOW: f64 = 2.6;
/// The share of a wall's area its breaches have torn out at which every window in
/// it breaks: the frame racks round them.
const RACKED_WALL: f64 = 0.22;
/// How far a roof sags over a breach that has taken out the wall under its eave (or
/// gable) at most, and the share of the wall's length the breach spans when the sag
/// starts and when it is full.
const ROOF_SAG: f64 = 0.7;
const SAG_SPAN: (f64, f64) = (0.25, 0.6);
/// How far round a roof hole the deck dips (times its size) and how deep at most.
const HOLE_DIP: (f64, f64) = (1.6, 0.14);
/// How a shelled cottage's frame gives: each wall bows in round its breaches (m
/// per share of its area torn out, at most [`BOW_MOST`], carried this deep inside
/// the face), and the whole frame racks toward its most breached side once its
/// walls have lost [`RACK_FROM`] of their area, up to [`RACK`] metres of lean per
/// metre up the walls.
const BOW: f64 = 0.3;
const BOW_MOST: f64 = 0.12;
const BULGE_BAND: f64 = 0.75;
const RACK_FROM: f64 = 0.06;
const RACK: f64 = 0.08;
/// Patches of shingles a shelled roof has shed round each roof hole, and over a
/// breach that reaches the eave; their radius (m), and the loose shingles each
/// sheds onto the roof and the ground.
const SHED_ROUND_HOLE: usize = 2;
const SHED_RADIUS: (f64, f64) = (0.4, 0.75);
const SHED_SHINGLES: usize = 5;
/// A roof hole's extent along and across its slope for a first strike, and how
/// much more of it each unit of mark size adds.
const ROOF_HOLE: (f64, f64) = (1.25, 1.15);
const ROOF_HOLE_GROWTH: f64 = 0.35;

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
/// Average colours of the brick and stone textures, for painted pieces.
const BRICK_AVERAGE: u32 = 0x864c3b;
const STONE_AVERAGE: u32 = 0x6b6459;

/// Freshly split boards round a standing cottage's breaches, pale and warm where
/// the light catches their ends, and the char a shell's blast leaves round them.
const FRESH_SPLIT: u32 = 0xe2bf86;
const CHAR: u32 = 0x231d18;
/// The standing roof's mean weathering ([`ROOF_WEATHER`]), which a fallen slab's
/// shingles keep.
const FALLEN_SHINGLES: f64 = 0.84;
/// Old framing and decking a collapse lays bare: weathered, dusty timber, darker
/// than the split ends ([`Surface::Splinters`]) that catch the light.
const RAW_TIMBER: u32 = 0x7a5d42;
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
/// Share of a cottage's side and back windows boarded over after shelling.
const BOARDED_WINDOWS: f64 = 0.14;
/// Thickness of the boards nailed over a shelled window.
const PLANK: f64 = 0.05;
/// Patches of plaster fallen off each collapsed wall's lining.
const PLASTER_FALLS: usize = 14;
/// A standing shingle roof's weathering (linear shade at the eave and at the
/// ridge) and the light its ridge caps catch.
const ROOF_WEATHER: (f64, f64) = (0.64, 1.04);
const RIDGE_SHINE: f64 = 1.32;

/// How much of a cottage a build makes. Both show the same damage, from the same
/// marks: a collapse starts from the cottage the player was looking at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Build {
    /// The standing cottage.
    Standing,
    /// Each collapse piece apart, as the cottage was when it fell.
    Pieces,
}

/// Where the parts of a cottage go while it is built: the standing model keeps
/// its roof (and chimney) apart from the walls so a shelled roof can sag; the
/// collapse gives each piece its own kit.
struct Parts {
    split: bool,
    kits: Vec<Kit<Surface>>,
    courses: Vec<Courses>,
    nodes: Vec<Vec<Node>>,
    /// Each wall's openings and shell holes as `[u0, y0, u1, y1]` in its frame, where
    /// a collapse's torn edges leave a gap.
    openings: [Vec<[f64; 4]>; 4],
    /// Each wall's breaches, which the roof over them and the frame answer to.
    holes: [Vec<Hole>; 4],
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
            holes: Default::default(),
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
        // A fallen wall keeps its boards or brick courses, so a panel lying in the
        // wreck still reads as a piece of wall rather than a painted sheet: its own
        // textured draw, painted and mottled through its vertices like the rest of
        // the piece. Its paints are the standing cottage's, so the cottage the
        // collapse starts from looks the one that stood; the dust cloud does the
        // dulling.
        if surface == Surface::Siding {
            return match walls {
                WallFinish::Brick => textured(BRICK_TEXTURE, palette.siding, 0.035, 0.9),
                WallFinish::Clapboard => textured(
                    CLAPBOARD_TEXTURE,
                    tint_over(palette.siding, CLAPBOARD_AVERAGE),
                    0.03,
                    0.8,
                ),
            };
        }
        let paint = match surface {
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
        return match surface {
            Surface::Splinters => satin(FRESH_SPLIT),
            Surface::Soot | Surface::Dark => satin(CHAR),
            _ => satin(paint),
        };
    }
    match surface {
        Surface::Siding if walls == WallFinish::Brick => {
            textured(BRICK_TEXTURE, palette.siding, 0.035, 0.9)
        }
        Surface::Siding | Surface::Brick | Surface::Stone | Surface::Glass | Surface::Lamp => {
            source
        }
        // A breach's freshly split boards catch the light round the dark of the
        // room and the blast's char round it is near black, so a shell hole reads
        // as torn timber from the battlefield's height rather than a smudge.
        Surface::Splinters => satin(FRESH_SPLIT),
        Surface::Soot => satin(CHAR),
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

/// Matte-looking satin for planting, so it joins the cottage's satin draw.
fn satin(color: u32) -> Arc<Material> {
    material(color, SATIN.0, SATIN.1)
}

static OCTAHEDRON: Cache<(), Mesh> = Cache::new("house_model::OCTAHEDRON");

/// `house(group, c)`: the village cottage for a cover box, its type chosen from its
/// position and marked where the shells and blasts that hit it struck.
#[allow(clippy::too_many_arguments)]
pub(super) fn house(group: &mut Node, x: f64, z: f64, w: f64, d: f64, h: f64, hits: &[TimberHit]) {
    let design = HouseDesign::of(x, z, w, d, h);
    let palette = palette(&design, x, z);
    group.children.push(cottage(&design, &palette, hits));
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
    group.children.push(cottage(&design, &palette, &[]));
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
    HousePieces::new(x, z, w, d, h, hits).collect()
}

/// A destroyed cottage's pieces ([`house_pieces`]) built a little at a time, in the
/// same order and from the same random draws as all at once. A cottage's pieces cost
/// a slow device about a second, so presentation builds them over many frames while
/// the damaged cottage still stands: its shell hits stop changing once a shot fells
/// it. Each [`step`](Self::step) cuts one half of a piece's shard or finishes a piece.
pub struct HousePieces {
    design: HouseDesign,
    palette: Palette,
    hits: Vec<TimberHit>,
    /// The cottage's collapse shapes and its kits split by part, worked out by the
    /// first two steps.
    shapes: Option<Vec<PieceShape>>,
    kits: Option<CottageKits>,
    /// The part being cut and its shapes still to cut.
    cutting: Option<(PartCut, std::vec::IntoIter<usize>)>,
    piece: Option<PieceCut>,
    rng: Random,
}

/// The piece being cut: its shape, the halves of its shard still to cut (each with
/// its fold) and what is cut so far.
struct PieceCut {
    shape: usize,
    halves: std::vec::IntoIter<(PieceShape, DMat4)>,
    shard: Node,
}

/// A collapsing cottage's roof, wall openings, how its frame had given and the parts
/// still to cut, with their kits and shingle courses.
struct CottageKits {
    roof: Roof,
    openings: [Vec<[f64; 4]>; 4],
    settle: Settle,
    parts: std::collections::VecDeque<(HousePart, Kit<Surface>, Courses)>,
}

/// A part's finished meshes and shingles, which each of its shapes is cut from.
struct PartCut {
    part: HousePart,
    meshes: KitMeshes<Surface>,
    shingled: Option<Mesh>,
}

/// What one [`HousePieces::step`] did.
pub enum PieceStep {
    /// Cut part of a piece.
    Cut,
    /// Finished a piece.
    Piece(HousePiece, Node),
    /// Every piece is built.
    Done,
}

impl HousePieces {
    pub fn new(x: f64, z: f64, w: f64, d: f64, h: f64, hits: &[TimberHit]) -> Self {
        let design = HouseDesign::of(x, z, w, d, h);
        Self {
            palette: palette(&design, x, z),
            rng: Random::new(f64::from(design.variant) * 13.0 + 5.0),
            design,
            hits: hits.to_vec(),
            shapes: None,
            kits: None,
            cutting: None,
            piece: None,
        }
    }

    /// Cut the next half of the current piece's shard, finish the piece once its
    /// halves are cut, or start the next piece (folding its shard into halves).
    pub fn step(&mut self) -> PieceStep {
        let Some(shapes) = &self.shapes else {
            self.shapes = Some(self.design.pieces());
            return PieceStep::Cut;
        };
        let Some(kits) = &mut self.kits else {
            let (roof, parts, settle) = cottage_parts(&self.design, Build::Pieces, &self.hits);
            let Parts {
                kits,
                courses,
                openings,
                ..
            } = parts;
            let parts = HousePart::ALL
                .into_iter()
                .zip(kits)
                .zip(courses)
                .map(|((part, kit), courses)| (part, kit, courses))
                .collect();
            self.kits = Some(CottageKits {
                roof,
                openings,
                settle,
                parts,
            });
            return PieceStep::Cut;
        };
        if let Some(PieceCut {
            shape,
            halves,
            shard,
        }) = &mut self.piece
        {
            let Some((cut, _)) = &self.cutting else {
                unreachable!("a piece is cut from its part");
            };
            let shape = &shapes[*shape];
            return match halves.next() {
                Some((half, fold)) => {
                    let mut bits = cut_half(
                        &self.design,
                        &self.palette,
                        &kits.roof,
                        &kits.settle,
                        &kits.openings,
                        cut,
                        shape,
                        &half,
                        &fold,
                        &mut self.rng,
                    );
                    shard.children.append(&mut bits.children);
                    PieceStep::Cut
                }
                None => {
                    let shard = std::mem::take(shard);
                    let piece = shape.piece;
                    let node = finish_piece(&self.design, cut.part, shape, shard);
                    self.piece = None;
                    PieceStep::Piece(piece, node)
                }
            };
        }
        loop {
            if let Some((_, queue)) = &mut self.cutting
                && let Some(index) = queue.next()
            {
                // Each half of a shard is buckled along its inner crack.
                let halves = fold_shard(&shapes[index], &kits.roof, &mut self.rng);
                self.piece = Some(PieceCut {
                    shape: index,
                    halves: halves.into_iter(),
                    shard: Node::group(""),
                });
                return PieceStep::Cut;
            }
            let Some((part, kit, courses)) = kits.parts.pop_front() else {
                return PieceStep::Done;
            };
            let kit = if part == HousePart::Mound {
                rubble_mound(&self.design)
            } else {
                kit
            };
            let cut = PartCut {
                part,
                meshes: kit.finish_scaled(cottage_uv(self.design.style.walls)),
                shingled: (!courses.positions.is_empty()).then(|| courses.mesh()),
            };
            let queue: Vec<usize> = (0..shapes.len())
                .filter(|&index| shapes[index].piece.part == part)
                .collect();
            self.cutting = Some((cut, queue.into_iter()));
        }
    }
}

impl Iterator for HousePieces {
    type Item = (HousePiece, Node);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.step() {
                PieceStep::Cut => {}
                PieceStep::Piece(piece, node) => return Some((piece, node)),
                PieceStep::Done => return None,
            }
        }
    }
}

/// One half of a collapse piece's shard: `half` of `cut`'s part cut from its meshes,
/// its torn edges closed with splintered timber and buckled by `fold`.
#[allow(clippy::too_many_arguments)]
fn cut_half(
    design: &HouseDesign,
    palette: &Palette,
    roof: &Roof,
    settle: &Settle,
    openings: &[Vec<[f64; 4]>; 4],
    cut: &PartCut,
    shape: &PieceShape,
    half: &PieceShape,
    fold: &DMat4,
    rng: &mut Random,
) -> Node {
    let PartCut {
        part,
        meshes,
        shingled,
    } = cut;
    let part = *part;
    let walls = design.style.walls;
    let uv = cottage_uv(walls);
    let whole = shape.region.is_whole();
    let mut bits = Node::group("");
    let clip = (!whole).then(|| ShardClip::new(&half.plane, &half.region));
    let cut = match &clip {
        Some(clip) => clip_meshes(meshes, clip),
        None => meshes.clone(),
    };
    add_cottage(&mut bits, cut, palette, walls, true);
    let mut breaks = Kit::new();
    if let Some(index) = part.wall_index() {
        wall_breaks(&mut breaks, half, &openings[index], rng);
    } else if part.is_roof() && !whole {
        roof_breaks(&mut breaks, half, roof, rng);
    } else if part == HousePart::Chimney {
        chimney_breaks(&mut breaks, half, design.flue(), design.chimney_snap(), rng);
    }
    add_cottage(&mut bits, breaks.finish_scaled(&uv), palette, walls, true);
    if let Some(mesh) = shingled {
        let mesh = match &clip {
            Some(clip) => clip_mesh(mesh, clip),
            None => Some(mesh.clone()),
        };
        if let Some(mesh) = mesh {
            // A fallen slab keeps its shingles, weathered as they were on the roof, so
            // it reads as a broken roof rather than a painted board.
            let material = house_material(
                HouseSurface::Shingles,
                scale_hex_color(palette.roof, FALLEN_SHINGLES),
            );
            bits.children.push(shadowed(Arc::new(mesh), material));
        }
    }
    // A collapse starts from the frame as the shells left it: racked, bowed and
    // sagging where they did that, square where they did not.
    for child in &mut bits.children {
        if let Some(drawable) = &mut child.drawable {
            let mesh = Arc::make_mut(&mut drawable.mesh);
            settle.mesh(mesh);
            mesh.apply_matrix4(fold);
        }
    }
    bits
}

/// A collapse piece from its cut `shard`: centred on its collider, batched and
/// grimed, casting a shadow only where that shows.
fn finish_piece(
    design: &HouseDesign,
    part: HousePart,
    shape: &PieceShape,
    mut shard: Node,
) -> Node {
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
    // Only the side slopes' ridge slabs and the chimney's top cast shadows as they
    // fall: they shade the wreck from overhead, the smaller pieces' shadows are lost
    // under them, and each caster is a draw.
    let casts = match part {
        HousePart::RightRoof | HousePart::LeftRoof => shape.piece.shard.is_multiple_of(2),
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
    root
}

/// A cottage's kits for `build`, with its marks cut into its walls and roof, and how
/// its frame gives under them.
fn cottage_parts(design: &HouseDesign, build: Build, hits: &[TimberHit]) -> (Roof, Parts, Settle) {
    let mut cut = PartsCut::new(design, build, hits);
    while !cut.step(design) {}
    let settle = cut.settle(design);
    (cut.roof, cut.parts, settle)
}

/// A cottage's kits built a section at a time: the plinth, each wall, then the roof
/// and the rest. Built at once, they took a slow device's whole frame.
struct PartsCut {
    roof: Roof,
    parts: Parts,
    rng: Random,
    hits: Vec<TimberHit>,
    /// The holes blasts tore in the roof, from its marks.
    roof_holes: Vec<RoofHole>,
    build: Build,
    /// The next section: the plinth, the four walls, then the roof and the rest.
    next: usize,
}

/// The plinth, four walls and the roof.
const HOUSE_SECTIONS: usize = 6;

impl PartsCut {
    fn new(design: &HouseDesign, build: Build, hits: &[TimberHit]) -> Self {
        let roof = design.roof();
        let parts = Parts::new(build);
        let rng = Random::new(f64::from(design.variant) + 41.0);
        Self {
            roof_holes: roof_holes(design, &roof, hits),
            roof,
            parts,
            rng,
            hits: hits.to_vec(),
            build,
            next: 0,
        }
    }

    /// Build the next section; whether that was the last.
    fn step(&mut self, design: &HouseDesign) -> bool {
        let Self {
            roof,
            parts,
            rng,
            hits,
            roof_holes,
            build,
            next,
        } = self;
        match *next {
            0 => house_base(parts, design),
            wall if wall < HOUSE_SECTIONS - 1 => {
                house_wall(parts, design, roof, *build, hits, rng, wall - 1);
            }
            _ => house_roof(parts, design, roof, *build, roof_holes, hits),
        }
        *next += 1;
        *next == HOUSE_SECTIONS
    }

    /// How the frame gives under the breaches cut so far.
    fn settle(&self, design: &HouseDesign) -> Settle {
        Settle::new(design, &self.roof, &self.parts.holes, &self.roof_holes)
    }
}

/// Build a standing cottage: one turned node.
fn cottage(design: &HouseDesign, palette: &Palette, hits: &[TimberHit]) -> Node {
    let mut build = CottageBuild::new(*design, *palette, hits);
    loop {
        if let Some(cottage) = build.step() {
            return cottage;
        }
    }
}

/// A standing cottage built in steps (its kits a section at a time, its walls, its
/// roof) with the same result as at once. A cottage's look changes with each shell
/// that holes it, and rebuilding it within one frame cost a slow device a frame or two
/// each time.
struct CottageBuild {
    design: HouseDesign,
    palette: Palette,
    hits: Vec<TimberHit>,
    /// The kits being cut, until they are done.
    cut: Option<PartsCut>,
    kits: Option<StandingKits>,
    cottage: Node,
}

/// A standing cottage's kits once built: its roof, how its frame gives, the walls'
/// and the roof's kits (taken as each is finished), the roof's shingle courses and
/// the loose nodes.
struct StandingKits {
    roof: Roof,
    settle: Settle,
    body: Option<Kit<Surface>>,
    roof_kit: Option<Kit<Surface>>,
    roof_courses: Courses,
    nodes: Vec<Node>,
}

impl CottageBuild {
    fn new(design: HouseDesign, palette: Palette, hits: &[TimberHit]) -> Self {
        Self {
            design,
            palette,
            hits: hits.to_vec(),
            cut: None,
            kits: None,
            cottage: Node::group(""),
        }
    }

    /// The next step; the turned cottage once its last is done.
    fn step(&mut self) -> Option<Node> {
        let (design, palette) = (&self.design, &self.palette);
        let walls = design.style.walls;
        let uv = cottage_uv(walls);
        let Some(StandingKits {
            roof,
            settle,
            body,
            roof_kit,
            roof_courses,
            nodes,
        }) = &mut self.kits
        else {
            let cut = self
                .cut
                .get_or_insert_with(|| PartsCut::new(design, Build::Standing, &self.hits));
            if !cut.step(design) {
                return None;
            }
            let cut = self.cut.take().expect("cut above");
            let settle = cut.settle(design);
            let PartsCut { roof, parts, .. } = cut;
            let Parts {
                kits,
                courses,
                nodes,
                ..
            } = parts;
            let mut kits = kits.into_iter();
            let body = kits.next().expect("body kit");
            let roof_kit = kits.next().expect("roof kit");
            let roof_courses = courses.into_iter().nth(1).expect("roof courses");
            let nodes = nodes.into_iter().flatten().collect();
            self.kits = Some(StandingKits {
                roof,
                settle,
                body: Some(body),
                roof_kit: Some(roof_kit),
                roof_courses,
                nodes,
            });
            return None;
        };
        if let Some(body) = body.take() {
            let mut body_meshes = body.finish_scaled(&uv);
            settle.meshes(&mut body_meshes);
            grime(&mut body_meshes, roof.ridge);
            add_weathered(&mut self.cottage, body_meshes, palette, walls);
            return None;
        }
        let cottage = &mut self.cottage;
        let mut roof_meshes = roof_kit.take().expect("roof kit").finish_scaled(&uv);
        settle.meshes(&mut roof_meshes);
        roof_courses.settle(settle);
        grime(&mut roof_meshes, roof.ridge);
        add_weathered(cottage, roof_meshes, palette, walls);
        if !roof_courses.positions.is_empty() {
            let material = WEATHERED_SHINGLES.get_or_insert(palette.roof, || Material {
                vertex_colors: true,
                ..(*house_material(HouseSurface::Shingles, palette.roof)).clone()
            });
            cottage.children.push(shadowed(
                Arc::new(roof_courses.weathered_mesh(roof)),
                material,
            ));
        }
        for mut node in std::mem::take(nodes) {
            node.position = settle.point(node.position);
            cottage.children.push(node);
        }
        cottage.set_rotation_euler(0.0, design.yaw(), 0.0);
        Some(std::mem::take(cottage))
    }
}

/// A cottage cover's model ([`cover_model`](super::cover_model) for a cottage) built
/// a step at a time: presentation spreads a hit cottage's new look over a few frames,
/// keeping its last look meanwhile.
pub struct HouseCoverBuild {
    x: f64,
    z: f64,
    cottage: CottageBuild,
}

impl HouseCoverBuild {
    pub fn new(x: f64, z: f64, w: f64, d: f64, h: f64, hits: &[TimberHit]) -> Self {
        let design = HouseDesign::of(x, z, w, d, h);
        let palette = palette(&design, x, z);
        Self {
            x,
            z,
            cottage: CottageBuild::new(design, palette, hits),
        }
    }

    /// The next step; the cover's model once its last is done.
    pub fn step(&mut self) -> Option<Node> {
        let cottage = self.cottage.step()?;
        let mut root = Node {
            position: DVec3::new(self.x, 0.0, self.z),
            ..Node::default()
        };
        root.children.push(cottage);
        Some(root)
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

/// How a shelled cottage's frame has given (build frame), worked out from its
/// breaches and roof holes alone, so an unbroken wall stands square: each breached
/// wall bowed in round its breaches, the frame racked toward its most breached side
/// once enough wall is gone, the eave (or gable end) over a breach that has taken out
/// much of its wall sagging from there, and the deck dipping round each roof hole.
/// The standing cottage and its collapse pieces are moved alike.
struct Settle {
    /// Lean per metre up the walls, in the build frame's x and z.
    rack: DVec2,
    /// How far each wall (front, back, right, left) bows in at its middle.
    bows: [f64; 4],
    sags: Vec<Sag>,
    /// The half box and the walls' height over the plinth, for the bows.
    hw: f64,
    hd: f64,
    wall: f64,
}

/// A triangle corner a [`Settle`] splits and moves: its point, normal, texture
/// coordinates, colour and whether it is a ridge cap's.
#[derive(Clone, Copy)]
struct Corner {
    p: DVec3,
    n: DVec3,
    uv: DVec2,
    color: DVec3,
    cap: bool,
}

impl Corner {
    fn lerp(&self, other: &Corner, t: f64) -> Corner {
        Corner {
            p: self.p.lerp(other.p, t),
            n: self.n.lerp(other.n, t).normalize_or(self.n),
            uv: self.uv.lerp(other.uv, t),
            color: self.color.lerp(other.color, t),
            cap: self.cap && other.cap,
        }
    }
}

/// The spacing (m) of the planes a sag slices the surfaces it bends along, and the
/// most slices a triangle takes along each axis.
const SAG_STEP: f64 = 0.9;
const MAX_SLICES: i64 = 40;

/// A sag of the roof: over a breach under an eave or a gable end, or round a hole in
/// the deck.
enum Sag {
    /// Over breach of the wall `out` from the middle (unit x or z), centred at
    /// `center` along it with `half` its reach, dropping by `drop` at the wall's
    /// line (less toward the ridge or the far gable) above `from` (the breach's top).
    Wall {
        out: DVec3,
        center: f64,
        half: f64,
        drop: f64,
        from: f64,
        span: f64,
    },
    /// Round a roof hole at `center` (x, z), `radius` wide and `drop` deep.
    Dip {
        center: DVec2,
        radius: f64,
        drop: f64,
        above: f64,
    },
}

impl Settle {
    fn new(
        design: &HouseDesign,
        roof: &Roof,
        holes: &[Vec<Hole>; 4],
        roof_holes: &[RoofHole],
    ) -> Self {
        let frames = wall_frames(design.w, design.d);
        let height = roof.wall - PLINTH;
        let mut bows = [0.0; 4];
        let mut sags = Vec::new();
        let mut lost = DVec2::ZERO;
        let (mut torn, mut total) = (0.0, 0.0);
        for (index, (frame, half)) in frames.iter().enumerate() {
            let area = 2.0 * half * height;
            let breached: f64 = holes[index].iter().map(Hole::area).sum();
            total += area;
            torn += breached;
            bows[index] = (BOW * breached / area).min(BOW_MOST);
            let out = frame.transform_vector3(DVec3::Z);
            lost += DVec2::new(out.x, out.z) * breached;
            for hole in &holes[index] {
                // A breach that has taken the wall up to its top plate and across
                // much of it lets the roof over it down.
                let [u0, _, u1, y1] = hole.bounds;
                let share = (u1 - u0) / (2.0 * half);
                let full = ((share - SAG_SPAN.0) / (SAG_SPAN.1 - SAG_SPAN.0)).clamp(0.0, 1.0);
                if full <= 0.0 || y1 < roof.wall - 0.7 {
                    continue;
                }
                let middle = frame.transform_point3(DVec3::new((u0 + u1) / 2.0, 0.0, 0.0));
                let along = frame.transform_vector3(DVec3::X);
                let center = middle.dot(along.abs());
                // The eave walls (±x) carry the deck's edge, which hangs below their
                // top: it drops with the strip of wall over the breach.
                let eave = out.x.abs() > 0.5;
                sags.push(Sag::Wall {
                    out,
                    center,
                    half: (u1 - u0) / 2.0 + 0.8,
                    drop: ROOF_SAG * full,
                    from: if eave { y1.min(roof.eave_y) - 0.05 } else { y1 },
                    span: if eave { roof.eave_x } else { roof.rake_z },
                });
            }
        }
        // A broken ridge lets the roof down along the break.
        for (a, b) in broken_ridge(roof, roof_holes) {
            sags.push(Sag::Dip {
                center: DVec2::new(0.0, (a + b) / 2.0),
                radius: (b - a) / 2.0 + RIDGE_DIP.0,
                drop: RIDGE_DIP.1,
                above: roof.eave_y - 0.05,
            });
        }
        for hole in roof_holes {
            sags.push(Sag::Dip {
                center: hole.plan,
                radius: hole.half_along.max(hole.half_across) * HOLE_DIP.0,
                drop: HOLE_DIP.1 * ((hole.size - 1.0) / 2.0).clamp(0.25, 1.0),
                above: roof.eave_y - 0.05,
            });
        }
        let share = torn / total;
        let rack = if share > RACK_FROM && lost.length() > 1e-9 {
            lost.normalize() * RACK * ((share - RACK_FROM) / (3.0 * RACK_FROM)).min(1.0)
        } else {
            DVec2::ZERO
        };
        Self {
            rack,
            bows,
            sags,
            hw: design.w / 2.0,
            hd: design.d / 2.0,
            wall: roof.wall,
        }
    }

    fn still(&self) -> bool {
        self.rack == DVec2::ZERO && self.bows.iter().all(|&bow| bow == 0.0) && self.sags.is_empty()
    }

    /// Where a point of the square cottage (build frame) has moved to.
    fn point(&self, p: DVec3) -> DVec3 {
        let mut q = p + DVec3::new(self.rack.x, 0.0, self.rack.y) * (p.y - PLINTH).max(0.0);
        let t = (p.y - PLINTH) / (self.wall - PLINTH);
        if (0.0..=1.0).contains(&t) {
            let (hw, hd) = (self.hw, self.hd);
            let bow = (t * PI).sin();
            // Front, back, right, left: each bowed in between its corners.
            let across = |v: f64, half: f64| (PI / 2.0 * (v / half).clamp(-1.0, 1.0)).cos();
            if p.z > hd - BULGE_BAND {
                q.z -= bow * self.bows[0] * across(p.x, hw);
            }
            if p.z < -hd + BULGE_BAND {
                q.z += bow * self.bows[1] * across(p.x, hw);
            }
            if p.x > hw - BULGE_BAND {
                q.x -= bow * self.bows[2] * across(p.z, hd);
            }
            if p.x < -hw + BULGE_BAND {
                q.x += bow * self.bows[3] * across(p.z, hd);
            }
        }
        for sag in &self.sags {
            q.y -= sag.drop_at(p);
        }
        q
    }

    fn mesh(&self, mesh: &mut Mesh) {
        if self.still() {
            return;
        }
        let count = mesh.positions.len();
        let sliced = mesh.indices.is_none()
            && (0..count / 3).any(|t| {
                let p = [3 * t, 3 * t + 1, 3 * t + 2].map(|i| widen(mesh.positions[i]));
                self.slices(p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]))
            });
        if !sliced {
            // A mesh no sag slices is moved where it lies, keeping its arrays.
            for i in 0..count {
                let (p, n) = (widen(mesh.positions[i]), widen(mesh.normals[i]));
                mesh.positions[i] = narrow(self.point(p));
                if let Some(normal) = mesh.normals.get_mut(i) {
                    *normal = narrow(self.normal(p, n));
                }
            }
            return;
        }
        let coloured = mesh.colors.len() == count;
        let source = std::mem::take(mesh);
        let corner = |i: usize| Corner {
            p: widen(source.positions[i]),
            n: widen(source.normals[i]),
            uv: DVec2::new(f64::from(source.uvs[i][0]), f64::from(source.uvs[i][1])),
            color: if coloured {
                widen(source.colors[i])
            } else {
                DVec3::ZERO
            },
            cap: false,
        };
        let mut out = Mesh {
            positions: Vec::with_capacity(count),
            normals: Vec::with_capacity(count),
            uvs: Vec::with_capacity(count),
            colors: Vec::new(),
            ..Mesh::default()
        };
        let triangles =
            (0..count / 3).map(|t| [corner(3 * t), corner(3 * t + 1), corner(3 * t + 2)]);
        self.reshape(triangles, |corner| {
            out.positions.push(narrow(corner.p));
            out.normals.push(narrow(corner.n));
            out.uvs.push([corner.uv.x as f32, corner.uv.y as f32]);
            if coloured {
                out.colors.push(narrow(corner.color));
            }
        });
        out.attributes = source.attributes;
        *mesh = out;
    }

    fn meshes(&self, meshes: &mut KitMeshes<Surface>) {
        for (_, mesh) in meshes.iter_mut() {
            self.mesh(Arc::make_mut(mesh));
        }
    }

    /// Whether a triangle spanning `lo`..`hi` is sliced before it is moved.
    fn slices(&self, lo: DVec3, hi: DVec3) -> bool {
        (hi - lo).max_element() > SAG_STEP && self.sliced_axes(lo, hi) != [None; 2]
    }

    /// The stretches along x and z where the sags reaching the box `lo`..`hi`
    /// (build frame) vary sharply: along the wall under an eave or gable sag, both
    /// ways round a dip. Triangles there are sliced before they are moved, so a sag
    /// bends the roof and the wall over a breach instead of sliding past their long
    /// faces.
    fn sliced_axes(&self, lo: DVec3, hi: DVec3) -> [Option<(f64, f64)>; 2] {
        let mut axes: [Option<(f64, f64)>; 2] = [None; 2];
        let mut add = |axis: usize, from: f64, to: f64| {
            axes[axis] = Some(axes[axis].map_or((from, to), |(a, b)| (a.min(from), b.max(to))));
        };
        for sag in self.sags.iter().filter(|sag| sag.reaches(lo, hi)) {
            match *sag {
                Sag::Wall {
                    out, center, half, ..
                } => add(usize::from(out.x.abs() > 0.5), center - half, center + half),
                Sag::Dip { center, radius, .. } => {
                    add(0, center.x - radius, center.x + radius);
                    add(1, center.y - radius, center.y + radius);
                }
            }
        }
        axes
    }

    /// How far the sags lower a point.
    fn drop(&self, p: DVec3) -> f64 {
        self.sags.iter().map(|sag| sag.drop_at(p)).sum()
    }

    /// A normal `n` at `p` turned with the sags' slope there, so a sagging roof is
    /// shaded as one smooth bent sheet rather than facets.
    fn normal(&self, p: DVec3, n: DVec3) -> DVec3 {
        if self.sags.is_empty() {
            return n;
        }
        const EPS: f64 = 0.05;
        let slope =
            |axis: DVec3| (self.drop(p + axis * EPS) - self.drop(p - axis * EPS)) / (2.0 * EPS);
        let (fx, fz) = (slope(DVec3::X), slope(DVec3::Z));
        (n + DVec3::new(fx, 0.0, fz) * n.y).normalize_or(n)
    }

    /// Slice `triangles` where a sag reaches them along a fixed grid of planes
    /// ([`SAG_STEP`] apart across x and z), move every corner as the frame gave and
    /// turn its normal with the sag, handing the corners to `emit` in order. The cut
    /// points on an edge depend on the edge alone, so the bent surface keeps no
    /// cracks between neighbouring triangles.
    fn reshape(
        &self,
        triangles: impl IntoIterator<Item = [Corner; 3]>,
        mut emit: impl FnMut(Corner),
    ) {
        let mut emit_moved = |corner: Corner| {
            emit(Corner {
                p: self.point(corner.p),
                n: self.normal(corner.p, corner.n),
                ..corner
            });
        };
        let mut pieces: Vec<Vec<Corner>> = Vec::new();
        for t in triangles {
            let lo = t[0].p.min(t[1].p).min(t[2].p);
            let hi = t[0].p.max(t[1].p).max(t[2].p);
            let axes = self.sliced_axes(lo, hi);
            if !self.slices(lo, hi) {
                t.into_iter().for_each(&mut emit_moved);
                continue;
            }
            pieces.clear();
            pieces.push(t.to_vec());
            for (axis, range) in [(0, axes[0]), (2, axes[1])] {
                let Some((from, to)) = range else {
                    continue;
                };
                // Only across the sag's own stretch: beyond it nothing bends.
                let first = (lo[axis].max(from - SAG_STEP) / SAG_STEP).floor() as i64 + 1;
                let last = (hi[axis].min(to + SAG_STEP) / SAG_STEP).ceil() as i64 - 1;
                for k in first..=last.min(first + MAX_SLICES) {
                    let cut = k as f64 * SAG_STEP;
                    let mut next = Vec::with_capacity(pieces.len() + 2);
                    for polygon in pieces.drain(..) {
                        let (below, above) = slice(&polygon, axis, cut);
                        next.extend([below, above].into_iter().filter(|p| p.len() >= 3));
                    }
                    pieces = next;
                }
            }
            for polygon in &pieces {
                for i in 1..polygon.len() - 1 {
                    for corner in [polygon[0], polygon[i], polygon[i + 1]] {
                        emit_moved(corner);
                    }
                }
            }
        }
    }
}

/// A convex polygon cut by the plane where coordinate `axis` is `cut`: its parts
/// below and above, each keeping the polygon's winding.
fn slice(polygon: &[Corner], axis: usize, cut: f64) -> (Vec<Corner>, Vec<Corner>) {
    let (mut below, mut above) = (Vec::new(), Vec::new());
    for i in 0..polygon.len() {
        let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
        let (da, db) = (a.p[axis] - cut, b.p[axis] - cut);
        if da <= 0.0 {
            below.push(a);
        }
        if da >= 0.0 {
            above.push(a);
        }
        if (da < 0.0 && db > 0.0) || (da > 0.0 && db < 0.0) {
            let m = a.lerp(&b, da / (da - db));
            below.push(m);
            above.push(m);
        }
    }
    (below, above)
}

impl Sag {
    /// Whether it could move anything in the box `lo`..`hi` (build frame).
    fn reaches(&self, lo: DVec3, hi: DVec3) -> bool {
        match *self {
            Sag::Wall {
                out,
                center,
                half,
                from,
                ..
            } => {
                let axis = DVec3::new(out.z.abs(), 0.0, out.x.abs());
                let (a, b) = (lo.dot(axis), hi.dot(axis));
                let outward = hi.dot(out.max(DVec3::ZERO)) - lo.dot((-out).max(DVec3::ZERO));
                a.min(b) < center + half && a.max(b) > center - half && outward > 0.0 && hi.y > from
            }
            Sag::Dip {
                center,
                radius,
                above,
                ..
            } => {
                let nearest = center.clamp(DVec2::new(lo.x, lo.z), DVec2::new(hi.x, hi.z));
                hi.y >= above && (nearest - center).length() < radius
            }
        }
    }

    fn drop_at(&self, p: DVec3) -> f64 {
        match *self {
            Sag::Wall {
                out,
                center,
                half,
                drop,
                from,
                span,
            } => {
                // How far out toward the breached wall (1 at its line and past it, to
                // the eave), along it from the breach, and up from the breach's top.
                let outward = (p.dot(out) / span).clamp(0.0, 1.0);
                let along = p.dot(DVec3::new(out.z.abs(), 0.0, out.x.abs())) - center;
                let near = (PI / 2.0 * (along / half).clamp(-1.0, 1.0)).cos().powi(2);
                let up = ((p.y - from) / 0.35).clamp(0.0, 1.0);
                drop * outward.powf(1.5) * near * up
            }
            Sag::Dip {
                center,
                radius,
                drop,
                above,
            } => {
                if p.y < above {
                    return 0.0;
                }
                let r = (DVec2::new(p.x, p.z) - center).length() / radius;
                if r >= 1.0 {
                    0.0
                } else {
                    drop * (1.0 - r * r).powi(2)
                }
            }
        }
    }
}

/// Patches where a shelled roof has shed its shingles or panels down to the
/// boards: round each hole a blast tore in it, and at the eave over a breach that
/// reaches the top of its wall, on the side slopes clear of the chimney, dormers and
/// every hole, with loose shingles lying about them and fallen into the yard. A
/// roof no blast reached sheds nothing.
fn shed_shingles(parts: &mut Parts, design: &HouseDesign, roof: &Roof, roof_holes: &[RoofHole]) {
    let mut rng = Random::new(f64::from(design.variant) * 11.0 + 401.0);
    let flue = design.flue();
    let dormers = design.dormers();
    let rise = (1.0 + roof.pitch * roof.pitch).sqrt();
    // Where each patch is wanted (plan x, z on a side slope) and its radius.
    let mut wanted: Vec<(f64, f64, f64)> = Vec::new();
    for hole in roof_holes.iter().filter(|hole| hole.on_side()) {
        for k in 0..SHED_ROUND_HOLE {
            let r = rng.range(SHED_RADIUS.0, SHED_RADIUS.1);
            let angle = (k as f64 + rng.range(0.0, 0.5)) * PI * 2.0 / SHED_ROUND_HOLE as f64;
            let along = hole.along + angle.sin() * (hole.half_along + r * 0.8);
            let across = hole.across + angle.cos() * (hole.half_across + r * 0.8);
            wanted.push((hole.side * (roof.eave_x - along / rise), across, r));
        }
    }
    // Steel sheet does not shake loose over a breach; shingles do.
    let shingled = design.style.cover == RoofCover::Shingles;
    for (index, side) in [(2, 1.0), (3, -1.0)].into_iter().filter(|_| shingled) {
        let (frame, _) = wall_frames(design.w, design.d)[index];
        for hole in &parts.holes[index] {
            let [u0, _, u1, y1] = hole.bounds;
            if u1 - u0 < 1.0 || y1 < roof.wall - 0.5 {
                continue;
            }
            let r = SHED_RADIUS.0;
            let z = frame
                .transform_point3(DVec3::new((u0 + u1) / 2.0, 0.0, 0.0))
                .z;
            wanted.push((side * (roof.eave_x - r - 0.15), z, r));
        }
    }
    let mut placed = Vec::new();
    for (x, z, r) in wanted {
        let side = x.signum();
        // Wholly on its side slope, below the ridge and above the eave.
        let hip_room = x.abs() - (z.abs() - roof.ridge_half);
        let clear_flue = DVec2::new(x - flue.x, z - flue.z).length() > r + CHIMNEY;
        let clear_dormers = dormers
            .iter()
            .all(|&(s, dz)| s != side || (dz - z).abs() > DORMER_HALF + r + 0.2);
        let clear_holes = roof_holes.iter().all(|hole| {
            !hole.on_side()
                || hole.side != side
                || (hole.plan - DVec2::new(x, z)).length() > hole.reach() + r * 0.9
        });
        if x.abs() < r + 0.25
            || x.abs() > roof.eave_x - r - 0.1
            || z.abs() > roof.rake_z - r - 0.1
            || hip_room < r + 0.2
            || !clear_flue
            || !clear_dormers
            || !clear_holes
        {
            continue;
        }
        placed.push((side, z));
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
        // Weathered boards round torn, tarred felt, and on a shelled roof the
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
    // Shingles fallen off the eave below each patch into the yard.
    if let Some(base) = parts.base() {
        for (side, from) in placed.into_iter().flat_map(|spot| [spot; 3]) {
            let x = side * (design.w / 2.0 + rng.range(0.25, 1.1));
            let z = (from + rng.range(-0.8, 0.8)).clamp(-design.d / 2.0, design.d / 2.0);
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

/// How far (m) a torn-off gutter's free end hangs at most.
const HANGING_GUTTER: f64 = 0.9;

/// The stretch of eave (build-frame z from, to) on the `side` slope over the widest
/// breach that reaches the top of the eave wall under it, if any.
fn torn_eave(parts: &Parts, w: f64, d: f64, roof: &Roof, side: f64) -> Option<(f64, f64)> {
    let index = if side > 0.0 { 2 } else { 3 };
    let (frame, _) = wall_frames(w, d)[index];
    parts.holes[index]
        .iter()
        .filter(|hole| hole.big() && hole.bounds[3] >= roof.wall - 0.5)
        .map(|hole| {
            let [u0, _, u1, _] = hole.bounds;
            let (a, b) = (
                frame.transform_point3(DVec3::new(u0, 0.0, 0.0)).z,
                frame.transform_point3(DVec3::new(u1, 0.0, 0.0)).z,
            );
            (a.min(b), a.max(b))
        })
        .max_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0)))
        .map(|(a, b)| (a.max(-roof.rake_z + 0.2), b.min(roof.rake_z - 0.2)))
        .filter(|(a, b)| b - a > 0.4)
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

/// A polygon seen from behind its plane: mirrored across u, its winding kept.
fn mirrored(polygon: &[DVec2]) -> Vec<DVec2> {
    polygon
        .iter()
        .rev()
        .map(|p| DVec2::new(-p.x, p.y))
        .collect()
}

/// A breach through a wall face, in the wall frame: where its shells struck and how
/// far it has torn from there.
#[derive(Clone, Debug)]
struct Hole {
    /// Where along the wall it was struck.
    u: f64,
    /// Its extent `[u0, y0, u1, y1]`: grown from the strike with every shell, stopped
    /// short of the openings it has not torn out, the corners, the other breaches
    /// and the wall's top.
    bounds: [f64; 4],
    /// The courses it tore through, bottom up, as `(y0, y1, left, right)`: its
    /// outline, lining, board ends and stripped siding all follow these.
    rows: Vec<(f64, f64, f64, f64)>,
    /// Its mark's size: how many shells it took.
    size: f64,
    seed: f64,
    /// The height its outline steps by: a course of the wall it is torn through.
    course: f64,
}

impl Hole {
    fn center(&self) -> DVec2 {
        let [u0, y0, u1, y1] = self.bounds;
        DVec2::new(u0 + u1, y0 + y1) / 2.0
    }

    fn half(&self) -> DVec2 {
        let [u0, y0, u1, y1] = self.bounds;
        DVec2::new(u1 - u0, y1 - y0) / 2.0
    }

    fn area(&self) -> f64 {
        let half = self.half();
        // The torn courses round off its corners.
        4.0 * half.x * half.y * 0.8
    }

    fn big(&self) -> bool {
        self.area() >= BIG_BREACH
    }

    /// How far its box lies from a rectangle `[u0, y0, u1, y1]` (0 if they touch).
    fn gap(&self, rect: [f64; 4]) -> f64 {
        let [u0, y0, u1, y1] = self.bounds;
        let du = (rect[0] - u1).max(u0 - rect[2]).max(0.0);
        let dy = (rect[1] - y1).max(y0 - rect[3]).max(0.0);
        du.hypot(dy)
    }
}

/// A rectangle in a wall frame that holes keep clear of: (u0, y0, u1, y1).
type Keepout = [f64; 4];

fn clear_of(keepouts: &[Keepout], u: f64, y: f64, radius: f64) -> bool {
    let r = radius + HOLE_MARGIN;
    keepouts
        .iter()
        .all(|k| u + r <= k[0] || u - r >= k[2] || y + r <= k[1] || y - r >= k[3])
}

/// The box a breach struck at `center` tears to, reaching `reach` either way, within
/// `limits` and stopped [`HOLE_MARGIN`] short of every keepout: each it would overlap
/// cuts back whichever side of the box (one the strike lies beyond) keeps the most
/// of it. None if too little is left.
fn hole_bounds(
    center: DVec2,
    reach: DVec2,
    limits: [f64; 4],
    keepouts: &[Keepout],
) -> Option<[f64; 4]> {
    let mut b = [
        (center.x - reach.x).max(limits[0]),
        (center.y - reach.y).max(limits[1]),
        (center.x + reach.x).min(limits[2]),
        (center.y + reach.y).min(limits[3]),
    ];
    let m = HOLE_MARGIN;
    for k in keepouts {
        let overlaps = b[0] < k[2] + m && b[2] > k[0] - m && b[1] < k[3] + m && b[3] > k[1] - m;
        if !overlaps {
            continue;
        }
        let area = |b: &[f64; 4]| (b[2] - b[0]).max(0.0) * (b[3] - b[1]).max(0.0);
        let cuts = [
            (center.x <= k[0] - m).then(|| [b[0], b[1], b[2].min(k[0] - m), b[3]]),
            (center.x >= k[2] + m).then(|| [b[0].max(k[2] + m), b[1], b[2], b[3]]),
            (center.y <= k[1] - m).then(|| [b[0], b[1], b[2], b[3].min(k[1] - m)]),
            (center.y >= k[3] + m).then(|| [b[0], b[1].max(k[3] + m), b[2], b[3]]),
        ];
        b = cuts
            .into_iter()
            .flatten()
            .max_by(|a, c| area(a).total_cmp(&area(c)))?;
    }
    (b[2] - b[0] >= 2.0 * HOLE_LEAST * 0.8 && b[3] - b[1] >= 2.0 * HOLE_LEAST * 0.8).then_some(b)
}

/// The breaches the marks on one wall tore through it: each mark is carried into the
/// build frame and given to the wall it struck, where its breach grows from the
/// strike with the mark's size, clear of the door, the trim and the breaches before
/// it. A strike on a window (or one with no room beside it) blows that window in, and
/// its breach opens beside it if there is room. A breach of [`SWALLOW`] shells or more
/// tears out the windows it reaches, growing round them. Returns the breaches, the
/// windows blown in and the windows torn out.
#[allow(clippy::too_many_arguments)]
fn wall_holes(
    design: &HouseDesign,
    hits: &[TimberHit],
    index: usize,
    half: f64,
    top: f64,
    windows: &[Opening],
    fixed: &[Keepout],
) -> (Vec<Hole>, Vec<usize>, Vec<usize>) {
    let along = crate::sim::house_layout::wall_frames(design.w, design.d)[index].2;
    let course = wall_course(design);
    let mut holes: Vec<Hole> = Vec::new();
    // Windows a shell blew in, and windows a breach tore out.
    let mut blasted = Vec::new();
    let mut swallowed: Vec<usize> = Vec::new();
    let reach = half - CORNER_BOARD - HOLE_MARGIN;
    let limits = [-reach, PLINTH + 0.12, reach, top - 0.12];
    let window_keepout = |o: &Opening| {
        let reach = o.width / 2.0 + CASING + 0.04;
        [o.u - reach, o.y - 0.16, o.u + reach, o.top() + 0.2]
    };
    let window_rect = |o: &Opening| [o.u - o.width / 2.0, o.y, o.u + o.width / 2.0, o.top()];
    let overlaps = |a: [f64; 4], b: [f64; 4]| {
        a[0] < b[2] + HOLE_MARGIN
            && a[2] > b[0] - HOLE_MARGIN
            && a[1] < b[3] + HOLE_MARGIN
            && a[3] > b[1] - HOLE_MARGIN
    };
    for (i, hit) in hits.iter().enumerate() {
        if design.struck_part(hit).wall_index() != Some(index) {
            continue;
        }
        let local = design.to_local(DVec3::new(hit.x, hit.y, hit.z));
        let struck = DVec2::new(local.dot(along), local.y);
        // A strike on a window (or close by) blows it in.
        let nearest = windows.iter().enumerate().min_by(|(_, a), (_, b)| {
            let at = |o: &Opening| DVec2::new(o.u, o.y + o.height / 2.0).distance(struck);
            at(a).total_cmp(&at(b))
        });
        let on_window = windows.iter().position(|o| {
            (struck.x - o.u).abs() < o.width / 2.0 + CASING + 0.1
                && struck.y > o.y - 0.2
                && struck.y < o.top() + 0.2
        });
        if let Some(k) = on_window.filter(|k| !blasted.contains(k)) {
            blasted.push(k);
        }
        let grown = DVec2::new(HOLE_REACH.0, HOLE_REACH.1) * hit.size;
        let tears_out = hit.size >= SWALLOW;
        // What this breach must stay clear of: the door and trim, the breaches before
        // it and the windows it cannot tear out (all of them before [`SWALLOW`]).
        let mut keepouts: Vec<Keepout> = fixed.to_vec();
        keepouts.extend(holes.iter().map(|hole| hole.bounds));
        let standing: Vec<usize> = (0..windows.len())
            .filter(|k| !swallowed.contains(k))
            .collect();
        let obstacles = |keepouts: &[Keepout], free: &[usize]| {
            let mut all = keepouts.to_vec();
            all.extend(
                standing
                    .iter()
                    .filter(|k| !free.contains(k))
                    .map(|&k| window_keepout(&windows[k])),
            );
            all
        };
        let least = HOLE_LEAST;
        let (low, high) = (limits[1] + least, limits[3] - least);
        let start = DVec2::new(
            struck.x.clamp(-reach + least, reach - least),
            struck.y.clamp(low, high.max(low)),
        );
        let free: Vec<usize> = if tears_out {
            standing.clone()
        } else {
            Vec::new()
        };
        let blocking = obstacles(&keepouts, &free);
        // Where a first shell's breach fits, nearest the strike.
        let spot = (0..25).find_map(|k| {
            let step = f64::from((k + 1) / 2) * 0.16 * if k % 2 == 0 { 1.0 } else { -1.0 };
            [0.0, 0.4, -0.4].into_iter().find_map(|dy| {
                let at = DVec2::new(start.x + step, (start.y + dy).clamp(low, high.max(low)));
                (at.x.abs() <= reach - least && clear_of(&blocking, at.x, at.y, least))
                    .then_some(at)
            })
        });
        let Some(at) = spot else {
            if let Some((k, _)) = nearest.filter(|(k, _)| !blasted.contains(k)) {
                blasted.push(k);
            }
            continue;
        };
        // Tear out each window the breach reaches, growing round it, unless that
        // would run into the door, the trim or another breach; a window it cannot
        // take holds, and the breach grows again stopping short of it.
        let mut held: Vec<usize> = standing
            .iter()
            .copied()
            .filter(|k| !free.contains(k))
            .collect();
        let mut taken: Vec<usize> = Vec::new();
        let mut bounds = None;
        for _ in 0..=windows.len() {
            taken.clear();
            let mut blocking = keepouts.clone();
            blocking.extend(held.iter().map(|&k| window_keepout(&windows[k])));
            let Some(mut b) = hole_bounds(at, grown, limits, &blocking) else {
                break;
            };
            let mut gave = None;
            while let Some(&k) = standing.iter().find(|&&k| {
                !held.contains(&k)
                    && !taken.contains(&k)
                    && overlaps(b, window_keepout(&windows[k]))
            }) {
                let w = window_rect(&windows[k]);
                let margin = DVec2::new(0.1, course + 0.06);
                let wider = [
                    b[0].min(w[0] - margin.x).max(limits[0]),
                    b[1].min(w[1] - margin.y).max(limits[1]),
                    b[2].max(w[2] + margin.x).min(limits[2]),
                    b[3].max(w[3] + margin.y).min(limits[3]),
                ];
                let contains = wider[0] <= w[0] - 0.08
                    && wider[1] <= w[1] - 0.04
                    && wider[2] >= w[2] + 0.08
                    && wider[3] >= w[3] + 0.04;
                let clear = blocking.iter().all(|&r| !overlaps(wider, r));
                if contains && clear {
                    taken.push(k);
                    b = wider;
                } else {
                    gave = Some(k);
                    break;
                }
            }
            match gave {
                Some(k) => held.push(k),
                None => {
                    bounds = Some(b);
                    break;
                }
            }
        }
        let Some(bounds) = bounds else {
            if let Some((k, _)) = nearest.filter(|(k, _)| !blasted.contains(k)) {
                blasted.push(k);
            }
            continue;
        };
        let seed = f64::from(design.variant) * 7.0 + i as f64 * 131.0 + f64::from(index as u32);
        let mut rows = torn_courses(&mut Random::new(seed), bounds, course);
        // The courses across a torn-out window run past its casing on both sides, so
        // the window's whole opening lies inside the breach.
        for &k in &taken {
            let w = window_rect(&windows[k]);
            for row in rows.iter_mut() {
                if row.1 > w[1] - 0.06 && row.0 < w[3] + 0.06 {
                    row.2 = row.2.min(w[0] - 0.09).max(bounds[0]);
                    row.3 = row.3.max(w[2] + 0.09).min(bounds[2]);
                }
            }
        }
        swallowed.extend(taken);
        holes.push(Hole {
            u: at.x,
            bounds,
            rows,
            size: hit.size,
            seed,
            course,
        });
    }
    blasted.retain(|k| !swallowed.contains(k));
    (holes, blasted, swallowed)
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

/// The courses a breach tears through, bottom up, each as `(y0, y1, left, right)`:
/// whole courses within its box, each broken back to its own length either side,
/// longest across the middle and never past the box.
fn torn_courses(rng: &mut Random, bounds: [f64; 4], course: f64) -> Vec<(f64, f64, f64, f64)> {
    let [u0, b0, u1, b1] = bounds;
    let (mut bottom, mut top) = (
        (b0 / course).ceil() * course,
        (b1 / course).floor() * course,
    );
    if top - bottom < course - 1e-9 {
        let (middle, reach) = ((b0 + b1) / 2.0, (b1 - b0) / 2.0 * 0.9);
        (bottom, top) = (middle - reach, middle + reach);
    }
    let rows = ((top - bottom) / course).round().max(1.0) as usize;
    let step = (top - bottom) / rows as f64;
    let (middle, half) = ((u0 + u1) / 2.0, (u1 - u0) / 2.0);
    let (y_middle, y_half) = ((bottom + top) / 2.0, (top - bottom) / 2.0 + step / 2.0);
    // Small breaches round off like a shell's burst; big ones tear out squarer, the
    // courses broken back far along the wall.
    let fullness = if half * y_half > 0.5 { 4 } else { 2 };
    (0..rows)
        .map(|k| {
            let (y0, y1) = (bottom + k as f64 * step, bottom + (k + 1) as f64 * step);
            let mid = ((y0 + y1) / 2.0 - y_middle) / y_half;
            let span = (1.0 - mid.abs().powi(fullness)).max(0.0).sqrt();
            let mut reach = || (half * span * rng.range(0.55, 1.0)).max(0.09);
            let (left, right) = ((middle - reach()).max(u0), (middle + reach()).min(u1));
            (y0, y1, left, right)
        })
        .collect()
}

/// A stepped outline round torn courses, clockwise in the wall plane: each board
/// end snapped a little ragged at its middle.
fn course_outline(rng: &mut Random, rows: &[(f64, f64, f64, f64)]) -> Vec<DVec2> {
    let mut outline = Vec::with_capacity(rows.len() * 6 + 2 * TORN_EDGE_KNOTS);
    // The top and bottom boards are split along their grain, not sawn: their
    // edges bite up into the course over the hole and down into the one under
    // it, so the breach reads torn rather than as a dark rectangle.
    let across =
        |rng: &mut Random, outline: &mut Vec<DVec2>, from: f64, to: f64, y: f64, bite: f64| {
            for k in 1..=TORN_EDGE_KNOTS {
                let f = k as f64 / (TORN_EDGE_KNOTS + 1) as f64;
                let jag = if k % 2 == 1 {
                    rng.range(0.4, 1.0)
                } else {
                    rng.range(-0.2, 0.3)
                };
                outline.push(DVec2::new(from + (to - from) * f, y + bite * jag));
            }
        };
    let (first, last) = (rows[0], rows[rows.len() - 1]);
    let course = last.1 - last.0;
    across(rng, &mut outline, first.2, first.3, first.0, -course * 0.45);
    for &(y0, y1, _, right) in rows {
        outline.push(DVec2::new(right, y0));
        outline.push(DVec2::new(right + rng.range(-0.05, 0.03), (y0 + y1) / 2.0));
        outline.push(DVec2::new(right, y1));
    }
    across(rng, &mut outline, last.3, last.2, last.1, course * 0.55);
    for &(y0, y1, left, _) in rows.iter().rev() {
        outline.push(DVec2::new(left, y1));
        outline.push(DVec2::new(left + rng.range(-0.03, 0.05), (y0 + y1) / 2.0));
        outline.push(DVec2::new(left, y0));
    }
    outline.reverse();
    outline
}

/// Points along a breach's torn top and bottom edges.
const TORN_EDGE_KNOTS: usize = 3;

/// A hole's ragged outline, clockwise in the wall plane: the siding torn back
/// course by course, every board broken off at its own length.
fn hole_outline(hole: &Hole) -> Vec<DVec2> {
    let mut rng = Random::new(hole.seed + 1.0);
    course_outline(&mut rng, &hole.rows)
}

/// Line a shell hole: the torn wall's thickness round its edge, splintered board
/// ends sticking into it course by course, the studs of the frame behind (one
/// snapped), the dark room beyond, soot fanned up the siding above it and
/// splintered boards bent out of the face. A standing wall also drops the boards
/// and plaster the shell knocked out on the ground under it.
fn line_hole(
    kit: &mut Kit<Surface>,
    frame: DMat4,
    hole: &Hole,
    outline: &[DVec2],
    standing: bool,
    room: &WallRoom,
) {
    let at = |p: DVec2, z: f64| frame.transform_point3(DVec3::new(p.x, p.y, z));
    let center = hole.center();
    let half = hole.half();
    let [u0, y0, u1, y1] = hole.bounds;
    let mut rng = Random::new(hole.seed + 5.0);
    let rows = &hole.rows;
    if hole.big() {
        stripped_round(kit, frame, hole, rows, room, &mut rng);
    }
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
                at(b, -PLASTER_LIP),
                at(a, -PLASTER_LIP),
            ],
            facing,
        );
        // The broken plaster behind the charred sheathing, pale where it faces
        // the light round the dark of the room.
        kit.quad(
            Surface::Concrete,
            [
                at(a, -PLASTER_LIP),
                at(b, -PLASTER_LIP),
                at(b, -HOLE_EDGE),
                at(a, -HOLE_EDGE),
            ],
            facing,
        );
    }
    // Soot fanned up the siding over the hole in streaks where the blast vented,
    // barely past its lower edge: a band of about the same width whatever the
    // breach's size, so a big one is not ringed by a black halo.
    let mut soot = Random::new(hole.seed + 9.0);
    let outer: Vec<DVec2> = outline
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let from = p - center;
            let ray = from / from.length().max(1e-6);
            let up = ray.y.max(0.0);
            let flare = if i % 2 == 0 {
                soot.range(0.35, 0.7)
            } else {
                soot.range(0.12, 0.3)
            };
            let band = (soot.range(0.22, 0.45) + up * flare) * 0.5;
            p + ray * band.min(from.length() * 0.6)
        })
        .collect();
    for i in 0..outline.len() {
        let j = (i + 1) % outline.len();
        kit.quad(
            Surface::Soot,
            [
                at(outline[i], SOOT_LIFT),
                at(outline[j], SOOT_LIFT),
                at(outer[j], SOOT_LIFT),
                at(outer[i], SOOT_LIFT),
            ],
            frame.transform_vector3(DVec3::Z),
        );
    }
    // The dark room behind; a collapse piece keeps it as the char inside the
    // wall, seen only from its face, so the cottage standing in its shards still
    // shows its breaches and a fallen panel lying lining up shows the ground. A big
    // breach in a standing wall opens on the room itself: its floor, walls and
    // ceiling, dim under the roof.
    if standing && hole.big() {
        room_behind(kit, frame, hole, room.reach);
    } else {
        // A falling wall's char lies just behind its siding, over the framing and
        // lining inside it, so its breach stays as dark as it stood.
        let (r, back) = if standing {
            (half * 1.3, -HOLE_BACK)
        } else {
            (half * 1.05, -SPLIT_FACE - 0.006)
        };
        kit.quad(
            Surface::Dark,
            [
                at(center + DVec2::new(-r.x, -r.y), back),
                at(center + DVec2::new(r.x, -r.y), back),
                at(center + DVec2::new(r.x, r.y), back),
                at(center + DVec2::new(-r.x, r.y), back),
            ],
            frame.transform_vector3(DVec3::Z),
        );
    }
    // Board ends: a torn course leaves its clapboard snapped off at the outline,
    // still nailed at its root on the wall and curling out of the face toward the
    // blast; painted like the siding, its split tip pale. Some courses keep a long
    // end, some a stub, some none.
    for (k, &(y0, y1, left, right)) in rows.iter().enumerate() {
        for (end, side) in [(right, -1.0), (left, 1.0)] {
            if rng.next() < 0.35 {
                continue;
            }
            let long = rng.next() < 0.3;
            let length = if long {
                rng.range(0.35, 0.6)
            } else {
                rng.range(0.1, 0.25)
            };
            let height = (y1 - y0) * rng.range(0.7, 0.95);
            let curl = rng.range(0.25, 0.65) + 0.03 * (k % 3) as f64;
            // Rooted just outside the outline, so it never floats in the void.
            let root = frame
                * shift(DVec3::new(end - side * 0.03, y0 + (y1 - y0) * 0.5, 0.012))
                * pose(
                    DVec3::ZERO,
                    DVec3::new(0.0, side * curl, rng.range(-0.12, 0.12)),
                );
            kit.block(
                Surface::Siding,
                DVec3::new(length, height, 0.02),
                root * shift(DVec3::new(side * length / 2.0, 0.0, 0.0)),
            );
            if long {
                kit.block(
                    Surface::Splinters,
                    DVec3::new(0.04, height * 0.8, 0.024),
                    root * shift(DVec3::new(side * (length + 0.01), 0.0, 0.0)),
                );
            }
        }
    }
    // The studs behind the siding that the shells struck, snapped, their raw ends
    // bent apart: in a small hole only the one it struck, as more would fill a hole
    // that has to read dark from the battlefield's height; across a big breach each
    // stud it tore through, hanging from the top plate and standing from the sill.
    let first = ((u0 + half.x * 0.2) / STUD_SPACING).ceil() as i32;
    let last = ((u1 - half.x * 0.2) / STUD_SPACING).floor() as i32;
    if last >= first {
        let studs: Vec<i32> = if hole.big() {
            (first..=last).take(SNAPPED_STUDS).collect()
        } else {
            vec![((hole.u / STUD_SPACING).round() as i32).clamp(first, last)]
        };
        for stud in studs {
            let u = f64::from(stud) * STUD_SPACING;
            for (end, lean) in [(1.0, rng.range(-0.3, 0.3)), (-1.0, rng.range(-0.3, 0.3))] {
                let reach = half.y * rng.range(0.35, 0.8);
                let edge = if end > 0.0 { y1 } else { y0 };
                kit.block(
                    Surface::Splinters,
                    DVec3::new(0.09, reach, 0.05),
                    frame
                        * pose(
                            DVec3::new(u, edge - end * reach * 0.4, -HOLE_EDGE * 0.6),
                            DVec3::new(lean * 0.6, 0.0, lean),
                        ),
                );
            }
        }
    }
    // A big breach that reaches the top of its wall leaves the top plate hanging
    // across it, sagging where it lost its studs.
    if hole.big() && standing {
        kit.block(
            Surface::Wood,
            DVec3::new(u1 - u0, 0.1, 0.09),
            frame
                * pose(
                    DVec3::new(center.x, y1 - 0.07, -HOLE_EDGE * 0.5),
                    DVec3::new(0.0, 0.0, rng.range(-0.06, 0.06)),
                ),
        );
    }
    // Exposed lath across the back of a small hole, snapped at its own length.
    if standing && !hole.big() {
        let r = half;
        let mut y = center.y - r.y * 0.7;
        while y < center.y + r.y * 0.7 {
            let left = center.x - r.x * rng.range(0.5, 1.1);
            let right = center.x + r.x * rng.range(-0.3, 0.6);
            if right - left > 0.12 {
                let tilt = rng.range(-0.04, 0.04);
                kit.quad(
                    Surface::Splinters,
                    [
                        at(DVec2::new(left, y - 0.018 - tilt), -HOLE_EDGE + 0.01),
                        at(DVec2::new(right, y - 0.018 + tilt), -HOLE_EDGE + 0.01),
                        at(DVec2::new(right, y + 0.018 + tilt), -HOLE_EDGE + 0.01),
                        at(DVec2::new(left, y + 0.018 - tilt), -HOLE_EDGE + 0.01),
                    ],
                    frame.transform_vector3(DVec3::Z),
                );
            }
            y += rng.range(0.14, 0.24);
        }
    }
    for k in 0..BENT_BOARDS {
        let p = outline[(k * 5 + 2) % outline.len()];
        let inward = (center - p).normalize_or_zero();
        let length = rng.range(0.26, 0.5);
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
        // What the shell knocked out lies under the hole: two lengths of siding
        // fallen against the wall, their tops on the plinth and their feet out on
        // the ground, and broken boards, plaster and a split stud heaped at their
        // feet, so the spill reads as a wall's worth of timber, not chips.
        for side in [-1.0, 1.0] {
            let foot = DVec3::new(
                center.x + side * half.x * rng.range(0.1, 0.6),
                0.02,
                rng.range(0.45, 0.75),
            );
            let top = DVec3::new(
                foot.x + side * rng.range(0.05, 0.3),
                PLINTH + rng.range(0.1, 0.3),
                0.06,
            );
            let length = (top - foot).length();
            let (a, b) = (frame.transform_point3(foot), frame.transform_point3(top));
            kit.block(
                Surface::Siding,
                DVec3::new(hole.course * 0.85, 0.025, length),
                aim(a, b) * shift(DVec3::Z * (length / 2.0)),
            );
        }
        // Every shell on the breach spilled more, further out: a fan of dust and
        // broken siding, plaster and studs that grows with it, dark enough to read
        // from overhead.
        let fan = 0.9 + 0.25 * hole.size;
        let width = half.x + 0.25 * hole.size;
        let mut dust = Random::new(hole.seed + 13.0);
        let ragged: Vec<DVec2> = (0..9)
            .map(|k| {
                let angle = PI * f64::from(k) / 8.0;
                let r = dust.range(0.65, 1.0);
                DVec2::new(
                    center.x + angle.cos() * width * r,
                    0.05 + angle.sin() * fan * r,
                )
            })
            .collect();
        let ground = |p: DVec2| frame.transform_point3(DVec3::new(p.x, SPILL_LIFT, p.y));
        let up = frame.transform_vector3(DVec3::Y);
        let foot = ground(DVec2::new(center.x, 0.05));
        for k in 1..ragged.len() {
            let (a, b) = (ground(ragged[k - 1]), ground(ragged[k]));
            kit.quad(Surface::Soot, [foot, a, b, b], up);
        }
        let spill = (5.0 + hole.size * 2.5) as usize;
        for k in 0..spill.min(MAX_SPILL) {
            let (coat, size) = match k % 5 {
                0 | 1 => (
                    Surface::Splinters,
                    DVec3::new(rng.range(0.35, 0.6), 0.05, 0.07),
                ),
                2 => (
                    Surface::Concrete,
                    DVec3::new(rng.range(0.18, 0.3), 0.09, rng.range(0.14, 0.24)),
                ),
                3 => (
                    Surface::Siding,
                    DVec3::new(rng.range(0.4, 0.7), 0.025, hole.course * 0.8),
                ),
                _ => (Surface::Wood, DVec3::new(0.09, 0.06, rng.range(0.5, 0.8))),
            };
            kit.block(
                coat,
                size,
                frame
                    * pose(
                        DVec3::new(
                            center.x + rng.range(-0.9, 0.9) * width,
                            size.y / 2.0 + rng.range(0.01, 0.08),
                            rng.range(0.25, 0.9) * fan,
                        ),
                        DVec3::new(
                            rng.range(-0.3, 0.3),
                            rng.range(0.0, PI),
                            rng.range(-0.3, 0.3),
                        ),
                    ),
            );
        }
    }
}

/// Where a wall's breaches may strip its siding: the wall's reach either side and the
/// openings and other breaches they stop short of.
struct WallRoom {
    reach: f64,
    top: f64,
    keepouts: Vec<Keepout>,
}

impl WallRoom {
    /// The furthest a strip along `y0..y1` may run left and right of `u`.
    fn span(&self, u: f64, y0: f64, y1: f64) -> (f64, f64) {
        let (mut left, mut right) = (-self.reach, self.reach);
        for k in &self.keepouts {
            if y1 <= k[1] - HOLE_MARGIN || y0 >= k[3] + HOLE_MARGIN {
                continue;
            }
            if k[2] <= u {
                left = left.max(k[2] + HOLE_MARGIN);
            } else if k[0] >= u {
                right = right.min(k[0] - HOLE_MARGIN);
            } else {
                return (u, u);
            }
        }
        (left, right)
    }
}

/// How far in front of the siding a breach's soot lies, and its stripped sheathing
/// under that.
const SOOT_LIFT: f64 = 0.008;
/// The dust fan under a breach lies this far over the ground, and a breach spills at
/// most this many pieces.
const SPILL_LIFT: f64 = 0.012;
const MAX_SPILL: usize = 16;
const STRIPPED_LIFT: f64 = 0.004;

/// The siding a big breach stripped back round it, down to the weathered sheathing
/// boards (or, on brick, the render behind): each torn course of the breach bared a
/// little further along, and a couple of courses over and under it, clear of the
/// openings and the other breaches.
fn stripped_round(
    kit: &mut Kit<Surface>,
    frame: DMat4,
    hole: &Hole,
    rows: &[(f64, f64, f64, f64)],
    room: &WallRoom,
    rng: &mut Random,
) {
    let bare = if hole.course < CLAPBOARD_COURSE - 1e-9 {
        Surface::Concrete
    } else {
        Surface::Wood
    };
    let at = |u: f64, y: f64| frame.transform_point3(DVec3::new(u, y, STRIPPED_LIFT));
    let facing = frame.transform_vector3(DVec3::Z);
    let center = hole.center().x;
    let strip = |kit: &mut Kit<Surface>, from: f64, to: f64, y0: f64, y1: f64| {
        let (left, right) = room.span(center, y0, y1);
        let (from, to) = (from.max(left), to.min(right));
        if to - from > 0.06 {
            kit.quad(
                bare,
                [at(from, y0), at(to, y0), at(to, y1), at(from, y1)],
                facing,
            );
        }
    };
    // Beside each torn course (clear of its ragged end).
    const END: f64 = 0.035;
    for &(y0, y1, left, right) in rows {
        let reach = |rng: &mut Random| rng.range(STRIPPED_REACH.0, STRIPPED_REACH.1);
        strip(kit, left - END - reach(rng), left - END, y0, y1);
        strip(kit, right + END, right + END + reach(rng), y0, y1);
    }
    // Over and under it: the course next to the breach is bitten into across the
    // breach's width, so it is stripped only beyond it; the next ones right across.
    let (first, last) = (rows[0], rows[rows.len() - 1]);
    for (row, edge, toward) in [(last, last.1, 1.0), (first, first.0, -1.0)] {
        let (_, _, left, right) = row;
        for k in 0..STRIPPED_ROWS {
            let (a, b) = (
                edge + toward * k as f64 * hole.course,
                edge + toward * (k + 1) as f64 * hole.course,
            );
            let (y0, y1) = (a.min(b), a.max(b));
            if y0 < PLINTH + 0.05 || y1 > room.top - 0.05 {
                break;
            }
            let shrink = 1.0 - 0.35 * k as f64;
            let width =
                (right - left) / 2.0 + rng.range(STRIPPED_REACH.0, STRIPPED_REACH.1) * shrink;
            let (from, to) = (center - width, center + width);
            if k == 0 {
                strip(kit, from, left - END, y0, y1);
                strip(kit, right + END, to, y0, y1);
            } else {
                strip(kit, from, to, y0, y1);
            }
        }
    }
}

/// The room behind a big breach: a dim box as deep as [`ROOM_DEPTH`], its floor
/// boards catching what light comes through, so the breach reads as a way into a
/// house rather than a black stencil, and nothing beyond it shows through.
fn room_behind(kit: &mut Kit<Surface>, frame: DMat4, hole: &Hole, reach: f64) {
    let [u0, _, u1, y1] = hole.bounds;
    // Its side walls stay inside the walls either side of it, so a breach by a
    // corner shows no dark panel outside the cottage.
    let inside = reach - WALL_DEPTH;
    let (l, r) = ((u0 - 0.25).max(-inside), (u1 + 0.25).min(inside));
    let (floor, ceiling) = (PLINTH + 0.02, y1 + 0.2);
    let (near, far) = (-HOLE_EDGE, -ROOM_DEPTH);
    let at = |u: f64, y: f64, z: f64| frame.transform_point3(DVec3::new(u, y, z));
    let facing = |x: f64, y: f64, z: f64| frame.transform_vector3(DVec3::new(x, y, z));
    // The far wall's plaster, dim in the shade of the roof.
    kit.quad(
        Surface::Concrete,
        [
            at(l, floor, far),
            at(r, floor, far),
            at(r, ceiling, far),
            at(l, ceiling, far),
        ],
        facing(0.0, 0.0, 1.0),
    );
    kit.quad(
        Surface::Wood,
        [
            at(l, floor, near),
            at(r, floor, near),
            at(r, floor, far),
            at(l, floor, far),
        ],
        facing(0.0, 1.0, 0.0),
    );
    kit.quad(
        Surface::Dark,
        [
            at(l, ceiling, far),
            at(r, ceiling, far),
            at(r, ceiling, near),
            at(l, ceiling, near),
        ],
        facing(0.0, -1.0, 0.0),
    );
    for (u, inward) in [(l, 1.0), (r, -1.0)] {
        kit.quad(
            Surface::Dark,
            [
                at(u, floor, near),
                at(u, floor, far),
                at(u, ceiling, far),
                at(u, ceiling, near),
            ],
            facing(inward, 0.0, 0.0),
        );
    }
    // A fallen board or two on the floor inside.
    let mut rng = Random::new(hole.seed + 21.0);
    for _ in 0..2 {
        kit.block(
            Surface::Splinters,
            DVec3::new(rng.range(0.4, 0.8), 0.04, 0.12),
            frame
                * pose(
                    DVec3::new(
                        rng.range(l.max(u0), r.min(u1)),
                        floor + 0.03,
                        rng.range(far + 0.3, near - 0.2),
                    ),
                    DVec3::new(0.0, rng.range(0.0, PI), rng.range(-0.1, 0.1)),
                ),
        );
    }
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

/// A boarded window a breach reached: one board still hangs from a nail by its
/// casing, swung down across the dark opening, and the others lie split on the
/// ground below.
fn torn_boards(kit: &mut Kit<Surface>, frame: DMat4, opening: &Opening, seed: f64) {
    let mut rng = Random::new(seed + 29.0);
    let (u, w) = (opening.u, opening.width);
    let length = w + 2.0 * CASING + rng.range(0.05, 0.2);
    let side = if rng.next() < 0.5 { -1.0 } else { 1.0 };
    let nail = DVec3::new(
        u + side * (w / 2.0 + CASING * 0.5),
        opening.top() - 0.12,
        0.03 + PLANK / 2.0,
    );
    kit.block(
        Surface::Wood,
        DVec3::new(length, 0.15, PLANK),
        frame
            * shift(nail)
            * pose(
                DVec3::ZERO,
                DVec3::new(0.0, rng.range(-0.1, 0.1), side * rng.range(1.05, 1.35)),
            )
            * shift(DVec3::new(-side * length / 2.0, 0.0, 0.0)),
    );
    for _ in 0..2 {
        kit.block(
            Surface::Wood,
            DVec3::new(length * rng.range(0.5, 0.9), PLANK, 0.15),
            frame
                * pose(
                    DVec3::new(
                        u + rng.range(-0.5, 0.5),
                        PLANK / 2.0 + 0.01,
                        rng.range(0.35, 0.8),
                    ),
                    DVec3::new(0.0, rng.range(-0.6, 0.6), rng.range(-0.08, 0.08)),
                ),
        );
    }
}

/// A window a breach tore out of its wall: its frame lying smashed face up on the
/// ground below, a stile split off and shards of its glass about it.
fn fallen_window(kit: &mut Kit<Surface>, frame: DMat4, opening: &Opening, seed: f64) {
    const STILE: f64 = 0.07;
    let mut rng = Random::new(seed + 41.0);
    let (w, h) = (opening.width + 0.12, opening.height + 0.12);
    let lying = frame
        * pose(
            DVec3::new(
                opening.u + rng.range(-0.3, 0.3),
                0.06,
                0.6 + h / 2.0 * rng.range(0.55, 0.85),
            ),
            DVec3::new(-PI / 2.0 + rng.range(0.06, 0.2), rng.range(-0.5, 0.5), 0.0),
        );
    for (size, at, roll) in [
        (
            DVec3::new(STILE, h, 0.06),
            DVec3::new(-w / 2.0, 0.0, 0.0),
            0.0,
        ),
        (
            DVec3::new(STILE, h * 0.7, 0.06),
            DVec3::new(w / 2.0 + 0.08, -h * 0.12, 0.0),
            0.12,
        ),
        (
            DVec3::new(w, STILE, 0.06),
            DVec3::new(0.0, h / 2.0, 0.0),
            0.0,
        ),
        (
            DVec3::new(w, STILE, 0.06),
            DVec3::new(0.0, -h / 2.0, 0.0),
            -0.05,
        ),
        (
            DVec3::new(w * 0.9, 0.045, 0.05),
            DVec3::new(0.0, 0.05, 0.0),
            0.2,
        ),
    ] {
        kit.block(
            Surface::Trim,
            size,
            lying * pose(at, DVec3::new(0.0, 0.0, roll)),
        );
    }
    let up = frame.transform_vector3(DVec3::Y);
    for _ in 0..3 {
        let c = DVec3::new(opening.u + rng.range(-0.7, 0.7), 0.012, rng.range(0.3, 1.2));
        let (a, b) = (rng.range(0.0, PI * 2.0), rng.range(0.12, 0.25));
        let corner = |angle: f64, r: f64| {
            frame.transform_point3(c + DVec3::new(angle.cos() * r, 0.0, angle.sin() * r))
        };
        let p = [
            corner(a, b),
            corner(a + 2.1, b * 0.8),
            corner(a + 4.0, b * 0.6),
        ];
        kit.quad(Surface::Glass, [p[0], p[1], p[2], p[2]], up);
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

/// The fieldstone plinth and the steps up to the door: a cottage's first section
/// ([`PartsCut`]).
fn house_base(parts: &mut Parts, design: &HouseDesign) {
    let (w, d) = (design.w, design.d);
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
}

/// Wall `index` with its openings, holes, trim and dressing: one of a cottage's
/// sections ([`PartsCut`]).
#[allow(clippy::too_many_arguments)]
fn house_wall(
    parts: &mut Parts,
    design: &HouseDesign,
    roof: &Roof,
    build: Build,
    hits: &[TimberHit],
    rng: &mut Random,
    index: usize,
) {
    let standing = build == Build::Standing;
    let (frame, half) = wall_frames(design.w, design.d)[index];
    let door_opening = Opening {
        u: 0.0,
        y: PLINTH,
        width: DOOR_WIDTH,
        height: DOOR_HEIGHT,
    };
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
        // The canopy or porch roof over the door; the door's own casing is kept
        // clear of like a window's, so a breach can tear the wall beside it.
        keepouts.push([-0.8, door_opening.top(), 0.8, door_opening.top() + 0.75]);
    }
    if two_storey {
        let belt = belt_line(roof);
        keepouts.push([-half, belt - BELT, half, belt + BELT]);
    }
    let (holes, blasted, torn_out) = wall_holes(
        design,
        hits,
        index,
        half,
        roof.wall,
        &windows,
        &keepouts[windows.len()..],
    );
    // A window a breach tore out leaves no opening of its own: the breach is it.
    let openings: Vec<Opening> = openings
        .into_iter()
        .enumerate()
        .filter(|(k, _)| !torn_out.contains(k))
        .map(|(_, opening)| opening)
        .collect();
    let keepouts: Vec<Keepout> = keepouts
        .into_iter()
        .enumerate()
        .filter(|(k, _)| !torn_out.contains(k))
        .map(|(_, keepout)| keepout)
        .collect();
    let breaches: Vec<Vec<DVec2>> = holes.iter().map(hole_outline).collect();
    parts.openings[index] = openings
        .iter()
        .map(|o| [o.u - o.width / 2.0, o.y, o.u + o.width / 2.0, o.top()])
        .chain(holes.iter().map(|h| h.bounds))
        .collect();
    parts.holes[index] = holes.clone();
    let kit = parts.kit(piece);
    wall_with_breaches(kit, frame, &outline, &openings, REVEAL, &breaches);
    for (k, (hole, outline)) in holes.iter().zip(&breaches).enumerate() {
        // Its stripped siding stops short of the openings and the other breaches.
        let mut others = keepouts.clone();
        others.extend(
            holes
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != k)
                .map(|(_, h)| h.bounds),
        );
        let room = WallRoom {
            reach: half - CORNER_BOARD - HOLE_MARGIN,
            top: roof.wall,
            keepouts: others,
        };
        line_hole(kit, frame, hole, outline, standing, &room);
    }
    // The share of the wall torn out: past [`RACKED_WALL`] its frame racks and
    // every window in it breaks.
    let wall_area = 2.0 * half * (roof.wall - PLINTH);
    let racked = holes.iter().map(Hole::area).sum::<f64>() / wall_area >= RACKED_WALL;
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
        if torn_out.contains(&i) {
            // Its frame lies smashed on the ground under the breach.
            if standing {
                fallen_window(kit, frame, opening, seed);
            }
            continue;
        }
        let blown = blasted.contains(&i);
        let upper = i >= ground_floor;
        let rect = [
            opening.u - opening.width / 2.0,
            opening.y,
            opening.u + opening.width / 2.0,
            opening.top(),
        ];
        // Glass breaks only beside a breach, where a shell struck the window, or
        // once this wall's frame racks.
        let beside = holes
            .iter()
            .filter(|hole| hole.gap(rect) < WINDOW_BLAST)
            .max_by(|a, b| a.area().total_cmp(&b.area()));
        let boarded = !role.door && Random::new(seed + 0.7).next() < BOARDED_WINDOWS;
        // Boards nailed over a window hold until a breach reaches its casing.
        let reach = if boarded { BOARDS_HOLD } else { WINDOW_BLAST };
        let beside = beside.filter(|hole| hole.gap(rect) < reach);
        let broken = blown || racked || beside.is_some();
        if blown {
            window_soot(kit, frame, opening, seed);
        }
        if broken && boarded {
            // Its boards torn loose: one hangs from a nail, the rest lie below.
            broken_window(kit, frame, opening, seed);
            torn_boards(kit, frame, opening, seed);
        } else if broken {
            // A big breach beside it knocks its frame askew, dropping the side
            // nearer the breach.
            let tilt = beside.filter(|hole| hole.big()).map_or(0.0, |hole| {
                let toward = (hole.center().x - opening.u).signum();
                -toward * ASKEW * (hole.area() / (2.0 * BIG_BREACH)).min(1.0)
            });
            let middle = DVec3::new(opening.u, opening.y + opening.height / 2.0, 0.0);
            let askew = frame
                * shift(middle)
                * pose(DVec3::ZERO, DVec3::new(0.0, 0.0, tilt))
                * shift(-middle);
            broken_window(kit, askew, opening, seed);
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
        // A hole beside a window takes its shutters with it.
        let reach = opening.width / 2.0 + CASING + 0.01 + room;
        let shuttered = [
            opening.u - reach,
            opening.y,
            opening.u + reach,
            opening.top(),
        ];
        let holed = holes.iter().any(|hole| hole.gap(shuttered) < 0.05);
        let lost = holed || blown;
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
        let door_rect = [
            door_opening.u - door_opening.width / 2.0,
            door_opening.y,
            door_opening.u + door_opening.width / 2.0,
            door_opening.top(),
        ];
        // A breach by the door (or a racked wall) blows it off its hinges.
        if racked
            || holes
                .iter()
                .any(|hole| hole.gap(door_rect) < WINDOW_BLAST + 0.3)
        {
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
        let gone: Vec<f64> = torn_out.iter().map(|&k| windows[k].u).collect();
        window_plants(
            &mut parts.nodes[0],
            frame,
            half,
            role,
            roof,
            design.variant,
            &gone,
        );
    }
    if standing && role.door {
        foundation_bed(parts, frame, half, design.variant);
    }
}

/// The roof with its covering, the chimney, the yard and the dormers: a cottage's
/// last section ([`PartsCut`]). Blasts that struck it hole it where they landed;
/// the walls' breaches only shake shingles off the eave over them.
fn house_roof(
    parts: &mut Parts,
    design: &HouseDesign,
    roof: &Roof,
    build: Build,
    roof_holes: &[RoofHole],
    hits: &[TimberHit],
) {
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
        let holes: Vec<&RoofHole> = roof_holes
            .iter()
            .filter(|hole| hole.piece == slope.piece)
            .collect();
        match design.style.cover {
            RoofCover::Shingles => shingle_slope(parts.courses(slope.piece), &slope, &holes),
            RoofCover::Seams => seam_slope(parts.kit(slope.piece), &slope, &holes),
        }
        for hole in &holes {
            hole_petals(parts.kit(slope.piece), &slope, hole);
            // A falling slab shows the sky through its holes, not an attic.
            if standing {
                attic(parts.kit(slope.piece), &slope, hole, roof);
            }
        }
    }
    ridge_caps(parts, design, roof, &broken_ridge(roof, roof_holes));
    gutters(parts, w, d, roof);
    shed_shingles(parts, design, roof, roof_holes);

    if let Some(base) = parts.base() {
        yard(base, design, false);
        let mut plants = Vec::new();
        garden(base, &mut plants, design, false);
        parts.nodes[0].extend(plants);
    }
    let toppled = design.chimney_struck(hits);
    chimney(parts.kit(HousePart::Chimney), design, roof, toppled);
    // Some cottages light their attics with dormers, so the roofscape varies.
    if roof.form == RoofForm::Gable {
        let flue = design.flue();
        if let Some(wing) = design.wing() {
            // A blast on its slope or a big breach in the wall under it.
            let broken = roof_holes
                .iter()
                .any(|hole| hole.side > 0.0 && (hole.across - wing.z).abs() < wing.half + 1.0)
                || parts.holes[design.door_wall].iter().any(Hole::big);
            cross_gable(parts, design, roof, &wing, broken);
        }
        for (side, z) in design.dormers() {
            let clear = flue.x * side < 0.0 || (flue.z - z).abs() > DORMER_HALF + CHIMNEY;
            let on_side = |hole: &&RoofHole| hole.on_side() && hole.side == side;
            let in_hole = roof_holes
                .iter()
                .filter(on_side)
                .any(|hole| (hole.across - z).abs() < DORMER_HALF + hole.half_across);
            let broken = roof_holes
                .iter()
                .filter(on_side)
                .any(|hole| (hole.across - z).abs() < DORMER_HALF + hole.half_across + 1.2);
            if clear && !in_hole {
                let piece = if side > 0.0 {
                    HousePart::RightRoof
                } else {
                    HousePart::LeftRoof
                };
                dormer(parts.kit(piece), roof, side, z, broken);
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
            RoofCover::Shingles => shingle_slope(parts.courses(piece), &wing_slope, &[]),
            RoofCover::Seams => seam_slope(parts.kit(piece), &wing_slope, &[]),
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
#[allow(clippy::too_many_arguments)]
fn window_plants(
    nodes: &mut Vec<Node>,
    frame: DMat4,
    half: f64,
    role: WallRole,
    roof: &Roof,
    v: u32,
    gone: &[f64],
) {
    let octahedron = OCTAHEDRON.get_or_insert((), || octahedron_geometry(1.0, 0));
    let mut rng = Random::new(f64::from(v) + 17.0 + role.index as f64 * 3.0);
    let flower = FLOWERS[(v / 3) as usize % FLOWERS.len()];
    let mut group = Node::group("");
    for u in window_positions(role, half) {
        // A window box torn out with its window took its plants.
        if gone.iter().any(|&g| (g - u).abs() < 1e-6) {
            continue;
        }
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

/// A hole a blast tore through the roof where it struck: its slope, centre (along
/// the slope and across it, and in plan) and half extents, ragged per course, grown
/// with the strikes that landed on it.
#[derive(Clone, Copy, Debug)]
struct RoofHole {
    piece: HousePart,
    /// The side slope's side (±1; 0 on a hip's end slope).
    side: f64,
    along: f64,
    across: f64,
    half_along: f64,
    half_across: f64,
    /// Its centre in plan (build frame x, z) and its mark's size.
    plan: DVec2,
    size: f64,
    seed: f64,
}

/// How far beyond a broken stretch of ridge (m) the roof dips with it, and how deep.
const RIDGE_DIP: (f64, f64) = (0.9, 0.3);
/// A roof hole this close (m) to the ridge has torn into it.
const RIDGE_REACH: f64 = 0.3;

/// The stretches of ridge (build-frame z from, to) broken by strikes on the roof:
/// where a hole that grew with several strikes reaches the ridge, or holes on both
/// slopes reach it facing each other. A gable ridge only.
fn broken_ridge(roof: &Roof, roof_holes: &[RoofHole]) -> Vec<(f64, f64)> {
    if roof.form != RoofForm::Gable {
        return Vec::new();
    }
    let length = ((roof.eave_x).powi(2) + (roof.ridge - roof.eave_y).powi(2)).sqrt();
    let near: Vec<&RoofHole> = roof_holes
        .iter()
        .filter(|hole| hole.on_side() && hole.at_ridge(length))
        .collect();
    near.iter()
        .filter(|hole| {
            hole.size >= 2.0
                || near.iter().any(|other| {
                    other.side != hole.side
                        && (other.across - hole.across).abs() < other.half_across + hole.half_across
                })
        })
        .map(|hole| {
            (
                hole.across - hole.half_across * 0.8,
                hole.across + hole.half_across * 0.8,
            )
        })
        .collect()
}

/// Torn sheet or shingles round a roof hole: ragged petals of the covering bent
/// down into it by the blast, so it reads as torn rather than cut.
fn hole_petals(kit: &mut Kit<Surface>, slope: &Slope, hole: &RoofHole) {
    let mut rng = Random::new(hole.seed + 7.0);
    let petals = 5;
    for k in 0..petals {
        let a = (f64::from(k) + rng.range(-0.25, 0.25)) * PI * 2.0 / f64::from(petals);
        let rim = |angle: f64, r: f64| {
            DVec2::new(
                hole.across + angle.cos() * hole.half_across * r,
                hole.along + angle.sin() * hole.half_along * r,
            )
        };
        let spread = rng.range(0.22, 0.38);
        let (left, right) = (rim(a - spread, 0.82), rim(a + spread, 0.82));
        let tip = rim(a + rng.range(-0.1, 0.1), rng.range(0.3, 0.5));
        let depth = -rng.range(0.1, 0.28);
        let corners = [
            slope.at(left.y, left.x, 0.006),
            slope.at(right.y, right.x, 0.006),
            slope.at(tip.y, tip.x, depth),
        ];
        kit.quad(
            Surface::Roof,
            [corners[0], corners[1], corners[2], corners[2]],
            slope.normal,
        );
    }
}

/// The holes blasts tore in a cottage's roof, one per roof mark (clear of the stack,
/// which a strike there topples instead, and of the holes before it).
fn roof_holes(design: &HouseDesign, roof: &Roof, hits: &[TimberHit]) -> Vec<RoofHole> {
    let slopes = slopes(roof);
    let rise = (1.0 + roof.pitch * roof.pitch).sqrt();
    let flue = design.flue();
    let mut holes: Vec<RoofHole> = Vec::new();
    for (i, hit) in hits.iter().enumerate() {
        if !is_roof_mark(hit.y, design.h) {
            continue;
        }
        let local = design.to_local(DVec3::new(hit.x, hit.y, hit.z));
        let piece = design.struck_part(hit);
        let Some(slope) = slopes.iter().find(|slope| slope.piece == piece) else {
            continue;
        };
        let side_slope = matches!(piece, HousePart::RightRoof | HousePart::LeftRoof);
        let (run, across) = if side_slope {
            (roof.eave_x - local.x.abs(), local.z)
        } else {
            (roof.rake_z - local.z.abs(), local.x)
        };
        let grow = 1.0 + ROOF_HOLE_GROWTH * (hit.size - 1.0).max(0.0);
        // A strike beside an earlier hole tears a smaller one of its own, as large
        // as fits between them.
        let place = |scale: f64| {
            let half_along = (ROOF_HOLE.0 / 2.0 * grow * scale).min(slope.length * 0.4);
            let along = (run * rise).clamp(half_along + 0.15, slope.length - half_along - 0.06);
            let room = slope.half(along) - 0.15;
            let half_across = (ROOF_HOLE.1 / 2.0 * grow * scale).min(room * 0.8);
            if half_across < 0.2 {
                return None;
            }
            let across = across.clamp(-room + half_across, room - half_across);
            let center = slope.at(along, across, 0.0);
            let plan = DVec2::new(center.x, center.z);
            let reach = half_along.max(half_across);
            let clear_flue = (plan - DVec2::new(flue.x, flue.z)).length() > reach + CHIMNEY * 0.8;
            let clear = holes.iter().all(|other| {
                other.piece != piece
                    || (other.along - along).abs() > other.half_along + half_along + 0.08
                    || (other.across - across).abs() > other.half_across + half_across + 0.08
            });
            (clear_flue && clear).then_some((along, across, half_along, half_across, plan))
        };
        let Some((along, across, half_along, half_across, plan)) =
            [1.0, 0.8, 0.62, 0.48].into_iter().find_map(place)
        else {
            continue;
        };
        holes.push(RoofHole {
            piece,
            side: match piece {
                HousePart::RightRoof => 1.0,
                HousePart::LeftRoof => -1.0,
                _ => 0.0,
            },
            along,
            across,
            half_along,
            half_across,
            plan,
            size: hit.size,
            seed: f64::from(design.variant) * 17.0 + i as f64 * 977.0 + 3.0,
        });
    }
    holes
}

impl RoofHole {
    /// Whether it tore through to the ridge: a big one, or one of a pair facing
    /// each other across it ([`broken_ridge`]).
    fn at_ridge(&self, slope_length: f64) -> bool {
        self.along + self.half_along >= slope_length - RIDGE_REACH
    }

    fn on_side(&self) -> bool {
        self.side != 0.0
    }

    /// How far it reaches from its centre at most (plan metres, about).
    fn reach(&self) -> f64 {
        self.half_along.max(self.half_across)
    }

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

    /// Move the courses as the frame has given ([`Settle`]).
    fn settle(&mut self, settle: &Settle) {
        if settle.still() {
            return;
        }
        let corner = |i: usize| Corner {
            p: DVec3::from_slice(&self.positions[3 * i..]),
            n: DVec3::from_slice(&self.normals[3 * i..]),
            uv: DVec2::from_slice(&self.uvs[2 * i..]),
            color: DVec3::ZERO,
            cap: self.caps[i],
        };
        let triangles =
            (0..self.caps.len() / 3).map(|t| [corner(3 * t), corner(3 * t + 1), corner(3 * t + 2)]);
        let (mut positions, mut normals, mut uvs, mut caps) = (
            Vec::with_capacity(self.positions.len()),
            Vec::with_capacity(self.normals.len()),
            Vec::with_capacity(self.uvs.len()),
            Vec::with_capacity(self.caps.len()),
        );
        settle.reshape(triangles, |corner| {
            positions.extend(corner.p.to_array());
            normals.extend(corner.n.to_array());
            uvs.extend(corner.uv.to_array());
            caps.push(corner.cap);
        });
        (self.positions, self.normals, self.uvs, self.caps) = (positions, normals, uvs, caps);
    }

    fn mesh(&self) -> Mesh {
        Mesh::from_f64(&self.positions, &self.normals, &self.uvs, None)
    }
}

/// One slope's shingle courses from eave to ridge. Each course's texture row
/// matches its geometry: the butt edge sits on the top of a row of `shingles.webp`.
/// Each hole leaves every course it crosses broken in ragged parts.
fn shingle_slope(out: &mut Courses, slope: &Slope, holes: &[&RoofHole]) {
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
        let mut gaps: Vec<(f64, f64)> = holes
            .iter()
            .filter_map(|hole| hole.span((s0 + s1) / 2.0 - SHINGLE_EAVE, k))
            .collect();
        gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
        // The runs between the gaps (±1 stand for the course's own ends).
        let mut runs = Vec::with_capacity(gaps.len() + 1);
        let mut from = -1.0;
        for (start, end) in gaps {
            if from == -1.0 || start > from {
                runs.push((from, start));
            }
            from = if from == -1.0 { end } else { from.max(end) };
        }
        runs.push((from, 1.0));
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
fn seam_slope(kit: &mut Kit<Surface>, slope: &Slope, holes: &[&RoofHole]) {
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
    let cuts: Vec<Vec<DVec2>> = holes
        .iter()
        .map(|hole| {
            let mut rng = Random::new(hole.seed);
            (0..10)
                .map(|i| {
                    let angle = -f64::from(i) / 10.0 * PI * 2.0;
                    let r = rng.range(0.7, 1.0);
                    DVec2::new(
                        hole.across + angle.cos() * hole.half_across * r,
                        hole.along + angle.sin() * hole.half_along * r,
                    )
                })
                .collect()
        })
        .collect();
    kit.face_with_holes(Surface::Roof, &outline, &cuts, frame);
    let count = (slope.half_eave * 2.0 / SEAM_SPACING).floor() as i32;
    for i in -count / 2..=count / 2 {
        let a = f64::from(i) * SEAM_SPACING;
        // A rib ends where the slope narrows past it (at a hip).
        let reach = if slope.half_ridge >= a.abs() {
            slope.length
        } else {
            slope.length * (slope.half_eave - a.abs()) / (slope.half_eave - slope.half_ridge)
        };
        // Cut short by each hole it crosses, bottom up.
        let mut crossed: Vec<&&RoofHole> = holes
            .iter()
            .filter(|hole| (a - hole.across).abs() < hole.half_across)
            .collect();
        crossed.sort_by(|p, q| p.along.total_cmp(&q.along));
        let mut runs = Vec::with_capacity(crossed.len() + 1);
        let mut from = 0.0;
        for hole in crossed {
            runs.push((from, hole.along - hole.half_along));
            from = hole.along + hole.half_along;
        }
        runs.push((from, reach - 0.05));
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
    // The attic's dark well below the deck, and the rafters just under the covering,
    // so they show only through the hole.
    let depth = -roof.thick - 0.3;
    let rafter = -0.085;
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
        // The snapped one hangs from its upper end into the attic.
        let top = slope.at(to, a + offset, rafter);
        let droop = if k == 1 { -0.35 } else { 0.0 };
        let basis = DMat4::from_cols(
            slope.across.extend(0.0),
            slope.normal.extend(0.0),
            slope.up.extend(0.0),
            top.extend(1.0),
        ) * pose(DVec3::ZERO, DVec3::new(droop, 0.0, 0.0))
            * shift(DVec3::new(0.0, 0.0, -(to - from) / 2.0));
        kit.block(Surface::Wood, DVec3::new(0.09, 0.14, to - from), basis);
    }
}

/// Cap shingles or a seamed cap along the ridge, and caps up every hip.
fn ridge_caps(parts: &mut Parts, design: &HouseDesign, roof: &Roof, broken: &[(f64, f64)]) {
    let crest = roof.ridge + roof.thick;
    let shingles = design.style.cover == RoofCover::Shingles;
    if shingles && roof.ridge_half > 0.01 {
        ridge_shingles(parts, roof, broken);
    } else if roof.ridge_half > 0.01 {
        // A galvanised ridge roll, catching the light along the crest, torn where
        // strikes broke the ridge.
        let end = roof.ridge_half + 0.02;
        let mut from = -end;
        let mut runs = Vec::new();
        let mut cuts = broken.to_vec();
        cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (a, b) in cuts {
            runs.push((from, a));
            from = from.max(b);
        }
        runs.push((from, end));
        for (a, b) in runs {
            if b - a > 0.1 {
                parts.kit(HousePart::RightRoof).block(
                    Surface::Metal,
                    DVec3::new(0.26, 0.07, b - a),
                    shift(DVec3::new(0.0, crest + 0.03, (a + b) / 2.0)),
                );
            }
        }
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
fn ridge_shingles(parts: &mut Parts, roof: &Roof, broken: &[(f64, f64)]) {
    let z_end = roof.ridge_half + 0.01;
    let pieces = (2.0 * z_end / RIDGE_PIECE).round().max(1.0);
    let piece = 2.0 * z_end / pieces;
    let crest = roof.ridge + roof.thick;
    let edge_y = roof.top(RIDGE_HALF, 0.0);
    for j in 0..pieces as u32 {
        let j = f64::from(j);
        let (z1, z0) = (z_end - j * piece, z_end - (j + 1.0) * piece);
        if broken.iter().any(|&(a, b)| z1 > a && z0 < b) {
            continue;
        }
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
        let torn = torn_eave(parts, w, d, roof, side);
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
        // Over a breach that took the wall under it, a length of gutter has torn
        // off its brackets and hangs from the far one.
        let runs = match torn {
            Some((z0, z1)) => vec![(-zr - 0.02, z0), (z1, zr + 0.02)],
            None => vec![(-zr - 0.02, zr + 0.02)],
        };
        for (from, to) in runs {
            if to - from > 0.05 {
                kit.solid(Surface::Metal, &[profile(from), profile(to)]);
            }
        }
        if let Some((z0, z1)) = torn {
            let hinge = DVec3::new(x(0.1), ye, z0);
            let length = z1 - z0;
            let drop = (length * 0.55).min(HANGING_GUTTER);
            let hang = DMat4::from_translation(hinge)
                * DMat4::from_axis_angle(DVec3::X, (drop / length).min(1.0).asin())
                * DMat4::from_translation(-hinge);
            let hanging = |z: f64| -> Vec<DVec3> {
                profile(z)
                    .into_iter()
                    .map(|p| hang.transform_point3(p))
                    .collect()
            };
            kit.solid(Surface::Metal, &[hanging(z0 + 0.04), hanging(z1 - 0.06)]);
        }
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
fn yard(kit: &mut Kit<Surface>, design: &HouseDesign, crushed: bool) {
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
        if crushed {
            // Knocked over by the falling wall, rolled out from it.
            let (from, to) = (
                at + DVec3::new(0.35, 0.27, -0.1),
                at + DVec3::new(0.35 + 0.8, 0.27, 0.15),
            );
            kit.rod(Surface::Wood, 0.27, from, to, 10);
            for t in [0.18, 0.76] {
                kit.rod(
                    Surface::Iron,
                    0.285,
                    from.lerp(to, t),
                    from.lerp(to, t + 0.04),
                    10,
                );
            }
        } else {
            kit.post(Surface::Wood, 0.27, 0.82, at, 10);
            for y in [0.15, 0.62] {
                kit.post(Surface::Iron, 0.285, 0.035, at + DVec3::Y * y, 10);
            }
            kit.post(Surface::Dark, 0.23, 0.01, at + DVec3::Y * 0.82, 10);
        }
    }
}

/// How flat a crushed garden's plants are pressed (share of their height).
const CRUSHED: f64 = 0.3;

/// The kitchen garden behind a cottage, off its back wall (the one opposite the
/// door): a bed of turned soil edged with boards, planted in rows of cabbages, or
/// with bean poles and a compost heap, so every lot reads as lived in and no two
/// neighbours' look alike from above. Low enough to drive over.
/// Under a collapsed cottage (`crushed`) the bean poles lie snapped flat and every
/// plant is pressed into the bed, so nothing stands up through the fallen panels.
fn garden(kit: &mut Kit<Surface>, plants: &mut Vec<Node>, design: &HouseDesign, crushed: bool) {
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
        let flat = if crushed { CRUSHED } else { 1.0 };
        clump.scale = DVec3::new(r * (2.0 - flat), r * 0.75 * flat, r * (2.0 - flat));
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
                let p = if crushed {
                    DVec3::new(p.x, r * 0.2, p.z)
                } else {
                    p
                };
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
                // Knocked flat along the bed by the fall.
                let top = if crushed {
                    at(
                        u + side * 0.25 + 0.6,
                        (near + far) / 2.0 + side * 0.45,
                        0.06,
                    )
                } else {
                    top
                };
                kit.rod(Surface::Splinters, 0.02, foot, top, 4);
                let p = foot.lerp(top, rng.range(0.4, 0.6));
                plant(p, rng.range(0.16, 0.22), LEAVES, &mut rng);
            }
        }
        kit.block(
            Surface::Dark,
            DVec3::new(0.6, 0.24, 0.5),
            frame
                * pose(
                    DVec3::new(u1 - 0.4, 0.0, far - 0.35),
                    DVec3::new(0.0, rng.range(-0.4, 0.4), 0.0),
                ),
        );
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
/// it. A chimney a roof strike toppled stands as a ragged stub, its bricks
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
        // Standing past where the collapse snaps the stack, so its upper piece
        // falls as the stub's broken top rather than a chimney that was gone.
        let stub = (roof.top(cx, cz) + 0.3)
            .max(design.chimney_snap() + 0.12)
            .min(full_top - 0.2);
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
    // The lot outlives the cottage: its path, woodpile and garden stay round the
    // ruin, the garden's poles and plants and the rain barrel knocked flat by it.
    yard(&mut kit, &design, true);
    let mut plants = Vec::new();
    garden(&mut kit, &mut plants, &design, true);
    let mut node = Node::group("");
    add_cottage(
        &mut node,
        kit.finish_scaled(cottage_uv(walls)),
        &palette,
        walls,
        true,
    );
    node.children.extend(plants);
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
    // The chimney breast, which ran down through the rooms to the hearth, still
    // standing out of the heap where the stack above it snapped off: ragged
    // courses on its broken top, bricks knocked from it lying at its foot.
    let flue = design.flue();
    let (bx, bz) = (
        flue.x.clamp(-fx + 0.5, fx - 0.5),
        flue.z.clamp(-fz + 0.5, fz - 0.5),
    );
    let breast = rise(bx, bz) + rng.range(0.75, 1.05);
    kit.block(
        Surface::Brick,
        DVec3::new(CHIMNEY * 0.95, breast + 0.1, CHIMNEY * 0.95),
        pose(
            DVec3::new(bx, (breast - 0.1) / 2.0, bz),
            DVec3::new(rng.range(-0.06, 0.06), 0.0, rng.range(-0.06, 0.06)),
        ),
    );
    for (sx, sz) in [(-1.0, -1.0), (1.0, 1.0), (1.0, -1.0)] {
        let course = rng.range(0.07, 0.24);
        kit.block(
            Surface::Brick,
            DVec3::new(CHIMNEY * 0.45, course, CHIMNEY * 0.45),
            shift(DVec3::new(
                bx + sx * CHIMNEY * 0.24,
                breast + course / 2.0,
                bz + sz * CHIMNEY * 0.24,
            )),
        );
    }
    // A corner post of the frame still upright at one corner, leaning in, with a
    // torn scrap of its wall nailed to it.
    let (cx, cz) = (
        if design.variant.is_multiple_of(2) {
            fx
        } else {
            -fx
        },
        if (design.variant / 2).is_multiple_of(2) {
            fz
        } else {
            -fz
        },
    );
    let post = rng.range(1.7, 2.3);
    let lean = DVec3::new(-cx.signum() * 0.16, 0.0, cz.signum() * 0.12);
    let placement = pose(
        DVec3::new(cx - cx.signum() * 0.1, 0.0, cz - cz.signum() * 0.1),
        lean,
    );
    kit.block(
        Surface::Wood,
        DVec3::new(0.13, post, 0.13),
        placement * shift(DVec3::Y * (post / 2.0)),
    );
    let scrap = jagged(&mut rng, 0.9, 1.1);
    slab(
        &mut kit,
        [Surface::Siding, Surface::Curtain, Surface::Splinters],
        &scrap,
        WALL_SLAB,
        placement
            * pose(
                DVec3::new(-cx.signum() * 0.42, post * 0.55, 0.0),
                DVec3::new(0.0, PI / 2.0, rng.range(-0.25, 0.25)),
            ),
    );
    // The roof's broken boards come to rest over the crown: slabs of shingled
    // deck tipped across it, rafters under them.
    for k in 0..2 {
        let at = on(&mut rng, 1.0);
        let (w, d) = (rng.range(1.2, 1.7), rng.range(0.8, 1.1));
        let outline = jagged(&mut rng, w, d);
        let placement = pose(
            at + DVec3::Y * (0.12 + 0.1 * k as f64),
            tilt(&mut rng, 0.35),
        );
        slab(
            &mut kit,
            [Surface::Roof, Surface::Wood, Surface::Splinters],
            &outline,
            ROOF_SLAB,
            placement * pose(DVec3::ZERO, DVec3::new(-PI / 2.0, 0.0, 0.0)),
        );
        kit.block(
            Surface::Wood,
            DVec3::new(0.07, 0.12, d + 0.4),
            placement * shift(DVec3::new(rng.range(-0.3, 0.3) * w, -0.12, 0.0)),
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

    /// Every vertex and transform of a node tree, to compare two builds.
    fn contents(node: &Node, out: &mut Vec<f64>) {
        out.extend(node.position.to_array());
        out.extend(node.rotation.to_array());
        if let Some(drawable) = &node.drawable {
            let mesh = &drawable.mesh;
            out.extend(mesh.positions.iter().flatten().map(|&v| f64::from(v)));
            out.extend(mesh.normals.iter().flatten().map(|&v| f64::from(v)));
            out.extend(mesh.colors.iter().flatten().map(|&v| f64::from(v)));
            out.extend(mesh.uvs.iter().flatten().map(|&v| f64::from(v)));
            out.push(f64::from(drawable.material.color.0));
        }
        for child in &node.children {
            contents(child, out);
        }
    }

    #[test]
    fn a_cottage_built_in_steps_matches_one_built_at_once() {
        let (x, z, w, d, h) = village()[0];
        let wall = TimberHit {
            x: 0.5,
            y: 1.2,
            z: 3.0,
            size: 1.0,
        };
        let roof = TimberHit {
            x: 0.8,
            y: h - 0.3,
            z: 0.2,
            size: 2.0,
        };
        for hits in [vec![wall], vec![TimberHit { size: 5.0, ..wall }, roof]] {
            let shape = super::super::CoverShape {
                kind: crate::sim::types::CoverKind::House,
                x,
                z,
                w,
                d,
                h,
                color: 0,
                debris_seed: None,
                timber_hits: hits.clone(),
                timber_join: None,
            };
            let at_once = super::super::cover_model(&shape, super::super::TreeDetail::Full, 0);
            let mut build = HouseCoverBuild::new(x, z, w, d, h, &hits);
            let mut steps = 1;
            let stepped = loop {
                match build.step() {
                    Some(root) => break root,
                    None => steps += 1,
                }
            };
            // The kits a section at a time (plinth, four walls, roof), then the walls'
            // and the roof's meshes.
            assert_eq!(steps, 8);
            let (mut a, mut b) = (Vec::new(), Vec::new());
            contents(&at_once, &mut a);
            contents(&stepped, &mut b);
            assert!(!a.is_empty() && a == b, "{hits:?}");
        }
    }

    /// Variety never costs more than the one cottage design did (about 6,400
    /// triangles), a second floor's windows and belt course about half again, and
    /// big breaches torn through every wall and two holes in the roof at most 8,500
    /// more, with the roof and walls sliced where they sag over them (only struck
    /// houses carry them; two big breaches at most 5,000).
    #[test]
    fn cottages_stay_within_the_former_triangle_budget() {
        let mut over = Vec::new();
        for (x, z, w, d, h) in village() {
            // A breach grown big in each wall and two roof holes.
            let mut hits: Vec<TimberHit> = (0..4)
                .map(|i| TimberHit {
                    x: [w / 2.0, -w / 2.0, 0.6, -0.6][i],
                    y: 1.2,
                    z: [0.4, -0.4, d / 2.0, -d / 2.0][i],
                    size: 6.0,
                })
                .collect();
            hits.extend([(0.25, 1.0), (-0.25, -1.0)].map(|(fx, fz)| TimberHit {
                x: w * fx,
                y: h - 0.4,
                z: fz,
                size: 3.0,
            }));
            for (cost, hits) in [(0, &[][..]), (5_000, &hits[..2]), (8_500, &hits[..])] {
                let mut group = Node::group("");
                house(&mut group, x, z, w, d, h, hits);
                let count = triangles(&group);
                let floors = if h >= crate::sim::house_layout::TWO_STOREY {
                    9_000
                } else {
                    6_400
                };
                if count > floors + cost {
                    over.push(format!("{x} {z} {} hits: {count}", hits.len()));
                }
            }
        }
        assert!(over.is_empty(), "{over:?}");
    }

    /// A mark of `size` on the middle of a cottage's wall without the door, 0.3 m
    /// off its centre.
    fn side_wall_mark(design: &HouseDesign, size: f64) -> TimberHit {
        let index = if design.door_wall == 0 { 2 } else { 0 };
        let (center, _, along, _) =
            crate::sim::house_layout::wall_frames(design.w, design.d)[index];
        let world = design.to_world(center + along * 0.3);
        TimberHit {
            x: world.x,
            y: 1.0,
            z: world.z,
            size,
        }
    }

    /// Marks on one wall break that wall and nothing else: no other wall is holed,
    /// the roof keeps every shingle course whole and the chimney stands.
    #[test]
    fn a_shelled_wall_leaves_the_rest_of_the_cottage_as_it_was() {
        let (x, z, w, d, h) = village()[0];
        let design = HouseDesign::of(x, z, w, d, h);
        let roof = design.roof();
        let hit = side_wall_mark(&design, 4.0);
        let (_, parts, _) = cottage_parts(&design, Build::Standing, &[hit]);
        let struck: Vec<usize> = (0..4).filter(|&i| !parts.holes[i].is_empty()).collect();
        assert_eq!(struck.len(), 1, "{struck:?}");
        assert!(roof_holes(&design, &roof, &[hit]).is_empty());
        assert!(!design.chimney_struck(&[hit]));
        let (_, intact, _) = cottage_parts(&design, Build::Standing, &[]);
        assert_eq!(
            parts.courses[1].positions.len(),
            intact.courses[1].positions.len(),
            "the roof's shingles are untouched"
        );
    }

    /// A breach grows with every shell that lands on it.
    #[test]
    fn a_breach_grows_with_its_mark() {
        let (x, z, w, d, h) = village()[0];
        let design = HouseDesign::of(x, z, w, d, h);
        let areas: Vec<f64> = [1.0, 1.4, 1.8, 2.6, 3.4]
            .into_iter()
            .map(|size| {
                let hit = side_wall_mark(&design, size);
                let (_, parts, _) = cottage_parts(&design, Build::Standing, &[hit]);
                parts.holes.iter().flatten().map(Hole::area).sum()
            })
            .collect();
        assert!(areas.windows(2).all(|pair| pair[1] > pair[0]), "{areas:?}");
    }

    fn draws(node: &Node) -> usize {
        let mut count = usize::from(node.drawable.is_some());
        for child in &node.children {
            count += draws(child);
        }
        count
    }

    #[test]
    fn collapse_pieces_and_heaps_draw_once_a_finish() {
        // One cottage with a shingled roof and one without cover every branch.
        let cottages = village();
        let shingled = |&(x, z, w, d, h): &(f64, f64, f64, f64, f64)| {
            HouseDesign::of(x, z, w, d, h).style.cover == RoofCover::Shingles
        };
        let picks = [
            cottages.iter().find(|c| shingled(c)),
            cottages.iter().find(|c| !shingled(c)),
        ];
        for &(x, z, w, d, h) in picks.map(|pick| pick.expect("both roof covers in the village")) {
            let pieces = house_pieces(x, z, w, d, h, &[]);
            let design = HouseDesign::of(x, z, w, d, h);
            assert_eq!(pieces.len(), design.pieces().len());
            // One draw a piece, its siding's boards or bricks one more (a wall's,
            // the mound's torn panels, a cross gable's), and a shingled roof
            // slab's shingles one more.
            let shingled = design.style.cover == RoofCover::Shingles;
            for (piece, node) in &pieces {
                let walled = piece.part.wall_index().is_some() || piece.part == HousePart::Mound;
                let most = 1
                    + usize::from(walled || piece.part.is_roof())
                    + usize::from(piece.part.is_roof() && shingled);
                assert!((1..=most).contains(&draws(node)), "{piece:?}");
                if walled {
                    assert_eq!(draws(node), 2, "{piece:?}");
                }
            }
        }
        let (x, z, w, d, _) = cottages[0];
        let mut heap = Node::group("");
        house_heap(&mut heap, x, z, w, d - HOUSE_HEAP.depth, 77.0 * 512.0);
        batch(&mut heap.children[0]);
        assert_eq!(draws(&heap), 2, "heap");
    }
}
